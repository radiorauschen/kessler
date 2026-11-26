//! The artist's column: what is known about whoever is being looked at.
//!
//! The state is here; the drawing is in [`super::library`] with the other
//! columns, and the looking up and keeping in [`crate::library::artistinfo`].
//!
//! It follows the window: the name picked in the first column, else the
//! artist of the track under the cursor, else of what is playing. The first
//! time an artist is shown the sources are asked, once, on a thread of their
//! own; after that what they said is read off disk, network or not, until a
//! new search is asked for.

use std::collections::HashSet;

use eframe::egui;
use std::path::PathBuf;
use std::sync::mpsc;

use crate::error::Result;
use crate::library::artistinfo::{self, ArtistInfo, Own};

use super::tagedit::Entry;

/// What a search sends back: what was found, whether the name was
/// recognised at all, and a picture if one turned up.
pub type Answer = Result<Taken>;

/// A search's answer once it is on disk: what is there now about the artist,
/// whether the picture changed, whether the sources knew them, and what was
/// found in a few words for the log.
pub struct Taken {
    pub info: ArtistInfo,
    pub pictured: bool,
    pub recognised: bool,
    pub summary: String,
}

/// The other artists: the gaps in the library, in the fourth column — see
/// [`crate::library::gaps`].
#[derive(Default)]
pub struct GapsPanel {
    /// They have the fourth column.
    pub open: bool,
    /// The artist shown was clicked on in the gaps: the way back to them.
    pub came_from: bool,
    /// The last count, kept while a new one is under way.
    pub counted: Option<crate::library::gaps::Gaps>,
    /// A count on its own thread: reading every artist's file is quick, but
    /// not one frame's worth quick in a big library.
    pub counting: Option<mpsc::Receiver<crate::library::gaps::Gaps>>,
    /// Something new was found about an artist since the last count.
    pub stale: bool,
}

/// The column's state.
/// What the artist's picture was made at: the info's stamp, whether
/// screened from the negative, whether screened at all, and how.
pub type PictureStamp = (u64, bool, bool, crate::gui::dither::Look);

#[derive(Default)]
pub struct ArtistPanel {
    /// Who is shown, as the library spells them, and their key.
    pub name: Option<String>,
    pub key: u64,
    /// What is on disk about them; `None` for nothing yet.
    pub info: Option<ArtistInfo>,
    /// A search under way: for whom, and where the answer arrives.
    pub looking: Option<(String, u64, mpsc::Receiver<Answer>)>,
    /// Who has been searched for by themselves this session, so an artist
    /// the sources know nothing about is not asked about again every time
    /// they come round.
    pub asked: HashSet<u64>,
    /// What the last search said went wrong, for whom.
    pub trouble: Option<(u64, String)>,
    /// The picture as shown: key, grain, the stamp it was made at, whether
    /// it was screened from the negative, whether screened at all and how,
    /// texture.
    pub picture: Option<(u64, u32, PictureStamp, egui::TextureHandle)>,
    /// Bumped whenever the picture on disk changes, so the texture is made
    /// again.
    pub stamp: u64,
    /// A picture file being chosen, and for whom.
    pub picking: Option<(u64, String, mpsc::Receiver<Option<PathBuf>>)>,
    /// What the library itself holds of them, worked out when they are
    /// first shown — see [`Mine`].
    pub mine: Option<Mine>,
}

/// The artist in the library's own figures: how much of them there is, how
/// much of it has been listened to, since when.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mine {
    pub albums: usize,
    pub tracks: usize,
    pub seconds: f64,
    pub plays: u64,
    /// When the first of it arrived, and when any of it was last played,
    /// as `YYYY-MM-DD`.
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
    /// Whether a search for this key is running.
    pub fn looking_for(&self, key: u64) -> bool {
        self.looking.as_ref().is_some_and(|(_, k, _)| *k == key)
    }
}

/// The form for what the person writes in themselves.
pub struct OwnForm {
    pub key: u64,
    pub name: String,
    pub entries: [Entry; FIELDS],
    pub focus: usize,
    /// Kept from before, as the form does not edit it.
    pub picture: bool,
}

/// How many fields the form has, and what they are called, in order.
pub const FIELDS: usize = 5;
pub const LABELS: [&str; FIELDS] = ["herkunft", "aktiv", "stil", "link", "notiz"];

/// The note is the one field that holds more than a line.
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

/// A run through every artist in the library the sources have not been asked
/// about yet, so that the column has everything before it is opened. On a
/// thread of its own, at MusicBrainz's pace; what it finds it writes to disk
/// itself — those are its own files, not the library's — and says here only
/// whom it has done.
pub struct Hunt {
    pub news: mpsc::Receiver<HuntNews>,
    pub stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub total: usize,
    pub asked: usize,
    pub found: usize,
    /// Everyone again, rather than only those never asked about.
    pub again: bool,
}

pub enum HuntNews {
    /// One more done: its key, and whether the sources knew them.
    Done(u64, bool),
    /// The network is not answering; the run ends here.
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

/// Look `name` up on a thread of its own; the answer arrives on the
/// receiver.
/// Ask the sources about an artist on a thread of its own, and put what they
/// say on disk there too: the picture is decoded, cut square and written as
/// a JPEG, which took the window most of a tenth of a second when it was done
/// on its own thread.
pub fn start_looking(name: String, lang: &'static str, lastfm_key: Option<String>, dir: std::path::PathBuf, key: u64) -> mpsc::Receiver<Answer> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let answer = artistinfo::look_up(&name, lang, lastfm_key.as_deref()).and_then(|(found, recognised, picture)| {
            let summary = artistinfo::summary(&found, picture.is_some());
            let (info, pictured) = artistinfo::take_in(&dir, key, &name, found, recognised, picture)?;
            Ok(Taken { info, pictured, recognised, summary })
        });
        let _ = tx.send(answer);
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The form hands back what was typed, tidied, and keeps the picture's
    /// flag it does not show.
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
