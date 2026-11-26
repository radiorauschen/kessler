//! The year a record came out, asked of MusicBrainz.
//!
//! MusicBrainz keeps, for every record, the date its first release came out
//! — the „release group", which is the record as opposed to any one pressing
//! of it — so a 2011 reissue of a 1973 album still says 1973. That is the
//! year a shelf wants.
//!
//! Only an answer that is plainly about the same record is taken: the title
//! and the artist have to match once case, accents and punctuation are set
//! aside, and MusicBrainz has to be sure of its own match. Anything less and
//! nothing is written, because a wrong year in a tag is worse than none —
//! it looks right.

use super::coverart::{encode, fetch_text, lucene_safe, musicbrainz_endpoint};
use crate::error::{KesslerError, Result};
use crate::json::Json;

/// How sure MusicBrainz has to be of its own match, out of a hundred.
pub(super) const LEAST_SCORE: f64 = 90.0;

/// How long to wait between two questions. MusicBrainz asks for no more than
/// one a second from anybody, and a program that asks faster is turned away.
pub const PACE: std::time::Duration = std::time::Duration::from_millis(1100);

/// A name with everything that is not a letter or a digit taken out, folded the
/// way the library folds names — so and are the same record.
pub(super) fn plain(name: &str) -> String {
    crate::db::sort_key(name)
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// Whether a name MusicBrainz gave is the one asked about. The artist may be
/// credited with more than one name, so for the artist it is enough that one
/// side contains the other.
pub(super) fn same_record(asked: &str, found: &str) -> bool {
    let (a, f) = (plain(asked), plain(found));
    !a.is_empty() && a == f
}

pub(super) fn same_artist(asked: &str, found: &str) -> bool {
    let (a, f) = (plain(asked), plain(found));
    !a.is_empty() && !f.is_empty() && (a == f || a.contains(&f) || f.contains(&a))
}

/// The year out of an answer, if one of its release groups is this record.
///
/// Apart from the asking so that it can be checked without the network.
pub fn year_in(answer: &str, artist: &str, album: &str) -> Option<i32> {
    let doc = Json::parse(answer)?;
    let mut best: Option<(f64, i32)> = None;
    for group in doc.list("release-groups") {
        let score = group.get("score").and_then(Json::as_number).unwrap_or(0.0);
        if score < LEAST_SCORE {
            continue;
        }
        let Some(title) = group.text("title") else { continue };
        if !same_record(album, title) {
            continue;
        }
        let credited: String = group
            .list("artist-credit")
            .iter()
            .filter_map(|c| {
                c.get("artist")
                    .and_then(|a| a.text("name"))
                    .or_else(|| c.text("name"))
            })
            .collect::<Vec<_>>()
            .join(" ");
        if !same_artist(artist, &credited) {
            continue;
        }
        let Some(year) = group
            .text("first-release-date")
            .and_then(|d| d.get(..4))
            .and_then(|y| y.parse::<i32>().ok())
            .filter(|y| (1850..=2100).contains(y))
        else {
            continue;
        };
        let better = match best {
            None => true,
            Some((s, y)) => score > s || (score == s && year < y),
        };
        if better {
            best = Some((score, year));
        }
    }
    best.map(|(_, year)| year)
}

/// Ask MusicBrainz for the year of one record. `Ok(None)` when it does not
/// know it well enough to say.
pub fn look_up(artist: &str, album: &str) -> Result<Option<i32>> {
    let query = format!(
        "artist:\"{}\" AND releasegroup:\"{}\"",
        lucene_safe(artist),
        lucene_safe(album)
    );
    if lucene_safe(artist).is_empty() || lucene_safe(album).is_empty() {
        return Ok(None);
    }
    let url = format!(
        "{}?query={}&fmt=json&limit=5",
        musicbrainz_endpoint(),
        encode(&query)
    );
    let answer = fetch_text(&url)?;
    if Json::parse(&answer).is_none() {
        return Err(KesslerError::Metadata(
            tr!("musicbrainz antwortet unverständlich").into(),
        ));
    }
    Ok(year_in(&answer, artist, album))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANSWER: &str = r#"{"release-groups":[
        {"id":"a","score":100,"title":"Music Has the Right to Children",
         "first-release-date":"1998-04-20",
         "artist-credit":[{"name":"Boards of Canada","artist":{"name":"Boards of Canada"}}]},
        {"id":"b","score":100,"title":"Music Has the Right to Children",
         "first-release-date":"2004",
         "artist-credit":[{"artist":{"name":"Boards of Canada"}}]},
        {"id":"c","score":95,"title":"Geogaddi","first-release-date":"2002-02-18",
         "artist-credit":[{"artist":{"name":"Boards of Canada"}}]},
        {"id":"d","score":60,"title":"Twoism","first-release-date":"1995",
         "artist-credit":[{"artist":{"name":"Boards of Canada"}}]}
    ]}"#;

    #[test]
    fn the_earliest_sure_match_is_the_year() {
        assert_eq!(
            year_in(ANSWER, "boards of canada", "music has the right to children."),
            Some(1998)
        );
        assert_eq!(year_in(ANSWER, "Boards Of Canada", "GEOGADDI"), Some(2002));
    }

    /// A match MusicBrainz is not sure of, a different record, or a different
    /// artist are all no answer.
    #[test]
    fn anything_unsure_is_no_answer() {
        assert_eq!(year_in(ANSWER, "boards of canada", "twoism"), None);
        assert_eq!(year_in(ANSWER, "boards of canada", "tomorrow's harvest"), None);
        assert_eq!(year_in(ANSWER, "aphex twin", "geogaddi"), None);
        assert_eq!(year_in("not json", "a", "b"), None);
        assert_eq!(year_in(r#"{"release-groups":[]}"#, "a", "b"), None);
    }

    /// Credited together still counts as the artist asked about.
    #[test]
    fn a_shared_credit_is_the_artist() {
        let answer = r#"{"release-groups":[{"score":100,"title":"Through the Hill",
            "first-release-date":"1994",
            "artist-credit":[{"artist":{"name":"Andy Partridge"}},{"artist":{"name":"Harold Budd"}}]}]}"#;
        assert_eq!(
            year_in(answer, "andy partridge & harold budd", "through the hill"),
            Some(1994)
        );
    }
}
