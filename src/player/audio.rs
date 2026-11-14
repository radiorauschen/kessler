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

pub struct AudioEngine {
    output: RefCell<Option<Output>>,
    volume: Cell<f32>,
    parked: RefCell<Option<PathBuf>>,
    paused_at: Cell<Option<Instant>>,
    borrowed: Cell<bool>,
    steadied: Cell<bool>,
    playing_path: RefCell<VecDeque<PathBuf>>,
    lined_up: Arc<Mutex<VecDeque<Duration>>>,
    playback_start: Arc<Mutex<Option<Instant>>>,
    accumulated_position: Arc<Mutex<Duration>>,
    track_duration: Arc<Mutex<Duration>>,
    unavailable_reason: RefCell<Option<String>>,
    heard: Arc<Heard>,
    leveling: Arc<super::leveling::Leveling>,
}

struct Output {
    _stream: MixerDeviceSink,
    sink: Arc<Mutex<Player>>,
    idle_since: Option<Instant>,
    rate: u32,
    asked_at: Instant,
    name: String,
    lost: Arc<AtomicBool>,
}

const FRESH_AFTER: Duration = Duration::from_secs(10);

const DEVICE_ASKED_EVERY: Duration = Duration::from_secs(3);

fn device_now() -> Option<(u32, String)> {
    use rodio::cpal::traits::{DeviceTrait, HostTrait};
    let device = rodio::cpal::default_host().default_output_device()?;
    let rate = device.default_output_config().ok()?.sample_rate();
    let name = device.description().map(|d| d.name().to_string()).unwrap_or_default();
    Some((rate, name))
}

const REST_EMPTY: Duration = Duration::from_secs(20);
const REST_PAUSED: Duration = Duration::from_secs(5 * 60);

impl AudioEngine {
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

    fn sink(&self) -> Option<Arc<Mutex<Player>>> {
        self.output.borrow().as_ref().map(|o| Arc::clone(&o.sink))
    }

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
                if let Some((rate, name)) = device_now().filter(|(rate, _)| *rate != open.rate) {
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

    pub fn heard(&self) -> Arc<Heard> {
        Arc::clone(&self.heard)
    }

    pub fn rest_due_in(&self) -> Option<Duration> {
        let output = self.output.borrow();
        let open = output.as_ref()?;
        let since = open.idle_since?;
        let empty = open.sink.lock().unwrap_or_else(|e| e.into_inner()).empty();
        let long_enough = if empty { REST_EMPTY } else { REST_PAUSED };
        Some(long_enough.saturating_sub(since.elapsed()) + Duration::from_millis(50))
    }

    pub fn is_available(&self) -> bool {
        self.unavailable_reason.borrow().is_none()
    }

    pub fn unavailable_reason(&self) -> Option<String> {
        self.unavailable_reason.borrow().clone()
    }

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

    pub fn mark_steadied(&self) {
        self.steadied.set(true);
    }

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

    pub fn play_stream(&self, source: Decoder<BufReader<super::stream::Stream>>, url: &str) {
        self.steadied.set(false);
        *self.parked.borrow_mut() = None;
        if let Some(sink) = self.woken_sink() {
            let sink = sink.lock().unwrap_or_else(|e| e.into_inner());
            sink.clear();
            sink.append(Tap::levelled(super::ahead::Ahead::new(source), Arc::clone(&self.heard), &self.leveling, Path::new(url)));
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

    pub fn now_playing(&self) -> Option<PathBuf> {
        self.playing_path.borrow().front().cloned()
    }

    pub fn lined_up_len(&self) -> usize {
        self.lined_up.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

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

    pub fn stop(&self) {
        *self.parked.borrow_mut() = None;
        if let Some(sink) = self.sink() {
            sink.lock().unwrap_or_else(|e| e.into_inner()).stop();
        }
        *self.playback_start.lock().unwrap_or_else(|e| e.into_inner()) = None;
        *self.accumulated_position.lock().unwrap_or_else(|e| e.into_inner()) = Duration::ZERO;
    }

    pub fn leveling(&self) -> Arc<super::leveling::Leveling> {
        Arc::clone(&self.leveling)
    }

    pub fn set_volume(&self, volume: f32) {
        let volume = volume.clamp(0.0, 1.0);
        self.volume.set(volume);
        if let Some(sink) = self.sink() {
            sink.lock().unwrap_or_else(|e| e.into_inner()).set_volume(volume);
        }
    }

    pub fn is_finished(&self) -> bool {
        match self.sink() {
            Some(sink) => sink.lock().unwrap_or_else(|e| e.into_inner()).empty(),
            None => false,
        }
    }

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

    pub fn duration(&self) -> Duration {
        *self.track_duration.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn progress(&self) -> f64 {
        let dur = self.duration();
        if dur.is_zero() {
            return 0.0;
        }
        let pos = self.position();
        (pos.as_secs_f64() / dur.as_secs_f64()).clamp(0.0, 1.0)
    }
}

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
    static QUIET: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

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

const KEPT: usize = 2048;

const HELD: usize = 131_072;

const BLOCK: usize = 128;

#[derive(Debug, Default)]
pub struct Heard {
    ring: Mutex<Ring>,
    rate: std::sync::atomic::AtomicU32,
}

#[derive(Debug)]
struct Ring {
    frames: Vec<[f32; 2]>,
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
    pub fn recent(&self) -> Vec<f32> {
        self.recent_pairs()
            .into_iter()
            .map(|[left, right]| (left + right) * 0.5)
            .collect()
    }

    pub fn recent_pairs(&self) -> Vec<[f32; 2]> {
        self.last_pairs(KEPT)
    }

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

    fn quiet(&self) {
        if let Ok(mut ring) = self.ring.lock() {
            ring.frames.fill([0.0; 2]);
            ring.at = 0;
        }
    }
}

struct Tap<S> {
    inner: S,
    heard: Arc<Heard>,
    scratch: Vec<[f32; 2]>,
    channels: u16,
    rate: u32,
    pending: ([f32; 2], u16),
    switch: Option<Arc<std::sync::atomic::AtomicU8>>,
    factors: super::leveling::Factors,
    gain_now: Option<f32>,
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

    #[test]
    fn apple_lossless_decodes_too() {
        let dir = fixtures::scratch("alac");
        let path = dir.join("silence.m4a");
        fixtures::place(&path, fixtures::M4A_ALAC);
        let samples: Vec<f32> = super::open(&path).expect("apple lossless opens").collect();
        assert!(!samples.is_empty(), "and gives sound");
    }

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
