use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;

use serde::{Deserialize, Serialize};

pub mod feed;

pub use feed::{Chapter, Episode, Feed};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Show {
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub author: String,
    pub feed: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub artwork: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Left {
    pub id: String,
    pub at: f64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub done: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Kept {
    #[serde(default)]
    pub show: Vec<Show>,
    #[serde(default)]
    pub left: Vec<Left>,
    #[serde(default)]
    pub fetched: Vec<Fetched>,
    #[serde(default)]
    pub archived: Vec<String>,
    #[serde(default)]
    pub queue: Vec<Fetched>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spent: Vec<Fetched>,
    #[serde(default)]
    pub seen_at: i64,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub followed_at: std::collections::BTreeMap<String, i64>,
    #[serde(default)]
    pub starred: Vec<Fetched>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fetched {
    pub id: String,
    pub title: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub published: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub chapters_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub artwork: String,
    pub show: Show,
}

impl Fetched {
    pub fn of(episode: &Episode, show: &Show) -> Fetched {
        Fetched {
            id: episode.id.clone(),
            title: episode.title.clone(),
            url: episode.url.clone(),
            published: episode.published.clone(),
            seconds: episode.seconds,
            chapters_url: episode.chapters_url.clone(),
            artwork: episode.artwork.clone(),
            show: show.clone(),
        }
    }

    pub fn episode(&self) -> (Episode, Show) {
        let episode = Episode {
            id: self.id.clone(),
            title: self.title.clone(),
            url: self.url.clone(),
            published: self.published.clone(),
            seconds: self.seconds,
            chapters_url: self.chapters_url.clone(),
            artwork: self.artwork.clone(),
            at: date_of(&self.published).map_or(0, |d| d.timestamp()),
            ..Default::default()
        };
        (episode, self.show.clone())
    }
}

pub const OPEN_UNKNOWN_DAYS: i64 = 30;

const SPENT_KEPT: usize = 50;

const LEFT_KEPT: usize = 500;

const DONE_WITHIN_SECS: f64 = 30.0;

impl Kept {
    pub fn read() -> Kept {
        let path = path();
        if let Some(kept) = held::get(&path) {
            return kept;
        }
        let Ok(text) = std::fs::read_to_string(&path) else { return Kept::default() };
        match toml::from_str::<Kept>(&text) {
            Ok(mut kept) => {
                kept.once_each();
                held::read(&path, &kept);
                kept
            }
            Err(e) => {
                let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
                let aside = path.with_extension(format!("toml.unlesbar-{stamp}"));
                tracing::warn!("podcasts.toml cannot be read ({e}); kept as {}", aside.display());
                let _ = std::fs::write(&aside, &text);
                Kept::default()
            }
        }
    }

    pub fn write(&self) {
        held::write(path(), self.clone(), true);
    }

    pub fn write_soon(&self) {
        held::write(path(), self.clone(), false);
    }

    pub fn flush() {
        held::flush();
    }

    pub fn follows(&self, feed: &str) -> bool {
        self.show.iter().any(|s| s.feed == feed)
    }

    pub fn toggle(&mut self, show: &Show) -> bool {
        if let Some(at) = self.show.iter().position(|s| s.feed == show.feed) {
            self.show.remove(at);
            self.followed_at.remove(&show.feed);
            false
        } else {
            self.follow(show);
            true
        }
    }

    pub fn follow(&mut self, show: &Show) {
        self.show.push(show.clone());
        self.followed_at.insert(show.feed.clone(), chrono::Utc::now().timestamp());
    }

    pub fn open_since(&self, feed: &str) -> i64 {
        self.followed_at.get(feed).copied().unwrap_or_else(|| chrono::Utc::now().timestamp() - OPEN_UNKNOWN_DAYS * 86_400)
    }

    pub fn is_open(&self, episode: &Episode, feed: &str, left: &std::collections::HashMap<&str, &Left>, archived: &std::collections::HashSet<&str>) -> bool {
        if archived.contains(episode.id.as_str()) {
            return false;
        }
        match left.get(episode.id.as_str()) {
            Some(l) if l.done => false,
            Some(l) if l.at > 0.0 => true,
            _ => when(episode) >= self.open_since(feed),
        }
    }

    pub fn is_begun(&self, id: &str) -> bool {
        self.left(id).is_some_and(|l| !l.done && l.at > 0.0)
    }

    pub fn fetched_now(&mut self, episode: &Episode, show: &Show) {
        self.fetched.retain(|f| f.url != episode.url);
        self.fetched.push(Fetched::of(episode, show));
    }

    pub fn is_archived(&self, id: &str) -> bool {
        self.archived.iter().any(|a| a == id)
    }

    pub fn toggle_archived(&mut self, id: &str) -> bool {
        if self.is_archived(id) {
            self.archived.retain(|a| a != id);
            false
        } else {
            self.archived.push(id.to_string());
            self.queue.retain(|q| q.id != id);
            true
        }
    }

    pub fn is_starred(&self, id: &str) -> bool {
        self.starred.iter().any(|s| s.id == id)
    }

    pub fn toggle_starred(&mut self, episode: &Episode, show: &Show) -> bool {
        if self.is_starred(&episode.id) {
            self.starred.retain(|s| s.id != episode.id);
            false
        } else {
            self.starred.push(Fetched::of(episode, show));
            true
        }
    }

    pub fn unheard(&mut self, id: &str) {
        self.left.retain(|l| l.id != id);
    }

    pub fn is_queued(&self, id: &str) -> bool {
        self.queue.iter().any(|q| q.id == id)
    }

    pub fn toggle_queued(&mut self, episode: &Episode, show: &Show) -> bool {
        if self.is_queued(&episode.id) {
            self.queue.retain(|q| q.id != episode.id);
            false
        } else {
            self.spent.retain(|s| s.id != episode.id);
            self.queue.push(Fetched::of(episode, show));
            true
        }
    }

    fn once_each(&mut self) {
        let mut seen = std::collections::HashSet::new();
        self.queue.retain(|q| seen.insert(q.id.clone()));
        self.spent.retain(|s| !seen.contains(&s.id));
        let mut spent_seen = std::collections::HashSet::new();
        self.spent.retain(|s| spent_seen.insert(s.id.clone()));
    }

    pub fn heard(&mut self, id: &str) {
        self.left.retain(|l| l.id != id);
        self.left.push(Left { id: id.to_string(), at: 0.0, done: true });
    }

    pub fn fetched_episodes(&self) -> Vec<(Episode, Show)> {
        let mut all: Vec<(Episode, Show)> = self
            .fetched
            .iter()
            .filter(|f| cached(&f.url).is_file())
            .map(Fetched::episode)
            .collect();
        all.sort_by_key(|(e, _)| std::cmp::Reverse(when(e)));
        all
    }

    pub fn left(&self, id: &str) -> Option<&Left> {
        self.left.iter().find(|l| l.id == id)
    }

    fn spend_row(&mut self, row: Fetched) {
        self.spent.retain(|s| s.id != row.id);
        self.queue.retain(|q| q.id != row.id);
        self.spent.push(row);
        if self.spent.len() > SPENT_KEPT {
            let over = self.spent.len() - SPENT_KEPT;
            self.spent.drain(..over);
        }
    }

    pub fn go_to(&mut self, episode: &Episode, before: Option<Fetched>) {
        if let Some(at) = self.spent.iter().position(|s| s.id == episode.id) {
            let mut back = self.spent.split_off(at + 1);
            self.spent.truncate(at);
            back.extend(before);
            self.queue.retain(|q| q.id != episode.id && !back.iter().any(|b| b.id == q.id));
            for (i, row) in back.into_iter().enumerate() {
                self.queue.insert(i, row);
            }
            self.once_each();
        } else if let Some(at) = self.queue.iter().position(|q| q.id == episode.id) {
            let skipped: Vec<Fetched> = self.queue.drain(..at).collect();
            self.queue.remove(0);
            if let Some(before) = before {
                self.spend_row(before);
            }
            for row in skipped {
                self.spend_row(row);
            }
        } else if let Some(before) = before {
            self.spend_row(before);
        }
        self.queue.retain(|q| q.id != episode.id);
        self.spent.retain(|s| s.id != episode.id);
    }

    pub fn leave(&mut self, id: &str, at: f64, length: f64) {
        let done = length > 0.0 && at >= length - DONE_WITHIN_SECS;
        self.left.retain(|l| l.id != id);
        self.left.push(Left { id: id.to_string(), at: if done { 0.0 } else { at }, done });
        if self.left.len() > LEFT_KEPT {
            let over = self.left.len() - LEFT_KEPT;
            self.left.drain(..over);
        }
    }
}

mod held {
    use super::Kept;
    use std::path::{Path, PathBuf};
    use std::sync::mpsc;
    use std::sync::{Mutex, OnceLock};
    use std::time::SystemTime;

    type Stamp = (SystemTime, u64);

    struct Held {
        path: PathBuf,
        kept: Kept,
        stamp: Option<Stamp>,
        generation: u64,
    }

    enum Job {
        Write { path: PathBuf, kept: Box<Kept>, generation: u64, done: Option<mpsc::Sender<()>> },
        Flush(mpsc::Sender<()>),
    }

    fn held() -> &'static Mutex<Option<Held>> {
        static HELD: OnceLock<Mutex<Option<Held>>> = OnceLock::new();
        HELD.get_or_init(|| Mutex::new(None))
    }

    fn stamp_of(path: &Path) -> Option<Stamp> {
        let meta = std::fs::metadata(path).ok()?;
        Some((meta.modified().ok()?, meta.len()))
    }

    pub fn get(path: &Path) -> Option<Kept> {
        let held = held().lock().ok()?;
        let held = held.as_ref().filter(|h| h.path == path)?;
        match held.stamp {
            None => Some(held.kept.clone()),
            Some(stamp) => (stamp_of(path) == Some(stamp)).then(|| held.kept.clone()),
        }
    }

    pub fn read(path: &Path, kept: &Kept) {
        if let (Ok(mut held), Some(stamp)) = (held().lock(), stamp_of(path)) {
            let generation = held.as_ref().map_or(0, |h| h.generation);
            *held = Some(Held { path: path.to_path_buf(), kept: kept.clone(), stamp: Some(stamp), generation });
        }
    }

    fn writer() -> &'static Mutex<mpsc::Sender<Job>> {
        static WRITER: OnceLock<Mutex<mpsc::Sender<Job>>> = OnceLock::new();
        WRITER.get_or_init(|| {
            let (tx, rx) = mpsc::channel::<Job>();
            let _ = std::thread::Builder::new().name("kessler-podcasts-toml".into()).spawn(move || {
                for job in rx {
                    match job {
                        Job::Write { path, kept, generation, done } => {
                            put(&path, &kept);
                            if let Ok(mut held) = held().lock() {
                                if let Some(h) = held.as_mut().filter(|h| h.path == path && h.generation == generation) {
                                    h.stamp = stamp_of(&path);
                                }
                            }
                            if let Some(done) = done {
                                let _ = done.send(());
                            }
                        }
                        Job::Flush(done) => {
                            let _ = done.send(());
                        }
                    }
                }
            });
            Mutex::new(tx)
        })
    }

    fn put(path: &Path, kept: &Kept) {
        let Ok(text) = toml::to_string(kept) else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let part = path.with_extension("toml.part");
        if std::fs::write(&part, text).is_ok() {
            let _ = std::fs::rename(&part, path);
        }
    }

    pub fn write(path: PathBuf, kept: Kept, wait: bool) {
        let generation = {
            let Ok(mut held) = held().lock() else {
                put(&path, &kept);
                return;
            };
            let generation = held.as_ref().map_or(0, |h| h.generation) + 1;
            *held = Some(Held { path: path.clone(), kept: kept.clone(), stamp: None, generation });
            generation
        };
        let (done, finished) = if wait { let (tx, rx) = mpsc::channel(); (Some(tx), Some(rx)) } else { (None, None) };
        let sent = writer().lock().is_ok_and(|tx| tx.send(Job::Write { path: path.clone(), kept: Box::new(kept.clone()), generation, done }).is_ok());
        if !sent {
            put(&path, &kept);
            if let Ok(mut held) = held().lock() {
                if let Some(h) = held.as_mut().filter(|h| h.generation == generation) {
                    h.stamp = stamp_of(&path);
                }
            }
            return;
        }
        if let Some(finished) = finished {
            let _ = finished.recv();
        }
    }

    pub fn flush() {
        let (tx, rx) = mpsc::channel();
        if writer().lock().is_ok_and(|w| w.send(Job::Flush(tx)).is_ok()) {
            let _ = rx.recv();
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn a_write_soon_is_read_at_once_and_on_disk_after_the_flush() {
            let dir = std::env::temp_dir().join(format!("kessler-held-{}", std::process::id()));
            let path = dir.join("podcasts.toml");
            let mut kept = Kept::default();
            kept.leave("one", 42.0, 3600.0);
            write(path.clone(), kept.clone(), false);
            assert_eq!(get(&path), Some(kept.clone()));
            flush();
            let on_disk: Kept = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            assert_eq!(on_disk, kept);
            assert_eq!(get(&path), Some(kept));
            std::thread::sleep(std::time::Duration::from_millis(20));
            std::fs::write(&path, "seen_at = 7\n").unwrap();
            assert_eq!(get(&path), None);
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}

fn path() -> PathBuf {
    crate::config::Config::config_dir().join("podcasts.toml")
}

pub fn cache_dir() -> PathBuf {
    crate::config::Config::data_dir().join("podcasts")
}

pub fn cached(url: &str) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    url.hash(&mut hasher);
    let bare = url.split(['?', '#']).next().unwrap_or(url);
    let ext = Path::new(bare)
        .extension()
        .and_then(|e| e.to_str())
        .filter(|e| e.len() <= 4 && e.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or("mp3")
        .to_ascii_lowercase();
    cache_dir().join(format!("{:016x}.{ext}", hasher.finish()))
}

pub fn search(term: &str) -> Result<Vec<Show>, String> {
    let url = format!(
        "https://itunes.apple.com/search?media=podcast&entity=podcast&limit=40&term={}",
        crate::library::coverart::encode(term.trim())
    );
    let text = crate::library::coverart::fetch_text(&url).map_err(|e| e.to_string())?;
    Ok(shows_in(&text))
}

pub fn shows_in(text: &str) -> Vec<Show> {
    let Some(json) = crate::json::Json::parse(text) else { return Vec::new() };
    json.list("results")
        .iter()
        .filter_map(|r| {
            let feed = r.text("feedUrl")?.trim().to_string();
            (!feed.is_empty()).then(|| Show {
                title: r.text("collectionName").or(r.text("trackName")).unwrap_or("").trim().to_string(),
                author: r.text("artistName").unwrap_or("").trim().to_string(),
                feed,
                artwork: r.text("artworkUrl600").or(r.text("artworkUrl100")).unwrap_or("").to_string(),
            })
        })
        .collect()
}

pub fn fetch_feed(url: &str) -> Result<Feed, String> {
    match fetch_feed_since(url, None)? {
        Reading::Fresh { feed, .. } | Reading::Cached { feed, .. } => Ok(feed),
        Reading::Unchanged => Err(tr!("kein podcast-feed: {}", url)),
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Validators {
    pub etag: String,
    pub modified: String,
}

impl Validators {
    fn any(&self) -> bool {
        !self.etag.is_empty() || !self.modified.is_empty()
    }
}

#[derive(Debug)]
pub enum Reading {
    Cached { feed: Feed, validators: Validators },
    Fresh { feed: Feed, validators: Validators, moved: Option<String> },
    Unchanged,
}

const FEED_CONNECT: &str = "15";
const FEED_STALLED: &str = "20";
const FEED_AT_MOST: &str = "180";

pub fn fetch_feed_since(url: &str, since: Option<&Validators>) -> Result<Reading, String> {
    let waited = std::time::Instant::now();
    let _turn = FeedSlot::take();
    let began = std::time::Instant::now();
    let answer = fetch_feed_taken(url, since);
    let wait = (began - waited).as_secs_f32();
    let took = began.elapsed().as_secs_f32();
    let said = match &answer {
        Ok(Reading::Fresh { feed, .. }) => format!("{} folgen", feed.episodes.len()),
        Ok(Reading::Unchanged) => "unverändert".to_string(),
        Ok(Reading::Cached { .. }) => String::new(),
        Err(_) => "fehler".to_string(),
    };
    crate::player::watch::note(format!("feed gelesen: {} · {said} · {took:.1} s (gewartet {wait:.1} s)", feed_name(url)));
    answer
}

fn feed_name(url: &str) -> String {
    let bare = url.split("://").nth(1).unwrap_or(url);
    let bare = bare.split(['?', '#']).next().unwrap_or(bare);
    let host = bare.split('/').next().unwrap_or(bare);
    let last = bare.trim_end_matches('/').rsplit('/').next().filter(|l| *l != host).unwrap_or("");
    if last.is_empty() { host.to_string() } else { format!("{host}/…/{last}") }
}

fn fetch_feed_taken(url: &str, since: Option<&Validators>) -> Result<Reading, String> {
    static COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let heads = std::env::temp_dir().join(format!(
        "kessler-feed-{}-{}.headers",
        std::process::id(),
        COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let mut command = std::process::Command::new("curl");
    command.args(["--silent", "--show-error", "--fail-with-body", "--location", "--compressed"]);
    command.args(["--connect-timeout", FEED_CONNECT, "--speed-limit", "1", "--speed-time", FEED_STALLED, "--max-time", FEED_AT_MOST]);
    command.arg("--user-agent").arg(crate::library::coverart::agent());
    command.arg("--dump-header").arg(&heads);
    if let Some(since) = since {
        if !since.etag.is_empty() {
            command.arg("--header").arg(format!("If-None-Match: {}", since.etag));
        }
        if !since.modified.is_empty() {
            command.arg("--header").arg(format!("If-Modified-Since: {}", since.modified));
        }
    }
    command.arg(url);
    let output = command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| tr!("curl lässt sich nicht starten: {}", e))?;
    let said = std::fs::read_to_string(&heads).unwrap_or_default();
    let _ = std::fs::remove_file(&heads);
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if err.is_empty() { tr!("{} antwortet nicht", url) } else { err });
    }
    let answers = answers_in(&said);
    if answers.last().is_some_and(|(code, _)| *code == 304) {
        return Ok(Reading::Unchanged);
    }
    let mut moved = moved_for_good(url, &answers);
    let text = String::from_utf8_lossy(&output.stdout);
    let parsing = std::time::Instant::now();
    let feed = feed::read(&text).ok_or_else(|| tr!("kein podcast-feed: {}", url))?;
    crate::player::watch::note(format!(
        "feed gelesen: {} · {:.1} mb · gelesen in {:.0} ms",
        feed_name(url),
        output.stdout.len() as f64 / 1e6,
        parsing.elapsed().as_secs_f64() * 1000.0
    ));
    let last = answers.last().map(|(_, heads)| heads.clone()).unwrap_or_default();
    let head = |name: &str| last.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone()).unwrap_or_default();
    let validators = Validators { etag: head("etag"), modified: head("last-modified") };
    let new_url = feed.new_url.trim();
    if moved.is_none() && new_url.starts_with("http") && new_url != url {
        moved = Some(new_url.to_string());
    }
    Ok(Reading::Fresh { feed, validators, moved })
}

fn answers_in(said: &str) -> Vec<(u16, Vec<(String, String)>)> {
    let mut answers: Vec<(u16, Vec<(String, String)>)> = Vec::new();
    for line in said.lines() {
        let line = line.trim_end();
        if line.starts_with("HTTP/") {
            let code = line.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
            answers.push((code, Vec::new()));
        } else if let (Some((name, value)), Some((_, heads))) = (line.split_once(':'), answers.last_mut()) {
            heads.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
        }
    }
    answers
}

fn moved_for_good(url: &str, answers: &[(u16, Vec<(String, String)>)]) -> Option<String> {
    let mut at = url.to_string();
    let mut moved = None;
    for (code, heads) in answers {
        if !matches!(code, 301 | 308) {
            break;
        }
        let Some(location) = heads.iter().find(|(k, _)| k == "location").map(|(_, v)| v.clone()) else { break };
        let next = if location.starts_with("http") {
            location
        } else if let Some(path) = location.strip_prefix('/') {
            let origin: String = at.splitn(4, '/').take(3).collect::<Vec<_>>().join("/");
            format!("{origin}/{path}")
        } else {
            break;
        };
        at = next.clone();
        moved = Some(next);
    }
    moved
}

pub fn cached_feed_path(url: &str) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    url.hash(&mut hasher);
    crate::config::Config::data_dir().join("podcast-feeds").join(format!("{:016x}.toml", hasher.finish()))
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct KeptFeed {
    validators: Validators,
    feed: Feed,
}

pub fn read_cached_feed(url: &str) -> Option<(Feed, Validators)> {
    let text = std::fs::read_to_string(cached_feed_path(url)).ok()?;
    let kept: KeptFeed = toml::from_str(&text).ok()?;
    Some((kept.feed, kept.validators))
}

pub fn write_cached_feed(url: &str, feed: Feed, validators: Validators) {
    let path = cached_feed_path(url);
    std::thread::spawn(move || {
        let Ok(text) = toml::to_string(&KeptFeed { validators, feed }) else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let part = path.with_extension("part");
        if std::fs::write(&part, text).is_ok() {
            let _ = std::fs::rename(&part, &path);
        }
    });
}

const FEEDS_AT_ONCE: usize = 4;

struct FeedSlot;

static FEED_SLOTS: (std::sync::Mutex<usize>, std::sync::Condvar) = (std::sync::Mutex::new(0), std::sync::Condvar::new());

impl FeedSlot {
    fn take() -> FeedSlot {
        let (taken, freed) = &FEED_SLOTS;
        let mut taken = taken.lock().unwrap_or_else(|e| e.into_inner());
        while *taken >= FEEDS_AT_ONCE {
            taken = freed.wait(taken).unwrap_or_else(|e| e.into_inner());
        }
        *taken += 1;
        FeedSlot
    }
}

impl Drop for FeedSlot {
    fn drop(&mut self) {
        let (taken, freed) = &FEED_SLOTS;
        *taken.lock().unwrap_or_else(|e| e.into_inner()) -= 1;
        freed.notify_one();
    }
}

pub fn fetch_feed_later(url: &str, since: Option<Validators>, from_disk: bool) -> mpsc::Receiver<Result<Reading, String>> {
    let (tx, rx) = mpsc::channel();
    let url = url.to_string();
    std::thread::spawn(move || {
        let mut since = since.filter(Validators::any);
        if from_disk {
            let reading = std::time::Instant::now();
            let cached = read_cached_feed(&url);
            if let Some((feed, _)) = &cached {
                crate::player::watch::note(format!(
                    "feed von der platte: {} · {} folgen · {:.0} ms",
                    feed_name(&url),
                    feed.episodes.len(),
                    reading.elapsed().as_secs_f64() * 1000.0
                ));
            }
            if let Some((feed, validators)) = cached {
                if validators.any() {
                    since = Some(validators.clone());
                }
                if tx.send(Ok(Reading::Cached { feed, validators })).is_err() {
                    return;
                }
            }
        }
        let _ = tx.send(fetch_feed_since(&url, since.as_ref()));
    });
    rx
}

pub fn chapters_later(url: &str) -> mpsc::Receiver<Vec<Chapter>> {
    let (tx, rx) = mpsc::channel();
    let url = url.to_string();
    std::thread::spawn(move || {
        let found = crate::library::coverart::fetch_text(&url).map(|text| feed::chapters_in(&text)).unwrap_or_default();
        let _ = tx.send(found);
    });
    rx
}

pub fn date_of(published: &str) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    let text = published.trim();
    let text = match text.split_once(',') {
        Some((day, rest)) if day.trim().chars().all(|c| c.is_ascii_alphabetic()) => rest.trim(),
        _ => text,
    };
    chrono::DateTime::parse_from_str(text, "%d %b %Y %H:%M:%S %z")
        .or_else(|_| chrono::DateTime::parse_from_rfc2822(text))
        .or_else(|_| chrono::DateTime::parse_from_str(&text.replace("GMT", "+0000").replace("UTC", "+0000"), "%d %b %Y %H:%M:%S %z"))
        .ok()
}

pub fn when(episode: &Episode) -> i64 {
    if episode.at != 0 {
        return episode.at;
    }
    date_of(&episode.published).map_or(0, |at| at.timestamp())
}

pub fn show_at_later(url: &str) -> mpsc::Receiver<Result<Vec<Show>, String>> {
    let (tx, rx) = mpsc::channel();
    let url = url.trim().to_string();
    std::thread::spawn(move || {
        let found = fetch_feed(&url).map(|feed| {
            let title = if feed.title.is_empty() { url.clone() } else { feed.title.clone() };
            vec![Show { title, author: feed.author.clone(), feed: url.clone(), artwork: feed.artwork.clone() }]
        });
        let _ = tx.send(found);
    });
    rx
}

pub fn is_address(typed: &str) -> bool {
    let typed = typed.trim();
    (typed.starts_with("https://") || typed.starts_with("http://") || typed.starts_with("feed://")) && !typed.contains(' ')
}

pub fn search_later(term: &str) -> mpsc::Receiver<Result<Vec<Show>, String>> {
    let (tx, rx) = mpsc::channel();
    let term = term.to_string();
    std::thread::spawn(move || {
        let began = std::time::Instant::now();
        let found = search(&term);
        let said = found.as_ref().map_or_else(|_| "fehler".to_string(), |f| format!("{} gefunden", f.len()));
        crate::player::watch::note(format!("podcast-suche: {said} · {:.1} s", began.elapsed().as_secs_f32()));
        let _ = tx.send(found);
    });
    rx
}

#[derive(Debug, Clone, PartialEq)]
pub enum Fetching {
    Under(u64, u64),
    Done(PathBuf),
    Failed(String),
}

pub fn fetch_episode(url: &str, bytes: u64) -> mpsc::Receiver<Fetching> {
    let (tx, rx) = mpsc::channel();
    let target = cached(url);
    if target.is_file() {
        let _ = tx.send(Fetching::Done(target));
        return rx;
    }
    let url = url.to_string();
    std::thread::spawn(move || {
        let part = target.with_extension("part");
        if let Err(e) = std::fs::create_dir_all(cache_dir()) {
            let _ = tx.send(Fetching::Failed(e.to_string()));
            return;
        }
        let child = Command::new("curl")
            .args(["--connect-timeout", "15", "--speed-limit", "1", "--speed-time", "60"])
            .args(["--silent", "--show-error", "--fail", "--location", "--output"])
            .arg(&part)
            .arg(&url)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn();
        let mut child = match child {
            Ok(child) => child,
            Err(e) => {
                let _ = tx.send(Fetching::Failed(tr!("curl lässt sich nicht starten: {}", e)));
                return;
            }
        };
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    let said = if status.success() {
                        match std::fs::rename(&part, &target) {
                            Ok(()) => {
                                fetched_changed();
                                Fetching::Done(target)
                            }
                            Err(e) => Fetching::Failed(e.to_string()),
                        }
                    } else {
                        let mut why = String::new();
                        if let Some(mut err) = child.stderr.take() {
                            let _ = std::io::Read::read_to_string(&mut err, &mut why);
                        }
                        let _ = std::fs::remove_file(&part);
                        Fetching::Failed(if why.trim().is_empty() { tr!("{} antwortet nicht", url) } else { why.trim().to_string() })
                    };
                    let _ = tx.send(said);
                    return;
                }
                Ok(None) => {
                    let have = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
                    if tx.send(Fetching::Under(have, bytes)).is_err() {
                        let _ = child.kill();
                        let _ = child.wait();
                        let _ = std::fs::remove_file(&part);
                        return;
                    }
                }
                Err(e) => {
                    let _ = tx.send(Fetching::Failed(e.to_string()));
                    return;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
    });
    rx
}

pub fn chapters_in_file(file: &Path) -> Vec<Chapter> {
    use std::io::Read;
    let Ok(mut f) = std::fs::File::open(file) else { return Vec::new() };
    let mut head = [0u8; 10];
    if f.read_exact(&mut head).is_err() || &head[..3] != b"ID3" {
        return Vec::new();
    }
    let size = head[6..10].iter().fold(0usize, |n, &x| (n << 7) | usize::from(x & 0x7f)).min(16 << 20);
    let mut tag = head.to_vec();
    tag.resize(10 + size, 0);
    if f.read_exact(&mut tag[10..]).is_err() {
        return Vec::new();
    }
    feed::chapters_in_id3(&tag)
}

pub fn forget(file: &Path) {
    fetched_changed();
    if file.starts_with(cache_dir()) {
        let _ = std::fs::remove_file(file);
    }
}

pub fn forget_all(but: Option<&Path>) {
    fetched_changed();
    let Ok(dir) = std::fs::read_dir(cache_dir()) else { return };
    for entry in dir.flatten() {
        let path = entry.path();
        if Some(path.as_path()) != but && !is_part(&path) {
            let _ = std::fs::remove_file(&path);
        }
    }
}

pub fn forget_older(days: u64, but: &[PathBuf]) -> usize {
    fetched_changed();
    let Ok(dir) = std::fs::read_dir(cache_dir()) else { return 0 };
    let mut gone = 0;
    let limit = std::time::Duration::from_secs(days * 86_400);
    for entry in dir.flatten() {
        let path = entry.path();
        let old = entry.metadata().and_then(|m| m.modified()).ok().and_then(|at| at.elapsed().ok()).is_some_and(|age| age > limit);
        if old && !but.contains(&path) && !is_part(&path) && std::fs::remove_file(&path).is_ok() {
            gone += 1;
        }
    }
    gone
}

pub fn is_fetched(url: &str) -> bool {
    use std::collections::HashMap;
    use std::time::{Duration, Instant};
    let mut known = FETCHED_KNOWN.lock().unwrap_or_else(|e| e.into_inner());
    let known = known.get_or_insert_with(HashMap::new);
    if let Some((here, at)) = known.get(url) {
        if at.elapsed() < Duration::from_secs(2) {
            return *here;
        }
    }
    let here = cached(url).is_file();
    known.insert(url.to_string(), (here, Instant::now()));
    here
}

type FetchedKnown = std::collections::HashMap<String, (bool, std::time::Instant)>;
static FETCHED_KNOWN: std::sync::Mutex<Option<FetchedKnown>> = std::sync::Mutex::new(None);

fn fetched_changed() {
    *FETCHED_KNOWN.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

fn is_part(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "part")
}

pub fn opml(shows: &[Show]) -> String {
    let escape = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;");
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<opml version=\"2.0\">\n  <head><title>kessler</title></head>\n  <body>\n");
    for show in shows {
        out.push_str(&format!(
            "    <outline type=\"rss\" text=\"{}\" title=\"{}\" xmlUrl=\"{}\"/>\n",
            escape(&show.title),
            escape(&show.title),
            escape(&show.feed)
        ));
    }
    out.push_str("  </body>\n</opml>\n");
    out
}

pub fn shows_in_opml(text: &str) -> Vec<Show> {
    feed::opml_shows(text).into_iter().map(|(title, feed)| Show { title, feed, ..Default::default() }).collect()
}

pub fn artwork_path(show: &Show) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    show.feed.hash(&mut hasher);
    crate::config::Config::data_dir().join("podcast-art").join(format!("{:016x}", hasher.finish()))
}

pub fn fetch_artwork(show: &Show, artwork: &str) {
    let path = artwork_path(show);
    let url = if show.artwork.is_empty() { artwork.to_string() } else { show.artwork.clone() };
    if path.is_file() || url.is_empty() {
        return;
    }
    picture_worker(url, path);
}

pub const PICTURE_SIDE: u32 = 600;

pub fn smaller_picture_url(url: &str) -> String {
    let Some(slash) = url.rfind('/') else { return url.to_string() };
    let (head, last) = url.split_at(slash + 1);
    if !head.contains("mzstatic.com") {
        return url.to_string();
    }
    let Some((size, _)) = last.split_once("bb.") else { return url.to_string() };
    let Some((w, h)) = size.split_once('x') else { return url.to_string() };
    if w.parse::<u32>().is_err() || h.parse::<u32>().is_err() {
        return url.to_string();
    }
    format!("{head}{PICTURE_SIDE}x{PICTURE_SIDE}bb.jpg")
}

fn picture_worker(url: String, path: PathBuf) {
    use std::sync::{mpsc, Mutex, OnceLock};
    static WORK: OnceLock<Mutex<mpsc::Sender<(String, PathBuf)>>> = OnceLock::new();
    let send = WORK.get_or_init(|| {
        let (send, take) = mpsc::channel::<(String, PathBuf)>();
        std::thread::Builder::new()
            .name("podcast-pictures".into())
            .spawn(move || {
                while let Ok((url, path)) = take.recv() {
                    save_picture(&url, &path);
                    give_back_memory();
                }
            })
            .ok();
        Mutex::new(send)
    });
    let _ = send.lock().unwrap_or_else(|e| e.into_inner()).send((url, path));
}

pub fn give_back_memory() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    unsafe {
        libc::malloc_trim(0);
    }
    #[cfg(target_os = "macos")]
    {
        unsafe extern "C" {
            fn malloc_zone_pressure_relief(zone: *mut std::ffi::c_void, goal: usize) -> usize;
        }
        unsafe {
            malloc_zone_pressure_relief(std::ptr::null_mut(), 0);
        }
    }
}

fn save_picture(url: &str, path: &Path) {
    let Ok(bytes) = crate::library::coverart::fetch(&smaller_picture_url(url)) else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let bytes = small_picture(&bytes).unwrap_or(bytes);
    let part = path.with_extension("part");
    if std::fs::write(&part, bytes).is_ok() {
        let _ = std::fs::rename(&part, path);
    }
}

pub fn small_picture(bytes: &[u8]) -> Option<Vec<u8>> {
    static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let image = image::load_from_memory(bytes).ok()?;
    if image.width().max(image.height()) <= PICTURE_SIDE {
        return None;
    }
    let small = image.thumbnail(PICTURE_SIDE, PICTURE_SIDE).to_rgb8();
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 88).encode_image(&small).ok()?;
    Some(out)
}

pub fn episode_art_path(url: &str) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    url.hash(&mut hasher);
    crate::config::Config::data_dir().join("podcast-art").join(format!("e{:016x}", hasher.finish()))
}

pub fn fetch_episode_art(url: &str) {
    use std::sync::Mutex;
    static ASKED: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let path = episode_art_path(url);
    if url.is_empty() || path.is_file() {
        return;
    }
    {
        let mut asked = ASKED.lock().unwrap_or_else(|e| e.into_inner());
        if asked.iter().any(|a| a == url) {
            return;
        }
        asked.push(url.to_string());
    }
    let url = url.to_string();
    picture_worker(url, path);
}

pub fn picture_for(episode: &Episode, show: &Show) -> PathBuf {
    if !episode.artwork.is_empty() {
        let own = episode_art_path(&episode.artwork);
        if own.is_file() {
            return own;
        }
    }
    artwork_path(show)
}

pub fn forget_artwork() {
    let _ = std::fs::remove_dir_all(crate::config::Config::data_dir().join("podcast-art"));
}

pub fn cache_size() -> u64 {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};
    static KNOWN: Mutex<Option<(Instant, u64)>> = Mutex::new(None);
    let mut known = KNOWN.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, size)) = *known {
        if at.elapsed() < Duration::from_secs(2) {
            return size;
        }
    }
    let size = std::fs::read_dir(cache_dir())
        .map(|dir| dir.flatten().filter_map(|e| e.metadata().ok()).filter(|m| m.is_file()).map(|m| m.len()).sum())
        .unwrap_or(0);
    *known = Some((Instant::now(), size));
    size
}

#[cfg(test)]
mod tests {

    #[test]
    fn answers_and_moves_are_read() {
        let said = "HTTP/1.1 301 Moved Permanently\r\nLocation: /new/feed.xml\r\n\r\nHTTP/2 200\r\netag: \"abc\"\r\nlast-modified: Mon, 01 Jan 2024 10:00:00 GMT\r\n\r\n";
        let answers = answers_in(said);
        assert_eq!(answers.len(), 2);
        assert_eq!(answers[1].0, 200);
        assert!(answers[1].1.contains(&("etag".to_string(), "\"abc\"".to_string())));
        assert_eq!(moved_for_good("https://a.example/old.xml", &answers).as_deref(), Some("https://a.example/new/feed.xml"));
        let passing = answers_in("HTTP/1.1 302 Found\r\nLocation: https://b.example/x\r\n\r\nHTTP/1.1 200 OK\r\n\r\n");
        assert_eq!(moved_for_good("https://a.example/old.xml", &passing), None);
    }

    #[test]
    fn a_kept_feed_comes_back_whole() {
        let feed = Feed {
            title: "t".into(),
            episodes: vec![Episode {
                id: "a".into(),
                seconds: None,
                season: Some(2),
                links: vec![("w".into(), "https://w".into())],
                chapters: vec![Chapter { start: 1.5, title: "c".into() }],
                ..Default::default()
            }],
            older: 7,
            ..Default::default()
        };
        let kept = KeptFeed { validators: Validators { etag: "e".into(), modified: String::new() }, feed: feed.clone() };
        let back: KeptFeed = toml::from_str(&toml::to_string(&kept).expect("written")).expect("read");
        assert_eq!(back.feed, feed);
        assert_eq!(back.validators.etag, "e");
    }
    use super::*;

    #[test]
    fn an_episode_stands_in_the_queue_once() {
        let show = Show { title: "p".into(), feed: "f".into(), ..Default::default() };
        let episode = |id: &str| Episode { id: id.into(), title: id.into(), ..Default::default() };
        let row = |id: &str| Fetched::of(&episode(id), &show);
        let ids = |rows: &[Fetched]| rows.iter().map(|r| r.id.clone()).collect::<Vec<_>>();
        let mut kept = Kept { spent: vec![row("a"), row("b")], queue: vec![row("b"), row("c"), row("b")], ..Default::default() };
        kept.once_each();
        assert_eq!(ids(&kept.spent), ["a"]);
        assert_eq!(ids(&kept.queue), ["b", "c"]);
        kept.go_to(&episode("c"), Some(row("x")));
        assert!(!kept.queue.iter().chain(&kept.spent).any(|r| r.id == "c"));
        kept.toggle_queued(&episode("a"), &show);
        assert_eq!(ids(&kept.spent).iter().filter(|i| *i == "a").count(), 0);
    }

    #[test]
    fn the_queue_goes_back_and_forward_as_the_musics_does() {
        let show = Show { title: "p".into(), feed: "f".into(), ..Default::default() };
        let episode = |id: &str| Episode { id: id.into(), title: id.into(), ..Default::default() };
        let row = |id: &str| Fetched::of(&episode(id), &show);
        let ids = |rows: &[Fetched]| rows.iter().map(|r| r.id.clone()).collect::<Vec<_>>();
        let mut kept = Kept { spent: vec![row("a"), row("b"), row("c")], queue: vec![row("e"), row("f")], ..Default::default() };
        kept.go_to(&episode("b"), Some(row("d")));
        assert_eq!(ids(&kept.spent), ["a"]);
        assert_eq!(ids(&kept.queue), ["c", "d", "e", "f"]);
        kept.go_to(&episode("e"), Some(row("b")));
        assert_eq!(ids(&kept.spent), ["a", "b", "c", "d"]);
        assert_eq!(ids(&kept.queue), ["f"]);
        kept.go_to(&episode("x"), Some(row("e")));
        assert_eq!(ids(&kept.spent), ["a", "b", "c", "d", "e"]);
        assert_eq!(ids(&kept.queue), ["f"]);
    }

    #[test]
    fn played_episodes_line_up_once_and_the_oldest_go() {
        let show = Show { title: "p".into(), feed: "f".into(), ..Default::default() };
        let episode = |id: &str| Episode { id: id.into(), title: id.into(), ..Default::default() };
        let mut kept = Kept::default();
        kept.spend_row(Fetched::of(&episode("a"), &show));
        kept.spend_row(Fetched::of(&episode("b"), &show));
        kept.spend_row(Fetched::of(&episode("a"), &show));
        assert_eq!(kept.spent.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["b", "a"]);
        for n in 0..SPENT_KEPT + 3 {
            kept.spend_row(Fetched::of(&episode(&n.to_string()), &show));
        }
        assert_eq!(kept.spent.len(), SPENT_KEPT);
        assert_eq!(kept.spent.last().map(|s| s.id.clone()), Some((SPENT_KEPT + 2).to_string()));
    }

    #[test]
    fn pictures_are_asked_for_small() {
        assert_eq!(
            smaller_picture_url("https://is1-ssl.mzstatic.com/image/thumb/Podcasts/ab/cd/3000x3000bb.jpg"),
            format!("https://is1-ssl.mzstatic.com/image/thumb/Podcasts/ab/cd/{PICTURE_SIDE}x{PICTURE_SIDE}bb.jpg")
        );
        assert_eq!(smaller_picture_url("https://example.org/cover/3000x3000bb.jpg"), "https://example.org/cover/3000x3000bb.jpg");
        assert_eq!(smaller_picture_url("https://is1-ssl.mzstatic.com/image/x/cover.png"), "https://is1-ssl.mzstatic.com/image/x/cover.png");
    }

    #[test]
    fn a_big_picture_is_kept_small() {
        let encode = |side: u32| {
            let mut out = Vec::new();
            image::DynamicImage::new_rgb8(side, side).write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
            out
        };
        let small = small_picture(&encode(1600)).expect("made small");
        let read = image::load_from_memory(&small).unwrap();
        assert_eq!((read.width(), read.height()), (PICTURE_SIDE, PICTURE_SIDE));
        assert!(small_picture(&encode(300)).is_none());
    }

    #[test]
    fn what_is_open() {
        let show = Show { title: "s".into(), feed: "https://x/feed".into(), ..Default::default() };
        let mut kept = Kept::default();
        kept.follow(&show);
        let since = kept.open_since(&show.feed);
        let at = |id: &str, at: i64| Episode { id: id.into(), at, url: format!("https://x/{id}.mp3"), ..Default::default() };
        let (new, old, begun, heard, gone) = (at("new", since + 10), at("old", since - 10), at("begun", since - 99), at("heard", since + 5), at("gone", since + 5));
        kept.left.push(Left { id: "begun".into(), at: 60.0, done: false });
        kept.left.push(Left { id: "heard".into(), at: 600.0, done: true });
        kept.archived.push("gone".into());
        let left: std::collections::HashMap<&str, &Left> = kept.left.iter().map(|l| (l.id.as_str(), l)).collect();
        let archived: std::collections::HashSet<&str> = kept.archived.iter().map(String::as_str).collect();
        let open = |e: &Episode| kept.is_open(e, &show.feed, &left, &archived);
        assert!(open(&new) && open(&begun));
        assert!(!open(&old) && !open(&heard) && !open(&gone));
        assert!(kept.is_begun("begun") && !kept.is_begun("heard"));
        let other = kept.open_since("https://y/feed");
        assert!((chrono::Utc::now().timestamp() - other - OPEN_UNKNOWN_DAYS * 86_400).abs() < 5);
    }

    #[test]
    fn an_address_is_told_from_words() {
        assert!(is_address("https://example.org/feed.xml"));
        assert!(is_address(" feed://example.org/rss "));
        assert!(!is_address("john gruber"));
        assert!(!is_address("https://example.org/a feed"));
    }

    #[test]
    fn an_episode_is_kept_and_let_go() {
        let mut kept = Kept::default();
        let episode = Episode { id: "e1".into(), title: "eins".into(), url: "https://x/1.mp3".into(), ..Default::default() };
        let show = Show { title: "x".into(), feed: "https://x/feed".into(), ..Default::default() };
        assert!(kept.toggle_starred(&episode, &show));
        assert!(kept.is_starred("e1"));
        kept.heard("e1");
        assert!(kept.left("e1").is_some_and(|l| l.done));
        kept.unheard("e1");
        assert!(kept.left("e1").is_none());
        assert!(!kept.toggle_starred(&episode, &show));
        assert!(kept.starred.is_empty());
    }

    #[test]
    fn the_search_reply_is_read() {
        let reply = r#"{"resultCount":2,"results":[
            {"collectionName":"Kessler Radio","artistName":"Radio Rauschen","feedUrl":"https://example.org/feed.xml","artworkUrl600":"https://example.org/a.jpg"},
            {"collectionName":"Without a feed","artistName":"Nobody"}
        ]}"#;
        let shows = shows_in(reply);
        assert_eq!(shows.len(), 1);
        assert_eq!(shows[0].title, "Kessler Radio");
        assert_eq!(shows[0].author, "Radio Rauschen");
        assert_eq!(shows[0].feed, "https://example.org/feed.xml");
        assert_eq!(shows[0].artwork, "https://example.org/a.jpg");
        assert!(shows_in("not json").is_empty());
    }

    #[test]
    fn following_and_leaving() {
        let mut kept = Kept::default();
        let show = Show { title: "a".into(), feed: "https://x/feed".into(), ..Default::default() };
        assert!(kept.toggle(&show));
        assert!(kept.follows("https://x/feed"));
        assert!(!kept.toggle(&show));
        assert!(!kept.follows("https://x/feed"));

        kept.leave("ep", 600.0, 3600.0);
        assert_eq!(kept.left("ep").map(|l| (l.at, l.done)), Some((600.0, false)));
        kept.leave("ep", 3590.0, 3600.0);
        assert_eq!(kept.left("ep").map(|l| (l.at, l.done)), Some((0.0, true)), "near the end counts as heard");
        assert_eq!(kept.left.len(), 1);

        let text = toml::to_string(&kept).unwrap();
        assert_eq!(toml::from_str::<Kept>(&text).unwrap(), kept);
    }

    #[test]
    fn a_list_goes_out_and_comes_back_as_opml() {
        let shows = vec![
            Show { title: "Kessler & Freunde".into(), feed: "https://x/a.xml?b=1&c=2".into(), ..Default::default() },
            Show { title: "Testfunk".into(), feed: "https://x/t.xml".into(), ..Default::default() },
        ];
        let back = shows_in_opml(&opml(&shows));
        assert_eq!(back, shows);
        let other = r#"<opml><body><outline text="Gruppe"><outline title="Eins" xmlUrl="https://e/1"/></outline></body></opml>"#;
        assert_eq!(shows_in_opml(other), vec![Show { title: "Eins".into(), feed: "https://e/1".into(), ..Default::default() }]);
    }

    #[test]
    fn a_date_is_read_whatever_its_weekday() {
        let at = |s: &str| date_of(s).map(|d| d.format("%Y-%m-%d").to_string());
        assert_eq!(at("Sat, 31 Oct 2026 10:00:00 +0000").as_deref(), Some("2026-10-31"));
        assert_eq!(at("Fri, 31 Oct 2026 10:00:00 +0000").as_deref(), Some("2026-10-31"), "the wrong day of the week");
        assert_eq!(at("31 Oct 2026 10:00:00 GMT").as_deref(), Some("2026-10-31"));
        assert_eq!(at("nonsense"), None);
    }

    #[test]
    fn an_episode_keeps_its_kind_in_the_cache() {
        assert!(cached("https://x/a/ep.MP3?token=1").to_string_lossy().ends_with(".mp3"));
        assert!(cached("https://x/a/ep.m4a").to_string_lossy().ends_with(".m4a"));
        assert!(cached("https://x/a/episode").to_string_lossy().ends_with(".mp3"));
        assert_ne!(cached("https://x/1.mp3"), cached("https://x/2.mp3"));
    }
}
