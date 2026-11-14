use crate::gui::*;

impl GuiApp {
    pub fn playing_library_track(&self) -> Option<&Track> {
        self.player.library_track()
    }

    pub fn play_selected(&mut self) -> Result<()> {
        if self.focus == Column::Artists && self.showing_radio() {
            self.play_radio_cursor();
            return Ok(());
        }
        if self.focus == Column::Lists {
            let at = self.lists_index;
            if at >= self.lists.len() {
                self.ask_for_a_list(Vec::new());
                return Ok(());
            }
            return self.play_list(at);
        }
        if self.tracks.is_empty() {
            return Ok(());
        }
        if self.focus == Column::Tracks {
            let picked: Vec<Track> = self.selected_tracks().into_iter().cloned().collect();
            if picked.len() > 1 {
                let _ = self.db.record_play(picked[0].id);
                self.add_and_play(picked, tr!("diese auswahl"))?;
                return Ok(());
            }
        }
        let index = self.track_index.min(self.tracks.len() - 1);
        let Some(track) = self.tracks.get(index).cloned() else {
            return Ok(());
        };
        let _ = self.db.record_play(track.id);
        self.add_and_play(vec![track], tr!("dieser titel"))?;
        Ok(())
    }

    pub fn play_column(&mut self) -> Result<()> {
        let tracks = self.tracks.clone();
        if let Some(first) = tracks.first() {
            let _ = self.db.record_play(first.id);
        }
        self.add_and_play(tracks, tr!("diese auswahl"))?;
        Ok(())
    }

    pub fn album_as_tracks(&self, index: usize) -> Result<Vec<Track>> {
        let Some(album) = self.albums.get(index) else {
            return Ok(Vec::new());
        };
        self.shelf_tracks(
            Some(&album.artist),
            Some(&album.name),
            SortField::TrackNumber,
        )
    }

    pub fn artist_as_tracks(&self, index: usize) -> Result<Vec<Track>> {
        let Some(artist) = self.artists.get(index) else {
            return Ok(Vec::new());
        };
        self.shelf_tracks(Some(&artist.name), None, SortField::Artist)
    }

    pub(in crate::gui) fn shelf_tracks(
        &self,
        artist: Option<&str>,
        album: Option<&str>,
        sort: SortField,
    ) -> Result<Vec<Track>> {
        if self.showing_ipod() {
            let Some(device) = &self.attached else {
                return Ok(Vec::new());
            };
            return Ok(device.tracks(artist, album, sort, SortOrder::Ascending, None));
        }
        self.db
            .get_tracks(artist, album, sort, SortOrder::Ascending, None)
    }

    pub fn start_drag(&mut self, tracks: Vec<Track>) {
        if tracks.is_empty() {
            return;
        }
        self.dragging = Some(Dragged::Incoming(tracks));
        if !self.showing_ipod() && !self.showing_radio() {
            if let Some(why) = self.refuses_drops() {
                self.set_status(why);
            }
        }
    }

    pub fn dragged_tracks(&self, row: usize) -> Vec<Track> {
        if self.track_selection.contains(&row) {
            self.selected_tracks().into_iter().cloned().collect()
        } else {
            self.tracks.get(row).cloned().into_iter().collect()
        }
    }

    pub fn dragged_record(&self, start: usize, len: usize) -> Vec<Track> {
        let end = (start + len).min(self.tracks.len());
        let inside = start < end && (start..end).all(|row| self.track_selection.contains(&row));
        if inside {
            self.selected_tracks().into_iter().cloned().collect()
        } else {
            self.tracks[start.min(end)..end].to_vec()
        }
    }

    pub fn drop_into_queue(&mut self, at: usize) -> Result<bool> {
        let Some(carried) = self.dragging.take() else {
            return Ok(false);
        };
        match carried {
            Dragged::Queued(from) => {
                let to = if at > from { at - 1 } else { at };
                self.player.move_queued(from, to);
                self.queue_index = to.min(self.player.queued().len().saturating_sub(1));
            }
            Dragged::Episodes(_) | Dragged::QueuedEpisode(_) => return Ok(false),
            Dragged::Incoming(tracks) => {
                self.keep_the_peek();
                let count = tracks.len();
                self.player.insert_at(at, tracks);
                self.queue_index = at;
                self.set_status(match count {
                    1 => tr!("1 titel eingereiht").to_string(),
                    n => tr!("{} titel eingereiht", text::count(n as u64)),
                });
            }
        }
        Ok(true)
    }

    pub fn play_album(&mut self, row: usize) -> Result<bool> {
        let tracks = match AlbumRow::at(row) {
            AlbumRow::Named(at) => {
                timed("die titel einer platte holen", || self.album_as_tracks(at))?
            }
            AlbumRow::All => {
                self.select(Column::Albums, 0)?;
                self.tracks.clone()
            }
        };
        self.add_and_play(tracks, tr!("dieses album"))
    }

    pub fn play_artist(&mut self, index: usize) -> Result<()> {
        let tracks = timed("die platten unter einem namen holen", || {
            self.artist_as_tracks(index)
        })?;
        self.add_and_play(tracks, tr!("unter diesem namen"))?;
        self.focus = Column::Artists;
        Ok(())
    }

    pub(in crate::gui) fn add_and_play(&mut self, tracks: Vec<Track>, what: &str) -> Result<bool> {
        if tracks.is_empty() {
            self.set_status(tr!("{} hat keine titel", what));
            return Ok(false);
        }
        let from = timed("an die warteschlange hängen und anspielen", || {
            self.player.append_and_play(tracks)
        })?;
        self.queue_index = from;
        self.scroll_to = Some(Column::Queue);
        Ok(true)
    }

    pub fn queue_open(&self) -> bool {
        (self.config.ui.show_queue || self.queue_peek)
            && !self.showing_ipod()
            && !self.lists_open()
            && (self.split() || (!self.log_open && !self.artist_open && !self.lyrics_open && !self.gaps.open && !self.eq_open))
    }

    pub(in crate::gui) fn peek_at_the_queue(&mut self) {
        let carrying = matches!(self.dragging, Some(Dragged::Incoming(_)));
        self.queue_peek = carrying
            && !self.config.ui.show_queue
            && !self.showing_ipod()
            && (self.split() || !self.device_shown())
            && !self.lists_open()
            && !self.narrow;
    }

    pub(in crate::gui) fn keep_the_peek(&mut self) {
        if self.queue_peek {
            self.queue_peek = false;
            self.config.ui.show_queue = true;
            self.save_config();
        }
    }

    pub(in crate::gui) fn close_queue(&mut self) {
        self.queue_peek = false;
        if self.config.ui.show_queue {
            self.config.ui.show_queue = false;
            self.save_config();
        }
        if self.focus == Column::Queue {
            self.focus = Column::Tracks;
        }
    }

    pub fn toggle_queue(&mut self) {
        if !self.split() && (self.log_open || self.artist_open || self.lyrics_open || self.gaps.open || self.eq_open) {
            self.log_open = false;
            self.artist_open = false;
            self.lyrics_open = false;
            self.gaps.open = false;
            self.eq_open = false;
            self.show_the_queue();
            self.focus = Column::Queue;
            return;
        }
        if self.lists_open() {
            self.show_the_queue();
            self.focus = Column::Queue;
            return;
        }
        let open = !self.config.ui.show_queue;
        self.config.ui.show_queue = open;
        self.save_config();
        if !open && self.focus == Column::Queue {
            self.focus = Column::Tracks;
        }
        if open {
            self.scroll_to = Some(Column::Queue);
            self.follow_the_playing();
        }
    }

    pub fn enqueue_selection(&mut self, next: bool) -> Result<()> {
        let tracks = self.selection_as_tracks();
        if tracks.is_empty() {
            self.set_status(tr!("nichts zum einreihen"));
            return Ok(());
        }
        let count = tracks.len();
        if next {
            self.player.play_next(tracks)?;
        } else {
            self.player.enqueue(tracks)?;
        }
        let where_to = if next { tr!("als nächstes") } else { tr!("ans ende") };
        self.set_status(match count {
            1 => tr!("1 titel {}", where_to),
            n => tr!("{} titel {}", text::count(n as u64), where_to),
        });
        if !self.queue_open() {
            self.config.ui.show_queue = true;
            self.save_config();
        }
        self.scroll_to = Some(Column::Queue);
        Ok(())
    }

    pub(in crate::gui) fn selection_as_tracks(&self) -> Vec<Track> {
        match self.focus {
            Column::Artists | Column::Albums => self.tracks.clone(),
            Column::Lists => self.editing_tracks.clone(),
            Column::Tracks => self.selected_tracks().into_iter().cloned().collect(),
            Column::Queue => Vec::new(),
        }
    }

    pub fn play_queued(&mut self, index: usize) -> Result<()> {
        if let Some(track) = self.player.queued().get(index) {
            let _ = self.db.record_play(track.id);
        }
        self.queue_index = index;
        self.focus = Column::Queue;
        timed("einen titel aus der warteschlange anspielen", || {
            self.player.play_queued(index)
        })
    }

    pub fn queue_picks(&mut self) -> Vec<usize> {
        let len = self.player.queued().len();
        if self.queue_picked_in != self.player.queue.revision() {
            self.queue_picked.clear();
        }
        self.queue_picked.retain(|i| *i < len);
        self.queue_picked.iter().copied().collect()
    }

    fn picked_now(&mut self) {
        self.queue_picked_in = self.player.queue.revision();
    }

    pub fn pick_all_queued(&mut self) {
        let len = self.player.queued().len();
        if len == 0 {
            return;
        }
        self.unmarked = false;
        self.queue_picked = (0..len).collect();
        self.picked_now();
    }

    pub fn pick_queued(&mut self, index: usize, through: bool, also: bool) {
        let len = self.player.queued().len();
        if index >= len {
            return;
        }
        self.unmarked = false;
        let mut picks: std::collections::BTreeSet<usize> = self.queue_picks().into_iter().collect();
        if through {
            let (a, b) = (self.queue_index.min(len - 1), index);
            picks = (a.min(b)..=a.max(b)).collect();
        } else if also {
            if picks.is_empty() {
                picks.insert(self.queue_index.min(len - 1));
            }
            if !picks.remove(&index) {
                picks.insert(index);
            }
            self.queue_index = index;
        } else {
            picks.clear();
            self.queue_index = index;
        }
        self.queue_picked = picks;
        self.picked_now();
        self.focus = Column::Queue;
    }

    pub fn carry_queued(&mut self, step: i32) -> Result<()> {
        if self.editing.is_some() {
            let len = self.editing_tracks.len();
            if len < 2 {
                return Ok(());
            }
            let from = self.queue_index.min(len - 1);
            let to = (from as i32 + step).clamp(0, len as i32 - 1) as usize;
            if to != from {
                let row = self.editing_tracks.remove(from);
                self.editing_tracks.insert(to, row);
                self.queue_index = to;
                self.write_editing()?;
            }
        } else {
            let len = self.player.queued().len();
            if len < 2 {
                return Ok(());
            }
            let picks = self.queue_picks();
            let run = picks.len() > 1 && picks.windows(2).all(|w| w[1] == w[0] + 1);
            if run {
                let (mut lo, mut hi) = (picks[0], picks[picks.len() - 1]);
                for _ in 0..step.unsigned_abs() {
                    if step > 0 && hi + 1 < len {
                        self.player.move_queued(hi + 1, lo);
                        lo += 1;
                        hi += 1;
                    } else if step < 0 && lo > 0 {
                        self.player.move_queued(lo - 1, hi);
                        lo -= 1;
                        hi -= 1;
                    }
                }
                self.queue_picked = (lo..=hi).collect();
                self.queue_index = (self.queue_index as i32 + step).clamp(lo as i32, hi as i32) as usize;
                self.picked_now();
                self.scroll_to = Some(Column::Queue);
                return Ok(());
            }
            self.queue_picked.clear();
            let from = self.queue_index.min(len - 1);
            let to = (from as i32 + step).clamp(0, len as i32 - 1) as usize;
            if to != from {
                self.player.move_queued(from, to);
                self.queue_index = to;
            }
        }
        self.scroll_to = Some(Column::Queue);
        Ok(())
    }

    pub fn drop_queued(&mut self) {
        let len = self.player.queued().len();
        if len == 0 {
            return;
        }
        let picks = self.queue_picks();
        if !picks.is_empty() {
            for index in picks.iter().rev() {
                self.player.drop_queued(*index);
            }
            self.queue_picked.clear();
            let first = picks[0];
            self.queue_index = first.min(self.player.queued().len().saturating_sub(1));
            return;
        }
        let index = self.queue_index.min(len - 1);
        self.player.drop_queued(index);
        self.queue_index = index.min(self.player.queued().len().saturating_sub(1));
    }

    pub(in crate::gui) fn remember_window(&mut self, ctx: &egui::Context) {
        const SETTLED: std::time::Duration = std::time::Duration::from_millis(600);

        let Some(now) = ctx.input(|i| {
            let viewport = i.viewport();
            viewport
                .outer_rect
                .zip(viewport.inner_rect)
                .map(|(outer, inner)| [outer.min.x, outer.min.y, inner.width(), inner.height()])
        }) else {
            return;
        };
        if now.iter().any(|v| !v.is_finite()) || now[2] < 1.0 || now[3] < 1.0 {
            return;
        }

        let moved = self
            .window_was
            .map(|was: [f32; 4]| {
                was.iter()
                    .zip(now.iter())
                    .any(|(a, b)| (a - b).abs() > 1.0)
            })
            .unwrap_or(true);
        if moved {
            self.window_was = Some(now);
            self.window_still_since = Some(std::time::Instant::now());
            ctx.request_repaint_after(SETTLED);
            return;
        }
        let Some(since) = self.window_still_since else {
            return;
        };
        if since.elapsed() < SETTLED {
            ctx.request_repaint_after(SETTLED - since.elapsed());
            return;
        }
        self.window_still_since = None;
        if self.config.ui.window != Some(now) {
            self.config.ui.window = Some(now);
            self.save_config();
        }
    }

    pub(in crate::gui) fn note_what_is_gone(&mut self) {
        for path in self.player.gone_missing() {
            self.missing.insert(path);
        }
    }

    pub fn is_gone(&self, path: &std::path::Path) -> bool {
        !self.missing.is_empty() && self.missing.contains(path)
    }

    pub(in crate::gui) fn remember_queue(&mut self) {
        let now = self.player.queue.revision();
        if now == self.queue_saved {
            return;
        }
        self.queue_saved = now;
        crate::player::remembered::save(&self.config.queue_path(), &self.player.remember());
    }

    pub(in crate::gui) fn put_away_on_close(&mut self, ctx: &egui::Context) {
        if !cfg!(target_os = "macos") || self.quitting {
            return;
        }
        if !ctx.input(|i| i.viewport().close_requested()) {
            return;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        if !crate::media::hide_application() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
    }

    pub(in crate::gui) fn restore_queue(&mut self) {
        let saved = crate::player::remembered::load(&self.config.queue_path());
        if saved.paths.is_empty() {
            return;
        }
        let mut tracks = Vec::with_capacity(saved.paths.len());
        let mut at = None;
        for (i, path) in saved.paths.iter().enumerate() {
            let Ok(Some(track)) = self.db.get_track_by_path(&path.to_string_lossy()) else {
                continue;
            };
            if saved.at == Some(i) {
                at = Some(tracks.len());
            }
            tracks.push(track);
        }
        self.player.restore(tracks, at);
        self.queue_saved = self.player.queue.revision();
    }

    pub fn clear_queue(&mut self) {
        self.player.clear_queue();
        self.queue_index = 0;
        self.set_status(tr!("warteschlange geleert"));
    }

    pub fn reveal_playing(&mut self) -> Result<()> {
        let Some(track) = self.player.current_track() else {
            return Ok(());
        };
        let id = track.id;
        let artist_key = crate::db::sort_key(crate::db::grouping_artist(track));
        let album_key = crate::db::sort_key(&track.album);

        if self.filter.is_some() {
            self.close_filter()?;
        }

        {
            let names: Vec<&str> = self.artists.iter().map(|a| a.name.as_str()).collect();
            let row = artist_row_for(&names, &artist_key);
            self.select(Column::Artists, row.index())?;
        }

        if !self.merged() {
            if let Some(index) = album_row_for(&self.albums, &album_key, &artist_key) {
                self.select(Column::Albums, index + 1)?;
            }
        }

        if let Some(index) = self.tracks.iter().position(|t| t.id == id) {
            self.select(Column::Tracks, index)?;
        }

        if self.queue_open() && self.editing.is_none() {
            if let Some(at) = self.player.queue_position() {
                self.queue_index = at;
                self.queue_followed = None;
            }
        }

        self.focus = Column::Tracks;
        self.scroll_to = Some(Column::Tracks);
        self.reveal_all = true;
        Ok(())
    }

    pub(in crate::gui) fn now_playing_info(&self) -> Option<crate::media::NowPlaying> {
        use crate::player::PlaybackState;
        let state = self.player.state();
        if state == PlaybackState::Stopped {
            return None;
        }
        if self.player.current_track().is_none() {
            self.on_air.as_ref()?;
            let [title, artist, album] = self.playing_words()?;
            return Some(crate::media::NowPlaying {
                playing: state == PlaybackState::Playing,
                title,
                artist,
                album,
                duration: 0.0,
                elapsed: 0.0,
                artwork: self.on_air.as_ref().and_then(crate::radio::picture_path),
            });
        }
        let track = self.player.current_track()?;
        let episode = self.podcasts.playing_from.as_ref().filter(|(e, _)| self.playing_this_episode(&e.id));
        if let Some((episode, show)) = episode {
            crate::podcast::fetch_episode_art(&episode.artwork);
            return Some(crate::media::NowPlaying {
                playing: state == PlaybackState::Playing,
                title: track.display_title().to_string(),
                artist: track.shown_artist().to_string(),
                album: track.album.clone(),
                duration: self.player.duration().as_secs_f64(),
                elapsed: self.player.position().as_secs_f64(),
                artwork: Some(crate::podcast::picture_for(episode, show)).filter(|path| path.is_file()),
            });
        }
        Some(crate::media::NowPlaying {
            playing: state == PlaybackState::Playing,
            title: track.display_title().to_string(),
            artist: track.shown_artist().to_string(),
            album: track.album.clone(),
            duration: self.player.duration().as_secs_f64(),
            elapsed: self.player.position().as_secs_f64(),
            artwork: Some(crate::library::artwork::cache_path(
                &self.config.artwork_dir(),
                &crate::db::sort_key(crate::db::grouping_artist(track)),
                &crate::db::sort_key(&track.album),
            ))
            .filter(|path| path.is_file()),
        })
    }

    pub(in crate::gui) fn take_media_commands(&mut self, ctx: &egui::Context) {
        use crate::media::Command;
        use crate::player::PlaybackState;
        while let Some(command) = crate::media::poll().or_else(crate::media::tray_poll) {
            let outcome = match command {
                Command::Play => {
                    match self.player.state() {
                        PlaybackState::Stopped => self.play_from_stopped(),
                        PlaybackState::Paused => {
                            self.player.toggle_pause();
                            Ok(())
                        }
                        PlaybackState::Playing => Ok(()),
                    }
                }
                Command::Pause => {
                    if self.player.state() == PlaybackState::Playing {
                        self.player.toggle_pause();
                    }
                    Ok(())
                }
                Command::Toggle => {
                    if self.player.state() == PlaybackState::Stopped {
                        self.play_from_stopped()
                    } else {
                        self.player.toggle_pause();
                        Ok(())
                    }
                }
                Command::Next => self.skip(true),
                Command::Previous => self.skip(false),
                Command::Stop => {
                    self.player.stop();
                    Ok(())
                }
                Command::Raise => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    Ok(())
                }
                Command::Quit => {
                    self.quitting = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    Ok(())
                }
                Command::Volume(percent) => {
                    self.player.set_volume(percent);
                    ctx.request_repaint();
                    Ok(())
                }
                Command::Sleep => {
                    if self.playing_episode() {
                        self.step_sleep();
                    }
                    ctx.request_repaint();
                    Ok(())
                }
            };
            if let Err(e) = outcome {
                self.set_status(format!("{e}"));
            }
        }
    }

}
