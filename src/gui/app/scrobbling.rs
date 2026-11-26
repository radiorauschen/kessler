//! last.fm: connecting an account and reporting what is heard.

use crate::gui::*;

impl GuiApp {
    /// Whether an account is in the middle of being connected.
    pub fn is_connecting(&self) -> bool {
        self.connecting.is_some()
    }

    /// Start connecting a Last.fm account: open the approval page and wait.
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

    /// Forget the connected account: the session goes from the config and the
    /// scrobbler stops. The key and the secret stay — they are the program's,
    /// not the account's — so connecting again is one click. What was waiting
    /// to be sent stays on disk and goes out with the next account connected.
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

    /// Connect a different account in its place. Last.fm asks the browser
    /// whoever is logged in there, so the page has to be told the other
    /// account — the status line says so.
    pub fn switch_lastfm(&mut self) {
        self.disconnect_lastfm();
        self.connect_lastfm();
        if self.connecting.is_some() {
            self.set_status(tr!("last.fm: im browser mit dem anderen konto anmelden und bestätigen…"));
        }
    }

    /// The connected account's page on last.fm, in the browser.
    pub fn open_lastfm_profile(&mut self) {
        match self.config.lastfm.username.as_deref().filter(|u| !u.is_empty()) {
            Some(user) => {
                let url = format!("https://www.last.fm/user/{}", crate::lastfm::api::encode(user));
                crate::lastfm::open_in_browser(&url);
            }
            None => self.set_status(tr!("last.fm: kein konto verbunden")),
        }
    }

    /// Pick up the answer, if it has come.
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

    /// Tell Last.fm what is being listened to.
    ///
    /// Every frame, because that is the only thing in this program that knows
    /// time is passing — but everything expensive happens on the scrobbler's
    /// own thread, and the watcher itself is arithmetic.
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
