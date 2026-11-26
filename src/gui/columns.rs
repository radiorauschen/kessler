//! Which fields the track list shows.
//!
//! The title is always there — a list of tracks without titles is not a list of
//! anything. Everything else is optional, and off by default: a column you have
//! to read past on every row costs more than the fact it carries, unless you
//! asked for it.
//!
//! Only the columns actually shown can be sorted by, which is the point. There
//! is no hidden sort order, and no head sitting there offering to sort by
//! something the reader cannot see.

use crate::db::models::{SortField, Track};

use super::text;

/// A field of the track list, other than the title.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackColumn {
    Number,
    Album,
    Year,
    Genre,
    Rating,
    Plays,
    Added,
    Bitrate,
    Length,
}

impl TrackColumn {
    /// Every column, in the order they appear on screen.
    ///
    /// The title is drawn between `Number` and everything after it; the order
    /// here is what the row and its heading both walk, so they cannot drift
    /// apart.
    pub const ALL: [TrackColumn; 9] = [
        Self::Number,
        Self::Album,
        Self::Year,
        Self::Genre,
        Self::Rating,
        Self::Plays,
        Self::Added,
        Self::Bitrate,
        Self::Length,
    ];

    /// What the interface starts with: the number, the title, the length.
    pub const DEFAULT: [TrackColumn; 2] = [Self::Number, Self::Length];

    /// The order columns are left out in where the track list is too narrow
    /// for all of them: first what the columns beside it say already or
    /// what is seldom looked for — the genre, the album the album column
    /// shows, the bitrate, when it came, how often and how well liked —
    /// then the year, and the number and the length, which the list is read
    /// by, last.
    pub const FIRST_TO_GO: [TrackColumn; 9] = [
        Self::Genre,
        Self::Bitrate,
        Self::Album,
        Self::Added,
        Self::Plays,
        Self::Rating,
        Self::Year,
        Self::Length,
        Self::Number,
    ];

    /// The name in the config file, and the label over the column.
    pub fn name(self) -> &'static str {
        match self {
            Self::Number => "nr",
            Self::Album => "album",
            Self::Year => "jahr",
            Self::Genre => "genre",
            Self::Rating => "wertung",
            Self::Plays => "plays",
            Self::Added => "hinzugefügt",
            Self::Bitrate => "bitrate",
            Self::Length => "länge",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        let name = name.trim().to_lowercase();
        Self::ALL.into_iter().find(|c| c.name() == name)
    }

    /// How wide the column is. Figures get exactly the room their digits need.
    pub fn width(self) -> f32 {
        match self {
            Self::Number => 26.0,
            Self::Album => 150.0,
            Self::Year => 38.0,
            Self::Genre => 104.0,
            Self::Rating => 64.0,
            Self::Plays => 42.0,
            Self::Added => 82.0,
            Self::Bitrate => 52.0,
            Self::Length => 52.0,
        }
    }

    /// Figures are right-aligned so their digits line up down the column; words
    /// are left-aligned so their beginnings do. The track number is the
    /// exception: it starts the row, so it stands at the row's left edge —
    /// under the cover, with the records in the list — rather than wherever
    /// its digits end. Right-aligned where that is asked for — see
    /// `UiConfig::numbers_right`.
    pub fn right_aligned(self) -> bool {
        match self {
            Self::Number => numbers_right(),
            _ => !matches!(self, Self::Genre | Self::Album),
        }
    }

    /// Whether this column is figures, and so belongs in the mono cut.
    ///
    /// The test is not "does it contain digits" but "is it read down the
    /// column": a number, a year, a count and a running time all line up under
    /// one another and get compared, and in a proportional face the same four
    /// digits are a different width in every row — so the column shivers
    /// instead of lining up. `wertung` is stars rather than figures, and
    /// `genre` and `album` are words.
    ///
    /// `hinzugefügt` is the exception: it mostly reads
    /// "heute", "gestern", "vor 3 tagen" — words with a figure in them rather
    /// than a figure — and a date has nothing under it worth lining up with.
    pub fn figures(self) -> bool {
        matches!(self, Self::Number | Self::Year | Self::Plays | Self::Bitrate | Self::Length)
    }

    /// What a row's cell of this column is set in: figures in the mono role,
    /// the length at the label's size where that is asked for, words as the
    /// title.
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

    /// Whether this says something about the record rather than the track.
    ///
    /// With the albums in the track list these move up into each record's
    /// heading, beside its cover, and leave the rows to what differs from
    /// track to track — how long, how often played, how rated. The album's
    /// own name is the heading itself and is not repeated at all.
    pub fn per_record(self) -> bool {
        matches!(self, Self::Album | Self::Year | Self::Genre | Self::Added)
    }

    /// Clicking the head sorts by this.
    pub fn sort_field(self) -> SortField {
        match self {
            Self::Number => SortField::TrackNumber,
            Self::Album => SortField::Album,
            Self::Year => SortField::Year,
            Self::Genre => SortField::Genre,
            Self::Rating => SortField::Rating,
            Self::Plays => SortField::PlayCount,
            Self::Added => SortField::DateAdded,
            Self::Bitrate => SortField::Bitrate,
            Self::Length => SortField::Duration,
        }
    }

    /// The value for one track, already formatted.
    ///
    /// An empty string where there is nothing to say — a rating of zero is not
    /// a rating, and a play count of zero is not a fact worth a column inch.
    ///
    /// It takes the track and nothing else. It used to take the row as well,
    /// so the number could fall back to a position when the tags had none, and
    /// a cell whose value depends on where it is drawn is a cell that can lie
    /// about the file.
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
            Self::Bitrate => track
                .bitrate
                .filter(|kbit| *kbit > 0)
                .map(|kbit| kbit.to_string())
                .unwrap_or_default(),
            Self::Length => text::duration(track.duration_secs as u64),
        }
    }
}

/// A rating in whole stars, „★★★"; nothing for none.
pub fn stars(rating: u8) -> String {
    "★".repeat(usize::from(rating.min(5)))
}

/// What to say about a file's format, beside its title — when there is
/// anything to say at all.
///
/// Nothing for the ordinary lossy ones. A mark on every row is a mark nobody
/// reads, and „mp3" beside nine hundred titles is a column of noise that says
/// only. The lossless formats get their name, because that is the one you
/// actually want to pick out of a row: the big file, the good copy, and — for
/// flac — the one no iPod will play, so the mark answers at a glance.
///
/// Read off the extension, which is all there is to read: a `.m4a` may hold
/// Apple Lossless or plain AAC and the name does not say, so it stays silent
/// rather than guessing.
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

/// The air between the track number and the title: half what stands between
/// other columns, since the number stands at the left and its column is as wide
/// as three digits whatever the row's are.
pub const NUMBER_GAP: f32 = 10.0;

/// Whether the track numbers stand right-aligned this frame — see
/// `UiConfig::numbers_right`. Set once a frame from the config, read where
/// the columns are laid out, which knows nothing of the config.
static NUMBERS_RIGHT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_numbers_right(on: bool) {
    NUMBERS_RIGHT.store(on, std::sync::atomic::Ordering::Relaxed);
}

fn numbers_right() -> bool {
    NUMBERS_RIGHT.load(std::sync::atomic::Ordering::Relaxed)
}

/// Whether the length stands at the label's size this frame — see
/// `UiConfig::small_length`; set once a frame, as the numbers' side is.
static SMALL_LENGTH: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_small_length(on: bool) {
    SMALL_LENGTH.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// The air after the track number: [`NUMBER_GAP`] beside a number at the
/// left, the other columns' own beside one at the right, as it stood.
pub fn number_gap(cell_gap: f32) -> f32 {
    if numbers_right() {
        cell_gap
    } else {
        NUMBER_GAP
    }
}

/// The least room the title keeps however many columns are switched on —
/// see [`TrackColumns::fitting`]. Enough for a short title whole, or the
/// start of a long one to know it by.
pub const TITLE_LEAST: f32 = 140.0;

/// The chosen set, kept in the order [`TrackColumn::ALL`] defines.
///
/// Order is not the user's to choose. Letting it be would mean a row whose
/// figures sit somewhere different from every screenshot and every memory of
/// it, for no gain — the interesting choice is which fields, not where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackColumns(Vec<TrackColumn>);

impl Default for TrackColumns {
    fn default() -> Self {
        Self(TrackColumn::DEFAULT.to_vec())
    }
}

impl TrackColumns {
    /// Read a set back from the config file, ignoring anything unrecognised.
    ///
    /// An empty or unreadable list falls back to the default rather than to a
    /// list with nothing but titles in it: a config file with a typo in it
    /// should not leave someone staring at a column of bare names.
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

    /// Turn one column on or off, keeping the canonical order.
    ///
    /// Turning the last one off is allowed: a list of titles alone is a
    /// legitimate thing to want, and the title is never at risk.
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

    /// The column drawn to the left of the title, if any.
    pub fn leading(&self) -> Option<TrackColumn> {
        self.0.first().copied().filter(|c| *c == TrackColumn::Number)
    }

    /// The columns drawn to the right of the title.
    pub fn trailing(&self) -> impl Iterator<Item = TrackColumn> + '_ {
        self.0.iter().copied().filter(|c| *c != TrackColumn::Number)
    }

    /// The same set without the columns a record's heading says instead —
    /// what the rows carry while the albums are in the track list.
    pub fn for_rows(&self) -> Self {
        Self(self.0.iter().copied().filter(|c| !c.per_record()).collect())
    }

    /// The same set with one column switched on or off, in the canonical
    /// order — for the combined album and track column, which keeps its own
    /// „nr", „bitrate" and „länge" whatever the plain track list has.
    pub fn with(&self, column: TrackColumn, on: bool) -> Self {
        Self(
            TrackColumn::ALL
                .into_iter()
                .filter(|c| if *c == column { on } else { self.contains(*c) })
                .collect(),
        )
    }

    /// The columns a record's heading says, in the order it says them. The
    /// album's name is left out: it is the heading.
    pub fn on_heading(&self) -> impl Iterator<Item = TrackColumn> + '_ {
        self.0
            .iter()
            .copied()
            .filter(|c| c.per_record() && *c != TrackColumn::Album)
    }

    /// The set that fits in `room`, leaving the title at least [`TITLE_LEAST`]:
    /// the columns switched on that do not fit are left out, the least telling
    /// first, until it does — else they would run over the queue beside them.
    /// What is left out stays switched on, and comes back as the column grows.
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

    /// How much width the title is not getting.
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

    /// A column switched on lands in its place; switched off, it is gone;
    /// the rest are untouched either way.
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

    /// Too narrow for every column switched on, the list leaves out the
    /// least telling until the title has its room, and gives them back as
    /// it widens; every column has its turn to go.
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

    /// What the interface looks like the first time it is opened.
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

    /// Whatever order they were switched on in, they appear in the same places.
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

    /// The number is the only column that belongs left of the title.
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

    /// With the albums in the list, what belongs to the record goes up into
    /// its heading and the rest stays by the titles.
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

    /// A config file is written by hand sometimes, and a typo in it should
    /// cost nothing.
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

    /// A file with no track number says so by saying nothing.
    ///
    /// The bug this exists for cost an evening: the cell used to fall
    /// back to the row's own position, so a file with no number showed a
    /// perfectly plausible one — and every edit to its tags changed nothing on
    /// screen, because nothing on screen had come from the tags.
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

        assert_eq!(TrackColumn::Bitrate.value(&track), "");
        track.bitrate = Some(0);
        assert_eq!(TrackColumn::Bitrate.value(&track), "");
        track.bitrate = Some(1411);
        assert_eq!(TrackColumn::Bitrate.value(&track), "1411");
    }

    /// Only the lossless formats are marked, and the mark is the format.
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
