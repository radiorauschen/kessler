//! The iPod as a place music is.
//!
//! The first column asks one question — where does what I am looking at come
//! from — and until now it had two answers: the whole library, and whatever
//! arrived last. A plugged-in iPod is a third, and the columns beside it work
//! exactly as they always do.
//!
//! What is different is where the rows come from. The library is a database
//! and answers in SQL; a device is a file that has been read into memory, and
//! answers here. So this builds the same `Artist`, `Album` and `Track` values
//! out of the device's own table, grouped and sorted the same way, and
//! everything downstream is none the wiser.
//!
//! Nothing in here writes to the device or to the library. It is a second,
//! read-only shelf standing next to the first.

use std::path::{Path, PathBuf};

use crate::db::models::{Album, Artist, SortField, SortOrder, Track};
use crate::ipod::itunesdb::ItunesDb;

/// A device that is plugged in and has been read.
pub struct Attached {
    /// The device's name: its own, as it says under settings → about, or
    /// the volume's, as the file manager shows it, where it has none.
    pub name: String,
    /// Where it is mounted — kept so the window can hand it back to the system
    /// to be unmounted, and so nothing has to go looking for it a second time.
    pub mount: PathBuf,
    /// Every track on it, in the shape the rest of the window understands.
    pub tracks: Vec<Track>,
    /// Total and free bytes, when the volume would say.
    pub capacity: Option<(u64, u64)>,
    /// How many playlists it carries. Shown, not yet opened.
    pub playlists: usize,
    /// What the device says it is — generation, and its model number when it
    /// still has a SysInfo to say so.
    pub model: String,
    /// Which one it is: a shuffle is drawn and offered otherwise.
    pub generation: crate::ipod::device_info::Generation,
    /// Why nothing may be written to it, when nothing may.
    ///
    /// Short, and in the window's own words: the device's own sentence is a
    /// paragraph of English, which is right for a terminal and wrong for a row
    /// in a menu. The long one is still what the status line says when a
    /// transfer is refused.
    pub refusal: Option<String>,
}

impl Attached {
    /// Read a device's database into library-shaped values.
    ///
    /// `info` is what the device says about itself, read once here rather than
    /// every frame: the menu shows the model and whether this one can be
    /// written to, and both of those are files on the device.
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
        let model = if info.signing().is_some() { format!("{model} {}", tr!("(beta)")) } else { model };
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
                        tr!("signatur (hash58) ohne firewire-guid — kessler schreibt nicht darauf").to_string()
                    }
                    _ if info.later_signature => tr!("neuere signatur — kessler schreibt dieses modell nicht").to_string(),
                    _ => match &info.model_number {
                        Some(number) => tr!("modell {} unbekannt — kessler schreibt nicht darauf", number),
                        None => tr!("modell unbekannt — kessler schreibt nicht darauf").to_string(),
                    },
                }
            }),
        }
    }

    /// How many of its tracks a typed query leaves standing.
    ///
    /// Kept beside the other counts rather than worked out while drawing: the
    /// row is drawn every frame, and a walk over eight thousand titles every
    /// frame is a walk too many.
    pub fn matching(&self, query: Option<&str>) -> usize {
        self.left_after(query).count()
    }

    /// Every artist on the device — see [`super::shelf`], which does the work
    /// for this shelf and for a list alike.
    pub fn artists(&self, query: Option<&str>) -> Vec<Artist> {
        super::shelf::artists(&self.tracks, query)
    }

    /// The albums on the device, narrowed to one artist when one is chosen.
    pub fn albums(&self, artist: Option<&str>, query: Option<&str>) -> Vec<Album> {
        super::shelf::albums(&self.tracks, artist, query)
    }

    /// The tracks, narrowed and sorted the way the track column asks for.
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

    /// The tracks a typed query leaves standing.
    fn left_after<'a>(&'a self, query: Option<&'a str>) -> impl Iterator<Item = &'a Track> + 'a {
        super::shelf::left_after(&self.tracks, query)
    }
}

/// One device track in the shape the rest of the window understands.
///
/// The id is the device's own, which is not a library id and must never be
/// mistaken for one — nothing here is written back, so it only has to be
/// unique among these tracks, which it is.
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

/// The device stores `:Music:F00:ABCD.mp3`; the file manager wants
/// `/Volumes/iPod/Music/F00/ABCD.mp3`.
fn resolve(mount: &Path, relative: &Path) -> PathBuf {
    let text = relative.to_string_lossy().replace(':', "/");
    mount.join(text.trim_start_matches('/'))
}

/// A word for the format, from the phrase the device stores.
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

    /// The device writes its paths with colons and no leading slash; the file
    /// manager wants slashes under the mount point. Without this every track
    /// on the device points at nothing.
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
