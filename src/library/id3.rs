#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Salvaged {
    pub title: String,
    pub artist: String,
    pub album_artist: String,
    pub album: String,
    pub genre: String,
    pub year: Option<i32>,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    pub has_artwork: bool,
}

impl Salvaged {
    pub fn is_empty(&self) -> bool {
        self == &Salvaged::default()
    }
}

const HEADER: usize = 10;

pub const READ_AT_MOST: usize = 4 * 1024 * 1024;

pub fn salvage(bytes: &[u8]) -> Option<Salvaged> {
    if bytes.len() < HEADER || &bytes[0..3] != b"ID3" {
        return None;
    }

    let major = bytes[3];
    if !(2..=4).contains(&major) {
        return None;
    }
    let flags = bytes[5];
    let size = synchsafe(&bytes[6..10])? as usize;

    let end = (HEADER + size).min(bytes.len());
    let body = &bytes[HEADER..end];

    let owned;
    let body = if flags & 0x80 != 0 {
        owned = resynchronise(body);
        &owned[..]
    } else {
        body
    };

    let body = skip_extended_header(body, major, flags);

    let mut out = Salvaged::default();

    for (id, data) in Frames::new(body, major) {
        let text = || text_of(data);
        match &id[..] {
            b"TIT2" | b"TT2" => set(&mut out.title, text()),
            b"TPE1" | b"TP1" => set(&mut out.artist, text()),
            b"TPE2" | b"TP2" => set(&mut out.album_artist, text()),
            b"TALB" | b"TAL" => set(&mut out.album, text()),
            b"TCON" | b"TCO" => set(&mut out.genre, text().map(|g| plain_genre(&g))),
            b"TRCK" | b"TRK" => out.track_number = out.track_number.or(text().and_then(numerator)),
            b"TPOS" | b"TPA" => out.disc_number = out.disc_number.or(text().and_then(numerator)),
            b"TYER" | b"TYE" | b"TDRC" | b"TDRL" | b"TDOR" | b"TORY" => {
                out.year = out.year.or(text().and_then(|t| leading_year(&t)))
            }
            b"APIC" | b"PIC" => out.has_artwork = out.has_artwork || data.len() > 32,
            _ => {}
        }
    }

    Some(out)
}

fn set(field: &mut String, value: Option<String>) {
    if field.is_empty() {
        if let Some(v) = value {
            if !v.is_empty() {
                *field = v;
            }
        }
    }
}

pub fn mend(file: &[u8], readable: impl Fn(&[u8]) -> bool) -> Option<Vec<u8>> {
    if file.len() < HEADER || &file[0..3] != b"ID3" {
        return None;
    }
    let major = file[3];
    if !(2..=4).contains(&major) {
        return None;
    }
    let flags = file[5];
    let size = synchsafe(&file[6..10])? as usize;
    let footer = if major == 4 && flags & 0x10 != 0 { HEADER } else { 0 };
    let audio_at = HEADER + size + footer;
    let audio = file.get(audio_at..)?;
    let body = &file[HEADER..HEADER + size];

    let owned;
    let body = if flags & 0x80 != 0 {
        owned = resynchronise(body);
        &owned[..]
    } else {
        body
    };
    let body = skip_extended_header(body, major, flags);

    let mut frames = Frames::new(body, major);
    for _ in frames.by_ref() {}
    let tag = |frames: &[&[u8]]| {
        let body: Vec<u8> = frames.concat();
        let size = body.len() as u32;
        let mut out = vec![b'I', b'D', b'3', major, 0, 0];
        out.extend_from_slice(&[
            ((size >> 21) & 0x7f) as u8,
            ((size >> 14) & 0x7f) as u8,
            ((size >> 7) & 0x7f) as u8,
            (size & 0x7f) as u8,
        ]);
        out.extend_from_slice(&body);
        out
    };
    let sample = &audio[..audio.len().min(64 * 1024)];
    let all: Vec<&[u8]> = frames.spans.iter().map(|&(a, b)| &body[a..b]).collect();
    let kept: Vec<&[u8]> = all
        .iter()
        .copied()
        .filter(|frame| readable(&[tag(&[frame]), sample.to_vec()].concat()))
        .collect();
    if kept.len() == all.len() {
        return None;
    }
    let mended = [tag(&kept), audio.to_vec()].concat();
    readable(&mended).then_some(mended)
}

fn synchsafe(b: &[u8]) -> Option<u32> {
    if b.len() < 4 || b[..4].iter().any(|byte| byte & 0x80 != 0) {
        return None;
    }
    Some(((b[0] as u32) << 21) | ((b[1] as u32) << 14) | ((b[2] as u32) << 7) | b[3] as u32)
}

fn plain_u32(b: &[u8]) -> u32 {
    ((b[0] as u32) << 24) | ((b[1] as u32) << 16) | ((b[2] as u32) << 8) | b[3] as u32
}

fn resynchronise(body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len());
    let mut i = 0;
    while i < body.len() {
        out.push(body[i]);
        if body[i] == 0xFF && i + 1 < body.len() && body[i + 1] == 0x00 {
            i += 2;
        } else {
            i += 1;
        }
    }
    out
}

fn skip_extended_header(body: &[u8], major: u8, flags: u8) -> &[u8] {
    if flags & 0x40 == 0 || body.len() < 4 {
        return body;
    }
    let skip = match major {
        3 => 4 + plain_u32(&body[0..4]) as usize,
        4 => synchsafe(&body[0..4]).unwrap_or(0) as usize,
        _ => 0,
    };
    body.get(skip..).unwrap_or(&[])
}

struct Frames<'a> {
    body: &'a [u8],
    at: usize,
    major: u8,
    spans: Vec<(usize, usize)>,
}

impl<'a> Frames<'a> {
    fn new(body: &'a [u8], major: u8) -> Self {
        Frames { body, at: 0, major, spans: Vec::new() }
    }

    fn id_len(&self) -> usize {
        if self.major == 2 {
            3
        } else {
            4
        }
    }

    fn header_len(&self) -> usize {
        if self.major == 2 {
            6
        } else {
            10
        }
    }
}

impl<'a> Iterator for Frames<'a> {
    type Item = (Vec<u8>, &'a [u8]);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let head = self.header_len();
            if self.at + head > self.body.len() {
                return None;
            }
            let id = &self.body[self.at..self.at + self.id_len()];

            if !id
                .iter()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
            {
                return None;
            }

            let size_at = self.at + self.id_len();
            let (size, flags) = match self.major {
                2 => (
                    ((self.body[size_at] as usize) << 16)
                        | ((self.body[size_at + 1] as usize) << 8)
                        | self.body[size_at + 2] as usize,
                    0u8,
                ),
                3 => (
                    plain_u32(&self.body[size_at..size_at + 4]) as usize,
                    self.body[size_at + 5],
                ),
                _ => {
                    let raw = &self.body[size_at..size_at + 4];
                    let safe = synchsafe(raw).map(|s| s as usize);
                    let plain = plain_u32(raw) as usize;
                    let size = match safe {
                        Some(s) if s != plain && !self.lands_on_a_frame(s) && self.lands_on_a_frame(plain) => plain,
                        Some(s) => s,
                        None => plain,
                    };
                    (size, self.body[size_at + 5])
                }
            };

            let start = self.at + head;
            let stop = start.saturating_add(size);
            if stop > self.body.len() {
                return None;
            }
            if size == 0 {
                self.at = start;
                continue;
            }
            self.spans.push((self.at, stop));
            self.at = stop;

            let compressed = match self.major {
                3 => flags & 0xC0 != 0,
                4 => flags & 0x0C != 0,
                _ => false,
            };
            if compressed {
                continue;
            }
            let mut data = &self.body[start..stop];
            if self.major == 4 && flags & 0x01 != 0 && data.len() >= 4 {
                data = &data[4..];
            }

            return Some((id.to_vec(), data));
        }
    }
}

impl Frames<'_> {
    fn lands_on_a_frame(&self, size: usize) -> bool {
        let next = match self.at.checked_add(self.header_len() + size) {
            Some(n) => n,
            None => return false,
        };
        if next == self.body.len() {
            return true;
        }
        match self.body.get(next..next + self.id_len()) {
            None => false,
            Some(id) if id.iter().all(|b| *b == 0) => true,
            Some(id) => id
                .iter()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()),
        }
    }
}

fn text_of(data: &[u8]) -> Option<String> {
    let (encoding, rest) = data.split_first()?;
    let decoded = match encoding {
        0 => rest.iter().map(|b| *b as char).collect::<String>(),
        1 => utf16(rest, None),
        2 => utf16(rest, Some(true)),
        3 => String::from_utf8_lossy(rest).into_owned(),
        _ => std::iter::once(*encoding)
            .chain(rest.iter().copied())
            .map(|b| b as char)
            .collect::<String>(),
    };

    let first = decoded.split('\0').find(|s| !s.trim().is_empty())?;
    let first = first.trim().to_string();
    if first.is_empty() {
        None
    } else {
        Some(first)
    }
}

fn utf16(bytes: &[u8], force_big_endian: Option<bool>) -> String {
    let (bytes, big_endian) = match (force_big_endian, bytes) {
        (Some(be), _) => (bytes, be),
        (None, [0xFF, 0xFE, rest @ ..]) => (rest, false),
        (None, [0xFE, 0xFF, rest @ ..]) => (rest, true),
        (None, _) => (bytes, false),
    };

    let pairs = bytes.len() / 2;
    let units = (0..pairs).map(|i| {
        let (a, b) = (bytes[i * 2], bytes[i * 2 + 1]);
        if big_endian {
            u16::from_be_bytes([a, b])
        } else {
            u16::from_le_bytes([a, b])
        }
    });

    char::decode_utf16(units).filter_map(|c| c.ok()).collect()
}

fn numerator(text: String) -> Option<u32> {
    let head = text.split('/').next().unwrap_or("").trim();
    head.parse().ok()
}

fn leading_year(text: &str) -> Option<i32> {
    let digits: String = text.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.len() < 4 {
        return None;
    }
    digits[..4].parse().ok()
}

fn plain_genre(text: &str) -> String {
    let rest = match text.strip_prefix('(') {
        None => return text.to_string(),
        Some(after) => match after.split_once(')') {
            Some((number, rest)) if number.chars().all(|c| c.is_ascii_digit()) => rest,
            _ => return text.to_string(),
        },
    };
    rest.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synchsafe_bytes(n: usize) -> Vec<u8> {
        vec![
            ((n >> 21) & 0x7f) as u8,
            ((n >> 14) & 0x7f) as u8,
            ((n >> 7) & 0x7f) as u8,
            (n & 0x7f) as u8,
        ]
    }

    fn frame(major: u8, id: &[u8], data: &[u8]) -> Vec<u8> {
        let mut out = id.to_vec();
        match major {
            2 => out.extend_from_slice(&[
                ((data.len() >> 16) & 0xff) as u8,
                ((data.len() >> 8) & 0xff) as u8,
                (data.len() & 0xff) as u8,
            ]),
            3 => {
                out.extend_from_slice(&(data.len() as u32).to_be_bytes());
                out.extend_from_slice(&[0, 0]);
            }
            _ => {
                out.extend_from_slice(&synchsafe_bytes(data.len()));
                out.extend_from_slice(&[0, 0]);
            }
        }
        out.extend_from_slice(data);
        out
    }

    fn tag(major: u8, frames: &[Vec<u8>]) -> Vec<u8> {
        let body: Vec<u8> = frames.concat();
        let mut out = b"ID3".to_vec();
        out.extend_from_slice(&[major, 0, 0]);
        out.extend_from_slice(&synchsafe_bytes(body.len()));
        out.extend_from_slice(&body);
        out
    }

    fn latin1(text: &str) -> Vec<u8> {
        let mut out = vec![0u8];
        out.extend(text.chars().map(|c| c as u8));
        out
    }

    fn utf16le(text: &str) -> Vec<u8> {
        let mut out = vec![1u8, 0xFF, 0xFE];
        for unit in text.encode_utf16() {
            out.extend_from_slice(&unit.to_le_bytes());
        }
        out
    }

    #[test]
    fn reads_an_ordinary_v23_tag() {
        let bytes = tag(
            3,
            &[
                frame(3, b"TIT2", &latin1("Intro")),
                frame(3, b"TPE1", &latin1("Panopticon")),
                frame(3, b"TALB", &latin1("Panopticon")),
                frame(3, b"TRCK", &latin1("2/11")),
                frame(3, b"TPOS", &latin1("1/2")),
                frame(3, b"TYER", &latin1("2008")),
                frame(3, b"TCON", &latin1("(9)Black Metal")),
            ],
        );

        let out = salvage(&bytes).expect("a tag is there");
        assert_eq!(out.title, "Intro");
        assert_eq!(out.artist, "Panopticon");
        assert_eq!(out.album, "Panopticon");
        assert_eq!(out.track_number, Some(2));
        assert_eq!(out.disc_number, Some(1));
        assert_eq!(out.year, Some(2008));
        assert_eq!(out.genre, "Black Metal");
    }

    #[test]
    fn a_utf16_string_cut_in_half_keeps_the_rest() {
        let mut broken = utf16le("Sonic Prayer");
        broken.pop();

        let bytes = tag(
            3,
            &[
                frame(3, b"TIT2", &broken),
                frame(3, b"TPE1", &latin1("Earthless")),
            ],
        );

        let out = salvage(&bytes).unwrap();
        assert_eq!(out.title, "Sonic Praye");
        assert_eq!(out.artist, "Earthless");
    }

    #[test]
    fn an_empty_frame_does_not_end_the_tag() {
        let bytes = tag(
            3,
            &[
                frame(3, b"TIT2", &[]),
                frame(3, b"TPE1", &latin1("Graupel")),
                frame(3, b"TALB", &latin1("Als der Nebel")),
            ],
        );

        let out = salvage(&bytes).unwrap();
        assert_eq!(out.title, "");
        assert_eq!(out.artist, "Graupel");
        assert_eq!(out.album, "Als der Nebel");
    }

    #[test]
    fn a_half_written_timestamp_still_gives_its_year() {
        let bytes = tag(
            4,
            &[
                frame(4, b"TDRC", &latin1("2008-")),
                frame(4, b"TIT2", &latin1("Whoopee")),
            ],
        );

        let out = salvage(&bytes).unwrap();
        assert_eq!(out.year, Some(2008));
        assert_eq!(out.title, "Whoopee");
    }

    #[test]
    fn a_v22_tag_uses_three_letter_names() {
        let bytes = tag(
            2,
            &[
                frame(2, b"TT2", &latin1("Foil")),
                frame(2, b"TP1", &latin1("Autechre")),
                frame(2, b"TAL", &latin1("Amber")),
                frame(2, b"TRK", &latin1("1")),
            ],
        );

        let out = salvage(&bytes).unwrap();
        assert_eq!(out.title, "Foil");
        assert_eq!(out.artist, "Autechre");
        assert_eq!(out.album, "Amber");
        assert_eq!(out.track_number, Some(1));
    }

    #[test]
    fn utf16_survives_a_missing_byte_order_mark() {
        let mut data = vec![1u8];
        for unit in "Hypnotized".encode_utf16() {
            data.extend_from_slice(&unit.to_le_bytes());
        }
        let bytes = tag(3, &[frame(3, b"TIT2", &data)]);
        assert_eq!(salvage(&bytes).unwrap().title, "Hypnotized");
    }

    #[test]
    fn a_picture_is_noticed_without_being_decoded() {
        let mut apic = vec![0u8];
        apic.extend_from_slice(b"image/jpeg\0\x03Cover\0");
        apic.extend_from_slice(&[0xFF; 64]);

        let bytes = tag(3, &[frame(3, b"APIC", &apic)]);
        assert!(salvage(&bytes).unwrap().has_artwork);
    }

    #[test]
    fn a_v24_size_written_the_old_way_is_still_read() {
        let payload = latin1(&"x".repeat(127));
        let mut bad = b"TIT2".to_vec();
        bad.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        bad.extend_from_slice(&[0, 0]);
        bad.extend_from_slice(&payload);

        let bytes = tag(4, &[bad, frame(4, b"TPE1", &latin1("Boredoms"))]);
        let out = salvage(&bytes).unwrap();
        assert_eq!(out.artist, "Boredoms");
    }

    #[test]
    fn unsynchronised_bodies_are_put_back_together() {
        let mut body = b"TIT2".to_vec();
        body.extend_from_slice(&4u32.to_be_bytes());
        body.extend_from_slice(&[0, 0]);
        body.extend_from_slice(&[0u8, b'A', 0xFF, 0x00, b'B']);

        let mut out = b"ID3".to_vec();
        out.extend_from_slice(&[3, 0, 0x80]);
        out.extend_from_slice(&synchsafe_bytes(body.len()));
        out.extend_from_slice(&body);

        assert_eq!(salvage(&out).unwrap().title, "AÿB");
    }

    #[test]
    fn a_file_without_a_tag_is_not_an_error() {
        assert_eq!(salvage(&[0xFF, 0xFB, 0x10, 0xC4]), None);
        assert_eq!(salvage(b"ID"), None);
    }

    #[test]
    fn a_bare_genre_number_says_nothing() {
        assert_eq!(plain_genre("(17)"), "");
        assert_eq!(plain_genre("(17)Rock"), "Rock");
        assert_eq!(plain_genre("Rock"), "Rock");
        assert_eq!(plain_genre("(not a number)"), "(not a number)");
    }

    #[test]
    fn a_truncated_tag_gives_up_what_it_has() {
        let full = tag(
            3,
            &[
                frame(3, b"TPE1", &latin1("Ajilvsga")),
                frame(3, b"TALB", &latin1("The Harvest")),
            ],
        );
        let cut = &full[..full.len() - 6];

        let out = salvage(cut).unwrap();
        assert_eq!(out.artist, "Ajilvsga");
        assert_eq!(out.album, "");
    }
}
