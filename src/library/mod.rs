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

/// The one name a file is filed under, whatever name it was reached by.
///
/// A file can be reached by more than one name, and the library has to write
/// down exactly one of them — otherwise it keeps two rows for one recording,
/// which is what a record showing every track twice actually is.
///
/// The loud case on a Mac is capitals. The filesystem does not care about
/// them, so `…/Music/Cave/…` and `…/Music/CAVE/…` open the same file: adding a
/// record makes the folder with the spelling the tag has, the disk quietly
/// puts it in the folder already sitting there under the other spelling, and a
/// later scan then finds it under the name the disk really holds. Two
/// spellings, one file, two rows — and `path` being unique never noticed,
/// because as text those are two different paths. Symlinks and `..` arrive at
/// the same place by another route.
///
/// So the filesystem is asked, rather than the text being tidied up:
/// `canonicalize` follows the links and hands back the name the disk actually
/// holds, capitals and all. Folding the case of the string would be wrong
/// anyway — on Linux two files really can differ by nothing but a capital.
///
/// A file that is not there to be asked keeps the name it came with. Nothing
/// can be settled about a file that cannot be found, and guessing is how a
/// library loses a track.
pub fn one_name_for(path: &Path) -> PathBuf {
    let Ok(real) = std::fs::canonicalize(path) else {
        return path.to_path_buf();
    };
    match real.to_str().and_then(|s| s.strip_prefix(r"\\?\")) {
        Some(plain) => PathBuf::from(plain),
        None => real,
    }
}

/// What the folders along a path are really called.
///
/// `canonicalize` above straightens out links and `..`, but whether it also
/// corrects a capital is a question about the particular system, and this is
/// not a thing to guess at: the difference between `Cave` and `CAVE` is the
/// whole bug. So the directories are asked, one level at a time — a folder
/// that holds `Cave` will happily open `CAVE` on a Mac, but only one of those
/// is the name it keeps, and the name it keeps is what a scan will find.
///
/// What does not exist yet keeps the spelling it was given. That is what makes
/// this useful for filing a new record as well as for reading an old row: a
/// new album folder inside an artist folder that is already there gets the
/// artist's own spelling and its own.
///
/// `seen` is a memory of the directories already read. Without it this is a
/// directory listing per component per file, which is fine for filing one
/// record and much too slow for a whole library.
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

/// A name as it compares: one way of writing each letter, small.
fn folded(name: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    name.nfc().collect::<String>().to_lowercase()
}

/// Directory listings already read, for [`as_the_disk_spells_it`].
pub type DiskNames = std::collections::HashMap<PathBuf, Vec<String>>;

/// High-level library management operations.
pub struct LibraryManager<'a> {
    db: &'a Database,
    /// Where extracted album covers go. Without one, scanning skips them —
    /// the terminal frontend has no use for pictures.
    artwork: Option<std::path::PathBuf>,
}

impl<'a> LibraryManager<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db, artwork: None }
    }

    /// Also extract album covers into this folder while scanning.
    pub fn with_artwork(mut self, dir: impl Into<std::path::PathBuf>) -> Self {
        self.artwork = Some(dir.into());
        self
    }

    /// Scan one or more directories and import all music files.
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

    /// Remove tracks from the database whose files no longer exist on disk.
    pub fn clean_missing(&self) -> Result<u32> {
        self.db.remove_missing_tracks()
    }

    /// The same, after a scan of these folders, and only within them: what the
    /// window does each time it reads the library.
    ///
    /// A folder that is not there, or holds no music at all, is passed over
    /// rather than emptied: that is an unplugged disk or a folder the system
    /// will not let us read, and taking it at its word would throw away the
    /// whole library — its lists and its play counts with it — on a day the
    /// drive was simply not connected.
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

    /// A file deleted from a music folder leaves the library on the next
    /// read; one that is still there stays, and so does everything under a
    /// folder that is not there at all — an unplugged disk is not a library
    /// thrown away.
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

    /// The name the disk keeps wins over the name we were handed.
    ///
    /// This is the whole of a doubled record in one assertion: the tag
    /// said CAVE, the folder on the disk said Cave, and on a Mac both open the
    /// same place. Asked here rather than left to the filesystem, so the
    /// answer is the same on every system.
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

    /// The same for an umlaut in capitals, and for an umlaut written as a
    /// vowel and its dots (NFD, as folders from older Macs hold it) where the
    /// tag has it as one letter.
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

    /// What is not there yet is left exactly as asked for — that is how a new
    /// record gets filed at all.
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

    /// And where a system does tell capitals apart, the name asked for is the
    /// name meant: an exact match is never passed over for a near one.
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
