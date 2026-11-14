use crate::gui::*;

impl GuiApp {
    pub fn is_connecting(&self) -> bool {
        self.connecting.is_some()
    }

    pub fn connect_lastfm(&mut self) {
        if self.connecting.is_some() {
            return;
        }
        let (Some(key), Some(secret)) = (
            self.config.lastfm.api_key.clone(),
            self.config.lastfm.api_secret.clone(),
        ) else {
            self.set_status(tr!("erst den schlüssel eintragen: menü → last.fm"));
            return;
        };
        self.connecting = Some(crate::lastfm::connect(key, secret));
        self.set_status(tr!("last.fm: im browser bestätigen…"));
    }

    pub fn disconnect_lastfm(&mut self) {
        let who = self.config.lastfm.username.take();
        self.config.lastfm.session_key = None;
        self.scrobbler = None;
        self.save_config();
        match who {
            Some(who) => {
                self.note(log::Kind::Lastfm, tr!("abgemeldet: {}", who));
                self.set_status(tr!("last.fm: {} abgemeldet", who));
            }
            None => self.set_status(tr!("last.fm: abgemeldet")),
        }
    }

    pub fn switch_lastfm(&mut self) {
        self.disconnect_lastfm();
        self.connect_lastfm();
        if self.connecting.is_some() {
            self.set_status(tr!("last.fm: im browser mit dem anderen konto anmelden und bestätigen…"));
        }
    }

    pub fn open_lastfm_profile(&mut self) {
        match self.config.lastfm.username.as_deref().filter(|u| !u.is_empty()) {
            Some(user) => {
                let url = format!("https://www.last.fm/user/{}", crate::lastfm::api::encode(user));
                crate::lastfm::open_in_browser(&url);
            }
            None => self.set_status(tr!("last.fm: kein konto verbunden")),
        }
    }

    pub(in crate::gui) fn poll_connecting(&mut self) {
        let Some(rx) = &self.connecting else { return };
        match rx.try_recv() {
            Ok(Ok((session, user))) => {
                self.connecting = None;
                self.config.lastfm.session_key = Some(session);
                self.config.lastfm.username = Some(user.clone());
                self.save_config();
                self.scrobbler = crate::lastfm::Scrobbler::start(
                    &self.config.lastfm,
                    self.config.scrobble_queue_path(),
                );
                self.note(log::Kind::Lastfm, tr!("verbunden als {}", user));
                self.set_status(tr!("last.fm: verbunden als {}", user));
            }
            Ok(Err(e)) => {
                self.connecting = None;
                self.set_status(format!("{e}"));
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.connecting = None;
                self.set_status(tr!("last.fm: verbinden abgebrochen"));
            }
        }
    }

    pub(in crate::gui) fn report_listening(&mut self) {
        if self.scrobbler.is_none() {
            return;
        }
        use crate::lastfm::watch::Moment;
        let playing = self.player.state() == crate::player::PlaybackState::Playing;
        let moment = Moment {
            track_id: if playing {
                self.playing_library_track().map(|t| t.id)
            } else {
                None
            },
            position: self.player.position().as_secs_f64(),
            now: crate::lastfm::now(),
        };
        let events = self
            .scrobble_watch
            .observe(moment, self.player.library_track());

        let Some(scrobbler) = &self.scrobbler else {
            return;
        };
        for event in events {
            scrobbler.submit(event);
        }

        let reports = scrobbler.poll();
        for report in reports {
            use crate::lastfm::Report;
            match report {
                Report::Scrobbled(title) => {
                    self.note(log::Kind::Scrobble, title.clone());
                    self.set_notice(format!("last.fm: {title}"));
                }
                Report::Loved(title) => {
                    self.note(log::Kind::Lastfm, tr!("geliebt: {}", title));
                    self.set_notice(tr!("last.fm: geliebt: {}", title));
                }
                Report::Trouble(why) => {
                    self.note(log::Kind::Lastfm, why.clone());
                    self.set_notice(format!("last.fm: {why}"));
                }
                Report::Disconnected => {
                    self.note(log::Kind::Lastfm, tr!("sitzung abgelaufen"));
                    self.set_notice(tr!("last.fm: sitzung abgelaufen — kessler lastfm login"))
                }
            }
        }
    }
}
