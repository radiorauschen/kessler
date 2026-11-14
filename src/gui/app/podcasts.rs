use crate::db::models::Track;
use crate::gui::*;
use crate::podcast::{Chapter, Episode, Feed, Fetching, Kept, Show};
use std::collections::HashMap;
use std::sync::mpsc::Receiver;
use std::time::Instant;

pub const EPISODE_ID: i64 = -2;

const TYPED_SECS: f32 = 0.6;

const KEEP_EVERY_SECS: u64 = 15;

const SEEK_GIVE_UP_SECS: u64 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sleep {
    At(Instant, u64),
    EpisodeEnd,
}

const SLEEP_FADE_SECS: f32 = 8.0;

fn text_minutes(seconds: u64) -> String {
    format!("{} min", seconds.div_ceil(60).max(1))
}

type KeptOrder = (u64, Vec<(u32, u32)>);

const STREAM_FROM_BYTES: u64 = 1_000_000;

type GrowingDecoder = rodio::Decoder<std::io::BufReader<crate::player::growing::Growing>>;

const LOOK_AGAIN_ON_OPEN_SECS: u64 = 5 * 60;

pub const EPISODES_PAGE: usize = 80;

pub type Underway = (Episode, Show, Receiver<Fetching>, (u64, u64));

type Answer<T> = Receiver<std::result::Result<T, String>>;

#[derive(Default)]
pub struct PodcastPanel {
    pub feeds_version: u64,
    order: std::cell::RefCell<Option<KeptOrder>>,
    open_counted: std::cell::Cell<Option<(Instant, usize)>>,
    pub open: bool,
    pub kept: Kept,
    pub found: Option<Vec<Show>>,
    pub searching: Option<Answer<Vec<Show>>>,
    pub typed_at: Option<Instant>,
    pub chosen: Option<Show>,
    pub more: usize,
    pub only_fetched: bool,
    pub only_starred: bool,
    pub only_open: bool,
    pub info_open: bool,
    pub focus: usize,
    pub sleep: Option<Sleep>,
    pub sleep_fade_from: Option<u8>,
    pub feeds: HashMap<String, Feed>,
    pub reading: Vec<(String, Answer<crate::podcast::Reading>)>,
    pub validators: HashMap<String, crate::podcast::Validators>,
    pub episode: Option<(Episode, Show)>,
    pub picked: Vec<(Episode, Show)>,
    pub queue_cursor: Option<String>,
    pub chapters: Option<(String, Vec<Chapter>)>,
    pub chapters_reading: Option<(String, Receiver<Vec<Chapter>>)>,
    pub follow: std::cell::RefCell<Option<String>>,
    pub whole: Option<String>,
    pub unreached: Vec<String>,
    pub fetching: Option<Underway>,
    pub start_at: Option<f64>,
    pub seek_once_playing: Option<(f64, Instant)>,
    pub playing: Option<(String, std::path::PathBuf)>,
    pub kept_at: Option<Instant>,
    pub checked_at: Option<Instant>,
    pub new_by_show: HashMap<String, usize>,
    pub recount: bool,
    pub import: Option<Receiver<Option<std::path::PathBuf>>>,
    pub streaming: Option<(std::sync::Arc<std::sync::atomic::AtomicBool>, String)>,
    pub fetch_asked_over: Option<(u8, bool)>,
    pub fetch_began: Option<Instant>,
    pub round_began: Option<Instant>,
    pub playing_from: Option<(Episode, Show)>,
    pub last_place: (f64, f64),
    pub playing_chapters: Vec<Chapter>,
}

impl PodcastPanel {
    fn waiting(&self) -> bool {
        self.searching.is_some()
            || !self.reading.is_empty()
            || self.fetching.is_some()
            || self.typed_at.is_some()
            || self.chapters_reading.is_some()
            || self.seek_once_playing.is_some()
            || self.import.is_some()
    }

    pub fn episodes(&self, archived: bool) -> (Vec<(Episode, Show)>, usize) {
        if self.only_fetched || self.only_starred || self.only_open {
            let all = self.listed(archived);
            let total = all.len();
            let page = all.into_iter().take(EPISODES_PAGE + self.more).map(|(e, s)| (e.into_owned(), s.into_owned())).collect();
            return (page, total);
        }
        let shows: Vec<&Show> = match &self.chosen {
            Some(show) => vec![show],
            None => self.kept.show.iter().collect(),
        };
        let key = self.order_key(archived);
        let mut order = self.order.borrow_mut();
        if order.as_ref().is_none_or(|(k, _)| *k != key) {
            let gone: std::collections::HashSet<&str> =
                if archived { std::collections::HashSet::new() } else { self.kept.archived.iter().map(String::as_str).collect() };
            let mut all: Vec<(i64, u32, u32)> = Vec::new();
            for (s, show) in shows.iter().enumerate() {
                let Some(feed) = self.feeds.get(&show.feed) else { continue };
                for (i, e) in feed.episodes.iter().enumerate() {
                    if !gone.contains(e.id.as_str()) {
                        all.push((crate::podcast::when(e), s as u32, i as u32));
                    }
                }
            }
            all.sort_by_key(|(at, ..)| std::cmp::Reverse(*at));
            if let Some(feed) = self.chosen.as_ref().and_then(|s| self.feeds.get(&s.feed)).filter(|f| f.serial) {
                all.sort_by_key(|&(at, _, i)| {
                    let e = &feed.episodes[i as usize];
                    (e.season.unwrap_or(0), e.number.unwrap_or(0), at)
                });
            }
            *order = Some((key, all.into_iter().map(|(_, s, i)| (s, i)).collect()));
        }
        let Some((_, indices)) = order.as_ref() else { return (Vec::new(), 0) };
        let page = indices
            .iter()
            .take(EPISODES_PAGE + self.more)
            .filter_map(|&(s, i)| {
                let show = shows.get(s as usize)?;
                let episode = self.feeds.get(&show.feed)?.episodes.get(i as usize)?;
                Some((episode.clone(), (*show).clone()))
            })
            .collect();
        (page, indices.len())
    }

    fn order_key(&self, archived: bool) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        archived.hash(&mut h);
        self.feeds_version.hash(&mut h);
        self.chosen.as_ref().map(|s| s.feed.as_str()).hash(&mut h);
        for show in &self.kept.show {
            show.feed.hash(&mut h);
        }
        if !archived {
            self.kept.archived.hash(&mut h);
        }
        h.finish()
    }

    pub(in crate::gui) fn listed(&self, archived: bool) -> Vec<(std::borrow::Cow<'_, Episode>, std::borrow::Cow<'_, Show>)> {
        use std::borrow::Cow;
        let mut all: Vec<(Cow<'_, Episode>, Cow<'_, Show>)> = if self.only_fetched {
            self.kept.fetched_episodes().into_iter().map(|(e, s)| (Cow::Owned(e), Cow::Owned(s))).collect()
        } else if self.only_starred {
            self.kept.starred.iter().map(crate::podcast::Fetched::episode).map(|(e, s)| (Cow::Owned(e), Cow::Owned(s))).collect()
        } else if self.only_open {
            let mut open = self.open_episodes();
            open.sort_by_key(|(e, _)| (!self.kept.is_begun(&e.id), std::cmp::Reverse(crate::podcast::when(e))));
            return open;
        } else if let Some(show) = &self.chosen {
            self.feeds
                .get(&show.feed)
                .map(|feed| feed.episodes.iter().map(|e| (Cow::Borrowed(e), Cow::Borrowed(show))).collect())
                .unwrap_or_default()
        } else {
            self.kept
                .show
                .iter()
                .filter_map(|show| self.feeds.get(&show.feed).map(|feed| (feed, show)))
                .flat_map(|(feed, show)| feed.episodes.iter().map(move |e| (Cow::Borrowed(e), Cow::Borrowed(show))))
                .collect()
        };
        if !archived && !self.kept.archived.is_empty() {
            let gone: std::collections::HashSet<&str> = self.kept.archived.iter().map(String::as_str).collect();
            all.retain(|(e, _)| !gone.contains(e.id.as_str()));
        }
        all.sort_by_key(|(e, _)| std::cmp::Reverse(crate::podcast::when(e)));
        all
    }

    pub fn fetched_hidden(&self, show_archived: bool) -> (usize, u64) {
        if show_archived {
            return (0, 0);
        }
        let archived: std::collections::HashSet<&str> = self.kept.archived.iter().map(String::as_str).collect();
        self.kept
            .fetched
            .iter()
            .filter(|f| archived.contains(f.id.as_str()))
            .filter_map(|f| std::fs::metadata(crate::podcast::cached(&f.url)).ok())
            .fold((0, 0), |(n, bytes), m| (n + 1, bytes + m.len()))
    }

    fn open_episodes(&self) -> Vec<(std::borrow::Cow<'_, Episode>, std::borrow::Cow<'_, Show>)> {
        use std::borrow::Cow;
        let left: HashMap<&str, &crate::podcast::Left> = self.kept.left.iter().map(|l| (l.id.as_str(), l)).collect();
        let archived: std::collections::HashSet<&str> = self.kept.archived.iter().map(String::as_str).collect();
        self.kept
            .show
            .iter()
            .filter_map(|show| self.feeds.get(&show.feed).map(|feed| (feed, show)))
            .flat_map(|(feed, show)| feed.episodes.iter().map(move |e| (e, show)))
            .filter(|(e, show)| self.kept.is_open(e, &show.feed, &left, &archived))
            .map(|(e, s)| (Cow::Borrowed(e), Cow::Borrowed(s)))
            .collect()
    }

    pub fn open_count(&self) -> usize {
        let now = std::time::Instant::now();
        if let Some((at, n)) = self.open_counted.get() {
            if now.duration_since(at) < std::time::Duration::from_secs(1) {
                return n;
            }
        }
        let n = self.open_episodes().len();
        self.open_counted.set(Some((now, n)));
        n
    }

    pub(in crate::gui) fn every_episode(&self) -> Vec<(Episode, Show)> {
        self.listed(true).into_iter().map(|(e, s)| (e.into_owned(), s.into_owned())).collect()
    }

    pub fn is_new(&self, episode: &Episode) -> bool {
        crate::podcast::when(episode) > self.kept.seen_at && self.kept.left(&episode.id).is_none() && !self.kept.is_archived(&episode.id)
    }

    pub fn new_total(&self) -> usize {
        self.new_by_show.values().sum()
    }

    fn count_new(&mut self) {
        self.recount = false;
        let counts: HashMap<String, usize> = self
            .kept
            .show
            .iter()
            .filter_map(|show| {
                let feed = self.feeds.get(&show.feed)?;
                let n = feed.episodes.iter().filter(|e| self.is_new(e)).count();
                (n > 0).then(|| (show.feed.clone(), n))
            })
            .collect();
        self.new_by_show = counts;
    }

    pub fn reading_feed(&self, url: &str) -> bool {
        self.reading.iter().any(|(u, _)| u == url)
    }
}

impl GuiApp {
    pub fn showing_podcasts(&self) -> bool {
        self.podcasts.open
    }

    pub fn show_podcasts(&mut self) {
        self.read_from(false, false);
        if let Err(e) = self.close_filter() {
            self.set_status(format!("{e}"));
        }
        self.podcasts.kept = Kept::read();
        if self.podcasts.kept.seen_at == 0 {
            self.podcasts.kept.seen_at = chrono::Utc::now().timestamp();
            self.podcasts.kept.write();
        }
        self.podcasts.open = true;
        self.switch_sides(View::Music, View::Podcasts);
        let shows = self.podcasts.kept.show.clone();
        for show in &shows {
            self.read_feed(show);
        }
        let stale = self.podcasts.checked_at.is_none_or(|at| at.elapsed().as_secs() >= LOOK_AGAIN_ON_OPEN_SECS);
        if stale && self.check_every().is_some() {
            self.check_for_episodes();
        }
        self.podcasts.recount = true;
        self.archive_episodes();
    }

    pub(in crate::gui) fn archive_episodes(&mut self) {
        let Ok(days) = self.config.ui.podcast_keep.parse::<u64>() else { return };
        let mut spared: Vec<std::path::PathBuf> = self.podcasts.playing.iter().map(|(_, path)| path.clone()).collect();
        spared.extend(Kept::read().starred.iter().map(|s| crate::podcast::cached(&s.url)));
        let gone = crate::podcast::forget_older(days, &spared);
        if gone > 0 {
            self.note(crate::gui::log::Kind::Podcast, tr!("{} geladene folgen nach {} tagen gelöscht", gone, days));
        }
    }

    pub(in crate::gui) fn forget_all_episodes(&mut self) {
        let playing = self.podcasts.playing.as_ref().map(|(_, path)| path.clone());
        crate::podcast::forget_all(playing.as_deref());
        self.set_notice(tr!("geladene folgen gelöscht"));
        self.note(crate::gui::log::Kind::Podcast, tr!("geladene folgen gelöscht"));
    }

    pub fn skip_episode(&mut self, seconds: f64) {
        if !self.playing_episode() {
            return;
        }
        let length = self.player.duration().as_secs_f64();
        let at = (self.player.position().as_secs_f64() + seconds).clamp(0.0, (length - 1.0).max(0.0));
        self.player.seek_to(at);
    }

    pub(in crate::gui) fn export_podcasts(&mut self) {
        let kept = Kept::read();
        if kept.show.is_empty() {
            self.set_status(tr!("keinem podcast gefolgt"));
            return;
        }
        let home = std::env::var_os("HOME").map(std::path::PathBuf::from).unwrap_or_default();
        let desktop = home.join("Desktop");
        let dir = if desktop.is_dir() { desktop } else { home };
        let stamp = chrono::Local::now().format("%Y-%m-%d-%H%M");
        let path = dir.join(format!("kessler-podcasts-{stamp}.opml"));
        match std::fs::write(&path, crate::podcast::opml(&kept.show)) {
            Ok(()) => {
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                self.set_notice(tr!("podcasts gespeichert: {}", name));
                self.note(crate::gui::log::Kind::Podcast, tr!("podcasts gespeichert: {}", path.display().to_string()));
                let _ = crate::gui::reveal::show(&path);
            }
            Err(e) => self.set_status(format!("{e}")),
        }
    }

    pub(in crate::gui) fn import_podcasts(&mut self) {
        if self.podcasts.import.is_some() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let prompt = tr!("podcast-liste wählen (opml)");
        std::thread::spawn(move || {
            let _ = tx.send(crate::gui::folder::ask_playlist(prompt));
        });
        self.podcasts.import = Some(rx);
    }

    fn take_podcast_import(&mut self) {
        let Some(asking) = &self.podcasts.import else { return };
        let picked = match asking.try_recv() {
            Ok(picked) => picked,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => None,
        };
        self.podcasts.import = None;
        let Some(path) = picked else { return };
        let shows = match std::fs::read(&path) {
            Ok(bytes) => crate::podcast::shows_in_opml(&String::from_utf8_lossy(&bytes)),
            Err(e) => {
                self.set_status(format!("{e}"));
                return;
            }
        };
        if shows.is_empty() {
            self.set_status(tr!("keine podcasts in der datei"));
            return;
        }
        let mut kept = Kept::read();
        let found = shows.len();
        let mut added = 0;
        for show in shows {
            if !kept.follows(&show.feed) {
                kept.follow(&show);
                added += 1;
                if self.podcasts.open {
                    self.read_feed(&show);
                }
            }
        }
        kept.write();
        self.podcasts.kept = kept;
        self.set_status(tr!("{} von {} podcasts neu gefolgt", added, found));
        self.note(crate::gui::log::Kind::Podcast, tr!("{} von {} podcasts neu gefolgt", added, found));
    }

    pub(in crate::gui) fn reload_podcast_pictures(&mut self) {
        crate::podcast::forget_artwork();
        crate::gui::library::forget_decoded_pictures();
        let shows = Kept::read().show;
        for show in &shows {
            let artwork = self.podcasts.feeds.get(&show.feed).map(|f| f.artwork.clone()).unwrap_or_default();
            let mut show = show.clone();
            if !artwork.is_empty() {
                show.artwork = artwork.clone();
            }
            crate::podcast::fetch_artwork(&show, &artwork);
        }
        self.set_notice(tr!("bilder werden neu geladen"));
    }

    pub(in crate::gui) fn leave_podcasts(&mut self) {
        if !self.podcasts.open {
            return;
        }
        self.podcasts.open = false;
        self.switch_sides(View::Podcasts, View::Music);
        self.podcasts.found = None;
        self.podcasts.searching = None;
        self.podcasts.typed_at = None;
        let mut kept = Kept::read();
        kept.seen_at = chrono::Utc::now().timestamp();
        kept.write();
        self.podcasts.kept = kept;
        self.podcasts.recount = true;
        if self.query().is_some() {
            if let Err(e) = self.close_filter() {
                self.set_status(format!("{e}"));
            }
        }
    }

    pub(in crate::gui) fn forget_feeds(&mut self) {
        self.podcasts.feeds = HashMap::new();
        self.podcasts.reading = Vec::new();
        self.podcasts.found = None;
        self.podcasts.episode = None;
        self.podcasts.picked = Vec::new();
        self.podcasts.chapters = None;
        self.podcasts.chapters_reading = None;
        self.podcasts.whole = None;
        self.podcasts.validators = HashMap::new();
        self.podcasts.checked_at = None;
        self.podcasts.feeds_version += 1;
        crate::podcast::give_back_memory();
    }

    pub(in crate::gui) fn load_older(&mut self, show: &Show) {
        self.podcasts.whole = Some(show.feed.clone());
        if !self.podcasts.reading_feed(&show.feed) {
            self.podcasts.reading.push((show.feed.clone(), crate::podcast::fetch_feed_later(&show.feed, None, false)));
        }
    }

    fn feed_moved(&mut self, from: &str, to: &str) -> String {
        let mut kept = Kept::read();
        if !kept.follows(from) || kept.follows(to) {
            return from.to_string();
        }
        let mut name = from.to_string();
        for show in &mut kept.show {
            if show.feed == from {
                show.feed = to.to_string();
                name = show.title.clone();
            }
        }
        for f in kept.fetched.iter_mut().chain(kept.queue.iter_mut()).chain(kept.starred.iter_mut()) {
            if f.show.feed == from {
                f.show.feed = to.to_string();
            }
        }
        if let Some(at) = kept.followed_at.remove(from) {
            kept.followed_at.insert(to.to_string(), at);
        }
        kept.write();
        self.podcasts.kept = kept;
        if let Some(chosen) = self.podcasts.chosen.as_mut().filter(|c| c.feed == from) {
            chosen.feed = to.to_string();
        }
        if let Some(feed) = self.podcasts.feeds.remove(from) {
            self.podcasts.feeds.insert(to.to_string(), feed);
        }
        self.podcasts.validators.remove(from);
        if self.podcasts.whole.as_deref() == Some(from) {
            self.podcasts.whole = Some(to.to_string());
        }
        let _ = std::fs::remove_file(crate::podcast::cached_feed_path(from));
        self.note(crate::gui::log::Kind::Podcast, tr!("{} ist umgezogen: {}", name, to));
        to.to_string()
    }

    pub(in crate::gui) fn podcasts_typed(&mut self) {
        self.podcasts.typed_at = Some(Instant::now());
    }

    pub(in crate::gui) fn check_for_episodes(&mut self) {
        self.podcasts.checked_at = Some(Instant::now());
        self.archive_episodes();
        if !self.podcasts.open {
            self.podcasts.kept = Kept::read();
        }
        let shows = self.podcasts.kept.show.clone();
        let before = self.podcasts.reading.len();
        for show in &shows {
            if !self.podcasts.reading_feed(&show.feed) {
                let held = self.podcasts.feeds.contains_key(&show.feed);
                let since = held.then(|| self.podcasts.validators.get(&show.feed).cloned()).flatten();
                self.podcasts.reading.push((show.feed.clone(), crate::podcast::fetch_feed_later(&show.feed, since, !held)));
            }
        }
        let asked = self.podcasts.reading.len() - before;
        if asked > 0 {
            crate::player::watch::note(format!("neue folgen suchen: {asked} feeds"));
            self.podcasts.round_began = Some(Instant::now());
        }
    }

    fn check_every(&self) -> Option<u64> {
        self.config.ui.podcast_check.parse::<u64>().ok().map(|minutes| minutes.max(1) * 60)
    }

    pub fn checking_for_episodes(&self) -> bool {
        !self.podcasts.reading.is_empty()
    }

    pub fn podcast_standing(&self) -> Option<String> {
        if !self.config.ui.podcasts {
            return None;
        }
        if let Some((_, _, _, (have, of))) = &self.podcasts.fetching {
            let how_far = if *of > 0 { format!("{} %", have * 100 / of) } else { format!("{:.1} mb", *have as f64 / 1_000_000.0) };
            return Some(tr!("podcasts  lädt {}", how_far));
        }
        match self.podcasts.reading.len() {
            0 => None,
            1 => Some(tr!("podcasts  sehe nach neuen folgen").to_string()),
            n => Some(tr!("podcasts  sehe nach neuen folgen · {} feeds", n)),
        }
    }

    fn read_feed(&mut self, show: &Show) {
        if self.podcasts.feeds.contains_key(&show.feed) || self.podcasts.reading_feed(&show.feed) {
            return;
        }
        self.podcasts.reading.push((show.feed.clone(), crate::podcast::fetch_feed_later(&show.feed, None, true)));
    }

    pub fn choose_podcast(&mut self, show: Option<Show>) {
        if let Some(show) = &show {
            self.read_feed(show);
        }
        if let Some(whole) = self.podcasts.whole.take_if(|w| show.as_ref().is_none_or(|s| s.feed != *w)) {
            let kept = &self.podcasts.kept;
            if let Some(feed) = self.podcasts.feeds.get_mut(&whole) {
                feed.keep_newest(|e| kept.is_begun(&e.id));
            }
            self.podcasts.feeds_version += 1;
            crate::podcast::give_back_memory();
        }
        self.podcasts.chosen = show;
        self.podcasts.only_fetched = false;
        self.podcasts.only_starred = false;
        self.podcasts.only_open = false;
        self.podcasts.more = 0;
        self.podcasts.picked.clear();
    }

    pub fn podcast_info(&mut self) {
        self.podcasts.info_open = !self.podcasts.info_open;
        if self.podcasts.info_open {
            self.log_open = false;
            self.eq_open = false;
            let show = self.podcasts.playing_from.clone().filter(|(e, _)| self.playing_this_episode(&e.id)).map(|(_, s)| s).or_else(|| self.podcasts.chosen.clone());
            if let Some(show) = show.filter(|s| !self.podcasts.feeds.contains_key(&s.feed)) {
                self.read_feed(&show);
            }
        }
    }

    pub fn choose_fetched(&mut self) {
        self.podcasts.chosen = None;
        self.podcasts.only_fetched = true;
        self.podcasts.only_starred = false;
        self.podcasts.only_open = false;
        self.podcasts.more = 0;
        self.podcasts.picked.clear();
    }

    pub fn forget_episode(&mut self, episode: &Episode) {
        if self.playing_this_episode(&episode.id) {
            return;
        }
        let file = crate::podcast::cached(&episode.url);
        if file.is_file() {
            crate::podcast::forget(&file);
            self.note(crate::gui::log::Kind::Podcast, tr!("datei gelöscht: {}", episode.title));
        }
    }

    pub fn delete_episode_file(&mut self, episode: &Episode) {
        if self.playing_this_episode(&episode.id) {
            self.keep_episode_place();
            self.player.stop();
            self.podcasts.playing = None;
        }
        if self.podcasts.fetching.as_ref().is_some_and(|(e, ..)| e.id == episode.id) {
            if let Some((done, _)) = self.podcasts.streaming.take() {
                done.store(true, std::sync::atomic::Ordering::Release);
            }
            self.podcasts.fetching = None;
            self.player.set_reach(None);
            let _ = std::fs::remove_file(crate::podcast::cached(&episode.url).with_extension("part"));
        }
        self.forget_episode(episode);
        self.podcasts.recount = true;
    }

    pub fn archive_episode(&mut self, episode: &Episode) {
        let mut kept = Kept::read();
        let now = kept.toggle_archived(&episode.id);
        kept.write();
        self.podcasts.kept = kept;
        if now && self.config.ui.podcast_archive_forgets && !self.podcasts.kept.is_starred(&episode.id) {
            self.forget_episode(episode);
        }
        self.podcasts.recount = true;
        self.note(crate::gui::log::Kind::Podcast, if now { tr!("archiviert: {}", episode.title) } else { tr!("nicht mehr archiviert: {}", episode.title) });
        self.set_notice(if now { tr!("archiviert") } else { tr!("nicht mehr archiviert") });
    }

    pub fn archive_picked(&mut self) {
        let picked = std::mem::take(&mut self.podcasts.picked);
        let mut kept = Kept::read();
        let all_in = picked.iter().all(|(e, _)| kept.is_archived(&e.id));
        for (episode, _) in &picked {
            if kept.is_archived(&episode.id) == all_in {
                kept.toggle_archived(&episode.id);
            }
        }
        kept.write();
        self.podcasts.kept = kept;
        if !all_in && self.config.ui.podcast_archive_forgets {
            for (episode, _) in &picked {
                if !self.podcasts.kept.is_starred(&episode.id) {
                    self.forget_episode(episode);
                }
            }
        }
        self.podcasts.recount = true;
        let n = picked.len();
        let said = if all_in { tr!("{} folgen nicht mehr archiviert", n) } else { tr!("{} folgen archiviert", n) };
        self.note(crate::gui::log::Kind::Podcast, said.clone());
        self.set_notice(said);
    }

    pub fn queue_picked(&mut self) {
        let picked = std::mem::take(&mut self.podcasts.picked);
        let mut kept = Kept::read();
        let mut added = 0;
        for (episode, show) in &picked {
            if !kept.is_queued(&episode.id) && !self.playing_this_episode(&episode.id) {
                kept.toggle_queued(episode, show);
                added += 1;
            }
        }
        kept.write();
        self.podcasts.kept = kept;
        self.set_notice(tr!("{} folgen in der warteschlange", added));
    }

    pub fn forget_picked(&mut self) {
        let picked = std::mem::take(&mut self.podcasts.picked);
        for (episode, _) in &picked {
            self.forget_episode(episode);
        }
    }

    pub(in crate::gui) fn episode_menu(&self, at: usize) -> Vec<(&'static str, bool)> {
        let archived = self.config.ui.podcast_show_archived;
        let (list, _) = self.podcasts.episodes(archived);
        let Some((episode, _)) = list.get(at) else { return Vec::new() };
        if self.podcasts.picked.len() > 1 && self.podcasts.picked.iter().any(|(e, _)| e.id == episode.id) {
            let all_archived = self.podcasts.picked.iter().all(|(e, _)| self.podcasts.kept.is_archived(&e.id));
            let all_starred = self.podcasts.picked.iter().all(|(e, _)| self.podcasts.kept.is_starred(&e.id));
            let all_heard = self.podcasts.picked.iter().all(|(e, _)| self.podcasts.kept.left(&e.id).is_some_and(|l| l.done));
            return vec![
                (tr!("markierte in die warteschlange"), true),
                (if all_archived { tr!("markierte zurückholen") } else { tr!("markierte archivieren") }, true),
                (tr!("dateien der markierten löschen"), self.podcasts.picked.iter().any(|(e, _)| crate::podcast::cached(&e.url).is_file())),
                (if all_starred { tr!("markierte nicht mehr merken") } else { tr!("markierte merken") }, true),
                (if all_heard { tr!("markierte als ungehört") } else { tr!("markierte als gehört") }, true),
            ];
        }
        let kept = &self.podcasts.kept;
        let fetched = crate::podcast::cached(&episode.url).is_file();
        vec![
            (tr!("abspielen"), true),
            (if kept.is_queued(&episode.id) { tr!("aus der warteschlange") } else { tr!("in die warteschlange") }, true),
            (if kept.is_archived(&episode.id) { tr!("zurückholen") } else { tr!("archivieren") }, true),
            (tr!("ab hier archivieren"), true),
            (tr!("datei löschen"), fetched),
            (if kept.is_starred(&episode.id) { tr!("nicht mehr merken") } else { tr!("merken") }, true),
            (if kept.left(&episode.id).is_some_and(|l| l.done) { tr!("als ungehört markieren") } else { tr!("als gehört markieren") }, true),
        ]
    }

    pub(in crate::gui) fn episode_menu_chosen(&mut self, at: usize, chosen: usize) {
        let archived = self.config.ui.podcast_show_archived;
        let (list, _) = self.podcasts.episodes(archived);
        let Some((episode, show)) = list.get(at).cloned() else { return };
        if self.podcasts.picked.len() > 1 && self.podcasts.picked.iter().any(|(e, _)| e.id == episode.id) {
            match chosen {
                0 => self.queue_picked(),
                1 => self.archive_picked(),
                2 => self.forget_picked(),
                3 => self.star_picked(),
                _ => self.mark_picked_heard(),
            }
            return;
        }
        match chosen {
            0 => {
                self.choose_episode(episode.clone(), show.clone());
                self.fetch_and_play(episode, show, None);
            }
            1 => self.queue_episode(&episode, &show),
            2 => self.archive_episode(&episode),
            3 => self.archive_from(at),
            4 => self.delete_episode_file(&episode),
            5 => self.star_episode(&episode, &show),
            _ => {
                let heard = self.podcasts.kept.left(&episode.id).is_some_and(|l| l.done);
                self.mark_heard(&episode, !heard);
            }
        }
    }

    fn archive_from(&mut self, at: usize) {
        let archived = self.config.ui.podcast_show_archived;
        let mut all = self.podcasts.every_episode();
        if !archived {
            all.retain(|(e, _)| !self.podcasts.kept.is_archived(&e.id));
        }
        let mut kept = Kept::read();
        let mut newly: Vec<Episode> = Vec::new();
        for (episode, _) in all.into_iter().skip(at) {
            if !kept.is_archived(&episode.id) && !kept.is_starred(&episode.id) {
                kept.toggle_archived(&episode.id);
                newly.push(episode);
            }
        }
        kept.write();
        self.podcasts.kept = kept;
        self.podcasts.recount = true;
        self.podcasts.picked.clear();
        if self.config.ui.podcast_archive_forgets {
            for episode in &newly {
                self.forget_episode(episode);
            }
        }
        self.note(crate::gui::log::Kind::Podcast, tr!("{} folgen archiviert", newly.len()));
        self.set_notice(tr!("{} folgen archiviert", newly.len()));
    }

    pub(in crate::gui) fn show_playing_episode(&mut self) {
        let Some((episode, show)) = self.podcasts.playing_from.clone() else { return };
        if !self.showing_podcasts() {
            self.show_podcasts();
        }
        self.choose_podcast(None);
        self.choose_episode(episode, show);
    }

    fn looked_at_is_gone(&self) -> bool {
        let Some((episode, show)) = &self.podcasts.episode else { return false };
        let kept = &self.podcasts.kept;
        !kept.follows(&show.feed)
            && self.podcasts.chosen.as_ref().is_none_or(|c| c.feed != show.feed)
            && !crate::podcast::cached(&episode.url).is_file()
            && !kept.is_queued(&episode.id)
            && !kept.is_starred(&episode.id)
            && !self.playing_this_episode(&episode.id)
    }

    pub(in crate::gui) fn shows_in_order(&self) -> Vec<Show> {
        let mut shows = self.podcasts.kept.show.clone();
        match self.config.ui.podcast_order.as_str() {
            "a-z" => shows.sort_by_key(|s| s.title.to_lowercase()),
            "z-a" => shows.sort_by_key(|s| std::cmp::Reverse(s.title.to_lowercase())),
            _ => {
                let newest = |s: &Show| self.podcasts.feeds.get(&s.feed).map_or(0, |f| f.episodes.iter().map(crate::podcast::when).max().unwrap_or(0));
                shows.sort_by_cached_key(|s| std::cmp::Reverse(newest(s)));
            }
        }
        shows
    }

    pub(in crate::gui) fn turn_podcast_order(&mut self) {
        self.config.ui.podcast_order = match self.config.ui.podcast_order.as_str() {
            "zuletzt" => "a-z",
            "a-z" => "z-a",
            _ => "zuletzt",
        }
        .to_string();
        self.save_config();
    }

    pub fn choose_open(&mut self) {
        self.podcasts.chosen = None;
        self.podcasts.only_fetched = false;
        self.podcasts.only_starred = false;
        self.podcasts.only_open = true;
        self.podcasts.more = 0;
        self.podcasts.picked.clear();
    }

    pub fn share_episode(&mut self, ctx: &egui::Context, episode: &Episode, show: &Show) {
        let page = self
            .podcasts
            .feeds
            .get(&show.feed)
            .and_then(|feed| feed.episodes.iter().find(|e| e.id == episode.id))
            .map(|e| e.link.clone())
            .filter(|l| !l.is_empty())
            .unwrap_or_else(|| episode.link.clone());
        let address = if page.is_empty() { episode.url.clone() } else { page };
        ctx.copy_text(format!("„{}“ — {}\n{}", episode.title, show.title, address));
        self.set_status(tr!("link kopiert"));
    }

    pub fn choose_starred(&mut self) {
        self.podcasts.chosen = None;
        self.podcasts.only_fetched = false;
        self.podcasts.only_starred = true;
        self.podcasts.only_open = false;
        self.podcasts.more = 0;
        self.podcasts.picked.clear();
    }

    pub fn star_episode(&mut self, episode: &Episode, show: &Show) {
        let mut kept = Kept::read();
        let now = kept.toggle_starred(episode, show);
        kept.write();
        self.podcasts.kept = kept;
        self.set_notice(if now { tr!("gemerkt: {}", episode.title) } else { tr!("nicht mehr gemerkt: {}", episode.title) });
    }

    pub fn star_picked(&mut self) {
        let picked = std::mem::take(&mut self.podcasts.picked);
        let mut kept = Kept::read();
        let all_in = picked.iter().all(|(e, _)| kept.is_starred(&e.id));
        for (episode, show) in &picked {
            if kept.is_starred(&episode.id) == all_in {
                kept.toggle_starred(episode, show);
            }
        }
        kept.write();
        self.podcasts.kept = kept;
    }

    pub fn mark_heard(&mut self, episode: &Episode, heard: bool) {
        let mut kept = Kept::read();
        if heard {
            kept.heard(&episode.id);
        } else {
            kept.unheard(&episode.id);
        }
        kept.write();
        self.podcasts.kept = kept;
        self.podcasts.recount = true;
    }

    pub fn mark_picked_heard(&mut self) {
        let picked = std::mem::take(&mut self.podcasts.picked);
        let mut kept = Kept::read();
        let all_heard = picked.iter().all(|(e, _)| kept.left(&e.id).is_some_and(|l| l.done));
        for (episode, _) in &picked {
            if all_heard {
                kept.unheard(&episode.id);
            } else {
                kept.heard(&episode.id);
            }
        }
        kept.write();
        self.podcasts.kept = kept;
        self.podcasts.recount = true;
    }

    pub fn step_sleep(&mut self) {
        let minutes = |m: u64| Sleep::At(Instant::now() + std::time::Duration::from_secs(m * 60), m);
        self.podcasts.sleep = match self.podcasts.sleep {
            None => Some(minutes(15)),
            Some(Sleep::At(_, 15)) => Some(minutes(30)),
            Some(Sleep::At(_, 30)) => Some(minutes(60)),
            Some(Sleep::At(..)) => Some(Sleep::EpisodeEnd),
            Some(Sleep::EpisodeEnd) => None,
        };
        if let Some(from) = self.podcasts.sleep_fade_from.take() {
            self.player.set_volume(from);
        }
    }

    pub fn sleep_longer(&mut self) {
        let five = std::time::Duration::from_secs(5 * 60);
        self.podcasts.sleep = match self.podcasts.sleep {
            Some(Sleep::At(at, chosen)) => Some(Sleep::At(at.max(Instant::now()) + five, chosen)),
            Some(Sleep::EpisodeEnd) => {
                let left = self.player.duration().saturating_sub(self.player.position());
                Some(Sleep::At(Instant::now() + left + five, 0))
            }
            None => None,
        };
        if let Some(from) = self.podcasts.sleep_fade_from.take() {
            self.player.set_volume(from);
        }
    }

    pub fn sleep_off(&mut self) {
        self.podcasts.sleep = None;
        if let Some(from) = self.podcasts.sleep_fade_from.take() {
            self.player.set_volume(from);
        }
    }

    pub fn sleep_said(&self) -> String {
        match self.podcasts.sleep {
            None => tr!("einschlafen: aus").to_string(),
            Some(Sleep::EpisodeEnd) => tr!("einschlafen: am ende der folge").to_string(),
            Some(Sleep::At(at, _)) => {
                let left = at.saturating_duration_since(Instant::now()).as_secs();
                tr!("einschlafen in {}", text_minutes(left))
            }
        }
    }

    pub fn sleep_standing(&self) -> Option<String> {
        self.podcasts.sleep.as_ref().map(|_| self.sleep_said())
    }

    fn tend_sleep(&mut self, ctx: &egui::Context) {
        if self.podcasts.sleep.is_some() && self.podcasts.playing.is_none() && !self.playing_episode() && self.podcasts.fetching.is_none() {
            self.podcasts.sleep = None;
            if let Some(from) = self.podcasts.sleep_fade_from.take() {
                self.player.set_volume(from);
            }
            return;
        }
        let Some(Sleep::At(at, _)) = self.podcasts.sleep else { return };
        let left = at.saturating_duration_since(Instant::now()).as_secs_f32();
        if left <= SLEEP_FADE_SECS {
            let from = *self.podcasts.sleep_fade_from.get_or_insert(self.player.volume());
            if left <= 0.0 {
                if self.player.state() == crate::player::PlaybackState::Playing {
                    self.player.toggle_pause();
                }
                self.player.set_volume(from);
                self.podcasts.sleep = None;
                self.podcasts.sleep_fade_from = None;
                self.set_notice(tr!("eingeschlafen"));
                return;
            }
            self.player.set_volume((f32::from(from) * left / SLEEP_FADE_SECS).round() as u8);
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        } else {
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
        }
    }

    pub(in crate::gui) fn forget_archived(&mut self) {
        let kept = Kept::read();
        let playing = self.podcasts.playing.as_ref().map(|(id, _)| id.clone());
        for fetched in &kept.fetched {
            if kept.is_archived(&fetched.id) && playing.as_deref() != Some(fetched.id.as_str()) {
                crate::podcast::forget(&crate::podcast::cached(&fetched.url));
            }
        }
    }

    pub fn queue_episode(&mut self, episode: &Episode, show: &Show) {
        if self.playing_this_episode(&episode.id) && !self.podcasts.kept.is_queued(&episode.id) {
            self.set_notice(tr!("läuft gerade: {}", episode.title));
            return;
        }
        let mut kept = Kept::read();
        let now = kept.toggle_queued(episode, show);
        kept.write();
        self.podcasts.kept = kept;
        self.set_notice(if now { tr!("in der warteschlange: {}", episode.title) } else { tr!("aus der warteschlange: {}", episode.title) });
    }

    pub(in crate::gui) fn drop_episodes(&mut self, at: usize) {
        let Some(carried) = self.dragging.take() else { return };
        let mut kept = Kept::read();
        match carried {
            crate::gui::Dragged::QueuedEpisode(from) => {
                if from >= kept.queue.len() {
                    return;
                }
                let to = if at > from { at - 1 } else { at };
                let row = kept.queue.remove(from);
                kept.queue.insert(to.min(kept.queue.len()), row);
            }
            crate::gui::Dragged::Episodes(episodes) => {
                let mut at = at.min(kept.queue.len());
                let mut added = 0;
                for (episode, show) in episodes {
                    if self.playing_this_episode(&episode.id) {
                        continue;
                    }
                    kept.spent.retain(|s| s.id != episode.id);
                    if let Some(was) = kept.queue.iter().position(|q| q.id == episode.id) {
                        kept.queue.remove(was);
                        if was < at {
                            at -= 1;
                        }
                    }
                    kept.queue.insert(at.min(kept.queue.len()), crate::podcast::Fetched::of(&episode, &show));
                    at += 1;
                    added += 1;
                }
                self.set_notice(match added {
                    1 => tr!("1 folge eingereiht").to_string(),
                    n => tr!("{} folgen eingereiht", n),
                });
                if added > 0 {
                    self.log_open = false;
                    self.eq_open = false;
                    self.podcasts.info_open = false;
                    if !self.config.ui.podcast_queue {
                        self.config.ui.podcast_queue = true;
                        self.save_config();
                    }
                }
            }
            _ => return,
        }
        kept.write();
        self.podcasts.kept = kept;
    }

    pub fn carry_queued_episode(&mut self, id: &str, by: i32) {
        let mut kept = Kept::read();
        let Some(from) = kept.queue.iter().position(|q| q.id == id) else { return };
        let to = (from as i64 + i64::from(by)).clamp(0, kept.queue.len() as i64 - 1) as usize;
        if to == from {
            return;
        }
        let row = kept.queue.remove(from);
        kept.queue.insert(to, row);
        kept.write();
        self.podcasts.kept = kept;
    }

    pub fn carry_queued_episodes(&mut self, ids: &[String], by: i32) {
        let mut kept = Kept::read();
        let mut moved = false;
        for _ in 0..by.unsigned_abs() {
            let mut at: Vec<usize> = kept.queue.iter().enumerate().filter(|(_, q)| ids.contains(&q.id)).map(|(i, _)| i).collect();
            if at.is_empty() {
                return;
            }
            if by > 0 {
                if at.last().is_some_and(|&last| last + 1 >= kept.queue.len()) {
                    break;
                }
                at.reverse();
                for i in at {
                    kept.queue.swap(i, i + 1);
                }
            } else {
                if at.first() == Some(&0) {
                    break;
                }
                for i in at {
                    kept.queue.swap(i, i - 1);
                }
            }
            moved = true;
        }
        if moved {
            kept.write();
            self.podcasts.kept = kept;
        }
    }

    pub fn forget_spent(&mut self, id: &str) {
        let mut kept = Kept::read();
        kept.spent.retain(|s| s.id != id);
        kept.write();
        self.podcasts.kept = kept;
    }

    pub fn clear_episode_queue(&mut self) {
        let mut kept = Kept::read();
        kept.queue.clear();
        kept.spent.clear();
        kept.write();
        self.podcasts.kept = kept;
    }

    fn play_next_queued(&mut self) -> bool {
        let Some((episode, show)) = Kept::read().queue.first().map(crate::podcast::Fetched::episode) else {
            return false;
        };
        self.fetch_and_play(episode, show, None);
        true
    }

    pub fn play_queued_episode(&mut self, episode: Episode, show: Show) {
        self.choose_episode(episode.clone(), show.clone());
        self.fetch_and_play(episode, show, None);
    }

    pub fn follow_podcast(&mut self, show: &Show) {
        let mut kept = Kept::read();
        let now = kept.toggle(show);
        kept.write();
        self.podcasts.kept = kept;
        if now {
            self.read_feed(show);
        } else {
            let _ = std::fs::remove_file(crate::podcast::cached_feed_path(&show.feed));
        }
        let said = if now { tr!("{} gefolgt", show.title) } else { tr!("{} nicht mehr gefolgt", show.title) };
        self.note(crate::gui::log::Kind::Podcast, said.clone());
        self.set_notice(said);
    }

    pub fn choose_episode(&mut self, episode: Episode, show: Show) {
        if !episode.chapters.is_empty() {
            self.podcasts.chapters = Some((episode.id.clone(), episode.chapters.clone()));
            self.podcasts.chapters_reading = None;
        } else if !episode.chapters_url.is_empty() {
            let known = self.podcasts.chapters.as_ref().is_some_and(|(id, _)| *id == episode.id);
            if !known {
                self.podcasts.chapters = None;
                self.podcasts.chapters_reading = Some((episode.id.clone(), crate::podcast::chapters_later(&episode.chapters_url)));
            }
        } else {
            let file = crate::podcast::cached(&episode.url);
            let in_file = if file.is_file() { crate::podcast::chapters_in_file(&file) } else { Vec::new() };
            self.podcasts.chapters = (!in_file.is_empty()).then(|| (episode.id.clone(), in_file));
            self.podcasts.chapters_reading = None;
        }
        self.podcasts.episode = Some((episode, show));
    }

    pub fn fetch_and_play(&mut self, episode: Episode, show: Show, at: Option<f64>) {
        if let Some((_, id)) = self.podcasts.streaming.take() {
            if self.playing_this_episode(&id) {
                self.keep_episode_place();
                self.podcasts.playing = None;
                if self.player.state() == crate::player::PlaybackState::Playing {
                    self.player.toggle_pause();
                }
            }
        }
        let rx = crate::podcast::fetch_episode(&episode.url, episode.bytes);
        let there = crate::podcast::cached(&episode.url).is_file();
        crate::player::watch::note(format!(
            "folge laden: {} — {}{}",
            show.title,
            episode.title,
            if there {
                " · schon da".to_string()
            } else if episode.bytes > 0 {
                format!(" · {:.0} mb", episode.bytes as f64 / 1e6)
            } else {
                String::new()
            }
        ));
        self.podcasts.fetch_began = Some(Instant::now());
        self.podcasts.start_at = at;
        self.podcasts.fetch_asked_over = Some(self.sounding_now());
        self.podcasts.fetching = Some((episode, show, rx, (0, 0)));
    }

    fn sounding_now(&self) -> (u8, bool) {
        let kind = if self.player.on_air().is_some() {
            2
        } else if self.playing_episode() {
            3
        } else if self.player.library_track().is_some() {
            1
        } else {
            0
        };
        (kind, self.player.state() == crate::player::PlaybackState::Playing)
    }

    fn something_else_put_on(&self) -> bool {
        let now = self.sounding_now();
        let other = now.1 && now.0 != 3;
        other && self.podcasts.fetch_asked_over.as_ref().is_some_and(|before| *before != now)
    }

    pub fn playing_this_episode(&self, id: &str) -> bool {
        self.playing_episode() && self.podcasts.playing.as_ref().is_some_and(|(playing, _)| playing == id)
    }

    pub fn play_chapter(&mut self, episode: Episode, show: Show, start: f64) {
        if self.playing_this_episode(&episode.id) {
            self.player.seek_to(start);
            if self.player.state() != crate::player::PlaybackState::Playing {
                self.player.toggle_pause();
            }
            return;
        }
        self.fetch_and_play(episode, show, Some(start));
    }

    fn play_episode(&mut self, episode: &Episode, show: &Show, path: std::path::PathBuf, growing: Option<GrowingDecoder>) {
        let played_before = self.podcasts.playing_from.clone();
        if let Some((before, file)) = self.podcasts.playing.take() {
            let kept = Kept::read();
            if before != episode.id && self.config.ui.podcast_keep == "gehört" && kept.left(&before).is_some_and(|l| l.done) && !kept.is_starred(&before) {
                crate::podcast::forget(&file);
            }
        }
        let now = chrono::Local::now().naive_local();
        let track = Track {
            id: EPISODE_ID,
            path: path.clone(),
            title: episode.title.clone(),
            artist: if show.author.is_empty() { show.title.clone() } else { show.author.clone() },
            album_artist: String::new(),
            album: show.title.clone(),
            genre: "podcast".into(),
            year: None,
            track_number: None,
            disc_number: None,
            duration_secs: episode.seconds.unwrap_or(0) as f64,
            bitrate: None,
            sample_rate: None,
            file_size: std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
            format: "podcast".into(),
            rating: 0,
            play_count: 0,
            last_played: None,
            date_added: now,
            date_modified: now,
            file_hash: None,
            has_artwork: false,
        };
        let streaming = growing.is_some();
        crate::player::watch::note(format!(
            "folge: {} — {} · {} · {} · {}",
            show.title,
            episode.title,
            episode.url,
            if streaming { "spielt beim laden" } else { "ganz geladen" },
            path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
        ));
        match growing {
            Some(source) => self.player.play_aside_growing(track, source),
            None => {
                if let Err(e) = self.player.play_aside(track) {
                    self.set_notice(e.to_string());
                    return;
                }
                let left = Kept::read().left(&episode.id).filter(|l| !l.done && l.at > 0.0).map(|l| l.at);
                self.podcasts.seek_once_playing = self.podcasts.start_at.take().or(left).map(|at| (at, Instant::now()));
            }
        }
        let tagged = if streaming { path.with_extension("part") } else { path.clone() };
        self.podcasts.playing_chapters = if !episode.chapters.is_empty() {
            episode.chapters.clone()
        } else if let Some((_, known)) = self.podcasts.chapters.as_ref().filter(|(id, _)| *id == episode.id) {
            known.clone()
        } else {
            let in_file = crate::podcast::chapters_in_file(&tagged);
            let looked = self.podcasts.episode.as_ref().is_some_and(|(e, _)| e.id == episode.id);
            if looked && !in_file.is_empty() && self.podcasts.chapters_reading.is_none() {
                self.podcasts.chapters = Some((episode.id.clone(), in_file.clone()));
            }
            in_file
        };
        self.podcasts.playing = Some((episode.id.clone(), path));
        self.podcasts.playing_from = Some((episode.clone(), show.clone()));
        self.podcasts.last_place = (0.0, 0.0);
        self.podcasts.kept_at = Some(Instant::now());
        crate::podcast::fetch_episode_art(&episode.artwork);
        let mut kept = Kept::read();
        let was = (kept.queue.clone(), kept.spent.clone());
        let before = played_before.filter(|(e, _)| e.id != episode.id);
        kept.go_to(episode, before.as_ref().map(|(e, s)| crate::podcast::Fetched::of(e, s)));
        let moved = (kept.queue.clone(), kept.spent.clone()) != was;
        let unbegun = kept.left(&episode.id).is_none();
        if unbegun {
            kept.leave(&episode.id, 0.0, 0.0);
        }
        if moved || unbegun {
            kept.write();
            self.podcasts.kept = kept;
            self.podcasts.recount = true;
        }
    }

    fn stream_what_comes_in(&mut self) {
        let Some((episode, show, _, (have, of))) = &self.podcasts.fetching else { return };
        let (episode, show, have, of) = (episode.clone(), show.clone(), *have, *of);
        if self.podcasts.streaming.as_ref().is_some_and(|(_, id)| *id == episode.id) {
            if self.playing_this_episode(&episode.id) {
                let length = self.player.duration().as_secs_f64();
                let reach = if of > 0 { length * (have as f64 / of as f64).min(1.0) } else { 0.0 };
                self.player.set_reach(Some(reach));
            }
            return;
        }
        let from_start = self.podcasts.start_at.is_none() && !Kept::read().left(&episode.id).is_some_and(|l| !l.done && l.at > 0.0);
        if !from_start || have < STREAM_FROM_BYTES || self.something_else_put_on() {
            return;
        }
        let path = crate::podcast::cached(&episode.url);
        let kind = path.extension().and_then(|e| e.to_str()).unwrap_or("mp3").to_string();
        let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let Ok(source) = crate::player::growing::open(&path.with_extension("part"), std::sync::Arc::clone(&done), of, &kind) else {
            self.podcasts.streaming = Some((done, String::new()));
            return;
        };
        self.keep_episode_place();
        self.podcasts.streaming = Some((done, episode.id.clone()));
        self.play_episode(&episode, &show, path, Some(source));
    }

    pub fn playing_chapter_starts(&self) -> Vec<f32> {
        if !self.playing_episode() || self.podcasts.playing.is_none() {
            return Vec::new();
        }
        let length = self.player.duration().as_secs_f64();
        if length <= 0.0 {
            return Vec::new();
        }
        self.podcasts.playing_chapters.iter().map(|c| (c.start / length).clamp(0.0, 1.0) as f32).collect()
    }

    pub fn playing_episode(&self) -> bool {
        self.player.current_track().is_some_and(|t| t.id == EPISODE_ID)
    }

    pub(in crate::gui) fn tend_podcasts(&mut self, ctx: &egui::Context) {
        self.take_podcast_import();
        self.tend_sleep(ctx);
        if self.podcasts.open && self.looked_at_is_gone() {
            self.podcasts.episode = None;
            self.podcasts.chapters = None;
        }
        if let (true, Some(every)) = (self.config.ui.podcasts, self.check_every()) {
            match self.podcasts.checked_at {
                Some(at) if at.elapsed().as_secs() < every => {
                    ctx.request_repaint_after(std::time::Duration::from_secs(every.saturating_sub(at.elapsed().as_secs()) + 1));
                }
                _ => self.check_for_episodes(),
            }
        }
        if self.podcasts.recount {
            let before = self.podcasts.new_by_show.clone();
            self.podcasts.count_new();
            let grown: Vec<String> = self
                .podcasts
                .new_by_show
                .iter()
                .filter(|(feed, n)| before.get(*feed).is_none_or(|was| *n > was))
                .filter_map(|(feed, n)| {
                    let was = before.get(feed).copied().unwrap_or(0);
                    let title = self.podcasts.kept.show.iter().find(|s| s.feed == *feed)?.title.clone();
                    Some(format!("{title} ({})", n - was))
                })
                .collect();
            if !grown.is_empty() {
                self.note(crate::gui::log::Kind::Podcast, tr!("neue folgen: {}", grown.join(", ")));
            }
        }
        if !self.podcasts.open && self.podcasts.playing.is_none() && self.podcasts.fetching.is_none() && self.podcasts.reading.is_empty() {
            return;
        }
        if let Some(at) = self.podcasts.typed_at {
            if at.elapsed().as_secs_f32() >= TYPED_SECS {
                self.podcasts.typed_at = None;
                match self.query() {
                    Some(term) if crate::podcast::is_address(&term) => {
                        let url = term.trim().replacen("feed://", "https://", 1);
                        self.podcasts.searching = Some(crate::podcast::show_at_later(&url));
                    }
                    Some(term) => self.podcasts.searching = Some(crate::podcast::search_later(&term)),
                    None => {
                        self.podcasts.found = None;
                        self.podcasts.searching = None;
                    }
                }
            }
        }
        if let Some(rx) = &self.podcasts.searching {
            match rx.try_recv() {
                Ok(Ok(found)) => {
                    self.podcasts.found = Some(found);
                    self.podcasts.searching = None;
                }
                Ok(Err(e)) => {
                    self.set_notice(e);
                    self.podcasts.searching = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.podcasts.searching = None,
            }
        }
        let mut read: Vec<(String, std::result::Result<crate::podcast::Reading, String>)> = Vec::new();
        self.podcasts.reading.retain(|(url, rx)| match rx.try_recv() {
            Ok(Ok(cached @ crate::podcast::Reading::Cached { .. })) => {
                read.push((url.clone(), Ok(cached)));
                true
            }
            Ok(answer) => {
                read.push((url.clone(), answer));
                false
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => true,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => false,
        });
        for (url, answer) in read {
            match answer {
                Ok(crate::podcast::Reading::Unchanged) => {}
                Ok(crate::podcast::Reading::Cached { feed, validators }) => {
                    if !self.podcasts.feeds.contains_key(&url) {
                        self.podcasts.validators.insert(url.clone(), validators);
                        self.podcasts.feeds.insert(url, feed);
                        self.podcasts.feeds_version += 1;
                        self.podcasts.recount = true;
                    }
                }
                Ok(crate::podcast::Reading::Fresh { feed, validators, moved }) => {
                    let url = match moved {
                        Some(to) if to != url => self.feed_moved(&url, &to),
                        _ => url,
                    };
                    let show = self.podcasts.kept.show.iter().chain(self.podcasts.chosen.iter()).find(|s| s.feed == url).cloned();
                    if let Some(show) = show {
                        crate::podcast::fetch_artwork(&show, &feed.artwork);
                    }
                    let mut feed = feed;
                    let kept = &self.podcasts.kept;
                    let whole = self.podcasts.whole.as_deref() == Some(url.as_str());
                    let mut on_disk = if whole { feed.clone() } else { Feed::default() };
                    if whole {
                        on_disk.keep_newest(|e| kept.is_begun(&e.id));
                    } else {
                        feed.keep_newest(|e| kept.is_begun(&e.id));
                        on_disk = feed.clone();
                    }
                    if self.podcasts.kept.follows(&url) {
                        crate::podcast::write_cached_feed(&url, on_disk, validators.clone());
                    }
                    self.podcasts.validators.insert(url.clone(), validators);
                    self.podcasts.feeds.insert(url, feed);
                    self.podcasts.feeds_version += 1;
                    self.podcasts.recount = true;
                }
                Err(e) => {
                    tracing::warn!("podcast feed {url}: {e}");
                    let name = self.podcasts.kept.show.iter().chain(self.podcasts.chosen.iter()).find(|s| s.feed == url).map_or(url.clone(), |s| s.title.clone());
                    let said = tr!("{} ist gerade nicht zu erreichen", name);
                    self.note(crate::gui::log::Kind::Podcast, format!("{said} ({e})"));
                    if self.podcasts.open {
                        self.podcasts.unreached.push(name);
                    }
                }
            }
        }
        if self.podcasts.reading.is_empty() {
            if let Some(began) = self.podcasts.round_began.take() {
                crate::player::watch::note(format!("neue folgen suchen: fertig in {:.1} s · {} neu", began.elapsed().as_secs_f32(), self.podcasts.new_total()));
            }
        }
        if self.podcasts.reading.is_empty() && !self.podcasts.unreached.is_empty() {
            let unreached = std::mem::take(&mut self.podcasts.unreached);
            let said = match unreached.as_slice() {
                [one] => tr!("{} ist gerade nicht zu erreichen", one),
                many => tr!("{} podcasts sind gerade nicht zu erreichen", many.len()),
            };
            self.set_notice(said);
        }
        if let Some((id, rx)) = &self.podcasts.chapters_reading {
            match rx.try_recv() {
                Ok(chapters) => {
                    if self.podcasts.playing.as_ref().is_some_and(|(playing, _)| playing == id) {
                        self.podcasts.playing_chapters = chapters.clone();
                    }
                    self.podcasts.chapters = Some((id.clone(), chapters));
                    self.podcasts.chapters_reading = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.podcasts.chapters_reading = None,
            }
        }
        let mut arrived: Option<(Episode, Show, std::path::PathBuf)> = None;
        if let Some((episode, show, rx, progress)) = &mut self.podcasts.fetching {
            let mut ended: Option<std::result::Result<std::path::PathBuf, String>> = None;
            while let Ok(said) = rx.try_recv() {
                match said {
                    Fetching::Under(have, of) => *progress = (have, of),
                    Fetching::Done(path) => ended = Some(Ok(path)),
                    Fetching::Failed(why) => ended = Some(Err(why)),
                }
            }
            match ended {
                Some(Ok(path)) => arrived = Some((episode.clone(), show.clone(), path)),
                Some(Err(why)) => {
                    tracing::warn!("podcast episode {}: {why}", episode.url);
                    let said = tr!("„{}“ lässt sich gerade nicht laden", episode.title);
                    self.note(crate::gui::log::Kind::Podcast, format!("{said} ({why})"));
                    self.set_notice(said);
                    self.podcasts.fetching = None;
                    if self.podcasts.playing.is_none() && self.playing_episode() {
                        self.player.stop();
                    }
                    if let Some((done, id)) = self.podcasts.streaming.take() {
                        done.store(true, std::sync::atomic::Ordering::Release);
                        if self.podcasts.playing.as_ref().is_some_and(|(playing, _)| *playing == id) {
                            self.podcasts.playing = None;
                            self.player.stop();
                        }
                    }
                }
                None => {}
            }
        }
        self.stream_what_comes_in();
        if let Some((done, _)) = &self.podcasts.streaming {
            if arrived.is_some() {
                done.store(true, std::sync::atomic::Ordering::Release);
            }
        }
        if let Some((_, _, path)) = &arrived {
            let took = self.podcasts.fetch_began.take().map_or(0.0, |at| at.elapsed().as_secs_f32());
            let size = std::fs::metadata(path).map_or(0, |m| m.len());
            crate::player::watch::note(format!("folge geladen: {:.0} mb in {took:.1} s", size as f64 / 1e6));
        }
        let streamed = arrived.as_ref().is_some_and(|(e, ..)| self.podcasts.streaming.as_ref().is_some_and(|(_, id)| *id == e.id));
        if streamed {
            let (episode, show, _) = arrived.take().expect("arrived");
            self.podcasts.fetching = None;
            self.podcasts.streaming = None;
            self.player.set_reach(None);
            let mut kept = Kept::read();
            kept.fetched_now(&episode, &show);
            self.note(crate::gui::log::Kind::Podcast, tr!("geladen: {} — {}", show.title, episode.title));
            kept.write();
            self.podcasts.kept = kept;
        }
        if let Some((episode, show, path)) = arrived {
            self.podcasts.fetching = None;
            self.podcasts.streaming = None;
            let mut kept = Kept::read();
            kept.fetched_now(&episode, &show);
            self.note(crate::gui::log::Kind::Podcast, tr!("geladen: {} — {}", show.title, episode.title));
            kept.write();
            self.podcasts.kept = kept;
            if self.something_else_put_on() {
                self.set_notice(tr!("geladen: {}", episode.title));
            } else {
                self.keep_episode_place();
                self.play_episode(&episode, &show, path, None);
            }
            self.podcasts.fetch_asked_over = None;
        }
        if let Some((at, since)) = self.podcasts.seek_once_playing {
            let playing = self.playing_episode() && self.player.state() == crate::player::PlaybackState::Playing;
            if (playing && self.player.seek_to(at)) || since.elapsed().as_secs() >= SEEK_GIVE_UP_SECS {
                self.podcasts.seek_once_playing = None;
            }
        }
        let still_playing = self.podcasts.playing.as_ref().is_some_and(|(_, path)| {
            self.player.current_track().is_some_and(|t| t.id == EPISODE_ID && &t.path == path)
        });
        if self.podcasts.playing.is_some() {
            let due = self.podcasts.kept_at.is_none_or(|at| at.elapsed().as_secs() >= KEEP_EVERY_SECS);
            if !still_playing {
                let ended = self.player.take_aside_ended().is_some_and(|ended| self.podcasts.playing.as_ref().is_some_and(|(_, path)| *path == ended));
                if let Some((id, path)) = self.podcasts.playing.take() {
                    let mut kept = Kept::read();
                    let elsewhere = !ended && !self.playing_episode();
                    if elsewhere {
                        let (at, length) = self.podcasts.last_place;
                        if at > 0.0 {
                            kept.leave(&id, at, length);
                        }
                        let heard = kept.left(&id).is_some_and(|l| l.done);
                        if let Some((episode, show)) = self.podcasts.playing_from.take().filter(|(e, _)| e.id == id) {
                            if !heard && !kept.is_archived(&id) {
                                kept.queue.retain(|q| q.id != id);
                                kept.queue.insert(0, crate::podcast::Fetched::of(&episode, &show));
                            }
                        }
                        kept.write();
                        self.podcasts.kept = kept.clone();
                        self.podcasts.recount = true;
                    }
                    if ended {
                        kept.heard(&id);
                        kept.write();
                        self.podcasts.kept = kept.clone();
                        self.podcasts.recount = true;
                    }
                    if kept.left(&id).is_some_and(|l| l.done) && self.config.ui.podcast_keep == "gehört" && !kept.is_starred(&id) {
                        crate::podcast::forget(&path);
                    }
                }
                let sleeping = ended && matches!(self.podcasts.sleep, Some(Sleep::EpisodeEnd));
                if sleeping {
                    self.podcasts.sleep = None;
                }
                if ended && !sleeping && self.podcasts.fetching.is_none() {
                    self.play_next_queued();
                }
            } else {
                self.podcasts.last_place = (self.player.position().as_secs_f64(), self.player.duration().as_secs_f64());
                if due {
                    self.keep_episode_place();
                }
            }
        }
        if self.podcasts.waiting() {
            let p = &self.podcasts;
            let streaming = p.fetching.as_ref().is_some_and(|(e, ..)| p.streaming.as_ref().is_some_and(|(_, id)| *id == e.id));
            let only_that = streaming
                && p.searching.is_none()
                && p.reading.is_empty()
                && p.typed_at.is_none()
                && p.chapters_reading.is_none()
                && p.seek_once_playing.is_none()
                && p.import.is_none();
            let every = if only_that { 1000 } else { 150 };
            ctx.request_repaint_after(std::time::Duration::from_millis(every));
        }
    }

    fn keep_episode_place(&mut self) {
        let Some((id, _)) = self.podcasts.playing.clone() else { return };
        if !self.playing_episode() {
            return;
        }
        let at = self.player.position().as_secs_f64();
        let length = self.player.duration().as_secs_f64();
        let mut kept = Kept::read();
        let first = kept.left(&id).is_none();
        kept.leave(&id, at, length);
        kept.write_soon();
        self.podcasts.kept = kept;
        self.podcasts.kept_at = Some(Instant::now());
        if first {
            self.podcasts.recount = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_comes_a_page_at_a_time() {
        let mut panel = PodcastPanel::default();
        for (n, name) in ["a", "b"].into_iter().enumerate() {
            let show = Show { title: name.into(), feed: format!("https://x/{name}"), ..Default::default() };
            let episodes = (0..50)
                .map(|i| Episode {
                    id: format!("{name}{i}"),
                    url: format!("https://x/{name}{i}.mp3"),
                    published: chrono::DateTime::from_timestamp(2_000_000_000 - i * 86_400 + n as i64 * 60, 0).unwrap().to_rfc2822(),
                    ..Default::default()
                })
                .collect();
            panel.feeds.insert(show.feed.clone(), Feed { episodes, ..Default::default() });
            panel.kept.show.push(show);
        }
        let (list, total) = panel.episodes(true);
        assert_eq!((list.len(), total), (EPISODES_PAGE, 100));
        assert_eq!(list[0].0.id, "b0");
        assert_eq!(list[1].0.id, "a0");
        panel.more += EPISODES_PAGE;
        assert_eq!(panel.episodes(true).0.len(), 100);
        let mut many = PodcastPanel::default();
        for p in 0..200 {
            let show = Show { title: format!("p{p}"), feed: format!("https://x/{p}"), ..Default::default() };
            let episodes = (0..100)
                .map(|i| Episode {
                    id: format!("{p}-{i}"),
                    about: "notes ".repeat(400),
                    at: 2_000_000_000 - i * 86_400 - p,
                    ..Default::default()
                })
                .collect();
            many.feeds.insert(show.feed.clone(), Feed { episodes, ..Default::default() });
            many.kept.show.push(show);
        }
        many.kept.archived = (0..2_000).map(|i| format!("0-{i}")).collect();
        let started = std::time::Instant::now();
        let (page, total) = many.episodes(false);
        assert_eq!((page.len(), total), (EPISODES_PAGE, 20_000 - 100));
        eprintln!("many: {:?}", started.elapsed());
        let again = std::time::Instant::now();
        assert_eq!(many.episodes(false).1, 20_000 - 100);
        eprintln!("many again: {:?}", again.elapsed());
        assert!(again.elapsed() < std::time::Duration::from_millis(20), "{:?}", again.elapsed());
        assert!(started.elapsed() < std::time::Duration::from_millis(200), "{:?}", started.elapsed());
        panel.kept.toggle_archived("b0");
        assert_eq!(panel.episodes(false).1, 99);
        assert_eq!(panel.episodes(false).0[0].0.id, "a0");
        assert_eq!(panel.episodes(true).1, 100);
    }
}
