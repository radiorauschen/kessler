use std::path::PathBuf;

use super::types::*;
use crate::error::{KesslerError, Result};

struct Cursor<'a> {
    data: &'a [u8],
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data }
    }

    fn bytes(&self, off: usize, len: usize) -> Result<&'a [u8]> {
        self.data
            .get(off..off + len)
            .ok_or_else(|| corrupt(format!("read of {len} bytes at {off} runs past end of file")))
    }

    fn tag(&self, off: usize) -> Result<[u8; 4]> {
        let b = self.bytes(off, 4)?;
        Ok([b[0], b[1], b[2], b[3]])
    }

    fn u8_at(&self, off: usize) -> Result<u8> {
        Ok(self.bytes(off, 1)?[0])
    }

    fn u16_at(&self, off: usize) -> Result<u16> {
        let b = self.bytes(off, 2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32_at(&self, off: usize) -> Result<u32> {
        let b = self.bytes(off, 4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn u64_at(&self, off: usize) -> Result<u64> {
        let b = self.bytes(off, 8)?;
        Ok(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    fn expect_tag(&self, off: usize, tag: &[u8; 4]) -> Result<()> {
        let found = self.tag(off)?;
        if &found == tag {
            Ok(())
        } else {
            Err(corrupt(format!(
                "expected '{}' at offset {off}, found {:?}",
                String::from_utf8_lossy(tag),
                String::from_utf8_lossy(&found)
            )))
        }
    }

    fn len(&self) -> usize {
        self.data.len()
    }
}

fn corrupt(msg: impl Into<String>) -> KesslerError {
    KesslerError::Ipod(format!("iTunesDB corrupt: {}", msg.into()))
}

struct Chunk {
    header_len: usize,
    total_len: usize,
}

impl Chunk {
    fn read(cur: &Cursor, off: usize, tag: &[u8; 4], min_header: usize) -> Result<Self> {
        cur.expect_tag(off, tag)?;
        let header_len = cur.u32_at(off + 4)? as usize;
        let total_len = cur.u32_at(off + 8)? as usize;
        let name = String::from_utf8_lossy(tag).to_string();

        if header_len < min_header {
            return Err(corrupt(format!(
                "{name} header at {off} is {header_len} bytes, expected at least {min_header}"
            )));
        }
        if total_len < header_len {
            return Err(corrupt(format!(
                "{name} at {off} claims total length {total_len} < header length {header_len}"
            )));
        }
        if off + total_len > cur.len() {
            return Err(corrupt(format!(
                "{name} at {off} claims total length {total_len}, past end of file"
            )));
        }
        Ok(Self {
            header_len,
            total_len,
        })
    }
}

fn read_mhod_string(cur: &Cursor, mhod_off: usize, header_len: usize) -> Result<String> {
    let base = mhod_off + header_len;
    let encoding = cur.u32_at(base)?;
    let byte_len = cur.u32_at(base + 4)? as usize;
    let raw = cur.bytes(base + 16, byte_len)?;

    if encoding == 2 {
        Ok(String::from_utf8_lossy(raw).into_owned())
    } else {
        let units: Vec<u16> = raw
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&c| u16::from_le_bytes(c))
            .collect();
        Ok(String::from_utf16_lossy(&units))
    }
}

fn ipod_path_to_relative(s: &str) -> PathBuf {
    s.split(':').filter(|p| !p.is_empty()).collect()
}

pub fn parse(data: &[u8]) -> Result<ItunesDb> {
    let cur = Cursor::new(data);

    if data.len() < 32 {
        return Err(corrupt("file is too short to contain an mhbd header"));
    }

    if cur.tag(0)? != *TAG_MHBD {
        if data.starts_with(&[0x78]) {
            return Err(KesslerError::Ipod(
                "this iTunesDB looks zlib-compressed (newer iPod/iOS devices). \
                 kessler cannot read that format yet."
                    .into(),
            ));
        }
        if cur.tag(0)? == *b"dbhm" {
            return Err(KesslerError::Ipod(
                "this iTunesDB is byte-reversed (big-endian, as used by some \
                 non-Apple devices). kessler only reads little-endian databases."
                    .into(),
            ));
        }
        return Err(corrupt("file does not start with an 'mhbd' header"));
    }

    let mhbd = Chunk::read(&cur, 0, TAG_MHBD, 32)?;
    let version = cur.u32_at(16)?;
    let child_count = cur.u32_at(20)?;
    let db_id = if mhbd.header_len >= 32 {
        cur.u64_at(24)?
    } else {
        0
    };

    let mut db = ItunesDb {
        version,
        db_id,
        ..Default::default()
    };

    let mut seek = mhbd.header_len;
    let mut playlists_loaded = false;

    for _ in 0..child_count {
        if seek >= cur.len() {
            break;
        }
        let mhsd = Chunk::read(&cur, seek, TAG_MHSD, 16)?;
        let kind = DatasetType::from_raw(cur.u32_at(seek + 12)?);

        match kind {
            DatasetType::Tracks => {
                db.tracks = parse_track_list(&cur, seek + mhsd.header_len)?;
            }
            DatasetType::Playlists | DatasetType::PodcastPlaylists if !playlists_loaded => {
                db.playlists = parse_playlist_list(&cur, seek + mhsd.header_len)?;
                playlists_loaded = true;
            }
            _ => {}
        }

        seek += mhsd.total_len;
    }

    Ok(db)
}

fn parse_track_list(cur: &Cursor, mhlt_off: usize) -> Result<Vec<IpodTrack>> {
    cur.expect_tag(mhlt_off, TAG_MHLT)?;
    let header_len = cur.u32_at(mhlt_off + 4)? as usize;
    let count = cur.u32_at(mhlt_off + 8)? as usize;

    let mut tracks = Vec::with_capacity(count.min(100_000));
    let mut seek = mhlt_off + header_len;

    for _ in 0..count {
        if seek >= cur.len() {
            break;
        }
        let (track, next) = parse_track(cur, seek)?;
        tracks.push(track);
        seek = next;
    }

    Ok(tracks)
}

fn parse_track(cur: &Cursor, off: usize) -> Result<(IpodTrack, usize)> {
    let mhit = Chunk::read(cur, off, TAG_MHIT, 0x9c)?;
    let mhod_count = cur.u32_at(off + 12)?;

    let sample_rate_packed = cur.u32_at(off + 60)?;

    let mut track = IpodTrack {
        id: cur.u32_at(off + 16)?,
        filetype: String::new(),
        compilation: cur.u8_at(off + 30)? != 0,
        rating: cur.u8_at(off + 31)?,
        date_modified: mac_to_unix(cur.u32_at(off + 32)?),
        size_bytes: cur.u32_at(off + 36)?,
        length_ms: cur.u32_at(off + 40)?,
        track_number: cur.u32_at(off + 44)?,
        total_tracks: cur.u32_at(off + 48)?,
        year: cur.u32_at(off + 52)?,
        bitrate: cur.u32_at(off + 56)?,
        sample_rate: sample_rate_packed >> 16,
        soundcheck: cur.u32_at(off + 76)?,
        play_count: cur.u32_at(off + 80)?,
        last_played: mac_to_unix(cur.u32_at(off + 88)?),
        disc_number: cur.u32_at(off + 92)?,
        total_discs: cur.u32_at(off + 96)?,
        date_added: mac_to_unix(cur.u32_at(off + 104)?),
        dbid: cur.u64_at(off + 112)?,
        artwork_size: cur.u32_at(off + 128)?,
        unchecked: cur.u8_at(off + 120)? != 0,
        ..Default::default()
    };

    if mhit.header_len >= 0xf4 {
        track.skip_count = cur.u32_at(off + 156)?;
        track.has_artwork = cur.u8_at(off + 164)? != 0;
    }

    let mut seek = off + mhit.header_len;
    for _ in 0..mhod_count {
        if seek >= cur.len() {
            break;
        }
        let mhod = Chunk::read(cur, seek, TAG_MHOD, 16)?;
        let kind = MhodType::from_raw(cur.u32_at(seek + 12)?);

        let value = match kind {
            MhodType::Title
            | MhodType::Path
            | MhodType::Album
            | MhodType::Artist
            | MhodType::Genre
            | MhodType::Filetype
            | MhodType::Comment
            | MhodType::Composer
            | MhodType::AlbumArtist => Some(read_mhod_string(cur, seek, mhod.header_len)?),
            _ => None,
        };

        if let Some(v) = value {
            match kind {
                MhodType::Title => track.title = v,
                MhodType::Path => track.path = Some(ipod_path_to_relative(&v)),
                MhodType::Album => track.album = v,
                MhodType::Artist => track.artist = v,
                MhodType::Genre => track.genre = v,
                MhodType::Filetype => track.filetype = v,
                MhodType::Comment => track.comment = v,
                MhodType::Composer => track.composer = v,
                MhodType::AlbumArtist => track.album_artist = v,
                _ => {}
            }
        }

        seek += mhod.total_len;
    }

    let next = if mhit.total_len > mhit.header_len {
        off + mhit.total_len
    } else {
        seek
    };
    Ok((track, next))
}

fn parse_playlist_list(cur: &Cursor, mhlp_off: usize) -> Result<Vec<IpodPlaylist>> {
    cur.expect_tag(mhlp_off, TAG_MHLP)?;
    let header_len = cur.u32_at(mhlp_off + 4)? as usize;
    let count = cur.u32_at(mhlp_off + 8)? as usize;

    let mut playlists = Vec::with_capacity(count.min(10_000));
    let mut seek = mhlp_off + header_len;

    for _ in 0..count {
        if seek >= cur.len() {
            break;
        }
        let (playlist, next) = parse_playlist(cur, seek)?;
        playlists.push(playlist);
        seek = next;
    }

    Ok(playlists)
}

fn parse_playlist(cur: &Cursor, off: usize) -> Result<(IpodPlaylist, usize)> {
    let mhyp = Chunk::read(cur, off, TAG_MHYP, 48)?;
    let mhod_count = cur.u32_at(off + 12)?;
    let item_count = cur.u32_at(off + 16)?;

    let mut playlist = IpodPlaylist {
        is_master: cur.u8_at(off + 20)? != 0,
        timestamp: mac_to_unix(cur.u32_at(off + 24)?),
        id: cur.u64_at(off + 28)?,
        is_podcast: cur.u16_at(off + 42)? != 0,
        ..Default::default()
    };

    let mut seek = off + mhyp.header_len;
    for _ in 0..mhod_count {
        if seek >= cur.len() {
            break;
        }
        let mhod = Chunk::read(cur, seek, TAG_MHOD, 16)?;
        if cur.u32_at(seek + 12)? == 52 {
            if let Ok(field) = cur.u32_at(seek + mhod.header_len) {
                playlist.indexed.push(field);
            }
        }
        if cur.u32_at(seek + 12)? == 53 {
            playlist.jumps += 1;
        }
        if MhodType::from_raw(cur.u32_at(seek + 12)?) == MhodType::Title && playlist.name.is_empty()
        {
            playlist.name = read_mhod_string(cur, seek, mhod.header_len)?;
        }
        seek += mhod.total_len;
    }

    let mut entries: Vec<(u32, u32)> = Vec::with_capacity(item_count.min(100_000) as usize);
    for _ in 0..item_count {
        if seek >= cur.len() {
            break;
        }
        let (track_id, position, next) = parse_playlist_item(cur, seek)?;
        entries.push((position, track_id));
        seek = next;
    }

    entries.sort_by_key(|(pos, _)| *pos);
    playlist.track_ids = entries.into_iter().map(|(_, id)| id).collect();

    let next = if mhyp.total_len > mhyp.header_len {
        off + mhyp.total_len
    } else {
        seek
    };
    Ok((playlist, next))
}

fn parse_playlist_item(cur: &Cursor, off: usize) -> Result<(u32, u32, usize)> {
    let mhip = Chunk::read(cur, off, TAG_MHIP, 36)?;
    let mhod_count = cur.u32_at(off + 12)?;
    let track_id = cur.u32_at(off + 24)?;

    let mut position = 0u32;
    let mut seek = off + mhip.header_len;

    for _ in 0..mhod_count {
        if seek >= cur.len() {
            break;
        }
        let mhod = Chunk::read(cur, seek, TAG_MHOD, 16)?;
        if MhodType::from_raw(cur.u32_at(seek + 12)?) == MhodType::PlaylistPosition {
            position = cur.u32_at(seek + 24)?;
        }
        seek += mhod.total_len;
    }

    let next = if mhip.total_len > mhip.header_len {
        off + mhip.total_len
    } else {
        seek
    };
    Ok((track_id, position, next))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipod::itunesdb::test_writer::*;

    #[test]
    fn rejects_non_itunesdb() {
        let err = parse(b"this is not an iTunesDB at all, not even close").unwrap_err();
        assert!(err.to_string().contains("mhbd"), "unexpected: {err}");
    }

    #[test]
    fn rejects_truncated_file() {
        assert!(parse(b"mhbd").is_err());
    }

    #[test]
    fn recognises_compressed_database() {
        let mut data = vec![0x78, 0x9c];
        data.extend_from_slice(&[0u8; 64]);
        let err = parse(&data).unwrap_err();
        assert!(err.to_string().contains("compressed"), "unexpected: {err}");
    }

    #[test]
    fn parses_tracks_and_metadata() {
        let db = parse(&build_test_db()).unwrap();

        assert_eq!(db.tracks.len(), 3);

        let first = &db.tracks[0];
        assert_eq!(first.id, 101);
        assert_eq!(first.title, "Xtal");
        assert_eq!(first.artist, "Aphex Twin");
        assert_eq!(first.album, "Selected Ambient Works 85-92");
        assert_eq!(first.year, 1992);
        assert_eq!(first.track_number, 1);
        assert_eq!(first.length_ms, 291_000);
        assert_eq!(first.sample_rate, 44_100);
        assert_eq!(first.stars(), 5);
        assert_eq!(first.duration_display(), "4:51");
        assert_eq!(
            first.path.as_deref(),
            Some(std::path::Path::new("iPod_Control/Music/F00/AAAA.mp3"))
        );
    }

    #[test]
    fn reads_utf16_and_utf8_strings() {
        let db = parse(&build_test_db()).unwrap();
        assert_eq!(db.tracks[1].title, "Ümlaut · 坂本龍一");
        assert_eq!(db.tracks[2].title, "Plain ASCII");
    }

    #[test]
    fn converts_mac_timestamps() {
        let db = parse(&build_test_db()).unwrap();
        assert_eq!(db.tracks[0].date_added, Some(0));
        assert_eq!(db.tracks[0].last_played, None);
    }

    #[test]
    fn parses_playlists_in_position_order() {
        let db = parse(&build_test_db()).unwrap();

        assert_eq!(db.playlists.len(), 2);

        let master = db.master_playlist().expect("master playlist");
        assert_eq!(master.name, "iPod");
        assert_eq!(master.track_ids, vec![101, 102, 103]);

        let user: Vec<_> = db.user_playlists().collect();
        assert_eq!(user.len(), 1);
        assert_eq!(user[0].name, "Late Night");
        assert_eq!(user[0].track_ids, vec![101, 103]);
    }

    #[test]
    fn track_lookup_and_totals() {
        let db = parse(&build_test_db()).unwrap();
        assert_eq!(db.track_by_id(102).map(|t| t.title.as_str()), Some("Ümlaut · 坂本龍一"));
        assert!(db.track_by_id(999).is_none());
        assert_eq!(db.total_bytes(), 3_000_000 + 4_000_000 + 5_000_000);
    }

    #[test]
    fn survives_unknown_mhod_types() {
        let db = parse(&build_db_with_unknown_mhod()).unwrap();
        assert_eq!(db.tracks.len(), 1);
        assert_eq!(db.tracks[0].title, "Survivor");
        assert_eq!(db.tracks[0].artist, "After Unknown");
    }

    #[test]
    fn rejects_chunk_claiming_impossible_length() {
        let mut data = build_test_db();
        let mhit_off = find_tag(&data, b"mhit").expect("mhit present");
        data[mhit_off + 8..mhit_off + 12].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        assert!(parse(&data).is_err());
    }

    #[test]
    fn rejects_chunk_with_zero_total_length() {
        let mut data = build_test_db();
        let mhit_off = find_tag(&data, b"mhit").expect("mhit present");
        data[mhit_off + 8..mhit_off + 12].copy_from_slice(&0u32.to_le_bytes());
        assert!(parse(&data).is_err());
    }
}
