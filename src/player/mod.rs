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

/// Playback state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
}

/// The main player controller, combining the audio engine with the play queue.
pub struct Player {
    engine: AudioEngine,
    pub queue: PlayQueue,
    /// The music switched off: the queue's track of the library is nobody's to
    /// see — not in the header under the radio or the podcasts, not in the
    /// system's media controls. The queue keeps it.
    pub library_hidden: bool,
    state: PlaybackState,
    /// Volume as percentage (0-100).
    volume: u8,
    /// Files that turned out not to be there. See `gone_missing`.
    gone: Vec<PathBuf>,
    /// Whether one track is laid behind the other so they run into each other.
    /// `player.gapless` in the settings, which until now promised something
    /// nobody had built.
    gapless: bool,
    /// A station playing instead of the queue, or being tuned in — see
    /// [`Self::play_stream`]. `None` all the time the radio is not used.
    on_air: Option<OnAir>,
    /// A file playing beside the queue, which waits meanwhile — a podcast's
    /// episode. See [`Self:play_aside`].
    aside: Option<Track>,
    /// The file set aside that was heard to its end, until it is asked for
    /// — see [`Self::take_aside_ended`].
    aside_ended: Option<PathBuf>,
    /// How far into what is set aside can be gone, in seconds, while its
    /// file is still being fetched — see [`Self::set_reach`].
    reach: Option<f64>,
}

/// A station the player is on, and its stream while it is still opening.
struct OnAir {
    url: String,
    said: stream::Said,
    introduced: stream::Introduced,
    opening: Option<Opening>,
    /// When a stream that broke off is tried again, and how many times it has
    /// been in a row (after Goodvibes and Transistor: a moment without wifi
    /// should not end the evening's radio).
    again_at: Option<std::time::Instant>,
    tries: u32,
    /// Since when the stream has played, so a station that holds for a while
    /// earns its tries back.
    since: Option<std::time::Instant>,
}

/// A stream being opened on its own thread.
type Opening = std::sync::mpsc::Receiver<std::result::Result<rodio::Decoder<std::io::BufReader<stream::Stream>>, String>>;

/// How many times in a row a stream that broke off is tried again, and how
/// long the first wait is (it doubles each time).
const RETRIES: u32 = 4;
const RETRY_AFTER: std::time::Duration = std::time::Duration::from_secs(2);

/// How far short of what has come in a seek stops, in seconds: the
/// decoder reads a little ahead of where it lands.
const REACH_MARGIN_SECS: f64 = 5.0;

/// How long a stream has to hold before its tries are counted afresh.
const HELD_FOR: std::time::Duration = std::time::Duration::from_secs(30);

/// Open `url` on a thread of its own, saying what it plays into `said` and
/// what it says about itself into `introduced`.
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
    /// Levelling's table and switch — see [`leveling`].
    pub fn leveling(&self) -> std::sync::Arc<leveling::Leveling> {
        self.engine.leveling()
    }

    /// Create a new player.
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

    /// Whether to play one track into the next without the hole between them.
    ///
    /// Off, every track is opened after the one before it has ended, which is
    /// kept because the hand-over has one price
    /// (see [`Self::line_up_next`]) and somebody may not want to pay it.
    pub fn set_gapless(&mut self, on: bool) {
        self.gapless = on;
    }

    /// Whether it is on, for the row in the menu that turns it over.
    pub fn is_gapless(&self) -> bool {
        self.gapless
    }

    /// Play a specific track list starting at the given index.
    pub fn play_tracks(&mut self, tracks: Vec<Track>, start_index: usize) -> Result<()> {
        self.or_as_it_was(false, |player| {
            player.queue.set_tracks(tracks, start_index);
            player.play_current()
        })
    }

    /// Whether something is being heard, or would be at the press of play:
    /// a track under way, or paused. Not one that has run out — that one is
    /// over, and the queue moving on from it is the queue doing its job.
    fn sounding(&self) -> bool {
        match self.state {
            PlaybackState::Playing => !self.engine.is_finished(),
            PlaybackState::Paused => true,
            PlaybackState::Stopped => false,
        }
    }

    /// Make a change to the queue and play from it — or, when what it points
    /// at cannot be played, leave everything as it was.
    ///
    /// A file the decoder could not open used to move the queue's place onto it
    /// while the old track went on sounding: the header named a track that was
    /// not playing, and the queue marked it. Now what is named is what is
    /// heard. While something sounds, a failed change is undone whole; `always`
    /// undoes it even in silence, for a change that only added — a double-click
    /// on a record nothing can play puts nothing in the queue. The error still
    /// comes back, so the window says why.
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

    /// A track's file has moved — see [`PlayQueue::retarget`].
    pub fn retarget(&mut self, id: i64, path: &std::path::Path) {
        self.queue.retarget(id, path);
    }

    /// What is queued up, in order, and where in it the player stands.
    pub fn queued(&self) -> &[Track] {
        self.queue.tracks()
    }

    /// Where in the queue the player stands — nowhere while a file set
    /// aside plays.
    pub fn queue_position(&self) -> Option<usize> {
        self.queue.current_index().filter(|_| self.aside.is_none())
    }

    /// Play `track` beside the queue: the queue keeps its place and its
    /// rows, and neither its repeat nor its shuffle touch it; at its end
    /// the player stops, and play goes on with the queue where it was.
    /// Every way back to the queue or the radio lets go of it.
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

    /// [`Self::play_aside`], from a file still being fetched: the decoder on it
    /// is opened by the caller — see [`growing::open`] — and until
    /// [`Self::set_reach`] says otherwise nothing past the start can be gone
    /// to.
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

    /// How far can be gone in what is set aside while its file is still
    /// coming in, in seconds; `None` once it is all there. A seek past it
    /// would wait for the fetch, the window with it.
    pub fn set_reach(&mut self, seconds: Option<f64>) {
        self.reach = seconds;
    }

    /// The file set aside that ended on its own, heard to its end — once.
    pub fn take_aside_ended(&mut self) -> Option<PathBuf> {
        self.aside_ended.take()
    }

    /// Queue tracks up directly after what is playing, or at the end.
    ///
    /// Neither starts anything: putting a record on the pile is not the same
    /// act as playing it. The exception is a player with nothing to do, where
    /// the pile *is* what plays, and standing silent next to a full queue
    /// would only look broken.
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

    /// Put tracks at the end and play the first of them straight away.
    ///
    /// What a double-click on a track, a record or a name does: what was queued
    /// before stays where it was, above the needle, and what you asked for is
    /// what you hear. Returns the row the first of them landed on. Nothing is
    /// added when the first of them cannot be played — see
    /// [`Self::or_as_it_was`].
    pub fn append_and_play(&mut self, tracks: Vec<Track>) -> Result<usize> {
        let from = self.queue.tracks().len();
        self.or_as_it_was(true, |player| {
            player.queue.append(tracks);
            player.queue.jump_to(from);
            player.play_current()
        })?;
        Ok(from)
    }

    /// Play this row of the queue, leaving the rest of it alone.
    pub fn play_queued(&mut self, index: usize) -> Result<()> {
        self.or_as_it_was(false, |player| {
            player.queue.jump_to(index);
            player.play_current()
        })
    }

    /// Take one row out.
    ///
    /// Taking out the row that is playing stops it rather than sliding the
    /// next one silently into its place: what you hear should change only when
    /// you asked for it to, and you asked to remove a track, not to skip.
    pub fn drop_queued(&mut self, index: usize) {
        let was_playing = self.queue.current_index() == Some(index);
        self.queue.remove(index);
        if was_playing {
            self.stop();
        }
    }

    /// Put tracks in at one exact place, which is what a hand that dropped
    /// them there asked for.
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

    /// Everything out but the track being heard, which plays on to its end
    /// and is then the end of the queue. With nothing sounding, all of it.
    pub fn clear_queue(&mut self) {
        if self.sounding() {
            self.queue.keep_only_current();
        } else {
            self.queue.clear();
            self.stop();
        }
    }

    /// The queue as something that can be written down and read back.
    pub fn remember(&self) -> remembered::Remembered {
        remembered::Remembered {
            paths: self.queue.tracks().iter().map(|t| t.path.clone()).collect(),
            at: self.queue.current_index(),
        }
    }

    /// Put a remembered queue back, without starting it.
    ///
    /// Nothing plays: coming back to a program should not make a noise. What
    /// it does restore is the place you were at, so pressing play carries on
    /// rather than starting the list again.
    pub fn restore(&mut self, tracks: Vec<Track>, at: Option<usize>) {
        if tracks.is_empty() {
            return;
        }
        self.queue.set_tracks(tracks, at.unwrap_or(0));
        self.state = PlaybackState::Stopped;
    }

    /// Play the current track in the queue.
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

    /// Play the current track, stepping over files that are no longer there.
    ///
    /// For the moves nobody pointed at a row for: a track ending, the skip
    /// key, the media keys. A file that has been moved or deleted since the
    /// shelf was read should not end the evening — it is noted, stepped over,
    /// and the next one plays.
    ///
    /// Pointing at a row is the other case and keeps [`Self::play_current`]:
    /// asked for *that* track, playing a different one instead would be
    /// answering a question nobody asked.
    ///
    /// Stepped over in the direction the hand was going, so „zurück" past a
    /// missing file lands on the one before it rather than the one after.
    ///
    /// The queue is walked at most once round, so a queue of nothing but
    /// missing files ends in silence rather than spinning.
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

    /// Files found missing since this was last asked — once.
    ///
    /// Drained rather than kept: the player's business is playing, and what
    /// the window does with the news (marking those rows for the rest of the
    /// session) is the window's. Every way of reaching a file passes through
    /// `play_current`, so there is one place that learns it and one place that
    /// tells.
    pub fn gone_missing(&mut self) -> Vec<PathBuf> {
        std::mem::take(&mut self.gone)
    }

    /// Tune in to a station: its stream is opened on a thread of its own —
    /// reaching a server and hearing enough of it to know what it is takes a
    /// moment — and starts playing at the first `tick` after that. The queue
    /// stays as it was, and is where `next` and a click on a track go back to.
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

    /// The stream playing or being tuned in, if a station is on.
    pub fn on_air(&self) -> Option<&str> {
        self.on_air.as_ref().map(|on| on.url.as_str())
    }

    /// What the station says it plays, if it says.
    pub fn on_air_said(&self) -> Option<String> {
        self.on_air.as_ref().and_then(|on| on.said.lock().unwrap_or_else(|e| e.into_inner()).clone())
    }

    /// What the station says about itself, once its stream has answered.
    pub fn on_air_introduced(&self) -> Option<stream::Introduction> {
        self.on_air.as_ref().and_then(|on| on.introduced.lock().unwrap_or_else(|e| e.into_inner()).clone())
    }

    /// Whether a station is still being tuned in.
    pub fn tuning(&self) -> bool {
        self.on_air.as_ref().is_some_and(|on| on.opening.is_some() || on.again_at.is_some())
    }

    /// The radio's part of `tick`: the stream that has opened starts, one
    /// that would not open or has broken off stops. Paused, a station is let
    /// go of altogether — a stream held still would only fall behind the
    /// air — and pressing play tunes in again.
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
                    let again = if on.tries > 0 { format!(" · wieder, {}. versuch", on.tries) } else { String::new() };
                    watch::note(format!("radio: {} · verbunden{again}", watch::short(&on.url)));
                    if playing {
                        self.engine.play_stream(source, &on.url);
                    }
                    return Ok(true);
                }
                Ok(Err(why)) => {
                    on.opening = None;
                    watch::note(format!("radio: {} · ging nicht auf: {why}", watch::short(&on.url)));
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
            let held = on.since.map(|since| since.elapsed().as_secs_f64()).unwrap_or_default();
            if on.tries < RETRIES {
                on.tries += 1;
                on.since = None;
                let wait = RETRY_AFTER * 2u32.pow(on.tries - 1);
                on.again_at = Some(std::time::Instant::now() + wait);
                watch::note(format!(
                    "radio: {} · abgebrochen nach {held:.0} s, neu verbinden in {:.0} s",
                    watch::short(&on.url),
                    wait.as_secs_f64()
                ));
                return Ok(true);
            }
            watch::note(format!("radio: {} · abgebrochen nach {held:.0} s, aufgegeben", watch::short(&on.url)));
            self.state = PlaybackState::Stopped;
            self.on_air = None;
            return Ok(true);
        }
        Ok(false)
    }

    /// Toggle play/pause.
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

    /// Skip to next track. On the radio there is none: the window steps
    /// through its stations instead.
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

    /// Go to previous track.
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

    /// Stop playback.
    pub fn stop(&mut self) {
        self.on_air = None;
        self.aside = None;
        self.reach = None;
        self.engine.stop();
        self.state = PlaybackState::Stopped;
    }

    /// Increase volume by 5%.
    pub fn volume_up(&mut self) {
        self.volume = (self.volume + 5).min(100);
        self.engine.set_volume(self.volume as f32 / 100.0);
    }

    /// Decrease volume by 5%.
    pub fn volume_down(&mut self) {
        self.volume = self.volume.saturating_sub(5);
        self.engine.set_volume(self.volume as f32 / 100.0);
    }

    /// Set the volume outright, for dragging along a rule.
    pub fn set_volume(&mut self, percent: u8) {
        self.volume = percent.min(100);
        self.engine.set_volume(self.volume as f32 / 100.0);
    }

    /// Get volume as percentage.
    pub fn volume(&self) -> u8 {
        self.volume
    }

    /// Get the current playback state.
    pub fn state(&self) -> PlaybackState {
        self.state
    }

    /// Get the current playback position.
    pub fn position(&self) -> Duration {
        self.engine.position()
    }

    /// Get the total duration of the current track.
    pub fn duration(&self) -> Duration {
        self.engine.duration()
    }

    /// Get the playback progress.
    pub fn progress(&self) -> f64 {
        self.engine.progress()
    }

    /// Toggle shuffle.
    pub fn toggle_shuffle(&mut self) {
        self.queue.toggle_shuffle();
    }

    /// Is shuffle enabled?
    pub fn is_shuffle(&self) -> bool {
        self.queue.is_shuffle()
    }

    /// Cycle repeat mode.
    pub fn toggle_repeat(&mut self) {
        self.queue.repeat = self.queue.repeat.next();
    }

    /// Get repeat mode.
    pub fn repeat_mode(&self) -> RepeatMode {
        self.queue.repeat
    }

    /// Keep the queue moving. Called every frame.
    ///
    /// Two ways a track can end. Either the sink has already run into the file
    /// laid out behind it — that is the gapless one, and all that is left to
    /// do here is to move the queue's own place along to match what is already
    /// being heard. Or the sink ran dry, which is what happens at the end of a
    /// queue, after a queue was edited under the hand-over, or whenever the
    /// next file could not be laid out; then it is opened and started the old
    /// way, with the small hole that has always been there.
    ///
    /// Returns whether the track changed.
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

    /// Put the track after this one at the sink's door.
    ///
    /// Only what the queue would go to anyway, and only while the sink is
    /// holding exactly the one that plays. What it cannot do is take it back
    /// before it is heard: clearing the sink to correct it would be a gap in
    /// the middle of the music. The hand-over checks instead, and a track laid
    /// out for a queue that has changed since is replaced there — see `tick`.
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

    /// Get the currently playing track.
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

    /// The track playing, if it is one of the library's — not an episode or
    /// anything else that borrows the player under an id no row of the library
    /// has.
    pub fn library_track(&self) -> Option<&Track> {
        self.current_track().filter(|t| t.id >= 0)
    }

    /// Jump to a fraction of the track, for dragging along the progress rule.
    pub fn seek_to_fraction(&self, fraction: f64) -> bool {
        let total = self.engine.duration().as_secs_f64();
        if total <= 0.0 {
            return false;
        }
        self.seek_to(fraction.clamp(0.0, 1.0) * (total - 0.25).max(0.0))
    }

    /// Jump to an absolute position, in seconds.
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

    /// When the device will be let go of if nothing happens — see
    /// [`AudioEngine::rest_due_in`].
    pub fn rest_due_in(&self) -> Option<Duration> {
        self.engine.rest_due_in()
    }

    /// Whether there is an output device. False means browsing works but
    /// nothing will be audible.
    pub fn is_audio_available(&self) -> bool {
        self.engine.is_available()
    }

    /// Why playback is silent, when it is.
    pub fn audio_unavailable_reason(&self) -> Option<String> {
        self.engine.unavailable_reason()
    }

    /// The sound on its way to the card, for the things that draw it.
    pub fn heard(&self) -> std::sync::Arc<audio::Heard> {
        self.engine.heard()
    }
}

/// What happens when the disk no longer agrees with the shelf.
///
/// The shelf is a picture of the disk taken when it was last read, and between
/// two readings files get moved, renamed and thrown away. Everything here is
/// about the difference between being *asked* for a track and *arriving* at
/// one: the first is answered, the second is walked past.
///
/// No sound card is needed for any of it — a missing file is refused before
/// the decoder is ever reached, which is the whole point of telling the two
/// troubles apart.
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

    /// A shelf with real files on it and names that stand for nothing.
    ///
    /// A directory of its own per test: `scratch` empties what it is given,
    /// and the tests run side by side.
    struct Shelf {
        dir: PathBuf,
    }

    impl Shelf {
        fn new(name: &str) -> Self {
            Self {
                dir: fixtures::scratch(&format!("gone-{name}")),
            }
        }

        /// A file that is really there.
        fn real(&self, name: &str) -> PathBuf {
            let at = self.dir.join(format!("{name}.mp3"));
            fixtures::place(&at, fixtures::MP3);
            at
        }

        /// A file that is there and is not music the decoder knows.
        fn broken(&self, name: &str) -> PathBuf {
            let at = self.dir.join(format!("{name}.m4a"));
            std::fs::write(&at, b"not a record").unwrap();
            at
        }

        /// A name the shelf still carries and the disk has forgotten.
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

    /// An episode plays beside the queue: the queue keeps its rows and its
    /// place, shows nothing playing meanwhile, and the next key goes back to
    /// where it stood.
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

    /// Back is back, even past a hole.
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

    /// A double-click adds to the queue and plays what it added; nothing that
    /// was there before goes.
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

    /// A double-click on something that cannot be played, while something else
    /// plays: nothing is added, and what is named is still what is heard.
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

    /// The same for a row of the queue that cannot be played: clicked, the
    /// place in the queue stays with the track that is heard.
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

    /// Emptying the queue while something plays leaves that one playing, and
    /// alone; with nothing playing, nothing is left.
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
