use std::time::Instant;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Heard {
    pub music: f64,
    pub radio: f64,
    pub podcasts: f64,
}

impl Heard {
    pub fn total(&self) -> f64 {
        self.music + self.radio + self.podcasts
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Music,
    Radio,
    Podcasts,
}

const MOST_A_FRAME: f64 = 2.0;

const SAVE_EVERY: f64 = 60.0;

pub struct Listened {
    pub heard: Heard,
    last: Option<Instant>,
    unsaved: f64,
}

fn path() -> std::path::PathBuf {
    crate::config::Config::config_dir().join("gehoert.toml")
}

impl Listened {
    pub fn open(seed: impl FnOnce() -> f64) -> Self {
        let heard = std::fs::read_to_string(path())
            .ok()
            .and_then(|text| toml::from_str::<Heard>(&text).ok())
            .unwrap_or_else(|| {
                let heard = Heard { music: seed().max(0.0), ..Heard::default() };
                write(&heard);
                heard
            });
        Self { heard, last: None, unsaved: 0.0 }
    }

    pub fn tick(&mut self, playing: Option<Source>) {
        let now = Instant::now();
        let since = self.last.map(|at| now.duration_since(at).as_secs_f64().min(MOST_A_FRAME));
        self.last = playing.map(|_| now);
        let (Some(source), Some(secs)) = (playing, since) else { return };
        match source {
            Source::Music => self.heard.music += secs,
            Source::Radio => self.heard.radio += secs,
            Source::Podcasts => self.heard.podcasts += secs,
        }
        self.unsaved += secs;
        if self.unsaved >= SAVE_EVERY {
            self.save();
        }
    }

    pub fn reset(&mut self) {
        self.heard = Heard::default();
        self.unsaved = 0.0;
        write(&self.heard);
    }

    pub fn save(&mut self) {
        if self.unsaved > 0.0 {
            write(&self.heard);
            self.unsaved = 0.0;
        }
    }
}

fn write(heard: &Heard) {
    if let Ok(text) = toml::to_string(heard) {
        let _ = std::fs::create_dir_all(crate::config::Config::config_dir());
        let _ = std::fs::write(path(), text);
    }
}

pub fn said(secs: f64) -> String {
    match amount(secs) {
        (hours, true) => tr!("{} std", hours),
        (minutes, false) => tr!("{} min", minutes),
    }
}

pub fn parts(heard: &Heard) -> String {
    let hours = amount(heard.total()).1;
    let n = |secs: f64| {
        let minutes = (secs.max(0.0) / 60.0).floor() as u64;
        if hours { minutes / 60 } else { minutes }
    };
    tr!("(musik {}, radio {}, podcast {})", n(heard.music), n(heard.radio), n(heard.podcasts))
}

fn amount(secs: f64) -> (u64, bool) {
    let minutes = (secs.max(0.0) / 60.0).floor() as u64;
    if minutes >= 60 {
        (minutes / 60, true)
    } else {
        (minutes, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hours_from_one_on_else_minutes() {
        assert_eq!(amount(59.0), (0, false));
        assert_eq!(amount(45.0 * 60.0), (45, false));
        assert_eq!(amount(3600.0 * 312.5), (312, true));
    }

    #[test]
    fn only_what_sounds_counts_and_a_long_sleep_counts_little() {
        let mut l = Listened { heard: Heard::default(), last: None, unsaved: -1e9 };
        l.tick(Some(Source::Radio));
        l.last = Some(Instant::now() - std::time::Duration::from_secs(600));
        l.tick(Some(Source::Radio));
        assert!(l.heard.radio <= MOST_A_FRAME + 0.01 && l.heard.radio > 1.9, "{:?}", l.heard);
        l.tick(None);
        l.tick(Some(Source::Music));
        assert_eq!(l.heard.music, 0.0, "nothing sounded in between");
    }
}
