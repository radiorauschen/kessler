use std::sync::Mutex;
use std::time::Duration;

use rodio::Source;

#[derive(Debug)]
pub struct Note {
    pub at: String,
    pub said: String,
    pub memory_mb: u64,
}

static NOTES: Mutex<Vec<Note>> = Mutex::new(Vec::new());

const NOTES_MOST: usize = 200;

pub fn note(said: String) {
    tracing::debug!("{said}");
    let note = Note { at: chrono::Local::now().format("%H:%M:%S").to_string(), said, memory_mb: memory_now_mb() };
    let mut notes = NOTES.lock().unwrap_or_else(|e| e.into_inner());
    notes.push(note);
    if notes.len() > NOTES_MOST {
        let over = notes.len() - NOTES_MOST;
        notes.drain(..over);
    }
}

pub fn take_notes() -> Vec<Note> {
    std::mem::take(&mut *NOTES.lock().unwrap_or_else(|e| e.into_inner()))
}

pub fn memory_now_mb() -> u64 {
    #[cfg(target_os = "macos")]
    {
        let mut info: libc::proc_taskinfo = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of::<libc::proc_taskinfo>() as libc::c_int;
        let got = unsafe {
            libc::proc_pidinfo(libc::getpid(), libc::PROC_PIDTASKINFO, 0, (&mut info as *mut libc::proc_taskinfo).cast(), size)
        };
        if got == size {
            return info.pti_resident_size / 1024 / 1024;
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(statm) = std::fs::read_to_string("/proc/self/statm") {
            if let Some(pages) = statm.split_whitespace().nth(1).and_then(|p| p.parse::<u64>().ok()) {
                let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) }.max(0) as u64;
                return pages * page / 1024 / 1024;
            }
        }
    }
    0
}

pub struct Watch<S: Source> {
    inner: S,
    name: String,
    spec: (u16, u32),
    samples: u64,
    seconds_before: f64,
}

impl<S: Source> Watch<S> {
    pub fn new(inner: S, name: &str) -> Self {
        let spec = (inner.channels().get(), inner.sample_rate().get());
        let length = inner.total_duration().map_or_else(|| "?".to_string(), |d| format!("{:.0} s", d.as_secs_f64()));
        note(format!("ton: {name} · {} kanäle · {} hz · länge {length}", spec.0, spec.1));
        Watch { inner, name: name.to_string(), spec, samples: 0, seconds_before: 0.0 }
    }

    fn seconds(&self) -> f64 {
        self.seconds_before + self.samples as f64 / (f64::from(self.spec.0.max(1)) * f64::from(self.spec.1.max(1)))
    }
}

impl<S: Source> Iterator for Watch<S> {
    type Item = rodio::Sample;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let sample = self.inner.next();
        self.samples += 1;
        if self.samples.is_multiple_of(1024) {
            let spec = (self.inner.channels().get(), self.inner.sample_rate().get());
            if spec != self.spec {
                let at = self.seconds();
                note(format!(
                    "ton: {} · wechsel nach {:.1} s: {} kanäle · {} hz (vorher {} · {})",
                    self.name, at, spec.0, spec.1, self.spec.0, self.spec.1
                ));
                self.seconds_before = at;
                self.samples = 0;
                self.spec = spec;
            }
        }
        if sample.is_none() {
            note(format!("ton: {} · zu ende nach {:.1} s", self.name, self.seconds()));
        }
        sample
    }
}

impl<S: Source> Source for Watch<S> {
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

    fn try_seek(&mut self, pos: Duration) -> Result<(), rodio::source::SeekError> {
        note(format!("ton: {} · springe nach {:.1} s", self.name, pos.as_secs_f64()));
        self.seconds_before = pos.as_secs_f64();
        self.samples = 0;
        self.inner.try_seek(pos)
    }
}

pub fn steady<S: Source + Send + 'static>(source: S, name: &str) -> impl Source + Send + 'static {
    let watched = Watch::new(source, name);
    let (channels, rate) = (watched.channels(), watched.sample_rate());
    rodio::source::UniformSourceIterator::new(watched, channels, rate)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Changing {
        at: usize,
    }

    impl Iterator for Changing {
        type Item = f32;
        fn next(&mut self) -> Option<f32> {
            self.at += 1;
            (self.at <= 44_100 + 44_100).then_some(0.0)
        }
    }

    impl Source for Changing {
        fn current_span_len(&self) -> Option<usize> {
            Some(if self.at < 44_100 { 44_100 - self.at } else { 44_100 * 2 - self.at })
        }
        fn channels(&self) -> rodio::ChannelCount {
            rodio::ChannelCount::new(if self.at < 44_100 { 1 } else { 2 }).unwrap()
        }
        fn sample_rate(&self) -> rodio::SampleRate {
            rodio::SampleRate::new(if self.at < 44_100 { 44_100 } else { 22_050 }).unwrap()
        }
        fn total_duration(&self) -> Option<Duration> {
            None
        }
    }

    #[test]
    fn a_change_on_the_way_is_said_and_smoothed_out() {
        take_notes();
        let steadied = steady(Changing { at: 0 }, "folge.mp3");
        assert_eq!((steadied.channels().get(), steadied.sample_rate().get()), (1, 44_100));
        let samples = steadied.count();
        let notes = take_notes();
        assert!(notes.iter().any(|n| n.said.contains("folge.mp3 · 1 kanäle · 44100 hz")), "{notes:?}");
        assert!(notes.iter().any(|n| n.said.contains("wechsel nach 1.0 s: 2 kanäle · 22050 hz")), "{notes:?}");
        assert!((86_000..=90_000).contains(&samples), "{samples}");
    }
}
