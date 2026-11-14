use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const KEPT: usize = 2000;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Heard {
    #[serde(default)]
    heard: Vec<Hearing>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hearing {
    pub artist: String,
    pub station: String,
    pub day: String,
}

fn path() -> PathBuf {
    crate::config::Config::config_dir().join("radio-heard.toml")
}

impl Heard {
    pub fn read() -> Heard {
        std::fs::read_to_string(path()).ok().and_then(|text| toml::from_str(&text).ok()).unwrap_or_default()
    }

    pub fn note(&mut self, artist: &str, station: &str, day: &str) -> bool {
        let key = crate::db::sort_key(artist);
        if let Some(at) = self.heard.iter().position(|h| crate::db::sort_key(&h.artist) == key) {
            if at == 0 && self.heard[0].station == station && self.heard[0].day == day {
                return false;
            }
            self.heard.remove(at);
        }
        self.heard.insert(0, Hearing { artist: artist.to_string(), station: station.to_string(), day: day.to_string() });
        self.heard.truncate(KEPT);
        true
    }

    pub fn last(&self, artist: &str) -> Option<&Hearing> {
        let key = crate::db::sort_key(artist);
        self.heard.iter().find(|h| crate::db::sort_key(&h.artist) == key)
    }

    pub fn on_station(&self, station: &str, most: usize) -> Vec<String> {
        self.heard.iter().filter(|h| h.station == station).take(most).map(|h| h.artist.clone()).collect()
    }

    pub fn save(&self) {
        if let Ok(text) = toml::to_string(self) {
            let _ = std::fs::create_dir_all(crate::config::Config::config_dir());
            let _ = std::fs::write(path(), text);
        }
    }

    pub fn exists() -> bool {
        path().exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_artist_is_kept_once_with_the_last_station() {
        let mut heard = Heard::default();
        assert!(heard.note("Midori Takada", "nts 1", "2026-10-20"));
        assert!(heard.note("Björk", "fip", "2026-10-21"));
        assert!(heard.note("midori takada", "dublab", "2026-10-21"));
        assert!(!heard.note("Midori Takada", "dublab", "2026-10-21"));
        assert_eq!(heard.heard.len(), 2);
        let last = heard.last("MIDORI TAKADA").unwrap();
        assert_eq!((last.station.as_str(), last.day.as_str()), ("dublab", "2026-10-21"));
        assert!(heard.last("Hiroshi Yoshimura").is_none());
    }
}
