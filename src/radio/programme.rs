use std::sync::mpsc;

use chrono::{DateTime, Local};

use crate::json::Json;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Show {
    pub title: String,
    pub start: DateTime<Local>,
    pub end: DateTime<Local>,
    pub picture: String,
    pub about: String,
    pub links: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Nts(u8),
    Montez(&'static str),
    Kzsu,
    Platform(super::platform::Platform),
}

const MONTEZ_STREAMS: [(&str, &str); 4] = [
    ("language, art", "reading-and-poetry-the-sounded-word"),
    ("music", "music"),
    ("talk", "talk"),
    ("recent", "recent"),
];

const MONTEZ_MAIN: &str = "default";

impl Source {
    pub fn of(url: &str, name: &str) -> Option<Source> {
        let url = url.to_ascii_lowercase();
        let name = name.to_lowercase();
        if url.contains("montezpress") || name.contains("montez") {
            let path = url.rsplit('/').next().unwrap_or("");
            let slug = MONTEZ_STREAMS
                .iter()
                .find(|(said, _)| name.contains(said) || path.contains(said.split(',').next().unwrap_or(said)))
                .map_or(MONTEZ_MAIN, |(_, slug)| slug);
            return Some(Source::Montez(slug));
        }
        if url.contains("kzsu") || name.contains("kzsu") {
            return Some(Source::Kzsu);
        }
        if !url.contains("ntslive.net") {
            return None;
        }
        let path = url.split(['?', '#']).next().unwrap_or("");
        Some(if path.trim_end_matches('/').ends_with('2') { Source::Nts(2) } else { Source::Nts(1) })
    }
}

fn montez_endpoint(slug: &str) -> String {
    let pattern = std::env::var("KESSLER_MONTEZ_ENDPOINT").unwrap_or_else(|_| MONTEZ_ENDPOINT.to_string());
    pattern.replace("{}", slug)
}

const MONTEZ_ENDPOINT: &str = "https://radio.montezpress.com/api/get-stream/{}";

fn montez_schedule() -> String {
    std::env::var("KESSLER_MONTEZ_SCHEDULE").unwrap_or_else(|_| "https://radio.montezpress.com/api/get-live-items".to_string())
}

fn kzsu_endpoint() -> String {
    std::env::var("KESSLER_KZSU_ENDPOINT").unwrap_or_else(|_| "https://kzsu.stanford.edu/api/shows/upcoming/5/".to_string())
}

fn nts_endpoint() -> String {
    std::env::var("KESSLER_NTS_ENDPOINT").unwrap_or_else(|_| "https://www.nts.live/api/v2/live".to_string())
}

pub fn ask(source: Source) -> mpsc::Receiver<Result<Vec<Show>, String>> {
    let (tell, answer) = mpsc::channel();
    std::thread::Builder::new()
        .name("kessler-radio-programme".into())
        .spawn(move || {
            let shows = match source {
                Source::Nts(channel) => crate::library::coverart::fetch_text(&nts_endpoint())
                    .map_err(|e| e.to_string())
                    .and_then(|text| Json::parse(&text).ok_or_else(|| "nts: unreadable".to_string()))
                    .map(|doc| nts_shows(&doc, channel)),
                Source::Platform(platform) => super::platform::shows(&platform),
                Source::Kzsu => crate::library::coverart::fetch_text(&kzsu_endpoint())
                    .map_err(|e| e.to_string())
                    .and_then(|text| Json::parse(&text).ok_or_else(|| "kzsu: unreadable".to_string()))
                    .map(|doc| kzsu_shows(&doc, Local::now())),
                Source::Montez(slug) => {
                    let read = |url: &str| crate::library::coverart::fetch_text(url).ok().and_then(|t| Json::parse(&t));
                    let live = read(&montez_schedule());
                    let mut shows = live.as_ref().map(|doc| montez_shows(doc, slug)).unwrap_or_default();
                    let mut asked = live.is_some();
                    if shows.is_empty() {
                        let own = read(&montez_endpoint(slug));
                        asked |= own.is_some();
                        shows = own.map(|doc| montez_shows(&doc, slug)).unwrap_or_default();
                    }
                    if slug == MONTEZ_MAIN {
                        let after = shows.last().map_or_else(Local::now, |s| s.end);
                        shows.extend(live.map(|doc| montez_live(&doc)).unwrap_or_default().into_iter().filter(|s| s.start >= after));
                    }
                    if asked { Ok(shows) } else { Err("montez: not reached".to_string()) }
                }
            };
            let _ = tell.send(shows);
        })
        .ok();
    answer
}

fn nts_shows(doc: &Json, channel: u8) -> Vec<Show> {
    let wanted = channel.to_string();
    let Some(entry) = doc.list("results").iter().find(|r| r.text("channel_name") == Some(wanted.as_str())) else {
        return Vec::new();
    };
    let keys = ["now".to_string(), "next".to_string()].into_iter().chain((2..=24).map(|n| format!("next{n}")));
    let mut shows = Vec::new();
    for key in keys {
        let Some(show) = entry.get(&key) else { break };
        let time = |k: &str| {
            show.text(k)
                .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
                .map(|t| t.with_timezone(&Local))
        };
        let (Some(start), Some(end)) = (time("start_timestamp"), time("end_timestamp")) else { continue };
        let title = show
            .text("broadcast_title")
            .or_else(|| show.get("embeds").and_then(|e| e.get("details")).and_then(|d| d.text("name")))
            .unwrap_or("")
            .trim();
        if title.is_empty() {
            continue;
        }
        let media = show.get("embeds").and_then(|e| e.get("details")).and_then(|d| d.get("media"));
        let picture = ["picture_large", "picture_medium_large", "picture_medium", "background_large"]
            .iter()
            .find_map(|key| media.and_then(|m| m.text(key)).filter(|u| u.starts_with("http")))
            .unwrap_or("")
            .to_string();
        let details = show.get("embeds").and_then(|e| e.get("details"));
        let html = details.and_then(|d| d.text("description_html").or_else(|| d.text("description"))).unwrap_or("");
        let mut links = links_in(html);
        for listed in details.and_then(|d| d.get("external_links")).and_then(|l| l.as_array()).unwrap_or(&[]) {
            let url = listed.as_str().or_else(|| listed.text("url")).unwrap_or("").trim();
            if url.starts_with("http") && !links.iter().any(|(_, u)| u == url) {
                links.push((site_word(url), url.to_string()));
            }
        }
        let about = details
            .and_then(|d| d.text("description").or_else(|| d.text("description_html")))
            .map(|d| without_link_lines(&unescape(&strip_tags(d)), &links))
            .unwrap_or_default();
        shows.push(Show { title: unescape(title), start, end, picture, about, links });
    }
    shows
}

fn montez_shows(doc: &Json, slug: &str) -> Vec<Show> {
    let stream = if slug == MONTEZ_MAIN {
        doc.get("playlist")
    } else {
        doc.list("streams").iter().find(|s| s.text("slug") == Some(slug))
    };
    let doc = stream.unwrap_or(doc);
    let when = |item: &Json| {
        let at = item.text("last_played")?;
        let at = chrono::NaiveDateTime::parse_from_str(at, "%Y-%m-%dT%H:%M:%S%.f").ok()?;
        Some(chrono::TimeZone::from_utc_datetime(&chrono::Utc, &at).with_timezone(&Local))
    };
    let mut shows: Vec<(bool, Show)> = Vec::new();
    for item in doc.list("items") {
        let (Some(start), Some(length)) = (when(item), item.get("duration").and_then(|d| d.as_number())) else { continue };
        let end = start + chrono::Duration::seconds(length.max(0.0) as i64);
        let artist = item.text("artist").map(str::trim).filter(|a| !a.is_empty());
        let title = item.text("title").map(str::trim).filter(|t| !t.is_empty());
        let Some(name) = title.or(artist) else { continue };
        let place = [item.text("venue"), item.text("city")].into_iter().flatten().map(str::trim).filter(|p| !p.is_empty()).collect::<Vec<_>>().join(", ");
        let about = [artist.filter(|_| title.is_some()).unwrap_or(""), place.as_str()].into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join("\n");
        let picture = ["image", "thumbnail", "small_thumbnail"].iter().find_map(|k| item.text(k)).filter(|u| u.starts_with("http")).unwrap_or("").to_string();
        let playing = matches!(item.get("playing"), Some(Json::Bool(true)));
        shows.push((playing, Show { title: unescape(name), start, end, picture, about, links: Vec::new() }));
    }
    let Some(now) = shows.iter().find(|(playing, _)| *playing).map(|(_, s)| s.start) else { return Vec::new() };
    let mut shows: Vec<Show> = shows.into_iter().map(|(_, s)| s).filter(|s| s.start <= now).collect();
    shows.sort_by_key(|s| s.start);
    shows
}

fn kzsu_shows(doc: &Json, now: DateTime<Local>) -> Vec<Show> {
    let mut shows = Vec::new();
    let words = |item: &Json, keys: &[&str]| {
        keys.iter().filter_map(|k| item.text(k)).map(str::trim).filter(|w| !w.is_empty()).map(unescape).collect::<Vec<_>>().join("\n")
    };
    if let Some(on) = doc.get("onnow").or(Some(doc).filter(|d| d.get("show_end").is_some())) {
        let end = on.text("show_end").and_then(|t| chrono::NaiveDateTime::parse_from_str(t, "%Y-%m-%d %H:%M:%S").ok()).and_then(pacific);
        if let (Some(title), Some(end)) = (on.text("name").map(str::trim).filter(|t| !t.is_empty()), end) {
            let start = (end - chrono::Duration::hours(1)).min(now);
            let picture = on.text("image_url").filter(|u| u.starts_with("http")).unwrap_or("").to_string();
            shows.push(Show { title: unescape(title), start, end, picture, about: words(on, &["airname"]), links: Vec::new() });
        }
    }
    for item in doc.list("upcoming") {
        let Some(day) = item.text("date").and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()) else { continue };
        let Some(at) = item.text("start_time").and_then(|t| t.trim().parse::<i64>().ok()) else { continue };
        let Some(title) = item.text("title").map(str::trim).filter(|t| !t.is_empty()) else { continue };
        let begins = day.and_hms_opt(0, 0, 0).map(|d| d + chrono::Duration::minutes(at / 100 * 60 + at % 100));
        let Some(start) = begins.and_then(pacific) else { continue };
        let minutes = item.get("duration").and_then(|d| d.as_number()).unwrap_or(60.0).max(1.0) as i64;
        let picture = item.text("show_image_url").filter(|u| u.starts_with("http")).unwrap_or("").to_string();
        let about = words(item, &["dj_name", "short_description", "long_description"]);
        shows.push(Show { title: unescape(title), start, end: start + chrono::Duration::minutes(minutes), picture, about, links: Vec::new() });
    }
    shows.sort_by_key(|s| s.start);
    shows
}

#[cfg(test)]
pub(super) fn kzsu_titles(doc: &Json) -> Vec<String> {
    kzsu_shows(doc, Local::now()).into_iter().map(|s| s.title).collect()
}

fn pacific(at: chrono::NaiveDateTime) -> Option<DateTime<Local>> {
    use chrono::Datelike;
    let sunday = |month: u32, nth: u32| {
        let first = chrono::NaiveDate::from_ymd_opt(at.year(), month, 1)?;
        let to_sunday = (7 - first.weekday().num_days_from_sunday()) % 7;
        first.checked_add_days(chrono::Days::new(u64::from(to_sunday + 7 * (nth - 1))))?.and_hms_opt(2, 0, 0)
    };
    let summer = at >= sunday(3, 2)? && at < sunday(11, 1)?;
    let zone = chrono::FixedOffset::west_opt(if summer { 7 } else { 8 } * 3600)?;
    Some(chrono::TimeZone::from_local_datetime(&zone, &at).single()?.with_timezone(&Local))
}

fn montez_live(doc: &Json) -> Vec<Show> {
    let mut shows: Vec<Show> = Vec::new();
    for item in doc.list("schedule_list") {
        let Some(at) = item.text("start_time").and_then(|t| chrono::NaiveDateTime::parse_from_str(t, "%Y-%m-%dT%H:%M:%S").ok()) else { continue };
        let Some(zone) = item.text("timezone").and_then(utc_offset) else { continue };
        let Some(start) = chrono::TimeZone::from_local_datetime(&zone, &at).single() else { continue };
        let Some(title) = item.text("title").map(str::trim).filter(|t| !t.is_empty()) else { continue };
        let description = item.text("show_description").unwrap_or("");
        let links = links_in(description);
        let place = [item.text("venue"), item.text("city")].into_iter().flatten().map(str::trim).filter(|p| !p.is_empty()).collect::<Vec<_>>().join(", ");
        let told = without_link_lines(unescape(&strip_tags(&description.replace("<br>", "\n"))).trim(), &links);
        let about = [item.text("artist").map(str::trim).unwrap_or(""), place.as_str(), told.trim()]
            .into_iter()
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        let picture = ["image", "thumbnail", "small_thumbnail"].iter().find_map(|k| item.text(k)).filter(|u| u.starts_with("http")).unwrap_or("").to_string();
        let start = start.with_timezone(&Local);
        shows.push(Show { title: unescape(title), start, end: start + chrono::Duration::hours(1), picture, about, links });
    }
    shows.sort_by_key(|s| s.start);
    for i in 1..shows.len() {
        let next = shows[i].start;
        if shows[i - 1].end > next {
            shows[i - 1].end = next;
        }
    }
    shows
}

fn utc_offset(zone: &str) -> Option<chrono::FixedOffset> {
    let rest = zone.trim().strip_prefix("UTC")?;
    if rest.is_empty() {
        return chrono::FixedOffset::east_opt(0);
    }
    let (sign, rest) = match rest.as_bytes()[0] {
        b'+' => (1, &rest[1..]),
        b'-' => (-1, &rest[1..]),
        _ => return None,
    };
    let (hours, minutes) = rest.split_once(':').unwrap_or((rest, "0"));
    let seconds = hours.parse::<i32>().ok()? * 3600 + minutes.parse::<i32>().ok()? * 60;
    chrono::FixedOffset::east_opt(sign * seconds)
}

fn links_in(html: &str) -> Vec<(String, String)> {
    let mut links: Vec<(String, String)> = Vec::new();
    let mut rest = html;
    while let Some(at) = rest.find("<a ") {
        rest = &rest[at..];
        let Some(close) = rest.find('>') else { break };
        let tag = &rest[..close];
        let url = tag
            .split("href=")
            .nth(1)
            .map(|v| v.trim_start_matches(['"', '\'']))
            .and_then(|v| v.split(['"', '\'', ' ']).next())
            .unwrap_or("")
            .to_string();
        let after = &rest[close + 1..];
        let words = after.split("</a>").next().unwrap_or("");
        let words = unescape(&strip_tags(words)).trim().to_string();
        rest = after;
        let url = unescape(&url);
        if url.starts_with("http") && !links.iter().any(|(_, u)| *u == url) {
            let words = if words.is_empty() || words.starts_with("http") { site_word(&url) } else { words };
            links.push((words, url));
        }
    }
    links
}

pub(super) fn site_word(url: &str) -> String {
    let host = url.split("//").nth(1).unwrap_or(url).split(['/', '?', '#']).next().unwrap_or("");
    let host = host.trim_start_matches("www.").trim_start_matches("m.");
    let parts: Vec<&str> = host.split('.').collect();
    match parts.len() {
        0 | 1 => host.to_string(),
        n => parts[n - 2].to_string(),
    }
}

fn without_link_lines(text: &str, links: &[(String, String)]) -> String {
    const ONLY_A_PLACE: [&str; 10] =
        ["soundcloud", "youtube", "mixcloud", "bandcamp", "instagram", "facebook", "twitter", "spotify", "website", "x"];
    text.lines()
        .filter(|line| {
            let line = line.trim().to_lowercase();
            !(links.iter().any(|(words, _)| words.to_lowercase() == line) || ONLY_A_PLACE.contains(&line.as_str()))
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn strip_tags(text: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for c in text.chars() {
        match c {
            '<' => inside = true,
            '>' => inside = false,
            c if !inside => out.push(c),
            _ => {}
        }
    }
    out
}

fn unescape(title: &str) -> String {
    title
        .replace("&amp;", "&")
        .replace("&#39;", "'")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

pub fn picture_path(url: &str) -> Option<std::path::PathBuf> {
    if url.is_empty() {
        return None;
    }
    let name = crate::lastfm::sign::md5_hex(url.as_bytes());
    Some(crate::config::Config::config_dir().join("radio-shows").join(name))
}

fn fetch_picture(url: &str) {
    let Some(path) = picture_path(url) else { return };
    if path.exists() {
        return;
    }
    let url = url.to_string();
    std::thread::Builder::new()
        .name("kessler-radio-show-picture".into())
        .spawn(move || {
            if let Ok(bytes) = crate::library::coverart::fetch(&url) {
                if image::guess_format(&bytes).is_ok() {
                    if let Some(dir) = path.parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    let _ = std::fs::write(&path, bytes);
                }
            }
        })
        .ok();
}

#[derive(Debug, Default)]
pub struct Programme {
    pub url: String,
    pub past: Vec<Show>,
    pub coming: Vec<Show>,
    asking: Option<mpsc::Receiver<Result<Vec<Show>, String>>>,
    asked_at: Option<std::time::Instant>,
}

const PAST_KEPT: usize = 3;
const ASK_AGAIN_SOON: std::time::Duration = std::time::Duration::from_secs(60);
const ASK_AGAIN_LATE: std::time::Duration = std::time::Duration::from_secs(15 * 60);

impl Programme {
    pub fn for_station(url: &str) -> Self {
        Programme { url: url.to_string(), ..Programme::default() }
    }

    pub fn now(&self, at: DateTime<Local>) -> Option<&Show> {
        self.coming.first().filter(|s| s.start <= at && at < s.end)
    }

    pub fn tend(&mut self, source: Source) -> bool {
        let mut changed = false;
        if let Some(asking) = &self.asking {
            match asking.try_recv() {
                Ok(Ok(shows)) => {
                    self.take(shows);
                    self.asking = None;
                    changed = true;
                }
                Ok(Err(_)) | Err(mpsc::TryRecvError::Disconnected) => self.asking = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        let now = Local::now();
        while self.coming.first().is_some_and(|s| s.end <= now) {
            let done = self.coming.remove(0);
            self.went_by(done);
            changed = true;
        }
        let due = match self.asked_at {
            None => true,
            Some(at) => {
                let ended = self.coming.first().is_none_or(|s| s.start > now);
                let asked = now - chrono::Duration::from_std(at.elapsed()).unwrap_or_default();
                let begun = self.coming.first().is_some_and(|s| s.start > asked && s.start <= now);
                ((ended || begun) && at.elapsed() >= ASK_AGAIN_SOON) || at.elapsed() >= ASK_AGAIN_LATE
            }
        };
        if due && self.asking.is_none() {
            self.asking = Some(ask(source));
            self.asked_at = Some(std::time::Instant::now());
        }
        changed
    }

    pub fn waiting(&self) -> bool {
        self.asking.is_some() && self.coming.is_empty() && self.past.is_empty()
    }

    fn take(&mut self, shows: Vec<Show>) {
        let now = Local::now();
        for show in shows.iter().filter(|s| s.end <= now) {
            self.went_by(show.clone());
        }
        let first = shows.first().map(|s| s.start);
        let before: Vec<Show> = std::mem::take(&mut self.coming)
            .into_iter()
            .filter(|s| first.is_none_or(|f| s.start < f) && s.start <= now)
            .collect();
        for show in before {
            self.went_by(show);
        }
        self.coming = shows.into_iter().filter(|s| s.end > now).collect();
        for show in self.coming.iter().take(2) {
            fetch_picture(&show.picture);
        }
    }

    pub fn links_now(&self) -> Vec<(String, String)> {
        self.now(Local::now()).map(|s| s.links.clone()).unwrap_or_default()
    }

    pub fn about_now(&self) -> Option<&str> {
        self.now(Local::now()).map(|s| s.about.as_str()).filter(|a| !a.is_empty())
    }

    pub fn picture_now(&self) -> Option<std::path::PathBuf> {
        let show = self.now(Local::now())?;
        let path = picture_path(&show.picture)?;
        image::ImageReader::open(&path).ok()?.with_guessed_format().ok()?.format().map(|_| path)
    }

    fn went_by(&mut self, show: Show) {
        if !self.past.iter().any(|p| p.start == show.start && p.title == show.title) {
            self.past.push(show);
        }
        let over = self.past.len().saturating_sub(PAST_KEPT);
        self.past.drain(..over);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nts_names_its_channels_by_their_streams() {
        assert_eq!(Source::of("https://stream-relay-geo.ntslive.net/stream?client=direct", "nts 1"), Some(Source::Nts(1)));
        assert_eq!(Source::of("https://stream-relay-geo.ntslive.net/stream2?client=direct", "nts 2"), Some(Source::Nts(2)));
        assert_eq!(Source::of("https://radio.example/live", "radio"), None);
        assert_eq!(
            Source::of("https://stream.montezpress.com/las", "Language, Art & Sound | Montez Press Radio"),
            Some(Source::Montez("reading-and-poetry-the-sounded-word"))
        );
        assert_eq!(Source::of("https://stream.montezpress.com/music", "Music | Montez Press Radio"), Some(Source::Montez("music")));
        assert_eq!(Source::of("https://stream.montezpress.com/radio", "Montez Press Radio"), Some(Source::Montez("default")));
    }

    #[test]
    fn nts_live_gives_now_and_next_in_order() {
        let doc = Json::parse(
            r#"{"results":[
              {"channel_name":"1","now":{"broadcast_title":"Breakfast w/ Flo","start_timestamp":"2026-10-21T08:00:00Z","end_timestamp":"2026-10-21T10:00:00Z",
                 "embeds":{"details":{"description":"Flo &amp; friends, <b>early</b> and gentle.","media":{"picture_small":"https://media.example/s.jpg","picture_large":"https://media.example/l.jpg"}}}},
               "next":{"broadcast_title":"Charlie Bones","start_timestamp":"2026-10-21T10:00:00Z","end_timestamp":"2026-10-21T12:00:00Z"},
               "next2":{"broadcast_title":"Tom &amp; Jerry","start_timestamp":"2026-10-21T12:00:00Z","end_timestamp":"2026-10-21T13:00:00Z"}},
              {"channel_name":"2","now":{"broadcast_title":"Other","start_timestamp":"2026-10-21T08:00:00Z","end_timestamp":"2026-10-21T09:00:00Z"}}
            ]}"#,
        )
        .unwrap();
        let shows = nts_shows(&doc, 1);
        let titles: Vec<&str> = shows.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["Breakfast w/ Flo", "Charlie Bones", "Tom & Jerry"]);
        assert_eq!((shows[0].end - shows[0].start).num_hours(), 2);
        assert_eq!(shows[0].picture, "https://media.example/l.jpg");
        assert_eq!(shows[1].picture, "");
        assert_eq!(shows[0].about, "Flo & friends, early and gentle.");
        assert_eq!(nts_shows(&doc, 2).len(), 1);
    }

    #[test]
    fn a_show_that_ended_goes_to_the_past() {
        let at = |h: i64| Local::now() + chrono::Duration::hours(h);
        let show = |t: &str, a: i64, b: i64| Show { title: t.into(), start: at(a), end: at(b), picture: String::new(), about: String::new(), links: Vec::new() };
        let mut programme = Programme::for_station("x");
        programme.take(vec![show("before", -2, -1), show("now", -1, 1), show("after", 1, 2)]);
        assert_eq!(programme.coming.len(), 2, "what is over is not to come");
        assert_eq!(programme.now(Local::now()).map(|s| s.title.as_str()), Some("now"));
        programme.take(vec![show("after", 1, 2)]);
        assert_eq!(programme.past.iter().map(|s| s.title.as_str()).collect::<Vec<_>>(), ["before", "now"]);
    }

    #[test]
    fn a_shows_links_stand_apart_from_its_words() {
        let html = r#"<p>Athenian Marketplace takes the reigns of a 2 hour monthly slot.</p><p><a href="https://soundcloud.com/athenian">Soundcloud</a><br><a href='https://www.youtube.com/@athenian'>YouTube</a></p>"#;
        let links = links_in(html);
        assert_eq!(
            links,
            [
                ("Soundcloud".to_string(), "https://soundcloud.com/athenian".to_string()),
                ("YouTube".to_string(), "https://www.youtube.com/@athenian".to_string()),
            ]
        );
        let plain = "Athenian Marketplace takes the reigns of a 2 hour monthly slot.\nSoundcloud\nYoutube\nInstagram";
        assert_eq!(without_link_lines(plain, &links), "Athenian Marketplace takes the reigns of a 2 hour monthly slot.");
        assert_eq!(site_word("https://m.soundcloud.com/x"), "soundcloud");
    }

    #[test]
    fn a_montez_stream_says_what_plays_and_what_played() {
        let doc = Json::parse(
            r#"{"type":"stream","slug":"reading-and-poetry-the-sounded-word","items":[
              {"pk":3938,"title":null,"artist":"Martha Skye Murphy","duration":3447,"image":"https://s3/a.jpg","venue":"46 Canal St","city":"New York","last_played":"2026-10-21T23:35:39.133","playing":false},
              {"pk":4067,"title":"Finding the end of the rainbow","artist":"Natalia Panzer","duration":1784,"image":"https://s3/b.jpg","venue":"151 Lafayette Street","city":"New York","last_played":"2026-10-22T00:33:06.884","playing":false},
              {"pk":4008,"title":"Black History Walks","artist":null,"duration":2727,"image":"https://s3/c.jpg","venue":"Forma HQ","city":"London","last_played":"2026-10-22T01:10:30.012","playing":true},
              {"pk":3972,"title":"free school","artist":"Ariel Yelen","duration":3732,"image":"https://s3/d.jpg","venue":"Red Hook Record Shop","city":"New York","last_played":"2026-10-21T01:44:45.160","playing":false},
              {"pk":1,"title":"never played","artist":"x","duration":60,"last_played":null,"playing":false}
            ]}"#,
        )
        .unwrap();
        let shows = montez_shows(&doc, "reading-and-poetry-the-sounded-word");
        let titles: Vec<&str> = shows.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["free school", "Martha Skye Murphy", "Finding the end of the rainbow", "Black History Walks"]);
        let now = shows.last().unwrap();
        assert_eq!((now.end - now.start).num_seconds(), 2727);
        assert_eq!(now.picture, "https://s3/c.jpg");
        assert_eq!(now.about, "Forma HQ, London");
        assert_eq!(shows[2].about, "Natalia Panzer\n151 Lafayette Street, New York");
        assert_eq!(now.start.with_timezone(&chrono::Utc).format("%H:%M").to_string(), "01:10");
    }

    #[test]
    fn every_montez_stream_comes_in_one_answer() {
        let doc = Json::parse(
            r#"{"streams":[
                {"slug":"music","items":[
                  {"title":"En vivo desde la Capital","artist":"Figura.03","duration":3099,"venue":"WIG","city":"Mexico City","last_played":"2026-10-22T01:35:47.974","playing":true},
                  {"title":null,"artist":"Embaci","duration":2187,"last_played":"2026-10-21T23:21:01.203","playing":false}]},
                {"slug":"talk","items":[
                  {"title":"Ulysses Part 2","artist":"Montez Press","duration":8536,"last_played":"2026-10-22T00:32:12.927","playing":true}]}],
              "playlist":{"slug":"default","items":[
                  {"title":"GP","artist":"Caribic Care","duration":3600,"last_played":"2026-10-22T00:48:58.831","playing":true},
                  {"title":"never yet","artist":"x","duration":60,"last_played":null,"playing":false}]}}"#,
        )
        .unwrap();
        let names = |slug| montez_shows(&doc, slug).into_iter().map(|s| s.title).collect::<Vec<_>>();
        assert_eq!(names("music"), ["Embaci", "En vivo desde la Capital"]);
        assert_eq!(names("talk"), ["Ulysses Part 2"]);
        assert_eq!(names("default"), ["GP"]);
        assert!(names("recent").is_empty());
    }

    #[test]
    fn montez_lists_its_live_broadcasts_in_their_own_time() {
        let doc = Json::parse(
            r#"{"schedule_list":[
                {"start_time":"2026-10-22T18:00:00","timezone":"UTC+1","title":"WITHOUT CONSENT","artist":"The BREAK—DOWN","venue":"Close-Up Cinema","city":"London",
                 "show_description":"We gather to discuss.<br><br>\r\nBuy it <a href=\"https://example.org/book\">here</a>","image":"https://x/i.jpg"},
                {"start_time":"2026-10-21T18:00:00","timezone":"UTC-4","title":"Slap","artist":"Taylor Skye","city":"New York","show_description":""},
                {"start_time":"2026-10-21T19:00:00","timezone":"UTC-4","title":"Open Bag","artist":"Nobutaka Aozaki","show_description":""}]}"#,
        )
        .unwrap();
        let shows = montez_live(&doc);
        assert_eq!(shows.iter().map(|s| s.title.as_str()).collect::<Vec<_>>(), ["Slap", "Open Bag", "WITHOUT CONSENT"]);
        assert_eq!(shows[0].start.with_timezone(&chrono::Utc).to_rfc3339(), "2026-10-21T22:00:00+00:00");
        assert_eq!(shows[0].end, shows[1].start);
        assert_eq!(shows[2].start.with_timezone(&chrono::Utc).to_rfc3339(), "2026-10-22T17:00:00+00:00");
        assert_eq!(shows[2].end - shows[2].start, chrono::Duration::hours(1));
        assert!(shows[2].about.starts_with("The BREAK—DOWN\nClose-Up Cinema, London\nWe gather to discuss."), "{}", shows[2].about);
        assert_eq!(shows[2].links.len(), 1);
        assert_eq!(utc_offset("UTC+5:30"), chrono::FixedOffset::east_opt(19800));
        assert_eq!(utc_offset("GMT"), None);
    }

    #[test]
    fn kzsu_says_the_show_on_now_and_the_evenings() {
        let doc = Json::parse(
            r#"{"onnow":{"airname":"I. Robot","image_url":"https://x/c.jpg","name":"Zootopia","show_end":"2026-10-21 20:00:00","track_artist":"Future Islands","track_title":"North Star "},
               "upcoming":[
                 {"date":"2026-10-21","start_time":"2400","duration":360,"title":"Zootopia","dj_name":"Team KZSU","short_description":"The cream"},
                 {"date":"2026-10-21","start_time":"2000","duration":60,"title":"Tea Party","dj_name":"DJ Mojo","short_description":"very good vibes <3 ","show_image_url":""}]}"#,
        )
        .unwrap();
        let now = chrono::TimeZone::from_utc_datetime(&chrono::Utc, &chrono::NaiveDate::from_ymd_opt(2026, 10, 22).unwrap().and_hms_opt(1, 30, 0).unwrap()).with_timezone(&Local);
        let shows = kzsu_shows(&doc, now);
        let utc = |s: &Show| s.start.with_timezone(&chrono::Utc).format("%d %H:%M").to_string();
        assert_eq!(shows.iter().map(|s| s.title.as_str()).collect::<Vec<_>>(), ["Zootopia", "Tea Party", "Zootopia"]);
        assert_eq!(shows[0].end.with_timezone(&chrono::Utc).format("%d %H:%M").to_string(), "22 03:00");
        assert_eq!(utc(&shows[1]), "22 03:00");
        assert_eq!(utc(&shows[2]), "22 07:00");
        assert_eq!(shows[2].end - shows[2].start, chrono::Duration::hours(6));
        assert_eq!(shows[0].about, "I. Robot");
        assert_eq!(shows[1].about, "DJ Mojo\nvery good vibes <3");
        let winter = pacific(chrono::NaiveDate::from_ymd_opt(2026, 11, 2).unwrap().and_hms_opt(20, 0, 0).unwrap()).unwrap();
        assert_eq!(winter.with_timezone(&chrono::Utc).format("%d %H:%M").to_string(), "03 04:00");
        assert_eq!(Source::of("https://kzsu-streams.stanford.edu/kzsu-1-256.mp3", "KZSU Stanford"), Some(Source::Kzsu));
    }
}
