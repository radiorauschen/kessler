//! Writer for the iPod `iTunesDB`.
//!
//! [`write`] produces a database for devices that do **not** require a
//! signed header (iPod 1G–5.5G, mini, nano 1–2, shuffle); [`write_signed`]
//! one for the iPod classic and the nano 3G and 4G, which want it signed with
//! their own FireWire GUID (see `hash58`). Callers must check
//! [`crate::ipod::device_info::DeviceInfo::writable`] first and pick by
//! [`crate::ipod::device_info::DeviceInfo::signing`] — this module has no way
//! to know what hardware the bytes are headed for.
//!
//! Layout mirrors `parser.rs`; every length field is computed from what was
//! actually written rather than hardcoded, so the two cannot drift apart.

use super::types::*;

/// Header sizes. These are the sizes iTunes 4.x–7.x wrote and every iPod in
/// the unsigned-database era accepts.
const MHBD_HEADER: usize = 0x68;
/// The header a signed database gets: the length iTunes 7.4 and libgpod
/// write, with room for the signature at 0x58.
const MHBD_HEADER_SIGNED: usize = 0xF4;
/// The oldest database version written for a device that wants a signature:
/// iTunes 7.4's, which those iPods came out with. Older ones it may refuse.
const SIGNED_DB_VERSION: u32 = 0x19;
const MHSD_HEADER: usize = 0x60;
const MHLT_HEADER: usize = 0x5c;
const MHLP_HEADER: usize = 0x5c;
const MHIT_HEADER: usize = 0xf4;
const MHOD_HEADER: usize = 0x18;
const MHYP_HEADER: usize = 0x6c;
const MHIP_HEADER: usize = 0x4c;

/// Database version written when the device's own database gave us no hint.
/// 0x13 is iTunes 7 era: new enough that the longer mhit fields are expected,
/// old enough that pre-Classic firmware is comfortable with it.
const DEFAULT_DB_VERSION: u32 = 0x13;

/// A chunk under construction. Fields are written at absolute offsets inside
/// the fixed-size header, then children are appended.
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

    /// Stamp the final size into the total-length field at +8.
    fn finish(mut self) -> Vec<u8> {
        let total = self.buf.len() as u32;
        self.u32(8, total);
        self.buf
    }

    /// For mhlt/mhlp, whose +8 field is a child count rather than a length.
    fn finish_counted(self) -> Vec<u8> {
        self.buf
    }
}

/// A string `mhod`.
///
/// Always written as UTF-16LE. UTF-8 payloads (marker 2) only appear in newer
/// databases, and older firmware does not expect them — UTF-16 is what every
/// supported generation was shipped reading.
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

/// The position `mhod` that gives a playlist entry its place in the order.
fn position_mhod(position: u32) -> Vec<u8> {
    let mut c = Chunk::new(b"mhod", MHOD_HEADER);
    c.u32(12, MhodType::PlaylistPosition.as_raw());
    c.append(&[0u8; 20]);
    c.u32(24, position);
    c.finish()
}

/// Only emit a string object when there is something to say — iTunes leaves
/// empty tags out rather than writing zero-length objects.
fn push_if_set(mhods: &mut Vec<Vec<u8>>, kind: MhodType, value: &str) {
    if !value.is_empty() {
        mhods.push(string_mhod(kind, value));
    }
}

/// The four-character format marker iPods store at mhit+24.
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

/// The path as the iPod stores it: colon-separated, with a leading colon.
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

/// The fields the iPod browses by, as its library index numbers them, in the
/// order libgpod writes them: title, artist, album, genre, composer (title,
/// album, artist … until then).
pub const INDEX_FIELDS: [u32; 5] = [0x03, 0x05, 0x04, 0x07, 0x12];

/// A string as the indices order it: as iTunes orders them on a 3G
/// (`crate::db::shelf_key`), then as written, so that two spellings of one
/// name sort side by side and each spelling's tracks stand together.
fn index_key(text: &str) -> (u8, String, &str) {
    let (class, key) = crate::db::shelf_key(text);
    (class, key, text)
}

/// The order of the tracks for one index field, iTunes's and libgpod's: the
/// field first, then album, disc, number and title, so a record's tracks
/// stand together and in their running order.
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

/// The library index for one field (`mhod` 52): every track, by its place in
/// the track list, in the field's order.
///
/// The iPod builds its Artists, Albums, Genres and Songs menus from these.
/// Without them a 3G walks the tracks in the order the database holds them and
/// folds only neighbours with the same name together — and a sync adds its
/// tracks at the end, so an artist synced in two goes stood in the menu twice,
/// one synced in six goes six times, and their records with them.
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

/// The long playlist `mhod` (type 100) libgpod writes after every playlist's
/// name: the columns iTunes shows it with. Its numbers are libgpod's; what they
/// mean nobody has written down, but iTunes and libgpod both write it, so a
/// database without it is one no iPod has been tested with.
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

/// Serialise a database to the bytes that go into
/// `iPod_Control/iTunes/iTunesDB`.
///
/// The result is always a complete, self-consistent file: this cannot fail
/// partway and leave a half-written database behind.
pub fn write(db: &ItunesDb) -> Vec<u8> {
    build(db, false)
}

/// [`write`] for a device that wants its database signed — the iPod classic,
/// the nano 3G and 4G — with its FireWire GUID: a longer header, a version
/// no older than those iPods know, and the signature in it.
pub fn write_signed(db: &ItunesDb, guid: &[u8; 8]) -> Vec<u8> {
    let mut bytes = build(db, true);
    crate::ipod::hash58::sign(&mut bytes, guid);
    bytes
}

/// The database, with the header for a signed one where `signed` says so;
/// signing it is the caller's.
fn build(db: &ItunesDb, signed: bool) -> Vec<u8> {
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

    let mut root = Chunk::new(b"mhbd", if signed { MHBD_HEADER_SIGNED } else { MHBD_HEADER });
    root.u32(12, 1);
    let version = if db.version == 0 { DEFAULT_DB_VERSION } else { db.version };
    root.u32(16, if signed { version.max(SIGNED_DB_VERSION) } else { version });
    root.u32(20, 2);
    root.u64(24, db.db_id);
    root.u16(32, 1);
    let language = if db.language == [0, 0] { language_code() } else { db.language };
    root.u8_at(0x46, language[0]);
    root.u8_at(0x47, language[1]);
    root.u64(0x48, if db.library_id == 0 { new_library_id(db.db_id) } else { db.library_id });

    root.append(&dataset_tracks.finish());
    root.append(&dataset_playlists.finish());
    root.finish()
}

/// The window's language as the two letters iTunes writes into the header.
fn language_code() -> [u8; 2] {
    match crate::lang::current() {
        crate::lang::Lang::De => *b"de",
        crate::lang::Lang::En => *b"en",
        crate::lang::Lang::Ja => *b"ja",
    }
}

/// A number for a library that has none yet: from the database's own where
/// it has one, else from the clock — any number that is not nought will do,
/// as long as it stays, and it does once it is written.
fn new_library_id(db_id: u64) -> u64 {
    let seed = if db_id != 0 {
        db_id
    } else {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64)
    };
    let mut x = seed ^ 0x9E37_79B9_7F4A_7C15;
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (x ^ (x >> 31)).max(1)
}

/// The same database with its tracks numbered afresh, rising in the order
/// they stand, and every playlist pointing at the new numbers.
///
/// A 3G finds a track by its number as if the numbers rose down the track
/// list, as they do in every database iTunes writes. The list stands in the
/// artists' order, and a track copied over later would carry a higher number
/// than the ones after it: the artists menu then stops at the first such
/// track (the same database renumbered lists everything).
///
/// Numbers that already rise are kept, so a database written twice keeps
/// its numbers.
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

/// The first track's number. Any rising run will do; this one was tried on a
/// 3G.
const FIRST_TRACK_ID: u32 = 1000;

/// The tracks in the master playlist's order, then any it does not list in
/// the order they were in.
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

/// Build the master playlist that lists every track in the database — in the
/// artist index's order, so a firmware that reads the list rather than the
/// indices finds each artist's tracks together too.
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

    /// The header's language and the library's number go out as they came
    /// in; where nothing wrote them, a language and a number are written and
    /// stay from then on.
    #[test]
    fn the_header_keeps_its_language_and_its_library() {
        let parse = crate::ipod::itunesdb::parse;
        let kept = ItunesDb { version: 0x13, db_id: 9, language: *b"fr", library_id: 0x1122_3344_5566_7788, tracks: Vec::new(), playlists: Vec::new() };
        let back = parse(&write(&kept)).unwrap();
        assert_eq!((back.language, back.library_id), (*b"fr", 0x1122_3344_5566_7788));
        let bare = ItunesDb { version: 0x13, db_id: 9, language: [0, 0], library_id: 0, tracks: Vec::new(), playlists: Vec::new() };
        let first = parse(&write(&bare)).unwrap();
        assert!(first.language != [0, 0] && first.library_id != 0, "filled in");
        let second = parse(&write(&first)).unwrap();
        assert_eq!((second.language, second.library_id), (first.language, first.library_id), "and kept");
    }

    /// The indices sort as iTunes sorts a 3G: marks before the first
    /// letter left aside, letters before digits, numbers as numbers, and the
    /// one track without an artist last — first, it left the 3G with no artists
    /// at all.
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

    /// The master playlist is laid out as libgpod lays it out: the name, the
    /// long column mhod (type 100, 0x288 bytes), then the indices as title,
    /// artist, album, genre, composer, without jump tables as Apple writes
    /// them; one string mhod said in the header.
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

    /// The track list goes out in the master playlist's order, so a place in
    /// an index means the same track in either list (a 3G reads them as the
    /// master playlist's, and otherwise lists artists twice and out of order).
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

    /// A signed database reads back as the same library, carries the
    /// signature where the iPod looks for it, and is told apart from an
    /// unsigned one by kessler itself.
    #[test]
    fn a_signed_database_reads_back_and_carries_its_signature() {
        let db = sample_db();
        let guid = crate::ipod::hash58::parse_guid("000A27001CE1B2D3").unwrap();
        let bytes = write_signed(&db, &guid);
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize, MHBD_HEADER_SIGNED);
        assert_eq!(&bytes[0x30..0x32], &[1, 0], "hashing scheme one");
        assert!(bytes[0x58..0x6C].iter().any(|b| *b != 0), "a signature at 0x58");
        assert!(u32::from_le_bytes(bytes[16..20].try_into().unwrap()) >= SIGNED_DB_VERSION);

        let back = parse(&bytes).unwrap();
        let plain = parse(&write(&db)).unwrap();
        assert_eq!(back.tracks.len(), plain.tracks.len());
        for (a, b) in back.tracks.iter().zip(&plain.tracks) {
            assert_eq!((&a.title, &a.artist, &a.path), (&b.title, &b.artist, &b.path));
        }
        assert_eq!(back.playlists.len(), plain.playlists.len());
        assert_eq!(back.db_id, db.db_id);

        let mut resigned = bytes.clone();
        crate::ipod::hash58::sign(&mut resigned, &guid);
        assert_eq!(resigned, bytes);
    }

    /// What the unsigned writer produces, pinned to the byte: the iPods that
    /// sync today must go on getting exactly this, whatever the signed path
    /// changes around it.
    #[test]
    fn the_unsigned_database_stays_byte_for_byte() {
        let mut db = sample_db();
        db.language = *b"de";
        db.library_id = 0x0102_0304_0506_0708;
        db.playlists[0].timestamp = Some(1_500_000_000);
        let bytes = write(&db);
        let digest = sha1_smol::Sha1::from(&bytes).digest().to_string();
        assert_eq!((bytes.len(), digest.as_str()), (4180, "42c96653d29ebb3874d5e148fc3db2353ba170c7"));
    }

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
            language: [0, 0],
            library_id: 0,
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

    /// The library index for `field` read out of a written database.
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

    /// Tracks synced in several goes stand scattered through the database;
    /// the artist index brings each artist's together, and each record's
    /// within it, in its running order — what the 3G's menus fold by.
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

    /// Every track goes on ticked — 0 at mhit+0x78, as iTunes writes it —
    /// and marked as audio; the parser sees the byte.
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
            language: [0, 0],
            library_id: 0,
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
            language: [0, 0],
            library_id: 0,
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
            language: [0, 0],
            library_id: 0,
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

/// Write a made-up iPod into a folder, so the window can be driven against one
/// without a device on the desk.
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
            language: [0, 0],
            library_id: 0,
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
