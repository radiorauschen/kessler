use std::collections::VecDeque;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::NaiveDateTime;

pub const KEEP: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Scrobble,
    Lastfm,
    Tag,
    Year,
    Genre,
    Cover,
    Ipod,
    Library,
    List,
    Artist,
    Radio,
    Podcast,
}

impl Kind {
    pub const ALL: [Kind; 12] = [
        Kind::Scrobble,
        Kind::Lastfm,
        Kind::Tag,
        Kind::Year,
        Kind::Genre,
        Kind::Cover,
        Kind::Ipod,
        Kind::Library,
        Kind::List,
        Kind::Artist,
        Kind::Radio,
        Kind::Podcast,
    ];

    pub fn word(self) -> &'static str {
        match self {
            Kind::Scrobble => "scrobble",
            Kind::Lastfm => "last.fm",
            Kind::Tag => "idtag",
            Kind::Year => "jahr",
            Kind::Genre => "genre",
            Kind::Cover => "cover",
            Kind::Ipod => "ipod",
            Kind::Library => "bibliothek",
            Kind::List => "liste",
            Kind::Artist => "künstler:in",
            Kind::Radio => "radio",
            Kind::Podcast => "podcast",
        }
    }

    fn from_word(word: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.word() == word)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub at: NaiveDateTime,
    pub kind: Kind,
    pub text: String,
}

#[derive(Default)]
pub struct Log {
    entries: VecDeque<Entry>,
    path: Option<PathBuf>,
    appended: usize,
}

const FORMAT: &str = "%Y-%m-%d %H:%M:%S";

fn line(entry: &Entry) -> String {
    let text: String = entry
        .text
        .chars()
        .map(|c| if c == '\t' || c == '\n' || c == '\r' { ' ' } else { c })
        .collect();
    format!("{}\t{}\t{}", entry.at.format(FORMAT), entry.kind.word(), text)
}

fn parse(line: &str) -> Option<Entry> {
    let mut parts = line.splitn(3, '\t');
    let at = NaiveDateTime::parse_from_str(parts.next()?, FORMAT).ok()?;
    let kind = Kind::from_word(parts.next()?)?;
    let text = parts.next()?.to_string();
    Some(Entry { at, kind, text })
}

impl Log {
    pub fn open(path: &Path) -> Self {
        let mut entries: VecDeque<Entry> = std::fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .filter_map(parse)
            .collect();
        while entries.len() > KEEP {
            entries.pop_front();
        }
        Self {
            entries,
            path: Some(path.to_path_buf()),
            appended: 0,
        }
    }

    pub fn add(&mut self, kind: Kind, text: impl Into<String>) {
        let entry = Entry {
            at: chrono::Local::now().naive_local(),
            kind,
            text: text.into(),
        };
        self.push(entry);
    }

    fn push(&mut self, entry: Entry) {
        if let Some(path) = &self.path {
            if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
                let _ = writeln!(file, "{}", line(&entry));
            }
        }
        self.entries.push_back(entry);
        while self.entries.len() > KEEP {
            self.entries.pop_front();
        }
        self.appended += 1;
        if self.appended >= KEEP {
            self.appended = 0;
            self.rewrite();
        }
    }

    fn rewrite(&self) {
        let Some(path) = &self.path else { return };
        let mut all = String::new();
        for entry in &self.entries {
            all.push_str(&line(entry));
            all.push('\n');
        }
        let _ = std::fs::write(path, all);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.appended = 0;
        self.rewrite();
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn markdown(&self, today: chrono::NaiveDate) -> String {
        let mut out = format!("# kessler {} · log\n", crate::VERSION_SHOWN);
        let mut day = None;
        for entry in self.entries.iter().rev() {
            let date = entry.at.date();
            if day != Some(date) {
                out.push_str(&format!("\n## {} ({})\n\n", date.format("%Y-%m-%d"), self::day(date, today)));
                day = Some(date);
            }
            let text = entry.text.replace(['\n', '\r'], " ");
            out.push_str(&format!("- {} `{}` {}\n", time(entry.at), entry.kind.word(), text));
        }
        out
    }

    pub fn nth_newest(&self, at: usize) -> Option<&Entry> {
        self.entries.len().checked_sub(at + 1).and_then(|i| self.entries.get(i))
    }
}

pub fn time(at: NaiveDateTime) -> String {
    at.format("%H:%M").to_string()
}

pub fn day(date: chrono::NaiveDate, today: chrono::NaiveDate) -> String {
    use chrono::Datelike;
    if date == today {
        tr!("heute").to_string()
    } else if today.pred_opt() == Some(date) {
        tr!("gestern").to_string()
    } else if date.year() == today.year() {
        date.format("%-d.%-m.").to_string()
    } else {
        date.format("%-d.%-m.%Y").to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(text, FORMAT).unwrap()
    }

    #[test]
    fn a_cleared_log_stays_cleared() {
        let dir = std::env::temp_dir().join(format!("kessler-log-cleared-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("log.txt");
        let mut log = Log::open(&path);
        log.add(Kind::Scrobble, "cluster — rosa");
        assert_eq!(Log::open(&path).len(), 1);
        log.clear();
        assert!(log.is_empty());
        assert!(Log::open(&path).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_line_comes_back_as_the_entry_it_was() {
        let entry = Entry {
            at: at("2026-09-25 14:02:11"),
            kind: Kind::Scrobble,
            text: "cluster — rosa\twith a tab".into(),
        };
        let back = parse(&line(&entry)).unwrap();
        assert_eq!(back.at, entry.at);
        assert_eq!(back.kind, Kind::Scrobble);
        assert_eq!(back.text, "cluster — rosa with a tab");
        assert_eq!(parse("rubbish"), None);
        assert_eq!(parse("2026-09-25 14:02:11\tnothing\ttext"), None);
    }

    #[test]
    fn the_log_is_kept_in_its_file_and_cut_back_to_what_is_kept() {
        let dir = std::env::temp_dir().join(format!("kessler-log-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("log.txt");
        let _ = std::fs::remove_file(&path);

        let mut log = Log::open(&path);
        assert_eq!(log.len(), 0);
        log.add(Kind::Year, "amber → 1994");
        log.add(Kind::Scrobble, "cluster — rosa");
        let again = Log::open(&path);
        assert_eq!(again.len(), 2);
        assert_eq!(again.nth_newest(0).unwrap().text, "cluster — rosa");
        assert_eq!(again.nth_newest(1).unwrap().kind, Kind::Year);

        let mut log = again;
        for n in 0..(KEEP * 2 + 5) {
            log.add(Kind::Scrobble, format!("{n}"));
        }
        assert_eq!(log.len(), KEEP);
        let lines = std::fs::read_to_string(&path).unwrap().lines().count();
        assert!(lines <= KEEP * 2, "the file grew to {lines} lines");
        assert_eq!(Log::open(&path).len(), KEEP);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_day_is_said_as_briefly_as_is_still_clear() {
        let today = at("2026-09-25 09:00:00").date();
        assert_eq!(time(at("2026-09-25 14:02:11")), "14:02");
        assert_eq!(day(today, today), "heute");
        assert_eq!(day(at("2026-09-24 23:59:00").date(), today), "gestern");
        assert_eq!(day(at("2026-09-02 10:00:00").date(), today), "2.9.");
        assert_eq!(day(at("2025-12-31 10:00:00").date(), today), "31.12.2025");
    }

    #[test]
    fn the_log_goes_out_as_markdown() {
        let mut log = Log::default();
        log.entries.push_back(Entry { at: at("2026-10-21 23:10:00"), kind: Kind::Radio, text: "nts 1: verbunden".into() });
        log.entries.push_back(Entry { at: at("2026-10-22 09:05:00"), kind: Kind::Scrobble, text: "cluster — rosa\nzwei".into() });
        let today = chrono::NaiveDate::from_ymd_opt(2026, 10, 22).unwrap();
        let said = log.markdown(today);
        let lines: Vec<&str> = said.lines().filter(|l| !l.is_empty()).collect();
        assert!(lines[0].starts_with("# kessler "));
        assert_eq!(lines[1], "## 2026-10-22 (heute)");
        assert_eq!(lines[2], "- 09:05 `scrobble` cluster — rosa zwei");
        assert_eq!(lines[3], "## 2026-10-21 (gestern)");
        assert_eq!(lines[4], "- 23:10 `radio` nts 1: verbunden");
    }
}
