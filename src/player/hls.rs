use std::io::{Read, Seek, SeekFrom};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::Arc;
use std::time::Duration;

use super::stream::Said;

const MOST_BITS: u64 = 320_000;

const LIVE_BEHIND: usize = 3;

const AHEAD: usize = 3;

const MASTERS_DEEP: u32 = 3;

pub struct Hls {
    pieces: std::sync::Mutex<Receiver<Vec<u8>>>,
    piece: Vec<u8>,
    at: usize,
    read: u64,
    stop: Arc<AtomicBool>,
}

impl Read for Hls {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        while self.at >= self.piece.len() {
            let next = self.pieces.lock().map_err(|_| std::io::Error::other("the pieces' lock broke"))?.recv();
            match next {
                Ok(piece) => {
                    self.piece = piece;
                    self.at = 0;
                }
                Err(_) => return Ok(0),
            }
        }
        let n = buf.len().min(self.piece.len() - self.at);
        buf[..n].copy_from_slice(&self.piece[self.at..self.at + n]);
        self.at += n;
        self.read += n as u64;
        Ok(n)
    }
}

impl Seek for Hls {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        match pos {
            SeekFrom::Current(0) => Ok(self.read),
            _ => Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "a stream cannot seek")),
        }
    }
}

impl Drop for Hls {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

pub fn is_hls(url: &str, kind: &str) -> bool {
    let path = url.split(['?', '#']).next().unwrap_or(url).to_ascii_lowercase();
    path.ends_with(".m3u8") || kind.contains("vnd.apple.mpegurl")
}

fn fetch(url: &str) -> Result<Vec<u8>, String> {
    let out = Command::new("curl")
        .args(["--silent", "--location", "--fail", "--connect-timeout", "10", "--max-time", "30"])
        .args(["--user-agent", concat!("kessler/", env!("CARGO_PKG_VERSION"))])
        .arg(url)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|e| format!("curl: {e}"))?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err(format!("{url} did not answer"))
    }
}

fn resolve(base: &str, link: &str) -> String {
    if link.starts_with("http://") || link.starts_with("https://") {
        return link.to_string();
    }
    let base = base.split(['?', '#']).next().unwrap_or(base);
    if let Some(rest) = link.strip_prefix("//") {
        let scheme = base.split("://").next().unwrap_or("https");
        return format!("{scheme}://{rest}");
    }
    if link.starts_with('/') {
        let origin_end = base.find("://").map(|s| s + 3).and_then(|s| base[s..].find('/').map(|e| s + e)).unwrap_or(base.len());
        return format!("{}{link}", &base[..origin_end]);
    }
    match base.rfind('/') {
        Some(slash) => format!("{}/{link}", &base[..slash]),
        None => link.to_string(),
    }
}

fn attribute<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let mut rest = line.split_once(':')?.1;
    while !rest.is_empty() {
        let (key, after) = rest.split_once('=')?;
        let (value, next) = if let Some(quoted) = after.strip_prefix('"') {
            let end = quoted.find('"')?;
            (&quoted[..end], quoted[end + 1..].trim_start_matches(','))
        } else {
            match after.split_once(',') {
                Some((value, next)) => (value, next),
                None => (after, ""),
            }
        };
        if key.trim().eq_ignore_ascii_case(name) {
            return Some(value);
        }
        rest = next;
    }
    None
}

fn chosen_version(text: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let audio = lines
        .iter()
        .filter(|l| l.starts_with("#EXT-X-MEDIA:") && attribute(l, "TYPE") == Some("AUDIO"))
        .filter_map(|l| attribute(l, "URI"))
        .next();
    if let Some(uri) = audio {
        return Some(uri.to_string());
    }
    let mut versions: Vec<(u64, bool, &str)> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if !line.starts_with("#EXT-X-STREAM-INF:") {
            continue;
        }
        let bits = attribute(line, "BANDWIDTH").and_then(|b| b.parse().ok()).unwrap_or(0);
        let codecs = attribute(line, "CODECS").unwrap_or("");
        let sound_only = attribute(line, "RESOLUTION").is_none() && !codecs.contains("avc") && !codecs.contains("hvc");
        if let Some(uri) = lines[i + 1..].iter().find(|l| !l.is_empty() && !l.starts_with('#')) {
            versions.push((bits, sound_only, uri));
        }
    }
    if versions.is_empty() {
        return None;
    }
    let pool: Vec<_> = if versions.iter().any(|v| v.1) { versions.iter().filter(|v| v.1).collect() } else { versions.iter().collect() };
    let below = pool.iter().filter(|v| v.0 <= MOST_BITS).max_by_key(|v| v.0);
    let best = below.or_else(|| pool.iter().min_by_key(|v| v.0))?;
    Some(best.2.to_string())
}

#[derive(Debug, Default, PartialEq)]
struct Pieces {
    target: f64,
    pieces: Vec<(u64, String)>,
    ended: bool,
    refused: Option<&'static str>,
}

fn pieces_in(text: &str, base: &str) -> Pieces {
    let mut list = Pieces { target: 6.0, ..Default::default() };
    let mut sequence = 0u64;
    let mut next = None;
    for line in text.lines().map(str::trim) {
        if let Some(value) = line.strip_prefix("#EXT-X-TARGETDURATION:") {
            list.target = value.trim().parse().unwrap_or(6.0);
        } else if let Some(value) = line.strip_prefix("#EXT-X-MEDIA-SEQUENCE:") {
            sequence = value.trim().parse().unwrap_or(0);
        } else if line.starts_with("#EXT-X-ENDLIST") {
            list.ended = true;
        } else if line.starts_with("#EXT-X-KEY:") && attribute(line, "METHOD").is_some_and(|m| m != "NONE") {
            list.refused = Some("verschlüsselt");
        } else if line.starts_with("#EXT-X-MAP:") {
            list.refused = Some("fmp4");
        } else if !line.is_empty() && !line.starts_with('#') {
            let number = next.unwrap_or(sequence);
            list.pieces.push((number, resolve(base, line)));
            next = Some(number + 1);
        }
    }
    list
}

#[derive(Debug, Default, PartialEq)]
struct Opened {
    sound: Vec<u8>,
    said: Option<String>,
    mp3: bool,
}

fn open_piece(piece: &[u8]) -> Opened {
    if piece.first() == Some(&0x47) && piece.len() >= 188 && piece.get(188).is_none_or(|&b| b == 0x47) {
        return transport(piece);
    }
    let (said, at) = id3_at(piece);
    let sound = piece[at..].to_vec();
    let mp3 = !is_adts(&sound);
    Opened { sound, said, mp3 }
}

fn is_adts(bytes: &[u8]) -> bool {
    bytes.len() > 2 && bytes[0] == 0xFF && bytes[1] & 0xF6 == 0xF0
}

fn id3_at(bytes: &[u8]) -> (Option<String>, usize) {
    if bytes.len() < 10 || &bytes[..3] != b"ID3" {
        return (None, 0);
    }
    let size = bytes[6..10].iter().fold(0usize, |n, &b| (n << 7) | usize::from(b & 0x7F));
    let end = (10 + size).min(bytes.len());
    (id3_said(&bytes[..end]), end)
}

fn id3_said(tag: &[u8]) -> Option<String> {
    if tag.len() < 10 || &tag[..3] != b"ID3" {
        return None;
    }
    let version = tag[3];
    let mut at = 10;
    let (mut artist, mut title) = (None, None);
    while at + 10 <= tag.len() {
        let id = &tag[at..at + 4];
        if id[0] == 0 {
            break;
        }
        let raw = &tag[at + 4..at + 8];
        let size = if version >= 4 {
            raw.iter().fold(0usize, |n, &b| (n << 7) | usize::from(b & 0x7F))
        } else {
            raw.iter().fold(0usize, |n, &b| (n << 8) | usize::from(b))
        };
        let body = &tag[(at + 10).min(tag.len())..(at + 10 + size).min(tag.len())];
        match id {
            b"TIT2" => title = id3_text(body),
            b"TPE1" => artist = id3_text(body),
            _ => {}
        }
        at += 10 + size;
    }
    let title = title.filter(|t| !t.is_empty())?;
    Some(match artist.filter(|a| !a.is_empty()) {
        Some(artist) => format!("{artist} - {title}"),
        None => title,
    })
}

fn id3_text(body: &[u8]) -> Option<String> {
    let (&encoding, text) = body.split_first()?;
    let text = match encoding {
        1 | 2 => {
            let (big, rest) = match text {
                [0xFE, 0xFF, rest @ ..] => (true, rest),
                [0xFF, 0xFE, rest @ ..] => (false, rest),
                rest => (encoding == 2, rest),
            };
            let units: Vec<u16> = rest
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&p| if big { u16::from_be_bytes(p) } else { u16::from_le_bytes(p) })
                .collect();
            String::from_utf16_lossy(&units)
        }
        3 => String::from_utf8_lossy(text).into_owned(),
        _ => text.iter().map(|&b| char::from(b)).collect(),
    };
    Some(text.trim_matches(|c: char| c == '\0' || c.is_whitespace()).to_string())
}

fn transport(bytes: &[u8]) -> Opened {
    let mut programme_pid = None;
    let (mut sound_pid, mut words_pid, mut mp3) = (None, None, false);
    let mut opened = Opened::default();
    let mut words: Vec<u8> = Vec::new();
    for packet in bytes.as_chunks::<188>().0 {
        if packet[0] != 0x47 {
            continue;
        }
        let pid = (u16::from(packet[1] & 0x1F) << 8) | u16::from(packet[2]);
        let start = packet[1] & 0x40 != 0;
        let adaptation = (packet[3] >> 4) & 0x3;
        let mut at = 4;
        if adaptation & 0x2 != 0 {
            at += 1 + usize::from(packet[4]);
        }
        if adaptation & 0x1 == 0 || at >= 188 {
            continue;
        }
        let payload = &packet[at..];
        if pid == 0 && start {
            let table = &payload[1 + usize::from(payload[0])..];
            let mut entry = 8;
            while entry + 4 <= table.len().saturating_sub(4) {
                let number = u16::from_be_bytes([table[entry], table[entry + 1]]);
                let map = (u16::from(table[entry + 2] & 0x1F) << 8) | u16::from(table[entry + 3]);
                if number != 0 {
                    programme_pid = Some(map);
                    break;
                }
                entry += 4;
            }
        } else if Some(pid) == programme_pid && start && sound_pid.is_none() {
            let table = &payload[1 + usize::from(payload[0])..];
            if table.len() < 12 {
                continue;
            }
            let length = (usize::from(table[1] & 0x0F) << 8) | usize::from(table[2]);
            let end = (3 + length).saturating_sub(4).min(table.len());
            let info = (usize::from(table[10] & 0x0F) << 8) | usize::from(table[11]);
            let mut entry = 12 + info;
            while entry + 5 <= end {
                let kind = table[entry];
                let stream = (u16::from(table[entry + 1] & 0x1F) << 8) | u16::from(table[entry + 2]);
                let more = (usize::from(table[entry + 3] & 0x0F) << 8) | usize::from(table[entry + 4]);
                match kind {
                    0x0F if sound_pid.is_none() => sound_pid = Some(stream),
                    0x03 | 0x04 if sound_pid.is_none() => {
                        sound_pid = Some(stream);
                        mp3 = true;
                    }
                    0x15 => words_pid = Some(stream),
                    _ => {}
                }
                entry += 5 + more;
            }
        } else if Some(pid) == sound_pid {
            opened.sound.extend_from_slice(if start { pes_body(payload) } else { payload });
        } else if Some(pid) == words_pid {
            if start {
                if let Some(said) = id3_said(&words) {
                    opened.said = Some(said);
                }
                words.clear();
                words.extend_from_slice(pes_body(payload));
            } else {
                words.extend_from_slice(payload);
            }
        }
    }
    if let Some(said) = id3_said(&words) {
        opened.said = Some(said);
    }
    opened.mp3 = mp3;
    opened
}

fn pes_body(payload: &[u8]) -> &[u8] {
    if payload.len() < 9 || payload[..3] != [0, 0, 1] {
        return payload;
    }
    let head = 9 + usize::from(payload[8]);
    &payload[head.min(payload.len())..]
}

pub fn open(url: &str, text: &str, said: Said) -> Result<(Hls, bool), String> {
    let (mut url, mut text) = (url.to_string(), text.to_string());
    for _ in 0..MASTERS_DEEP {
        let Some(version) = chosen_version(&text) else { break };
        url = resolve(&url, &version);
        text = String::from_utf8_lossy(&fetch(&url)?).into_owned();
    }
    let list = pieces_in(&text, &url);
    if let Some(why) = list.refused {
        return Err(format!("hls ({why}) wird noch nicht gespielt"));
    }
    let from = if list.ended { 0 } else { list.pieces.len().saturating_sub(LIVE_BEHIND) };
    let Some((first_number, first_url)) = list.pieces.get(from).cloned() else {
        return Err("the hls playlist names no pieces".to_string());
    };
    let first = open_piece(&fetch(&first_url)?);
    if first.sound.is_empty() {
        return Err("the hls stream holds no sound kessler can play".to_string());
    }
    let mp3 = first.mp3;
    if let Some(words) = &first.said {
        *said.lock().unwrap_or_else(|e| e.into_inner()) = Some(words.clone());
    }
    let (tell, pieces) = sync_channel::<Vec<u8>>(AHEAD);
    let _ = tell.send(first.sound);
    let stop = Arc::new(AtomicBool::new(false));
    let held = stop.clone();
    std::thread::Builder::new()
        .name("kessler-hls".into())
        .spawn(move || follow(&url, list, first_number, tell, said, &held))
        .map_err(|e| e.to_string())?;
    Ok((Hls { pieces: std::sync::Mutex::new(pieces), piece: Vec::new(), at: 0, read: 0, stop }, mp3))
}

fn follow(url: &str, mut list: Pieces, mut done: u64, tell: SyncSender<Vec<u8>>, said: Said, stop: &AtomicBool) {
    let mut misses = 0;
    loop {
        let new: Vec<(u64, String)> = list.pieces.iter().filter(|(n, _)| *n > done).cloned().collect();
        for (number, piece_url) in &new {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            let Ok(bytes) = fetch(piece_url) else { continue };
            let opened = open_piece(&bytes);
            if let Some(words) = opened.said {
                *said.lock().unwrap_or_else(|e| e.into_inner()) = Some(words);
            }
            done = *number;
            if tell.send(opened.sound).is_err() {
                return;
            }
        }
        if list.ended || stop.load(Ordering::Relaxed) {
            return;
        }
        std::thread::sleep(Duration::from_secs_f64((list.target / 2.0).clamp(1.0, 10.0)));
        match fetch(url) {
            Ok(text) => {
                list = pieces_in(&String::from_utf8_lossy(&text), url);
                misses = 0;
            }
            Err(_) => {
                misses += 1;
                if misses > 5 {
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_master_playlist_gives_the_best_audio_below_the_limit() {
        let master = "#EXTM3U\n\
            #EXT-X-STREAM-INF:BANDWIDTH=48000,CODECS=\"mp4a.40.5\"\nlow/index.m3u8\n\
            #EXT-X-STREAM-INF:BANDWIDTH=128000,CODECS=\"mp4a.40.2\"\nmid/index.m3u8\n\
            #EXT-X-STREAM-INF:BANDWIDTH=2000000,RESOLUTION=1280x720,CODECS=\"avc1.4d401f,mp4a.40.2\"\nvideo/index.m3u8\n\
            #EXT-X-STREAM-INF:BANDWIDTH=400000,CODECS=\"mp4a.40.2\"\nhigh/index.m3u8\n";
        assert_eq!(chosen_version(master).as_deref(), Some("mid/index.m3u8"));
        let rendition = "#EXTM3U\n#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"a\",NAME=\"en\",URI=\"audio/en.m3u8\"\n\
            #EXT-X-STREAM-INF:BANDWIDTH=900000,AUDIO=\"a\"\nv.m3u8\n";
        assert_eq!(chosen_version(rendition).as_deref(), Some("audio/en.m3u8"));
        assert_eq!(chosen_version("#EXTM3U\n#EXTINF:6,\na.ts\n"), None);
    }

    #[test]
    fn a_playlist_of_pieces_is_read_with_its_numbers() {
        let text = "#EXTM3U\n#EXT-X-TARGETDURATION:10\n#EXT-X-MEDIA-SEQUENCE:41\n\
            #EXTINF:10,\nseg41.ts\n#EXTINF:10,\n/abs/seg42.ts\n#EXTINF:10,\nhttps://cdn.example/seg43.ts\n";
        let list = pieces_in(text, "https://radio.example/live/index.m3u8?token=1");
        assert_eq!(list.target, 10.0);
        assert!(!list.ended && list.refused.is_none());
        assert_eq!(
            list.pieces,
            vec![
                (41, "https://radio.example/live/seg41.ts".to_string()),
                (42, "https://radio.example/abs/seg42.ts".to_string()),
                (43, "https://cdn.example/seg43.ts".to_string()),
            ]
        );
        assert_eq!(pieces_in("#EXT-X-KEY:METHOD=AES-128,URI=\"k\"\n#EXTINF:6,\na.ts\n", "https://x/a.m3u8").refused, Some("verschlüsselt"));
        assert_eq!(pieces_in("#EXT-X-MAP:URI=\"init.mp4\"\n#EXTINF:6,\na.m4s\n", "https://x/a.m3u8").refused, Some("fmp4"));
        assert!(pieces_in("#EXTINF:6,\na.aac\n#EXT-X-ENDLIST\n", "https://x/a.m3u8").ended);
    }

    fn id3(artist: &str, title: &str) -> Vec<u8> {
        let frame = |id: &[u8], text: &str| {
            let mut body = vec![3u8];
            body.extend_from_slice(text.as_bytes());
            let size = body.len() as u32;
            let mut out = id.to_vec();
            out.extend((0..4).rev().map(|i| ((size >> (7 * i)) & 0x7F) as u8));
            out.extend([0, 0]);
            out.extend(body);
            out
        };
        let mut frames = frame(b"TPE1", artist);
        frames.extend(frame(b"TIT2", title));
        let size = frames.len() as u32;
        let mut tag = b"ID3\x04\x00\x00".to_vec();
        tag.extend((0..4).rev().map(|i| ((size >> (7 * i)) & 0x7F) as u8));
        tag.extend(frames);
        tag
    }

    #[test]
    fn a_piece_of_aac_loses_its_tag_and_says_what_plays() {
        let mut piece = id3("Alice Coltrane", "Turiya and Ramakrishna");
        let adts = [0xFF, 0xF1, 0x50, 0x80, 0x01, 0x7F, 0xFC, 0x21];
        piece.extend(adts);
        let opened = open_piece(&piece);
        assert_eq!(opened.sound, adts);
        assert!(!opened.mp3);
        assert_eq!(opened.said.as_deref(), Some("Alice Coltrane - Turiya and Ramakrishna"));
    }

    fn transport_stream(sound: &[u8], tag: &[u8]) -> Vec<u8> {
        let packet = |pid: u16, start: bool, payload: &[u8]| {
            let mut p = vec![0x47, (if start { 0x40 } else { 0 }) | (pid >> 8) as u8, pid as u8, 0x10];
            p.extend_from_slice(payload);
            p.resize(188, 0xFF);
            p
        };
        let pat = [0, 0x00, 0xB0, 0x0D, 0, 1, 0xC1, 0, 0, 0, 1, 0xF0, 0x00, 0, 0, 0, 0];
        let pmt = [
            0, 0x02, 0xB0, 0x17, 0, 1, 0xC1, 0, 0, 0xE1, 0x01, 0xF0, 0x00,
            0x0F, 0xE1, 0x01, 0xF0, 0x00,
            0x15, 0xE1, 0x02, 0xF0, 0x00,
            0, 0, 0, 0,
        ];
        let pes = |body: &[u8]| {
            let mut p = vec![0, 0, 1, 0xC0, 0, 0, 0x80, 0x00, 0x00];
            p.extend_from_slice(body);
            p
        };
        let mut out = packet(0, true, &pat);
        out.extend(packet(0x1000, true, &pmt));
        out.extend(packet(0x101, true, &pes(sound)));
        out.extend(packet(0x102, true, &pes(tag)));
        out
    }

    #[test]
    fn a_transport_stream_gives_its_sound_and_words() {
        let adts = [0xFF, 0xF1, 0x50, 0x80, 0x01, 0x7F, 0xFC, 0x21];
        let tag = id3("Hiroshi Yoshimura", "Water Copy");
        let ts = transport_stream(&adts, &tag);
        let opened = open_piece(&ts);
        assert!(opened.sound.starts_with(&adts), "{:02X?}", &opened.sound[..12.min(opened.sound.len())]);
        assert!(!opened.mp3);
        assert_eq!(opened.said.as_deref(), Some("Hiroshi Yoshimura - Water Copy"));
    }

    #[test]
    fn links_are_read_against_the_playlist() {
        assert_eq!(resolve("https://a.example/x/y.m3u8?t=1", "z.ts"), "https://a.example/x/z.ts");
        assert_eq!(resolve("https://a.example/x/y.m3u8", "/z.ts"), "https://a.example/z.ts");
        assert_eq!(resolve("https://a.example/x/y.m3u8", "//b.example/z.ts"), "https://b.example/z.ts");
        assert!(is_hls("https://a.example/live.m3u8?x", ""));
        assert!(is_hls("https://a.example/live", "application/vnd.apple.mpegurl"));
        assert!(!is_hls("https://a.example/live.mp3", "audio/mpeg"));
    }
}
