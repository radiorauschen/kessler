use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: i64,
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub album_artist: String,
    pub album: String,
    pub genre: String,
    pub year: Option<i32>,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    pub duration_secs: f64,
    pub bitrate: Option<u32>,
    pub sample_rate: Option<u32>,
    pub file_size: u64,
    pub format: String,
    pub rating: u8,
    pub play_count: u32,
    pub last_played: Option<NaiveDateTime>,
    pub date_added: NaiveDateTime,
    pub date_modified: NaiveDateTime,
    pub file_hash: Option<String>,
    pub has_artwork: bool,
}

impl Track {

    pub fn shown_artist(&self) -> &str {
        let named = if self.artist.is_empty() { &self.album_artist } else { &self.artist };
        crate::lang::said(named, self.display_artist())
    }

    pub fn display_artist(&self) -> &str {
        if self.artist.is_empty() {
            if self.album_artist.is_empty() {
                "unbekannt"
            } else {
                &self.album_artist
            }
        } else {
            &self.artist
        }
    }

    pub fn display_title(&self) -> &str {
        if self.title.is_empty() {
            self.path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Unknown")
        } else {
            &self.title
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Album {
    pub name: String,
    pub artist: String,
    pub year: Option<i32>,
    pub added: String,
    pub track_count: u32,
    pub total_duration_secs: f64,
    pub has_artwork: bool,
}

impl Album {
    pub fn display_name(&self) -> &str {
        if self.name.is_empty() {
            "ohne album"
        } else {
            &self.name
        }
    }

    pub fn shown_name(&self) -> &str {
        crate::lang::said(&self.name, self.display_name())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artist {
    pub name: String,
    pub album_count: u32,
    pub track_count: u32,
}

impl Artist {
    pub fn display_name(&self) -> &str {
        if self.name.is_empty() {
            "unbekannt"
        } else {
            &self.name
        }
    }

    pub fn shown_name(&self) -> &str {
        crate::lang::said(&self.name, self.display_name())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playlist {
    pub id: i64,
    pub name: String,
    pub is_smart: bool,
    pub smart_rules: Option<String>,
    pub date_created: NaiveDateTime,
    pub date_modified: NaiveDateTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortField {
    Title,
    Artist,
    Album,
    Year,
    Duration,
    DateAdded,
    PlayCount,
    Rating,
    TrackNumber,
    Genre,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    Ascending,
    Descending,
}

impl SortOrder {
    pub fn toggle(&self) -> Self {
        match self {
            Self::Ascending => Self::Descending,
            Self::Descending => Self::Ascending,
        }
    }

    pub fn sql(&self) -> &'static str {
        match self {
            Self::Ascending => "ASC",
            Self::Descending => "DESC",
        }
    }
}

impl SortField {
    pub fn sql_column(&self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Artist => "artist",
            Self::Album => "album",
            Self::Year => "year",
            Self::Duration => "duration_secs",
            Self::DateAdded => "date_added",
            Self::PlayCount => "play_count",
            Self::Rating => "rating",
            Self::TrackNumber => "track_number",
            Self::Genre => "genre",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatMode {
    Off,
    All,
    One,
}

impl RepeatMode {
    pub fn next(&self) -> Self {
        match self {
            Self::Off => Self::All,
            Self::All => Self::One,
            Self::One => Self::Off,
        }
    }

}
