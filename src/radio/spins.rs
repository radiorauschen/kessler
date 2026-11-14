use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::Station;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spin {
    pub artist: String,
    pub song: String,
}

impl Spin {
    pub fn said(&self) -> String {
        if self.artist.is_empty() {
            self.song.clone()
        } else {
            format!("{} - {}", self.artist, self.song)
        }
    }
}

const KNOWN: [(&str, &str); 1] = [("kzsu", "kzsu")];

pub fn of(station: &Station) -> Option<&'static str> {
    let url = station.url.to_ascii_lowercase();
    let page = station.homepage.to_ascii_lowercase();
    let name = station.name.to_ascii_lowercase();
    KNOWN
        .iter()
        .find(|(mark, _)| url.contains(mark) || page.contains(mark) || name.contains(mark))
        .map(|(_, name)| *name)
}

fn endpoint(name: &str) -> String {
    let base = std::env::var("KESSLER_SPINITRON_ENDPOINT")
        .unwrap_or_else(|_| "https://widgets.spinitron.com/widget/now-playing-v2".to_string());
    format!("{base}?station={name}&num=5&sharing=0&player=0&cover=0&merch=0&meta=0")
}

fn station_page(name: &str) -> String {
    let base = std::env::var("KESSLER_SPINITRON_PAGE").unwrap_or_else(|_| "https://spinitron.com".to_string());
    format!("{base}/{}/", name.to_ascii_uppercase())
}

fn ask_both(name: &str) -> Result<Vec<Spin>, String> {
    let mut trouble = String::new();
    if name == "kzsu" {
        let url = kzsu_on_now_endpoint();
        match crate::library::coverart::fetch_text(&url) {
            Ok(text) => match crate::json::Json::parse(&text).and_then(|doc| kzsu_on_now(&doc)) {
                Some(spin) => return Ok(vec![spin]),
                None => trouble = format!("{url}: nichts gefunden"),
            },
            Err(e) => trouble = format!("{url}: {e}"),
        }
    }
    for url in [endpoint(name), station_page(name)] {
        match crate::library::coverart::fetch_text(&url) {
            Ok(page) => {
                let spins = spins_in(&page);
                if !spins.is_empty() {
                    return Ok(spins);
                }
                trouble = format!("{url}: nichts gefunden");
            }
            Err(e) => trouble = format!("{url}: {e}"),
        }
    }
    Err(trouble)
}

fn kzsu_on_now_endpoint() -> String {
    std::env::var("KESSLER_KZSU_ONNOW").unwrap_or_else(|_| "https://kzsu.stanford.edu/api/shows/onnow/?focusevent=1".to_string())
}

fn kzsu_on_now(doc: &crate::json::Json) -> Option<Spin> {
    let on = doc.get("onnow").or(Some(doc).filter(|d| d.get("track_title").is_some()))?;
    let word = |k: &str| on.text(k).map(str::trim).unwrap_or("").to_string();
    let (artist, song) = (word("track_artist"), word("track_title"));
    (!song.is_empty()).then_some(Spin { artist, song })
}

const EVERY: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
enum From {
    Spinitron(&'static str),
    Platform { homepage: String },
}

type Answer = Result<(Option<&'static str>, Vec<Spin>), String>;

#[derive(Debug)]
pub struct Watch {
    pub url: String,
    from: From,
    asking: Option<mpsc::Receiver<Answer>>,
    asked_at: Option<Instant>,
    pub latest: Option<Spin>,
    pub first: Option<Result<(Option<&'static str>, usize), String>>,
    answered: bool,
    idle: bool,
}

impl Watch {
    pub fn of(station: &Station) -> Option<Watch> {
        let from = match of(station) {
            Some(name) => From::Spinitron(name),
            None if super::programme::Source::of(&station.url, &station.name).is_some() => return None,
            None => From::Platform { homepage: station.homepage.clone() },
        };
        Some(Watch { url: station.url.clone(), from, asking: None, asked_at: None, latest: None, first: None, answered: false, idle: false })
    }

    pub fn tend(&mut self) -> bool {
        let mut changed = false;
        if let Some(asking) = &self.asking {
            match asking.try_recv() {
                Ok(answer) => {
                    self.asking = None;
                    if !self.answered {
                        self.answered = true;
                        self.first = Some(answer.as_ref().map(|(via, spins)| (*via, spins.len())).map_err(Clone::clone));
                    }
                    self.idle |= matches!(answer, Ok((None, _)));
                    if let Some(newest) = answer.ok().and_then(|(_, spins)| spins.into_iter().next()) {
                        changed = self.latest.as_ref() != Some(&newest);
                        self.latest = Some(newest);
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => self.asking = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if !self.idle && self.asking.is_none() && self.asked_at.is_none_or(|at| at.elapsed() >= EVERY) {
            self.asked_at = Some(Instant::now());
            let (tell, answer) = mpsc::channel();
            let (from, url) = (self.from.clone(), self.url.clone());
            std::thread::Builder::new()
                .name("kessler-radio-spins".into())
                .spawn(move || {
                    let answer = match from {
                        From::Spinitron(name) => ask_both(name).map(|spins| (Some("spinitron"), spins)),
                        From::Platform { homepage } => match super::platform::find(&url, &homepage) {
                            None => Ok((None, Vec::new())),
                            Some(platform) => super::platform::ask(&platform).map(|(song, _)| (Some(platform.word()), song.into_iter().collect())),
                        },
                    };
                    let _ = tell.send(answer);
                })
                .ok();
            self.asking = Some(answer);
        }
        changed
    }
}

pub fn spins_in(page: &str) -> Vec<Spin> {
    let from_data = spins_from_data(page);
    if !from_data.is_empty() {
        return from_data;
    }
    let artists = marked(page, "artist");
    let songs = marked(page, "song");
    artists
        .into_iter()
        .zip(songs)
        .filter(|(_, song)| !song.is_empty())
        .map(|(artist, song)| Spin { artist, song })
        .collect()
}

fn spins_from_data(page: &str) -> Vec<Spin> {
    let mut spins = Vec::new();
    let mut rest = page;
    while let Some(at) = rest.find("data-spin=") {
        rest = &rest[at + "data-spin=".len()..];
        let Some(quote) = rest.chars().next().filter(|c| *c == '\'' || *c == '"') else { continue };
        let Some(end) = rest[1..].find(quote) else { break };
        let json = unescape(&rest[1..1 + end]);
        rest = &rest[1 + end..];
        let Some(doc) = crate::json::Json::parse(&json) else { continue };
        let text = |keys: &[&str]| keys.iter().find_map(|k| doc.text(k)).unwrap_or("").trim().to_string();
        let song = text(&["s", "song"]);
        if !song.is_empty() {
            spins.push(Spin { artist: text(&["a", "artist"]), song });
        }
    }
    spins
}

fn marked(page: &str, what: &str) -> Vec<String> {
    let mut found = Vec::new();
    let lower = page.to_ascii_lowercase();
    let mut from = 0;
    while let Some(at) = lower[from..].find("class=") {
        let at = from + at + "class=".len();
        from = at;
        let Some(quote) = lower[at..].chars().next().filter(|c| *c == '\'' || *c == '"') else { continue };
        let Some(end) = lower[at + 1..].find(quote) else { break };
        let classes = &lower[at + 1..at + 1 + end];
        let named = classes.split_whitespace().any(|c| c == what || c == format!("spin-{what}"));
        if !named {
            continue;
        }
        let Some(open_end) = page[at..].find('>') else { break };
        let inner = &page[at + open_end + 1..];
        let close = inner.find("</").unwrap_or(inner.len());
        let words = strip_tags(&inner[..close.min(400)]);
        found.push(words);
    }
    found
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
    let out = unescape(&out);
    out.trim().trim_matches(['“', '”', '"']).trim().to_string()
}

fn unescape(text: &str) -> String {
    text.replace("&quot;", "\"")
        .replace("&#039;", "'")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
        .replace("&ldquo;", "“")
        .replace("&rdquo;", "”")
        .replace("&#8220;", "“")
        .replace("&#8221;", "”")
        .replace("&rsquo;", "’")
        .replace("&#8217;", "’")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kzsu_is_known_by_its_stream() {
        let kzsu = Station { name: "kzsu stanford".into(), url: "http://kzsu-streams.stanford.edu/kzsu-1-128.mp3".into(), ..Station::default() };
        assert_eq!(of(&kzsu), Some("kzsu"));
        let other = Station { url: "https://radio.example/live".into(), ..Station::default() };
        assert_eq!(of(&other), None);
    }

    #[test]
    fn songs_are_read_from_the_marked_words() {
        let page = r#"<div class="spin-item"><span class="spin-time">2:35 PM</span>
            <span class="artist">Midori Takada</span> <span class="song">&ldquo;Mr. Henri Rousseau&#039;s Dream&rdquo;</span>
            <span class="release">Through the Looking Glass</span></div>
            <div class="spin-item"><span class="artist">Hiroshi Yoshimura</span><span class="song">Blink</span></div>"#;
        let spins = spins_in(page);
        assert_eq!(spins[0], Spin { artist: "Midori Takada".into(), song: "Mr. Henri Rousseau's Dream".into() });
        assert_eq!(spins[1].said(), "Hiroshi Yoshimura - Blink");
    }

    #[test]
    fn songs_are_read_from_the_players_data_first() {
        let page = r#"<div class="spin-item" data-spin='{"i":"1","a":"Björk","s":"Hyperballad","r":"Post"}'>
            <span class="artist">ignored</span><span class="song">ignored</span></div>"#;
        assert_eq!(spins_in(page), [Spin { artist: "Björk".into(), song: "Hyperballad".into() }]);
    }

    #[test]
    fn kzsu_names_its_song_on_now() {
        let doc = crate::json::Json::parse(r#"{"onnow":{"name":"Zootopia","track_artist":"Future Islands","track_title":"North Star "}}"#).unwrap();
        assert_eq!(kzsu_on_now(&doc).map(|s| s.said()).as_deref(), Some("Future Islands - North Star"));
        assert_eq!(kzsu_on_now(&crate::json::Json::parse(r#"{"onnow":{"track_title":""}}"#).unwrap()), None);
        let alone = crate::json::Json::parse(r#"{"airname":"I. Robot","name":"Zootopia","show_end":"2026-10-21 20:00:00","track_artist":"Speedometer","track_title":"Soul Safari "}"#).unwrap();
        assert_eq!(kzsu_on_now(&alone).map(|s| s.said()).as_deref(), Some("Speedometer - Soul Safari"));
        assert_eq!(super::super::programme::kzsu_titles(&alone), ["Zootopia"]);
    }
}
