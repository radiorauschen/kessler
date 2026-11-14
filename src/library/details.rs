use std::path::Path;

use crate::db::models::Track;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    Section(String),
    Field { label: String, value: String },
}

impl Line {
    fn field(label: impl Into<String>, value: impl Into<String>) -> Line {
        Line::Field {
            label: label.into(),
            value: value.into(),
        }
    }
}

pub fn read(track: &Track) -> Vec<Line> {
    let mut lines = Vec::new();

    lines.push(Line::Section(tr!("datei").into()));
    let path = track.path.as_path();
    lines.push(Line::field(
        "name",
        path.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "—".into()),
    ));
    lines.push(Line::field(
        tr!("ordner"),
        path.parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| "—".into()),
    ));
    match std::fs::metadata(path) {
        Ok(meta) => lines.push(Line::field(tr!("grösse"), crate::ipod::format_bytes(meta.len()))),
        Err(_) => lines.push(Line::field(tr!("grösse"), tr!("datei nicht gefunden"))),
    }

    lines.push(Line::Section(tr!("klang").into()));
    lines.push(Line::field("format", &track.format));
    lines.push(Line::field(
        tr!("dauer"),
        crate::gui::text::duration(track.duration_secs.max(0.0) as u64),
    ));
    if let Some(bitrate) = track.bitrate {
        lines.push(Line::field("bitrate", format!("{bitrate} kbit/s")));
    }
    if let Some(rate) = track.sample_rate {
        lines.push(Line::field(
            tr!("abtastrate"),
            format!("{:.1} khz", rate as f64 / 1000.0),
        ));
    }

    let tags = tags_of(path);
    if !tags.is_empty() {
        lines.push(Line::Section("tags".into()));
        lines.extend(tags.into_iter().map(|(k, v)| Line::field(k, v)));
    }

    lines.push(Line::Section(tr!("bibliothek").into()));
    lines.push(Line::field(
        tr!("hinzugefügt"),
        track.date_added.format("%d.%m.%Y").to_string(),
    ));
    lines.push(Line::field(
        tr!("gespielt"),
        match track.play_count {
            0 => tr!("noch nie").to_string(),
            1 => tr!("1 mal").to_string(),
            n => tr!("{} mal", n),
        },
    ));
    if let Some(last) = track.last_played {
        lines.push(Line::field(
            tr!("zuletzt|gespielt"),
            last.format("%d.%m.%Y %H:%M").to_string(),
        ));
    }
    if track.rating > 0 {
        lines.push(Line::field(tr!("wertung"), "★".repeat(track.rating as usize)));
    }

    lines
}

fn tags_of(path: &Path) -> Vec<(String, String)> {
    use lofty::file::TaggedFileExt;
    use lofty::tag::ItemValue;

    let Ok(file) = lofty::probe::Probe::open(path).and_then(|p| p.read()) else {
        return Vec::new();
    };

    let mut out: Vec<(String, String)> = Vec::new();
    for tag in file.tags() {
        for item in tag.items() {
            let value = match item.value() {
                ItemValue::Text(text) | ItemValue::Locator(text) => text.trim().to_string(),
                ItemValue::Binary(bytes) => format!("{} bytes", bytes.len()),
            };
            if value.is_empty() {
                continue;
            }
            let label = label_for(item.key());
            if out.iter().any(|(l, _)| *l == label) {
                continue;
            }
            out.push((label, value));
        }
    }
    out
}

fn label_for(key: &lofty::prelude::ItemKey) -> String {
    use lofty::prelude::ItemKey;
    let known = match key {
        ItemKey::TrackTitle => tr!("titel|name"),
        ItemKey::TrackArtist => tr!("künstler:in"),
        ItemKey::AlbumTitle => "album",
        ItemKey::AlbumArtist => tr!("album-künstler:in"),
        ItemKey::Genre => "genre",
        ItemKey::Year | ItemKey::RecordingDate => tr!("jahr"),
        ItemKey::TrackNumber => tr!("nr"),
        ItemKey::TrackTotal => tr!("von"),
        ItemKey::DiscNumber => tr!("cd"),
        ItemKey::DiscTotal => tr!("von cd"),
        ItemKey::Composer => tr!("komponist:in"),
        ItemKey::Comment => tr!("kommentar"),
        ItemKey::Publisher => "label",
        ItemKey::CatalogNumber => tr!("katalognummer"),
        ItemKey::EncoderSoftware => tr!("kodiert mit"),
        ItemKey::Bpm | ItemKey::IntegerBpm => "bpm",
        other => return format!("{other:?}").to_lowercase(),
    };
    known.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_track() -> Track {
        Track {
            id: 1,
            path: std::path::PathBuf::from("/musik/autechre/amber/01 foil.mp3"),
            title: "foil".into(),
            artist: "autechre".into(),
            album_artist: "autechre".into(),
            album: "amber".into(),
            genre: "electronic".into(),
            year: Some(1994),
            track_number: Some(1),
            disc_number: Some(1),
            duration_secs: 245.0,
            bitrate: Some(320),
            sample_rate: Some(44100),
            file_size: 9_800_000,
            format: "mp3".into(),
            rating: 0,
            play_count: 3,
            last_played: None,
            date_added: chrono::NaiveDateTime::default(),
            date_modified: chrono::NaiveDateTime::default(),
            file_hash: None,
            has_artwork: false,
        }
    }

    fn value(lines: &[Line], label: &str) -> Option<String> {
        lines.iter().find_map(|line| match line {
            Line::Field { label: l, value } if l == label => Some(value.clone()),
            _ => None,
        })
    }

    #[test]
    fn it_says_where_the_file_is() {
        let lines = read(&a_track());
        assert_eq!(value(&lines, "name").as_deref(), Some("01 foil.mp3"));
        assert_eq!(
            value(&lines, "ordner").as_deref(),
            Some("/musik/autechre/amber")
        );
    }

    #[test]
    fn a_missing_file_is_not_covered_up() {
        let lines = read(&a_track());
        assert_eq!(
            value(&lines, "grösse").as_deref(),
            Some("datei nicht gefunden")
        );
    }

    #[test]
    fn the_sound_is_read_out_in_units() {
        let lines = read(&a_track());
        assert_eq!(value(&lines, "format").as_deref(), Some("mp3"));
        assert_eq!(value(&lines, "dauer").as_deref(), Some("4:05"));
        assert_eq!(value(&lines, "bitrate").as_deref(), Some("320 kbit/s"));
        assert_eq!(value(&lines, "abtastrate").as_deref(), Some("44.1 khz"));
    }

    #[test]
    fn a_track_nobody_has_played_says_so_in_words() {
        let mut track = a_track();
        track.play_count = 0;
        assert_eq!(
            value(&read(&track), "gespielt").as_deref(),
            Some("noch nie")
        );
        track.play_count = 1;
        assert_eq!(value(&read(&track), "gespielt").as_deref(), Some("1 mal"));
        track.play_count = 9;
        assert_eq!(value(&read(&track), "gespielt").as_deref(), Some("9 mal"));
    }

    #[test]
    fn no_section_stands_empty() {
        let lines = read(&a_track());
        for (i, line) in lines.iter().enumerate() {
            if let Line::Section(name) = line {
                assert!(
                    matches!(lines.get(i + 1), Some(Line::Field { .. })),
                    "the section {name} has nothing under it"
                );
            }
        }
    }

    #[test]
    fn the_tags_come_out_of_the_file() {
        use crate::library::fixtures;
        let path = fixtures::scratch("details").join("probe.mp3");
        fixtures::place(&path, fixtures::MP3);

        let mut track = a_track();
        track.title = "was die datenbank glaubt".into();
        track.path = path.clone();
        let lines = read(&track);

        assert!(
            std::fs::metadata(&path).is_ok(),
            "the fixture is really on disk"
        );
        assert_ne!(
            value(&lines, "titel").as_deref(),
            Some("was die datenbank glaubt"),
            "the panel reported the database's title instead of the file's"
        );
        let _ = std::fs::remove_file(&path);
    }
}
