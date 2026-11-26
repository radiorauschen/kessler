//! A podcast's feed read: its name, who makes it, its picture, and its
//! episodes with where each one's sound is.
//!
//! Written by hand, as the JSON reader is: an RSS feed is a small, loose kind
//! of XML, and what is wanted out of it is a dozen fields. The reader below
//! knows tags, attributes, text, CDATA and the five named entities and the
//! numbered ones; it does not check that the document is well formed, so a
//! feed with a stray tag still gives what it can.

/// One episode of a feed.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Episode {
    /// What tells it from the others for good: its `guid`, else its sound's
    /// address.
    pub id: String,
    pub title: String,
    /// Where its sound is: the enclosure's address.
    pub url: String,
    /// How long the sound file is, in bytes, as the feed says — 0 where it
    /// does not.
    pub bytes: u64,
    /// When it came out, as the feed writes it (RFC 822).
    pub published: String,
    /// How long it plays, in seconds, where the feed says.
    pub seconds: Option<u64>,
    /// What it is about, as plain text.
    pub about: String,
    /// Its chapters, where the feed lists them itself (Podlove's
    /// `psc:chapter`), the earliest first.
    pub chapters: Vec<Chapter>,
    /// Where its chapters are, where the feed points at a file of them
    /// (Podcasting 2.0's `podcast:chapters`, JSON) — read when it is opened.
    pub chapters_url: String,
    /// Its own picture, where it has one — else the podcast's stands for it.
    pub artwork: String,
    /// A wider picture of it, where the feed gives one beside the square
    /// `itunes:image` (`media:thumbnail`, an image's `media:content`): what its
    /// page shows large, where the square was a crop of it.
    pub wide_artwork: String,
    /// The pictures in what it is about, their addresses, as many as
    /// [`IMAGES_MOST`].
    pub images: Vec<String>,
    /// The links in what it is about, with their words.
    pub links: Vec<(String, String)>,
    /// Its own page, where the feed names one: what „teilen" passes on.
    pub link: String,
    /// When it came out, in seconds since 1970, read once from `published` as
    /// the feed is read — 0 where it does not say. Lists are sorted by it on
    /// every frame, and reading the date from its words each time was most of
    /// what a long „alle" cost.
    pub at: i64,
    /// Its season and number, where the feed says (`itunes:season`,
    /// `itunes:episode`).
    pub season: Option<u32>,
    pub number: Option<u32>,
}

/// How many of the pictures in an episode's notes are kept.
const IMAGES_MOST: usize = 6;

/// The pictures an episode's notes show (`<img src>`), in their order —
/// not the one-point ones that only count who read them.
/// How many links of a text are kept.
pub const LINKS_MOST: usize = 24;

/// The links in a text: each `<a href>` with its words, then any address
/// written out in it; once each, in order, web addresses only. The words are
/// the address where the link has none of its own.
pub fn links_in(html: &str) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = Vec::new();
    let keep = |words: String, url: String, found: &mut Vec<(String, String)>| {
        let url = url.trim().trim_end_matches(['.', ',', ';', ':', ')', '!', '?']).to_string();
        if !(url.starts_with("https://") || url.starts_with("http://")) || found.iter().any(|(_, u)| *u == url) || found.len() >= LINKS_MOST {
            return;
        }
        let words = words.trim().to_string();
        let words = if words.is_empty() || words.starts_with("http") { url.clone() } else { words };
        found.push((words, url));
    };
    let lower = html.to_ascii_lowercase();
    let mut from = 0;
    let mut outside = String::new();
    while let Some(at) = lower[from..].find("<a ") {
        outside.push_str(&html[from..from + at]);
        let start = from + at;
        let Some(close) = lower[start..].find('>').map(|e| start + e) else { break };
        let tag = &html[start..close];
        let tag_lower = &lower[start..close];
        let end = lower[close..].find("</a").map_or(lower.len(), |e| close + e);
        if let Some(at) = tag_lower.find("href=") {
            let rest = &tag[at + 5..];
            let href = match rest.chars().next() {
                Some(q @ ('"' | '\'')) => rest[1..].find(q).map(|e| rest[1..1 + e].to_string()),
                _ => Some(rest.split(|c: char| c.is_whitespace()).next().unwrap_or("").to_string()),
            };
            if let Some(href) = href {
                keep(plain(&html[close + 1..end]), unescape(&href), &mut found);
            }
        }
        from = end.max(close + 1);
    }
    outside.push_str(&html[from.min(html.len())..]);
    for word in plain(&outside).split(|c: char| c.is_whitespace() || c == '<' || c == '>' || c == '"') {
        if let Some(at) = word.find("http") {
            keep(String::new(), word[at..].to_string(), &mut found);
        }
    }
    found
}

fn images_in(html: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let lower = html.to_ascii_lowercase();
    let mut from = 0;
    while let Some(at) = lower[from..].find("<img") {
        let start = from + at;
        let end = lower[start..].find('>').map_or(lower.len(), |e| start + e);
        let tag = &html[start..end];
        let tag_lower = &lower[start..end];
        let attr = |name: &str| -> Option<String> {
            let at = tag_lower.find(&format!("{name}="))? + name.len() + 1;
            let rest = &tag[at..];
            let quote = rest.chars().next()?;
            if quote == '"' || quote == '\'' {
                rest[1..].find(quote).map(|e| rest[1..1 + e].to_string())
            } else {
                Some(rest.split(|c: char| c.is_whitespace() || c == '/').next().unwrap_or("").to_string())
            }
        };
        let tiny = ["width", "height"].iter().any(|d| attr(d).is_some_and(|v| v.trim().parse::<u32>().is_ok_and(|n| n <= 2)));
        if let Some(src) = attr("src").map(|s| unescape(s.trim())) {
            if !tiny && (src.starts_with("https://") || src.starts_with("http://")) && !found.contains(&src) {
                found.push(src);
            }
        }
        from = end;
        if found.len() >= IMAGES_MOST {
            break;
        }
    }
    found
}

/// Whether an enclosure is a video's rather than sound: by its type, else —
/// none given — by an ending only video has.
fn is_video(url: &str, kind: &str) -> bool {
    let kind = kind.trim().to_ascii_lowercase();
    if !kind.is_empty() {
        return kind.starts_with("video/");
    }
    let bare = url.split(['?', '#']).next().unwrap_or(url).to_ascii_lowercase();
    bare.ends_with(".m4v") || bare.ends_with(".mov")
}

/// A chapter of an episode: where it starts, in seconds, and what it is called.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Chapter {
    pub start: f64,
    pub title: String,
}

/// The chapters an MP3 carries in its own ID3 tag, as `CHAP` frames: each
/// frame's start, and the title in the `TIT2` inside it. ID3 2.3 and 2.4;
/// nothing for a file without a tag, or one with no chapters in it.
pub fn chapters_in_id3(bytes: &[u8]) -> Vec<Chapter> {
    let synchsafe = |b: &[u8]| b.iter().take(4).fold(0usize, |n, &x| (n << 7) | usize::from(x & 0x7f));
    let plain = |b: &[u8]| b.iter().take(4).fold(0usize, |n, &x| (n << 8) | usize::from(x));
    if bytes.len() < 10 || &bytes[..3] != b"ID3" {
        return Vec::new();
    }
    let version = bytes[3];
    if !(3..=4).contains(&version) {
        return Vec::new();
    }
    let flags = bytes[5];
    let end = (10 + synchsafe(&bytes[6..10])).min(bytes.len());
    let mut at = 10;
    if flags & 0x40 != 0 && at + 4 <= end {
        at += if version == 4 { synchsafe(&bytes[at..]) } else { plain(&bytes[at..]) + 4 };
    }
    let size_of = |b: &[u8]| if version == 4 { synchsafe(b) } else { plain(b) };
    let mut chapters = Vec::new();
    for_frames(&bytes[at.min(end)..end], &size_of, &mut |id, body| {
        if id != b"CHAP" {
            return;
        }
        let Some(nul) = body.iter().position(|b| *b == 0) else { return };
        let times = &body[nul + 1..];
        if times.len() < 16 {
            return;
        }
        let start = plain(&times[..4]) as f64 / 1000.0;
        let mut title = String::new();
        for_frames(&times[16..], &size_of, &mut |id, body| {
            if id == b"TIT2" && title.is_empty() {
                title = id3_text(body);
            }
        });
        chapters.push(Chapter { start, title });
    });
    chapters.sort_by(|a, b| a.start.total_cmp(&b.start));
    chapters
}

/// Each frame in `bytes`: its id and its body.
fn for_frames(bytes: &[u8], size_of: &dyn Fn(&[u8]) -> usize, each: &mut dyn FnMut(&[u8], &[u8])) {
    let mut at = 0;
    while at + 10 <= bytes.len() {
        let id = &bytes[at..at + 4];
        if id[0] == 0 || !id.iter().all(|b| b.is_ascii_alphanumeric()) {
            break;
        }
        let size = size_of(&bytes[at + 4..at + 8]);
        let body = at + 10;
        if size == 0 || body + size > bytes.len() {
            break;
        }
        each(id, &bytes[body..body + size]);
        at = body + size;
    }
}

/// A text frame's words, in whichever of ID3's four encodings.
fn id3_text(body: &[u8]) -> String {
    let Some((&encoding, text)) = body.split_first() else { return String::new() };
    let utf16 = |text: &[u8], big: bool| {
        let units: Vec<u16> = text
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&p| if big { u16::from_be_bytes(p) } else { u16::from_le_bytes(p) })
            .take_while(|u| *u != 0)
            .collect();
        String::from_utf16_lossy(&units)
    };
    let words = match encoding {
        0 => text.iter().take_while(|b| **b != 0).map(|b| char::from(*b)).collect(),
        1 => match text {
            [0xfe, 0xff, rest @ ..] => utf16(rest, true),
            [0xff, 0xfe, rest @ ..] => utf16(rest, false),
            _ => utf16(text, false),
        },
        2 => utf16(text, true),
        _ => String::from_utf8_lossy(text.split(|b| *b == 0).next().unwrap_or_default()).into_owned(),
    };
    words.trim().to_string()
}

/// A time as chapters write it: `HH:MM:SS.mmm`, `MM:SS` or seconds.
fn time_of(text: &str) -> Option<f64> {
    let parts: Vec<f64> = text.trim().split(':').map(|p| p.trim().parse::<f64>().ok()).collect::<Option<_>>()?;
    Some(parts.iter().fold(0.0, |sum, p| sum * 60.0 + p))
}

/// The chapters in a Podcasting 2.0 chapters file: `{"chapters": [{"startTime":
/// 0, "title": "…"}, …]}`, those without a title or marked not to be shown
/// left out.
pub fn chapters_in(text: &str) -> Vec<Chapter> {
    let Some(json) = crate::json::Json::parse(text) else { return Vec::new() };
    let mut chapters: Vec<Chapter> = json
        .list("chapters")
        .iter()
        .filter(|c| c.get("toc").and_then(|t| if let crate::json::Json::Bool(b) = t { Some(*b) } else { None }) != Some(false))
        .filter_map(|c| {
            let title = c.text("title")?.trim().to_string();
            let start = c.get("startTime").and_then(|s| s.as_number())?;
            (!title.is_empty()).then_some(Chapter { start, title })
        })
        .collect();
    chapters.sort_by(|a, b| a.start.total_cmp(&b.start));
    chapters
}

/// A whole feed.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Feed {
    pub title: String,
    pub author: String,
    pub about: String,
    pub artwork: String,
    /// Its own website.
    pub link: String,
    /// Who makes it, where the feed names them.
    pub people: Vec<String>,
    /// Whose it is, as the feed's owner (`itunes:owner`) — often the
    /// network or the paper behind it.
    pub owner: String,
    /// The links in what it is about.
    pub about_links: Vec<(String, String)>,
    /// In the feed's order, which is newest first nearly everywhere.
    pub episodes: Vec<Episode>,
    /// How many older episodes the feed has that are not held — see
    /// [`Feed::keep_newest`].
    pub older: usize,
    /// A series, to be heard from the first episode on (`itunes:type
    /// serial`) rather than the newest.
    pub serial: bool,
    /// Where to support it (`podcast:funding`): its words and address.
    pub funding: Vec<(String, String)>,
    /// Where it says it has moved to (`itunes:new-feed-url`).
    pub new_url: String,
}

/// How many of a podcast's newest episodes are held in memory.
pub const KEPT: usize = 30;

impl Feed {
    /// Only the newest [`KEPT`] episodes held, and those `also` asks for
    /// (begun ones, so that „offen" keeps them); the rest are counted in
    /// [`Feed::older`], to be read when asked for. Every episode of every
    /// podcast with all it said came to a hundred megabytes and more with
    /// sixty podcasts, some of them with thousands of episodes — when what
    /// is listened to is nearly always among the newest.
    pub fn keep_newest(&mut self, also: impl Fn(&Episode) -> bool) {
        if self.serial {
            return;
        }
        let mut order: Vec<usize> = (0..self.episodes.len()).collect();
        order.sort_by_key(|&i| std::cmp::Reverse(self.episodes[i].at));
        let mut keep = vec![false; self.episodes.len()];
        for (rank, &i) in order.iter().enumerate() {
            keep[i] = rank < KEPT || also(&self.episodes[i]);
        }
        let before = self.episodes.len();
        let mut flags = keep.into_iter();
        self.episodes.retain(|_| flags.next().unwrap_or(false));
        self.episodes.shrink_to_fit();
        self.older += before - self.episodes.len();
    }
}

/// What the reader sees, one piece at a time.
#[derive(Debug, PartialEq)]
enum Piece {
    Open(String, Vec<(String, String)>),
    Close(String),
    Text(String),
}

/// The pieces of `xml`, in order. A tag that opens and closes itself
/// (`<enclosure … />`) is an `Open` and a `Close`.
fn pieces(xml: &str) -> Vec<Piece> {
    let mut out = Vec::new();
    let mut rest = xml;
    while !rest.is_empty() {
        let Some(at) = rest.find('<') else {
            push_text(&mut out, rest);
            break;
        };
        push_text(&mut out, &rest[..at]);
        rest = &rest[at..];
        if let Some(inner) = rest.strip_prefix("<![CDATA[") {
            let end = inner.find("]]>").unwrap_or(inner.len());
            out.push(Piece::Text(inner[..end].to_string()));
            rest = inner.get(end + 3..).unwrap_or("");
        } else if let Some(inner) = rest.strip_prefix("<!--") {
            rest = inner.find("-->").map_or("", |end| &inner[end + 3..]);
        } else if rest.starts_with("<?") || rest.starts_with("<!") {
            rest = rest.find('>').map_or("", |end| &rest[end + 1..]);
        } else {
            let Some(end) = tag_end(rest) else { break };
            let tag = &rest[1..end];
            rest = &rest[end + 1..];
            if let Some(name) = tag.strip_prefix('/') {
                out.push(Piece::Close(name.trim().to_string()));
            } else {
                let closes = tag.ends_with('/');
                let tag = tag.trim_end_matches('/');
                let name_end = tag.find(|c: char| c.is_whitespace()).unwrap_or(tag.len());
                let name = tag[..name_end].to_string();
                out.push(Piece::Open(name.clone(), attributes(&tag[name_end..])));
                if closes {
                    out.push(Piece::Close(name));
                }
            }
        }
    }
    out
}

/// Where the tag at the start of `rest` ends: its `>`, not one inside a
/// quoted attribute.
fn tag_end(rest: &str) -> Option<usize> {
    let mut quote: Option<char> = None;
    for (at, c) in rest.char_indices() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '>') => return Some(at),
            _ => {}
        }
    }
    None
}

fn push_text(out: &mut Vec<Piece>, text: &str) {
    if !text.trim().is_empty() {
        out.push(Piece::Text(unescape(text)));
    }
}

/// `name="value"` pairs, either quote.
fn attributes(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(eq) = rest.find('=') {
        let name = rest[..eq].trim().to_string();
        let after = rest[eq + 1..].trim_start();
        let Some(quote) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else { break };
        let value_and_rest = &after[1..];
        let Some(close) = value_and_rest.find(quote) else { break };
        out.push((name, unescape(&value_and_rest[..close])));
        rest = &value_and_rest[close + 1..];
    }
    out
}

/// The named entities HTML has that show notes use, beyond the five XML
/// knows: spaces, dashes, quotes, the letters with marks, a few signs.
/// Escaped once more inside the feed's XML, they came through as „&nbsp;".
fn html_entity(name: &str) -> Option<char> {
    Some(match name {
        "nbsp" => '\u{a0}',
        "ensp" | "emsp" | "thinsp" => ' ',
        "shy" => '\u{ad}',
        "zwnj" | "zwj" => '\u{200c}',
        "ndash" => '–',
        "mdash" => '—',
        "hellip" => '…',
        "lsquo" => '‘',
        "rsquo" => '’',
        "sbquo" => '‚',
        "ldquo" => '“',
        "rdquo" => '”',
        "bdquo" => '„',
        "laquo" => '«',
        "raquo" => '»',
        "lsaquo" => '‹',
        "rsaquo" => '›',
        "middot" => '·',
        "bull" => '•',
        "deg" => '°',
        "euro" => '€',
        "pound" => '£',
        "copy" => '©',
        "reg" => '®',
        "trade" => '™',
        "times" => '×',
        "auml" => 'ä',
        "ouml" => 'ö',
        "uuml" => 'ü',
        "Auml" => 'Ä',
        "Ouml" => 'Ö',
        "Uuml" => 'Ü',
        "szlig" => 'ß',
        "eacute" => 'é',
        "egrave" => 'è',
        "ecirc" => 'ê',
        "aacute" => 'á',
        "agrave" => 'à',
        "acirc" => 'â',
        "iacute" => 'í',
        "oacute" => 'ó',
        "uacute" => 'ú',
        "ccedil" => 'ç',
        "ntilde" => 'ñ',
        "Eacute" => 'É',
        _ => return None,
    })
}

/// The entities XML names, the numbered ones, and HTML's named ones (see
/// [`html_entity`]).
fn unescape(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let Some(end) = rest.find(';').filter(|&end| end <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let name = &rest[1..end];
        let said = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ if html_entity(name).is_some() => html_entity(name),
            _ => name
                .strip_prefix("#x")
                .or_else(|| name.strip_prefix("#X"))
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| name.strip_prefix('#').and_then(|dec| dec.parse().ok()))
                .and_then(char::from_u32),
        };
        match said {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// What a description says once its HTML is taken out: tags gone, the
/// entities they hid read, breaks and paragraphs as line breaks, runs of
/// space as one.
pub fn plain(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(at) = rest.find('<') {
        out.push_str(&rest[..at]);
        let end = rest[at..].find('>').map_or(rest.len(), |e| at + e + 1);
        let tag = rest[at..end].to_ascii_lowercase();
        if tag.starts_with("<br") || tag.starts_with("</p") || tag.starts_with("</li") {
            out.push('\n');
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    let out = unescape(&out).replace('\u{ad}', "");
    out.lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join("\n")
        .split("\n\n\n")
        .collect::<Vec<_>>()
        .join("\n\n")
        .trim()
        .to_string()
}

/// `itunes:duration` in seconds: `HH:MM:SS`, `MM:SS` or seconds alone.
fn seconds_of(text: &str) -> Option<u64> {
    let parts: Vec<u64> = text.trim().split(':').map(|p| p.trim().parse::<f64>().ok().map(|v| v as u64)).collect::<Option<_>>()?;
    match parts.as_slice() {
        [s] => Some(*s),
        [m, s] => Some(m * 60 + s),
        [h, m, s] => Some(h * 3600 + m * 60 + s),
        _ => None,
    }
    .filter(|&s| s > 0)
}

/// The podcasts in an OPML file, as every podcast app writes its list: each
/// `outline` with a feed (`xmlUrl`), named by its `text` or `title`.
pub fn opml_shows(text: &str) -> Vec<(String, String)> {
    pieces(text)
        .into_iter()
        .filter_map(|piece| match piece {
            Piece::Open(name, attrs) if name.eq_ignore_ascii_case("outline") => {
                let attr = |key: &str| attrs.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.trim().to_string());
                let feed = attr("xmlUrl").filter(|f| !f.is_empty())?;
                let title = attr("text").or_else(|| attr("title")).unwrap_or_default();
                Some((title, feed))
            }
            _ => None,
        })
        .collect()
}

/// Read a feed. `None` where there is no channel in it at all.
pub fn read(xml: &str) -> Option<Feed> {
    let mut feed = Feed::default();
    let mut seen_channel = false;
    let mut open: Vec<String> = Vec::new();
    let mut episode: Option<Episode> = None;
    let mut guid = String::new();
    let mut wide_sized = false;
    for piece in pieces(xml) {
        match piece {
            Piece::Open(name, attrs) => {
                let attr = |key: &str| attrs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()).unwrap_or_default();
                match name.as_str() {
                    "channel" => seen_channel = true,
                    "item" => {
                        episode = Some(Episode::default());
                        guid.clear();
                        wide_sized = false;
                    }
                    "enclosure" => {
                        if let Some(e) = episode.as_mut() {
                            if e.url.is_empty() && !is_video(&attr("url"), &attr("type")) {
                                e.url = attr("url");
                                e.bytes = attr("length").trim().parse().unwrap_or(0);
                            }
                        }
                    }
                    "itunes:image" if episode.is_none() && feed.artwork.is_empty() => feed.artwork = attr("href"),
                    "itunes:image" => {
                        if let Some(e) = episode.as_mut().filter(|e| e.artwork.is_empty()) {
                            e.artwork = attr("href").trim().to_string();
                        }
                    }
                    "media:thumbnail" | "media:content" => {
                        let image = name == "media:thumbnail" || attr("medium") == "image" || attr("type").starts_with("image/");
                        let (wide, tall) = (attr("width").trim().parse::<u32>().unwrap_or(0), attr("height").trim().parse::<u32>().unwrap_or(0));
                        let url = attr("url").trim().to_string();
                        if let Some(e) = episode.as_mut().filter(|_| image && url.starts_with("http")) {
                            let sized = wide > 0 && tall > 0;
                            if sized && u64::from(wide) * 20 > u64::from(tall) * 21 && !wide_sized {
                                e.wide_artwork = url;
                                wide_sized = true;
                            } else if !sized && e.wide_artwork.is_empty() {
                                e.wide_artwork = url;
                            }
                        }
                    }
                    "psc:chapter" => {
                        if let (Some(e), Some(start)) = (episode.as_mut(), time_of(&attr("start"))) {
                            let title = attr("title").trim().to_string();
                            if !title.is_empty() {
                                e.chapters.push(Chapter { start, title });
                            }
                        }
                    }
                    "podcast:funding" if episode.is_none() => {
                        let url = attr("url").trim().to_string();
                        if url.starts_with("http") && !feed.funding.iter().any(|(_, u)| *u == url) {
                            feed.funding.push((String::new(), url));
                        }
                    }
                    "podcast:chapters" => {
                        if let Some(e) = episode.as_mut() {
                            e.chapters_url = attr("url");
                        }
                    }
                    _ => {}
                }
                open.push(name);
            }
            Piece::Close(name) => {
                if name == "item" {
                    if let Some(mut e) = episode.take() {
                        e.id = if guid.trim().is_empty() { e.url.clone() } else { guid.trim().to_string() };
                        e.images = images_in(&e.about);
                        e.links = links_in(&e.about);
                        e.at = super::date_of(&e.published).map_or(0, |d| d.timestamp());
                        e.about = plain(&e.about);
                        if !e.url.is_empty() {
                            feed.episodes.push(e);
                        }
                    }
                }
                if let Some(at) = open.iter().rposition(|o| *o == name) {
                    open.truncate(at);
                }
            }
            Piece::Text(text) => {
                let here = open.last().map(String::as_str).unwrap_or("");
                let parent = open.len().checked_sub(2).and_then(|i| open.get(i)).map(String::as_str).unwrap_or("");
                match episode.as_mut() {
                    Some(e) => match here {
                        "title" if e.title.is_empty() => e.title = text.trim().to_string(),
                        "link" if e.link.is_empty() => e.link = text.trim().to_string(),
                        "guid" => guid.push_str(&text),
                        "pubDate" => e.published = text.trim().to_string(),
                        "itunes:duration" => e.seconds = seconds_of(&text),
                        "itunes:season" => e.season = text.trim().parse().ok(),
                        "itunes:episode" => e.number = text.trim().parse().ok(),
                        "description" | "itunes:summary" | "content:encoded" if e.about.is_empty() => e.about = text,
                        _ => {}
                    },
                    None => match (parent, here) {
                        ("channel", "title") if feed.title.is_empty() => feed.title = text.trim().to_string(),
                        ("channel", "itunes:author") if feed.author.is_empty() => feed.author = text.trim().to_string(),
                        ("itunes:owner", "itunes:name") if feed.owner.is_empty() => feed.owner = text.trim().to_string(),
                        ("channel", "description" | "itunes:summary") if feed.about.is_empty() => {
                            feed.about = plain(&text);
                            feed.about_links = links_in(&text);
                        }
                        ("image", "url") if feed.artwork.is_empty() => feed.artwork = text.trim().to_string(),
                        ("channel", "link") if feed.link.is_empty() => feed.link = text.trim().to_string(),
                        ("channel", "itunes:new-feed-url") => feed.new_url = text.trim().to_string(),
                        ("channel", "itunes:type") => feed.serial = text.trim().eq_ignore_ascii_case("serial"),
                        (_, "podcast:funding") => {
                            if let Some((words, _)) = feed.funding.last_mut().filter(|(w, _)| w.is_empty()) {
                                *words = text.trim().to_string();
                            }
                        }
                        ("channel", "podcast:person") => {
                            let name = text.trim().to_string();
                            if !name.is_empty() && !feed.people.contains(&name) {
                                feed.people.push(name);
                            }
                        }
                        _ => {}
                    },
                }
            }
        }
    }
    seen_channel.then_some(feed)
}

impl Feed {
    /// Who makes it: the people the feed names, else its author.
    pub fn makers(&self) -> String {
        if self.people.is_empty() { self.author.clone() } else { self.people.join(", ") }
    }

    /// Who is behind it, where that is someone else than who makes it — the
    /// paper or the network, as „New York Times Opinion" over a host's name:
    /// its author where the people stand above, else its owner. Nothing where
    /// it would only say the makers or the title again.
    pub fn publisher(&self) -> Option<String> {
        let makers = self.makers().to_lowercase();
        let title = self.title.to_lowercase();
        [&self.author, &self.owner]
            .into_iter()
            .map(|who| who.trim())
            .find(|who| {
                let low = who.to_lowercase();
                !who.is_empty() && low != makers && low != title && !makers.contains(&low)
            })
            .map(str::to_string)
    }
}

/// How often a podcast comes out, read off the days its episodes came out: how
/// often, on which day mostly, and when the next one is about due — in words,
/// or nothing where there is no rhythm to it.
pub fn rhythm(episodes: &[Episode], today: chrono::NaiveDate) -> Option<String> {
    use chrono::Datelike;
    let mut ats: Vec<i64> = episodes.iter().map(super::when).filter(|at| *at > 0).collect();
    ats.sort_unstable_by(|a, b| b.cmp(a));
    ats.truncate(12);
    let days: Vec<chrono::NaiveDate> = ats
        .iter()
        .filter_map(|at| chrono::DateTime::from_timestamp(*at, 0))
        .map(|d| d.with_timezone(&chrono::Local).date_naive())
        .collect();
    if days.len() < 4 {
        return None;
    }
    let mut gaps: Vec<i64> = days.windows(2).map(|w| (w[0] - w[1]).num_days()).collect();
    gaps.sort_unstable();
    let gap = gaps[gaps.len() / 2];
    let how = match gap {
        0..=1 => tr!("täglich"),
        2..=4 => tr!("mehrmals die woche"),
        5..=9 => tr!("wöchentlich"),
        12..=17 => tr!("alle zwei wochen"),
        25..=35 => tr!("monatlich"),
        _ => return None,
    };
    let mut said = how.to_string();
    if (5..=17).contains(&gap) {
        let recent = &days[..days.len().min(8)];
        let mut counts = [0usize; 7];
        for day in recent {
            counts[day.weekday().num_days_from_monday() as usize] += 1;
        }
        if let Some((weekday, &n)) = counts.iter().enumerate().max_by_key(|(_, n)| **n) {
            if n * 10 >= recent.len() * 6 {
                const ON: [&str; 7] = ["montags", "dienstags", "mittwochs", "donnerstags", "freitags", "samstags", "sonntags"];
                said = tr!("{}, meist {}", how, crate::lang::t(ON[weekday]));
            }
        }
    }
    let next = days[0] + chrono::Days::new(gap.max(1) as u64);
    if next >= today {
        said = tr!("{} · nächste folge um den {}", said, next.format("%d.%m.").to_string());
    }
    Some(said)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rhythm_is_read_off_the_days() {
        let saturdays: Vec<Episode> = (0..8)
            .map(|i| Episode {
                published: (chrono::NaiveDate::from_ymd_opt(2026, 10, 31).unwrap() - chrono::Days::new(7 * i))
                    .and_hms_opt(12, 0, 0)
                    .unwrap()
                    .and_utc()
                    .to_rfc2822(),
                ..Default::default()
            })
            .collect();
        let today = chrono::NaiveDate::from_ymd_opt(2026, 11, 2).unwrap();
        assert_eq!(rhythm(&saturdays, today).as_deref(), Some("wöchentlich, meist samstags · nächste folge um den 07.11."));
        assert_eq!(rhythm(&saturdays[..2], today), None, "too few to tell");
    }

    #[test]
    fn the_links_in_the_notes_are_found() {
        let html = r#"<p>Mit <a href="https://example.org/gast">unserem Gast</a> und <a href='https://x.org/a?b=1&amp;c=2'>https://x.org/a</a>. Mehr unter https://example.org/mehr. <a href="mailto:a@b.c">post</a> <a href="https://example.org/gast">noch mal</a></p>"#;
        assert_eq!(
            links_in(html),
            vec![
                ("unserem Gast".to_string(), "https://example.org/gast".to_string()),
                ("https://x.org/a?b=1&c=2".to_string(), "https://x.org/a?b=1&c=2".to_string()),
                ("https://example.org/mehr".to_string(), "https://example.org/mehr".to_string()),
            ]
        );
    }

    #[test]
    fn the_pictures_in_the_notes_are_found() {
        let html = r#"<p>Hallo</p><img src="https://x.org/a.jpg" alt=""><img width="1" height="1" src="https://t.org/p.gif"><IMG SRC='https://x.org/b.png'/><img src="https://x.org/a.jpg">"#;
        assert_eq!(images_in(html), vec!["https://x.org/a.jpg".to_string(), "https://x.org/b.png".to_string()]);
    }

    /// One ID3 2.3 frame.
    fn frame(id: &[u8], body: &[u8]) -> Vec<u8> {
        let mut f = id.to_vec();
        f.extend_from_slice(&(body.len() as u32).to_be_bytes());
        f.extend_from_slice(&[0, 0]);
        f.extend_from_slice(body);
        f
    }

    #[test]
    fn chapters_come_out_of_the_id3_tag() {
        let chap = |element: &str, start: u32, title: &[u8]| {
            let mut body = element.as_bytes().to_vec();
            body.push(0);
            for n in [start, start + 1000, u32::MAX, u32::MAX] {
                body.extend_from_slice(&n.to_be_bytes());
            }
            body.extend(frame(b"TIT2", title));
            frame(b"CHAP", &body)
        };
        let mut frames = frame(b"TIT2", b"\x03die folge");
        frames.extend(chap("ch1", 754_500, b"\x03zweites"));
        frames.extend(chap("ch0", 0, &[1, 0xff, 0xfe, b'I', 0, b'n', 0, 0, 0]));
        frames.extend([0; 32]);
        let mut tag = b"ID3\x03\x00\x00".to_vec();
        let size = frames.len();
        tag.extend([(size >> 21) as u8 & 0x7f, (size >> 14) as u8 & 0x7f, (size >> 7) as u8 & 0x7f, size as u8 & 0x7f]);
        tag.extend(frames);
        tag.extend([0xff, 0xfb, 0x90, 0x00]);
        let chapters = chapters_in_id3(&tag);
        assert_eq!(chapters, vec![Chapter { start: 0.0, title: "In".into() }, Chapter { start: 754.5, title: "zweites".into() }]);
        assert!(chapters_in_id3(b"not a tag at all").is_empty());
    }

    const FEED: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:itunes="http://www.itunes.com/dtds/podcast-1.0.dtd">
<channel>
  <title>Kessler &amp; Freunde</title>
  <itunes:author>Radio Rauschen</itunes:author>
  <itunes:owner><itunes:name>Rauschen Netzwerk</itunes:name><itunes:email>a@example.org</itunes:email></itunes:owner>
  <description><![CDATA[<p>Über <b>Musik</b>.</p><p>Jeden Monat.</p>]]></description>
  <itunes:image href="https://example.org/show.jpg"/>
  <item>
    <title>Folge 2: Harmonia</title>
    <guid isPermaLink="false">ep-2</guid>
    <pubDate>Tue, 01 Oct 2026 10:00:00 +0000</pubDate>
    <itunes:duration>1:02:03</itunes:duration>
    <enclosure url="https://example.org/2.mp3?a=1&amp;b=2" length="1234" type="audio/mpeg" />
    <description>&lt;p&gt;Kluster, Cluster &amp;amp; Eno&lt;/p&gt;</description>
    <psc:chapters version="1.2"><psc:chapter start="00:00:00.000" title="Anfang"/><psc:chapter start="01:30.5" title="Mitte"/></psc:chapters>
    <podcast:chapters url="https://example.org/2.json" type="application/json+chapters"/>
  </item>
  <item>
    <title><![CDATA[Folge 1]]></title>
    <itunes:duration>754</itunes:duration>
    <enclosure url='https://example.org/1.mp3' type='audio/mpeg'/>
  </item>
  <item>
    <title>Ohne Ton</title>
  </item>
  <item>
    <title>Nur Bild</title>
    <enclosure url="https://example.org/3.mp4" type="video/mp4"/>
  </item>
  <item>
    <title>Bild und Ton</title>
    <itunes:image href="https://example.org/4.jpg"/>
    <media:content medium="image" url="https://example.org/4-unsized.jpg"/>
    <media:thumbnail height="720" width="1280" url="https://example.org/4-wide.jpg"/>
    <media:thumbnail height="300" width="300" url="https://example.org/4-square.jpg"/>
    <enclosure url="https://example.org/4.m4v"/>
    <enclosure url="https://example.org/4.m4a" type="audio/mp4"/>
  </item>
</channel>
</rss>"#;

    /// The one behind a podcast is said only where it is someone else than
    /// who makes it.
    #[test]
    fn the_publisher_is_someone_else() {
        let mut feed = Feed { title: "The Ezra Klein Show".into(), author: "New York Times Opinion".into(), ..Feed::default() };
        assert_eq!(feed.publisher(), None, "the author is the makers line already");
        feed.people = vec!["Ezra Klein".into()];
        assert_eq!(feed.publisher().as_deref(), Some("New York Times Opinion"));
        let alone = Feed { title: "Team Human".into(), author: "Douglas Rushkoff".into(), owner: "Douglas Rushkoff".into(), ..Feed::default() };
        assert_eq!(alone.publisher(), None);
        let npr = Feed { title: "Machine Gods".into(), author: "Some Host".into(), owner: "NPR".into(), ..Feed::default() };
        assert_eq!(npr.publisher().as_deref(), Some("NPR"));
    }

    #[test]
    fn html_entities_are_read_in_notes() {
        assert_eq!(plain("we love hearing from you, write to&nbsp;vergecast@theverge.com"), "we love hearing from you, write to vergecast@theverge.com");
        assert_eq!(plain("&bdquo;so&ldquo; &ndash; gut&hellip; f&uuml;r&nbsp;&nbsp;alle"), "„so“ – gut… für alle");
        assert_eq!(plain("Silben&shy;trennung"), "Silbentrennung");
        assert_eq!(plain("&unbekannt; bleibt"), "&unbekannt; bleibt");
    }

    #[test]
    fn a_feed_is_read() {
        let feed = read(FEED).expect("a channel");
        assert_eq!(feed.title, "Kessler & Freunde");
        assert_eq!(feed.author, "Radio Rauschen");
        assert_eq!(feed.owner, "Rauschen Netzwerk");
        assert_eq!(feed.publisher().as_deref(), Some("Rauschen Netzwerk"));
        assert_eq!(feed.about, "Über Musik.\nJeden Monat.");
        assert_eq!(feed.artwork, "https://example.org/show.jpg");
        assert_eq!(feed.episodes.len(), 3, "the ones without sound are left out, the video's too");
        let both = &feed.episodes[2];
        assert_eq!(both.url, "https://example.org/4.m4a", "of video and sound, the sound");
        assert_eq!(both.artwork, "https://example.org/4.jpg", "with its own picture");
        assert_eq!(both.wide_artwork, "https://example.org/4-wide.jpg", "and a wider one beside it, not the square, before one without a size");
        assert!(feed.episodes[0].artwork.is_empty());
        assert!(feed.episodes[0].wide_artwork.is_empty());
        let two = &feed.episodes[0];
        assert_eq!(two.title, "Folge 2: Harmonia");
        assert_eq!(two.id, "ep-2");
        assert_eq!(two.url, "https://example.org/2.mp3?a=1&b=2");
        assert_eq!(two.bytes, 1234);
        assert_eq!(two.seconds, Some(3723));
        assert_eq!(two.about, "Kluster, Cluster & Eno", "escaped html, read and taken out");
        assert_eq!(two.chapters, vec![Chapter { start: 0.0, title: "Anfang".into() }, Chapter { start: 90.5, title: "Mitte".into() }]);
        assert_eq!(two.chapters_url, "https://example.org/2.json");
        let one = &feed.episodes[1];
        assert_eq!(one.title, "Folge 1");
        assert_eq!(one.id, "https://example.org/1.mp3", "no guid: its address");
        assert_eq!(one.seconds, Some(754));
    }

    #[test]
    fn a_chapters_file_is_read() {
        let text = r#"{"version":"1.2.0","chapters":[
            {"startTime":61.5,"title":"Zwei"},
            {"startTime":0,"title":"Eins"},
            {"startTime":30,"title":"Versteckt","toc":false},
            {"startTime":90}
        ]}"#;
        assert_eq!(chapters_in(text), vec![Chapter { start: 0.0, title: "Eins".into() }, Chapter { start: 61.5, title: "Zwei".into() }]);
        assert!(chapters_in("nope").is_empty());
    }

    #[test]
    fn nothing_is_no_feed() {
        assert!(read("<html><body>404</body></html>").is_none());
        assert!(read("").is_none());
    }

    /// The newest are held, and the begun; the rest are counted.
    #[test]
    fn only_the_newest_are_held() {
        let episodes = (0..KEPT as i64 + 5).map(|i| Episode { id: format!("e{i}"), at: i, ..Default::default() }).collect();
        let mut feed = Feed { episodes, ..Default::default() };
        feed.keep_newest(|e| e.id == "e0");
        assert_eq!(feed.episodes.len(), KEPT + 1);
        assert_eq!(feed.older, 4);
        assert!(feed.episodes.iter().any(|e| e.id == "e0"), "the begun stays");
        assert!(!feed.episodes.iter().any(|e| e.id == "e1"));
    }

    /// A series says so, and its episodes' places; a podcast its support.
    #[test]
    fn series_and_support_are_read() {
        let xml = r#"<rss><channel><title>t</title><itunes:type>serial</itunes:type>
<podcast:funding url="https://steady.page/x">Unterstütze uns</podcast:funding>
<item><title>a</title><guid>a</guid><enclosure url="https://a.mp3" type="audio/mpeg"/><itunes:season>2</itunes:season><itunes:episode>4</itunes:episode></item>
</channel></rss>"#;
        let feed = read(xml).expect("a feed");
        assert!(feed.serial);
        assert_eq!(feed.funding, vec![("Unterstütze uns".to_string(), "https://steady.page/x".to_string())]);
        assert_eq!((feed.episodes[0].season, feed.episodes[0].number), (Some(2), Some(4)));
    }
}
