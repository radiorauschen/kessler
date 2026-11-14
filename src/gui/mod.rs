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

pub(crate) fn timed<T>(what: &str, work: impl FnOnce() -> T) -> T {
    if !tracing::enabled!(tracing::Level::DEBUG) {
        return work();
    }
    let began = Instant::now();
    let done = work();
    tracing::debug!("{what}: {:.1?}", began.elapsed());
    done
}

const STATUS_SECS: u64 = 4;
const NOTICE_SECS: u64 = 12;

pub const SEEK_STEP_SECS: f64 = 10.0;
pub const SEEK_MINUTE_SECS: f64 = 60.0;
pub const QUIT_WINDOW_SECS: f64 = 2.0;

const GG_WINDOW_SECS: f64 = 1.0;

pub struct ScanOutcome {
    pub total: u32,
    pub added: u32,
    pub updated: u32,
    pub removed: u32,
    pub error: Option<String>,
}

use theme::{Brightness, FontSource, Palette, Theme};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtistRow {
    All,
    Recent,
    Compilations,
    Named(usize),
}

pub const PINNED: [ArtistRow; 3] = [ArtistRow::All, ArtistRow::Recent, ArtistRow::Compilations];

impl ArtistRow {
    pub fn at(index: usize) -> ArtistRow {
        match PINNED.get(index) {
            Some(row) => *row,
            None => ArtistRow::Named(index - PINNED.len()),
        }
    }

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlbumRow {
    All,
    Named(usize),
}

impl AlbumRow {
    pub fn at(row: usize) -> AlbumRow {
        match row.checked_sub(1) {
            None => AlbumRow::All,
            Some(at) => AlbumRow::Named(at),
        }
    }

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

pub fn frame_theme_for(brightness: Brightness, palette: &Palette) -> egui::SystemTheme {
    match (brightness == Brightness::System, palette.is_dark()) {
        (true, _) => egui::SystemTheme::SystemDefault,
        (false, true) => egui::SystemTheme::Dark,
        (false, false) => egui::SystemTheme::Light,
    }
}

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

#[derive(Clone)]
pub enum Dragged {
    Queued(usize),
    Incoming(Vec<Track>),
    Episodes(Vec<(crate::podcast::Episode, crate::podcast::Show)>),
    QueuedEpisode(usize),
}

impl Dragged {
    pub fn count(&self) -> usize {
        match self {
            Self::Queued(_) | Self::QueuedEpisode(_) => 1,
            Self::Incoming(tracks) => tracks.len(),
            Self::Episodes(episodes) => episodes.len(),
        }
    }
}

pub struct Details {
    pub title: String,
    pub lines: Vec<crate::library::details::Line>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Source {
    #[default]
    Library,
    Device,
}

pub fn artist_row_for(names: &[&str], artist_key: &str) -> ArtistRow {
    match names
        .iter()
        .position(|name| crate::db::sort_key(name) == artist_key)
    {
        Some(at) => ArtistRow::Named(at),
        None => ArtistRow::All,
    }
}

pub const COMPILATION_ARTISTS: usize = 3;

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
    ]
    .into_iter()
    .find(|f| f.sql_column() == column)?;
    Some((field, order))
}

pub struct TagHunt {
    pub what: Hunted,
    news: mpsc::Receiver<HuntNews>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub total: usize,
    pub asked: usize,
    pub found: usize,
    unshown: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hunted {
    Years,
    Genres,
}

impl Hunted {
    pub fn word(self) -> &'static str {
        match self {
            Self::Years => "jahre",
            Self::Genres => "genres",
        }
    }
}

enum Found {
    Year(i32),
    Genre(String),
}

enum HuntNews {
    Asked,
    Found {
        artist: String,
        album: String,
        found: Found,
    },
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Music,
    Radio,
    Podcasts,
    Ipod,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Sides {
    pub artist: bool,
    pub log: bool,
    pub lyrics: bool,
    pub others: bool,
    pub eq: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fourth {
    Queue,
    Lists,
    Device,
    Log,
    Artist,
    Lyrics,
    Others,
    Equalizer,
}

impl Fourth {
    pub const SIDES: [Fourth; 6] = [Fourth::Artist, Fourth::Lyrics, Fourth::Others, Fourth::Equalizer, Fourth::Device, Fourth::Log];

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ListSort {
    #[default]
    Ascending,
    Descending,
    Added,
    Year,
    YearBack,
}

impl ListSort {
    pub fn next(self) -> Self {
        match self {
            Self::Ascending => Self::Descending,
            Self::Descending => Self::Added,
            Self::Added => Self::Ascending,
            Self::Year | Self::YearBack => Self::Ascending,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Ascending => "a-z",
            Self::Descending => "z-a",
            Self::Added => "zuletzt",
            Self::Year => "jahr",
            Self::YearBack => "jahr rückwärts",
        }
    }

    pub fn from_key(key: &str, years: bool) -> Self {
        match key.trim() {
            "z-a" => Self::Descending,
            "zuletzt" => Self::Added,
            "jahr" if years => Self::Year,
            "jahr rückwärts" if years => Self::YearBack,
            _ => Self::Ascending,
        }
    }

    pub fn other_year(self) -> Self {
        match self {
            Self::Year => Self::YearBack,
            _ => Self::Year,
        }
    }

    pub fn is_year(self) -> bool {
        matches!(self, Self::Year | Self::YearBack)
    }

    pub fn mark(self) -> &'static str {
        match self {
            Self::Ascending => "↓",
            Self::Descending => "↑",
            Self::Added => tr!("zuletzt"),
            Self::Year | Self::YearBack => "",
        }
    }
}

pub struct RowMenu {
    pub at: egui::Pos2,
    pub about: About,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum About {
    Tracks,
    Album(usize),
    Queued(usize),
    Listed(usize),
    List(usize),
    Artist(usize),
    Station(usize),
    Episode(usize),
}

pub type ArtistAnchor = (Option<String>, usize, Option<i64>);

pub fn artist_rows(names: usize) -> usize {
    names + PINNED.len()
}

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

fn list_after(standing: Option<usize>, lists: usize) -> Option<usize> {
    if lists == 0 {
        return None;
    }
    Some(match standing {
        Some(at) => (at + 1) % lists,
        None => 0,
    })
}

fn landed(from: usize, at: usize) -> usize {
    if at > from {
        at - 1
    } else {
        at
    }
}

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

pub struct Prompt {
    pub entry: tagedit::Entry,
    pub ask: Ask,
}

pub enum Ask {
    NewList(Vec<Track>),
    Rename(i64),
    Delete(i64),
}

impl Prompt {
    pub fn typing(&self) -> bool {
        !matches!(self.ask, Ask::Delete(_))
    }

    pub fn heading(&self) -> &'static str {
        match self.ask {
            Ask::NewList(_) => tr!("neue liste"),
            Ask::Rename(_) => tr!("liste umbenennen"),
            Ask::Delete(_) => tr!("liste löschen"),
        }
    }

    pub fn word(&self) -> &'static str {
        match self.ask {
            Ask::NewList(_) => tr!("liste anlegen"),
            Ask::Rename(_) => tr!("umbenennen"),
            Ask::Delete(_) => tr!("löschen"),
        }
    }
}

pub struct Transfer {
    pub what: String,
    pub plan: crate::ipod::sync::SyncPlan,
    pub free: Option<u64>,
    pub can_convert: bool,
    pub stage: Stage,
    pub music_at_start: u64,
    pub space_read_at: Option<std::time::Instant>,
}

pub enum Stage {
    Asking,
    Going {
        done: usize,
        total: usize,
        doing: String,
        failed: usize,
        steps: mpsc::Receiver<Moved>,
    },
    Done(std::result::Result<String, String>),
}

pub enum Moved {
    Along(crate::ipod::sync::SyncProgress),
    Ended(std::result::Result<(), String>),
}

pub enum DeviceNews {
    Attached(Option<attached::Attached>),
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Said(String),
}

pub type OnDevice = std::sync::Arc<std::collections::HashSet<i64>>;

pub type MarkKey = (usize, crate::lang::Lang, (String, u32, Option<u32>));

pub struct GuiApp {
    pub db: Database,
    pub config: Config,
    pub player: Player,

    pub theme: Theme,
    pub brightness: Brightness,
    pub resolved: Theme,
    fading: Option<(Palette, f64)>,
    pub palette: Palette,
    frame_theme: Option<egui::SystemTheme>,
    pub font_source: FontSource,
    pub font_fallback: Option<String>,
    pub transfer: Option<Transfer>,
    pub typeface: Option<String>,
    pub families: Vec<theme::Family>,
    pub theme_list_open: bool,
    pub seed_role_open: Option<usize>,
    pub seed_entry: Option<tagedit::Entry>,
    pub seed_hex: Option<(usize, tagedit::Entry)>,
    pub licenses_open: bool,
    pub sets_open: bool,
    pub sets_offered: Option<(String, Vec<theme::StyleSet>)>,
    pub update: Option<update::Check>,
    pub backup_said: Option<String>,
    pub naming_device: Option<tagedit::Entry>,
    pub forget_asked: Option<usize>,
    pub picture_screen_list_open: bool,
    pub saver_screen_list_open: bool,
    pub menu_heights: [f32; menu::SECTION_COUNT],
    pub menu_top: Option<f32>,
    pub menu_leaving: Option<f64>,
    pub zoom_dip: Option<app::ZoomDip>,
    pub menu_tab: usize,
    pub fourth_was: Fourth,
    pub side_was: Fourth,
    pub side_fits: bool,
    pub(super) was_split: bool,
    pub fourth_sliding: bool,
    pub(super) mark_still: Option<((usize, dither::Screen), egui::TextureHandle)>,

    pub queue_peek: bool,
    pub attached: Option<attached::Attached>,
    pub device_open: bool,
    pub device_summary: Option<device::Summary>,
    pub on_device: Option<((std::path::PathBuf, usize), OnDevice)>,
    pub removal: Option<device::Removal>,
    pub device_art: Option<([i32; 4], device::Made, egui::TextureHandle)>,
    pub device_handling: device::Handling,
    pub prompt: Option<Prompt>,
    heard: std::sync::Arc<crate::player::audio::Heard>,
    pub spectrum: sound::Spectrum,
    pub scope: scope::Scope,
    scope_picture: Option<egui::TextureHandle>,
    pub mark_scope: scope::Scope,
    field_art: library::FieldArt,
    reading: library::Reading,
    mark_art: library::FieldArt,
    pub(super) mark_picture: Option<egui::TextureHandle>,
    pub(super) mark_paused: Option<(MarkKey, egui::TextureHandle)>,
    pub saver: saver::Saver,
    pub source: Source,
    pub device_matching: usize,
    device_watch: Option<mpsc::Receiver<DeviceNews>>,

    pub artists: Vec<Artist>,
    pub albums: Vec<Album>,
    pub tracks: Vec<Track>,
    pub compilations: Vec<Vec<Track>>,
    pub compiled: std::collections::HashSet<i64>,
    pub guests: std::collections::HashSet<i64>,
    pub lists: Vec<(crate::db::models::Playlist, usize)>,
    pub editing: Option<i64>,
    pub editing_tracks: Vec<Track>,
    pub drawn_with: String,
    pub total_tracks: u32,
    pub total_albums: usize,
    pub total_seconds: f64,
    pub total_bytes: u64,
    pub matching_tracks: u32,

    pub artist_index: usize,
    pub album_index: usize,
    pub track_index: usize,
    pub focus: Column,
    pub untouched: bool,
    pub recent_more: usize,

    pub track_selection: std::collections::BTreeSet<usize>,
    pub track_anchor: usize,
    pub record_picked: bool,
    pub unmarked: bool,
    queue_saved: u64,
    pub dragging: Option<Dragged>,
    editor_in: Column,
    queue_followed: Option<usize>,
    pub queue_index: usize,
    pub queue_picked: std::collections::BTreeSet<usize>,
    pub queue_picked_in: u64,

    pub editor: Option<tagedit::Editor>,
    pub row_menu: Option<RowMenu>,
    pub row_menu_focus: Option<usize>,
    pub row_menu_step: i32,
    pub row_menu_enter: bool,
    pub row_menu_side: i32,
    pub cover_pick: Option<coverpick::Picker>,
    pub tag_hunt: Option<TagHunt>,
    pub details: Option<Details>,

    pub sort: SortField,
    pub order: SortOrder,
    pub artist_sort: ListSort,
    pub album_sort: ListSort,

    pub columns: columns::TrackColumns,
    pub cover_cache: covers::CoverCache,

    pub narrow: bool,
    pub lists_index: usize,
    pub lists_last: Option<i64>,

    pub reveal_all: bool,
    pub reveal_next: bool,

    pub scroll_to: Option<Column>,

    pub scrobbler: Option<crate::lastfm::Scrobbler>,
    scrobble_watch: crate::lastfm::watch::Watch,
    connecting: Option<mpsc::Receiver<Result<(String, String)>>>,

    media: crate::media::Reporter,
    menu_bar: menubar::MenuBar,
    media_started: bool,
    desktop_theme: Option<egui::Theme>,
    desktop_news: Option<mpsc::Receiver<egui::Theme>>,
    quitting: bool,
    quit_asked_at: Option<f64>,
    clear_asked_at: Option<f64>,
    g_pressed_at: Option<f64>,
    c_pressed_at: Option<f64>,
    view_now: View,
    view_before: Option<View>,
    pub keys_sheet_open: bool,
    pub keys_sheet_scroll: f32,
    pub keys_query: String,

    pub menu_open: bool,
    pub measure: Option<measure::Measure>,
    pub(in crate::gui) reel: Option<Box<app::Reel>>,
    pub(in crate::gui) reel_written: Option<app::Written>,
    frame_started: Option<std::time::Instant>,
    pub filter: Option<String>,
    pub radio: Option<Box<crate::radio::Radio>>,
    pub on_air: Option<crate::radio::Station>,
    pub station_import: Option<std::sync::mpsc::Receiver<Option<std::path::PathBuf>>>,
    radio_typed_at: Option<std::time::Instant>,
    sides_left: [Option<Sides>; 4],
    radio_heard: Option<Box<crate::radio::heard::Heard>>,
    pub aired: Vec<crate::radio::Aired>,
    station_noted: Option<String>,
    spins: Option<Box<crate::radio::spins::Watch>>,
    pub(in crate::gui) programme: Option<Box<crate::radio::programme::Programme>>,
    window_was: Option<[f32; 4]>,
    window_still_since: Option<std::time::Instant>,

    missing: std::collections::HashSet<std::path::PathBuf>,

    pub filter_marked: bool,

    pub filter_typing: bool,
    pub filter_caret: Option<usize>,
    pub radio_heard_keys: Option<usize>,
    scan: Option<mpsc::Receiver<ScanOutcome>>,
    scan_progress: Option<mpsc::Receiver<crate::library::scanner::ScanProgress>>,
    scan_done: u32,
    scan_total: u32,
    foreign: Vec<(String, u32)>,
    scan_updates: Vec<crate::library::scanner::Update>,
    picking: Option<mpsc::Receiver<Option<std::path::PathBuf>>>,
    adding: Option<mpsc::Receiver<crate::library::import::Outcome>>,
    adding_pick: Option<mpsc::Receiver<Vec<std::path::PathBuf>>>,
    add_progress: Option<mpsc::Receiver<crate::library::import::Progress>>,
    add_done: u32,
    add_total: u32,
    pub hovering: usize,
    picking_cover: Option<(mpsc::Receiver<Option<std::path::PathBuf>>, usize)>,

    status: Option<(String, Instant, u64)>,
    pub log: log::Log,
    pub log_open: bool,
    pub artist_open: bool,
    pub lyrics_open: bool,
    pub eq_open: bool,
    pub listened: listened::Listened,
    icon_dressed: Option<(egui::Color32, egui::Color32, Option<String>, crate::lang::Lang)>,
    titled: Option<crate::lang::Lang>,
    pub lyrics: lyrics::LyricsPanel,
    pub artist: artist::ArtistPanel,
    pub artist_form: Option<artist::OwnForm>,
    pub exporting_artists: Option<mpsc::Receiver<Option<std::path::PathBuf>>>,
    pub artist_hunt: Option<artist::Hunt>,
    pub artist_pin: Option<(String, ArtistAnchor)>,
    pub artist_trail: Vec<String>,
    pub gaps: artist::GapsPanel,
    pub podcasts: app::podcasts::PodcastPanel,
    pub lastfm_keys: Option<lastfmkeys::KeyForm>,
    pub measuring: Option<levelrun::Measuring>,
    pub level_check: bool,
    pub loudness_count: (usize, usize),
    pub log_from_newest: bool,
}

impl GuiApp {
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
            update: None,
            forget_asked: None,
            backup_said: None,
            naming_device: None,
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
            mark_paused: None,
            saver: saver::Saver::default(),
            radio: None,
            on_air: None,
            station_import: None,
            radio_typed_at: None,
            sides_left: [None; 4],
            radio_heard: None,
            aired: Vec::new(),
            station_noted: None,
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

    fn remember_orders(&mut self) {
        self.config.ui.artist_order = self.artist_sort.key().to_string();
        self.config.ui.album_order = self.album_sort.key().to_string();
        self.config.ui.track_order = track_order_key(self.sort, self.order);
        self.save_config();
    }

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

    pub fn cycle_album_order(&mut self) -> Result<()> {
        self.album_sort = self.album_sort.next();
        self.remember_orders();
        self.album_index = 0;
        self.scroll_to = Some(Column::Albums);
        self.reload_dependent()
    }

    pub fn cycle_album_years(&mut self) -> Result<()> {
        self.album_sort = self.album_sort.other_year();
        self.remember_orders();
        self.album_index = 0;
        self.scroll_to = Some(Column::Albums);
        self.reload_dependent()
    }

    pub fn query(&self) -> Option<String> {
        self.filter
            .as_deref()
            .map(str::trim)
            .filter(|q| !q.is_empty())
            .map(str::to_string)
    }

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
            (_, true) => self.db.get_albums_recent(None, query)?,
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
            (true, true) if self.merged() => self.recent_records(query)?,
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

    pub fn track_order(&self) -> (SortField, SortOrder) {
        if self.merged() {
            (SortField::Artist, SortOrder::Ascending)
        } else {
            (self.sort, self.order)
        }
    }

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

    pub(in crate::gui) fn recent_wanted(&self) -> usize {
        crate::config::RECENT_SHOWN + self.recent_more
    }

    pub(in crate::gui) fn more_recent_to_fetch(&self) -> bool {
        self.showing_recent()
            && !self.showing_ipod()
            && self.album_index == 0
            && self.tracks.len() >= self.recent_wanted()
            && self.tracks.len() < self.matching_tracks as usize
    }

    fn recent_records(&self, query: Option<&str>) -> Result<Vec<Track>> {
        let wanted = self.recent_wanted();
        let mut tracks = Vec::new();
        for album in self.db.get_albums_recent(None, query)? {
            if tracks.len() >= wanted {
                break;
            }
            tracks.extend(self.db.get_tracks(
                Some(&album.artist),
                Some(&album.name),
                SortField::Artist,
                SortOrder::Ascending,
                query,
            )?);
        }
        Ok(tracks)
    }

    pub fn save_config(&mut self) {
        if let Err(e) = self.config.save() {
            self.set_status(tr!("einstellungen nicht gespeichert: {}", e));
        }
    }
}

impl GuiApp {
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

    fn hold_the_frame_rate(&mut self) {
        let cap = self.config.ui.frame_cap;
        if let (true, Some(last)) = (cap > 0, self.frame_started) {
            let gap = std::time::Duration::from_secs_f64(1.0 / f64::from(cap))
                .saturating_sub(std::time::Duration::from_millis(2));
            let since = last.elapsed();
            if since < gap {
                std::thread::sleep(gap - since);
            }
        }
        self.frame_started = Some(std::time::Instant::now());
    }

    fn lap(&mut self, what: &'static str) {
        if let Some(measure) = self.measure.as_mut() {
            measure.lap(what);
        }
    }
}

fn keep_the_wheel_where_it_turned(ctx: &egui::Context) {
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

    #[cfg(not(target_os = "macos"))]
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.leave();
    }

    #[cfg(target_os = "macos")]
    fn on_exit(&mut self) {
        self.leave();
    }

    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.hold_the_frame_rate();
        keep_the_wheel_where_it_turned(ctx);
        crate::media::set_notify(self.config.ui.notify, ctx.input(|i| i.viewport().focused.unwrap_or(true)));
        #[cfg(target_os = "linux")]
        crate::ipod::udisks::AUTO.store(self.config.ipod_mount, std::sync::atomic::Ordering::Relaxed);
        if let Some(placed) = macwindow::apply(frame) {
            theme::set_titlebar(placed.band);
            theme::set_buttons_end(placed.buttons_end);
        }
        if let Some(measure) = self.measure.as_mut() {
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
        self.poll_cover_file();
        self.poll_cover_pick();
        self.poll_artist();
        self.poll_lyrics();
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
        if self.drawn_with.is_empty() {
            self.drawn_with = drawn_with(frame);
        }
        let full = ui.available_rect_before_wrap();
        ui.painter().rect_filled(full, 0.0, self.palette.ground);
        library::render(self, ui);
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

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn scheme_from(line: &str) -> egui::Theme {
    if line.contains("prefer-dark") {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    }
}

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
    let icon = if crate::launched_from_a_bundle() {
        egui::IconData::default()
    } else {
        app_icon().unwrap_or_default()
    };
    viewport = viewport.with_icon(icon);

    let options = eframe::NativeOptions {
        viewport,
        renderer: renderer_wanted(),
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

fn drawn_with(frame: &eframe::Frame) -> String {
    match frame.wgpu_render_state() {
        Some(state) => match format!("{:?}", state.adapter.get_info().backend).to_lowercase() {
            metal if metal == "metal" => metal,
            other => format!("wgpu · {other}"),
        },
        None => "opengl".to_string(),
    }
}

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

pub(crate) fn copy_or_cut(ctx: &egui::Context) -> (bool, bool) {
    ctx.input(|i| {
        let copy = i.events.iter().any(|e| matches!(e, egui::Event::Copy));
        let cut = i.events.iter().any(|e| matches!(e, egui::Event::Cut));
        (copy, cut)
    })
}

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

    #[test]
    fn the_column_ends_where_its_last_row_is() {
        for names in 1..6 {
            let rows = artist_rows(names);
            assert_eq!(ArtistRow::at(rows - 1), ArtistRow::Named(names - 1));
        }
        assert_eq!(artist_rows(0), PINNED.len());
    }

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

    #[test]
    fn the_application_has_its_own_icon() {
        let icon = app_icon().expect("the embedded icon did not decode");
        assert_eq!(icon.width, icon.height, "the icon is not square");
        assert!(icon.width >= 256, "the icon is too small for a dock tile");
        assert_eq!(icon.rgba.len(), (icon.width * icon.height * 4) as usize);
    }

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

    #[test]
    fn gnome_says_light_or_dark() {
        assert_eq!(scheme_from("'prefer-dark'\n"), egui::Theme::Dark);
        assert_eq!(scheme_from("color-scheme: 'prefer-dark'"), egui::Theme::Dark);
        assert_eq!(scheme_from("'default'"), egui::Theme::Light);
        assert_eq!(scheme_from("color-scheme: 'prefer-light'"), egui::Theme::Light);
    }

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
