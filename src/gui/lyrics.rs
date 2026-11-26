//! The lyrics column's state: whose words it shows, what it found, and the
//! question out to LRCLIB — see [`crate::library::lyrics`].

use std::sync::mpsc;

use crate::library::lyrics::{self, Lyrics};

/// What LRCLIB answered: the words, or that it had none, and the text to
/// keep — or why it could not be asked.
pub type Answer = std::result::Result<(Option<Lyrics>, String), String>;

/// The song the column is about: artist and title, as they are asked for.
pub type Song = (String, String);

#[derive(Default)]
pub struct LyricsPanel {
    /// Whose words are shown.
    pub of: Option<Song>,
    /// The words, once there are any. `None` while asking, and for a song
    /// that has none anywhere.
    pub lyrics: Option<Lyrics>,
    /// The question out to LRCLIB, and whom it is about.
    pub looking: Option<(Song, mpsc::Receiver<Answer>)>,
    /// Why the last question went unanswered.
    pub trouble: Option<String>,
}

impl LyricsPanel {
    /// Whether the column is waiting for an answer about the song it shows.
    pub fn looking_for(&self, song: &Song) -> bool {
        self.looking.as_ref().is_some_and(|(s, _)| s == song)
    }
}

/// Ask LRCLIB on a thread of its own.
pub fn start_looking(artist: String, title: String, secs: f64) -> mpsc::Receiver<Answer> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(lyrics::look_up(&artist, &title, secs).map_err(|e| e.to_string()));
    });
    rx
}
