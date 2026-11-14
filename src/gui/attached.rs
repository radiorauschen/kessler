use std::path::{Path, PathBuf};

use crate::db::models::{Album, Artist, SortField, SortOrder, Track};
use crate::ipod::itunesdb::ItunesDb;

pub struct Attached {
    pub name: String,
    pub mount: PathBuf,
    pub tracks: Vec<Track>,
    pub capacity: Option<(u64, u64)>,
    pub playlists: usize,
    pub model: String,
    pub generation: crate::ipod::device_info::Generation,
    pub refusal: Option<String>,
}

impl Attached {
    pub fn read(
        name: String,
        mount: PathBuf,
        db: &ItunesDb,
        capacity: Option<(u64, u64)>,
        info: &crate::ipod::device_info::DeviceInfo,
    ) -> Self {
        let tracks = db.tracks.iter().map(|t| as_track(t, &mount)).collect();
        let generation = info
            .generation
            .label()
            .split(" (")
            .next()
            .unwrap_or_default()
            .to_string();
        let model = match &info.model_number {
            Some(number) => format!("{generation} · {number}"),
            None => generation,
        };
        let name = db
            .master_playlist()
            .map(|p| p.name.trim())
            .filter(|n| !n.is_empty())
            .map(str::to_string)
            .unwrap_or(name);
        Attached {
            name,
            mount,
            tracks,
            capacity,
            playlists: db.user_playlists().count(),
            model,
            generation: info.generation,
            refusal: info.writable().err().map(|_| {
                match info.checksum() {
                    crate::ipod::device_info::Checksum::Hash58 => {
                        tr!("signatur (hash58) — kessler schreibt dieses modell nicht").to_string()
                    }
                    _ => match &info.model_number {
                        Some(number) => tr!("modell {} unbekannt — kessler schreibt nicht darauf", number),
                        None => tr!("modell unbekannt — kessler schreibt nicht darauf").to_string(),
                    },
                }
            }),
        }
    }

    pub fn matching(&self, query: Option<&str>) -> usize {
        self.left_after(query).count()
    }

    pub fn artists(&self, query: Option<&str>) -> Vec<Artist> {
        super::shelf::artists(&self.tracks, query)
    }

    pub fn albums(&self, artist: Option<&str>, query: Option<&str>) -> Vec<Album> {
        super::shelf::albums(&self.tracks, artist, query)
    }

    pub fn tracks(
        &self,
        artist: Option<&str>,
        album: Option<&str>,
        sort: SortField,
        order: SortOrder,
        query: Option<&str>,
    ) -> Vec<Track> {
        super::shelf::tracks(&self.tracks, artist, album, sort, order, query)
    }

    fn left_after<'a>(&'a self, query: Option<&'a str>) -> impl Iterator<Item = &'a Track> + 'a {
        super::shelf::left_after(&self.tracks, query)
    }
}

fn as_track(track: &crate::ipod::itunesdb::IpodTrack, mount: &Path) -> Track {
    let path = track
        .path
        .as_ref()
        .map(|relative| resolve(mount, relative))
        .unwrap_or_default();

    Track {
        id: track.id as i64,
        path,
        title: track.title.clone(),
        artist: track.artist.clone(),
        album_artist: track.album_artist.clone(),
        album: track.album.clone(),
        genre: track.genre.clone(),
        year: (track.year > 0).then_some(track.year as i32),
        track_number: (track.track_number > 0).then_some(track.track_number),
        disc_number: (track.disc_number > 0).then_some(track.disc_number),
        duration_secs: track.length_ms as f64 / 1000.0,
        bitrate: (track.bitrate > 0).then_some(track.bitrate),
        sample_rate: (track.sample_rate > 0).then_some(track.sample_rate),
        file_size: track.size_bytes as u64,
        format: format_of(&track.filetype),
        rating: track.stars(),
        play_count: track.play_count,
        last_played: track.last_played.and_then(from_unix),
        date_added: track.date_added.and_then(from_unix).unwrap_or_default(),
        date_modified: track.date_modified.and_then(from_unix).unwrap_or_default(),
        file_hash: None,
        has_artwork: track.has_artwork,
    }
}

fn resolve(mount: &Path, relative: &Path) -> PathBuf {
    let text = relative.to_string_lossy().replace(':', "/");
    mount.join(text.trim_start_matches('/'))
}

fn format_of(filetype: &str) -> String {
    let lower = filetype.to_lowercase();
    if lower.contains("aac") || lower.contains("mpeg-4") || lower.contains("m4a") {
        "m4a".to_string()
    } else if lower.contains("mpeg") || lower.contains("mp3") {
        "mp3".to_string()
    } else if lower.contains("wav") {
        "wav".to_string()
    } else if lower.contains("aiff") {
        "aiff".to_string()
    } else {
        String::new()
    }
}

fn from_unix(seconds: i64) -> Option<chrono::NaiveDateTime> {
    chrono::DateTime::from_timestamp(seconds, 0).map(|t| t.naive_local())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipod::itunesdb::IpodTrack;

    fn device_track(id: u32, artist: &str, album: &str, title: &str, year: u32) -> IpodTrack {
        IpodTrack {
            id,
            title: title.into(),
            artist: artist.into(),
            album: album.into(),
            year,
            length_ms: 200_000,
            track_number: id,
            path: Some(PathBuf::from(format!(":Music:F00:{id:04}.mp3"))),
            filetype: "MPEG audio file".into(),
            ..Default::default()
        }
    }

    fn shelf(tracks: Vec<IpodTrack>) -> Attached {
        let db = ItunesDb {
            tracks,
            ..Default::default()
        };
        Attached::read(
            "iPod".into(),
            PathBuf::from("/Volumes/iPod"),
            &db,
            None,
            &crate::ipod::device_info::DeviceInfo::read(Path::new("/nowhere")),
        )
    }

    fn a_few() -> Attached {
        shelf(vec![
            device_track(1, "Autechre", "Amber", "Foil", 1994),
            device_track(2, "Autechre", "Amber", "Montreal", 1994),
            device_track(3, "Autechre", "Tri Repetae", "Dael", 1995),
            device_track(4, "Aphex Twin", "Drukqs", "Jynweythek", 2001),
            device_track(5, "", "", "namenlos", 0),
        ])
    }

    #[test]
    fn the_device_becomes_artists_albums_and_tracks() {
        let shelf = a_few();
        assert_eq!(shelf.tracks.len(), 5);

        let artists = shelf.artists(None);
        let names: Vec<&str> = artists.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, vec!["Aphex Twin", "Autechre", ""]);

        let autechre = &artists[1];
        assert_eq!(autechre.track_count, 3);
        assert_eq!(autechre.album_count, 2);
    }

    #[test]
    fn albums_narrow_to_the_chosen_artist() {
        let shelf = a_few();
        let all = shelf.albums(None, None);
        assert_eq!(all.len(), 4, "amber, drukqs, tri repetae, and the nameless one");
        assert_eq!(
            all.last().unwrap().name,
            "",
            "the nameless album goes last, as in the library"
        );

        let theirs = shelf.albums(Some("Autechre"), None);
        let names: Vec<&str> = theirs.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, vec!["Amber", "Tri Repetae"]);
        assert_eq!(theirs[0].track_count, 2);
    }

    #[test]
    fn tracks_narrow_to_the_chosen_album() {
        let shelf = a_few();
        let found = shelf.tracks(
            Some("Autechre"),
            Some("Amber"),
            SortField::TrackNumber,
            SortOrder::Ascending,
            None,
        );
        let titles: Vec<&str> = found.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, vec!["Foil", "Montreal"]);
    }

    #[test]
    fn a_search_narrows_the_device_the_way_it_narrows_the_library() {
        let shelf = a_few();
        assert_eq!(shelf.artists(Some("amber")).len(), 1);

        let found = shelf.tracks(None, None, SortField::Title, SortOrder::Ascending, Some("mont"));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].title, "Montreal");

        assert!(shelf
            .tracks(None, None, SortField::Title, SortOrder::Ascending, Some("foil drukqs"))
            .is_empty());
    }

    #[test]
    fn sorting_turns_round_when_asked() {
        let shelf = a_few();
        let up = shelf.tracks(None, None, SortField::Title, SortOrder::Ascending, None);
        let down = shelf.tracks(None, None, SortField::Title, SortOrder::Descending, None);
        let first: Vec<&str> = up.iter().map(|t| t.title.as_str()).collect();
        let last: Vec<&str> = down.iter().rev().map(|t| t.title.as_str()).collect();
        assert_eq!(first, last);
    }

    #[test]
    fn a_device_path_becomes_a_real_one() {
        assert_eq!(
            resolve(Path::new("/Volumes/iPod"), Path::new(":Music:F00:ABCD.mp3")),
            PathBuf::from("/Volumes/iPod/Music/F00/ABCD.mp3")
        );
        assert_eq!(
            resolve(Path::new("/Volumes/iPod"), Path::new("iPod_Control/Music/F01/X.mp3")),
            PathBuf::from("/Volumes/iPod/iPod_Control/Music/F01/X.mp3")
        );
    }

    #[test]
    fn the_stored_phrase_becomes_a_format() {
        assert_eq!(format_of("MPEG audio file"), "mp3");
        assert_eq!(format_of("AAC audio file"), "m4a");
        assert_eq!(format_of("MPEG-4 audio file"), "m4a");
        assert_eq!(format_of("WAV audio file"), "wav");
        assert_eq!(format_of("something else"), "");
    }

    #[test]
    fn an_empty_device_is_an_empty_shelf() {
        let shelf = shelf(Vec::new());
        assert!(shelf.artists(None).is_empty());
        assert!(shelf.albums(None, None).is_empty());
        assert!(shelf
            .tracks(None, None, SortField::Title, SortOrder::Ascending, None)
            .is_empty());
    }
}
