use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use rodio::Source;

pub const AHEAD: Duration = Duration::from_secs(3);

const CHUNK: usize = 4096;

struct Store {
    chunks: VecDeque<Vec<f32>>,
    done: bool,
    stop: bool,
    seek_to: Option<Duration>,
    seeking: bool,
    starved: Option<std::time::Instant>,
}

const STARVED_AFTER: Duration = Duration::from_millis(30);

struct Shared<S> {
    inner: Mutex<S>,
    store: Mutex<Store>,
    changed: Condvar,
    room: usize,
}

pub struct Ahead<S: Source + Send + 'static> {
    shared: Arc<Shared<S>>,
    current: Vec<f32>,
    at: usize,
    channels: rodio::ChannelCount,
    rate: rodio::SampleRate,
    total: Option<Duration>,
}

impl<S: Source + Send + 'static> Ahead<S> {
    pub fn new(inner: S) -> Self {
        let channels = inner.channels();
        let rate = inner.sample_rate();
        let total = inner.total_duration();
        let per_second = rate.get() as usize * usize::from(channels.get());
        let room = (per_second * AHEAD.as_millis() as usize / 1000).div_ceil(CHUNK).max(2);
        let shared = Arc::new(Shared {
            inner: Mutex::new(inner),
            store: Mutex::new(Store { chunks: VecDeque::new(), done: false, stop: false, seek_to: None, seeking: false, starved: None }),
            changed: Condvar::new(),
            room,
        });
        let reading = Arc::clone(&shared);
        std::thread::Builder::new()
            .name("kessler-ahead".into())
            .spawn(move || read(&reading))
            .ok();
        Ahead { shared, current: Vec::new(), at: 0, channels, rate, total }
    }
}

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
        store.done = ended;
        store.seeking = false;
        if let Some(since) = store.starved.take() {
            let silent = since.elapsed();
            if silent >= Duration::from_millis(250) {
                super::watch::note(format!("ton: {:.1} s stille, es kam nichts nach", silent.as_secs_f64()));
            }
        }
        drop(inner);
        shared.changed.notify_all();
    }
}

impl<S: Source + Send + 'static> Drop for Ahead<S> {
    fn drop(&mut self) {
        if let Ok(mut store) = self.shared.store.lock() {
            store.stop = true;
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
            if let Some(chunk) = store.chunks.pop_front() {
                drop(store);
                self.shared.changed.notify_all();
                self.current = chunk;
                self.at = 1;
                return self.current.first().copied();
            }
            if store.done || store.stop {
                return None;
            }
            if store.seeking || store.starved.is_some() {
                drop(store);
                self.current = vec![0.0; usize::from(self.channels.get())];
                self.at = 1;
                return Some(0.0);
            }
            store = wait_a_little(&self.shared, store);
        }
    }
}

fn wait_a_little<'a, S>(shared: &'a Shared<S>, store: std::sync::MutexGuard<'a, Store>) -> std::sync::MutexGuard<'a, Store> {
    let (mut store, waited) = shared.changed.wait_timeout(store, STARVED_AFTER).unwrap_or_else(|e| e.into_inner());
    if waited.timed_out() && store.chunks.is_empty() && !store.done && !store.stop && !store.seeking {
        store.starved = Some(std::time::Instant::now());
    }
    store
}

impl<S: Source + Send + 'static> Source for Ahead<S> {
    fn current_span_len(&self) -> Option<usize> {
        let left = self.current.len().saturating_sub(self.at);
        if left > 0 {
            return Some(left);
        }
        let mut store = self.shared.store.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if let Some(chunk) = store.chunks.front() {
                return Some(chunk.len());
            }
            if store.done || store.stop {
                return Some(0);
            }
            if store.seeking || store.starved.is_some() {
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

    #[test]
    fn everything_comes_through_in_order() {
        let ahead = Ahead::new(Counting { at: 0, end: 50_000 });
        let got: Vec<f32> = ahead.collect();
        assert_eq!(got.len(), 50_000);
        assert!(got.iter().enumerate().all(|(i, s)| *s == i as f32));
    }

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
}
