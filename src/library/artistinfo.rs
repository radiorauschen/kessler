use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::coverart::{encode, fetch, fetch_text};
use crate::error::{KesslerError, Result};
use crate::json::Json;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    pub label: String,
    pub url: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Found {
    pub mbid: String,
    pub kind: String,
    pub origin: String,
    pub active: String,
    pub disambiguation: String,
    pub about: String,
    pub about_from: String,
    pub told: Vec<Told>,
    pub asked: Vec<String>,
    pub tags: Vec<String>,
    pub similar: Vec<String>,
    pub kin: Vec<Kin>,
    pub links: Vec<Link>,
    pub members: Vec<Tie>,
    pub groups: Vec<Tie>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Told {
    pub lang: String,
    pub text: String,
    pub from: String,
}

impl Found {
    pub fn about_in(&self, lang: &str) -> (&str, &str, Option<&str>) {
        if let Some(told) = self.told.iter().find(|t| t.lang == lang && !t.text.is_empty()) {
            return (&told.text, &told.from, None);
        }
        let other = self
            .told
            .iter()
            .find(|t| t.text == self.about)
            .map(|t| t.lang.as_str())
            .filter(|l| *l != lang);
        (&self.about, &self.about_from, other)
    }

    pub fn lacks(&self, lang: &str) -> bool {
        !self.asked.iter().any(|l| l == lang) && !self.told.iter().any(|t| t.lang == lang && !t.text.is_empty())
    }
}

pub fn summary(found: &Found, pictured: bool) -> String {
    let mut said: Vec<String> = Vec::new();
    if !found.told.is_empty() {
        let langs: Vec<&str> = found.told.iter().map(|t| t.lang.as_str()).collect();
        said.push(tr!("text ({})", langs.join(", ")));
    } else if !found.about.is_empty() {
        said.push(tr!("text").to_string());
    }
    if pictured {
        said.push(tr!("bild").to_string());
    }
    if !found.members.is_empty() {
        said.push(tr!("{} mitglieder", found.members.len()));
    }
    if !found.groups.is_empty() {
        said.push(tr!("{} gruppen", found.groups.len()));
    }
    if !found.similar.is_empty() {
        said.push(tr!("{} ähnliche", found.similar.len()));
    }
    if said.is_empty() && !found.mbid.is_empty() {
        said.push(tr!("erkannt, sonst wenig").to_string());
    }
    said.join(" · ")
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Kin {
    pub name: String,
    pub weight: f32,
}

const KIN_ASKED: usize = 30;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tie {
    pub name: String,
    pub mbid: String,
    pub years: String,
    pub current: bool,
    pub also: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Own {
    pub origin: String,
    pub active: String,
    pub style: String,
    pub note: String,
    pub link: String,
    pub picture: bool,
}

impl Own {
    pub fn is_empty(&self) -> bool {
        self.origin.is_empty()
            && self.active.is_empty()
            && self.style.is_empty()
            && self.note.is_empty()
            && self.link.is_empty()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ArtistInfo {
    pub name: String,
    pub looked_up: String,
    pub recognised: bool,
    pub found: Found,
    pub own: Own,
}

impl ArtistInfo {
    pub fn origin(&self) -> &str {
        pick(&self.own.origin, &self.found.origin)
    }

    pub fn active(&self) -> &str {
        pick(&self.own.active, &self.found.active)
    }

    pub fn style(&self) -> String {
        if self.own.style.is_empty() {
            self.found.tags.iter().take(4).cloned().collect::<Vec<_>>().join(", ")
        } else {
            self.own.style.clone()
        }
    }
}

fn pick<'a>(own: &'a str, found: &'a str) -> &'a str {
    if own.trim().is_empty() {
        found
    } else {
        own
    }
}

pub fn key(artist_key: &str) -> u64 {
    super::artwork::key(artist_key, "\u{2}artist")
}

pub fn info_path(dir: &Path, key: u64) -> PathBuf {
    dir.join(format!("{key:016x}.toml"))
}

pub fn picture_path(dir: &Path, key: u64) -> PathBuf {
    dir.join(format!("{key:016x}.jpg"))
}

pub fn load(dir: &Path, key: u64) -> Option<ArtistInfo> {
    let text = std::fs::read_to_string(info_path(dir, key)).ok()?;
    toml::from_str(&text).ok()
}

pub fn save(dir: &Path, key: u64, info: &ArtistInfo) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    let text = toml::to_string_pretty(info)
        .map_err(|e| KesslerError::Metadata(format!("artist info: {e}")))?;
    let path = info_path(dir, key);
    let temp = path.with_extension("toml-tmp");
    std::fs::write(&temp, text)?;
    std::fs::rename(&temp, &path)?;
    Ok(())
}

pub fn store_picture(dir: &Path, key: u64, raw: &[u8]) -> Result<bool> {
    let Ok(image) = image::load_from_memory(raw) else {
        return Ok(false);
    };
    let side = image.width().min(image.height());
    if side < 32 {
        return Ok(false);
    }
    let square = image.crop_imm(
        (image.width() - side) / 2,
        ((image.height() - side) / 3).min(image.height() - side),
        side,
        side,
    );
    let mut bytes = Vec::new();
    square
        .write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
        .map_err(|e| KesslerError::Metadata(format!("artist picture: {e}")))?;
    let Some(jpeg) = super::artwork::shrink_to_jpeg(&bytes, 400, 85) else {
        return Ok(false);
    };
    std::fs::create_dir_all(dir)?;
    std::fs::write(picture_path(dir, key), jpeg)?;
    Ok(true)
}

pub fn take_in(
    dir: &Path,
    key: u64,
    name: &str,
    found: Found,
    recognised: bool,
    picture: Option<Vec<u8>>,
) -> Result<(ArtistInfo, bool)> {
    let mut info = load(dir, key).unwrap_or_default();
    info.name = name.to_string();
    info.found = found;
    info.recognised = recognised;
    info.looked_up = chrono::Local::now().format("%Y-%m-%d").to_string();
    let mut pictured = false;
    if let Some(bytes) = picture.filter(|_| !info.own.picture) {
        pictured = store_picture(dir, key, &bytes)?;
    }
    save(dir, key, &info)?;
    Ok((info, pictured))
}

pub fn looked_up(dir: &Path, key: u64) -> bool {
    load(dir, key).is_some_and(|i| !i.looked_up.is_empty())
}

pub fn to_markdown(info: &ArtistInfo, picture: Option<&str>) -> String {
    let mut out = String::new();
    let quoted = |s: &str| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""));
    out.push_str("---\n");
    out.push_str(&format!("name: {}\n", quoted(&info.name)));
    for (key, value) in [
        ("kind", info.found.kind.as_str()),
        ("origin", info.origin()),
        ("active", info.active()),
        ("mbid", info.found.mbid.as_str()),
        ("looked_up", info.looked_up.as_str()),
    ] {
        if !value.is_empty() {
            out.push_str(&format!("{key}: {}\n", quoted(value)));
        }
    }
    let style = info.style();
    if !style.is_empty() {
        out.push_str(&format!("style: {}\n", quoted(&style)));
    }
    if !info.found.tags.is_empty() {
        let tags: Vec<String> = info.found.tags.iter().map(|t| quoted(t)).collect();
        out.push_str(&format!("tags: [{}]\n", tags.join(", ")));
    }
    out.push_str("---\n\n");
    out.push_str(&format!("# {}\n\n", info.name));
    if let Some(picture) = picture {
        out.push_str(&format!("![]({})\n\n", picture.replace(' ', "%20")));
    }
    let facts = [
        (crate::lang::t("art"), crate::lang::t(&info.found.kind).to_string()),
        (crate::lang::t("herkunft"), info.origin().to_string()),
        (crate::lang::t("aktiv"), info.active().to_string()),
        (crate::lang::t("stil"), style.clone()),
    ];
    let mut any = false;
    for (label, value) in facts {
        if !value.is_empty() {
            out.push_str(&format!("- **{label}:** {value}\n"));
            any = true;
        }
    }
    if any {
        out.push('\n');
    }
    let (about, from, _) = info.found.about_in(crate::lang::current().key());
    if !about.is_empty() {
        out.push_str(&format!("{about}\n\n"));
        if !from.is_empty() {
            out.push_str(&format!("_{}_\n\n", tr!("aus {}", from)));
        }
    }
    if !info.own.note.is_empty() {
        out.push_str(&format!("## {}\n\n{}\n\n", tr!("notiz"), info.own.note));
    }
    let ties = |out: &mut String, heading: &str, ties: &[Tie]| {
        if ties.is_empty() {
            return;
        }
        out.push_str(&format!("## {heading}\n\n"));
        for tie in ties {
            let mut line = format!("- {}", tie.name);
            if !tie.years.is_empty() {
                line.push_str(&format!(" ({})", tie.years));
            }
            if !tie.also.is_empty() {
                line.push_str(&format!(" — {} {}", tr!("auch in"), tie.also.join(", ")));
            }
            out.push_str(&line);
            out.push('\n');
        }
        out.push('\n');
    };
    ties(&mut out, tr!("mitglieder"), &info.found.members);
    ties(&mut out, tr!("in gruppen"), &info.found.groups);
    if !info.found.similar.is_empty() {
        out.push_str(&format!("## {}\n\n", tr!("ähnliche künstler:innen")));
        for name in &info.found.similar {
            out.push_str(&format!("- {name}\n"));
        }
        out.push('\n');
    }
    let mut links = info.found.links.clone();
    if !info.own.link.is_empty() {
        links.insert(0, Link { label: tr!("eigener link").to_string(), url: info.own.link.clone() });
    }
    if !links.is_empty() {
        out.push_str(&format!("## {}\n\n", tr!("mehr lesen")));
        for link in &links {
            out.push_str(&format!("- [{}]({})\n", link.label, link.url));
        }
        out.push('\n');
    }
    out.trim_end().to_string() + "\n"
}

fn file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() { ' ' } else { c })
        .collect();
    let cleaned = cleaned.trim().trim_start_matches('.').trim();
    if cleaned.is_empty() {
        "unbenannt".to_string()
    } else {
        cleaned.chars().take(90).collect()
    }
}

pub fn export_all(dir: &Path, to: &Path) -> Result<usize> {
    std::fs::create_dir_all(to)?;
    let mut written = 0;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(0);
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .collect();
    paths.sort();
    for path in paths {
        let Some(info) = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| toml::from_str::<ArtistInfo>(&t).ok())
        else {
            continue;
        };
        if info.name.is_empty() {
            continue;
        }
        let base = file_name(&info.name);
        let picture = path.with_extension("jpg");
        let shown = if picture.is_file() {
            let name = format!("{base}.jpg");
            std::fs::copy(&picture, to.join(&name))?;
            Some(name)
        } else {
            None
        };
        std::fs::write(to.join(format!("{base}.md")), to_markdown(&info, shown.as_deref()))?;
        written += 1;
    }
    Ok(written)
}

fn musicbrainz_artist_endpoint() -> String {
    std::env::var("KESSLER_MB_ARTIST_ENDPOINT")
        .unwrap_or_else(|_| "https://musicbrainz.org/ws/2/artist".to_string())
}

fn wikidata_endpoint() -> String {
    std::env::var("KESSLER_WIKIDATA_ENDPOINT")
        .unwrap_or_else(|_| "https://www.wikidata.org/w/api.php".to_string())
}

fn sparql_endpoint() -> String {
    std::env::var("KESSLER_WIKIDATA_SPARQL_ENDPOINT")
        .unwrap_or_else(|_| "https://query.wikidata.org/sparql".to_string())
}

fn wikipedia_endpoint() -> String {
    std::env::var("KESSLER_WIKIPEDIA_ENDPOINT")
        .unwrap_or_else(|_| "https://{lang}.wikipedia.org/api/rest_v1/page/summary/".to_string())
}

fn deezer_artist_endpoint() -> String {
    std::env::var("KESSLER_DEEZER_ARTIST_ENDPOINT")
        .unwrap_or_else(|_| "https://api.deezer.com/search/artist".to_string())
}

fn lastfm_endpoint() -> String {
    std::env::var("KESSLER_LASTFM_ENDPOINT")
        .unwrap_or_else(|_| "https://ws.audioscrobbler.com/2.0/".to_string())
}

fn musicbrainz_pause() {
    if std::env::var("KESSLER_MB_ARTIST_ENDPOINT").is_err() {
        std::thread::sleep(std::time::Duration::from_millis(1100));
    }
}

pub fn plain(name: &str) -> String {
    let lower: String = name
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect();
    let lower = lower.trim();
    let lower = lower.strip_prefix("the ").unwrap_or(lower);
    lower.chars().filter(|c| c.is_alphanumeric()).collect()
}

pub fn look_up(name: &str, lang: &str, lastfm_key: Option<&str>) -> Result<(Found, bool, Option<Vec<u8>>)> {
    let mut found = Found { asked: WIKIPEDIA_LANGS.iter().map(|l| l.to_string()).collect(), ..Found::default() };
    let mut recognised = false;
    let mut picture: Option<Vec<u8>> = None;
    let mut trouble: Option<String> = None;
    fn note(e: KesslerError, trouble: &mut Option<String>) {
        if trouble.is_none() {
            *trouble = Some(e.to_string());
        }
    }

    let mut wikidata: Option<String> = None;
    match musicbrainz_search(name) {
        Ok(Some(mbid)) => {
            musicbrainz_pause();
            match fetch_text(&format!(
                "{}/{}?inc=url-rels+genres+tags+artist-rels&fmt=json",
                musicbrainz_artist_endpoint(),
                mbid
            ))
            .and_then(|text| {
                Json::parse(&text)
                    .ok_or_else(|| KesslerError::Metadata("musicbrainz: unreadable".into()))
            }) {
                Ok(doc) => {
                    recognised = true;
                    wikidata = read_musicbrainz(&doc, &mut found);
                    let group = doc.text("type").is_some_and(|t| t != "Person");
                    let ties = read_ties(&doc, group);
                    if group {
                        found.members = ties;
                        for member in found.members.iter_mut().take(MEMBERS_FOLLOWED) {
                            if member.mbid.is_empty() {
                                continue;
                            }
                            musicbrainz_pause();
                            let other = fetch_text(&format!(
                                "{}/{}?inc=artist-rels&fmt=json",
                                musicbrainz_artist_endpoint(),
                                member.mbid
                            ))
                            .ok()
                            .and_then(|text| Json::parse(&text));
                            if let Some(other) = other {
                                member.also = read_ties(&other, false)
                                    .into_iter()
                                    .filter(|g| g.mbid != mbid && plain(&g.name) != plain(name))
                                    .map(|g| g.name)
                                    .collect();
                            }
                        }
                    } else {
                        found.groups = ties;
                    }
                    found.mbid = mbid;
                }
                Err(e) => note(e, &mut trouble),
            }
        }
        Ok(None) => {}
        Err(e) => note(e, &mut trouble),
    }

    if let Some(item) = &wikidata {
        let preferred = match lang {
            "de" => ["de", "en", "ja"],
            "ja" => ["ja", "en", "de"],
            _ => ["en", "de", "ja"],
        };
        match wikipedia_titles(item) {
            Ok(titles) => {
                for lang in preferred {
                    let Some(title) = titles.iter().find(|(l, _)| l == lang).map(|(_, t)| t) else {
                        continue;
                    };
                    match wikipedia_summary(lang, title) {
                        Ok(Some(summary)) => {
                            found.told.push(Told {
                                lang: lang.to_string(),
                                text: summary.extract.clone(),
                                from: "wikipedia".into(),
                            });
                            if !found.about.is_empty() {
                                continue;
                            }
                            found.about = summary.extract;
                            found.about_from = "wikipedia".into();
                            found.links.retain(|l| l.label != "wikipedia");
                            found.links.insert(
                                0,
                                Link { label: "wikipedia".into(), url: summary.page },
                            );
                            if let Some(url) = summary.picture {
                                picture = fetch(&url).ok();
                            }
                        }
                        Ok(None) => {}
                        Err(e) => note(e, &mut trouble),
                    }
                }
            }
            Err(e) => note(e, &mut trouble),
        }
    }

    match deezer_picture(name) {
        Ok(Some(url)) => {
            if let Ok(bytes) = fetch(&url) {
                picture = Some(bytes);
                recognised = true;
            }
        }
        Ok(None) => {}
        Err(e) => note(e, &mut trouble),
    }

    if let Some(key) = lastfm_key.filter(|k| !k.is_empty()) {
        match lastfm_info(name, lang, key) {
            Ok(Some(info)) => {
                recognised = true;
                if !info.bio.is_empty() && !found.told.iter().any(|t| t.lang == lang) {
                    found.told.push(Told {
                        lang: lang.to_string(),
                        text: info.bio.clone(),
                        from: "last.fm".into(),
                    });
                }
                if found.about.is_empty() && !info.bio.is_empty() {
                    found.about = info.bio;
                    found.about_from = "last.fm".into();
                }
                if found.tags.is_empty() {
                    found.tags = info.tags;
                } else {
                    for tag in info.tags {
                        if !found.tags.iter().any(|t| plain(t) == plain(&tag)) {
                            found.tags.push(tag);
                        }
                    }
                }
                found.similar = info.similar;
            }
            Ok(None) => {}
            Err(e) => note(e, &mut trouble),
        }
        if let Ok(kin) = lastfm_similar(name, key) {
            found.kin = kin;
        }
    }
    found.tags.truncate(8);

    let encoded = encode(name).replace("%20", "+");
    found.links.push(Link {
        label: "last.fm".into(),
        url: format!("https://www.last.fm/music/{encoded}"),
    });
    if !found.mbid.is_empty() {
        found.links.push(Link {
            label: "musicbrainz".into(),
            url: format!("https://musicbrainz.org/artist/{}", found.mbid),
        });
    }

    if !recognised {
        if let Some(e) = trouble {
            return Err(KesslerError::Metadata(e));
        }
    }
    Ok((found, recognised, picture))
}

fn musicbrainz_search(name: &str) -> Result<Option<String>> {
    let query = format!("artist:\"{}\"", name.replace('"', ""));
    let url = format!(
        "{}?query={}&fmt=json&limit=5",
        musicbrainz_artist_endpoint(),
        encode(&query)
    );
    let doc = Json::parse(&fetch_text(&url)?)
        .ok_or_else(|| KesslerError::Metadata("musicbrainz: unreadable".into()))?;
    Ok(best_musicbrainz_match(&doc, name))
}

fn best_musicbrainz_match(doc: &Json, name: &str) -> Option<String> {
    let wanted = plain(name);
    doc.list("artists")
        .iter()
        .find(|a| {
            let score = a.get("score").and_then(Json::as_number).unwrap_or(0.0);
            let named = a.text("name").is_some_and(|n| plain(n) == wanted)
                || a.list("aliases")
                    .iter()
                    .any(|alias| alias.text("name").is_some_and(|n| plain(n) == wanted));
            named && score >= 80.0
        })
        .and_then(|a| a.text("id"))
        .map(str::to_string)
}

fn read_musicbrainz(doc: &Json, found: &mut Found) -> Option<String> {
    found.kind = doc.text("type").map(kind_word).unwrap_or_default().to_string();
    found.disambiguation = doc.text("disambiguation").unwrap_or_default().to_string();
    let area = |key: &str| doc.get(key).and_then(|a| a.text("name")).map(str::to_string);
    found.origin = area("begin-area")
        .or_else(|| area("area"))
        .or_else(|| doc.text("country").map(str::to_string))
        .unwrap_or_default();
    if let Some(span) = doc.get("life-span") {
        let year = |key: &str| span.text(key).map(|d| d.chars().take(4).collect::<String>());
        let ended = span.get("ended") == Some(&Json::Bool(true));
        found.active = match (year("begin"), year("end")) {
            (Some(b), Some(e)) => format!("{b}–{e}"),
            (Some(b), None) if ended => format!("{b}–?"),
            (Some(b), None) => format!("{b}–"),
            (None, Some(e)) => format!("–{e}"),
            (None, None) => String::new(),
        };
    }

    let mut named: Vec<(String, f64)> = Vec::new();
    for key in ["genres", "tags"] {
        for tag in doc.list(key) {
            let Some(name) = tag.text("name") else { continue };
            let count = tag.get("count").and_then(Json::as_number).unwrap_or(0.0);
            if count > 0.0 && !named.iter().any(|(n, _)| n == name) {
                named.push((name.to_string(), count + if key == "genres" { 1000.0 } else { 0.0 }));
            }
        }
    }
    named.sort_by(|a, b| b.1.total_cmp(&a.1));
    found.tags = named.into_iter().map(|(n, _)| n).collect();

    let mut wikidata = None;
    for relation in doc.list("relations") {
        let Some(url) = relation.get("url").and_then(|u| u.text("resource")) else {
            continue;
        };
        let label = match relation.text("type").unwrap_or_default() {
            "wikidata" => {
                wikidata = url.rsplit('/').next().map(str::to_string);
                continue;
            }
            "official homepage" => "website",
            "discogs" => "discogs",
            "bandcamp" => "bandcamp",
            "soundcloud" => "soundcloud",
            "allmusic" => "allmusic",
            "youtube" => "youtube",
            "wikipedia" => "wikipedia",
            _ => continue,
        };
        if !found.links.iter().any(|l| l.label == label) {
            found.links.push(Link { label: label.into(), url: url.to_string() });
        }
    }
    wikidata
}

const MEMBERS_FOLLOWED: usize = 6;

fn read_ties(doc: &Json, group: bool) -> Vec<Tie> {
    let wanted = if group { "backward" } else { "forward" };
    let mut ties: Vec<(String, Tie)> = Vec::new();
    for relation in doc.list("relations") {
        let kind = relation.text("type").unwrap_or_default();
        if !matches!(kind, "member of band" | "collaboration") {
            continue;
        }
        if relation.text("direction") != Some(wanted) {
            continue;
        }
        let Some(other) = relation.get("artist") else { continue };
        let Some(name) = other.text("name") else { continue };
        let year = |key: &str| relation.text(key).map(|d| d.chars().take(4).collect::<String>());
        let ended = relation.get("ended") == Some(&Json::Bool(true));
        let years = match (year("begin"), year("end")) {
            (Some(b), Some(e)) if b == e => b,
            (Some(b), Some(e)) => format!("{b}–{e}"),
            (Some(b), None) if ended => format!("{b}–?"),
            (Some(b), None) => format!("{b}–"),
            (None, Some(e)) => format!("–{e}"),
            (None, None) => String::new(),
        };
        let begin = year("begin").unwrap_or_else(|| "9999".into());
        if let Some((_, known)) = ties.iter_mut().find(|(_, t)| plain(&t.name) == plain(name)) {
            known.current |= !ended;
            continue;
        }
        ties.push((
            begin,
            Tie {
                name: name.to_string(),
                mbid: other.text("id").unwrap_or_default().to_string(),
                years,
                current: !ended,
                also: Vec::new(),
            },
        ));
    }
    ties.sort_by(|(a, x), (b, y)| y.current.cmp(&x.current).then(a.cmp(b)));
    ties.into_iter().map(|(_, t)| t).collect()
}

pub fn kind_word(kind: &str) -> &'static str {
    match kind.to_lowercase().as_str() {
        "person" => "person",
        "group" => "gruppe",
        "orchestra" => "orchester",
        "choir" => "chor",
        "character" => "figur",
        _ => "",
    }
}

const WIKIPEDIA_LANGS: [&str; 3] = ["de", "en", "ja"];

fn wikipedia_titles(item: &str) -> Result<Vec<(String, String)>> {
    let url = format!(
        "{}?action=wbgetentities&ids={}&props=sitelinks&sitefilter=dewiki%7Cenwiki%7Cjawiki&format=json",
        wikidata_endpoint(),
        encode(item)
    );
    let doc = Json::parse(&fetch_text(&url)?)
        .ok_or_else(|| KesslerError::Metadata("wikidata: unreadable".into()))?;
    Ok(read_sitelinks(&doc, item))
}

fn read_sitelinks(doc: &Json, item: &str) -> Vec<(String, String)> {
    let Some(links) = doc.get("entities").and_then(|e| e.get(item)).and_then(|i| i.get("sitelinks"))
    else {
        return Vec::new();
    };
    WIKIPEDIA_LANGS
        .into_iter()
        .filter_map(|lang| {
            links
                .get(&format!("{lang}wiki"))
                .and_then(|l| l.text("title"))
                .map(|t| (lang.to_string(), t.to_string()))
        })
        .collect()
}

struct Summary {
    extract: String,
    page: String,
    picture: Option<String>,
}

fn wikipedia_summary(lang: &str, title: &str) -> Result<Option<Summary>> {
    let url = format!(
        "{}{}",
        wikipedia_endpoint().replace("{lang}", lang),
        encode(&title.replace(' ', "_"))
    );
    let doc = Json::parse(&fetch_text(&url)?)
        .ok_or_else(|| KesslerError::Metadata("wikipedia: unreadable".into()))?;
    Ok(read_summary(&doc, lang, title))
}

fn read_summary(doc: &Json, lang: &str, title: &str) -> Option<Summary> {
    if doc.text("type") == Some("disambiguation") {
        return None;
    }
    let extract = doc.text("extract")?.trim().to_string();
    if extract.is_empty() {
        return None;
    }
    let page = doc
        .get("content_urls")
        .and_then(|c| c.get("desktop"))
        .and_then(|d| d.text("page"))
        .map(str::to_string)
        .unwrap_or_else(|| format!("https://{lang}.wikipedia.org/wiki/{}", title.replace(' ', "_")));
    let picture = doc.get("thumbnail").and_then(|t| t.text("source")).map(str::to_string);
    Some(Summary { extract, page, picture })
}

pub type StationArticle = (Vec<(String, String, String)>, Option<Vec<u8>>);

pub fn station_wikipedia(site: &str) -> Result<StationArticle> {
    let Some(item) = station_item(site)? else { return Ok((Vec::new(), None)) };
    let titles = wikipedia_titles(&item)?;
    let mut told = Vec::new();
    let mut picture = None;
    for (lang, title) in titles {
        if let Some(summary) = wikipedia_summary(&lang, &title)? {
            if picture.is_none() {
                picture = summary.picture.as_deref().and_then(|url| fetch(url).ok());
            }
            told.push((lang, summary.extract, summary.page));
        }
    }
    Ok((told, picture))
}

fn station_item(site: &str) -> Result<Option<String>> {
    let ways = site_ways(site);
    if ways.is_empty() {
        return Ok(None);
    }
    let values: String = ways.iter().map(|w| format!("<{w}>")).collect::<Vec<_>>().join(" ");
    let query = format!("SELECT ?item WHERE {{ VALUES ?site {{ {values} }} ?item wdt:P856 ?site . }} LIMIT 5");
    let url = format!("{}?format=json&query={}", sparql_endpoint(), encode(&query));
    let doc = Json::parse(&fetch_text(&url)?)
        .ok_or_else(|| KesslerError::Metadata("wikidata: unreadable".into()))?;
    Ok(read_station_item(&doc))
}

fn read_station_item(doc: &Json) -> Option<String> {
    doc.get("results")?
        .list("bindings")
        .iter()
        .filter_map(|b| b.get("item").and_then(|i| i.text("value")))
        .filter_map(|v| v.rsplit('/').next())
        .find(|q| q.starts_with('Q'))
        .map(str::to_string)
}

fn site_ways(site: &str) -> Vec<String> {
    let site = site.trim();
    let rest = site.strip_prefix("https://").or_else(|| site.strip_prefix("http://")).unwrap_or(site);
    let rest = rest.strip_prefix("www.").unwrap_or(rest);
    let rest = rest.split(['?', '#']).next().unwrap_or("");
    let (host, path) = match rest.find('/') {
        Some(at) => (&rest[..at], rest[at..].trim_end_matches('/')),
        None => (rest, ""),
    };
    let clean = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "-._~/%:".contains(c));
    if !clean(host) || !host.contains('.') || !(path.is_empty() || clean(path)) {
        return Vec::new();
    }
    let host = host.to_ascii_lowercase();
    let mut ways = Vec::new();
    for scheme in ["https", "http"] {
        for www in ["www.", ""] {
            let base = format!("{scheme}://{www}{host}");
            ways.push(format!("{base}/"));
            ways.push(base.clone());
            if !path.is_empty() {
                ways.push(format!("{base}{path}"));
                ways.push(format!("{base}{path}/"));
            }
        }
    }
    ways
}

fn deezer_picture(name: &str) -> Result<Option<String>> {
    let url = format!("{}?q={}&limit=5", deezer_artist_endpoint(), encode(name));
    let doc = Json::parse(&fetch_text(&url)?)
        .ok_or_else(|| KesslerError::Metadata("deezer: unreadable".into()))?;
    Ok(read_deezer(&doc, name))
}

fn read_deezer(doc: &Json, name: &str) -> Option<String> {
    let wanted = plain(name);
    doc.list("data")
        .iter()
        .filter(|a| a.text("name").is_some_and(|n| plain(n) == wanted))
        .filter_map(|a| a.text("picture_xl").or_else(|| a.text("picture_big")))
        .find(|url| !url.contains("/artist//"))
        .map(str::to_string)
}

struct LastfmInfo {
    bio: String,
    tags: Vec<String>,
    similar: Vec<String>,
}

fn lastfm_info(name: &str, lang: &str, key: &str) -> Result<Option<LastfmInfo>> {
    let url = format!(
        "{}?method=artist.getinfo&artist={}&api_key={}&format=json&autocorrect=1&lang={}",
        lastfm_endpoint(),
        encode(name),
        encode(key),
        encode(lang)
    );
    let doc = Json::parse(&fetch_text(&url)?)
        .ok_or_else(|| KesslerError::Metadata("last.fm: unreadable".into()))?;
    Ok(read_lastfm(&doc))
}

fn read_lastfm(doc: &Json) -> Option<LastfmInfo> {
    let artist = doc.get("artist")?;
    let names = |outer: &str, inner: &str| -> Vec<String> {
        let Some(holder) = artist.get(outer) else { return Vec::new() };
        let items: Vec<&Json> = match holder.get(inner) {
            Some(Json::Array(list)) => list.iter().collect(),
            Some(one @ Json::Object(_)) => vec![one],
            _ => Vec::new(),
        };
        items.iter().filter_map(|i| i.text("name")).map(str::to_string).collect()
    };
    let bio = artist
        .get("bio")
        .and_then(|b| b.text("summary"))
        .map(without_markup)
        .unwrap_or_default();
    Some(LastfmInfo {
        bio,
        tags: names("tags", "tag"),
        similar: names("similar", "artist"),
    })
}

fn lastfm_similar(name: &str, key: &str) -> Result<Vec<Kin>> {
    let url = format!(
        "{}?method=artist.getsimilar&artist={}&api_key={}&format=json&autocorrect=1&limit={}",
        lastfm_endpoint(),
        encode(name),
        encode(key),
        KIN_ASKED
    );
    let doc = Json::parse(&fetch_text(&url)?)
        .ok_or_else(|| KesslerError::Metadata("last.fm: unreadable".into()))?;
    Ok(read_kin(&doc))
}

fn read_kin(doc: &Json) -> Vec<Kin> {
    let items: Vec<&Json> = match doc.get("similarartists").and_then(|s| s.get("artist")) {
        Some(Json::Array(list)) => list.iter().collect(),
        Some(one @ Json::Object(_)) => vec![one],
        _ => Vec::new(),
    };
    items
        .iter()
        .filter_map(|item| {
            let name = item.text("name")?.trim();
            let weight = match item.get("match") {
                Some(Json::String(s)) => s.trim().parse::<f32>().ok(),
                Some(Json::Number(n)) => Some(*n as f32),
                _ => None,
            }
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
            (!name.is_empty()).then(|| Kin { name: name.to_string(), weight })
        })
        .collect()
}

fn without_markup(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut inside = false;
    for c in html.chars() {
        match c {
            '<' => inside = true,
            '>' => inside = false,
            _ if !inside => out.push(c),
            _ => {}
        }
    }
    let out = out
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">");
    let out = match out.rfind("Read more on Last.fm") {
        Some(at) => out[..at].to_string(),
        None => out,
    };
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(text: &str) -> Json {
        Json::parse(text).expect("test json")
    }

    #[test]
    fn names_are_compared_plainly() {
        assert_eq!(plain("The Knife"), plain("knife"));
        assert_eq!(plain("Boards of Canada"), plain("boards of canada"));
        assert_eq!(plain("Sigur Rós"), plain("sigur rós"));
        assert_ne!(plain("Coil"), plain("Coil (band)"));
    }

    #[test]
    fn musicbrainz_is_taken_at_its_word_only_when_the_name_is_the_same() {
        let doc = json(
            r#"{"artists":[
                {"id":"a","name":"Coil Spring","score":100},
                {"id":"b","name":"Coil","score":96,"aliases":[]},
                {"id":"c","name":"Coil","score":60}]}"#,
        );
        assert_eq!(best_musicbrainz_match(&doc, "Coil").as_deref(), Some("b"));
        assert_eq!(best_musicbrainz_match(&doc, "Nobody"), None);
    }

    #[test]
    fn a_musicbrainz_artist_gives_its_facts_links_and_wikidata() {
        let doc = json(
            r#"{"id":"69158f97","name":"Boards of Canada","type":"Group",
                "country":"GB","area":{"name":"United Kingdom"},
                "begin-area":{"name":"Edinburgh"},
                "life-span":{"begin":"1986","end":null,"ended":false},
                "disambiguation":"",
                "genres":[{"name":"idm","count":9},{"name":"ambient","count":4}],
                "tags":[{"name":"electronic","count":12},{"name":"idm","count":8},{"name":"zero","count":0}],
                "relations":[
                  {"type":"wikidata","url":{"resource":"https://www.wikidata.org/wiki/Q27902"}},
                  {"type":"discogs","url":{"resource":"https://www.discogs.com/artist/3207"}},
                  {"type":"official homepage","url":{"resource":"https://boardsofcanada.com"}},
                  {"type":"free streaming","url":{"resource":"https://x.test"}}]}"#,
        );
        let mut found = Found::default();
        let item = read_musicbrainz(&doc, &mut found);
        assert_eq!(item.as_deref(), Some("Q27902"));
        assert_eq!(found.kind, "gruppe");
        assert_eq!(found.origin, "Edinburgh");
        assert_eq!(found.active, "1986–");
        assert_eq!(found.tags, vec!["idm", "ambient", "electronic"]);
        let labels: Vec<&str> = found.links.iter().map(|l| l.label.as_str()).collect();
        assert_eq!(labels, vec!["discogs", "website"]);
    }

    #[test]
    fn members_and_groups_are_read_from_either_end() {
        let group = json(
            r#"{"type":"Group","relations":[
                {"type":"member of band","direction":"backward","begin":"1995","end":"2004","ended":true,
                 "artist":{"id":"m1","name":"Left Early"}},
                {"type":"member of band","direction":"backward","begin":"1986","ended":false,
                 "artist":{"id":"m2","name":"Marcus Eoin"}},
                {"type":"member of band","direction":"forward","artist":{"id":"x","name":"Not A Member"}},
                {"type":"producer","direction":"backward","artist":{"id":"p","name":"Producer"}}]}"#,
        );
        let members = read_ties(&group, true);
        let names: Vec<&str> = members.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["Marcus Eoin", "Left Early"]);
        assert_eq!(members[0].years, "1986–");
        assert_eq!(members[1].years, "1995–2004");
        assert!(!members[1].current);

        let person = json(
            r#"{"type":"Person","relations":[
                {"type":"member of band","direction":"forward","begin":"1986","artist":{"id":"g","name":"Boards of Canada"}},
                {"type":"collaboration","direction":"forward","artist":{"id":"h","name":"Hell Interface"}}]}"#,
        );
        let groups = read_ties(&person, false);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].name, "Boards of Canada");
    }

    #[test]
    fn a_person_who_has_died_has_both_years() {
        let doc = json(r#"{"type":"Person","life-span":{"begin":"1946-03-01","end":"1994-11-02","ended":true}}"#);
        let mut found = Found::default();
        read_musicbrainz(&doc, &mut found);
        assert_eq!(found.kind, "person");
        assert_eq!(found.active, "1946–1994");
    }

    #[test]
    fn a_station_is_found_by_its_page_written_any_way() {
        let ways = site_ways("https://www.nts.live/radio/");
        assert!(ways.contains(&"https://www.nts.live/".to_string()));
        assert!(ways.contains(&"http://nts.live".to_string()));
        assert!(ways.contains(&"https://nts.live/radio".to_string()));
        assert_eq!(ways.len(), 16);
        assert_eq!(site_ways("kzsu.stanford.edu").len(), 8);
        assert!(site_ways("not a page").is_empty());
        assert!(site_ways("https://a.example/x>y").is_empty());
        let doc = Json::parse(
            r#"{"head":{"vars":["item"]},"results":{"bindings":[
                {"item":{"type":"uri","value":"http://www.wikidata.org/entity/Q6955034"}}]}}"#,
        )
        .unwrap();
        assert_eq!(read_station_item(&doc).as_deref(), Some("Q6955034"));
        let none = Json::parse(r#"{"results":{"bindings":[]}}"#).unwrap();
        assert_eq!(read_station_item(&none), None);
    }

    #[test]
    fn wikidata_names_the_articles_in_every_language() {
        let doc = json(
            r#"{"entities":{"Q27902":{"sitelinks":{
                "enwiki":{"site":"enwiki","title":"Boards of Canada"},
                "jawiki":{"site":"jawiki","title":"ボーズ・オブ・カナダ"},
                "dewiki":{"site":"dewiki","title":"Boards of Canada"}}}}}"#,
        );
        let titles = read_sitelinks(&doc, "Q27902");
        assert_eq!(titles.len(), 3);
        assert_eq!(titles[2], ("ja".to_string(), "ボーズ・オブ・カナダ".to_string()));
        assert_eq!(titles[0].0, "de");
    }

    #[test]
    fn a_disambiguation_page_is_no_summary() {
        let page = json(r#"{"type":"disambiguation","extract":"Coil may refer to"}"#);
        assert!(read_summary(&page, "en", "Coil").is_none());
        let page = json(
            r#"{"type":"standard","extract":"Boards of Canada ist ein Duo.",
                "thumbnail":{"source":"https://upload.test/boc.jpg"},
                "content_urls":{"desktop":{"page":"https://de.wikipedia.org/wiki/Boards_of_Canada"}}}"#,
        );
        let summary = read_summary(&page, "de", "Boards of Canada").unwrap();
        assert_eq!(summary.extract, "Boards of Canada ist ein Duo.");
        assert_eq!(summary.picture.as_deref(), Some("https://upload.test/boc.jpg"));
    }

    #[test]
    fn deezer_gives_a_picture_only_of_the_same_name() {
        let doc = json(
            r#"{"data":[
                {"name":"Coil Spring","picture_xl":"https://e.test/artist/aa/1000.jpg"},
                {"name":"Coil","picture_xl":"https://e.test/images/artist//1000x1000.jpg"},
                {"name":"COIL","picture_xl":"https://e.test/images/artist/bb/1000x1000.jpg"}]}"#,
        );
        assert_eq!(
            read_deezer(&doc, "Coil").as_deref(),
            Some("https://e.test/images/artist/bb/1000x1000.jpg")
        );
    }

    #[test]
    fn lastfm_gives_tags_similar_and_a_plain_biography() {
        let doc = json(
            r#"{"artist":{"name":"Aphex Twin",
                "similar":{"artist":[{"name":"Autechre"},{"name":"Squarepusher"}]},
                "tags":{"tag":{"name":"idm","url":"x"}},
                "bio":{"summary":"Richard D. James &amp; friends. <a href=\"https://www.last.fm/music/Aphex+Twin\">Read more on Last.fm</a>"}}}"#,
        );
        let info = read_lastfm(&doc).unwrap();
        assert_eq!(info.similar, vec!["Autechre", "Squarepusher"]);
        assert_eq!(info.tags, vec!["idm"]);
        assert_eq!(info.bio, "Richard D. James & friends.");
    }

    #[test]
    fn lastfm_gives_the_kin_with_their_measure() {
        let doc = json(
            r#"{"similarartists":{"artist":[
                {"name":"Slowdive","match":"0.92"},
                {"name":"Ride","match":1},
                {"name":"  ","match":"0.5"},
                {"name":"Lush","match":"viel"}],"@attr":{"artist":"My Bloody Valentine"}}}"#,
        );
        let kin = read_kin(&doc);
        assert_eq!(
            kin,
            vec![
                Kin { name: "Slowdive".into(), weight: 0.92 },
                Kin { name: "Ride".into(), weight: 1.0 },
                Kin { name: "Lush".into(), weight: 0.0 },
            ]
        );
        assert!(read_kin(&json(r#"{"error":6,"message":"not found"}"#)).is_empty());
    }

    #[test]
    fn the_paragraph_is_in_the_window_s_language_when_there_is_one() {
        let told = |lang: &str, text: &str| Told { lang: lang.into(), text: text.into(), from: "wikipedia".into() };
        let found = Found {
            about: "Ein Duo.".into(),
            about_from: "wikipedia".into(),
            told: vec![told("de", "Ein Duo."), told("en", "A duo.")],
            ..Found::default()
        };
        assert_eq!(found.about_in("en"), ("A duo.", "wikipedia", None));
        assert_eq!(found.about_in("de"), ("Ein Duo.", "wikipedia", None));
        let only = Found { told: vec![told("de", "Ein Duo.")], ..found.clone() };
        assert_eq!(only.about_in("en"), ("Ein Duo.", "wikipedia", Some("de")));
        let asked = |langs: &[&str]| Found { asked: langs.iter().map(|l| l.to_string()).collect(), ..found.clone() };
        assert!(asked(&["de", "en"]).lacks("ja"));
        assert!(!asked(&["de", "en", "ja"]).lacks("ja"));
        assert!(!asked(&[]).lacks("de"), "it has the German");
        let old = Found { told: Vec::new(), ..found };
        assert_eq!(old.about_in("en"), ("Ein Duo.", "wikipedia", None));
    }

    #[test]
    fn an_artist_comes_out_as_a_page() {
        let mut info = ArtistInfo {
            name: "Boards of Canada".into(),
            looked_up: "2026-09-26".into(),
            found: Found {
                kind: "gruppe".into(),
                origin: "Edinburgh".into(),
                about: "Ein Duo.".into(),
                about_from: "wikipedia".into(),
                tags: vec!["idm".into()],
                similar: vec!["Aphex Twin".into()],
                links: vec![Link { label: "bandcamp".into(), url: "https://boc.bandcamp.com".into() }],
                ..Found::default()
            },
            ..ArtistInfo::default()
        };
        info.own.note = "Geogaddi zuerst.".into();
        let page = to_markdown(&info, Some("Boards of Canada.jpg"));
        assert!(page.starts_with("---\nname: \"Boards of Canada\"\n"), "{page}");
        assert!(page.contains("tags: [\"idm\"]"));
        assert!(page.contains("# Boards of Canada"));
        assert!(page.contains("![](Boards%20of%20Canada.jpg)"));
        assert!(page.contains("Ein Duo."));
        assert!(page.contains("Geogaddi zuerst."));
        assert!(page.contains("- Aphex Twin"));
        assert!(page.contains("- [bandcamp](https://boc.bandcamp.com)"));
        assert_eq!(file_name("AC/DC: live?"), "AC DC  live");
        assert_eq!(file_name(".hidden"), "hidden");
    }

    #[test]
    fn everything_gathered_is_exported() {
        let base = std::env::temp_dir().join(format!("kessler-export-{}", std::process::id()));
        let dir = base.join("artists");
        let to = base.join("out");
        let info = ArtistInfo { name: "Coil".into(), ..ArtistInfo::default() };
        save(&dir, 1, &info).unwrap();
        std::fs::write(picture_path(&dir, 1), b"jpeg").unwrap();
        save(&dir, 2, &ArtistInfo { name: "AC/DC".into(), ..ArtistInfo::default() }).unwrap();
        assert_eq!(export_all(&dir, &to).unwrap(), 2);
        assert!(to.join("Coil.md").is_file());
        assert!(to.join("Coil.jpg").is_file());
        assert!(to.join("AC DC.md").is_file());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn their_own_words_are_kept_and_come_first() {
        let dir = std::env::temp_dir().join(format!("kessler-artist-{}", std::process::id()));
        let mut info = ArtistInfo {
            name: "Coil".into(),
            found: Found { origin: "London".into(), tags: vec!["industrial".into()], ..Found::default() },
            ..ArtistInfo::default()
        };
        info.own.origin = "Weston-super-Mare".into();
        info.own.note = "zwei zeilen\nnotiz".into();
        save(&dir, 7, &info).unwrap();
        let back = load(&dir, 7).unwrap();
        assert_eq!(back, info);
        assert_eq!(back.origin(), "Weston-super-Mare");
        assert_eq!(back.style(), "industrial");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
