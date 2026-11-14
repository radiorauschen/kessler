use crate::gui::*;

impl GuiApp {
    pub fn editor_column(&self) -> Column {
        self.editor_in
    }

    pub fn device_shown(&self) -> bool {
        (self.device_open || self.transfer.is_some()) && self.attached.is_some()
    }

    pub fn fourth_open(&self) -> bool {
        self.queue_open()
            || self.device_shown()
            || self.lists_open()
            || self.log_open
            || self.artist_open
            || self.lyrics_open
            || self.gaps.open
            || self.eq_open
    }

    pub fn split(&self) -> bool {
        self.side_fits && (self.merged() || self.config.ui.all_columns)
    }

    pub fn all_sides(&self) -> bool {
        self.config.ui.all_columns && self.split()
    }

    pub fn side_is_open(&self, side: Fourth) -> bool {
        match side {
            Fourth::Artist => self.artist_open,
            Fourth::Lyrics => self.lyrics_open,
            Fourth::Others => self.gaps.open,
            Fourth::Equalizer => self.eq_open,
            Fourth::Device => self.device_shown(),
            Fourth::Log => self.log_open,
            Fourth::Queue | Fourth::Lists => false,
        }
    }

    pub(in crate::gui) fn sides_now(&self) -> Sides {
        Sides {
            artist: self.artist_open,
            log: self.log_open,
            lyrics: self.lyrics_open,
            others: self.gaps.open,
            eq: self.eq_open,
        }
    }

    pub(in crate::gui) fn switch_sides(&mut self, from: View, to: View) {
        if from == to {
            return;
        }
        let now = self.sides_now();
        self.sides_left[from as usize] = Some(now);
        self.artist_pin = None;
        let first = match to {
            View::Radio => Sides { artist: self.config.ui.radio_info, ..Sides::default() },
            View::Podcasts | View::Ipod => Sides::default(),
            View::Music => now,
        };
        let wanted = self.sides_left[to as usize].unwrap_or(first);
        self.put_sides(wanted);
    }

    fn put_sides(&mut self, sides: Sides) {
        if sides == self.sides_now() {
            return;
        }
        if !sides.artist {
            self.artist_form = None;
            self.gaps.came_from = false;
        }
        if sides.others && !self.gaps.open {
            self.gaps.came_from = false;
            self.gaps.stale = true;
        }
        self.artist_open = sides.artist;
        self.log_open = sides.log;
        self.lyrics_open = sides.lyrics;
        self.gaps.open = sides.others;
        self.eq_open = sides.eq;
        if sides.log {
            self.log_from_newest = true;
        }
        if sides.artist || sides.log || sides.lyrics || sides.others || sides.eq {
            self.leave_the_left();
        } else {
            self.follow_the_playing();
        }
    }

    pub fn right_side(&self) -> Option<Fourth> {
        if self.log_open {
            Some(Fourth::Log)
        } else if self.device_shown() {
            Some(Fourth::Device)
        } else if self.artist_open {
            Some(Fourth::Artist)
        } else if self.lyrics_open {
            Some(Fourth::Lyrics)
        } else if self.gaps.open {
            Some(Fourth::Others)
        } else if self.eq_open {
            Some(Fourth::Equalizer)
        } else {
            None
        }
    }

    pub fn left_side(&self) -> Option<Fourth> {
        if self.lists_open() {
            Some(Fourth::Lists)
        } else if self.queue_open() {
            Some(Fourth::Queue)
        } else {
            None
        }
    }

    pub(in crate::gui) fn leave_the_left(&mut self) {
        if !self.split() && matches!(self.focus, Column::Queue | Column::Lists) {
            self.focus = if self.narrow { Column::Queue } else { Column::Tracks };
        }
    }

    pub(in crate::gui) fn export_log(&mut self) {
        let today = chrono::Local::now().date_naive();
        let home = std::env::var_os("HOME").map(std::path::PathBuf::from).unwrap_or_default();
        let desktop = home.join("Desktop");
        let dir = if desktop.is_dir() { desktop } else { home };
        let stamp = chrono::Local::now().format("%Y-%m-%d-%H%M");
        let path = dir.join(format!("kessler-log-{stamp}.md"));
        match std::fs::write(&path, self.log.markdown(today)) {
            Ok(()) => {
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                self.set_notice(tr!("log gespeichert: {}", name));
                let _ = crate::gui::reveal::show(&path);
            }
            Err(e) => self.set_status(format!("{e}")),
        }
    }

    pub fn note(&mut self, kind: log::Kind, text: impl Into<String>) {
        self.log.add(kind, text);
    }

    pub fn toggle_log(&mut self) {
        if self.log_open {
            self.log_open = false;
            self.follow_the_playing();
            return;
        }
        if !self.all_sides() {
            self.artist_open = false;
            self.lyrics_open = false;
            self.gaps.open = false;
            self.eq_open = false;
        }
        self.log_open = true;
        self.log_from_newest = true;
        self.podcasts.info_open = false;
        self.leave_the_left();
    }

    pub fn fourth(&self) -> Fourth {
        if self.log_open {
            Fourth::Log
        } else if self.device_shown() {
            Fourth::Device
        } else if self.artist_open {
            Fourth::Artist
        } else if self.lyrics_open {
            Fourth::Lyrics
        } else if self.gaps.open {
            Fourth::Others
        } else if self.eq_open {
            Fourth::Equalizer
        } else if self.lists_open() {
            Fourth::Lists
        } else {
            Fourth::Queue
        }
    }

    pub fn lists_open(&self) -> bool {
        self.config.ui.show_lists
            && !self.showing_ipod()
            && (self.split() || (!self.device_shown() && !self.log_open && !self.artist_open && !self.lyrics_open && !self.gaps.open && !self.eq_open))
    }

    pub fn open_lists(&mut self) {
        if !self.split() {
            if self.device_shown() {
                self.close_device_panel();
            }
            self.log_open = false;
            self.artist_open = false;
            self.lyrics_open = false;
            self.gaps.open = false;
            self.eq_open = false;
        }
        if !self.config.ui.show_lists {
            self.config.ui.show_lists = true;
            self.save_config();
        }
        if self.editing.is_none() {
            if let Some(at) = self.lists_last.and_then(|id| self.list_at(id)) {
                if let Ok(tracks) = self.db.get_playlist_tracks(self.lists[at].0.id) {
                    self.editing = Some(self.lists[at].0.id);
                    self.editing_tracks = tracks;
                    self.queue_index = 0;
                    self.lists_index = at;
                }
            }
        }
        if self.focus == Column::Queue && self.editing.is_none() {
            self.focus = Column::Lists;
        }
    }

    pub fn close_lists(&mut self) {
        if self.prompt.as_ref().is_some_and(Prompt::typing) {
            self.cancel_prompt();
        }
        self.editing = None;
        self.editing_tracks = Vec::new();
        self.queue_index = 0;
        if self.config.ui.show_lists {
            self.config.ui.show_lists = false;
            self.save_config();
        }
        if matches!(self.focus, Column::Lists | Column::Queue) {
            self.focus = if self.queue_open() {
                Column::Queue
            } else {
                Column::Tracks
            };
        }
        self.follow_the_playing();
    }

    pub(in crate::gui) fn follow_the_playing(&mut self) {
        if let Some(at) = self.player.queue_position() {
            if self.editing.is_none() {
                self.queue_index = at;
            }
            self.queue_followed = None;
        }
    }

    pub fn toggle_lists(&mut self) -> Result<()> {
        if self.lists_open() {
            self.close_lists();
            return Ok(());
        }
        self.use_device(false)?;
        self.open_lists();
        self.focus = Column::Lists;
        self.scroll_to = Some(Column::Lists);
        Ok(())
    }

    pub fn show_the_queue(&mut self) {
        if !self.all_sides() {
            self.log_open = false;
            self.artist_open = false;
        }
        if self.config.ui.show_lists {
            self.close_lists();
        }
        if !self.config.ui.show_queue {
            self.config.ui.show_queue = true;
            self.save_config();
        }
        self.scroll_to = Some(Column::Queue);
        self.follow_the_playing();
    }

    pub fn prompt_inline(&self) -> bool {
        self.prompt.as_ref().is_some_and(Prompt::typing)
    }
}
