pub mod artistinfo;
pub mod artwork;
#[cfg(test)]
pub mod fixtures;
#[cfg(test)]
mod smoke;

pub mod coverart;
pub mod details;
pub mod id3;
pub mod import;
pub mod lyrics;
pub mod loudness;
pub mod metadata;
pub mod scanner;
pub mod tags;
pub mod gaps;
pub mod genres;
pub mod years;

use std::path::{Path, PathBuf};
use std::sync::mpsc;

use crate::db::Database;
use crate::error::Result;

pub fn one_name_for(path: &Path) -> PathBuf {
    let Ok(real) = std::fs::canonicalize(path) else {
        return path.to_path_buf();
    };
    match real.to_str().and_then(|s| s.strip_prefix(r"\\?\")) {
        Some(plain) => PathBuf::from(plain),
        None => real,
    }
}

pub fn as_the_disk_spells_it(path: &Path, seen: &mut DiskNames) -> PathBuf {
    let mut spelled = PathBuf::new();
    for part in path.components() {
        let piece: &Path = part.as_ref();
        let Some(wanted) = piece.to_str() else {
            spelled.push(piece);
            continue;
        };
        if !matches!(part, std::path::Component::Normal(_)) || spelled.as_os_str().is_empty() {
            spelled.push(piece);
            continue;
        }
        let here = seen
            .entry(spelled.clone())
            .or_insert_with(|| match std::fs::read_dir(&spelled) {
                Ok(entries) => entries
                    .filter_map(|e| e.ok())
                    .filter_map(|e| e.file_name().to_str().map(str::to_string))
                    .collect(),
                Err(_) => Vec::new(),
            });
        let key = folded(wanted);
        let found = here
            .iter()
            .find(|name| name.as_str() == wanted)
            .or_else(|| here.iter().find(|name| folded(name) == key));
        match found {
            Some(real) => spelled.push(real),
            None => spelled.push(piece),
        }
    }
    spelled
}

fn folded(name: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    name.nfc().collect::<String>().to_lowercase()
}

pub type DiskNames = std::collections::HashMap<PathBuf, Vec<String>>;

pub struct LibraryManager<'a> {
    db: &'a Database,
    artwork: Option<std::path::PathBuf>,
}

impl<'a> LibraryManager<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db, artwork: None }
    }

    pub fn with_artwork(mut self, dir: impl Into<std::path::PathBuf>) -> Self {
        self.artwork = Some(dir.into());
        self
    }

    pub fn scan_dirs(
        &self,
        dirs: &[impl AsRef<Path>],
        progress_tx: Option<&mpsc::Sender<scanner::ScanProgress>>,
    ) -> Result<(u32, u32, u32)> {
        let mut total_files = 0u32;
        let mut total_added = 0u32;
        let mut total_updated = 0u32;

        if let Some(tx) = progress_tx {
            let total: u32 = dirs
                .iter()
                .map(|d| d.as_ref())
                .filter(|d| d.exists())
                .map(scanner::count_files)
                .sum();
            let _ = tx.send(scanner::ScanProgress::Counted { total });
        }

        let mut foreign: Vec<(String, u32)> = Vec::new();

        let arrival = if self.db.track_count()? == 0 {
            scanner::Arrival::FromFile
        } else {
            scanner::Arrival::Now
        };

        for dir in dirs {
            let dir = dir.as_ref();
            if !dir.exists() {
                tracing::warn!("Music directory does not exist: {}", dir.display());
                continue;
            }
            let swept =
                scanner::scan_directory(
                    dir,
                    self.db,
                    self.artwork.as_deref(),
                    progress_tx,
                    arrival,
                )?;
            total_files += swept.files;
            total_added += swept.added;
            total_updated += swept.updated;
            scanner::merge_foreign(&mut foreign, swept.foreign);
        }

        if let Some(tx) = progress_tx {
            if !foreign.is_empty() {
                let _ = tx.send(scanner::ScanProgress::Foreign {
                    counted: foreign,
                });
            }
            let _ = tx.send(scanner::ScanProgress::Complete {
                total: total_files,
                added: total_added,
                updated: total_updated,
            });
        }

        Ok((total_files, total_added, total_updated))
    }

    pub fn clean_missing(&self) -> Result<u32> {
        self.db.remove_missing_tracks()
    }

    pub fn clean_missing_in(&self, dirs: &[impl AsRef<Path>]) -> Result<u32> {
        let mut removed = 0;
        for dir in dirs {
            let dir = dir.as_ref();
            if !dir.is_dir() || scanner::count_files(dir) == 0 {
                continue;
            }
            removed += self.db.remove_missing_under(dir)?;
        }
        Ok(removed)
    }
}

#[cfg(test)]
mod pruning_tests {
    use super::*;
    use crate::db::models::Track;

    fn track(path: &Path) -> Track {
        Track {
            id: 0,
            path: path.to_path_buf(),
            title: "t".into(),
            artist: "a".into(),
            album_artist: "a".into(),
            album: "b".into(),
            genre: String::new(),
            year: None,
            track_number: None,
            disc_number: None,
            duration_secs: 1.0,
            bitrate: None,
            sample_rate: None,
            file_size: 1,
            format: "mp3".into(),
            rating: 0,
            play_count: 0,
            last_played: None,
            date_added: chrono::NaiveDateTime::default(),
            date_modified: chrono::NaiveDateTime::default(),
            file_hash: None,
            has_artwork: false,
        }
    }

    #[test]
    fn a_read_takes_out_what_has_gone_and_nothing_else() {
        let root = std::env::temp_dir().join("kessler-pruning");
        let _ = std::fs::remove_dir_all(&root);
        let music = root.join("music");
        std::fs::create_dir_all(&music).unwrap();
        let kept = music.join("kept.mp3");
        std::fs::write(&kept, fixtures::bytes(fixtures::MP3)).unwrap();
        let gone = music.join("gone.mp3");
        let elsewhere = root.join("elsewhere").join("gone.mp3");
        let unplugged = root.join("unplugged");
        let on_the_disk = unplugged.join("gone.mp3");

        let db = Database::open_memory().unwrap();
        for path in [&kept, &gone, &elsewhere, &on_the_disk] {
            db.upsert_track(&track(path)).unwrap();
        }
        let removed = LibraryManager::new(&db).clean_missing_in(&[&music, &unplugged]).unwrap();
        assert_eq!(removed, 1);
        assert_eq!(db.track_count().unwrap(), 3);
        let _ = std::fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
mod disk_spelling_tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kessler-spelling-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_folder_keeps_the_spelling_it_already_has() {
        let root = scratch("kept");
        std::fs::create_dir_all(root.join("Cave").join("Allways")).unwrap();

        let asked = root.join("CAVE").join("Allways").join("02 San'Yago.mp3");
        let spelled = as_the_disk_spells_it(&asked, &mut DiskNames::new());
        assert_eq!(
            spelled,
            root.join("Cave").join("Allways").join("02 San'Yago.mp3"),
            "the folder's own spelling was not kept"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_umlaut_finds_its_folder_however_it_is_written() {
        let root = scratch("umlaut");
        let decomposed = "Bjo\u{308}rk";
        std::fs::create_dir_all(root.join("Ämter").join(decomposed)).unwrap();

        let asked = root.join("ÄMTER").join("Björk").join("01 Über.mp3");
        assert_eq!(
            as_the_disk_spells_it(&asked, &mut DiskNames::new()),
            root.join("Ämter").join(decomposed).join("01 Über.mp3"),
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_folder_that_is_not_there_yet_keeps_what_it_was_given() {
        let root = scratch("new");
        let asked = root.join("Alda").join("Tahoma").join("01 Tahoma.mp3");
        assert_eq!(
            as_the_disk_spells_it(&asked, &mut DiskNames::new()),
            asked,
            "a name nobody has taken was changed anyway"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    #[cfg(unix)]
    fn an_exact_name_wins_over_one_that_only_looks_like_it() {
        let root = scratch("both");
        std::fs::create_dir_all(root.join("Cave")).unwrap();
        let _ = std::fs::create_dir(root.join("CAVE"));
        if !root.join("CAVE").is_dir() || std::fs::read_dir(&root).unwrap().count() < 2 {
            let _ = std::fs::remove_dir_all(&root);
            return;
        }
        let asked = root.join("CAVE").join("01.mp3");
        assert_eq!(as_the_disk_spells_it(&asked, &mut DiskNames::new()), asked);
        let asked = root.join("Cave").join("01.mp3");
        assert_eq!(as_the_disk_spells_it(&asked, &mut DiskNames::new()), asked);
        let _ = std::fs::remove_dir_all(&root);
    }
}
