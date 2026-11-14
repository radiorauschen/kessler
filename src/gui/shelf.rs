use crate::db::models::{Album, Artist, SortField, SortOrder, Track};
use crate::db::{grouping_artist, shelf_key, sort_key};

pub fn artists(shelf: &[Track], query: Option<&str>) -> Vec<Artist> {
    let mut found: Vec<Artist> = Vec::new();
    let mut places: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut albums_of: Vec<std::collections::HashSet<String>> = Vec::new();

    for track in left_after(shelf, query) {
        let name = grouping_artist(track);
        let key = sort_key(name);
        let album_key = sort_key(&track.album);

        match places.get(&key) {
            Some(&at) => {
                found[at].track_count += 1;
                albums_of[at].insert(album_key);
            }
            None => {
                places.insert(key, found.len());
                albums_of.push(std::collections::HashSet::from([album_key]));
                found.push(Artist {
                    name: name.to_string(),
                    album_count: 0,
                    track_count: 1,
                });
            }
        }
    }

    for (artist, albums) in found.iter_mut().zip(&albums_of) {
        artist.album_count = albums.len() as u32;
    }
    found.sort_by_cached_key(|a| (shelf_key(&a.name), sort_key(&a.name)));
    found
}

pub fn albums(shelf: &[Track], artist: Option<&str>, query: Option<&str>) -> Vec<Album> {
    let wanted = artist.map(sort_key);
    let mut found: Vec<Album> = Vec::new();
    let mut places: std::collections::HashMap<(String, String), usize> = std::collections::HashMap::new();

    for track in left_after(shelf, query) {
        let artist_key = sort_key(grouping_artist(track));
        if let Some(wanted) = &wanted {
            if artist_key != *wanted {
                continue;
            }
        }
        let key = (sort_key(&track.album), artist_key);
        match places.get(&key) {
            Some(&at) => {
                found[at].track_count += 1;
                found[at].total_duration_secs += track.duration_secs;
                found[at].has_artwork |= track.has_artwork;
                if let Some(year) = track.year {
                    found[at].year = Some(found[at].year.map_or(year, |had| had.min(year)));
                }
            }
            None => {
                places.insert(key, found.len());
                found.push(Album {
                    name: track.album.clone(),
                    artist: grouping_artist(track).to_string(),
                    year: track.year,
                    added: String::new(),
                    track_count: 1,
                    total_duration_secs: track.duration_secs,
                    has_artwork: track.has_artwork,
                });
            }
        }
    }

    let mut keyed: Vec<(String, Album)> = found.into_iter().map(|a| (sort_key(&a.name), a)).collect();
    keyed.sort_by(|(ak, a), (bk, b)| {
        ak.is_empty().cmp(&bk.is_empty()).then_with(|| {
            if artist.is_some() {
                a.year.cmp(&b.year).then_with(|| ak.cmp(bk))
            } else {
                ak.cmp(bk)
            }
        })
    });
    keyed.into_iter().map(|(_, a)| a).collect()
}

pub fn tracks(
shelf: &[Track],
artist: Option<&str>,
album: Option<&str>,
sort: SortField,
order: SortOrder,
query: Option<&str>,
) -> Vec<Track> {
    let wanted_artist = artist.map(sort_key);
    let wanted_album = album.map(sort_key);

    struct Keys {
        rank: ((u8, String), String),
        album: String,
        year: Option<i32>,
        other: String,
    }
    let mut found: Vec<(Keys, Track)> = left_after(shelf, query)
        .filter(|track| match &wanted_artist {
            Some(key) => sort_key(grouping_artist(track)) == *key,
            None => true,
        })
        .filter(|track| match &wanted_album {
            Some(key) => sort_key(&track.album) == *key,
            None => true,
        })
        .map(|track| {
            let other = match sort {
                SortField::Title => sort_key(&track.title),
                SortField::Genre => sort_key(&track.genre),
                _ => String::new(),
            };
            (Keys { rank: artist_rank(track), album: sort_key(&track.album), year: track.year, other }, track.clone())
        })
        .collect();
    let mut record_years: std::collections::HashMap<(String, String), i32> = std::collections::HashMap::new();
    for (keys, _) in &found {
        if let Some(year) = keys.year {
            let held = record_years.entry((keys.rank.1.clone(), keys.album.clone())).or_insert(year);
            *held = (*held).min(year);
        }
    }
    for (keys, _) in &mut found {
        keys.year = record_years.get(&(keys.rank.1.clone(), keys.album.clone())).copied();
    }

    found.sort_by(|(ka, a), (kb, b)| {
        let first = match sort {
            SortField::Title => ka.other.cmp(&kb.other),
            SortField::Artist => ka.rank.cmp(&kb.rank),
            SortField::Album => ka.album.cmp(&kb.album),
            SortField::Year => a.year.cmp(&b.year),
            SortField::Duration => a
                .duration_secs
                .partial_cmp(&b.duration_secs)
                .unwrap_or(std::cmp::Ordering::Equal),
            SortField::TrackNumber => a.track_number.cmp(&b.track_number),
            SortField::Genre => ka.other.cmp(&kb.other),
            SortField::Rating => a.rating.cmp(&b.rating),
            SortField::PlayCount => a.play_count.cmp(&b.play_count),
            SortField::DateAdded => a.date_added.cmp(&b.date_added),
        };
        let settled = first
            .then_with(|| ka.rank.cmp(&kb.rank))
            .then_with(|| ka.year.cmp(&kb.year))
            .then_with(|| ka.album.cmp(&kb.album))
            .then_with(|| a.disc_number.cmp(&b.disc_number))
            .then_with(|| a.track_number.cmp(&b.track_number));
        match order {
            SortOrder::Ascending => settled,
            SortOrder::Descending => settled.reverse(),
        }
    });
    found.into_iter().map(|(_, track)| track).collect()
}

fn artist_rank(track: &Track) -> ((u8, String), String) {
    let shelved = grouping_artist(track);
    (shelf_key(shelved), sort_key(shelved))
}

pub fn left_after<'a>(shelf: &'a [Track], query: Option<&'a str>) -> impl Iterator<Item = &'a Track> + 'a {
    let words: Vec<String> = query
        .map(|q| q.split_whitespace().map(sort_key).collect())
        .unwrap_or_default();

    shelf.iter().filter(move |track| {
        if words.is_empty() {
            return true;
        }
        let haystack = sort_key(&format!("{} {} {}", track.title, track.artist, track.album));
        words.iter().all(|word| haystack.contains(word.as_str()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: i64, artist: &str, album_artist: &str, album: &str, year: Option<i32>, number: u32) -> Track {
        Track {
            id,
            path: std::path::PathBuf::from(format!("/ipod/{id}.mp3")),
            title: format!("titel {id}"),
            artist: artist.into(),
            album_artist: album_artist.into(),
            album: album.into(),
            genre: String::new(),
            year,
            track_number: Some(number),
            disc_number: None,
            duration_secs: 100.0,
            bitrate: None,
            sample_rate: None,
            file_size: 0,
            format: "mp3".into(),
            rating: 0,
            play_count: 0,
            last_played: None,
            date_added: chrono::Local::now().naive_local(),
            date_modified: chrono::Local::now().naive_local(),
            file_hash: None,
            has_artwork: false,
        }
    }

    #[test]
    fn a_record_is_not_split_by_its_track_artists_or_years() {
        let shelf = vec![
            track(1, "zeta", "band", "platte", Some(2001), 1),
            track(2, "alpha", "band", "platte", Some(1999), 2),
            track(3, "band", "", "andere", Some(2000), 1),
            track(4, "band", "band", "platte", None, 3),
        ];
        let listed = tracks(&shelf, None, None, SortField::Artist, SortOrder::Ascending, None);
        let ids: Vec<i64> = listed.iter().map(|t| t.id).collect();
        assert_eq!(ids, vec![1, 2, 4, 3]);
    }
}
