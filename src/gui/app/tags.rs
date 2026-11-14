use crate::gui::*;

impl GuiApp {
    pub(in crate::gui) fn rate_tracks(&mut self, ids: &[i64], stars: u8) {
        let stars = stars.min(5);
        for id in ids {
            if let Err(e) = self.db.set_rating(*id, stars) {
                self.set_status(format!("{e}"));
                return;
            }
        }
        for track in self.tracks.iter_mut().chain(self.editing_tracks.iter_mut()) {
            if ids.contains(&track.id) {
                track.rating = stars;
            }
        }
    }

    pub fn open_tag_editor(&mut self) {
        self.row_menu = None;
        self.cover_pick = None;
        let tracks = self.selected_tracks();
        if tracks.is_empty() {
            self.set_status(tr!("kein titel ausgewählt"));
            return;
        }
        let extras = tagedit::extras_of(&tracks);
        self.editor = Some(tagedit::Editor::open(&tracks, &extras));
        self.editor_in = Column::Tracks;
        self.focus = Column::Tracks;
    }

    pub fn cancel_tag_editor(&mut self) {
        self.editor = None;
    }

    pub fn open_details(&mut self) {
        self.row_menu = None;
        self.editor = None;
        self.cover_pick = None;
        let Some(track) = self.tracks.get(self.track_index) else {
            self.set_status(tr!("kein titel ausgewählt"));
            return;
        };
        self.details = Some(Details {
            title: track.display_title().to_string(),
            lines: crate::library::details::read(track),
        });
        self.focus = Column::Tracks;
    }

    pub fn close_details(&mut self) {
        self.details = None;
    }

    pub(in crate::gui) fn album_tracks(&self, index: usize) -> Result<(String, String, Vec<Track>)> {
        let Some(album) = self.albums.get(index) else {
            return Ok((String::new(), String::new(), Vec::new()));
        };
        let tracks = self.db.get_tracks(
            Some(&album.artist),
            Some(&album.name),
            SortField::TrackNumber,
            SortOrder::Ascending,
            None,
        )?;
        Ok((album.artist.clone(), album.name.clone(), tracks))
    }

    pub fn edit_album_tags(&mut self, index: usize) -> Result<()> {
        self.row_menu = None;
        self.cover_pick = None;
        let (_, _, tracks) = self.album_tracks(index)?;
        if tracks.is_empty() {
            self.set_status(tr!("dieses album hat keine titel"));
            return Ok(());
        }
        let tracks: Vec<&Track> = tracks.iter().collect();
        let extras = tagedit::extras_of(&tracks);
        self.editor = Some(tagedit::Editor::open(&tracks, &extras));
        let column = if self.merged() { Column::Tracks } else { Column::Albums };
        self.editor_in = column;
        self.focus = column;
        Ok(())
    }

    pub fn search_cover(&mut self, index: usize) -> Result<()> {
        self.row_menu = None;
        self.editor = None;
        let (artist, album, tracks) = self.album_tracks(index)?;
        if tracks.is_empty() {
            self.set_status(tr!("dieses album hat keine titel"));
            return Ok(());
        }
        let artist_key = crate::db::sort_key(crate::db::grouping_artist(&tracks[0]));
        let album_key = crate::db::sort_key(&tracks[0].album);
        self.cover_pick = Some(coverpick::Picker::start(
            artist,
            album,
            artist_key,
            album_key,
            tracks.iter().map(|t| t.path.clone()).collect(),
            self.config.lastfm.api_key.clone(),
        ));
        Ok(())
    }

    pub fn cover_from_file(&mut self, index: usize) -> Result<()> {
        self.row_menu = None;
        let (_, _, tracks) = self.album_tracks(index)?;
        if tracks.is_empty() {
            self.set_status(tr!("dieses album hat keine titel"));
            return Ok(());
        }
        let (tx, rx) = mpsc::channel();
        let prompt = tr!("Cover wählen");
        std::thread::spawn(move || {
            let _ = tx.send(folder::ask_image(prompt));
        });
        self.picking_cover = Some((rx, index));
        Ok(())
    }

    pub(in crate::gui) fn poll_cover_file(&mut self) {
        let Some((rx, index)) = &self.picking_cover else {
            return;
        };
        let index = *index;
        match rx.try_recv() {
            Ok(Some(path)) => {
                self.picking_cover = None;
                match std::fs::read(&path) {
                    Ok(bytes) => {
                        if let Err(e) = self.put_cover(index, &bytes) {
                            self.set_status(format!("{e}"));
                        }
                    }
                    Err(e) => self.set_status(tr!("{} lässt sich nicht lesen: {}", path.display(), e)),
                }
            }
            Ok(None) => self.picking_cover = None,
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => self.picking_cover = None,
        }
    }

    pub fn start_tag_hunt(&mut self, what: Hunted) -> Result<()> {
        if self.tag_hunt.is_some() {
            return Ok(());
        }
        if self.artist_hunt.is_some() {
            self.set_status(tr!("erst wenn die suche nach künstler:innen fertig ist"));
            return Ok(());
        }
        let wanting: Vec<(String, String)> = match what {
            Hunted::Years => self
                .db
                .get_albums(None, None)?
                .into_iter()
                .filter(|a| a.year.is_none() && !a.name.trim().is_empty())
                .map(|a| (a.artist, a.name))
                .collect(),
            Hunted::Genres => records_without_genre(&self.db.get_tracks(
                None,
                None,
                SortField::Artist,
                SortOrder::Ascending,
                None,
            )?),
        };
        if wanting.is_empty() {
            self.set_status(match what {
                Hunted::Years => tr!("jedes album hat schon ein jahr"),
                Hunted::Genres => tr!("jedes album hat schon ein genre"),
            });
            return Ok(());
        }
        let total = wanting.len();
        let (tx, rx) = mpsc::channel();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stopped = std::sync::Arc::clone(&stop);
        std::thread::spawn(move || {
            let mut failures = 0;
            for (artist, album) in wanting {
                if stopped.load(std::sync::atomic::Ordering::Relaxed) {
                    return;
                }
                let answer = match what {
                    Hunted::Years => crate::library::years::look_up(&artist, &album)
                        .map(|y| y.map(Found::Year)),
                    Hunted::Genres => crate::library::genres::look_up(&artist, &album)
                        .map(|g| g.map(Found::Genre)),
                };
                let news = match answer {
                    Ok(Some(found)) => {
                        failures = 0;
                        HuntNews::Found {
                            artist,
                            album,
                            found,
                        }
                    }
                    Ok(None) => {
                        failures = 0;
                        HuntNews::Asked
                    }
                    Err(e) => {
                        failures += 1;
                        if failures >= 3 {
                            let _ = tx.send(HuntNews::Failed(e.to_string()));
                            return;
                        }
                        HuntNews::Asked
                    }
                };
                if tx.send(news).is_err() {
                    return;
                }
                std::thread::sleep(crate::library::years::PACE);
            }
        });
        self.tag_hunt = Some(TagHunt {
            what,
            news: rx,
            stop,
            total,
            asked: 0,
            found: 0,
            unshown: 0,
        });
        self.set_status(tr!("{}: {} alben ohne, frage musicbrainz",
            crate::lang::t(what.word()),
            text::count(total as u64)
        ));
        Ok(())
    }

    pub fn stop_tag_hunt(&mut self) {
        if let Some(hunt) = self.tag_hunt.take() {
            hunt.stop.store(true, std::sync::atomic::Ordering::Relaxed);
            let _ = self.reload_all();
            self.set_status(tr!("{}: angehalten — {} von {} gefunden",
                crate::lang::t(hunt.what.word()),
                text::count(hunt.found as u64),
                text::count(hunt.asked as u64)
            ));
        }
    }

    pub(in crate::gui) fn poll_tag_hunt(&mut self) {
        let Some(hunt) = self.tag_hunt.as_mut() else {
            return;
        };
        let what = hunt.what;
        let mut arrived = Vec::new();
        let mut ended: Option<Option<String>> = None;
        let before = hunt.asked;
        loop {
            match hunt.news.try_recv() {
                Ok(HuntNews::Asked) => hunt.asked += 1,
                Ok(HuntNews::Found {
                    artist,
                    album,
                    found,
                }) => {
                    hunt.asked += 1;
                    arrived.push((artist, album, found));
                }
                Ok(HuntNews::Failed(why)) => {
                    ended = Some(Some(why));
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    ended = Some(None);
                    break;
                }
            }
        }
        let (asked, total) = (hunt.asked, hunt.total);
        let moved = asked != before;

        for (artist, album, found) in arrived {
            match self.write_found(&artist, &album, &found) {
                Ok(written) if written > 0 => {
                    let (kind, value) = match &found {
                        Found::Year(year) => (log::Kind::Year, year.to_string()),
                        Found::Genre(genre) => (log::Kind::Genre, genre.clone()),
                    };
                    self.note(
                        kind,
                        tr!("{} · {} → {} ({} titel)", artist, album, value, text::count(written as u64)),
                    );
                    if let Some(hunt) = self.tag_hunt.as_mut() {
                        hunt.found += 1;
                        hunt.unshown += written;
                    }
                }
                Ok(_) => {}
                Err(e) => self.set_status(tr!("{} für {}: {}", crate::lang::t(what.word()), album, e)),
            }
        }

        let Some(hunt) = self.tag_hunt.as_mut() else {
            return;
        };
        let found = hunt.found;
        if hunt.unshown >= 40 || (ended.is_some() && hunt.unshown > 0) {
            hunt.unshown = 0;
            let _ = self.reload_all();
        }
        let word = crate::lang::t(what.word());
        match ended {
            Some(Some(why)) => {
                self.tag_hunt = None;
                self.set_status(tr!("{}: musicbrainz antwortet nicht — {}", word, why));
            }
            Some(None) => {
                self.tag_hunt = None;
                self.set_status(tr!("{}: {} von {} alben gefunden und geschrieben",
                    word,
                    text::count(found as u64),
                    text::count(total as u64)
                ));
            }
            None if moved => self.set_status(tr!("{}: {} / {}, {} gefunden",
                word,
                text::count(asked as u64),
                text::count(total as u64),
                text::count(found as u64)
            )),
            None => {}
        }
    }

    pub(in crate::gui) fn write_found(&mut self, artist: &str, album: &str, found: &Found) -> Result<usize> {
        let tracks = self.db.get_tracks(
            Some(artist),
            Some(album),
            SortField::TrackNumber,
            SortOrder::Ascending,
            None,
        )?;
        let mut edit = crate::library::tags::Edit::new();
        match found {
            Found::Year(year) => edit.set(crate::library::tags::Field::Year, year.to_string()),
            Found::Genre(genre) => edit.set(crate::library::tags::Field::Genre, genre.clone()),
        }
        let lacking = |t: &&Track| match found {
            Found::Year(_) => t.year.is_none(),
            Found::Genre(_) => t.genre.trim().is_empty(),
        };
        let artwork = self.config.artwork_dir();
        let mut written = 0;
        let mut refreshed = Vec::new();
        for track in tracks.iter().filter(lacking) {
            crate::library::tags::write(&track.path, &edit)?;
            crate::library::scanner::ingest_file(&track.path, &self.db, Some(artwork.as_path()))?;
            refreshed.push((track.title.clone(), track.path.clone()));
            written += 1;
        }
        self.refresh_in_music(&refreshed);
        Ok(written)
    }

    pub(in crate::gui) fn refresh_in_music(&mut self, files: &[(String, std::path::PathBuf)]) {
        if files.is_empty() || !crate::library::import::music_app_available() {
            return;
        }
        let told: Vec<(String, Option<std::path::PathBuf>)> =
            crate::library::import::music_tracks(files, self.config.add_to_music)
                .into_iter()
                .flatten()
                .map(|id| (id, None))
                .collect();
        crate::library::import::tell_music(&told);
    }

    pub(in crate::gui) fn poll_cover_pick(&mut self) {
        let Some(picker) = self.cover_pick.as_mut() else {
            return;
        };
        let Some(bytes) = picker.poll() else {
            return;
        };
        let picker = self.cover_pick.take().expect("just borrowed");
        let written = self.write_cover(
            &picker.artist_key,
            &picker.album_key,
            &picker.paths,
            &bytes,
        );
        if let Err(e) = written {
            self.set_status(format!("{e}"));
        }
    }

    pub(in crate::gui) fn put_cover(&mut self, index: usize, bytes: &[u8]) -> Result<()> {
        let (_, _, tracks) = self.album_tracks(index)?;
        if tracks.is_empty() {
            return Ok(());
        }
        let artist_key = crate::db::sort_key(crate::db::grouping_artist(&tracks[0]));
        let album_key = crate::db::sort_key(&tracks[0].album);
        let paths: Vec<_> = tracks.iter().map(|t| t.path.clone()).collect();
        self.write_cover(&artist_key, &album_key, &paths, bytes)
    }

    pub(in crate::gui) fn write_cover(
        &mut self,
        artist_key: &str,
        album_key: &str,
        paths: &[std::path::PathBuf],
        bytes: &[u8],
    ) -> Result<()> {
        let jpeg = crate::library::tags::picture_for_tag(bytes)?;

        let mut written = 0u32;
        let mut trouble = None;
        for path in paths {
            if let Err(e) = crate::library::tags::write_picture(path, &jpeg) {
                trouble = Some(e.to_string());
                break;
            }
            written += 1;
        }

        let artwork = self.config.artwork_dir();
        let _ = crate::library::artwork::adopt(&artwork, artist_key, album_key, &jpeg);

        for path in paths.iter().take(written as usize) {
            let _ = crate::library::scanner::ingest_file(path, &self.db, Some(artwork.as_path()));
        }
        let refreshed: Vec<(String, std::path::PathBuf)> = paths
            .iter()
            .take(written as usize)
            .filter_map(|p| self.db.get_track_by_path(&p.to_string_lossy()).ok().flatten())
            .map(|t| (t.title, t.path))
            .collect();
        self.refresh_in_music(&refreshed);

        self.cover_cache.clear();
        self.reload_all()?;

        if written > 0 {
            self.note(
                log::Kind::Cover,
                tr!("{} · {} ({} titel)", artist_key, album_key, text::count(written as u64)),
            );
        }
        match trouble {
            Some(e) => self.set_status(tr!("cover nur teilweise gesetzt: {}", e)),
            None => self.set_status(tr!("cover gesetzt, {} titel",
                text::count(written as u64)
            )),
        }
        Ok(())
    }

    pub fn save_tags(&mut self) -> Result<()> {
        let Some(editor) = self.editor.take() else {
            return Ok(());
        };
        let edit = editor.edit();
        if edit.is_empty() {
            self.set_status(tr!("nichts geändert"));
            return Ok(());
        }
        if let Err(e) = crate::library::tags::check(&edit) {
            self.set_status(format!("{e}"));
            self.editor = Some(editor);
            return Ok(());
        }
        self.device_summary = None;

        let artwork = self.config.artwork_dir();
        let roots: Vec<std::path::PathBuf> =
            self.config.music_dirs.iter().map(|d| crate::library::one_name_for(d)).collect();
        let mut written = 0u32;
        let mut moved = 0u32;
        let mut trouble: Option<String> = None;

        let mut plan: Vec<(i64, crate::db::models::Track, Option<std::path::PathBuf>)> = Vec::new();
        for id in &editor.tracks {
            let Some(track) = self.db.get_track(*id)? else {
                continue;
            };
            if let Err(e) = crate::library::tags::write(&track.path, &edit) {
                trouble = Some(e.to_string());
                break;
            }
            let target = crate::library::metadata::read_metadata(&track.path)
                .ok()
                .and_then(|after| crate::library::import::refiled(&roots, &track, &after));
            plan.push((*id, track, target));
        }

        let asked: Vec<(String, std::path::PathBuf)> = plan
            .iter()
            .map(|(_, track, _)| (track.title.clone(), track.path.clone()))
            .collect();
        let in_music = if asked.is_empty() {
            Vec::new()
        } else {
            crate::library::import::music_tracks(&asked, self.config.add_to_music)
        };
        let mut known = in_music.into_iter();
        let mut told: Vec<(String, Option<std::path::PathBuf>)> = Vec::new();

        for (id, track, target) in plan {
            let id = &id;
            let mut path = track.path.clone();
            let music_id = known.next().flatten();
            let mut now_at = None;
            if let Some(target) = target {
                match crate::library::import::move_filed(&path, &target) {
                    Ok(landed) => {
                        path = crate::library::one_name_for(&landed);
                        self.db.move_track_path(*id, &path)?;
                        self.player.retarget(*id, &path);
                        now_at = Some(path.clone());
                        moved += 1;
                    }
                    Err(e) => trouble = Some(tr!("nicht verschoben: {}", e)),
                }
            }
            if let Some(music_id) = music_id {
                told.push((music_id, now_at));
            }
            if let Err(e) = crate::library::scanner::ingest_file(
                &path,
                &self.db,
                Some(artwork.as_path()),
            ) {
                trouble = Some(e.to_string());
                break;
            }
            written += 1;
        }

        let followed = crate::library::import::tell_music(&told);
        if followed > 0 {
            self.note(log::Kind::Tag, tr!("music.app hat {} titel neu gelesen", text::count(followed as u64)));
        }

        self.cover_cache.clear();
        self.reload_all()?;

        if written > 0 {
            let which = match (written, editor.tracks.first()) {
                (1, Some(id)) => self
                    .db
                    .get_track(*id)?
                    .map(|t| format!("{} — {}", t.shown_artist(), t.display_title()))
                    .unwrap_or_else(|| tr!("1 titel").to_string()),
                (n, _) => tr!("{} titel", text::count(n as u64)),
            };
            self.note(log::Kind::Tag, format!("{which}: {}", edit.summary()));
        }
        match moved {
            0 => {}
            1 => self.note(log::Kind::Tag, tr!("1 datei in einen neuen ordner verschoben").to_string()),
            n => self.note(log::Kind::Tag, tr!("{} dateien in neue ordner verschoben", text::count(n as u64))),
        }

        match trouble {
            Some(e) if written == 0 => self.set_status(tr!("nicht geschrieben: {}", e)),
            Some(e) => self.set_status(tr!("{} von {} geändert, dann: {}",
                text::count(written as u64),
                text::count(editor.tracks.len() as u64),
                e
            )),
            None if written == 1 => self.set_status(tr!("1 titel geändert")),
            None => self.set_status(tr!("{} titel geändert",
                text::count(written as u64)
            )),
        }
        Ok(())
    }

    pub fn look_for_covers_again(&mut self) {
        let dir = self.config.artwork_dir();
        match crate::library::artwork::forget_misses(&dir) {
            Ok(0) => self.set_status(tr!("es gab nichts zu vergessen")),
            Ok(count) => {
                self.cover_cache.clear();
                self.set_status(tr!("{} alben werden neu nach covern durchsucht",
                    text::count(count as u64)
                ));
                self.start_rescan();
            }
            Err(e) => self.set_status(format!("{e}")),
        }
    }
}
