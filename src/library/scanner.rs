use std::path::Path;
use std::sync::mpsc;
use tracing::{debug, info};
use walkdir::WalkDir;

use super::{artwork, metadata};
use crate::db::Database;
use crate::error::Result;

#[derive(Debug, Clone)]
pub enum ScanProgress {
    Counted { total: u32 },
    StartingDir(String),
    FileScanned { count: u32, path: String },
    Complete { total: u32, added: u32, updated: u32 },
    Foreign { counted: Vec<(String, u32)> },
    FileError { path: String, error: String },
    Updated(Update),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Update {
    pub path: String,
    pub artist: String,
    pub title: String,
    pub why: Reread,
    pub changes: Vec<Change>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reread {
    Changed,
    Cover,
    Nameless,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub field: Field,
    pub was: String,
    pub now: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Title,
    Artist,
    AlbumArtist,
    Album,
    Genre,
    Year,
    Track,
    Disc,
    Length,
    Format,
    Bitrate,
    SampleRate,
    Size,
    Cover,
}

pub fn changes(was: &crate::db::models::Track, now: &crate::db::models::Track) -> Vec<Change> {
    fn opt<T: ToString>(v: Option<T>) -> String {
        v.map(|v| v.to_string()).unwrap_or_default()
    }
    fn length(secs: f64) -> String {
        let secs = secs.round() as u64;
        format!("{}:{:02}", secs / 60, secs % 60)
    }
    let mut out = Vec::new();
    let mut put = |field: Field, was: String, now: String| {
        if was != now {
            out.push(Change { field, was, now });
        }
    };
    put(Field::Title, was.title.clone(), now.title.clone());
    put(Field::Artist, was.artist.clone(), now.artist.clone());
    put(Field::AlbumArtist, was.album_artist.clone(), now.album_artist.clone());
    put(Field::Album, was.album.clone(), now.album.clone());
    put(Field::Genre, was.genre.clone(), now.genre.clone());
    put(Field::Year, opt(was.year), opt(now.year));
    put(Field::Track, opt(was.track_number), opt(now.track_number));
    put(Field::Disc, opt(was.disc_number), opt(now.disc_number));
    put(Field::Length, length(was.duration_secs), length(now.duration_secs));
    put(Field::Format, was.format.clone(), now.format.clone());
    put(Field::Bitrate, opt(was.bitrate), opt(now.bitrate));
    put(Field::SampleRate, opt(was.sample_rate), opt(now.sample_rate));
    put(Field::Size, was.file_size.to_string(), now.file_size.to_string());
    put(Field::Cover, was.has_artwork.to_string(), now.has_artwork.to_string());
    out
}

pub fn count_files(dir: &Path) -> u32 {
    WalkDir::new(dir)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file() && metadata::is_supported_audio(e.path()))
        .count() as u32
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Swept {
    pub files: u32,
    pub added: u32,
    pub updated: u32,
    pub foreign: Vec<(String, u32)>,
}

pub fn merge_foreign(into: &mut Vec<(String, u32)>, more: Vec<(String, u32)>) {
    for (ext, count) in more {
        match into.iter_mut().find(|(had, _)| *had == ext) {
            Some((_, running)) => *running += count,
            None => into.push((ext, count)),
        }
    }
    into.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
}

#[derive(Debug, Clone, PartialEq)]
pub enum Ingested {
    Added,
    Updated {
        artist: String,
        title: String,
        changes: Vec<Change>,
    },
}

pub fn ingest_file(
    path: &Path,
    db: &Database,
    artwork_dir: Option<&Path>,
) -> Result<Ingested> {
    ingest(path, db, artwork_dir, Arrival::Now)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrival {
    Now,
    FromFile,
}

pub fn ingest(
    path: &Path,
    db: &Database,
    artwork_dir: Option<&Path>,
    arrival: Arrival,
) -> Result<Ingested> {
    let path = &crate::library::one_name_for(path);
    let path = path.as_path();
    let path_str = path.to_string_lossy().to_string();
    let mut track = metadata::read_metadata(path)?;
    let had = db.get_track_by_path(&path_str)?;
    let existed = had.is_some();
    if !existed && arrival == Arrival::Now {
        track.date_added = chrono::Local::now().naive_local();
    }

    if let Some(dir) = artwork_dir {
        let artist = crate::db::sort_key(crate::db::grouping_artist(&track));
        let album = crate::db::sort_key(&track.album);
        if let Err(e) = artwork::ensure(dir, &artist, &album, path, track.has_artwork) {
            debug!("no cover from {}: {}", path_str, e);
        }
    }

    db.upsert_track(&track)?;
    debug!("Imported: {} - {}", track.artist, track.title);
    Ok(match had {
        Some(had) => Ingested::Updated {
            artist: track.display_artist().to_string(),
            title: track.display_title().to_string(),
            changes: changes(&had, &track),
        },
        None => Ingested::Added,
    })
}

pub fn scan_directory(
    dir: &Path,
    db: &Database,
    artwork_dir: Option<&Path>,
    progress_tx: Option<&mpsc::Sender<ScanProgress>>,
    arrival: Arrival,
) -> Result<Swept> {
    let mut total = 0u32;
    let mut added = 0u32;
    let mut updated = 0u32;
    let mut foreign: std::collections::HashMap<String, u32> = std::collections::HashMap::new();

    if let Some(tx) = &progress_tx {
        let _ = tx.send(ScanProgress::StartingDir(dir.display().to_string()));
    }

    info!("Scanning directory: {}", dir.display());

    for entry in WalkDir::new(dir)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        if !path.is_file() {
            continue;
        }
        if !metadata::is_supported_audio(path) {
            if let Some(ext) = metadata::foreign_audio(path) {
                *foreign.entry(ext).or_insert(0) += 1;
            }
            continue;
        }

        total += 1;

        let path_str = path.to_string_lossy().to_string();
        let file_modified = std::fs::metadata(path)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| {
                t.duration_since(std::time::UNIX_EPOCH)
                    .ok()
                    .map(|d| d.as_secs())
            })
            .and_then(|secs| {
                chrono::DateTime::from_timestamp(secs as i64, 0)
                    .map(|dt| dt.naive_local().to_string())
            });

        let mut why = Reread::Changed;
        if let (Some(mod_time), Ok(Some(stored))) =
            (&file_modified, db.scan_state(&path_str))
        {
            let covered = match artwork_dir {
                Some(dir) => artwork::settled(dir, &stored.artist_key, &stored.album_key),
                None => true,
            };
            let nameless = stored.artist_key.is_empty() && stored.album_key.is_empty();
            if stored.modified == *mod_time && covered && !nameless {
                debug!("Skipping unchanged file: {}", path_str);
                continue;
            }
            if stored.modified == *mod_time {
                why = if nameless { Reread::Nameless } else { Reread::Cover };
            }
        }

        match ingest(path, db, artwork_dir, arrival) {
            Ok(Ingested::Added) => added += 1,
            Ok(Ingested::Updated { artist, title, changes }) => {
                updated += 1;
                if let Some(tx) = &progress_tx {
                    let _ = tx.send(ScanProgress::Updated(Update {
                        path: path_str.clone(),
                        artist,
                        title,
                        why,
                        changes,
                    }));
                }
            }
            Err(e) => {
                debug!("Failed to import {}: {}", path_str, e);
                if let Some(tx) = &progress_tx {
                    let _ = tx.send(ScanProgress::FileError {
                        path: path_str.clone(),
                        error: e.to_string(),
                    });
                }
            }
        }

        if let Some(tx) = &progress_tx {
            let _ = tx.send(ScanProgress::FileScanned {
                count: total,
                path: path_str,
            });
        }
    }

    info!(
        "Swept {}: {} files, {} added, {} updated",
        dir.display(),
        total,
        added,
        updated
    );

    let mut counted: Vec<(String, u32)> = foreign.into_iter().collect();
    counted.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    Ok(Swept {
        files: total,
        added,
        updated,
        foreign: counted,
    })
}

#[cfg(test)]
mod foreign_tests {
    use super::merge_foreign;

    #[test]
    fn two_folders_are_added_up_not_listed_twice() {
        let mut running = Vec::new();
        merge_foreign(&mut running, vec![("m4a".into(), 200), ("aiff".into(), 3)]);
        merge_foreign(&mut running, vec![("m4a".into(), 120), ("wma".into(), 1)]);

        assert_eq!(
            running,
            vec![
                ("m4a".to_string(), 320),
                ("aiff".to_string(), 3),
                ("wma".to_string(), 1),
            ]
        );
    }

    #[test]
    fn equal_counts_keep_a_fixed_order() {
        let mut running = Vec::new();
        merge_foreign(&mut running, vec![("wma".into(), 2), ("aiff".into(), 2)]);
        assert_eq!(
            running,
            vec![("aiff".to_string(), 2), ("wma".to_string(), 2)]
        );
    }

    #[test]
    fn nothing_to_add_changes_nothing() {
        let mut running = vec![("m4a".to_string(), 5)];
        merge_foreign(&mut running, Vec::new());
        assert_eq!(running, vec![("m4a".to_string(), 5)]);
    }
}

#[cfg(test)]
mod one_file_one_row_tests {
    use super::*;
    use crate::db::Database;
    use crate::library::fixtures;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("kessler-names-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn rows(db: &Database) -> Vec<String> {
        db.get_tracks(None, None, crate::db::models::SortField::Title,
                      crate::db::models::SortOrder::Ascending, None)
            .unwrap()
            .into_iter()
            .map(|t| t.path.to_string_lossy().to_string())
            .collect()
    }

    #[test]
    fn a_new_track_arrives_now_unless_the_library_is_being_read_for_the_first_time() {
        let root = scratch("arrival");
        let old = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_300_000_000);
        for name in ["a.mp3", "b.mp3"] {
            let path = root.join(name);
            fixtures::place(&path, fixtures::MP3);
            std::fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(old)
                .unwrap();
        }
        let db = Database::open_memory().unwrap();
        let year = |name: &str| {
            let path = crate::library::one_name_for(&root.join(name));
            db.get_track_by_path(&path.to_string_lossy())
                .unwrap()
                .unwrap()
                .date_added
                .format("%Y")
                .to_string()
        };

        ingest(&root.join("a.mp3"), &db, None, Arrival::FromFile).unwrap();
        assert_eq!(year("a.mp3"), "2011");
        ingest(&root.join("b.mp3"), &db, None, Arrival::Now).unwrap();
        assert_eq!(year("b.mp3"), chrono::Local::now().format("%Y").to_string());
        ingest(&root.join("a.mp3"), &db, None, Arrival::Now).unwrap();
        assert_eq!(year("a.mp3"), "2011");
    }

    #[test]
    fn changes_name_what_the_file_says_differently() {
        let then = chrono::Local::now().naive_local();
        let was = crate::db::models::Track {
            id: 1,
            path: "/music/1.mp3".into(),
            title: "a".into(),
            artist: "b".into(),
            album_artist: String::new(),
            album: "c".into(),
            genre: String::new(),
            year: Some(1999),
            track_number: Some(1),
            disc_number: None,
            duration_secs: 200.2,
            bitrate: Some(320),
            sample_rate: Some(44100),
            file_size: 1000,
            format: "mp3".into(),
            rating: 0,
            play_count: 3,
            last_played: None,
            date_added: then,
            date_modified: then,
            file_hash: None,
            has_artwork: false,
        };
        let now = crate::db::models::Track {
            year: Some(2001),
            genre: "jazz".into(),
            duration_secs: 200.4,
            play_count: 0,
            ..was.clone()
        };
        let found = changes(&was, &now);
        assert_eq!(
            found,
            vec![
                Change { field: Field::Genre, was: String::new(), now: "jazz".into() },
                Change { field: Field::Year, was: "1999".into(), now: "2001".into() },
            ]
        );
        assert!(changes(&was, &was).is_empty());
    }

    #[test]
    #[cfg(unix)]
    fn the_same_file_under_two_names_is_one_track() {
        let root = scratch("ingest");
        let real = root.join("Cave");
        std::fs::create_dir_all(&real).unwrap();
        fixtures::place(&real.join("01.mp3"), fixtures::MP3);
        let other = root.join("CAVE");
        std::os::unix::fs::symlink(&real, &other).unwrap();

        let db = Database::open_memory().unwrap();
        assert!(matches!(
            ingest_file(&real.join("01.mp3"), &db, None).unwrap(),
            Ingested::Added
        ));
        assert!(
            matches!(
                ingest_file(&other.join("01.mp3"), &db, None).unwrap(),
                Ingested::Updated { ref changes, .. } if changes.is_empty()
            ),
            "the same file under another name came in as a new track"
        );
        assert_eq!(rows(&db).len(), 1, "one file, one row");
        let _ = std::fs::remove_dir_all(&root);
    }
}
