use super::types::*;

const MHBD_HEADER: usize = 0x68;
const MHSD_HEADER: usize = 0x60;
const MHLT_HEADER: usize = 0x5c;
const MHLP_HEADER: usize = 0x5c;
const MHIT_HEADER: usize = 0xf4;
const MHOD_HEADER: usize = 0x18;
const MHYP_HEADER: usize = 0x6c;
const MHIP_HEADER: usize = 0x4c;

const DEFAULT_DB_VERSION: u32 = 0x13;

struct Chunk {
    buf: Vec<u8>,
}

impl Chunk {
    fn new(tag: &[u8; 4], header_len: usize) -> Self {
        let mut buf = vec![0u8; header_len];
        buf[0..4].copy_from_slice(tag);
        let mut c = Self { buf };
        c.u32(4, header_len as u32);
        c
    }

    fn u8_at(&mut self, off: usize, v: u8) {
        self.buf[off] = v;
    }

    fn u16(&mut self, off: usize, v: u16) {
        self.buf[off..off + 2].copy_from_slice(&v.to_le_bytes());
    }

    fn u32(&mut self, off: usize, v: u32) {
        self.buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
    }

    fn u64(&mut self, off: usize, v: u64) {
        self.buf[off..off + 8].copy_from_slice(&v.to_le_bytes());
    }

    fn append(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    fn finish(mut self) -> Vec<u8> {
        let total = self.buf.len() as u32;
        self.u32(8, total);
        self.buf
    }

    fn finish_counted(self) -> Vec<u8> {
        self.buf
    }
}

fn string_mhod(kind: MhodType, value: &str) -> Vec<u8> {
    let utf16: Vec<u8> = value.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();

    let mut c = Chunk::new(b"mhod", MHOD_HEADER);
    c.u32(12, kind.as_raw());
    c.append(&1u32.to_le_bytes());
    c.append(&(utf16.len() as u32).to_le_bytes());
    c.append(&1u32.to_le_bytes());
    c.append(&[0u8; 4]);
    c.append(&utf16);
    c.finish()
}

fn position_mhod(position: u32) -> Vec<u8> {
    let mut c = Chunk::new(b"mhod", MHOD_HEADER);
    c.u32(12, MhodType::PlaylistPosition.as_raw());
    c.append(&[0u8; 20]);
    c.u32(24, position);
    c.finish()
}

fn push_if_set(mhods: &mut Vec<Vec<u8>>, kind: MhodType, value: &str) {
    if !value.is_empty() {
        mhods.push(string_mhod(kind, value));
    }
}

fn filetype_marker(track: &IpodTrack) -> u32 {
    let ext = track
        .path
        .as_ref()
        .and_then(|p| p.extension())
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let tag: &[u8; 4] = match ext.as_str() {
        "mp3" => b"MP3 ",
        "aac" | "m4a" | "mp4" => b"M4A ",
        "wav" => b"WAV ",
        "aif" | "aiff" => b"AIFF",
        _ => return 0,
    };
    u32::from_be_bytes(*tag)
}

fn relative_to_ipod_path(track: &IpodTrack) -> Option<String> {
    let path = track.path.as_ref()?;
    let joined: Vec<String> = path
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    Some(format!(":{}", joined.join(":")))
}

fn track_chunk(track: &IpodTrack) -> Vec<u8> {
    let mut mhods: Vec<Vec<u8>> = Vec::new();
    push_if_set(&mut mhods, MhodType::Title, &track.title);
    if let Some(p) = relative_to_ipod_path(track) {
        mhods.push(string_mhod(MhodType::Path, &p));
    }
    push_if_set(&mut mhods, MhodType::Album, &track.album);
    push_if_set(&mut mhods, MhodType::Artist, &track.artist);
    push_if_set(&mut mhods, MhodType::Genre, &track.genre);
    push_if_set(&mut mhods, MhodType::Filetype, &track.filetype);
    push_if_set(&mut mhods, MhodType::Comment, &track.comment);
    push_if_set(&mut mhods, MhodType::Composer, &track.composer);
    push_if_set(&mut mhods, MhodType::AlbumArtist, &track.album_artist);

    let mut c = Chunk::new(b"mhit", MHIT_HEADER);
    c.u32(12, mhods.len() as u32);
    c.u32(16, track.id);
    c.u32(20, 1);
    c.u32(24, filetype_marker(track));
    c.u8_at(30, track.compilation as u8);
    c.u8_at(31, track.rating);
    c.u32(32, unix_to_mac(track.date_modified));
    c.u32(36, track.size_bytes);
    c.u32(40, track.length_ms);
    c.u32(44, track.track_number);
    c.u32(48, track.total_tracks);
    c.u32(52, track.year);
    c.u32(56, track.bitrate);
    c.u32(60, track.sample_rate << 16);
    c.u32(80, track.play_count);
    c.u32(84, track.play_count);
    c.u32(76, track.soundcheck);
    c.u32(88, unix_to_mac(track.last_played));
    c.u32(92, track.disc_number);
    c.u32(96, track.total_discs);
    c.u32(104, unix_to_mac(track.date_added));
    c.u64(112, track.dbid);
    c.u8_at(120, 0);
    c.u32(128, track.artwork_size);
    c.u32(156, track.skip_count);
    c.u8_at(164, track.has_artwork as u8);
    c.u32(0xD0, 1);

    for m in &mhods {
        c.append(m);
    }
    c.finish()
}

fn playlist_item(track_id: u32, position: u32) -> Vec<u8> {
    let pos = position_mhod(position);
    let mut c = Chunk::new(b"mhip", MHIP_HEADER);
    c.u32(12, 1);
    c.u32(24, track_id);
    c.append(&pos);
    c.finish()
}

pub const INDEX_FIELDS: [u32; 5] = [0x03, 0x05, 0x04, 0x07, 0x12];

fn index_key(text: &str) -> (u8, String, &str) {
    let (class, key) = crate::db::shelf_key(text);
    (class, key, text)
}

fn index_order(tracks: &[IpodTrack], field: u32) -> Vec<u32> {
    let mut order: Vec<u32> = (0..tracks.len() as u32).collect();
    let rest = |t: &IpodTrack| (t.disc_number, t.track_number, crate::db::sort_key(&t.title));
    order.sort_by(|&a, &b| {
        let (a, b) = (&tracks[a as usize], &tracks[b as usize]);
        match field {
            0x03 => index_key(&a.title).cmp(&index_key(&b.title)),
            0x04 => index_key(&a.album).cmp(&index_key(&b.album)).then_with(|| rest(a).cmp(&rest(b))),
            0x07 => index_key(&a.genre)
                .cmp(&index_key(&b.genre))
                .then_with(|| index_key(&a.artist).cmp(&index_key(&b.artist)))
                .then_with(|| index_key(&a.album).cmp(&index_key(&b.album)))
                .then_with(|| rest(a).cmp(&rest(b))),
            0x12 => index_key(&a.composer)
                .cmp(&index_key(&b.composer))
                .then_with(|| index_key(&a.album).cmp(&index_key(&b.album)))
                .then_with(|| rest(a).cmp(&rest(b))),
            _ => index_key(&a.artist)
                .cmp(&index_key(&b.artist))
                .then_with(|| index_key(&a.album).cmp(&index_key(&b.album)))
                .then_with(|| rest(a).cmp(&rest(b))),
        }
    });
    order
}

fn index_mhod(order: &[u32], field: u32) -> Vec<u8> {
    let mut c = Chunk::new(b"mhod", MHOD_HEADER);
    c.u32(12, 52);
    c.append(&field.to_le_bytes());
    c.append(&(order.len() as u32).to_le_bytes());
    c.append(&[0u8; 40]);
    for at in order {
        c.append(&at.to_le_bytes());
    }
    c.finish()
}

fn playlist_columns_mhod() -> Vec<u8> {
    let mut words: Vec<u32> = vec![0; 6];
    words.extend([0x010084, 0x05, 0x09, 0x03, 0x120001]);
    for word in [0xc80002, 0x3c000d, 0x7d0004, 0x7d0003, 0x640008] {
        words.extend([0, 0, 0, word]);
    }
    words.extend([0, 0, 0, 0x640017, 0x01, 0, 0, 0x500014, 0x01, 0, 0, 0x7d0015, 0x01]);
    words.extend(std::iter::repeat_n(0, 114));
    let mut c = Chunk::new(b"mhod", MHOD_HEADER);
    c.u32(12, 100);
    for word in &words[2..] {
        c.append(&word.to_le_bytes());
    }
    c.finish()
}

fn playlist_chunk(playlist: &IpodPlaylist, tracks: &[IpodTrack]) -> Vec<u8> {
    let title = string_mhod(MhodType::Title, &playlist.name);
    let mut indices = Vec::new();
    if playlist.is_master {
        for field in INDEX_FIELDS {
            indices.push(index_mhod(&index_order(tracks, field), field));
        }
    }

    let mut c = Chunk::new(b"mhyp", MHYP_HEADER);
    c.u32(12, 2 + indices.len() as u32);
    c.u32(16, playlist.track_ids.len() as u32);
    c.u8_at(20, playlist.is_master as u8);
    c.u32(24, unix_to_mac(playlist.timestamp));
    c.u64(28, playlist.id);
    c.u16(40, 1);
    c.u16(42, playlist.is_podcast as u16);

    c.append(&title);
    c.append(&playlist_columns_mhod());
    for index in &indices {
        c.append(index);
    }
    for (position, track_id) in playlist.track_ids.iter().enumerate() {
        c.append(&playlist_item(*track_id, position as u32));
    }
    c.finish()
}

pub fn write(db: &ItunesDb) -> Vec<u8> {
    let ordered = in_master_order(db);
    let db = &numbered(ItunesDb { tracks: ordered, ..db.clone() });

    let mut tracks = Chunk::new(b"mhlt", MHLT_HEADER);
    tracks.u32(8, db.tracks.len() as u32);
    for t in &db.tracks {
        tracks.append(&track_chunk(t));
    }

    let mut playlists = Chunk::new(b"mhlp", MHLP_HEADER);
    playlists.u32(8, db.playlists.len() as u32);
    for p in &db.playlists {
        playlists.append(&playlist_chunk(p, &db.tracks));
    }

    let mut dataset_tracks = Chunk::new(b"mhsd", MHSD_HEADER);
    dataset_tracks.u32(12, 1);
    dataset_tracks.append(&tracks.finish_counted());

    let mut dataset_playlists = Chunk::new(b"mhsd", MHSD_HEADER);
    dataset_playlists.u32(12, 2);
    dataset_playlists.append(&playlists.finish_counted());

    let mut root = Chunk::new(b"mhbd", MHBD_HEADER);
    root.u32(12, 1);
    root.u32(
        16,
        if db.version == 0 {
            DEFAULT_DB_VERSION
        } else {
            db.version
        },
    );
    root.u32(20, 2);
    root.u64(24, db.db_id);
    root.u16(32, 1);

    root.append(&dataset_tracks.finish());
    root.append(&dataset_playlists.finish());
    root.finish()
}

fn numbered(mut db: ItunesDb) -> ItunesDb {
    if db.tracks.windows(2).all(|w| w[0].id < w[1].id) {
        return db;
    }
    let new: std::collections::HashMap<u32, u32> =
        db.tracks.iter().enumerate().map(|(at, t)| (t.id, FIRST_TRACK_ID + at as u32)).collect();
    for track in &mut db.tracks {
        track.id = new[&track.id];
    }
    for playlist in &mut db.playlists {
        playlist.track_ids = playlist.track_ids.iter().filter_map(|id| new.get(id).copied()).collect();
    }
    db
}

const FIRST_TRACK_ID: u32 = 1000;

fn in_master_order(db: &ItunesDb) -> Vec<IpodTrack> {
    let Some(master) = db.playlists.iter().find(|p| p.is_master) else {
        return db.tracks.clone();
    };
    let at: std::collections::HashMap<u32, usize> =
        db.tracks.iter().enumerate().map(|(i, t)| (t.id, i)).collect();
    let mut taken = vec![false; db.tracks.len()];
    let mut ordered = Vec::with_capacity(db.tracks.len());
    for id in &master.track_ids {
        if let Some(&i) = at.get(id) {
            if !std::mem::replace(&mut taken[i], true) {
                ordered.push(db.tracks[i].clone());
            }
        }
    }
    ordered.extend(db.tracks.iter().zip(&taken).filter(|(_, t)| !**t).map(|(track, _)| track.clone()));
    ordered
}

pub fn master_playlist(name: &str, db: &ItunesDb) -> IpodPlaylist {
    IpodPlaylist {
        id: 1,
        name: name.to_string(),
        is_master: true,
        is_podcast: false,
        track_ids: index_order(&db.tracks, 0x05)
            .into_iter()
            .map(|at| db.tracks[at as usize].id)
            .collect(),
        timestamp: Some(now_unix()),
        indexed: INDEX_FIELDS.to_vec(),
        jumps: 0,
    }
}

pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {

    #[test]
    fn the_artist_index_is_sorted_as_itunes_sorts_it() {
        let names = ["", "6955", "(((Microwaves)))", "40318", "Acid Mothers Guru Guru", "ÄÄNIPÄÄ", "Zeni Geva"];
        let tracks: Vec<IpodTrack> = names
            .iter()
            .enumerate()
            .map(|(i, name)| IpodTrack { id: i as u32 + 1, artist: name.to_string(), ..Default::default() })
            .collect();
        let sorted: Vec<&str> = index_order(&tracks, 0x05).iter().map(|&at| names[at as usize]).collect();
        assert_eq!(sorted, ["ÄÄNIPÄÄ", "Acid Mothers Guru Guru", "(((Microwaves)))", "Zeni Geva", "6955", "40318", ""]);
    }

    #[test]
    fn a_string_mhod_says_1_where_itunes_does() {
        let bytes = string_mhod(MhodType::Artist, "Can");
        assert_eq!(u32::from_le_bytes(bytes[24..28].try_into().unwrap()), 1, "utf-16");
        assert_eq!(u32::from_le_bytes(bytes[28..32].try_into().unwrap()), 6);
        assert_eq!(u32::from_le_bytes(bytes[32..36].try_into().unwrap()), 1);
        assert_eq!(bytes.len(), 46);
    }

    #[test]
    fn the_master_playlist_is_laid_out_as_libgpod_does() {
        let mut db = ItunesDb { version: 0x13, ..Default::default() };
        db.tracks.push(IpodTrack { id: 7, artist: "Can".into(), title: "Vitamin C".into(), ..Default::default() });
        db.playlists.push(master_playlist("ipod", &db));
        let bytes = write(&db);
        let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        let mhyp = bytes.windows(4).position(|w| w == b"mhyp").unwrap();
        assert_eq!(word(mhyp + 12), 7, "seven mhods");
        assert_eq!(u16::from_le_bytes([bytes[mhyp + 40], bytes[mhyp + 41]]), 1);
        let mut at = mhyp + word(mhyp + 4) as usize;
        let mut kinds = Vec::new();
        for _ in 0..7 {
            assert_eq!(&bytes[at..at + 4], b"mhod");
            let kind = word(at + 12);
            kinds.push(if kind == 52 { kind * 100 + word(at + 24) } else { kind });
            if kind == 100 {
                assert_eq!(word(at + 8), 0x288);
            }
            at += word(at + 8) as usize;
        }
        assert_eq!(kinds, [1, 100, 5203, 5205, 5204, 5207, 5218]);
        assert_eq!(&bytes[at..at + 4], b"mhip");
    }

    #[test]
    fn the_track_list_stands_in_the_master_order() {
        let names = ["Popol Vuh", "Ash Ra Tempel", "Qluster", "Ash Ra Tempel", "Can"];
        let mut db = ItunesDb { version: 0x13, ..Default::default() };
        for (i, name) in names.iter().enumerate() {
            db.tracks.push(IpodTrack {
                id: 10 + i as u32,
                artist: name.to_string(),
                album: format!("{name} live"),
                title: format!("t{i}"),
                track_number: i as u32 + 1,
                ..Default::default()
            });
        }
        db.playlists.push(master_playlist("ipod", &db));
        let back = crate::ipod::itunesdb::parse(&write(&db)).unwrap();
        let listed: Vec<u32> = back.tracks.iter().map(|t| t.id).collect();
        let master = back.playlists.iter().find(|p| p.is_master).unwrap();
        assert_eq!(listed, master.track_ids);
        let artists: Vec<&str> = back.tracks.iter().map(|t| t.artist.as_str()).collect();
        assert_eq!(artists, ["Ash Ra Tempel", "Ash Ra Tempel", "Can", "Popol Vuh", "Qluster"]);
        let places = index_in(&write(&back), 0x05);
        let by_place: Vec<&str> = places.iter().map(|p| back.tracks[*p as usize].artist.as_str()).collect();
        assert_eq!(by_place, artists);
    }

    use super::*;
    use crate::ipod::itunesdb::{parse, test_writer};

    fn sample_db() -> ItunesDb {
        let tracks = vec![
            IpodTrack {
                id: 1,
                dbid: 0xAAAA_BBBB_CCCC_DDDD,
                title: "Xtal".into(),
                artist: "Aphex Twin".into(),
                album: "Selected Ambient Works 85-92".into(),
                album_artist: "Aphex Twin".into(),
                genre: "Electronic".into(),
                path: Some("iPod_Control/Music/F00/AAAA.mp3".into()),
                track_number: 1,
                total_tracks: 13,
                disc_number: 1,
                total_discs: 1,
                year: 1992,
                length_ms: 291_000,
                size_bytes: 3_000_000,
                bitrate: 320,
                sample_rate: 44_100,
                rating: 100,
                play_count: 7,
                date_added: Some(1_600_000_000),
                ..Default::default()
            },
            IpodTrack {
                id: 2,
                title: "Ümlaut · 坂本龍一".into(),
                artist: "Björk".into(),
                album: "Homogénic".into(),
                path: Some("iPod_Control/Music/F01/BBBB.mp3".into()),
                length_ms: 240_000,
                size_bytes: 4_000_000,
                sample_rate: 44_100,
                rating: 80,
                ..Default::default()
            },
        ];

        let mut db = ItunesDb {
            version: 0x13,
            db_id: 0x0123_4567_89AB_CDEF,
            tracks,
            playlists: Vec::new(),
        };
        db.playlists = vec![
            master_playlist("kessler", &db),
            IpodPlaylist {
                id: 42,
                name: "Late Night".into(),
                is_master: false,
                is_podcast: false,
                track_ids: vec![2, 1],
                timestamp: Some(1_600_000_000),
                indexed: Vec::new(),
                jumps: 0,
            },
        ];
        db
    }

    fn index_in(bytes: &[u8], field: u32) -> Vec<u32> {
        let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        for at in 0..bytes.len().saturating_sub(4) {
            if &bytes[at..at + 4] == b"mhod" && word(at + 12) == 52 && word(at + 24) == field {
                let count = word(at + 28) as usize;
                return (0..count).map(|i| word(at + 72 + 4 * i)).collect();
            }
        }
        panic!("no index for field {field}");
    }

    #[test]
    fn the_artist_index_keeps_an_artist_and_a_record_together() {
        let track = |id: u32, artist: &str, album: &str, number: u32| IpodTrack {
            id,
            artist: artist.into(),
            album: album.into(),
            title: format!("{id}"),
            track_number: number,
            ..Default::default()
        };
        let mut db = ItunesDb {
            tracks: vec![
                track(1, "Can", "Rare Two", 2),
                track(2, "Mr Oizo", "Analog Worms Attack", 1),
                track(3, "Can", "Monster Movie", 1),
                track(4, "Mr Oizo", "Analog Worms Attack", 2),
                track(5, "Can", "Rare Two", 1),
            ],
            ..Default::default()
        };
        db.playlists = vec![master_playlist("ipod", &db)];
        let bytes = write(&db);
        let reparsed = parse(&bytes).unwrap();
        let title = |at: u32| reparsed.tracks[at as usize].title.clone();
        let artists: Vec<String> = index_in(&bytes, 0x05).iter().map(|&at| title(at)).collect();
        assert_eq!(artists, ["3", "5", "1", "2", "4"]);
        let albums: Vec<String> = index_in(&bytes, 0x04).iter().map(|&at| title(at)).collect();
        assert_eq!(albums, ["2", "4", "3", "5", "1"]);
        let listed: Vec<u32> = reparsed.tracks.iter().map(|t| t.id).collect();
        assert_eq!(reparsed.master_playlist().unwrap().track_ids, listed);
        assert_eq!(reparsed.tracks.len(), 5);
    }

    #[test]
    fn tracks_are_written_ticked_and_as_audio() {
        let db = sample_db();
        let bytes = write(&db);
        let reparsed = parse(&bytes).unwrap();
        assert!(reparsed.tracks.iter().all(|t| !t.unchecked));
        let mut seen = 0;
        for at in 0..bytes.len() - 4 {
            if &bytes[at..at + 4] == b"mhit" {
                assert_eq!(bytes[at + 0x78], 0, "ticked");
                assert_eq!(u32::from_le_bytes(bytes[at + 0xD0..at + 0xD4].try_into().unwrap()), 1, "audio");
                seen += 1;
            }
        }
        assert_eq!(seen, db.tracks.len());
        let mut old = bytes.clone();
        let first = old.windows(4).position(|w| w == b"mhit").unwrap();
        old[first + 0x78] = 1;
        assert!(parse(&old).unwrap().tracks[0].unchecked);
    }

    #[test]
    fn written_database_parses_back_identically() {
        let original = sample_db();
        let bytes = write(&original);
        let reparsed = parse(&bytes).expect("written database must parse");

        assert_eq!(reparsed.version, original.version);
        assert_eq!(reparsed.db_id, original.db_id);
        assert_eq!(reparsed.tracks.len(), original.tracks.len());

        for (a, b) in original.tracks.iter().zip(&reparsed.tracks) {
            assert_eq!(a.id, b.id, "track id");
            assert_eq!(a.dbid, b.dbid, "dbid");
            assert_eq!(a.title, b.title, "title");
            assert_eq!(a.artist, b.artist, "artist");
            assert_eq!(a.album, b.album, "album");
            assert_eq!(a.album_artist, b.album_artist, "album artist");
            assert_eq!(a.genre, b.genre, "genre");
            assert_eq!(a.path, b.path, "path");
            assert_eq!(a.year, b.year, "year");
            assert_eq!(a.track_number, b.track_number, "track number");
            assert_eq!(a.total_tracks, b.total_tracks, "total tracks");
            assert_eq!(a.disc_number, b.disc_number, "disc number");
            assert_eq!(a.length_ms, b.length_ms, "length");
            assert_eq!(a.size_bytes, b.size_bytes, "size");
            assert_eq!(a.bitrate, b.bitrate, "bitrate");
            assert_eq!(a.sample_rate, b.sample_rate, "sample rate");
            assert_eq!(a.rating, b.rating, "rating");
            assert_eq!(a.play_count, b.play_count, "play count");
            assert_eq!(a.date_added, b.date_added, "date added");
        }
    }

    #[test]
    fn playlists_survive_the_round_trip_in_order() {
        let original = sample_db();
        let reparsed = parse(&write(&original)).unwrap();

        let master = reparsed.master_playlist().expect("master playlist");
        assert_eq!(master.name, "kessler");
        assert_eq!(master.track_ids, vec![1, 2]);

        let user: Vec<_> = reparsed.user_playlists().collect();
        assert_eq!(user.len(), 1);
        assert_eq!(user[0].name, "Late Night");
        assert_eq!(user[0].track_ids, vec![2, 1]);
    }

    #[test]
    fn round_trips_a_database_we_did_not_write() {
        let from_fixture = parse(&test_writer::build_test_db()).unwrap();
        let round_tripped = parse(&write(&from_fixture)).unwrap();

        assert_eq!(round_tripped.tracks.len(), from_fixture.tracks.len());
        for (a, b) in from_fixture.tracks.iter().zip(&round_tripped.tracks) {
            assert_eq!(a.title, b.title);
            assert_eq!(a.artist, b.artist);
            assert_eq!(a.path, b.path);
            assert_eq!(a.rating, b.rating);
            assert_eq!(a.length_ms, b.length_ms);
            assert_eq!(a.date_added, b.date_added);
        }
        assert_eq!(
            round_tripped.master_playlist().map(|p| p.track_ids.clone()),
            from_fixture.master_playlist().map(|p| p.track_ids.clone())
        );
    }

    #[test]
    fn empty_database_is_still_valid() {
        let db = ItunesDb {
            version: 0x13,
            db_id: 7,
            tracks: Vec::new(),
            playlists: vec![IpodPlaylist {
                id: 1,
                name: "kessler".into(),
                is_master: true,
                ..Default::default()
            }],
        };
        let reparsed = parse(&write(&db)).unwrap();
        assert!(reparsed.tracks.is_empty());
        assert_eq!(reparsed.master_playlist().unwrap().name, "kessler");
    }

    #[test]
    fn empty_tags_do_not_become_empty_objects() {
        let db = ItunesDb {
            version: 0x13,
            db_id: 1,
            tracks: vec![IpodTrack {
                id: 1,
                title: "Only a title".into(),
                ..Default::default()
            }],
            playlists: vec![],
        };
        let reparsed = parse(&write(&db)).unwrap();
        let t = &reparsed.tracks[0];
        assert_eq!(t.title, "Only a title");
        assert_eq!(t.artist, "");
        assert_eq!(t.album, "");
        assert_eq!(t.path, None);
    }

    #[test]
    fn writes_the_ipod_path_spelling() {
        let db = ItunesDb {
            version: 0x13,
            db_id: 1,
            tracks: vec![IpodTrack {
                id: 1,
                path: Some("iPod_Control/Music/F12/ZZZZ.mp3".into()),
                ..Default::default()
            }],
            playlists: vec![],
        };
        let bytes = write(&db);
        let utf16: Vec<u8> = ":iPod_Control:Music:F12:ZZZZ.mp3"
            .encode_utf16()
            .flat_map(|u| u.to_le_bytes())
            .collect();
        assert!(
            bytes.windows(utf16.len()).any(|w| w == utf16),
            "iPod-style path not found in the written database"
        );
    }

    #[test]
    fn timestamps_round_trip_through_the_mac_epoch() {
        assert_eq!(mac_to_unix(unix_to_mac(Some(1_600_000_000))), Some(1_600_000_000));
        assert_eq!(unix_to_mac(None), 0);
        assert_eq!(mac_to_unix(0), None);
    }
}

#[cfg(test)]
mod fake_device {
    use super::*;
    use crate::ipod::itunesdb::{IpodPlaylist, IpodTrack};

    #[test]
    fn make_one() {
        let Ok(root) = std::env::var("KESSLER_FAKE_IPOD") else {
            return;
        };
        let root = std::path::PathBuf::from(root);

        let records = [
            ("Autechre", "Amber", 1994, &["Foil", "Montreal", "Slip"][..]),
            ("Autechre", "Tri Repetae", 1995, &["Dael", "Clipper"][..]),
            ("Aphex Twin", "Drukqs", 2001, &["Jynweythek", "Vordhosbn"][..]),
            ("Boards of Canada", "Geogaddi", 2002, &["Ready Lets Go", "Music Is Math"][..]),
            ("Coil", "Musick to Play in the Dark", 1999, &["Are You Shivering?"][..]),
            ("", "", 0, &["ohne alles"][..]),
        ];

        let mut tracks = Vec::new();
        let mut id = 1u32;
        for (artist, album, year, titles) in records {
            for (nr, title) in titles.iter().enumerate() {
                tracks.push(IpodTrack {
                    id,
                    title: (*title).into(),
                    artist: artist.into(),
                    album: album.into(),
                    year,
                    track_number: nr as u32 + 1,
                    length_ms: 180_000 + id * 1000,
                    size_bytes: 5_000_000,
                    bitrate: 320,
                    sample_rate: 44100,
                    filetype: "MPEG audio file".into(),
                    path: Some(std::path::PathBuf::from(format!(":Music:F00:{id:04}.mp3"))),
                    ..Default::default()
                });
                id += 1;
            }
        }

        let all: Vec<u32> = tracks.iter().map(|t| t.id).collect();
        let db = ItunesDb {
            version: 0x75,
            db_id: 0x1234_5678,
            tracks,
            playlists: vec![
                IpodPlaylist {
                    name: "iPod".into(),
                    is_master: true,
                    track_ids: all.clone(),
                    ..Default::default()
                },
                IpodPlaylist {
                    name: "lange stücke".into(),
                    is_master: false,
                    track_ids: all.iter().take(3).copied().collect(),
                    ..Default::default()
                },
            ],
        };

        let dir = root.join("iPod_Control").join("iTunes");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("iTunesDB"), write(&db)).unwrap();
        println!("{} titel nach {}", db.tracks.len(), root.display());
    }
}
