use std::process::{Command, Stdio};

use crate::error::{KesslerError, Result};
use crate::json::Json;

pub(crate) fn agent() -> String {
    std::env::var("KESSLER_COVERART_AGENT").unwrap_or_else(|_| {
        concat!("kessler/", env!("CARGO_PKG_VERSION"), " ( music library )").to_string()
    })
}

pub(crate) const TIMEOUT_SECS: u32 = 20;

pub const MOST_OFFERED: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub source: &'static str,
    pub artist: String,
    pub album: String,
    pub year: Option<i32>,
    pub thumb: String,
    pub full: String,
}

fn itunes_endpoint() -> String {
    std::env::var("KESSLER_ITUNES_ENDPOINT")
        .unwrap_or_else(|_| "https://itunes.apple.com/search".to_string())
}

pub(crate) fn musicbrainz_endpoint() -> String {
    std::env::var("KESSLER_MUSICBRAINZ_ENDPOINT")
        .unwrap_or_else(|_| "https://musicbrainz.org/ws/2/release-group".to_string())
}

fn deezer_endpoint() -> String {
    std::env::var("KESSLER_DEEZER_ENDPOINT")
        .unwrap_or_else(|_| "https://api.deezer.com/search/album".to_string())
}

fn coverart_endpoint() -> String {
    std::env::var("KESSLER_COVERART_ENDPOINT")
        .unwrap_or_else(|_| "https://coverartarchive.org/release-group".to_string())
}

pub(crate) fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

pub fn fetch(url: &str) -> Result<Vec<u8>> {
    let output = Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--fail-with-body",
            "--location",
            "--max-time",
            &TIMEOUT_SECS.to_string(),
            "--user-agent",
            &agent(),
            url,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| KesslerError::Metadata(tr!("curl lässt sich nicht starten: {}", e)))?;

    if !output.status.success() {
        let said = String::from_utf8_lossy(&output.stderr);
        let said = said.trim();
        return Err(KesslerError::Metadata(if said.is_empty() {
            tr!("{} antwortet nicht", url)
        } else {
            said.to_string()
        }));
    }
    Ok(output.stdout)
}

pub(crate) fn fetch_text(url: &str) -> Result<String> {
    Ok(String::from_utf8_lossy(&fetch(url)?).into_owned())
}

pub fn search(artist: &str, album: &str, lastfm_key: Option<&str>) -> Result<Vec<Candidate>> {
    let mut found = Vec::new();
    let mut trouble: Option<String> = None;

    if let Some(key) = lastfm_key.filter(|k| !k.is_empty()) {
        match from_lastfm(artist, album, key) {
            Ok(mut list) => found.append(&mut list),
            Err(e) => trouble = Some(e.to_string()),
        }
    }
    for asked in [from_archive, from_deezer, from_itunes] {
        match asked(artist, album) {
            Ok(mut list) => found.append(&mut list),
            Err(e) => {
                if trouble.is_none() {
                    trouble = Some(e.to_string());
                }
            }
        }
    }

    if found.is_empty() {
        if let Some(e) = trouble {
            return Err(KesslerError::Metadata(e));
        }
    }
    found.truncate(MOST_OFFERED);
    Ok(found)
}

fn from_archive(artist: &str, album: &str) -> Result<Vec<Candidate>> {
    let query = format!(
        "artist:\"{}\" AND releasegroup:\"{}\"",
        lucene_safe(artist),
        lucene_safe(album)
    );
    let url = format!(
        "{}?query={}&fmt=json&limit=5",
        musicbrainz_endpoint(),
        encode(&query)
    );
    let doc = Json::parse(&fetch_text(&url)?)
        .ok_or_else(|| KesslerError::Metadata(tr!("musicbrainz antwortet unverständlich").into()))?;

    let mut out = Vec::new();
    for group in doc.list("release-groups") {
        let Some(id) = group.text("id") else { continue };
        let name = group.text("title").unwrap_or(album).to_string();
        let by = group
            .list("artist-credit")
            .first()
            .and_then(|c| c.get("artist"))
            .and_then(|a| a.text("name"))
            .unwrap_or(artist)
            .to_string();
        let year = group
            .text("first-release-date")
            .and_then(|d| d.get(..4))
            .and_then(|y| y.parse().ok());
        out.push(Candidate {
            source: "archiv",
            artist: by,
            album: name,
            year,
            thumb: format!("{}/{id}/front-250", coverart_endpoint()),
            full: format!("{}/{id}/front-500", coverart_endpoint()),
        });
    }
    Ok(out)
}

pub(crate) fn lucene_safe(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '+' | '-' | '&' | '|' | '!' | '(' | ')' | '{' | '}' | '[' | ']' | '^' | '"' | '~'
            | '*' | '?' | ':' | '\\' | '/' => ' ',
            other => other,
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn from_deezer(artist: &str, album: &str) -> Result<Vec<Candidate>> {
    let query = format!("artist:\"{}\" album:\"{}\"", quoteless(artist), quoteless(album));
    let url = format!("{}?q={}&limit=5", deezer_endpoint(), encode(&query));
    let doc = Json::parse(&fetch_text(&url)?)
        .ok_or_else(|| KesslerError::Metadata(tr!("deezer antwortet unverständlich").into()))?;

    let mut out = Vec::new();
    for entry in doc.list("data") {
        let Some(full) = entry.text("cover_xl").or_else(|| entry.text("cover_big")) else {
            continue;
        };
        let thumb = entry.text("cover_medium").unwrap_or(full);
        out.push(Candidate {
            source: "deezer",
            artist: entry
                .get("artist")
                .and_then(|a| a.text("name"))
                .unwrap_or(artist)
                .to_string(),
            album: entry.text("title").unwrap_or(album).to_string(),
            year: None,
            thumb: thumb.to_string(),
            full: full.to_string(),
        });
    }
    Ok(out)
}

fn lastfm_endpoint() -> String {
    std::env::var("KESSLER_LASTFM_ENDPOINT")
        .unwrap_or_else(|_| "https://ws.audioscrobbler.com/2.0/".to_string())
}

fn from_lastfm(artist: &str, album: &str, key: &str) -> Result<Vec<Candidate>> {
    let url = format!(
        "{}?method=album.getinfo&artist={}&album={}&api_key={}&format=json&autocorrect=1",
        lastfm_endpoint(),
        encode(artist),
        encode(album),
        encode(key)
    );
    let doc = Json::parse(&fetch_text(&url)?)
        .ok_or_else(|| KesslerError::Metadata(tr!("last.fm antwortet unverständlich").into()))?;
    Ok(read_lastfm_album(&doc, artist, album).into_iter().collect())
}

fn read_lastfm_album(doc: &Json, artist: &str, album: &str) -> Option<Candidate> {
    const PLACEHOLDER: &str = "2a96cbd8b46e442fc41c2b86b821562f";
    let found = doc.get("album")?;
    let rank = |size: &str| ["small", "medium", "large", "extralarge", "mega"].iter().position(|s| *s == size);
    let best = found
        .list("image")
        .iter()
        .filter_map(|i| {
            let url = i.text("#text")?;
            (!url.is_empty() && !url.contains(PLACEHOLDER)).then(|| (rank(i.text("size").unwrap_or("")), url))
        })
        .max_by_key(|(rank, _)| *rank)?
        .1
        .to_string();
    Some(Candidate {
        source: "last.fm",
        artist: found.text("artist").unwrap_or(artist).to_string(),
        album: found.text("name").unwrap_or(album).to_string(),
        year: None,
        full: original_size(&best),
        thumb: best,
    })
}

fn original_size(url: &str) -> String {
    let Some(at) = url.find("/i/u/") else {
        return url.to_string();
    };
    let (head, tail) = url.split_at(at + "/i/u/".len());
    match tail.split_once('/') {
        Some((size, rest)) if size.chars().next().is_some_and(|c| c.is_ascii_digit()) => {
            format!("{head}{rest}")
        }
        _ => url.to_string(),
    }
}

fn quoteless(text: &str) -> String {
    text.replace(['"', '\\'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn from_itunes(artist: &str, album: &str) -> Result<Vec<Candidate>> {
    let url = format!(
        "{}?term={}&entity=album&limit=6",
        itunes_endpoint(),
        encode(&format!("{artist} {album}"))
    );
    let doc = Json::parse(&fetch_text(&url)?)
        .ok_or_else(|| KesslerError::Metadata(tr!("itunes antwortet unverständlich").into()))?;

    let mut out = Vec::new();
    for entry in doc.list("results") {
        let Some(art) = entry.text("artworkUrl100") else {
            continue;
        };
        out.push(Candidate {
            source: "itunes",
            artist: entry.text("artistName").unwrap_or(artist).to_string(),
            album: entry.text("collectionName").unwrap_or(album).to_string(),
            year: entry
                .text("releaseDate")
                .and_then(|d| d.get(..4))
                .and_then(|y| y.parse().ok()),
            thumb: art.to_string(),
            full: enlarged(art),
        });
    }
    Ok(out)
}

fn enlarged(url: &str) -> String {
    url.replace("100x100bb", "600x600bb")
}

#[cfg(test)]
mod lastfm_tests {
    use super::*;

    #[test]
    fn lastfm_gives_its_largest_real_picture_at_full_size() {
        let doc = Json::parse(
            r##"{"album":{"name":"Geogaddi","artist":"Boards of Canada","image":[
                {"#text":"https://lastfm.freetls.fastly.net/i/u/34s/abc.png","size":"small"},
                {"#text":"https://lastfm.freetls.fastly.net/i/u/300x300/abc.png","size":"extralarge"},
                {"#text":"","size":"mega"}]}}"##,
        )
        .unwrap();
        let c = read_lastfm_album(&doc, "x", "y").unwrap();
        assert_eq!(c.source, "last.fm");
        assert_eq!(c.album, "Geogaddi");
        assert_eq!(c.thumb, "https://lastfm.freetls.fastly.net/i/u/300x300/abc.png");
        assert_eq!(c.full, "https://lastfm.freetls.fastly.net/i/u/abc.png");
    }

    #[test]
    fn the_grey_star_is_no_picture() {
        let doc = Json::parse(
            r##"{"album":{"image":[{"#text":"https://lastfm.freetls.fastly.net/i/u/300x300/2a96cbd8b46e442fc41c2b86b821562f.png","size":"extralarge"}]}}"##,
        )
        .unwrap();
        assert!(read_lastfm_album(&doc, "x", "y").is_none());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    fn server(body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buffer = [0u8; 2048];
                let _ = stream.read(&mut buffer);
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
            }
        });
        address
    }

    const ITUNES_REPLY: &str = r#"{"resultCount": 2, "results": [
        {"artistName": "Boards of Canada", "collectionName": "Geogaddi",
         "artworkUrl100": "https://is1.test/a/source/100x100bb.jpg",
         "releaseDate": "2002-02-18T08:00:00Z"},
        {"artistName": "Boards of Canada", "collectionName": "Trans Canada Highway",
         "artworkUrl100": "https://is1.test/b/source/100x100bb.jpg",
         "releaseDate": "2006-05-30T07:00:00Z"}
    ]}"#;

    const MUSICBRAINZ_REPLY: &str = r#"{"count": 1, "release-groups": [
        {"id": "aa1b2c3d", "title": "Geogaddi", "first-release-date": "2002-02-18",
         "artist-credit": [{"artist": {"name": "Boards of Canada"}}]}
    ]}"#;

    #[test]
    fn the_store_reply_becomes_two_suggestions() {
        std::env::set_var("KESSLER_ITUNES_ENDPOINT", server(ITUNES_REPLY));
        let found = from_itunes("boards of canada", "geogaddi").unwrap();

        assert_eq!(found.len(), 2);
        assert_eq!(found[0].album, "Geogaddi");
        assert_eq!(found[0].year, Some(2002));
        assert_eq!(found[0].source, "itunes");
        assert_eq!(found[0].thumb, "https://is1.test/a/source/100x100bb.jpg");
        assert_eq!(found[0].full, "https://is1.test/a/source/600x600bb.jpg");
    }

    #[test]
    fn the_archive_reply_points_at_a_picture() {
        std::env::set_var("KESSLER_MUSICBRAINZ_ENDPOINT", server(MUSICBRAINZ_REPLY));
        std::env::set_var("KESSLER_COVERART_ENDPOINT", "https://art.test/release-group");
        let found = from_archive("boards of canada", "geogaddi").unwrap();

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].album, "Geogaddi");
        assert_eq!(found[0].artist, "Boards of Canada");
        assert_eq!(found[0].year, Some(2002));
        assert_eq!(found[0].thumb, "https://art.test/release-group/aa1b2c3d/front-250");
        assert_eq!(found[0].full, "https://art.test/release-group/aa1b2c3d/front-500");
    }

    #[test]
    fn a_title_made_of_syntax_is_still_a_query() {
        assert_eq!(lucene_safe("[ ]"), "");
        assert_eq!(lucene_safe("+/-"), "");
        assert_eq!(lucene_safe("mezzanine"), "mezzanine");
        assert_eq!(lucene_safe("AC/DC"), "AC DC");
        assert_eq!(lucene_safe("g??z"), "g z");
        assert_eq!(lucene_safe("kid a mnesia"), "kid a mnesia");
    }

    #[test]
    fn a_bigger_picture_is_asked_for_by_name() {
        assert_eq!(
            enlarged("https://is1.test/x/100x100bb.jpg"),
            "https://is1.test/x/600x600bb.jpg"
        );
        let odd = "https://is1.test/x/cover.jpg";
        assert_eq!(enlarged(odd), odd);
    }

    const DEEZER_REPLY: &str = r#"{"data": [
        {"id": 302127, "title": "Geogaddi",
         "cover_medium": "https://cdn.test/250.jpg",
         "cover_big": "https://cdn.test/500.jpg",
         "cover_xl": "https://cdn.test/1000.jpg",
         "artist": {"id": 1, "name": "Boards of Canada"}},
        {"id": 302128, "title": "Twoism", "artist": {"name": "Boards of Canada"}}
    ]}"#;

    #[test]
    fn deezer_gives_the_biggest_it_has() {
        std::env::set_var("KESSLER_DEEZER_ENDPOINT", server(DEEZER_REPLY));
        let found = from_deezer("boards of canada", "geogaddi").unwrap();

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].source, "deezer");
        assert_eq!(found[0].album, "Geogaddi");
        assert_eq!(found[0].artist, "Boards of Canada");
        assert_eq!(found[0].thumb, "https://cdn.test/250.jpg");
        assert_eq!(found[0].full, "https://cdn.test/1000.jpg");
    }

    #[test]
    fn a_quoted_term_stays_one_term() {
        assert_eq!(quoteless("say \"hello\""), "say hello");
        assert_eq!(quoteless("AC\\DC"), "AC DC");
        assert_eq!(quoteless("  mezzanine  "), "mezzanine");
    }

    #[test]
    fn the_query_string_is_encoded() {
        assert_eq!(encode("boards of canada"), "boards%20of%20canada");
        assert_eq!(encode("a\"b"), "a%22b");
        assert_eq!(encode("ä"), "%C3%A4");
        assert_eq!(encode("ok-_.~"), "ok-_.~");
    }
}
