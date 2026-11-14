use std::sync::mpsc;

use crate::library::lyrics::{self, Lyrics};

pub type Answer = std::result::Result<(Option<Lyrics>, String), String>;

pub type Song = (String, String);

#[derive(Default)]
pub struct LyricsPanel {
    pub of: Option<Song>,
    pub lyrics: Option<Lyrics>,
    pub looking: Option<(Song, mpsc::Receiver<Answer>)>,
    pub trouble: Option<String>,
}

impl LyricsPanel {
    pub fn looking_for(&self, song: &Song) -> bool {
        self.looking.as_ref().is_some_and(|(s, _)| s == song)
    }
}

pub fn start_looking(artist: String, title: String, secs: f64) -> mpsc::Receiver<Answer> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(lyrics::look_up(&artist, &title, secs).map_err(|e| e.to_string()));
    });
    rx
}
