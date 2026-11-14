use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use crate::error::Result;

#[derive(Debug, Clone, PartialEq)]
pub struct Pending {
    pub artist: String,
    pub title: String,
    pub album: String,
    pub album_artist: String,
    pub track_number: Option<u32>,
    pub duration_secs: u64,
    pub started_at: i64,
}

impl Pending {
    pub fn filable(&self) -> bool {
        !self.artist.is_empty() && !self.title.is_empty()
    }

    pub fn why_not_filable(&self) -> String {
        match (self.artist.is_empty(), self.title.is_empty()) {
            (true, true) => tr!("eine datei ohne künstler:in und ohne titel im tag — nicht gesendet")
                .to_string(),
            (true, false) => tr!("„{}“ hat keine künstler:in im tag — nicht gesendet",
                self.title
            ),
            _ => tr!("etwas von „{}“ hat keinen titel im tag — nicht gesendet",
                self.artist
            ),
        }
    }

    fn to_line(&self) -> String {
        let clean = |text: &str| text.replace(['\t', '\n', '\r'], " ");
        let mut line = String::new();
        let _ = write!(
            line,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}",
            self.started_at,
            clean(&self.artist),
            clean(&self.title),
            clean(&self.album),
            clean(&self.album_artist),
            self.track_number.map(|n| n.to_string()).unwrap_or_default(),
            self.duration_secs
        );
        line
    }

    fn from_line(line: &str) -> Option<Self> {
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 7 {
            return None;
        }
        Some(Self {
            started_at: fields[0].parse().ok()?,
            artist: fields[1].to_string(),
            title: fields[2].to_string(),
            album: fields[3].to_string(),
            album_artist: fields[4].to_string(),
            track_number: fields[5].parse().ok(),
            duration_secs: fields[6].parse().unwrap_or(0),
        })
    }
}

#[derive(Debug, Clone)]
pub struct Queue {
    path: PathBuf,
}

const MOST_KEPT: usize = 500;

impl Queue {
    pub fn at(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
        }
    }

    pub fn read(&self) -> Vec<Pending> {
        let Ok(text) = std::fs::read_to_string(&self.path) else {
            return Vec::new();
        };
        text.lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(Pending::from_line)
            .collect()
    }

    pub fn write(&self, pending: &[Pending]) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if pending.is_empty() {
            let _ = std::fs::remove_file(&self.path);
            return Ok(());
        }
        let keep = if pending.len() > MOST_KEPT {
            &pending[pending.len() - MOST_KEPT..]
        } else {
            pending
        };
        let temporary = self.path.with_extension("part");
        {
            let mut file = std::fs::File::create(&temporary)?;
            for entry in keep {
                writeln!(file, "{}", entry.to_line())?;
            }
        }
        std::fs::rename(&temporary, &self.path)?;
        Ok(())
    }

    #[cfg(test)]
    pub fn push(&self, entry: &Pending) -> Result<()> {
        let mut all = self.read();
        all.push(entry.clone());
        self.write(&all)
    }

    pub fn len(&self) -> usize {
        self.read().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kessler-queue-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("scrobbles.tsv")
    }

    fn pending(title: &str, at: i64) -> Pending {
        Pending {
            artist: "Boards of Canada".into(),
            title: title.into(),
            album: "Music Has the Right to Children".into(),
            album_artist: "Boards of Canada".into(),
            track_number: Some(3),
            duration_secs: 251,
            started_at: at,
        }
    }

    #[test]
    fn a_track_with_no_artist_or_no_title_is_not_filable() {
        let full = pending("Roygbiv", 1000);
        assert!(full.filable());

        let mut nameless = full.clone();
        nameless.artist = String::new();
        assert!(!nameless.filable());
        assert!(
            nameless.why_not_filable().contains("Roygbiv"),
            "the line has to name the track, or nobody can find the file: {}",
            nameless.why_not_filable()
        );
        assert!(nameless.why_not_filable().contains("künstler:in"));

        let mut untitled = full.clone();
        untitled.title = String::new();
        assert!(!untitled.filable());
        assert!(untitled.why_not_filable().contains("Boards of Canada"));
        assert!(untitled.why_not_filable().contains("titel"));

        let mut blank = full;
        blank.artist = String::new();
        blank.title = String::new();
        assert!(!blank.filable());
        assert!(blank.why_not_filable().contains("datei"));
    }

    #[test]
    fn what_goes_in_comes_back_out() {
        let path = scratch("roundtrip");
        let queue = Queue::at(&path);
        assert!(queue.read().is_empty());

        queue.push(&pending("telephasic workshop", 1000)).unwrap();
        queue.push(&pending("triangles & rhombuses", 1300)).unwrap();

        let back = queue.read();
        assert_eq!(back.len(), 2);
        assert_eq!(back[0], pending("telephasic workshop", 1000));
        assert_eq!(back[1].title, "triangles & rhombuses");
        assert_eq!(back[1].started_at, 1300);
    }

    #[test]
    fn a_tab_in_a_title_cannot_break_the_line() {
        let path = scratch("tabs");
        let queue = Queue::at(&path);
        let mut odd = pending("one\ttwo\nthree", 1000);
        odd.artist = "a\tb".into();
        queue.push(&odd).unwrap();

        let back = queue.read();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].title, "one two three");
        assert_eq!(back[0].artist, "a b");
        assert_eq!(back[0].duration_secs, 251);
    }

    #[test]
    fn a_broken_line_is_skipped_not_fatal() {
        let path = scratch("broken");
        std::fs::write(
            &path,
            "1000\tartist\ttitle\talbum\talbum artist\t1\t200\n\
             rubbish\n\
             \n\
             1200\tb\tt\ta\taa\t2\t300\n",
        )
        .unwrap();
        let back = Queue::at(&path).read();
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].started_at, 1000);
        assert_eq!(back[1].started_at, 1200);
    }

    #[test]
    fn an_emptied_queue_leaves_no_file() {
        let path = scratch("empty");
        let queue = Queue::at(&path);
        queue.push(&pending("olson", 1000)).unwrap();
        assert!(path.exists());
        queue.write(&[]).unwrap();
        assert!(!path.exists());
        assert_eq!(queue.len(), 0);
    }

    #[test]
    fn the_queue_stops_growing_somewhere() {
        let path = scratch("cap");
        let queue = Queue::at(&path);
        let many: Vec<Pending> = (0..MOST_KEPT as i64 + 40)
            .map(|i| pending("sixtyten", 1000 + i))
            .collect();
        queue.write(&many).unwrap();

        let back = queue.read();
        assert_eq!(back.len(), MOST_KEPT);
        assert_eq!(back.last().unwrap().started_at, 1000 + MOST_KEPT as i64 + 39);
    }
}
