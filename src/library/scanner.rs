use std::path::Path;
use std::sync::mpsc;
use tracing::{debug, info};
use walkdir::WalkDir;

use super::{artwork, metadata};
use crate::db::Database;
use crate::error::Result;

/// Progress updates from the scanner.
#[derive(Debug, Clone)]
pub enum ScanProgress {
    /// How many files the scan is about to work through, counted before it
    /// starts. Without it "scanning" is a word with no end in sight; with it
    /// the interface can say how far along it is.
    Counted { total: u32 },
    /// Starting scan of a directory.
    StartingDir(String),
    /// Scanned a file (current count, file path).
    FileScanned { count: u32, path: String },
    /// Completed scanning.
    Complete { total: u32, added: u32, updated: u32 },
    /// Music that was walked past because Kessler cannot play the format,
    /// counted per extension and reported once at the end.
    ///
    /// Silence here is what made "Swinsian lists more tracks than we do" an
    /// unanswerable question: the files were never mentioned, because they
    /// were never opened.
    Foreign { counted: Vec<(String, u32)> },
    /// Error scanning a specific file.
    FileError { path: String, error: String },
    /// A track the library already had, read again — why, and what came out
    /// different. For the log: „26 aktualisiert" on its own left the question
    /// which ones and what about them.
    Updated(Update),
}

/// One track read again by a scan: what it is, why it was read, and what
/// is different now. See [`ScanProgress::Updated`].
#[derive(Debug, Clone, PartialEq)]
pub struct Update {
    pub path: String,
    pub artist: String,
    pub title: String,
    pub why: Reread,
    pub changes: Vec<Change>,
}

/// Why a scan read a file it had read before.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reread {
    /// The file is newer than when it was last read.
    Changed,
    /// Its record's cover has not been settled yet — see `artwork::settled`.
    Cover,
    /// It came out with neither artist nor album last time, and such a file
    /// is read every time — see `scan_directory`.
    Nameless,
}

/// One field of a track that came out different, as it was and as it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub field: Field,
    pub was: String,
    pub now: String,
}

/// The fields a file says about itself, as far as the library keeps them.
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

/// What came out different between what the library had and what the file
/// says now. Only what the file says: plays, ratings and dates are the
/// library's own and a scan leaves them alone.
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

/// How many playable files a directory tree holds.
///
/// A separate walk before the real one. It costs a directory listing and an
/// extension check per file — next to nothing beside the scan itself, which
/// opens and parses every one of them — and it buys a progress figure that
/// means something.
pub fn count_files(dir: &Path) -> u32 {
    WalkDir::new(dir)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file() && metadata::is_supported_audio(e.path()))
        .count() as u32
}

/// What sweeping one directory came to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Swept {
    pub files: u32,
    pub added: u32,
    pub updated: u32,
    /// Music passed over for its format, counted per extension.
    pub foreign: Vec<(String, u32)>,
}

/// Add one directory's tally of passed-over formats to the running one.
///
/// A scan walks each music folder in turn and reports each one's count
/// separately, so both the terminal and the window have to add them up. Doing
/// that by appending would list `m4a` once per folder — three lines saying
/// three different numbers, none of them the answer.
pub fn merge_foreign(into: &mut Vec<(String, u32)>, more: Vec<(String, u32)>) {
    for (ext, count) in more {
        match into.iter_mut().find(|(had, _)| *had == ext) {
            Some((_, running)) => *running += count,
            None => into.push((ext, count)),
        }
    }
    into.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
}

/// What reading one file into the library did.
#[derive(Debug, Clone, PartialEq)]
pub enum Ingested {
    Added,
    /// Read again, with what came out different — nothing, often: see
    /// [`Reread`] for why a file is read again without having changed.
    Updated {
        artist: String,
        title: String,
        changes: Vec<Change>,
    },
}

/// Read one file and put what it says into the database, cover and all.
///
/// The single step a scan is made of, lifted out so that it is also the step
/// taken after the tag editor has written to a file: what the library knows
/// about a track is then read back from the track, exactly as a scan would
/// have read it, rather than patched in place from what the form thought it
/// had done.
pub fn ingest_file(
    path: &Path,
    db: &Database,
    artwork_dir: Option<&Path>,
) -> Result<Ingested> {
    ingest(path, db, artwork_dir, Arrival::Now)
}

/// When a track new to the library counts as having arrived.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrival {
    /// Now, when it was found. Right for anything that comes into a library
    /// that already exists — a record added through the window, or dropped
    /// into the music folder and found at the next start.
    ///
    /// The file's own date is the wrong answer there, and it was the answer
    /// for a long time: a copy keeps the date the file was *made* — on a Mac
    /// even the program's own „hinzufügen" keeps it, and so does every
    /// download that comes out of a zip — so a record added this morning
    /// could be dated 2011 and never reach „zuletzt".
    Now,
    /// Its own date. Right only for the first reading of an empty library,
    /// where everything is being found at once and „now" would make it all
    /// equally new — and „newest first" would have nothing to say.
    FromFile,
}

/// [`ingest_file`], saying when a new track arrived.
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

/// Scan a directory tree for music files and import them into the database.
/// Sends progress updates through the provided channel.
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

/// One file, two names.
///
/// A library had a record whose every track showed twice. Two rows, two
/// paths — `…/Music/Cave/…` and `…/Music/CAVE/…` — and on a Mac those open the
/// same file, so the unique path never caught it.
///
/// Linux is not obliging enough to have a filesystem that ignores capitals, so
/// the same shape is built with a symlink: one folder reachable under two
/// names. It is the same question — is this file already in the library, asked
/// of a name rather than of the file — and the same answer.
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

    /// A record copied in keeps the date its files were made; in a library
    /// that exists it has still arrived now, and „zuletzt" has to show it.
    /// Only the first reading of an empty library goes by the files.
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

    /// What a scan says it changed: only what the file says, each field as
    /// it was and as it is, and the length only to the second.
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
