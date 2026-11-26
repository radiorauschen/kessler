//! Data types for the iPod `iTunesDB` file.
//!
//! Field offsets and semantics were taken from libgpod's `itdb_itunesdb.c`
//! (the reference implementation) rather than from prose documentation —
//! several widely-copied web tables get the timestamp epoch and the string
//! payload layout wrong.

use std::path::PathBuf;

/// Seconds between the Mac epoch (1904-01-01) and the Unix epoch (1970-01-01).
///
/// iTunesDB timestamps count from 1904. Treating them as Unix time — as more
/// than one format write-up suggests — puts every track in 2036.
pub const MAC_EPOCH_OFFSET: i64 = 2_082_844_800;

/// Convert an iTunesDB timestamp to a Unix timestamp. Zero means "never".
pub fn mac_to_unix(mac: u32) -> Option<i64> {
    if mac == 0 {
        None
    } else {
        Some(mac as i64 - MAC_EPOCH_OFFSET)
    }
}

/// Convert a Unix timestamp to an iTunesDB timestamp.
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

/// `mhsd` dataset types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatasetType {
    Tracks,
    /// Standard playlists.
    Playlists,
    /// Playlists with the special podcast playlist.
    PodcastPlaylists,
    Albums,
    /// Newer playlist section (iTunes 8+).
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

/// `mhod` payload types we care about. The full list is much longer; anything
/// not named here is skipped rather than guessed at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MhodType {
    Title,
    /// File path on the iPod, colon-separated.
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
    /// Position of a track inside a playlist (found inside `mhip`).
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

/// One track as stored on the iPod.
#[derive(Debug, Clone, Default)]
pub struct IpodTrack {
    /// iPod-internal track id, referenced by playlist entries.
    pub id: u32,
    /// Persistent 64-bit id, stable across syncs.
    pub dbid: u64,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub genre: String,
    pub composer: String,
    pub comment: String,
    /// Format marker as stored ("MPEG audio file", "AAC audio file", ...).
    pub filetype: String,
    /// Path relative to the iPod root, e.g. `iPod_Control/Music/F00/ABCD.mp3`.
    pub path: Option<PathBuf>,
    pub track_number: u32,
    pub total_tracks: u32,
    pub disc_number: u32,
    pub total_discs: u32,
    pub year: u32,
    /// Length in milliseconds.
    pub length_ms: u32,
    pub size_bytes: u32,
    pub bitrate: u32,
    pub sample_rate: u32,
    /// 0–100 in steps of 20 (one star = 20).
    pub rating: u8,
    pub play_count: u32,
    pub skip_count: u32,
    /// Unix timestamps, `None` when the iPod stored zero.
    pub last_played: Option<i64>,
    pub date_added: Option<i64>,
    pub date_modified: Option<i64>,
    pub compilation: bool,
    pub has_artwork: bool,
    pub artwork_size: u32,
    /// Sound Check: how much the iPod turns the track down or up when its
    /// own Sound Check is on — 1000 × 10^(−gain/10), 0 for not known.
    pub soundcheck: u32,
    /// Not ticked: the byte at mhit+0x78 is 1, as iTunes and libgpod mean it
    /// (older kessler wrote 1 meaning „ticked"), and older iPods pass over an unticked track when they play on. Read so
    /// a sync can write such a database again — see `SyncPlan::reorder`.
    pub unchecked: bool,
}

impl IpodTrack {
    /// Rating in whole stars (0–5).
    pub fn stars(&self) -> u8 {
        self.rating / 20
    }

    /// Length as `M:SS`.
    pub fn duration_display(&self) -> String {
        let secs = self.length_ms / 1000;
        format!("{}:{:02}", secs / 60, secs % 60)
    }
}

/// One playlist as stored on the iPod.
#[derive(Debug, Clone, Default)]
pub struct IpodPlaylist {
    pub id: u64,
    pub name: String,
    /// The master playlist is the full library; every iPod has exactly one.
    pub is_master: bool,
    pub is_podcast: bool,
    /// Track ids in playlist order, referencing [`IpodTrack::id`].
    pub track_ids: Vec<u32>,
    pub timestamp: Option<i64>,
    /// The fields its library indices (`mhod` 52) order by, as read; the
    /// writer makes its own for the master playlist and ignores these.
    pub indexed: Vec<u32>,
    /// How many jump tables (`mhod` 53) stand beside them, as read: older
    /// kessler wrote one per index, Apple writes none.
    pub jumps: usize,
}

/// A parsed iTunesDB.
#[derive(Debug, Clone, Default)]
pub struct ItunesDb {
    /// Database format version from the `mhbd` header.
    pub version: u32,
    pub db_id: u64,
    /// The language and the library's own number iTunes writes into the
    /// header (at 0x46 and 0x48), kept as they were found; zero where
    /// nothing wrote them.
    pub language: [u8; 2],
    pub library_id: u64,
    pub tracks: Vec<IpodTrack>,
    pub playlists: Vec<IpodPlaylist>,
}

impl ItunesDb {
    pub fn master_playlist(&self) -> Option<&IpodPlaylist> {
        self.playlists.iter().find(|p| p.is_master)
    }

    /// Playlists the user actually made, i.e. everything but the master list.
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
