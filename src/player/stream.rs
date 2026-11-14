use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};

pub type Said = Arc<Mutex<Option<String>>>;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Introduction {
    pub name: String,
    pub description: String,
    pub genre: String,
    pub url: String,
    pub bitrate: String,
    pub titles: bool,
    pub hls: bool,
}

pub type Introduced = Arc<Mutex<Option<Introduction>>>;

use rodio::decoder::DecoderBuilder;
use rodio::Decoder;

pub enum Stream {
    Icy(Icy),
    Hls(super::hls::Hls),
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Icy(icy) => icy.read(buf),
            Self::Hls(hls) => hls.read(buf),
        }
    }
}

impl Seek for Stream {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        match self {
            Self::Icy(icy) => icy.seek(pos),
            Self::Hls(hls) => hls.seek(pos),
        }
    }
}

pub struct Icy {
    child: Child,
    out: BufReader<ChildStdout>,
    read: u64,
    every: Option<usize>,
    left: usize,
    said: Said,
}

impl Read for Icy {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if let Some(every) = self.every {
            if self.left == 0 {
                if !self.take_words()? {
                    return Ok(0);
                }
                self.left = every;
            }
        }
        let wanted = if self.every.is_some() { buf.len().min(self.left) } else { buf.len() };
        let n = self.out.read(&mut buf[..wanted])?;
        self.read += n as u64;
        self.left = self.left.saturating_sub(n);
        Ok(n)
    }
}

impl Icy {
    fn take_words(&mut self) -> std::io::Result<bool> {
        let mut length = [0u8; 1];
        if !read_all(&mut self.out, &mut length)? {
            return Ok(false);
        }
        if length[0] == 0 {
            return Ok(true);
        }
        let mut words = vec![0u8; usize::from(length[0]) * 16];
        if !read_all(&mut self.out, &mut words)? {
            return Ok(false);
        }
        if let Some(title) = stream_title(&words) {
            *self.said.lock().unwrap_or_else(|e| e.into_inner()) = Some(title);
        }
        Ok(true)
    }
}

fn read_all(from: &mut impl Read, buf: &mut [u8]) -> std::io::Result<bool> {
    match from.read_exact(buf) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(false),
        Err(e) => Err(e),
    }
}

fn stream_title(words: &[u8]) -> Option<String> {
    let end = words.iter().position(|&b| b == 0).unwrap_or(words.len());
    let words = match std::str::from_utf8(&words[..end]) {
        Ok(text) => text.to_string(),
        Err(_) => words[..end].iter().map(|&b| char::from(b)).collect(),
    };
    let from = words.find("StreamTitle='")? + "StreamTitle='".len();
    let rest = &words[from..];
    let to = rest.find("';").unwrap_or(rest.len());
    let title = rest[..to].trim();
    (!title.is_empty()).then(|| title.to_string())
}

struct Head {
    every: Option<usize>,
    kind: String,
    intro: Introduction,
}

fn head_line(out: &mut BufReader<ChildStdout>) -> Result<Option<String>, String> {
    let mut bytes = Vec::new();
    if out.read_until(b'\n', &mut bytes).map_err(|e| e.to_string())? == 0 {
        return Ok(None);
    }
    Ok(Some(match String::from_utf8(bytes) {
        Ok(line) => line,
        Err(e) => e.into_bytes().into_iter().map(char::from).collect(),
    }))
}

fn plainly(value: &str) -> String {
    let value: String = value.chars().filter(|c| !c.is_control()).collect();
    let value = value.trim();
    const PLACEHOLDERS: [&str; 12] = [
        "unspecified description",
        "unspecified name",
        "unspecified",
        "no description",
        "this is my server description",
        "this is my server name",
        "my station name",
        "default genre",
        "n/a",
        "none",
        "-",
        "(none)",
    ];
    if PLACEHOLDERS.iter().any(|p| value.eq_ignore_ascii_case(p)) {
        return String::new();
    }
    value.to_string()
}

fn introduce(intro: &mut Introduction, name: &str, value: &str) {
    let name = name.trim().to_ascii_lowercase();
    match name.as_str() {
        "icy-name" => intro.name = plainly(value),
        "icy-description" => intro.description = plainly(value),
        "icy-genre" => intro.genre = plainly(value),
        "icy-url" => {
            let url = plainly(value);
            intro.url = if url.starts_with("http://") || url.starts_with("https://") {
                url
            } else if url.contains('.') && !url.contains(' ') {
                format!("https://{url}")
            } else {
                String::new()
            };
        }
        "icy-br" => {
            intro.bitrate = value.split(',').next().unwrap_or("").trim().chars().filter(char::is_ascii_digit).collect();
        }
        _ => {}
    }
}

fn answer_head(out: &mut BufReader<ChildStdout>) -> Result<Head, String> {
    loop {
        let Some(status) = head_line(out)? else {
            return Err("the station did not answer".to_string());
        };
        let code: u16 = status.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
        let mut every = None;
        let mut kind = String::new();
        let mut intro = Introduction::default();
        loop {
            let Some(line) = head_line(out)? else {
                return Err("the station did not answer".to_string());
            };
            let line = line.trim_end();
            if line.is_empty() {
                break;
            }
            if let Some((name, value)) = line.split_once(':') {
                if name.trim().eq_ignore_ascii_case("icy-metaint") {
                    every = value.trim().parse::<usize>().ok().filter(|&n| n > 0);
                }
                if name.trim().eq_ignore_ascii_case("content-type") {
                    kind = value.trim().to_ascii_lowercase();
                }
                introduce(&mut intro, name, value);
            }
        }
        if (100..200).contains(&code) || (300..400).contains(&code) {
            continue;
        }
        return Ok(Head { every, kind, intro });
    }
}

fn is_playlist(url: &str, kind: &str) -> bool {
    let path = url.split(['?', '#']).next().unwrap_or(url).to_ascii_lowercase();
    if super::hls::is_hls(url, kind) {
        return false;
    }
    path.ends_with(".pls")
        || path.ends_with(".m3u")
        || kind.contains("scpls")
        || kind.contains("pls+xml")
        || kind.contains("mpegurl")
}

fn first_in_playlist(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .map(|line| match line.split_once('=') {
            Some((key, value)) if key.trim().to_ascii_lowercase().starts_with("file") => value.trim(),
            _ => line,
        })
        .find(|line| line.starts_with("http://") || line.starts_with("https://"))
        .map(str::to_string)
}

const PLAYLISTS_DEEP: u32 = 3;

impl Seek for Icy {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        match pos {
            SeekFrom::Current(0) => Ok(self.read),
            _ => Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "a stream cannot seek")),
        }
    }
}

impl Drop for Icy {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn ask(url: &str) -> Result<(Child, BufReader<ChildStdout>, Head), String> {
    let mut child = Command::new("curl")
        .args(["--silent", "--location", "--fail", "--no-buffer", "--connect-timeout", "10"])
        .args(["--speed-limit", "1", "--speed-time", "15"])
        .args(["--http1.1"])
        .args(["--include", "--suppress-connect-headers", "--header", "Icy-MetaData: 1"])
        .args(["--user-agent", concat!("kessler/", env!("CARGO_PKG_VERSION"))])
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("curl: {e}"))?;
    let mut out = BufReader::new(child.stdout.take().ok_or("curl gave no output")?);
    match answer_head(&mut out) {
        Ok(head) => Ok((child, out, head)),
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(e)
        }
    }
}

pub fn open(url: &str, said: Said, introduced: Introduced) -> Result<Decoder<BufReader<Stream>>, String> {
    let mut url = url.to_string();
    let mut deep = 0;
    let (child, out, every) = loop {
        let (mut child, mut out, head) = ask(&url)?;
        if super::hls::is_hls(&url, &head.kind) {
            let mut text = String::new();
            let _ = (&mut out).take(1024 * 1024).read_to_string(&mut text);
            let _ = child.kill();
            let _ = child.wait();
            let (hls, mp3) = super::hls::open(&url, &text, said)?;
            *introduced.lock().unwrap_or_else(|e| e.into_inner()) = Some(Introduction { titles: false, hls: true, ..head.intro });
            return decoder(Stream::Hls(hls), Some(if mp3 { "mp3" } else { "aac" }));
        }
        if !is_playlist(&url, &head.kind) {
            *introduced.lock().unwrap_or_else(|e| e.into_inner()) = Some(Introduction { titles: head.every.is_some(), ..head.intro });
            break (child, out, head.every);
        }
        let mut text = String::new();
        let _ = (&mut out).take(64 * 1024).read_to_string(&mut text);
        let _ = child.kill();
        let _ = child.wait();
        deep += 1;
        match first_in_playlist(&text) {
            Some(named) if deep <= PLAYLISTS_DEEP => url = named,
            _ => return Err("the playlist names no stream".to_string()),
        }
    };
    let stream = Stream::Icy(Icy { child, out, read: 0, every, left: every.unwrap_or(0), said });
    decoder(stream, None)
}

fn decoder(stream: Stream, hint: Option<&'static str>) -> Result<Decoder<BufReader<Stream>>, String> {
    let opened = super::audio::quietly(move || {
        let builder = DecoderBuilder::new().with_data(BufReader::new(stream)).with_seekable(false);
        let builder = match hint {
            Some(hint) => builder.with_hint(hint),
            None => builder,
        };
        builder.build().map_err(|e| e.to_string())
    });
    opened.unwrap_or_else(|_| Err("the decoder gave up on this stream".to_string()))
}

#[cfg(test)]
mod tests {
    use rodio::Source;

    #[test]
    #[ignore]
    fn a_stream_gives_long_sound() {
        let Ok(url) = std::env::var("KESSLER_STREAM_LONG") else { return };
        let decoder = super::open(&url, super::Said::default(), super::Introduced::default()).expect("the stream should open");
        let per_second = decoder.sample_rate().get() as usize * usize::from(decoder.channels().get());
        let started = std::time::Instant::now();
        assert_eq!(decoder.take(per_second * 20).count(), per_second * 20);
        eprintln!("twenty seconds in {:?}", started.elapsed());
    }

    #[test]
    #[ignore]
    fn a_stream_gives_sound() {
        let Ok(url) = std::env::var("KESSLER_STREAM_TEST") else { return };
        let said = super::Said::default();
        let decoder = super::open(&url, said.clone(), super::Introduced::default()).expect("the stream should open");
        let per_second = decoder.sample_rate().get() as usize * usize::from(decoder.channels().get());
        assert_eq!(decoder.take(per_second).count(), per_second);
        eprintln!("said: {:?}", said.lock().unwrap_or_else(|e| e.into_inner()));
    }

    #[test]
    fn a_station_introduces_itself_without_placeholders() {
        let mut intro = super::Introduction::default();
        super::introduce(&mut intro, "icy-name", " NTS Radio 1 ");
        super::introduce(&mut intro, "ICY-Description", "Unspecified description");
        super::introduce(&mut intro, "icy-genre", "Eclectic");
        super::introduce(&mut intro, "icy-url", "www.nts.live");
        super::introduce(&mut intro, "icy-br", "128,128");
        super::introduce(&mut intro, "server", "Icecast 2.4");
        assert_eq!(
            intro,
            super::Introduction {
                name: "NTS Radio 1".into(),
                description: String::new(),
                genre: "Eclectic".into(),
                url: "https://www.nts.live".into(),
                bitrate: "128".into(),
                titles: false,
                hls: false,
            }
        );
    }

    #[test]
    fn a_playlist_names_its_stream() {
        assert!(super::is_playlist("https://a.example/listen.pls?x=1", ""));
        assert!(super::is_playlist("https://a.example/live", "audio/x-mpegurl"));
        assert!(!super::is_playlist("https://a.example/live.m3u8", "application/vnd.apple.mpegurl"));
        assert!(!super::is_playlist("https://a.example/live.mp3", "audio/mpeg"));
        let pls = "[playlist]\nNumberOfEntries=2\nFile1=https://s.example/radio.mp3\nTitle1=x\nFile2=http://b\n";
        assert_eq!(super::first_in_playlist(pls).as_deref(), Some("https://s.example/radio.mp3"));
        let m3u = "#EXTM3U\n#EXTINF:-1,Radio\nhttp://s.example:8000/stream\n";
        assert_eq!(super::first_in_playlist(m3u).as_deref(), Some("http://s.example:8000/stream"));
        assert_eq!(super::first_in_playlist("nothing here"), None);
    }

    #[test]
    fn the_words_in_a_stream_are_its_title() {
        let mut words = b"StreamTitle='Hiroshi Yoshimura - Blink';StreamUrl='';".to_vec();
        words.resize(64, 0);
        assert_eq!(super::stream_title(&words).as_deref(), Some("Hiroshi Yoshimura - Blink"));
        assert_eq!(super::stream_title(b"StreamTitle='';\0\0"), None);
        assert_eq!(super::stream_title(b"StreamTitle='Bj\xf6rk';").as_deref(), Some("Bj\u{f6}rk"));
    }
}
