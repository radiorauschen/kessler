use super::coverart::{encode, fetch_text, lucene_safe, musicbrainz_endpoint};
use super::years::{same_artist, same_record, LEAST_SCORE, PACE};
use crate::error::{KesslerError, Result};
use crate::json::Json;

pub fn record_in(answer: &str, artist: &str, album: &str) -> Option<String> {
    let doc = Json::parse(answer)?;
    let mut best: Option<(f64, String)> = None;
    for group in doc.list("release-groups") {
        let score = group.get("score").and_then(Json::as_number).unwrap_or(0.0);
        if score < LEAST_SCORE {
            continue;
        }
        let (Some(id), Some(title)) = (group.text("id"), group.text("title")) else {
            continue;
        };
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
        if best.as_ref().is_none_or(|(s, _)| score > *s) {
            best = Some((score, id.to_string()));
        }
    }
    best.map(|(_, id)| id)
}

pub fn genre_in(answer: &str) -> Option<String> {
    let doc = Json::parse(answer)?;
    let mut best: Option<(f64, String)> = None;
    for genre in doc.list("genres") {
        let Some(name) = genre.text("name").map(str::trim).filter(|n| !n.is_empty()) else {
            continue;
        };
        let count = genre.get("count").and_then(Json::as_number).unwrap_or(0.0);
        if best.as_ref().is_none_or(|(c, _)| count > *c) {
            best = Some((count, name.to_string()));
        }
    }
    best.filter(|(count, _)| *count > 0.0).map(|(_, name)| name)
}

pub fn look_up(artist: &str, album: &str) -> Result<Option<String>> {
    if lucene_safe(artist).is_empty() || lucene_safe(album).is_empty() {
        return Ok(None);
    }
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
    let answer = fetch_text(&url)?;
    if Json::parse(&answer).is_none() {
        return Err(KesslerError::Metadata(
            tr!("musicbrainz antwortet unverständlich").into(),
        ));
    }
    let Some(id) = record_in(&answer, artist, album) else {
        return Ok(None);
    };
    std::thread::sleep(PACE);
    let url = format!("{}/{}?inc=genres&fmt=json", musicbrainz_endpoint(), encode(&id));
    let answer = fetch_text(&url)?;
    Ok(genre_in(&answer))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEARCH: &str = r#"{"release-groups":[
        {"id":"geo","score":100,"title":"Geogaddi",
         "artist-credit":[{"artist":{"name":"Boards of Canada"}}]},
        {"id":"other","score":100,"title":"Geogaddi",
         "artist-credit":[{"artist":{"name":"Somebody Else"}}]},
        {"id":"unsure","score":50,"title":"Geogaddi",
         "artist-credit":[{"artist":{"name":"Boards of Canada"}}]}
    ]}"#;

    #[test]
    fn the_record_is_the_sure_match_by_the_same_artist() {
        assert_eq!(
            record_in(SEARCH, "boards of canada", "geogaddi").as_deref(),
            Some("geo")
        );
        assert_eq!(record_in(SEARCH, "aphex twin", "geogaddi"), None);
        assert_eq!(record_in(SEARCH, "boards of canada", "twoism"), None);
    }

    #[test]
    fn the_genre_is_the_one_most_people_gave() {
        let answer = r#"{"id":"geo","genres":[
            {"name":"ambient","count":2},
            {"name":"electronic","count":7},
            {"name":"idm","count":7}
        ]}"#;
        assert_eq!(genre_in(answer).as_deref(), Some("electronic"));
        assert_eq!(genre_in(r#"{"genres":[]}"#), None);
        assert_eq!(genre_in(r#"{"genres":[{"name":"ambient","count":0}]}"#), None);
    }
}
