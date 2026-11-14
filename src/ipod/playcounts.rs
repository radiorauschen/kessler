use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::itunesdb::types::{mac_to_unix, IpodTrack, ItunesDb};
use super::IpodDevice;
use crate::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Played {
    pub plays: u32,
    pub last_played: Option<i64>,
    pub rating: Option<u8>,
}

pub fn path_in(mount: &Path) -> PathBuf {
    mount.join("iPod_Control").join("iTunes").join("Play Counts")
}

pub fn read(device: &IpodDevice) -> Result<Vec<Played>> {
    let path = path_in(&device.mount_point);
    match std::fs::read(&path) {
        Ok(bytes) => Ok(parse(&bytes).unwrap_or_default()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e.into()),
    }
}

pub fn parse(bytes: &[u8]) -> Option<Vec<Played>> {
    let word = |at: usize| -> Option<u32> {
        bytes.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    if bytes.get(0..4)? != b"mhdp" {
        return None;
    }
    let header = word(4)? as usize;
    let entry = word(8)? as usize;
    let count = word(12)? as usize;
    if entry < 12 || header < 16 {
        return None;
    }
    let mut played = Vec::with_capacity(count.min(100_000));
    for i in 0..count {
        let at = header + i * entry;
        let plays = word(at)?;
        let last_played = mac_to_unix(word(at + 4)?);
        let rating = if entry >= 16 { word(at + 12).map(|r| r.min(100) as u8) } else { None };
        played.push(Played { plays, last_played, rating });
    }
    Some(played)
}

pub fn fold(db: &mut ItunesDb, played: &[Played]) {
    for (track, heard) in db.tracks.iter_mut().zip(played) {
        track.play_count = track.play_count.saturating_add(heard.plays);
        if heard.last_played > track.last_played {
            track.last_played = heard.last_played;
        }
        if let Some(rating) = heard.rating {
            track.rating = rating;
        }
    }
}

pub fn forget(device: &IpodDevice) -> Result<()> {
    match std::fs::remove_file(path_in(&device.mount_point)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Taken {
    pub database: String,
    pub plays: HashMap<String, u32>,
    pub ratings: HashMap<String, u8>,
}

#[derive(Debug, Clone)]
pub struct Heard {
    pub track: IpodTrack,
    pub plays: u32,
    pub last_played: Option<i64>,
    pub rating: Option<u8>,
}

pub fn database_stamp(device: &IpodDevice) -> String {
    let path = device.itunesdb_path();
    let meta = std::fs::metadata(&path).ok();
    let len = meta.as_ref().map_or(0, |m| m.len());
    let when = meta
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    format!("{len}-{when}")
}

pub fn new_plays(db: &ItunesDb, played: &[Played], taken: &Taken, stamp: &str) -> (Vec<Heard>, Taken) {
    let fresh = Taken::default();
    let before = if taken.database == stamp { taken } else { &fresh };
    let mut now = Taken { database: stamp.to_string(), ..Taken::default() };
    let mut heard = Vec::new();
    for (track, entry) in db.tracks.iter().zip(played) {
        let key = format!("{:016x}", track.dbid);
        let had = before.plays.get(&key).copied().unwrap_or(0);
        let plays = if entry.plays >= had { entry.plays - had } else { entry.plays };
        let rating = entry
            .rating
            .filter(|r| *r != track.rating)
            .filter(|r| before.ratings.get(&key) != Some(r));
        if entry.plays > 0 {
            now.plays.insert(key.clone(), entry.plays);
        }
        if let Some(r) = entry.rating.filter(|r| *r != track.rating) {
            now.ratings.insert(key, r);
        }
        if plays > 0 || rating.is_some() {
            heard.push(Heard { track: track.clone(), plays, last_played: entry.last_played, rating });
        }
    }
    (heard, now)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(entries: &[(u32, u32, u32)]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"mhdp");
        bytes.extend_from_slice(&0x60u32.to_le_bytes());
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        bytes.resize(0x60, 0);
        for &(plays, when, rating) in entries {
            bytes.extend_from_slice(&plays.to_le_bytes());
            bytes.extend_from_slice(&when.to_le_bytes());
            bytes.extend_from_slice(&0u32.to_le_bytes());
            bytes.extend_from_slice(&rating.to_le_bytes());
        }
        bytes
    }

    fn db(n: usize) -> ItunesDb {
        ItunesDb {
            tracks: (0..n)
                .map(|i| IpodTrack { id: i as u32, dbid: 100 + i as u64, ..IpodTrack::default() })
                .collect(),
            ..ItunesDb::default()
        }
    }

    #[test]
    fn the_file_is_read_entry_by_entry() {
        let when = 3_000_000_000;
        let played = parse(&file(&[(2, when, 60), (0, 0, 0)])).unwrap();
        assert_eq!(played.len(), 2);
        assert_eq!(played[0], Played { plays: 2, last_played: mac_to_unix(when), rating: Some(60) });
        assert_eq!(played[1].plays, 0);
        assert!(parse(b"mhbd....").is_none(), "not one");
        assert!(parse(&file(&[(1, 1, 1)])[..0x64]).is_none(), "cut short");
    }

    #[test]
    fn folding_adds_the_plays_to_the_database() {
        let mut db = db(2);
        db.tracks[0].play_count = 3;
        let played = [Played { plays: 2, last_played: Some(50), rating: Some(80) }, Played::default()];
        fold(&mut db, &played);
        assert_eq!(db.tracks[0].play_count, 5);
        assert_eq!(db.tracks[0].last_played, Some(50));
        assert_eq!(db.tracks[0].rating, 80);
        assert_eq!(db.tracks[1].play_count, 0);
    }

    #[test]
    fn only_new_plays_are_taken() {
        let db = db(2);
        let first = [Played { plays: 2, last_played: Some(10), rating: None }, Played::default()];
        let (heard, taken) = new_plays(&db, &first, &Taken::default(), "a");
        assert_eq!(heard.len(), 1);
        assert_eq!(heard[0].plays, 2);

        let second = [Played { plays: 3, last_played: Some(20), rating: None }, Played::default()];
        let (heard, taken) = new_plays(&db, &second, &taken, "a");
        assert_eq!(heard[0].plays, 1, "one more since");

        let (heard, _) = new_plays(&db, &second, &taken, "b");
        assert_eq!(heard[0].plays, 3, "a new database counts from nothing");
    }

    #[test]
    fn a_rating_set_on_the_ipod_is_taken_once() {
        let mut db = db(1);
        db.tracks[0].rating = 40;
        let rated = [Played { plays: 0, last_played: None, rating: Some(100) }];
        let (heard, taken) = new_plays(&db, &rated, &Taken::default(), "a");
        assert_eq!(heard[0].rating, Some(100));
        let (heard, _) = new_plays(&db, &rated, &taken, "a");
        assert!(heard.is_empty(), "already taken");
        let same = [Played { plays: 0, last_played: None, rating: Some(40) }];
        assert!(new_plays(&db, &same, &Taken::default(), "a").0.is_empty(), "not changed");
    }
}
