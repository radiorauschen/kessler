use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::error::{KesslerError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub music_dirs: Vec<PathBuf>,
    pub watch_dirs: bool,
    pub add_move: bool,
    pub add_to_music: bool,
    pub ipod_convert: bool,
    pub ipod_on_connect: String,
    pub ipod_mount: bool,
    pub ipod_take_plays: bool,
    pub ipod_podcasts: bool,
    pub database_path: Option<PathBuf>,
    pub ui: UiConfig,
    pub player: PlayerConfig,
    pub lastfm: LastfmConfig,
    #[serde(skip)]
    pub set_aside: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LastfmConfig {
    pub api_key: Option<String>,
    pub api_secret: Option<String>,
    pub session_key: Option<String>,
    pub username: Option<String>,
    pub now_playing: bool,
}

fn music_where_unsaid() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    pub ground: String,
    pub track_columns: Vec<String>,
    pub artist_counts: bool,
    pub artist_total: bool,
    pub albums_in_tracks: bool,
    pub compilations_whole: bool,
    pub library_line: bool,
    pub show_compilations: bool,
    pub radio: bool,
    pub ipod: bool,
    pub podcasts: bool,
    #[serde(default = "music_where_unsaid")]
    pub music: bool,
    pub capitals: bool,
    pub podcast_back: u32,
    pub podcast_ahead: u32,
    pub podcast_keep: String,
    pub podcast_show_archived: bool,
    pub podcast_check: String,
    pub podcast_queue: bool,
    pub radio_info: bool,
    pub podcast_order: String,
    pub podcast_archive_forgets: bool,
    pub radio_scrobble: bool,
    pub radio_love: bool,
    pub eq_on: bool,
    pub eq_own_1: String,
    pub eq_own_2: String,
    pub eq_slot: u8,
    pub scroll_speed: String,
    pub eq_gains: String,
    pub radio_logos: bool,
    pub radio_found: String,
    pub show_recent: bool,
    pub show_all_artists: bool,
    pub start_on_recent: bool,
    pub show_lists: bool,
    pub header_playing: bool,
    pub playing_at: String,
    pub saver_after: u32,
    pub saver_dots: u8,
    pub saver_glow: u8,
    pub saver_strength: u8,
    pub saver_size: u8,
    pub saver_silent: bool,
    pub saver_dither_method: String,
    pub saver_colours: u8,
    pub saver_picture: String,
    pub menubar: String,
    pub custom_colours: String,
    pub menubar_cover: String,
    pub menubar_cover_screened: bool,
    pub menubar_ink: String,
    pub language: String,
    pub zoom: f32,
    pub brightness: String,
    pub merged_number: bool,
    pub merged_length: bool,
    pub numbers_right: bool,
    pub small_length: bool,
    pub all_columns: bool,
    pub device_mark: String,
    pub notify: bool,
    pub heading_count: bool,
    pub modes_in_queue: bool,
    pub info_picture: bool,
    pub info_picture_wide: bool,
    pub info_facts: bool,
    pub info_wikipedia: bool,
    pub info_library: bool,
    pub info_ties: bool,
    pub info_similar: bool,
    pub lyrics_size: String,
    pub lyrics_follow: bool,
    pub log_hidden: Vec<String>,
    pub others_shown: u32,
    pub others_tree: bool,
    pub artist_order: String,
    pub album_order: String,
    pub track_order: String,
    pub field_spectrum: bool,
    pub field_picture: String,
    pub book_at: usize,
    pub frame_cap: u32,
    pub scope_rate: u32,
    pub field_rate: u32,
    pub cover_dither: bool,
    pub dither_method: String,
    pub dither_fineness: f32,
    pub dither_tones: u8,
    #[serde(skip_serializing)]
    pub playing_dither: Option<bool>,
    #[serde(skip_serializing)]
    pub playing_dither_method: Option<String>,
    #[serde(skip_serializing)]
    pub playing_dither_fineness: Option<f32>,
    pub heading_cover_size: f32,
    pub heading_covers: bool,
    #[serde(skip_serializing)]
    pub heading_dither: Option<bool>,
    #[serde(skip_serializing)]
    pub heading_dither_method: Option<String>,
    #[serde(skip_serializing)]
    pub heading_dither_fineness: Option<f32>,
    pub artists_share: Option<f32>,
    pub albums_share: Option<f32>,
    pub queue_share: Option<f32>,
    pub merged_even: bool,
    pub side_share: Option<f32>,
    pub radio_stations_share: Option<f32>,
    pub radio_side_share: Option<f32>,
    pub radio_page_share: Option<f32>,
    pub podcast_shares: Vec<f32>,

    pub artists_width: Option<f32>,
    pub albums_width: Option<f32>,
    pub queue_width: Option<f32>,
    pub show_queue: bool,
    pub window: Option<[f32; 4]>,
    pub font_dir: Option<String>,
    pub typeface: String,
    pub font_width: f32,
    pub system_fonts: bool,
    pub font_weight: f32,
    pub font_hinting: String,
    pub font_sets: std::collections::BTreeMap<String, Vec<String>>,
    pub font_fallback: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PlayerConfig {
    pub volume: u8,
    pub gapless: bool,
    pub leveling: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            music_dirs: vec![],
            watch_dirs: true,
            add_move: false,
            add_to_music: false,
            ipod_convert: true,
            ipod_on_connect: "nichts".into(),
            ipod_mount: true,
            ipod_take_plays: true,
            ipod_podcasts: false,
            database_path: None,
            ui: UiConfig::default(),
            player: PlayerConfig::default(),
            set_aside: None,
            lastfm: LastfmConfig {
                now_playing: true,
                ..Default::default()
            },
        }
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            ground: "公園".into(),
            track_columns: vec!["nr".into(), "jahr".into(), "genre".into(), "länge".into()],
            artist_counts: false,
            artist_total: false,
            albums_in_tracks: true,
            compilations_whole: true,
            library_line: true,
            show_compilations: true,
            radio: false,
            ipod: true,
            podcasts: false,
            music: false,
            capitals: false,
            podcast_back: 10,
            podcast_ahead: 10,
            podcast_keep: "gehört".into(),
            podcast_show_archived: true,
            podcast_check: "30".into(),
            podcast_queue: true,
            radio_info: true,
            podcast_order: "zuletzt".into(),
            podcast_archive_forgets: true,
            radio_scrobble: true,
            radio_love: false,
            eq_on: false,
            eq_own_1: String::new(),
            eq_own_2: String::new(),
            eq_slot: 0,
            scroll_speed: "normal".into(),
            eq_gains: String::new(),
            radio_logos: true,
            radio_found: "rechts".to_string(),
            show_recent: true,
            show_all_artists: true,
            start_on_recent: true,
            show_lists: false,
            all_columns: false,
            header_playing: false,
            playing_at: "kompakt".to_string(),
            saver_after: 0,
            saver_dots: 2,
            saver_glow: 1,
            saver_strength: 1,
            saver_size: 2,
            saver_silent: false,
            saver_dither_method: "geordnet".into(),
            saver_colours: 0,
            saver_picture: "vektorskop".into(),
            menubar: "aus".into(),
            custom_colours: String::new(),
            menubar_cover: "daneben".into(),
            menubar_cover_screened: true,
            menubar_ink: "system".into(),
            language: String::new(),
            zoom: 1.0,
            brightness: "system".into(),
            merged_number: true,
            merged_length: true,
            numbers_right: false,
            small_length: false,
            device_mark: "wort".into(),
            notify: false,
            heading_count: true,
            modes_in_queue: false,
            info_picture: true,
            info_picture_wide: false,
            info_facts: true,
            info_wikipedia: true,
            info_library: true,
            info_ties: true,
            info_similar: true,
            lyrics_size: "normal".into(),
            lyrics_follow: true,
            log_hidden: Vec::new(),
            others_shown: 0,
            others_tree: false,
            artist_order: String::new(),
            album_order: String::new(),
            track_order: String::new(),
            field_spectrum: false,
            field_picture: "vektorskop".into(),
            book_at: 0,
            frame_cap: if SMALL_MACHINE { 30 } else { 0 },
            scope_rate: if SMALL_MACHINE { 12 } else { 20 },
            field_rate: 4,
            cover_dither: true,
            dither_method: "blaues rauschen".into(),
            dither_fineness: 0.6,
            dither_tones: 4,
            playing_dither: None,
            playing_dither_method: None,
            playing_dither_fineness: None,
            heading_cover_size: HEADING_COVER_DEFAULT,
            heading_covers: true,
            heading_dither: None,
            heading_dither_method: None,
            heading_dither_fineness: None,
            font_dir: None,
            typeface: String::new(),
            font_width: 85.0,
            system_fonts: false,
            font_weight: 400.0,
            font_hinting: "voll".into(),
            font_sets: [("asap", "ss01:g"), ("muto", "ss01:a"), ("mutomono", "ss01:a")]
                .into_iter()
                .map(|(face, tag)| (face.to_string(), vec![tag.to_string()]))
                .collect(),
            font_fallback: None,
            artists_width: None,
            albums_width: None,
            queue_width: None,
            artists_share: None,
            albums_share: None,
            queue_share: None,
            side_share: None,
            radio_stations_share: None,
            radio_side_share: None,
            radio_page_share: None,
            podcast_shares: Vec::new(),
            merged_even: true,
            show_queue: true,
            window: None,
        }
    }
}

impl Default for PlayerConfig {
    fn default() -> Self {
        Self {
            volume: 70,
            gapless: true,
            leveling: "aus".to_string(),
        }
    }
}

const FORMER_NAME: &str = "cratedig";

fn adopt(old: &std::path::Path, new: &std::path::Path) -> bool {
    if new.exists() || !old.is_dir() {
        return false;
    }
    if let Some(parent) = new.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return false;
        }
    }
    match std::fs::rename(old, new) {
        Ok(()) => true,
        Err(e) => {
            tracing::warn!(
                "could not move {} to {}: {e}",
                old.display(),
                new.display()
            );
            false
        }
    }
}

impl Config {
    pub fn adopt_former_name() {
        let former = |dir: Option<PathBuf>| dir.map(|d| d.join(FORMER_NAME));
        for (old, new) in [
            (former(dirs::config_dir()), Self::config_dir()),
            (former(dirs::data_dir()), Self::data_dir()),
        ] {
            if let Some(old) = old {
                if adopt(&old, &new) {
                    tracing::info!("moved {} to {}", old.display(), new.display());
                }
            }
        }
    }

    pub fn config_dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("kessler")
    }

    pub fn data_dir() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("kessler")
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    pub fn lyrics_dir(&self) -> PathBuf {
        self.db_path()
            .parent()
            .map(|d| d.join("lyrics"))
            .unwrap_or_else(|| Self::data_dir().join("lyrics"))
    }

    pub fn artists_dir(&self) -> PathBuf {
        self.db_path()
            .parent()
            .map(|d| d.join("artists"))
            .unwrap_or_else(|| Self::data_dir().join("artists"))
    }

    pub fn artwork_dir(&self) -> PathBuf {
        self.db_path()
            .parent()
            .map(|d| d.join("artwork"))
            .unwrap_or_else(|| Self::data_dir().join("artwork"))
    }

    pub fn queue_path(&self) -> PathBuf {
        self.db_path()
            .parent()
            .map(|d| d.join("queue.txt"))
            .unwrap_or_else(|| Self::data_dir().join("queue.txt"))
    }

    pub fn ipod_plays_path(&self) -> PathBuf {
        self.db_path()
            .parent()
            .map(|d| d.join("ipod-plays.toml"))
            .unwrap_or_else(|| Self::data_dir().join("ipod-plays.toml"))
    }

    pub fn scrobble_queue_path(&self) -> PathBuf {
        self.db_path()
            .parent()
            .map(|d| d.join("scrobbles.tsv"))
            .unwrap_or_else(|| Self::data_dir().join("scrobbles.tsv"))
    }

    pub fn db_path(&self) -> PathBuf {
        self.database_path
            .clone()
            .unwrap_or_else(|| Self::data_dir().join("library.db"))
    }

    pub fn load() -> Result<Self> {
        Self::adopt_former_name();
        let config_path = Self::config_path();

        if config_path.exists() {
            let contents = std::fs::read_to_string(&config_path).map_err(|e| {
                KesslerError::Config(format!("Failed to read config at {}: {}", config_path.display(), e))
            })?;
            let mut config: Config = match toml::from_str(&contents) {
                Ok(config) => config,
                Err(e) => return Ok(Self::set_aside(&config_path, &e.to_string())),
            };
            if !said_language(&contents) {
                config.ui.language = "de".to_string();
            }
            config.ui.fold_pictures();
            Ok(config)
        } else {
            let config = Config::default();
            if let Err(e) = config.save() {
                tracing::warn!("Could not save default config: {}", e);
            }
            Ok(config)
        }
    }

    fn set_aside(path: &std::path::Path, why: &str) -> Self {
        let aside = path.with_extension("toml.kaputt");
        eprintln!("kessler: config unreadable, set aside as {}: {why}", aside.display());
        tracing::warn!("config unreadable, set aside as {}: {why}", aside.display());
        let mut config = Config::default();
        if std::fs::rename(path, &aside).is_ok() {
            config.set_aside = Some(aside);
        }
        config
    }

    pub fn load_from(path: &std::path::Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path).map_err(|e| {
            KesslerError::Config(format!("Failed to read config at {}: {}", path.display(), e))
        })?;
        let mut config: Config = toml::from_str(&contents)
            .map_err(|e| KesslerError::Config(format!("Failed to parse config: {}", e)))?;
        config.ui.fold_pictures();
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path();
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let contents = toml::to_string_pretty(self)
            .map_err(|e| KesslerError::Config(format!("Failed to serialize config: {}", e)))?;
        std::fs::write(&config_path, contents)?;
        Ok(())
    }
}

fn said_language(contents: &str) -> bool {
    contents
        .parse::<toml::Table>()
        .ok()
        .and_then(|t| t.get("ui").and_then(|ui| ui.get("language")).cloned())
        .is_some()
}

pub const SMALL_MACHINE: bool = cfg!(all(target_os = "linux", target_arch = "aarch64"));

pub const HEADING_COVER_LEAST: f32 = 32.0;
pub const HEADING_COVER_DEFAULT: f32 = 60.0;
pub const HEADING_COVER_MOST: f32 = 256.0;

pub const RECENT_SHOWN: usize = 80;

impl UiConfig {
    pub fn sets_of(&self, key: &str) -> Vec<String> {
        self.font_sets.get(key).cloned().unwrap_or_default()
    }

    pub fn heading_cover(&self) -> f32 {
        let size = self.heading_cover_size;
        if size.is_finite() {
            size.clamp(HEADING_COVER_LEAST, HEADING_COVER_MOST)
        } else {
            HEADING_COVER_DEFAULT
        }
    }

    pub fn fold_pictures(&mut self) {
        let dither = self.heading_dither.take().or(self.playing_dither.take());
        let method = self.heading_dither_method.take().or(self.playing_dither_method.take());
        let fineness = self.heading_dither_fineness.take().or(self.playing_dither_fineness.take());
        self.playing_dither = None;
        self.playing_dither_method = None;
        self.playing_dither_fineness = None;
        if let Some(dither) = dither {
            self.cover_dither = dither;
        }
        if let Some(method) = method {
            self.dither_method = method;
        }
        if let Some(fineness) = fineness {
            self.dither_fineness = fineness;
        }
    }

    pub fn picture_fineness(&self) -> f32 {
        if self.dither_fineness.is_finite() {
            self.dither_fineness.clamp(0.0, 1.0)
        } else {
            0.5
        }
    }
}

impl UiConfig {
    pub fn even_columns_again(&mut self) {
        self.merged_even = true;
        self.artists_share = None;
        self.albums_share = None;
        self.queue_share = None;
        self.side_share = None;
        self.artists_width = None;
        self.albums_width = None;
        self.queue_width = None;
        self.radio_stations_share = None;
        self.radio_side_share = None;
        self.radio_page_share = None;
        self.podcast_shares.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_first_start_has_the_chosen_defaults() {
        let ui = UiConfig::default();
        assert_eq!(ui.ground, "公園");
        assert_eq!(ui.brightness, "system");
        assert!(ui.show_all_artists && ui.show_recent && ui.start_on_recent && ui.show_compilations);
        assert!(!ui.artist_counts);
        assert!(ui.albums_in_tracks && ui.heading_covers && ui.heading_count && ui.compilations_whole);
        assert_eq!(ui.heading_cover(), 60.0);
        assert_eq!(ui.track_columns, ["nr", "jahr", "genre", "länge"]);
        assert!(!ui.numbers_right && ui.merged_number && ui.merged_length);
        assert_eq!(ui.saver_after, 0, "no screen saver");
        assert!(!ui.field_spectrum, "the scope costs, so it waits to be asked for");
        assert_eq!(ui.language, "", "the system's");
        assert_eq!(ui.brightness, "system");
        assert_eq!((ui.typeface.as_str(), ui.font_width, ui.font_weight), ("", 85.0, 400.0));
        assert!(ui.cover_dither);
        assert_eq!((ui.dither_method.as_str(), ui.dither_tones, ui.dither_fineness), ("blaues rauschen", 4, 0.6));
        assert_eq!(ui.playing_at, "kompakt");
        assert!(ui.library_line && !ui.modes_in_queue);
    }

    #[test]
    fn the_music_is_off_at_first_and_on_in_old_files() {
        assert!(!Config::default().ui.music);
        let old: Config = toml::from_str("[ui]\nradio = true\n").unwrap();
        assert!(old.ui.music);
        let said: Config = toml::from_str("[ui]\nmusic = false\n").unwrap();
        assert!(!said.ui.music);
    }

    #[test]
    fn the_pictures_own_settings_are_folded() {
        let mut config: Config = toml::from_str(
            "[ui]\nplaying_dither = false\nplaying_dither_method = \"atkinson\"\nheading_dither_fineness = 0.8\nplaying_dither_fineness = 0.2\n",
        )
        .unwrap();
        config.ui.fold_pictures();
        assert!(!config.ui.cover_dither);
        assert_eq!(config.ui.dither_method, "atkinson");
        assert_eq!(config.ui.picture_fineness(), 0.8);
        let written = toml::to_string_pretty(&config).unwrap();
        assert!(!written.contains("playing_dither"));
        assert!(!written.contains("heading_dither"));
    }

    #[test]
    fn an_unreadable_config_is_set_aside() {
        let dir = std::env::temp_dir().join(format!("kessler-aside-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "music_dirs = [\n[ui]\n").unwrap();
        let config = Config::set_aside(&path, "invalid array");
        let aside = dir.join("config.toml.kaputt");
        assert_eq!(config.set_aside.as_deref(), Some(aside.as_path()));
        assert!(!path.exists());
        assert_eq!(std::fs::read_to_string(&aside).unwrap(), "music_dirs = [\n[ui]\n");
        assert!(config.music_dirs.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_config_says_its_language_or_nothing() {
        assert!(!said_language("[ui]\nground = \"nord\"\n"));
        assert!(!said_language("language = \"en\"\n"));
        assert!(said_language("[ui]\nlanguage = \"\"\n"));
        assert!(said_language("[ui]\nlanguage = \"en\"\n"));
        assert!(!said_language("not toml at all ["));
    }

    #[test]
    fn a_heading_cover_keeps_to_its_sizes() {
        let mut ui = UiConfig::default();
        assert_eq!(ui.heading_cover(), HEADING_COVER_DEFAULT);
        ui.heading_cover_size = 900.0;
        assert_eq!(ui.heading_cover(), HEADING_COVER_MOST);
        ui.heading_cover_size = f32::NAN;
        assert_eq!(ui.heading_cover(), HEADING_COVER_DEFAULT);
    }

    #[test]
    fn an_older_settings_file_keeps_its_pictures() {
        let mut ui: UiConfig = toml::from_str("cover_dither = true\ndither_fineness = 0.3\n").unwrap();
        ui.fold_pictures();
        assert!(ui.cover_dither);
        assert!((ui.picture_fineness() - 0.3).abs() < 1e-6);
    }

    #[test]
    fn an_existing_library_moves_to_the_new_name() {
        let root = std::env::temp_dir().join("kessler-adopt-move");
        let _ = std::fs::remove_dir_all(&root);
        let old = root.join("cratedig");
        let new = root.join("kessler");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("library.db"), b"a library").unwrap();

        assert!(adopt(&old, &new));
        assert!(!old.exists());
        assert_eq!(std::fs::read(new.join("library.db")).unwrap(), b"a library");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_existing_new_folder_is_never_replaced() {
        let root = std::env::temp_dir().join("kessler-adopt-keep");
        let _ = std::fs::remove_dir_all(&root);
        let old = root.join("cratedig");
        let new = root.join("kessler");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(old.join("library.db"), b"old").unwrap();
        std::fs::write(new.join("library.db"), b"current").unwrap();

        assert!(!adopt(&old, &new));
        assert_eq!(std::fs::read(new.join("library.db")).unwrap(), b"current");
        assert!(old.exists());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn nothing_to_adopt_is_fine() {
        let root = std::env::temp_dir().join("kessler-adopt-none");
        let _ = std::fs::remove_dir_all(&root);
        assert!(!adopt(&root.join("cratedig"), &root.join("kessler")));
    }
}
