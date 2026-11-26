//! Reading ahead: a file decoded a few seconds before it is heard.
//!
//! Without this, the sound card's own thread asked the decoder for every sample
//! at the moment it needed it — which meant reading the file and unpacking FLAC
//! in the few milliseconds the card allows. Nearly always that is plenty. With
//! the machine busy — an image being generated, the memory full and paging to
//! disk — a block arrived late, and a late block is a gap, and a gap is a
//! click.
//!
//! So the file is decoded on a thread of its own into a store of about
//! [`AHEAD`], and the card only ever takes from the store. A second of the
//! machine being elsewhere is then a second less in the store, not a gap.
//!
//! Seeking empties the store and asks the reading thread to go to the new
//! place; until it has, silence is heard. The seek used to be done right there,
//! on the sound card's thread, with the window waiting on it — over a hundred
//! milliseconds in a long MP3.
//!
//! A station is read the same way, but can only be ahead by what the network
//! has already brought. So it waits for a cushion of [`CUSHION`] before it is
//! heard, and again after the network has let it run dry: otherwise playing
//! went on with the first scrap that came, ran dry at once, and a wobbly
//! connection was heard as a stutter rather than as one pause.

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use rodio::Source;

/// How much is decoded ahead of what is heard.
pub const AHEAD: Duration = Duration::from_secs(3);

/// How much of a station is gathered before it is heard, and again after it
/// ran dry.
pub const CUSHION: Duration = Duration::from_secs(2);

/// How much of a station is kept at most: what the network brings faster
/// than it is heard — the burst a server sends as it answers, the catching up
/// after a wobble — is kept for the next wobble.
pub const LIVE_ROOM: Duration = Duration::from_secs(8);

/// How much of the station playing the store holds, in milliseconds — for the
/// field-strength needle. One station plays at a time.
static STATION_HELD: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Which station's store [`STATION_HELD`] speaks for: the newest. One being
/// let go of — its thread still in the middle of a chunk, or dropped after
/// the next has started — may not write over it.
static STATION_NOW: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Counted up for each station's store, for [`STATION_NOW`].
static STATIONS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// How much of the station playing has been brought ahead of what is heard.
pub fn station_held() -> Duration {
    Duration::from_millis(STATION_HELD.load(std::sync::atomic::Ordering::Relaxed))
}

/// Samples decoded in one go, and handed over as one piece.
const CHUNK: usize = 4096;

/// What the two threads share.
struct Store {
    chunks: VecDeque<Vec<f32>>,
    /// The file has run out.
    done: bool,
    /// The source has been let go of; the thread is to stop.
    stop: bool,
    /// A place to go to, asked for and not yet gone to by the thread.
    seek_to: Option<Duration>,
    /// Gone somewhere, and nothing from the new place in the store yet:
    /// silence rather than waiting.
    seeking: bool,
    /// Since when the store has been empty with the card asking: silence
    /// rather than waiting, until the cushion is there — see
    /// [`STARVED_AFTER`].
    starved: Option<std::time::Instant>,
    /// A station not heard yet: its first cushion being gathered.
    first: bool,
}

/// How long the sound card's thread waits for an empty store before it plays
/// silence instead. It used to wait for as long as it took — and a station
/// whose connection went without being closed never sent another byte: the
/// card's thread stood still, and with it everything that waits for that thread
/// to let a source go — the next press on pause, on another station, on a track
/// — which is the window. A machine too busy to decode is heard the same way as
/// before, as a gap.
const STARVED_AFTER: Duration = Duration::from_millis(30);

struct Shared<S> {
    /// The decoder. Held by the reading thread while it decodes a chunk and
    /// puts it in the store, and by a seek — so the two never cross.
    inner: Mutex<S>,
    store: Mutex<Store>,
    /// Rung when there is something in the store, or room in it.
    changed: Condvar,
    /// How many chunks the store holds when full.
    room: usize,
    /// How many chunks end a silence: one for a file, which the machine can
    /// always read faster than it is heard; [`CUSHION`] for a station.
    cushion: usize,
    /// Whether it is a station, for the measurement.
    live: bool,
    /// How long one chunk plays, in milliseconds, for [`STATION_HELD`].
    chunk_ms: u64,
    /// Its number, for [`STATION_NOW`]; nought for a file.
    station: u64,
}

/// A source that is read ahead of time — see the module.
pub struct Ahead<S: Source + Send + 'static> {
    shared: Arc<Shared<S>>,
    current: Vec<f32>,
    at: usize,
    channels: rodio::ChannelCount,
    rate: rodio::SampleRate,
    total: Option<Duration>,
}

impl<S: Source + Send + 'static> Ahead<S> {
    /// A file, read [`AHEAD`] ahead.
    pub fn new(inner: S) -> Self {
        Self::with(inner, AHEAD, None)
    }

    /// A station: silent until [`CUSHION`] has come, and again after it ran
    /// dry.
    pub fn live(inner: S) -> Self {
        Self::with(inner, LIVE_ROOM, Some(CUSHION))
    }

    fn with(inner: S, ahead: Duration, cushion: Option<Duration>) -> Self {
        let channels = inner.channels();
        let rate = inner.sample_rate();
        let total = inner.total_duration();
        let per_second = rate.get() as usize * usize::from(channels.get());
        let chunks = |d: Duration| (per_second * d.as_millis() as usize / 1000).div_ceil(CHUNK);
        let room = chunks(ahead).max(2);
        let cushion_chunks = cushion.map_or(1, |c| chunks(c).clamp(1, room - 1));
        let live = cushion.is_some();
        let store = Store {
            chunks: VecDeque::new(),
            done: false,
            stop: false,
            seek_to: None,
            seeking: false,
            starved: live.then(std::time::Instant::now),
            first: live,
        };
        let shared = Arc::new(Shared {
            inner: Mutex::new(inner),
            store: Mutex::new(store),
            changed: Condvar::new(),
            room,
            cushion: cushion_chunks,
            live,
            chunk_ms: (CHUNK as u64 * 1000 / per_second.max(1) as u64).max(1),
            station: if live {
                let number = STATIONS.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                STATION_NOW.store(number, std::sync::atomic::Ordering::Relaxed);
                number
            } else {
                0
            },
        });
        let reading = Arc::clone(&shared);
        std::thread::Builder::new()
            .name("kessler-ahead".into())
            .spawn(move || read(&reading))
            .ok();
        Ahead { shared, current: Vec::new(), at: 0, channels, rate, total }
    }
}

/// The reading thread: fill the store, wait while it is full or the file
/// has run out, stop when told.
fn read<S: Source>(shared: &Shared<S>) {
    loop {
        let seek_to = {
            let mut store = shared.store.lock().unwrap_or_else(|e| e.into_inner());
            while !store.stop && store.seek_to.is_none() && (store.done || store.chunks.len() >= shared.room) {
                store = shared.changed.wait(store).unwrap_or_else(|e| e.into_inner());
            }
            if store.stop {
                return;
            }
            store.seek_to.take()
        };
        let mut inner = shared.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(to) = seek_to {
            if let Err(e) = inner.try_seek(to) {
                super::watch::note(format!("ton: sprung nach {:.1} s ging nicht: {e}", to.as_secs_f64()));
            }
        }
        let mut chunk = Vec::with_capacity(CHUNK);
        let mut ended = false;
        for _ in 0..CHUNK {
            match inner.next() {
                Some(sample) => chunk.push(sample),
                None => {
                    ended = true;
                    break;
                }
            }
        }
        let mut store = shared.store.lock().unwrap_or_else(|e| e.into_inner());
        if store.seek_to.is_some() {
            continue;
        }
        if !chunk.is_empty() {
            store.chunks.push_back(chunk);
        }
        note_held(shared, &store);
        store.done = ended;
        store.seeking = false;
        if store.starved.is_some() && (ended || store.chunks.len() >= shared.cushion) {
            let silent = store.starved.take().map(|since| since.elapsed()).unwrap_or_default();
            let first = std::mem::take(&mut store.first);
            if first {
                super::watch::note(format!("radio: {:.0} s polster nach {:.1} s da, es klingt", CUSHION.as_secs_f64(), silent.as_secs_f64()));
            } else if shared.live {
                super::watch::note(format!(
                    "ton: {:.1} s stille, es kam nichts nach · gewartet, bis wieder {:.0} s da waren",
                    silent.as_secs_f64(),
                    CUSHION.as_secs_f64()
                ));
            } else if silent >= Duration::from_millis(250) {
                super::watch::note(format!("ton: {:.1} s stille, es kam nichts nach", silent.as_secs_f64()));
            }
        }
        drop(inner);
        shared.changed.notify_all();
    }
}

/// How much a station's store holds, for the needle; nothing for a file.
fn note_held<S>(shared: &Shared<S>, store: &Store) {
    let current = STATION_NOW.load(std::sync::atomic::Ordering::Relaxed) == shared.station;
    if shared.live && current && !store.stop {
        let held = store.chunks.len() as u64 * shared.chunk_ms;
        STATION_HELD.store(held, std::sync::atomic::Ordering::Relaxed);
    }
}

impl<S: Source + Send + 'static> Drop for Ahead<S> {
    fn drop(&mut self) {
        if let Ok(mut store) = self.shared.store.lock() {
            store.stop = true;
            if self.shared.live && STATION_NOW.load(std::sync::atomic::Ordering::Relaxed) == self.shared.station {
                STATION_HELD.store(0, std::sync::atomic::Ordering::Relaxed);
            }
        }
        self.shared.changed.notify_all();
    }
}

impl<S: Source + Send + 'static> Iterator for Ahead<S> {
    type Item = rodio::Sample;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.at < self.current.len() {
            let sample = self.current[self.at];
            self.at += 1;
            return Some(sample);
        }
        let mut store = self.shared.store.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if store.stop {
                return None;
            }
            if store.starved.is_some() && !store.done {
                drop(store);
                self.current = vec![0.0; usize::from(self.channels.get())];
                self.at = 1;
                return Some(0.0);
            }
            if let Some(chunk) = store.chunks.pop_front() {
                note_held(&self.shared, &store);
                drop(store);
                self.shared.changed.notify_all();
                self.current = chunk;
                self.at = 1;
                return self.current.first().copied();
            }
            if store.done || store.stop {
                return None;
            }
            if store.seeking {
                drop(store);
                self.current = vec![0.0; usize::from(self.channels.get())];
                self.at = 1;
                return Some(0.0);
            }
            store = wait_a_little(&self.shared, store);
        }
    }
}

/// Wait for the store to change, [`STARVED_AFTER`] at most: still empty
/// then, it is starved, and silence is played until something comes.
fn wait_a_little<'a, S>(shared: &'a Shared<S>, store: std::sync::MutexGuard<'a, Store>) -> std::sync::MutexGuard<'a, Store> {
    let (mut store, waited) = shared.changed.wait_timeout(store, STARVED_AFTER).unwrap_or_else(|e| e.into_inner());
    if waited.timed_out() && store.chunks.is_empty() && !store.done && !store.stop && !store.seeking {
        store.starved = Some(std::time::Instant::now());
    }
    store
}

impl<S: Source + Send + 'static> Source for Ahead<S> {
    /// What is left of the piece in hand, or the next piece's length — waited
    /// for, as `next` would — and 0 at the end.
    ///
    /// the player's queue, the mixer turning it to the card's channels and rate
    /// — looks at its channels and rate only where a span begins, so the first
    /// thing played set them for everything after it until the device was let
    /// go. A mono episode after the stereo radio played twice as fast; a stereo
    /// one after a mono one half as fast.
    fn current_span_len(&self) -> Option<usize> {
        let left = self.current.len().saturating_sub(self.at);
        if left > 0 {
            return Some(left);
        }
        let mut store = self.shared.store.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if store.stop {
                return Some(0);
            }
            if store.starved.is_some() && !store.done {
                return Some(usize::from(self.channels.get()));
            }
            if let Some(chunk) = store.chunks.front() {
                return Some(chunk.len());
            }
            if store.done {
                return Some(0);
            }
            if store.seeking {
                return Some(usize::from(self.channels.get()));
            }
            store = wait_a_little(&self.shared, store);
        }
    }

    fn channels(&self) -> rodio::ChannelCount {
        self.channels
    }

    fn sample_rate(&self) -> rodio::SampleRate {
        self.rate
    }

    fn total_duration(&self) -> Option<Duration> {
        self.total
    }

    /// Away with everything read for the old place — in the store and in hand —
    /// and the reading thread sent to the new one; back at once. A place the
    /// decoder cannot go to is said in „messen" and it plays on where it was.
    fn try_seek(&mut self, pos: Duration) -> Result<(), rodio::source::SeekError> {
        let mut store = self.shared.store.lock().unwrap_or_else(|e| e.into_inner());
        store.chunks.clear();
        store.done = false;
        store.seek_to = Some(pos);
        store.seeking = true;
        drop(store);
        self.current.clear();
        self.at = 0;
        self.shared.changed.notify_all();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Counts up from zero, and seeks by setting the count: a source whose
    /// every sample says where it came from.
    struct Counting {
        at: u32,
        end: u32,
    }

    impl Iterator for Counting {
        type Item = f32;
        fn next(&mut self) -> Option<f32> {
            (self.at < self.end).then(|| {
                self.at += 1;
                (self.at - 1) as f32
            })
        }
    }

    impl Source for Counting {
        fn current_span_len(&self) -> Option<usize> {
            None
        }
        fn channels(&self) -> rodio::ChannelCount {
            rodio::nz!(1)
        }
        fn sample_rate(&self) -> rodio::SampleRate {
            rodio::nz!(1000)
        }
        fn total_duration(&self) -> Option<Duration> {
            None
        }
        fn try_seek(&mut self, pos: Duration) -> Result<(), rodio::source::SeekError> {
            self.at = pos.as_millis() as u32;
            Ok(())
        }
    }

    /// Every sample comes through, in order, and the end is the end —
    /// through many more chunks than the store holds at once.
    #[test]
    fn everything_comes_through_in_order() {
        let ahead = Ahead::new(Counting { at: 0, end: 50_000 });
        let got: Vec<f32> = ahead.collect();
        assert_eq!(got.len(), 50_000);
        assert!(got.iter().enumerate().all(|(i, s)| *s == i as f32));
    }

    /// After a seek the next sample is the new place's, not what was read
    /// ahead for the old one.
    #[test]
    fn a_seek_throws_away_what_was_read_ahead() {
        let mut ahead = Ahead::new(Counting { at: 0, end: 100_000 });
        assert_eq!(ahead.next(), Some(0.0));
        std::thread::sleep(Duration::from_millis(20));
        ahead.try_seek(Duration::from_millis(40_000)).unwrap();
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(ahead.next(), Some(40_000.0));
        assert_eq!(ahead.next(), Some(40_001.0));
    }

    /// A decoder that takes its time to go somewhere, as a long MP3 does.
    struct Slow(Counting);

    impl Iterator for Slow {
        type Item = f32;
        fn next(&mut self) -> Option<f32> {
            self.0.next()
        }
    }

    impl Source for Slow {
        fn current_span_len(&self) -> Option<usize> {
            None
        }
        fn channels(&self) -> rodio::ChannelCount {
            rodio::nz!(2)
        }
        fn sample_rate(&self) -> rodio::SampleRate {
            rodio::nz!(1000)
        }
        fn total_duration(&self) -> Option<Duration> {
            None
        }
        fn try_seek(&mut self, pos: Duration) -> Result<(), rodio::source::SeekError> {
            std::thread::sleep(Duration::from_millis(200));
            self.0.try_seek(pos)
        }
    }

    /// A seek is back at once, however long the decoder takes to get there:
    /// silence meanwhile, a whole frame at a time, then the new place.
    #[test]
    fn a_seek_does_not_wait_for_the_decoder() {
        let mut ahead = Ahead::new(Slow(Counting { at: 0, end: 100_000 }));
        assert_eq!(ahead.next(), Some(0.0));
        let asked = std::time::Instant::now();
        ahead.try_seek(Duration::from_millis(30_000)).unwrap();
        assert!(asked.elapsed() < Duration::from_millis(50), "back at once");
        assert_eq!((ahead.next(), ahead.next()), (Some(0.0), Some(0.0)), "a frame of silence");
        std::thread::sleep(Duration::from_millis(300));
        let after: Vec<f32> = ahead.by_ref().skip_while(|s| *s == 0.0).take(2).collect();
        assert_eq!(after, vec![30_000.0, 30_001.0]);
    }

    /// Silence of a given shape.
    struct Shaped {
        left: usize,
        channels: u16,
    }

    impl Iterator for Shaped {
        type Item = f32;
        fn next(&mut self) -> Option<f32> {
            (self.left > 0).then(|| {
                self.left -= 1;
                0.0
            })
        }
    }

    impl Source for Shaped {
        fn current_span_len(&self) -> Option<usize> {
            None
        }
        fn channels(&self) -> rodio::ChannelCount {
            rodio::ChannelCount::new(self.channels).unwrap()
        }
        fn sample_rate(&self) -> rodio::SampleRate {
            rodio::nz!(1000)
        }
        fn total_duration(&self) -> Option<Duration> {
            None
        }
    }

    /// A mono file after a stereo one in the same player — or the other way
    /// round — plays at its own pace. Each said it was one endless piece, so
    /// what took them in never looked at the next one's channels: mono read as
    /// stereo went twice as fast, stereo read as mono half as fast.
    #[test]
    fn the_next_source_is_read_in_its_own_shape() {
        for (first, second) in [(1u16, 2u16), (2, 1)] {
            let (input, output) = rodio::queue::queue(false);
            input.append(Ahead::new(Shaped { left: 1000 * usize::from(first), channels: first }));
            input.append(Ahead::new(Shaped { left: 1000 * usize::from(second), channels: second }));
            let heard = rodio::source::UniformSourceIterator::new(output, rodio::nz!(2), rodio::nz!(1000)).count();
            assert_eq!(heard, 2 * 2000, "{first} then {second} channels: a second each, in stereo");
        }
    }

    /// Let go of, the reading thread ends rather than waiting for ever.
    /// A source that never gives another sample and never ends — a
    /// station gone quiet without hanging up — is heard as silence, at
    /// once and for as long as it lasts; the card's thread is not held.
    #[test]
    fn a_source_that_stalls_is_silence_not_a_wait() {
        struct Stalled;
        impl Iterator for Stalled {
            type Item = f32;
            fn next(&mut self) -> Option<f32> {
                std::thread::sleep(Duration::from_secs(3600));
                None
            }
        }
        impl Source for Stalled {
            fn current_span_len(&self) -> Option<usize> {
                None
            }
            fn channels(&self) -> rodio::ChannelCount {
                rodio::nz!(2)
            }
            fn sample_rate(&self) -> rodio::SampleRate {
                rodio::nz!(44100)
            }
            fn total_duration(&self) -> Option<Duration> {
                None
            }
        }
        let mut ahead = Ahead::new(Stalled);
        let began = std::time::Instant::now();
        for _ in 0..88_200 {
            assert_eq!(ahead.next(), Some(0.0));
        }
        assert_eq!(ahead.current_span_len(), Some(2));
        assert!(began.elapsed() < Duration::from_secs(1), "{:?}", began.elapsed());
    }

    #[test]
    fn dropped_it_stops_reading() {
        let ahead = Ahead::new(Counting { at: 0, end: u32::MAX });
        let shared = Arc::clone(&ahead.shared);
        drop(ahead);
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(Arc::strong_count(&shared), 1);
    }

    /// A station that sends only what it is let: ones, as many as `given`
    /// says, then waits — the network, as slow as the test wants it.
    struct Gated {
        given: Arc<std::sync::atomic::AtomicUsize>,
        sent: usize,
    }

    impl Iterator for Gated {
        type Item = f32;
        fn next(&mut self) -> Option<f32> {
            use std::sync::atomic::Ordering;
            loop {
                if self.sent < self.given.load(Ordering::SeqCst) {
                    self.sent += 1;
                    return Some(1.0);
                }
                if Arc::strong_count(&self.given) == 1 {
                    return None;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }

    impl Source for Gated {
        fn current_span_len(&self) -> Option<usize> {
            None
        }
        fn channels(&self) -> rodio::ChannelCount {
            rodio::nz!(2)
        }
        fn sample_rate(&self) -> rodio::SampleRate {
            rodio::nz!(8000)
        }
        fn total_duration(&self) -> Option<Duration> {
            None
        }
    }

    /// Samples in a cushion of the gated station: whole chunks, as counted.
    fn cushion_of_gated() -> usize {
        (2 * 8000 * CUSHION.as_millis() as usize / 1000).div_ceil(CHUNK) * CHUNK
    }

    /// Asks for samples the way the card does, for a while, and says how
    /// many were sound rather than silence.
    fn heard(ahead: &mut Ahead<Gated>, asked: usize) -> usize {
        (0..asked).filter(|_| ahead.next() == Some(1.0)).count()
    }

    /// A station is silent until the cushion has come — not from the first
    /// scrap on.
    #[test]
    fn a_station_waits_for_its_cushion() {
        use std::sync::atomic::Ordering;
        let given = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut ahead = Ahead::live(Gated { given: Arc::clone(&given), sent: 0 });
        given.store(cushion_of_gated() - CHUNK, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(heard(&mut ahead, 1000), 0, "a scrap is not a cushion");
        given.store(cushion_of_gated(), Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(heard(&mut ahead, 1000), 1000);
    }

    /// Run dry, a station gathers the cushion again before it goes on: one
    /// pause, not a stutter of scraps.
    #[test]
    fn a_station_run_dry_gathers_again() {
        use std::sync::atomic::Ordering;
        let given = Arc::new(std::sync::atomic::AtomicUsize::new(cushion_of_gated()));
        let mut ahead = Ahead::live(Gated { given: Arc::clone(&given), sent: 0 });
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(heard(&mut ahead, cushion_of_gated()), cushion_of_gated());
        assert_eq!(heard(&mut ahead, 1000), 0);
        given.fetch_add(CHUNK, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(heard(&mut ahead, 1000), 0, "one chunk does not end the silence");
        given.fetch_add(cushion_of_gated(), Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(heard(&mut ahead, 1000), 1000);
    }
}
