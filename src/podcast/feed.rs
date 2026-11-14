#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Episode {
    pub id: String,
    pub title: String,
    pub url: String,
    pub bytes: u64,
    pub published: String,
    pub seconds: Option<u64>,
    pub about: String,
    pub chapters: Vec<Chapter>,
    pub chapters_url: String,
    pub artwork: String,
    pub images: Vec<String>,
    pub links: Vec<(String, String)>,
    pub link: String,
    pub at: i64,
    pub season: Option<u32>,
    pub number: Option<u32>,
}

const IMAGES_MOST: usize = 6;

pub const LINKS_MOST: usize = 24;

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

fn is_video(url: &str, kind: &str) -> bool {
    let kind = kind.trim().to_ascii_lowercase();
    if !kind.is_empty() {
        return kind.starts_with("video/");
    }
    let bare = url.split(['?', '#']).next().unwrap_or(url).to_ascii_lowercase();
    bare.ends_with(".m4v") || bare.ends_with(".mov")
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Chapter {
    pub start: f64,
    pub title: String,
}

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

fn time_of(text: &str) -> Option<f64> {
    let parts: Vec<f64> = text.trim().split(':').map(|p| p.trim().parse::<f64>().ok()).collect::<Option<_>>()?;
    Some(parts.iter().fold(0.0, |sum, p| sum * 60.0 + p))
}

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

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Feed {
    pub title: String,
    pub author: String,
    pub about: String,
    pub artwork: String,
    pub link: String,
    pub people: Vec<String>,
    pub about_links: Vec<(String, String)>,
    pub episodes: Vec<Episode>,
    pub older: usize,
    pub serial: bool,
    pub funding: Vec<(String, String)>,
    pub new_url: String,
}

pub const KEPT: usize = 30;

impl Feed {
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

#[derive(Debug, PartialEq)]
enum Piece {
    Open(String, Vec<(String, String)>),
    Close(String),
    Text(String),
}

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
    let out = unescape(&out);
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

pub fn read(xml: &str) -> Option<Feed> {
    let mut feed = Feed::default();
    let mut seen_channel = false;
    let mut open: Vec<String> = Vec::new();
    let mut episode: Option<Episode> = None;
    let mut guid = String::new();
    for piece in pieces(xml) {
        match piece {
            Piece::Open(name, attrs) => {
                let attr = |key: &str| attrs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()).unwrap_or_default();
                match name.as_str() {
                    "channel" => seen_channel = true,
                    "item" => {
                        episode = Some(Episode::default());
                        guid.clear();
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
    <enclosure url="https://example.org/4.m4v"/>
    <enclosure url="https://example.org/4.m4a" type="audio/mp4"/>
  </item>
</channel>
</rss>"#;

    #[test]
    fn a_feed_is_read() {
        let feed = read(FEED).expect("a channel");
        assert_eq!(feed.title, "Kessler & Freunde");
        assert_eq!(feed.author, "Radio Rauschen");
        assert_eq!(feed.about, "Über Musik.\nJeden Monat.");
        assert_eq!(feed.artwork, "https://example.org/show.jpg");
        assert_eq!(feed.episodes.len(), 3, "the ones without sound are left out, the video's too");
        let both = &feed.episodes[2];
        assert_eq!(both.url, "https://example.org/4.m4a", "of video and sound, the sound");
        assert_eq!(both.artwork, "https://example.org/4.jpg", "with its own picture");
        assert!(feed.episodes[0].artwork.is_empty());
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
