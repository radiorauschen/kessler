use crate::gui::*;

impl GuiApp {
    pub fn start_rescan(&mut self) {
        if self.scan.is_some() {
            self.set_status(tr!("es läuft schon ein durchlauf"));
            return;
        }
        let dirs = self.config.music_dirs.clone();
        if dirs.is_empty() {
            self.set_status(tr!("keine musikordner eingetragen"));
            return;
        }

        self.foreign.clear();
        self.scan_updates.clear();

        let db_path = self.config.db_path();
        let artwork_dir = self.config.artwork_dir();
        let (tx, rx) = mpsc::channel();
        let (progress_tx, progress_rx) = mpsc::channel();

        std::thread::spawn(move || {
            let failed = |e: crate::error::KesslerError| ScanOutcome {
                total: 0,
                added: 0,
                updated: 0,
                removed: 0,
                error: Some(e.to_string()),
            };
            let outcome = match Database::open(&db_path) {
                Ok(db) => {
                    let manager = LibraryManager::new(&db).with_artwork(&artwork_dir);
                    match manager.scan_dirs(&dirs, Some(&progress_tx)) {
                        Ok((total, added, updated)) => match manager.clean_missing_in(&dirs) {
                            Ok(removed) => ScanOutcome {
                                total,
                                added,
                                updated,
                                removed,
                                error: None,
                            },
                            Err(e) => failed(e),
                        },
                        Err(e) => failed(e),
                    }
                }
                Err(e) => failed(e),
            };
            let _ = tx.send(outcome);
        });

        self.scan = Some(rx);
        self.scan_progress = Some(progress_rx);
        self.scan_done = 0;
        self.scan_total = 0;
        self.set_status(tr!("lese die bibliothek neu ein…"));
    }

    pub fn is_scanning(&self) -> bool {
        self.scan.is_some()
    }

    pub fn scan_fraction(&self) -> Option<f32> {
        (self.is_scanning() && self.scan_total > 0).then(|| {
            (self.scan_done as f32 / self.scan_total as f32).clamp(0.0, 1.0)
        })
    }

    pub fn scan_counts(&self) -> (u32, u32) {
        (self.scan_done, self.scan_total)
    }

    pub(in crate::gui) fn poll_scan_progress(&mut self) {
        let Some(rx) = &self.scan_progress else { return };
        while let Ok(progress) = rx.try_recv() {
            use crate::library::scanner::ScanProgress;
            match progress {
                ScanProgress::Counted { total } => self.scan_total = total,
                ScanProgress::FileScanned { count, .. } => self.scan_done = count,
                ScanProgress::Foreign { counted } => {
                    crate::library::scanner::merge_foreign(&mut self.foreign, counted)
                }
                ScanProgress::Updated(update) => self.scan_updates.push(update),
                _ => {}
            }
        }
    }

    pub(in crate::gui) fn poll_rescan(&mut self) {
        self.poll_scan_progress();
        let Some(rx) = &self.scan else { return };
        match rx.try_recv() {
            Ok(outcome) => {
                self.scan = None;
                self.scan_progress = None;
                match outcome.error {
                    Some(e) => self.set_status(tr!("einlesen fehlgeschlagen: {}", e)),
                    None => {
                        self.cover_cache.clear();
                        self.missing.clear();
                        let _ = self.reload_all();
                        let mut said = tr!("{} titel gesehen, {} neu, {} aktualisiert",
                            outcome.total, outcome.added, outcome.updated
                        );
                        if outcome.removed > 0 {
                            said.push_str(&tr!(", {} entfernt", outcome.removed));
                        }
                        let updates = std::mem::take(&mut self.scan_updates);
                        let shown = updates.len().min(UPDATES_LOGGED);
                        for update in &updates[..shown] {
                            self.note(log::Kind::Library, update_line(update));
                        }
                        if updates.len() > shown {
                            self.note(
                                log::Kind::Library,
                                tr!("… und {} weitere aktualisiert", updates.len() - shown),
                            );
                        }
                        if outcome.added > 0 || outcome.updated > 0 || outcome.removed > 0 {
                            self.note(log::Kind::Library, tr!("eingelesen: {}", said));
                        }
                        if let Some(passed) = foreign_note(&self.foreign) {
                            said.push_str(" — ");
                            said.push_str(&passed);
                            self.set_notice(said);
                        } else {
                            self.set_status(said);
                        }
                    }
                }
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.scan = None;
                self.scan_progress = None;
                self.set_status(tr!("einlesen abgebrochen"));
            }
        }
    }

    pub fn ask_for_folder(&mut self) {
        if self.picking.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let prompt = tr!("musikordner wählen");
        std::thread::spawn(move || {
            let _ = tx.send(folder::ask(prompt));
        });
        self.picking = Some(rx);
        self.set_status(tr!("ordner wählen…"));
    }

    pub(in crate::gui) fn poll_picker(&mut self) {
        let Some(rx) = &self.picking else { return };
        match rx.try_recv() {
            Ok(chosen) => {
                self.picking = None;
                let Some(dir) = chosen else {
                    self.set_status(tr!("kein ordner gewählt"));
                    return;
                };
                if self.config.music_dirs.contains(&dir) {
                    self.set_status(tr!("der ordner ist schon eingetragen"));
                    return;
                }
                self.config.music_dirs.push(dir);
                self.save_config();
                self.start_rescan();
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.picking = None;
                self.set_status(tr!("der ordnerdialog ließ sich nicht öffnen"));
            }
        }
    }

    pub fn is_picking(&self) -> bool {
        self.picking.is_some()
    }

    pub fn forget_folder(&mut self, index: usize) -> Result<()> {
        if index >= self.config.music_dirs.len() {
            return Ok(());
        }
        let dir = self.config.music_dirs.remove(index);
        self.save_config();

        let removed = self.db.remove_tracks_under(&dir)?;
        self.cover_cache.clear();
        self.artist_index = self.home_row();
        self.album_index = 0;
        self.track_index = 0;
        self.reload_all()?;
        let said = tr!("{} entfernt, {} titel weniger",
            dir.display(),
            text::count(removed as u64)
        );
        self.note(log::Kind::Library, said.clone());
        self.set_status(said);
        Ok(())
    }

    pub fn add_music(&mut self, paths: Vec<std::path::PathBuf>) {
        use crate::library::import;

        if self.adding.is_some() {
            self.set_status(tr!("es werden schon titel hinzugefügt"));
            return;
        }
        let Some(root) = self.config.music_dirs.first().cloned() else {
            self.set_status(tr!("kein musikordner eingetragen — erst einen im menü wählen"));
            return;
        };

        let found = import::gather(&paths);
        let passed_over = if found.is_empty() {
            import::passed_over(&paths)
        } else {
            Vec::new()
        };
        if found.is_empty() {
            match foreign_note(&passed_over) {
                Some(note) => self.set_notice(tr!("keine abspielbare musik dabei — {}",
                    note
                )),
                None => self.set_status(tr!("darin war keine musik, die kessler abspielen kann")),
            }
            return;
        }

        let db_path = self.config.db_path();
        let artwork = self.config.artwork_dir();
        let moving = self.config.add_move;
        let to_music = self.config.add_to_music && import::music_app_available();
        let (tx, rx) = mpsc::channel();
        let (progress_tx, progress_rx) = mpsc::channel();

        std::thread::spawn(move || {
            let outcome = match Database::open(&db_path) {
                Ok(db) => import::take(
                    &found,
                    &root,
                    &db,
                    Some(artwork.as_path()),
                    moving,
                    to_music,
                    Some(&progress_tx),
                ),
                Err(e) => import::Outcome {
                    failed: vec![(root.clone(), e.to_string())],
                    ..Default::default()
                },
            };
            let _ = tx.send(outcome);
        });

        self.adding = Some(rx);
        self.add_progress = Some(progress_rx);
        self.add_done = 0;
        self.add_total = 0;
        self.set_status(tr!("titel werden hinzugefügt…"));
    }

    pub fn is_adding(&self) -> bool {
        self.adding.is_some()
    }

    pub fn add_counts(&self) -> (u32, u32) {
        (self.add_done, self.add_total)
    }

    pub(in crate::gui) fn poll_adding(&mut self) {
        if let Some(rx) = &self.add_progress {
            while let Ok(progress) = rx.try_recv() {
                self.add_done = progress.done;
                self.add_total = progress.total;
            }
        }
        let Some(rx) = &self.adding else { return };
        match rx.try_recv() {
            Ok(outcome) => {
                self.adding = None;
                self.add_progress = None;
                self.cover_cache.clear();
                let _ = self.reload_all();

                let mut said = match outcome.added {
                    0 => String::new(),
                    1 => tr!("1 titel hinzugefügt").to_string(),
                    count => tr!("{} titel hinzugefügt", text::count(count as u64)),
                };
                if outcome.already > 0 {
                    if !said.is_empty() {
                        said.push_str(", ");
                    }
                    said.push_str(&tr!("{} waren schon da",
                        text::count(outcome.already as u64)
                    ));
                }
                if outcome.added > 0
                    && self.config.add_to_music
                    && crate::library::import::music_app_available()
                {
                    said.push_str(if outcome.handed == outcome.added {
                        tr!(", auch an music.app")
                    } else if outcome.handed == 0 {
                        tr!(", music.app hat nichts genommen")
                    } else {
                        tr!(", music.app nur zum teil")
                    });
                }
                if let Some((path, why)) = outcome.failed.first() {
                    if !said.is_empty() {
                        said.push_str(", ");
                    }
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    said.push_str(&tr!("{} fehlgeschlagen — {}: {}",
                        text::count(outcome.failed.len() as u64),
                        name,
                        why
                    ));
                }
                if said.is_empty() {
                    said.push_str(tr!("nichts hinzugefügt"));
                }
                self.note(log::Kind::Library, said.clone());
                self.set_status(said);
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.adding = None;
                self.add_progress = None;
                self.set_status(tr!("hinzufügen abgebrochen"));
            }
        }
    }

    pub(in crate::gui) fn take_dropped_files(&mut self, ctx: &egui::Context) {
        let (hovering, dropped) = ctx.input(|i| {
            (
                i.raw.hovered_files.len(),
                i.raw
                    .dropped_files
                    .iter()
                    .map(|file| file.path().to_path_buf())
                    .filter(|path| path.is_absolute())
                    .collect::<Vec<_>>(),
            )
        });
        self.hovering = hovering;
        if !dropped.is_empty() {
            self.take_dropped(dropped);
        }
    }

    pub(in crate::gui) fn take_dropped(&mut self, paths: Vec<std::path::PathBuf>) {
        const PICTURES: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp", "tif", "tiff"];
        let picture = paths.len() == 1
            && paths[0]
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| PICTURES.contains(&e.to_lowercase().as_str()))
                .unwrap_or(false);

        if picture && self.album_index > 0 {
            let index = self.album_index - 1;
            match std::fs::read(&paths[0]) {
                Ok(bytes) => {
                    if let Err(e) = self.put_cover(index, &bytes) {
                        self.set_status(format!("{e}"));
                    }
                }
                Err(e) => self.set_status(format!("{e}")),
            }
            return;
        }
        self.add_music(paths);
    }

    pub fn ask_for_music(&mut self) {
        if self.is_picking() || self.adding.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(folder::ask_audio());
        });
        self.adding_pick = Some(rx);
    }

    pub(in crate::gui) fn poll_music_picker(&mut self) {
        let Some(rx) = &self.adding_pick else { return };
        match rx.try_recv() {
            Ok(paths) => {
                self.adding_pick = None;
                if !paths.is_empty() {
                    self.add_music(paths);
                }
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => self.adding_pick = None,
        }
    }
}

const UPDATES_LOGGED: usize = 200;

fn update_line(update: &crate::library::scanner::Update) -> String {
    use crate::library::scanner::{Field, Reread};
    let who = format!("{} – {}", update.artist, update.title);
    if update.changes.is_empty() {
        let why = match update.why {
            Reread::Changed => tr!("datei neuer, daten gleich"),
            Reread::Cover => tr!("cover noch nicht gesucht"),
            Reread::Nameless => tr!("ohne künstler:in und album, wird jedes mal gelesen"),
        };
        return tr!("neu gelesen: {} ({})", who, why);
    }
    let said: Vec<String> = update
        .changes
        .iter()
        .map(|change| {
            let field = match change.field {
                Field::Title => tr!("titel|name"),
                Field::Artist => tr!("künstler:in"),
                Field::AlbumArtist => tr!("albumkünstler:in"),
                Field::Album => tr!("album"),
                Field::Genre => tr!("genre"),
                Field::Year => tr!("jahr"),
                Field::Track => tr!("nr"),
                Field::Disc => tr!("cd"),
                Field::Length => tr!("länge"),
                Field::Format => tr!("format"),
                Field::Bitrate => tr!("bitrate"),
                Field::SampleRate => tr!("abtastrate"),
                Field::Size => tr!("grösse"),
                Field::Cover => tr!("cover"),
            };
            let value = |v: &str| -> String {
                match change.field {
                    _ if v.is_empty() => tr!("leer").to_string(),
                    Field::Size => v.parse().map(crate::ipod::format_bytes).unwrap_or_else(|_| v.to_string()),
                    Field::Cover if v == "true" => tr!("mit").to_string(),
                    Field::Cover => tr!("ohne").to_string(),
                    _ => v.to_string(),
                }
            };
            format!("{field} {} → {}", value(&change.was), value(&change.now))
        })
        .collect();
    tr!("aktualisiert: {}: {}", who, said.join(", "))
}
