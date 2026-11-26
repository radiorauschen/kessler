//! Looking a cover up on the open databases.
//!
//! Three sources, none of which wants a key:
//!
//! * the **Cover Art Archive**, reached through MusicBrainz. Community scans,
//!   often large, occasionally the wrong pressing.
//! * **Deezer**, which hands back a thousand pixels square as a matter of
//!   course and knows most of what was ever released.
//! * the **iTunes search**, as a fallback. Always there, always square, never
//!   bigger than what the store shows.
//!
//! * **Last.fm**, when the program has a key for it (it does, once it
//!   scrobbles): its album pictures are the ones its listeners settled on,
//!   and it knows the small releases the shops never carried. Only its
//!   *artist* pictures are the grey placeholder star; album covers are real.
//!
//! All three are asked politely: one request each, a real user agent, a short
//! timeout, and nothing cached beyond what the caller does with the answer.
//! None is asked at all unless somebody clicks.
//!
//! The network goes through `curl`, as it does for Last.fm and for the same
//! reason: this build cannot take on an HTTP crate, and curl is on every
//! machine this program runs on.

use std::process::Stdio;

use crate::error::{KesslerError, Result};
use crate::json::Json;

/// What every request says it is.
///
/// MusicBrainz asks clients to name themselves, and a generic agent is what
/// gets one blocked. It asks for a contact address too; this program has none
/// to give that would not be somebody's private one, so it gives its name and
/// its version and nothing else. `KESSLER_COVERART_AGENT` replaces the lot for
/// anyone who wants to identify themselves properly.
pub(crate) fn agent() -> String {
    std::env::var("KESSLER_COVERART_AGENT").unwrap_or_else(|_| {
        concat!("kessler/", env!("CARGO_PKG_VERSION"), " ( music library )").to_string()
    })
}

/// Long enough for a slow archive, short enough that a hung request does not
/// sit in front of the person for a minute.
pub(crate) const TIMEOUT_SECS: u32 = 20;

/// How many suggestions are worth showing. More than this and the choice stops
/// being a choice.
pub const MOST_OFFERED: usize = 8;

/// One suggestion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// Where it came from, as the interface says it: "archiv", "deezer" or
    /// "itunes".
    pub source: &'static str,
    pub artist: String,
    pub album: String,
    pub year: Option<i32>,
    /// A small version, for showing.
    pub thumb: String,
    /// The one actually written into the files.
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

/// Percent-encode one value for a query string.
///
/// Same rule as the Last.fm form encoder, and written out here rather than
/// borrowed so that the two are free to differ if a service ever demands it.
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

/// Fetch a URL and hand back whatever came down.
///
/// `--fail-with-body` so that a 404 — which is what the archive says when a
/// release has no picture — arrives as an error rather than as an HTML page
/// that would go into a file as if it were a JPEG.
pub fn fetch(url: &str) -> Result<Vec<u8>> {
    for wait in BUSY_WAITS {
        match fetch_once(url) {
            Err(Fetched::Busy) => std::thread::sleep(std::time::Duration::from_secs(wait)),
            done => return done.map_err(|e| e.into_error(url)),
        }
    }
    fetch_once(url).map_err(|e| e.into_error(url))
}

/// The most a search's answer or a picture may weigh: a large cover or a
/// podcast's picture is a few megabytes, so this leaves room enough, while a
/// server sending without end cannot fill the memory.
const MOST_BYTES: u64 = 40_000_000;

/// How long to wait before asking a busy service again, in seconds.
const BUSY_WAITS: [u64; 2] = [2, 5];

/// Why a fetch came back with nothing.
enum Fetched {
    /// The service said it is busy: 503 or 429.
    Busy,
    /// It did not answer in time.
    Slow,
    /// The name of the host could not be found: no network.
    Offline,
    /// Anything else, as curl said it.
    Said(String),
}

impl Fetched {
    /// As the window says it: which service, and what, in words.
    fn into_error(self, url: &str) -> KesslerError {
        let host = url.split("://").nth(1).unwrap_or(url).split('/').next().unwrap_or(url).trim_start_matches("www.");
        KesslerError::Metadata(match self {
            Self::Busy => tr!("{} ist gerade überlastet — später noch einmal", host),
            Self::Slow => tr!("{} antwortet nicht", host),
            Self::Offline => tr!("kein netz — {} ist nicht zu erreichen", host),
            Self::Said(said) => said,
        })
    }
}

fn fetch_once(url: &str) -> std::result::Result<Vec<u8>, Fetched> {
    let mut command = crate::net::curl();
    command.args([
        "--silent",
        "--show-error",
        "--fail-with-body",
        "--location",
        "--max-time",
        &TIMEOUT_SECS.to_string(),
        "--max-filesize",
        &MOST_BYTES.to_string(),
        "--user-agent",
        &agent(),
    ]);
    if url.starts_with("https://") {
        command.args(["--proto-redir", "=https"]);
    }
    let output = crate::net::address(&mut command, url)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| Fetched::Said(tr!("curl lässt sich nicht starten: {}", e)))?;

    if !output.status.success() {
        let said = String::from_utf8_lossy(&output.stderr);
        let said = said.trim();
        return Err(match output.status.code() {
            Some(6) => Fetched::Offline,
            Some(7 | 28) => Fetched::Slow,
            Some(22) if said.contains(" 503") || said.contains(" 429") => Fetched::Busy,
            _ if said.is_empty() => Fetched::Slow,
            _ => Fetched::Said(said.to_string()),
        });
    }
    Ok(output.stdout)
}

pub(crate) fn fetch_text(url: &str) -> Result<String> {
    Ok(String::from_utf8_lossy(&fetch(url)?).into_owned())
}

/// Ask every source and put what they say in one list.
///
/// A source that is down, blocked or simply has nothing contributes nothing;
/// it does not take the others with it. Only if all of them fail does this
/// fail, because then the person is owed a reason.
///
/// Deliberately *not* Last.fm: its album images have been the same grey star
/// placeholder for years, so a list of them would be a list of the same blank
/// square three times over.
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

/// MusicBrainz names the record; the Cover Art Archive has the picture.
///
/// The search is on the release *group* rather than a release: a person
/// looking for the cover of Geogaddi means the record, not the Japanese CD
/// pressing of it, and the archive keeps a group-level picture for exactly
/// that reason.
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

/// The characters Lucene reads as syntax, taken out.
///
/// An album called `[ ]` or `+ +` is a real thing, and a query that breaks on
/// it comes back as an error rather than as no results — which looks to the
/// person like the network is down.
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

/// Deezer's search. No key either, and the one source of the three that hands
/// back a thousand pixels square as a matter of course.
///
/// Its field-qualified query (`artist:"…" album:"…"`) is stricter than a bare
/// search and comes back with the record rather than with everything that
/// mentions the band.
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

/// Last.fm's picture of the record, if it has a real one.
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

/// The picture out of an `album.getinfo` reply: the largest one listed, and
/// as the full-size original — the address without its size step — for
/// writing into files. The grey star Last.fm puts where it has nothing is
/// no picture.
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

/// A Last.fm image address without its size step —
/// `…/i/u/300x300/abc.jpg` → `…/i/u/abc.jpg` — which serves the picture as
/// it was uploaded.
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

/// A value fit to go inside a quoted query term.
fn quoteless(text: &str) -> String {
    text.replace(['"', '\\'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The iTunes search. No key, no sign-up, and it knows nearly everything that
/// was ever sold.
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

/// The store's own trick: the size is in the file name, and asking for a
/// bigger one works.
///
/// 600 rather than 1200 or 100000 — the larger sizes exist for some records
/// and not for others, and a link that four-oh-fours on half the library is
/// worse than a picture that is merely big enough.
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

    /// A stand-in for the service: answers one request with this body, then
    /// stops. Returns the address to point the code at.
    ///
    /// The real services cannot be reached from the build environment, so the
    /// shape of the reply is what gets tested — which is the part that can be
    /// got wrong anyway.
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

    /// `[ ]` by Sigur Rós, `+/-` by anyone: album titles really do consist of
    /// Lucene syntax, and a query built straight out of one comes back as an
    /// error rather than as nothing found.
    #[test]
    fn a_title_made_of_syntax_is_still_a_query() {
        assert_eq!(lucene_safe("[ ]"), "");
        assert_eq!(lucene_safe("+/-"), "");
        assert_eq!(lucene_safe("mezzanine"), "mezzanine");
        assert_eq!(lucene_safe("AC/DC"), "AC DC");
        assert_eq!(lucene_safe("g??z"), "g z");
        assert_eq!(lucene_safe("kid a mnesia"), "kid a mnesia");
    }

    /// The one piece of knowledge about the store's URLs, kept in one place.
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

    /// The second entry has no picture at all, and an entry without one is not
    /// a suggestion.
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

    /// A quotation mark in an album title would end the quoted term early and
    /// turn the rest of it into syntax.
    #[test]
    fn a_quoted_term_stays_one_term() {
        assert_eq!(quoteless("say \"hello\""), "say hello");
        assert_eq!(quoteless("AC\\DC"), "AC DC");
        assert_eq!(quoteless("  mezzanine  "), "mezzanine");
    }

    /// A value with a space or a quotation mark in it has to survive the trip
    /// into a query string.
    #[test]
    fn the_query_string_is_encoded() {
        assert_eq!(encode("boards of canada"), "boards%20of%20canada");
        assert_eq!(encode("a\"b"), "a%22b");
        assert_eq!(encode("ä"), "%C3%A4");
        assert_eq!(encode("ok-_.~"), "ok-_.~");
    }
}
