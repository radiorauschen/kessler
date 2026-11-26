use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, Source};

use crate::error::{KesslerError, Result};

/// Low-level audio output using rodio.
///
/// The output device is optional. A machine with no sound card — a container,
/// a session over SSH, a laptop whose Bluetooth headphones just walked out of
/// range — should still be able to browse a library and read its tags. So a
/// missing device is not an error here: the engine keeps working and stays
/// silent, and [`AudioEngine::is_available`] lets the interface say so.
///
/// **The device is opened when there is something to play, and let go of
/// when there has not been for a while** — see [`AudioEngine::rest_when_idle`].
/// An open output never stops: with nothing to play it sends silence to the
/// card, all day, and on a Mac an output that is running keeps the machine
/// from going to sleep by itself. It also stays bound to the device it was
/// opened on, so headphones plugged in later were never heard from; opened
/// afresh, it takes whatever the system's output is now.
pub struct AudioEngine {
    /// The open device and the sink on it, while there is one.
    output: RefCell<Option<Output>>,
    /// The volume, kept here so a device opened later starts at it.
    volume: Cell<f32>,
    /// A paused track whose device was let go of: which file, and where in
    /// it. Resuming opens the device again, then the file, then goes there.
    parked: RefCell<Option<PathBuf>>,
    /// When the pause began — see [`FRESH_AFTER`].
    paused_at: Cell<Option<Instant>>,
    /// What plays was opened elsewhere and may still be growing — an
    /// episode on its way in. Opened again from its file it would end where
    /// the file ends now, so a pause alone does not open it afresh.
    borrowed: Cell<bool>,
    /// Whether what plays was kept steady — see [`super::watch::steady`] — so
    /// that taken up again after a long pause it is again.
    steadied: Cell<bool>,
    /// Which file is at the front of the sink — what [`Self::parked`] takes
    /// when the device is let go of.
    playing_path: RefCell<VecDeque<PathBuf>>,
    /// How long each file handed to the sink runs, oldest first.
    ///
    /// A sink is a queue, and that is the whole of gapless playing: the next
    /// file is decoded and put behind the current one while it is still
    /// running, so the hand-over happens inside the sink with no gap at all.
    /// What the sink cannot say is *which* of them is playing now — only how
    /// many are left — so their lengths are kept here in the same order, and
    /// the front of this line is what is playing.
    lined_up: Arc<Mutex<VecDeque<Duration>>>,
    /// When playback started (for position tracking).
    playback_start: Arc<Mutex<Option<Instant>>>,
    /// Accumulated position before last pause.
    accumulated_position: Arc<Mutex<Duration>>,
    /// Total duration of the current track.
    track_duration: Arc<Mutex<Duration>>,
    /// Why there is no sound, when there is none.
    unavailable_reason: RefCell<Option<String>>,
    /// The samples on their way to the card, for whatever draws them.
    heard: Arc<Heard>,
    /// How loud each file is to be played — see [`super::leveling`].
    leveling: Arc<super::leveling::Leveling>,
}

/// An open device and the sink playing into it.
struct Output {
    /// Held, not used: dropping it closes the device.
    _stream: MixerDeviceSink,
    sink: Arc<Mutex<Player>>,
    /// Since when it has had nothing to do, if it has had nothing to do.
    idle_since: Option<Instant>,
    /// The rate it was opened at, and when the device was last asked whether it
    /// still runs at it.
    rate: u32,
    asked_at: Instant,
    /// The device's name as it was opened, to see at „play" whether the
    /// system has moved on to another one since.
    name: String,
    /// Said by the stream itself, from its own thread: the device went away or
    /// was changed under it, and nothing played into it is heard any more.
    lost: Arc<AtomicBool>,
}

/// A pause longer than this ends on a freshly opened device rather than on the
/// one it began on. A stream left open over a pause is the one thing here
/// nobody is listening to: the lid closed, the headphones taken off, the system
/// moved to another output — and „play" went into a stream that no longer
/// sounded. Opening it costs a few milliseconds; below this, a pause is a pause
/// and resumes on the sample.
const FRESH_AFTER: Duration = Duration::from_secs(10);

/// How often the device is asked whether its rate is still the one it was
/// opened at, while something plays.
const DEVICE_ASKED_EVERY: Duration = Duration::from_secs(3);

/// The default output device's rate and name as the system says them now.
fn device_now() -> Option<(u32, String)> {
    use rodio::cpal::traits::{DeviceTrait, HostTrait};
    let device = rodio::cpal::default_host().default_output_device()?;
    let rate = device.default_output_config().ok()?.sample_rate();
    let name = device.description().map(|d| d.name().to_string()).unwrap_or_default();
    Some((rate, name))
}

/// How long the device is kept with nothing in it — stopped, or at the end
/// of the queue — before it is let go of. Long enough that nothing between
/// two clicks closes it; short enough that a finished record does not keep
/// the card running for the rest of the evening.
const REST_EMPTY: Duration = Duration::from_secs(20);
/// And how long with a track paused in it. Longer: a pause is often a phone
/// call, and the first second after pressing play again is spent opening the
/// device rather than playing.
const REST_PAUSED: Duration = Duration::from_secs(5 * 60);

impl AudioEngine {
    /// Create an engine. The device is not opened yet — only looked for, so
    /// the interface can say straight away when there is none.
    ///
    /// Never fails for want of a device — see the type's documentation.
    pub fn new() -> Result<Self> {
        let engine = Self {
            output: RefCell::new(None),
            volume: Cell::new(1.0),
            parked: RefCell::new(None),
            paused_at: Cell::new(None),
            borrowed: Cell::new(false),
            steadied: Cell::new(false),
            playing_path: RefCell::new(VecDeque::new()),
            lined_up: Arc::new(Mutex::new(VecDeque::new())),
            playback_start: Arc::new(Mutex::new(None)),
            accumulated_position: Arc::new(Mutex::new(Duration::ZERO)),
            track_duration: Arc::new(Mutex::new(Duration::ZERO)),
            unavailable_reason: RefCell::new(None),
            heard: Arc::new(Heard::default()),
            leveling: Arc::new(super::leveling::Leveling::default()),
        };
        if let Err(e) = DeviceSinkBuilder::from_default_device() {
            engine.no_device(e.to_string());
        }
        Ok(engine)
    }

    fn no_device(&self, why: String) {
        let reason = format!("no audio device: {why}");
        tracing::warn!("{reason} — playback is silent, browsing still works");
        *self.unavailable_reason.borrow_mut() = Some(reason);
    }

    /// The sink, if the device is open.
    fn sink(&self) -> Option<Arc<Mutex<Player>>> {
        self.output.borrow().as_ref().map(|o| Arc::clone(&o.sink))
    }

    /// The sink, opening the device for it if it is not open.
    fn woken_sink(&self) -> Option<Arc<Mutex<Player>>> {
        if let Some(sink) = self.sink() {
            return Some(sink);
        }
        let lost = Arc::new(AtomicBool::new(false));
        let says = Arc::clone(&lost);
        let told = move |err: rodio::cpal::Error| {
            use rodio::cpal::ErrorKind::{DeviceChanged, DeviceNotAvailable, StreamInvalidated};
            if matches!(err.kind(), DeviceChanged | DeviceNotAvailable | StreamInvalidated) && !says.swap(true, Ordering::Relaxed) {
                super::watch::note(format!("ausgabe verloren: {err}"));
            }
        };
        let opened = DeviceSinkBuilder::from_default_device()
            .and_then(|builder| builder.with_error_callback(told).open_sink_or_fallback())
            .or_else(|_| DeviceSinkBuilder::open_default_sink());
        match opened {
            Ok(mut stream) => {
                stream.log_on_drop(false);
                let said = *stream.config();
                let name = device_now().map(|(_, name)| name).unwrap_or_default();
                super::watch::note(format!(
                    "ausgabe: {name} · {} hz · {} kanäle · {:?}",
                    said.sample_rate().get(),
                    said.channel_count().get(),
                    said.sample_format()
                ));
                let sink = Player::connect_new(stream.mixer());
                sink.pause();
                sink.set_volume(self.volume.get());
                let sink = Arc::new(Mutex::new(sink));
                *self.output.borrow_mut() = Some(Output {
                    _stream: stream,
                    sink: Arc::clone(&sink),
                    idle_since: None,
                    rate: said.sample_rate().get(),
                    asked_at: Instant::now(),
                    name,
                    lost,
                });
                *self.unavailable_reason.borrow_mut() = None;
                Some(sink)
            }
            Err(e) => {
                self.no_device(e.to_string());
                None
            }
        }
    }

    /// Let go of the device if it has had nothing to play for long enough.
    ///
    /// Asked on every tick of the player, playing or not. Nothing to play
    /// means an empty sink, or a paused one; a paused track is remembered
    /// (which file, where in it) so pressing play picks it up where it was.
    pub fn rest_when_idle(&self) {
        if self.output.borrow().as_ref().is_some_and(|open| open.lost.load(Ordering::Relaxed)) {
            self.open_afresh();
            return;
        }
        let mut output = self.output.borrow_mut();
        let Some(open) = output.as_mut() else { return };
        let (empty, paused) = {
            let sink = open.sink.lock().unwrap_or_else(|e| e.into_inner());
            (sink.empty(), sink.is_paused())
        };
        if !empty && !paused {
            open.idle_since = None;
            if open.asked_at.elapsed() >= DEVICE_ASKED_EVERY {
                open.asked_at = Instant::now();
                let now = device_now();
                if let Some((_, name)) = now.as_ref().filter(|(_, name)| !name.is_empty() && !open.name.is_empty() && *name != open.name) {
                    super::watch::note(format!("ausgabe gewechselt: {} → {name}", open.name));
                    drop(output);
                    self.open_afresh();
                    return;
                }
                if let Some((rate, name)) = now.filter(|(rate, _)| *rate != open.rate) {
                    super::watch::note(format!("ausgabe gewechselt: {name} · jetzt {rate} hz (geöffnet mit {} hz)", open.rate));
                    open.rate = rate;
                }
            }
            return;
        }
        let since = *open.idle_since.get_or_insert_with(Instant::now);
        let long_enough = if empty { REST_EMPTY } else { REST_PAUSED };
        if since.elapsed() < long_enough {
            return;
        }
        if !empty {
            *self.parked.borrow_mut() = self.playing_path.borrow().front().cloned();
        }
        self.playing_path.borrow_mut().clear();
        let mut lined_up = self.lined_up.lock().unwrap_or_else(|e| e.into_inner());
        lined_up.truncate(1);
        if empty {
            lined_up.clear();
        }
        *output = None;
        tracing::debug!("das ausgabegerät ist losgelassen");
    }

    /// Close the device and open it again, with what was in it: a track
    /// playing goes on from where it was, a paused one waits there for
    /// „play", an empty sink is simply let go of.
    fn open_afresh(&self) {
        let Some(open) = self.output.borrow_mut().take() else { return };
        let (empty, paused) = {
            let sink = open.sink.lock().unwrap_or_else(|e| e.into_inner());
            (sink.empty(), sink.is_paused())
        };
        drop(open);
        let front = self.playing_path.borrow().front().cloned();
        self.playing_path.borrow_mut().clear();
        {
            let mut lined_up = self.lined_up.lock().unwrap_or_else(|e| e.into_inner());
            lined_up.truncate(1);
            if empty {
                lined_up.clear();
            }
        }
        let Some(path) = front.filter(|path| !empty && path.is_file()) else {
            if !empty && !paused {
                self.woken_sink();
            }
            return;
        };
        let at = self.position();
        *self.accumulated_position.lock().unwrap_or_else(|e| e.into_inner()) = at;
        *self.playback_start.lock().unwrap_or_else(|e| e.into_inner()) = None;
        if paused {
            *self.parked.borrow_mut() = Some(path);
        } else {
            self.unpark(&path);
        }
    }

    /// The sound itself, for the things that draw it.
    ///
    /// Handed out rather than copied: two pictures may want it in the same
    /// frame, and it is the same stream.
    pub fn heard(&self) -> Arc<Heard> {
        Arc::clone(&self.heard)
    }

    /// How long until [`Self::rest_when_idle`] would let the device go, if
    /// it is open and idle — for the window, which draws nothing while
    /// nothing changes and so has to be asked back at that moment.
    pub fn rest_due_in(&self) -> Option<Duration> {
        let output = self.output.borrow();
        let open = output.as_ref()?;
        let since = open.idle_since?;
        let empty = open.sink.lock().unwrap_or_else(|e| e.into_inner()).empty();
        let long_enough = if empty { REST_EMPTY } else { REST_PAUSED };
        Some(long_enough.saturating_sub(since.elapsed()) + Duration::from_millis(50))
    }

    /// Whether there is an output device to play through — open, or to be
    /// opened when there is something to play.
    pub fn is_available(&self) -> bool {
        self.unavailable_reason.borrow().is_none()
    }

    /// Why playback is silent, when it is.
    pub fn unavailable_reason(&self) -> Option<String> {
        self.unavailable_reason.borrow().clone()
    }

    /// Load and start playing a file.
    ///
    /// `duration_hint` is the length we already know from the file's tags.
    /// Most MP3 decoders cannot report a total duration up front, so without
    /// the hint the progress bar would stay empty.
    ///
    /// A file that is simply not there any more is answered with
    /// [`KesslerError::Gone`] rather than with a decoder's complaint: the
    /// shelf is a picture of the disk taken when it was last read, and the
    /// disk is allowed to have moved on. Asked before opening, because the
    /// reason matters more than the failure — the window marks the row with
    /// it.
    pub fn play_file(&self, path: &Path, duration_hint: Option<Duration>) -> Result<()> {
        self.steadied.set(false);
        if matches!(path.try_exists(), Ok(false)) {
            return Err(KesslerError::Gone(path.to_path_buf()));
        }
        let opening = Instant::now();
        let source = open(path).map_err(|why| {
            KesslerError::Playback(format!("Failed to decode {}: {why}", path.display()))
        })?;
        let opened = opening.elapsed();

        let total = source
            .total_duration()
            .or(duration_hint)
            .unwrap_or(Duration::ZERO);

        let handing = Instant::now();
        self.borrowed.set(false);
        *self.parked.borrow_mut() = None;
        if let Some(sink) = self.woken_sink() {
            let sink = sink.lock().unwrap_or_else(|e| e.into_inner());
            sink.clear();
            sink.append(Tap::levelled(super::ahead::Ahead::new(source), Arc::clone(&self.heard), &self.leveling, path));
            sink.play();
        }
        tracing::debug!(
            "die datei öffnen: {opened:.1?}, an die karte geben: {:.1?}",
            handing.elapsed()
        );

        let mut lined_up = self.lined_up.lock().unwrap_or_else(|e| e.into_inner());
        lined_up.clear();
        lined_up.push_back(total);
        let mut paths = self.playing_path.borrow_mut();
        paths.clear();
        paths.push_back(path.to_path_buf());

        *self.track_duration.lock().unwrap_or_else(|e| e.into_inner()) = total;
        *self.accumulated_position.lock().unwrap_or_else(|e| e.into_inner()) = Duration::ZERO;
        *self.playback_start.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());

        Ok(())
    }

    /// [`Self::play_file`], kept at the rate and channels the file begins with,
    /// and watched — see [`super::watch`]. For a podcast's episode.
    pub fn play_file_steady(&self, path: &Path, duration_hint: Option<Duration>) -> Result<()> {
        if matches!(path.try_exists(), Ok(false)) {
            return Err(KesslerError::Gone(path.to_path_buf()));
        }
        let source = open(path).map_err(|why| KesslerError::Playback(format!("Failed to decode {}: {why}", path.display())))?;
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let hint = source.total_duration().or(duration_hint);
        self.play_source(super::watch::steady(source, &name), path, hint);
        self.borrowed.set(false);
        self.steadied.set(true);
        Ok(())
    }

    /// What plays now was kept steady by the caller — see
    /// [`Self::play_file_steady`].
    pub fn mark_steadied(&self) {
        self.steadied.set(true);
    }

    /// Start a source opened elsewhere — a file still being fetched — as
    /// `play_file` starts a file: `path` names it, `duration_hint` is its
    /// length where the source does not know it.
    pub fn play_source<S: Source + Send + 'static>(&self, source: S, path: &Path, duration_hint: Option<Duration>) {
        let total = source.total_duration().or(duration_hint).unwrap_or(Duration::ZERO);
        self.borrowed.set(true);
        *self.parked.borrow_mut() = None;
        if let Some(sink) = self.woken_sink() {
            let sink = sink.lock().unwrap_or_else(|e| e.into_inner());
            sink.clear();
            sink.append(Tap::levelled(super::ahead::Ahead::new(source), Arc::clone(&self.heard), &self.leveling, path));
            sink.play();
        }
        let mut lined_up = self.lined_up.lock().unwrap_or_else(|e| e.into_inner());
        lined_up.clear();
        lined_up.push_back(total);
        let mut paths = self.playing_path.borrow_mut();
        paths.clear();
        paths.push_back(path.to_path_buf());
        *self.track_duration.lock().unwrap_or_else(|e| e.into_inner()) = total;
        *self.accumulated_position.lock().unwrap_or_else(|e| e.into_inner()) = Duration::ZERO;
        *self.playback_start.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
    }

    /// Start a station's stream, opened on another thread by
    /// [`super::stream::open`]: what `play_file` does with a file, with no
    /// length to know and nothing to lay behind it.
    pub fn play_stream(&self, source: Decoder<BufReader<super::stream::Stream>>, url: &str) {
        self.steadied.set(false);
        *self.parked.borrow_mut() = None;
        if let Some(sink) = self.woken_sink() {
            let sink = sink.lock().unwrap_or_else(|e| e.into_inner());
            sink.clear();
            sink.append(Tap::levelled(super::ahead::Ahead::live(source), Arc::clone(&self.heard), &self.leveling, Path::new(url)));
            sink.play();
        }
        let mut lined_up = self.lined_up.lock().unwrap_or_else(|e| e.into_inner());
        lined_up.clear();
        lined_up.push_back(Duration::ZERO);
        let mut paths = self.playing_path.borrow_mut();
        paths.clear();
        paths.push_back(PathBuf::from(url));
        *self.track_duration.lock().unwrap_or_else(|e| e.into_inner()) = Duration::ZERO;
        *self.accumulated_position.lock().unwrap_or_else(|e| e.into_inner()) = Duration::ZERO;
        *self.playback_start.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
    }

    /// Lay a file behind the one playing, so the two run into each other.
    ///
    /// Nothing about the sound changes when the hand-over comes: the sink
    /// simply reaches the end of one source and starts the next, which is what
    /// a record does between two bands. The old way — waiting for the sink to
    /// run dry, then opening the next file and starting the decoder — put a
    /// hole of a few hundred milliseconds between every two tracks, and on a
    /// record that runs through, that hole is the whole difference.
    ///
    /// Refused while nothing is playing, and refused twice over: exactly one
    /// track is ever laid out, because a sink cannot take a source back once
    /// it has been given one — see [`Self::lined_up_len`], which is how the
    /// player finds out that what it laid out is no longer what it wants.
    pub fn line_up(&self, path: &Path, duration_hint: Option<Duration>) -> bool {
        let Some(sink) = self.sink() else { return false };
        if self.lined_up.lock().unwrap_or_else(|e| e.into_inner()).len() != 1 {
            return false;
        }
        let Ok(source) = open(path) else {
            return false;
        };
        let total = source
            .total_duration()
            .or(duration_hint)
            .unwrap_or(Duration::ZERO);
        sink.lock()
            .unwrap_or_else(|e| e.into_inner())
            .append(Tap::levelled(super::ahead::Ahead::new(source), Arc::clone(&self.heard), &self.leveling, path));
        self.lined_up.lock().unwrap_or_else(|e| e.into_inner()).push_back(total);
        self.playing_path.borrow_mut().push_back(path.to_path_buf());
        true
    }

    /// The file being heard right now, as the sink has it.
    pub fn now_playing(&self) -> Option<PathBuf> {
        self.playing_path.borrow().front().cloned()
    }

    /// How many files the sink is holding, the one playing included.
    pub fn lined_up_len(&self) -> usize {
        self.lined_up.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// Whether the sink has moved on to the file laid out behind this one.
    ///
    /// Asked every frame. The sink says only how many sources are left, so a
    /// hand-over is seen rather than announced: one fewer than we put there
    /// means the next one has begun, and the clock that counts the position
    /// starts again from nothing.
    /// A sink that has run all the way dry is not a hand-over, whatever it is
    /// holding: both files ended between two frames, which the old path deals
    /// with properly. Counting it here would move the queue on twice for one
    /// track that was heard.
    pub fn handed_over(&self) -> bool {
        let Some(sink) = self.sink() else { return false };
        let left = sink.lock().unwrap_or_else(|e| e.into_inner()).len();
        let mut lined_up = self.lined_up.lock().unwrap_or_else(|e| e.into_inner());
        if left == 0 || left >= lined_up.len() || lined_up.len() < 2 {
            return false;
        }
        let mut paths = self.playing_path.borrow_mut();
        while lined_up.len() > left.max(1) {
            lined_up.pop_front();
            paths.pop_front();
        }
        let Some(total) = lined_up.front().copied() else {
            return false;
        };
        *self.track_duration.lock().unwrap_or_else(|e| e.into_inner()) = total;
        *self.accumulated_position.lock().unwrap_or_else(|e| e.into_inner()) = Duration::ZERO;
        *self.playback_start.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
        true
    }

    /// Pause playback.
    pub fn pause(&self) {
        let Some(sink) = self.sink() else { return };
        let sink = sink.lock().unwrap_or_else(|e| e.into_inner());
        if !sink.is_paused() {
            if let Some(start) = self.playback_start.lock().unwrap_or_else(|e| e.into_inner()).take() {
                let elapsed = start.elapsed();
                *self.accumulated_position.lock().unwrap_or_else(|e| e.into_inner()) += elapsed;
            }
            sink.pause();
            self.paused_at.set(Some(Instant::now()));
        }
    }

    /// Resume playback.
    pub fn resume(&self) {
        let long = self.paused_at.take().is_some_and(|at| at.elapsed() >= FRESH_AFTER) && !self.borrowed.get();
        let moved = self.output.borrow().as_ref().is_some_and(|open| {
            open.lost.load(Ordering::Relaxed)
                || device_now().is_some_and(|(_, name)| !name.is_empty() && !open.name.is_empty() && name != open.name)
        });
        if long || moved {
            self.open_afresh();
        }
        let parked = self.parked.borrow_mut().take();
        if let Some(path) = parked {
            self.unpark(&path);
            return;
        }
        let Some(sink) = self.sink() else { return };
        let sink = sink.lock().unwrap_or_else(|e| e.into_inner());
        if sink.is_paused() {
            *self.playback_start.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
            sink.play();
        }
    }

    /// Pick a parked track up where it was paused.
    fn unpark(&self, path: &Path) {
        let at = *self.accumulated_position.lock().unwrap_or_else(|e| e.into_inner());
        let Some(sink) = self.woken_sink() else { return };
        let Ok(source) = open(path) else { return };
        let sink = sink.lock().unwrap_or_else(|e| e.into_inner());
        sink.clear();
        if self.steadied.get() {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            sink.append(Tap::levelled(super::ahead::Ahead::new(super::watch::steady(source, &name)), Arc::clone(&self.heard), &self.leveling, path));
        } else {
            sink.append(Tap::levelled(super::ahead::Ahead::new(source), Arc::clone(&self.heard), &self.leveling, path));
        }
        let reached = if sink.try_seek(at).is_ok() { at } else { Duration::ZERO };
        sink.play();
        *self.accumulated_position.lock().unwrap_or_else(|e| e.into_inner()) = reached;
        *self.playback_start.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
        let mut paths = self.playing_path.borrow_mut();
        paths.clear();
        paths.push_back(path.to_path_buf());
    }

    /// Stop playback completely.
    pub fn stop(&self) {
        *self.parked.borrow_mut() = None;
        if let Some(sink) = self.sink() {
            sink.lock().unwrap_or_else(|e| e.into_inner()).stop();
        }
        *self.playback_start.lock().unwrap_or_else(|e| e.into_inner()) = None;
        *self.accumulated_position.lock().unwrap_or_else(|e| e.into_inner()) = Duration::ZERO;
    }

    /// The levelling table and switch, for the window to fill and set.
    pub fn leveling(&self) -> Arc<super::leveling::Leveling> {
        Arc::clone(&self.leveling)
    }

    /// Set volume.
    pub fn set_volume(&self, volume: f32) {
        let volume = volume.clamp(0.0, 1.0);
        self.volume.set(volume);
        if let Some(sink) = self.sink() {
            sink.lock().unwrap_or_else(|e| e.into_inner()).set_volume(volume);
        }
    }

    /// Whether the sink has finished playing (queue empty).
    ///
    /// With no device there is nothing to finish, so this stays false rather
    /// than making the queue race to its end.
    pub fn is_finished(&self) -> bool {
        match self.sink() {
            Some(sink) => sink.lock().unwrap_or_else(|e| e.into_inner()).empty(),
            None => false,
        }
    }

    /// Jump to a position in the current track.
    ///
    /// Returns false when the format cannot be seeked (rodio refuses some) or
    /// there is no device. The position bookkeeping runs off wall-clock time,
    /// so it has to be reset here or the display would keep counting from
    /// wherever it was.
    pub fn seek(&self, position: Duration) -> bool {
        if self.parked.borrow().is_some() {
            *self.accumulated_position.lock().unwrap_or_else(|e| e.into_inner()) = position;
            return true;
        }
        let Some(sink) = self.sink() else { return false };
        let sink = sink.lock().unwrap_or_else(|e| e.into_inner());

        if sink.try_seek(position).is_err() {
            return false;
        }

        *self.accumulated_position.lock().unwrap_or_else(|e| e.into_inner()) = position;
        *self.playback_start.lock().unwrap_or_else(|e| e.into_inner()) = if sink.is_paused() {
            None
        } else {
            Some(Instant::now())
        };
        true
    }

    /// Get the current playback position — never past the track's end. The
    /// clock is a stopwatch beside the sound, not the sound itself; with no
    /// card to play on it ran on to „0:40 / 0:04" (smoke test).
    pub fn position(&self) -> Duration {
        let acc = *self.accumulated_position.lock().unwrap_or_else(|e| e.into_inner());
        let at = if let Some(start) = *self.playback_start.lock().unwrap_or_else(|e| e.into_inner()) {
            acc + start.elapsed()
        } else {
            acc
        };
        let total = self.duration();
        if total.is_zero() { at } else { at.min(total) }
    }

    /// Get the total duration of the current track.
    pub fn duration(&self) -> Duration {
        *self.track_duration.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Get the playback progress as a fraction 0.0..1.0.
    pub fn progress(&self) -> f64 {
        let dur = self.duration();
        if dur.is_zero() {
            return 0.0;
        }
        let pos = self.position();
        (pos.as_secs_f64() / dur.as_secs_f64()).clamp(0.0, 1.0)
    }
}

/// Open a file for playback.
///
/// `Decoder::try_from(File)` rather than `Decoder::new(reader)`: only the
/// first tells the decoder how long the file is, and the MP4 reader cannot
/// work without that — it has to reach the index, which may sit behind the
/// audio. Handed a reader of unknown length it tries to seek, fails, and the
/// failure is not returned but reached as an `unreachable!()` deep in rodio.
///
/// Which is why the panic is caught as well. One odd file in eight thousand
/// should be a sentence in the status line, not the end of the evening.
pub fn open(
    path: &Path,
) -> std::result::Result<super::gapless::Trimmed<Decoder<BufReader<File>>>, String> {
    let trim = super::gapless::trim_for(path);
    let path = path.to_path_buf();
    let opened = quietly(move || {
        let file = File::open(&path).map_err(|e| e.to_string())?;
        Decoder::try_from(file).map_err(|e| e.to_string())
    });

    match opened {
        Ok(result) => result.map(|decoder| super::gapless::Trimmed::new(decoder, trim)),
        Err(_) => Err("the decoder gave up on this file".to_string()),
    }
}

thread_local! {
    /// Whether a panic on this thread is one [`quietly`] expects.
    static QUIET: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Run `work`, a panic in it caught and not written to the terminal.
///
/// Files are opened on several threads at once — the player's, the one
/// measuring loudness. One hook now, set once, silent only for the thread that
/// asked.
pub(super) fn quietly<T>(work: impl FnOnce() -> T + std::panic::UnwindSafe) -> std::thread::Result<T> {
    static HOOKED: std::sync::Once = std::sync::Once::new();
    HOOKED.call_once(|| {
        let before = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if !QUIET.with(std::cell::Cell::get) {
                before(info);
            }
        }));
    });
    QUIET.with(|quiet| quiet.set(true));
    let done = std::panic::catch_unwind(work);
    QUIET.with(|quiet| quiet.set(false));
    done
}

/// How many frames are kept for whoever wants to draw them.
///
/// Two thousand is a little under a twentieth of a second at CD rate: long
/// enough for the lowest band either picture shows to have a couple of whole
/// waves in it, and a little more than one frame of the search field's
/// picture at thirty a second — so the vectorscope draws every sample that
/// went past, with a margin for its filters to settle in.
const KEPT: usize = 2048;

/// How many frames the ring holds: three seconds at CD rate, well over one at
/// 96 kHz. The pictures take the last [`KEPT`] of them as they always did; the
/// long-exposed vectorscope takes all it missed since its last frame — a
/// quarter of a second, or a whole one in step with the clock — and lays it
/// down in steps; see `gui::library::picture::look_in`. A megabyte, written in
/// blocks.
const HELD: usize = 131_072;

/// How many frames are collected before the ring is touched.
///
/// The tap sits in the audio thread, between the decoder and the card, and
/// that thread has a deadline: a lock taken per sample would be forty
/// thousand locks a second for a picture nobody is even always looking at.
/// So the frames pile up in the tap's own scratch and go over in blocks.
const BLOCK: usize = 128;

/// The sound itself, for the things that draw it.
///
/// Two readings of the same stream: the last few hundredths of a second of
/// it folded to one channel, and the same moments with the two channels kept
/// apart. The first is what a spectrum is made of — see [`crate::gui::sound`]; the
/// second is what a vectorscope is made of — see [`crate::gui::scope`].
///
/// Written by the audio thread, read by the window. Neither waits for the
/// other for longer than a memcpy of sixteen kilobytes.
#[derive(Debug, Default)]
pub struct Heard {
    ring: Mutex<Ring>,
    /// Frames a second, of whatever was heard last. Nought before anything
    /// has been.
    rate: std::sync::atomic::AtomicU32,
}

#[derive(Debug)]
struct Ring {
    /// Left and right. A mono file is heard as the same sample twice, which
    /// is what it is: a sound in the middle.
    frames: Vec<[f32; 2]>,
    /// Where the next frame goes.
    at: usize,
}

impl Default for Ring {
    fn default() -> Self {
        Self {
            frames: vec![[0.0; 2]; HELD],
            at: 0,
        }
    }
}

impl Heard {
    /// The last [`KEPT`] frames, oldest first, each folded to one sample.
    ///
    /// Copied out rather than borrowed: the audio thread is writing into the
    /// ring while the window draws, and a picture built from a buffer that is
    /// being written under it is a picture with a seam in it.
    pub fn recent(&self) -> Vec<f32> {
        self.recent_pairs()
            .into_iter()
            .map(|[left, right]| (left + right) * 0.5)
            .collect()
    }

    /// The last [`KEPT`] frames, oldest first, left and right.
    pub fn recent_pairs(&self) -> Vec<[f32; 2]> {
        self.last_pairs(KEPT)
    }

    /// The last `wanted` frames, at most [`HELD`], oldest first.
    pub fn last_pairs(&self, wanted: usize) -> Vec<[f32; 2]> {
        let wanted = wanted.min(HELD);
        if wanted == 0 {
            return Vec::new();
        }
        let ring = match self.ring.lock() {
            Ok(ring) => ring,
            Err(poisoned) => poisoned.into_inner(),
        };
        let mut out = Vec::with_capacity(wanted);
        let from = (ring.at + HELD - wanted) % HELD;
        if from < ring.at {
            out.extend_from_slice(&ring.frames[from..ring.at]);
        } else {
            out.extend_from_slice(&ring.frames[from..]);
            out.extend_from_slice(&ring.frames[..ring.at]);
        }
        out
    }

    /// Frames a second of what [`Heard::recent_pairs`] holds. A filter tuned
    /// to a pitch has to know how far apart the samples are.
    pub fn rate(&self) -> u32 {
        self.rate.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn take(&self, block: &[[f32; 2]], rate: u32) {
        if let Ok(mut ring) = self.ring.lock() {
            for frame in block {
                let at = ring.at;
                ring.frames[at] = *frame;
                ring.at = (at + 1) % HELD;
            }
        }
        self.rate
            .store(rate, std::sync::atomic::Ordering::Relaxed);
    }

    /// Nothing is playing any more: the picture should settle rather than
    /// freeze on the last thing it heard.
    fn quiet(&self) {
        if let Ok(mut ring) = self.ring.lock() {
            ring.frames.fill([0.0; 2]);
            ring.at = 0;
        }
    }
}

/// A source that hands every sample on — levelled, when levelling is on —
/// and keeps a copy.
///
/// The one honest way to draw the music: not the file's tags, not the
/// position in the track, but the samples on their way to the card. It costs
/// one copy per sample, and nothing at all when nobody looks.
///
/// The first two channels are kept as left and right; a mono file is heard in
/// both, and anything past the second channel — a centre, a surround — is
/// left out rather than guessed into one side or the other.
struct Tap<S> {
    inner: S,
    heard: Arc<Heard>,
    scratch: Vec<[f32; 2]>,
    channels: u16,
    rate: u32,
    /// The frame being put together, and how many of its samples are in.
    pending: ([f32; 2], u16),
    /// Levelling's switch, read per sample so a change is heard at once,
    /// and this file's two factors — see [`super::leveling`].
    switch: Option<Arc<std::sync::atomic::AtomicU8>>,
    factors: super::leveling::Factors,
    /// The factor being applied, which walks to the wanted one over some
    /// milliseconds rather than jumping — a jump in the middle of a wave is
    /// a click. `None` until the first sample, which starts where it should.
    gain_now: Option<f32>,
    /// The equalizer, after the levelling — see [`super::eq`].
    eq: super::eq::Running,
}

impl<S> Tap<S>
where
    S: Source,
{
    fn new(inner: S, heard: Arc<Heard>) -> Self {
        let channels = inner.channels().get();
        let rate = inner.sample_rate().get();
        Self {
            inner,
            heard,
            scratch: Vec::with_capacity(BLOCK),
            channels,
            rate,
            pending: ([0.0; 2], 0),
            switch: None,
            factors: super::leveling::Factors { track: 1.0, album: 1.0 },
            gain_now: None,
            eq: super::eq::Running::new(&super::eq::SETTINGS, rate),
        }
    }

    /// The same, levelled by the engine's table for `path`.
    fn levelled(inner: S, heard: Arc<Heard>, leveling: &super::leveling::Leveling, path: &Path) -> Self {
        let mut tap = Self::new(inner, heard);
        tap.switch = Some(leveling.switch());
        tap.factors = leveling.factors(path);
        tap
    }
}

impl<S> Iterator for Tap<S>
where
    S: Source,
{
    type Item = rodio::Sample;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let mut sample = self.inner.next()?;
        if let Some(switch) = &self.switch {
            let wanted = super::leveling::pick(
                switch.load(std::sync::atomic::Ordering::Relaxed),
                self.factors,
            );
            let now = self.gain_now.get_or_insert(wanted);
            if *now != wanted {
                let step = 1.0 / (0.03 * self.rate.max(1) as f32 * f32::from(self.channels.max(1)));
                *now += (wanted - *now).clamp(-step, step);
            }
            if *now != 1.0 {
                sample *= *now;
            }
        }

        sample = self.eq.process(sample, self.pending.1);

        let (frame, filled) = &mut self.pending;
        match *filled {
            0 => *frame = [sample, sample],
            1 => frame[1] = sample,
            _ => {}
        }
        *filled += 1;
        if *filled >= self.channels {
            self.scratch.push(*frame);
            self.pending = ([0.0; 2], 0);
        }
        if self.scratch.len() >= BLOCK {
            self.heard.take(&self.scratch, self.rate);
            self.scratch.clear();
        }
        Some(sample)
    }
}

impl<S> Source for Tap<S>
where
    S: Source,
{
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }

    fn channels(&self) -> rodio::ChannelCount {
        self.inner.channels()
    }

    fn sample_rate(&self) -> rodio::SampleRate {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }

    /// Hand a seek straight through to the file underneath.
    ///
    /// Not optional, and the reason for the test at the foot of this file: a
    /// `Source` that does not say this gets the default, and the default
    /// refuses every seek. So putting anything at all in front of the decoder
    /// takes seeking away from every format at once — and the window can only
    /// report what it is told, about files that seek perfectly well. That is
    /// what happened when this tap was put in the path.
    ///
    /// Everything the tap is holding goes with the jump: half a frame that
    /// belongs to the old position, a block that was still filling, and the
    /// ring the picture is drawn from. Otherwise the first thing drawn after a
    /// seek is the second before it.
    fn try_seek(&mut self, pos: Duration) -> std::result::Result<(), rodio::source::SeekError> {
        self.inner.try_seek(pos)?;
        self.pending = ([0.0; 2], 0);
        self.scratch.clear();
        self.heard.quiet();
        self.eq.reset();
        Ok(())
    }
}

#[cfg(test)]
mod format_support {
    use crate::library::{fixtures, metadata};

    /// A file of this format, as hex — or `None` if we never made one.
    fn fixture(extension: &str) -> Option<&'static str> {
        Some(match extension {
            "mp3" => fixtures::MP3,
            "flac" => fixtures::FLAC,
            "ogg" | "oga" => fixtures::OGG,
            "wav" => fixtures::WAV,
            "m4a" => fixtures::M4A,
            "aif" | "aiff" => fixtures::AIFF,
            "aifc" => fixtures::AIFC,
            _ => return None,
        })
    }

    #[test]
    fn every_format_we_claim_has_a_file_to_prove_it() {
        for extension in metadata::SUPPORTED_EXTENSIONS {
            assert!(
                fixture(extension).is_some(),
                "{extension} is listed as playable but has no fixture here — \
                 add one, or take it off the list"
            );
        }
    }

    /// Exactly what playback does, minus the sound card — the same function,
    /// so a format that passes here is a format that plays.
    fn opens(path: &std::path::Path) -> Result<(), String> {
        super::open(path).map(|_| ())
    }

    #[test]
    fn every_format_we_claim_actually_decodes() {
        let dir = fixtures::scratch("formats");
        let mut failed: Vec<String> = Vec::new();

        for extension in metadata::SUPPORTED_EXTENSIONS {
            let hex = fixture(extension).expect("checked by the test above");
            let path = dir.join(format!("silence.{extension}"));
            fixtures::place(&path, hex);

            if let Err(why) = opens(&path) {
                failed.push(format!("{extension}: {why}"));
            }
        }

        assert!(
            failed.is_empty(),
            "the library would import these and then play nothing:\n  {}",
            failed.join("\n  ")
        );
    }

    /// Apple Lossless shares its extension with AAC, so the loop above never
    /// meets it: an `.m4a` from iTunes' lossless import said „unrecognized
    /// format" without the decoder for it built in.
    #[test]
    fn apple_lossless_decodes_too() {
        let dir = fixtures::scratch("alac");
        let path = dir.join("silence.m4a");
        fixtures::place(&path, fixtures::M4A_ALAC);
        let samples: Vec<f32> = super::open(&path).expect("apple lossless opens").collect();
        assert!(!samples.is_empty(), "and gives sound");
    }

    /// Read ahead, every format gives the same samples as read on the spot
    /// — the store changes when the file is decoded, not what comes out.
    #[test]
    fn reading_ahead_changes_nothing_that_is_heard() {
        let dir = fixtures::scratch("ahead");
        for extension in metadata::SUPPORTED_EXTENSIONS {
            let hex = fixture(extension).expect("checked above");
            let path = dir.join(format!("silence.{extension}"));
            fixtures::place(&path, hex);
            let direct: Vec<f32> = super::open(&path).unwrap().collect();
            let ahead: Vec<f32> = super::super::ahead::Ahead::new(super::open(&path).unwrap()).collect();
            assert_eq!(direct.len(), ahead.len(), "{extension}");
            assert!(direct == ahead, "{extension}");
        }
    }

    /// Point this at a file of your own to find out whether it would play.
    ///
    ///   KESSLER_TRY_FILE=".../something.m4a" cargo test a_real_file -- --nocapture
    ///
    /// The fixtures here are made by ffmpeg, and a format has corners that one
    /// encoder may never turn. When the question is "will my two hundred files
    /// play", only one of those files can answer it.
    #[test]
    fn a_real_file_decodes() {
        let Ok(path) = std::env::var("KESSLER_TRY_FILE") else {
            println!("KESSLER_TRY_FILE is not set — nothing to try");
            return;
        };
        let path = std::path::PathBuf::from(path);
        match opens(&path) {
            Ok(()) => println!("{}: opens", path.display()),
            Err(why) => panic!("{}: {why}", path.display()),
        }
    }

    /// The two lists must stay disjoint as formats move between them.
    #[test]
    fn nothing_is_both_playable_and_passed_over() {
        for extension in metadata::SUPPORTED_EXTENSIONS {
            assert!(
                !metadata::FOREIGN_EXTENSIONS.contains(extension),
                "{extension} is in both lists"
            );
        }
    }
}

#[cfg(test)]
mod tap {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A source that counts the seeks it is asked for and plays a steady tone.
    struct Stand {
        seeks: Arc<AtomicUsize>,
        left: usize,
    }

    impl Iterator for Stand {
        type Item = rodio::Sample;
        fn next(&mut self) -> Option<Self::Item> {
            if self.left == 0 {
                return None;
            }
            self.left -= 1;
            Some(0.5)
        }
    }

    impl Source for Stand {
        fn current_span_len(&self) -> Option<usize> {
            None
        }
        fn channels(&self) -> rodio::ChannelCount {
            rodio::nz!(1)
        }
        fn sample_rate(&self) -> rodio::SampleRate {
            rodio::nz!(44_100)
        }
        fn total_duration(&self) -> Option<Duration> {
            None
        }
        fn try_seek(&mut self, _: Duration) -> std::result::Result<(), rodio::source::SeekError> {
            self.seeks.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
    }

    /// The tap hands a seek through to the file underneath.
    ///
    /// The regression this exists for: putting the tap in front of the decoder
    /// gave `Source:try_seek` its default, which refuses — so every track in
    /// every format answered, and the tap, which has nothing to do with
    /// seeking, was the last place anybody would look. Anything else put in
    /// this path must pass this test too.
    #[test]
    fn a_seek_reaches_the_file_through_the_tap() {
        let seeks = Arc::new(AtomicUsize::new(0));
        let mut tap = Tap::new(
            Stand {
                seeks: Arc::clone(&seeks),
                left: usize::MAX,
            },
            Arc::new(Heard::default()),
        );

        assert!(
            tap.try_seek(Duration::from_secs(5)).is_ok(),
            "the tap refused a seek the file would have taken"
        );
        assert_eq!(seeks.load(Ordering::Relaxed), 1, "the file was never asked");
    }

    /// And throws away what it was holding, so the picture does not carry the
    /// old position across the jump.
    #[test]
    fn a_seek_empties_what_the_tap_was_holding() {
        let heard = Arc::new(Heard::default());
        let mut tap = Tap::new(
            Stand {
                seeks: Arc::new(AtomicUsize::new(0)),
                left: usize::MAX,
            },
            Arc::clone(&heard),
        );

        for _ in 0..BLOCK * 3 {
            tap.next();
        }
        assert!(
            heard.recent().iter().any(|s| *s != 0.0),
            "nothing reached the ring to begin with"
        );
        for _ in 0..BLOCK / 2 {
            tap.next();
        }
        assert!(!tap.scratch.is_empty());

        tap.try_seek(Duration::from_secs(5)).unwrap();
        assert!(tap.scratch.is_empty(), "the tap kept a half-filled block");
        assert_eq!(tap.pending.1, 0, "the tap kept half a frame");
        assert!(
            heard.recent().iter().all(|s| *s == 0.0),
            "the ring still holds the second before the jump"
        );
    }
    /// Two channels go into the ring as two, not folded: the vectorscope in
    /// the search field draws the left against the right, and a ring of means
    /// would draw every record as mono. The mean is still there for the
    /// spectrum, worked out on the way out.
    #[test]
    fn the_ring_keeps_left_and_right_apart() {
        struct Wide(usize);
        impl Iterator for Wide {
            type Item = rodio::Sample;
            fn next(&mut self) -> Option<Self::Item> {
                self.0 += 1;
                Some(if self.0 % 2 == 1 { 0.25 } else { -0.75 })
            }
        }
        impl Source for Wide {
            fn current_span_len(&self) -> Option<usize> {
                None
            }
            fn channels(&self) -> rodio::ChannelCount {
                rodio::nz!(2)
            }
            fn sample_rate(&self) -> rodio::SampleRate {
                rodio::nz!(48_000)
            }
            fn total_duration(&self) -> Option<Duration> {
                None
            }
        }

        let heard = Arc::new(Heard::default());
        let mut tap = Tap::new(Wide(0), Arc::clone(&heard));
        for _ in 0..BLOCK * 2 * 3 {
            tap.next();
        }
        let last = *heard.recent_pairs().last().unwrap();
        assert_eq!(last, [0.25, -0.75]);
        assert_eq!(*heard.recent().last().unwrap(), -0.25);
        assert_eq!(heard.rate(), 48_000, "the ring does not know how fast it runs");
    }

    /// And a mono file is heard in both, which is where it is: the middle.
    #[test]
    fn a_mono_file_is_heard_in_both_channels() {
        let heard = Arc::new(Heard::default());
        let mut tap = Tap::new(
            Stand {
                seeks: Arc::new(AtomicUsize::new(0)),
                left: usize::MAX,
            },
            Arc::clone(&heard),
        );
        for _ in 0..BLOCK * 3 {
            tap.next();
        }
        assert_eq!(*heard.recent_pairs().last().unwrap(), [0.5, 0.5]);
    }

    /// The ring keeps more than the pictures take: the last [`KEPT`] as
    /// before, and as far back as [`HELD`] for a long exposure, oldest first
    /// across the seam where it wraps.
    #[test]
    fn the_ring_hands_back_as_much_as_is_asked() {
        let heard = Heard::default();
        let block: Vec<[f32; 2]> = (0..HELD + 100).map(|i| [i as f32, 0.0]).collect();
        heard.take(&block, 44_100);
        assert_eq!(heard.recent_pairs().len(), KEPT);
        let last = (HELD + 99) as f32;
        assert_eq!(heard.recent_pairs().last().unwrap()[0], last);
        let long = heard.last_pairs(5000);
        assert_eq!(long.len(), 5000);
        assert_eq!(long[0][0], last - 4999.0);
        assert!(long.windows(2).all(|w| w[1][0] == w[0][0] + 1.0), "oldest first, without a seam");
        assert_eq!(heard.last_pairs(HELD * 2).len(), HELD);
        assert!(heard.last_pairs(0).is_empty());
    }
}

#[cfg(test)]
mod gapless_probe {
    /// Not a test of the program but of the decoders under it: two halves of
    /// one continuous tone, decoded the way playback decodes them, laid end
    /// to end — how many samples of silence, or of a jump, stand at the join?
    ///
    /// `GAP_DIR` names a folder holding `a.<ext>` and `b.<ext>`.
    #[test]
    #[ignore]
    fn the_join_between_two_halves() {
        use rodio::Source;
        let Ok(dir) = std::env::var("GAP_DIR") else { return };
        for ext in ["wav", "flac", "mp3", "m4a"] {
            let a = std::path::Path::new(&dir).join(format!("a.{ext}"));
            let b = std::path::Path::new(&dir).join(format!("b.{ext}"));
            let (Ok(a), Ok(b)) = (super::open(&a), super::open(&b)) else {
                println!("{ext}: will not open");
                continue;
            };
            let channels = usize::from(a.channels().get());
            let first: Vec<f32> = a.collect();
            let second: Vec<f32> = b.collect();
            let quiet = |s: &[f32]| s.iter().take_while(|v| v.abs() < 1e-3).count() / channels;
            let tail = {
                let mut r = first.clone();
                r.reverse();
                quiet(&r)
            };
            let head = quiet(&second);
            println!(
                "{ext}: first {} frames ({:.3} s), silence at its end {tail}, silence at the start of the next {head}",
                first.len() / channels,
                first.len() as f32 / channels as f32 / 44100.0
            );
        }
    }
}
