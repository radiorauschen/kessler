use std::sync::Mutex;

use chrono::{DateTime, Local};

use super::programme::Show;
use super::spins::Spin;
use crate::json::Json;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Platform {
    AzuraCast { ask: String, path: String },
    LibreTime { site: String },
    Icecast { ask: String, path: String },
}

impl Platform {
    pub fn word(&self) -> &'static str {
        match self {
            Platform::AzuraCast { .. } => "azuracast",
            Platform::LibreTime { .. } => "libretime",
            Platform::Icecast { .. } => "icecast",
        }
    }
}

static FOUND: Mutex<Vec<(String, Option<Platform>)>> = Mutex::new(Vec::new());

fn found(stream: &str) -> Option<Option<Platform>> {
    FOUND.lock().ok()?.iter().find(|(s, _)| s == stream).map(|(_, p)| p.clone())
}

pub fn find(stream: &str, homepage: &str) -> Option<Platform> {
    if let Some(known) = found(stream) {
        return known;
    }
    let platform = look_for(stream, homepage, |url| {
        crate::library::coverart::fetch_text(url).ok().and_then(|text| Json::parse(&text))
    });
    if let Ok(mut all) = FOUND.lock() {
        all.push((stream.to_string(), platform.clone()));
    }
    platform
}

pub fn programme_of(stream: &str) -> Option<Platform> {
    found(stream)?.filter(|p| !matches!(p, Platform::Icecast { .. }))
}

fn parts(url: &str) -> Option<(String, String, String)> {
    let (scheme, rest) = url.split_once("://")?;
    let rest = rest.split(['?', '#']).next().unwrap_or("");
    let (authority, path) = rest.split_once('/').map_or((rest, ""), |(a, p)| (a, p));
    let host = authority.rsplit('@').next().unwrap_or(authority);
    let bare = host.split(':').next().unwrap_or(host);
    if bare.is_empty() {
        return None;
    }
    Some((format!("{scheme}://{host}"), format!("https://{bare}"), format!("/{path}")))
}

fn look_for(stream: &str, homepage: &str, read: impl Fn(&str) -> Option<Json>) -> Option<Platform> {
    let (origin, site, path) = parts(stream)?;
    let mut sites = vec![site];
    if let Some((_, home, _)) = parts(homepage) {
        if !sites.contains(&home) {
            sites.push(home);
        }
    }
    for site in &sites {
        let code = path.strip_prefix("/listen/").and_then(|p| p.split('/').next()).filter(|c| !c.is_empty());
        let ask = code.map_or_else(|| format!("{site}/api/nowplaying"), |c| format!("{site}/api/nowplaying/{c}"));
        if read(&ask).is_some_and(|doc| azuracast_station(&doc, &path).is_some()) {
            return Some(Platform::AzuraCast { ask, path: path.clone() });
        }
        let ask = libretime_address(site);
        if read(&ask).is_some_and(|doc| doc.get("tracks").is_some() || doc.get("shows").is_some()) {
            return Some(Platform::LibreTime { site: site.clone() });
        }
    }
    let ask = format!("{origin}/status-json.xsl");
    if read(&ask).is_some_and(|doc| icecast_source(&doc, &path).is_some()) {
        return Some(Platform::Icecast { ask, path });
    }
    None
}

fn libretime_address(site: &str) -> String {
    format!("{site}/api/live-info-v2?timezone=UTC&days=1&shows=6")
}

pub fn ask(platform: &Platform) -> Result<(Option<Spin>, Vec<Show>), String> {
    let read = |url: &str| {
        crate::library::coverart::fetch_text(url)
            .map_err(|e| e.to_string())
            .and_then(|text| Json::parse(&text).ok_or_else(|| format!("{url}: unreadable")))
    };
    Ok(match platform {
        Platform::AzuraCast { ask, path } => {
            let doc = read(ask)?;
            let station = azuracast_station(&doc, path);
            (station.and_then(azuracast_song), station.map(azuracast_shows).unwrap_or_default())
        }
        Platform::LibreTime { site } => {
            let doc = read(&libretime_address(site))?;
            (libretime_song(&doc), libretime_shows(&doc))
        }
        Platform::Icecast { ask, path } => (icecast_source(&read(ask)?, path).and_then(icecast_song), Vec::new()),
    })
}

pub fn shows(platform: &Platform) -> Result<Vec<Show>, String> {
    ask(platform).map(|(_, shows)| shows)
}

fn spin(artist: &str, song: &str) -> Option<Spin> {
    let (artist, song) = (artist.trim(), song.trim());
    if song.is_empty() {
        return None;
    }
    if artist.is_empty() {
        if let Some((a, s)) = song.split_once(" - ") {
            return Some(Spin { artist: a.trim().to_string(), song: s.trim().to_string() });
        }
    }
    Some(Spin { artist: artist.to_string(), song: song.to_string() })
}

fn azuracast_station<'a>(doc: &'a Json, path: &str) -> Option<&'a Json> {
    let is_one = |s: &&Json| s.get("now_playing").is_some() && s.get("station").is_some();
    if let Some(all) = doc.as_array() {
        let all: Vec<&Json> = all.iter().filter(is_one).collect();
        let ours = all.iter().copied().find(|s| {
            let station = s.get("station");
            let urls = station.map(|st| {
                let mut urls: Vec<&str> = st.text("listen_url").into_iter().collect();
                urls.extend(st.list("mounts").iter().filter_map(|m| m.text("url")));
                urls
            });
            urls.unwrap_or_default().iter().any(|u| parts(u).is_some_and(|(_, _, p)| p == path))
        });
        return ours.or(if all.len() == 1 { all.first().copied() } else { None });
    }
    Some(doc).filter(is_one)
}

fn azuracast_shows(station: &Json) -> Vec<Show> {
    let as_show = |item: &Json, at_key: &str| -> Option<Show> {
        let song = item.get("song")?;
        let song_title = song.text("title").map(str::trim).filter(|t| !t.is_empty())?;
        let artist = song.text("artist").map(str::trim).unwrap_or("").to_string();
        let title = if artist.is_empty() { song_title.to_string() } else { format!("{artist} — {song_title}") };
        let at = item.get(at_key).or(item.get("played_at")).and_then(Json::as_number)?;
        let start = chrono::DateTime::from_timestamp(at as i64, 0)?.with_timezone(&Local);
        let length = item.get("duration").and_then(Json::as_number).unwrap_or(0.0).max(1.0);
        let picture = song.text("art").filter(|u| u.starts_with("http")).unwrap_or("").to_string();
        Some(Show { title, start, end: start + chrono::Duration::seconds(length as i64), picture, about: artist, links: Vec::new() })
    };
    let mut shows: Vec<Show> = station.list("song_history").iter().take(3).filter_map(|i| as_show(i, "played_at")).collect();
    shows.extend(station.get("now_playing").and_then(|i| as_show(i, "played_at")));
    if let Some(next) = station.get("playing_next").and_then(|i| as_show(i, "cued_at")) {
        let now_end = shows.last().map(|s| s.end);
        let next = match now_end {
            Some(end) if end > next.start => Show { start: end, end: end + (next.end - next.start), ..next },
            _ => next,
        };
        shows.push(next);
    }
    shows.sort_by_key(|s| s.start);
    shows
}

fn azuracast_song(station: &Json) -> Option<Spin> {
    let song = station.get("now_playing")?.get("song")?;
    spin(song.text("artist").unwrap_or(""), song.text("title").or(song.text("text")).unwrap_or(""))
}

fn icecast_source<'a>(doc: &'a Json, path: &str) -> Option<&'a Json> {
    let sources = doc.get("icestats")?.get("source")?;
    let all: Vec<&Json> = match sources.as_array() {
        Some(all) => all.iter().collect(),
        None => vec![sources],
    };
    let ours = all.iter().copied().find(|s| s.text("listenurl").and_then(parts).is_some_and(|(_, _, p)| p == path));
    ours.or(if all.len() == 1 { all.first().copied() } else { None })
}

fn icecast_song(source: &Json) -> Option<Spin> {
    spin(source.text("artist").unwrap_or(""), source.text("title").unwrap_or(""))
}

fn libretime_song(doc: &Json) -> Option<Spin> {
    let current = doc.get("tracks")?.get("current")?;
    let meta = current.get("metadata");
    let field = |k: &str| meta.and_then(|m| m.text(k)).unwrap_or("");
    spin(field("artist_name"), Some(field("track_title")).filter(|t| !t.is_empty()).or(current.text("name")).unwrap_or(""))
}

fn libretime_shows(doc: &Json) -> Vec<Show> {
    let Some(shows) = doc.get("shows") else { return Vec::new() };
    let when = |s: &Json, k: &str| -> Option<DateTime<Local>> {
        let at = chrono::NaiveDateTime::parse_from_str(s.text(k)?, "%Y-%m-%d %H:%M:%S").ok()?;
        Some(chrono::TimeZone::from_utc_datetime(&chrono::Utc, &at).with_timezone(&Local))
    };
    let mut all: Vec<&Json> = shows.get("current").filter(|c| c.get("name").is_some()).into_iter().collect();
    all.extend(shows.list("next"));
    let mut out: Vec<Show> = all
        .into_iter()
        .filter_map(|s| {
            let title = s.text("name").map(str::trim).filter(|t| !t.is_empty())?.to_string();
            let (start, end) = (when(s, "starts")?, when(s, "ends")?);
            let picture = s.text("image_path").filter(|u| u.starts_with("http")).unwrap_or("").to_string();
            let about = s.text("description").map(str::trim).unwrap_or("").to_string();
            let links = s.text("url").filter(|u| u.starts_with("http")).map(|u| vec![(super::programme::site_word(u), u.to_string())]).unwrap_or_default();
            Some(Show { title, start, end, picture, about, links })
        })
        .collect();
    out.sort_by_key(|s| s.start);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answers(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<Json> {
        let pairs: Vec<(String, String)> = pairs.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
        move |url: &str| pairs.iter().find(|(a, _)| a == url).and_then(|(_, b)| Json::parse(b))
    }

    #[test]
    fn a_stream_says_where_it_is_asked() {
        assert_eq!(
            parts("http://radio.example:8000/live.mp3?x=1"),
            Some(("http://radio.example:8000".into(), "https://radio.example".into(), "/live.mp3".into()))
        );
        assert_eq!(parts("https://a.example").map(|p| p.2), Some("/".to_string()));
        assert_eq!(parts("nonsense"), None);
    }

    #[test]
    fn azuracast_is_found_by_its_listening_path_and_names_the_song() {
        let read = answers(&[(
            "https://radio.example/api/nowplaying/kaffee",
            r#"{"station":{"shortcode":"kaffee","listen_url":"https://radio.example/listen/kaffee/radio.mp3"},
                "now_playing":{"song":{"artist":"Hiroshi Yoshimura","title":"Creek","art":"https://radio.example/art.jpg"}}}"#,
        )]);
        let found = look_for("https://radio.example/listen/kaffee/radio.mp3", "", &read).unwrap();
        assert_eq!(found.word(), "azuracast");
        let Platform::AzuraCast { ask, path } = &found else { unreachable!() };
        let song = azuracast_station(&read(ask).unwrap(), path).and_then(azuracast_song).unwrap();
        assert_eq!(song.said(), "Hiroshi Yoshimura - Creek");
    }

    #[test]
    fn azuracast_says_what_comes_next() {
        let doc = Json::parse(
            r#"{"station":{"listen_url":"x"},
                "now_playing":{"played_at":1792700000,"duration":200,"song":{"artist":"Hiroshi Yoshimura","title":"Creek"}},
                "playing_next":{"cued_at":1792700190,"duration":300,"song":{"artist":"Midori Takada","title":"Mr. Henri Rousseau's Dream"}},
                "song_history":[{"played_at":1792699800,"duration":200,"song":{"artist":"Satoshi Ashikawa","title":"Still Space"}}]}"#,
        )
        .unwrap();
        let shows = azuracast_shows(&doc);
        assert_eq!(shows.iter().map(|s| s.title.as_str()).collect::<Vec<_>>(), ["Satoshi Ashikawa — Still Space", "Hiroshi Yoshimura — Creek", "Midori Takada — Mr. Henri Rousseau's Dream"]);
        assert_eq!(shows[2].start, shows[1].end);
        assert_eq!((shows[2].end - shows[2].start).num_seconds(), 300);
        assert_eq!(shows[2].about, "Midori Takada");
    }

    #[test]
    fn an_azuracast_server_of_several_picks_the_station_by_its_stream() {
        let doc = Json::parse(
            r#"[{"station":{"listen_url":"https://r.example/listen/a/a.mp3","mounts":[]},"now_playing":{"song":{"artist":"A","title":"one"}}},
                {"station":{"listen_url":"https://r.example/listen/b/b.mp3","mounts":[{"url":"https://r.example/radio/8010/b.mp3"}]},"now_playing":{"song":{"artist":"B","title":"two"}}}]"#,
        )
        .unwrap();
        assert_eq!(azuracast_station(&doc, "/radio/8010/b.mp3").and_then(azuracast_song).map(|s| s.said()).as_deref(), Some("B - two"));
        assert!(azuracast_station(&doc, "/other.mp3").is_none());
    }

    #[test]
    fn libretime_gives_the_song_and_a_programme_in_our_time() {
        let read = answers(&[(
            "https://radio.example/api/live-info-v2?timezone=UTC&days=1&shows=6",
            r#"{"station":{"timezone":"UTC"},
                "tracks":{"current":{"name":"Ana Roxanne - Suite pour l'invisible","metadata":{"artist_name":"","track_title":""}}},
                "shows":{"current":{"name":"Morning Drift","starts":"2026-10-22 08:00:00","ends":"2026-10-22 10:00:00","description":"slow","url":"https://radio.example/drift"},
                         "next":[{"name":"Lunch Tapes","starts":"2026-10-22 10:00:00","ends":"2026-10-22 11:00:00"}]}}"#,
        )]);
        let found = look_for("https://stream.example:8443/main", "https://radio.example/", &read).unwrap();
        assert_eq!(found, Platform::LibreTime { site: "https://radio.example".into() });
        let doc = read(&libretime_address("https://radio.example")).unwrap();
        assert_eq!(libretime_song(&doc).map(|s| s.said()).as_deref(), Some("Ana Roxanne - Suite pour l'invisible"));
        let shows = libretime_shows(&doc);
        assert_eq!(shows.iter().map(|s| s.title.as_str()).collect::<Vec<_>>(), ["Morning Drift", "Lunch Tapes"]);
        assert_eq!(shows[0].start.with_timezone(&chrono::Utc).format("%H:%M").to_string(), "08:00");
        assert_eq!(shows[0].links.len(), 1);
    }

    #[test]
    fn icecast_names_the_title_of_the_streams_mount() {
        let read = answers(&[(
            "http://radio.example:8000/status-json.xsl",
            r#"{"icestats":{"source":[{"listenurl":"http://radio.example:8000/low","title":"x"},
                                      {"listenurl":"http://radio.example:8000/high","title":"Midori Takada - Mr. Henri Rousseau's Dream"}]}}"#,
        )]);
        let found = look_for("http://radio.example:8000/high", "", &read).unwrap();
        let Platform::Icecast { ask, path } = &found else { panic!("{found:?}") };
        assert_eq!(icecast_source(&read(ask).unwrap(), path).and_then(icecast_song).map(|s| (s.artist, s.song)), Some(("Midori Takada".into(), "Mr. Henri Rousseau's Dream".into())));
    }

    #[test]
    fn a_station_on_no_known_platform_is_none() {
        assert_eq!(look_for("https://radio.example/live", "https://radio.example", answers(&[])), None);
    }
}
