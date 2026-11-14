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

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

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

#[derive(Debug, Clone)]
pub struct Credentials {
    pub key: String,
    pub secret: String,
    pub session: String,
}

impl Credentials {
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

pub fn approval_url(key: &str, token: &str) -> String {
    format!("https://www.last.fm/api/auth/?api_key={key}&token={token}")
}

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

pub fn connect(key: String, secret: String) -> mpsc::Receiver<Result<(String, String)>> {
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

enum Job {
    NowPlaying(Box<Pending>),
    Scrobble(Box<Pending>),
    Love(Box<Pending>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Report {
    Scrobbled(String),
    Loved(String),
    Trouble(String),
    Disconnected,
}

pub struct Scrobbler {
    jobs: mpsc::Sender<Job>,
    reports: mpsc::Receiver<Report>,
    voice: mpsc::Sender<Report>,
    user: String,
    waiting: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl Scrobbler {
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

    pub fn poll(&self) -> Vec<Report> {
        self.reports.try_iter().collect()
    }

    pub fn user(&self) -> &str {
        &self.user
    }

    pub fn waiting(&self) -> usize {
        self.waiting.load(std::sync::atomic::Ordering::Relaxed)
    }
}

struct Drained {
    left: Vec<Pending>,
    last_sent: Option<String>,
    trouble: Option<KesslerError>,
}

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

    #[test]
    fn all_three_are_needed_before_anything_is_sent() {
        assert!(Credentials::from_config(&config(Some("k"), Some("s"), Some("x"))).is_some());
        assert!(Credentials::from_config(&config(Some("k"), Some("s"), None)).is_none());
        assert!(Credentials::from_config(&config(None, Some("s"), Some("x"))).is_none());
        assert!(Credentials::from_config(&config(Some("k"), Some("  "), Some("x"))).is_none());
    }

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

    #[test]
    fn a_queue_that_gets_through_is_emptied() {
        let queue = vec![waiting("space 1", 100), waiting("space 2", 200)];
        let outcome = drain(queue, |_| Ok(()));
        assert!(outcome.left.is_empty());
        assert_eq!(outcome.last_sent.as_deref(), Some("space 2"));
        assert!(outcome.trouble.is_none());
    }

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

    #[test]
    fn a_dead_connection_loses_nothing() {
        let queue = vec![waiting("space 1", 100), waiting("space 2", 200)];
        let outcome = drain(queue, |_| Err(KesslerError::Lastfm("weg".into())));
        assert_eq!(outcome.left.len(), 2);
        assert_eq!(outcome.left[0].started_at, 100);
        assert!(outcome.last_sent.is_none());
    }

    #[test]
    fn an_unconnected_account_starts_nothing() {
        let path = std::env::temp_dir().join("kessler-lastfm-none.tsv");
        assert!(Scrobbler::start(&config(Some("k"), Some("s"), None), path).is_none());
    }
}
