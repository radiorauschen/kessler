use crate::gui::*;

const ASK_AFTER: std::time::Duration = std::time::Duration::from_millis(400);

const HEAR_EVERY: std::time::Duration = std::time::Duration::from_secs(2);

pub(in crate::gui) const RADIO_SCROBBLE_AFTER: i64 = 30;

impl GuiApp {
    pub fn showing_radio(&self) -> bool {
        self.radio.is_some()
    }

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
        self.radio = Some(Box::new(crate::radio::Radio::open()));
        self.switch_sides(View::Music, View::Radio);
    }

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

    pub(in crate::gui) fn music_shown(&self) -> bool {
        !self.showing_radio() && !self.showing_podcasts() && !self.showing_ipod()
    }

    pub(in crate::gui) fn all_off(&self) -> bool {
        !self.config.ui.music && self.music_shown()
    }

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

    fn back_to_before(&mut self, gone: View) {
        let before = self.view_before.filter(|v| *v != gone);
        match before {
            Some(View::Radio) if self.config.ui.radio => self.read_from(true, false),
            Some(View::Podcasts) if self.config.ui.podcasts => self.show_podcasts(),
            Some(View::Ipod) if self.attached.is_some() => self.read_from(false, true),
            _ => {}
        }
    }

    pub(in crate::gui) fn music_held(&self) -> bool {
        !self.config.ui.music && self.on_air.is_none() && !self.playing_episode()
    }

    pub fn toggle_pause(&mut self) {
        if self.music_held() && self.player.state() == crate::player::PlaybackState::Stopped {
            return;
        }
        self.player.toggle_pause();
    }

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

    pub(in crate::gui) fn leave_radio(&mut self) {
        if self.radio.take().is_none() {
            return;
        }
        self.radio_typed_at = None;
        if self.config.ui.radio_info != self.artist_open {
            self.config.ui.radio_info = self.artist_open;
            self.save_config();
        }
        self.switch_sides(View::Radio, View::Music);
    }

    pub(in crate::gui) fn show_on_air(&mut self) {
        let Some(url) = self.on_air.as_ref().map(|s| s.url.clone()) else { return };
        if !self.showing_radio() {
            self.read_from(true, false);
        }
        if let Err(e) = self.close_filter() {
            self.set_status(format!("{e}"));
        }
        let Some(radio) = self.radio.as_mut() else { return };
        if let Some(at) = radio.kept.iter().position(|s| s.url == url) {
            radio.cursor = at;
        }
        self.focus = crate::gui::Column::Artists;
    }

    pub(in crate::gui) fn radio_listed(&self) -> Vec<crate::radio::Station> {
        let Some(radio) = &self.radio else { return Vec::new() };
        if self.query().is_some() {
            radio.found.clone()
        } else if let Some(similar) = &radio.similar {
            similar.stations.clone()
        } else {
            radio.kept.clone()
        }
    }

    pub(in crate::gui) fn move_radio_cursor(&mut self, delta: i32) {
        let len = self.radio_listed().len();
        let Some(radio) = self.radio.as_mut() else { return };
        radio.resting = false;
        let last = len.saturating_sub(1) as i64;
        radio.cursor = (radio.cursor as i64 + i64::from(delta)).clamp(0, last) as usize;
    }

    fn heard_len(&self) -> (usize, usize) {
        let songs = self.radio.as_ref().map_or(0, |r| r.songs.len());
        (self.aired.len(), songs)
    }

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

    pub(in crate::gui) fn play_radio_cursor(&mut self) {
        let listed = self.radio_listed();
        let Some(at) = self.radio.as_ref().map(|r| r.cursor) else { return };
        if let Some(station) = listed.get(at) {
            self.play_station(station.clone());
        }
    }

    pub fn play_station(&mut self, station: crate::radio::Station) {
        if self.on_air.as_ref().is_none_or(|on| on.url != station.url) {
            self.aired.clear();
        }
        if self.config.ui.radio_logos {
            crate::radio::fetch_logo(&station);
            if let (Some(asking), Some(radio)) = (crate::radio::fetch_site_picture(&station.homepage), self.radio.as_mut()) {
                radio.site_pictures.push((station.name.clone(), asking));
            }
        }
        self.player.play_stream(&station.url);
        self.on_air = Some(station);
    }

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
        let kept = match &self.radio {
            Some(radio) => radio.kept.clone(),
            None => crate::radio::Radio::open().kept,
        };
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

    pub(in crate::gui) fn radio_typed(&mut self) {
        self.radio_typed_at = Some(std::time::Instant::now());
    }

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
        let counting = matches!(
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
            let heard = (aired.at - done.at).num_seconds();
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
        self.aired.insert(0, aired);
        self.artist_pin = None;
        self.aired.truncate(crate::radio::AIRED_KEPT);
    }

    fn scrobble_the_long_ones(&mut self) {
        if self.player.state() != crate::player::PlaybackState::Playing || self.player.tuning() {
            return;
        }
        let Some(now) = self.aired.first_mut() else { return };
        let heard = (chrono::Local::now() - now.at).num_seconds();
        if !now.scrobbled && heard as f64 >= crate::lastfm::watch::ENOUGH_SECS {
            now.scrobbled = true;
            let now = now.clone();
            self.scrobble_aired(&now, true);
        }
    }

    fn scrobble_aired(&self, aired: &crate::radio::Aired, listened: bool) {
        if !self.config.ui.radio_scrobble {
            return;
        }
        let (Some(scrobbler), Some(artist)) = (&self.scrobbler, &aired.artist) else { return };
        scrobbler.submit_aired(artist, &aired.title, listened.then(|| aired.at.timestamp()));
    }

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

    pub fn aired_artist(&self) -> Option<String> {
        self.on_air.as_ref()?;
        self.aired.first()?.artist.clone()
    }

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

    pub(in crate::gui) fn show_picture(&self) -> Option<std::path::PathBuf> {
        let station = self.on_air.as_ref()?;
        self.programme
            .as_deref()
            .filter(|p| p.url == station.url)
            .and_then(|p| p.picture_now())
    }

    pub(in crate::gui) fn heard_on_station(&mut self, station: &str, most: usize) -> Vec<String> {
        if self.radio_heard.is_none() {
            if !crate::radio::heard::Heard::exists() {
                return Vec::new();
            }
            self.radio_heard = Some(Box::new(crate::radio::heard::Heard::read()));
        }
        self.radio_heard.as_ref().map(|h| h.on_station(station, most)).unwrap_or_default()
    }

    pub(in crate::gui) fn heard_on_radio(&mut self, artist: &str) -> Option<crate::radio::heard::Hearing> {
        if self.radio_heard.is_none() {
            if !crate::radio::heard::Heard::exists() {
                return None;
            }
            self.radio_heard = Some(Box::new(crate::radio::heard::Heard::read()));
        }
        self.radio_heard.as_ref()?.last(artist).cloned()
    }

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

    pub(in crate::gui) fn show_on_now(&self) -> Option<String> {
        let programme = self.programme.as_deref()?;
        if self.on_air.as_ref().map(|s| s.url.as_str()) != Some(programme.url.as_str()) {
            return None;
        }
        programme.now(chrono::Local::now()).map(|s| s.title.clone())
    }
}
