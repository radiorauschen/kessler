//! The cover of the song a station plays, where the station says only
//! „artist - title".
//!
//! A stream names the song and nothing more; the record it is from, and its
//! sleeve, are what the radio sites beside the stream look up themselves, in
//! the iTunes store's open search. kessler asks the same: the song, by artist
//! and title, and takes the picture of the first answer whose artist is the
//! one named — no picture rather than another artist's. Asked once a song,
//! the answer kept on disk: a picture, or an empty file for none. Shown
//! under „informationen", where the artist has no picture of their own; the
//! station's column keeps the station's.

use std::path::PathBuf;

use crate::json::Json;

/// The search, overridable for the tests.
fn endpoint() -> String {
    std::env::var("KESSLER_ITUNES_SEARCH_ENDPOINT").unwrap_or_else(|_| "https://itunes.apple.com/search".to_string())
}

/// Where the cover of `artist`'s `title` is kept: a picture, an empty file
/// when the search had none, nothing while it was not asked yet.
pub fn path(artist: &str, title: &str) -> PathBuf {
    let key = format!("{}\n{}", crate::db::sort_key(artist), crate::db::sort_key(title));
    let name = crate::lastfm::sign::md5_hex(key.as_bytes());
    crate::config::Config::config_dir().join("radio-covers").join(name)
}

/// The cover kept for the song, if one was found.
pub fn found(artist: &str, title: &str) -> Option<PathBuf> {
    let path = path(artist, title);
    std::fs::metadata(&path).ok().filter(|m| m.len() > 0).map(|_| path)
}

/// Look the song up on a thread of its own, unless it was asked before.
/// First in the store of the United States, then in Japan's, whose
/// catalogue holds much a station of Japanese music plays that the other
/// does not.
pub fn fetch(artist: &str, title: &str) {
    let path = path(artist, title);
    if path.exists() || artist.trim().is_empty() || title.trim().is_empty() {
        return;
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if std::fs::write(&path, []).is_err() {
        return;
    }
    let (artist, title) = (artist.to_string(), title.to_string());
    std::thread::Builder::new()
        .name("kessler-radio-cover".into())
        .spawn(move || {
            for country in ["us", "jp"] {
                let url = format!(
                    "{}?term={}&entity=song&limit=10&country={country}",
                    endpoint(),
                    crate::library::coverart::encode(&format!("{artist} {title}"))
                );
                let Some(doc) = crate::library::coverart::fetch_text(&url).ok().and_then(|t| Json::parse(&t)) else { continue };
                let Some(picture) = picture_in(&doc, &artist, &title) else { continue };
                if let Ok(bytes) = crate::library::coverart::fetch(&picture) {
                    if image::guess_format(&bytes).is_ok() {
                        let _ = std::fs::write(&path, bytes);
                        return;
                    }
                }
            }
        })
        .ok();
}

/// The picture of the first answer by the artist named — a band named
/// „with" someone, or someone „and his trio", counts — at 600 points, the
/// size the store keeps beside the 100 it names.
fn picture_in(doc: &Json, artist: &str, title: &str) -> Option<String> {
    let wanted = crate::db::sort_key(artist);
    let song = crate::db::sort_key(title);
    let same = |a: &str, b: &str| !a.is_empty() && !b.is_empty() && (a.contains(b) || b.contains(a));
    doc.list("results").iter().find_map(|r| {
        let by = crate::db::sort_key(r.text("artistName")?);
        let named = crate::db::sort_key(r.text("trackName")?);
        if !same(&by, &wanted) || !same(&named, &song) {
            return None;
        }
        let small = r.text("artworkUrl100")?;
        Some(small.replace("100x100bb", "600x600bb"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cover_is_the_named_artists_and_large() {
        let doc = Json::parse(
            r#"{"results":[
                {"artistName":"Someone Else","trackName":"Waltz for Debby","artworkUrl100":"https://a.example/x/100x100bb.jpg"},
                {"artistName":"Bill Evans Trio","trackName":"Waltz for Debby (Take 2)","artworkUrl100":"https://a.example/y/100x100bb.jpg"}
            ]}"#,
        )
        .unwrap();
        assert_eq!(picture_in(&doc, "Bill Evans", "Waltz for Debby").as_deref(), Some("https://a.example/y/600x600bb.jpg"));
        assert_eq!(picture_in(&doc, "Keith Jarrett", "Waltz for Debby"), None);
        assert_eq!(picture_in(&doc, "Bill Evans", "My Foolish Heart"), None);
    }
}
