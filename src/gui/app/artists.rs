use crate::gui::*;

impl GuiApp {
    pub fn toggle_artist(&mut self) {
        if self.artist_open {
            self.artist_open = false;
            self.artist_form = None;
            self.gaps.came_from = false;
            self.follow_the_playing();
            return;
        }
        if !self.all_sides() {
            if self.device_shown() {
                self.close_device_panel();
            }
            self.log_open = false;
            self.lyrics_open = false;
            self.gaps.open = false;
            self.eq_open = false;
        }
        self.artist_open = true;
        self.leave_the_left();
    }

    pub fn about_artist(&mut self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        if !self.artist_open {
            self.toggle_artist();
        }
        self.artist_pin = Some((name.to_string(), self.artist_anchor()));
    }

    pub(in crate::gui) fn artist_anchor(&self) -> ArtistAnchor {
        (
            self.selected_artist(),
            self.track_index,
            self.player.current_track().map(|t| t.id),
        )
    }

    pub fn shown_artist(&self) -> Option<String> {
        if self.showing_ipod() {
            return None;
        }
        if let Some((name, anchor)) = &self.artist_pin {
            if *anchor == self.artist_anchor() {
                return Some(name.clone());
            }
        }
        if self.showing_radio() {
            return self.aired_artist();
        }
        let of = |t: &Track| {
            let name = if t.artist.is_empty() { &t.album_artist } else { &t.artist };
            (!name.is_empty()).then(|| name.clone())
        };
        if self.focus == Column::Queue {
            let row = if self.editing.is_some() {
                self.editing_tracks.get(self.queue_index)
            } else {
                self.player.queued().get(self.queue_index)
            };
            if let Some(name) = row.and_then(of) {
                return Some(name);
            }
        }
        if let Some(name) = self.selected_artist() {
            return Some(name);
        }
        self.tracks
            .get(self.track_index)
            .and_then(of)
            .or_else(|| self.playing_library_track().and_then(of))
            .or_else(|| self.aired_artist())
    }

    pub fn follow_the_artist(&mut self) {
        if self
            .artist_pin
            .as_ref()
            .is_some_and(|(_, anchor)| *anchor != self.artist_anchor())
        {
            self.artist_pin = None;
        }
        if self.artist_pin.is_none() {
            self.artist_trail.clear();
            self.gaps.came_from = false;
        }
        let Some(name) = self.shown_artist() else {
            self.artist.name = None;
            return;
        };
        let key = crate::library::artistinfo::key(&crate::db::sort_key(&name));
        if self.artist.name.is_some() && self.artist.key == key {
            return;
        }
        let dir = self.config.artists_dir();
        self.artist.name = Some(name.clone());
        self.artist.key = key;
        self.artist.info = crate::library::artistinfo::load(&dir, key);
        self.artist.mine = self
            .db
            .get_tracks(Some(&name), None, SortField::Artist, SortOrder::Ascending, None)
            .ok()
            .and_then(|tracks| artist::Mine::of(&tracks));
        let never = self.artist.info.as_ref().is_none_or(|i| i.looked_up.is_empty());
        if never && !self.artist.asked.contains(&key) && self.artist.looking.is_none() {
            self.look_up_artist();
        }
    }

    pub fn look_up_artist(&mut self) {
        let Some(name) = self.artist.name.clone() else {
            return;
        };
        if self.artist.looking.is_some() {
            return;
        }
        let key = self.artist.key;
        self.artist.asked.insert(key);
        self.artist.trouble = None;
        let rx = artist::start_looking(
            name.clone(),
            crate::lang::current().key(),
            self.config.lastfm.api_key.clone(),
        );
        self.artist.looking = Some((name, key, rx));
    }

    pub(in crate::gui) fn poll_artist(&mut self) {
        use crate::library::artistinfo;
        let dir = self.config.artists_dir();
        let lang = crate::lang::current().key();
        let stale = self.artist.info.as_ref().is_some_and(|i| !i.looked_up.is_empty() && i.found.lacks(lang));
        if stale && self.artist.looking.is_none() && !self.artist.asked.contains(&self.artist.key) {
            self.look_up_artist();
        }
        if let Some((name, key, rx)) = &self.artist.looking {
            let (name, key) = (name.clone(), *key);
            match rx.try_recv() {
                Ok(answer) => {
                    self.artist.looking = None;
                    match answer {
                        Ok((found, recognised, picture)) => {
                            let summary = artistinfo::summary(&found, picture.is_some());
                            match artistinfo::take_in(&dir, key, &name, found, recognised, picture) {
                                Ok((info, pictured)) => {
                                    if pictured {
                                        self.artist.stamp += 1;
                                    }
                                    if recognised {
                                        self.note(log::Kind::Artist, format!("{name}: {summary}"));
                                    }
                                    if self.artist.key == key {
                                        self.artist.info = Some(info);
                                    }
                                    self.gaps.stale = true;
                                }
                                Err(e) => self.set_status(format!("{e}")),
                            }
                            if !recognised {
                                self.note(log::Kind::Artist, tr!("nichts gefunden über {}", name));
                            }
                        }
                        Err(e) => {
                            self.artist.trouble = Some((key, e.to_string()));
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => self.artist.looking = None,
            }
        }
        if let Some((key, name, rx)) = &self.artist.picking {
            let (key, name) = (*key, name.clone());
            match rx.try_recv() {
                Ok(Some(path)) => {
                    self.artist.picking = None;
                    let stored = std::fs::read(&path)
                        .map_err(crate::error::KesslerError::from)
                        .and_then(|bytes| artistinfo::store_picture(&dir, key, &bytes));
                    match stored {
                        Ok(true) => {
                            let mut info = artistinfo::load(&dir, key).unwrap_or_default();
                            info.name = name;
                            info.own.picture = true;
                            if let Err(e) = artistinfo::save(&dir, key, &info) {
                                self.set_status(format!("{e}"));
                            }
                            if self.artist.key == key {
                                self.artist.info = Some(info);
                            }
                            self.artist.stamp += 1;
                        }
                        Ok(false) => self.set_status(tr!("das bild liess sich nicht lesen")),
                        Err(e) => self.set_status(format!("{e}")),
                    }
                }
                Ok(None) => self.artist.picking = None,
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => self.artist.picking = None,
            }
        }
    }

    pub fn artist_picture_from_file(&mut self) {
        let Some(name) = self.artist.name.clone() else {
            return;
        };
        let (tx, rx) = mpsc::channel();
        let prompt = tr!("Bild wählen");
        std::thread::spawn(move || {
            let _ = tx.send(folder::ask_image(prompt));
        });
        self.artist.picking = Some((self.artist.key, name, rx));
    }

    pub fn start_artist_hunt(&mut self, again: bool) -> Result<()> {
        if self.artist_hunt.is_some() {
            return Ok(());
        }
        if self.tag_hunt.is_some() {
            self.set_status(tr!("erst wenn die suche nach jahren oder genres fertig ist"));
            return Ok(());
        }
        let dir = self.config.artists_dir();
        use crate::library::artistinfo;
        let key = |name: &str| artistinfo::key(&crate::db::sort_key(name));
        let everyone = self
            .db
            .get_artists(None)?
            .into_iter()
            .map(|a| a.name)
            .filter(|name| !name.trim().is_empty());
        let names: Vec<String> = if again {
            let mut aged: Vec<(String, String)> = everyone
                .map(|name| {
                    let when = artistinfo::load(&dir, key(&name))
                        .map(|i| i.looked_up)
                        .unwrap_or_default();
                    (when, name)
                })
                .collect();
            aged.sort();
            aged.into_iter().map(|(_, name)| name).collect()
        } else {
            everyone.filter(|name| !artistinfo::looked_up(&dir, key(name))).collect()
        };
        if names.is_empty() {
            self.set_status(tr!("über alle künstler:innen wurde schon gesucht"));
            return Ok(());
        }
        let total = names.len();
        self.artist_hunt = Some(artist::Hunt::start(
            names,
            dir,
            crate::lang::current().key(),
            self.config.lastfm.api_key.clone(),
            again,
        ));
        let said = if again {
            tr!("künstler:innen: frage für alle {} neu nach", text::count(total as u64))
        } else {
            tr!("künstler:innen: {} ohne infos, frage nach", text::count(total as u64))
        };
        self.note(log::Kind::Artist, said.clone());
        self.set_status(said);
        Ok(())
    }

    pub fn stop_artist_hunt(&mut self) {
        if let Some(hunt) = self.artist_hunt.take() {
            hunt.stop.store(true, std::sync::atomic::Ordering::Relaxed);
            let said = tr!("künstler:innen: angehalten — {} von {} gefunden",
                text::count(hunt.found as u64),
                text::count(hunt.asked as u64)
            );
            self.note(log::Kind::Artist, said.clone());
            self.set_status(said);
        }
    }

    pub(in crate::gui) fn poll_artist_hunt(&mut self) {
        let Some(hunt) = self.artist_hunt.as_mut() else {
            return;
        };
        let mut ended: Option<Option<String>> = None;
        let mut touched: Vec<u64> = Vec::new();
        loop {
            match hunt.news.try_recv() {
                Ok(artist::HuntNews::Done(key, recognised)) => {
                    hunt.asked += 1;
                    if recognised {
                        hunt.found += 1;
                    }
                    touched.push(key);
                }
                Ok(artist::HuntNews::Failed(why)) => {
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
        let (asked, found, total) = (hunt.asked, hunt.found, hunt.total);
        if !touched.is_empty() {
            self.gaps.stale = true;
        }
        if touched.contains(&self.artist.key) {
            self.artist.info =
                crate::library::artistinfo::load(&self.config.artists_dir(), self.artist.key);
            self.artist.stamp += 1;
        }
        if let Some(why) = ended {
            self.artist_hunt = None;
            let said = match why {
                Some(why) => tr!("künstler:innen: abgebrochen nach {} — {}", text::count(asked as u64), why),
                None => tr!("künstler:innen: fertig — {} von {} gefunden",
                    text::count(found as u64),
                    text::count(total as u64)
                ),
            };
            self.note(log::Kind::Artist, said.clone());
            self.set_notice(said);
        }
    }

    pub fn toggle_gaps(&mut self) {
        if self.gaps.open {
            self.gaps.open = false;
            self.follow_the_playing();
        } else {
            self.show_gaps();
        }
    }

    pub fn show_gaps(&mut self) {
        if !self.all_sides() {
            if self.device_shown() {
                self.close_device_panel();
            }
            self.log_open = false;
            self.artist_open = false;
            self.artist_form = None;
            self.lyrics_open = false;
            self.eq_open = false;
        }
        self.gaps.open = true;
        self.gaps.came_from = false;
        self.gaps.stale = true;
        self.leave_the_left();
    }

    pub fn visit_gap(&mut self, name: &str) {
        self.gaps.open = false;
        self.artist_trail.clear();
        self.about_artist(name);
        self.gaps.came_from = true;
    }

    pub(in crate::gui) fn poll_gaps(&mut self) {
        if let Some(rx) = &self.gaps.counting {
            match rx.try_recv() {
                Ok(gaps) => {
                    self.gaps.counted = Some(gaps);
                    self.gaps.counting = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => self.gaps.counting = None,
            }
        }
        if !(self.gaps.open && self.gaps.stale) || self.gaps.counting.is_some() {
            return;
        }
        self.gaps.stale = false;
        let artists: Vec<String> = match self.db.get_artists(None) {
            Ok(all) => all.into_iter().map(|a| a.name).filter(|n| !n.trim().is_empty()).collect(),
            Err(e) => {
                self.set_status(format!("{e}"));
                return;
            }
        };
        let dir = self.config.artists_dir();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(crate::library::gaps::find(&dir, &artists));
        });
        self.gaps.counting = Some(rx);
    }

    pub fn export_artists(&mut self) {
        let (tx, rx) = mpsc::channel();
        let prompt = tr!("Wohin sollen die Künstler:innen?");
        std::thread::spawn(move || {
            let _ = tx.send(folder::ask(prompt));
        });
        self.exporting_artists = Some(rx);
    }

    pub(in crate::gui) fn poll_artist_export(&mut self) {
        let Some(rx) = &self.exporting_artists else {
            return;
        };
        match rx.try_recv() {
            Ok(Some(folder)) => {
                self.exporting_artists = None;
                let to = folder.join(tr!("kessler künstler:innen"));
                match crate::library::artistinfo::export_all(&self.config.artists_dir(), &to) {
                    Ok(0) => self.set_status(tr!("noch nichts über künstler:innen gesammelt")),
                    Ok(n) => {
                        self.set_status(tr!("{} künstler:innen als markdown in {}",
                            text::count(n as u64),
                            to.display()
                        ));
                        let _ = reveal::show(&to);
                    }
                    Err(e) => self.set_status(format!("{e}")),
                }
            }
            Ok(None) => self.exporting_artists = None,
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => self.exporting_artists = None,
        }
    }

    pub fn edit_artist(&mut self) {
        let Some(name) = self.artist.name.clone() else {
            return;
        };
        let own = self.artist.info.as_ref().map(|i| i.own.clone()).unwrap_or_default();
        self.artist_form = Some(artist::OwnForm::open(self.artist.key, &name, &own));
    }

    pub fn save_artist_form(&mut self) {
        let Some(form) = self.artist_form.take() else {
            return;
        };
        let dir = self.config.artists_dir();
        let mut info = crate::library::artistinfo::load(&dir, form.key).unwrap_or_default();
        info.name = form.name.clone();
        info.own = form.own();
        match crate::library::artistinfo::save(&dir, form.key, &info) {
            Ok(()) => {
                if self.artist.key == form.key {
                    self.artist.info = Some(info);
                }
                self.set_status(tr!("gespeichert"));
            }
            Err(e) => self.set_status(format!("{e}")),
        }
    }
}
