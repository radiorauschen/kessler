use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Level {
    #[default]
    Off,
    Track,
    Album,
}

impl Level {
    pub const ALL: [Level; 3] = [Level::Off, Level::Track, Level::Album];

    pub fn name(self) -> &'static str {
        match self {
            Level::Off => "aus",
            Level::Track => "pro titel",
            Level::Album => "pro album",
        }
    }

    pub fn from_name(name: &str) -> Level {
        Level::ALL.into_iter().find(|l| l.name() == name.trim()).unwrap_or_default()
    }

    fn code(self) -> u8 {
        self as u8
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Factors {
    pub track: f32,
    pub album: f32,
}

#[derive(Default)]
pub struct Leveling {
    level: Arc<AtomicU8>,
    factors: RwLock<HashMap<PathBuf, Factors>>,
}

impl Leveling {
    pub fn set_level(&self, level: Level) {
        self.level.store(level.code(), Ordering::Relaxed);
    }

    pub fn switch(&self) -> Arc<AtomicU8> {
        Arc::clone(&self.level)
    }

    pub fn set_all(&self, all: HashMap<PathBuf, Factors>) {
        *self.factors.write().unwrap_or_else(|e| e.into_inner()) = all;
    }

    #[cfg(test)]
    pub fn set(&self, path: PathBuf, factors: Factors) {
        self.factors.write().unwrap_or_else(|e| e.into_inner()).insert(path, factors);
    }

    pub fn factors(&self, path: &Path) -> Factors {
        self.factors
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(path)
            .copied()
            .unwrap_or(Factors { track: 1.0, album: 1.0 })
    }
}

#[inline]
pub fn pick(switch: u8, factors: Factors) -> f32 {
    match switch {
        1 => factors.track,
        2 => factors.album,
        _ => 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_switch_chooses_and_the_unknown_plays_as_it_is() {
        let leveling = Leveling::default();
        leveling.set(PathBuf::from("/a.mp3"), Factors { track: 0.5, album: 0.7 });
        let known = leveling.factors(Path::new("/a.mp3"));
        let switch = leveling.switch();
        assert_eq!(pick(switch.load(Ordering::Relaxed), known), 1.0);
        leveling.set_level(Level::Track);
        assert_eq!(pick(switch.load(Ordering::Relaxed), known), 0.5);
        leveling.set_level(Level::Album);
        assert_eq!(pick(switch.load(Ordering::Relaxed), known), 0.7);
        assert_eq!(leveling.factors(Path::new("/b.mp3")), Factors { track: 1.0, album: 1.0 });
        assert_eq!(Level::from_name("pro album"), Level::Album);
        assert_eq!(Level::from_name("quatsch"), Level::Off);
    }
}
