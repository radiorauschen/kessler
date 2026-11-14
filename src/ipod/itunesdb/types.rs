use std::path::PathBuf;

pub const MAC_EPOCH_OFFSET: i64 = 2_082_844_800;

pub fn mac_to_unix(mac: u32) -> Option<i64> {
    if mac == 0 {
        None
    } else {
        Some(mac as i64 - MAC_EPOCH_OFFSET)
    }
}

pub fn unix_to_mac(unix: Option<i64>) -> u32 {
    match unix {
        Some(t) => (t + MAC_EPOCH_OFFSET).max(0) as u32,
        None => 0,
    }
}

pub const TAG_MHBD: &[u8; 4] = b"mhbd";
pub const TAG_MHSD: &[u8; 4] = b"mhsd";
pub const TAG_MHLT: &[u8; 4] = b"mhlt";
pub const TAG_MHIT: &[u8; 4] = b"mhit";
pub const TAG_MHOD: &[u8; 4] = b"mhod";
pub const TAG_MHLP: &[u8; 4] = b"mhlp";
pub const TAG_MHYP: &[u8; 4] = b"mhyp";
pub const TAG_MHIP: &[u8; 4] = b"mhip";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatasetType {
    Tracks,
    Playlists,
    PodcastPlaylists,
    Albums,
    SmartPlaylists,
    Other(u32),
}

impl DatasetType {
    pub fn from_raw(v: u32) -> Self {
        match v {
            1 => Self::Tracks,
            2 => Self::Playlists,
            3 => Self::PodcastPlaylists,
            4 => Self::Albums,
            5 => Self::SmartPlaylists,
            other => Self::Other(other),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MhodType {
    Title,
    Path,
    Album,
    Artist,
    Genre,
    Filetype,
    Comment,
    Composer,
    Grouping,
    Description,
    AlbumArtist,
    SortArtist,
    SortTitle,
    SortAlbum,
    SortAlbumArtist,
    PlaylistPosition,
    Other(u32),
}

impl MhodType {
    pub fn from_raw(v: u32) -> Self {
        match v {
            1 => Self::Title,
            2 => Self::Path,
            3 => Self::Album,
            4 => Self::Artist,
            5 => Self::Genre,
            6 => Self::Filetype,
            8 => Self::Comment,
            12 => Self::Composer,
            13 => Self::Grouping,
            14 => Self::Description,
            22 => Self::AlbumArtist,
            23 => Self::SortArtist,
            27 => Self::SortTitle,
            28 => Self::SortAlbum,
            29 => Self::SortAlbumArtist,
            100 => Self::PlaylistPosition,
            other => Self::Other(other),
        }
    }

    pub fn as_raw(self) -> u32 {
        match self {
            Self::Title => 1,
            Self::Path => 2,
            Self::Album => 3,
            Self::Artist => 4,
            Self::Genre => 5,
            Self::Filetype => 6,
            Self::Comment => 8,
            Self::Composer => 12,
            Self::Grouping => 13,
            Self::Description => 14,
            Self::AlbumArtist => 22,
            Self::SortArtist => 23,
            Self::SortTitle => 27,
            Self::SortAlbum => 28,
            Self::SortAlbumArtist => 29,
            Self::PlaylistPosition => 100,
            Self::Other(v) => v,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct IpodTrack {
    pub id: u32,
    pub dbid: u64,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub genre: String,
    pub composer: String,
    pub comment: String,
    pub filetype: String,
    pub path: Option<PathBuf>,
    pub track_number: u32,
    pub total_tracks: u32,
    pub disc_number: u32,
    pub total_discs: u32,
    pub year: u32,
    pub length_ms: u32,
    pub size_bytes: u32,
    pub bitrate: u32,
    pub sample_rate: u32,
    pub rating: u8,
    pub play_count: u32,
    pub skip_count: u32,
    pub last_played: Option<i64>,
    pub date_added: Option<i64>,
    pub date_modified: Option<i64>,
    pub compilation: bool,
    pub has_artwork: bool,
    pub artwork_size: u32,
    pub soundcheck: u32,
    pub unchecked: bool,
}

impl IpodTrack {
    pub fn stars(&self) -> u8 {
        self.rating / 20
    }

    pub fn duration_display(&self) -> String {
        let secs = self.length_ms / 1000;
        format!("{}:{:02}", secs / 60, secs % 60)
    }
}

#[derive(Debug, Clone, Default)]
pub struct IpodPlaylist {
    pub id: u64,
    pub name: String,
    pub is_master: bool,
    pub is_podcast: bool,
    pub track_ids: Vec<u32>,
    pub timestamp: Option<i64>,
    pub indexed: Vec<u32>,
    pub jumps: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ItunesDb {
    pub version: u32,
    pub db_id: u64,
    pub tracks: Vec<IpodTrack>,
    pub playlists: Vec<IpodPlaylist>,
}

impl ItunesDb {
    pub fn master_playlist(&self) -> Option<&IpodPlaylist> {
        self.playlists.iter().find(|p| p.is_master)
    }

    pub fn user_playlists(&self) -> impl Iterator<Item = &IpodPlaylist> {
        self.playlists.iter().filter(|p| !p.is_master)
    }

    pub fn total_bytes(&self) -> u64 {
        self.tracks.iter().map(|t| t.size_bytes as u64).sum()
    }

    #[cfg(test)]
    pub fn track_by_id(&self, id: u32) -> Option<&IpodTrack> {
        self.tracks.iter().find(|t| t.id == id)
    }
}
