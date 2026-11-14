use std::path::Path;

use super::device_info::Generation;
use super::itunesdb::types::{IpodTrack, ItunesDb};
use crate::error::{KesslerError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Classic,
    Chunked,
}

impl Layout {
    pub fn of(generation: Generation) -> Option<Self> {
        match generation {
            Generation::Shuffle1 | Generation::Shuffle2 => Some(Self::Classic),
            Generation::Shuffle3 | Generation::Shuffle4 => Some(Self::Chunked),
            _ => None,
        }
    }
}

const STATE_FILES: [&str; 3] = ["iTunesPState", "iTunesShuffle", "iTunesStats"];

pub fn install(itunes_dir: &Path, layout: Layout, db: &ItunesDb) -> Result<()> {
    let bytes = match layout {
        Layout::Classic => classic(db),
        Layout::Chunked => chunked(db),
    };
    let final_path = itunes_dir.join("iTunesSD");
    let temp_path = itunes_dir.join("iTunesSD.kessler-tmp");
    std::fs::write(&temp_path, &bytes)?;
    std::fs::rename(&temp_path, &final_path).map_err(|e| {
        let _ = std::fs::remove_file(&temp_path);
        KesslerError::Ipod(format!("could not install the shuffle's iTunesSD: {e}"))
    })?;
    for name in STATE_FILES {
        let _ = std::fs::remove_file(itunes_dir.join(name));
    }
    Ok(())
}

fn kind(path: &str) -> Option<u32> {
    let extension = path.rsplit('.').next()?.to_ascii_lowercase();
    match extension.as_str() {
        "mp3" => Some(1),
        "m4a" | "m4b" | "m4p" | "aac" => Some(2),
        "wav" | "aif" | "aiff" => Some(4),
        _ => None,
    }
}

fn device_path(track: &IpodTrack) -> Option<String> {
    let path = track.path.as_ref()?.to_string_lossy().replace(':', "/");
    Some(format!("/{}", path.trim_start_matches('/')))
}

fn playable(db: &ItunesDb) -> Vec<(&IpodTrack, String, u32)> {
    let mut tracks: Vec<(&IpodTrack, String, u32)> = db
        .tracks
        .iter()
        .filter_map(|track| {
            let path = device_path(track)?;
            let kind = kind(&path)?;
            Some((track, path, kind))
        })
        .collect();
    tracks.sort_by(|(a, ..), (b, ..)| {
        let artist = |t: &IpodTrack| {
            if t.album_artist.is_empty() { t.artist.to_lowercase() } else { t.album_artist.to_lowercase() }
        };
        (artist(a), a.album.to_lowercase(), a.disc_number, a.track_number, a.title.to_lowercase())
            .cmp(&(artist(b), b.album.to_lowercase(), b.disc_number, b.track_number, b.title.to_lowercase()))
    });
    tracks
}

const CLASSIC_ENTRY: usize = 0x22e;
const CLASSIC_HEADER: usize = 0x12;
const CLASSIC_PATH_UNITS: usize = 261;

fn put24(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_be_bytes()[1..]);
}

pub fn classic(db: &ItunesDb) -> Vec<u8> {
    let tracks = playable(db);
    let mut out = Vec::with_capacity(CLASSIC_HEADER + tracks.len() * CLASSIC_ENTRY);
    put24(&mut out, tracks.len() as u32);
    put24(&mut out, 0x010600);
    put24(&mut out, CLASSIC_HEADER as u32);
    out.resize(CLASSIC_HEADER, 0);
    for (_, path, kind) in &tracks {
        let start = out.len();
        put24(&mut out, CLASSIC_ENTRY as u32);
        put24(&mut out, 0x5aa501);
        put24(&mut out, 0);
        put24(&mut out, 0);
        put24(&mut out, 0);
        put24(&mut out, 0);
        put24(&mut out, 0);
        put24(&mut out, 0);
        put24(&mut out, 100);
        put24(&mut out, *kind);
        put24(&mut out, 0x200);
        let units: Vec<u16> = path.encode_utf16().take(CLASSIC_PATH_UNITS).collect();
        for unit in &units {
            out.extend_from_slice(&unit.to_le_bytes());
        }
        out.resize(start + 33 + CLASSIC_PATH_UNITS * 2, 0);
        out.push(1);
        out.push(0);
        out.push(0);
        debug_assert_eq!(out.len() - start, CLASSIC_ENTRY);
    }
    out
}

const SHDB_LEN: u32 = 64;
const SHTH_LEN: u32 = 20;
const SHTR_LEN: u32 = 0x174;
const SHPH_LEN: u32 = 20;
const SHPL_LEN: u32 = 44;
const CHUNKED_PATH: usize = 256;

fn put32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

pub fn chunked(db: &ItunesDb) -> Vec<u8> {
    let tracks = playable(db);
    let index: std::collections::HashMap<u32, u32> =
        tracks.iter().enumerate().map(|(at, (t, ..))| (t.id, at as u32)).collect();

    let mut lists: Vec<(u32, u64, Vec<u32>)> = vec![(1, 0, (0..tracks.len() as u32).collect())];
    for list in db.user_playlists() {
        let entries: Vec<u32> = list.track_ids.iter().filter_map(|id| index.get(id).copied()).collect();
        lists.push((if list.is_podcast { 3 } else { 2 }, list.id, entries));
    }

    let mut albums: Vec<String> = Vec::new();
    let mut artists: Vec<String> = Vec::new();
    fn number(seen: &mut Vec<String>, name: String) -> u32 {
        match seen.iter().position(|s| *s == name) {
            Some(at) => at as u32,
            None => {
                seen.push(name);
                (seen.len() - 1) as u32
            }
        }
    }

    let track_header_at = SHDB_LEN;
    let first_track_at = track_header_at + SHTH_LEN + 4 * tracks.len() as u32;
    let playlist_header_at = first_track_at + SHTR_LEN * tracks.len() as u32;
    let first_list_at = playlist_header_at + SHPH_LEN + 4 * lists.len() as u32;
    let podcasts = lists.iter().filter(|(kind, ..)| *kind == 3).count();

    let mut out = Vec::new();
    out.extend_from_slice(b"bdhs");
    put32(&mut out, 0x0200_0003);
    put32(&mut out, SHDB_LEN);
    put32(&mut out, tracks.len() as u32);
    put32(&mut out, lists.len() as u32);
    out.extend_from_slice(&[0; 8]);
    out.push(0);
    out.push(0);
    put16(&mut out, 0);
    put32(&mut out, tracks.len() as u32);
    put32(&mut out, track_header_at);
    put32(&mut out, playlist_header_at);
    out.resize(SHDB_LEN as usize, 0);

    out.extend_from_slice(b"hths");
    put32(&mut out, SHTH_LEN + 4 * tracks.len() as u32);
    put32(&mut out, tracks.len() as u32);
    out.extend_from_slice(&[0; 8]);
    for at in 0..tracks.len() as u32 {
        put32(&mut out, first_track_at + at * SHTR_LEN);
    }

    for (track, path, kind) in &tracks {
        let start = out.len();
        out.extend_from_slice(b"rths");
        put32(&mut out, SHTR_LEN);
        put32(&mut out, 0);
        put32(&mut out, 0);
        put32(&mut out, 0);
        put32(&mut out, *kind);
        let bytes = path.as_bytes();
        out.extend_from_slice(&bytes[..bytes.len().min(CHUNKED_PATH - 1)]);
        out.resize(start + 24 + CHUNKED_PATH, 0);
        put32(&mut out, 0);
        out.push(1);
        out.push(0);
        out.push(0);
        out.push(0);
        put32(&mut out, 0x200);
        put32(&mut out, 0x200);
        put32(&mut out, 0);
        put32(&mut out, 0);
        put32(&mut out, 0);
        put32(&mut out, 0);
        let album = number(&mut albums, track.album.to_lowercase());
        put32(&mut out, album);
        put16(&mut out, track.track_number.min(u16::MAX as u32) as u16);
        put16(&mut out, track.disc_number.min(u16::MAX as u32) as u16);
        out.extend_from_slice(&[0; 8]);
        out.extend_from_slice(&track.dbid.to_le_bytes());
        let artist = number(&mut artists, track.artist.to_lowercase());
        put32(&mut out, artist);
        out.resize(start + SHTR_LEN as usize, 0);
    }

    debug_assert_eq!(out.len() as u32, playlist_header_at);
    let plain = (lists.len() - podcasts) as u16;
    out.extend_from_slice(b"hphs");
    put32(&mut out, SHPH_LEN + 4 * lists.len() as u32);
    put32(&mut out, lists.len() as u32);
    put16(&mut out, plain);
    put16(&mut out, 1);
    put16(&mut out, plain);
    put16(&mut out, 0);
    let mut at = first_list_at;
    for (_, _, entries) in &lists {
        put32(&mut out, at);
        at += SHPL_LEN + 4 * entries.len() as u32;
    }

    for (kind, id, entries) in &lists {
        out.extend_from_slice(b"lphs");
        put32(&mut out, SHPL_LEN + 4 * entries.len() as u32);
        put32(&mut out, entries.len() as u32);
        put32(&mut out, entries.len() as u32);
        out.extend_from_slice(&id.to_le_bytes());
        put32(&mut out, *kind);
        out.extend_from_slice(&[0; 16]);
        for entry in entries {
            put32(&mut out, *entry);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipod::itunesdb::types::IpodPlaylist;
    use std::path::PathBuf;

    fn track(id: u32, artist: &str, album: &str, number: u32, path: &str) -> IpodTrack {
        IpodTrack {
            id,
            dbid: 0x1000 + id as u64,
            title: format!("titel {id}"),
            artist: artist.into(),
            album: album.into(),
            track_number: number,
            path: Some(PathBuf::from(path)),
            ..Default::default()
        }
    }

    fn library() -> ItunesDb {
        ItunesDb {
            tracks: vec![
                track(7, "qluster", "echtzeit", 2, "iPod_Control/Music/F01/BBBB.m4a"),
                track(3, "phew", "phew", 1, "iPod_Control/Music/F00/AAAA.mp3"),
                track(5, "qluster", "echtzeit", 1, ":iPod_Control:Music:F02:CCCC.wav"),
                track(9, "x", "y", 1, "iPod_Control/Music/F00/DDDD.flac"),
            ],
            playlists: vec![
                IpodPlaylist { id: 1, is_master: true, track_ids: vec![7, 3, 5, 9], ..Default::default() },
                IpodPlaylist { id: 0xabcd, name: "nacht".into(), track_ids: vec![5, 9, 3], ..Default::default() },
            ],
            ..Default::default()
        }
    }

    fn be24(bytes: &[u8], at: usize) -> u32 {
        u32::from_be_bytes([0, bytes[at], bytes[at + 1], bytes[at + 2]])
    }

    fn le32(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    fn utf16_path(entry: &[u8]) -> String {
        let units: Vec<u16> = entry[33..33 + 522]
            .chunks(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .take_while(|&unit| unit != 0)
            .collect();
        String::from_utf16(&units).unwrap()
    }

    #[test]
    fn the_first_two_generations_get_one_entry_per_playable_track() {
        let bytes = classic(&library());
        assert_eq!(bytes.len(), CLASSIC_HEADER + 3 * CLASSIC_ENTRY);
        assert_eq!(be24(&bytes, 0), 3);
        assert_eq!(be24(&bytes, 3), 0x010600);
        assert_eq!(be24(&bytes, 6), 0x12);
        let entries: Vec<&[u8]> = bytes[CLASSIC_HEADER..].chunks(CLASSIC_ENTRY).collect();
        let paths: Vec<String> = entries.iter().map(|e| utf16_path(e)).collect();
        assert_eq!(
            paths,
            ["/iPod_Control/Music/F00/AAAA.mp3", "/iPod_Control/Music/F02/CCCC.wav", "/iPod_Control/Music/F01/BBBB.m4a"]
        );
        let kinds: Vec<u32> = entries.iter().map(|e| be24(e, 27)).collect();
        assert_eq!(kinds, [1, 4, 2]);
        for entry in entries {
            assert_eq!(be24(entry, 0), 0x22e);
            assert_eq!(be24(entry, 3), 0x5aa501);
            assert_eq!(be24(entry, 24), 100);
            assert_eq!(&entry[555..], &[1, 0, 0]);
        }
    }

    #[test]
    fn the_last_two_generations_get_tracks_and_lists() {
        let bytes = chunked(&library());
        assert_eq!(&bytes[0..4], b"bdhs");
        assert_eq!(le32(&bytes, 0x0c), 3, "tracks");
        assert_eq!(le32(&bytes, 0x10), 2, "lists");
        assert_eq!(bytes[0x1d], 0, "no VoiceOver");
        let tracks_at = le32(&bytes, 0x24) as usize;
        let lists_at = le32(&bytes, 0x28) as usize;

        assert_eq!(&bytes[tracks_at..tracks_at + 4], b"hths");
        assert_eq!(le32(&bytes, tracks_at + 8), 3);
        let mut paths = Vec::new();
        for n in 0..3 {
            let at = le32(&bytes, tracks_at + 20 + 4 * n) as usize;
            assert_eq!(&bytes[at..at + 4], b"rths");
            assert_eq!(le32(&bytes, at + 4), 0x174);
            let field = &bytes[at + 24..at + 24 + 256];
            let end = field.iter().position(|&b| b == 0).unwrap();
            paths.push((String::from_utf8(field[..end].to_vec()).unwrap(), le32(&bytes, at + 20)));
        }
        assert_eq!(
            paths,
            [
                ("/iPod_Control/Music/F00/AAAA.mp3".to_string(), 1),
                ("/iPod_Control/Music/F02/CCCC.wav".to_string(), 4),
                ("/iPod_Control/Music/F01/BBBB.m4a".to_string(), 2),
            ]
        );

        assert_eq!(&bytes[lists_at..lists_at + 4], b"hphs");
        assert_eq!(le32(&bytes, lists_at + 8), 2);
        let master = le32(&bytes, lists_at + 20) as usize;
        let night = le32(&bytes, lists_at + 24) as usize;
        assert_eq!(&bytes[master..master + 4], b"lphs");
        assert_eq!(le32(&bytes, master + 8), 3);
        assert_eq!(le32(&bytes, master + 24), 1, "the master list");
        let entries = |at: usize| (0..le32(&bytes, at + 8) as usize).map(|n| le32(&bytes, at + 44 + 4 * n)).collect::<Vec<_>>();
        assert_eq!(entries(master), [0, 1, 2]);
        assert_eq!(le32(&bytes, night + 24), 2, "an ordinary list");
        assert_eq!(entries(night), [1, 0]);
        assert_eq!(bytes.len(), night + 44 + 2 * 4);
    }

    #[test]
    fn only_the_shuffles_get_one() {
        assert_eq!(Layout::of(Generation::Shuffle1), Some(Layout::Classic));
        assert_eq!(Layout::of(Generation::Shuffle2), Some(Layout::Classic));
        assert_eq!(Layout::of(Generation::Shuffle3), Some(Layout::Chunked));
        assert_eq!(Layout::of(Generation::Shuffle4), Some(Layout::Chunked));
        assert_eq!(Layout::of(Generation::Third), None);
    }

    #[test]
    fn installing_replaces_the_file_and_clears_the_state() {
        let dir = std::env::temp_dir().join(format!("kessler-itunessd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for name in STATE_FILES {
            std::fs::write(dir.join(name), b"old").unwrap();
        }
        install(&dir, Layout::Classic, &library()).unwrap();
        assert_eq!(std::fs::read(dir.join("iTunesSD")).unwrap(), classic(&library()));
        for name in STATE_FILES {
            assert!(!dir.join(name).exists(), "{name} is cleared");
        }
        assert!(!dir.join("iTunesSD.kessler-tmp").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
