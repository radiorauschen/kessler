use crate::gui::*;

impl GuiApp {
    pub(in crate::gui) fn reload_from_device(
        &mut self,
        artist: Option<String>,
        album: Option<String>,
        query: Option<&str>,
    ) -> Result<()> {
        let Some(device) = &self.attached else {
            return Ok(());
        };

        self.albums = device.albums(artist.as_deref(), query);
        if self.album_index > self.albums.len() {
            self.album_index = 0;
        }
        let album = if album.is_some() {
            self.selected_album()
        } else {
            None
        };
        let artist = self.selected_album_artist().or(artist);

        let (sort, order) = self.track_order();
        let device = self.attached.as_ref().expect("held above");
        self.tracks = device.tracks(artist.as_deref(), album.as_deref(), sort, order, query);
        if self.track_index >= self.tracks.len() {
            self.track_index = 0;
        }
        self.reset_track_selection();
        Ok(())
    }

    pub(in crate::gui) fn give_up_devices(&mut self) {
        self.device_watch = None;
        if self.attached.is_none() {
            return;
        }
        if self.showing_ipod() {
            if let Err(e) = self.use_device(false) {
                self.set_status(format!("{e}"));
            }
        }
        self.close_device_panel();
        self.attached = None;
        if let Err(e) = self.reload_all() {
            self.set_status(format!("{e}"));
        }
    }

    pub(in crate::gui) fn watch_for_devices(&mut self) {
        if self.device_watch.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut known: Option<std::path::PathBuf> = None;
            let mut mounted: Option<Vec<std::path::PathBuf>> = None;
            #[cfg(target_os = "linux")]
            let mut tried: Vec<std::path::PathBuf> = Vec::new();
            loop {
                #[cfg(target_os = "linux")]
                if crate::ipod::udisks::AUTO.load(std::sync::atomic::Ordering::Relaxed) {
                    use crate::ipod::udisks;
                    tried.retain(|device| device.exists());
                    for device in udisks::unmounted() {
                        if tried.contains(&device) {
                            continue;
                        }
                        tried.push(device.clone());
                        let said = match udisks::mount(&device) {
                            Ok(at) => tr!("ipod eingehängt: {}", at.display()),
                            Err(why) => tr!("ipod nicht eingehängt: {}", why),
                        };
                        if tx.send(DeviceNews::Said(said)).is_err() {
                            return;
                        }
                    }
                }
                let now = crate::ipod::mounted();
                if mounted.as_ref() == Some(&now) {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    continue;
                }
                mounted = Some(now);
                let found = crate::ipod::IpodDevice::detect().into_iter().next();
                let here = found.as_ref().map(|d| d.mount_point.clone());

                if here != known {
                    known = here;
                    let news = found.and_then(|device| {
                        let db = device.read_database().ok()?;
                        let info =
                            crate::ipod::device_info::DeviceInfo::read(&device.mount_point);
                        Some(attached::Attached::read(
                            device.name.clone(),
                            device.mount_point.clone(),
                            &db,
                            device.capacity(),
                            &info,
                        ))
                    });
                    if tx.send(DeviceNews::Attached(news)).is_err() {
                        return;
                    }
                }
                std::thread::sleep(std::time::Duration::from_secs(2));
            }
        });
        self.device_watch = Some(rx);
    }

    pub(in crate::gui) fn poll_devices(&mut self) -> Result<()> {
        let Some(rx) = &self.device_watch else {
            return Ok(());
        };
        let news = match rx.try_recv() {
            Ok(DeviceNews::Attached(news)) => news,
            Ok(DeviceNews::Said(said)) => {
                self.note(log::Kind::Ipod, said.clone());
                self.set_status(said);
                return Ok(());
            }
            Err(_) => return Ok(()),
        };

        let was_showing = self.showing_ipod();
        let gone = news.is_none();
        let name = news.as_ref().map(|a| a.name.clone());
        let count = news.as_ref().map(|a| a.tracks.len()).unwrap_or(0);
        self.attached = news;
        if gone {
            self.close_device_panel();
        } else if self.device_open {
            self.refresh_device_summary();
        }

        if gone {
            if was_showing {
                self.source = Source::Library;
                self.artist_index = self.home_row();
                self.album_index = 0;
                self.track_index = 0;
                self.set_notice(tr!("ipod abgezogen"));
                self.note(log::Kind::Ipod, tr!("abgezogen"));
            }
            self.reload_all()?;
        } else {
            self.set_status(tr!("{} angesteckt — {} titel",
                name.unwrap_or_default(),
                text::count(count as u64)
            ));
            self.take_plays();
            self.on_connect();
        }
        Ok(())
    }

    fn take_plays(&mut self) {
        use crate::ipod::playcounts;
        if !self.config.ipod_take_plays {
            return;
        }
        let Some(mount) = self.attached.as_ref().map(|a| a.mount.clone()) else {
            return;
        };
        let Ok(device) = crate::ipod::IpodDevice::at(&mount) else {
            return;
        };
        let played = match playcounts::read(&device) {
            Ok(played) if !played.is_empty() => played,
            _ => return,
        };
        let Ok(db) = device.read_database() else {
            return;
        };
        let store = self.config.ipod_plays_path();
        let mut all: std::collections::HashMap<String, playcounts::Taken> = std::fs::read_to_string(&store)
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default();
        let key = format!("{:016x}", db.db_id);
        let taken = all.get(&key).cloned().unwrap_or_default();
        let (heard, now) = playcounts::new_plays(&db, &played, &taken, &playcounts::database_stamp(&device));
        if heard.is_empty() {
            return;
        }
        let Ok(library) = self.db.get_tracks(None, None, SortField::Artist, SortOrder::Ascending, None) else {
            return;
        };
        let oldest = chrono::Utc::now().timestamp() - 14 * 24 * 3600;
        let (mut plays, mut scrobbled, mut rated, mut unknown) = (0u32, 0u32, 0u32, 0u32);
        for one in &heard {
            let Some(at) = crate::ipod::sync::library_index(&one.track, &library) else {
                unknown += one.plays;
                continue;
            };
            let track = &library[at];
            if one.plays > 0 {
                let last = one
                    .last_played
                    .and_then(|t| chrono::DateTime::from_timestamp(t, 0))
                    .map(|t| t.naive_utc())
                    .unwrap_or_else(|| chrono::Local::now().naive_local());
                let seconds = track.duration_secs.max(1.0);
                if self.db.add_plays(track.id, one.plays, last, seconds).is_ok() {
                    plays += one.plays;
                }
                if let Some(scrobbler) = &self.scrobbler {
                    for k in (0..one.plays).rev() {
                        let ended = last - chrono::Duration::milliseconds((seconds * 1000.0) as i64) * k as i32;
                        let started = ended - chrono::Duration::milliseconds((seconds * 1000.0) as i64);
                        let Some(at) = chrono::TimeZone::from_local_datetime(&chrono::Local, &started).earliest() else {
                            continue;
                        };
                        let started_at = at.timestamp();
                        if started_at < oldest {
                            continue;
                        }
                        scrobbler.submit(crate::lastfm::watch::Event::Scrobble {
                            track: Box::new(track.clone()),
                            started_at,
                        });
                        scrobbled += 1;
                    }
                }
            }
            if let Some(rating) = one.rating {
                if self.db.set_rating(track.id, rating / 20).is_ok() {
                    rated += 1;
                }
            }
        }
        all.insert(key, now);
        if let Ok(text) = toml::to_string(&all) {
            let _ = std::fs::write(&store, text);
        }
        let mut said = tr!("vom ipod: {} wiedergaben übernommen", text::count(plays as u64));
        if scrobbled > 0 {
            said.push_str(&tr!(", {} an last.fm", text::count(scrobbled as u64)));
        }
        if rated > 0 {
            said.push_str(&tr!(", {} bewertungen", text::count(rated as u64)));
        }
        if unknown > 0 {
            said.push_str(&tr!(", {} nicht in der bibliothek", text::count(unknown as u64)));
        }
        self.note(log::Kind::Ipod, said.clone());
        self.set_status(said);
        let _ = self.reload_all();
    }

    fn on_connect(&mut self) {
        let wanted = self.config.ipod_on_connect.clone();
        if (wanted != "fragen" && wanted != "abgleichen") || !self.config.ui.music {
            return;
        }
        let writable = self.attached.as_ref().is_some_and(|a| a.refusal.is_none());
        if !writable || self.transfer.is_some() {
            return;
        }
        if let Err(e) = self.transfer_library() {
            self.set_status(format!("{e}"));
            return;
        }
        if wanted == "abgleichen" {
            let convert = self.converts_for_ipod();
            let idle = self.transfer.as_ref().is_some_and(|t| {
                t.plan.is_empty() && !(convert && !t.plan.incompatible.is_empty())
            });
            if idle {
                self.transfer = None;
                self.set_status(tr!("nichts zu überspielen"));
                return;
            }
            self.run_transfer(convert);
            self.note(log::Kind::Ipod, tr!("synchronisierung beim anschliessen gestartet"));
        }
    }

    pub fn toggle_device_view(&mut self) {
        let to = !self.showing_ipod();
        if let Err(e) = self.use_device(to) {
            self.set_status(format!("{e}"));
        }
        if to {
            self.open_device_panel();
        } else {
            self.close_device_panel();
        }
    }

    pub fn open_device_panel(&mut self) {
        if self.attached.is_none() {
            self.set_status(tr!("kein ipod angesteckt"));
            return;
        }
        self.menu_open = false;
        self.show_device_panel();
    }

    pub(in crate::gui) fn show_device_panel(&mut self) {
        self.device_open = true;
        if !self.all_sides() {
            self.log_open = false;
            self.artist_open = false;
            self.lyrics_open = false;
            self.gaps.open = false;
            self.eq_open = false;
        }
        self.leave_the_left();
        self.refresh_device_summary();
    }

    pub fn close_device_panel(&mut self) {
        self.close_transfer();
        if self.transfer.is_some() && self.attached.is_some() {
            self.set_status(tr!("die synchronisierung läuft noch"));
        }
        self.device_open = false;
        self.device_art = None;
        self.device_handling = device::Handling::default();
        self.naming_device = None;
        self.follow_the_playing();
    }

    pub fn refresh_device_summary(&mut self) {
        let Some(attached) = &self.attached else {
            self.device_summary = None;
            return;
        };
        let key = (attached.mount.clone(), attached.tracks.len());
        if self
            .device_summary
            .as_ref()
            .is_some_and(|s| s.for_device == key)
        {
            return;
        }
        let info = crate::ipod::device_info::DeviceInfo::read(&attached.mount);
        let written = crate::ipod::IpodDevice::at(&attached.mount)
            .ok()
            .and_then(|d| std::fs::metadata(d.itunesdb_path()).ok())
            .and_then(|m| m.modified().ok())
            .map(|t| {
                chrono::DateTime::<chrono::Local>::from(t)
                    .format("%Y-%m-%d")
                    .to_string()
            });
        let library = self
            .db
            .get_tracks(None, None, SortField::Artist, SortOrder::Ascending, None)
            .unwrap_or_default();
        let generation = info.generation;
        let mut summary = device::Summary::of(
            attached.mount.clone(),
            &attached.tracks,
            &library,
            |format| generation.plays(format),
            info.serial.clone(),
            written,
        );
        let device_db = crate::ipod::IpodDevice::at(&attached.mount)
            .ok()
            .and_then(|d| d.read_database().ok());
        if let Some(device_db) = device_db {
            let newest = self.newest_list();
            if let Some(list) = &newest {
                summary.newest_current = crate::ipod::sync::list_is_current(
                    list,
                    &device_db,
                    generation,
                    self.converts_for_ipod(),
                );
            }
            let loud = crate::ipod::sync::soundcheck_table(&self.db);
            let plan = crate::ipod::sync::plan(&library, &device_db, generation, &loud);
            summary.missing = plan.to_copy.len() + plan.incompatible.len();
            summary.missing_bytes = plan.bytes_to_copy
                + plan.incompatible.iter().map(|t| t.file_size).sum::<u64>();
            summary.unplayable = plan.incompatible.len();
            summary.unplayable_formats = plan.incompatible_formats();
            summary.retag = plan.retag.len();
            summary.sound_only = plan.sound_only;
            summary.respell = plan.respell.len();
            summary.reorder = plan.reorder;
            let keep: Vec<_> = newest.into_iter().collect();
            summary.old_lists = crate::ipod::sync::foreign_lists(&device_db, &keep).len();
            if !generation.has_playlists() {
                summary.newest_current = true;
                summary.old_lists = 0;
            }
        }
        self.device_summary = Some(summary);
    }

    pub fn tracks_on_device(&mut self) -> crate::gui::OnDevice {
        let Some(attached) = &self.attached else {
            self.on_device = None;
            return std::sync::Arc::default();
        };
        let key = (attached.mount.clone(), attached.tracks.len());
        if let Some((at, ids)) = &self.on_device {
            if *at == key {
                return ids.clone();
            }
        }
        let library = self
            .db
            .get_tracks(None, None, SortField::Artist, SortOrder::Ascending, None)
            .unwrap_or_default();
        let ids = crate::ipod::IpodDevice::at(&attached.mount)
            .ok()
            .and_then(|d| d.read_database().ok())
            .map(|db| crate::ipod::sync::on_device(&library, &db))
            .unwrap_or_default();
        let ids = std::sync::Arc::new(ids);
        self.on_device = Some((key, ids.clone()));
        ids
    }

    pub fn converts_for_ipod(&self) -> bool {
        self.config.ipod_convert && crate::ipod::sync::transcoder_available()
    }

    pub fn newest_list(&self) -> Option<crate::ipod::sync::DeviceList> {
        const NAME: &str = "zuletzt hinzugefügt";
        if self.attached.as_ref().is_some_and(|a| !a.generation.has_playlists()) {
            return None;
        }
        let tracks = self
            .db
            .get_recent_tracks(None, crate::config::RECENT_SHOWN)
            .ok()?;
        if tracks.is_empty() {
            return None;
        }
        let name = crate::lang::t(NAME).to_string();
        let also_named = crate::lang::Lang::ALL
            .into_iter()
            .map(|lang| crate::lang::in_lang(lang, NAME).to_string())
            .filter(|n| *n != name)
            .collect();
        Some(crate::ipod::sync::DeviceList { name, also_named, tracks })
    }

    pub fn podcast_list(&self) -> Option<crate::ipod::sync::DeviceList> {
        if !self.config.ipod_podcasts || !self.config.ui.podcasts {
            return None;
        }
        if self.attached.as_ref().is_some_and(|a| !a.generation.has_playlists()) {
            return None;
        }
        let kept = crate::podcast::Kept::read();
        let mut tracks = Vec::new();
        for (episode, show) in kept.fetched_episodes() {
            let starred = kept.is_starred(&episode.id);
            let heard = kept.left(&episode.id).is_some_and(|left| left.done);
            if !starred && (heard || kept.is_archived(&episode.id)) {
                continue;
            }
            let path = crate::podcast::cached(&episode.url);
            let Ok(mut track) = crate::library::metadata::read_metadata(&path) else { continue };
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            std::hash::Hash::hash(&episode.id, &mut hasher);
            track.id = -1 - (std::hash::Hasher::finish(&hasher) & 0x7FFF_FFFF) as i64;
            track.title = episode.title.clone();
            track.artist = show.title.clone();
            track.album_artist = String::new();
            track.album = show.title.clone();
            track.genre = "podcast".into();
            track.track_number = None;
            track.disc_number = None;
            tracks.push(track);
        }
        if tracks.is_empty() {
            return None;
        }
        Some(crate::ipod::sync::DeviceList { name: "podcasts".into(), also_named: Vec::new(), tracks })
    }

    pub fn showing_ipod(&self) -> bool {
        self.source == Source::Device && self.attached.is_some()
    }

    pub fn use_device(&mut self, on: bool) -> Result<()> {
        let wanted = match (on, self.attached.is_some()) {
            (true, true) => Source::Device,
            _ => Source::Library,
        };
        if wanted == self.source {
            return Ok(());
        }
        self.source = wanted;
        if wanted == Source::Device {
            self.leave_radio();
            self.switch_sides(View::Music, View::Ipod);
        } else {
            self.switch_sides(View::Ipod, View::Music);
        }
        self.artist_index = self.home_row();
        self.album_index = 0;
        self.track_index = 0;
        self.focus = Column::Artists;
        self.reveal_all = true;
        self.reload_all()
    }

    pub fn eject_device(&mut self) -> Result<String> {
        let Some(device) = &self.attached else {
            return Err(crate::error::KesslerError::Ipod(tr!("kein ipod da").into()));
        };
        let name = device.name.clone();
        let mount = device.mount.clone();

        let playing_from_it = self
            .player
            .current_track()
            .is_some_and(|track| track.path.starts_with(&mount));
        if playing_from_it {
            self.player.stop();
        }

        crate::ipod::IpodDevice::at(&mount)?.eject()?;

        self.attached = None;
        self.close_device_panel();
        if self.source == Source::Device {
            self.source = Source::Library;
            self.artist_index = self.home_row();
            self.album_index = 0;
            self.track_index = 0;
        }
        self.reload_all()?;
        Ok(name)
    }

    pub fn transfer_library(&mut self) -> Result<()> {
        let mut everything =
            self.db
                .get_tracks(None, None, SortField::Artist, SortOrder::Ascending, None)?;
        let podcasts = self.podcast_list();
        if let Some(list) = &podcasts {
            everything.extend(list.tracks.iter().cloned());
        }
        self.menu_open = false;
        let lists = self.newest_list().into_iter().chain(podcasts.clone()).collect();
        self.plan_transfer(everything, tr!("die ganze bibliothek").to_string(), lists)?;
        let keep: Vec<crate::ipod::sync::DeviceList> = self.newest_list().into_iter().chain(podcasts.clone()).collect();
        let db = self
            .attached
            .as_ref()
            .and_then(|a| crate::ipod::IpodDevice::at(&a.mount).ok())
            .and_then(|d| d.read_database().ok());
        if let (Some(transfer), Some(db)) = (self.transfer.as_mut(), db) {
            crate::ipod::sync::plan_dropping_lists(&mut transfer.plan, &db, &keep);
            if self.config.ipod_podcasts {
                let now = podcasts.unwrap_or(crate::ipod::sync::DeviceList { name: "podcasts".into(), also_named: Vec::new(), tracks: Vec::new() });
                crate::ipod::sync::plan_podcasts(&mut transfer.plan, &now, &db);
            }
        }
        Ok(())
    }

    pub fn backup_device(&mut self) {
        let Some(mount) = self.attached.as_ref().map(|a| a.mount.clone()) else {
            self.set_status(tr!("kein ipod angesteckt"));
            return;
        };
        let done = crate::ipod::IpodDevice::at(&mount)
            .and_then(|device| crate::ipod::sync::backup_database(&device));
        match done {
            Ok(path) => {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.display().to_string());
                self.backup_said = Some(tr!("gesichert").to_string());
                self.set_status(tr!("datenbank gesichert: {}", name));
            }
            Err(e) => {
                self.backup_said = Some(tr!("ging nicht").to_string());
                self.set_status(format!("{e}"));
            }
        }
    }

    pub fn start_naming_device(&mut self) {
        let Some(device) = &self.attached else { return };
        let mut entry = crate::gui::tagedit::Entry::new(device.name.clone());
        entry.mark_all();
        self.naming_device = Some(entry);
    }

    pub fn rename_device(&mut self) {
        let Some(entry) = self.naming_device.take() else { return };
        let Some(device) = &self.attached else { return };
        let name = entry.text().trim().to_string();
        if name.is_empty() || name == device.name {
            return;
        }
        let mount = device.mount.clone();
        let done = crate::ipod::IpodDevice::at(&mount).and_then(|ipod| {
            let info = crate::ipod::device_info::DeviceInfo::read(&mount);
            crate::ipod::sync::rename(&ipod, &info, &name)
        });
        match done {
            Ok(()) => {
                let said = tr!("der ipod heisst jetzt {}", crate::gui::theme::lower(&name));
                self.note(log::Kind::Ipod, said.clone());
                self.set_notice(said);
                self.reread_device();
            }
            Err(e) => self.set_status(format!("{e}")),
        }
    }

    pub fn save_device_report(&mut self) {
        let Some(mount) = self.attached.as_ref().map(|a| a.mount.clone()) else { return };
        let library = self
            .db
            .get_tracks(None, None, SortField::Artist, SortOrder::Ascending, None)
            .unwrap_or_default();
        let loud = crate::ipod::sync::soundcheck_table(&self.db);
        let done = crate::ipod::IpodDevice::at(&mount).and_then(|device| {
            let info = crate::ipod::device_info::DeviceInfo::read(&mount);
            let db = device.read_database()?;
            let plan = crate::ipod::sync::plan(&library, &db, info.generation, &loud);
            let said = crate::ipod::report::write(&device, &info, &db, Some(&plan));
            let home = std::env::var_os("HOME").map(std::path::PathBuf::from).unwrap_or_default();
            let desktop = home.join("Desktop");
            let dir = if desktop.is_dir() { desktop } else { home };
            let stamp = chrono::Local::now().format("%Y-%m-%d-%H%M");
            let path = dir.join(format!("kessler-ipod-bericht-{stamp}.txt"));
            std::fs::write(&path, said)?;
            Ok(path)
        });
        match done {
            Ok(path) => {
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let said = tr!("bericht gespeichert: {}", name);
                self.note(log::Kind::Ipod, said.clone());
                self.set_notice(said);
                let _ = crate::gui::reveal::show(&path);
            }
            Err(e) => self.set_status(format!("{e}")),
        }
    }

    pub(in crate::gui) fn handle_naming_keys(&mut self, ctx: &egui::Context) {
        use egui::{Key, Modifiers};
        const NONE: Modifiers = Modifiers::NONE;
        let taken =
            |m: Modifiers, k: Key| -> usize { ctx.input_mut(|i| i.count_and_consume_key(m, k)) };
        let pressed = |m: Modifiers, k: Key| -> bool { ctx.input_mut(|i| i.consume_key(m, k)) };
        if pressed(NONE, Key::Escape) {
            self.naming_device = None;
            return;
        }
        if pressed(NONE, Key::Enter) {
            self.rename_device();
            return;
        }
        let all = pressed(Modifiers::COMMAND, Key::A);
        let cleared = pressed(Modifiers::COMMAND, Key::Backspace);
        let typed = crate::gui::typed(ctx, false);
        let Some(entry) = self.naming_device.as_mut() else { return };
        if all {
            entry.mark_all();
        }
        if cleared {
            entry.clear();
        }
        for _ in 0..taken(NONE, Key::ArrowLeft) {
            entry.left();
        }
        for _ in 0..taken(NONE, Key::ArrowRight) {
            entry.right();
        }
        if pressed(NONE, Key::Home) || pressed(NONE, Key::ArrowUp) {
            entry.home();
        }
        if pressed(NONE, Key::End) || pressed(NONE, Key::ArrowDown) {
            entry.end();
        }
        for _ in 0..taken(NONE, Key::Backspace) {
            entry.backspace();
        }
        for _ in 0..taken(NONE, Key::Delete) {
            entry.delete();
        }
        if !typed.is_empty() {
            entry.insert(&typed);
        }
    }

    pub fn plan_transfer(
        &mut self,
        tracks: Vec<Track>,
        what: String,
        lists: Vec<crate::ipod::sync::DeviceList>,
    ) -> Result<()> {
        let Some(attached) = &self.attached else {
            self.set_status(tr!("kein ipod angesteckt"));
            return Ok(());
        };
        if !self.config.ui.music {
            self.set_status(tr!("die musik ist ausgeschaltet — kein abgleich mit dem ipod"));
            return Ok(());
        }
        if tracks.is_empty() {
            self.set_status(tr!("nichts ausgewählt"));
            return Ok(());
        }

        let device = crate::ipod::IpodDevice::at(&attached.mount)?;
        let info = crate::ipod::device_info::DeviceInfo::read(&attached.mount);
        if let Err(why) = info.writable() {
            self.set_status(why);
            return Ok(());
        }

        let on_device = device.read_database()?;
        let loud = crate::ipod::sync::soundcheck_table(&self.db);
        let whole = self
            .db
            .get_tracks(None, None, SortField::Artist, SortOrder::Ascending, None)
            .unwrap_or_default();
        let mut plan = crate::ipod::sync::plan_in(&tracks, &whole, &on_device, info.generation, &loud);
        for list in lists {
            crate::ipod::sync::plan_list(
                &mut plan,
                list,
                &on_device,
                info.generation,
                self.converts_for_ipod(),
            );
        }
        let can_convert =
            !plan.incompatible.is_empty() && crate::ipod::sync::transcoder_available();

        self.row_menu = None;
        if !self.showing_ipod() {
            self.use_device(true)?;
        }
        self.show_device_panel();
        if self.removal.as_ref().is_some_and(|r| r.going.is_none()) {
            self.removal = None;
        }
        let music_at_start = self.device_summary.as_ref().map_or(0, |s| s.music_bytes);
        self.transfer = Some(Transfer {
            what,
            plan,
            free: device.capacity().map(|(_, free)| free),
            can_convert,
            stage: Stage::Asking,
            music_at_start,
            space_read_at: None,
        });
        Ok(())
    }

    pub fn run_transfer(&mut self, convert: bool) {
        let Some(transfer) = self.transfer.as_mut() else {
            return;
        };
        if !matches!(transfer.stage, Stage::Asking) {
            return;
        }
        let Some(mount) = self.attached.as_ref().map(|a| a.mount.clone()) else {
            self.set_status(tr!("kein ipod angesteckt"));
            return;
        };
        let plan = std::mem::take(&mut transfer.plan);
        let total = plan.to_copy.len() + if convert { plan.incompatible.len() } else { 0 };

        let (tx, steps) = mpsc::channel();
        let said = tx.clone();
        std::thread::spawn(move || {
            let outcome = (|| -> std::result::Result<(), String> {
                let device =
                    crate::ipod::IpodDevice::at(&mount).map_err(|e| e.to_string())?;
                let info = crate::ipod::device_info::DeviceInfo::read(&mount);
                crate::ipod::sync::apply(&device, &info, plan, convert, |step| {
                    let _ = said.send(Moved::Along(step));
                })
                .map_err(|e| e.to_string())
            })();
            let _ = tx.send(Moved::Ended(outcome));
        });

        transfer.stage = Stage::Going {
            done: 0,
            total,
            doing: String::new(),
            failed: 0,
            steps,
        };
    }

    pub fn close_transfer(&mut self) {
        if matches!(
            self.transfer.as_ref().map(|t| &t.stage),
            Some(Stage::Going { .. })
        ) {
            return;
        }
        self.transfer = None;
    }

    pub fn transferring(&self) -> bool {
        self.transfer.is_some()
    }

    pub fn refuses_drops(&self) -> Option<String> {
        let attached = self.attached.as_ref()?;
        if let Some(why) = &attached.refusal {
            return Some(why.clone());
        }
        if self.copying() {
            return Some(tr!("auf den ipod wird gerade kopiert").to_string());
        }
        if self.removal.as_ref().is_some_and(|r| r.going.is_some()) {
            return Some(tr!("vom ipod wird gerade gelöscht").to_string());
        }
        None
    }

    pub fn copying(&self) -> bool {
        matches!(
            self.transfer.as_ref().map(|t| &t.stage),
            Some(Stage::Going { .. })
        )
    }

    pub(in crate::gui) fn follow_the_space(&mut self) {
        const EVERY: std::time::Duration = std::time::Duration::from_millis(500);
        let Some(transfer) = self.transfer.as_mut() else {
            return;
        };
        if !matches!(transfer.stage, Stage::Going { .. })
            || transfer.space_read_at.is_some_and(|at| at.elapsed() < EVERY)
        {
            return;
        }
        transfer.space_read_at = Some(std::time::Instant::now());
        let Some(attached) = self.attached.as_mut() else {
            return;
        };
        let Some(space) = crate::ipod::IpodDevice::at(&attached.mount).ok().and_then(|d| d.capacity())
        else {
            return;
        };
        attached.capacity = Some(space);
        if let (Some(summary), Some(was)) = (self.device_summary.as_mut(), transfer.free) {
            summary.music_bytes = transfer.music_at_start + was.saturating_sub(space.1);
        }
    }

    pub(in crate::gui) fn poll_transfer(&mut self) {
        use crate::ipod::sync::SyncProgress;
        self.follow_the_space();
        let Some(transfer) = self.transfer.as_mut() else {
            return;
        };
        let Stage::Going {
            done,
            total,
            doing,
            failed,
            steps,
        } = &mut transfer.stage
        else {
            return;
        };

        let mut ended: Option<std::result::Result<String, String>> = None;
        while let Ok(moved) = steps.try_recv() {
            match moved {
                Moved::Along(SyncProgress::Copying { index, title, .. })
                | Moved::Along(SyncProgress::Transcoding { index, title, .. }) => {
                    *done = index;
                    *doing = title;
                }
                Moved::Along(SyncProgress::CopyFailed { .. }) => *failed += 1,
                Moved::Along(SyncProgress::WritingDatabase) => {
                    *done = *total;
                    *doing = tr!("datenbank").to_string();
                }
                Moved::Along(SyncProgress::Done {
                    copied,
                    failed: f,
                    lists,
                    retagged,
                    dropped,
                }) => {
                    let mut parts: Vec<String> = Vec::new();
                    match copied {
                        0 => {}
                        1 => parts.push(tr!("1 titel überspielt").to_string()),
                        n => parts.push(tr!("{} titel überspielt", text::count(n as u64))),
                    }
                    if f > 0 {
                        parts.push(tr!("{} nicht", text::count(f as u64)));
                    }
                    if retagged > 0 {
                        parts.push(tr!("{} titel aktualisiert", text::count(retagged as u64)));
                    }
                    if lists > 0 {
                        parts.push(tr!("playlist neu").to_string());
                    }
                    if dropped > 0 {
                        parts.push(tr!("{} alte playlists entfernt", text::count(dropped as u64)));
                    }
                    let said = if parts.is_empty() {
                        tr!("überspielt").to_string()
                    } else {
                        parts.join(", ")
                    };
                    ended = Some(Ok(said));
                }
                Moved::Along(SyncProgress::BackedUp(_)) => {}
                Moved::Ended(Ok(())) => {
                    ended.get_or_insert_with(|| Ok(tr!("überspielt").to_string()));
                }
                Moved::Ended(Err(why)) => ended = Some(Err(why)),
            }
        }

        let Some(outcome) = ended else {
            return;
        };
        let said = match &outcome {
            Ok(said) => said.clone(),
            Err(why) => tr!("überspielen fehlgeschlagen: {}", why),
        };
        transfer.stage = Stage::Done(outcome);
        self.note(log::Kind::Ipod, said);
        self.reread_device();
    }

    pub(in crate::gui) fn reread_device(&mut self) {
        let Some(mount) = self.attached.as_ref().map(|a| a.mount.clone()) else {
            return;
        };
        let Ok(device) = crate::ipod::IpodDevice::at(&mount) else {
            return;
        };
        let Ok(db) = device.read_database() else {
            return;
        };
        let info = crate::ipod::device_info::DeviceInfo::read(&mount);
        self.attached = Some(attached::Attached::read(
            device.name.clone(),
            mount,
            &db,
            device.capacity(),
            &info,
        ));
        if self.device_open {
            self.refresh_device_summary();
        }
        let _ = self.reload_all();
    }

    pub fn ask_to_remove(&mut self, tracks: Vec<Track>, what: String) {
        let Some(device) = &self.attached else { return };
        if let Some(why) = &device.refusal {
            self.set_status(why.clone());
            return;
        }
        if tracks.is_empty() {
            return;
        }
        let bytes = tracks.iter().map(|t| t.file_size).sum();
        let ids = tracks.iter().map(|t| t.id as u32).collect();
        self.removal = Some(device::Removal { ids, what, bytes, going: None });
        self.show_device_panel();
    }

    pub fn run_removal(&mut self) {
        let Some(mount) = self.attached.as_ref().map(|a| a.mount.clone()) else { return };
        let Some(removal) = self.removal.as_mut() else { return };
        if removal.going.is_some() {
            return;
        }
        let ids = removal.ids.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = crate::ipod::IpodDevice::at(&mount).and_then(|device| {
                let info = crate::ipod::device_info::DeviceInfo::read(&mount);
                crate::ipod::sync::remove(&device, &info, &ids)
            });
            let _ = tx.send(result);
        });
        removal.going = Some(rx);
    }

    pub(in crate::gui) fn poll_removal(&mut self) {
        let Some(rx) = self.removal.as_ref().and_then(|r| r.going.as_ref()) else { return };
        let answer = match rx.try_recv() {
            Ok(answer) => answer,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                Err(crate::error::KesslerError::Ipod(tr!("entfernen abgebrochen").into()))
            }
        };
        self.removal = None;
        match answer {
            Ok(done) => {
                let mut said = tr!(
                    "{} titel vom ipod entfernt · {} frei geworden",
                    text::count(done.tracks as u64),
                    crate::ipod::format_bytes(done.bytes)
                );
                if done.files_left > 0 {
                    said = format!("{said} · {}", tr!("{} dateien blieben liegen", text::count(done.files_left as u64)));
                }
                self.note(log::Kind::Ipod, said.clone());
                self.set_notice(said);
                self.reread_device();
                self.refresh_device_summary();
            }
            Err(e) => self.set_status(format!("{e}")),
        }
    }
}
