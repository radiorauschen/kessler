//! Last.fm: connecting an account, saying what is playing, and scrobbling.
//!
//! Four pieces, kept apart on purpose:
//!
//! - [`sign`] — the digest and the signature Last.fm asks for.
//! - [`api`] — building a request and reading the reply.
//! - [`watch`] — the rules for what counts as having listened to something.
//!   No network, no player, no window: just the rules, and tests for them.
//! - [`queue`] — scrobbles that could not be sent yet.
//!
//! What ties them together here is [`Scrobbler`], which owns a thread. Nothing
//! about reporting a track is urgent, and nothing about it may ever hold up a
//! frame, so the window hands over what happened and forgets about it.

pub mod api;
pub mod queue;
pub mod sign;
pub mod watch;

use std::path::PathBuf;
use std::sync::mpsc;

use crate::config::LastfmConfig;
use crate::db::models::Track;
use crate::error::{KesslerError, Result};

use api::ApiError;
use queue::{Pending, Queue};

/// Unix time now, in seconds.
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Turn a track into the fields Last.fm files it under.
fn fields(track: &Track) -> Pending {
    Pending {
        artist: track.artist.trim().to_string(),
        title: track.title.trim().to_string(),
        album: track.album.trim().to_string(),
        album_artist: track.album_artist.trim().to_string(),
        track_number: track.track_number,
        duration_secs: track.duration_secs.round().max(0.0) as u64,
        started_at: 0,
    }
}

/// What the account needs before anything can be sent.
#[derive(Debug, Clone)]
pub struct Credentials {
    pub key: String,
    pub secret: String,
    pub session: String,
}

impl Credentials {
    /// The three values, if all three are there.
    ///
    /// The key and secret are the application's and come from the config; the
    /// session is this person's and is written by `kessler lastfm login`.
    pub fn from_config(config: &LastfmConfig) -> Option<Self> {
        Some(Self {
            key: non_empty(config.api_key.as_deref())?,
            secret: non_empty(config.api_secret.as_deref())?,
            session: non_empty(config.session_key.as_deref())?,
        })
    }
}

fn non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

/// Step one of connecting: ask for a token, which the person then approves in
/// a browser.
pub fn request_token(key: &str, secret: &str) -> Result<String> {
    let reply = api::call(
        vec![
            ("method".into(), "auth.getToken".into()),
            ("api_key".into(), key.to_string()),
        ],
        secret,
    )?;
    if let Some(error) = api::error_in(&reply) {
        return Err(KesslerError::Lastfm(tr!("{} (fehler {})",
            error.message, error.code
        )));
    }
    api::tag(&reply, "token")
        .filter(|token| !token.trim().is_empty())
        .ok_or_else(|| KesslerError::Lastfm(tr!("last.fm gab keinen token zurück").into()))
}

/// Where the person approves that token.
pub fn approval_url(key: &str, token: &str) -> String {
    format!("https://www.last.fm/api/auth/?api_key={key}&token={token}")
}

/// Step two: trade the approved token for a session key that does not expire.
pub fn request_session(key: &str, secret: &str, token: &str) -> Result<(String, String)> {
    let reply = api::call(
        vec![
            ("method".into(), "auth.getSession".into()),
            ("api_key".into(), key.to_string()),
            ("token".into(), token.to_string()),
        ],
        secret,
    )?;
    if let Some(error) = api::error_in(&reply) {
        return Err(KesslerError::Lastfm(tr!("{} (fehler {})",
            error.message, error.code
        )));
    }
    let session = api::tag(&reply, "key")
        .ok_or_else(|| KesslerError::Lastfm(tr!("keine sitzung in der antwort").into()))?;
    let user = api::tag(&reply, "name").unwrap_or_default();
    Ok((session, user))
}

/// Open a URL in whatever the system uses for the web.
pub fn open_in_browser(url: &str) -> bool {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };
    std::process::Command::new(opener)
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Connect an account from inside the window, without a terminal.
///
/// The only part of this that needs a person is approving the token in a
/// browser, and the only part that needed a terminal was waiting for them to
/// come back. So it waits by asking: once the token is approved, `getSession`
/// starts answering, and until then it says "unauthorized token" — which is not
/// an error here, it is "not yet". Asking every couple of seconds for a few
/// minutes costs nothing and saves the person a command line.
pub fn connect(key: String, secret: String) -> mpsc::Receiver<Result<(String, String)>> {
    /// How long the person has to approve it, and how often we look.
    const PATIENCE: std::time::Duration = std::time::Duration::from_secs(180);
    const EVERY: std::time::Duration = std::time::Duration::from_secs(2);

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let outcome = (|| {
            let token = request_token(&key, &secret)?;
            let url = approval_url(&key, &token);
            if !open_in_browser(&url) {
                return Err(KesslerError::Lastfm(tr!("browser liess sich nicht öffnen — diese adresse aufrufen: {}",
                    url
                )));
            }

            let until = std::time::Instant::now() + PATIENCE;
            let mut last = KesslerError::Lastfm(tr!("nicht bestätigt").into());
            while std::time::Instant::now() < until {
                std::thread::sleep(EVERY);
                match request_session(&key, &secret, &token) {
                    Ok(session) => return Ok(session),
                    Err(e) => last = e,
                }
            }
            Err(last)
        })();
        let _ = tx.send(outcome);
    });
    rx
}

/// Tell Last.fm what is playing now. Nothing is stored if this fails: by the
/// time it could be retried the track would be over.
fn send_now_playing(credentials: &Credentials, track: &Pending) -> Result<()> {
    let mut params = vec![
        ("method".to_string(), "track.updateNowPlaying".to_string()),
        ("api_key".to_string(), credentials.key.clone()),
        ("sk".to_string(), credentials.session.clone()),
        ("artist".to_string(), track.artist.clone()),
        ("track".to_string(), track.title.clone()),
    ];
    add_optional(&mut params, track);
    let reply = api::call(params, &credentials.secret)?;
    match api::error_in(&reply) {
        Some(error) => Err(reported(error)),
        None => Ok(()),
    }
}

/// Send one scrobble.
fn send_scrobble(credentials: &Credentials, track: &Pending) -> Result<()> {
    let mut params = vec![
        ("method".to_string(), "track.scrobble".to_string()),
        ("api_key".to_string(), credentials.key.clone()),
        ("sk".to_string(), credentials.session.clone()),
        ("artist".to_string(), track.artist.clone()),
        ("track".to_string(), track.title.clone()),
        ("timestamp".to_string(), track.started_at.to_string()),
    ];
    add_optional(&mut params, track);
    let reply = api::call(params, &credentials.secret)?;
    match api::error_in(&reply) {
        Some(error) => Err(reported(error)),
        None => Ok(()),
    }
}

/// Love a track on the account (a song kept from the radio, when „bei last.fm
/// lieben" is on).
fn send_love(credentials: &Credentials, track: &Pending) -> Result<()> {
    let params = vec![
        ("method".to_string(), "track.love".to_string()),
        ("api_key".to_string(), credentials.key.clone()),
        ("sk".to_string(), credentials.session.clone()),
        ("artist".to_string(), track.artist.clone()),
        ("track".to_string(), track.title.clone()),
    ];
    let reply = api::call(params, &credentials.secret)?;
    match api::error_in(&reply) {
        Some(error) => Err(reported(error)),
        None => Ok(()),
    }
}

/// The fields that are worth sending when they are there and must not be sent
/// empty: Last.fm files a track under an empty album rather than under none.
fn add_optional(params: &mut Vec<(String, String)>, track: &Pending) {
    if !track.album.is_empty() {
        params.push(("album".to_string(), track.album.clone()));
    }
    if !track.album_artist.is_empty() && track.album_artist != track.artist {
        params.push(("albumArtist".to_string(), track.album_artist.clone()));
    }
    if let Some(number) = track.track_number {
        params.push(("trackNumber".to_string(), number.to_string()));
    }
    if track.duration_secs > 0 {
        params.push(("duration".to_string(), track.duration_secs.to_string()));
    }
}

fn reported(error: ApiError) -> KesslerError {
    KesslerError::Lastfm(tr!("{} (fehler {})", error.message, error.code))
}

/// What the window hands over.
enum Job {
    NowPlaying(Box<Pending>),
    Scrobble(Box<Pending>),
    Love(Box<Pending>),
}

/// What comes back, for the interface to say.
#[derive(Debug, Clone, PartialEq)]
pub enum Report {
    /// A scrobble got through. The title is for the status line.
    Scrobbled(String),
    /// A track was loved.
    Loved(String),
    /// Something went wrong and is worth saying once.
    Trouble(String),
    /// The session is no longer accepted: the account has to be connected
    /// again, and nothing will work until it is.
    Disconnected,
}

/// Reports listening to Last.fm, on a thread of its own.
pub struct Scrobbler {
    jobs: mpsc::Sender<Job>,
    reports: mpsc::Receiver<Report>,
    /// The same channel the thread reports on, for the things the window works
    /// out for itself without troubling the network.
    voice: mpsc::Sender<Report>,
    user: String,
    waiting: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl Scrobbler {
    /// Start one, if the account is connected. `None` means "not set up", which
    /// is a normal state and not an error.
    pub fn start(config: &LastfmConfig, queue_path: PathBuf) -> Option<Self> {
        let credentials = Credentials::from_config(config)?;
        let user = config.username.clone().unwrap_or_default();
        let now_playing_wanted = config.now_playing;

        let (jobs_tx, jobs_rx) = mpsc::channel::<Job>();
        let (reports_tx, reports_rx) = mpsc::channel::<Report>();
        let reports_tx_here = reports_tx.clone();
        let queue = Queue::at(&queue_path);
        let waiting = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(queue.len()));
        let counter = waiting.clone();

        std::thread::spawn(move || {
            use std::sync::atomic::Ordering;
            for job in jobs_rx {
                match job {
                    Job::NowPlaying(track) => {
                        if now_playing_wanted {
                            if let Err(e) = send_now_playing(&credentials, &track) {
                                let _ = reports_tx.send(trouble(e));
                            }
                        }
                    }
                    Job::Love(track) => match send_love(&credentials, &track) {
                        Ok(()) => {
                            let _ = reports_tx.send(Report::Loved(track.title.clone()));
                        }
                        Err(e) => {
                            let _ = reports_tx.send(trouble(e));
                        }
                    },
                    Job::Scrobble(track) => {
                        let mut pending = queue.read();
                        pending.push(*track);

                        let outcome = drain(pending, |entry| send_scrobble(&credentials, entry));
                        let _ = queue.write(&outcome.left);
                        counter.store(outcome.left.len(), Ordering::Relaxed);
                        if let Some(e) = outcome.trouble {
                            let _ = reports_tx.send(trouble(e));
                        }
                        if let Some(title) = outcome.last_sent {
                            let _ = reports_tx.send(Report::Scrobbled(title));
                        }
                    }
                }
            }
        });

        Some(Self {
            jobs: jobs_tx,
            reports: reports_rx,
            voice: reports_tx_here,
            user,
            waiting,
        })
    }

    /// Hand over what the watcher decided. Never blocks, never fails loudly: a
    /// scrobbler whose thread has gone is a scrobbler that stops scrobbling,
    /// not a program that stops playing music.
    pub fn submit(&self, event: watch::Event) {
        let job = match event {
            watch::Event::NowPlaying(track) => {
                let entry = fields(&track);
                if !entry.filable() {
                    return;
                }
                Job::NowPlaying(Box::new(entry))
            }
            watch::Event::Scrobble { track, started_at } => {
                let mut entry = fields(&track);
                entry.started_at = started_at;
                if !entry.filable() {
                    let _ = self.voice.send(Report::Trouble(entry.why_not_filable()));
                    return;
                }
                Job::Scrobble(Box::new(entry))
            }
        };
        let _ = self.jobs.send(job);
    }

    /// What a station said it played: playing now, or — with when it began —
    /// listened to. There is no file behind it, only the two names the station
    /// gave.
    pub fn submit_aired(&self, artist: &str, title: &str, started_at: Option<i64>) {
        let entry = Pending {
            artist: artist.to_string(),
            title: title.to_string(),
            album: String::new(),
            album_artist: String::new(),
            track_number: None,
            duration_secs: 0,
            started_at: started_at.unwrap_or(0),
        };
        if !entry.filable() {
            return;
        }
        let job = if started_at.is_some() { Job::Scrobble(Box::new(entry)) } else { Job::NowPlaying(Box::new(entry)) };
        let _ = self.jobs.send(job);
    }

    /// Love `artist` – `title` on the account.
    pub fn love(&self, artist: &str, title: &str) {
        let entry = Pending {
            artist: artist.to_string(),
            title: title.to_string(),
            album: String::new(),
            album_artist: String::new(),
            track_number: None,
            duration_secs: 0,
            started_at: 0,
        };
        if entry.filable() {
            let _ = self.jobs.send(Job::Love(Box::new(entry)));
        }
    }

    /// Whatever the thread has said since last time.
    pub fn poll(&self) -> Vec<Report> {
        self.reports.try_iter().collect()
    }

    pub fn user(&self) -> &str {
        &self.user
    }

    /// How many scrobbles are still waiting to get through.
    pub fn waiting(&self) -> usize {
        self.waiting.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// What came of trying to send a queue.
struct Drained {
    /// What still has not got through, in the order it happened.
    left: Vec<Pending>,
    /// The title of the last one that did, for the status line.
    last_sent: Option<String>,
    /// Why it stopped, if it stopped.
    trouble: Option<KesslerError>,
}

/// Send these, oldest first, and stop at the first one that fails.
///
/// Stopping matters: scrobbles are a sequence, and sending the newest while the
/// one before it waits would file a listening session out of order. Whatever is
/// left comes back in its original order to be written down and tried again.
fn drain(pending: Vec<Pending>, send: impl Fn(&Pending) -> Result<()>) -> Drained {
    let mut left = Vec::new();
    let mut last_sent = None;
    let mut trouble = None;

    for entry in pending {
        if trouble.is_some() {
            left.push(entry);
            continue;
        }
        match send(&entry) {
            Ok(()) => last_sent = Some(entry.title.clone()),
            Err(e) => {
                trouble = Some(e);
                left.push(entry);
            }
        }
    }

    Drained {
        left,
        last_sent,
        trouble,
    }
}

fn trouble(error: KesslerError) -> Report {
    let text = error.to_string();
    let text = text
        .strip_prefix("last.fm: ")
        .unwrap_or(&text)
        .to_string();
    if [9, 4, 14].iter().any(|code| text.contains(&tr!("fehler {}", code))) {
        Report::Disconnected
    } else {
        Report::Trouble(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The window puts „last.fm: " in front of every report; the error type
    /// carries one too. Exactly one of them survives.
    #[test]
    fn a_report_names_last_fm_once() {
        let Report::Trouble(said) = trouble(KesslerError::Lastfm(
            "invalid parameters - your request is missing a required parameter (fehler 6)".into(),
        )) else {
            panic!("that should have been trouble, not a disconnection");
        };
        assert!(
            !said.starts_with("last.fm"),
            "the report still carries its own name: {said}"
        );
        assert!(said.starts_with("invalid parameters"));

        assert_eq!(
            trouble(KesslerError::Lastfm("session gone (fehler 9)".into())),
            Report::Disconnected
        );
    }

    fn config(key: Option<&str>, secret: Option<&str>, session: Option<&str>) -> LastfmConfig {
        LastfmConfig {
            api_key: key.map(str::to_string),
            api_secret: secret.map(str::to_string),
            session_key: session.map(str::to_string),
            username: None,
            now_playing: true,
        }
    }

    /// Two of the three is not enough, and a blank is not a value.
    #[test]
    fn all_three_are_needed_before_anything_is_sent() {
        assert!(Credentials::from_config(&config(Some("k"), Some("s"), Some("x"))).is_some());
        assert!(Credentials::from_config(&config(Some("k"), Some("s"), None)).is_none());
        assert!(Credentials::from_config(&config(None, Some("s"), Some("x"))).is_none());
        assert!(Credentials::from_config(&config(Some("k"), Some("  "), Some("x"))).is_none());
    }

    /// The approval link is what the person opens; it carries the token.
    #[test]
    fn the_approval_url_carries_key_and_token() {
        let url = approval_url("KEY", "TOKEN");
        assert!(url.starts_with("https://www.last.fm/api/auth/"));
        assert!(url.contains("api_key=KEY"));
        assert!(url.contains("token=TOKEN"));
    }

    fn track() -> Track {
        Track {
            id: 1,
            path: std::path::PathBuf::from("/m/1.mp3"),
            title: "  roygbiv  ".into(),
            artist: "Boards of Canada".into(),
            album_artist: "Boards of Canada".into(),
            album: "Music Has the Right to Children".into(),
            genre: String::new(),
            year: Some(1998),
            track_number: Some(8),
            disc_number: Some(1),
            duration_secs: 151.4,
            bitrate: None,
            sample_rate: None,
            file_size: 1,
            format: "mp3".into(),
            rating: 0,
            play_count: 0,
            last_played: None,
            date_added: chrono::NaiveDateTime::default(),
            date_modified: chrono::NaiveDateTime::default(),
            file_hash: None,
            has_artwork: false,
        }
    }

    #[test]
    fn a_track_becomes_the_fields_lastfm_wants() {
        let sent = fields(&track());
        assert_eq!(sent.title, "roygbiv", "whitespace would be scrobbled as-is");
        assert_eq!(sent.duration_secs, 151);
        assert_eq!(sent.track_number, Some(8));
    }

    /// An album artist equal to the artist is noise, and an empty album would
    /// be filed as an album called "".
    #[test]
    fn empty_and_redundant_fields_are_left_out() {
        let mut entry = fields(&track());
        let mut params = Vec::new();
        add_optional(&mut params, &entry);
        let names: Vec<&str> = params.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"album"));
        assert!(!names.contains(&"albumArtist"), "same as the artist");
        assert!(names.contains(&"trackNumber"));
        assert!(names.contains(&"duration"));

        entry.album = String::new();
        entry.album_artist = "Various".into();
        entry.track_number = None;
        entry.duration_secs = 0;
        let mut params = Vec::new();
        add_optional(&mut params, &entry);
        let names: Vec<&str> = params.iter().map(|(n, _)| n.as_str()).collect();
        assert!(!names.contains(&"album"));
        assert!(names.contains(&"albumArtist"));
        assert!(!names.contains(&"trackNumber"));
        assert!(!names.contains(&"duration"));
    }

    /// A dead session has to be told apart from a blocked network: one needs
    /// the person, the other needs patience.
    #[test]
    fn a_dead_session_is_reported_as_such() {
        assert_eq!(
            trouble(KesslerError::Lastfm(
                "Invalid session key (fehler 9)".into()
            )),
            Report::Disconnected
        );
        assert!(matches!(
            trouble(KesslerError::Lastfm("nicht erreichbar".into())),
            Report::Trouble(_)
        ));
    }

    fn waiting(title: &str, at: i64) -> Pending {
        Pending {
            artist: "Nala Sinephro".into(),
            title: title.into(),
            album: "Space 1.8".into(),
            album_artist: "Nala Sinephro".into(),
            track_number: Some(1),
            duration_secs: 300,
            started_at: at,
        }
    }

    /// Everything goes, and the last one is the one worth naming.
    #[test]
    fn a_queue_that_gets_through_is_emptied() {
        let queue = vec![waiting("space 1", 100), waiting("space 2", 200)];
        let outcome = drain(queue, |_| Ok(()));
        assert!(outcome.left.is_empty());
        assert_eq!(outcome.last_sent.as_deref(), Some("space 2"));
        assert!(outcome.trouble.is_none());
    }

    /// The one that failed and everything after it wait, in order — a session
    /// filed out of order is worse than one filed late.
    #[test]
    fn a_failure_holds_the_rest_back_in_order() {
        let queue = vec![
            waiting("space 1", 100),
            waiting("space 2", 200),
            waiting("space 3", 300),
            waiting("space 4", 400),
        ];
        let attempts = std::cell::Cell::new(0);
        let outcome = drain(queue, |entry| {
            attempts.set(attempts.get() + 1);
            if entry.title == "space 2" {
                Err(KesslerError::Lastfm("nicht erreichbar".into()))
            } else {
                Ok(())
            }
        });

        assert_eq!(attempts.get(), 2);
        let titles: Vec<&str> = outcome.left.iter().map(|p| p.title.as_str()).collect();
        assert_eq!(titles, vec!["space 2", "space 3", "space 4"]);
        assert_eq!(outcome.last_sent.as_deref(), Some("space 1"));
        assert!(outcome.trouble.is_some());
    }

    /// Nothing sent means nothing to report, and nothing lost.
    #[test]
    fn a_dead_connection_loses_nothing() {
        let queue = vec![waiting("space 1", 100), waiting("space 2", 200)];
        let outcome = drain(queue, |_| Err(KesslerError::Lastfm("weg".into())));
        assert_eq!(outcome.left.len(), 2);
        assert_eq!(outcome.left[0].started_at, 100);
        assert!(outcome.last_sent.is_none());
    }

    /// Not being set up is a state, not a failure.
    #[test]
    fn an_unconnected_account_starts_nothing() {
        let path = std::env::temp_dir().join("kessler-lastfm-none.tsv");
        assert!(Scrobbler::start(&config(Some("k"), Some("s"), None), path).is_none());
    }
}
