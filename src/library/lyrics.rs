//! The words of a song.
//!
//! From two places, in this order. The file's own tag first — ID3's USLT,
//! MP4's ©lyr, Vorbis' LYRICS — because what somebody put in the file is
//! what they meant, and it costs nothing to read. Then LRCLIB, an open
//! collection of lyrics that needs no key and often has them timed, a line
//! to a moment of the song, so the window can follow along.
//!
//! What LRCLIB says is kept on disk beside the library, one file a song,
//! and so is that it had nothing: the next time the song plays nothing is
//! asked again.

use std::path::{Path, PathBuf};

use crate::json::Json;
use crate::library::coverart::{encode, fetch_text};
use crate::error::Result;

/// One line of the words, and when it is sung, if that is known.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub at: Option<f64>,
    pub text: String,
    /// When each word is sung, where the words say (the enhanced LRC way,
    /// `<01:23.45>word`): the moment and where in `text` the word begins.
    /// Empty for words timed only by the line.
    pub words: Vec<(f64, usize)>,
}

impl Line {
    /// How much of the line has been sung at `secs`, as a place in `text`:
    /// up to the end of the last word begun. `None` for a line without
    /// moments of its words.
    pub fn sung_to(&self, secs: f64) -> Option<usize> {
        let begun = self.words.iter().rposition(|(at, _)| *at <= secs)?;
        Some(self.words.get(begun + 1).map_or(self.text.len(), |(_, from)| *from))
    }
}

/// A line's words without the moments of each word, and those moments with
/// where each word begins.
fn word_moments(raw: &str) -> (String, Vec<(f64, usize)>) {
    let mut text = String::with_capacity(raw.len());
    let mut moments = Vec::new();
    let mut rest = raw;
    while let Some(open) = rest.find('<') {
        let Some(close) = rest[open..].find('>') else { break };
        let inner = &rest[open + 1..open + close];
        match moment(inner) {
            Some(at) => {
                text.push_str(&rest[..open]);
                let from = text.len() + rest[open + close + 1..].len() - rest[open + close + 1..].trim_start().len();
                moments.push((at, from));
                rest = &rest[open + close + 1..];
            }
            None => {
                text.push_str(&rest[..open + 1]);
                rest = &rest[open + 1..];
            }
        }
    }
    text.push_str(rest);
    let lead = text.len() - text.trim_start().len();
    let text_trimmed = text.trim().to_string();
    let moments = moments
        .into_iter()
        .map(|(at, from)| (at, from.saturating_sub(lead).min(text_trimmed.len())))
        .collect();
    (text_trimmed, moments)
}

/// Where the words came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum From {
    File,
    Lrclib,
}

impl From {
    /// What the column says over the words, as under the artist's paragraph.
    pub fn name(self) -> &'static str {
        match self {
            From::File => "der datei",
            From::Lrclib => "lrclib",
        }
    }
}

/// The words of one song.
#[derive(Debug, Clone, PartialEq)]
pub struct Lyrics {
    pub lines: Vec<Line>,
    pub from: From,
    /// LRCLIB knows the song and says it has no words.
    pub instrumental: bool,
}

impl Lyrics {
    /// Whether the lines carry their moments.
    pub fn timed(&self) -> bool {
        self.lines.iter().any(|l| l.at.is_some())
    }

    /// The line being sung at `secs`: the last one that has begun. `None`
    /// before the first, and for words without moments.
    pub fn now(&self, secs: f64) -> Option<usize> {
        self.lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.at.is_some_and(|at| at <= secs))
            .map(|(i, _)| i)
            .next_back()
    }
}

/// Read words out of text that may be timed, the LRC way:
/// `[01:23.45] a line`. Header lines — `[ar:…]`, `[ti:…]` — are dropped; a
/// line with several moments is the same words sung again, and stands at
/// each of them. Untimed text is its lines as they are.
pub fn parse(text: &str) -> Vec<Line> {
    let mut lines = Vec::new();
    for raw in text.lines() {
        let raw = raw.trim_end_matches('\r');
        let mut rest = raw.trim_start();
        let mut moments = Vec::new();
        let mut tagged = false;
        while let Some(inner) = rest.strip_prefix('[') {
            let Some(end) = inner.find(']') else { break };
            tagged = true;
            if let Some(at) = moment(&inner[..end]) {
                moments.push(at);
            }
            rest = &inner[end + 1..];
        }
        let (words, timed_words) = word_moments(rest.trim());
        if tagged && moments.is_empty() {
            continue;
        }
        if moments.is_empty() {
            lines.push(Line { at: None, text: words, words: timed_words });
        } else {
            for at in moments {
                lines.push(Line { at: Some(at), text: words.clone(), words: timed_words.clone() });
            }
        }
    }
    if lines.iter().any(|l| l.at.is_some()) {
        lines.sort_by(|a, b| a.at.unwrap_or(0.0).total_cmp(&b.at.unwrap_or(0.0)));
    }
    let mut tidy: Vec<Line> = Vec::with_capacity(lines.len());
    for line in lines {
        let blank = line.text.is_empty();
        if blank && tidy.last().is_none_or(|l: &Line| l.text.is_empty()) {
            continue;
        }
        tidy.push(line);
    }
    while tidy.last().is_some_and(|l| l.text.is_empty()) {
        tidy.pop();
    }
    tidy
}

/// `mm:ss`, `mm:ss.xx` or `mm:ss:xx` as seconds.
fn moment(tag: &str) -> Option<f64> {
    let (minutes, seconds) = tag.trim().split_once(':')?;
    let minutes: f64 = minutes.parse().ok()?;
    let seconds: f64 = seconds.replacen(':', ".", 1).parse().ok()?;
    (minutes >= 0.0 && (0.0..60.0).contains(&seconds)).then_some(minutes * 60.0 + seconds)
}

/// The words the file carries itself, if it has any.
///
/// Read where taggers put them, which is not always where the standard
/// says: an ID3 USLT frame as it should be, but also a TXXX frame named
/// „LYRICS", „UNSYNCEDLYRICS" or — what ffmpeg writes — „USLT", which
/// lofty's common view of a tag leaves out; in other formats a field of
/// any of those names.
pub fn in_file(path: &Path) -> Option<Lyrics> {
    let text = from_id3(path).or_else(|| from_tags(path))?;
    let lines = parse(&text);
    (!lines.is_empty()).then_some(Lyrics { lines, from: From::File, instrumental: false })
}

/// The names lyrics go by, other than the standard's own.
const NAMES: [&str; 4] = ["LYRICS", "UNSYNCEDLYRICS", "USLT", "SYNCEDLYRICS"];

fn from_id3(path: &Path) -> Option<String> {
    use lofty::file::AudioFile;
    let mp3 = path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("mp3"));
    if !mp3 {
        return None;
    }
    let mut file = std::fs::File::open(path).ok()?;
    let read = lofty::mpeg::MpegFile::read_from(&mut file, lofty::config::ParseOptions::new()).ok()?;
    let tag = read.id3v2()?;
    let filled = |t: &str| !t.trim().is_empty();
    let found = tag
        .unsync_text()
        .map(|frame| frame.content.clone())
        .find(|t| filled(t))
        .or_else(|| {
            NAMES.iter().find_map(|name| {
                [name.to_string(), name.to_lowercase()]
                    .iter()
                    .find_map(|n| tag.get_user_text(n).filter(|t| filled(t)).map(str::to_string))
            })
        });
    found
}

fn from_tags(path: &Path) -> Option<String> {
    use lofty::prelude::*;
    use lofty::tag::ItemKey;
    let tagged = lofty::read_from_path(path).ok()?;
    tagged.tags().iter().find_map(|tag| {
        tag.get_string(&ItemKey::Lyrics).map(str::to_string).or_else(|| {
            tag.items().find_map(|item| match item.key() {
                ItemKey::Unknown(name) if NAMES.iter().any(|n| n.eq_ignore_ascii_case(name)) => {
                    item.value().text().map(str::to_string)
                }
                _ => None,
            })
        })
    })
}

/// The file the words of a song are kept in, by artist and title.
pub fn cache_path(dir: &Path, artist: &str, title: &str) -> PathBuf {
    let key = super::artwork::key(&crate::db::sort_key(artist), &format!("\u{3}lyrics\u{1}{}", crate::db::sort_key(title)));
    dir.join(format!("{key:016x}.lrc"))
}

/// What was kept of a song: its words, `Some(None)` for a song LRCLIB had
/// nothing for, and `None` for one never asked about.
pub fn load(dir: &Path, artist: &str, title: &str) -> Option<Option<Lyrics>> {
    let text = std::fs::read_to_string(cache_path(dir, artist, title)).ok()?;
    Some(from_kept(&text))
}

/// The kept file's words: a first line saying „instrumental" or „none", or
/// the words themselves.
fn from_kept(text: &str) -> Option<Lyrics> {
    match text.lines().next().map(str::trim) {
        Some(NONE) => None,
        Some(INSTRUMENTAL) => Some(Lyrics { lines: Vec::new(), from: From::Lrclib, instrumental: true }),
        _ => {
            let lines = parse(text);
            (!lines.is_empty()).then_some(Lyrics { lines, from: From::Lrclib, instrumental: false })
        }
    }
}

const NONE: &str = "# none";
const INSTRUMENTAL: &str = "# instrumental";

/// Keep what LRCLIB said about a song.
pub fn keep(dir: &Path, artist: &str, title: &str, found: Option<&Lyrics>, raw: &str) {
    let text = match found {
        None => NONE.to_string(),
        Some(l) if l.instrumental => INSTRUMENTAL.to_string(),
        Some(_) => raw.to_string(),
    };
    let _ = std::fs::create_dir_all(dir);
    let _ = std::fs::write(cache_path(dir, artist, title), text);
}

/// Ask LRCLIB for a song's words. Hands back what it found and the text to
/// keep, timed if it had them timed.
///
/// Its search rather than its exact lookup: the lookup answers a song it
/// does not know with a 404, which curl reports as a failure like any
/// other, and „not known" is not trouble. Of what the search gives, the
/// first by the same artist and title that runs within a few seconds of the
/// file — the same song, not a live take twice as long.
pub fn look_up(artist: &str, title: &str, secs: f64) -> Result<(Option<Lyrics>, String)> {
    let url = format!(
        "https://lrclib.net/api/search?artist_name={}&track_name={}",
        encode(artist),
        encode(title)
    );
    let answer = fetch_text(&url)?;
    Ok(pick(&answer, artist, title, secs))
}

/// The best of LRCLIB's answers — see [`look_up`].
fn pick(answer: &str, artist: &str, title: &str, secs: f64) -> (Option<Lyrics>, String) {
    let Some(json) = Json::parse(answer) else {
        return (None, String::new());
    };
    let same = |a: &str, b: &str| super::years::plain(a) == super::years::plain(b);
    let fits = |j: &Json| {
        let length = j.get("duration").and_then(Json::as_number).unwrap_or(0.0);
        let close = secs <= 0.0 || length <= 0.0 || (length - secs).abs() <= 4.0;
        close
            && j.text("trackName").is_some_and(|t| same(t, title))
            && j.text("artistName").is_some_and(|a| super::years::same_artist(artist, a))
    };
    let candidates: Vec<&Json> = json.as_array().unwrap_or(&[]).iter().filter(|j| fits(j)).collect();
    let timed = candidates.iter().find_map(|j| j.text("syncedLyrics").filter(|t| !t.trim().is_empty()));
    let plain = candidates.iter().find_map(|j| j.text("plainLyrics").filter(|t| !t.trim().is_empty()));
    if let Some(text) = timed.or(plain) {
        let lines = parse(text);
        if !lines.is_empty() {
            return (Some(Lyrics { lines, from: From::Lrclib, instrumental: false }), text.to_string());
        }
    }
    if candidates.iter().any(|j| matches!(j.get("instrumental"), Some(Json::Bool(true)))) {
        return (Some(Lyrics { lines: Vec::new(), from: From::Lrclib, instrumental: true }), String::new());
    }
    (None, String::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Enhanced LRC: the moments of the words come out of the text and say
    /// how far the line has been sung.
    #[test]
    fn the_words_of_a_line_carry_their_moments() {
        let lines = parse("[00:10.00] <00:10.00>so <00:10.50>lang <00:11.20>war es\n[00:12.00] ohne");
        assert_eq!(lines[0].text, "so lang war es");
        assert_eq!(lines[0].words.len(), 3);
        assert_eq!(lines[0].sung_to(9.0), None);
        assert_eq!(&lines[0].text[..lines[0].sung_to(10.6).unwrap()], "so lang ");
        assert_eq!(lines[0].sung_to(11.5), Some(lines[0].text.len()));
        assert!(lines[1].words.is_empty());
        assert_eq!(lines[1].sung_to(13.0), None);
        assert_eq!(parse("a <b> c")[0].text, "a <b> c");
    }

    #[test]
    fn timed_words_come_apart_into_moments() {
        let lines = parse("[ar: Coil]\n[00:12.50]first\n[00:05.00][01:00.00]again\n\n[00:20.00]\n[00:21.00]third\n");
        let said: Vec<(Option<f64>, &str)> = lines.iter().map(|l| (l.at, l.text.as_str())).collect();
        assert_eq!(
            said,
            vec![
                (Some(5.0), "again"),
                (Some(12.5), "first"),
                (Some(20.0), ""),
                (Some(21.0), "third"),
                (Some(60.0), "again"),
            ]
        );
    }

    #[test]
    fn plain_words_are_their_lines_tidied() {
        let lines = parse("\n\nverse one\nline two\n\n\n\nverse two\n\n");
        let said: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(said, vec!["verse one", "line two", "", "verse two"]);
        assert!(lines.iter().all(|l| l.at.is_none()));
    }

    #[test]
    fn the_line_sung_is_the_last_begun() {
        let words = Lyrics { lines: parse("[00:01.00]a\n[00:03.00]b\n[00:07.00]c"), from: From::Lrclib, instrumental: false };
        assert_eq!(words.now(0.5), None);
        assert_eq!(words.now(1.0), Some(0));
        assert_eq!(words.now(6.9), Some(1));
        assert_eq!(words.now(100.0), Some(2));
    }

    #[test]
    fn the_answer_that_fits_is_taken_timed_first() {
        let answer = r#"[
            {"trackName":"Other","artistName":"Coil","duration":200,"plainLyrics":"no"},
            {"trackName":"Ostia","artistName":"Coil","duration":400,"plainLyrics":"too long"},
            {"trackName":"Ostia","artistName":"Coil","duration":301,"plainLyrics":"plain words","syncedLyrics":"[00:01.00]timed words"}
        ]"#;
        let (found, raw) = pick(answer, "Coil", "ostia", 300.0);
        let found = found.unwrap();
        assert!(found.timed());
        assert_eq!(found.lines[0].text, "timed words");
        assert_eq!(raw, "[00:01.00]timed words");
        let (nothing, _) = pick("[]", "Coil", "Ostia", 300.0);
        assert!(nothing.is_none());
        let (none, _) = pick(answer, "Coil", "Ostia", 100.0);
        assert!(none.is_none(), "nothing within a few seconds");
    }

    /// The words in the file are found where the standard puts them and
    /// where ffmpeg puts them.
    #[test]
    fn the_words_in_the_file_are_found() {
        use lofty::config::WriteOptions;
        use lofty::id3::v2::Id3v2Tag;
        use lofty::tag::TagExt;
        let fixtures = crate::library::fixtures::scratch("lyrics-in-file");
        let _ = std::fs::create_dir_all(&fixtures);

        let standard = fixtures.join("standard.mp3");
        crate::library::fixtures::place(&standard, crate::library::fixtures::MP3);
        let mut tag = lofty::tag::Tag::new(lofty::tag::TagType::Id3v2);
        tag.insert_text(lofty::tag::ItemKey::Lyrics, "one\ntwo".into());
        tag.save_to_path(&standard, WriteOptions::default()).unwrap();
        let words = in_file(&standard).unwrap();
        assert_eq!(words.lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), vec!["one", "two"]);
        assert_eq!(words.from, From::File);

        let ffmpeg = fixtures.join("ffmpeg.mp3");
        crate::library::fixtures::place(&ffmpeg, crate::library::fixtures::MP3);
        let mut tag = Id3v2Tag::new();
        tag.insert_user_text("USLT".into(), "three".into());
        tag.save_to_path(&ffmpeg, WriteOptions::default()).unwrap();
        assert_eq!(in_file(&ffmpeg).unwrap().lines[0].text, "three");

        let bare = fixtures.join("bare.mp3");
        crate::library::fixtures::place(&bare, crate::library::fixtures::MP3);
        assert!(in_file(&bare).is_none());
        let _ = std::fs::remove_dir_all(fixtures);
    }

    #[test]
    fn an_instrumental_is_said_and_kept() {
        let answer = r#"[{"trackName":"Twoism","artistName":"Boards of Canada","duration":300,"instrumental":true,"plainLyrics":null}]"#;
        let (found, _) = pick(answer, "Boards of Canada", "Twoism", 299.0);
        assert!(found.as_ref().unwrap().instrumental);
        let dir = std::env::temp_dir().join(format!("kessler-lyrics-{}", std::process::id()));
        keep(&dir, "Boards of Canada", "Twoism", found.as_ref(), "");
        assert!(load(&dir, "Boards of Canada", "Twoism").unwrap().unwrap().instrumental);
        keep(&dir, "x", "y", None, "");
        assert_eq!(load(&dir, "x", "y"), Some(None));
        assert_eq!(load(&dir, "never", "asked"), None);
        let words = parse("[00:01.00]hello");
        keep(&dir, "a", "b", Some(&Lyrics { lines: words.clone(), from: From::Lrclib, instrumental: false }), "[00:01.00]hello");
        assert_eq!(load(&dir, "a", "b").unwrap().unwrap().lines, words);
        let _ = std::fs::remove_dir_all(dir);
    }
}
