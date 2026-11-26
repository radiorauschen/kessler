//! The radio.
//!
//! Asleep until it is switched on: nothing here is read, held or fetched
//! before the window opens the radio, and what it held is let go of when the
//! window goes back to the library. The stations kept are a small file of
//! their own beside the settings; a search asks the Radio Browser directory
//! (radio-browser.info — free, no key) through curl, as everything else in
//! kessler reaches the network.

use std::path::PathBuf;
use std::sync::mpsc;

use serde::{Deserialize, Serialize};

pub mod cover;
pub mod heard;
pub mod platform;
pub mod programme;
pub mod spins;

/// A station: what it is called, and where its stream is.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Station {
    pub name: String,
    pub url: String,
    /// Its picture, as the directory names it — shown where the cover of what
    /// plays goes.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub favicon: String,
    /// What the directory says about it, for „informationen" while the station
    /// does not say whom it plays: where it is, in which language, what it
    /// plays, and its own page.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub country: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub language: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tags: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub homepage: String,
    /// Where it is, as the directory's two letters — short enough to stand
    /// beside its name in a list of what a search found.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub countrycode: String,
    /// How many kbit/s its stream says it carries, as the directory has it;
    /// empty where it says nothing.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub bitrate: String,
    /// How its stream is coded — „mp3", „aac", „ogg" — as the directory has
    /// it: at one bitrate an AAC stream sounds a good deal better than an
    /// MP3 one.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub codec: String,
    /// How many of the directory's users voted for it.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub votes: u32,
    /// The directory's own last look at it found nothing on the air: it is
    /// set apart in a search, and found last.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub silent: bool,
    /// The directory says it sends HLS, pieces rather than one stream — said in
    /// the station's facts.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hls: bool,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// What stands beside a found station's name, each empty where the
/// directory says nothing — see [`Station::found_facts`].
pub struct FoundFacts<'a> {
    pub genre: &'a str,
    pub land: &'a str,
    pub codec: &'a str,
    pub bitrate: &'a str,
}

impl Station {
    /// Its first genre, its land as two letters, its codec and its bitrate.
    pub fn found_facts(&self) -> FoundFacts<'_> {
        FoundFacts {
            genre: self.tags.split(',').map(str::trim).find(|t| !t.is_empty()).unwrap_or(""),
            land: &self.countrycode,
            codec: &self.codec,
            bitrate: &self.bitrate,
        }
    }

    /// A station known only by the address of its stream, or of a `.pls` or
    /// `.m3u` naming it — one the directory does not have, or has wrong. It
    /// is called after its server until its stream says its own name.
    pub fn by_address(url: &str) -> Station {
        Station { name: address_name(url), url: url.trim().to_string(), ..Station::default() }
    }

    /// Whether it is still called after its address, as [`Station::by_address`]
    /// named it.
    pub fn named_by_address(&self) -> bool {
        self.name == address_name(&self.url)
    }
}

/// Whether what was typed in the search field is the address of a stream
/// rather than words to look for.
pub fn is_address(typed: &str) -> bool {
    let typed = typed.trim();
    (typed.starts_with("http://") || typed.starts_with("https://"))
        && !typed.contains(char::is_whitespace)
        && typed.len() > "https://".len()
}

/// A station's server, as its name until it says one itself:
/// `https://stream.example.org:8000/live.mp3` is „stream.example.org".
fn address_name(url: &str) -> String {
    let rest = url.trim().split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let host = host.rsplit_once('@').map(|(_, h)| h).unwrap_or(host);
    let host = host.split(':').next().unwrap_or(host);
    host.strip_prefix("www.").unwrap_or(host).to_lowercase()
}

/// The stations as an `.m3u` playlist, each with its name, which any player
/// reads and kessler reads back with [`read_playlist`].
pub fn playlist(stations: &[Station]) -> String {
    let mut out = String::from("#EXTM3U\n");
    for station in stations {
        let name = station.name.replace(['\n', '\r'], " ");
        out.push_str(&format!("#EXTINF:-1,{name}\n{}\n", station.url));
    }
    out
}

/// The stations an `.m3u` or a `.pls` names, each under the name it gives
/// (`#EXTINF`, `TitleN=`), else after its address. Each once.
pub fn read_playlist(text: &str) -> Vec<Station> {
    let text = text.trim_start_matches('\u{feff}');
    let mut stations: Vec<Station> = Vec::new();
    if text.lines().any(|l| l.trim().eq_ignore_ascii_case("[playlist]")) {
        let mut files: Vec<(u32, String)> = Vec::new();
        let mut titles: Vec<(u32, String)> = Vec::new();
        for line in text.lines().map(str::trim) {
            let Some((key, value)) = line.split_once('=') else { continue };
            let key = key.trim().to_ascii_lowercase();
            let number = |prefix: &str| key.strip_prefix(prefix).and_then(|n| n.parse::<u32>().ok());
            if let Some(n) = number("file") {
                files.push((n, value.trim().to_string()));
            } else if let Some(n) = number("title") {
                titles.push((n, value.trim().to_string()));
            }
        }
        files.sort_by_key(|(n, _)| *n);
        for (n, url) in files {
            if !is_address(&url) {
                continue;
            }
            let mut station = Station::by_address(&url);
            if let Some((_, title)) = titles.iter().find(|(t, title)| *t == n && !title.is_empty()) {
                station.name = title.clone();
            }
            stations.push(station);
        }
    } else {
        let mut name: Option<String> = None;
        for line in text.lines().map(str::trim) {
            if let Some(info) = line.strip_prefix("#EXTINF:") {
                name = info.split_once(',').map(|(_, n)| n.trim().to_string()).filter(|n| !n.is_empty());
            } else if is_address(line) {
                let mut station = Station::by_address(line);
                if let Some(name) = name.take() {
                    station.name = name;
                }
                stations.push(station);
            }
        }
    }
    let mut seen: Vec<String> = Vec::new();
    stations.retain(|s| {
        let new = !seen.contains(&s.url);
        seen.push(s.url.clone());
        new
    });
    stations
}

/// Keep `stations` beside those kept, whether the radio is open or not;
/// those kept already stay as they are. How many were new.
pub fn add_kept(stations: Vec<Station>) -> usize {
    let read = std::fs::read_to_string(path()).ok().and_then(|text| toml::from_str::<Kept>(&text).ok());
    let mut kept = read.unwrap_or_default();
    let mut added = 0;
    for station in stations {
        if !kept.station.iter().any(|k| k.url == station.url) {
            kept.station.push(station);
            added += 1;
        }
    }
    if added > 0 {
        if let Ok(text) = toml::to_string(&kept) {
            let _ = std::fs::create_dir_all(crate::config::Config::config_dir());
            let _ = std::fs::write(path(), text);
        }
    }
    added
}

/// The station kept at `url` under its address takes `name`, the one its
/// stream says, in `radio.toml`. Whether it did.
pub fn name_kept(url: &str, name: &str) -> bool {
    let read = std::fs::read_to_string(path()).ok().and_then(|text| toml::from_str::<Kept>(&text).ok());
    let mut kept = read.unwrap_or_default();
    let Some(station) = kept.station.iter_mut().find(|s| s.url == url && s.named_by_address()) else {
        return false;
    };
    station.name = name.to_string();
    if let Ok(text) = toml::to_string(&kept) {
        let _ = std::fs::write(path(), text);
    }
    true
}

/// The stations kept, as they stand in `radio.toml`, whether the radio is
/// open or not.
pub fn kept_stations() -> Vec<Station> {
    let read = std::fs::read_to_string(path()).ok().and_then(|text| toml::from_str::<Kept>(&text).ok());
    read.unwrap_or_default().station
}

/// A song kept from the radio: who, what, on which station and when.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Song {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub artist: String,
    pub title: String,
    #[serde(default)]
    pub station: String,
    #[serde(default)]
    pub at: String,
}

/// A line a station sent about what it plays, heard at `at`. Mostly „artist -
/// title"; a station that names its shows instead has no artist in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Aired {
    pub said: String,
    pub artist: Option<String>,
    pub title: String,
    pub at: chrono::DateTime<chrono::Local>,
    /// Whether it was heard from its beginning — the first line after tuning
    /// in is a song already under way — and whether it went to last.fm.
    pub from_start: bool,
    pub scrobbled: bool,
    /// How long it stood paused: not heard, so not counted towards last.fm.
    pub paused_secs: i64,
}

impl Aired {
    /// How long it has been heard by `now`: the time since it came, less the
    /// pauses — those over and the one going on since `paused_since`, if any.
    pub fn heard_secs(&self, now: chrono::DateTime<chrono::Local>, paused_since: Option<chrono::DateTime<chrono::Local>>) -> i64 {
        let pausing = paused_since.map_or(0, |since| (now - since).num_seconds().max(0));
        ((now - self.at).num_seconds() - self.paused_secs - pausing).max(0)
    }

    pub fn new(said: &str) -> Self {
        let parted = [" - ", " – ", " — "].iter().find_map(|apart| said.split_once(apart));
        let (artist, title) = match parted {
            Some((artist, title)) if !artist.trim().is_empty() && !title.trim().is_empty() => {
                (Some(artist.trim().to_string()), title.trim().to_string())
            }
            _ => (None, said.trim().to_string()),
        };
        Aired { said: said.to_string(), artist, title, at: chrono::Local::now(), from_start: true, scrobbled: false, paused_secs: 0 }
    }
}

/// How many lines a station's are kept: what was heard this evening, not
/// a history.
pub const AIRED_KEPT: usize = 40;

/// The stations kept, in the order they were kept, and which of the
/// stations offered from the start have been offered already — one let go
/// of stays gone.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Kept {
    #[serde(default)]
    station: Vec<Station>,
    #[serde(default)]
    offered: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    song: Vec<Song>,
    /// Whether the example stations were asked for (menu → radio): the list
    /// starts empty, and they come only then.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    examples: bool,
}

/// What the directory will say, once it has.
type Answer = mpsc::Receiver<Result<Vec<Station>, String>>;

/// What the radio holds while it is open.
#[derive(Debug)]
pub struct Radio {
    /// The stations kept — the column's list when nothing is searched for.
    pub kept: Vec<Station>,
    /// The stations offered from the start that are in the list already, or
    /// were and were let go of.
    offered: Vec<String>,
    /// Whether the example stations are wanted — see `Kept::examples`.
    examples: bool,
    /// The songs kept, the newest first.
    pub songs: Vec<Song>,
    /// What the search field asked for last, and what came back.
    pub found: Vec<Station>,
    /// The keyboard's row among the stations shown.
    pub cursor: usize,
    /// The keyboard's row unmarked, as the library's is once one clicks
    /// somewhere else: a click outside the stations' column lets go of
    /// it, the arrows and a click on a station take it up again.
    pub resting: bool,
    /// The query that is out, and since when it has stood still, so a
    /// search goes out once the typing stops rather than at every letter.
    pub asked: String,
    pub waiting: Option<Answer>,
    /// Stations offered from the start whose streams' addresses are the
    /// directory's to know, not ours, being looked up.
    looking_up: Vec<(&'static str, Answer)>,
    /// What Wikipedia says about stations, by their page, as read from
    /// `radio-about/` or asked for; `None` while being asked.
    about: Vec<(String, Option<About>)>,
    asking_about: Vec<(String, mpsc::Receiver<Option<About>>)>,
    /// The pictures being fetched from stations' own pages, by station, for the
    /// log.
    pub site_pictures: Vec<(String, mpsc::Receiver<Result<(), String>>)>,
    /// „ähnliche sender", asked for by a right click: listed in place of those
    /// kept until put away.
    pub similar: Option<Similar>,
}

/// Stations like one, as the directory knows them: the station, and those
/// found once they have come.
#[derive(Debug)]
pub struct Similar {
    pub to: String,
    pub stations: Vec<Station>,
    waiting: Option<Answer>,
}

impl Similar {
    pub fn waiting(&self) -> bool {
        self.waiting.is_some()
    }
}

/// What Wikipedia says about a station, in each language the window speaks that
/// has an article, kept beside the settings under
/// `radio-about/` so it is asked once. Found nothing is kept too, with the
/// day it was asked, and asked again after `ABOUT_AGAIN_DAYS`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct About {
    #[serde(default)]
    pub told: Vec<AboutText>,
    #[serde(default)]
    pub asked: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AboutText {
    pub lang: String,
    pub text: String,
    pub page: String,
}

impl About {
    /// The text in `lang`, else in English, else in whichever there is —
    /// with the language it is in.
    pub fn in_lang(&self, lang: &str) -> Option<&AboutText> {
        self.told
            .iter()
            .find(|t| t.lang == lang)
            .or_else(|| self.told.iter().find(|t| t.lang == "en"))
            .or_else(|| self.told.first())
    }
}

/// How long „nothing found" holds before Wikipedia is asked again.
const ABOUT_AGAIN_DAYS: i64 = 30;

fn about_path(site: &str) -> PathBuf {
    let name = crate::lastfm::sign::md5_hex(site.as_bytes());
    crate::config::Config::config_dir().join("radio-about").join(format!("{name}.toml"))
}

/// Where the picture of a station's Wikipedia article is kept.
fn about_picture_path(site: &str) -> PathBuf {
    about_path(site).with_extension("picture")
}

/// The picture to show for `station`: its own from the directory when it is one
/// that can be shown and not a speck, else the icon its own page names, else
/// its Wikipedia article's, else a speck after all. NTS has none in the
/// directory, and Wikipedia keeps most stations' logos to itself; an `.svg`, or
/// a page where a picture should be, cannot be shown.
pub fn picture_path(station: &Station) -> Option<PathBuf> {
    if let Some(own) = own_picture(&station.url) {
        return Some(own);
    }
    let logo = logo_path(station).filter(|p| shown_size(p).is_some());
    let site = (!station.homepage.is_empty()).then_some(station.homepage.as_str());
    logo.clone()
        .filter(|p| shown_size(p).is_some_and(|size| size >= LARGE_ENOUGH))
        .or_else(|| site.map(|site| named_picture_path(site, &station.name)).filter(|p| shown_size(p).is_some()))
        .or_else(|| site.map(site_picture_path).filter(|p| shown_size(p).is_some()))
        .or_else(|| site.map(about_picture_path).filter(|p| shown_size(p).is_some()))
        .or(logo)
}

/// Where the picture chosen by hand for the station at `url` is kept.
fn own_picture_path(url: &str) -> PathBuf {
    let name = crate::lastfm::sign::md5_hex(url.as_bytes());
    crate::config::Config::config_dir().join("radio-logos").join(format!("own-{name}"))
}

/// The picture chosen by hand for the station at `url`, if there is one.
pub fn own_picture(url: &str) -> Option<PathBuf> {
    let path = own_picture_path(url);
    shown_size(&path).map(|_| path)
}

/// Take the picture at `from` for the station at `url`: copied beside the
/// settings, so it stays when the file it came from moves. A file that is
/// not a picture kessler can show is refused, and said why.
pub fn choose_picture(url: &str, from: &std::path::Path) -> Result<(), String> {
    let bytes = std::fs::read(from).map_err(|e| e.to_string())?;
    image::load_from_memory(&bytes).map_err(|_| "kein bild, das kessler zeigen kann".to_string())?;
    let path = own_picture_path(url);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, bytes).map_err(|e| e.to_string())
}

/// Let go of the picture chosen for the station at `url`: its own, or the
/// directory's, is shown again.
pub fn forget_picture(url: &str) {
    let _ = std::fs::remove_file(own_picture_path(url));
}

/// The smaller side of the picture at `path`, if it is one that can be
/// shown.
fn shown_size(path: &std::path::Path) -> Option<u32> {
    let (w, h) = image::ImageReader::open(path).ok()?.with_guessed_format().ok()?.into_dimensions().ok()?;
    Some(w.min(h))
}

/// How large a picture must be to stand for a station without help: at 56
/// points a favicon of 16 pixels is a blur.
const LARGE_ENOUGH: u32 = 48;

/// Where the icon a station's page names is kept, or an empty file when the
/// page named none.
fn site_picture_path(site: &str) -> PathBuf {
    let name = crate::lastfm::sign::md5_hex(site.as_bytes());
    crate::config::Config::config_dir().join("radio-logos").join(format!("site-{name}"))
}

/// Where the picture on `site` that bears the station's name is kept, or an
/// empty file when there was none — see [`pictures_named_for`].
fn named_picture_path(site: &str, name: &str) -> PathBuf {
    let key = format!("{site}\n{}", crate::db::sort_key(name));
    let name = crate::lastfm::sign::md5_hex(key.as_bytes());
    crate::config::Config::config_dir().join("radio-logos").join(format!("named-{name}"))
}

/// The name a station's picture would be filed under: its letters and
/// figures, small, without what stands in brackets after it — „Jazz Sakura
/// (asia dream radio)" is `jazzsakura`. None for one too short to tell a
/// file by.
fn filed_as(name: &str) -> Option<String> {
    let name = name.split(['(', '[']).next().unwrap_or(name);
    let squashed: String = name.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect();
    (squashed.chars().count() >= 4).then_some(squashed)
}

/// The pictures on a page whose file bears the station's name: a site of
/// several stations — one network's page of all its channels — names no
/// icon for each, but files each one's logo under its name
/// (`JazzSakuraLogo2019.jpeg`). Addresses as the page would load them, the
/// first found first.
fn pictures_named_for(page: &str, site: &str, name: &str) -> Vec<String> {
    let Some(filed) = filed_as(name) else { return Vec::new() };
    let mut found: Vec<String> = Vec::new();
    for quote in ['"', '\''] {
        for part in page.split(quote).skip(1).step_by(2) {
            let address = part.trim().replace("&amp;", "&");
            let bare = address.split(['?', '#']).next().unwrap_or(&address);
            let lower = bare.to_ascii_lowercase();
            if address.len() > 500 || ![".png", ".jpg", ".jpeg", ".webp", ".gif"].iter().any(|e| lower.ends_with(e)) {
                continue;
            }
            let file = lower.rsplit('/').next().unwrap_or(&lower);
            let file: String = file.chars().filter(|c| c.is_alphanumeric()).collect();
            if !file.contains(&filed) {
                continue;
            }
            let url = resolved(site, &address);
            if !found.contains(&url) {
                found.push(url);
            }
        }
    }
    found
}

/// `address` as a page at `page` would load it: whole already, from the
/// site's root, or beside the page.
fn resolved(page: &str, address: &str) -> String {
    if address.starts_with("http://") || address.starts_with("https://") {
        return address.to_string();
    }
    if let Some(rest) = address.strip_prefix("//") {
        return format!("https://{rest}");
    }
    let (scheme, rest) = page.split_once("://").unwrap_or(("https", page));
    let rest = rest.split(['?', '#']).next().unwrap_or(rest);
    let host = rest.split('/').next().unwrap_or(rest);
    if address.starts_with('/') {
        return format!("{scheme}://{host}{address}");
    }
    let folder = match rest.rfind('/') {
        Some(at) if at > host.len() || rest[host.len()..].starts_with('/') => &rest[..at],
        _ => host,
    };
    format!("{scheme}://{folder}/{address}")
}

/// Fetch the picture filed under the station's name on `site` (see
/// [`pictures_named_for`]) and the icon `site` names for itself, on a
/// thread of its own, once each:
/// the one for phones' home screens first (square, and large), then the one
/// it shows when shared, then its favicons. The first that can be shown
/// and is large enough is kept. Whether one was found, or why not, comes
/// back for the log.
pub fn fetch_site_picture(site: &str, name: &str) -> Option<mpsc::Receiver<Result<(), String>>> {
    let path = site_picture_path(site);
    let named = named_picture_path(site, name);
    if site.is_empty() || (path.exists() && named.exists()) {
        return None;
    }
    let (tell, answer) = mpsc::channel();
    let (site, name) = (site.to_string(), name.to_string());
    std::thread::Builder::new()
        .name("kessler-radio-site-picture".into())
        .spawn(move || {
            let page = crate::library::coverart::fetch_text(&site).map_err(|e| e.to_string());
            let told = page.map(|page| {
                let mut any = false;
                if !named.exists() {
                    let mut best: Option<(u32, Vec<u8>)> = None;
                    for url in pictures_named_for(&page, &site, &name).into_iter().take(6) {
                        let Ok(bytes) = crate::library::coverart::fetch(&url) else { continue };
                        let Some(side) = image::load_from_memory(&bytes).ok().map(|i| i.width().min(i.height())) else { continue };
                        if side >= LARGE_ENOUGH && best.as_ref().is_none_or(|(b, _)| side > *b) {
                            best = Some((side, bytes));
                        }
                    }
                    any |= best.is_some();
                    keep(&named, best.as_ref().map_or(&[][..], |(_, b)| b));
                }
                if !path.exists() {
                    let icon = icons_named(&page, &site).into_iter().find_map(|url| {
                        let bytes = crate::library::coverart::fetch(&url).ok()?;
                        image::load_from_memory(&bytes).ok().filter(|i| i.width().min(i.height()) >= 32).map(|_| bytes)
                    });
                    any |= icon.is_some();
                    keep(&path, icon.as_deref().unwrap_or(&[]));
                }
                if any { Ok(()) } else { Err("die seite nennt kein bild".to_string()) }
            });
            let _ = tell.send(told.and_then(|t| t));
        })
        .ok();
    Some(answer)
}

fn keep(path: &std::path::Path, bytes: &[u8]) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, bytes);
}

/// The pictures a page names for itself, the most fitting first, as
/// addresses; and the places such pictures are kept by custom.
fn icons_named(page: &str, site: &str) -> Vec<String> {
    let origin = {
        let rest = site.split_once("://").map_or(site, |(_, r)| r);
        let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
        let scheme = site.split_once("://").map_or("https", |(s, _)| s);
        format!("{scheme}://{host}")
    };
    let absolute = |href: &str| {
        let href = href.trim().replace("&amp;", "&");
        if href.starts_with("http://") || href.starts_with("https://") {
            href
        } else if let Some(rest) = href.strip_prefix("//") {
            format!("https://{rest}")
        } else if href.starts_with('/') {
            format!("{origin}{href}")
        } else {
            format!("{origin}/{href}")
        }
    };
    let mut found: Vec<(u32, String)> = Vec::new();
    let lower = page.to_ascii_lowercase();
    for (at, _) in lower.match_indices('<') {
        let tag = &page[at..];
        let tag = &tag[..tag.find('>').unwrap_or(tag.len()).min(2000)];
        let lower_tag = tag.to_ascii_lowercase();
        let value = |name: &str| attribute(tag, &lower_tag, name);
        let rank = if lower_tag.starts_with("<link") {
            let rel = value("rel").unwrap_or_default().to_ascii_lowercase();
            let size: u32 = value("sizes")
                .and_then(|s| s.split(['x', 'X']).next().and_then(|n| n.trim().parse().ok()))
                .unwrap_or(0);
            if rel.contains("apple-touch-icon") {
                3000 + size
            } else if rel.split_whitespace().any(|r| r == "icon") {
                1000 + size
            } else {
                continue;
            }
        } else if lower_tag.starts_with("<meta") {
            let property = value("property").or_else(|| value("name")).unwrap_or_default().to_ascii_lowercase();
            if property == "og:image" || property == "twitter:image" {
                2000
            } else {
                continue;
            }
        } else {
            continue;
        };
        let href = if lower_tag.starts_with("<link") { value("href") } else { value("content") };
        if let Some(href) = href.filter(|h| !h.trim().is_empty() && !h.ends_with(".svg")) {
            found.push((rank, absolute(&href)));
        }
    }
    found.sort_by_key(|a| std::cmp::Reverse(a.0));
    let mut urls: Vec<String> = found.into_iter().map(|(_, url)| url).collect();
    for usual in ["/apple-touch-icon.png", "/favicon.ico"] {
        urls.push(format!("{origin}{usual}"));
    }
    urls.dedup();
    urls
}

/// The value of attribute `name` in `tag`, quoted or not.
fn attribute(tag: &str, lower: &str, name: &str) -> Option<String> {
    let mut from = 0;
    while let Some(found) = lower[from..].find(name) {
        let at = from + found;
        from = at + name.len();
        let before = lower[..at].chars().last();
        if !before.is_some_and(char::is_whitespace) {
            continue;
        }
        let rest = tag[at + name.len()..].trim_start();
        let Some(rest) = rest.strip_prefix('=') else { continue };
        let rest = rest.trim_start();
        let value = match rest.chars().next() {
            Some(q @ ('"' | '\'')) => rest[1..].split(q).next().unwrap_or(""),
            _ => rest.split(|c: char| c.is_whitespace() || c == '>').next().unwrap_or(""),
        };
        return Some(value.to_string());
    }
    None
}

/// What was kept about `site`, unless it is an old „nothing".
fn read_about(site: &str) -> Option<About> {
    let text = std::fs::read_to_string(about_path(site)).ok()?;
    let about: About = toml::from_str(&text).ok()?;
    if about.told.is_empty() {
        let asked = chrono::NaiveDate::parse_from_str(&about.asked, "%Y-%m-%d").ok()?;
        if (chrono::Local::now().date_naive() - asked).num_days() >= ABOUT_AGAIN_DAYS {
            return None;
        }
    }
    Some(about)
}

/// Ask Wikipedia about `site` on a thread of its own, and keep the answer.
/// Not reached is `None` and kept nowhere: asked again next time.
fn ask_about(site: &str) -> mpsc::Receiver<Option<About>> {
    let (tell, answer) = mpsc::channel();
    let site = site.to_string();
    std::thread::Builder::new()
        .name("kessler-radio-about".into())
        .spawn(move || {
            let found = crate::library::artistinfo::station_wikipedia(&site).ok();
            let about = found.map(|(told, picture)| {
                if let Some(bytes) = picture.filter(|b| image::guess_format(b).is_ok()) {
                    let path = about_picture_path(&site);
                    if let Some(dir) = path.parent() {
                        let _ = std::fs::create_dir_all(dir);
                        let _ = std::fs::write(&path, bytes);
                    }
                }
                About {
                    told: told.into_iter().map(|(lang, text, page)| AboutText { lang, text, page }).collect(),
                    asked: chrono::Local::now().format("%Y-%m-%d").to_string(),
                }
            });
            if let Some(about) = &about {
                let path = about_path(&site);
                if let (Some(dir), Ok(text)) = (path.parent(), toml::to_string(about)) {
                    let _ = std::fs::create_dir_all(dir);
                    let _ = std::fs::write(&path, text);
                }
            }
            let _ = tell.send(about);
        })
        .ok();
    answer
}

/// In the list from the start, as a demonstration, and let go of like any
/// other. NTS's two channels at the addresses NTS hands to players, with where
/// they are and their page.
const DEMO: [(&str, &str); 2] = [
    ("nts 1", "https://stream-relay-geo.ntslive.net/stream?client=direct"),
    ("nts 2", "https://stream-relay-geo.ntslive.net/stream2?client=direct"),
];

fn demo_station(name: &str, url: &str) -> Station {
    Station {
        name: name.to_string(),
        url: url.to_string(),
        country: "united kingdom".into(),
        language: "english".into(),
        tags: "eclectic, underground".into(),
        homepage: "https://www.nts.live".into(),
        ..Station::default()
    }
}

/// Also there from the start, but looked up in the directory: the name they are
/// kept under, and what to ask for. One not found is asked for again at the
/// next opening. Cashmere Radio. Kept where somebody has it already.
const DEMO_LOOKED_UP: [(&str, &str); 3] = [
    ("montez press radio", "montez press radio"),
    ("kzsu stanford", "kzsu"),
    ("cashmere radio", "cashmere radio"),
];

/// Where a station's picture is kept, once fetched: beside the settings,
/// named after where it came from.
pub fn logo_path(station: &Station) -> Option<PathBuf> {
    if station.favicon.is_empty() {
        return None;
    }
    let name = crate::lastfm::sign::md5_hex(station.favicon.as_bytes());
    Some(crate::config::Config::config_dir().join("radio-logos").join(name))
}

/// Fetch a station's picture if it has one and it is not kept yet, on a
/// thread of its own; the window shows it once it is there.
pub fn fetch_logo(station: &Station) {
    let Some(path) = logo_path(station) else { return };
    if path.exists() {
        return;
    }
    let url = station.favicon.clone();
    std::thread::Builder::new()
        .name("kessler-radio-logo".into())
        .spawn(move || {
            let Some(dir) = path.parent() else { return };
            let _ = std::fs::create_dir_all(dir);
            let part = path.with_extension("part");
            let mut command = crate::net::curl();
            command
                .args(["--silent", "--fail", "--location", "--max-time", "12", "--max-filesize", "2000000"])
                .args(["--user-agent", concat!("kessler/", env!("CARGO_PKG_VERSION"))])
                .arg("--output")
                .arg(&part);
            let fetched = crate::net::address(&mut command, &url)
                .status()
                .is_ok_and(|s| s.success());
            if fetched {
                let _ = std::fs::rename(&part, &path);
            } else {
                let _ = std::fs::remove_file(&part);
            }
        })
        .ok();
}

/// Put the example stations in the list at the next opening: those let go
/// of come back, those still kept stay once. Until asked for, the list
/// starts empty — none of them is there unless somebody wants it.
pub fn offer_again() {
    let read = std::fs::read_to_string(path()).ok().and_then(|text| toml::from_str::<Kept>(&text).ok());
    let mut kept = read.unwrap_or_default();
    kept.offered.clear();
    kept.examples = true;
    if let Ok(text) = toml::to_string(&kept) {
        let _ = std::fs::create_dir_all(crate::config::Config::config_dir());
        let _ = std::fs::write(path(), text);
    }
}

/// Where the stations kept are written.
fn path() -> PathBuf {
    crate::config::Config::config_dir().join("radio.toml")
}

impl Radio {
    /// Open the radio: the stations kept, and those offered from the start
    /// that have not been offered yet.
    pub fn open() -> Self {
        let read = std::fs::read_to_string(path()).ok().and_then(|text| toml::from_str::<Kept>(&text).ok());
        let Kept { station: kept, offered, song: songs, examples } = read.unwrap_or_default();
        let mut radio = Radio { kept, offered, examples, songs, found: Vec::new(), cursor: 0, resting: false, asked: String::new(), waiting: None, looking_up: Vec::new(), about: Vec::new(), asking_about: Vec::new(), site_pictures: Vec::new(), similar: None };
        let mut changed = false;
        for station in &mut radio.kept {
            if let Some((name, url)) = DEMO.iter().find(|(_, url)| *url == station.url) {
                if station.homepage.is_empty() {
                    *station = Station { name: station.name.clone(), ..demo_station(name, url) };
                    changed = true;
                }
            }
        }
        if radio.examples {
            for (name, url) in DEMO {
                if !radio.was_offered(name) {
                    radio.offer(demo_station(name, url));
                    changed = true;
                }
            }
            radio.look_up_missing();
        }
        if changed {
            radio.save();
        }
        radio
    }

    /// Ask the directory for the demonstration's stations it has not found
    /// yet, unless they are being asked for already.
    fn look_up_missing(&mut self) {
        for (name, query) in DEMO_LOOKED_UP {
            if !self.was_offered(name) && !self.looking_up.iter().any(|(asked, _)| *asked == name) {
                self.looking_up.push((name, search(query, Reading::Name)));
            }
        }
    }

    fn was_offered(&self, name: &str) -> bool {
        self.offered.iter().any(|o| o == name)
    }

    /// Put a station offered from the start into the list, once ever.
    fn offer(&mut self, station: Station) {
        self.offered.push(station.name.clone());
        if !self.keeps(&station) {
            self.kept.push(station);
        }
    }

    /// Whether `station` is among those kept.
    pub fn keeps(&self, station: &Station) -> bool {
        self.kept.iter().any(|kept| kept.url == station.url)
    }

    /// Keep `station`, or let go of it if it is kept already.
    pub fn toggle_kept(&mut self, station: &Station) {
        if self.keeps(station) {
            self.kept.retain(|kept| kept.url != station.url);
        } else {
            self.kept.push(station.clone());
        }
        self.save();
    }

    /// Move the station kept at `from` to stand before the one at `to` (`to`
    /// may be one past the last), as dragged. The keyboard's row goes with it.
    pub fn move_kept(&mut self, from: usize, to: usize) {
        if let Some(at) = moved(&mut self.kept, from, to) {
            self.cursor = at;
            self.save();
        }
    }

    /// The station kept at `url` under a name of one's own, in the lists and
    /// in `radio.toml`. Not its address, so its stream's own name never
    /// takes its place again (see [`Station::named_by_address`]). Whether it
    /// was kept.
    pub fn rename(&mut self, url: &str, name: &str) -> bool {
        let mut renamed = false;
        for station in self.kept.iter_mut().filter(|s| s.url == url) {
            station.name = name.to_string();
            renamed = true;
        }
        for station in self.found.iter_mut().filter(|s| s.url == url) {
            station.name = name.to_string();
        }
        if renamed {
            self.save();
        }
        renamed
    }

    /// The station at `url`, known by its address, under the name its stream
    /// says — in the lists shown; `radio.toml` is [`name_kept`]'s.
    pub fn named(&mut self, url: &str, name: &str) {
        for station in self.kept.iter_mut().chain(self.found.iter_mut()) {
            if station.url == url && station.named_by_address() {
                station.name = name.to_string();
            }
        }
    }

    fn save(&self) {
        let kept = Kept { station: self.kept.clone(), offered: self.offered.clone(), song: self.songs.clone(), examples: self.examples };
        if let Ok(text) = toml::to_string(&kept) {
            let _ = std::fs::create_dir_all(crate::config::Config::config_dir());
            let _ = std::fs::write(path(), text);
        }
    }

    /// Whether a song like `aired` is kept.
    pub fn keeps_song(&self, aired: &Aired) -> bool {
        let artist = aired.artist.as_deref().unwrap_or("");
        self.songs.iter().any(|s| s.artist == artist && s.title == aired.title)
    }

    /// Keep what `station` played, or let go of it if it is kept already.
    /// Whether it is kept now.
    pub fn toggle_song(&mut self, aired: &Aired, station: &str) -> bool {
        let artist = aired.artist.clone().unwrap_or_default();
        let kept = if self.keeps_song(aired) {
            self.songs.retain(|s| !(s.artist == artist && s.title == aired.title));
            false
        } else {
            let at = aired.at.format("%Y-%m-%d %H:%M").to_string();
            self.songs.insert(0, Song { artist, title: aired.title.clone(), station: station.to_string(), at });
            true
        };
        self.save();
        kept
    }

    /// Let go of a kept song.
    pub fn forget_song(&mut self, song: &Song) {
        self.songs.retain(|s| s != song);
        self.save();
    }

    /// Ask the directory for `query`, unless it is what was asked last.
    pub fn ask(&mut self, query: &str) {
        let query = query.trim();
        if query == self.asked {
            return;
        }
        self.asked = query.to_string();
        if is_address(query) {
            self.found = vec![Station::by_address(query)];
            self.waiting = None;
            return;
        }
        if query.chars().count() < 2 {
            self.found.clear();
            self.waiting = None;
            return;
        }
        self.waiting = Some(search(query, Reading::Words));
    }

    /// Take in what has come back. An error is handed on, for the status
    /// line; whether anything changed comes back too, for a repaint.
    pub fn take_answers(&mut self) -> Result<bool, String> {
        let mut changed = false;
        let mut told = Vec::new();
        self.asking_about.retain(|(site, asking)| match asking.try_recv() {
            Ok(about) => {
                told.push((site.clone(), about));
                false
            }
            Err(mpsc::TryRecvError::Empty) => true,
            Err(mpsc::TryRecvError::Disconnected) => false,
        });
        for (site, about) in told {
            if let Some(slot) = self.about.iter_mut().find(|(s, _)| *s == site) {
                slot.1 = about;
            }
            changed = true;
        }
        let mut answered = Vec::new();
        self.looking_up.retain(|(name, looking)| match looking.try_recv() {
            Ok(answer) => {
                answered.push((*name, answer));
                false
            }
            Err(mpsc::TryRecvError::Empty) => true,
            Err(mpsc::TryRecvError::Disconnected) => false,
        });
        for (name, answer) in answered {
            if let Some(first) = answer.ok().and_then(|found| found.into_iter().next()) {
                self.offer(Station { name: name.to_string(), ..first });
                self.save();
            }
            changed = true;
        }
        if let Some(similar) = self.similar.as_mut() {
            if let Some(Ok(answer)) = similar.waiting.as_ref().map(|w| w.try_recv()) {
                similar.waiting = None;
                similar.stations = answer?;
                changed = true;
            }
        }
        if let Some(waiting) = &self.waiting {
            if let Ok(answer) = waiting.try_recv() {
                self.waiting = None;
                self.found = answer?;
                changed = true;
                self.look_up_missing();
            }
        }
        Ok(changed)
    }

    /// What Wikipedia says about the station whose page is `site`: kept, or
    /// asked for now (once while the radio is open; a station not reached
    /// is asked again at the next opening). `None` while it is being asked
    /// or when nothing was found.
    pub fn about(&mut self, site: &str) -> Option<&About> {
        if site.is_empty() {
            return None;
        }
        if !self.about.iter().any(|(s, _)| s == site) {
            let kept = read_about(site);
            if kept.is_none() {
                self.asking_about.push((site.to_string(), ask_about(site)));
            }
            self.about.push((site.to_string(), kept));
        }
        self.about.iter().find(|(s, _)| s == site).and_then(|(_, a)| a.as_ref()).filter(|a| !a.told.is_empty())
    }

    /// Whether Wikipedia is still being asked about `site`.
    pub fn asking_about(&self, site: &str) -> bool {
        self.asking_about.iter().any(|(s, _)| s == site)
    }

    /// Whether an answer is still out.
    pub fn busy(&self) -> bool {
        self.waiting.is_some() || !self.looking_up.is_empty() || self.similar.as_ref().is_some_and(Similar::waiting)
    }

    /// „ähnliche sender": the directory asked for stations like `station`,
    /// listed in place of those kept once they come.
    pub fn find_similar(&mut self, station: &Station) {
        let (tell, answer) = mpsc::channel();
        let station_asked = station.clone();
        std::thread::Builder::new()
            .name("kessler-radio-similar".into())
            .spawn(move || {
                let _ = tell.send(ask_similar(&station_asked));
            })
            .ok();
        self.similar = Some(Similar { to: station.name.clone(), stations: Vec::new(), waiting: Some(answer) });
        self.cursor = 0;
    }

}

/// Move what stands at `from` in `list` to before `to` (one past the last
/// for the end); where it stands now, or `None` if it stays where it was.
fn moved<T>(list: &mut Vec<T>, from: usize, to: usize) -> Option<usize> {
    if from >= list.len() || to > list.len() || to == from || to == from + 1 {
        return None;
    }
    let item = list.remove(from);
    let to = if to > from { to - 1 } else { to };
    list.insert(to, item);
    Some(to)
}

/// How a search is read: as a station's name only — the demonstration's, looked
/// up by name — or as words, a land among them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reading {
    Name,
    Words,
}

/// Ask the Radio Browser directory for `query`, on a thread of its own.
fn search(query: &str, reading: Reading) -> Answer {
    let (tell, answer) = mpsc::channel();
    let query = query.to_string();
    std::thread::Builder::new()
        .name("kessler-radio-search".into())
        .spawn(move || {
            let _ = tell.send(ask_directory(&query, reading));
        })
        .ok();
    answer
}

/// How many stations „ähnliche sender" shows.
const SIMILAR_SHOWN: usize = 8;

/// Stations like `station`. Nobody keeps who listens to which station alongside
/// which — the directory counts votes and plays per station, last.fm knows no
/// stations — so „like" is what the directory's users said of them: those with
/// its genres, asked for genre by genre (its first three), ranked by how many
/// of its genres they share, then by whether they speak its language and stand
/// in its land, then by how liked they are. Without genres, its language and
/// land stand in for them.
fn ask_similar(station: &Station) -> Result<Vec<Station>, String> {
    let tags: Vec<String> = station.tags.split(',').map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).take(3).collect();
    let mut asks: Vec<Vec<(&str, String)>> = tags.iter().map(|tag| vec![("tag", tag.clone()), ("tagExact", "true".to_string())]).collect();
    if asks.is_empty() {
        let mut by = Vec::new();
        if !station.language.is_empty() {
            by.push(("language", station.language.clone()));
        }
        if !station.country.is_empty() {
            by.push(("country", station.country.clone()));
        }
        if by.is_empty() {
            return Ok(Vec::new());
        }
        asks.push(by);
    }
    let mut entries: Vec<(Station, Standing)> = Vec::new();
    let mut trouble = None;
    for ask in asks {
        match ask_by(&ask) {
            Ok(answer) => {
                for (found, standing) in entries_in(&answer) {
                    if !entries.iter().any(|(s, _)| s.url == found.url) {
                        entries.push((found, standing));
                    }
                }
            }
            Err(why) => trouble = Some(why),
        }
    }
    match trouble {
        Some(why) if entries.is_empty() => Err(why),
        _ => Ok(similar_ranked(station, entries)),
    }
}

/// The stations found, the most like `station` first, itself and its
/// copies left out.
fn similar_ranked(station: &Station, entries: Vec<(Station, Standing)>) -> Vec<Station> {
    let tags_of = |s: &Station| s.tags.split(',').map(folded).filter(|t| !t.is_empty()).collect::<Vec<_>>();
    let own = tags_of(station);
    let own_name = folded(&station.name);
    let mut found: Vec<(f64, Station)> = entries
        .into_iter()
        .filter(|(s, _)| s.url != station.url && folded(&s.name) != own_name)
        .map(|(s, standing)| {
            let shared = tags_of(&s).iter().filter(|t| own.contains(t)).count() as f64;
            let speaks = f64::from(u8::from(!station.language.is_empty() && s.language == station.language));
            let lives = f64::from(u8::from(!station.country.is_empty() && s.country == station.country));
            let liked = standing.votes.max(0.0).ln_1p() + 0.5 * standing.clicks.max(0.0).ln_1p();
            (shared * 4.0 + speaks * 1.5 + lives + liked * 0.3, s)
        })
        .collect();
    found.sort_by(|a, b| a.1.silent.cmp(&b.1.silent).then(b.0.total_cmp(&a.0)));
    let mut names: Vec<String> = Vec::new();
    let mut stations = Vec::new();
    for (_, s) in found {
        let name = folded(&s.name);
        if names.contains(&name) {
            continue;
        }
        names.push(name);
        stations.push(s);
        if stations.len() == SIMILAR_SHOWN {
            break;
        }
    }
    stations
}

/// Where the directory is asked; `KESSLER_RADIO_BROWSER` for a stand-in.
fn directory() -> String {
    std::env::var("KESSLER_RADIO_BROWSER")
        .ok()
        .filter(|d| !d.is_empty())
        .unwrap_or_else(|| "https://all.api.radio-browser.info".to_string())
}

/// How many the directory is asked for: more than are shown, so that the
/// ranking below has something to choose from.
const ASKED_FOR: usize = 100;
/// How many of them the column shows.
const FOUND_SHOWN: usize = 40;

/// Ask the directory for `query`, best first. Read as words, a land named among
/// them — „japan", „japanisch", „日本" — narrows the search to the stations
/// there, and the rest is looked for both in the names and in the genres the
/// directory's users gave them, so „jazz japan" finds Japan's jazz stations
/// whatever they are called.
fn ask_directory(query: &str, reading: Reading) -> Result<Vec<Station>, String> {
    let (rest, land) = match reading {
        Reading::Name => (query.trim().to_string(), None),
        Reading::Words => understood(query),
    };
    let mut asks: Vec<Vec<(&str, String)>> = Vec::new();
    let base: Vec<(&str, String)> = land.map(|code| vec![("countrycode", code.to_string())]).unwrap_or_default();
    if rest.is_empty() {
        asks.push(base);
    } else {
        asks.push([base.clone(), vec![("name", rest.clone())]].concat());
        if reading == Reading::Words {
            asks.push([base, vec![("tag", rest.clone())]].concat());
        }
    }
    let mut entries: Vec<(Station, Standing)> = Vec::new();
    let mut trouble = None;
    for ask in asks {
        match ask_by(&ask) {
            Ok(answer) => {
                for (station, standing) in entries_in(&answer) {
                    if !entries.iter().any(|(s, _)| s.url == station.url) {
                        entries.push((station, standing));
                    }
                }
            }
            Err(why) => trouble = Some(why),
        }
    }
    match trouble {
        Some(why) if entries.is_empty() => Err(why),
        _ => Ok(ranked(&rest, entries)),
    }
}

/// One question to the directory, its answer as it came.
fn ask_by(asked: &[(&str, String)]) -> Result<String, String> {
    let mut command = crate::net::curl();
    command
        .args(["--silent", "--show-error", "--fail", "--max-time", "12", "--get"])
        .args(["--user-agent", concat!("kessler/", env!("CARGO_PKG_VERSION"))]);
    for (key, value) in asked {
        command.arg("--data-urlencode").arg(format!("{key}={value}"));
    }
    let output = command
        .arg("--data")
        .arg(format!("limit={ASKED_FOR}"))
        .args(["--data", "hidebroken=true", "--data", "reverse=true", "--data", "order=votes"])
        .arg(format!("{}/json/stations/search", directory()))
        .output()
        .map_err(|e| format!("curl: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// What was typed, read: the words without the land, and the land's code (ISO
/// 3166, as the directory keeps it) if one was named. Two words are tried
/// before one, so „south korea" is not „korea"; a German adjective's ending
/// does not matter; a Japanese name may stand inside a word, as Japanese writes
/// without spaces.
fn understood(query: &str) -> (String, Option<&'static str>) {
    let mut words: Vec<String> = folded(query).split(' ').filter(|w| !w.is_empty()).map(str::to_string).collect();
    let is = |said: &str| {
        let bare = ["en", "er", "es", "em", "e"].iter().find_map(|end| said.strip_suffix(end)).filter(|b| b.chars().count() >= 4);
        LANDS.iter().find(|(_, names)| names.iter().any(|n| *n == said || Some(*n) == bare)).map(|(code, _)| *code)
    };
    for at in 0..words.len().saturating_sub(1) {
        if let Some(code) = is(&format!("{} {}", words[at], words[at + 1])) {
            words.drain(at..at + 2);
            return (words.join(" "), Some(code));
        }
    }
    for at in 0..words.len() {
        if let Some(code) = is(&words[at]) {
            words.remove(at);
            return (words.join(" "), Some(code));
        }
    }
    for word in words.iter_mut() {
        for (code, names) in LANDS {
            if let Some(name) = names.iter().find(|n| !n.is_ascii() && word.contains(**n)) {
                *word = word.replacen(name, " ", 1).trim().trim_start_matches('の').trim().to_string();
                let rest = words.iter().filter(|w| !w.is_empty()).cloned().collect::<Vec<_>>().join(" ");
                return (rest, Some(code));
            }
        }
    }
    (words.join(" "), None)
}

/// The lands a search knows by name, in the window's three languages, with what
/// they are called as an adjective — folded as `folded` folds. Not the short
/// codes, nor words that are also ordinary ones.
const LANDS: &[(&str, &[&str])] = &[
    ("JP", &["japan", "japanisch", "japanese", "nippon", "日本", "にほん"]),
    ("DE", &["germany", "deutschland", "deutsch", "german", "ドイツ"]),
    ("AT", &["austria", "österreich", "österreichisch", "austrian", "オーストリア"]),
    ("CH", &["switzerland", "schweiz", "schweizerisch", "swiss", "スイス"]),
    ("FR", &["france", "frankreich", "französisch", "french", "フランス"]),
    ("GB", &["uk", "united kingdom", "great britain", "britain", "england", "großbritannien", "britisch", "british", "イギリス"]),
    ("US", &["usa", "united states", "america", "amerika", "amerikanisch", "american", "アメリカ"]),
    ("CA", &["canada", "kanada", "kanadisch", "canadian", "カナダ"]),
    ("IT", &["italy", "italien", "italienisch", "italian", "イタリア"]),
    ("ES", &["spain", "spanien", "spanisch", "spanish", "スペイン"]),
    ("PT", &["portugal", "portugiesisch", "portuguese", "ポルトガル"]),
    ("NL", &["netherlands", "niederlande", "holland", "niederländisch", "dutch", "オランダ"]),
    ("BE", &["belgium", "belgien", "belgisch", "belgian", "ベルギー"]),
    ("DK", &["denmark", "dänemark", "dänisch", "danish", "デンマーク"]),
    ("SE", &["sweden", "schweden", "schwedisch", "swedish", "スウェーデン"]),
    ("NO", &["norway", "norwegen", "norwegisch", "norwegian", "ノルウェー"]),
    ("FI", &["finland", "finnland", "finnisch", "finnish", "フィンランド"]),
    ("PL", &["poland", "polen", "polnisch", "polish", "ポーランド"]),
    ("CZ", &["czechia", "czech republic", "tschechien", "tschechisch", "czech", "チェコ"]),
    ("HU", &["hungary", "ungarn", "ungarisch", "hungarian", "ハンガリー"]),
    ("GR", &["greece", "griechenland", "griechisch", "greek", "ギリシャ"]),
    ("TR", &["turkey", "türkei", "türkisch", "turkish", "トルコ"]),
    ("RU", &["russia", "russland", "russisch", "russian", "ロシア"]),
    ("UA", &["ukraine", "ukrainisch", "ukrainian", "ウクライナ"]),
    ("IE", &["ireland", "irland", "irisch", "irish", "アイルランド"]),
    ("KR", &["south korea", "korea", "südkorea", "koreanisch", "korean", "韓国"]),
    ("CN", &["china", "chinesisch", "chinese", "中国"]),
    ("TW", &["taiwan", "taiwanisch", "taiwanese", "台湾"]),
    ("HK", &["hong kong", "hongkong", "香港"]),
    ("IN", &["india", "indien", "indisch", "indian", "インド"]),
    ("TH", &["thailand", "thailändisch", "thai", "タイ"]),
    ("ID", &["indonesia", "indonesien", "indonesisch", "indonesian", "インドネシア"]),
    ("AU", &["australia", "australien", "australisch", "australian", "オーストラリア"]),
    ("NZ", &["new zealand", "neuseeland", "ニュージーランド"]),
    ("BR", &["brazil", "brasilien", "brasilianisch", "brazilian", "ブラジル"]),
    ("AR", &["argentina", "argentinien", "argentinisch", "argentinian", "アルゼンチン"]),
    ("MX", &["mexico", "mexiko", "mexikanisch", "mexican", "メキシコ"]),
    ("CU", &["cuba", "kuba", "kubanisch", "cuban", "キューバ"]),
    ("CO", &["colombia", "kolumbien", "kolumbianisch", "colombian", "コロンビア"]),
    ("CL", &["chile", "chilenisch", "chilean", "チリ"]),
    ("JM", &["jamaica", "jamaika", "jamaikanisch", "jamaican", "ジャマイカ"]),
    ("NG", &["nigeria", "nigerianisch", "nigerian", "ナイジェリア"]),
    ("ZA", &["south africa", "südafrika", "südafrikanisch", "南アフリカ"]),
    ("EG", &["egypt", "ägypten", "ägyptisch", "egyptian", "エジプト"]),
    ("MA", &["morocco", "marokko", "marokkanisch", "moroccan", "モロッコ"]),
    ("SN", &["senegal", "senegalesisch", "セネガル"]),
    ("GH", &["ghana", "ガーナ"]),
    ("KE", &["kenya", "kenia", "ケニア"]),
    ("IL", &["israel", "israelisch", "israeli", "イスラエル"]),
    ("LB", &["lebanon", "libanon", "レバノン"]),
];

/// What the directory found, best first: how well the name answers what was
/// typed — all of it, its start, the start of one of its words, somewhere in it
/// — and how much the station is liked, by the directory's figures: its votes,
/// and how often it was played lately (`clickcount`, the last day;
/// `clicktrend`, how that is changing). On a log scale, so that a name that is
/// exactly what was typed still beats a popular one that only has it inside,
/// but a station with thousands of votes beats a nameless copy with none.
/// Letters found only inside a word come after all the rest, however liked. A
/// genre given to the station counts as a word of its name, a little below one.
/// One name the directory lists several times over is shown once, its best
/// entry.
fn ranked(query: &str, entries: Vec<(Station, Standing)>) -> Vec<Station> {
    let asked = folded(query);
    let mut found: Vec<(Rank, Station)> = entries
        .into_iter()
        .map(|(station, standing)| (standing.score(&asked, &folded(&station.name), &station.tags), station))
        .collect();
    found.sort_by(|a, b| a.1.silent.cmp(&b.1.silent).then(b.0.total_cmp(&a.0)));
    let mut names: Vec<String> = Vec::new();
    let mut stations = Vec::new();
    for (_, station) in found {
        let name = folded(&station.name);
        if names.contains(&name) {
            continue;
        }
        names.push(name);
        stations.push(station);
        if stations.len() == FOUND_SHOWN {
            break;
        }
    }
    stations
}

/// A name as searched for: small, letters and figures, one space between
/// words.
fn folded(name: &str) -> String {
    let spaced: String = name.chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect();
    spaced.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// Where a station found stands: whether its name has what was typed as a
/// word or the start of one, then its score. Best is greatest.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Rank {
    named: bool,
    score: f64,
}

impl Rank {
    fn total_cmp(&self, other: &Rank) -> std::cmp::Ordering {
        self.named.cmp(&other.named).then(self.score.total_cmp(&other.score))
    }
}

/// The directory's figures for one station.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Standing {
    votes: f64,
    clicks: f64,
    trend: f64,
}

impl Standing {
    fn score(&self, asked: &str, name: &str, tags: &str) -> Rank {
        let tagged = !asked.is_empty() && tags.split(',').map(folded).any(|tag| tag.split(' ').any(|word| word.starts_with(asked)) || tag.contains(asked));
        let matched = if asked.is_empty() {
            0.0
        } else if name == asked {
            4.0
        } else if name.starts_with(asked) {
            3.0
        } else if name.split(' ').any(|word| word.starts_with(asked)) || name.contains(&format!(" {asked}")) {
            2.0
        } else if tagged {
            1.5
        } else {
            0.0
        };
        let liked = self.votes.max(0.0).ln_1p() + 0.5 * (self.clicks + self.trend.max(0.0)).max(0.0).ln_1p();
        Rank { named: matched > 0.0 || asked.is_empty(), score: matched + liked }
    }
}

/// The stations in a directory answer, those the player can read, with
/// their figures, in its order.
fn entries_in(answer: &str) -> Vec<(Station, Standing)> {
    let Some(json) = crate::json::Json::parse(answer) else { return Vec::new() };
    let mut stations: Vec<(Station, Standing)> = Vec::new();
    for entry in json.as_array().unwrap_or(&[]) {
        let name = entry.text("name").unwrap_or("").trim();
        let url = entry.text("url_resolved").filter(|u| !u.is_empty()).or_else(|| entry.text("url")).unwrap_or("");
        let favicon = entry.text("favicon").map(str::trim).filter(|f| f.starts_with("http")).unwrap_or("");
        if name.is_empty() || url.is_empty() {
            continue;
        }
        if stations.iter().any(|(s, _)| s.url == url) {
            continue;
        }
        let said = |key: &str| entry.text(key).map(|t| t.trim().to_lowercase()).unwrap_or_default();
        let figure = |key: &str| entry.get(key).and_then(|n| n.as_number()).unwrap_or(0.0);
        let homepage = entry.text("homepage").map(str::trim).filter(|h| h.starts_with("http")).unwrap_or("");
        let station = Station {
            name: name.to_string(),
            url: url.to_string(),
            favicon: favicon.to_string(),
            country: said("country"),
            language: said("language"),
            tags: said("tags").split(',').map(str::trim).filter(|t| !t.is_empty()).take(4).collect::<Vec<_>>().join(", "),
            homepage: homepage.to_string(),
            countrycode: said("countrycode"),
            bitrate: Some(figure("bitrate")).filter(|b| *b > 0.0).map(|b| format!("{}", b.round() as u32)).unwrap_or_default(),
            codec: Some(said("codec")).filter(|c| c != "unknown").unwrap_or_default(),
            votes: figure("votes").max(0.0).round() as u32,
            silent: entry.get("lastcheckok").and_then(|n| n.as_number()).is_some_and(|ok| ok == 0.0),
            hls: entry.get("hls").and_then(|h| h.as_number()).is_some_and(|h| h != 0.0),
        };
        let standing = Standing { votes: figure("votes"), clicks: figure("clickcount"), trend: figure("clicktrend") };
        stations.push((station, standing));
    }
    stations
}

#[cfg(test)]
mod tests {

    /// A pause is not heard: the time it stood, over or going on, comes off.
    #[test]
    fn a_pause_is_not_heard() {
        let mut song = Aired::new("thou - blew");
        let now = song.at + chrono::Duration::seconds(100);
        assert_eq!(song.heard_secs(now, None), 100);
        song.paused_secs = 30;
        assert_eq!(song.heard_secs(now, None), 70);
        let since = song.at + chrono::Duration::seconds(90);
        assert_eq!(song.heard_secs(now, Some(since)), 60, "the pause going on counts too");
    }

    use super::*;

    #[test]
    fn a_page_names_its_square_icon_first() {
        let page = r#"<html><head>
            <link rel="icon" href="/favicon-16.png" sizes="16x16">
            <meta property="og:image" content="https://cdn.example/share.jpg">
            <link rel="apple-touch-icon" sizes="180x180" href="/touch.png">
            <link rel="icon" type="image/svg+xml" href="/logo.svg">
        </head></html>"#;
        let urls = icons_named(page, "https://www.example.org/radio/");
        assert_eq!(
            urls,
            [
                "https://www.example.org/touch.png",
                "https://cdn.example/share.jpg",
                "https://www.example.org/favicon-16.png",
                "https://www.example.org/apple-touch-icon.png",
                "https://www.example.org/favicon.ico",
            ]
        );
    }

    #[test]
    fn a_station_offered_once_and_let_go_of_stays_gone() {
        let old: Kept = toml::from_str("[[station]]\nname = \"nts 1\"\nurl = \"https://a\"\n").unwrap();
        assert!(old.offered.is_empty());
        let mut radio = Radio { kept: old.station, offered: old.offered, examples: old.examples, songs: old.song, found: Vec::new(), cursor: 0, resting: false, asked: String::new(), waiting: None, looking_up: Vec::new(), about: Vec::new(), asking_about: Vec::new(), site_pictures: Vec::new(), similar: None };
        let nts = Station { name: "nts 1".into(), url: "https://a".into(), ..Station::default() };
        radio.offer(nts.clone());
        assert_eq!(radio.kept.len(), 1, "kept already: not twice");
        radio.kept.retain(|s| s != &nts);
        assert!(radio.was_offered("nts 1"), "let go of, it is not offered again");
    }

    #[test]
    fn what_wikipedia_says_comes_in_the_window_language_first() {
        let text = |lang: &str| AboutText { lang: lang.into(), text: format!("in {lang}"), page: String::new() };
        let about = About { told: vec![text("de"), text("en")], asked: "2026-10-21".into() };
        assert_eq!(about.in_lang("de").unwrap().lang, "de");
        assert_eq!(about.in_lang("ja").unwrap().lang, "en");
        let only = About { told: vec![text("ja")], asked: String::new() };
        assert_eq!(only.in_lang("de").unwrap().lang, "ja");
        let back: About = toml::from_str(&toml::to_string(&about).unwrap()).unwrap();
        assert_eq!(back, about);
    }

    #[test]
    fn a_line_heard_parts_into_artist_and_title() {
        let heard = Aired::new("Midori Takada - Mr. Henri Rousseau's Dream");
        assert_eq!(heard.artist.as_deref(), Some("Midori Takada"));
        assert_eq!(heard.title, "Mr. Henri Rousseau's Dream");
        let show = Aired::new("The Breakfast Show w/ Flo");
        assert_eq!(show.artist, None);
        assert_eq!(show.title, "The Breakfast Show w/ Flo");
        assert_eq!(Aired::new(" - nothing").artist, None);
    }

    #[test]
    fn a_directory_answer_keeps_readable_streams_once() {
        let answer = r#"[
            {"name":"Montez Press Radio","url":"http://a/x","url_resolved":"https://stream.example/radio","hls":0,"country":"The United States Of America","countrycode":"US","bitrate":128,"tags":"art,talk, ,music","homepage":"https://radio.montezpress.com"},
            {"name":"Montez Press Radio","url":"http://a/x","url_resolved":"https://stream.example/radio","hls":0},
            {"name":"Some HLS","url":"https://b/live.m3u8","url_resolved":"https://b/live.m3u8","hls":1},
            {"name":"  ","url":"https://c","url_resolved":"https://c","hls":0}
        ]"#;
        let found: Vec<Station> = entries_in(answer).into_iter().map(|(station, _)| station).collect();
        assert_eq!(
            found,
            vec![Station {
                name: "Montez Press Radio".into(),
                url: "https://stream.example/radio".into(),
                country: "the united states of america".into(),
                tags: "art, talk, music".into(),
                homepage: "https://radio.montezpress.com".into(),
                countrycode: "us".into(),
                bitrate: "128".into(),
                ..Station::default()
            }, Station { name: "Some HLS".into(), url: "https://b/live.m3u8".into(), hls: true, ..Station::default() }]
        );
    }

    #[test]
    fn the_search_puts_the_name_asked_for_and_the_liked_first() {
        let answer = r#"[
            {"name":"Radio Antennas NTS mix","url":"https://a","votes":40,"clickcount":3},
            {"name":"NTS","url":"https://b","votes":0,"clickcount":0},
            {"name":"NTS Radio 1","url":"https://c","votes":4200,"clickcount":900},
            {"name":"nts radio 1","url":"https://d","votes":2,"clickcount":0},
            {"name":"Montents","url":"https://e","votes":9000,"clickcount":4000}
        ]"#;
        let names: Vec<String> = ranked("nts", entries_in(answer)).into_iter().map(|s| s.name).collect();
        assert_eq!(names, ["NTS Radio 1", "Radio Antennas NTS mix", "NTS", "Montents"]);
    }

    #[test]
    fn a_silent_station_is_found_last_and_says_how_it_sounds() {
        let answer = r#"[
            {"name":"NTS Radio 1","url":"https://c","votes":4200,"clickcount":900,"lastcheckok":0,"codec":"AAC","bitrate":128},
            {"name":"NTS Live","url":"https://b","votes":3,"clickcount":0,"lastcheckok":1,"codec":"UNKNOWN"},
            {"name":"NTS Two","url":"https://d","votes":1,"clickcount":0}
        ]"#;
        let found = ranked("nts", entries_in(answer));
        let names: Vec<&str> = found.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["NTS Live", "NTS Two", "NTS Radio 1"]);
        let silent = &found[2];
        assert!(silent.silent);
        assert_eq!((silent.codec.as_str(), silent.bitrate.as_str(), silent.votes), ("aac", "128", 4200));
        assert_eq!(found[0].codec, "", "„unknown\" is no codec");
    }

    #[test]
    fn an_address_is_a_station_named_after_its_server() {
        assert!(is_address("https://stream.example.org:8000/live.mp3"));
        assert!(is_address("  http://a.b/listen.pls "));
        assert!(!is_address("nts radio"));
        assert!(!is_address("https://"));
        assert!(!is_address("https://a b"));
        let station = Station::by_address("https://www.Stream.example.org:8000/live.mp3?x=1");
        assert_eq!(station.name, "stream.example.org");
        assert!(station.named_by_address());
        assert!(!Station { name: "jazz".into(), ..station }.named_by_address());
    }

    #[test]
    fn the_search_field_takes_an_address_as_the_one_station() {
        let mut radio = Radio { kept: Vec::new(), offered: Vec::new(), examples: false, songs: Vec::new(), found: Vec::new(), cursor: 0, resting: false, asked: String::new(), waiting: None, looking_up: Vec::new(), about: Vec::new(), asking_about: Vec::new(), site_pictures: Vec::new(), similar: None };
        radio.ask("https://radio.example.net/stream");
        assert!(radio.waiting.is_none(), "nothing asked of the directory");
        assert_eq!(radio.found, [Station::by_address("https://radio.example.net/stream")]);
        radio.named("https://radio.example.net/stream", "Example FM");
        assert_eq!(radio.found[0].name, "Example FM");
        radio.named("https://radio.example.net/stream", "Other");
        assert_eq!(radio.found[0].name, "Example FM");
    }

    #[test]
    fn a_playlist_of_stations_goes_out_and_comes_back() {
        let stations = vec![
            Station { name: "NTS 1".into(), url: "https://stream-relay-geo.ntslive.net/stream".into(), ..Station::default() },
            Station::by_address("http://kzsu-streams.stanford.edu/kzsu-1-128.mp3"),
        ];
        let m3u = playlist(&stations);
        assert!(m3u.starts_with("#EXTM3U\n#EXTINF:-1,NTS 1\nhttps://"));
        assert_eq!(read_playlist(&m3u), stations);
        let pls = "[playlist]\nFile2=http://b.example/two\nTitle1=One\nFile1=http://a.example/one\nFile3=nonsense\nFile4=http://a.example/one\nNumberOfEntries=4\n";
        let names: Vec<String> = read_playlist(pls).into_iter().map(|s| s.name).collect();
        assert_eq!(names, ["One", "b.example"]);
        let plain = "\u{feff}# my stations\r\nhttp://c.example/x\r\n\r\n#EXTINF:-1,Named\r\nhttps://d.example/y\r\n";
        let names: Vec<String> = read_playlist(plain).into_iter().map(|s| s.name).collect();
        assert_eq!(names, ["c.example", "Named"]);
    }

    #[test]
    fn a_kept_station_moves_where_it_is_dropped() {
        let mut list = vec!['a', 'b', 'c'];
        assert_eq!(moved(&mut list, 0, 3), Some(2));
        assert_eq!(list, ['b', 'c', 'a']);
        assert_eq!(moved(&mut list, 2, 0), Some(0));
        assert_eq!(list, ['a', 'b', 'c']);
        assert_eq!(moved(&mut list, 1, 2), None);
        assert_eq!(moved(&mut list, 1, 1), None);
        assert_eq!(list, ['a', 'b', 'c']);
    }

    #[test]
    fn a_search_reads_the_land_out_of_its_words() {
        assert_eq!(understood("jazz japan"), ("jazz".into(), Some("JP")));
        assert_eq!(understood("Japanische Jazz"), ("jazz".into(), Some("JP")));
        assert_eq!(understood("deutscher hip hop"), ("hip hop".into(), Some("DE")));
        assert_eq!(understood("South Korea indie"), ("indie".into(), Some("KR")));
        assert_eq!(understood("日本のジャズ"), ("ジャズ".into(), Some("JP")));
        assert_eq!(understood("japan"), ("".into(), Some("JP")));
        assert_eq!(understood("radio paradise"), ("radio paradise".into(), None));
        assert_eq!(understood("island records"), ("island records".into(), None));
    }

    #[test]
    fn a_genre_counts_as_a_word_of_the_name() {
        let station = |name: &str, tags: &str| Station { name: name.into(), url: format!("http://{name}"), tags: tags.into(), ..Station::default() };
        let liked = Standing { votes: 50.0, ..Standing::default() };
        let names: Vec<String> = ranked(
            "jazz",
            vec![(station("Somewhere FM", "pop"), liked), (station("Tokyo Night", "smooth jazz, lounge"), liked), (station("Jazz Sakura", ""), Standing::default())],
        )
        .into_iter()
        .map(|s| s.name)
        .collect();
        assert_eq!(names, ["Tokyo Night", "Jazz Sakura", "Somewhere FM"]);
    }

    #[test]
    fn a_similar_station_shares_its_genres_and_its_tongue() {
        let station = |name: &str, tags: &str, language: &str| Station {
            name: name.into(),
            url: format!("http://{name}"),
            tags: tags.into(),
            language: language.into(),
            ..Station::default()
        };
        let liked = Standing { votes: 500.0, ..Standing::default() };
        let quiet = Standing::default();
        let jazz = station("Jazz Sakura", "jazz, smooth jazz", "japanese");
        let found = similar_ranked(
            &jazz,
            vec![
                (station("Jazz Sakura", "jazz", "japanese"), liked),
                (station("Big Pop", "pop", "english"), liked),
                (station("Tokyo Smooth", "smooth jazz, jazz", "japanese"), quiet),
                (station("Jazz FM", "jazz", "english"), liked),
            ],
        );
        let names: Vec<&str> = found.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["Tokyo Smooth", "Jazz FM", "Big Pop"]);
    }
}
