use std::collections::HashSet;

use eframe::egui;
use std::path::PathBuf;
use std::sync::mpsc;

use crate::error::Result;
use crate::library::artistinfo::{self, ArtistInfo, Found, Own};

use super::tagedit::Entry;

pub type Answer = Result<(Found, bool, Option<Vec<u8>>)>;

#[derive(Default)]
pub struct GapsPanel {
    pub open: bool,
    pub came_from: bool,
    pub counted: Option<crate::library::gaps::Gaps>,
    pub counting: Option<mpsc::Receiver<crate::library::gaps::Gaps>>,
    pub stale: bool,
}

pub type PictureStamp = (u64, bool, bool, crate::gui::dither::Look);

#[derive(Default)]
pub struct ArtistPanel {
    pub name: Option<String>,
    pub key: u64,
    pub info: Option<ArtistInfo>,
    pub looking: Option<(String, u64, mpsc::Receiver<Answer>)>,
    pub asked: HashSet<u64>,
    pub trouble: Option<(u64, String)>,
    pub picture: Option<(u64, u32, PictureStamp, egui::TextureHandle)>,
    pub stamp: u64,
    pub picking: Option<(u64, String, mpsc::Receiver<Option<PathBuf>>)>,
    pub mine: Option<Mine>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mine {
    pub albums: usize,
    pub tracks: usize,
    pub seconds: f64,
    pub plays: u64,
    pub since: String,
    pub last_played: String,
}

impl Mine {
    pub fn of(tracks: &[crate::db::models::Track]) -> Option<Mine> {
        if tracks.is_empty() {
            return None;
        }
        let mut albums: Vec<String> = tracks.iter().map(|t| crate::db::sort_key(&t.album)).collect();
        albums.sort();
        albums.dedup();
        let since = tracks.iter().map(|t| t.date_added).min();
        let last = tracks.iter().filter_map(|t| t.last_played).max();
        let day = |d: chrono::NaiveDateTime| d.format("%Y-%m-%d").to_string();
        Some(Mine {
            albums: albums.len(),
            tracks: tracks.len(),
            seconds: tracks.iter().map(|t| t.duration_secs).sum(),
            plays: tracks.iter().map(|t| u64::from(t.play_count)).sum(),
            since: since.map(day).unwrap_or_default(),
            last_played: last.map(day).unwrap_or_default(),
        })
    }
}

impl ArtistPanel {
    pub fn looking_for(&self, key: u64) -> bool {
        self.looking.as_ref().is_some_and(|(_, k, _)| *k == key)
    }
}

pub struct OwnForm {
    pub key: u64,
    pub name: String,
    pub entries: [Entry; FIELDS],
    pub focus: usize,
    pub picture: bool,
}

pub const FIELDS: usize = 5;
pub const LABELS: [&str; FIELDS] = ["herkunft", "aktiv", "stil", "link", "notiz"];

pub const NOTE: usize = 4;

impl OwnForm {
    pub fn open(key: u64, name: &str, own: &Own) -> Self {
        let mut entries = [
            Entry::new(own.origin.clone()),
            Entry::new(own.active.clone()),
            Entry::new(own.style.clone()),
            Entry::new(own.link.clone()),
            Entry::new(own.note.clone()),
        ];
        for entry in &mut entries {
            entry.end();
        }
        OwnForm {
            key,
            name: name.to_string(),
            entries,
            focus: 0,
            picture: own.picture,
        }
    }

    pub fn own(&self) -> Own {
        let text = |i: usize| self.entries[i].text().trim().to_string();
        Own {
            origin: text(0),
            active: text(1),
            style: text(2),
            link: text(3),
            note: self.entries[NOTE].text().trim_end().to_string(),
            picture: self.picture,
        }
    }

    pub fn step(&mut self, delta: i32) {
        self.entries[self.focus].unmark();
        let n = FIELDS as i32;
        self.focus = (self.focus as i32 + delta).rem_euclid(n) as usize;
        self.entries[self.focus].end();
    }

    pub fn entry(&mut self) -> &mut Entry {
        &mut self.entries[self.focus]
    }
}

pub struct Hunt {
    pub news: mpsc::Receiver<HuntNews>,
    pub stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub total: usize,
    pub asked: usize,
    pub found: usize,
    pub again: bool,
}

pub enum HuntNews {
    Done(u64, bool),
    Failed(String),
}

impl Hunt {
    pub fn start(
        names: Vec<String>,
        dir: PathBuf,
        lang: &'static str,
        lastfm_key: Option<String>,
        again: bool,
    ) -> Hunt {
        let (tx, rx) = mpsc::channel();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stopped = std::sync::Arc::clone(&stop);
        let total = names.len();
        std::thread::spawn(move || {
            let mut failures = 0;
            for name in names {
                if stopped.load(std::sync::atomic::Ordering::Relaxed) {
                    return;
                }
                let key = artistinfo::key(&crate::db::sort_key(&name));
                let news = match artistinfo::look_up(&name, lang, lastfm_key.as_deref()) {
                    Ok((found, recognised, picture)) => {
                        failures = 0;
                        match artistinfo::take_in(&dir, key, &name, found, recognised, picture) {
                            Ok(_) => HuntNews::Done(key, recognised),
                            Err(e) => HuntNews::Failed(e.to_string()),
                        }
                    }
                    Err(e) => {
                        failures += 1;
                        if failures >= 3 {
                            HuntNews::Failed(e.to_string())
                        } else {
                            HuntNews::Done(key, false)
                        }
                    }
                };
                let failed = matches!(news, HuntNews::Failed(_));
                if tx.send(news).is_err() || failed {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(1100));
            }
        });
        Hunt { news: rx, stop, total, asked: 0, found: 0, again }
    }
}

pub fn start_looking(name: String, lang: &'static str, lastfm_key: Option<String>) -> mpsc::Receiver<Answer> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(artistinfo::look_up(&name, lang, lastfm_key.as_deref()));
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_form_hands_back_what_was_written() {
        let own = Own { origin: "Hebden Bridge".into(), picture: true, ..Own::default() };
        let mut form = OwnForm::open(1, "Hebden", &own);
        form.step(1);
        form.entry().insert(" 1995– ");
        form.focus = NOTE;
        form.entry().insert("erste zeile\nzweite\n\n");
        let back = form.own();
        assert_eq!(back.origin, "Hebden Bridge");
        assert_eq!(back.active, "1995–");
        assert_eq!(back.note, "erste zeile\nzweite");
        assert!(back.picture);
        form.step(1);
        assert_eq!(form.focus, 0);
    }
}
