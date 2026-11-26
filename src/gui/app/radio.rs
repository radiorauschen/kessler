//! The radio's column and the station on the air — see `crate::radio`.

use crate::gui::*;

/// How long the search field has to stand still before the directory is
/// asked: a search at every letter would be a search for every letter.
const ASK_AFTER: std::time::Duration = std::time::Duration::from_millis(400);

/// How often a station playing is asked what it plays — see
/// `hear_the_station`.
const HEAR_EVERY: std::time::Duration = std::time::Duration::from_secs(2);

/// How long a song from the radio has to have been heard, from its start,
/// to count on last.fm when the next one comes. Its length is unknown, so
/// last.fm's „half of it" cannot be asked; its thirty-second floor can.
pub(in crate::gui) const RADIO_SCROBBLE_AFTER: i64 = 30;

impl GuiApp {
    /// Whether the first column shows the radio.
    pub fn showing_radio(&self) -> bool {
        self.radio.is_some()
    }

    /// „radio" in the header: the stations into the first column, or the
    /// library back. Opening it reads the stations kept; leaving lets go of
    /// everything but the station still playing.
    pub fn toggle_radio_view(&mut self) {
        if self.radio.is_some() {
            self.leave_radio();
            if let Err(e) = self.close_filter() {
                self.set_status(format!("{e}"));
            }
            return;
        }
        if self.showing_ipod() {
            if let Err(e) = self.use_device(false) {
                self.set_status(format!("{e}"));
            }
        }
        if let Err(e) = self.close_filter() {
            self.set_status(format!("{e}"));
        }
        self.music_focus = Some(self.focus);
        self.radio = Some(Box::new(crate::radio::Radio::open()));
        self.switch_sides(View::Music, View::Radio);
    }

    /// The columns over to the music, the radio or the iPod, as the masthead's
    /// switch chooses: whatever was open goes first, the iPod's column with the
    /// iPod.
    pub(in crate::gui) fn read_from(&mut self, radio: bool, device: bool) {
        self.leave_podcasts();
        if self.showing_ipod() && !device {
            self.toggle_device_view();
        }
        if self.showing_radio() && !radio {
            self.toggle_radio_view();
        }
        if radio && !self.showing_radio() {
            self.toggle_radio_view();
        }
        if device && !self.showing_ipod() {
            self.toggle_device_view();
        }
    }

    /// The music's columns are what the window shows: neither the radio,
    /// the podcasts nor the iPod is open.
    pub(in crate::gui) fn music_shown(&self) -> bool {
        !self.showing_radio() && !self.showing_podcasts() && !self.showing_ipod()
    }

    /// Everything switched off: the music, and nothing else open in its place —
    /// the window shows the note that says where to switch it on again.
    pub(in crate::gui) fn all_off(&self) -> bool {
        !self.config.ui.music && self.music_shown()
    }

    /// Once a frame: with the music switched off, its columns are not shown
    /// — the radio in their place, or the podcasts, or the iPod, whichever
    /// is on.
    pub(in crate::gui) fn fall_back_from_music(&mut self) {
        self.player.library_hidden = !self.config.ui.music;
        let now = self.view_shown();
        if now != self.view_now {
            self.view_before = Some(self.view_now);
            self.view_now = now;
        }
        if !self.all_off() {
            return;
        }
        if self.config.ui.radio {
            self.read_from(true, false);
        } else if self.config.ui.podcasts {
            self.show_podcasts();
        } else if self.attached.is_some() {
            self.read_from(false, true);
        }
    }

    /// The music switched on or off. Off, a track of the library that plays
    /// stops — a station or an episode plays on.
    pub(in crate::gui) fn set_music(&mut self, on: bool) {
        self.config.ui.music = on;
        if !on && self.player.library_track().is_some() && self.on_air.is_none() {
            self.player.stop();
        }
        if !on {
            self.stop_measuring();
        }
        if let Err(e) = self.reload_all() {
            self.set_status(format!("{e}"));
        }
        if on && self.config.watch_dirs && !self.config.music_dirs.is_empty() && self.scan.is_none() {
            self.start_rescan();
        }
        self.save_config();
    }

    /// The radio switched on or off (menu → allgemein, and `c` `2` / `2`
    /// `2`). Off, its columns go, and a station on the air stops.
    pub(in crate::gui) fn set_radio(&mut self, on: bool) {
        self.config.ui.radio = on;
        if !on {
            if self.showing_radio() {
                self.toggle_radio_view();
                self.back_to_before(View::Radio);
            }
            if self.on_air.is_some() {
                self.player.stop();
                self.on_air = None;
            }
        }
        self.save_config();
    }

    /// The podcasts switched on or off (menu → allgemein, and `c` `3` /
    /// `3` `3`). Off, their column goes, and what was read of the feeds.
    pub(in crate::gui) fn set_podcasts(&mut self, on: bool) {
        self.config.ui.podcasts = on;
        if !on {
            let shown = self.showing_podcasts();
            self.leave_podcasts();
            self.forget_feeds();
            if shown {
                self.back_to_before(View::Podcasts);
            }
        }
        self.save_config();
    }

    /// Which view the window shows.
    pub(in crate::gui) fn view_shown(&self) -> View {
        if self.showing_podcasts() {
            View::Podcasts
        } else if self.showing_radio() {
            View::Radio
        } else if self.showing_ipod() {
            View::Ipod
        } else {
            View::Music
        }
    }

    /// `gone` switched off while it was open: back to the view open before it,
    /// where that is still on — the radio after the podcasts opened from it,
    /// not the music. Otherwise the music, as before — or, with the music off
    /// too, whatever `fall_back_from_music` finds.
    fn back_to_before(&mut self, gone: View) {
        let before = self.view_before.filter(|v| *v != gone);
        match before {
            Some(View::Radio) if self.config.ui.radio => self.read_from(true, false),
            Some(View::Podcasts) if self.config.ui.podcasts => self.show_podcasts(),
            Some(View::Ipod) if self.attached.is_some() => self.read_from(false, true),
            _ => {}
        }
    }

    /// With the music off, nothing of the library is started: not by play,
    /// space, `n` or `p`, nor by the keys on the keyboard's media row.
    pub(in crate::gui) fn music_held(&self) -> bool {
        !self.config.ui.music && self.on_air.is_none() && !self.playing_episode()
    }

    /// Play or pause, as the key in the header, space and the k in the menu
    /// have it — but stopped, with the music off, it does not start the
    /// queue's track out of sight.
    pub fn toggle_pause(&mut self) {
        if self.music_held() && self.player.state() == crate::player::PlaybackState::Stopped {
            return;
        }
        self.player.toggle_pause();
    }

    /// Play from nothing playing, for the media keys: the station under the
    /// keyboard on the radio, the queue where it stood in the podcasts —
    /// not the music's selection, out of sight there —, and in the music
    /// what is selected.
    pub(in crate::gui) fn play_from_stopped(&mut self) -> Result<()> {
        if self.showing_radio() {
            self.play_radio_cursor();
            return Ok(());
        }
        if !self.config.ui.music {
            return Ok(());
        }
        if self.showing_podcasts() {
            self.player.toggle_pause();
            return Ok(());
        }
        self.play_selected()
    }

    /// Put the radio's columns away: the library comes back with the side as it
    /// was left — not with „informationen" open because the radio opened it,
    /// nor without the lyrics or the log that made room for it. A station on
    /// the air plays on.
    pub(in crate::gui) fn leave_radio(&mut self) {
        if self.radio.take().is_none() {
            return;
        }
        self.radio_typed_at = None;
        if let Some(focus) = self.music_focus.take() {
            self.focus = focus;
        }
        if self.config.ui.radio_info != self.artist_open {
            self.config.ui.radio_info = self.artist_open;
            self.save_config();
        }
        self.switch_sides(View::Radio, View::Music);
    }

    /// The station on the air where it lives: the radio opened if it was not,
    /// and the keyboard on it among the stations kept.
    pub(in crate::gui) fn show_on_air(&mut self) {
        let Some(url) = self.on_air.as_ref().map(|s| s.url.clone()) else { return };
        if !self.showing_radio() {
            self.read_from(true, false);
        }
        if let Err(e) = self.close_filter() {
            self.set_status(format!("{e}"));
        }
        let at = self.radio_listed().iter().position(|s| s.url == url);
        let Some(radio) = self.radio.as_mut() else { return };
        if let Some(at) = at {
            radio.cursor = at;
        }
        self.focus = crate::gui::Column::Artists;
    }

    /// The stations the column lists: what was found while something is
    /// searched for, else those kept.
    pub(in crate::gui) fn radio_listed(&self) -> Vec<crate::radio::Station> {
        let Some(radio) = &self.radio else { return Vec::new() };
        if self.query().is_some() {
            radio.found.clone()
        } else if let Some(similar) = &radio.similar {
            similar.stations.clone()
        } else {
            self.kept_in_order(radio.kept.clone())
        }
    }

    /// The stations kept in the column's order: as dragged, or by name.
    pub(in crate::gui) fn kept_in_order(&self, mut kept: Vec<crate::radio::Station>) -> Vec<crate::radio::Station> {
        match self.config.ui.radio_order.as_str() {
            "a-z" => kept.sort_by_cached_key(|s| s.name.to_lowercase()),
            "z-a" => kept.sort_by_cached_key(|s| std::cmp::Reverse(s.name.to_lowercase())),
            _ => {}
        }
        kept
    }

    /// Whether the stations kept stand as they were dragged — the only order
    /// a station can be dragged to a new place in.
    pub(in crate::gui) fn radio_by_hand(&self) -> bool {
        !matches!(self.config.ui.radio_order.as_str(), "a-z" | "z-a")
    }

    /// The stations' head clicked: their order one step on — by hand, a–z,
    /// z–a. The keyboard's row goes back to the top.
    pub(in crate::gui) fn turn_radio_order(&mut self) {
        self.config.ui.radio_order = match self.config.ui.radio_order.as_str() {
            "a-z" => "z-a",
            "z-a" => "eigene",
            _ => "a-z",
        }
        .to_string();
        if let Some(radio) = self.radio.as_mut() {
            radio.cursor = 0;
        }
        self.save_config();
    }

    /// ↑ ↓ in the radio's column: the keyboard's row among the stations, as in
    /// any column.
    pub(in crate::gui) fn move_radio_cursor(&mut self, delta: i32) {
        let len = self.radio_listed().len();
        let Some(radio) = self.radio.as_mut() else { return };
        radio.resting = false;
        let last = len.saturating_sub(1) as i64;
        radio.cursor = (radio.cursor as i64 + i64::from(delta)).clamp(0, last) as usize;
    }

    /// How many rows „gerade gehört" has for the keyboard: what was heard,
    /// then the songs kept.
    fn heard_len(&self) -> (usize, usize) {
        let songs = self.radio.as_ref().map_or(0, |r| r.songs.len());
        (self.aired.len(), songs)
    }

    /// The radio's keys for its two columns: `←` `→` between the stations and
    /// „gerade gehört"; there `↑` `↓` go through what was heard and the songs
    /// kept, `enter` keeps a line heard (or lets it go) and shows who sang a
    /// song kept, `d` or `backspace` forgets a song kept. Returns whether the
    /// keyboard is in „gerade gehört", where the keys every column has must
    /// leave the stations alone.
    pub(in crate::gui) fn handle_radio_keys(&mut self, ctx: &egui::Context) -> bool {
        use egui::{Key, Modifiers};
        const NONE: Modifiers = Modifiers::NONE;
        let taken = |k: Key| ctx.input_mut(|i| i.count_and_consume_key(NONE, k));
        let pressed = |k: Key| ctx.input_mut(|i| i.consume_key(NONE, k));
        let (aired, songs) = self.heard_len();
        let rows = aired + songs;
        let right = taken(Key::ArrowRight) + taken(Key::L);
        let left = taken(Key::ArrowLeft) + taken(Key::H);
        if right > left && rows > 0 {
            self.radio_heard_keys.get_or_insert(0);
        } else if left > right {
            self.radio_heard_keys = None;
        }
        let Some(at) = self.radio_heard_keys else { return false };
        if rows == 0 {
            self.radio_heard_keys = None;
            return false;
        }
        let down = taken(Key::ArrowDown) + taken(Key::J);
        let up = taken(Key::ArrowUp) + taken(Key::K);
        let at = ((at as i64 + down as i64 - up as i64).clamp(0, rows as i64 - 1)) as usize;
        if self.radio_heard_keys != Some(at) || right > left {
            ctx.data_mut(|d| d.insert_temp(crate::gui::library::heard_follow_id(), true));
        }
        self.radio_heard_keys = Some(at);
        if pressed(Key::Enter) {
            if at < aired {
                let line = self.aired[at].clone();
                self.keep_aired(&line);
            } else if let Some(artist) = self.radio.as_ref().and_then(|r| r.songs.get(at - aired)).map(|s| s.artist.clone()).filter(|a| !a.is_empty()) {
                self.about_artist(&artist);
            }
        }
        if at >= aired && (pressed(Key::D) || pressed(Key::Backspace) || pressed(Key::Delete)) {
            if let Some(radio) = self.radio.as_mut() {
                if let Some(song) = radio.songs.get(at - aired).cloned() {
                    radio.forget_song(&song);
                }
            }
            let (_, songs) = self.heard_len();
            if songs == 0 && aired == 0 {
                self.radio_heard_keys = None;
            } else {
                self.radio_heard_keys = Some(at.min(aired + songs.max(1) - 1));
            }
        }
        true
    }

    /// Enter in the radio's column: the station under the keyboard plays.
    pub(in crate::gui) fn play_radio_cursor(&mut self) {
        let listed = self.radio_listed();
        let Some(at) = self.radio.as_ref().map(|r| r.cursor) else { return };
        if let Some(station) = listed.get(at) {
            self.play_station(station.clone());
        }
    }

    /// The station „sender" shows: the one on the air, else the one under
    /// the keyboard.
    pub(in crate::gui) fn station_shown(&self) -> Option<crate::radio::Station> {
        let at = self.radio.as_ref().map_or(0, |r| r.cursor);
        self.on_air.clone().or_else(|| self.radio_listed().get(at).cloned())
    }

    /// Begin typing a new name for the kept station `station`, all of the
    /// old one marked, so typing replaces it.
    pub(in crate::gui) fn start_naming_station(&mut self, station: &crate::radio::Station) {
        let mut entry = crate::gui::tagedit::Entry::new(station.name.clone());
        entry.mark_all();
        self.naming_station = Some((station.url.clone(), entry));
    }

    /// The typed name, onto the station: in the lists, `radio.toml`, and the
    /// header while it plays. Nothing typed, or the name as it was, changes
    /// nothing.
    pub(in crate::gui) fn rename_station(&mut self) {
        let Some((url, entry)) = self.naming_station.take() else { return };
        let name = entry.text().trim().to_string();
        if name.is_empty() {
            return;
        }
        let Some(radio) = self.radio.as_mut() else { return };
        if !radio.rename(&url, &name) {
            return;
        }
        if let Some(on) = self.on_air.as_mut().filter(|on| on.url == url) {
            on.name = name.clone();
        }
        self.set_status(tr!("der sender heisst jetzt {}", crate::gui::theme::lower(&name)));
    }

    /// Typing a station's name: enter keeps it, escape leaves it as it was.
    pub(in crate::gui) fn handle_station_naming_keys(&mut self, ctx: &egui::Context) {
        use egui::{Key, Modifiers};
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.naming_station = None;
            return;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter)) {
            self.rename_station();
            return;
        }
        if let Some((_, entry)) = self.naming_station.as_mut() {
            super::device::type_into(ctx, entry);
        }
    }

    /// Ask the system for a picture for the station at `url`, without
    /// stopping the window.
    pub(in crate::gui) fn choose_station_picture(&mut self, url: &str) {
        if self.picking_station_picture.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let prompt = tr!("bild für den sender wählen");
        std::thread::spawn(move || {
            let _ = tx.send(crate::gui::folder::ask_image(prompt));
        });
        self.picking_station_picture = Some((url.to_string(), rx));
    }

    /// Take the picture chosen, once the dialog answers.
    pub(in crate::gui) fn poll_station_picture(&mut self) {
        let Some((url, rx)) = &self.picking_station_picture else { return };
        let url = url.clone();
        match rx.try_recv() {
            Ok(chosen) => {
                self.picking_station_picture = None;
                let Some(path) = chosen else { return };
                match crate::radio::choose_picture(&url, &path) {
                    Ok(()) => {
                        crate::gui::library::forget_decoded_pictures();
                        self.set_status(tr!("bild für den sender gesetzt"));
                    }
                    Err(why) => self.set_status(tr!("das bild liess sich nicht nehmen ({})", why)),
                }
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.picking_station_picture = None;
                self.set_status(tr!("der dateidialog liess sich nicht öffnen"));
            }
        }
    }

    /// The picture chosen for the station at `url` let go of: its own is
    /// shown again.
    pub(in crate::gui) fn forget_station_picture(&mut self, url: &str) {
        crate::radio::forget_picture(url);
        crate::gui::library::forget_decoded_pictures();
        self.set_status(tr!("eigenes bild entfernt"));
    }

    /// Play `station`, in place of whatever plays.
    pub fn play_station(&mut self, station: crate::radio::Station) {
        if self.on_air.as_ref().is_none_or(|on| on.url != station.url) {
            self.aired.clear();
        }
        if self.config.ui.radio_logos {
            crate::radio::fetch_logo(&station);
            if let (Some(asking), Some(radio)) = (crate::radio::fetch_site_picture(&station.homepage, &station.name), self.radio.as_mut()) {
                radio.site_pictures.push((station.name.clone(), asking));
            }
        }
        self.player.play_stream(&station.url);
        self.on_air = Some(station);
    }

    /// ← and →: on the radio the next or the one before among the stations
    /// kept, otherwise the queue's next or previous track.
    pub fn skip(&mut self, forward: bool) -> Result<()> {
        if self.playing_episode() {
            let by = if forward { self.config.ui.podcast_ahead as f64 } else { -(self.config.ui.podcast_back as f64) };
            self.skip_episode(by);
            return Ok(());
        }
        let Some(on) = self.on_air.clone() else {
            if !self.config.ui.music {
                return Ok(());
            }
            return if forward { self.player.next() } else { self.player.previous() };
        };
        let kept = self.kept_in_order(match &self.radio {
            Some(radio) => radio.kept.clone(),
            None => crate::radio::Radio::open().kept,
        });
        if kept.is_empty() {
            return Ok(());
        }
        let at = kept.iter().position(|s| s.url == on.url);
        let next = match (at, forward) {
            (Some(at), true) => (at + 1) % kept.len(),
            (Some(at), false) => (at + kept.len() - 1) % kept.len(),
            (None, _) => 0,
        };
        self.play_station(kept[next].clone());
        Ok(())
    }

    /// The search field changed while the radio is open: asked once it
    /// stands still — see `tend_the_radio`.
    pub(in crate::gui) fn radio_typed(&mut self) {
        self.radio_typed_at = Some(std::time::Instant::now());
    }

    /// Once a frame: the station that has stopped is forgotten, a search
    /// whose typing has stopped goes out, and what came back is taken in.
    /// The stations kept, written as an `.m3u` to the desktop (the home
    /// folder where there is none), to be read back by kessler or played by
    /// any other player.
    pub(in crate::gui) fn export_stations(&mut self) {
        let kept = match &self.radio {
            Some(radio) => radio.kept.clone(),
            None => crate::radio::kept_stations(),
        };
        if kept.is_empty() {
            self.set_status(tr!("keine stationen gemerkt"));
            return;
        }
        let home = std::env::var_os("HOME").map(std::path::PathBuf::from).unwrap_or_default();
        let desktop = home.join("Desktop");
        let dir = if desktop.is_dir() { desktop } else { home };
        let stamp = chrono::Local::now().format("%Y-%m-%d-%H%M");
        let path = dir.join(format!("kessler-stationen-{stamp}.m3u"));
        match std::fs::write(&path, crate::radio::playlist(&kept)) {
            Ok(()) => {
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                self.set_notice(tr!("stationen gespeichert: {}", name));
                let _ = crate::gui::reveal::show(&path);
            }
            Err(e) => self.set_status(format!("{e}")),
        }
    }

    /// Ask for an `.m3u` or a `.pls`, and keep every station it names —
    /// one's own list from another player, or one somebody passed on.
    pub(in crate::gui) fn import_stations(&mut self) {
        if self.station_import.is_some() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let prompt = tr!("stationen wählen (m3u, pls)");
        std::thread::spawn(move || {
            let _ = tx.send(crate::gui::folder::ask_playlist(prompt));
        });
        self.station_import = Some(rx);
    }

    /// Take in the list asked for, once the picker has answered.
    fn take_station_import(&mut self, ctx: &egui::Context) {
        let Some(asking) = &self.station_import else { return };
        let picked = match asking.try_recv() {
            Ok(picked) => picked,
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(250));
                return;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => None,
        };
        self.station_import = None;
        let Some(path) = picked else { return };
        let read = std::fs::read(&path).map(|bytes| String::from_utf8_lossy(&bytes).into_owned());
        let stations = match read {
            Ok(text) => crate::radio::read_playlist(&text),
            Err(e) => {
                self.set_status(format!("{e}"));
                return;
            }
        };
        if stations.is_empty() {
            self.set_status(tr!("keine stationen in der datei"));
            return;
        }
        let found = stations.len();
        let added = crate::radio::add_kept(stations);
        if added > 0 && self.radio.is_some() {
            self.radio = Some(Box::new(crate::radio::Radio::open()));
        }
        self.set_status(tr!("{} von {} stationen neu gemerkt", added, found));
    }

    pub(in crate::gui) fn tend_the_radio(&mut self, ctx: &egui::Context) {
        self.take_station_import(ctx);
        if self.player.on_air().is_none() {
            self.on_air = None;
        }
        if self.player.tuning() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        self.hold_the_pauses();
        self.hear_the_station(ctx);
        let query = self.query().unwrap_or_default();
        let Some(radio) = self.radio.as_mut() else { return };
        if let Some(typed) = self.radio_typed_at {
            if typed.elapsed() >= ASK_AFTER {
                self.radio_typed_at = None;
                radio.ask(&query);
            } else {
                ctx.request_repaint_after(ASK_AFTER);
            }
        }
        let mut told = Vec::new();
        radio.site_pictures.retain(|(name, asking)| match asking.try_recv() {
            Ok(said) => {
                told.push((name.clone(), said));
                false
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => true,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => false,
        });
        if !radio.site_pictures.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
        match radio.take_answers() {
            Ok(_) => {}
            Err(why) => {
                self.note(log::Kind::Radio, tr!("radio-suche: {}", why));
                self.set_status(tr!("radio-suche: radio-browser.info ist gerade nicht zu erreichen"));
            }
        }
        if self.radio.as_ref().is_some_and(|r| r.busy()) {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }
        for (name, said) in told {
            match said {
                Ok(()) => self.note(log::Kind::Radio, tr!("{}: bild von der eigenen seite geholt", name)),
                Err(why) => self.note(log::Kind::Radio, tr!("{}: kein bild von der eigenen seite ({})", name, why)),
            }
        }
    }

    /// Keep what the song on the air was not heard: from when a station is
    /// paused until it plays again, added to the song's pauses, so last.fm's
    /// count stands still meanwhile rather than running on in silence.
    fn hold_the_pauses(&mut self) {
        let paused = self.on_air.is_some() && self.player.state() == crate::player::PlaybackState::Paused;
        let now = chrono::Local::now();
        match (paused, self.radio_paused_since) {
            (true, None) => self.radio_paused_since = Some(now),
            (false, Some(since)) => {
                if let Some(song) = self.aired.first_mut() {
                    song.paused_secs += (now - since).num_seconds().max(0);
                }
                self.radio_paused_since = None;
            }
            _ => {}
        }
    }

    /// What the station says it plays, taken in when it changes. Looked at
    /// every two seconds while a station plays — a line comes with each new
    /// song, so this is plenty.
    fn hear_the_station(&mut self, ctx: &egui::Context) {
        if self.on_air.is_none() {
            self.aired.clear();
            self.spins = None;
            self.programme = None;
            return;
        }
        if self.on_air.as_ref().is_some_and(|s| s.named_by_address()) {
            let said = self.player.on_air_introduced().map(|i| i.name.trim().to_string()).filter(|n| !n.is_empty());
            if let (Some(name), Some(station)) = (said, self.on_air.as_mut()) {
                station.name = name.clone();
                let url = station.url.clone();
                crate::radio::name_kept(&url, &name);
                if let Some(radio) = self.radio.as_mut() {
                    radio.named(&url, &name);
                }
            }
        }
        let on_air_name = self.on_air.as_ref().map(|s| s.name.clone()).unwrap_or_default();
        match self.on_air.as_ref().map(|s| s.url.clone()).and_then(|url| {
            let source = crate::radio::programme::Source::of(&url, &on_air_name)
                .or_else(|| crate::radio::platform::programme_of(&url).map(crate::radio::programme::Source::Platform));
            source.map(|s| (url, s))
        }) {
            Some((url, source)) => {
                if self.programme.as_ref().is_none_or(|p| p.url != url) {
                    self.programme = Some(Box::new(crate::radio::programme::Programme::for_station(&url)));
                }
                if let Some(programme) = self.programme.as_mut() {
                    programme.tend(source);
                    let soon = if programme.waiting() { 250 } else { 30_000 };
                    ctx.request_repaint_after(std::time::Duration::from_millis(soon));
                }
            }
            None => self.programme = None,
        }
        let counting = self.radio_paused_since.is_none()
            && matches!(
                self.radio_scrobble_standing(),
                Some(super::look::OnAirScrobble::Listening(_) | super::look::OnAirScrobble::Counts)
            );
        if counting {
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
        } else if self.on_air.is_some() {
            ctx.request_repaint_after(HEAR_EVERY);
        }
        self.scrobble_the_long_ones();
        if let (Some(station), Some(intro)) = (self.on_air.clone(), self.player.on_air_introduced()) {
            if self.station_noted.as_deref() != Some(station.url.as_str()) {
                self.station_noted = Some(station.url.clone());
                let said = if intro.titles {
                    tr!("{}: verbunden, der sender schickt mit, was läuft", station.name)
                } else {
                    tr!("{}: verbunden, der sender schickt nicht mit, was läuft", station.name)
                };
                self.note(log::Kind::Radio, said);
            }
        }
        let station = self.on_air.clone();
        if let Some(station) = &station {
            if self.spins.as_ref().is_none_or(|w| w.url != station.url) {
                self.spins = crate::radio::spins::Watch::of(station).map(Box::new);
            }
        }
        if let Some(watch) = self.spins.as_mut() {
            if watch.tend() {
                ctx.request_repaint();
            }
            if let (Some(first), Some(station)) = (watch.first.take(), &station) {
                let said = match first {
                    Ok((Some(via), n)) if n > 0 => tr!("{}: {} sagt, was läuft", station.name, via),
                    Ok((Some(via), _)) => tr!("{}: {} nennt nichts, was läuft", station.name, via),
                    Ok((None, _)) => tr!("{}: keine bekannte sendeplattform gefunden", station.name),
                    Err(why) => tr!("{}: nicht zu erreichen ({})", station.name, why),
                };
                self.note(log::Kind::Radio, said);
            }
        }
        let from_the_stream = self.player.on_air_said();
        let logged = self.spins.as_ref().and_then(|w| w.latest.as_ref()).map(|s| s.said());
        let Some(said) = from_the_stream.or(logged) else { return };
        if self.aired.first().is_some_and(|h| h.said == said) {
            return;
        }
        let mut aired = crate::radio::Aired::new(&said);
        let station = self.on_air.as_ref().map(|s| crate::db::sort_key(&s.name));
        if aired.artist.as_deref().map(crate::db::sort_key) == station {
            aired.artist = None;
        }
        aired.from_start = !self.aired.is_empty();
        if let Some(done) = self.aired.first_mut() {
            let heard = done.heard_secs(aired.at, self.radio_paused_since);
            if done.from_start && !done.scrobbled && heard >= RADIO_SCROBBLE_AFTER {
                done.scrobbled = true;
                let done = done.clone();
                self.scrobble_aired(&done, true);
            }
        }
        self.scrobble_aired(&aired, false);
        if let (Some(artist), Some(station)) = (aired.artist.clone(), self.on_air.as_ref().map(|s| s.name.clone())) {
            let day = aired.at.format("%Y-%m-%d").to_string();
            let heard = self.radio_heard.get_or_insert_with(|| Box::new(crate::radio::heard::Heard::read()));
            if heard.note(&artist, &station, &day) {
                heard.save();
            }
        }
        if self.config.ui.radio_song_covers {
            if let Some(artist) = &aired.artist {
                crate::radio::cover::fetch(artist, &aired.title);
            }
        }
        self.aired.insert(0, aired);
        self.artist_pin = None;
        self.aired.truncate(crate::radio::AIRED_KEPT);
    }

    /// Four minutes of one song count whatever happens after, as they do
    /// for a file (last.fm's rule).
    fn scrobble_the_long_ones(&mut self) {
        if self.player.state() != crate::player::PlaybackState::Playing || self.player.tuning() {
            return;
        }
        let Some(now) = self.aired.first_mut() else { return };
        let heard = now.heard_secs(chrono::Local::now(), None);
        if !now.scrobbled && heard as f64 >= crate::lastfm::watch::ENOUGH_SECS {
            now.scrobbled = true;
            let now = now.clone();
            self.scrobble_aired(&now, true);
        }
    }

    /// Hand what a station played to last.fm: playing now, or listened to.
    fn scrobble_aired(&self, aired: &crate::radio::Aired, listened: bool) {
        if !self.config.ui.radio_scrobble {
            return;
        }
        let (Some(scrobbler), Some(artist)) = (&self.scrobbler, &aired.artist) else { return };
        scrobbler.submit_aired(artist, &aired.title, listened.then(|| aired.at.timestamp()));
    }

    /// „merken" on a line under „gerade gehört": kept in the radio's own list,
    /// and loved on last.fm when that is asked for.
    pub fn keep_aired(&mut self, aired: &crate::radio::Aired) {
        let station = self.on_air.as_ref().map(|s| s.name.clone()).unwrap_or_default();
        let Some(radio) = self.radio.as_mut() else { return };
        let kept = radio.toggle_song(aired, &station);
        if kept && self.config.ui.radio_love {
            if let (Some(scrobbler), Some(artist)) = (&self.scrobbler, &aired.artist) {
                scrobbler.love(artist, &aired.title);
            }
        }
    }

    /// What the station plays now, as a song with no file behind it — for the
    /// words: a name and a title, nothing else.
    pub fn aired_song(&self) -> Option<Track> {
        self.on_air.as_ref()?;
        let aired = self.aired.first()?;
        let artist = aired.artist.clone()?;
        let never = chrono::NaiveDateTime::default();
        Some(Track {
            id: -1,
            path: std::path::PathBuf::new(),
            title: aired.title.clone(),
            artist,
            album_artist: String::new(),
            album: String::new(),
            genre: String::new(),
            year: None,
            track_number: None,
            disc_number: None,
            duration_secs: 0.0,
            bitrate: None,
            sample_rate: None,
            file_size: 0,
            format: String::new(),
            rating: 0,
            play_count: 0,
            last_played: None,
            date_added: never,
            date_modified: never,
            file_hash: None,
            has_artwork: false,
        })
    }

    /// The artist the station said it plays now, if it named one.
    pub fn aired_artist(&self) -> Option<String> {
        self.on_air.as_ref()?;
        self.aired.first()?.artist.clone()
    }

    /// What plays, in three words — title, artist, and the record or, on
    /// the radio, the station — for everything outside the window that says
    /// so: the menu bar and its menu, the system's panel and MPRIS, the
    /// keyboard's screen, the screen saver — on the radio no track plays, and
    /// they would say nothing.
    pub(in crate::gui) fn playing_words(&self) -> Option<[String; 3]> {
        if let Some(track) = self.player.current_track() {
            return Some([track.display_title().to_string(), track.shown_artist().to_string(), track.album.clone()]);
        }
        let station = self.on_air.as_ref()?;
        Some(match (self.aired.first(), self.show_on_now()) {
            (Some(aired), _) => [aired.title.clone(), aired.artist.clone().unwrap_or_default(), station.name.clone()],
            (None, Some(show)) => [show, station.name.clone(), String::new()],
            (None, None) => [station.name.clone(), tr!("radio").to_string(), String::new()],
        })
    }

    /// The picture of the show on now, where the programme gives one — shown
    /// only under „informationen"; everywhere else the station's own.
    pub(in crate::gui) fn show_picture(&self) -> Option<std::path::PathBuf> {
        let station = self.on_air.as_ref()?;
        self.programme
            .as_deref()
            .filter(|p| p.url == station.url)
            .and_then(|p| p.picture_now())
    }

    /// Who was last heard on `station`, the newest first, for the column about
    /// whom it plays while it names nobody.
    pub(in crate::gui) fn heard_on_station(&mut self, station: &str, most: usize) -> Vec<String> {
        if self.radio_heard.is_none() {
            if !crate::radio::heard::Heard::exists() {
                return Vec::new();
            }
            self.radio_heard = Some(Box::new(crate::radio::heard::Heard::read()));
        }
        self.radio_heard.as_ref().map(|h| h.on_station(station, most)).unwrap_or_default()
    }

    /// When and on which station `artist` was last heard on the radio, for
    /// their column. Read from the file the first time, and only if the radio
    /// ever named anybody.
    pub(in crate::gui) fn heard_on_radio(&mut self, artist: &str) -> Option<crate::radio::heard::Hearing> {
        if self.radio_heard.is_none() {
            if !crate::radio::heard::Heard::exists() {
                return None;
            }
            self.radio_heard = Some(Box::new(crate::radio::heard::Heard::read()));
        }
        self.radio_heard.as_ref()?.last(artist).cloned()
    }

    /// What the show on now is, in the programme's words. Where else the show
    /// on now is to be found — its soundcloud, its youtube — as the programme
    /// says.
    pub(in crate::gui) fn show_links(&self) -> Vec<(String, String)> {
        let Some(station) = self.on_air.as_ref() else { return Vec::new() };
        self.programme
            .as_deref()
            .filter(|p| p.url == station.url)
            .map(|p| p.links_now())
            .unwrap_or_default()
    }

    pub(in crate::gui) fn show_about(&self) -> Option<String> {
        let station = self.on_air.as_ref()?;
        let programme = self.programme.as_deref().filter(|p| p.url == station.url)?;
        programme.about_now().map(str::to_string)
    }

    /// The show on now, where the station publishes its programme: what plays,
    /// for a station that does not say it in its stream.
    pub(in crate::gui) fn show_on_now(&self) -> Option<String> {
        let programme = self.programme.as_deref()?;
        if self.on_air.as_ref().map(|s| s.url.as_str()) != Some(programme.url.as_str()) {
            return None;
        }
        programme.now(chrono::Local::now()).map(|s| s.title.clone())
    }
}
