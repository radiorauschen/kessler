use lofty::prelude::*;
use lofty::probe::Probe;
use std::path::Path;

use crate::db::models::Track;
use crate::error::{KesslerError, Result};

pub const SUPPORTED_EXTENSIONS: &[&str] = &["mp3", "flac", "ogg", "oga", "wav", "m4a", "aif", "aiff", "aifc"];

pub const FOREIGN_EXTENSIONS: &[&str] = &[
    "aac", "m4b", "alac", "opus", "wma", "ape", "wv", "mpc", "dsf",
];

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_lowercase())
        .unwrap_or_default()
}

pub fn is_supported_audio(path: &Path) -> bool {
    SUPPORTED_EXTENSIONS.contains(&extension(path).as_str())
}

pub fn foreign_audio(path: &Path) -> Option<String> {
    let ext = extension(path);
    FOREIGN_EXTENSIONS.contains(&ext.as_str()).then_some(ext)
}

fn arrival(
    modified: chrono::NaiveDateTime,
    now: chrono::NaiveDateTime,
) -> chrono::NaiveDateTime {
    modified.min(now)
}

fn in_order(file: &lofty::file::TaggedFile) -> Vec<&lofty::tag::Tag> {
    let mut tags: Vec<&lofty::tag::Tag> = Vec::new();
    if let Some(primary) = file.primary_tag() {
        tags.push(primary);
    }
    for tag in file.tags() {
        if !tags.iter().any(|t| t.tag_type() == tag.tag_type()) {
            tags.push(tag);
        }
    }
    tags
}

fn first_text(tags: &[&lofty::tag::Tag], get: &dyn Fn(&lofty::tag::Tag) -> Option<String>) -> String {
    tags.iter()
        .find_map(|t| get(t).filter(|s| !s.trim().is_empty()))
        .unwrap_or_default()
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Extras {
    pub composer: String,
    pub grouping: String,
    pub bpm: Option<u32>,
    pub comment: String,
}

pub fn read_extras(path: &Path) -> Extras {
    strict_extras(path).unwrap_or_default()
}

pub fn strict_extras(path: &Path) -> Option<Extras> {
    let file = Probe::open(path).and_then(|p| p.read()).ok()?;
    let tags = in_order(&file);
    let text = |key: ItemKey| first_text(&tags, &|t| t.get_string(&key).map(str::to_string));
    let mut grouping = text(ItemKey::ContentGroup);
    if grouping.is_empty() {
        grouping = text(ItemKey::AppleId3v2ContentGroup);
    }
    let mut bpm = text(ItemKey::IntegerBpm);
    if bpm.is_empty() {
        bpm = text(ItemKey::Bpm);
    }
    Some(Extras {
        composer: text(ItemKey::Composer),
        grouping,
        bpm: bpm.trim().parse::<f64>().ok().filter(|n| *n > 0.0 && *n < 10_000.0).map(|n| n.round() as u32),
        comment: first_text(&tags, &|t| {
            t.get_items(&ItemKey::Comment)
                .find(|item| item.description().is_empty())
                .and_then(|item| item.value().text())
                .map(str::to_string)
        }),
    })
}

pub fn read_metadata(path: &Path) -> Result<Track> {
    let tagged_file = match Probe::open(path)
        .map_err(|e| KesslerError::Metadata(format!("Cannot open {}: {}", path.display(), e)))?
        .read()
    {
        Ok(file) => file,
        Err(strict) => return salvaged(path, &strict.to_string()),
    };

    let properties = tagged_file.properties();
    let duration = properties.duration();
    let bitrate = properties.audio_bitrate();
    let sample_rate = properties.sample_rate();

    let tags = in_order(&tagged_file);
    let text = |get: &dyn Fn(&lofty::tag::Tag) -> Option<String>| first_text(&tags, get);
    let number = |get: &dyn Fn(&lofty::tag::Tag) -> Option<u32>| tags.iter().find_map(|t| get(t));
    let title = text(&|t| t.title().map(|s| s.to_string()));
    let artist = text(&|t| t.artist().map(|s| s.to_string()));
    let album_artist = text(&|t| t.get_string(&ItemKey::AlbumArtist).map(|s| s.to_string()));
    let album = text(&|t| t.album().map(|s| s.to_string()));
    let genre = text(&|t| t.genre().map(|s| s.to_string()));
    let year = number(&|t| t.year()).map(|y| y as i32);
    let track_number = number(&|t| t.track());
    let disc_number = number(&|t| t.disk());
    let has_artwork = tags.iter().any(|t| !t.pictures().is_empty());

    assemble(
        path,
        Fields {
            title,
            artist,
            album_artist,
            album,
            genre,
            year,
            track_number,
            disc_number,
            has_artwork,
        },
        Sound {
            duration_secs: duration.as_secs_f64(),
            bitrate,
            sample_rate,
        },
    )
}

#[derive(Debug, Clone, Default)]
struct Fields {
    title: String,
    artist: String,
    album_artist: String,
    album: String,
    genre: String,
    year: Option<i32>,
    track_number: Option<u32>,
    disc_number: Option<u32>,
    has_artwork: bool,
}

#[derive(Debug, Clone, Default)]
struct Sound {
    duration_secs: f64,
    bitrate: Option<u32>,
    sample_rate: Option<u32>,
}

fn assemble(path: &Path, fields: Fields, sound: Sound) -> Result<Track> {
    let file_meta = std::fs::metadata(path)?;
    let file_size = file_meta.len();
    let modified = file_meta
        .modified()
        .ok()
        .and_then(|t| {
            t.duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|d| d.as_secs())
        })
        .and_then(|secs| {
            chrono::DateTime::from_timestamp(secs as i64, 0).map(|dt| dt.naive_local())
        })
        .unwrap_or_else(|| chrono::Local::now().naive_local());

    let format = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    Ok(Track {
        id: 0,
        path: path.to_path_buf(),
        title: fields.title,
        artist: fields.artist,
        album_artist: fields.album_artist,
        album: fields.album,
        genre: fields.genre,
        year: fields.year,
        track_number: fields.track_number,
        disc_number: fields.disc_number,
        duration_secs: sound.duration_secs,
        bitrate: sound.bitrate,
        sample_rate: sound.sample_rate,
        file_size,
        format,
        rating: 0,
        play_count: 0,
        last_played: None,
        date_added: arrival(modified, chrono::Local::now().naive_local()),
        date_modified: modified,
        file_hash: None,
        has_artwork: fields.has_artwork,
    })
}

fn salvaged(path: &Path, strict: &str) -> Result<Track> {
    let sound = Probe::open(path).ok().and_then(|p| {
        p.options(lofty::config::ParseOptions::new().read_tags(false))
            .read()
            .ok()
            .map(|f| Sound {
                duration_secs: f.properties().duration().as_secs_f64(),
                bitrate: f.properties().audio_bitrate(),
                sample_rate: f.properties().sample_rate(),
            })
    });

    let head = read_head(path)?;
    let tag = super::id3::salvage(&head).unwrap_or_default();

    if sound.is_none() && tag.is_empty() {
        return Err(KesslerError::Metadata(format!(
            "Cannot read tags from {}: {}",
            path.display(),
            strict
        )));
    }
    let sound = sound.unwrap_or_default();

    let mut fields = Fields {
        title: tag.title,
        artist: tag.artist,
        album_artist: tag.album_artist,
        album: tag.album,
        genre: tag.genre,
        year: tag.year,
        track_number: tag.track_number,
        disc_number: tag.disc_number,
        has_artwork: tag.has_artwork,
    };
    from_path(path, &mut fields);

    tracing::debug!(
        "Salvaged {} — {} / {} / {}",
        path.display(),
        fields.artist,
        fields.album,
        fields.title
    );

    assemble(path, fields, sound)
}

fn read_head(path: &Path) -> Result<Vec<u8>> {
    use std::io::Read;

    let file = std::fs::File::open(path)?;
    let mut head = Vec::new();
    file.take(super::id3::READ_AT_MOST as u64)
        .read_to_end(&mut head)?;
    Ok(head)
}

fn from_path(path: &Path, fields: &mut Fields) {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    let parent = path.parent();
    let album_dir = parent
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let artist_dir = parent
        .and_then(|p| p.parent())
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("");

    let named = split_leading_number(&stem);
    if fields.title.is_empty() {
        fields.title = if named.title.is_empty() {
            stem
        } else {
            named.title
        };
    }
    if fields.track_number.is_none() {
        fields.track_number = named.track;
    }
    if fields.disc_number.is_none() {
        fields.disc_number = named.disc;
    }
    if fields.album.is_empty() {
        fields.album = album_dir.to_string();
    }
    if fields.artist.is_empty() {
        fields.artist = artist_dir.to_string();
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Named {
    disc: Option<u32>,
    track: Option<u32>,
    title: String,
}

fn split_leading_number(stem: &str) -> Named {
    let trimmed = stem.trim_start();
    let digits: String = trimmed.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return Named::default();
    }
    let after = &trimmed[digits.len()..];

    if let Some(rest) = after.strip_prefix('-') {
        let second: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !second.is_empty() {
            let title = rest[second.len()..]
                .trim_start_matches([' ', '.', '-', '_'])
                .trim();
            return Named {
                disc: digits.parse().ok(),
                track: second.parse().ok(),
                title: title.to_string(),
            };
        }
    }

    let title = after.trim_start_matches([' ', '.', '-', '_']).trim();
    if title.is_empty() {
        return Named::default();
    }
    Named {
        disc: None,
        track: digits.parse().ok(),
        title: title.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(y: i32, m: u32, d: u32) -> chrono::NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(9, 30, 0)
            .unwrap()
    }

    #[test]
    fn names_missing_from_the_front_tag_come_from_the_one_at_the_end() {
        fn synchsafe(n: usize) -> [u8; 4] {
            [((n >> 21) & 0x7f) as u8, ((n >> 14) & 0x7f) as u8, ((n >> 7) & 0x7f) as u8, (n & 0x7f) as u8]
        }
        fn frame(id: &[u8], words: &str) -> Vec<u8> {
            let mut data = vec![3u8];
            data.extend_from_slice(words.as_bytes());
            [id.to_vec(), synchsafe(data.len()).to_vec(), vec![0, 0], data].concat()
        }
        fn padded(text: &str, len: usize) -> Vec<u8> {
            let mut out = text.as_bytes().to_vec();
            out.resize(len, 0);
            out
        }
        let body = [frame(b"TCON", "Psych Folk / Drone"), frame(b"TDRC", "2006"), frame(b"TRCK", "02/08")].concat();
        let mut raw = b"ID3".to_vec();
        raw.extend_from_slice(&[4, 0, 0]);
        raw.extend_from_slice(&synchsafe(body.len()));
        raw.extend_from_slice(&body);
        raw.extend_from_slice(&super::super::fixtures::bytes(super::super::fixtures::MP3));
        raw.extend_from_slice(b"TAG");
        raw.extend_from_slice(&padded("Cherry", 30));
        raw.extend_from_slice(&padded("Ajilvsga", 30));
        raw.extend_from_slice(&padded("The Harvest", 30));
        raw.extend_from_slice(b"2006");
        raw.extend_from_slice(&[0u8; 28]);
        raw.extend_from_slice(&[0, 2, 255]);

        let dir = std::env::temp_dir().join("kessler-meta-two-tags");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("02 Cherry.mp3");
        std::fs::write(&path, raw).unwrap();

        let track = read_metadata(&path).unwrap();
        assert_eq!(track.title, "Cherry");
        assert_eq!(track.artist, "Ajilvsga");
        assert_eq!(track.album, "The Harvest");
        assert_eq!(track.genre, "Psych Folk / Drone");
        assert_eq!(track.track_number, Some(2));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_arrives_when_it_was_written() {
        let now = at(2026, 9, 11);
        assert_eq!(arrival(at(2024, 2, 2), now), at(2024, 2, 2));
        assert_eq!(arrival(at(2026, 9, 10), now), at(2026, 9, 10));
    }

    #[test]
    fn nothing_arrives_later_than_now() {
        let now = at(2026, 9, 11);
        assert_eq!(arrival(at(2031, 1, 1), now), now);
    }
}

#[cfg(test)]
mod salvage_tests {
    use super::*;
    use crate::library::fixtures;

    fn synchsafe(n: usize) -> [u8; 4] {
        [
            ((n >> 21) & 0x7f) as u8,
            ((n >> 14) & 0x7f) as u8,
            ((n >> 7) & 0x7f) as u8,
            (n & 0x7f) as u8,
        ]
    }

    fn frame(id: &[u8], data: &[u8], v24: bool) -> Vec<u8> {
        let mut out = id.to_vec();
        if v24 {
            out.extend_from_slice(&synchsafe(data.len()));
        } else {
            out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        }
        out.extend_from_slice(&[0, 0]);
        out.extend_from_slice(data);
        out
    }

    fn latin1(text: &str) -> Vec<u8> {
        let mut out = vec![0u8];
        out.extend(text.chars().map(|c| c as u8));
        out
    }

    fn write(scratch: &str, filed_as: &str, major: u8, frames: &[Vec<u8>]) -> std::path::PathBuf {
        let body: Vec<u8> = frames.concat();
        let mut file = b"ID3".to_vec();
        file.extend_from_slice(&[major, 0, 0]);
        file.extend_from_slice(&synchsafe(body.len()));
        file.extend_from_slice(&body);
        file.extend_from_slice(&fixtures::bytes(fixtures::MP3));

        let path = fixtures::scratch(scratch).join(filed_as);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, file).unwrap();
        path
    }

    #[test]
    fn a_halved_utf16_character_does_not_lose_the_file() {
        let mut broken = vec![1u8, 0xFF, 0xFE];
        for unit in "Sonic Prayer".encode_utf16() {
            broken.extend_from_slice(&unit.to_le_bytes());
        }
        broken.pop();

        let path = write(
            "salvage-utf16",
            "Earthless/Rhythms From A Cosmic Sky/01 Sonic Prayer.mp3",
            3,
            &[
                frame(b"TIT2", &broken, false),
                frame(b"TPE1", &latin1("Earthless"), false),
                frame(b"TALB", &latin1("Rhythms From A Cosmic Sky"), false),
            ],
        );

        assert!(Probe::open(&path).unwrap().read().is_err());

        let track = read_metadata(&path).expect("salvaged");
        assert_eq!(track.artist, "Earthless");
        assert_eq!(track.album, "Rhythms From A Cosmic Sky");
        assert_eq!(track.title, "Sonic Praye");
        assert_eq!(track.track_number, Some(1));
        assert!(track.duration_secs > 0.0, "the audio is still measured");
    }

    #[test]
    fn a_frame_too_short_for_its_own_header_does_not_lose_the_file() {
        let mut truncated = b"TXXX".to_vec();
        truncated.extend_from_slice(&synchsafe(4));
        truncated.extend_from_slice(&[0, 0x01]);
        truncated.extend_from_slice(&[0, 0, 0, 4]);

        let path = write(
            "salvage-short",
            "Graupel/Als der Nebel/01 Intro.mp3",
            4,
            &[
                frame(b"TPE1", &latin1("Graupel"), true),
                frame(b"TALB", &latin1("Als der Nebel"), true),
                truncated,
            ],
        );

        assert!(Probe::open(&path).unwrap().read().is_err());

        let track = read_metadata(&path).expect("salvaged");
        assert_eq!(track.artist, "Graupel");
        assert_eq!(track.album, "Als der Nebel");
        assert_eq!(track.title, "Intro");
        assert_eq!(track.track_number, Some(1));
    }

    #[test]
    fn a_half_written_date_does_not_lose_the_file() {
        let path = write(
            "salvage-date",
            "Ash Ra Tempel/The Private Tapes Vol. 3/01 Whoopee.mp3",
            4,
            &[
                frame(b"TDRC", &latin1("2008-"), true),
                frame(b"TIT2", &latin1("Whoopee"), true),
                frame(b"TPE1", &latin1("Ash Ra Tempel"), true),
            ],
        );

        assert!(Probe::open(&path).unwrap().read().is_err());

        let track = read_metadata(&path).expect("salvaged");
        assert_eq!(track.title, "Whoopee");
        assert_eq!(track.artist, "Ash Ra Tempel");
        assert_eq!(track.year, Some(2008));
    }

    #[test]
    fn a_disc_prefix_in_the_filename_is_read_as_a_disc_prefix() {
        let named = |disc, track, title: &str| Named {
            disc,
            track,
            title: title.to_string(),
        };

        assert_eq!(
            split_leading_number("1-11 Animals"),
            named(Some(1), Some(11), "Animals")
        );
        assert_eq!(
            split_leading_number("2-14 Outro"),
            named(Some(2), Some(14), "Outro")
        );
        assert_eq!(
            split_leading_number("04 Magickon"),
            named(None, Some(4), "Magickon")
        );
        assert_eq!(
            split_leading_number("33 000 Honeybees"),
            named(None, Some(33), "000 Honeybees")
        );
        assert_eq!(split_leading_number("Foil"), Named::default());
    }

    #[test]
    fn an_ordinary_file_is_read_the_ordinary_way() {
        let path = write(
            "salvage-ordinary",
            "Autechre/Amber/01 Foil.mp3",
            3,
            &[
                frame(b"TIT2", &latin1("Foil"), false),
                frame(b"TPE1", &latin1("Autechre"), false),
                frame(b"TALB", &latin1("Amber"), false),
            ],
        );

        assert!(Probe::open(&path).unwrap().read().is_ok());
        let track = read_metadata(&path).unwrap();
        assert_eq!(track.title, "Foil");
    }
}

#[cfg(test)]
mod format_tests {
    use super::*;

    fn p(name: &str) -> std::path::PathBuf {
        std::path::PathBuf::from("/music/x").join(name)
    }

    #[test]
    fn the_formats_we_play_are_taken() {
        for name in ["a.mp3", "a.flac", "a.ogg", "a.oga", "a.wav", "a.m4a", "a.M4A"] {
            assert!(is_supported_audio(&p(name)), "{name}");
            assert_eq!(foreign_audio(&p(name)), None, "{name}");
        }
    }

    #[test]
    fn the_formats_we_cannot_play_are_counted() {
        for (name, ext) in [
            ("a.aac", "aac"),
            ("a.AAC", "aac"),
            ("a.m4b", "m4b"),
            ("a.opus", "opus"),
            ("a.wma", "wma"),
            ("a.ape", "ape"),
        ] {
            assert!(!is_supported_audio(&p(name)), "{name}");
            assert_eq!(foreign_audio(&p(name)), Some(ext.to_string()), "{name}");
        }
    }

    #[test]
    fn everything_else_is_neither() {
        for name in ["cover.jpg", "notes.txt", "folder.png", "album.m3u", "no-extension"] {
            assert!(!is_supported_audio(&p(name)), "{name}");
            assert_eq!(foreign_audio(&p(name)), None, "{name}");
        }
    }

    #[test]
    fn the_two_lists_do_not_overlap() {
        for ext in SUPPORTED_EXTENSIONS {
            assert!(
                !FOREIGN_EXTENSIONS.contains(ext),
                "{ext} is in both lists"
            );
        }
    }
}
