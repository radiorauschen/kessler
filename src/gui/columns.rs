use crate::db::models::{SortField, Track};

use super::text;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackColumn {
    Number,
    Album,
    Year,
    Genre,
    Rating,
    Plays,
    Added,
    Length,
}

impl TrackColumn {
    pub const ALL: [TrackColumn; 8] = [
        Self::Number,
        Self::Album,
        Self::Year,
        Self::Genre,
        Self::Rating,
        Self::Plays,
        Self::Added,
        Self::Length,
    ];

    pub const DEFAULT: [TrackColumn; 2] = [Self::Number, Self::Length];

    pub const FIRST_TO_GO: [TrackColumn; 8] = [
        Self::Genre,
        Self::Album,
        Self::Added,
        Self::Plays,
        Self::Rating,
        Self::Year,
        Self::Length,
        Self::Number,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Number => "nr",
            Self::Album => "album",
            Self::Year => "jahr",
            Self::Genre => "genre",
            Self::Rating => "wertung",
            Self::Plays => "plays",
            Self::Added => "hinzugefügt",
            Self::Length => "länge",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        let name = name.trim().to_lowercase();
        Self::ALL.into_iter().find(|c| c.name() == name)
    }

    pub fn width(self) -> f32 {
        match self {
            Self::Number => 26.0,
            Self::Album => 150.0,
            Self::Year => 38.0,
            Self::Genre => 104.0,
            Self::Rating => 64.0,
            Self::Plays => 42.0,
            Self::Added => 82.0,
            Self::Length => 52.0,
        }
    }

    pub fn right_aligned(self) -> bool {
        match self {
            Self::Number => numbers_right(),
            _ => !matches!(self, Self::Genre | Self::Album),
        }
    }

    pub fn figures(self) -> bool {
        matches!(self, Self::Number | Self::Year | Self::Plays | Self::Length)
    }

    pub fn role(self) -> crate::gui::theme::Role {
        use crate::gui::theme::Role;
        if self == Self::Length && SMALL_LENGTH.load(std::sync::atomic::Ordering::Relaxed) {
            Role::FiguresLabel
        } else if self.figures() {
            Role::Figures
        } else {
            Role::Content
        }
    }

    pub fn per_record(self) -> bool {
        matches!(self, Self::Album | Self::Year | Self::Genre | Self::Added)
    }

    pub fn sort_field(self) -> SortField {
        match self {
            Self::Number => SortField::TrackNumber,
            Self::Album => SortField::Album,
            Self::Year => SortField::Year,
            Self::Genre => SortField::Genre,
            Self::Rating => SortField::Rating,
            Self::Plays => SortField::PlayCount,
            Self::Added => SortField::DateAdded,
            Self::Length => SortField::Duration,
        }
    }

    pub fn value(self, track: &Track) -> String {
        match self {
            Self::Number => track
                .track_number
                .map(|n| n.to_string())
                .unwrap_or_default(),
            Self::Album => track.album.clone(),
            Self::Year => track.year.map(|y| y.to_string()).unwrap_or_default(),
            Self::Genre => track.genre.clone(),
            Self::Rating => stars(track.rating),
            Self::Plays => {
                if track.play_count > 0 {
                    text::count(track.play_count as u64)
                } else {
                    String::new()
                }
            }
            Self::Added => track.date_added.format("%d.%m.%Y").to_string(),
            Self::Length => text::duration(track.duration_secs as u64),
        }
    }
}

pub fn stars(rating: u8) -> String {
    "★".repeat(usize::from(rating.min(5)))
}

pub fn format_mark(path: &std::path::Path) -> Option<&'static str> {
    match path
        .extension()?
        .to_str()?
        .to_ascii_lowercase()
        .as_str()
    {
        "flac" => Some("flac"),
        "wav" => Some("wav"),
        "aif" | "aiff" | "aifc" => Some("aiff"),
        _ => None,
    }
}

pub const NUMBER_GAP: f32 = 10.0;

static NUMBERS_RIGHT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_numbers_right(on: bool) {
    NUMBERS_RIGHT.store(on, std::sync::atomic::Ordering::Relaxed);
}

fn numbers_right() -> bool {
    NUMBERS_RIGHT.load(std::sync::atomic::Ordering::Relaxed)
}

static SMALL_LENGTH: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_small_length(on: bool) {
    SMALL_LENGTH.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub fn number_gap(cell_gap: f32) -> f32 {
    if numbers_right() {
        cell_gap
    } else {
        NUMBER_GAP
    }
}

pub const TITLE_LEAST: f32 = 140.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackColumns(Vec<TrackColumn>);

impl Default for TrackColumns {
    fn default() -> Self {
        Self(TrackColumn::DEFAULT.to_vec())
    }
}

impl TrackColumns {
    pub fn from_names(names: &[String]) -> Self {
        let mut chosen: Vec<TrackColumn> = TrackColumn::ALL
            .into_iter()
            .filter(|c| names.iter().any(|n| TrackColumn::from_name(n) == Some(*c)))
            .collect();
        if chosen.is_empty() {
            chosen = TrackColumn::DEFAULT.to_vec();
        }
        Self(chosen)
    }

    pub fn names(&self) -> Vec<String> {
        self.0.iter().map(|c| c.name().to_string()).collect()
    }

    pub fn contains(&self, column: TrackColumn) -> bool {
        self.0.contains(&column)
    }

    pub fn toggle(&mut self, column: TrackColumn) {
        if self.contains(column) {
            self.0.retain(|c| *c != column);
        } else {
            self.0 = TrackColumn::ALL
                .into_iter()
                .filter(|c| *c == column || self.contains(*c))
                .collect();
        }
    }

    pub fn leading(&self) -> Option<TrackColumn> {
        self.0.first().copied().filter(|c| *c == TrackColumn::Number)
    }

    pub fn trailing(&self) -> impl Iterator<Item = TrackColumn> + '_ {
        self.0.iter().copied().filter(|c| *c != TrackColumn::Number)
    }

    pub fn for_rows(&self) -> Self {
        Self(self.0.iter().copied().filter(|c| !c.per_record()).collect())
    }

    pub fn with(&self, column: TrackColumn, on: bool) -> Self {
        Self(
            TrackColumn::ALL
                .into_iter()
                .filter(|c| if *c == column { on } else { self.contains(*c) })
                .collect(),
        )
    }

    pub fn on_heading(&self) -> impl Iterator<Item = TrackColumn> + '_ {
        self.0
            .iter()
            .copied()
            .filter(|c| c.per_record() && *c != TrackColumn::Album)
    }

    pub fn fitting(&self, room: f32, cell_gap: f32) -> Self {
        let mut fit = self.clone();
        for column in TrackColumn::FIRST_TO_GO {
            if room - fit.reserved(cell_gap) >= TITLE_LEAST {
                break;
            }
            fit.0.retain(|c| *c != column);
        }
        fit
    }

    pub fn reserved(&self, cell_gap: f32) -> f32 {
        self.0
            .iter()
            .map(|c| c.width() + if *c == TrackColumn::Number { number_gap(cell_gap) } else { cell_gap })
            .sum::<f32>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_switches_one_column_in_place() {
        let plays = TrackColumns::from_names(&["plays".into(), "länge".into()]);
        let numbered = plays.with(TrackColumn::Number, true);
        assert_eq!(numbered.names(), vec!["nr", "plays", "länge"]);
        assert_eq!(numbered.leading(), Some(TrackColumn::Number));
        let short = numbered.with(TrackColumn::Length, false);
        assert_eq!(short.names(), vec!["nr", "plays"]);
        assert_eq!(short.with(TrackColumn::Length, false), short);
    }

    #[test]
    fn a_narrow_list_leaves_columns_out() {
        let all = TrackColumns::from_names(&TrackColumn::ALL.map(|c| c.name().to_string()));
        let wide = all.reserved(12.0) + TITLE_LEAST;
        assert_eq!(all.fitting(wide, 12.0), all);
        let narrower = all.fitting(wide - 1.0, 12.0);
        assert!(!narrower.contains(TrackColumn::Genre));
        assert!(narrower.contains(TrackColumn::Album));
        let narrow = all.fitting(400.0, 12.0);
        assert!(narrow.contains(TrackColumn::Number) && narrow.contains(TrackColumn::Length));
        assert!(400.0 - narrow.reserved(12.0) >= TITLE_LEAST);
        assert_eq!(all.fitting(0.0, 12.0).names(), Vec::<String>::new());
        for column in TrackColumn::ALL {
            assert!(TrackColumn::FIRST_TO_GO.contains(&column), "{column:?}");
        }
    }

    #[test]
    fn starts_with_number_and_length_only() {
        let columns = TrackColumns::default();
        assert!(columns.contains(TrackColumn::Number));
        assert!(columns.contains(TrackColumn::Length));
        for off in [
            TrackColumn::Rating,
            TrackColumn::Plays,
            TrackColumn::Year,
            TrackColumn::Genre,
            TrackColumn::Added,
        ] {
            assert!(!columns.contains(off), "{} should start off", off.name());
        }
    }

    #[test]
    fn order_on_screen_never_depends_on_the_order_chosen() {
        let mut a = TrackColumns(vec![]);
        a.toggle(TrackColumn::Length);
        a.toggle(TrackColumn::Year);
        a.toggle(TrackColumn::Number);

        let mut b = TrackColumns(vec![]);
        b.toggle(TrackColumn::Number);
        b.toggle(TrackColumn::Year);
        b.toggle(TrackColumn::Length);

        assert_eq!(a, b);
        assert_eq!(a.names(), vec!["nr", "jahr", "länge"]);
    }

    #[test]
    fn toggling_twice_puts_it_back() {
        let before = TrackColumns::default();
        let mut after = before.clone();
        after.toggle(TrackColumn::Plays);
        assert_ne!(before, after);
        after.toggle(TrackColumn::Plays);
        assert_eq!(before, after);
    }

    #[test]
    fn only_the_number_leads() {
        let columns = TrackColumns::default();
        assert_eq!(columns.leading(), Some(TrackColumn::Number));
        assert_eq!(
            columns.trailing().collect::<Vec<_>>(),
            vec![TrackColumn::Length]
        );

        let mut without = columns.clone();
        without.toggle(TrackColumn::Number);
        assert_eq!(without.leading(), None);
    }

    #[test]
    fn the_record_s_own_columns_go_up_into_its_heading() {
        let mut columns = TrackColumns::default();
        for c in [
            TrackColumn::Album,
            TrackColumn::Year,
            TrackColumn::Genre,
            TrackColumn::Plays,
            TrackColumn::Rating,
        ] {
            columns.toggle(c);
        }
        assert_eq!(
            columns.for_rows().names(),
            vec!["nr", "wertung", "plays", "länge"]
        );
        assert_eq!(
            columns.on_heading().collect::<Vec<_>>(),
            vec![TrackColumn::Year, TrackColumn::Genre]
        );
    }

    #[test]
    fn unreadable_names_fall_back_rather_than_empty_the_list() {
        let names: Vec<String> = vec!["länge".into(), "nonsense".into()];
        assert_eq!(
            TrackColumns::from_names(&names).names(),
            vec!["länge".to_string()]
        );
        assert_eq!(
            TrackColumns::from_names(&["nonsense".to_string()]),
            TrackColumns::default()
        );
        assert_eq!(TrackColumns::from_names(&[]), TrackColumns::default());
    }

    #[test]
    fn a_missing_track_number_is_not_invented() {
        let mut track = Track {
            id: 0,
            path: std::path::PathBuf::from("/m/a.flac"),
            title: "antibiotics".into(),
            artist: "oneida".into(),
            album_artist: "oneida".into(),
            album: "each one teach one".into(),
            genre: String::new(),
            year: Some(2002),
            track_number: Some(4),
            disc_number: Some(1),
            duration_secs: 996.0,
            bitrate: None,
            sample_rate: None,
            file_size: 1,
            format: "flac".into(),
            rating: 0,
            play_count: 0,
            last_played: None,
            date_added: Default::default(),
            date_modified: Default::default(),
            file_hash: None,
            has_artwork: false,
        };
        assert_eq!(TrackColumn::Number.value(&track), "4");

        track.track_number = None;
        assert_eq!(
            TrackColumn::Number.value(&track),
            "",
            "a file with no number was given a number anyway"
        );
    }

    #[test]
    fn the_mark_names_the_lossless_ones_and_stays_quiet_otherwise() {
        use std::path::Path;
        let mark = |name: &str| format_mark(Path::new(name));

        assert_eq!(mark("/music/a/01 song.flac"), Some("flac"));
        assert_eq!(mark("/music/a/01 song.wav"), Some("wav"));
        assert_eq!(mark("/music/a/01 song.aif"), Some("aiff"));
        assert_eq!(mark("/music/a/01 song.aiff"), Some("aiff"));
        assert_eq!(mark("/music/a/01 song.aifc"), Some("aiff"));
        assert_eq!(mark("/music/a/01 song.FLAC"), Some("flac"));

        assert_eq!(mark("/music/a/01 song.mp3"), None);
        assert_eq!(mark("/music/a/01 song.m4a"), None);
        assert_eq!(mark("/music/a/01 song.ogg"), None);

        assert_eq!(mark("/music/a/01 song"), None);
        assert_eq!(mark(""), None);
        assert_eq!(mark("/music/v1.0/song"), None);
    }

    #[test]
    fn names_round_trip() {
        for column in TrackColumn::ALL {
            assert_eq!(TrackColumn::from_name(column.name()), Some(column));
        }
        assert_eq!(TrackColumn::from_name("  LÄNGE "), Some(TrackColumn::Length));
        assert_eq!(TrackColumn::from_name("titel"), None);
    }
}
