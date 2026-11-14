struct Builder {
    buf: Vec<u8>,
}

impl Builder {
    fn new(tag: &[u8; 4], header_len: usize) -> Self {
        let mut buf = vec![0u8; header_len];
        buf[0..4].copy_from_slice(tag);
        let mut b = Self { buf };
        b.u32(4, header_len as u32);
        b
    }

    fn u8_at(&mut self, off: usize, v: u8) -> &mut Self {
        self.buf[off] = v;
        self
    }

    fn u16(&mut self, off: usize, v: u16) -> &mut Self {
        self.buf[off..off + 2].copy_from_slice(&v.to_le_bytes());
        self
    }

    fn u32(&mut self, off: usize, v: u32) -> &mut Self {
        self.buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
        self
    }

    fn u64(&mut self, off: usize, v: u64) -> &mut Self {
        self.buf[off..off + 8].copy_from_slice(&v.to_le_bytes());
        self
    }

    fn append(&mut self, bytes: &[u8]) -> &mut Self {
        self.buf.extend_from_slice(bytes);
        self
    }

    fn finish_with_total(mut self) -> Vec<u8> {
        let total = self.buf.len() as u32;
        self.u32(8, total);
        self.buf
    }

    fn finish(self) -> Vec<u8> {
        self.buf
    }
}

const MHOD_HEADER: usize = 0x18;

fn string_mhod(mhod_type: u32, value: &str, utf16: bool) -> Vec<u8> {
    let payload: Vec<u8> = if utf16 {
        value.encode_utf16().flat_map(|u| u.to_le_bytes()).collect()
    } else {
        value.as_bytes().to_vec()
    };

    let mut b = Builder::new(b"mhod", MHOD_HEADER);
    b.u32(12, mhod_type);
    b.append(&(if utf16 { 1u32 } else { 2u32 }).to_le_bytes());
    b.append(&(payload.len() as u32).to_le_bytes());
    b.append(&[0u8; 8]);
    b.append(&payload);
    b.finish_with_total()
}

fn position_mhod(position: u32) -> Vec<u8> {
    let mut b = Builder::new(b"mhod", MHOD_HEADER);
    b.u32(12, 100);
    b.append(&[0u8; 20]);
    b.u32(24, position);
    b.finish_with_total()
}

#[allow(clippy::too_many_arguments)]
struct TestTrack<'a> {
    id: u32,
    title: &'a str,
    title_utf16: bool,
    artist: &'a str,
    album: &'a str,
    path: &'a str,
    year: u32,
    track_number: u32,
    length_ms: u32,
    size_bytes: u32,
    rating: u8,
    date_added_mac: u32,
}

const MHIT_HEADER: usize = 0xf4;

fn track_chunk(t: &TestTrack) -> Vec<u8> {
    let mhods = [
        string_mhod(1, t.title, t.title_utf16),
        string_mhod(2, t.path, true),
        string_mhod(3, t.album, true),
        string_mhod(4, t.artist, true),
    ];

    let mut b = Builder::new(b"mhit", MHIT_HEADER);
    b.u32(12, mhods.len() as u32);
    b.u32(16, t.id);
    b.u32(20, 1);
    b.u32(24, u32::from_be_bytes(*b"MP3 "));
    b.u8_at(31, t.rating);
    b.u32(32, t.date_added_mac);
    b.u32(36, t.size_bytes);
    b.u32(40, t.length_ms);
    b.u32(44, t.track_number);
    b.u32(48, 12);
    b.u32(52, t.year);
    b.u32(56, 320);
    b.u32(60, 44_100 << 16);
    b.u32(80, 7);
    b.u32(88, 0);
    b.u32(92, 1);
    b.u32(96, 1);
    b.u32(104, t.date_added_mac);
    b.u64(112, 0xDEAD_BEEF_0000_0000 | t.id as u64);
    b.u32(156, 2);
    b.u8_at(164, 1);

    for m in &mhods {
        b.append(m);
    }
    b.finish_with_total()
}

const MHYP_HEADER: usize = 0x6c;
const MHIP_HEADER: usize = 0x4c;

fn playlist_item(track_id: u32, position: u32) -> Vec<u8> {
    let pos = position_mhod(position);
    let mut b = Builder::new(b"mhip", MHIP_HEADER);
    b.u32(12, 1);
    b.u32(24, track_id);
    b.append(&pos);
    b.finish_with_total()
}

fn playlist_chunk(name: &str, is_master: bool, entries: &[(u32, u32)]) -> Vec<u8> {
    let title = string_mhod(1, name, true);

    let mut b = Builder::new(b"mhyp", MHYP_HEADER);
    b.u32(12, 1);
    b.u32(16, entries.len() as u32);
    b.u8_at(20, if is_master { 1 } else { 0 });
    b.u32(24, 2_082_844_800 + 1_000);
    b.u64(28, if is_master { 1 } else { 42 });
    b.u16(42, 0);

    b.append(&title);
    for (track_id, position) in entries {
        b.append(&playlist_item(*track_id, *position));
    }
    b.finish_with_total()
}

const MHSD_HEADER: usize = 0x60;
const MHLT_HEADER: usize = 0x5c;
const MHLP_HEADER: usize = 0x5c;
const MHBD_HEADER: usize = 0x68;

fn dataset(kind: u32, body: &[u8]) -> Vec<u8> {
    let mut b = Builder::new(b"mhsd", MHSD_HEADER);
    b.u32(12, kind);
    b.append(body);
    b.finish_with_total()
}

fn database(datasets: &[Vec<u8>]) -> Vec<u8> {
    let mut b = Builder::new(b"mhbd", MHBD_HEADER);
    b.u32(12, 1);
    b.u32(16, 0x19);
    b.u32(20, datasets.len() as u32);
    b.u64(24, 0x0123_4567_89AB_CDEF);

    for d in datasets {
        b.append(d);
    }
    b.finish_with_total()
}

pub fn build_test_db() -> Vec<u8> {
    let tracks = [
        TestTrack {
            id: 101,
            title: "Xtal",
            title_utf16: true,
            artist: "Aphex Twin",
            album: "Selected Ambient Works 85-92",
            path: ":iPod_Control:Music:F00:AAAA.mp3",
            year: 1992,
            track_number: 1,
            length_ms: 291_000,
            size_bytes: 3_000_000,
            rating: 100,
            date_added_mac: 2_082_844_800,
        },
        TestTrack {
            id: 102,
            title: "Ümlaut · 坂本龍一",
            title_utf16: true,
            artist: "Björk",
            album: "Homogénic",
            path: ":iPod_Control:Music:F01:BBBB.mp3",
            year: 1997,
            track_number: 2,
            length_ms: 240_000,
            size_bytes: 4_000_000,
            rating: 80,
            date_added_mac: 2_082_844_800 + 86_400,
        },
        TestTrack {
            id: 103,
            title: "Plain ASCII",
            title_utf16: false,
            artist: "Test Artist",
            album: "Test Album",
            path: ":iPod_Control:Music:F02:CCCC.mp3",
            year: 2001,
            track_number: 3,
            length_ms: 180_000,
            size_bytes: 5_000_000,
            rating: 60,
            date_added_mac: 2_082_844_800 + 172_800,
        },
    ];

    let mut track_list = Builder::new(b"mhlt", MHLT_HEADER);
    track_list.u32(8, tracks.len() as u32);
    for t in &tracks {
        track_list.append(&track_chunk(t));
    }

    let mut playlist_list = Builder::new(b"mhlp", MHLP_HEADER);
    playlist_list.u32(8, 2);
    playlist_list.append(&playlist_chunk(
        "iPod",
        true,
        &[(101, 0), (102, 1), (103, 2)],
    ));
    playlist_list.append(&playlist_chunk("Late Night", false, &[(103, 1), (101, 0)]));

    database(&[
        dataset(1, &track_list.finish()),
        dataset(2, &playlist_list.finish()),
    ])
}

pub fn build_db_with_unknown_mhod() -> Vec<u8> {
    let mhods = [
        string_mhod(1, "Survivor", true),
        string_mhod(9999, "who knows", true),
        string_mhod(4, "After Unknown", true),
    ];

    let mut b = Builder::new(b"mhit", MHIT_HEADER);
    b.u32(12, mhods.len() as u32);
    b.u32(16, 1);
    b.u32(20, 1);
    b.u32(60, 44_100 << 16);
    for m in &mhods {
        b.append(m);
    }
    let track = b.finish_with_total();

    let mut track_list = Builder::new(b"mhlt", MHLT_HEADER);
    track_list.u32(8, 1);
    track_list.append(&track);

    let mut playlist_list = Builder::new(b"mhlp", MHLP_HEADER);
    playlist_list.u32(8, 0);

    database(&[
        dataset(1, &track_list.finish()),
        dataset(2, &playlist_list.finish()),
    ])
}

pub fn find_tag(data: &[u8], tag: &[u8; 4]) -> Option<usize> {
    data.windows(4).position(|w| w == tag)
}
