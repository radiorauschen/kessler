use crate::gui::*;
use crate::library::lyrics;

impl GuiApp {
    pub fn toggle_lyrics(&mut self) {
        if self.lyrics_open {
            self.lyrics_open = false;
            self.follow_the_playing();
            return;
        }
        if !self.all_sides() {
            if self.device_shown() {
                self.close_device_panel();
            }
            self.log_open = false;
            self.artist_open = false;
            self.artist_form = None;
            self.gaps.open = false;
            self.eq_open = false;
        }
        self.lyrics_open = true;
        self.leave_the_left();
    }

    pub fn shown_song(&self) -> Option<Track> {
        if self.on_air.is_some() {
            return self.aired_song();
        }
        self.playing_library_track()
            .cloned()
            .or_else(|| self.tracks.get(self.track_index).cloned())
            .filter(|t| !t.title.trim().is_empty())
    }

    pub fn follow_the_words(&mut self) {
        let Some(track) = self.shown_song() else {
            self.lyrics.of = None;
            self.lyrics.lyrics = None;
            return;
        };
        let artist = if track.artist.is_empty() { track.album_artist.clone() } else { track.artist.clone() };
        let song = (artist.clone(), track.title.clone());
        if self.lyrics.of.as_ref() == Some(&song) {
            return;
        }
        self.lyrics.of = Some(song.clone());
        self.lyrics.trouble = None;
        if let Some(words) = (!track.path.as_os_str().is_empty()).then(|| lyrics::in_file(&track.path)).flatten() {
            self.lyrics.lyrics = Some(words);
            return;
        }
        let dir = self.config.lyrics_dir();
        match lyrics::load(&dir, &artist, &track.title) {
            Some(kept) => self.lyrics.lyrics = kept,
            None => {
                self.lyrics.lyrics = None;
                if !self.lyrics.looking_for(&song) {
                    let rx = crate::gui::lyrics::start_looking(artist, track.title.clone(), track.duration_secs);
                    self.lyrics.looking = Some((song, rx));
                }
            }
        }
    }

    pub(in crate::gui) fn poll_lyrics(&mut self) {
        let Some((song, rx)) = &self.lyrics.looking else { return };
        let answer = match rx.try_recv() {
            Ok(answer) => answer,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Err(tr!("keine antwort").to_string()),
        };
        let song = song.clone();
        self.lyrics.looking = None;
        let here = self.lyrics.of.as_ref() == Some(&song);
        match answer {
            Ok((found, raw)) => {
                lyrics::keep(&self.config.lyrics_dir(), &song.0, &song.1, found.as_ref(), &raw);
                if here {
                    self.lyrics.lyrics = found;
                }
            }
            Err(why) => {
                tracing::warn!("lyrics for {} – {}: {}", song.0, song.1, why);
                if here {
                    self.lyrics.trouble = Some(tr!("lrclib ist gerade nicht erreichbar").to_string());
                }
            }
        }
    }

    pub fn look_up_lyrics(&mut self) {
        let Some((artist, title)) = self.lyrics.of.clone() else { return };
        let _ = std::fs::remove_file(lyrics::cache_path(&self.config.lyrics_dir(), &artist, &title));
        self.lyrics.of = None;
        self.lyrics.looking = None;
        self.follow_the_words();
    }
}
