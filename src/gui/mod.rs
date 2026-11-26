//! The window frontend.
//!
//! A second frontend over the same core: `db`, `library`, `player` and `ipod`
//! know nothing about either the terminal or this. What lives here is only the
//! drawing and the input handling.
//!
//! The design rules it obeys are in [`theme`] — two type sizes, two weights,
//! three tonal values, lowercase throughout, and no lines except the progress
//! rule.

mod app;
pub mod appicon;
pub mod artist;
pub mod attached;
pub mod shelf;
pub mod sound;
#[cfg(test)]
pub mod changes;
pub mod columns;
pub mod device;
pub mod dither;
pub mod coverpick;
pub mod covers;
pub mod folder;
pub mod glyphs;
pub mod lastfmkeys;
pub mod levelrun;
pub mod library;
pub mod listened;
pub mod log;
pub mod macwindow;
pub mod lyrics;
pub mod menu;
pub mod measure;
pub mod menubar;
#[cfg(target_os = "linux")]
mod portal;
#[cfg(target_os = "macos")]
mod macaccent;
pub mod reveal;
pub mod saver;
pub mod scope;
pub mod tagedit;
pub mod text;
pub mod theme;
pub mod update;

use std::sync::mpsc;
use std::time::Instant;

use eframe::egui;

use crate::config::Config;
use crate::db::models::{Album, Artist, SortField, SortOrder, Track};
use crate::db::Database;
use crate::error::Result;
use crate::library::LibraryManager;
use crate::player::Player;

/// Time one step of putting music on, and say how long it took.
///
/// Playing something has parts that can each be slow for reasons of their own
/// — asking the library what the tracks are, handing them to the queue,
/// opening the file — and a pause cannot be argued with until they are told
/// apart. Silent unless somebody asks: `KESSLER_LOG=kessler=debug`
/// (or `-vv`), and then the log says which part it was.
pub(crate) fn timed<T>(what: &str, work: impl FnOnce() -> T) -> T {
    if !tracing::enabled!(tracing::Level::DEBUG) {
        return work();
    }
    let began = Instant::now();
    let done = work();
    tracing::debug!("{what}: {:.1?}", began.elapsed());
    done
}

/// How long a message stays on the bottom line: what you just did, and what
/// happened by itself.
const STATUS_SECS: u64 = 4;
const NOTICE_SECS: u64 = 12;

/// How far one press of the seek key jumps.
pub const SEEK_STEP_SECS: f64 = 10.0;
/// How far `,` and `.` seek: a minute, for recordings an hour long.
pub const SEEK_MINUTE_SECS: f64 = 60.0;
/// How soon the second „q" has to follow the first to quit.
pub const QUIT_WINDOW_SECS: f64 = 2.0;

/// How long a first `g` waits for the second of `gg`, in seconds.
const GG_WINDOW_SECS: f64 = 1.0;

/// What a finished background scan reports back.
pub struct ScanOutcome {
    pub total: u32,
    pub added: u32,
    pub updated: u32,
    /// Tracks whose files had gone, taken out of the library.
    pub removed: u32,
    pub error: Option<String>,
}

use theme::{Brightness, FontSource, Palette, Theme};

/// What one row of the first column stands for.
///
/// Above the names sit rows that are not names: "alle", which is the whole
/// library, and "zuletzt", which is whatever arrived last. The lists have the
/// fourth column, beside the queue they are the other kind of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtistRow {
    /// Every artist — what the window opens on.
    All,
    /// The newest arrivals, whoever they are by.
    Recent,
    /// Every compilation in the library, as records, as iTunes has them — see
    /// `GuiApp::compilation_records`.
    Compilations,
    /// The artist at this position of `artists`.
    Named(usize),
}

/// The rows above the names, in the order they are drawn.
///
/// Fixed, and not a function of the settings: turning the "alle" row off hides
/// its drawing, but index zero still *means* "every artist" — it is where the
/// window opens. The place is kept and only the line is skipped.
///
/// A column whose indices shifted would need every caller to know why, which
/// is the arithmetic this type exists to end. What does change is which rows
/// are *drawn*, and the keyboard steps over the ones that are not — see
/// [`GuiApp::drawn`].
///
/// The iPod was briefly a third row here. It is a shelf, not a heading: which
/// shelf you are reading is one question and which of its artists you are
/// looking at is another, and answering both in one column meant the first
/// column's top row quietly changed what the other two columns were made of.
/// It moved to the masthead, where switching it switches the whole window.
pub const PINNED: [ArtistRow; 3] = [ArtistRow::All, ArtistRow::Recent, ArtistRow::Compilations];

impl ArtistRow {
    /// What sits at this index of the column: the pinned rows, then the names.
    pub fn at(index: usize) -> ArtistRow {
        match PINNED.get(index) {
            Some(row) => *row,
            None => ArtistRow::Named(index - PINNED.len()),
        }
    }

    /// Where a row is drawn — the inverse of [`ArtistRow::at`].
    pub fn index(self) -> usize {
        match self {
            ArtistRow::Named(at) => at + PINNED.len(),
            other => PINNED
                .iter()
                .position(|row| *row == other)
                .unwrap_or_default(),
        }
    }
}

/// What one row of the album column stands for.
///
/// The column is not the list. „alle" stands at the top of it, so row 1 is
/// album 0, and the two numbers must never be mistaken for one another. The
/// first column has had [`ArtistRow`] for exactly this reason from the
/// beginning; the album column had the arithmetic written out at each call
/// site instead — and the day one call site forgot it, every record played the
/// one above it and the last record in the column played nothing at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlbumRow {
    /// Not a record: everything the track column is showing.
    All,
    /// The record at this place in [`GuiApp::albums`].
    Named(usize),
}

impl AlbumRow {
    /// What sits at this row of the column.
    pub fn at(row: usize) -> AlbumRow {
        match row.checked_sub(1) {
            None => AlbumRow::All,
            Some(at) => AlbumRow::Named(at),
        }
    }

    /// Where it is drawn — the inverse of [`AlbumRow::at`].
    pub fn row(self) -> usize {
        match self {
            AlbumRow::All => 0,
            AlbumRow::Named(at) => at + 1,
        }
    }
}

#[cfg(test)]
mod album_rows {
    use super::AlbumRow;

    /// The bug this guards was on screen: a double-click on a record played
    /// the one above it, and the last record in the column played nothing,
    /// because a row number was handed to a list that has no „alle" in it.
    #[test]
    fn a_row_is_not_a_place_in_the_list() {
        assert_eq!(AlbumRow::at(0), AlbumRow::All);
        assert_eq!(AlbumRow::at(1), AlbumRow::Named(0));
        assert_eq!(AlbumRow::at(7), AlbumRow::Named(6));

        assert_eq!(AlbumRow::All.row(), 0);
        assert_eq!(AlbumRow::Named(0).row(), 1);

        for row in 0..6 {
            assert_eq!(AlbumRow::at(row).row(), row);
        }
    }
}

/// What the window frame should be, given the scheme and what it came out as.
///
/// Kept apart from the sending so it can be checked without a window: the
/// wrong answer here is invisible in a screenshot taken on a machine with no
/// title bar, and shows up only on somebody's Mac.
pub fn frame_theme_for(brightness: Brightness, palette: &Palette) -> egui::SystemTheme {
    match (brightness == Brightness::System, palette.is_dark()) {
        (true, _) => egui::SystemTheme::SystemDefault,
        (false, true) => egui::SystemTheme::Dark,
        (false, false) => egui::SystemTheme::Light,
    }
}

/// Which album of the list a record is, by both of its keys.
///
/// Both, because a title is not an identity: the grid holds every artist's
/// records at once, so "greatest hits" on its own lands on whoever comes first
/// alphabetically, and every untagged track in the library shares the same
/// empty title. That is how clicking the line that says what is playing landed
/// on a stranger's cover.
///
/// Falls back to the title alone, because a record whose album artist is
/// spelled one way in the tag and another in the list is still that record, and
/// landing on the title beats landing nowhere.
pub fn album_row_for(albums: &[Album], album_key: &str, artist_key: &str) -> Option<usize> {
    albums
        .iter()
        .position(|a| {
            crate::db::sort_key(&a.name) == album_key
                && crate::db::sort_key(&a.artist) == artist_key
        })
        .or_else(|| {
            albums
                .iter()
                .position(|a| crate::db::sort_key(&a.name) == album_key)
        })
}

/// "412 dateien übergangen: m4a, aiff" — or nothing, when there were none.
///
/// Two formats named at most: the point is which one accounts for the
/// difference, and a list of nine is a list nobody finishes reading.
pub fn foreign_note(counted: &[(String, u32)]) -> Option<String> {
    if counted.is_empty() {
        return None;
    }
    let all: u32 = counted.iter().map(|(_, n)| n).sum();
    let named: Vec<&str> = counted.iter().take(2).map(|(ext, _)| ext.as_str()).collect();
    let more = if counted.len() > named.len() {
        tr!(" und {} weitere", counted.len() - named.len())
    } else {
        String::new()
    };
    Some(tr!("{} dateien übergangen: {}{}",
        all,
        named.join(", "),
        more
    ))
}

/// What is being carried, while a hand is carrying it.
///
/// Two kinds, and they end differently: a row of the queue moves inside it,
/// and anything from the shelf joins it. Holding the tracks themselves rather
/// than a reference to where they came from is what lets the source column
/// scroll, reload or change underneath a drag without the drop landing on
/// something else.
#[derive(Clone)]
pub enum Dragged {
    /// One row of the queue, on its way to another place in it.
    Queued(usize),
    /// Tracks from the shelf, on their way in.
    Incoming(Vec<Track>),
    /// Episodes from the podcasts' list, on their way into their queue.
    Episodes(Vec<(crate::podcast::Episode, crate::podcast::Show)>),
    /// One row of the podcasts' queue, on its way to another place in it.
    QueuedEpisode(usize),
}

impl Dragged {
    /// How many tracks this is, for the mark that follows the pointer.
    pub fn count(&self) -> usize {
        match self {
            Self::Queued(_) | Self::QueuedEpisode(_) => 1,
            Self::Incoming(tracks) => tracks.len(),
            Self::Episodes(episodes) => episodes.len(),
        }
    }
}

/// One track, read out in full, as the panel shows it.
pub struct Details {
    /// What the panel is headed with — the title of the track it is about.
    pub title: String,
    pub lines: Vec<crate::library::details::Line>,
}

/// Which shelf the three columns are reading from.
///
/// Not the same question as which row is selected. Clicking "ipod" chooses the
/// device; clicking a name *below* it chooses one of the device's artists and
/// must not quietly send you back to the library. So the source is kept apart
/// from the cursor, and only the pinned rows change it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Source {
    #[default]
    Library,
    Device,
}

/// Which row of the first column stands for this artist.
///
/// Falls back to "alle" when the library has no such name — under a filter, or
/// in the middle of a rescan, the artist of the track being played may simply
/// not be in the column, and the album below is still worth finding.
///
/// A function rather than three lines where it is used, because those three
/// lines were wrong: they added one to the position, which was right while
/// "alle" was the only row above the names and became an off-by-one the day
/// "zuletzt" joined it. The window then opened the artist *before* the one
/// playing, which from the outside looks like it opens a random one.
pub fn artist_row_for(names: &[&str], artist_key: &str) -> ArtistRow {
    match names
        .iter()
        .position(|name| crate::db::sort_key(name) == artist_key)
    {
        Some(at) => ArtistRow::Named(at),
        None => ArtistRow::All,
    }
}

/// The track list's order as the config file keeps it: the field's own
/// column name and the direction, „year desc".
/// How many artists make a record without an album artist a compilation —
/// see `GuiApp::widen_compilations`. Two is a duet or a split single.
pub const COMPILATION_ARTISTS: usize = 3;

/// A record's own order: by disc, then by number, then by title.
fn running_order(record: &mut [Track]) {
    record.sort_by(|a, b| {
        (a.disc_number.unwrap_or(0), a.track_number.unwrap_or(u32::MAX), a.title.as_str())
            .cmp(&(b.disc_number.unwrap_or(0), b.track_number.unwrap_or(u32::MAX), b.title.as_str()))
    });
}

pub fn track_order_key(field: SortField, order: SortOrder) -> String {
    let direction = match order {
        SortOrder::Ascending => "asc",
        SortOrder::Descending => "desc",
    };
    format!("{} {direction}", field.sql_column())
}

/// And back. `None` for anything it does not recognise.
pub fn track_order_from_key(key: &str) -> Option<(SortField, SortOrder)> {
    let mut words = key.split_whitespace();
    let column = words.next()?;
    let order = match words.next() {
        Some("desc") => SortOrder::Descending,
        _ => SortOrder::Ascending,
    };
    let field = [
        SortField::Title,
        SortField::Artist,
        SortField::Album,
        SortField::Year,
        SortField::Duration,
        SortField::DateAdded,
        SortField::PlayCount,
        SortField::Rating,
        SortField::TrackNumber,
        SortField::Genre,
        SortField::Bitrate,
    ]
    .into_iter()
    .find(|f| f.sql_column() == column)?;
    Some((field, order))
}

/// A run through the albums that lack a year or a genre, asking MusicBrainz
/// for each.
///
/// The asking happens on a thread of its own, a second apart as MusicBrainz
/// asks; the writing happens here, one record at a time as the answers come
/// back, because the files and the library belong to the window. One run at
/// a time, whichever it is: two would ask MusicBrainz twice as often as it
/// allows.
pub struct TagHunt {
    pub what: Hunted,
    news: mpsc::Receiver<HuntNews>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub total: usize,
    pub asked: usize,
    pub found: usize,
    /// Tracks written since the columns were last read again.
    unshown: usize,
}

/// What a run is looking for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hunted {
    Years,
    Genres,
}

impl Hunted {
    /// The word the status line and the menu use for it.
    pub fn word(self) -> &'static str {
        match self {
            Self::Years => "jahre",
            Self::Genres => "genres",
        }
    }
}

/// What was found for one record.
enum Found {
    Year(i32),
    Genre(String),
}

/// What the thread says.
enum HuntNews {
    /// One more record asked about, and nothing to write.
    Asked,
    /// One more asked, and this is what it has.
    Found {
        artist: String,
        album: String,
        found: Found,
    },
    /// The network is not answering; the run ends here.
    Failed(String),
}

/// Which column has the keyboard.
///
/// They read left to right as a sentence: whose records, which record, which
/// track, and — when it is open — what is going to be played. The queue is the
/// only one that is not narrowed by the one to its left; it is where things
/// end up rather than another way of looking at the shelf.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    /// The lists' names, at the top of the fourth column while the lists
    /// have it. The list open under them is `Queue`, as the queue is — it is
    /// the same kind of thing, a running order to walk and rearrange.
    Lists,
    Artists,
    Albums,
    Tracks,
    Queue,
}

impl Column {
    pub fn left(self) -> Self {
        match self {
            Self::Artists => Self::Artists,
            Self::Albums => Self::Artists,
            Self::Lists => Self::Tracks,
            Self::Tracks => Self::Albums,
            Self::Queue => Self::Tracks,
        }
    }

    pub fn right(self) -> Self {
        match self {
            Self::Lists => Self::Lists,
            Self::Artists => Self::Albums,
            Self::Albums => Self::Tracks,
            Self::Tracks => Self::Queue,
            Self::Queue => Self::Queue,
        }
    }
}

/// The three things the masthead's switch goes between, each with a side
/// of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Music,
    Radio,
    Podcasts,
    /// The iPod's names in the columns.
    Ipod,
}

/// What stands in the side, as a view left it: each column asked for, on or
/// off. The iPod's column is the iPod's, not a view's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Sides {
    pub artist: bool,
    pub log: bool,
    pub lyrics: bool,
    pub others: bool,
    pub eq: bool,
}

/// What the fourth column is holding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fourth {
    /// The queue — what it holds by default.
    Queue,
    /// The lists, with the open one under them. They take turns with the
    /// queue: `u` and `w`.
    Lists,
    /// The device, while its panel has borrowed the column.
    Device,
    /// The log, while it is open: `t`, or a click on the status line.
    Log,
    /// What is known about the artist being looked at: `i`.
    Artist,
    /// The words of the song: `y`.
    Lyrics,
    /// Who the library's artists keep naming and who is missing: `o`.
    Others,
    /// The equalizer: `z`.
    Equalizer,
}

impl Fourth {
    /// What can stand in the side, in the order the columns stand in when
    /// all of them are open (`[ui] all_columns`).
    pub const SIDES: [Fourth; 6] = [Fourth::Artist, Fourth::Lyrics, Fourth::Others, Fourth::Equalizer, Fourth::Device, Fourth::Log];

    /// A name for its own column's place in egui's memory.
    pub fn key(self) -> &'static str {
        match self {
            Fourth::Queue => "queue",
            Fourth::Lists => "lists",
            Fourth::Device => "device",
            Fourth::Log => "log",
            Fourth::Artist => "artist",
            Fourth::Lyrics => "lyrics",
            Fourth::Others => "others",
            Fourth::Equalizer => "equalizer",
        }
    }
}

/// How a name column runs.
///
/// Alphabetical either way, plus a third order that is worth a click of its
/// own: newest first, which is the only way to see what has just come in.
/// Artists and albums both offer all three, so they share one answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ListSort {
    #[default]
    Ascending,
    Descending,
    /// By the day the files arrived, newest first.
    Added,
    /// By the year on the record, oldest first — the album column only, where
    /// the year is a column of its own and can hold the order itself.
    Year,
    YearBack,
}

impl ListSort {
    /// The next order the heading offers. Three states on one label, because a
    /// second control for the same question would be one control too many.
    pub fn next(self) -> Self {
        match self {
            Self::Ascending => Self::Descending,
            Self::Descending => Self::Added,
            Self::Added => Self::Ascending,
            Self::Year | Self::YearBack => Self::Ascending,
        }
    }

    /// What the config file calls it.
    pub fn key(self) -> &'static str {
        match self {
            Self::Ascending => "a-z",
            Self::Descending => "z-a",
            Self::Added => "zuletzt",
            Self::Year => "jahr",
            Self::YearBack => "jahr rückwärts",
        }
    }

    /// Read one back. Anything unknown — including every config written
    /// before the order was kept — is the shelf's own order. `years` says
    /// whether the column has a year to be ordered by.
    pub fn from_key(key: &str, years: bool) -> Self {
        match key.trim() {
            "z-a" => Self::Descending,
            "zuletzt" => Self::Added,
            "jahr" if years => Self::Year,
            "jahr rückwärts" if years => Self::YearBack,
            _ => Self::Ascending,
        }
    }

    /// The other way round, for the head that holds the year.
    pub fn other_year(self) -> Self {
        match self {
            Self::Year => Self::YearBack,
            _ => Self::Year,
        }
    }

    /// Whether the year is what the list is in.
    pub fn is_year(self) -> bool {
        matches!(self, Self::Year | Self::YearBack)
    }

    /// What the column heading shows beside the word "alben".
    pub fn mark(self) -> &'static str {
        match self {
            Self::Ascending => "↓",
            Self::Descending => "↑",
            Self::Added => tr!("zuletzt"),
            Self::Year | Self::YearBack => "",
        }
    }
}

/// The little menu a right-click opens, and what it is about.
///
/// One menu rather than two: the entries differ, but the sheet, the way it
/// closes and the place it appears are the same thing, and two of those would
/// drift apart.
pub struct RowMenu {
    pub at: egui::Pos2,
    pub about: About,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum About {
    /// The selected tracks.
    Tracks,
    /// One album, by its place in `albums`.
    Album(usize),
    /// One row of the queue, by its place in it.
    Queued(usize),
    /// One row of the list open in the fourth column, by its place in it.
    Listed(usize),
    /// One list, by its place in `lists`.
    List(usize),
    /// One name in the first column, by its place in `artists`.
    Artist(usize),
    /// One station in the radio's column, by its place among those listed.
    Station(usize),
    /// The picture of the station under „sender" — the one on the air, else
    /// the one under the keyboard.
    StationPicture,
    /// One episode in the podcasts' second column, by its place in the list.
    Episode(usize),
    /// One podcast in the podcasts' first column, by its place among the
    /// podcasts it lists (see `GuiApp::podcasts_listed`).
    Podcast(usize),
}

/// The picked name, the cursor's row and the playing track's id.
pub type ArtistAnchor = (Option<String>, usize, Option<i64>);

/// How many rows the first column has: the pinned two, then the names.
///
/// Written once because two things read it and they must not disagree —
/// [`ArtistRow::at`], which says what is *at* a row, and
/// [`GuiApp::column_len`], from which the keyboard learns where the column
/// ends.
pub fn artist_rows(names: usize) -> usize {
    names + PINNED.len()
}

/// The records none of whose tracks says a genre, as (artist, album) —
/// named the way the album column names them, so the tracks can be found
/// again by those two to be written. Records without a title are left out:
/// there is nothing to ask about.
fn records_without_genre(tracks: &[Track]) -> Vec<(String, String)> {
    let mut records: Vec<(String, String, bool)> = Vec::new();
    let mut places: std::collections::HashMap<(String, String), usize> =
        std::collections::HashMap::new();
    for track in tracks {
        if track.album.trim().is_empty() {
            continue;
        }
        let artist = crate::db::grouping_artist(track);
        let key = (crate::db::sort_key(artist), crate::db::sort_key(&track.album));
        let has = !track.genre.trim().is_empty();
        match places.get(&key) {
            Some(&at) => records[at].2 |= has,
            None => {
                places.insert(key, records.len());
                records.push((artist.to_string(), track.album.clone(), has));
            }
        }
    }
    records
        .into_iter()
        .filter(|(_, _, has)| !has)
        .map(|(artist, album, _)| (artist, album))
        .collect()
}

/// Which list `shift + u` goes to next, given where the lists are standing.
///
/// From anywhere else, the first — the key's first press is, and that means the
/// top of them. From a list, the one after it, and round from the last to the
/// first, so holding the key down walks the whole set rather than sticking at
/// the bottom.
fn list_after(standing: Option<usize>, lists: usize) -> Option<usize> {
    if lists == 0 {
        return None;
    }
    Some(match standing {
        Some(at) => (at + 1) % lists,
        None => 0,
    })
}

/// Where a row lifted from `from` and dropped at `at` actually lands.
///
/// The gap a row is dropped into is counted while the row is still in the
/// list, so dragging one *downwards* has to allow for the hole it leaves
/// behind it — without this every such drag lands one place too far, which is
/// the off-by-one every list anybody has ever written has had once.
fn landed(from: usize, at: usize) -> usize {
    if at > from {
        at - 1
    } else {
        at
    }
}

/// What to call a list made out of these tracks.
///
/// What they have in common, when they have anything: one artist, one record.
/// A name somebody will recognise beats „liste 3", and it is a suggestion —
/// the field opens with it selected in the sense that typing replaces nothing,
/// which is the one thing a suggestion must not do to a name somebody means.
fn suggest_list_name(tracks: &[Track]) -> String {
    let same = |of: fn(&Track) -> &str| -> Option<String> {
        let first = of(tracks.first()?);
        tracks
            .iter()
            .all(|t| of(t) == first)
            .then(|| first.to_string())
            .filter(|name| !name.trim().is_empty())
    };
    if let Some(album) = same(|t| t.album.as_str()) {
        return album;
    }
    if let Some(artist) = same(|t| crate::db::grouping_artist(t)) {
        return artist;
    }
    chrono::Local::now().format("%-d.%-m.%Y").to_string()
}

/// A question the window is waiting for an answer to, about a list.
///
/// One panel, three questions, because they are one act: a list is being
/// made, renamed, or thrown away, and all three need the same thing — the
/// column, a name, and a word to press. Making and renaming want a name
/// typed; throwing away wants only the word, and gets the name to read.
pub struct Prompt {
    /// What is being typed. Unused while a list is being deleted.
    pub entry: tagedit::Entry,
    pub ask: Ask,
}

pub enum Ask {
    /// A new list holding these tracks — which may be none, for an empty one.
    NewList(Vec<Track>),
    /// The list with this id, under a new name.
    Rename(i64),
    /// The list with this id, gone.
    Delete(i64),
}

impl Prompt {
    /// Whether there is a name being typed, or only a word to press.
    pub fn typing(&self) -> bool {
        !matches!(self.ask, Ask::Delete(_))
    }

    /// What the panel calls itself.
    pub fn heading(&self) -> &'static str {
        match self.ask {
            Ask::NewList(_) => tr!("neue liste"),
            Ask::Rename(_) => tr!("liste umbenennen"),
            Ask::Delete(_) => tr!("liste löschen"),
        }
    }

    /// The word that answers it.
    pub fn word(&self) -> &'static str {
        match self.ask {
            Ask::NewList(_) => tr!("liste anlegen"),
            Ask::Rename(_) => tr!("umbenennen"),
            Ask::Delete(_) => tr!("löschen"),
        }
    }
}

/// Music on its way to the device: what would go over, and how far it got.
///
/// Worked out and looked at before anything is written. A sync is the one thing
/// this program does that changes somebody else's hardware, and the difference
/// between and „passt nicht" is worth reading beforehand rather than
/// afterwards.
pub struct Transfer {
    /// What the plan is of, for the heading — „12 titel", „eine platte".
    pub what: String,
    pub plan: crate::ipod::sync::SyncPlan,
    /// Free bytes on the device when the plan was made.
    pub free: Option<u64>,
    /// Whether the leftovers can be converted on the way — only ever true
    /// when there are any and ffmpeg is on this machine.
    pub can_convert: bool,
    pub stage: Stage,
    /// How much of the device was music when the plan was made, so the bar
    /// in the device's column can grow by what has landed since.
    pub music_at_start: u64,
    /// When the device's free space was last read while the copy runs.
    pub space_read_at: Option<std::time::Instant>,
}

/// The three states a transfer is ever in.
pub enum Stage {
    /// Read it, then say yes.
    Asking,
    /// Under way. `failed` counts what would otherwise pass silently.
    Going {
        done: usize,
        total: usize,
        doing: String,
        failed: usize,
        steps: mpsc::Receiver<Moved>,
        /// Set by „abbrechen": the worker stops before the next track and
        /// writes the database for what is across.
        stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    },
    /// Over, with what came of it. Kept on screen rather than reduced to a
    /// line at the bottom: this is the one operation nobody wants to have
    /// half-read while it scrolls away.
    Done(std::result::Result<String, String>),
}

/// What the worker says while it works.
pub enum Moved {
    Along(crate::ipod::sync::SyncProgress),
    Ended(std::result::Result<(), String>),
}

/// What the thread watching for devices has to say.
pub enum DeviceNews {
    /// The device now there, or none.
    Attached(Option<attached::Attached>),
    /// Something to put in the status line and the log: an iPod mounted by
    /// kessler on Linux, or why not.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Said(String),
}

/// The library ids of the tracks a plugged-in iPod has, shared with the
/// frame that draws them.
pub type OnDevice = std::sync::Arc<std::collections::HashSet<i64>>;

/// What the menu head's still k was drawn for: its size in pixels, the
/// language (a ケ in Japanese), the typeface, and the width and bold it is
/// set at.
pub type MarkKey = (usize, crate::lang::Lang, (String, u32, Option<u32>));

pub struct GuiApp {
    pub db: Database,
    pub config: Config,
    pub player: Player,

    /// The scheme the user picked — one half of it; which half is painted
    /// is `brightness`'s to say.
    pub theme: Theme,
    /// Light, dark, or as the system is set.
    pub brightness: Brightness,
    /// What the two came out as, last time the system was asked.
    pub resolved: Theme,
    /// The colours a change of scheme is fading from, and since when.
    fading: Option<(Palette, f64)>,
    pub palette: Palette,
    /// What the window frame was last told to be — see [`dress_the_frame`].
    /// `None` until it has been told anything, which is why the first frame
    /// always tells it.
    ///
    /// [`dress_the_frame`]: GuiApp::dress_the_frame
    frame_theme: Option<egui::SystemTheme>,
    pub font_source: FontSource,
    /// The face standing in for characters the chosen one has no glyph for,
    /// if this machine had one to offer.
    pub font_fallback: Option<String>,
    /// A transfer to the device, while one is being looked at or run.
    pub transfer: Option<Transfer>,
    /// The name of the typeface drawing the window — see [`theme::TYPEFACE`].
    /// None when the `fonts` folder was not found.
    pub typeface: Option<String>,
    /// The families in the `fonts` folder, for the menu to step through.
    pub families: Vec<theme::Family>,
    /// Whether the colour schemes are expanded in the menu.
    pub theme_list_open: bool,
    /// Which of one's own scheme's four colours has its rules open in the menu,
    /// if any.
    pub seed_role_open: Option<usize>,
    /// A scheme's code being pasted into the menu, while it is.
    pub seed_entry: Option<tagedit::Entry>,
    /// One colour of it being typed as hex, under its rules, and which.
    pub seed_hex: Option<(usize, tagedit::Entry)>,
    /// Whether the licences are folded out under „über".
    pub licenses_open: bool,
    /// Whether the face's alternate forms are folded out under „schrift", and
    /// the sets of the face in use, read once per face.
    pub sets_open: bool,
    pub sets_offered: Option<(String, Vec<theme::StyleSet>)>,
    /// Whether a newer kessler is out, once asked under „über".
    pub update: Option<update::Check>,
    /// Whether a newer version found at the start was said at the foot.
    update_told: bool,
    /// What the font texture was last filled for in advance — the screen's
    /// scale and the face — see [`text::prewarm`].
    prewarmed: Option<(u32, String, bool)>,
    /// What the last database backup was called, while the menu is open.
    ///
    /// The status line says it too, but the status line is at the foot of the
    /// window and the menu sheet stands in front of it — so the row that was
    /// clicked says it as well, which is where anybody who clicked it is
    /// looking.
    pub backup_said: Option<String>,
    /// The device's new name, while it is being typed in the device
    /// column — see [`GuiApp::rename_device`].
    pub naming_device: Option<tagedit::Entry>,
    /// A kept station's new name, while it is typed beside its picture
    /// under „sender", with the station's address — see
    /// [`GuiApp::rename_station`].
    pub naming_station: Option<(String, tagedit::Entry)>,
    /// The picture being chosen for a kept station: its address, and the
    /// answer of the system's file dialog to come.
    pub picking_station_picture: Option<(String, mpsc::Receiver<Option<std::path::PathBuf>>)>,
    /// The list of models open in the device's column, for a device that
    /// cannot say which it is — see `ipod::device_info::write_model`.
    pub choosing_model: bool,
    /// Which music folder has been asked about — the row whose „wirklich
    /// entfernen?" is open. Taking a folder out takes its tracks out of the
    /// library with it, which is too far to go on one click of a row that is
    /// mostly there to show a path.
    pub forget_asked: Option<usize>,
    /// Whether the list of halftone methods is unfolded under „bilder".
    pub picture_screen_list_open: bool,
    /// The same for the screen saver's.
    pub saver_screen_list_open: bool,
    /// How tall each section of the menu came out when last drawn, and
    /// where its two columns break — found when it opens and kept while it
    /// stays open. See `menu::body`.
    pub menu_heights: [f32; menu::SECTION_COUNT],
    /// Where the open menu's sheet stands, worked out when it opened.
    pub menu_top: Option<f32>,
    /// When the menu began to go, handing over to the key sheet — see
    /// `menu::render`.
    pub menu_leaving: Option<f64>,
    /// A change of size under way — see `app::ZoomDip`.
    pub zoom_dip: Option<app::ZoomDip>,
    /// The menu's tab, as panels — see `menu::TABS`.
    pub menu_tab: usize,
    /// Whether the fourth column held the device when it was last open — so
    /// it slides out still showing it.
    pub fourth_was: Fourth,
    /// The same for the side column, right of the fourth.
    pub side_was: Fourth,
    /// Whether the window was wide enough, last frame, for the side to have
    /// a column of its own — see [`GuiApp::split`].
    pub side_fits: bool,
    /// Whether the last frame drew the side in a column of its own, so the
    /// frame that changes it can let go of the slides half way.
    pub(super) was_split: bool,
    /// Whether the fourth column is in the middle of sliding in or out.
    pub fourth_sliding: bool,
    /// The still mark, screened, and what it was screened for (its size in
    /// dots and the method) — made once, not every frame.
    pub(super) mark_still: Option<((usize, dither::Screen), egui::TextureHandle)>,

    /// The queue, shown for as long as tracks are being carried while it is
    /// put away — so there is somewhere to put them. See `peek_at_the_queue`.
    pub queue_peek: bool,
    /// The iPod that is plugged in, once its database has been read.
    pub attached: Option<attached::Attached>,
    /// Whether the fourth column is showing the device rather than the
    /// queue. Only ever true while a device is there.
    pub device_open: bool,
    /// What that column says, worked out when it opened.
    pub device_summary: Option<device::Summary>,
    /// The library's tracks the plugged-in iPod has already, for the mark
    /// beside their titles, and the device they were worked out for: where
    /// it is mounted and how many tracks it had — see
    /// `GuiApp::tracks_on_device`.
    pub on_device: Option<((std::path::PathBuf, usize), OnDevice)>,
    /// Tracks being taken off the iPod, asked about or under way.
    pub removal: Option<device::Removal>,
    /// The player at its head: how it stood when the texture was drawn (its
    /// two angles in thousandths), what for, and the texture.
    pub device_art: Option<([i32; 4], device::Made, egui::TextureHandle)>,
    /// How the player is held — see [`device::Handling`].
    pub device_handling: device::Handling,
    /// A list being made, renamed or thrown away.
    pub prompt: Option<Prompt>,
    /// The sound on its way to the card, and what it looks like.
    ///
    /// Read once a frame and handed to everything that draws it, so two
    /// pictures of the same music cannot disagree about what they heard.
    heard: std::sync::Arc<crate::player::audio::Heard>,
    pub spectrum: sound::Spectrum,
    /// The vectorscope behind the search field, and the texture it was last
    /// drawn into — kept, because a texture made afresh every frame is an
    /// allocation on the graphics card thirty times a second.
    pub scope: scope::Scope,
    scope_picture: Option<egui::TextureHandle>,
    /// The small scope at the foot of the menu, over „kessler" and the
    /// version: one figure, all registers inside each other — and its texture.
    pub mark_scope: scope::Scope,
    /// What the quiet pictures of the search field remember — see
    /// `library::FieldPicture`.
    field_art: library::FieldArt,
    /// How far into the book the field has read, beyond the config's place.
    reading: library::Reading,
    /// The same for the figure at the head of the menu.
    mark_art: library::FieldArt,
    pub(super) mark_picture: Option<egui::TextureHandle>,
    /// The same picture once more, for the Touch Bar: its own scope and
    /// memory, and a fingerprint of the dots last handed over, so a step
    /// that changed nothing is not sent again.
    pub(super) touch_scope: scope::Scope,
    /// The needles behind the search field — see `library::meter`.
    meter: library::Meter,
    touch_art: library::FieldArt,
    pub(super) touch_sent: Option<u64>,
    /// What the Touch Bar was last set up for — shown, small, whole, a
    /// station — so it is set up again only when one of them changes.
    pub(super) touch_mode: Option<(bool, bool, bool, bool)>,
    /// What the measurement last said of the Touch Bar, since when, and the
    /// pictures sent since — see `library::picture::touch_bar_picture`.
    pub(super) touch_said: (String, f64, u32),
    /// The needles for the Touch Bar, moving of their own.
    touch_meter: library::MeterDots,
    /// The k for the menu's head while nothing plays, with what it was
    /// drawn for — see [`MarkKey`].
    pub(super) mark_paused: Option<(MarkKey, egui::TextureHandle)>,
    /// The screen saver: whether it is up, and what it draws with.
    pub saver: saver::Saver,
    /// Which shelf the columns are reading from.
    pub source: Source,
    /// How many of the device's tracks the filter leaves, so the row can say
    /// what it stands for without counting them again every frame.
    pub device_matching: usize,
    /// The watcher's answers. It looks every couple of seconds and only reads
    /// a database when the volume it sees has changed.
    device_watch: Option<mpsc::Receiver<DeviceNews>>,

    pub artists: Vec<Artist>,
    pub albums: Vec<Album>,
    pub tracks: Vec<Track>,
    /// Every compilation in the library, each a record in its running order,
    /// read with the names in `reload_all` — for the „compilations" row.
    pub compilations: Vec<Vec<Track>>,
    /// The tracks in the list that belong to a compilation shown whole under
    /// one of its artists — see `widen_compilations` — and, of those, the
    /// other artists' ones, which are dimmed.
    pub compiled: std::collections::HashSet<i64>,
    pub guests: std::collections::HashSet<i64>,
    /// The user's own lists, with how long each one is. Read once when the window
    /// opens and again whenever one of them changes — they are a handful of
    /// rows, not a library.
    pub lists: Vec<(crate::db::models::Playlist, usize)>,
    /// Which list the fourth column is holding open instead of the queue.
    ///
    /// The queue's column is the one place in this window with room for a
    /// running order you can take hold of, and a list *is* a running order —
    /// so rather than inventing a fifth column, a list borrows the fourth one
    /// for as long as it is being worked on and hands it back afterwards. The
    /// queue is not disturbed by this: it is still there, still playing, just
    /// not the thing that column is showing.
    pub editing: Option<i64>,
    /// Its tracks, in their running order, as that column shows them.
    ///
    /// Kept here rather than read from the database each frame, because this
    /// is what a drag rearranges: the column moves rows about in it and writes
    /// the whole order back afterwards.
    pub editing_tracks: Vec<Track>,
    /// What the window is drawn through, as „über" says it — read from
    /// the first frame.
    pub drawn_with: String,
    pub total_tracks: u32,
    /// Albums in the whole library, as opposed to the filtered `albums`.
    pub total_albums: usize,
    /// How long the whole library plays, and what its files weigh — for the
    /// line at the foot of the compact window.
    pub total_seconds: f64,
    pub total_bytes: u64,
    /// Tracks the filter matches across the whole library. Equal to
    /// `total_tracks` when nothing is filtered.
    pub matching_tracks: u32,

    pub artist_index: usize,
    pub album_index: usize,
    pub track_index: usize,
    pub focus: Column,
    /// Nothing has been pressed or clicked since the window opened: the
    /// row it opens on is chosen (bold, with its arrow) but wears no
    /// marker yet, as the keyboard has not been put anywhere.
    pub untouched: bool,
    /// How many tracks „zuletzt" has fetched beyond its length, by
    /// scrolling to its end; back to none once another row is chosen.
    pub recent_more: usize,
    /// „zuletzt"'s records, newest first, as the database gave them, and how
    /// many of them stand in the track list — so scrolling on fetches the
    /// next records alone, without asking for every record again.
    recent_albums: Vec<Album>,
    recent_taken: usize,

    /// Which track rows are picked out, as indexes into `tracks`. Always holds
    /// `track_index`: the cursor is part of the selection, so that everything
    /// acting on "the selection" also works when nobody has extended it.
    pub track_selection: std::collections::BTreeSet<usize>,
    /// Where a shift-click or shift-arrow measures its range from.
    pub track_anchor: usize,
    /// Whether the selection is a whole record, taken by a click on its heading
    /// in the combined view — for the arrow keys, which then step in from the
    /// heading rather than from its last track. Any other change of the
    /// selection ends it.
    pub record_picked: bool,
    /// Whether escape has let everything go: no row of the tracks, the queue or
    /// an open list is marked, the cursor's neither. The cursor stays where it
    /// was, for the next step to go on from; until then the keys that act on
    /// what is marked act on nothing. See [`GuiApp::let_go`].
    pub unmarked: bool,
    /// The queue's revision as it was last written to disk, so that writing
    /// it again costs one comparison on the frames where nothing happened.
    queue_saved: u64,
    /// What a hand is carrying, while it is carrying something.
    pub dragging: Option<Dragged>,
    /// Which column the tag editor stands in while it is open.
    ///
    /// It takes the place of the column whose rows it is about: editing one
    /// track's tags happens where that track was, and editing a whole record's
    /// happens where the record was — so what you are working on is still on
    /// screen beside the form rather than replaced by it.
    editor_in: Column,
    /// Which row of the queue was playing when the column last carried itself
    /// along. `None` until it has, so the first track scrolls into place too.
    queue_followed: Option<usize>,
    /// Which row of the queue the keyboard is on. Not the row that is playing
    /// — that is the player's business, and the two move independently: you
    /// look down the queue while something else goes on playing.
    pub queue_index: usize,
    /// Rows of the queue picked together, beside the cursor: shift takes a run
    /// of them, cmd or ctrl one more. Only for the queue's revision they were
    /// picked in — a queue changed from elsewhere lets them go, since the
    /// numbers would point at other tracks.
    pub queue_picked: std::collections::BTreeSet<usize>,
    pub queue_picked_in: u64,

    /// The tag editor, while it is open. It takes the track column and the
    /// keyboard with it.
    pub editor: Option<tagedit::Editor>,
    /// The little menu a right-click opened, if it is still up.
    pub row_menu: Option<RowMenu>,
    /// The right-click menu by the keyboard: the entry the keys stand on, the
    /// steps asked for since the last frame, and whether enter was pressed on
    /// it.
    pub row_menu_focus: Option<usize>,
    pub row_menu_step: i32,
    pub row_menu_enter: bool,
    /// `←` `→` on the stars at its foot, since the last frame.
    pub row_menu_side: i32,
    /// Choosing a cover for one album. Takes the track column while it is
    /// open, like the tag editor.
    pub cover_pick: Option<coverpick::Picker>,
    /// Going through every album without a year, asking MusicBrainz.
    pub tag_hunt: Option<TagHunt>,
    /// What one track is, read out in full. Takes the track column while it is
    /// open, like the tag editor — it is about the row you were looking at, so
    /// it stands where that row was.
    pub details: Option<Details>,

    /// How the track list is ordered, and which way round.
    pub sort: SortField,
    pub order: SortOrder,
    /// How each of the two name columns runs.
    pub artist_sort: ListSort,
    pub album_sort: ListSort,

    /// Which fields the track list shows, besides the title.
    pub columns: columns::TrackColumns,
    /// The covers currently held as textures.
    pub cover_cache: covers::CoverCache,

    /// Whether the window is too narrow to hold the columns side by side, as
    /// the last frame found it. Read by the columns: while they are sliding, a
    /// click on a name also moves on to the next one.
    pub narrow: bool,
    /// Where the cursor stands among the lists' names, one past the last for.
    pub lists_index: usize,
    /// The list open last, to open again when the lists come back.
    pub lists_last: Option<i64>,

    /// Every column should bring its selection back into view, not only the
    /// one the keyboard is in. Set when the window jumps somewhere of its own
    /// accord — which is once: finding the playing track.
    pub reveal_all: bool,
    /// Keep `scroll_to` and `reveal_all` for one more frame — see
    /// `library::found_it`.
    pub reveal_next: bool,

    /// A column whose selected row wants bringing back into view.
    ///
    /// Set by the keyboard, never by the mouse: a click lands on a row the user
    /// can already see, and scrolling under their pointer would be rude.
    pub scroll_to: Option<Column>,

    /// Last.fm, when an account is connected. `None` is the ordinary state of
    /// a program nobody has connected one to.
    pub scrobbler: Option<crate::lastfm::Scrobbler>,
    /// Follows the player and decides what counts as listened to.
    scrobble_watch: crate::lastfm::watch::Watch,
    /// An account being connected: the browser is open and the answer will
    /// arrive on this channel.
    connecting: Option<mpsc::Receiver<Result<(String, String)>>>,

    /// Keeps the system's Now Playing panel in step, and is the reason the
    /// media keys reach this program at all.
    media: crate::media::Reporter,
    /// The symbol in the macOS menu bar, when it is asked for.
    menu_bar: menubar::MenuBar,
    /// Whether the system's transport has been asked for yet. It cannot be
    /// asked before the application exists, which is after the first frame.
    media_started: bool,
    /// What GNOME's own setting says, light or dark, and where news of it
    /// changing comes in — on Linux, where the window system does not pass it
    /// on (see `desktop_scheme`). Unused elsewhere.
    desktop_theme: Option<egui::Theme>,
    desktop_news: Option<mpsc::Receiver<egui::Theme>>,
    /// Set by „q", which means the program and not only the window: the red
    /// button on a Mac only puts the window away (see `put_away_on_close`),
    /// and this is how the one is told from the other.
    quitting: bool,
    /// When „q" was pressed once, waiting for the second press that quits: one
    /// q on its own was too easy to hit while typing elsewhere.
    quit_asked_at: Option<f64>,
    /// When `shift` + `d` was pressed once in the queue, waiting for the second
    /// press that empties it.
    clear_asked_at: Option<f64>,
    /// When a first `g` was pressed, waiting for the second of `gg`.
    g_pressed_at: Option<f64>,
    /// When `c` was pressed, for `c` and a digit switching a view off.
    c_pressed_at: Option<f64>,
    /// The view open now and the one open before it, kept once a frame: a view
    /// switched off goes back to the one before.
    view_now: View,
    view_before: Option<View>,
    /// The sheet of every key, over the window (`?`).
    pub keys_sheet_open: bool,
    /// How far the key sheet is to scroll on its next frame, in points.
    pub keys_sheet_scroll: f32,
    /// What has been typed into the key sheet's field, as typed.
    pub keys_query: String,

    /// The settings menu, when it is open.
    pub menu_open: bool,
    /// The measurement, while the menu has one running — see [`measure`].
    pub measure: Option<measure::Measure>,
    /// Every scheme being run through for a GIF — see [`app::Reel`].
    pub(in crate::gui) reel: Option<Box<app::Reel>>,
    /// The thread writing the GIFs, while it writes.
    pub(in crate::gui) reel_written: Option<app::Written>,
    /// When the last frame began, for `config.ui.frame_cap`.
    frame_started: Option<std::time::Instant>,
    /// The live filter. `None` is the closed state; `Some("")` is an open,
    /// empty filter line, which looks different and behaves differently.
    pub filter: Option<String>,
    /// The radio, while the first column shows it — `None` the rest of the
    /// time, holding nothing (see `crate::radio`).
    pub radio: Option<Box<crate::radio::Radio>>,
    /// The station on the air, for the header: kept while the column has
    /// gone back to the library.
    pub on_air: Option<crate::radio::Station>,
    /// The file picker asked for a list of stations to keep, while it is
    /// open — see `GuiApp::import_stations`.
    pub station_import: Option<std::sync::mpsc::Receiver<Option<std::path::PathBuf>>>,
    /// When the search field last changed while the radio was open: the
    /// directory is asked once the typing has stopped.
    radio_typed_at: Option<std::time::Instant>,
    /// Where the music's keyboard was when the radio opened: the radio
    /// holds it on its stations, and the music gets it back as it was left.
    music_focus: Option<Column>,
    /// Each view's side as it was left — the music's, the radio's, the
    /// podcasts' — put back when it is come back to; see `View`.
    sides_left: [Option<Sides>; 4],
    /// Whom the radio played and where, read when first asked for — see
    /// [`crate::radio::heard`].
    radio_heard: Option<Box<crate::radio::heard::Heard>>,
    /// What the station on the air said it played, newest first.
    pub aired: Vec<crate::radio::Aired>,
    /// The station whose answer has been written in the log, so it is written
    /// once.
    station_noted: Option<String>,
    /// Since when the station on the air stands paused: the time it stands
    /// is not heard, and is not counted towards last.fm.
    pub(in crate::gui) radio_paused_since: Option<chrono::DateTime<chrono::Local>>,
    /// The playlist of the station on the air, where it keeps one in Spinitron.
    spins: Option<Box<crate::radio::spins::Watch>>,
    /// The programme of the station on the air, where it publishes one: kept
    /// while it plays, in the library too.
    pub(in crate::gui) programme: Option<Box<crate::radio::programme::Programme>>,
    /// Where the window was when it was last looked at, and since when it has
    /// been standing still there — see [`GuiApp::remember_window`].
    window_was: Option<[f32; 4]>,
    window_still_since: Option<std::time::Instant>,

    /// Files the player has found missing, kept for as long as the program
    /// runs — see [`GuiApp::note_what_is_gone`].
    missing: std::collections::HashSet<std::path::PathBuf>,

    /// Whether everything in the field is marked — what cmd+a leaves behind.
    ///
    /// The field holds a string and no cursor, so there is no selection to
    /// speak of and no half of one. This is the whole of it: everything, or
    /// nothing. The next letter replaces it, backspace empties it, and that
    /// covers what the key is actually for: a typo, gone.
    pub filter_marked: bool,

    /// Whether the keyboard is still in the field.
    ///
    /// Typing and *having typed* are two different states, and the window had
    /// only one: while a query stood in the field every letter went into it,
    /// so the only way back to the list was to throw the search away with esc.
    /// A click anywhere else now ends the typing and leaves the search
    /// standing — which is what clicking away from a field means everywhere.
    pub filter_typing: bool,
    /// Where the caret stands in the search field, in characters from its
    /// start; `None` at the end, where typing has always gone on.
    pub filter_caret: Option<usize>,
    /// On the radio, whether the keyboard is in „gerade gehört" rather than the
    /// stations, and on which of its rows — what was heard, then the songs
    /// kept.
    pub radio_heard_keys: Option<usize>,
    /// A rescan running on another thread. Scanning a real library takes long
    /// enough that doing it on the UI thread would freeze the window.
    scan: Option<mpsc::Receiver<ScanOutcome>>,
    /// How far that scan has got: files read, and how many there are.
    scan_progress: Option<mpsc::Receiver<crate::library::scanner::ScanProgress>>,
    scan_done: u32,
    scan_total: u32,
    /// Music the last scan walked past for its format, counted per extension.
    foreign: Vec<(String, u32)>,
    /// The tracks the running scan read again, said in the log when it ends.
    scan_updates: Vec<crate::library::scanner::Update>,
    /// A folder picker waiting for an answer. The system panel blocks until
    /// the person closes it, which on the drawing thread would be a window
    /// that has stopped redrawing while a dialog sits in front of it.
    picking: Option<mpsc::Receiver<Option<std::path::PathBuf>>>,
    /// Music being filed into the library on another thread, and how far it
    /// has got.
    adding: Option<mpsc::Receiver<crate::library::import::Outcome>>,
    /// A file panel open for "musik hinzufügen…".
    adding_pick: Option<mpsc::Receiver<Vec<std::path::PathBuf>>>,
    add_progress: Option<mpsc::Receiver<crate::library::import::Progress>>,
    add_done: u32,
    add_total: u32,
    /// How many files are hovering over the window right now, waiting to be
    /// let go of.
    pub hovering: usize,
    /// The same, for a cover file, with the album it is for.
    picking_cover: Option<(mpsc::Receiver<Option<std::path::PathBuf>>, usize)>,

    status: Option<(String, Instant, u64)>,
    /// What the program did, written down — see [`log`].
    pub log: log::Log,
    /// Whether the log has the fourth column.
    pub log_open: bool,
    /// Whether the artist's column has it — see [`artist`].
    pub artist_open: bool,
    /// Whether the lyrics have the side, and what they show.
    pub lyrics_open: bool,
    /// Whether the equalizer has the side — see `app::eq`.
    pub eq_open: bool,
    /// How long kessler has played, by source — under „über".
    pub listened: listened::Listened,
    /// What the icon was last dressed in — ground, accent and typeface —
    /// so it is drawn again only when one of them changes.
    icon_dressed: Option<(egui::Color32, egui::Color32, Option<String>, crate::lang::Lang)>,
    /// The icon being drawn on its own thread, to be handed to the system
    /// once it is done — see `dress_the_icon`.
    icon_coming: Option<std::sync::mpsc::Receiver<Vec<u8>>>,
    /// Whether the window was last told to wear its title bar — see
    /// `dress_the_bar`. Nothing until the first frame has told it.
    bar_worn: Option<bool>,
    /// The language the window's title was last given in — see
    /// `lang::app_name`.
    titled: Option<crate::lang::Lang>,
    pub lyrics: lyrics::LyricsPanel,
    pub artist: artist::ArtistPanel,
    /// The form for what the person writes about an artist, while open.
    pub artist_form: Option<artist::OwnForm>,
    /// A folder being chosen for the artists' Markdown.
    pub exporting_artists: Option<mpsc::Receiver<Option<std::path::PathBuf>>>,
    /// The run through every artist not yet looked up — see [`artist::Hunt`].
    pub artist_hunt: Option<artist::Hunt>,
    /// An artist asked for by name, and where the window stood then — see
    /// [`GuiApp::about_artist`].
    pub artist_pin: Option<(String, ArtistAnchor)>,
    /// The artists' pages gone through by name, from within the column, for
    /// the way back — emptied when the column follows the window again.
    pub artist_trail: Vec<String>,
    /// The other artists, a column of their own — see
    /// [`artist::GapsPanel`].
    pub gaps: artist::GapsPanel,
    /// The podcasts' view — see [`app::podcasts::PodcastPanel`].
    pub podcasts: app::podcasts::PodcastPanel,
    /// last.fm's key and secret being typed in, in the menu — see
    /// [`lastfmkeys`].
    pub lastfm_keys: Option<lastfmkeys::KeyForm>,
    /// The run finding out how loud everything is — see [`levelrun`] —
    /// whether to look for one when the library has changed, and how many
    /// tracks are known out of how many.
    pub measuring: Option<levelrun::Measuring>,
    pub level_check: bool,
    pub loudness_count: (usize, usize),
    /// Set when the log is opened: it starts at the newest entry, not where
    /// it was left.
    pub log_from_newest: bool,
}

impl GuiApp {
    /// What is done on the way out, whichever way the window is drawn.
    fn leave(&mut self) {
        self.listened.save();
        crate::podcast::Kept::flush();
    }

    pub fn new(mut config: Config, ctx: &egui::Context) -> Result<Self> {
        config.ui.even_columns_again();
        crate::lang::set(crate::lang::Lang::chosen(&config.ui.language));
        let db = Database::open(&config.db_path())?;
        let listened = listened::Listened::open(|| db.heard_before().unwrap_or(0.0));
        let config_log_path = config.db_path().with_file_name("log.txt");
        let mut player = Player::new(config.player.volume)?;
        player.set_gapless(config.player.gapless);
        theme::set_custom(theme::Seed::parse(&config.ui.custom_colours).unwrap_or(theme::Seed::DEFAULT));
        let theme = Theme::from_name(&config.ui.ground);
        let brightness = Brightness::from_config(&config.ui.brightness, &config.ui.ground);
        let resolved = brightness.resolve(theme, ctx.system_theme().or_else(system_theme));
        let palette = resolved.palette();

        let families = theme::all_families(config.ui.font_dir.as_deref(), config.ui.system_fonts);
        if config.ui.typeface.is_empty() && families.iter().any(|f| f.key == theme::PREFERRED) {
            config.ui.typeface = theme::PREFERRED.to_string();
        }
        let chosen = theme::the_typeface(&families, &config.ui.typeface);
        theme::set_face(&config.ui);
        theme::set_light_ground(!palette.is_dark());
        let (font_source, font_fallback) = theme::install_fonts(
            ctx,
            chosen.as_ref().map(|c| c.regular.as_str()),
            chosen.as_ref().and_then(|c| c.bold.as_deref()),
            chosen.as_ref().and_then(|c| c.figures.as_deref()),
            config.ui.font_fallback.as_deref(),
            chosen.as_ref().map_or(1.0, |c| theme::face_scale(&c.key)),
        );
        let typeface = chosen.map(|c| c.name);

        let wake = ctx.clone();
        crate::media::on_wake(move || wake.request_repaint());
        theme::install_style(ctx, &palette);

        let config_columns = config.ui.track_columns.clone();
        let artist_order = ListSort::from_key(&config.ui.artist_order, false);
        let album_order = ListSort::from_key(&config.ui.album_order, true);
        let (track_sort, track_direction) = {
            let shown = columns::TrackColumns::from_names(&config_columns);
            match track_order_from_key(&config.ui.track_order) {
                Some((field, order))
                    if matches!(field, SortField::Artist | SortField::Title)
                        || columns::TrackColumn::ALL
                            .iter()
                            .any(|c| shown.contains(*c) && c.sort_field() == field) =>
                {
                    (field, order)
                }
                _ => (SortField::Artist, SortOrder::Ascending),
            }
        };
        let config_for_lastfm = config.lastfm.clone();
        let queue_path = config.scrobble_queue_path();
        let artwork_dir = config.artwork_dir();
        let heard = player.heard();

        let ask_at_start = config.ui.update_on_start;
        if config.ui.podcasts {
            std::thread::spawn(crate::podcast::sweep_parts);
        }
        std::thread::spawn(crate::ipod::sync::sweep_scratch);
        let mut app = Self {
            db,
            config,
            player,
            theme,
            brightness,
            resolved,
            fading: None,
            frame_theme: None,
            palette,
            font_source,
            font_fallback,
            typeface,
            families,
            transfer: None,
            theme_list_open: false,
            seed_role_open: None,
            seed_entry: None,
            seed_hex: None,
            licenses_open: false,
            sets_open: false,
            sets_offered: None,
            update: ask_at_start.then(update::Check::start),
            update_told: false,
            prewarmed: None,
            forget_asked: None,
            backup_said: None,
            naming_device: None,
            naming_station: None,
            picking_station_picture: None,
            choosing_model: false,
            picture_screen_list_open: false,
            saver_screen_list_open: false,
            menu_heights: [0.0; menu::SECTION_COUNT],
            menu_top: None,
            menu_leaving: None,
            zoom_dip: None,
            menu_tab: 0,
            fourth_was: Fourth::Queue,
            side_was: Fourth::Artist,
            side_fits: false,
            was_split: false,
            fourth_sliding: false,
            mark_still: None,
            queue_peek: false,
            attached: None,
            device_open: false,
            device_summary: None,
            on_device: None,
            device_art: None,
            device_handling: device::Handling::default(),
            prompt: None,
            heard,
            spectrum: sound::Spectrum::default(),
            scope: scope::Scope::default(),
            scope_picture: None,
            mark_scope: scope::Scope::default(),
            field_art: library::FieldArt::default(),
            reading: library::Reading::default(),
            mark_art: library::FieldArt::default(),
            mark_picture: None,
            touch_scope: scope::Scope::default(),
            meter: library::Meter::default(),
            touch_art: library::FieldArt::default(),
            touch_sent: None,
            touch_mode: None,
            touch_said: (String::new(), 0.0, 0),
            touch_meter: library::MeterDots::new(),
            mark_paused: None,
            saver: saver::Saver::default(),
            radio: None,
            on_air: None,
            station_import: None,
            radio_typed_at: None,
            music_focus: None,
            sides_left: [None; 4],
            radio_heard: None,
            aired: Vec::new(),
            station_noted: None,
            radio_paused_since: None,
            spins: None,
            programme: None,
            source: Source::default(),
            device_matching: 0,
            device_watch: None,
            artists: Vec::new(),
            lists: Vec::new(),
            editing: None,
            editing_tracks: Vec::new(),
            drawn_with: String::new(),
            albums: Vec::new(),
            tracks: Vec::new(),
            compilations: Vec::new(),
            compiled: Default::default(),
            guests: Default::default(),
            total_tracks: 0,
            total_albums: 0,
            total_seconds: 0.0,
            total_bytes: 0,
            matching_tracks: 0,
            artist_index: 0,
            album_index: 0,
            track_index: 0,
            focus: Column::Artists,
            untouched: true,
            recent_more: 0,
            recent_albums: Vec::new(),
            recent_taken: 0,
            lists_index: 0,
            lists_last: None,
            track_selection: std::collections::BTreeSet::new(),
            track_anchor: 0,
            record_picked: false,
            unmarked: false,
            queue_index: 0,
            queue_picked: std::collections::BTreeSet::new(),
            queue_picked_in: 0,
            queue_saved: 0,
            queue_followed: None,
            editor_in: Column::Tracks,
            dragging: None,
            editor: None,
            row_menu: None,
            row_menu_focus: None,
            row_menu_step: 0,
            row_menu_enter: false,
            row_menu_side: 0,
            cover_pick: None,
            details: None,
            tag_hunt: None,
            sort: track_sort,
            order: track_direction,
            artist_sort: artist_order,
            album_sort: album_order,
            columns: columns::TrackColumns::from_names(&config_columns),
            cover_cache: covers::CoverCache::new(artwork_dir),
            narrow: false,
            reveal_all: false,
            reveal_next: false,
            scroll_to: None,
            media: crate::media::Reporter::new(),
            menu_bar: menubar::MenuBar::default(),
            media_started: false,
            quitting: false,
            quit_asked_at: None,
            clear_asked_at: None,
            g_pressed_at: None,
            c_pressed_at: None,
            view_now: View::Music,
            view_before: None,
            keys_sheet_open: false,
            keys_sheet_scroll: 0.0,
            keys_query: String::new(),
            desktop_theme: None,
            desktop_news: None,
            menu_open: false,
            measure: std::env::var_os("KESSLER_MESSUNG").and_then(|path| measure::Measure::start(path.into()).ok()),
            reel: None,
            reel_written: None,
            frame_started: None,
            filter: None,
            filter_typing: false,
            filter_marked: false,
            filter_caret: None,
            radio_heard_keys: None,
            window_was: None,
            window_still_since: None,
            missing: std::collections::HashSet::new(),
            scan: None,
            scan_progress: None,
            scan_done: 0,
            scan_total: 0,
            foreign: Vec::new(),
            scan_updates: Vec::new(),
            scrobbler: crate::lastfm::Scrobbler::start(
                &config_for_lastfm,
                queue_path,
            ),
            scrobble_watch: crate::lastfm::watch::Watch::new(),
            connecting: None,
            picking: None,
            picking_cover: None,
            adding: None,
            adding_pick: None,
            add_progress: None,
            add_done: 0,
            add_total: 0,
            hovering: 0,
            status: None,
            log: log::Log::open(&config_log_path),
            log_open: false,
            artist_open: false,
            lyrics_open: false,
            eq_open: false,
            listened,
            icon_dressed: None,
            icon_coming: None,
            bar_worn: None,
            titled: None,
            lyrics: lyrics::LyricsPanel::default(),
            artist: artist::ArtistPanel::default(),
            artist_form: None,
            exporting_artists: None,
            artist_hunt: None,
            lastfm_keys: None,
            artist_pin: None,
            artist_trail: Vec::new(),
            gaps: artist::GapsPanel::default(),
            podcasts: Default::default(),
            measuring: None,
            removal: None,
            level_check: false,
            loudness_count: (0, 0),
            log_from_newest: false,
        };
        app.apply_leveling();
        app.apply_eq();
        if app.config.ui.start_on_recent && app.config.ui.show_recent {
            app.artist_index = ArtistRow::Recent.index();
        }
        app.reload_all()?;

        if !app.player.is_audio_available() {
            app.set_status(tr!("kein audiogerät — abspielen bleibt stumm"));
        }
        if let Some(aside) = app.config.set_aside.as_ref().and_then(|p| p.file_name()) {
            let name = aside.to_string_lossy().to_string();
            app.set_status(tr!("einstellungen unlesbar — beiseitegelegt als {}", name));
        }

        app.restore_queue();

        if app.config.watch_dirs && !app.config.music_dirs.is_empty() && app.config.ui.music {
            app.start_rescan();
        }
        Ok(app)
    }

    pub fn reload_all(&mut self) -> Result<()> {
        self.level_check = true;
        if !self.config.ui.music && !self.showing_ipod() {
            self.artists = Vec::new();
            self.albums = Vec::new();
            self.tracks = Vec::new();
            self.compilations = Vec::new();
            return Ok(());
        }
        self.total_tracks = self.db.track_count()?;
        self.total_albums = self.db.get_albums(None, None)?.len();
        (self.total_seconds, self.total_bytes) = self.db.library_totals()?;
        let query = self.query();
        self.matching_tracks = self.db.track_count_matching(query.as_deref())?;
        self.device_matching = self
            .attached
            .as_ref()
            .map(|d| d.matching(query.as_deref()))
            .unwrap_or(0);

        self.lists = self.db.playlists_with_counts()?;
        self.artists = match (self.showing_ipod(), self.artist_sort) {
            (true, _) => self
                .attached
                .as_ref()
                .map(|d| d.artists(query.as_deref()))
                .unwrap_or_default(),
            (false, ListSort::Added) => self.db.get_artists_recent(query.as_deref())?,
            (false, _) => self.db.get_artists(query.as_deref())?,
        };
        if self.artist_sort == ListSort::Descending {
            self.artists.reverse();
        }
        self.compilations = if self.config.ui.show_compilations {
            self.compilation_records()?
        } else {
            Vec::new()
        };
        self.reload_dependent()
    }

    /// Sort the track list by this field, or reverse it if it already is.
    ///
    /// Clicking a column head twice to turn it round is the one convention
    /// worth keeping from every other list of things ever built.
    pub fn sort_tracks_by(&mut self, field: SortField) -> Result<()> {
        if self.sort == field {
            self.order = self.order.toggle();
        } else {
            self.sort = field;
            self.order = SortOrder::Ascending;
        }
        self.remember_orders();
        self.track_index = 0;
        self.scroll_to = Some(Column::Tracks);
        self.reload_dependent()
    }

    /// Write the three orders into the settings, so the window opens the
    /// way it was left.
    fn remember_orders(&mut self) {
        self.config.ui.artist_order = self.artist_sort.key().to_string();
        self.config.ui.album_order = self.album_sort.key().to_string();
        self.config.ui.track_order = track_order_key(self.sort, self.order);
        self.save_config();
    }

    /// Step the artist list on to its next order. Stepping the order keeps what
    /// is chosen: „zuletzt" stays „zuletzt", and a name is found again wherever
    /// the new order puts it. It used to start the column over, on everything.
    pub fn cycle_artist_order(&mut self) -> Result<()> {
        let kept = self.selected_artist();
        self.artist_sort = self.artist_sort.next();
        self.remember_orders();
        self.scroll_to = Some(Column::Artists);
        let row = self.artist_row(self.artist_index);
        self.reload_all()?;
        if let (ArtistRow::Named(_), Some(name)) = (row, kept) {
            let key = crate::db::sort_key(&name);
            let names: Vec<&str> = self.artists.iter().map(|a| a.name.as_str()).collect();
            let found = artist_row_for(&names, &key);
            if found != self.artist_row(self.artist_index) {
                self.artist_index = found.index();
                self.reload_dependent()?;
            }
        }
        Ok(())
    }

    /// Step the album list on to its next order.
    pub fn cycle_album_order(&mut self) -> Result<()> {
        self.album_sort = self.album_sort.next();
        self.remember_orders();
        self.album_index = 0;
        self.scroll_to = Some(Column::Albums);
        self.reload_dependent()
    }

    /// Put the records in the order of the year on them, or turn that round.
    ///
    /// The year has a column of its own in this window, so it gets to hold the
    /// order itself rather than being one more stop on the name's round — a
    /// figure you can see down the column is a figure you expect to be able to
    /// sort by.
    pub fn cycle_album_years(&mut self) -> Result<()> {
        self.album_sort = self.album_sort.other_year();
        self.remember_orders();
        self.album_index = 0;
        self.scroll_to = Some(Column::Albums);
        self.reload_dependent()
    }

    /// The filter as the database wants it: `None` unless something was typed.
    ///
    /// An open but empty filter line must not narrow anything, or the library
    /// would appear to vanish the moment the line is opened. Owned, because
    /// every caller assigns into `self` while holding it.
    pub fn query(&self) -> Option<String> {
        self.filter
            .as_deref()
            .map(str::trim)
            .filter(|q| !q.is_empty())
            .map(str::to_string)
    }

    /// Albums and tracks follow whatever is selected to their left.
    pub fn reload_dependent(&mut self) -> Result<()> {
        let artist = self.selected_artist();
        let album = self.selected_album();
        let query = self.query();
        let query = query.as_deref();

        let recent = self.showing_recent();
        if !recent {
            self.recent_more = 0;
        }

        if self.showing_ipod() {
            return self.reload_from_device(artist, album, query);
        }
        if self.showing_compilations() {
            return self.reload_compilations();
        }

        self.albums = match (self.album_sort, recent) {
            (_, true) => {
                self.recent_albums = self.db.get_albums_recent(None, query)?;
                self.recent_albums.clone()
            }
            (ListSort::Added, _) => self.db.get_albums_recent(artist.as_deref(), query)?,
            (_, false) => self.db.get_albums(artist.as_deref(), query)?,
        };
        if self.album_sort == ListSort::Descending {
            self.albums.reverse();
        }
        if self.album_sort.is_year() {
            let back = self.album_sort == ListSort::YearBack;
            self.albums.sort_by_key(|album| {
                let year = album.year.unwrap_or(0);
                (album.year.is_none(), if back { -year } else { year })
            });
        }
        if self.album_index > self.albums.len() {
            self.album_index = 0;
        }
        let album = if album.is_some() { self.selected_album() } else { None };

        let artist = self.selected_album_artist().or(artist);

        self.tracks = match (recent, album.is_none()) {
            (true, true) if self.merged() => {
                self.recent_taken = 0;
                let mut tracks = Vec::new();
                self.take_recent_records(&mut tracks, query)?;
                tracks
            }
            (true, true) => self
                .db
                .get_recent_tracks(query, self.recent_wanted())?,
            _ => {
                let (sort, order) = self.track_order();
                self.db
                    .get_tracks(artist.as_deref(), album.as_deref(), sort, order, query)?
            }
        };
        self.compiled.clear();
        self.guests.clear();
        if let (false, None, Some(named)) = (recent, query, self.selected_artist()) {
            if self.config.ui.compilations_whole
                && self.track_order() == (SortField::Artist, SortOrder::Ascending)
            {
                self.widen_compilations(&named)?;
            }
        }
        if self.track_index >= self.tracks.len() {
            self.track_index = 0;
        }
        self.reset_track_selection();
        Ok(())
    }

    /// The order the track list is read in.
    ///
    /// The one the heads hold — except with the albums in the list, where it
    /// is always the shelf's own: every record under its heading, in its own
    /// running order. Sorted by length, a record's tracks would no longer
    /// stand together and the headings would have nothing to head, so there
    /// the heads are only names and offer no order at all.
    pub fn track_order(&self) -> (SortField, SortOrder) {
        if self.merged() {
            (SortField::Artist, SortOrder::Ascending)
        } else {
            (self.sort, self.order)
        }
    }

    /// Each compilation in the list, whole: the chosen artist's tracks on it
    /// and around them everyone else's, in the record's own running order.
    ///
    /// A record without an album artist is filed under each of its artists, so
    /// a compilation fell apart into a record of one track under every name on
    /// it. Whole again, it says where the song comes from, and plays on from
    /// there like any record. The others' tracks are dimmed (`guests`): they
    /// are on the record, not by this artist.
    ///
    /// A compilation is a title with no album artist, in one folder, by at
    /// least [`COMPILATION_ARTISTS`] artists. The folder keeps two records
    /// that only share a name — „greatest hits" — apart.
    fn widen_compilations(&mut self, artist: &str) -> Result<()> {
        use std::collections::HashSet;
        let own = crate::db::sort_key(artist);
        let tracks = std::mem::take(&mut self.tracks);
        let mut done: HashSet<(String, Option<std::path::PathBuf>)> = HashSet::new();
        let mut widened = Vec::with_capacity(tracks.len());
        for track in tracks {
            if !track.album_artist.trim().is_empty() || track.album.trim().is_empty() {
                widened.push(track);
                continue;
            }
            let folder = track.path.parent().map(std::path::Path::to_path_buf);
            let key = (crate::db::sort_key(&track.album), folder.clone());
            if done.contains(&key) {
                continue;
            }
            let mut record: Vec<Track> = self
                .db
                .get_tracks(None, Some(&track.album), SortField::Artist, SortOrder::Ascending, None)?
                .into_iter()
                .filter(|t| t.album_artist.trim().is_empty() && t.path.parent().map(std::path::Path::to_path_buf) == folder)
                .collect();
            let names: HashSet<String> = record.iter().map(|t| crate::db::sort_key(&t.artist)).collect();
            if names.len() < COMPILATION_ARTISTS {
                widened.push(track);
                continue;
            }
            running_order(&mut record);
            for t in &record {
                self.compiled.insert(t.id);
                if crate::db::sort_key(crate::db::grouping_artist(t)) != own {
                    self.guests.insert(t.id);
                }
            }
            done.insert(key);
            widened.extend(record);
        }
        self.tracks = widened;
        Ok(())
    }

    /// Every compilation in the library — a title without an album artist,
    /// in one folder, by at least [`COMPILATION_ARTISTS`] artists, as
    /// `widen_compilations` reads one — each in its running order, the
    /// records by title.
    pub(in crate::gui) fn compilation_records(&self) -> Result<Vec<Vec<Track>>> {
        use std::collections::{BTreeMap, HashSet};
        let mut records = Vec::new();
        for key in self.db.compilation_album_keys(COMPILATION_ARTISTS)? {
            let mut folders: BTreeMap<Option<std::path::PathBuf>, Vec<Track>> = BTreeMap::new();
            for track in self.db.get_tracks(None, Some(&key), SortField::Artist, SortOrder::Ascending, None)? {
                if track.album_artist.trim().is_empty() {
                    folders.entry(track.path.parent().map(std::path::Path::to_path_buf)).or_default().push(track);
                }
            }
            for (_, mut record) in folders {
                let names: HashSet<String> = record.iter().map(|t| crate::db::sort_key(&t.artist)).collect();
                if names.len() >= COMPILATION_ARTISTS {
                    running_order(&mut record);
                    records.push(record);
                }
            }
        }
        Ok(records)
    }

    /// The „compilations" row: its records in the album column, and their
    /// tracks — all of them, or the one record picked — in the track list,
    /// each record under one heading.
    fn reload_compilations(&mut self) -> Result<()> {
        let records = &self.compilations;
        self.albums = records
            .iter()
            .filter_map(|record| {
                let first = record.first()?;
                Some(Album {
                    name: first.album.clone(),
                    artist: tr!("verschiedene").to_string(),
                    year: record.iter().find_map(|t| t.year),
                    added: record
                        .iter()
                        .map(|t| t.date_added)
                        .max()
                        .map(|at| at.format("%Y-%m-%d %H:%M:%S").to_string())
                        .unwrap_or_default(),
                    track_count: record.len() as u32,
                    total_duration_secs: record.iter().map(|t| t.duration_secs).sum(),
                    has_artwork: record.iter().any(|t| t.has_artwork),
                })
            })
            .collect();
        if self.album_index > self.albums.len() {
            self.album_index = 0;
        }
        self.tracks = match self.album_index.checked_sub(1) {
            Some(at) => records.get(at).cloned().unwrap_or_default(),
            None => records.concat(),
        };
        self.compiled = self.tracks.iter().map(|t| t.id).collect();
        self.guests.clear();
        if self.track_index >= self.tracks.len() {
            self.track_index = 0;
        }
        self.reset_track_selection();
        Ok(())
    }

    /// How many tracks „zuletzt" shows now: its length, and whatever
    /// scrolling to its end has fetched since.
    pub(in crate::gui) fn recent_wanted(&self) -> usize {
        crate::config::RECENT_SHOWN + self.recent_more
    }

    /// Whether scrolling to the end of „zuletzt" would find more: it is the
    /// list showing, no record in it is chosen, and the last fetch was cut
    /// off rather than the end of the library.
    pub(in crate::gui) fn more_recent_to_fetch(&self) -> bool {
        self.showing_recent()
            && !self.showing_ipod()
            && self.album_index == 0
            && self.tracks.len() >= self.recent_wanted()
            && self.tracks.len() < self.matching_tracks as usize
    }

    /// The newest records, whole, onto `tracks` until there are about as many
    /// tracks as „zuletzt" is meant to show — from the first record not taken
    /// yet, so fetching more goes on where the list ends.
    ///
    /// The count is reached by records rather than cut at a track: the last
    /// one would otherwise stop half-way through, under a heading that says
    /// it has twelve.
    fn take_recent_records(&mut self, tracks: &mut Vec<Track>, query: Option<&str>) -> Result<()> {
        let wanted = self.recent_wanted();
        while tracks.len() < wanted {
            let Some(album) = self.recent_albums.get(self.recent_taken) else { break };
            tracks.extend(self.db.get_tracks(
                Some(&album.artist),
                Some(&album.name),
                SortField::Artist,
                SortOrder::Ascending,
                query,
            )?);
            self.recent_taken += 1;
        }
        Ok(())
    }

    /// „zuletzt" scrolled to its end: the next tracks onto the list, and
    /// nothing else asked again. Reading everything anew — every record of
    /// the library with its name and date, twice — took a sixteenth of a
    /// second with a large library, and the scrolling stood still for it each
    /// time. What is chosen stays chosen: the rows already there have not
    /// moved.
    pub(in crate::gui) fn fetch_more_recent(&mut self) -> Result<()> {
        self.recent_more += crate::config::RECENT_SHOWN;
        let query = self.query();
        let query = query.as_deref();
        if self.merged() {
            let mut tracks = std::mem::take(&mut self.tracks);
            let taken = self.take_recent_records(&mut tracks, query);
            self.tracks = tracks;
            taken
        } else {
            self.tracks = self.db.get_recent_tracks(query, self.recent_wanted())?;
            Ok(())
        }
    }

    /// Write the current settings back to the config file.
    pub fn save_config(&mut self) {
        if let Err(e) = self.config.save() {
            self.set_status(tr!("einstellungen nicht gespeichert: {}", e));
        }
    }
}

/// The most frames a second the window draws when no rate is chosen in the
/// menu — see `hold_the_frame_rate`.
const FRAME_CEILING: u32 = 125;
/// The same while the window is in the back.
const FRAME_CEILING_BEHIND: u32 = 60;

impl GuiApp {
    /// Which view and which list the window shows, for the measurement:
    /// the music, the radio, or the podcasts with the list in their
    /// second column and how far it has been scrolled on.
    fn view_measured(&self) -> String {
        let p = &self.podcasts;
        if p.open {
            let list = if p.found.is_some() {
                "gefunden".to_string()
            } else if p.only_fetched {
                "geladen".to_string()
            } else if p.only_starred {
                "gemerkt".to_string()
            } else if p.only_open {
                "offen".to_string()
            } else if let Some(show) = &p.chosen {
                format!("podcast {}", show.title)
            } else {
                "alle".to_string()
            };
            let more = if p.more > 0 { format!(" · {} mehr", p.more) } else { String::new() };
            format!("podcasts · {list}{more} · {} feeds gehalten", p.feeds.len())
        } else if self.showing_radio() {
            "radio".to_string()
        } else {
            "musik".to_string()
        }
    }

    /// What is going on, for the measurement's line.
    fn doing(&self, ctx: &egui::Context) -> measure::Doing {
        let (focused, unseen) = ctx.input(|i| {
            let viewport = i.viewport();
            (i.focused, viewport.occluded == Some(true) || viewport.minimized == Some(true))
        });
        measure::Doing {
            playing: self.player.state() == crate::player::PlaybackState::Playing,
            focused,
            unseen,
            field_scope: self.config.ui.field_spectrum,
            menu: self.menu_open,
            covers_waiting: self.cover_cache.waiting() > 0,
            podcasts: self.podcasts.open,
            feeds: !self.podcasts.reading.is_empty(),
            fetching: self.podcasts.fetching.is_some(),
            input: ctx.input(|i| !i.events.is_empty() || i.pointer.is_moving()),
        }
    }

    /// Wait, if this frame comes sooner after the last than the frame rate in
    /// the menu allows. Whatever the pointer did meanwhile is not lost: the
    /// system holds it and the next frame takes it all at once, so a mouse
    /// moved across a 120 Hz screen costs half the frames and nobody sees the
    /// difference. Frames the program asks for itself, the scope's twenty and
    /// the clock's four, are slower than this anyway and never wait.
    ///
    /// Without a rate chosen there is still a ceiling, above every screen's
    /// refresh, so it costs nothing while the screen paces the frames. It is
    /// for when the screen does not: a window in the back, on another desktop
    /// or covered, is not held to the refresh by every system, and an
    /// animation asking for the next frame was then drawn some two hundred
    /// times a second, the processor at sixty percent and the memory growing.
    /// In the back it is held lower again: nobody is looking closely.
    fn hold_the_frame_rate(&mut self, ctx: &egui::Context) {
        let chosen = self.config.ui.frame_cap;
        let cap = if chosen > 0 { chosen } else { FRAME_CEILING };
        let cap = if ctx.input(|i| i.focused) { cap } else { cap.min(FRAME_CEILING_BEHIND) };
        if let Some(last) = self.frame_started {
            let gap = std::time::Duration::from_secs_f64(1.0 / f64::from(cap))
                .saturating_sub(std::time::Duration::from_millis(2));
            let since = last.elapsed();
            if since < gap {
                std::thread::sleep(gap - since);
            }
        }
        self.frame_started = Some(std::time::Instant::now());
    }

    /// The answer to the question asked at the start, once it comes: a newer
    /// version said at the foot, once, where to find it. Nothing for the same
    /// or for no answer — a start is not the moment for „not reachable".
    fn say_a_newer_version(&mut self, ctx: &egui::Context) {
        if self.update_told || !self.config.ui.update_on_start {
            return;
        }
        let Some(check) = self.update.as_mut() else { return };
        if check.tend() {
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
            return;
        }
        self.update_told = true;
        if let Some(update::Answer::Newer { version, .. }) = check.answer.clone() {
            self.set_notice(tr!("kessler {} ist da — menü → allgemein → neue version", version));
        }
    }

    /// One step of `logic` done, for the measurement — see [`measure::Measure::lap`].
    fn lap(&mut self, what: &'static str) {
        if let Some(measure) = self.measure.as_mut() {
            measure.lap(what);
        }
    }

    /// One part of the drawing done, for the measurement — see
    /// [`measure::Measure::drawn`].
    fn drawn_part(&mut self, what: &'static str) {
        if let Some(measure) = self.measure.as_mut() {
            measure.drawn(what);
        }
    }

    /// What was pressed this frame, for the measurement's line about a slow
    /// one: the keys by name with what was held, and the pointer's buttons.
    fn note_pressed(&mut self, ctx: &egui::Context) {
        let Some(measure) = self.measure.as_mut() else { return };
        let said: Vec<String> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Key { key, pressed: true, modifiers, .. } => {
                        let held = [
                            (modifiers.command, "cmd+"),
                            (modifiers.ctrl && !modifiers.command, "ctrl+"),
                            (modifiers.alt, "alt+"),
                            (modifiers.shift, "shift+"),
                        ];
                        let held: String = held.iter().filter(|(on, _)| *on).map(|(_, w)| *w).collect();
                        Some(format!("{held}{}", key.name()))
                    }
                    egui::Event::PointerButton { pressed: true, button, .. } => Some(format!("{button:?}").to_lowercase()),
                    egui::Event::MouseWheel { .. } => Some("rad".to_string()),
                    _ => None,
                })
                .collect()
        });
        if !said.is_empty() {
            let mut said = said;
            said.dedup();
            measure.pressed(said.join(" "));
        }
    }
}

/// A mouse wheel's notch is spread over a few frames by egui, and what is
/// left of it went to whatever the pointer was over by then: scrolled in the
/// queue and moved straight on to the episode's page, that page went one
/// notch down as well. What is left of a turn goes only where the wheel was
/// turned — once the pointer has moved off from there, the rest is dropped.
/// A trackpad's scrolling is not spread out, and is not touched.
fn keep_the_wheel_where_it_turned(ctx: &egui::Context) {
    /// How far the pointer may drift while the wheel turns before what is
    /// left of the turn counts as meant for where it was.
    const DRIFT: f32 = 24.0;
    let id = egui::Id::unique("wheel-turned-at");
    let (turned, pointer) = ctx.input(|i| (i.events.iter().any(|e| matches!(e, egui::Event::MouseWheel { .. })), i.pointer.latest_pos()));
    if turned {
        if let Some(at) = pointer {
            ctx.data_mut(|d| d.insert_temp(id, at));
        }
        return;
    }
    let Some(at) = ctx.data(|d| d.get_temp::<egui::Pos2>(id)) else { return };
    if pointer.is_some_and(|now| now.distance(at) > DRIFT) {
        ctx.input_mut(|i| i.smooth_scroll_delta = egui::Vec2::ZERO);
    }
}

impl eframe::App for GuiApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        let c = self.palette.ground;
        [
            c.r() as f32 / 255.0,
            c.g() as f32 / 255.0,
            c.b() as f32 / 255.0,
            1.0,
        ]
    }

    /// A trackpad's and a TrackPoint's own distances, scaled before egui
    /// reads them — see [`library::touch_factor`]. The wheel's lines are
    /// scaled by egui itself (`line_scroll_speed`).
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        let factor = library::touch_factor(&self.config.ui.scroll_speed, cfg!(target_os = "macos"));
        if factor == 1.0 {
            return;
        }
        for event in &mut raw.events {
            if let egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta, .. } = event {
                *delta *= factor;
            }
        }
    }

    /// The keyboard's screen wiped on the way out, so it does not go on showing
    /// a song that stopped with the program.
    #[cfg(not(target_os = "macos"))]
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.leave();
    }

    /// The same where the window is drawn through wgpu, which hands over no
    /// OpenGL context.
    #[cfg(target_os = "macos")]
    fn on_exit(&mut self) {
        self.leave();
    }

    /// Input and playback state, before any drawing happens.
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.hold_the_frame_rate(ctx);
        keep_the_wheel_where_it_turned(ctx);
        self.say_a_newer_version(ctx);
        crate::media::set_notify(self.config.ui.notify, ctx.input(|i| i.viewport().focused.unwrap_or(true)));
        #[cfg(target_os = "linux")]
        crate::ipod::udisks::AUTO.store(self.config.ipod_mount, std::sync::atomic::Ordering::Relaxed);
        macwindow::own_buttons(self.config.ui.window_buttons != "system");
        let placed = macwindow::apply(frame);
        theme::set_button_marks(placed.map(|p| {
            let zoom = ctx.zoom_factor().max(0.1);
            (p.middles.map(|x| x / zoom), p.across / zoom)
        }));
        if let Some(placed) = placed {
            let zoom = ctx.zoom_factor().max(0.1);
            theme::set_titlebar(placed.band / zoom);
            theme::set_buttons_end(placed.buttons_end / zoom);
        }
        if let Some(measure) = self.measure.as_mut() {
            measure::watch_repaints(ctx);
            measure.begin();
        }
        if self.measure.is_some() {
            let view = self.view_measured();
            if let Some(measure) = self.measure.as_mut() {
                for note in crate::player::watch::take_notes() {
                    measure.noted(&note);
                }
                measure.view_is(&view);
            }
        }
        self.put_away_on_close(ctx);
        self.peek_at_the_queue();
        self.lap("anfang");
        self.tend_the_reel(ctx);
        self.follow_system(ctx);
        self.lap("farben");
        if !self.media_started {
            self.media_started = true;
            crate::media::start();
            crate::media::route_start();
            #[cfg(target_os = "linux")]
            portal::follow_accent(ctx.clone());
            #[cfg(target_os = "macos")]
            macaccent::follow(ctx.clone());
        }
        self.lap("medien-start");
        let seen = ctx.input(|i| {
            let v = i.viewport();
            !v.minimized.unwrap_or(false) && !v.occluded.unwrap_or(false)
        });
        let drawn = (self.config.ui.field_spectrum || self.saver.on) && seen;
        let playing = self.player.state() == crate::player::PlaybackState::Playing;
        if drawn && playing {
            self.spectrum.hear(&self.heard.recent());
        } else {
            self.spectrum.hear(&[]);
        }
        columns::set_numbers_right(self.config.ui.numbers_right);
        columns::set_small_length(self.config.ui.small_length);
        self.lap("spektrum");
        self.take_media_commands(ctx);
        self.lap("medientasten");
        {
            let mode = menubar::Mode::from_name(&self.config.ui.menubar);
            let playing = self.player.state() == crate::player::PlaybackState::Playing;
            let words = self.playing_words();
            let title = words
                .as_ref()
                .map(|[title, artist, _]| {
                    if artist.is_empty() { title.clone() } else { format!("{title} — {artist}") }
                })
                .unwrap_or_default();
            let light = match self.config.ui.menubar_ink.as_str() {
                "hell" => true,
                "dunkel" => false,
                _ => self.desktop_theme.or(ctx.system_theme()) != Some(egui::Theme::Light),
            };
            let volume = self.player.volume();
            let about = words
                .map(|words| words.into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n"))
                .unwrap_or_default();
            let on_air = self.on_air.is_some();
            if cfg!(target_os = "macos") && mode != menubar::Mode::Off {
                let ui = &self.config.ui;
                let screen = ui.menubar_cover_screened.then(|| menubar::CoverScreen {
                    look: dither::Look { tones: dither::Look::of(ui).tones.min(3), ..dither::Look::of(ui) },
                    fineness: ui.picture_fineness(),
                    ground: self.palette.field,
                    ink: self.palette.ink,
                    third: self.palette.third_tone(self.palette.ink),
                    negative: self.palette.screens_negative(),
                });
                let path = self.now_playing_info().and_then(|now| now.artwork);
                let at = menubar::CoverAt::from_name(&ui.menubar_cover);
                self.menu_bar.cover(menubar::CoverWish { path, at, screen });
            }
            let sleep = if self.playing_episode() { self.sleep_said() } else { String::new() };
            self.menu_bar.sync(ctx, mode, light, playing, &title, &about, volume, on_air, &sleep);
        }
        self.lap("menüleiste");
        self.take_dropped_files(ctx);
        self.poll_adding();
        self.poll_transfer();
        self.poll_music_picker();
        self.poll_picker();
        self.lap("abholen");
        self.poll_station_picture();
        self.lap("senderbild");
        self.poll_cover_file();
        self.poll_cover_pick();
        self.lap("cover abholen");
        self.poll_artist();
        self.lap("künstler:in abholen");
        self.poll_lyrics();
        self.lap("liedtext abholen");
        self.poll_artist_export();
        self.poll_artist_hunt();
        self.poll_gaps();
        self.lap("abholen");
        self.lap("tastatur");
        self.poll_tag_hunt();
        self.poll_connecting();
        self.poll_measuring();
        self.poll_removal();
        self.lap("abholen");
        if !self.menu_open {
            self.lastfm_keys = None;
        }
        self.report_listening();
        self.lap("last.fm");
        self.remember_queue();
        self.remember_window(ctx);
        self.lap("merken");
        self.poll_rescan();
        self.lap("einlesen");

        if self.config.ui.ipod {
            self.watch_for_devices();
            if let Err(e) = self.poll_devices() {
                self.set_status(format!("{e}"));
            }
        } else if self.device_watch.is_some() || self.attached.is_some() {
            self.give_up_devices();
        }
        menu::note_features(&self.config.ui);
        theme::set_capitals(self.config.ui.capitals);
        self.fall_back_from_music();
        self.lap("ipod");

        saver::watch(self, ctx);
        if self.untouched
            && ctx.input(|i| {
                i.pointer.any_pressed() || i.events.iter().any(|e| matches!(e, egui::Event::Key { pressed: true, .. }))
            })
        {
            self.untouched = false;
        }
        self.note_pressed(ctx);
        if let Err(e) = self.handle_keys(ctx) {
            self.set_status(format!("{e}"));
        }
        self.lap("tasten");
        let sounding = (self.player.state() == crate::player::PlaybackState::Playing).then(|| {
            if self.on_air.is_some() {
                Some(listened::Source::Radio)
            } else if self.playing_episode() {
                Some(listened::Source::Podcasts)
            } else {
                self.player.library_track().map(|_| listened::Source::Music)
            }
        });
        self.listened.tick(sounding.flatten());

        self.tend_the_radio(ctx);
        self.tend_podcasts(ctx);
        match self.player.tick() {
            Ok(true) => {
                if self.player.state() == crate::player::PlaybackState::Playing {
                    if let Some(track) = self.player.library_track() {
                        let _ = self.db.record_play(track.id);
                    }
                }
            }
            Ok(false) => {}
            Err(e) if self.on_air.is_some() => {
                let name = self.on_air.as_ref().map(|s| s.name.clone()).unwrap_or_default();
                self.note(log::Kind::Radio, format!("{name}: {e}"));
                self.set_status(tr!("radio: {} ist gerade nicht zu erreichen", name));
            }
            Err(e) => self.set_status(format!("{e}")),
        }
        self.lap("player");

        self.note_what_is_gone();

        let playing = self.now_playing_info();
        self.media.report(playing);
        self.lap("medien-melden");

        if self.player.state() == crate::player::PlaybackState::Playing && self.player.current_track().is_some() {
            let in_front = ctx.input(|i| i.focused) && self.config.ui.scope_rate != library::SECOND_RATE && !library::on_the_second(self);
            let step = if self.saver.on {
                let to_end = self.player.duration().saturating_sub(self.player.position());
                to_end.clamp(std::time::Duration::from_millis(20), std::time::Duration::from_secs(5))
            } else if in_front {
                library::to_tick(ctx, 0.25)
            } else {
                let position = self.player.position();
                let to_second = std::time::Duration::from_millis(
                    1000 - u64::from(position.subsec_millis()) + 40,
                );
                let to_end = self.player.duration().saturating_sub(position);
                if to_end.is_zero() { to_second } else { to_second.min(to_end) }
            };
            ctx.request_repaint_after(step);
        }
        if let Some(due) = self.player.rest_due_in() {
            ctx.request_repaint_after(due);
        }

        if self.is_scanning()
            || self.is_picking()
            || self.is_connecting()
            || self.picking_cover.is_some()
            || self.adding.is_some()
            || self.adding_pick.is_some()
            || self.cover_pick.as_ref().is_some_and(|p| p.is_busy())
            || matches!(
                self.transfer.as_ref().map(|t| &t.stage),
                Some(Stage::Going { .. })
            )
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
        }
        self.lap("rest");
        if let Some(measure) = self.measure.as_mut() {
            measure.logic_done();
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let warm_for = (ui.ctx().pixels_per_point().to_bits(), self.config.ui.typeface.clone(), self.config.ui.capitals);
        if self.prewarmed.as_ref() != Some(&warm_for) {
            text::prewarm(ui.ctx(), self.config.ui.capitals);
            self.prewarmed = Some(warm_for);
        }
        if self.drawn_with.is_empty() {
            self.drawn_with = drawn_with(frame);
        }
        covers::set_outline(covers::outline_colour(&self.config.ui.picture_outline, &self.palette));
        let full = ui.available_rect_before_wrap();
        ui.painter().rect_filled(full, 0.0, self.palette.ground);
        library::render(self, ui);
        self.drawn_part("schoner und überblendung");
        if self.measure.is_some() {
            let doing = self.doing(ui.ctx());
            let asked = ui.ctx().repaint_causes();
            let textures = measure::textures_said(ui.ctx());
            if let Some(measure) = self.measure.as_mut() {
                measure.textures = textures;
                measure.frame(doing, &asked);
            }
        }
    }
}

/// The application icon, for a window that has no bundle behind it.
///
/// eframe sets an icon on every start — on macOS by calling
/// `NSApplication.setApplicationIconImage` — and it substitutes egui's own
/// default when the program says nothing. So a build run straight from a
/// terminal has to hand one over or it sits in the Dock as egui.
///
/// Inside a bundle it is the other way round, which is the bug this comment
/// now exists for: the Finder showed the new layered icon and the Dock
/// switched to this flat one the moment the window opened. A bundle's own icon
/// is the better one — macOS 26 takes the layered `.icon` out of `Assets.car`
/// and lights it itself, and no picture handed over at runtime can be that —
/// so there the window says nothing at all. See [`run`].
///
/// Embedded rather than read from disk because an icon that goes missing when
/// the application is moved is worse than no icon.
///
/// The black and white k, in the desktop's shape. It stands only until the
/// window has drawn its own in its colours — see [`appicon`].
fn app_icon() -> Option<egui::IconData> {
    const MAC: &[u8] = include_bytes!("../../bundle/icon.iconset/icon_256x256@2x.png");
    const GNOME: &[u8] = include_bytes!("../../bundle/icon-gnome/icon_256x256@2x.png");
    const KDE: &[u8] = include_bytes!("../../bundle/icon-kde/icon_256x256@2x.png");
    let picture = match appicon::Shape::here() {
        appicon::Shape::Mac => MAC,
        appicon::Shape::Gnome => GNOME,
        appicon::Shape::Kde => KDE,
    };
    let decoded = image::load_from_memory(picture)
        .inspect_err(|e| tracing::warn!("the application icon did not decode: {e}"))
        .ok()?
        .to_rgba8();
    Some(egui::IconData {
        width: decoded.width(),
        height: decoded.height(),
        rgba: decoded.into_raw(),
    })
}

/// What the Mac is set to, from the same setting System Settings writes.
/// Only asked outside a bundle, and only once.
#[cfg(target_os = "macos")]
fn system_theme() -> Option<egui::Theme> {
    let answer = std::process::Command::new("defaults")
        .args(["read", "-g", "AppleInterfaceStyle"])
        .output()
        .ok()?;
    Some(if String::from_utf8_lossy(&answer.stdout).trim() == "Dark" {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    })
}

/// What the desktop is set to: the portal's answer where there is one —
/// see `portal` — and otherwise GNOME's own, whose `color-scheme` is
/// „prefer-dark" in dark mode and „default" (or „prefer-light") otherwise.
/// Neither, no answer.
#[cfg(target_os = "linux")]
fn system_theme() -> Option<egui::Theme> {
    if let Some(theme) = portal::read() {
        return Some(theme);
    }
    let answer = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "color-scheme"])
        .output()
        .ok()?;
    answer
        .status
        .success()
        .then(|| scheme_from(&String::from_utf8_lossy(&answer.stdout)))
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn system_theme() -> Option<egui::Theme> {
    None
}

/// A line of `gsettings` about `color-scheme`, as light or dark — either the
/// bare value `get` prints or the `color-scheme: 'prefer-dark'` `monitor`
/// does.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn scheme_from(line: &str) -> egui::Theme {
    if line.contains("prefer-dark") {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    }
}

/// Open the window.
pub fn run(config: Config) -> Result<()> {
    let remembered = config.ui.window.filter(|[x, y, w, h]| {
        w.is_finite() && h.is_finite() && *w >= 360.0 && *h >= 420.0 && x.is_finite() && y.is_finite()
    });
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size(
            remembered
                .map(|[_, _, w, h]| [w, h])
                .unwrap_or([1440.0, 900.0]),
        )
        .with_min_inner_size([360.0, 420.0])
        .with_title("kessler");
    if let Some([x, y, _, _]) = remembered {
        viewport = viewport.with_position([x, y]);
    }
    #[cfg(target_os = "linux")]
    {
        viewport = viewport.with_app_id("kessler");
    }

    #[cfg(target_os = "macos")]
    {
        viewport = viewport
            .with_fullsize_content_view(true)
            .with_titlebar_shown(false)
            .with_title_shown(false);
    }
    #[cfg(target_os = "linux")]
    {
        viewport = viewport.with_decorations(config.ui.window_bar != "ohne");
    }
    let icon = if crate::launched_from_a_bundle() {
        egui::IconData::default()
    } else {
        app_icon().unwrap_or_default()
    };
    viewport = viewport.with_icon(icon);

    let options = eframe::NativeOptions {
        viewport,
        renderer: renderer_wanted(),
        wgpu_options: wgpu_wanted(),
        ..Default::default()
    };

    eframe::run_native(
        "kessler",
        options,
        Box::new(|cc| {
            cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
            cc.egui_ctx.set_zoom_factor(crate::gui::menu::zoom_of(config.ui.zoom));
            let app = GuiApp::new(config, &cc.egui_ctx)
                .map_err(|e| Box::<dyn std::error::Error + Send + Sync>::from(e.to_string()))?;
            Ok(Box::new(app))
        }),
    )
    .map_err(|e| crate::error::KesslerError::Ui(e.to_string()))
}

/// What the window is drawn through. On a Mac only wgpu is built in, and
/// draws into Metal. Elsewhere OpenGL, unless `KESSLER_GRAFIK` asks for
/// wgpu (`wgpu` or `vulkan`) — to compare the two on one machine.
fn renderer_wanted() -> eframe::Renderer {
    #[cfg(target_os = "macos")]
    {
        eframe::Renderer::Wgpu
    }
    #[cfg(not(target_os = "macos"))]
    {
        match std::env::var("KESSLER_GRAFIK").unwrap_or_default().to_ascii_lowercase().as_str() {
            "wgpu" | "vulkan" => eframe::Renderer::Wgpu,
            _ => eframe::Renderer::Glow,
        }
    }
}

/// How wgpu presents. On a Mac one frame waits for the screen at most, not
/// two: what the hand does — a row lighting up, a list scrolled — is on the
/// screen a refresh sooner, some eight milliseconds at 120 Hz. The window
/// draws little on the graphics card, so the throughput two frames would
/// buy it is not needed. Resizing takes two all the same (egui's own rule).
/// Elsewhere wgpu's default.
fn wgpu_wanted() -> eframe::egui_wgpu::WgpuConfiguration {
    let mut wanted = eframe::egui_wgpu::WgpuConfiguration::default();
    if cfg!(target_os = "macos") {
        wanted.surface = eframe::egui_wgpu::SurfaceConfig::LOW_LATENCY;
    }
    wanted
}

/// How it actually is drawn, for „über": `metal` on a Mac, `wgpu · vulkan`
/// (or `wgpu · gl`, where there is no Vulkan) when asked for, `opengl`
/// where the window is drawn through glow.
fn drawn_with(frame: &eframe::Frame) -> String {
    match frame.wgpu_render_state() {
        Some(state) => match format!("{:?}", state.adapter.get_info().backend).to_lowercase() {
            metal if metal == "metal" => metal,
            other => format!("wgpu · {other}"),
        },
        None => "opengl".to_string(),
    }
}

/// What was typed this frame, and what was pasted.
///
/// egui hands a paste over as an event of its own and not as text, so a
/// field that read only the text never saw ⌘V — the tag editor, the search
/// line and the artist's note all ignored it until the last.fm key, which
/// nobody types by hand, made it obvious. Control characters are dropped; a
/// pasted line break stays one where the field has lines (`lines`), and is a
/// space everywhere else.
pub(crate) fn typed(ctx: &egui::Context, lines: bool) -> String {
    ctx.input(|i| {
        let mut out = String::new();
        for event in &i.events {
            match event {
                egui::Event::Text(t) => out.extend(t.chars().filter(|c| !c.is_control())),
                egui::Event::Paste(t) => out.push_str(&pasted(t, lines)),
                _ => {}
            }
        }
        out
    })
}

/// Whether cmd+c or cmd+x arrived this frame: (copy, cut). Read as events,
/// not as keys, because that is how the window system hands them over.
pub(crate) fn copy_or_cut(ctx: &egui::Context) -> (bool, bool) {
    ctx.input(|i| {
        let copy = i.events.iter().any(|e| matches!(e, egui::Event::Copy));
        let cut = i.events.iter().any(|e| matches!(e, egui::Event::Cut));
        (copy, cut)
    })
}

/// A paste as a field takes it — see [`typed`].
fn pasted(text: &str, lines: bool) -> String {
    let text = text.replace("\r\n", "\n");
    let text = if lines { text.as_str() } else { text.trim() };
    text.chars()
        .filter_map(|c| match c {
            '\n' if lines => Some('\n'),
            '\n' | '\t' | '\r' => Some(' '),
            c if c.is_control() => None,
            c => Some(c),
        })
        .collect()
}

#[cfg(test)]
mod paste_tests {
    use super::pasted;

    #[test]
    fn a_paste_keeps_its_lines_only_where_the_field_has_them() {
        assert_eq!(pasted("  eins\r\nzwei\t\n", false), "eins zwei");
        assert_eq!(pasted("eins\r\nzwei\u{7}", true), "eins\nzwei");
    }
}

#[cfg(test)]
mod row_tests {
    use super::*;

    #[test]
    fn the_names_start_after_the_pinned_rows() {
        assert_eq!(ArtistRow::at(0), ArtistRow::All);
        assert_eq!(ArtistRow::at(1), ArtistRow::Recent);
        assert_eq!(ArtistRow::at(2), ArtistRow::Compilations);
        assert_eq!(ArtistRow::Compilations.index(), 2);
        assert_eq!(ArtistRow::at(3), ArtistRow::Named(0));
        assert_eq!(ArtistRow::at(10), ArtistRow::Named(7));
        for index in 0..12 {
            assert_eq!(ArtistRow::at(index).index(), index);
        }
    }

    /// The column's length and its layout are the same arithmetic.
    ///
    /// The keyboard learns where the column stops from `column_len`, and what
    /// is at each row from `ArtistRow::at`. While those two disagreed the
    /// last rows of the column could be clicked and not walked to, and
    /// nothing on screen said why.
    #[test]
    fn the_column_ends_where_its_last_row_is() {
        for names in 1..6 {
            let rows = artist_rows(names);
            assert_eq!(ArtistRow::at(rows - 1), ArtistRow::Named(names - 1));
        }
        assert_eq!(artist_rows(0), PINNED.len());
    }

    /// The bug: clicking the track that is playing opened the
    /// wrong artist, and which one it was depended on where in the alphabet
    /// the right one sat — so it read as random.
    #[test]
    fn the_playing_artist_lands_on_its_own_row() {
        let names = ["aphex twin", "autechre", "boards of canada"];
        let key = crate::db::sort_key("boards of canada");

        let row = artist_row_for(&names, &key);
        assert_eq!(row, ArtistRow::Named(2));
        assert_eq!(row.index(), 2 + PINNED.len(), "the names begin below the pinned rows");
        assert_ne!(row.index(), 3, "adding one lands on autechre");

        assert_eq!(artist_row_for(&names, "niemand"), ArtistRow::All);
        assert_eq!(artist_row_for(&[], &key), ArtistRow::All);
    }

    /// Another: clicking the line that says what is playing, in the
    /// grid, should land on that record's cover. The grid has no artist column
    /// to narrow the wall, so the title alone was picking whoever came first.
    #[test]
    fn the_playing_record_is_found_by_both_its_keys() {
        let wall = |records: &[(&str, &str)]| -> Vec<Album> {
            records
                .iter()
                .map(|(artist, name)| Album {
                    name: (*name).to_string(),
                    artist: (*artist).to_string(),
                    year: None,
                    added: String::new(),
                    track_count: 1,
                    total_duration_secs: 0.0,
                    has_artwork: false,
                })
                .collect()
        };

        let albums = wall(&[
            ("Cher", "Greatest Hits"),
            ("Cher", ""),
            ("Slowdive", "Greatest Hits"),
            ("Slowdive", ""),
        ]);
        let key = crate::db::sort_key;

        assert_eq!(
            album_row_for(&albums, &key("Greatest Hits"), &key("Slowdive")),
            Some(2)
        );
        assert_eq!(album_row_for(&albums, "", &key("Slowdive")), Some(3));

        assert_eq!(
            album_row_for(&albums, &key("greatest hits"), &key("The Cher")),
            Some(0)
        );
        assert_eq!(album_row_for(&albums, &key("nothing"), &key("Cher")), None);
    }

    /// The bar with the three buttons is the operating system's, and it has to
    /// be told which way round the window is — every scheme, not just the two
    /// the window opens on.
    #[test]
    fn every_scheme_says_which_way_round_the_frame_is() {
        use egui::SystemTheme;
        for theme in Theme::all() {
            let palette = Brightness::System.resolve(theme, None).palette();
            assert_eq!(
                frame_theme_for(Brightness::System, &palette),
                SystemTheme::SystemDefault,
                "{} pinned the frame instead of letting it follow",
                theme.name()
            );
            for fixed in [Brightness::Hell, Brightness::Dunkel] {
                let palette = fixed.resolve(theme, None).palette();
                let expected = if palette.is_dark() {
                    SystemTheme::Dark
                } else {
                    SystemTheme::Light
                };
                assert_eq!(frame_theme_for(fixed, &palette), expected, "{} dressed the frame wrong", theme.name());
            }
        }
    }

    /// And the two that name their own darkness really do come out that way,
    /// so the test above is checking something.
    #[test]
    fn the_named_schemes_are_the_darkness_they_claim() {
        use egui::SystemTheme;
        let dark = |t: Theme| frame_theme_for(Brightness::Hell, &t.palette());
        assert_eq!(dark(Theme::MacosDunkel), SystemTheme::Dark);
        assert_eq!(dark(Theme::Dracula), SystemTheme::Dark);
        assert_eq!(dark(Theme::MacosHell), SystemTheme::Light);
        assert_eq!(dark(Theme::SolarisiertHell), SystemTheme::Light);
    }

    #[test]
    fn the_note_names_the_format_and_stops() {
        assert_eq!(foreign_note(&[]), None);
        assert_eq!(
            foreign_note(&[("m4a".into(), 412)]).unwrap(),
            "412 dateien übergangen: m4a"
        );
        assert_eq!(
            foreign_note(&[("m4a".into(), 412), ("aiff".into(), 9)]).unwrap(),
            "421 dateien übergangen: m4a, aiff"
        );
        assert_eq!(
            foreign_note(&[
                ("m4a".into(), 412),
                ("aiff".into(), 9),
                ("wma".into(), 2),
                ("ape".into(), 1),
            ])
            .unwrap(),
            "424 dateien übergangen: m4a, aiff und 2 weitere"
        );
    }

    #[test]
    fn a_row_and_its_index_agree_both_ways() {
        for index in 0..12 {
            assert_eq!(ArtistRow::at(index).index(), index, "index {index}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A record is asked about for its genre only when none of its tracks
    /// says one — one track that does is the record saying it.
    #[test]
    fn only_a_record_with_no_genre_at_all_is_asked_about() {
        let track = |artist: &str, album: &str, genre: &str| Track {
            id: 0,
            path: std::path::PathBuf::new(),
            title: "t".into(),
            artist: artist.into(),
            album_artist: String::new(),
            album: album.into(),
            genre: genre.into(),
            year: None,
            track_number: None,
            disc_number: None,
            duration_secs: 1.0,
            bitrate: None,
            sample_rate: None,
            file_size: 1,
            format: "mp3".into(),
            rating: 0,
            play_count: 0,
            last_played: None,
            date_added: Default::default(),
            date_modified: Default::default(),
            file_hash: None,
            has_artwork: false,
        };
        let tracks = [
            track("Boards of Canada", "Geogaddi", ""),
            track("boards of canada", "geogaddi", "electronic"),
            track("Aphex Twin", "Drukqs", ""),
            track("Aphex Twin", "Drukqs", ""),
            track("Aphex Twin", "", ""),
        ];
        assert_eq!(
            records_without_genre(&tracks),
            vec![("Aphex Twin".to_string(), "Drukqs".to_string())]
        );
    }

    /// The three orders come back as they were written, and anything the
    /// file does not recognise is the shelf's own order.
    #[test]
    fn the_orders_are_kept() {
        for sort in [
            ListSort::Ascending,
            ListSort::Descending,
            ListSort::Added,
            ListSort::Year,
            ListSort::YearBack,
        ] {
            assert_eq!(ListSort::from_key(sort.key(), true), sort);
        }
        assert_eq!(ListSort::from_key("jahr", false), ListSort::Ascending);
        assert_eq!(ListSort::from_key("", true), ListSort::Ascending);

        let key = track_order_key(SortField::Year, SortOrder::Descending);
        assert_eq!(key, "year desc");
        assert_eq!(
            track_order_from_key(&key),
            Some((SortField::Year, SortOrder::Descending))
        );
        assert_eq!(
            track_order_from_key("duration_secs asc"),
            Some((SortField::Duration, SortOrder::Ascending))
        );
        assert_eq!(track_order_from_key("nonsense"), None);
        assert_eq!(track_order_from_key(""), None);
    }

    /// eframe replaces the dock icon at runtime unless it is given one, so a
    /// missing or unreadable file here means the application silently wears
    /// eframe's default. The files are embedded at build time, so this catches
    /// it at build time too.
    #[test]
    fn the_application_has_its_own_icon() {
        let icon = app_icon().expect("the embedded icon did not decode");
        assert_eq!(icon.width, icon.height, "the icon is not square");
        assert!(icon.width >= 256, "the icon is too small for a dock tile");
        assert_eq!(icon.rgba.len(), (icon.width * icon.height * 4) as usize);
    }

    /// How many opaque pixels of a picture are `colour`, and how many are a
    /// colour the icon has no business with: anything that is not a mix of
    /// the beam and the tile's teal — which the beam's edges are.
    fn inks_in(rgba: &[u8], colour: egui::Color32, ground: egui::Color32) -> (usize, usize) {
        let mut ours = 0;
        let mut foreign = 0;
        let (c, g) = (
            [colour.r() as f32, colour.g() as f32, colour.b() as f32],
            [ground.r() as f32, ground.g() as f32, ground.b() as f32],
        );
        for p in rgba.chunks(4) {
            if p[3] < 250 {
                continue;
            }
            let px = [p[0] as f32, p[1] as f32, p[2] as f32];
            let near = (0..3).map(|i| (px[i] - c[i]).abs()).sum::<f32>();
            let along: Vec<f32> = (0..3).map(|i| c[i] - g[i]).collect();
            let length = along.iter().map(|a| a * a).sum::<f32>();
            let t = ((0..3).map(|i| (px[i] - g[i]) * along[i]).sum::<f32>() / length).clamp(0.0, 1.0);
            let off = (0..3).map(|i| (px[i] - (g[i] + along[i] * t)).abs()).sum::<f32>();
            if near < 6.0 {
                ours += 1;
            } else if off > 40.0 {
                foreign += 1;
            }
        }
        (ours, foreign)
    }

    /// Both ways `gsettings` says GNOME's scheme are read the same.
    #[test]
    fn gnome_says_light_or_dark() {
        assert_eq!(scheme_from("'prefer-dark'\n"), egui::Theme::Dark);
        assert_eq!(scheme_from("color-scheme: 'prefer-dark'"), egui::Theme::Dark);
        assert_eq!(scheme_from("'default'"), egui::Theme::Light);
        assert_eq!(scheme_from("color-scheme: 'prefer-light'"), egui::Theme::Light);
    }

    /// The icon that ships is the white k on black, in every shape: drawn
    /// by `bundle/make-icon.py`, in another language and only when somebody
    /// runs it, so what shipped is decoded and looked at. The window dresses
    /// it in its own colours once it runs — see [`appicon`].
    #[test]
    fn the_icon_is_a_white_k_on_black() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("bundle");
        let (white, black) = (egui::Color32::WHITE, egui::Color32::BLACK);
        for name in [
            "icon.iconset/icon_512x512@2x.png",
            "icon-gnome/icon_512x512@2x.png",
            "icon-kde/icon_512x512@2x.png",
        ] {
            let path = root.join(name);
            let picture = image::open(&path)
                .unwrap_or_else(|e| panic!("{} did not open: {e}", path.display()))
                .to_rgba8();
            assert_eq!((picture.width(), picture.height()), (1024, 1024), "{name}");
            let (ours, foreign) = inks_in(picture.as_raw(), white, black);
            assert!(ours > 20_000, "{name}: only {ours} pixels of the k");
            assert_eq!(foreign, 0, "{name}: a colour that is not the k's or the ground's");
        }
    }

    /// The layered icon is the k alone, standing on nothing — the ground
    /// is Icon Composer's to draw — and one look for light and dark.
    #[test]
    fn the_layered_icon_is_the_k_alone() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("bundle/kessler.icon");
        let path = root.join("Assets/Image.png");
        let picture = image::open(&path)
            .unwrap_or_else(|e| panic!("{} did not open: {e}", path.display()))
            .to_rgba8();
        let clear = picture.pixels().filter(|p| p.0[3] == 0).count();
        assert!(clear > picture.pixels().len() / 2, "the layer has a ground");
        let recipe = std::fs::read_to_string(root.join("icon.json")).expect("icon.json");
        assert!(recipe.contains("\"Image.png\""), "icon.json does not name the layer");
        assert!(!recipe.contains("\"dark\""), "icon.json still has a dark appearance");
    }

    /// `shift + l` walks the lists and comes round again.
    #[test]
    fn the_key_walks_the_lists_and_comes_round() {
        assert_eq!(list_after(None, 3), Some(0));
        assert_eq!(list_after(Some(0), 3), Some(1));
        assert_eq!(list_after(Some(1), 3), Some(2));
        assert_eq!(list_after(Some(2), 3), Some(0));
        assert_eq!(list_after(Some(0), 1), Some(0));
        assert_eq!(list_after(None, 0), None);
        assert_eq!(list_after(Some(0), 0), None);
    }

    /// A row dragged about a running order lands where the line was drawn.
    ///
    /// Written against a word rather than against tracks, because what is
    /// being checked is the arithmetic and nothing else — and because the
    /// failure it guards is quiet: every drag downwards landing one place too
    /// far looks, for a while, like having been clumsy with the mouse.
    #[test]
    fn a_row_lands_where_the_line_was_drawn() {
        let move_row = |word: &str, from: usize, at: usize| -> String {
            let mut order: Vec<char> = word.chars().collect();
            let to = landed(from, at);
            let row = order.remove(from);
            order.insert(to.min(order.len()), row);
            order.into_iter().collect()
        };

        assert_eq!(move_row("abcde", 1, 4), "acdbe");
        assert_eq!(move_row("abcde", 0, 5), "bcdea");
        assert_eq!(move_row("abcde", 4, 0), "eabcd");
        assert_eq!(move_row("abcde", 3, 1), "adbce");
        assert_eq!(move_row("abcde", 2, 2), "abcde");
        assert_eq!(move_row("abcde", 2, 3), "abcde");
    }

    #[test]
    fn focus_moves_between_columns_and_stops_at_the_edges() {
        assert_eq!(Column::Artists.left(), Column::Artists);
        assert_eq!(Column::Artists.right(), Column::Albums);
        assert_eq!(Column::Lists.left(), Column::Tracks);
        assert_eq!(Column::Lists.right(), Column::Lists);
        assert_eq!(Column::Albums.left(), Column::Artists);
        assert_eq!(Column::Albums.right(), Column::Tracks);
        assert_eq!(Column::Tracks.left(), Column::Albums);
        assert_eq!(Column::Tracks.right(), Column::Queue);
        assert_eq!(Column::Queue.right(), Column::Queue);
        assert_eq!(Column::Queue.left(), Column::Tracks);
    }
}
