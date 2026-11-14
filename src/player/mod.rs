pub mod ahead;
pub mod audio;
pub mod eq;
pub mod gapless;
pub mod growing;
pub mod hls;
pub mod leveling;
pub mod queue;
pub mod remembered;
pub mod stream;
pub mod watch;

use std::path::PathBuf;
use std::time::Duration;

use crate::db::models::{RepeatMode, Track};
use crate::error::{KesslerError, Result};

use audio::AudioEngine;
use queue::PlayQueue;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
}

pub struct Player {
    engine: AudioEngine,
    pub queue: PlayQueue,
    pub library_hidden: bool,
    state: PlaybackState,
    volume: u8,
    gone: Vec<PathBuf>,
    gapless: bool,
    on_air: Option<OnAir>,
    aside: Option<Track>,
    aside_ended: Option<PathBuf>,
    reach: Option<f64>,
}

struct OnAir {
    url: String,
    said: stream::Said,
    introduced: stream::Introduced,
    opening: Option<Opening>,
    again_at: Option<std::time::Instant>,
    tries: u32,
    since: Option<std::time::Instant>,
}

type Opening = std::sync::mpsc::Receiver<std::result::Result<rodio::Decoder<std::io::BufReader<stream::Stream>>, String>>;

const RETRIES: u32 = 4;
const RETRY_AFTER: std::time::Duration = std::time::Duration::from_secs(2);

const REACH_MARGIN_SECS: f64 = 5.0;

const HELD_FOR: std::time::Duration = std::time::Duration::from_secs(30);

fn open_stream(url: &str, said: &stream::Said, introduced: &stream::Introduced) -> Opening {
    let (tell, opened) = std::sync::mpsc::channel();
    let url = url.to_string();
    let said = said.clone();
    let introduced = introduced.clone();
    std::thread::Builder::new()
        .name("kessler-radio".into())
        .spawn(move || {
            let _ = tell.send(stream::open(&url, said, introduced));
        })
        .ok();
    opened
}

impl Player {
    pub fn leveling(&self) -> std::sync::Arc<leveling::Leveling> {
        self.engine.leveling()
    }

    pub fn new(default_volume: u8) -> Result<Self> {
        let engine = AudioEngine::new()?;
        let volume = default_volume.min(100);
        engine.set_volume(volume as f32 / 100.0);

        Ok(Self {
            engine,
            queue: PlayQueue::new(),
            state: PlaybackState::Stopped,
            volume,
            gone: Vec::new(),
            gapless: true,
            on_air: None,
            aside: None,
            library_hidden: false,
            aside_ended: None,
            reach: None,
        })
    }

    pub fn set_gapless(&mut self, on: bool) {
        self.gapless = on;
    }

    pub fn is_gapless(&self) -> bool {
        self.gapless
    }

    pub fn play_tracks(&mut self, tracks: Vec<Track>, start_index: usize) -> Result<()> {
        self.or_as_it_was(false, |player| {
            player.queue.set_tracks(tracks, start_index);
            player.play_current()
        })
    }

    fn sounding(&self) -> bool {
        match self.state {
            PlaybackState::Playing => !self.engine.is_finished(),
            PlaybackState::Paused => true,
            PlaybackState::Stopped => false,
        }
    }

    fn or_as_it_was(
        &mut self,
        always: bool,
        change: impl FnOnce(&mut Self) -> Result<()>,
    ) -> Result<()> {
        let before = self.queue.clone();
        let state = self.state;
        let sounding = self.sounding();
        let outcome = change(self);
        if outcome.is_err() && (sounding || always) {
            self.queue.put_back(before);
            self.state = state;
        }
        outcome
    }

    pub fn retarget(&mut self, id: i64, path: &std::path::Path) {
        self.queue.retarget(id, path);
    }

    pub fn queued(&self) -> &[Track] {
        self.queue.tracks()
    }

    pub fn queue_position(&self) -> Option<usize> {
        self.queue.current_index().filter(|_| self.aside.is_none())
    }

    pub fn play_aside(&mut self, track: Track) -> Result<()> {
        self.on_air = None;
        let hint = (track.duration_secs > 0.0).then(|| Duration::from_secs_f64(track.duration_secs));
        self.engine.play_file_steady(&track.path, hint)?;
        self.aside_ended = None;
        self.reach = None;
        self.aside = Some(track);
        self.state = PlaybackState::Playing;
        Ok(())
    }

    pub fn play_aside_growing(&mut self, track: Track, source: rodio::Decoder<std::io::BufReader<growing::Growing>>) {
        self.on_air = None;
        let hint = (track.duration_secs > 0.0).then(|| Duration::from_secs_f64(track.duration_secs));
        let name = track.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        self.engine.play_source(watch::steady(source, &name), &track.path, hint);
        self.engine.mark_steadied();
        self.aside_ended = None;
        self.reach = Some(0.0);
        self.aside = Some(track);
        self.state = PlaybackState::Playing;
    }

    pub fn set_reach(&mut self, seconds: Option<f64>) {
        self.reach = seconds;
    }

    pub fn take_aside_ended(&mut self) -> Option<PathBuf> {
        self.aside_ended.take()
    }

    pub fn play_next(&mut self, tracks: Vec<Track>) -> Result<()> {
        let idle = self.queue.is_empty();
        self.queue.insert_next(tracks);
        if idle {
            return self.play_current();
        }
        Ok(())
    }

    pub fn enqueue(&mut self, tracks: Vec<Track>) -> Result<()> {
        let idle = self.queue.is_empty();
        self.queue.append(tracks);
        if idle {
            return self.play_current();
        }
        Ok(())
    }

    pub fn append_and_play(&mut self, tracks: Vec<Track>) -> Result<usize> {
        let from = self.queue.tracks().len();
        self.or_as_it_was(true, |player| {
            player.queue.append(tracks);
            player.queue.jump_to(from);
            player.play_current()
        })?;
        Ok(from)
    }

    pub fn play_queued(&mut self, index: usize) -> Result<()> {
        self.or_as_it_was(false, |player| {
            player.queue.jump_to(index);
            player.play_current()
        })
    }

    pub fn drop_queued(&mut self, index: usize) {
        let was_playing = self.queue.current_index() == Some(index);
        self.queue.remove(index);
        if was_playing {
            self.stop();
        }
    }

    pub fn insert_at(&mut self, at: usize, tracks: Vec<Track>) {
        let idle = self.queue.is_empty();
        self.queue.insert_at(at, tracks);
        if idle {
            let _ = self.play_current();
        }
    }

    pub fn move_queued(&mut self, from: usize, to: usize) {
        self.queue.move_row(from, to);
    }

    pub fn clear_queue(&mut self) {
        if self.sounding() {
            self.queue.keep_only_current();
        } else {
            self.queue.clear();
            self.stop();
        }
    }

    pub fn remember(&self) -> remembered::Remembered {
        remembered::Remembered {
            paths: self.queue.tracks().iter().map(|t| t.path.clone()).collect(),
            at: self.queue.current_index(),
        }
    }

    pub fn restore(&mut self, tracks: Vec<Track>, at: Option<usize>) {
        if tracks.is_empty() {
            return;
        }
        self.queue.set_tracks(tracks, at.unwrap_or(0));
        self.state = PlaybackState::Stopped;
    }

    fn play_current(&mut self) -> Result<()> {
        self.on_air = None;
        self.aside = None;
        self.reach = None;
        if let Some(track) = self.queue.current() {
            let path = track.path.clone();
            let hint = if track.duration_secs > 0.0 {
                Some(Duration::from_secs_f64(track.duration_secs))
            } else {
                None
            };
            if let Err(trouble) = self.engine.play_file(&path, hint) {
                if matches!(trouble, KesslerError::Gone(_)) {
                    self.gone.push(path);
                    self.state = PlaybackState::Stopped;
                }
                return Err(trouble);
            }
            self.state = PlaybackState::Playing;
        } else {
            self.state = PlaybackState::Stopped;
        }
        Ok(())
    }

    fn play_on(&mut self, back: bool) -> Result<()> {
        let room = self.queue.tracks().len();
        for _ in 0..room {
            match self.play_current() {
                Err(KesslerError::Gone(_)) if back => self.queue.previous(),
                Err(KesslerError::Gone(_)) => self.queue.next(),
                outcome => return outcome,
            };
        }
        self.state = PlaybackState::Stopped;
        Ok(())
    }

    pub fn gone_missing(&mut self) -> Vec<PathBuf> {
        std::mem::take(&mut self.gone)
    }

    pub fn play_stream(&mut self, url: &str) {
        self.aside = None;
        self.reach = None;
        self.engine.stop();
        let said = stream::Said::default();
        let introduced = stream::Introduced::default();
        let opening = open_stream(url, &said, &introduced);
        self.on_air = Some(OnAir {
            url: url.to_string(),
            said,
            introduced,
            opening: Some(opening),
            again_at: None,
            tries: 0,
            since: None,
        });
        self.state = PlaybackState::Playing;
    }

    pub fn on_air(&self) -> Option<&str> {
        self.on_air.as_ref().map(|on| on.url.as_str())
    }

    pub fn on_air_said(&self) -> Option<String> {
        self.on_air.as_ref().and_then(|on| on.said.lock().unwrap_or_else(|e| e.into_inner()).clone())
    }

    pub fn on_air_introduced(&self) -> Option<stream::Introduction> {
        self.on_air.as_ref().and_then(|on| on.introduced.lock().unwrap_or_else(|e| e.into_inner()).clone())
    }

    pub fn tuning(&self) -> bool {
        self.on_air.as_ref().is_some_and(|on| on.opening.is_some() || on.again_at.is_some())
    }

    fn tick_on_air(&mut self) -> Result<bool> {
        let playing = self.state == PlaybackState::Playing;
        let Some(on) = self.on_air.as_mut() else { return Ok(false) };
        if let Some(at) = on.again_at {
            if !playing || std::time::Instant::now() < at {
                return Ok(false);
            }
            on.again_at = None;
            on.opening = Some(open_stream(&on.url, &on.said, &on.introduced));
            return Ok(true);
        }
        if let Some(opening) = &on.opening {
            match opening.try_recv() {
                Ok(Ok(source)) => {
                    on.opening = None;
                    on.since = Some(std::time::Instant::now());
                    if playing {
                        self.engine.play_stream(source, &on.url);
                    }
                    return Ok(true);
                }
                Ok(Err(why)) => {
                    on.opening = None;
                    if on.tries > 0 && on.tries < RETRIES {
                        on.tries += 1;
                        on.again_at = Some(std::time::Instant::now() + RETRY_AFTER * 2u32.pow(on.tries - 1));
                        return Ok(true);
                    }
                    self.state = PlaybackState::Stopped;
                    self.on_air = None;
                    return Err(KesslerError::Playback(why));
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => return Ok(false),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    on.opening = None;
                    self.state = PlaybackState::Stopped;
                    return Ok(true);
                }
            }
        }
        if playing && self.engine.is_finished() {
            if on.since.is_some_and(|since| since.elapsed() >= HELD_FOR) {
                on.tries = 0;
            }
            if on.tries < RETRIES {
                on.tries += 1;
                on.since = None;
                on.again_at = Some(std::time::Instant::now() + RETRY_AFTER * 2u32.pow(on.tries - 1));
                return Ok(true);
            }
            self.state = PlaybackState::Stopped;
            self.on_air = None;
            return Ok(true);
        }
        Ok(false)
    }

    pub fn toggle_pause(&mut self) {
        if let Some(url) = self.on_air.as_ref().map(|on| on.url.clone()) {
            if self.state == PlaybackState::Playing {
                self.engine.stop();
                if let Some(on) = self.on_air.as_mut() {
                    on.opening = None;
                    on.again_at = None;
                }
                self.state = PlaybackState::Paused;
            } else {
                self.play_stream(&url);
            }
            return;
        }
        match self.state {
            PlaybackState::Playing => {
                self.engine.pause();
                self.state = PlaybackState::Paused;
            }
            PlaybackState::Paused => {
                self.engine.resume();
                self.state = PlaybackState::Playing;
            }
            PlaybackState::Stopped => {
                let _ = self.play_current();
            }
        }
    }

    pub fn next(&mut self) -> Result<()> {
        if self.on_air.is_some() {
            return Ok(());
        }
        if self.aside.is_some() {
            return self.or_as_it_was(false, |player| player.play_on(false));
        }
        self.or_as_it_was(false, |player| {
            player.queue.next();
            player.play_on(false)
        })
    }

    pub fn previous(&mut self) -> Result<()> {
        if self.on_air.is_some() {
            return Ok(());
        }
        if self.aside.is_some() {
            self.seek_to(0.0);
            return Ok(());
        }
        if self.engine.position() > Duration::from_secs(3) {
            return self.play_current();
        }
        self.or_as_it_was(false, |player| {
            player.queue.previous();
            player.play_on(true)
        })
    }

    pub fn stop(&mut self) {
        self.on_air = None;
        self.aside = None;
        self.reach = None;
        self.engine.stop();
        self.state = PlaybackState::Stopped;
    }

    pub fn volume_up(&mut self) {
        self.volume = (self.volume + 5).min(100);
        self.engine.set_volume(self.volume as f32 / 100.0);
    }

    pub fn volume_down(&mut self) {
        self.volume = self.volume.saturating_sub(5);
        self.engine.set_volume(self.volume as f32 / 100.0);
    }

    pub fn set_volume(&mut self, percent: u8) {
        self.volume = percent.min(100);
        self.engine.set_volume(self.volume as f32 / 100.0);
    }

    pub fn volume(&self) -> u8 {
        self.volume
    }

    pub fn state(&self) -> PlaybackState {
        self.state
    }

    pub fn position(&self) -> Duration {
        self.engine.position()
    }

    pub fn duration(&self) -> Duration {
        self.engine.duration()
    }

    pub fn progress(&self) -> f64 {
        self.engine.progress()
    }

    pub fn toggle_shuffle(&mut self) {
        self.queue.toggle_shuffle();
    }

    pub fn is_shuffle(&self) -> bool {
        self.queue.is_shuffle()
    }

    pub fn toggle_repeat(&mut self) {
        self.queue.repeat = self.queue.repeat.next();
    }

    pub fn repeat_mode(&self) -> RepeatMode {
        self.queue.repeat
    }

    pub fn tick(&mut self) -> Result<bool> {
        self.engine.rest_when_idle();
        if self.on_air.is_some() {
            return self.tick_on_air();
        }
        if self.state != PlaybackState::Playing {
            return Ok(false);
        }
        if self.aside.is_some() {
            if self.engine.is_finished() {
                self.aside_ended = self.aside.take().map(|t| t.path);
                self.state = PlaybackState::Stopped;
                return Ok(true);
            }
            return Ok(false);
        }

        if self.engine.handed_over() {
            self.queue.next();
            let heard = self.engine.now_playing();
            let meant = self.queue.current().map(|t| t.path.clone());
            if heard != meant {
                if meant.is_some() {
                    self.play_on(false)?;
                } else {
                    self.stop();
                }
                return Ok(true);
            }
            self.line_up_next();
            return Ok(true);
        }

        if self.engine.is_finished() {
            self.queue.next();
            if self.queue.current().is_some() {
                self.play_on(false)?;
                return Ok(true);
            }
            self.state = PlaybackState::Stopped;
            return Ok(true);
        }

        self.line_up_next();
        Ok(false)
    }

    fn line_up_next(&mut self) {
        if !self.gapless {
            return;
        }
        if self.engine.lined_up_len() != 1 {
            return;
        }
        let Some(track) = self.queue.peek_next() else {
            return;
        };
        let path = track.path.clone();
        let hint = (track.duration_secs > 0.0)
            .then(|| Duration::from_secs_f64(track.duration_secs));
        self.engine.line_up(&path, hint);
    }

    pub fn current_track(&self) -> Option<&Track> {
        if self.on_air.is_some() {
            return None;
        }
        let shown = |t: &&Track| !(self.library_hidden && t.id >= 0);
        if let Some(aside) = &self.aside {
            return Some(aside).filter(shown);
        }
        self.queue.current().filter(shown)
    }

    pub fn library_track(&self) -> Option<&Track> {
        self.current_track().filter(|t| t.id >= 0)
    }

    pub fn seek_to_fraction(&self, fraction: f64) -> bool {
        let total = self.engine.duration().as_secs_f64();
        if total <= 0.0 {
            return false;
        }
        self.seek_to(fraction.clamp(0.0, 1.0) * (total - 0.25).max(0.0))
    }

    pub fn seek_to(&self, seconds: f64) -> bool {
        if self.state == PlaybackState::Stopped {
            return false;
        }
        let seconds = match (self.reach, &self.aside) {
            (Some(reach), Some(_)) => seconds.min((reach - REACH_MARGIN_SECS).max(0.0)),
            _ => seconds,
        };
        self.engine
            .seek(Duration::from_secs_f64(seconds.max(0.0)))
    }

    pub fn rest_due_in(&self) -> Option<Duration> {
        self.engine.rest_due_in()
    }

    pub fn is_audio_available(&self) -> bool {
        self.engine.is_available()
    }

    pub fn audio_unavailable_reason(&self) -> Option<String> {
        self.engine.unavailable_reason()
    }

    pub fn heard(&self) -> std::sync::Arc<audio::Heard> {
        self.engine.heard()
    }
}

#[cfg(test)]
mod when_a_file_is_gone {
    use super::*;
    use crate::library::fixtures;

    fn track(path: &std::path::Path) -> Track {
        Track {
            id: 1,
            path: path.to_path_buf(),
            title: path.file_stem().unwrap().to_string_lossy().into_owned(),
            artist: "autechre".into(),
            album_artist: "autechre".into(),
            album: "amber".into(),
            genre: String::new(),
            year: None,
            track_number: None,
            disc_number: None,
            duration_secs: 0.0,
            bitrate: None,
            sample_rate: None,
            file_size: 0,
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

    struct Shelf {
        dir: PathBuf,
    }

    impl Shelf {
        fn new(name: &str) -> Self {
            Self {
                dir: fixtures::scratch(&format!("gone-{name}")),
            }
        }

        fn real(&self, name: &str) -> PathBuf {
            let at = self.dir.join(format!("{name}.mp3"));
            fixtures::place(&at, fixtures::MP3);
            at
        }

        fn broken(&self, name: &str) -> PathBuf {
            let at = self.dir.join(format!("{name}.m4a"));
            std::fs::write(&at, b"not a record").unwrap();
            at
        }

        fn gone(&self, name: &str) -> PathBuf {
            self.dir.join(format!("{name}.mp3"))
        }
    }

    #[test]
    fn a_track_somebody_pointed_at_says_so_rather_than_playing_another() {
        let shelf = Shelf::new("pointed");
        let (missing, real) = (shelf.gone("weg"), shelf.real("da"));
        let mut player = Player::new(50).expect("a player without a card is still a player");
        let outcome = player.play_tracks(vec![track(&missing), track(&real)], 0);

        assert!(
            matches!(outcome, Err(KesslerError::Gone(ref at)) if *at == missing),
            "asked for a file that is not there, the player should say which"
        );
        assert_eq!(player.state(), PlaybackState::Stopped, "nothing is playing");
        assert_eq!(
            player.queue_position(),
            Some(0),
            "and the queue still stands where it was asked to stand"
        );
        assert_eq!(player.gone_missing(), vec![missing], "told once");
        assert!(player.gone_missing().is_empty(), "and only once");
    }

    #[test]
    fn a_track_arrived_at_is_stepped_over() {
        let shelf = Shelf::new("arrived");
        let (first, missing, second) = (shelf.real("eins"), shelf.gone("weg"), shelf.real("drei"));
        let mut player = Player::new(50).unwrap();
        player
            .play_tracks(
                vec![track(&first), track(&missing), track(&second)],
                0,
            )
            .expect("the first one is there");

        player
            .next()
            .expect("a missing file is not an error to the hand");

        assert_eq!(
            player.queue_position(),
            Some(2),
            "past the missing one to the next that plays"
        );
        assert_eq!(player.state(), PlaybackState::Playing);
        assert_eq!(
            player.gone_missing(),
            vec![missing],
            "and it was noted on the way"
        );
    }

    #[test]
    fn a_queue_of_nothing_but_missing_files_ends_in_silence() {
        let shelf = Shelf::new("nothing");
        let (one, two) = (shelf.gone("weg-1"), shelf.gone("weg-2"));
        let mut player = Player::new(50).unwrap();
        let _ = player.play_tracks(vec![track(&one), track(&two)], 0);

        player
            .next()
            .expect("walking off the end is not an error either");

        assert_eq!(player.state(), PlaybackState::Stopped);
        assert_eq!(
            player.gone_missing(),
            vec![one, two],
            "the one pointed at, then the one walked past — each noted once"
        );
    }

    #[test]
    fn a_file_set_aside_leaves_the_queue_alone() {
        let shelf = Shelf::new("aside");
        let (first, second, episode) = (shelf.real("eins"), shelf.real("zwei"), shelf.real("folge"));
        let mut player = Player::new(50).unwrap();
        player.play_tracks(vec![track(&first), track(&second)], 1).unwrap();
        player.play_aside(track(&episode)).expect("the file is there");

        assert_eq!(player.queued().len(), 2, "nothing was added to the queue");
        assert_eq!(player.current_track().map(|t| t.path.clone()), Some(episode.clone()));
        assert_eq!(player.queue_position(), None, "no row of the queue plays meanwhile");
        assert_eq!(player.state(), PlaybackState::Playing);

        player.next().expect("on to the queue");
        assert_eq!(player.queue_position(), Some(1), "where the queue stood, not past it");
        assert_eq!(player.current_track().map(|t| t.path.clone()), Some(second));
        assert!(player.take_aside_ended().is_none(), "left, not heard to its end");
    }

    #[test]
    fn going_back_past_a_missing_file_keeps_going_back() {
        let shelf = Shelf::new("back");
        let (first, missing, third) = (shelf.real("eins"), shelf.gone("weg"), shelf.real("drei"));
        let mut player = Player::new(50).unwrap();
        player
            .play_tracks(vec![track(&first), track(&missing), track(&third)], 2)
            .expect("the third one is there");

        player.previous().expect("nor is this");

        assert_eq!(
            player.queue_position(),
            Some(0),
            "the one before the missing one, not the one after it"
        );
        assert_eq!(player.state(), PlaybackState::Playing);
    }

    #[test]
    fn a_double_click_adds_and_plays_without_emptying() {
        let shelf = Shelf::new("added");
        let (one, two, three) = (shelf.real("eins"), shelf.real("zwei"), shelf.real("drei"));
        let mut player = Player::new(50).unwrap();

        let first = player.append_and_play(vec![track(&one), track(&two)]).expect("both are there");
        assert_eq!(first, 0, "into an empty queue, from the top");
        assert_eq!(player.queue_position(), Some(0));

        let later = player.append_and_play(vec![track(&three)]).expect("so is this");
        assert_eq!(later, 2, "after what was already waiting");
        assert_eq!(player.queued().len(), 3, "nothing thrown away");
        assert_eq!(player.queue_position(), Some(2), "and what was asked for plays");
        assert_eq!(player.state(), PlaybackState::Playing);
    }

    #[test]
    fn what_cannot_be_played_is_not_queued() {
        let shelf = Shelf::new("unplayable");
        let (one, bad) = (shelf.real("eins"), shelf.broken("kaputt"));
        let mut player = Player::new(50).unwrap();
        player.append_and_play(vec![track(&one)]).expect("plays");

        let outcome = player.append_and_play(vec![track(&bad)]);
        assert!(outcome.is_err(), "says why");
        assert_eq!(player.queued().len(), 1, "nothing was added");
        assert_eq!(player.queue_position(), Some(0), "still on the one heard");
        assert_eq!(player.current_track().unwrap().path, one);
        assert_eq!(player.state(), PlaybackState::Playing);
    }

    #[test]
    fn a_row_that_cannot_be_played_does_not_take_the_place() {
        let shelf = Shelf::new("row");
        let (one, bad) = (shelf.real("eins"), shelf.broken("kaputt"));
        let mut player = Player::new(50).unwrap();
        player.play_tracks(vec![track(&one), track(&bad)], 0).expect("plays");

        assert!(player.play_queued(1).is_err());
        assert_eq!(player.queue_position(), Some(0));
        assert_eq!(player.current_track().unwrap().path, one);
        assert_eq!(player.queued().len(), 2, "the row itself stays; it was put there");
    }

    #[test]
    fn emptying_the_queue_keeps_what_plays() {
        let shelf = Shelf::new("emptying");
        let (one, two, three) = (shelf.real("eins"), shelf.real("zwei"), shelf.real("drei"));
        let mut player = Player::new(50).unwrap();
        player.play_tracks(vec![track(&one), track(&two), track(&three)], 1).unwrap();

        player.clear_queue();
        assert_eq!(player.queued().len(), 1);
        assert_eq!(player.current_track().unwrap().path, two);
        assert_eq!(player.state(), PlaybackState::Playing);

        player.stop();
        player.clear_queue();
        assert!(player.queued().is_empty());
    }
}
