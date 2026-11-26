//! The design system, as code.
//!
//! The whole interface is built from a deliberately tiny set of means:
//!
//! - **two type sizes** — 15px for content, 11px for labels. No third size.
//! - **two weights** — regular and bold.
//! - **three tonal values** plus one accent.
//! - **no lines**, except the two-pixel progress rule, which carries
//!   information nothing else carries.
//! - **lowercase throughout** — see [`lower`].
//!
//! Because size is off the table as a device, hierarchy has to come from
//! somewhere else. Four axes carry it, and each does exactly one job:
//! weight says what is selected, tone says what recedes, letter spacing says
//! what is a label rather than content, and the marker behind a row says where
//! the selection sits. Keeping them separate is what makes this a system
//! instead of decoration.

use eframe::egui::{self, Color32, FontFamily, FontId};

pub const SIZE_CONTENT: f32 = 15.0;
pub const SIZE_LABEL: f32 = 11.0;

/// Letter spacing on labels, in pixels at label size. The one signal that
/// separates a label from content without changing size or weight.
///
/// Enough to read as tracking, little enough that a two-word heading still
/// reads as two words rather than as a row of letters.
pub const LABEL_TRACKING: f32 = 1.5;

/// How much of [`LABEL_TRACKING`] the face in use takes: Muto's labels a touch
/// less tracked than Asap's, its letters stand further apart as drawn.
fn label_tracking_share(key: &str) -> f32 {
    match key {
        "muto" | "mutomono" => 0.75,
        _ => 1.0,
    }
}

/// How far a record heading's second line stands under its name, as a share of
/// a row: Muto's small letters are short beside its capitals and figures, so a
/// full row between the two looked like a gap.
fn head_line_share(key: &str) -> f32 {
    match key {
        "muto" | "mutomono" => 0.84,
        _ => 1.0,
    }
}

static HEAD_LINE_SHARE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x3F80_0000);

/// See [`head_line_share`], for the face in use.
pub fn head_line() -> f32 {
    f32::from_bits(HEAD_LINE_SHARE.load(std::sync::atomic::Ordering::Relaxed))
}

/// The share for the face in use, as bits — set with the face.
static LABEL_TRACKING_SHARE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x3F80_0000);

/// Just enough rounding to stop the marker's corners looking cut. Not a button.
pub const MARKER_RADIUS: u8 = 2;

/// The corners of the plate a row wears — under the pointer, under the
/// keyboard, in the menu and the right-click menu — a little rounder than a
/// picture's (`MARKER_RADIUS`): a cover sits inside a record's plate, and the
/// same corner outside as in looked pinched. One for every row, picture in it
/// or not, so the plates of one column are alike.
pub const ROW_RADIUS: f32 = 4.0;

/// How far up and down the selection keeps clear of a column's edge, in rows.
///
/// Without it the selection walks straight out of the viewport and the list
/// scrolls only once the row is already gone.
pub const SCROLL_MARGIN_ROWS: f32 = 3.0;

/// The measurements that depend on how much room there is.
///
/// A short window should show more rows rather than the same rows with the same
/// air around them, so the leading tightens as the window loses height. The two
/// sizes never change — type size is a fixed part of the design; only the space
/// around the type gives.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    /// One row of a list, from the top of one to the top of the next.
    pub row: f32,
    /// Air inside the marker, left and right of the text. Without it the
    /// marker ends exactly where the text does and reads as a block of colour
    /// rather than a stroke drawn over a line.
    pub marker_pad_x: f32,
    /// How far the marker stays clear of the row's top and bottom edge.
    pub marker_inset_y: f32,
    /// The two fixed-width name columns.
    pub artists_width: f32,
    pub albums_width: f32,
    /// The whitespace that stands in for the rules between columns.
    pub column_gap: f32,
    /// The margin down each side of the window.
    pub margin_x: f32,
}

/// Below this width the three columns stop standing side by side and start
/// sliding: one at a time, full width, as on an iPod.
///
/// Three columns need 110 + 110 + 240 of content plus the gaps and the
/// margins before anything is readable; under that the window is a phone held
/// sideways, and the answer is the one an iPod gave.
pub const NARROW_UNTIL: f32 = 640.0;

/// How far apart two columns stand while they slide — never seen at rest,
/// only on the way from one to the next.
pub const SLIDE_GAP: f32 = 24.0;

/// How wide the whole strip of columns is, laid end to end.
///
/// `count` columns with a gap *between* them — and not after the last one.
pub fn strip_width(count: usize, width: f32, gap: f32) -> f32 {
    if count == 0 {
        return 0.0;
    }
    count as f32 * width + (count - 1) as f32 * gap
}

/// How far the strip of columns is pushed left so that column `index` fills
/// the window.
///
/// One column, the whole width, and nothing of its neighbours. They used to
/// show as a sliver at each edge, which on a small screen was a sliver of
/// unreadable words taken out of the one column being read; the way to them is
/// now the line of arrows under it — see
/// `library::column_steps`.
pub fn slide_offset(index: usize, count: usize, width: f32, gap: f32) -> f32 {
    if count == 0 {
        return 0.0;
    }
    index.min(count - 1) as f32 * (width + gap)
}

/// The tallest and tightest a row is allowed to be.
///
/// One point more than it was, at both ends: fifteen point type on a
/// twenty-one point line rather than a twenty point one. A point is the whole
/// range here — every measure is rounded to the pixel grid, so there is no
/// smaller step to take — and at a dozen rows down a column it is the
/// difference between a list and a block.
pub const ROW_ROOMY: f32 = 21.0;
const ROW_TIGHT: f32 = 18.0;

/// Between these two widths the columns give up their air; outside them
/// nothing more changes.
const WIDE: f32 = 1280.0;
const NARROW: f32 = 820.0;

/// Interpolate between the roomy and the cramped value.
fn between(t: f32, cramped: f32, roomy: f32) -> f32 {
    (cramped + t * (roomy - cramped)).round()
}

/// The points every row gets above and below its words: all of it to the
/// plate, so the gap between two plates stays as it was, and as much air
/// above and below the words as the plate keeps left and right of them.
pub const ROW_AIR: f32 = 4.0;

/// The sides at most: „menü"'s plate as far from the window's right edge as
/// „super kompakt" sets it from the top on a Mac — 24 less the plate's eight.
const MARGIN_TIGHT: f32 = 24.0;

/// How much less than twice the side the words of two columns stand apart.
const TIGHT_GAP_LESS: f32 = 2.0;

impl Metrics {
    /// The same measurements with the sides never wider than `MARGIN_TIGHT`, in
    /// any view, and the columns' gap to match.
    pub fn with_margins(mut self) -> Self {
        self.margin_x = self.margin_x.min(MARGIN_TIGHT);
        let twice = 2.0 * self.margin_x - 2.0 * self.marker_pad_x - TIGHT_GAP_LESS;
        self.column_gap = self.column_gap.min(twice);
        self
    }

    /// The sides on the window's buttons: where a Mac's buttons stand in the
    /// band (its own, or kessler's circles in their place), the first one's
    /// left edge is where the columns' words begin — the masthead's field and
    /// every list on one line down from it, and the right side as far in.
    /// A hair nearer the edge than the margin alone would set them.
    pub fn on_the_buttons(mut self, first_left: f32) -> Self {
        if first_left <= self.marker_pad_x {
            return self;
        }
        self.margin_x = first_left;
        let twice = 2.0 * self.margin_x - 2.0 * self.marker_pad_x - TIGHT_GAP_LESS;
        self.column_gap = self.column_gap.min(twice);
        self
    }

    /// The same measurements with `ROW_AIR` more to every row.
    pub fn with_air(mut self) -> Self {
        self.row += ROW_AIR;
        self
    }

    /// Work the measurements out from the size of the window.
    ///
    /// Two separate questions: height decides how much air a row gets, width
    /// decides how much of it the name columns keep. They are independent — a
    /// tall narrow window is a real shape and should not be treated as small.
    pub fn for_size(width: f32, height: f32) -> Self {
        let tall = ((height - 440.0) / 380.0).clamp(0.0, 1.0);
        let wide = ((width - NARROW) / (WIDE - NARROW)).clamp(0.0, 1.0);
        let row = between(tall, ROW_TIGHT, ROW_ROOMY);

        Self {
            row,
            marker_pad_x: 6.0,
            marker_inset_y: (row * 0.07).round().max(1.0),
            artists_width: between(wide, 158.0, 248.0),
            albums_width: between(wide, 176.0, 288.0),
            column_gap: between(wide, 26.0, 48.0),
            margin_x: between(wide, 22.0, 40.0),
        }
    }

    /// Work them out from the height alone, for callers that have no width.
    ///
    /// Only the tests ask this — they vary the height to check the row pitch
    /// and hold the width still.
    #[cfg(test)]
    pub fn for_height(height: f32) -> Self {
        Self::for_size(WIDE, height)
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::for_size(1440.0, 900.0)
    }
}

/// How narrow a name column may be dragged, and how much the track list keeps
/// whatever happens to the other two.
///
/// The floor is not politeness: a column narrower than this shows no name, only
/// the beginning of one, and a window where the track list has been squeezed
/// out is a window somebody has to drag back before they can use it.
pub const MIN_NAME_COLUMN: f32 = 110.0;
pub const MIN_TRACK_COLUMN: f32 = 240.0;

/// What a name column may become, given what else is on the row.
///
/// `others` is everything else the row spends width on — the other name column
/// and the gaps. Returns the nearest width to `wanted` that leaves the track
/// list its floor, and never goes below the column's own.
pub fn fit_column(wanted: f32, others: f32, total: f32) -> f32 {
    let most = (total - others - MIN_TRACK_COLUMN).max(MIN_NAME_COLUMN);
    wanted.clamp(MIN_NAME_COLUMN, most)
}

/// What the queue column asks for before anybody has dragged it.
///
/// Narrower than a name column, because what stands in it is a title and a
/// name and nothing else — no year, no running time, no count. Wide enough
/// that a long title is recognisable before it is cut.
pub const QUEUE_WIDTH: f32 = 268.0;

/// What the person has dragged, as shares of the row.
///
/// `None` means "work it out from the window", which is what a column does
/// until somebody moves it.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Shares {
    pub artists: Option<f32>,
    pub albums: Option<f32>,
    pub queue: Option<f32>,
    /// The column right of the queue — the iPod, the information, the log —
    /// when it stands in a column of its own. Unset: as wide as the queue.
    pub side: Option<f32>,
    /// With the artists alone beside the track list: every column the same
    /// width, however many are open — `UiConfig::merged_even`.
    pub even: bool,
}

/// How wide each column is, for a row this wide.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Columns {
    pub artists: f32,
    pub albums: f32,
    /// What is left over. The track list is the one column whose content has
    /// no natural length, so it takes the remainder rather than asking for a
    /// width — which is exactly why everything else has to be a share of the
    /// row and not a number of points.
    pub tracks: f32,
    pub queue: f32,
    /// The side column, right of the queue: the iPod, the information or
    /// the log, when the combined view gives them a column of their own.
    pub side: f32,
}

impl Columns {
    /// Part of the way from one arrangement to another — for the queue sliding
    /// in or out, where the row is worked out both ways and drawn between.
    pub fn between(from: Columns, to: Columns, t: f32) -> Columns {
        let t = t.clamp(0.0, 1.0);
        let mix = |a: f32, b: f32| a + (b - a) * t;
        Columns {
            artists: mix(from.artists, to.artists),
            albums: mix(from.albums, to.albums),
            tracks: mix(from.tracks, to.tracks),
            queue: mix(from.queue, to.queue),
            side: mix(from.side, to.side),
        }
    }
}

/// How long the fourth column takes to slide in or out, in seconds, eased
/// out (see `library::slide`): most of the way is gone in the first third,
/// and the rest is it settling.
pub const FOURTH_SLIDE: f32 = 0.22;

/// What stands to the left of the track list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Names {
    /// The artists and the albums, two columns.
    Two,
    /// The artists alone: the albums are in the track list, as headings over
    /// their own tracks.
    One,
}

impl Names {
    /// How many columns stand there.
    pub fn count(self) -> usize {
        match self {
            Self::Two => 2,
            Self::One => 1,
        }
    }
}

/// Work out the whole row at once.
///
/// Gathered into one place because it used to be three: the metrics override,
/// the grid's width and the queue's were each worked out where they happened
/// to be needed, and no one of them could see what the others had decided. A
/// row is one arrangement, and this is it — which also makes it something that
/// can be checked at several window widths without a screenshot.
pub fn columns_for(
    room: f32,
    m: Metrics,
    shares: Shares,
    names: impl Into<Names>,
    queue: bool,
) -> Columns {
    let layout: Names = names.into();
    let gap = m.column_gap;
    let points = |share: Option<f32>| share.map(|s| s * room);

    let queue_wish = if queue {
        points(shares.queue).unwrap_or(QUEUE_WIDTH)
    } else {
        0.0
    };
    let queue_gap = if queue { gap } else { 0.0 };
    let round = queue_handed_round(room, m, shares.queue, layout, queue);

    let beside_artists = if layout == Names::One {
        gap + queue_wish + queue_gap
    } else {
        m.albums_width + gap * 2.0 + queue_wish + queue_gap
    };
    let artists = fit_column(
        points(shares.artists).unwrap_or(m.artists_width) + round,
        beside_artists,
        room,
    );
    let albums = if layout == Names::One {
        0.0
    } else {
        fit_column(
            points(shares.albums).unwrap_or(m.albums_width) + round,
            artists + gap * 2.0 + queue_wish + queue_gap,
            room,
        )
    };
    let names = match layout {
        Names::Two => artists + gap + albums,
        Names::One => artists,
    };
    let queue = if queue {
        fit_column(queue_wish, names + gap * 2.0, room)
    } else {
        0.0
    };
    let tracks = (room - names - queue - gap - queue_gap).max(0.0);

    Columns {
        artists,
        albums,
        tracks,
        queue,
        side: 0.0,
    }
}

/// The row with the fourth column split in two, each half open as far as it is:
/// `left` for the queue or the lists, `right` for the iPod, the information or
/// the log.
///
/// Either half alone stands where the fourth column always stood, so a row
/// with one of them looks the way it did before the split. The second one
/// takes its width out of the track list, the one column that lives on the
/// remainder — the queue keeps its width and the names keep theirs.
///
/// With `right` at nought this is [`columns_for`] part of the way open, the
/// row as it has always been drawn.
pub fn columns_with_side(
    room: f32,
    m: Metrics,
    shares: Shares,
    names: impl Into<Names>,
    left: f32,
    right: f32,
) -> Columns {
    let layout: Names = names.into();
    let gap = m.column_gap;
    let (left, right) = (left.clamp(0.0, 1.0), right.clamp(0.0, 1.0));
    if shares.even && layout == Names::One {
        return even_row(room, gap, left, right);
    }
    let open = left.max(right);
    let alone = Shares {
        queue: shares.side.or(shares.queue),
        ..shares
    };
    let with_queue = columns_for(room, m, shares, layout, true);
    let with_side = columns_for(room, m, alone, layout, true);
    let without = columns_for(room, m, shares, layout, false);
    let with = if left >= right { with_queue } else { with_side };
    let base = Columns::between(without, with, open);
    let names_width = match layout {
        Names::Two => base.artists + gap + base.albums,
        Names::One => base.artists,
    };
    let queue = with_queue.queue * left;
    let side = fit_column(
        with_side.queue,
        names_width + gap * 3.0 + with_queue.queue,
        room,
    ) * right;
    let tracks = room - names_width - gap - queue - gap * left - side - gap * right;
    let short = (MIN_TRACK_COLUMN - tracks).max(0.0) * right;
    let give = short.min((queue - MIN_NAME_COLUMN * left).max(0.0));
    let queue = queue - give;
    let tracks = (tracks + give).max(0.0);
    Columns {
        tracks,
        queue,
        side,
        ..base
    }
}

/// The combined view's even row: the artists, the track list, and the queue
/// and the side as far as each is open, all one width. A column sliding in
/// counts for as much of a column as it has slid, so the others give up their
/// room to it smoothly and the row always adds up.
fn even_row(room: f32, gap: f32, left: f32, right: f32) -> Columns {
    let gaps = gap * (1.0 + left + right);
    let each = ((room - gaps) / (2.0 + left + right)).max(0.0);
    Columns {
        artists: each,
        albums: 0.0,
        tracks: each,
        queue: each * left,
        side: each * right,
    }
}

/// How wide the window has to be for the side to have a column of its own.
/// Below this it lies over the queue, as it always did — a fifth column in
/// a window that narrow would leave the track list nothing.
pub const SIDE_FROM: f32 = NARROW_UNTIL + (QUEUE_WIDTH + 48.0) * 2.0;

/// What each column gains while the queue is put away.
///
/// A share is a fraction of the row as it stands with every column open; that
/// is what makes a share a stable thing to write down. So putting the queue
/// away leaves a strip that belongs to nobody, and it used to fall to the
/// track list entire — a quarter of the window arriving in one column, which
/// looked less like a column being hidden than like the layout being thrown.
///
/// The queue stood beside all of them, so all of them get a piece: this is
/// that piece. The track list is not counted here and does not need to be — it
/// takes whatever the others leave, and what they leave is exactly its share.
///
/// Nothing of this reaches the settings file. It is what the row is drawn at
/// while the queue is away, and it is given back the moment it returns.
pub fn queue_handed_round(
    room: f32,
    m: Metrics,
    share: Option<f32>,
    names: impl Into<Names>,
    queue_open: bool,
) -> f32 {
    if queue_open {
        return 0.0;
    }
    let strip = share.map(|s| s * room).unwrap_or(QUEUE_WIDTH) + m.column_gap;
    let drawn = names.into().count() as f32 + 1.0;
    strip / drawn
}

/// One column's share of an even split — what clicking a divider goes back to.
///
/// Not the widths the window opens on: those are fixed figures chosen to suit
/// the kind of text each column holds, and a name column is narrower than a
/// track list for good reason. An even split is the other answer, the one you
/// can ask for when a drag has left the row lopsided: half each in the grid,
/// a third each in the list, whatever the window is.
pub fn even_columns(count: usize, total: f32, gap: f32) -> f32 {
    let count = count.max(1);
    let gaps = gap * count.saturating_sub(1) as f32;
    ((total - gaps) / count as f32).max(MIN_NAME_COLUMN)
}

/// The band at the top of the window the system still owns.
///
/// On a Mac the ground runs right up to the top edge — what makes it one
/// window rather than a coloured panel under a grey bar — but the title bar is
/// still there, transparent: the three buttons float in it and it takes the
/// clicks. Nothing that has to be clicked may stand in it. Everywhere else
/// there is no such band.
///
/// The `cfg` chooses this one number and nothing else. Everything that follows
/// from it is ordinary arithmetic that compiles on every machine — which is
/// the point: a Mac-only branch is a branch that is never built where it is
/// written, and so can ship a line that does not compile.
#[cfg(target_os = "macos")]
const TITLEBAR: f32 = 28.0;
#[cfg(not(target_os = "macos"))]
const TITLEBAR: f32 = 0.0;

/// How tall that band is now, as the window says once its buttons are set — see
/// `macwindow`. [`TITLEBAR`] until then; with the buttons set in from the
/// corner, a taller one.
static TITLEBAR_NOW: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(TITLEBAR.to_bits());

/// The band at the top of the window the system owns, in points.
pub fn titlebar() -> f32 {
    f32::from_bits(TITLEBAR_NOW.load(std::sync::atomic::Ordering::Relaxed))
}

/// What the window says the band is, once its buttons are set.
pub fn set_titlebar(height: f32) {
    TITLEBAR_NOW.store(height.max(0.0).to_bits(), std::sync::atomic::Ordering::Relaxed);
}

/// The middle of a Mac's band, from the window's top, in the window's own
/// points: AppKit sets it in the screen's, which the zoom in the menu makes
/// more or fewer of the window's. Until the window says, as AppKit has it.
pub fn band_middle() -> f32 {
    match titlebar() {
        band if band > 0.0 => band / 2.0,
        _ => crate::gui::macwindow::BY_HAND_MIDDLE,
    }
}

/// Where the window's last button ends, from its left edge, when they are
/// set by hand — see `macwindow`. Nought where there are none to clear.
static BUTTONS_END: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub fn buttons_end() -> f32 {
    f32::from_bits(BUTTONS_END.load(std::sync::atomic::Ordering::Relaxed))
}

pub fn set_buttons_end(at: f32) {
    BUTTONS_END.store(at.max(0.0).to_bits(), std::sync::atomic::Ordering::Relaxed);
}

/// Where the middle of each of the window's three buttons stands, from its
/// left edge, and how wide a circle they are, in the window's own points —
/// for kessler's own circles in their place. Nothing until the window says.
static BUTTON_MARKS: std::sync::Mutex<Option<([f32; 3], f32)>> = std::sync::Mutex::new(None);

pub fn button_marks() -> Option<([f32; 3], f32)> {
    *BUTTON_MARKS.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn set_button_marks(marks: Option<([f32; 3], f32)>) {
    *BUTTON_MARKS.lock().unwrap_or_else(|e| e.into_inner()) = marks;
}

/// The top of a line `height` tall set in the band beside the buttons, on their
/// middle. Where there is no band, the window's own margin.
pub fn band_line_top(height: f32) -> f32 {
    if cfg!(target_os = "macos") {
        (band_middle() - height / 2.0).round()
    } else {
        AIR_TOP
    }
}

/// The air between that band and the masthead.
///
/// It used to be the full 34 points of the window's own top margin, added on
/// top of the title bar — so on a Mac there were two margins stacked above the
/// masthead, and the band under the three buttons read as space nobody had a
/// use for. Beneath a title bar the masthead is set the way a toolbar is set,
/// under it; with no title bar it keeps its margin.
///
/// Sixteen was a shade too close — the masthead read as pressed up against
/// the buttons — and twenty-four is where it sits well, still some ten
/// points under what it was with two margins stacked.
const AIR_UNDER_TITLEBAR: f32 = 24.0;
const AIR_TOP: f32 = 34.0;

/// The least air under a taller band: it already stands its buttons in air
/// of their own.
const LEAST_UNDER_TITLEBAR: f32 = 12.0;

/// The air above the masthead: where it stood under the plain band, or, under
/// a taller one — the buttons set in from the corner, see `macwindow` — that
/// band and a little air: where „kompakt" and a narrow window set their line
/// (the other views set it in the band, see `band_line_top`). With no band,
/// the window's own margin.
pub fn margin_top() -> f32 {
    margin_under(titlebar())
}

/// How far down the words of a line on the band's middle start their ink:
/// measured, 24 points — the sides' widest, so the corner is even.
pub const BAND_INK_TOP: f32 = 24.0;

/// The masthead's top under a line of its own where a Mac has its band — for
/// a window without one that still wants the switch and „menü" up there.
pub fn margin_under_line() -> f32 {
    margin_under(band_middle() * 2.0)
}

/// The top of a line `height` tall on the middle of a Mac's band, whether or
/// not the window has one.
pub fn line_on_band_middle(height: f32) -> f32 {
    (band_middle() - height / 2.0).round()
}

fn margin_under(band: f32) -> f32 {
    if band > 0.0 {
        (band + LEAST_UNDER_TITLEBAR).max(TITLEBAR.max(28.0) + AIR_UNDER_TITLEBAR)
    } else {
        AIR_TOP
    }
}

/// The air below the foot.
pub const MARGIN_BOTTOM: f32 = 28.0;

/// How far a floating sheet stands in from the top of the window, given how
/// far it stands in from the side.
///
/// The same as the side, so a panel over the window sits in an even margin on
/// every free edge — but never so high that its top edge is in the title bar.
/// It used to take the masthead's margin, which on a Mac is the title bar's
/// and then some, so the menu hung twice as far from the top as from the side.
pub fn sheet_inset_top(side: f32, titlebar: f32) -> f32 {
    side.max(titlebar + 6.0)
}

/// How far the fade at the top and bottom of a list reaches at most. Rows do
/// not meet a cut edge when they scroll out; they thin into the ground.
///
/// About a row deep: a fade much shorter than the type it covers takes the
/// bottom off a line and leaves the top, which reads as a smear; a deeper one
/// stood over the list like a band of its own.
pub const FADE_HEIGHT: f32 = 20.0;

/// egui has no synthetic bold, so the two weights are two loaded faces.
pub fn family_regular() -> FontFamily {
    FontFamily::Proportional
}

pub fn family_bold() -> FontFamily {
    FontFamily::Name("bold".into())
}

/// The labels' face: the regular, or in Muto its medium — a family of its own
/// either way, so the roles need not know which.
pub fn family_label() -> FontFamily {
    FontFamily::Name("label".into())
}

/// The small figures' face: the mono cut, or in Muto Muto Mono's medium — see
/// [`family_label`].
pub fn family_figures_label() -> FontFamily {
    FontFamily::Name("figures-label".into())
}

/// Whether the small sizes are set in Muto's medium — see [`family_label`].
static LABEL_MEDIUM: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The mono cut of whatever family is in use — for figures, and only figures.
///
/// A track number, a year, a date, a running time: these are read *down* a
/// column, or watched while they change, and a proportional face makes the
/// same four digits a different width from one row to the next. The column
/// then shivers instead of lining up, and `4:10 / 5:00` jitters once a second
/// as the ones and the sevens go by.
///
/// It is the same typeface, not a second one — every family that ships with
/// the program has both cuts. Where a family has no mono cut, this points back
/// at the proportional one and nothing changes.
pub fn family_figures() -> FontFamily {
    FontFamily::Name("figures".into())
}

/// The type roles — two sizes times two weights, plus the figures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Names, titles, times: everything the user came for.
    Content,
    /// The same, marked as selected or playing.
    ContentBold,
    /// A number at content size, set in the mono cut — see [`family_figures`].
    ///
    /// There is no bold companion, and that is deliberate rather than missing:
    /// many mono cuts ship without one, and a figure never needed to shout
    /// anyway. A row being played says so in the accent, which is the signal
    /// the figures in it already carry.
    Figures,
    /// A figure at label size, in the mono cut: the volume, which changes
    /// under your hand and would otherwise shuffle the words beside it
    /// sideways with every step.
    FiguresLabel,
    /// Column heads, status, key hints: everything about the interface.
    Label,
    /// Running text at label size, set solid rather than tracked: the log's
    /// entries, which are sentences rather than words about the interface, and
    /// would lose a third of what fits to the tracking.
    Note,
    /// A label carrying emphasis, used sparingly.
    ///
    /// One thing draws it: the title of what is playing, when that line stands
    /// in the header among the interface's own words. It has to be the title
    /// there and not one more word on the line, and weight is the only way to
    /// say so without leaving the label size.
    LabelBold,
}

impl Role {
    pub fn font(self) -> FontId {
        match self {
            Self::Content => FontId::new(SIZE_CONTENT, family_regular()),
            Self::ContentBold => FontId::new(SIZE_CONTENT, family_bold()),
            Self::Figures => FontId::new(SIZE_CONTENT, family_figures()),
            Self::FiguresLabel => FontId::new(SIZE_LABEL, family_figures_label()),
            Self::Label => FontId::new(SIZE_LABEL, family_label()),
            Self::Note => FontId::new(SIZE_LABEL, family_label()),
            Self::LabelBold if LABEL_MEDIUM.load(std::sync::atomic::Ordering::Relaxed) => FontId::new(SIZE_LABEL, family_label()),
            Self::LabelBold => FontId::new(SIZE_LABEL, family_bold()),
        }
    }

    /// Labels are tracked out; content is set solid.
    pub fn tracking(self) -> f32 {
        match self {
            Self::Label | Self::LabelBold => {
                LABEL_TRACKING * f32::from_bits(LABEL_TRACKING_SHARE.load(std::sync::atomic::Ordering::Relaxed))
            }
            _ => 0.0,
        }
    }
}

/// A colour scheme.
///
/// Mostly the schemes people already read code in all day, fitted to the same
/// five roles.
/// Fitting rather than copying is the point: each scheme brings its own ground
/// and its own accent, but the *system* — one primary, one secondary, one
/// tertiary, one accent, one marker — does not change, so the interface reads
/// the same whichever is chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
///
/// Every scheme comes as a light and a dark one, so the menu chooses the scheme
/// and, apart from it, whether it is light, dark or follows the system — see
/// [`Brightness`]. A variant is one half of such a pair; [`Theme::family`]
/// names the pair.
pub enum Theme {
    MacosHell,
    MacosDunkel,
    /// GNOME's own, Adwaita: the window ground, the text at the opacities
    /// libadwaita sets its dim and faint labels at, and GNOME's blue.
    GnomeHell,
    GnomeDunkel,
    /// Ubuntu's own, Yaru as the current release paints it: GNOME's
    /// window grounds, Yaru's warmer grey text, Ubuntu orange for what
    /// plays and aubergine for the heads.
    UbuntuHell,
    UbuntuDunkel,
    SolarisiertHell,
    SolarisiertDunkel,
    Nord,
    /// Nord's snow storm: its three whites as the ground, polar night as the
    /// ink, frost for what plays.
    NordHell,
    Dracula,
    /// Dracula's own light one, „alucard": cream, near-black, its pink
    /// taken down to a crimson that reads on white.
    DraculaHell,
    /// Sublime Text's own: Mariana, the scheme it opens with — slate blue,
    /// cool white, and the orange of its caret and its logo.
    Sublime,
    /// Mariana in daylight: a cool white, its slate as the ink, the caret's
    /// orange burnt until it reads.
    SublimeHell,
    /// The grey panel of an iPod, and the ink on it. Two colours, no third.
    IpodClassic,
    /// The panel with its light off: the ink as the ground, the panel's
    /// grey-green as the words.
    IpodDunkel,
    /// Soulseek as it looked on Windows 2000:
    /// the grey of every button, the white of its lists, the navy of its
    /// title bar.
    Soulseek,
    /// Soulseek after dark: the button grey as the words on a dark grey,
    /// the Windows 2000 selection blue, lightened, for what plays.
    SoulseekDunkel,
    /// Nord's frost blue as the ground.
    NordFrost,
    /// Frost under the ice: a deep blue-green, frost as the words and the
    /// accent.
    NordFrostDunkel,
    /// Hudson Soft's blue, with the bee's yellow.
    Hudson,
    /// Hudson on paper: navy ink, its blue for what plays, the bee's yellow
    /// as the marker.
    HudsonHell,
    /// The white PC Engine, its blue name, the orange of its box.
    PcEngine,
    /// The PC Engine's navy as the ground, its plastic white as the words,
    /// the box's orange for what plays.
    PcEngineDunkel,
    /// NEC's last console: white, its purple spiral, its yellow and pink.
    PcFx,
    /// The spiral's purple at night.
    PcFxDunkel,
    /// The PC-9801 at night: indigo, and the cyan of its sixteen colours.
    Pc98,
    /// The PC-9801 by day: its lilac white, indigo ink, a teal that reads.
    Pc98Hell,
    /// Sharp's X68000: the black tower, the red of its X.
    X68000,
    /// The X68000's warm grey as the ground, its black as the ink.
    X68000Hell,
    /// Fujitsu's FM Towns: white windows, the red menu bar, the pink title.
    FmTowns,
    /// TownsOS with the lights down: the red menu bar, lightened, on black.
    FmTownsDunkel,
    /// 公園, a park: its playground — the gravel,
    /// the red bar, the yellow joint, the blue pole.
    Koen,
    /// 公園 by day: the gravel dry, dark words, the red bar for what plays,
    /// the yellow joint as the marker, the blue pole for a marked cover.
    KoenHell,
    /// Paysage d'Hiver, Wintherr's winter black metal: black and the greys of
    /// snow at night, and no colour at all.
    PaysageDHiver,
    /// The same winter by day: the grey of the snow on its page, black ink.
    PaysageDHiverHell,
    /// CoCo壱番屋, the curry house: its yellow sign, the curry's brown as the
    /// words, a curry red for what plays.
    CocoIchibanya,
    /// The same at night: the roux as the ground, the rice white as the
    /// words, the sign's yellow for what plays.
    CocoIchibanyaDunkel,
    /// Runebending: Runebender's grounds quiet, and the tiles' colours in the
    /// type instead — the names in ink, what stands beside them in the grid's
    /// green, the figures in its orange, what plays in the comb's violet.
    Runebending,
    /// The same at night.
    RunebendingDunkel,
    /// Rubrication, as old books were printed: black and red on paper, the
    /// names in black, what stands beside them in red, the figures in a
    /// blue-grey, what plays in blue.
    Rubrik,
    /// The same by lamplight.
    RubrikDunkel,
    /// Three on Gruvbox's colours with every kind of thing in a colour of its
    /// own — heads, names, what stands beside them, figures, what plays — named
    /// after a Hessian half-timbered house. The wild man: the braces of a
    /// Fachwerk storey post, in Gruvbox's dark, heads in its yellow, what
    /// stands beside the names in its blue, what plays in its red.
    WilderMann,
    /// By day: Gruvbox's light, the same roles in its deeper colours.
    WilderMannHell,
    /// Ox blood, the red the timbers were painted: heads in it, what stands
    /// beside the names in orange, what plays in green, the row marked in a
    /// dark red.
    Ochsenblut,
    /// By day: the plaster pale, the red and orange deepened.
    OchsenblutHell,
    /// The clay of the panels between the timbers: a cream ground, the
    /// timbers' dark for names, heads in burnt clay, what stands beside the
    /// names in moss green, what plays in blue.
    Lehmgefach,
    /// By night: the clay dark, the colours lifted.
    LehmgefachDunkel,
    /// Nothing but the full colours a screen has — every channel on or off:
    /// black, white, red, green, blue and what two of them make.
    Rgb,
    /// The same on white, where only the darker of them read.
    RgbHell,
    /// Zeitvogel, the bird that lives in time, after a toy's tile with a
    /// cuckoo clock on it: by day the lime tile beside the clock as the
    /// ground, the roof's teal in the words, coral for what plays, the
    /// lavender tile behind the picked row.
    ZeitvogelHell,
    /// The same bird at night: the lavender tile darkened to a deep violet,
    /// the clock's green lightened for the words.
    Zeitvogel,
    /// One's own, from four colours set in the menu — see [`Seed`]. The light
    /// half.
    Eigenes,
    /// Its dark half.
    EigenesDunkel,
}

impl Theme {
    /// In the order they are offered.
    pub fn all() -> [Theme; 54] {
        [
            Self::MacosHell,
            Self::MacosDunkel,
            Self::GnomeHell,
            Self::GnomeDunkel,
            Self::UbuntuHell,
            Self::UbuntuDunkel,
            Self::SolarisiertHell,
            Self::SolarisiertDunkel,
            Self::NordHell,
            Self::Nord,
            Self::DraculaHell,
            Self::Dracula,
            Self::SublimeHell,
            Self::Sublime,
            Self::IpodClassic,
            Self::IpodDunkel,
            Self::Soulseek,
            Self::SoulseekDunkel,
            Self::NordFrost,
            Self::NordFrostDunkel,
            Self::HudsonHell,
            Self::Hudson,
            Self::PcEngine,
            Self::PcEngineDunkel,
            Self::PcFx,
            Self::PcFxDunkel,
            Self::Pc98Hell,
            Self::Pc98,
            Self::X68000Hell,
            Self::X68000,
            Self::FmTowns,
            Self::FmTownsDunkel,
            Self::KoenHell,
            Self::Koen,
            Self::PaysageDHiverHell,
            Self::PaysageDHiver,
            Self::CocoIchibanya,
            Self::CocoIchibanyaDunkel,
            Self::Runebending,
            Self::RunebendingDunkel,
            Self::Rubrik,
            Self::RubrikDunkel,
            Self::WilderMann,
            Self::WilderMannHell,
            Self::Ochsenblut,
            Self::OchsenblutHell,
            Self::Lehmgefach,
            Self::LehmgefachDunkel,
            Self::Rgb,
            Self::RgbHell,
            Self::ZeitvogelHell,
            Self::Zeitvogel,
            Self::Eigenes,
            Self::EigenesDunkel,
        ]
    }

    /// One of each pair, in the order the menu offers them.
    pub fn families() -> Vec<Theme> {
        Self::GROUPS.iter().flat_map(|(_, schemes)| schemes.iter().copied()).collect()
    }

    /// The schemes as the menu lists them, under a caption a group, in two
    /// columns, and the order `shift` + `f` leafs through them. One half of
    /// each scheme stands for both.
    pub const GROUPS: [(&'static str, &'static [Theme]); 5] = [
        ("system", &[Theme::MacosHell, Theme::GnomeHell, Theme::UbuntuHell]),
        (
            "klassiker",
            &[
                Theme::SolarisiertHell,
                Theme::NordHell,
                Theme::NordFrost,
                Theme::DraculaHell,
                Theme::SublimeHell,
                Theme::Soulseek,
            ],
        ),
        (
            "geräte",
            &[
                Theme::IpodClassic,
                Theme::HudsonHell,
                Theme::PcEngine,
                Theme::PcFx,
                Theme::FmTowns,
                Theme::Pc98,
                Theme::X68000,
            ],
        ),
        (
            "eigene welten",
            &[
                Theme::KoenHell,
                Theme::PaysageDHiverHell,
                Theme::CocoIchibanya,
                Theme::Runebending,
                Theme::Rubrik,
                Theme::WilderMannHell,
                Theme::OchsenblutHell,
                Theme::Lehmgefach,
                Theme::RgbHell,
                Theme::ZeitvogelHell,
            ],
        ),
        ("selbst gemacht", &[Theme::Eigenes]),
    ];

    /// The pair's name: the scheme's own without „hell" or „dunkel".
    pub fn family(self) -> &'static str {
        let name = self.name();
        name.strip_suffix(" hell")
            .or_else(|| name.strip_suffix(" dunkel"))
            .unwrap_or(name)
    }

    /// What the menu calls it, and what goes in the config file.
    pub fn name(self) -> &'static str {
        match self {
            Self::MacosHell => "macos hell",
            Self::MacosDunkel => "macos dunkel",
            Self::GnomeHell => "gnome hell",
            Self::GnomeDunkel => "gnome dunkel",
            Self::UbuntuHell => "ubuntu hell",
            Self::UbuntuDunkel => "ubuntu dunkel",
            Self::SolarisiertHell => "solarisiert hell",
            Self::SolarisiertDunkel => "solarisiert dunkel",
            Self::Nord => "nord",
            Self::Dracula => "dracula",
            Self::Sublime => "sublime",
            Self::IpodClassic => "ipod classic",
            Self::Soulseek => "soulseek",
            Self::NordFrost => "nord frost",
            Self::Hudson => "hu-card",
            Self::PcEngine => "pc engine",
            Self::PcFx => "pc-fx",
            Self::Pc98 => "pc-98",
            Self::X68000 => "x68000",
            Self::FmTowns => "fm towns",
            Self::Koen => "公園",
            Self::NordHell => "nord hell",
            Self::DraculaHell => "dracula hell",
            Self::SublimeHell => "sublime hell",
            Self::IpodDunkel => "ipod classic dunkel",
            Self::SoulseekDunkel => "soulseek dunkel",
            Self::NordFrostDunkel => "nord frost dunkel",
            Self::HudsonHell => "hu-card hell",
            Self::PcEngineDunkel => "pc engine dunkel",
            Self::PcFxDunkel => "pc-fx dunkel",
            Self::Pc98Hell => "pc-98 hell",
            Self::X68000Hell => "x68000 hell",
            Self::FmTownsDunkel => "fm towns dunkel",
            Self::KoenHell => "公園 hell",
            Self::PaysageDHiver => "paysage d'hiver",
            Self::PaysageDHiverHell => "paysage d'hiver hell",
            Self::CocoIchibanya => "coco壱番屋",
            Self::CocoIchibanyaDunkel => "coco壱番屋 dunkel",
            Self::Runebending => "runebending",
            Self::RunebendingDunkel => "runebending dunkel",
            Self::Rgb => "rgb",
            Self::RgbHell => "rgb hell",
            Self::ZeitvogelHell => "zeitvogel hell",
            Self::Zeitvogel => "zeitvogel dunkel",
            Self::Rubrik => "rubrik",
            Self::RubrikDunkel => "rubrik dunkel",
            Self::WilderMann => "wilder mann",
            Self::WilderMannHell => "wilder mann hell",
            Self::Ochsenblut => "ochsenblut",
            Self::OchsenblutHell => "ochsenblut hell",
            Self::Lehmgefach => "lehmgefach",
            Self::LehmgefachDunkel => "lehmgefach dunkel",
            Self::Eigenes => "eigenes",
            Self::EigenesDunkel => "eigenes dunkel",
        }
    }

    /// This scheme's half on the given side: itself, or its partner.
    pub fn side(self, dark: bool) -> Theme {
        if self.palette().is_dark() == dark {
            self
        } else {
            self.partner()
        }
    }

    /// The scheme a new config opens on: GNOME's on Linux, where GNOME is
    /// what the desktop most likely is; the Mac's everywhere else.
    pub fn native() -> (Theme, Theme) {
        if cfg!(target_os = "linux") {
            (Self::GnomeHell, Self::GnomeDunkel)
        } else {
            (Self::MacosHell, Self::MacosDunkel)
        }
    }

    /// Read a name back. Schemes that have since been taken out are mapped to
    /// the native one of the same brightness, so an existing config file opens
    /// on something close to what it named rather than on a surprise; and
    /// „systemfarben", which was once a scheme and is now
    /// [`Brightness::System`], opens on the native pair.
    pub fn from_name(name: &str) -> Self {
        let name = name.trim().to_lowercase();
        match name.as_str() {
            "ink" | "dark" | "tinte" | "monokai" | "gruvbox" | "blaupause" => {
                Self::MacosDunkel
            }
            "paper" | "papier" | "light" | "riso" => Self::MacosHell,
            "kōen" | "koen" => Self::Koen,
            "coco ichibanya" | "cocoichibanya" | "cocoichi" => Self::CocoIchibanya,
            "famicom" | "virtual boy" => Self::PcEngineDunkel,
            "famicom hell" | "virtual boy hell" | "disk system" => Self::PcEngine,
            "disk system dunkel" => Self::PcEngineDunkel,
            "runebender" | "runebender schrift" => Self::Runebending,
            "windpark" => Self::Rubrik,
            "windpark dunkel" => Self::RubrikDunkel,
            "runebender dunkel" | "runebender schrift dunkel" => Self::RunebendingDunkel,
            "kuckucksstunde" | "kuckucksstunde hell" => Self::ZeitvogelHell,
            "kuckucksstunde dunkel" => Self::Zeitvogel,
            "hudson" => Self::Hudson,
            "hudson hell" => Self::HudsonHell,
            other => Self::all()
                .into_iter()
                .find(|t| t.name() == other)
                .unwrap_or(Self::native().0),
        }
    }

    /// Its other half: the same scheme, light where this is dark.
    pub fn partner(self) -> Self {
        const PAIRS: [(Theme, Theme); 27] = [
            (Theme::MacosHell, Theme::MacosDunkel),
            (Theme::GnomeHell, Theme::GnomeDunkel),
            (Theme::UbuntuHell, Theme::UbuntuDunkel),
            (Theme::SolarisiertHell, Theme::SolarisiertDunkel),
            (Theme::NordHell, Theme::Nord),
            (Theme::DraculaHell, Theme::Dracula),
            (Theme::SublimeHell, Theme::Sublime),
            (Theme::IpodClassic, Theme::IpodDunkel),
            (Theme::Soulseek, Theme::SoulseekDunkel),
            (Theme::NordFrost, Theme::NordFrostDunkel),
            (Theme::HudsonHell, Theme::Hudson),
            (Theme::PcEngine, Theme::PcEngineDunkel),
            (Theme::PcFx, Theme::PcFxDunkel),
            (Theme::Pc98Hell, Theme::Pc98),
            (Theme::X68000Hell, Theme::X68000),
            (Theme::FmTowns, Theme::FmTownsDunkel),
            (Theme::KoenHell, Theme::Koen),
            (Theme::PaysageDHiverHell, Theme::PaysageDHiver),
            (Theme::CocoIchibanya, Theme::CocoIchibanyaDunkel),
            (Theme::Runebending, Theme::RunebendingDunkel),
            (Theme::Rubrik, Theme::RubrikDunkel),
            (Theme::WilderMannHell, Theme::WilderMann),
            (Theme::OchsenblutHell, Theme::Ochsenblut),
            (Theme::Lehmgefach, Theme::LehmgefachDunkel),
            (Theme::RgbHell, Theme::Rgb),
            (Theme::ZeitvogelHell, Theme::Zeitvogel),
            (Theme::Eigenes, Theme::EigenesDunkel),
        ];
        PAIRS
            .iter()
            .find_map(|&(a, b)| {
                if a == self {
                    Some(b)
                } else if b == self {
                    Some(a)
                } else {
                    None
                }
            })
            .unwrap_or(self)
    }

    /// The icon's ground and its k while the window is up: the scheme's
    /// ground and accent, except where those would be the closed app's own
    /// white k on black — then the running app could not be told from the
    /// closed one in the Dock. Paysage d'Hiver's night is that, and takes its
    /// marker's grey for the ground: the k still its one brightest thing.
    pub fn icon_colours(self) -> (Color32, Color32) {
        let palette = self.palette();
        let ground = match self {
            Theme::PaysageDHiver => palette.marker,
            Theme::PcFx
            | Theme::PcFxDunkel
            | Theme::Pc98Hell
            | Theme::Pc98
            | Theme::FmTowns
            | Theme::FmTownsDunkel
            | Theme::Koen
            | Theme::CocoIchibanya
            | Theme::CocoIchibanyaDunkel
            | Theme::Ochsenblut
            | Theme::RgbHell
            | Theme::Zeitvogel => palette.marker,
            Theme::GnomeDunkel => rgb(0x10233A),
            Theme::UbuntuDunkel => rgb(0x2C001E),
            Theme::Soulseek => rgb(0xC0D1F2),
            Theme::X68000 => rgb(0x30120E),
            Theme::Runebending => rgb(0xE5E2FF),
            Theme::WilderMann => rgb(0x342506),
            Theme::LehmgefachDunkel => rgb(0x24361D),
            _ => palette.ground,
        };
        (ground, palette.accent)
    }

    pub fn palette(self) -> Palette {
        Palette::for_theme(self)
    }
}

/// Light, dark, or whichever the system is set to: chosen apart from the scheme
/// since every scheme comes in both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Brightness {
    System,
    Hell,
    Dunkel,
}

impl Brightness {
    /// What the menu says, and what goes in the config file.
    pub fn name(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Hell => "hell",
            Self::Dunkel => "dunkel",
        }
    }

    /// Round the three, for the menu's row.
    pub fn next(self) -> Self {
        match self {
            Self::System => Self::Hell,
            Self::Hell => Self::Dunkel,
            Self::Dunkel => Self::System,
        }
    }

    /// Read back from the config, with the scheme it was written beside.
    pub fn from_config(word: &str, scheme: &str) -> Self {
        match word.trim() {
            "system" => Self::System,
            "hell" => Self::Hell,
            "dunkel" => Self::Dunkel,
            _ => match scheme.trim() {
                "" | "systemfarben" => Self::System,
                name if Theme::from_name(name).palette().is_dark() => Self::Dunkel,
                _ => Self::Hell,
            },
        }
    }

    /// The half of the scheme actually painted, once the operating system
    /// has been asked. No answer from the system counts as light: the safer
    /// guess, and one the user can still overrule.
    pub fn resolve(self, theme: Theme, system: Option<egui::Theme>) -> Theme {
        let dark = match self {
            Self::Hell => false,
            Self::Dunkel => true,
            Self::System => system == Some(egui::Theme::Dark),
        };
        theme.side(dark)
    }
}

/// The four colours a scheme of one's own is made from: the ground, the names,
/// what plays, and the columns' heads. Everything else is worked out from them
/// in Oklab, so it keeps their hues; the other half of the scheme has every
/// lightness turned over.
///
/// Passed on as a code — `k1-282828-ebdbb2-fb4934-fabd2f` — so somebody else
/// can take the scheme; `k1` says which way of reading it this is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Seed {
    pub ground: Color32,
    pub ink: Color32,
    pub accent: Color32,
    pub heading: Color32,
}

impl Seed {
    /// Where one starts: the wild man's colours.
    pub const DEFAULT: Seed = Seed {
        ground: rgb(0x282828),
        ink: rgb(0xEBDBB2),
        accent: rgb(0xFB4934),
        heading: rgb(0xFABD2F),
    };

    pub fn get(&self, role: usize) -> Color32 {
        [self.ground, self.ink, self.accent, self.heading][role.min(3)]
    }

    pub fn set(&mut self, role: usize, colour: Color32) {
        match role {
            0 => self.ground = colour,
            1 => self.ink = colour,
            2 => self.accent = colour,
            _ => self.heading = colour,
        }
    }

    /// The code that passes it on.
    pub fn code(&self) -> String {
        let hex = |c: Color32| format!("{:02x}{:02x}{:02x}", c.r(), c.g(), c.b());
        format!("k1-{}-{}-{}-{}", hex(self.ground), hex(self.ink), hex(self.accent), hex(self.heading))
    }

    /// A code read back, forgiving of how it was copied: with or without
    /// `k1`, `#` or spaces, any marks between the four colours.
    pub fn parse(code: &str) -> Option<Self> {
        let code = code.trim().to_ascii_lowercase();
        let code = code.strip_prefix("k1").unwrap_or(&code);
        let colours: Vec<Color32> = code
            .split(|c: char| !c.is_ascii_hexdigit())
            .filter(|run| !run.is_empty())
            .map(|run| (run.len() == 6).then(|| u32::from_str_radix(run, 16).ok()).flatten().map(rgb))
            .collect::<Option<Vec<_>>>()?;
        let [ground, ink, accent, heading] = colours[..] else {
            return None;
        };
        Some(Self { ground, ink, accent, heading })
    }

    /// One colour typed as hex: `#fb4934`, `fb4934`, or the short `#f43`.
    pub fn parse_colour(text: &str) -> Option<Color32> {
        let hex = text.trim().trim_start_matches('#');
        if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let long: String = match hex.len() {
            6 => hex.to_string(),
            3 => hex.chars().flat_map(|c| [c, c]).collect(),
            _ => return None,
        };
        u32::from_str_radix(&long, 16).ok().map(rgb)
    }

    /// Whether its ground is a dark one, by the measure palettes use.
    fn is_dark(&self) -> bool {
        (self.ground.r() as u32 + self.ground.g() as u32 + self.ground.b() as u32) < 384
    }

    /// The seed for the light half or the dark: itself, or with every
    /// lightness turned over and the hues kept.
    pub fn side(self, dark: bool) -> Self {
        if self.is_dark() == dark {
            return self;
        }
        let turned = |c: Color32| {
            let lab = Lab::of(c);
            Lab { l: 1.0 - lab.l, ..lab }.colour()
        };
        Self {
            ground: turned(self.ground),
            ink: turned(self.ink),
            accent: turned(self.accent),
            heading: turned(self.heading),
        }
    }

    /// The whole palette: the tones between ink and ground, the plates a
    /// step from the ground towards the ink.
    fn palette(&self) -> Palette {
        let mix = |a: Color32, b: Color32, t: f32| {
            let (a, b) = (Lab::of(a), Lab::of(b));
            Lab { l: a.l + (b.l - a.l) * t, a: a.a + (b.a - a.a) * t, b: a.b + (b.b - a.b) * t }.colour()
        };
        Palette {
            ground: self.ground,
            ink: self.ink,
            mid: mix(self.ink, self.ground, 0.28),
            faint: mix(self.ink, self.ground, 0.52),
            accent: self.accent,
            accent_faint: mix(self.accent, self.ground, 0.45),
            marker: mix(self.ground, self.ink, 0.12),
            field: mix(self.ground, self.ink, 0.07),
            rule: mix(self.ground, self.ink, 0.18),
            hover: mix(self.ground, self.ink, 0.06),
            lit_dots: None,
            heading: Some(self.heading),
        }
    }
}

/// The seed of one's own scheme, set from the config at start and from the
/// menu while it is being made.
static CUSTOM: std::sync::RwLock<Seed> = std::sync::RwLock::new(Seed::DEFAULT);

pub fn set_custom(seed: Seed) {
    if let Ok(mut held) = CUSTOM.write() {
        *held = seed;
    }
}

pub fn custom() -> Seed {
    CUSTOM.read().map(|s| *s).unwrap_or(Seed::DEFAULT)
}

/// A colour as the menu's three rules hold it: lightness 0–1, colourfulness
/// 0–1 (of [`CHROMA_MOST`]), hue 0–1 round the circle.
pub fn rules_of(colour: Color32) -> [f32; 3] {
    let lab = Lab::of(colour);
    let hue = lab.b.atan2(lab.a).to_degrees().rem_euclid(360.0) / 360.0;
    [lab.l.clamp(0.0, 1.0), (lab.chroma() / CHROMA_MOST).clamp(0.0, 1.0), hue]
}

/// And back: the nearest colour the screen can show.
pub fn from_rules([l, c, h]: [f32; 3]) -> Color32 {
    let (chroma, angle) = (c * CHROMA_MOST, (h * 360.0).to_radians());
    Lab { l, a: chroma * angle.cos(), b: chroma * angle.sin() }.colour()
}

/// About the most colourful a screen's colour gets, in Oklab.
const CHROMA_MOST: f32 = 0.32;

/// The three tonal values plus the accent, for one scheme.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    /// The ground the interface sits on.
    pub ground: Color32,
    /// Primary text.
    pub ink: Color32,
    /// Secondary — present but stepping back.
    pub mid: Color32,
    /// Tertiary — counts, years, anything read only when looked for.
    pub faint: Color32,
    /// The single accent: what is playing, and what needs a warning.
    pub accent: Color32,
    /// The accent, receded, for figures beside accented text.
    pub accent_faint: Color32,
    /// Behind the selected row, the width of the whole row. Since the selection
    /// no longer steps out of the margin, this and the bold face are the only
    /// two things saying where it is.
    pub marker: Color32,
    /// Behind the search field. A shade off the ground — enough to say "you can
    /// type here", not enough to become a box.
    pub field: Color32,
    /// The unfilled part of the progress rule.
    pub rule: Color32,
    /// Hover feedback. Barely there on purpose — it is a hint, not a button.
    pub hover: Color32,
    /// The dots of a halftone cover that is marked — the one you are on, the
    /// one you are hearing — where they are not simply the accent: 公園 sets
    /// every word and every unmarked cover in yellow, and a marked cover in the
    /// blue of its pole. See [`Palette::lit_dots`].
    pub lit_dots: Option<Color32>,
    /// The columns' heads, where they are not simply in the third tone: with
    /// the colour in the type, a colour of their own. See [`Palette::heading`].
    pub heading: Option<Color32>,
}

/// How much more colourful than the ground a colour must be to count as one
/// — see [`Palette::in_the_type`] — in Oklab's chroma.
const COLOURFUL: f32 = 0.035;

/// What a scheme's colour other than the accent costs as a screened
/// picture's tone, and a grey one on top, in Oklab's lightness — see
/// [`Palette::tones_under`] — and how far from its share of the way a tone
/// may stay, as a share of the way from the field to the ink.
const DITHER_OTHER: f32 = 0.12;
const DITHER_GREY: f32 = 0.05;
const DITHER_NEAR: f32 = 0.08;
/// How colourful a tone under the ink may be on a light ground, in Oklab's
/// chroma: there a full red or magenta under dark ink was the loudest thing in
/// the picture and took its shape.
const DITHER_LIGHT_CHROMA: f32 = 0.11;

/// How far apart two hues must be, in degrees, to colour two tones.
const HUE_APART: f32 = 40.0;

/// How far a coloured plate comes towards the ground's lightness when its
/// colour goes into the type: halfway, still seen, no longer loud.
const PLATE_TOWARD_GROUND: f32 = 0.5;

/// How colourful each plate is made, at least, with the colour on the
/// plates — see [`Palette::on_the_plates`] — in Oklab's chroma: the marker
/// and the rule plainly coloured, the search field and the hover a tint.
const PLATE_CHROMA: [f32; 4] = [0.10, 0.05, 0.09, 0.035];

/// A colour in Oklab (Björn Ottosson, 2020): lightness as the eye has it,
/// and two axes of hue, so a colour can be made lighter or darker without
/// its hue wandering.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Lab {
    l: f32,
    a: f32,
    b: f32,
}

impl Lab {
    fn of(c: Color32) -> Self {
        let lin = |v: u8| {
            let v = v as f32 / 255.0;
            if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
        };
        let (r, g, b) = (lin(c.r()), lin(c.g()), lin(c.b()));
        let l = (0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
        let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
        let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
        Self {
            l: 0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
            a: 1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
            b: 0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
        }
    }

    /// In linear light, each channel as it comes out — outside 0 to 1 where
    /// the colour is beyond the screen's.
    fn linear(self) -> [f32; 3] {
        let l = (self.l + 0.396_337_78 * self.a + 0.215_803_76 * self.b).powi(3);
        let m = (self.l - 0.105_561_346 * self.a - 0.063_854_17 * self.b).powi(3);
        let s = (self.l - 0.089_484_18 * self.a - 1.291_485_5 * self.b).powi(3);
        [
            4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s,
            -1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s,
            -0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s,
        ]
    }

    /// The colour on the screen: with its chroma taken back, keeping its
    /// lightness and hue, until the screen can show it.
    fn colour(self) -> Color32 {
        let mut lab = self;
        for _ in 0..24 {
            if lab.linear().iter().all(|v| (-0.0005..=1.0005).contains(v)) {
                break;
            }
            lab = lab.quieter(0.9);
        }
        let gamma = |v: f32| {
            let v = v.clamp(0.0, 1.0);
            let v = if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
            (v * 255.0).round() as u8
        };
        let [r, g, b] = lab.linear();
        Color32::from_rgb(gamma(r), gamma(g), gamma(b))
    }

    fn chroma(&self) -> f32 {
        self.a.hypot(self.b)
    }

    /// How far apart two hues are, in degrees.
    fn hue_apart(&self, other: &Lab) -> f32 {
        let apart = (self.b.atan2(self.a) - other.b.atan2(other.a)).to_degrees().abs() % 360.0;
        apart.min(360.0 - apart)
    }

    /// The same hue, `by` as colourful.
    fn quieter(self, by: f32) -> Self {
        Self { a: self.a * by, b: self.b * by, ..self }
    }

    /// The same hue and chroma at another lightness.
    fn at_lightness(self, l: f32) -> Self {
        Self { l, ..self }
    }

    /// Kept between black and white.
    fn clone_clamped(self) -> Self {
        Self { l: self.l.clamp(0.0, 1.0), ..self }
    }
}

/// Shorthand, so the table below reads as a table.
const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb(
        ((hex >> 16) & 0xFF) as u8,
        ((hex >> 8) & 0xFF) as u8,
        (hex & 0xFF) as u8,
    )
}

/// The accent colour the system is set to — a Mac's, GNOME's and Ubuntu's,
/// KDE's too — which the schemes after a system (macos, gnome, ubuntu) take,
/// with no setting of their own: chosen in the system, it is there. Nothing
/// where the system has none to say, or a Mac is left on „mehrfarbig".
static DESKTOP_ACCENT: std::sync::Mutex<Option<Color32>> = std::sync::Mutex::new(None);

/// What the desktop says its accent is now: on Linux through the desktop
/// portal, on a Mac from the system settings (see `macaccent`).
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn set_desktop_accent(colour: Option<Color32>) {
    *DESKTOP_ACCENT.lock().unwrap_or_else(|e| e.into_inner()) = colour;
}

/// The desktop's accent for a scheme named after a desktop, where it is to be
/// taken.
fn desktop_accent_for(theme: Theme) -> Option<Color32> {
    let after_a_desktop = matches!(
        theme,
        Theme::MacosHell | Theme::MacosDunkel | Theme::GnomeHell | Theme::GnomeDunkel | Theme::UbuntuHell | Theme::UbuntuDunkel
    );
    if !after_a_desktop {
        return None;
    }
    *DESKTOP_ACCENT.lock().unwrap_or_else(|e| e.into_inner())
}

/// How much contrast the accent keeps against the ground once it is taken
/// from the desktop: as much as words want.
const DESKTOP_ACCENT_CONTRAST: f32 = 4.5;

impl Palette {
    /// The same scheme with another accent — the desktop's: in its hue, as
    /// dark (on a light ground) or as light (on a dark one) as it has to be to
    /// read as words, the receded one part of the way back to the ground. The
    /// heads lose their own colour where it is the accent's hue, so the two
    /// are not taken for each other.
    fn with_accent(mut self, colour: Color32) -> Self {
        let dark = self.is_dark();
        let mut lab = Lab::of(colour);
        for _ in 0..40 {
            if contrast(lab.colour(), self.ground) >= DESKTOP_ACCENT_CONTRAST {
                break;
            }
            lab = lab.at_lightness(if dark { lab.l + 0.02 } else { lab.l - 0.02 }).clone_clamped();
        }
        self.accent = lab.colour();
        self.accent_faint = self.ground.lerp_to_gamma(self.accent, if dark { 0.47 } else { 0.55 });
        if let Some(heading) = self.heading {
            if Lab::of(heading).hue_apart(&Lab::of(self.accent)) < 30.0 {
                self.heading = None;
            }
        }
        self
    }

    pub fn for_theme(theme: Theme) -> Self {
        let drawn = Self::as_drawn(theme);
        let drawn = match desktop_accent_for(theme) {
            Some(colour) => drawn.with_accent(colour),
            None => drawn,
        };
        if matches!(theme, Theme::Rgb | Theme::RgbHell) {
            return drawn;
        }
        drawn.plates_retuned()
    }

    /// The plates as the eye wants them against each other: the marked row's
    /// a little quieter than each scheme draws it — it is where the hand is,
    /// not a block to look at — and the hover's a little stronger, so the
    /// row under the pointer reads as it is passed over.
    fn plates_retuned(self) -> Self {
        let mix = |a: Color32, b: Color32, t: f32| {
            let (a, b) = (Lab::of(a), Lab::of(b));
            Lab { l: a.l + (b.l - a.l) * t, a: a.a + (b.a - a.a) * t, b: a.b + (b.b - a.b) * t }.colour()
        };
        Palette {
            marker: mix(self.marker, self.ground, MARKER_QUIETER),
            hover: mix(self.ground, self.hover, HOVER_STRONGER),
            ..self
        }
    }

    fn as_drawn(theme: Theme) -> Self {
        match theme {
            Theme::Eigenes => custom().side(false).palette(),
            Theme::EigenesDunkel => custom().side(true).palette(),
            Theme::MacosHell => Self {
                ground: rgb(0xFFFFFF),
                ink: rgb(0x262626),
                mid: rgb(0x767676),
                faint: rgb(0xBDBDBD),
                accent: rgb(0x0064D8),
                accent_faint: rgb(0x7FA9DF),
                marker: rgb(0xDBDBDB),
                field: rgb(0xF1F1F4),
                rule: rgb(0xE4E4E6),
                hover: rgb(0xF6F6F6),
                lit_dots: None,
                heading: Some(rgb(0x8A55B8)),
            },
            Theme::MacosDunkel => Self {
                ground: rgb(0x1E1E1E),
                ink: rgb(0xDEDEDE),
                mid: rgb(0x9A9A9E),
                faint: rgb(0x5A5A5C),
                accent: rgb(0x3F9DFF),
                accent_faint: rgb(0x2C6BAF),
                marker: rgb(0x3A3A3C),
                field: rgb(0x2C2C2E),
                rule: rgb(0x333335),
                hover: rgb(0x262628),
                lit_dots: None,
                heading: Some(rgb(0xB58CE0)),
            },
            Theme::GnomeHell => Self {
                ground: rgb(0xFAFAFB),
                ink: rgb(0x323236),
                mid: rgb(0x77777B),
                faint: rgb(0xB9B9BC),
                accent: rgb(0x1C71D8),
                accent_faint: rgb(0x86AEE3),
                marker: rgb(0xE3E3E5),
                field: rgb(0xEDEDEF),
                rule: rgb(0xDEDEE0),
                hover: rgb(0xF2F2F3),
                lit_dots: None,
                heading: Some(rgb(0x9141AC)),
            },
            Theme::GnomeDunkel => Self {
                ground: rgb(0x222226),
                ink: rgb(0xF2F2F3),
                mid: rgb(0x9E9EA1),
                faint: rgb(0x606064),
                accent: rgb(0x78AEED),
                accent_faint: rgb(0x4A6A90),
                marker: rgb(0x3A3A3E),
                field: rgb(0x2E2E32),
                rule: rgb(0x3F3F43),
                hover: rgb(0x29292D),
                lit_dots: None,
                heading: Some(rgb(0xDC8ADD)),
            },
            Theme::UbuntuHell => Self {
                ground: rgb(0xFAFAFA),
                ink: rgb(0x3D3D3D),
                mid: rgb(0x787878),
                faint: rgb(0xBABABA),
                accent: rgb(0xC7461A),
                accent_faint: rgb(0xE8A58B),
                marker: rgb(0xE5E5E5),
                field: rgb(0xEDEDED),
                rule: rgb(0xDEDEDE),
                hover: rgb(0xF2F2F2),
                lit_dots: None,
                heading: Some(rgb(0x77216F)),
            },
            Theme::UbuntuDunkel => Self {
                ground: rgb(0x2C2C2C),
                ink: rgb(0xF7F7F7),
                mid: rgb(0xA5A5A5),
                faint: rgb(0x666666),
                accent: rgb(0xF27B4A),
                accent_faint: rgb(0x93492C),
                marker: rgb(0x444444),
                field: rgb(0x363636),
                rule: rgb(0x474747),
                hover: rgb(0x333333),
                lit_dots: None,
                heading: Some(rgb(0xD48BC8)),
            },
            Theme::SolarisiertHell => Self {
                ground: rgb(0xFDF6E3),
                ink: rgb(0x073642),
                mid: rgb(0x657B83),
                faint: rgb(0x93A1A1),
                accent: rgb(0xCB4B16),
                accent_faint: rgb(0xD68B66),
                marker: rgb(0xEDE4C8),
                field: rgb(0xF5EDD8),
                rule: rgb(0xEEE8D5),
                hover: rgb(0xF7F0DD),
                lit_dots: None,
                heading: Some(rgb(0x6C71C4)),
            },
            Theme::SolarisiertDunkel => Self {
                ground: rgb(0x002B36),
                ink: rgb(0xEEE8D5),
                mid: rgb(0x93A1A1),
                faint: rgb(0x586E75),
                accent: rgb(0xB58900),
                accent_faint: rgb(0x7C5F0B),
                marker: rgb(0x0B4351),
                field: rgb(0x01323D),
                rule: rgb(0x073642),
                hover: rgb(0x013640),
                lit_dots: None,
                heading: Some(rgb(0x8F93D8)),
            },
            Theme::Nord => Self {
                ground: rgb(0x2E3440),
                ink: rgb(0xECEFF4),
                mid: rgb(0xA3B3C7),
                faint: rgb(0x4C566A),
                accent: rgb(0x88C0D0),
                accent_faint: rgb(0x5E818C),
                marker: rgb(0x424C5E),
                field: rgb(0x39414F),
                rule: rgb(0x3B4252),
                hover: rgb(0x343B49),
                lit_dots: None,
                heading: Some(rgb(0xB48EAD)),
            },
            Theme::IpodClassic => {
                const PANEL: Color32 = rgb(0xCBCFC1);
                const INK: Color32 = rgb(0x21251C);
                let mix = |t: f32| PANEL.lerp_to_gamma(INK, t);
                Self {
                    ground: PANEL,
                    ink: mix(0.88),
                    mid: mix(0.56),
                    faint: mix(0.34),
                    accent: INK,
                    accent_faint: mix(0.62),
                    marker: mix(0.12),
                    field: mix(0.07),
                    rule: mix(0.20),
                    hover: mix(0.04),
                    lit_dots: None,
                    heading: Some(mix(0.74)),
                }
            }
            Theme::Dracula => Self {
                ground: rgb(0x282A36),
                ink: rgb(0xF8F8F2),
                mid: rgb(0x9BA3C4),
                faint: rgb(0x6272A4),
                accent: rgb(0xFF79C6),
                accent_faint: rgb(0xAD528A),
                marker: rgb(0x41455C),
                field: rgb(0x333648),
                rule: rgb(0x3A3D51),
                hover: rgb(0x2F3141),
                lit_dots: None,
                heading: Some(rgb(0xBD93F9)),
            },
            Theme::Sublime => Self {
                ground: rgb(0x303841),
                ink: rgb(0xD8DEE9),
                mid: rgb(0xA6ACB9),
                faint: rgb(0x65737E),
                accent: rgb(0xF9AE58),
                accent_faint: rgb(0x9F794E),
                marker: rgb(0x434D58),
                field: rgb(0x384049),
                rule: rgb(0x3D454E),
                hover: rgb(0x353D46),
                lit_dots: None,
                heading: Some(rgb(0xC695C6)),
            },
            Theme::Soulseek => Self {
                ground: rgb(0xD4D0C8),
                ink: rgb(0x000000),
                mid: rgb(0x5E5E5E),
                faint: rgb(0x808080),
                accent: rgb(0x0A246A),
                accent_faint: rgb(0x3A6EA5),
                marker: rgb(0xFFFFFF),
                field: rgb(0xE6E3DD),
                rule: rgb(0xB5B1A8),
                hover: rgb(0xDDDAD3),
                lit_dots: None,
                heading: Some(rgb(0x007070)),
            },

            Theme::NordFrost => Self {
                ground: rgb(0x88C0D0),
                ink: rgb(0x1C2129),
                mid: rgb(0x33495A),
                faint: rgb(0x5A8A9B),
                accent: rgb(0x8E2F3A),
                accent_faint: rgb(0xA86A6F),
                marker: rgb(0x6FA9BB),
                field: rgb(0x80B8C8),
                rule: rgb(0x74AFC0),
                hover: rgb(0x84BCCC),
                lit_dots: None,
                heading: Some(rgb(0x5E3F6B)),
            },
            Theme::Hudson => Self {
                ground: rgb(0x1A46B8),
                ink: rgb(0xFFFFFF),
                mid: rgb(0xC9D6F7),
                faint: rgb(0xE0C25A),
                accent: rgb(0xFFD83A),
                accent_faint: rgb(0x8C8F79),
                marker: rgb(0x0C2C84),
                field: rgb(0x2150C2),
                rule: rgb(0xB89A2E),
                hover: rgb(0x1F4CBF),
                lit_dots: None,
                heading: Some(rgb(0xFF9F8F)),
            },
            Theme::PcEngine => Self {
                ground: rgb(0xECEAE4),
                ink: rgb(0x1B1F33),
                mid: rgb(0x2A46B0),
                faint: rgb(0xC0602A),
                accent: rgb(0xC8102E),
                accent_faint: rgb(0xDA7D89),
                marker: rgb(0xF5A36B),
                field: rgb(0xE2DFD7),
                rule: rgb(0xAEB8DE),
                hover: rgb(0xE6E3DC),
                lit_dots: None,
                heading: Some(rgb(0x4F6A12)),
            },
            Theme::PcFx => Self {
                ground: rgb(0xFFFFFF),
                ink: rgb(0x1E1624),
                mid: rgb(0xB8186A),
                faint: rgb(0xA68A00),
                accent: rgb(0x2E5BD8),
                accent_faint: rgb(0x96ADEC),
                marker: rgb(0xF8E36A),
                field: rgb(0xFCEAF2),
                rule: rgb(0xF3C6D8),
                hover: rgb(0xFBF6FC),
                lit_dots: None,
                heading: Some(rgb(0x9A1FC8)),
            },
            Theme::Pc98 => Self {
                ground: rgb(0x0B1030),
                ink: rgb(0xE8F0FF),
                mid: rgb(0x55E0D8),
                faint: rgb(0x7FD86A),
                accent: rgb(0xFF5FA8),
                accent_faint: rgb(0x85386C),
                marker: rgb(0x1E2A6E),
                field: rgb(0x141A42),
                rule: rgb(0x2A3A8A),
                hover: rgb(0x111840),
                lit_dots: None,
                heading: Some(rgb(0xFFE05A)),
            },
            Theme::X68000 => Self {
                ground: rgb(0x1B1C1F),
                ink: rgb(0xE9E8E4),
                mid: rgb(0x9C9B96),
                faint: rgb(0x5A5A58),
                accent: rgb(0xFF4433),
                accent_faint: rgb(0xA33A30),
                marker: rgb(0x36373B),
                field: rgb(0x25262A),
                rule: rgb(0x2E2F33),
                hover: rgb(0x222326),
                lit_dots: None,
                heading: Some(rgb(0xE0B040)),
            },
            Theme::FmTowns => Self {
                ground: rgb(0xFFFBFF),
                ink: rgb(0x212021),
                mid: rgb(0x737173),
                faint: rgb(0xB5B2B5),
                accent: rgb(0xD8383C),
                accent_faint: rgb(0xEE9A9C),
                marker: rgb(0xFFC4C6),
                field: rgb(0xECE9EC),
                rule: rgb(0xC6C3C6),
                hover: rgb(0xF7F3F7),
                lit_dots: None,
                heading: Some(rgb(0x3A6BB0)),
            },
            Theme::Koen => Self {
                ground: rgb(0x33312E),
                ink: rgb(0xF2DC8A),
                mid: rgb(0x6FD3C0),
                faint: rgb(0xF28C8C),
                accent: rgb(0xFFD41F),
                accent_faint: rgb(0x998226),
                marker: rgb(0x821A23),
                field: rgb(0x294459),
                rule: rgb(0x5A4538),
                hover: rgb(0x3A3835),
                lit_dots: Some(rgb(0x5AB7EF)),
                heading: Some(rgb(0x5AB7EF)),
            },
            Theme::NordHell => Self {
                ground: rgb(0xECEFF4),
                ink: rgb(0x2E3440),
                mid: rgb(0x4C566A),
                faint: rgb(0x9AA5B8),
                accent: rgb(0x5277A3),
                accent_faint: rgb(0x9DB2CF),
                marker: rgb(0xD3DAE5),
                field: rgb(0xE3E7EE),
                rule: rgb(0xD8DEE9),
                hover: rgb(0xE8EBF1),
                lit_dots: None,
                heading: Some(rgb(0x8A6584)),
            },
            Theme::DraculaHell => Self {
                ground: rgb(0xFFFBEB),
                ink: rgb(0x1F1F1F),
                mid: rgb(0x635D97),
                faint: rgb(0xB0AAC8),
                accent: rgb(0xA3144D),
                accent_faint: rgb(0xD08AA6),
                marker: rgb(0xEFE6CB),
                field: rgb(0xF6F0DB),
                rule: rgb(0xE8E0C6),
                hover: rgb(0xFBF6E4),
                lit_dots: None,
                heading: Some(rgb(0x036A96)),
            },
            Theme::SublimeHell => Self {
                ground: rgb(0xF7F9FA),
                ink: rgb(0x2C333A),
                mid: rgb(0x5F6B76),
                faint: rgb(0xA9B2BA),
                accent: rgb(0xB0520A),
                accent_faint: rgb(0xD9A27A),
                marker: rgb(0xDEE4E9),
                field: rgb(0xECF0F2),
                rule: rgb(0xE1E6EA),
                hover: rgb(0xF1F4F5),
                lit_dots: None,
                heading: Some(rgb(0x7E4F96)),
            },
            Theme::IpodDunkel => Self {
                ground: rgb(0x21251C),
                ink: rgb(0xCBCFC1),
                mid: rgb(0x8F9486),
                faint: rgb(0x5C6154),
                accent: rgb(0xE8ECDD),
                accent_faint: rgb(0x9EA294),
                marker: rgb(0x3C4136),
                field: rgb(0x2A2E24),
                rule: rgb(0x363A30),
                hover: rgb(0x262A21),
                lit_dots: None,
                heading: Some(rgb(0x7FA7C8)),
            },
            Theme::SoulseekDunkel => Self {
                ground: rgb(0x2B2A27),
                ink: rgb(0xE8E5DF),
                mid: rgb(0xA8A49C),
                faint: rgb(0x6F6C66),
                accent: rgb(0x8FB6E0),
                accent_faint: rgb(0x5A7FA8),
                marker: rgb(0x45443F),
                field: rgb(0x353430),
                rule: rgb(0x403F3A),
                hover: rgb(0x302F2C),
                lit_dots: None,
                heading: Some(rgb(0x4FB3B3)),
            },
            Theme::NordFrostDunkel => Self {
                ground: rgb(0x1C2A33),
                ink: rgb(0xDCEBF0),
                mid: rgb(0x8FB4C2),
                faint: rgb(0x4F6E7B),
                accent: rgb(0x88C0D0),
                accent_faint: rgb(0x5E8795),
                marker: rgb(0x2F4550),
                field: rgb(0x24343E),
                rule: rgb(0x2C3E48),
                hover: rgb(0x21303A),
                lit_dots: None,
                heading: Some(rgb(0xB48EAD)),
            },
            Theme::HudsonHell => Self {
                ground: rgb(0xFFF3C4),
                ink: rgb(0x0C2C84),
                mid: rgb(0x3D5BB5),
                faint: rgb(0x8C7420),
                accent: rgb(0x1A46B8),
                accent_faint: rgb(0x8C9CBE),
                marker: rgb(0xFFD83A),
                field: rgb(0xFFEDB0),
                rule: rgb(0xF2D36B),
                hover: rgb(0xFFF0BC),
                lit_dots: None,
                heading: Some(rgb(0xC8102E)),
            },
            Theme::PcEngineDunkel => Self {
                ground: rgb(0x1B1F33),
                ink: rgb(0xECEAE4),
                mid: rgb(0x9FB4F5),
                faint: rgb(0xC98A5A),
                accent: rgb(0xFF6A5A),
                accent_faint: rgb(0x8D4446),
                marker: rgb(0x2E3A6E),
                field: rgb(0x242942),
                rule: rgb(0x5A4A6E),
                hover: rgb(0x20253B),
                lit_dots: None,
                heading: Some(rgb(0xF5A36B)),
            },
            Theme::PcFxDunkel => Self {
                ground: rgb(0x1E1624),
                ink: rgb(0xF4EEF8),
                mid: rgb(0xFF8FC4),
                faint: rgb(0xE8CF5A),
                accent: rgb(0x7FB2FF),
                accent_faint: rgb(0x4E6492),
                marker: rgb(0x3E2A50),
                field: rgb(0x2A2032),
                rule: rgb(0x5A2E48),
                hover: rgb(0x251C2C),
                lit_dots: None,
                heading: Some(rgb(0xD597F0)),
            },
            Theme::Pc98Hell => Self {
                ground: rgb(0xE6F4F1),
                ink: rgb(0x0B1030),
                mid: rgb(0x0E7068),
                faint: rgb(0x3A8A2E),
                accent: rgb(0xC2186E),
                accent_faint: rgb(0xD486B0),
                marker: rgb(0xB8E2DA),
                field: rgb(0xD8EEEA),
                rule: rgb(0xC4E2DC),
                hover: rgb(0xDEF0EC),
                lit_dots: None,
                heading: Some(rgb(0x1A3AB8)),
            },
            Theme::X68000Hell => Self {
                ground: rgb(0xECEBE7),
                ink: rgb(0x1B1C1F),
                mid: rgb(0x5E5D59),
                faint: rgb(0xA9A8A3),
                accent: rgb(0xC8281A),
                accent_faint: rgb(0xE0857C),
                marker: rgb(0xD3D1CB),
                field: rgb(0xE2E1DC),
                rule: rgb(0xD9D8D2),
                hover: rgb(0xE6E5E1),
                lit_dots: None,
                heading: Some(rgb(0x8A6A10)),
            },
            Theme::FmTownsDunkel => Self {
                ground: rgb(0x211F21),
                ink: rgb(0xEFEBEF),
                mid: rgb(0xA6A2A6),
                faint: rgb(0x625E62),
                accent: rgb(0xFF7A7C),
                accent_faint: rgb(0xA84B4D),
                marker: rgb(0x5A2A2E),
                field: rgb(0x2C292C),
                rule: rgb(0x3A363A),
                hover: rgb(0x272427),
                lit_dots: None,
                heading: Some(rgb(0x7FA4E0)),
            },
            Theme::PaysageDHiver => Self {
                ground: rgb(0x0A0A0A),
                ink: rgb(0xD6D6D6),
                mid: rgb(0x8A8A8A),
                faint: rgb(0x4C4C4C),
                accent: rgb(0xFFFFFF),
                accent_faint: rgb(0x9C9C9C),
                marker: rgb(0x262626),
                field: rgb(0x171717),
                rule: rgb(0x2A2A2A),
                hover: rgb(0x141414),
                lit_dots: None,
                heading: Some(rgb(0xB4B4B4)),
            },
            Theme::PaysageDHiverHell => Self {
                ground: rgb(0xD9D9D9),
                ink: rgb(0x141414),
                mid: rgb(0x4A4A4A),
                faint: rgb(0x8E8E8E),
                accent: rgb(0x000000),
                accent_faint: rgb(0x6A6A6A),
                marker: rgb(0xBDBDBD),
                field: rgb(0xCDCDCD),
                rule: rgb(0xC2C2C2),
                hover: rgb(0xD1D1D1),
                lit_dots: None,
                heading: Some(rgb(0x2C2C2C)),
            },
            Theme::CocoIchibanya => Self {
                ground: rgb(0xFFEFA0),
                ink: rgb(0x3A200C),
                mid: rgb(0x8A4A10),
                faint: rgb(0xB8860B),
                accent: rgb(0xB3170C),
                accent_faint: rgb(0xD98356),
                marker: rgb(0xF5C842),
                field: rgb(0xFFE68E),
                rule: rgb(0xF0CF5A),
                hover: rgb(0xFFEA98),
                lit_dots: None,
                heading: Some(rgb(0x3F6E2A)),
            },
            Theme::CocoIchibanyaDunkel => Self {
                ground: rgb(0x2E1A0C),
                ink: rgb(0xFFF3D6),
                mid: rgb(0xE8B060),
                faint: rgb(0xC9875A),
                accent: rgb(0xFF6A3A),
                accent_faint: rgb(0x964223),
                marker: rgb(0x5A3620),
                field: rgb(0x3A2414),
                rule: rgb(0x5A3A22),
                hover: rgb(0x38200F),
                lit_dots: None,
                heading: Some(rgb(0xFFD500)),
            },
            Theme::Runebending => Self {
                ground: rgb(0xE7E6E2),
                ink: rgb(0x17171B),
                mid: rgb(0x2E6A2A),
                faint: rgb(0xA8602A),
                accent: rgb(0x5A3FC0),
                accent_faint: rgb(0x8E80D6),
                marker: rgb(0xCFCEC9),
                field: rgb(0xDDDCD8),
                rule: rgb(0xCBCAC5),
                hover: rgb(0xDEDDD9),
                lit_dots: Some(rgb(0xD0506A)),
                heading: Some(rgb(0x8B6BB5)),
            },
            Theme::RunebendingDunkel => Self {
                ground: rgb(0x1F1F21),
                ink: rgb(0xEDEDED),
                mid: rgb(0x8CCB7B),
                faint: rgb(0xB87A48),
                accent: rgb(0xF0708A),
                accent_faint: rgb(0xA45466),
                marker: rgb(0x37373A),
                field: rgb(0x29292B),
                rule: rgb(0x3A3A3D),
                hover: rgb(0x2A2A2C),
                lit_dots: Some(rgb(0x8C7CF0)),
                heading: Some(rgb(0x8C7CF0)),
            },
            Theme::Rubrik => Self {
                ground: rgb(0xD3E4CF),
                ink: rgb(0x1A1714),
                mid: rgb(0xA8281F),
                faint: rgb(0x5E6E78),
                accent: rgb(0x2B4FA8),
                accent_faint: rgb(0x7F9ABC),
                marker: rgb(0xB9CFB4),
                field: rgb(0xC9DCC4),
                rule: rgb(0xBACFB4),
                hover: rgb(0xCCDEC7),
                lit_dots: None,
                heading: Some(rgb(0x7A5A10)),
            },
            Theme::RubrikDunkel => Self {
                ground: rgb(0x162019),
                ink: rgb(0xEFE8DA),
                mid: rgb(0xE0705E),
                faint: rgb(0x8FA39A),
                accent: rgb(0x7FA2F0),
                accent_faint: rgb(0x4A6184),
                marker: rgb(0x2E3E33),
                field: rgb(0x1E2A22),
                rule: rgb(0x324238),
                hover: rgb(0x1C2720),
                lit_dots: None,
                heading: Some(rgb(0xD8B060)),
            },
            Theme::WilderMann => Self {
                ground: rgb(0x282828),
                ink: rgb(0xEBDBB2),
                mid: rgb(0x83A598),
                faint: rgb(0xA89984),
                accent: rgb(0x8EC07C),
                accent_faint: rgb(0x66855B),
                marker: rgb(0x504945),
                field: rgb(0x3C3836),
                rule: rgb(0x504945),
                hover: rgb(0x32302F),
                lit_dots: None,
                heading: Some(rgb(0xFABD2F)),
            },
            Theme::WilderMannHell => Self {
                ground: rgb(0xFBF1C7),
                ink: rgb(0x3C3836),
                mid: rgb(0x076678),
                faint: rgb(0x928374),
                accent: rgb(0x427B58),
                accent_faint: rgb(0x8FAC86),
                marker: rgb(0xEBDBB2),
                field: rgb(0xF2E5BC),
                rule: rgb(0xD5C4A1),
                hover: rgb(0xF4E9C1),
                lit_dots: None,
                heading: Some(rgb(0x8F3F71)),
            },
            Theme::Ochsenblut => Self {
                ground: rgb(0x1D2021),
                ink: rgb(0xFBF1C7),
                mid: rgb(0xFE8019),
                faint: rgb(0x7C6F64),
                accent: rgb(0xB8BB26),
                accent_faint: rgb(0x7C7E2A),
                marker: rgb(0x4A2320),
                field: rgb(0x282828),
                rule: rgb(0x45403D),
                hover: rgb(0x2C2423),
                lit_dots: Some(rgb(0xFABD2F)),
                heading: Some(rgb(0xFB4934)),
            },
            Theme::OchsenblutHell => Self {
                ground: rgb(0xF9F5D7),
                ink: rgb(0x282828),
                mid: rgb(0xAF3A03),
                faint: rgb(0x7C6F64),
                accent: rgb(0x79740E),
                accent_faint: rgb(0xB0AD6B),
                marker: rgb(0xEBCFC2),
                field: rgb(0xF2E5BC),
                rule: rgb(0xE0CFB3),
                hover: rgb(0xF3EBD0),
                lit_dots: Some(rgb(0xB57614)),
                heading: Some(rgb(0x9D0006)),
            },
            Theme::Lehmgefach => Self {
                ground: rgb(0xF2E5BC),
                ink: rgb(0x282828),
                mid: rgb(0x79740E),
                faint: rgb(0x8F6F86),
                accent: rgb(0x076678),
                accent_faint: rgb(0x83A5AD),
                marker: rgb(0xD5C4A1),
                field: rgb(0xEBDBB2),
                rule: rgb(0xD5C4A1),
                hover: rgb(0xEADDB4),
                lit_dots: Some(rgb(0x427B58)),
                heading: Some(rgb(0xAF3A03)),
            },
            Theme::LehmgefachDunkel => Self {
                ground: rgb(0x32302F),
                ink: rgb(0xFBF1C7),
                mid: rgb(0xB8BB26),
                faint: rgb(0xB49AAB),
                accent: rgb(0x83A598),
                accent_faint: rgb(0x55706C),
                marker: rgb(0x504945),
                field: rgb(0x3C3836),
                rule: rgb(0x504945),
                hover: rgb(0x3A3735),
                lit_dots: Some(rgb(0x8EC07C)),
                heading: Some(rgb(0xFE8019)),
            },
            Theme::Rgb => Self {
                ground: rgb(0x000000),
                ink: rgb(0xFFFFFF),
                mid: rgb(0x00FF00),
                faint: rgb(0x00FFFF),
                accent: rgb(0xFF0000),
                accent_faint: rgb(0xFF0000),
                marker: rgb(0x0000FF),
                field: rgb(0x0000FF),
                rule: rgb(0x0000FF),
                hover: rgb(0x000080),
                lit_dots: None,
                heading: Some(rgb(0xFF00FF)),
            },
            Theme::RgbHell => Self {
                ground: rgb(0xFFFFFF),
                ink: rgb(0x000000),
                mid: rgb(0x0000FF),
                faint: rgb(0xFF00FF),
                accent: rgb(0xFF0000),
                accent_faint: rgb(0xFF0000),
                marker: rgb(0xFFFF00),
                field: rgb(0x00FFFF),
                rule: rgb(0x00FF00),
                hover: rgb(0x00FFFF),
                lit_dots: None,
                heading: Some(rgb(0x008000)),
            },
            Theme::KoenHell => Self {
                ground: rgb(0xEFE6D2),
                ink: rgb(0x2B2A27),
                mid: rgb(0x2E7D5A),
                faint: rgb(0xC25E3A),
                accent: rgb(0xA81C27),
                accent_faint: rgb(0xCC817C),
                marker: rgb(0xE8C65A),
                field: rgb(0xE4DAC4),
                rule: rgb(0xD8CCB2),
                hover: rgb(0xE9DFC9),
                lit_dots: Some(rgb(0x2A6FA3)),
                heading: Some(rgb(0x2A6FA3)),
            },
            Theme::Zeitvogel => Self {
                ground: rgb(0x231C33),
                ink: rgb(0xEAE3F6),
                mid: rgb(0x9DD47E),
                faint: rgb(0x9488AE),
                accent: rgb(0xC3A6FF),
                accent_faint: rgb(0x705E95),
                marker: rgb(0x463A63),
                field: rgb(0x2D2541),
                rule: rgb(0x3D3354),
                hover: rgb(0x2A2240),
                lit_dots: None,
                heading: Some(rgb(0x5FC4B6)),
            },
            Theme::ZeitvogelHell => Self {
                ground: rgb(0xD6DCAB),
                ink: rgb(0x1B2A1C),
                mid: rgb(0x22594F),
                faint: rgb(0x58652E),
                accent: rgb(0x6A35C2),
                accent_faint: rgb(0xA591B5),
                marker: rgb(0xBDAED6),
                field: rgb(0xCAD19E),
                rule: rgb(0xBAC18F),
                hover: rgb(0xCFD6A4),
                lit_dots: None,
                heading: Some(rgb(0x9A3A26)),
            },
        }
    }

    /// The same scheme with its colour in the type, as runebender schrift and
    /// rubrik have it by themselves. The coloured plates behind the words — the
    /// marker, the search field, the rule — take the ground's hue and come
    /// halfway to its lightness, and the colours they had go into the second
    /// and third tones of the words, each as light as the tone it colours, so
    /// nothing reads harder than before: the most colourful to the second tone,
    /// one of another hue to the third. A scheme whose plates are grey has no
    /// colour to give, and one whose words are coloured already keeps them;
    /// tinting a grey scheme's words with its accent made everything the colour
    /// of what plays. The columns' heads take a colour of their own, as dark as
    /// the second tone: the third tone's, else the second's, else the most
    /// colourful plate's, else the accent's. The ink, the accent and the ground
    /// are left alone.
    pub fn in_the_type(&self) -> Palette {
        let ground = Lab::of(self.ground);
        let colourful = |lab: &Lab| lab.chroma() - ground.chroma() > COLOURFUL;
        let plain = |c: Color32| {
            let lab = Lab::of(c);
            let toward = if colourful(&lab) { PLATE_TOWARD_GROUND } else { 0.0 };
            Lab { l: lab.l + (ground.l - lab.l) * toward, a: ground.a, b: ground.b }.colour()
        };
        let mut given: Vec<Lab> = [self.marker, self.field, self.rule].into_iter().map(Lab::of).filter(colourful).collect();
        given.sort_by(|p, q| q.chroma().total_cmp(&p.chroma()));
        let typed = [self.mid, self.faint].iter().any(|&tone| Lab::of(tone).chroma() > COLOURFUL);
        let (mid, faint) = match given.first() {
            Some(&first) if !typed => {
                let second = given.iter().copied().find(|lab| lab.hue_apart(&first) > HUE_APART).unwrap_or(first);
                (
                    first.at_lightness(Lab::of(self.mid).l).colour(),
                    second.at_lightness(Lab::of(self.faint).l).colour(),
                )
            }
            _ => (self.mid, self.faint),
        };
        let heads = [Lab::of(faint), Lab::of(mid)]
            .into_iter()
            .find(|lab| lab.chroma() > COLOURFUL)
            .or(given.first().copied())
            .unwrap_or(Lab::of(self.accent));
        Palette {
            mid,
            faint,
            marker: plain(self.marker),
            field: plain(self.field),
            rule: plain(self.rule),
            hover: plain(self.hover),
            heading: Some(heads.at_lightness(Lab::of(self.mid).l).colour()),
            ..*self
        }
    }

    /// The same scheme with its colour on the plates. Until then „flächen" was
    /// the scheme as drawn, and most schemes draw their plates in a grey, so
    /// the two looked alike but for the words. Now the marker, the search
    /// field, the rule and the hover take a colour each, at the lightness they
    /// had, so nothing on them reads harder: a plate coloured already keeps its
    /// hue and is made at least so colourful, a grey one takes the hue of the
    /// scheme's most colourful plate, or of its accent. The words are left as
    /// the scheme has them. A scheme with no colour at all (paysage d'hiver)
    /// stays grey.
    pub fn on_the_plates(&self) -> Palette {
        let ground = Lab::of(self.ground);
        let colourful = |lab: &Lab| lab.chroma() - ground.chroma() > COLOURFUL;
        let source = [self.marker, self.field, self.rule]
            .into_iter()
            .map(Lab::of)
            .filter(colourful)
            .max_by(|p, q| p.chroma().total_cmp(&q.chroma()))
            .unwrap_or(Lab::of(self.accent));
        if source.chroma() <= COLOURFUL {
            return *self;
        }
        let tint = |c: Color32, least: f32| {
            let lab = Lab::of(c);
            let hue = if lab.chroma() > COLOURFUL { lab } else { source };
            let chroma = lab.chroma().max(least);
            let (a, b) = (hue.a / hue.chroma(), hue.b / hue.chroma());
            Lab { l: lab.l, a: a * chroma, b: b * chroma }.colour()
        };
        let [marker, field, rule, hover] = PLATE_CHROMA;
        Palette {
            marker: tint(self.marker, marker),
            field: tint(self.field, field),
            rule: tint(self.rule, rule),
            hover: tint(self.hover, hover),
            ..*self
        }
    }

    /// The scheme as the window shows it: the plates coloured, as
    /// [`Self::on_the_plates`] makes them, and the columns' heads in a colour
    /// of their own, as [`Self::in_the_type`] finds one — the words otherwise
    /// as the scheme has them. Until then „farbfokus" chose one or the other;
    /// one with the colour on the plates lost the heads'. A scheme that names
    /// its heads' colour keeps it.
    ///
    /// The third tone is held to at least [`FAINT_LEAST`] against the ground,
    /// so a track's number can be read.
    pub fn coloured(&self) -> Palette {
        let typed = self.in_the_type();
        let heading = self.heading.or(typed.heading);
        let faint = Palette::legible_on(self.faint, self.ground, FAINT_LEAST);
        Palette { heading, faint, field: typed.field, ..self.on_the_plates() }
    }

    /// Part of the way from this scheme to `other`, every colour alike —
    /// `t` nought is this, one is `other`. For the fade between two.
    pub fn mix(&self, other: &Palette, t: f32) -> Palette {
        let m = |a: Color32, b: Color32| a.lerp_to_gamma(b, t);
        Palette {
            ground: m(self.ground, other.ground),
            ink: m(self.ink, other.ink),
            mid: m(self.mid, other.mid),
            faint: m(self.faint, other.faint),
            accent: m(self.accent, other.accent),
            accent_faint: m(self.accent_faint, other.accent_faint),
            marker: m(self.marker, other.marker),
            field: m(self.field, other.field),
            rule: m(self.rule, other.rule),
            hover: m(self.hover, other.hover),
            lit_dots: match (self.lit_dots, other.lit_dots) {
                (None, None) => None,
                (a, b) => Some(m(a.unwrap_or(self.accent), b.unwrap_or(other.accent))),
            },
            heading: match (self.heading, other.heading) {
                (None, None) => None,
                (a, b) => Some(m(a.unwrap_or(self.faint), b.unwrap_or(other.faint))),
            },
        }
    }

    /// The colour the columns' heads are written in — each scheme names its
    /// own, apart from the facts, the second tone and the accent (see the
    /// test `the_heads_stand_apart`); the third tone only where none is
    /// named.
    pub fn heading(&self) -> Color32 {
        self.heading.unwrap_or(self.faint)
    }

    /// The colours a screened picture's dots take under `ink`, the one nearest
    /// the field first: none in two tones, one in three, two in four.
    ///
    /// Each stands at its share of the way from the field's lightness to the
    /// ink's, evenly. Until then it was the accent, and in four the faint tone
    /// under it, whatever their lightness: in pc-98 at night the faint green
    /// was lighter than the pink accent above it, in ochsenblut by day the two
    /// were equally light, and the picture lost its shape. Now each takes the
    /// scheme's colour nearest that lightness — the accent where it is about as
    /// near as any, a colourful one before a grey — and is brought the rest of
    /// the way there, keeping its hue. Never the ink itself, nor a colour taken
    /// twice; with nothing left, the ink part of the way to the field.
    pub fn tones_under(&self, ink: Color32, tones: u8) -> Vec<Color32> {
        let between = usize::from(tones.clamp(2, 4)) - 2;
        let (field, top) = (Lab::of(self.field), Lab::of(ink));
        let light = field.l > top.l;
        let offered = [
            (self.accent, 0.0),
            (self.lit_dots(), DITHER_OTHER),
            (self.accent_faint, DITHER_OTHER),
            (self.mid, DITHER_OTHER),
            (self.faint, DITHER_OTHER),
            (self.heading(), DITHER_OTHER),
        ];
        let mut taken: Vec<Color32> = vec![ink];
        (1..=between)
            .map(|i| {
                let share = i as f32 / (between + 1) as f32;
                let target = field.l + (top.l - field.l) * share;
                let near = DITHER_NEAR * (top.l - field.l).abs();
                let cost = |c: Color32, extra: f32| {
                    let lab = Lab::of(c);
                    let grey = if lab.chroma() < COLOURFUL { DITHER_GREY } else { 0.0 };
                    (lab.l - target).abs() + extra + grey
                };
                let best = offered
                    .iter()
                    .filter(|(c, _)| !taken.contains(c))
                    .min_by(|(a, x), (b, y)| cost(*a, *x).total_cmp(&cost(*b, *y)))
                    .map(|(c, _)| *c);
                match best {
                    Some(c) => {
                        taken.push(c);
                        let lab = Lab::of(c);
                        let lab = lab.at_lightness(lab.l.clamp(target - near, target + near));
                        let lab = if light && lab.chroma() > DITHER_LIGHT_CHROMA {
                            lab.quieter(DITHER_LIGHT_CHROMA / lab.chroma())
                        } else {
                            lab
                        };
                        lab.colour()
                    }
                    None => self.field.lerp_to_gamma(ink, share),
                }
            })
            .collect()
    }

    /// The one tone under the ink in three — see [`Self::tones_under`].
    pub fn third_tone(&self, ink: Color32) -> Color32 {
        self.tones_under(ink, 3).first().copied().unwrap_or(self.field.lerp_to_gamma(ink, 0.5))
    }

    /// The colour a marked halftone cover is drawn in: the accent, unless
    /// the scheme names a colour of its own for it.
    pub fn lit_dots(&self) -> Color32 {
        self.lit_dots.unwrap_or(self.accent)
    }

    /// Whether a halftone has to be screened from the picture's negative.
    ///
    /// The screen sets a dot where the picture is light, and the dot is drawn
    /// in the ink. On a dark ground the ink is light, and a light part of the
    /// picture comes out light. On a light ground the ink is dark, and the same
    /// dot made every cover and every face a photographic negative. There the
    /// dots go where the picture is dark instead.
    ///
    /// Only for pictures of something — covers, the artist's picture. The
    /// scope and the screen saver draw light itself, which is ink on any
    /// ground.
    pub fn screens_negative(&self) -> bool {
        !self.is_dark()
    }

    /// The plate under the keyboard's row and the one clicked: the scheme's
    /// marker a step towards the ink, so the one row `j` and `k` move is the
    /// one that is seen. The menu's was the plain marker, a shade lighter for
    /// the same thing.
    ///
    /// At least [`MARKED_LEAST`] against the ground: where the marker sits
    /// nearly on the ground's own lightness (hu-card's navy on its blue) it
    /// goes further towards the ink until it is seen.
    pub fn marked(&self) -> Color32 {
        let mut t = 0.14;
        let mut marked = self.marker.lerp_to_gamma(self.ink, t);
        while contrast(marked, self.ground) < MARKED_LEAST && t < 0.5 {
            t += 0.02;
            marked = self.marker.lerp_to_gamma(self.ink, t);
        }
        marked
    }

    /// `colour`, its hue and colourfulness kept, made light or dark enough
    /// to stand at least `least` against `behind`: moved away from the
    /// lightness of what is behind it, a step at a time, as far as it must.
    pub fn legible_on(colour: Color32, behind: Color32, least: f32) -> Color32 {
        if contrast(colour, behind) >= least {
            return colour;
        }
        let lab = Lab::of(colour);
        let mut best = (contrast(colour, behind), colour);
        for step in 1..=100 {
            for l in [lab.l + 0.01 * step as f32, lab.l - 0.01 * step as f32] {
                if !(0.0..=1.0).contains(&l) {
                    continue;
                }
                let shown = lab.at_lightness(l).colour();
                let reached = contrast(shown, behind);
                if reached >= least {
                    return shown;
                }
                if reached > best.0 {
                    best = (reached, shown);
                }
            }
        }
        best.1
    }

    /// Whether this scheme sits on a dark ground. egui needs telling, and the
    /// answer is simply how light the ground is.
    pub fn is_dark(&self) -> bool {
        let sum =
            self.ground.r() as u32 + self.ground.g() as u32 + self.ground.b() as u32;
        sum < 384
    }
}

/// How far the marked row's plate stands at least from the ground, and the
/// third tone at least from the ground; what is small on a plate — a track's
/// number, „flac", its length — at least from the plate.
pub const MARKED_LEAST: f32 = 1.3;

/// How far each scheme's marker is taken back towards the ground, and how far
/// its hover is taken on away from it — see `Palette::plates_retuned`.
const MARKER_QUIETER: f32 = 0.15;
const HOVER_STRONGER: f32 = 1.4;
pub const FAINT_LEAST: f32 = 2.2;
pub const ON_PLATE_LEAST: f32 = 3.0;

/// The contrast of two colours as WCAG reckons it: 1 for the same, 21 for
/// black on white.
pub fn contrast(a: Color32, b: Color32) -> f32 {
    fn luminance(c: Color32) -> f32 {
        let f = |v: u8| {
            let v = v as f32 / 255.0;
            if v <= 0.03928 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * f(c.r()) + 0.7152 * f(c.g()) + 0.0722 * f(c.b())
    }
    let (x, y) = (luminance(a), luminance(b));
    let (hi, lo) = if x > y { (x, y) } else { (y, x) };
    (hi + 0.05) / (lo + 0.05)
}

/// Everything the interface draws goes through here.
///
/// Lowercase throughout, after the HfG Ulm, which set its own name that way.
/// The practical gain in a list of several thousand rows: without capitals the
/// line is quieter and word shapes stay easier to tell apart.
///
/// Unless capitals are asked for, with ⌥ under „darstellung": then names stand
/// as they were written. The window's own words are written small in the first
/// place, and stay so.
pub fn lower(text: &str) -> String {
    if CAPITALS.load(std::sync::atomic::Ordering::Relaxed) {
        text.to_string()
    } else {
        text.to_lowercase()
    }
}

/// Whether capitals are kept — see [`lower`]. Set once a frame from the
/// config.
static CAPITALS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_capitals(on: bool) {
    CAPITALS.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// Where the machine keeps typefaces.
///
/// The user's own folder comes first, which is also the answer to "how do I add
/// a font": put the files there and they show up in the menu. Nothing is ever
/// bundled — shipping a face would mean shipping its licence.
fn font_dirs() -> Vec<std::path::PathBuf> {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    let mut dirs: Vec<std::path::PathBuf> = Vec::new();

    if cfg!(target_os = "macos") {
        if let Some(h) = &home {
            dirs.push(h.join("Library/Fonts"));
        }
        dirs.push("/Library/Fonts".into());
        dirs.push("/System/Library/Fonts/Supplemental".into());
        dirs.push("/System/Library/Fonts".into());
    } else if cfg!(target_os = "windows") {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(std::path::PathBuf::from(local).join("Microsoft/Windows/Fonts"));
        }
        dirs.push("C:\\Windows\\Fonts".into());
    } else {
        if let Some(h) = &home {
            dirs.push(h.join(".local/share/fonts"));
            dirs.push(h.join(".fonts"));
        }
        dirs.push("/usr/local/share/fonts".into());
        dirs.push("/usr/share/fonts".into());
    }
    dirs
}

/// Which of the two weights a file is, if either.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Weight {
    Regular,
    Bold,
    /// Italic, light, condensed — real faces, but not ones this design uses.
    Other,
}

/// Reject font collections and anything that is not a font at all.
///
/// A `.ttc` holds several faces; egui takes the file whole and panics on one.
/// Checking four bytes is cheaper than crashing on the user's font folder.
fn is_single_face(path: &std::path::Path) -> bool {
    use std::io::Read;
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut magic = [0u8; 4];
    if file.read_exact(&mut magic).is_err() {
        return false;
    }
    matches!(
        &magic,
        [0x00, 0x01, 0x00, 0x00] | b"OTTO" | b"true" | b"typ1"
    )
}

/// One family from the `fonts` folder, in whichever of its two shapes it has.
///
/// Two shapes rather than two families, because that is what they are: mono and
/// proportional cut of the same design. Choosing between them is a separate
/// question from choosing the typeface, and the menu asks it separately.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Family {
    /// What the menu calls it, lowercased like everything else.
    pub name: String,
    /// What the config stores. Letters and digits only, so a renamed file or a
    /// changed spelling cannot orphan a setting.
    pub key: String,
    pub proportional: Option<Faces>,
    pub mono: Option<Faces>,
}

/// The two weights of one cut.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Faces {
    pub regular: std::path::PathBuf,
    pub bold: Option<std::path::PathBuf>,
}

impl Family {
    /// The cut asked for, or the other one if this family only has one.
    pub fn faces(&self, mono: bool) -> Option<&Faces> {
        let (wanted, other) = if mono {
            (&self.mono, &self.proportional)
        } else {
            (&self.proportional, &self.mono)
        };
        wanted.as_ref().or(other.as_ref())
    }
}

/// Where the program's own typefaces are, if they came with it.
///
/// Looked for in the places the same folder ends up in: beside the executable,
/// inside a macOS bundle's `Resources`, a couple of levels up from
/// `target/release` while developing, and the working directory. The config can
/// name a different one, which is the answer for anyone who keeps them
/// elsewhere.
pub fn font_folder(configured: Option<&str>) -> Option<std::path::PathBuf> {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if let Some(named) = configured {
        candidates.push(std::path::PathBuf::from(named));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("fonts"));
            candidates.push(dir.join("../Resources/fonts"));
            candidates.push(dir.join("../../fonts"));
        }
    }
    candidates.push(std::path::PathBuf::from("fonts"));

    candidates.into_iter().find(|dir| dir.is_dir())
}

/// What a file name says about the face inside it.
///
/// Only three things are asked: which family, whether it is the mono cut, and
/// which of the two weights. Everything else in a name is ignored rather than
/// guessed at.
fn read_face_name(stem: &str) -> (String, bool, Weight) {
    let lower = stem.to_lowercase();
    let squashed: String = lower
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect();

    let mono = squashed.contains("mono");
    let weight = if squashed.contains("bold") {
        Weight::Bold
    } else if squashed.contains("italic") || squashed.contains("oblique") {
        Weight::Other
    } else {
        Weight::Regular
    };

    let mut family = squashed.clone();
    for word in [
        "mono", "bold", "regular", "medium", "light", "book", "roman", "italic",
        "oblique", "gt",
    ] {
        family = family.replace(word, "");
    }
    (family, mono, weight)
}

/// A name folded to what a settings file can hold: letters and digits only.
///
/// So that renaming a folder from "Lumen" to "XY Lumen" does not orphan
/// the setting that points at it.
fn folded(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// How good a candidate a file is for the regular face.
///
/// A file named "regular" beats one with no weight word, which beats "light"
/// and "medium" — all three are plausible, and the point is to be predictable
/// rather than clever.
fn regular_rank(stem: &str) -> u8 {
    let lower = stem.to_lowercase();
    if ["condensed", "narrow", "compressed", "extended", "expanded", "wide"].iter().any(|word| lower.contains(word)) {
        return 5;
    }
    if lower.contains("regular") {
        0
    } else if !["light", "medium", "book", "roman"]
        .iter()
        .any(|word| lower.contains(word))
    {
        1
    } else if lower.contains("book") || lower.contains("roman") {
        2
    } else if lower.contains("medium") {
        3
    } else {
        4
    }
}

/// Read the `fonts` folder into families.
pub fn families_in(dir: &std::path::Path) -> Vec<Family> {
    use std::collections::BTreeMap;

    /// regular candidate (rank, path), bold path
    type Cut = (Option<(u8, std::path::PathBuf)>, Option<std::path::PathBuf>);
    let mut found: BTreeMap<String, (String, Cut, Cut)> = BTreeMap::new();

    for entry in walkdir::WalkDir::new(dir)
        .max_depth(3)
        .into_iter()
        .filter_entry(|e| e.file_name() != BUNDLED_FALLBACK)
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
            .to_lowercase();
        if ext != "ttf" && ext != "otf" {
            continue;
        }
        if !is_single_face(path) {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let (key, mono, weight) = read_face_name(stem);
        if key.is_empty() || weight == Weight::Other {
            continue;
        }

        let shown = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .map(|n| n.to_lowercase())
            .filter(|n| n != "fonts")
            .unwrap_or_else(|| key.clone());
        let key = folded(&shown);

        let slot = found
            .entry(key.clone())
            .or_insert_with(|| (shown, (None, None), (None, None)));
        let cut = if mono { &mut slot.2 } else { &mut slot.1 };
        match weight {
            Weight::Bold => {
                if cut.1.is_none() {
                    cut.1 = Some(path.to_path_buf());
                }
            }
            _ => {
                let rank = regular_rank(stem);
                let better = match &cut.0 {
                    Some((current, _)) => rank < *current,
                    None => true,
                };
                if better {
                    cut.0 = Some((rank, path.to_path_buf()));
                }
            }
        }
    }

    found
        .into_iter()
        .filter_map(|(key, (name, prop, mono))| {
            let shape = |cut: Cut| -> Option<Faces> {
                cut.0.map(|(_, regular)| Faces {
                    regular,
                    bold: cut.1,
                })
            };
            let family = Family {
                name,
                key,
                proportional: shape(prop),
                mono: shape(mono),
            };
            (family.proportional.is_some() || family.mono.is_some()).then_some(family)
        })
        .collect()
}

/// Where this machine keeps its typefaces: the ones somebody installed
/// first, then the system's own.
fn system_font_dirs() -> Vec<std::path::PathBuf> {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    let mut dirs = Vec::new();
    if cfg!(target_os = "macos") {
        dirs.extend(home.map(|h| h.join("Library/Fonts")));
        dirs.push("/Library/Fonts".into());
        dirs.push("/System/Library/Fonts".into());
    } else if cfg!(target_os = "windows") {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(std::path::PathBuf::from(local).join("Microsoft/Windows/Fonts"));
        }
        dirs.push("C:/Windows/Fonts".into());
    } else {
        if let Some(h) = home {
            dirs.push(h.join(".local/share/fonts"));
            dirs.push(h.join(".fonts"));
        }
        dirs.push("/usr/local/share/fonts".into());
        dirs.push("/usr/share/fonts".into());
    }
    dirs
}

/// The typefaces installed on this machine, for trying out — hidden in the menu
/// behind ⌥. Nothing of them goes anywhere: they are read where they lie, as
/// the fallback face always was.
///
/// Read differently from the `fonts` folder, because a system folder says
/// nothing about what belongs together and its file names say little that
/// can be relied on. Each file is asked instead: its family by the name
/// table, its weight and slant by its own attributes. The upright face
/// nearest 400 is the regular, the one nearest 700 above it the bold.
/// Collections (.ttc, much of macOS's own) are passed over, as in the
/// `fonts` folder: one file, one face. So are faces without the Latin
/// letters.
pub fn system_families() -> Vec<Family> {
    scan_system(|_| true)
}

/// The machine's typefaces the menu offers without ⌥, beside the folder's,
/// once installed: Muto and Muto Mono. Their licence is for the machine
/// they are installed on, so they are read where they lie and never come
/// with kessler. Only files named for them are opened, so looking costs
/// next to nothing.
const SYSTEM_OFFERED: [&str; 2] = ["muto", "mutomono"];

fn system_offered() -> Vec<Family> {
    scan_system(|file| file.to_lowercase().contains("muto"))
        .into_iter()
        .filter(|f| SYSTEM_OFFERED.contains(&f.key.as_str()))
        .collect()
}

/// The system's typefaces in files `wanted` says yes to by name.
fn scan_system(wanted: impl Fn(&str) -> bool) -> Vec<Family> {
    use skrifa::MetadataProvider as _;
    use std::collections::BTreeMap;

    type Slot = (String, Option<(f32, std::path::PathBuf)>, Option<(f32, std::path::PathBuf)>);
    let mut found: BTreeMap<String, Slot> = BTreeMap::new();

    for dir in system_font_dirs() {
        for entry in walkdir::WalkDir::new(&dir)
            .max_depth(5)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or_default()
                .to_lowercase();
            if !path.is_file() || (ext != "ttf" && ext != "otf") || !is_single_face(path) {
                continue;
            }
            if !path.file_name().and_then(|n| n.to_str()).is_some_and(&wanted) {
                continue;
            }
            let Some(map) = mapped(path) else {
                continue;
            };
            let Ok(font) = skrifa::FontRef::new(&map) else {
                continue;
            };
            let named = |id| {
                font.localized_strings(id)
                    .english_or_first()
                    .map(|s| s.to_string())
                    .filter(|s| !s.trim().is_empty())
            };
            let Some(name) = named(skrifa::string::StringId::TYPOGRAPHIC_FAMILY_NAME)
                .or_else(|| named(skrifa::string::StringId::FAMILY_NAME))
            else {
                continue;
            };
            if name.starts_with('.') {
                continue;
            }
            let charmap = font.charmap();
            if charmap.is_symbol() || "aenrstAENRST".chars().any(|c| charmap.map(c).is_none()) {
                continue;
            }
            let pictures = charmap
                .map('a')
                .and_then(|glyph| font.glyph_names().get(glyph))
                .filter(|name| !name.is_synthesized())
                .is_some_and(|name| {
                    let name = name.as_str();
                    !name.is_empty() && name != "a" && name != "uni0061" && !name.starts_with("a.")
                });
            if pictures {
                continue;
            }
            let attributes = font.attributes();
            if attributes.style != skrifa::attribute::Style::Normal {
                continue;
            }
            let weight = attributes.weight.value();
            let width_off = (attributes.stretch.ratio() - 1.0).abs() * 1000.0;
            let key = folded(&name);
            if key.is_empty() {
                continue;
            }
            let slot = found
                .entry(key)
                .or_insert_with(|| (name.to_lowercase(), None, None));
            let closer = |current: &Option<(f32, std::path::PathBuf)>, distance: f32| {
                current.as_ref().is_none_or(|(d, _)| distance < *d)
            };
            if weight >= 600.0 {
                let distance = (weight - 700.0).abs() + width_off;
                if closer(&slot.2, distance) {
                    slot.2 = Some((distance, path.to_path_buf()));
                }
            } else {
                let distance = (weight - 400.0).abs() + width_off;
                if closer(&slot.1, distance) {
                    slot.1 = Some((distance, path.to_path_buf()));
                }
            }
        }
    }

    found
        .into_iter()
        .filter_map(|(key, (name, regular, bold))| {
            let (_, regular) = regular?;
            Some(Family {
                name,
                key,
                proportional: Some(Faces {
                    regular,
                    bold: bold.map(|(_, path)| path),
                }),
                mono: None,
            })
        })
        .collect()
}

/// The families the menu steps through: the `fonts` folder's, and with
/// `system` the machine's after them, less any the folder already brings.
pub fn all_families(folder: Option<&str>, system: bool) -> Vec<Family> {
    let mut families = offered(
        font_folder(folder)
            .map(|dir| families_in(&dir))
            .unwrap_or_default(),
    );
    let mut more: Vec<Family> = if system { system_families() } else { system_offered() }
        .into_iter()
        .filter(|f| families.iter().all(|own| own.key != f.key))
        .collect();
    families.append(&mut more);
    if let Some(system_ui) = system_ui_family() {
        if families.iter().all(|f| f.key != system_ui.key) {
            families.push(system_ui);
        }
    }
    let mono = families.iter().find(|f| f.key == "mutomono").and_then(|f| f.proportional.clone());
    if let Some(muto) = families.iter_mut().find(|f| f.key == "muto" && f.mono.is_none()) {
        muto.mono = mono;
    }
    if families.iter().all(|f| f.key != "mutomono") {
        if let Some(mono) = families.iter().find(|f| f.key == "muto").and_then(|f| f.mono.clone()) {
            let at = families.iter().position(|f| f.key == "muto").map_or(families.len(), |i| i + 1);
            families.insert(at, Family { name: "muto mono".into(), key: "mutomono".into(), proportional: None, mono: Some(mono) });
        }
    }
    families
}

/// What the menu calls the system's own interface face, and the config.
pub const SYSTEM_UI: &str = "systemschrift";

/// The face the system sets its own interface in, read where it lies and never
/// carried along: on a Mac San Francisco; on Linux the one GNOME is set to
/// (`org.gnome.desktop.interface font-name` — Ubuntu Sans on Ubuntu,
/// Cantarell or Adwaita Sans elsewhere), else fontconfig's sans. Its bold from
/// fontconfig too, where it is a file of its own; a face with a weight axis is
/// its own bold. Nothing where none is to be had as a single-face file.
fn system_ui_family() -> Option<Family> {
    let usable = |path: &std::path::Path| {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or_default().to_lowercase();
        path.is_file() && (ext == "ttf" || ext == "otf") && is_single_face(path)
    };
    let (regular, bold) = if cfg!(target_os = "macos") {
        let found = ["/System/Library/Fonts/SFNS.ttf", "/System/Library/Fonts/SFNSText.ttf", "/Library/Fonts/SF-Pro.ttf"]
            .iter()
            .map(std::path::PathBuf::from)
            .find(|p| usable(p))?;
        (found, None)
    } else {
        let gnome = std::process::Command::new("gsettings")
            .args(["get", "org.gnome.desktop.interface", "font-name"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().trim_matches('\'').to_string())
            .map(|name| {
                let mut words: Vec<&str> = name.split_whitespace().collect();
                if words.last().is_some_and(|w| w.parse::<f32>().is_ok()) {
                    words.pop();
                }
                words.join(" ")
            })
            .filter(|name| !name.is_empty());
        let asked = gnome.unwrap_or_else(|| "sans-serif".to_string());
        let matched = |pattern: &str| {
            std::process::Command::new("fc-match")
                .args(["-f", "%{file}", pattern])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| std::path::PathBuf::from(String::from_utf8_lossy(&o.stdout).trim()))
                .filter(|p| usable(p))
        };
        let regular = matched(&asked)?;
        let bold = matched(&format!("{asked}:bold")).filter(|b| *b != regular);
        (regular, bold)
    };
    Some(Family {
        name: SYSTEM_UI.into(),
        key: SYSTEM_UI.into(),
        proportional: Some(Faces { regular, bold }),
        mono: None,
    })
}

/// a config naming it gets Asap.
const FAMILIES_OFFERED: [&str; 2] = [TYPEFACE, "muto"];

/// Of the families in a folder, those the menu offers — or, where a folder
/// holds none of them, whatever it does hold, because a folder somebody
/// filled with one typeface plainly means that one.
fn offered(families: Vec<Family>) -> Vec<Family> {
    if families.iter().any(|f| FAMILIES_OFFERED.contains(&f.key.as_str())) {
        families.into_iter().filter(|f| FAMILIES_OFFERED.contains(&f.key.as_str())).collect()
    } else {
        families
    }
}

/// The typeface the interface is set in unless another is chosen.
///
/// Asap: its weights are all one width, so a chosen name thickens rather than
/// jumps — see `weights` — and it has a width axis, set in the menu — see
/// `width_axis`. It comes with the repository, so every copy opens in it. The
/// menu offers the other families in the `fonts` folder beside it — see
/// [`FAMILIES_OFFERED`] — and the rest of the machine's only when asked for,
/// behind ⌥ — see [`system_families`].
pub const TYPEFACE: &str = "asap";

/// The face the window opens in where none was ever chosen and it is there:
/// Muto. Only builds with its folder in `fonts` carry it; a build without it
/// opens in [`TYPEFACE`].
pub const PREFERRED: &str = "muto";

/// Its three faces: regular, bold, and the mono cut that sets the figures.
///
/// Looked for by name in the fonts folder; failing that, whatever single
/// family the folder does hold, because a build carrying one typeface plainly
/// means that one. With no folder at all there is nothing to install and
/// egui's own face draws the window — which looks wrong, and is meant to: the
/// faces belong in `fonts/`.
///
/// `figures` falls back to the proportional cut where a family has no mono
/// one, which is the right answer rather than a missing one: figures then look
/// like everything else, as they did before there was a mono cut to reach for.
///
/// `wanted` is the key the config names; empty, or naming a family that is
/// not in the folder, means [`TYPEFACE`].
pub fn the_typeface(families: &[Family], wanted: &str) -> Option<Chosen> {
    let family = families
        .iter()
        .find(|f| !wanted.is_empty() && f.key == wanted)
        .or_else(|| families.iter().find(|f| f.key == TYPEFACE))
        .or_else(|| families.first())?;
    let faces = family.faces(false)?;
    Some(Chosen {
        name: family.name.clone(),
        key: family.key.clone(),
        regular: faces.regular.display().to_string(),
        bold: faces.bold.as_ref().map(|b| b.display().to_string()),
        figures: family
            .faces(true)
            .map(|faces| faces.regular.display().to_string()),
    })
}

/// How much smaller a family is set than the sizes the window is measured
/// in — fifteen for the lists, eleven for the labels.
///
/// One factor for all of it, so the two sizes keep their proportion and nothing
/// is a third size.
pub fn face_scale(key: &str) -> f32 {
    match key {
        "muto" | "mutomono" => 14.0 / 15.0,
        _ => 1.0,
    }
}

/// What [`the_typeface`] found, as paths ready to hand to [`install_fonts`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chosen {
    /// What the menu calls it.
    pub name: String,
    /// What the config stores — [`Family::key`].
    pub key: String,
    pub regular: String,
    pub bold: Option<String>,
    pub figures: Option<String>,
}

/// A face read off disk: where it came from, and its bytes.
type LoadedFace = (String, Vec<u8>);
/// A regular face, and a bold one if the machine had a matching file.
type LoadedPair = (String, Vec<u8>, Option<LoadedFace>);

/// What actually got loaded, so the app can say so rather than pretend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FontSource {
    /// A regular/bold pair from these two files.
    Pair(String, String),
    /// Only a regular face was found; the weight axis is unavailable.
    RegularOnly(String),
    /// egui's built-in face. One weight, so selection falls back to tone.
    BuiltIn,
}

impl FontSource {
    /// Whether a real bold face is available. Without one, the weight axis
    /// silently does nothing, and the interface has to say so.
    pub fn has_bold(&self) -> bool {
        matches!(self, Self::Pair(_, _))
    }

    /// What was actually loaded, in words. Only the tests read it today — the
    /// menu shows the face's own name rather than this sentence.
    #[cfg(test)]
    pub fn describe(&self) -> String {
        match self {
            Self::Pair(r, _) => format!("{r} (+ bold)"),
            Self::RegularOnly(r) => format!("{r} (no bold face found)"),
            Self::BuiltIn => "egui built-in (no bold face found)".to_string(),
        }
    }
}

/// Install the two faces, preferring an explicitly configured pair.
///
/// `regular`/`bold` come from the config, so a bought typeface is a path away
/// and never needs to be bundled.
/// Faces worth falling back to when the chosen typeface has no glyph, best
/// first.
///
/// Picked for coverage, not for looks. A Latin typeface — which is every
/// typeface anyone chooses this program for — has nothing for 坂本龍一, and
/// what the column shows instead is a row of empty boxes: not a wrong name, no
/// name at all. Any legible face beats that, and CJK is the coverage actually
/// missing in a music library.
const FALLBACK_FILES: &[&str] = &[
    "arial unicode.ttf",
    "hiragino sans gb w3.otf",
    "hiraginosans-w3.ttc",
    "ヒラギノ角ゴシック w3.ttc",
    "pingfang.ttc",
    "applesdgothicneo.ttc",
    "applegothic.ttf",
    "notosanscjk-regular.ttc",
    "notosanscjkjp-regular.otf",
    "droidsansfallbackfull.ttf",
    "fonts-japanese-gothic.ttf",
    "wqy-zenhei.ttc",
    "unifont.ttf",
    "yugothr.ttc",
    "meiryo.ttc",
    "msgothic.ttc",
    "malgun.ttf",
    "simsun.ttc",
];

/// The folder of the face kessler brings for Japanese, in `fonts/` beside the
/// others, free like them (SIL OFL). The machine's own fallback, Hiragino on a Mac, is a different voice from one Mac to the next
/// and from the window's; this one travels with the program.
///
/// M PLUS 1 Code: one variable file
/// whose weights are all one width, so a Japanese name thickens with the rest
/// of its row — the weight a row asks of its face reaches the fallback too —
/// and does not move.
pub const BUNDLED_FALLBACK: &str = "M PLUS 1 Code";

/// The bundled Japanese face, regular and bold, if it is there. One variable
/// file, so no bold of its own: [`append_fallback`] sets it further along
/// its weight axis instead.
pub fn bundled_fallback() -> Option<(std::path::PathBuf, Option<std::path::PathBuf>)> {
    let regular = font_folder(None)?.join(BUNDLED_FALLBACK).join("MPLUS1Code-Variable.ttf");
    regular.is_file().then_some((regular, None))
}

/// The first of those this machine actually has.
///
/// The candidates are tried in their own order rather than in the order the
/// folders happen to be walked, so the choice does not depend on which folder
/// a machine keeps its fonts in.
pub fn fallback_face() -> Option<std::path::PathBuf> {
    use std::collections::HashMap;
    let mut found: HashMap<String, std::path::PathBuf> = HashMap::new();
    for dir in font_dirs() {
        for entry in walkdir::WalkDir::new(&dir)
            .max_depth(3)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            found.entry(name.to_lowercase()).or_insert_with(|| path.to_path_buf());
        }
    }
    FALLBACK_FILES
        .iter()
        .find_map(|wanted| found.get(*wanted).cloned())
}

/// A readable name for a font file: its file name, without the extension.
fn face_name(path: &std::path::Path) -> String {
    path.file_stem()
        .and_then(|n| n.to_str())
        .unwrap_or(tr!("unbekannt"))
        .to_lowercase()
}

/// Put a face at the end of both families, where egui reaches for it only when
/// the faces before it have nothing for a character.
fn append_fallback(
    fonts: &mut egui::FontDefinitions,
    path: &std::path::Path,
    bold: Option<&std::path::Path>,
    firsts: &[(FontFamily, Option<EmMetrics>)],
    tag: &str,
) -> Option<String> {
    let regular = fallback_bytes(path)?;
    let bold = bold.and_then(fallback_bytes);
    let own_bold = match bold {
        None => variable_weights(regular).map(|_| weights().map_or(700.0, |(_, heavy)| heavy)),
        Some(_) => None,
    };
    let own_regular = variable_weights(regular).and_then(|_| {
        weights().map(|(light, _)| light).or_else(|| FALLBACK_REGULAR.read().ok().and_then(|held| *held))
    });
    for (family, first) in firsts {
        let is_bold = matches!(family, FontFamily::Name(name) if &**name == "bold");
        let data = match bold {
            Some(bold) if is_bold => bold,
            _ => regular,
        };
        let coords = match (own_bold, own_regular) {
            (Some(weight), _) if is_bold => egui::epaint::text::VariationCoords::new([(b"wght", weight)]),
            (_, Some(weight)) if !is_bold => egui::epaint::text::VariationCoords::new([(b"wght", weight)]),
            _ => Default::default(),
        };
        let scale = fallback_scale(*first, data);
        let own = em_metrics(data, scale);
        let key = match family {
            FontFamily::Name(name) => format!("fallback{tag}-{name}"),
            _ => format!("fallback{tag}"),
        };
        let nudge = match (first, own) {
            (Some(first), Some(own)) => baseline_nudge(*first, own),
            _ => 0.0,
        } - kanji_raise(data, scale);
        fonts.font_data.insert(
            key.clone(),
            std::sync::Arc::new(egui::FontData::from_static(data).tweak(hinting().on(egui::FontTweak {
                scale,
                y_offset_factor: nudge / scale,
                coords,
                ..Default::default()
            }))),
        );
        if let Some(list) = fonts.families.get_mut(family) {
            list.push(key);
        }
    }
    Some(face_name(path))
}

/// A face's ascent and the height of its row, as shares of its size — what
/// egui places its glyphs by.
#[derive(Clone, Copy, Debug)]
pub struct EmMetrics {
    ascent: f32,
    row: f32,
    /// How tall an H stands, where the face has one — what a fallback's
    /// characters are sized against.
    cap: Option<f32>,
}

/// Read [`EmMetrics`] out of a font file (the first face of a collection),
/// at `scale` — the tweak the face is installed with.
fn em_metrics(data: &[u8], scale: f32) -> Option<EmMetrics> {
    use skrifa::MetadataProvider as _;
    let font = skrifa::FontRef::from_index(data, 0).ok()?;
    let m = font.metrics(skrifa::instance::Size::unscaled(), skrifa::instance::LocationRef::default());
    let em = m.units_per_em as f32;
    if em <= 0.0 {
        return None;
    }
    Some(EmMetrics {
        ascent: m.ascent / em * scale,
        row: (m.ascent - m.descent + m.leading) / em * scale,
        cap: ink_height(data, 'H').map(|h| h * scale),
    })
}

/// How tall one character's ink stands, as a share of the size.
fn ink_height(data: &[u8], c: char) -> Option<f32> {
    use skrifa::MetadataProvider as _;
    let font = skrifa::FontRef::from_index(data, 0).ok()?;
    let glyph = font.charmap().map(c)?;
    let em = font
        .metrics(skrifa::instance::Size::unscaled(), skrifa::instance::LocationRef::default())
        .units_per_em as f32;
    let bounds = font
        .glyph_metrics(skrifa::instance::Size::unscaled(), skrifa::instance::LocationRef::default())
        .bounds(glyph)?;
    let height = bounds.y_max - bounds.y_min;
    (em > 0.0 && height > 0.0).then_some(height / em)
}

/// How much taller a kanji stands than the face's capitals, when the two
/// look the same height. A kanji fills its square edge to edge and a capital
/// does not, so set to exactly the capital's height the kanji reads small.
const KANJI_OVER_CAPS: f32 = 1.12;

/// The size a fallback is set at against the face it stands behind: its kanji
/// brought to the face's capitals, so a Japanese title no longer stands taller
/// than a Latin one beside it. Never larger than it was, and never so small it
/// stops reading; one where either face says nothing.
fn fallback_scale(first: Option<EmMetrics>, data: &[u8]) -> f32 {
    match (first.and_then(|f| f.cap), ink_height(data, '永')) {
        (Some(cap), Some(kanji)) => (cap * KANJI_OVER_CAPS / kanji).clamp(0.7, 1.0),
        _ => 1.0,
    }
}

/// How far down to move the fallback's glyphs, as a share of the size, so they
/// stand on the line the family's own face stands on.
///
/// egui sets a glyph from another face by the middle of the two rows rather
/// than by the baseline: its baseline lands at its own ascent plus half the
/// difference in row height. Two faces with different vertical metrics then
/// part company — on a Mac the Japanese in sank below the Latin in the regular
/// cut and sat right in the bold. This is the distance between where egui puts
/// the baseline and where the family's face has it.
fn baseline_nudge(first: EmMetrics, own: EmMetrics) -> f32 {
    first.ascent - own.ascent - 0.5 * (first.row - own.row)
}

/// How far up to lift a fallback's kanji, as a share of the size, so they
/// stand on the line rather than below it.
///
/// A Japanese face draws its kanji in a square that reaches under the Latin
/// baseline — about a fifteenth of the size in M PLUS — and set on the same
/// baseline as the Latin beside it, a kanji's foot hung a point under the
/// letters'. Lifted until it overshoots the line by no more than a round
/// letter does; its head then stands a little above the capitals, as an
/// ascender does. Nothing for a face without kanji, or with kanji on the
/// line already.
fn kanji_raise(data: &[u8], scale: f32) -> f32 {
    use skrifa::MetadataProvider as _;
    let Ok(font) = skrifa::FontRef::from_index(data, 0) else { return 0.0 };
    let Some(glyph) = font.charmap().map('永') else { return 0.0 };
    let unscaled = skrifa::instance::Size::unscaled();
    let location = skrifa::instance::LocationRef::default();
    let em = font.metrics(unscaled, location).units_per_em as f32;
    let Some(bounds) = font.glyph_metrics(unscaled, location).bounds(glyph) else { return 0.0 };
    if em <= 0.0 {
        return 0.0;
    }
    let below = -bounds.y_min / em * scale;
    (below - KANJI_OVERSHOOT).max(0.0)
}

/// How far a kanji's foot may still reach under the line, as a share of the
/// size: what a round letter's bowl does.
const KANJI_OVERSHOOT: f32 = 0.012;

/// The fallback's bytes, mapped once per file and kept for the life of the
/// program: it is installed three times over (a nudge for each family), it
/// is often a collection of tens of megabytes, and it is installed again
/// every time the typeface changes.
///
/// Mapped, not read: read, the two faces kessler brings and the machine's own
/// (on a Mac „arial unicode", 23 mb) sat whole in the program's memory, some 30
/// mb of the little it has, for the few hundred glyphs a window of names ever
/// asks for. Mapped, only the pages a glyph is read from come in, and the
/// system can let them go again whenever it likes, as they are the file's.
/// Should the mapping fail, the file is read as before.
fn fallback_bytes(path: &std::path::Path) -> Option<&'static [u8]> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static READ: OnceLock<Mutex<HashMap<std::path::PathBuf, &'static [u8]>>> = OnceLock::new();
    let mut read = READ.get_or_init(Default::default).lock().ok()?;
    if let Some(data) = read.get(path) {
        return Some(data);
    }
    let data: &'static [u8] = match mapped(path) {
        Some(map) => &Box::leak(Box::new(map))[..],
        None => Box::leak(std::fs::read(path).ok()?.into_boxed_slice()),
    };
    read.insert(path.to_path_buf(), data);
    Some(data)
}

/// A font file mapped into memory, read-only.
fn mapped(path: &std::path::Path) -> Option<memmap2::Mmap> {
    let file = std::fs::File::open(path).ok()?;
    unsafe { memmap2::Mmap::map(&file) }.ok()
}

/// The regular and the bold weight of a face with a weight axis, or none for a
/// face without one: the regular where the menu set it — 400 unless moved — and
/// the bold [`BOLD_STEP`] above it, both kept inside the range the axis has.
fn variable_weights(data: &[u8]) -> Option<(f32, f32)> {
    let light = ON_LIGHT.read().is_ok_and(|held| *held);
    let extra = if light { LIGHT_GROUND_EXTRA } else { 0.0 };
    weights_at(data, WEIGHT.read().map_or(400.0, |w| *w) + extra)
}

/// How much heavier the type is set on a light ground than the weight chosen:
/// light words on a dark ground spread a little into it, dark ones on a light
/// ground thin, so the same weight read lighter there. The weight was set
/// looking at dark schemes. Asap is as wide at every weight, so nothing moves
/// when the ground changes.
pub const LIGHT_GROUND_EXTRA: f32 = 80.0;

/// Whether the ground is light, for the next [`install_fonts`].
static ON_LIGHT: std::sync::RwLock<bool> = std::sync::RwLock::new(false);

/// Say whether the ground the type is set on is light; true if that changed.
pub fn set_light_ground(light: bool) -> bool {
    match ON_LIGHT.write() {
        Ok(mut held) if *held != light => {
            *held = light;
            true
        }
        _ => false,
    }
}

/// The same for the regular `wanted`.
fn weights_at(data: &[u8], wanted: f32) -> Option<(f32, f32)> {
    let (least, most) = axis_range(data, b"wght")?;
    let regular = wanted.clamp(least, (most - BOLD_STEP).max(least));
    Some((regular, (regular + BOLD_STEP).clamp(least, most)))
}

/// How much heavier the bold is than the regular, wherever the regular is set:
/// a chosen name stands out by the same step at every weight: 400 to 550.
/// Two hundred reads a touch heavy in Asap.
const BOLD_STEP: f32 = 150.0;

/// The weight the regular is set at, on a face with a weight axis.
static WEIGHT: std::sync::RwLock<f32> = std::sync::RwLock::new(400.0);

/// The weight axis of the face in use, while it has one. The menu's slider
/// stops short of its top by the bold's step, so the bold has somewhere to
/// go — see [`weight_axis`].
static WEIGHT_AXIS: std::sync::RwLock<Option<(f32, f32)>> = std::sync::RwLock::new(None);

/// Set the weight the next [`install_fonts`] sets the regular at.
fn set_font_weight(weight: f32) {
    if let Ok(mut held) = WEIGHT.write() {
        *held = weight;
    }
}

/// Everything the config says about how the face is set — its width, its
/// weight, its alternates — for the next [`install_fonts`].
pub fn set_face(ui: &crate::config::UiConfig) {
    set_font_width(ui.font_width);
    set_font_weight(ui.font_weight);
    set_alternates(Alternates { sets: ui.sets_of(face_key(&ui.typeface)) });
    set_fallback_regular(fallback_regular(face_key(&ui.typeface)));
    LABEL_TRACKING_SHARE.store(label_tracking_share(face_key(&ui.typeface)).to_bits(), std::sync::atomic::Ordering::Relaxed);
    LABEL_MEDIUM.store(matches!(face_key(&ui.typeface), "muto" | "mutomono"), std::sync::atomic::Ordering::Relaxed);
    HEAD_LINE_SHARE.store(head_line_share(face_key(&ui.typeface)).to_bits(), std::sync::atomic::Ordering::Relaxed);
    let hinted = Hinting::from_name(&ui.font_hinting);
    HINTING.store(Hinting::ALL.iter().position(|h| *h == hinted).unwrap_or(0) as u8, std::sync::atomic::Ordering::Relaxed);
    SQUARE_ENDS.store(square_ends_for(face_key(&ui.typeface)), std::sync::atomic::Ordering::Relaxed);
    if let Ok(mut held) = LABEL_SETS.write() {
        *held = label_sets_for(face_key(&ui.typeface));
    }
    if let Ok(mut held) = LABEL_FORMS.write() {
        held.clear();
    }
}

/// Whether the rules (the timeline, the volume), the mark beside a scrolled
/// list and the knobs are cut square rather than round, for a face whose own
/// ends are square.
fn square_ends_for(key: &str) -> bool {
    matches!(key, "muto" | "mutomono")
}

static SQUARE_ENDS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// See [`square_ends_for`].
pub fn square_ends() -> bool {
    SQUARE_ENDS.load(std::sync::atomic::Ordering::Relaxed)
}

/// How the letters are fitted to the pixels — `[ui] font_hinting`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hinting {
    /// egui's own: stems snapped to whole pixels across and down.
    Full,
    /// Only up and down: heights snap, the shapes across are left alone.
    Light,
    /// None: the outline as the face draws it, softer at small sizes.
    Off,
}

impl Hinting {
    pub const ALL: [Hinting; 3] = [Self::Full, Self::Light, Self::Off];

    pub fn name(self) -> &'static str {
        match self {
            Self::Full => "voll",
            Self::Light => "leicht",
            Self::Off => "aus",
        }
    }

    pub fn from_name(name: &str) -> Self {
        Self::ALL.into_iter().find(|h| h.name() == name.trim()).unwrap_or(Self::Full)
    }

    /// `tweak` hinted this way.
    fn on(self, tweak: egui::FontTweak) -> egui::FontTweak {
        match self {
            Self::Full => tweak,
            Self::Light => egui::FontTweak {
                hinting: Some(true),
                hinting_target: egui::epaint::text::HintingTarget::Smooth(egui::epaint::text::SmoothHinting {
                    light: true,
                    ..Default::default()
                }),
                ..tweak
            },
            Self::Off => egui::FontTweak { hinting: Some(false), ..tweak },
        }
    }
}

static HINTING: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

fn hinting() -> Hinting {
    Hinting::ALL[usize::from(HINTING.load(std::sync::atomic::Ordering::Relaxed)).min(2)]
}

/// Where the Japanese behind a face without a weight axis is set, so its
/// strokes are as heavy as the face's own: Muto's regular stem is about a fifth
/// of its x-height, M PLUS 1 Code's at 400 not quite three twentieths of its
/// own; at 520 they match (measured on „l" and „一"). `None`: the fallback's own
/// regular.
fn fallback_regular(key: &str) -> Option<f32> {
    match key {
        "muto" | "mutomono" => Some(520.0),
        _ => None,
    }
}

static FALLBACK_REGULAR: std::sync::RwLock<Option<f32>> = std::sync::RwLock::new(None);

fn set_fallback_regular(weight: Option<f32>) {
    if let Ok(mut held) = FALLBACK_REGULAR.write() {
        *held = weight;
    }
}

/// Where the regular can be set on the face in use, if it has a weight axis.
pub fn weight_axis() -> Option<(f32, f32)> {
    weight_range().map(|(least, most)| (least, (most - BOLD_STEP).max(least)))
}

/// The whole of the weight axis of the face in use, lightest to heaviest.
pub fn weight_range() -> Option<(f32, f32)> {
    WEIGHT_AXIS.read().ok().and_then(|held| *held)
}

/// The alternate forms a face keeps behind a feature, drawn in place of the
/// usual ones: the tags of the features asked for — stylistic sets (`ss01` …),
/// `salt`, `zero`. Since then any of a face's sets, chosen for each face (`[ui]
/// font_sets`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Alternates {
    pub sets: Vec<String>,
}

/// What the next [`install_fonts`] is to draw.
static ALTERNATES: std::sync::RwLock<Alternates> = std::sync::RwLock::new(Alternates { sets: Vec::new() });

/// Set what the next [`install_fonts`] draws.
fn set_alternates(alternates: Alternates) {
    if let Ok(mut held) = ALTERNATES.write() {
        *held = alternates;
    }
}

/// Glyph for glyph, the one drawn and the one drawn in its place.
pub(super) type Swaps = Vec<(u32, u32)>;

/// How many plain letters a set may redraw and still be offered a letter at
/// a time; more, and it is one row, „ss06:*".
const WHOLE_SET_LETTERS: usize = 8;

/// One letter a face can draw another way, and the feature that does it:
/// `id` is the feature's tag and the letter („ss01:a"), what the config
/// keeps; the glyphs it swaps (the letter with its accents); the characters
/// that change. A set that changes several letters — Muto's `ss04` A, M, N,
/// V, W, v, w — is one of these for each, so each can be chosen alone, and
/// Asap's `ss01`, the g and the Y, can keep the g alone as it always has.
/// What has no plain letter or figure under it (arrows, ©) is one more.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleSet {
    pub id: String,
    pub tag: String,
    /// The plain letter or figure, `None` for the rest of the set.
    pub letter: Option<char>,
    pub swaps: Swaps,
    pub shows: Vec<char>,
}

/// The other alternates of the character `tag` redraws: one of them on means
/// the others off. None for a set that redraws no one character.
pub fn rivals_of(offered: &[StyleSet], tag: &str) -> Vec<String> {
    let Some(letter) = offered.iter().find(|s| s.id == tag).and_then(|s| s.letter) else {
        return Vec::new();
    };
    offered.iter().filter(|s| s.id != tag && s.letter == Some(letter)).map(|s| s.id.clone()).collect()
}

/// The key a face's sets are kept under: the config's typeface, Asap where
/// it names none.
pub fn face_key(typeface: &str) -> &str {
    if typeface.is_empty() { TYPEFACE } else { typeface }
}

/// The alternates the labels are always set with, whatever the rest is set with
/// — for Muto its open a. Each letter of the set is given a character of the
/// private use area pointing at its alternate, and a label's letters are
/// swapped for those before it is set ([`styled`]): egui has one face for every
/// role.
fn label_sets_for(key: &str) -> Vec<String> {
    match key {
        "muto" | "mutomono" => vec!["ss01:a".into()],
        _ => Vec::new(),
    }
}

/// What the next [`install_fonts`] gives the labels — see [`label_sets_for`].
static LABEL_SETS: std::sync::RwLock<Vec<String>> = std::sync::RwLock::new(Vec::new());

/// The letters a label is set with otherwise, and the characters they are
/// swapped for — filled in as the face is installed.
static LABEL_FORMS: std::sync::RwLock<LabelForms> = std::sync::RwLock::new(Vec::new());

/// Each label letter with the private-use character standing for it.
type LabelForms = Vec<(char, char)>;

/// Where the labels' own letters start in the private use area.
const LABEL_FORMS_AT: u32 = 0xF700;

/// `text` as `role` sets it: a label with the letters it always has (see
/// [`label_sets_for`]).
pub fn styled(text: &str, role: Role) -> std::borrow::Cow<'_, str> {
    let text = std::borrow::Cow::Borrowed(text);
    if !matches!(role, Role::Label | Role::LabelBold) {
        return text;
    }
    let Ok(forms) = LABEL_FORMS.read() else { return text };
    if forms.is_empty() || !text.chars().any(|c| forms.iter().any(|(from, _)| *from == c)) {
        return text;
    }
    std::borrow::Cow::Owned(text.chars().map(|c| forms.iter().find(|(from, _)| *from == c).map_or(c, |(_, to)| *to)).collect())
}

/// A face's stylistic sets (`ss01` …), `salt` and `zero`, as glyph swaps.
///
/// Only swaps to an alternate named after its glyph („g" → „g.ss01",
/// „gbreve" → „gbreve.ss01") are taken — which is how a face says it is the
/// same letter drawn another way — and only sets that change a character
/// typed directly: one that only redraws another alternate (Muto's slashed
/// zero) has nothing to show on its own.
pub fn style_sets(data: &[u8]) -> Vec<StyleSet> {
    use skrifa::raw::tables::gsub::{SingleSubst, SubstitutionSubtables};
    use skrifa::raw::TableProvider as _;
    use skrifa::MetadataProvider as _;
    let mut sets: Vec<StyleSet> = Vec::new();
    let Ok(font) = skrifa::raw::FontRef::new(data) else {
        return sets;
    };
    let Ok(face) = skrifa::FontRef::new(data) else {
        return sets;
    };
    let names = skrifa::GlyphNames::new(&face);
    let (Ok(gsub), charmap) = (font.gsub(), face.charmap()) else {
        return sets;
    };
    let (Ok(features), Ok(lookups)) = (gsub.feature_list(), gsub.lookup_list()) else {
        return sets;
    };
    let typed: std::collections::HashMap<u32, Vec<char>> = charmap.mappings().fold(Default::default(), |mut map, (code, glyph)| {
        if let Some(c) = char::from_u32(code) {
            map.entry(glyph.to_u32()).or_default().push(c);
        }
        map
    });
    let name = |glyph: u32| names.get(skrifa::GlyphId::new(glyph)).filter(|n| !n.is_synthesized()).map(|n| n.as_str().to_string());
    let mut found: Vec<(String, Vec<(u32, u32)>)> = Vec::new();
    for record in features.feature_records() {
        let bytes = record.feature_tag().to_be_bytes();
        let wanted = (bytes.starts_with(b"ss") && bytes[2].is_ascii_digit()) || &bytes == b"salt" || &bytes == b"zero";
        if !wanted {
            continue;
        }
        let tag = String::from_utf8_lossy(&bytes).to_string();
        if found.iter().any(|(t, _)| *t == tag) {
            continue;
        }
        let Ok(feature) = record.feature(features.offset_data()) else {
            continue;
        };
        let mut pairs: Vec<(u32, u32)> = Vec::new();
        for index in feature.lookup_list_indices() {
            let Ok(lookup) = lookups.lookups().get(index.get() as usize) else {
                continue;
            };
            let Ok(SubstitutionSubtables::Single(subtables)) = lookup.subtables() else {
                continue;
            };
            for subtable in subtables.iter().flatten() {
                match subtable {
                    SingleSubst::Format1(table) => {
                        if let Ok(coverage) = table.coverage() {
                            let delta = table.delta_glyph_id() as i32;
                            pairs.extend(coverage.iter().map(|from| {
                                let from = from.to_u32();
                                (from, (from as i32 + delta).rem_euclid(65536) as u32)
                            }));
                        }
                    }
                    SingleSubst::Format2(table) => {
                        if let Ok(coverage) = table.coverage() {
                            pairs.extend(
                                coverage
                                    .iter()
                                    .zip(table.substitute_glyph_ids())
                                    .map(|(from, to)| (from.to_u32(), to.get().to_u32())),
                            );
                        }
                    }
                }
            }
        }
        found.push((tag, pairs));
    }
    let via: std::collections::HashMap<u32, u32> = found
        .iter()
        .flat_map(|(_, pairs)| pairs.iter())
        .filter(|(from, _)| typed.contains_key(from))
        .map(|&(from, to)| (to, from))
        .collect();
    for (tag, pairs) in found {
        let swaps: Swaps = pairs
            .into_iter()
            .filter(|&(from, to)| match (name(from), name(to)) {
                (Some(from), Some(to)) => to.starts_with(&format!("{from}.")),
                _ => false,
            })
            .collect();
        let mut by_letter: std::collections::BTreeMap<Option<char>, (Swaps, Vec<char>)> = Default::default();
        for (from, to) in swaps {
            let from = if typed.contains_key(&from) { from } else { via.get(&from).copied().unwrap_or(from) };
            let Some(chars) = typed.get(&from) else {
                continue;
            };
            let letter = chars.iter().find_map(|&c| {
                use unicode_normalization::UnicodeNormalization as _;
                std::iter::once(c).nfd().next().filter(|b| b.is_ascii_alphanumeric())
            });
            let entry = by_letter.entry(letter).or_default();
            entry.0.push((from, to));
            entry.1.extend(chars.iter().copied());
        }
        let letters = by_letter.keys().filter(|letter| letter.is_some()).count();
        if letters > WHOLE_SET_LETTERS {
            let (swaps, mut shows): (Swaps, Vec<char>) = by_letter.into_values().fold((Vec::new(), Vec::new()), |(mut swaps, mut shows), (s, c)| {
                swaps.extend(s);
                shows.extend(c);
                (swaps, shows)
            });
            shows.sort_unstable();
            shows.dedup();
            sets.push(StyleSet { id: format!("{tag}:*"), tag: tag.clone(), letter: None, swaps, shows });
            continue;
        }
        if by_letter.len() == 2 && by_letter.contains_key(&None) {
            if let Some((rest_swaps, rest_shows)) = by_letter.remove(&None) {
                if let Some((_, (swaps, shows))) = by_letter.iter_mut().next() {
                    swaps.extend(rest_swaps);
                    shows.extend(rest_shows);
                }
            }
        }
        for (letter, (swaps, mut shows)) in by_letter {
            shows.sort_unstable();
            shows.dedup();
            let id = match letter {
                Some(c) => format!("{tag}:{c}"),
                None => format!("{tag}:"),
            };
            sets.push(StyleSet { id, tag: tag.clone(), letter, swaps, shows });
        }
    }
    let rank = |set: &StyleSet| match set.letter {
        Some(c) if c.is_ascii_lowercase() => 0,
        Some(c) if c.is_ascii_uppercase() => 1,
        Some(_) => 2,
        None => 3,
    };
    sets.sort_by(|a, b| {
        (rank(a), a.letter.map(|c| c.to_ascii_lowercase()), &a.tag).cmp(&(rank(b), b.letter.map(|c| c.to_ascii_lowercase()), &b.tag))
    });
    sets
}

/// The face with its character map pointing at the alternates asked for, or
/// `None` where there is nothing to change. Egui shapes text with the face's
/// default features but no way to ask for a stylistic set, so the only way to
/// an alternate is to make it the glyph the character stands for — as if the
/// face had been drawn that way.
///
/// `label_sets` are the sets the labels always have (see
/// [`label_sets_for`]); their letters come back as the characters they were
/// given in the private use area. Both are handed in and out rather than read
/// from and written to the statics, so the face can be built without touching
/// what the window is drawn with.
fn with_alternates(data: &[u8], wanted: &Alternates, label_sets: &[String]) -> Option<(Vec<u8>, LabelForms)> {
    use skrifa::MetadataProvider as _;
    let swap: std::collections::HashMap<u32, u32> = style_sets(data)
        .into_iter()
        .filter(|set| wanted.sets.contains(&set.id))
        .flat_map(|set| set.swaps)
        .fold(Default::default(), |mut swap, (from, to)| {
            swap.entry(from).or_insert(to);
            swap
        });
    let font = skrifa::FontRef::new(data).ok()?;
    let mut forms: Vec<(char, char, u32)> = Vec::new();
    if !label_sets.is_empty() {
        let typed: Vec<(char, u32)> = font.charmap().mappings().filter_map(|(code, glyph)| Some((char::from_u32(code)?, glyph.to_u32()))).collect();
        let mut pairs: Vec<(char, u32)> = style_sets(data)
            .into_iter()
            .filter(|set| label_sets.contains(&set.id))
            .flat_map(|set| set.swaps)
            .flat_map(|(from, to)| typed.iter().filter(move |(_, g)| *g == from).map(move |(c, _)| (*c, to)).collect::<Vec<_>>())
            .collect();
        pairs.sort_unstable();
        pairs.dedup_by_key(|(c, _)| *c);
        forms = pairs
            .into_iter()
            .enumerate()
            .filter_map(|(i, (c, to))| Some((c, char::from_u32(LABEL_FORMS_AT + i as u32)?, to)))
            .collect();
    }
    if swap.is_empty() && forms.is_empty() {
        return None;
    }
    let mut mappings: Vec<(char, write_fonts::types::GlyphId)> = font
        .charmap()
        .mappings()
        .filter_map(|(code, glyph)| {
            let glyph = glyph.to_u32();
            let glyph = swap.get(&glyph).copied().unwrap_or(glyph);
            Some((char::from_u32(code)?, write_fonts::types::GlyphId::new(glyph)))
        })
        .filter(|(c, _)| !forms.iter().any(|(_, pua, _)| pua == c))
        .collect();
    mappings.extend(forms.iter().map(|&(_, pua, glyph)| (pua, write_fonts::types::GlyphId::new(glyph))));
    mappings.sort_by_key(|(c, _)| *c);
    let cmap = write_fonts::tables::cmap::Cmap::from_mappings(mappings).ok()?;
    let mut builder = write_fonts::FontBuilder::new();
    builder.add_table(&cmap).ok()?;
    builder.copy_missing_tables(font);
    Some((builder.build(), forms.iter().map(|&(c, pua, _)| (c, pua)).collect()))
}

/// Where a face's axis of this tag runs, from its least to its most, when it
/// has one that runs anywhere.
fn axis_range(data: &[u8], tag: &[u8; 4]) -> Option<(f32, f32)> {
    use skrifa::MetadataProvider;
    let font = skrifa::FontRef::new(data).ok()?;
    let axis = font.axes().iter().find(|a| a.tag() == skrifa::Tag::new(tag))?;
    let (least, most) = (axis.min_value(), axis.max_value());
    (most > least).then_some((least, most))
}

/// How wide the face is to be set, on its width axis — `[ui] font_width`,
/// handed over before the faces are installed.
static WIDTH: std::sync::RwLock<f32> = std::sync::RwLock::new(100.0);

/// The range of the width axis of the face in use, while it has one — for
/// the menu, which offers the slider only then.
static WIDTH_AXIS: std::sync::RwLock<Option<(f32, f32)>> = std::sync::RwLock::new(None);

/// Set the width the next [`install_fonts`] sets the face at.
pub fn set_font_width(width: f32) {
    if let Ok(mut held) = WIDTH.write() {
        *held = width;
    }
}

/// The width the face is set at, on a face with a width axis.
pub fn font_width() -> f32 {
    WIDTH.read().map_or(100.0, |held| *held)
}

/// Where the width axis of the face in use runs, if it has one.
pub fn width_axis() -> Option<(f32, f32)> {
    WIDTH_AXIS.read().ok().and_then(|held| *held)
}

/// The weights of the face in use, while it has an axis to set them on —
/// see [`weights`].
static WEIGHTS: std::sync::RwLock<Option<(f32, f32)>> = std::sync::RwLock::new(None);

fn set_weights(weights: Option<(f32, f32)>) {
    if let Ok(mut held) = WEIGHTS.write() {
        *held = weights;
    }
}

/// The regular and bold weights of the face the window is set in, when it
/// is variable: a name can then be set anywhere between the two, and move
/// from one to the other over a few frames instead of in one.
pub fn weights() -> Option<(f32, f32)> {
    WEIGHTS.read().ok().and_then(|held| *held)
}

/// Install the faces, and report what was installed.
///
/// `figures` is the mono cut of the same family, for the one thing it is
/// better at — see [`family_figures`]. Without one, figures are set in the
/// face everything else is set in, which is what happened before there was a
/// mono cut to reach for and is a perfectly good answer.
///
/// `scale` sets the chosen faces that much smaller or larger than their
/// size, advances and all — see [`face_scale`].
pub fn install_fonts(
    ctx: &egui::Context,
    regular: Option<&str>,
    bold: Option<&str>,
    figures: Option<&str>,
    fallback: Option<&str>,
    scale: f32,
) -> (FontSource, Option<String>) {
    let mut fonts = egui::FontDefinitions::default();
    let face = |data: Vec<u8>| {
        std::sync::Arc::new(egui::FontData::from_owned(data).tweak(hinting().on(egui::FontTweak {
            scale,
            ..Default::default()
        })))
    };

    let wanted = ALTERNATES.read().map_or_else(|_| Alternates::default(), |held| held.clone());
    let label_sets = LABEL_SETS.read().map(|held| held.clone()).unwrap_or_default();
    let altered = |data: Vec<u8>| match with_alternates(&data, &wanted, &label_sets) {
        Some((altered, forms)) => {
            if let Ok(mut held) = LABEL_FORMS.write() {
                *held = forms;
            }
            altered
        }
        None => data,
    };
    let chosen: Option<LoadedPair> = regular
        .and_then(|r| std::fs::read(r).ok().map(|data| (r.to_string(), data)))
        .map(|(r_path, r_data)| {
            let b = bold.and_then(|b| {
                std::fs::read(b).ok().map(|data| (b.to_string(), altered(data)))
            });
            (r_path, altered(r_data), b)
        });
    super::glyphs::use_face(ctx, chosen.as_ref().map(|(_, data, _)| data.clone()), scale);
    super::glyphs::use_icon_face(
        ctx,
        chosen.as_ref().map(|(_, r_data, bold)| {
            let at: super::glyphs::Place = super::appicon::ICON_AT
                .iter()
                .filter_map(|&(tag, value)| {
                    let tag4: [u8; 4] = tag.as_bytes().try_into().ok()?;
                    let (least, most) = axis_range(r_data, &tag4)?;
                    Some((tag, value.clamp(least, most)))
                })
                .collect();
            match bold {
                Some((_, b_data)) if at.is_empty() => (b_data.clone(), at),
                _ => (r_data.clone(), at),
            }
        }),
    );

    let mut firsts: Vec<(FontFamily, Option<EmMetrics>)> = Vec::new();
    let variable = chosen.as_ref().and_then(|(_, data, _)| variable_weights(data));
    set_weights(variable);
    if let Ok(mut held) = WEIGHT_AXIS.write() {
        *held = chosen
            .as_ref()
            .and_then(|(_, data, _)| axis_range(data, b"wght"));
    }
    let width_axis = chosen.as_ref().and_then(|(_, data, _)| axis_range(data, b"wdth"));
    if let Ok(mut held) = WIDTH_AXIS.write() {
        *held = width_axis;
    }
    let width = width_axis.map(|(least, most)| WIDTH.read().map_or(100.0, |w| *w).clamp(least, most));
    let at_weight = |data: Vec<u8>, weight: f32| {
        let mut coords = egui::epaint::text::VariationCoords::new([(b"wght", weight)]);
        if let Some(width) = width {
            coords.push(b"wdth", width);
        }
        std::sync::Arc::new(egui::FontData::from_owned(data).tweak(hinting().on(egui::FontTweak {
            scale,
            coords,
            ..Default::default()
        })))
    };
    let source = match chosen {
        Some((r_path, r_data, b_pair)) => {
            let regular_metrics = em_metrics(&r_data, scale);
            firsts.push((FontFamily::Proportional, regular_metrics));
            let b_pair = match (b_pair, variable) {
                (None, Some((_, bold))) => {
                    fonts.font_data.insert("bold".into(), at_weight(r_data.clone(), bold));
                    fonts
                        .families
                        .insert(FontFamily::Name("bold".into()), vec!["bold".into()]);
                    firsts.push((FontFamily::Name("bold".into()), regular_metrics));
                    None
                }
                (pair, _) => pair,
            };
            match variable {
                Some((regular, _)) => fonts.font_data.insert("regular".into(), at_weight(r_data, regular)),
                None => fonts.font_data.insert("regular".into(), face(r_data)),
            };
            fonts
                .families
                .entry(FontFamily::Proportional)
                .or_default()
                .insert(0, "regular".into());

            match b_pair {
                Some((b_path, b_data)) => {
                    firsts.push((FontFamily::Name("bold".into()), em_metrics(&b_data, scale)));
                    fonts.font_data.insert("bold".into(), face(b_data));
                    fonts
                        .families
                        .insert(FontFamily::Name("bold".into()), vec!["bold".into()]);
                    FontSource::Pair(r_path, b_path)
                }
                None if variable.is_some() => FontSource::Pair(r_path.clone(), r_path),
                None => {
                    firsts.push((FontFamily::Name("bold".into()), regular_metrics));
                    fonts
                        .families
                        .insert(FontFamily::Name("bold".into()), vec!["regular".into()]);
                    FontSource::RegularOnly(r_path)
                }
            }
        }
        None => {
            firsts.push((FontFamily::Proportional, None));
            firsts.push((FontFamily::Name("bold".into()), None));
            let fallback = fonts
                .families
                .get(&FontFamily::Proportional)
                .and_then(|v| v.first().cloned())
                .unwrap_or_else(|| "Ubuntu-Light".into());
            fonts
                .families
                .insert(FontFamily::Name("bold".into()), vec![fallback]);
            FontSource::BuiltIn
        }
    };

    let mono = figures
        .filter(|path| Some(*path) != regular)
        .and_then(|path| std::fs::read(path).ok())
        .map(altered);
    let regular_first = firsts.first().and_then(|(_, m)| *m);
    let figure_faces = match mono {
        Some(data) => {
            firsts.push((FontFamily::Name("figures".into()), em_metrics(&data, scale)));
            fonts.font_data.insert("figures".into(), face(data));
            vec!["figures".to_string()]
        }
        None => {
            firsts.push((FontFamily::Name("figures".into()), regular_first));
            fonts
                .families
                .get(&FontFamily::Proportional)
                .cloned()
                .unwrap_or_default()
        }
    };
    fonts
        .families
        .insert(FontFamily::Name("figures".into()), figure_faces);

    let medium = regular
        .filter(|_| LABEL_MEDIUM.load(std::sync::atomic::Ordering::Relaxed))
        .map(std::path::Path::new)
        .and_then(|path| {
            let name = path.file_name()?.to_str()?.replace("Regular", "Medium");
            Some(path.with_file_name(name))
        })
        .and_then(|path| std::fs::read(path).ok())
        .map(altered);
    let label_faces = match medium {
        Some(data) => {
            firsts.push((family_label(), em_metrics(&data, scale)));
            fonts.font_data.insert("label".into(), face(data));
            vec!["label".to_string()]
        }
        None => {
            firsts.push((family_label(), regular_first));
            fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default()
        }
    };
    fonts.families.insert(family_label(), label_faces);

    let figures_medium = figures
        .filter(|_| LABEL_MEDIUM.load(std::sync::atomic::Ordering::Relaxed))
        .map(std::path::Path::new)
        .and_then(|path| {
            let name = path.file_name()?.to_str()?.replace("Regular", "Medium");
            Some(path.with_file_name(name))
        })
        .and_then(|path| std::fs::read(path).ok())
        .map(altered);
    let figure_label_faces = match figures_medium {
        Some(data) => {
            firsts.push((family_figures_label(), em_metrics(&data, scale)));
            fonts.font_data.insert("figures-label".into(), face(data));
            vec!["figures-label".to_string()]
        }
        None => {
            let first = firsts.iter().find(|(f, _)| *f == family_figures()).and_then(|(_, m)| *m);
            firsts.push((family_figures_label(), first));
            fonts.families.get(&family_figures()).cloned().unwrap_or_default()
        }
    };
    fonts.families.insert(family_figures_label(), figure_label_faces);

    if let Some(asap) = font_folder(None)
        .map(|dir| dir.join("Asap").join("Asap-Variable.ttf"))
        .filter(|path| path.is_file() && regular.map(std::path::Path::new) != Some(path.as_path()))
    {
        append_fallback(&mut fonts, &asap, None, &firsts, "-marks");
    }

    let configured = fallback.map(std::path::PathBuf::from).filter(|p| p.is_file());
    let bundled = if configured.is_none() { bundled_fallback() } else { None };
    let bundled_name = bundled
        .as_ref()
        .and_then(|(r, b)| append_fallback(&mut fonts, r, b.as_deref(), &firsts, ""))
        .map(|_| BUNDLED_FALLBACK.to_lowercase());
    let system_name = configured
        .or_else(fallback_face)
        .and_then(|p| append_fallback(&mut fonts, &p, None, &firsts, "-system"));
    let fallback_name = bundled_name.or(system_name);

    ctx.set_fonts(fonts);
    ctx.request_repaint();
    (source, fallback_name)
}

/// Strip every default egui decoration: no widget fills, no strokes, no
/// rounding, no shadows. What is left is text on a ground.
pub fn install_style(ctx: &egui::Context, palette: &Palette) {
    let palette = *palette;
    ctx.all_styles_mut(move |style| {
    let v = &mut style.visuals;
    v.dark_mode = palette.is_dark();
    v.panel_fill = palette.ground;
    v.window_fill = palette.ground;
    v.extreme_bg_color = palette.ground;
    v.faint_bg_color = palette.ground;
    v.override_text_color = Some(palette.ink);
    v.window_stroke = egui::Stroke::NONE;
    v.selection.bg_fill = Color32::TRANSPARENT;
    v.selection.stroke = egui::Stroke::NONE;

    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.bg_fill = Color32::TRANSPARENT;
        w.weak_bg_fill = Color32::TRANSPARENT;
        w.bg_stroke = egui::Stroke::NONE;
        w.fg_stroke = egui::Stroke::new(1.0, palette.ink);
        w.corner_radius = egui::CornerRadius::ZERO;
        w.expansion = 0.0;
    }

    style.spacing.item_spacing = egui::vec2(0.0, 0.0);
    style.spacing.window_margin = egui::Margin::ZERO;
    style.spacing.scroll = scroll_style();
    });
}

/// Scrolling that takes up no room.
///
/// This window has no scroll bars — how long a list is gets said by fading its
/// edge and by the mark outside the column — but egui was still *reserving* the
/// width of one, six points of bar and four of margin, the moment a list grew
/// long enough to scroll. The rows are drawn at the column's full width
/// regardless, so the column came out ten points wider than it asked to be, the
/// gap to its right looked ten points too wide, and those ten points came off
/// the track list.
///
/// Hidden at each scroll area *and* zeroed here: the first is what the reader
/// of that code sees, the second is what holds if somebody adds a fourth scroll
/// area and forgets. How long a flick's coast takes to die away, in seconds —
/// see [`scroll_style`]. Egui's own is 0.5, as iOS's „normal"; egui lets the
/// rubber band be on or off and no more, and how far it stretches follows from
/// how far the flick still coasts.
const KINETIC_DECAY: f32 = 0.15;

fn scroll_style() -> egui::style::ScrollStyle {
    egui::style::ScrollStyle {
        bar_width: 0.0,
        bar_inner_margin: 0.0,
        bar_outer_margin: 0.0,
        kinetic: egui::style::KineticScrollStyle { decay_time: KINETIC_DECAY, ..Default::default() },
        ..egui::style::ScrollStyle::solid()
    }
}

#[cfg(test)]
mod band_tests {
    use super::margin_under;

    /// The masthead stands where it always stood under the plain band, and
    /// under a taller one — the buttons set in, `macwindow` — clear of it by
    /// a little air; with no band, the window's own margin.
    #[test]
    fn the_masthead_stands_under_the_band() {
        assert_eq!(margin_under(28.0), 52.0);
        assert_eq!(margin_under(38.0), 52.0);
        assert_eq!(margin_under(52.0), 64.0);
        assert_eq!(margin_under(0.0), 34.0);
    }

    /// The sides come in to 24 in a wide window and a narrow one keeps its
    /// own.
    #[test]
    fn the_sides_come_in() {
        use super::Metrics;
        let wide = Metrics::for_size(1600.0, 900.0);
        let narrow = Metrics::for_size(700.0, 900.0);
        assert_eq!(wide.with_margins().margin_x, 24.0);
        assert_eq!(narrow.with_margins().margin_x, narrow.margin_x);
    }
}

#[cfg(test)]
mod sliding_tests {
    use super::*;

    /// Whichever column is being read fills the window exactly, and its
    /// neighbours are wholly outside it.
    #[test]
    fn one_column_at_a_time_and_nothing_of_the_next() {
        let room = 400.0;
        for count in [2usize, 3, 4] {
            for index in 0..count {
                let offset = slide_offset(index, count, room, SLIDE_GAP);
                for at in 0..count {
                    let left = at as f32 * (room + SLIDE_GAP) - offset;
                    let right = left + room;
                    if at == index {
                        assert_eq!((left, right), (0.0, room), "{index} of {count}");
                    } else {
                        assert!(right <= 0.0 || left >= room, "{at} shows beside {index}");
                    }
                }
            }
        }
    }

    /// The strip is as wide as what is in it, and its last column ends where
    /// the window does when it is the one being read.
    #[test]
    fn the_strip_is_exactly_as_wide_as_what_is_in_it() {
        for count in [2usize, 3] {
            let laid_out = strip_width(count, 420.0, SLIDE_GAP);
            assert_eq!(laid_out - slide_offset(count - 1, count, 420.0, SLIDE_GAP), 420.0);
        }
    }

    /// An index past the end is the end, and no columns is no offset.
    #[test]
    fn the_edges_of_the_edges() {
        assert_eq!(slide_offset(9, 3, 400.0, SLIDE_GAP), slide_offset(2, 3, 400.0, SLIDE_GAP));
        assert_eq!(slide_offset(0, 0, 400.0, SLIDE_GAP), 0.0);
        assert_eq!(strip_width(0, 400.0, SLIDE_GAP), 0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every scheme names its heads' colour, and it stands apart from what
    /// it could be taken for: the facts under the names (the third tone), the
    /// second tone of what can be clicked, and the accent of what plays — so a
    /// head reads as a head in every scheme — and it reads on the ground.
    #[test]
    fn the_heads_stand_apart() {
        let apart = |a: Color32, b: Color32| {
            let (p, q) = (Lab::of(a), Lab::of(b));
            ((p.l - q.l).powi(2) + (p.a - q.a).powi(2) + (p.b - q.b).powi(2)).sqrt()
        };
        for theme in Theme::all() {
            let p = theme.palette();
            let name = theme.name();
            let head = p.heading.unwrap_or_else(|| panic!("{name}: no colour for the heads"));
            assert!(apart(head, p.faint) >= 0.09, "{name}: heads near the facts, {:.3}", apart(head, p.faint));
            assert!(apart(head, p.accent) >= 0.09, "{name}: heads near the accent, {:.3}", apart(head, p.accent));
            assert!(apart(head, p.mid) >= 0.075, "{name}: heads near the second tone, {:.3}", apart(head, p.mid));
            assert!(contrast(head, p.ground) >= 3.0, "{name}: heads only {:.1}:1 on the ground", contrast(head, p.ground));
        }
    }

    /// The tones under the ink stand in order between the field and the ink,
    /// each well apart from its neighbours, in every scheme, light and dark, on
    /// the ink and on a marked cover's colour — and none is the colour it
    /// stands under.
    #[test]
    fn the_tones_under_stand_in_order() {
        for theme in Theme::all() {
            let p = theme.palette().coloured();
            for ink in [p.ink, p.lit_dots()] {
                for tones in [3u8, 4] {
                    let under = p.tones_under(ink, tones);
                    assert_eq!(under.len(), usize::from(tones) - 2);
                    assert!(!under.contains(&ink), "{}", theme.name());
                    let mut steps = vec![Lab::of(p.field).l];
                    steps.extend(under.iter().map(|c| Lab::of(*c).l));
                    steps.push(Lab::of(ink).l);
                    let way = steps[steps.len() - 1] - steps[0];
                    if way.abs() < 0.2 {
                        continue;
                    }
                    for pair in steps.windows(2) {
                        let step = (pair[1] - pair[0]) / way;
                        assert!(step > 0.12, "{} in {tones}: {steps:?}", theme.name());
                    }
                }
            }
        }
    }

    /// With the colour in the type, every scheme keeps its ink, accent and
    /// ground; its second and third tones are as light as before, so they
    /// read as well; its plates are no more colourful than its ground; a
    /// scheme of greys and one that had its colour in the type already keep
    /// their tones.
    #[test]
    fn the_colour_goes_into_the_type() {
        for theme in Theme::all() {
            let before = theme.palette();
            let after = before.in_the_type();
            let name = theme.name();
            assert_eq!((after.ink, after.accent, after.ground), (before.ink, before.accent, before.ground), "{name}");
            for (was, now) in [(before.mid, after.mid), (before.faint, after.faint)] {
                assert!((Lab::of(was).l - Lab::of(now).l).abs() < 0.03, "{name}: {was:?} → {now:?}");
            }
            let ground = Lab::of(before.ground).chroma();
            for plate in [after.marker, after.field, after.rule, after.hover] {
                assert!(Lab::of(plate).chroma() < ground + 0.02, "{name}: {plate:?}");
            }
            assert!(after.heading.is_some(), "{name}");
        }
        let (before, after) = (Theme::MacosHell.palette(), Theme::MacosHell.palette().in_the_type());
        assert_eq!((after.mid, after.faint), (before.mid, before.faint));
        assert!(Lab::of(after.heading()).chroma() > COLOURFUL);
        for theme in [Theme::Runebending, Theme::Rubrik] {
            let (before, after) = (theme.palette(), theme.palette().in_the_type());
            assert_eq!((after.mid, after.faint), (before.mid, before.faint), "{}", theme.name());
        }
    }

    /// With the colour on the plates every scheme that has a colour shows
    /// it behind the words, and the words on the marker still read.
    #[test]
    fn the_colour_goes_onto_the_plates() {
        for theme in Theme::all() {
            let (before, after) = (theme.palette(), theme.palette().on_the_plates());
            let name = theme.name();
            assert_eq!((after.ink, after.mid, after.faint, after.accent), (before.ink, before.mid, before.faint, before.accent), "{name}");
            if after == before {
                let colourless = matches!(
                    theme,
                    Theme::PaysageDHiver | Theme::PaysageDHiverHell | Theme::IpodClassic | Theme::IpodDunkel
                );
                assert!(colourless || Lab::of(before.marker).chroma() >= PLATE_CHROMA[0], "{name} kept grey plates");
                continue;
            }
            if Lab::of(before.marker).l < 0.95 {
                assert!(Lab::of(after.marker).chroma() >= 0.025, "{name}: marker {:?}", after.marker);
            }
            let near = |a: f32, b: f32| (a - b).abs() < 0.03;
            assert!(near(Lab::of(after.marker).l, Lab::of(before.marker).l), "{name}: the marker moved in lightness");
        }
    }

    /// Oklab there and back is the same colour.
    #[test]
    fn oklab_comes_back() {
        for c in [rgb(0x7CC86A), rgb(0x141414), rgb(0xFFFFFF), rgb(0x0064D8), rgb(0xE4C850)] {
            let back = Lab::of(c).colour();
            let near = |a: u8, b: u8| (a as i32 - b as i32).abs() <= 1;
            assert!(near(c.r(), back.r()) && near(c.g(), back.g()) && near(c.b(), back.b()), "{c:?} → {back:?}");
        }
    }

    /// The menu sits in an even margin, and never in the title bar.
    ///
    /// Worked for both kinds of window on every machine: the Mac's is the one
    /// that has a title bar, and it is also the one never compiled where this
    /// code is written — so the arithmetic takes the band as a number instead
    /// of reading the constant, and both are checked here.
    #[test]
    fn the_menu_stands_in_an_even_margin_and_out_of_the_title_bar() {
        for side in [22.0_f32, 30.0, 34.0, 40.0] {
            assert_eq!(sheet_inset_top(side, 0.0), side);
            let mac = sheet_inset_top(side, 28.0);
            assert!(mac >= 28.0 + 6.0, "{side}: the sheet's top is in the title bar");
            if side >= 34.0 {
                assert_eq!(mac, side, "{side}: top and side differ with room to spare");
            }
        }
    }

    /// The constraint this whole module exists to hold: exactly two sizes.
    #[test]
    fn there_are_only_two_type_sizes() {
        let sizes: std::collections::BTreeSet<u32> = [
            Role::Content,
            Role::ContentBold,
            Role::Label,
            Role::LabelBold,
        ]
        .iter()
        .map(|r| r.font().size.to_bits())
        .collect();

        assert_eq!(sizes.len(), 2, "the interface must use exactly two sizes");
    }

    #[test]
    fn only_labels_are_tracked_out() {
        assert_eq!(Role::Content.tracking(), 0.0);
        assert_eq!(Role::ContentBold.tracking(), 0.0);
        assert!(Role::Label.tracking() > 0.0);
        assert!(Role::LabelBold.tracking() > 0.0);
    }

    #[test]
    fn the_two_weights_are_different_families() {
        assert_ne!(Role::Content.font().family, Role::ContentBold.font().family);
        assert_eq!(Role::Label.font().family, family_label());
        assert_eq!(Role::Note.font().family, family_label());
        assert_eq!(Role::FiguresLabel.font().family, family_figures_label());
    }

    #[test]
    fn lowercases_everything_it_draws() {
        assert_eq!(lower("Brian Eno"), "brian eno");
        assert_eq!(lower("MUSIC FOR AIRPORTS"), "music for airports");
        assert_eq!(lower("Björk – LÄNGE"), "björk – länge");
        assert_eq!(lower("STRASSE"), "strasse");
    }

    /// Every scheme has to be reachable by cycling, and every name has to come
    /// back as the scheme it names — including the two spellings this setting
    /// carried before there were more than two schemes.
    /// Every scheme stands in one group of the menu, once.
    #[test]
    fn every_scheme_is_in_one_group() {
        let listed: Vec<&str> = Theme::families().iter().map(|t| t.family()).collect();
        let mut all: Vec<&str> = Theme::all().iter().map(|t| t.family()).collect();
        all.dedup();
        let mut sorted = listed.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), listed.len(), "a scheme in two groups");
        let mut every = all.clone();
        every.sort();
        every.dedup();
        assert_eq!(sorted, every);
        assert_eq!(Theme::from_name("hudson"), Theme::Hudson);
        assert_eq!(Theme::from_name("hudson hell"), Theme::HudsonHell);
    }

    /// One's own scheme passes on as a code and comes back the same, however it
    /// was copied; its other half has the lightness turned over and is dark
    /// where it was light.
    #[test]
    fn a_seed_passes_on_as_a_code() {
        let seed = Seed::DEFAULT;
        assert_eq!(seed.code(), "k1-282828-ebdbb2-fb4934-fabd2f");
        assert_eq!(Seed::parse(&seed.code()), Some(seed));
        assert_eq!(Seed::parse(" K1 #282828 #EBDBB2 #fb4934 #fabd2f "), Some(seed));
        assert_eq!(Seed::parse("282828.ebdbb2.fb4934.fabd2f"), Some(seed));
        assert_eq!(Seed::parse("k1-282828-ebdbb2-fb4934"), None, "three colours are not a scheme");
        assert_eq!(Seed::parse("k1-28282-ebdbb2-fb4934-fabd2f"), None);
        assert_eq!(Seed::parse(""), None);
        assert_eq!(Seed::parse_colour("#fb4934"), Some(rgb(0xFB4934)));
        assert_eq!(Seed::parse_colour(" FB4934 "), Some(rgb(0xFB4934)));
        assert_eq!(Seed::parse_colour("#f43"), Some(rgb(0xFF4433)));
        assert_eq!(Seed::parse_colour("#fb493"), None);
        assert_eq!(Seed::parse_colour("#gg4934"), None);
        assert!(seed.is_dark());
        assert!(!seed.side(false).is_dark());
        assert_eq!(seed.side(true), seed);
        assert!(Theme::EigenesDunkel.palette().is_dark());
        assert!(!Theme::Eigenes.palette().is_dark());
        assert_eq!(Theme::Eigenes.partner(), Theme::EigenesDunkel);
    }

    /// A colour set by the menu's three rules comes back as nearly itself.
    #[test]
    fn a_colour_survives_its_rules() {
        for colour in [rgb(0xFB4934), rgb(0x282828), rgb(0xEBDBB2), rgb(0x076678)] {
            let back = from_rules(rules_of(colour));
            let near = |a: u8, b: u8| a.abs_diff(b) <= 2;
            assert!(near(back.r(), colour.r()) && near(back.g(), colour.g()) && near(back.b(), colour.b()), "{colour:?} → {back:?}");
        }
    }

    #[test]
    fn themes_round_trip_through_their_names() {
        for theme in Theme::all() {
            assert_eq!(Theme::from_name(theme.name()), theme, "{}", theme.name());
        }
        assert_eq!(Theme::from_name("  NORD "), Theme::Nord);
        for gone in ["ink", "dark", "tinte", "monokai", "gruvbox", "blaupause"] {
            assert_eq!(Theme::from_name(gone), Theme::MacosDunkel, "{gone}");
        }
        for gone in ["paper", "light", "papier", "riso"] {
            assert_eq!(Theme::from_name(gone), Theme::MacosHell, "{gone}");
        }
        assert_eq!(Theme::from_name("whatever"), Theme::native().0);
        assert_eq!(Theme::from_name("systemfarben"), Theme::native().0);
    }

    /// Every scheme has its other half: the two point at each other, share
    /// a name in the menu, and the one is light where the other is dark —
    /// which is what „hell, dunkel" and `shift` + `t` rely on.
    #[test]
    fn partners_are_each_others_and_opposite() {
        for theme in Theme::all() {
            let other = theme.partner();
            assert_ne!(other, theme, "{} has no partner", theme.name());
            assert_eq!(other.partner(), theme, "{}", theme.name());
            assert_eq!(other.family(), theme.family(), "{}", theme.name());
            assert_ne!(theme.palette().is_dark(), other.palette().is_dark(), "{}", theme.name());
        }
        assert_eq!(Theme::families().len() * 2, Theme::all().len());
        assert_eq!(Theme::SolarisiertHell.family(), "solarisiert");
        assert_eq!(Theme::Nord.family(), Theme::NordHell.family());
    }

    /// A desktop's accent, taken by the schemes after a desktop, reads as
    /// words on either half's ground, in its own hue; the others keep theirs.
    #[test]
    fn a_desktop_accent_reads_on_either_ground() {
        let accents = [0x3584E4, 0x2190A4, 0x3A944A, 0xC88800, 0xED5B00, 0xE62D42, 0xD56199, 0x9141AC, 0x6F8396];
        for theme in [Theme::GnomeHell, Theme::GnomeDunkel, Theme::UbuntuHell, Theme::UbuntuDunkel] {
            for hex in accents {
                let p = Palette::as_drawn(theme).with_accent(rgb(hex));
                assert!(contrast(p.accent, p.ground) >= DESKTOP_ACCENT_CONTRAST - 0.2, "{} {hex:06x}", theme.name());
                assert!(Lab::of(p.accent).hue_apart(&Lab::of(rgb(hex))) < 12.0, "{} {hex:06x}", theme.name());
                if std::env::var_os("KESSLER_PRINT_ACCENTS").is_some() {
                    let c = p.accent;
                    eprintln!("ACCENT {} {hex:06x} {:02x}{:02x}{:02x}", theme.name(), c.r(), c.g(), c.b());
                }
            }
        }
    }

    /// Dark schemes have to be recognised as dark, or egui dresses its own
    /// leftovers in the wrong values.
    #[test]
    fn light_and_dark_are_told_apart() {
        for light in [
            Theme::SolarisiertHell,
            Theme::MacosHell,
            Theme::GnomeHell,
            Theme::IpodClassic,
            Theme::Soulseek,
        ] {
            assert!(!light.palette().is_dark(), "{}", light.name());
        }
        for dark in [
            Theme::MacosDunkel,
            Theme::GnomeDunkel,
            Theme::SolarisiertDunkel,
            Theme::Nord,
            Theme::Dracula,
        ] {
            assert!(dark.palette().is_dark(), "{}", dark.name());
        }
    }

    /// The fade from one scheme to another starts on the one and ends on
    /// the other.
    #[test]
    fn a_fade_runs_from_one_scheme_to_the_other() {
        let (a, b) = (Theme::SolarisiertHell.palette(), Theme::SolarisiertDunkel.palette());
        assert_eq!(a.mix(&b, 0.0), a);
        assert_eq!(a.mix(&b, 1.0), b);
        let half = a.mix(&b, 0.5);
        assert_ne!(half, a);
        assert_ne!(half, b);
    }

    /// Following the system lands on the half the system says, and on the
    /// light one when it says nothing; a fixed choice ignores it.
    #[test]
    fn brightness_picks_the_half() {
        for theme in Theme::all() {
            let dark = Brightness::System.resolve(theme, Some(egui::Theme::Dark));
            assert!(dark.palette().is_dark(), "{}", theme.name());
            assert_eq!(dark.family(), theme.family());
            assert!(!Brightness::System.resolve(theme, None).palette().is_dark());
            for system in [None, Some(egui::Theme::Dark), Some(egui::Theme::Light)] {
                assert!(!Brightness::Hell.resolve(theme, system).palette().is_dark());
                assert!(Brightness::Dunkel.resolve(theme, system).palette().is_dark());
            }
        }
    }

    /// the scheme it names decides, so nobody's window changes under them.
    #[test]
    fn old_configs_keep_their_look() {
        assert_eq!(Brightness::from_config("", "systemfarben"), Brightness::System);
        assert_eq!(Brightness::from_config("", ""), Brightness::System);
        assert_eq!(Brightness::from_config("", "nord"), Brightness::Dunkel);
        assert_eq!(Brightness::from_config("", "macos hell"), Brightness::Hell);
        assert_eq!(Brightness::from_config("system", "nord"), Brightness::System);
        for b in [Brightness::System, Brightness::Hell, Brightness::Dunkel] {
            assert_eq!(Brightness::from_config(b.name(), "nord"), b);
        }
    }

    /// A report of every scheme as the window shows it, for looking at by
    /// hand: `cargo test scheme_report -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn scheme_report() {
        for theme in Theme::all() {
            let p = theme.palette().coloured();
            let m = p.marked();
            println!(
                "{:22} marked/ground {:.2}  ink/marked {:.1}  mid/marked {:.1}  faint/marked {:.1}  faint/ground {:.1}  accent_faint/marked {:.1}  heading/ground {:.1}  field/ground {:.2}",
                theme.name(),
                contrast(m, p.ground),
                contrast(p.ink, m),
                contrast(p.mid, m),
                contrast(Palette::legible_on(p.faint, m, ON_PLATE_LEAST), m),
                contrast(p.faint, p.ground),
                contrast(Palette::legible_on(p.accent_faint, m, ON_PLATE_LEAST), m),
                contrast(p.heading(), p.ground),
                contrast(p.field, p.ground),
            );
        }
    }

    /// Every scheme as the window shows it: the marked plate is seen on the
    /// ground, the words on it can be read, and so can the third tone and what
    /// is small on the plate.
    #[test]
    fn every_scheme_reads_on_its_plates() {
        for theme in Theme::all() {
            let p = theme.palette().coloured();
            let name = theme.name();
            let m = p.marked();
            assert!(contrast(m, p.ground) >= MARKED_LEAST - 0.01, "{name}: plate {:.2}", contrast(m, p.ground));
            assert!(contrast(p.ink, m) >= 4.5, "{name}: ink on the plate {:.1}", contrast(p.ink, m));
            assert!(contrast(p.faint, p.ground) >= FAINT_LEAST - 0.05, "{name}: faint {:.1}", contrast(p.faint, p.ground));
            for tone in [p.faint, p.mid, p.accent_faint, p.accent] {
                let shown = Palette::legible_on(tone, m, ON_PLATE_LEAST);
                assert!(contrast(shown, m) >= ON_PLATE_LEAST - 0.05, "{name}: small words on the plate {:.1}", contrast(shown, m));
            }
        }
    }

    /// Text has to stay legible against its ground on both palettes. Rough
    /// relative-luminance contrast, enough to catch a value picked carelessly.
    #[test]
    fn primary_text_carries_enough_contrast() {
        fn luminance(c: Color32) -> f32 {
            let f = |v: u8| {
                let v = v as f32 / 255.0;
                if v <= 0.03928 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * f(c.r()) + 0.7152 * f(c.g()) + 0.0722 * f(c.b())
        }
        fn ratio(a: Color32, b: Color32) -> f32 {
            let (x, y) = (luminance(a), luminance(b));
            let (hi, lo) = if x > y { (x, y) } else { (y, x) };
            (hi + 0.05) / (lo + 0.05)
        }

        for theme in Theme::all() {
            let p = theme.palette();
            let name = theme.name();
            assert!(
                ratio(p.ink, p.ground) >= 7.0,
                "{name}: primary text only {:.1}:1",
                ratio(p.ink, p.ground)
            );
            assert!(
                ratio(p.mid, p.ground) >= 3.0,
                "{name}: secondary text only {:.1}:1",
                ratio(p.mid, p.ground)
            );
            assert!(
                ratio(p.accent, p.ground) >= 3.5,
                "{name}: accent only {:.1}:1",
                ratio(p.accent, p.ground)
            );
            let loud = matches!(theme, Theme::Rgb | Theme::RgbHell);
            assert!(
                ratio(p.ink, p.marker) >= 6.0,
                "{name}: text on the marker only {:.1}:1",
                ratio(p.ink, p.marker)
            );
            if loud {
                continue;
            }
            let marker = ratio(p.marker, p.ground);
            assert!(
                (1.12..=1.9).contains(&marker),
                "{name}: marker sits at {marker:.2}:1 against the ground"
            );
            assert!(
                ratio(p.ink, p.marker) >= 6.0,
                "{name}: text on the marker only {:.1}:1",
                ratio(p.ink, p.marker)
            );
            let field = ratio(p.field, p.ground);
            assert!(
                (1.02..=1.35).contains(&field),
                "{name}: search field sits at {field:.2}:1 against the ground"
            );
        }
    }

    /// A short window shows more rows rather than the same rows with the same
    /// air; the type size never moves.
    /// Putting the queue away gives its strip to every column alike, not to
    /// the track list entire.
    ///
    /// The report: hiding the queue made the layout jump, because a quarter
    /// of the window arrived in one column. What the shares mean is unchanged —
    /// they are still fractions of the row with everything open — so this is a
    /// share-out on the way to the screen and it has to be exact in both
    /// directions: the pieces are equal, they add up to the strip, and putting
    /// the queue back leaves every column where it was.
    #[test]
    fn putting_the_queue_away_widens_every_column_alike() {
        let dragged = Shares {
            artists: Some(0.22),
            albums: Some(0.24),
            queue: Some(0.18),
            side: None,
            even: false,
        };
        let room = 1400.0_f32;
        let m = Metrics::for_size(room + 80.0, 900.0);
        let strip = 0.18 * room + m.column_gap;

        let open = columns_for(room, m, dragged, Names::Two, true);
        let away = columns_for(room, m, dragged, Names::Two, false);
        let grew = [
            away.artists - open.artists,
            away.albums - open.albums,
            away.tracks - open.tracks,
        ];
        for got in grew {
            assert!(
                (got - strip / 3.0).abs() < 0.5,
                "a column gained {got:.1} where every one should gain {:.1}",
                strip / 3.0
            );
        }
        assert!(
            (grew.iter().sum::<f32>() - strip).abs() < 0.5,
            "the queue's strip did not all go somewhere"
        );
        assert_eq!(away.queue, 0.0);

        let there_and_back = columns_for(room, m, dragged, Names::Two, true);
        assert_eq!(there_and_back, columns_for(room, m, dragged, Names::Two, true));
        let open = columns_for(room, m, dragged, Names::Two, true);
        assert!((open.artists / room - 0.22).abs() < 0.01, "the share drifted");
    }

    /// Without the side the split row is the row as it always was, all the
    /// way through the slide — so turning the switch on changes nothing
    /// until something opens on the right.
    #[test]
    fn without_the_side_the_row_is_as_it_was() {
        let dragged = Shares {
            artists: Some(0.22),
            queue: Some(0.2),
            ..Shares::default()
        };
        let room = 1600.0_f32;
        let m = Metrics::for_size(room + 80.0, 900.0);
        for t in [0.0_f32, 0.3, 0.7, 1.0] {
            let old = Columns::between(
                columns_for(room, m, dragged, Names::One, false),
                columns_for(room, m, dragged, Names::One, true),
                t,
            );
            let new = columns_with_side(room, m, dragged, Names::One, t, 0.0);
            for (a, b) in [
                (old.artists, new.artists),
                (old.tracks, new.tracks),
                (old.queue, new.queue),
            ] {
                assert!((a - b).abs() < 0.01, "at {t}: {a} became {b}");
            }
            assert_eq!(new.side, 0.0);
        }
    }

    /// The example: `w`, then `a`, then `w` again. Three columns, four,
    /// three — and the side alone stands where the fourth column stood.
    #[test]
    fn the_side_opens_beside_the_queue_and_stands_alone_without_it() {
        let dragged = Shares {
            artists: Some(0.2),
            queue: Some(0.18),
            side: Some(0.22),
            ..Shares::default()
        };
        let room = 1600.0_f32;
        let m = Metrics::for_size(room + 80.0, 900.0);
        let gap = m.column_gap;
        let sum = |c: Columns, left: f32, right: f32| {
            c.artists + gap + c.tracks + (gap + c.queue) * left + (gap + c.side) * right
        };

        let queue = columns_with_side(room, m, dragged, Names::One, 1.0, 0.0);
        let both = columns_with_side(room, m, dragged, Names::One, 1.0, 1.0);
        let side = columns_with_side(room, m, dragged, Names::One, 0.0, 1.0);

        assert!((sum(queue, 1.0, 0.0) - room).abs() < 0.5);
        assert!((sum(both, 1.0, 1.0) - room).abs() < 0.5);
        assert!((sum(side, 0.0, 1.0) - room).abs() < 0.5);

        assert!((both.artists - queue.artists).abs() < 0.01);
        assert!((both.queue - queue.queue).abs() < 0.01);
        assert!((both.side - 0.22 * room).abs() < 0.5);
        assert!(both.tracks >= MIN_TRACK_COLUMN);

        assert_eq!(side.queue, 0.0);
        assert!((side.side - 0.22 * room).abs() < 0.5);
        assert!((side.artists - queue.artists).abs() < 0.01);
    }

    /// The combined view, left alone: half each with two columns, a third with
    /// three, a quarter with four — and a column sliding in takes its room from
    /// all of them alike, the row adding up all the way.
    #[test]
    fn the_combined_view_stays_even() {
        let even = Shares {
            artists: Some(0.12),
            queue: Some(0.3),
            ..Shares::default()
        };
        let even = Shares { even: true, ..even };
        let room = 1600.0_f32;
        let m = Metrics::for_size(room + 80.0, 900.0);
        let gap = m.column_gap;
        for (left, right, count) in [(0.0, 0.0, 2.0), (1.0, 0.0, 3.0), (0.0, 1.0, 3.0), (1.0, 1.0, 4.0)] {
            let c = columns_with_side(room, m, even, Names::One, left, right);
            let each = (room - gap * (count - 1.0)) / count;
            assert!((c.artists - each).abs() < 0.01, "{count}: {c:?}");
            assert!((c.tracks - each).abs() < 0.01, "{count}: {c:?}");
            assert!((c.queue - each * left).abs() < 0.01, "{count}: {c:?}");
            assert!((c.side - each * right).abs() < 0.01, "{count}: {c:?}");
        }
        for t in [0.2_f32, 0.5, 0.8] {
            let c = columns_with_side(room, m, even, Names::One, t, 1.0);
            let sum = c.artists + c.tracks + c.queue + c.side + gap * (1.0 + t + 1.0);
            assert!((sum - room).abs() < 0.5, "at {t} the row is {sum}");
            assert!((c.artists - c.tracks).abs() < 0.01);
        }
        let two = columns_with_side(room, m, even, Names::Two, 1.0, 0.0);
        assert!((two.artists - 0.12 * room).abs() < 0.5, "{two:?}");
    }

    /// With both open in a window only just wide enough, the track list keeps
    /// its floor, and the side is the one that gives way.
    #[test]
    fn the_side_never_leaves_the_track_list_without_its_floor() {
        let wide = Shares {
            artists: Some(0.3),
            queue: Some(0.3),
            side: Some(0.4),
            ..Shares::default()
        };
        let room = SIDE_FROM - 80.0;
        let m = Metrics::for_size(SIDE_FROM, 900.0);
        let both = columns_with_side(room, m, wide, Names::One, 1.0, 1.0);
        assert!(both.tracks >= MIN_TRACK_COLUMN - 0.5, "{both:?}");
        assert!(both.side >= MIN_NAME_COLUMN - 0.5, "{both:?}");
    }

    /// The thing shares exist for: a window made smaller takes a little off
    /// every column, instead of taking the whole loss out of the one column
    /// that lives on the remainder.
    ///
    /// The report was that everything "slid into one another" on a smaller
    /// window; what was really happening is the line marked below.
    #[test]
    fn a_smaller_window_shrinks_every_column() {
        let dragged = Shares {
            artists: Some(0.22),
            albums: Some(0.26),
            queue: Some(0.20),
            side: None,
            even: false,
        };

        let mut before: Option<Columns> = None;
        for room in [1400.0_f32, 1200.0, 1000.0] {
            let m = Metrics::for_size(room + 80.0, 900.0);
            let now = columns_for(room, m, dragged, Names::Two, true);

            assert!(
                now.tracks >= MIN_TRACK_COLUMN,
                "at {room}: the track list was squeezed to {:.0}",
                now.tracks
            );
            let crowded = now.tracks <= MIN_TRACK_COLUMN + 1.0;
            if !crowded {
                for (share, got) in
                    [(0.22, now.artists), (0.26, now.albums), (0.20, now.queue)]
                {
                    let held = got / room;
                    assert!(
                        (held - share).abs() < 0.02,
                        "at {room}: asked for {share}, got {held:.3}"
                    );
                }
            }
            if let Some(was) = before {
                assert!(
                    now.artists < was.artists && now.queue < was.queue,
                    "at {room}: a column held its size while the window shrank"
                );
            }
            before = Some(now);
        }
    }

    /// The row adds up: nothing overlaps, nothing is left unaccounted for.
    #[test]
    fn the_columns_and_the_gaps_are_the_whole_row() {
        for layout in [Names::Two, Names::One] {
            for queue in [false, true] {
                let room = 1363.0;
                let m = Metrics::for_size(1427.0, 900.0);
                let c = columns_for(room, m, Shares::default(), layout, queue);
                let names = if layout == Names::One {
                    c.artists
                } else {
                    c.artists + m.column_gap + c.albums
                };
                let gaps = m.column_gap * if queue { 2.0 } else { 1.0 };
                let total = names + c.tracks + c.queue + gaps;
                assert!(
                    (total - room).abs() < 0.5,
                    "{layout:?}, queue {queue}: {total:.1} of {room:.1}"
                );
                assert_eq!(queue, c.queue > 0.0, "the queue took room it has no use for");
            }
        }
    }

    /// A scroll bar that reserves width steals it from the track list, and
    /// only on a list long enough to scroll — which is why a small test
    /// library never caught it and a column of several thousand did. The
    /// gap between two columns is `column_gap` and nothing else.
    #[test]
    fn scrolling_takes_up_no_room() {
        assert_eq!(
            scroll_style().allocated_width(),
            0.0,
            "a reserved scroll bar makes the column it is in wider than it asked to be"
        );
    }

    /// An even split really is even: the three columns and the two gaps add up
    /// to the row, and nothing is left over.
    #[test]
    fn an_even_split_spends_the_whole_row() {
        let gap = 48.0;
        let room = 1440.0;

        let third = even_columns(3, room, gap);
        assert_eq!(third * 3.0 + gap * 2.0, room);

        let half = even_columns(2, room, gap);
        assert_eq!(half * 2.0 + gap, room);
        assert!(half > third);

        assert_eq!(even_columns(3, 200.0, gap), MIN_NAME_COLUMN);
    }

    /// A narrow window has to give the track list room to exist, and it can
    /// only come from the two columns whose content has a natural length.
    #[test]
    fn name_columns_give_up_width_before_the_track_list_does() {
        let wide = Metrics::for_size(1440.0, 900.0);
        let narrow = Metrics::for_size(820.0, 900.0);

        assert!(narrow.artists_width < wide.artists_width);
        assert!(narrow.albums_width < wide.albums_width);
        assert!(narrow.column_gap < wide.column_gap);
        assert!(narrow.margin_x < wide.margin_x);

        let left_over = |m: Metrics, window: f32| {
            window - m.margin_x * 2.0
                - m.artists_width
                - m.albums_width
                - m.column_gap * 2.0
        };
        assert!(left_over(narrow, 820.0) > 200.0, "the track list is squeezed out");
        assert_eq!(Metrics::for_size(400.0, 900.0), narrow);
        assert_eq!(Metrics::for_size(2400.0, 900.0), wide);

        assert_eq!(narrow.row, wide.row);
        assert_eq!(
            Metrics::for_size(1440.0, 420.0).artists_width,
            wide.artists_width
        );
    }

    #[test]
    fn rows_tighten_as_the_window_loses_height() {
        let roomy = Metrics::for_height(900.0);
        let cramped = Metrics::for_height(420.0);
        assert!(cramped.row < roomy.row);
        assert_eq!(roomy.row, ROW_ROOMY);
        assert_eq!(cramped.row, ROW_TIGHT);
        assert_eq!(Metrics::for_height(1.0).row, ROW_TIGHT);
        assert_eq!(Metrics::for_height(4000.0).row, ROW_ROOMY);
        assert!(cramped.marker_inset_y >= 1.0);
        assert!(cramped.row - cramped.marker_inset_y * 2.0 >= SIZE_CONTENT);
    }

    /// A .ttc holds several faces and egui takes a file whole; handing it one
    /// panics. Four bytes are cheaper than a crash on someone's font folder.
    #[test]
    fn font_collections_are_refused() {
        let dir = std::env::temp_dir().join("kessler-font-test");
        std::fs::create_dir_all(&dir).unwrap();

        let collection = dir.join("collection.ttc");
        std::fs::write(&collection, b"ttcf\x00\x01\x00\x00").unwrap();
        assert!(!is_single_face(&collection));

        let truetype = dir.join("single.ttf");
        std::fs::write(&truetype, [0x00, 0x01, 0x00, 0x00, 0x00, 0x00]).unwrap();
        assert!(is_single_face(&truetype));

        let opentype = dir.join("single.otf");
        std::fs::write(&opentype, b"OTTO\x00\x00").unwrap();
        assert!(is_single_face(&opentype));

        let nonsense = dir.join("readme.txt");
        std::fs::write(&nonsense, b"not a font at all").unwrap();
        assert!(!is_single_face(&nonsense));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_bold_face_is_reported_not_hidden() {
        assert!(FontSource::Pair("a".into(), "b".into()).has_bold());
        assert!(!FontSource::RegularOnly("a".into()).has_bold());
        assert!(!FontSource::BuiltIn.has_bold());
        assert!(FontSource::BuiltIn.describe().contains("no bold"));
    }

    /// The fallback goes *after* the chosen faces, never before them: in front
    /// it would decide the look of every Latin letter in the window.
    #[test]
    fn a_nudged_fallback_stands_on_the_family_line() {
        let egui_baseline = |first: EmMetrics, own: EmMetrics| {
            own.ascent + 0.5 * (first.row - own.row) + baseline_nudge(first, own)
        };
        let regular = EmMetrics { ascent: 1.0, row: 1.2, cap: None };
        let tall_bold = EmMetrics { ascent: 1.25, row: 1.6, cap: None };
        let cjk = EmMetrics { ascent: 1.16, row: 1.448, cap: None };
        for first in [regular, tall_bold] {
            assert!((egui_baseline(first, cjk) - first.ascent).abs() < 1e-6);
        }
        assert_eq!(baseline_nudge(regular, regular), 0.0);
    }

    /// The Japanese face is set smaller against Asap, so that its kanji
    /// stand a little over Asap's capitals rather than well above them.
    #[test]
    fn kanji_are_sized_to_the_faces_capitals() {
        let asap = std::fs::read("fonts/Asap/Asap-Variable.ttf").unwrap();
        let mplus = std::fs::read("fonts/M PLUS 1 Code/MPLUS1Code-Variable.ttf").unwrap();
        let first = em_metrics(&asap, 1.0);
        let scale = fallback_scale(first, &mplus);
        assert!(scale > 0.7 && scale < 1.0, "scale {scale}");
        let kanji = ink_height(&mplus, '永').unwrap() * scale;
        let cap = first.unwrap().cap.unwrap();
        assert!((kanji / cap - KANJI_OVER_CAPS).abs() < 0.01, "{kanji} against {cap}");
    }

    #[test]
    fn the_fallback_face_is_last_in_both_families() {
        let dir = std::env::temp_dir().join("kessler-fallback-test");
        std::fs::create_dir_all(&dir).unwrap();
        let face = dir.join("pretend.ttf");
        std::fs::write(&face, b"not really a font, only a path").unwrap();

        let mut fonts = egui::FontDefinitions::default();
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, "regular".into());
        fonts
            .families
            .insert(FontFamily::Name("bold".into()), vec!["bold".into()]);

        let firsts = [
            (FontFamily::Proportional, None),
            (FontFamily::Name("bold".into()), None),
        ];
        let name = append_fallback(&mut fonts, &face, None, &firsts, "").unwrap();
        assert_eq!(name, "pretend");
        for family in [FontFamily::Proportional, FontFamily::Name("bold".into())] {
            let list = &fonts.families[&family];
            assert!(list.last().unwrap().starts_with("fallback"), "{family:?}");
            assert!(list.len() > 1, "{family:?} lost its own face");
        }
        let _ = std::fs::remove_file(&face);
    }

    /// Whatever this machine offers has to be a file that exists and a face
    /// this build can actually load.
    #[test]
    fn a_found_fallback_is_a_real_file() {
        if let Some(path) = fallback_face() {
            assert!(path.is_file(), "{} is not a file", path.display());
            let data = std::fs::read(&path).unwrap();
            assert!(data.len() > 1000, "{} is too small to be a face", path.display());
        }
    }

    /// Dragging a column wider must never push the track list out of the
    /// window, and dragging it narrower must stop while there is still a
    /// column there.
    #[test]
    fn a_dragged_column_leaves_room_for_the_tracks() {
        let total = 1200.0;
        let others = 300.0;

        assert_eq!(fit_column(400.0, others, total), 400.0);
        assert_eq!(
            fit_column(5_000.0, others, total),
            total - others - MIN_TRACK_COLUMN
        );
        assert_eq!(fit_column(0.0, others, total), MIN_NAME_COLUMN);
        assert_eq!(fit_column(-40.0, others, total), MIN_NAME_COLUMN);

        assert_eq!(fit_column(300.0, 300.0, 400.0), MIN_NAME_COLUMN);
    }

    /// A face whose file name does not say which family it is still lands in
    /// the family of the folder it was put in.
    ///
    /// The bug this guards let the swap fail silently: a foundry file called
    /// `XY-Lumen-Standard-Regular.otf` read as a family named
    /// "lumenstandard", so it sat in the folder unused while the window
    /// went on drawing with the face beside it and said nothing about either.
    #[test]
    fn the_folder_decides_the_family_not_the_file_name() {
        let dir = std::env::temp_dir().join("kessler-folder-family");
        let _ = std::fs::remove_dir_all(&dir);
        let source = std::path::Path::new("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf");
        if !source.exists() {
            return;
        }
        let at = dir.join("Lumen");
        std::fs::create_dir_all(&at).unwrap();
        for file in ["XY-Lumen-Standard-Regular.otf", "XY Lumen Bold.ttf"] {
            std::fs::copy(source, at.join(file)).unwrap();
        }

        let families = families_in(&dir);
        assert_eq!(families.len(), 1, "one folder, one family: {families:?}");
        assert_eq!(families[0].key, "lumen", "keyed by the folder");
        let chosen = the_typeface(&families, "").expect("a typeface");
        assert!(
            chosen.regular.ends_with("XY-Lumen-Standard-Regular.otf"),
            "{chosen:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Two alternates of the same figure are rivals; a set of no one character,
    /// or of another, is not.
    #[test]
    fn two_alternates_of_one_figure_are_rivals() {
        let set = |id: &str, letter: Option<char>| StyleSet { id: id.into(), tag: id.into(), letter, swaps: Vec::new(), shows: Vec::new() };
        let offered = [set("ss10", Some('0')), set("zero", Some('0')), set("ss01", Some('g')), set("ss02:*", None)];
        assert_eq!(rivals_of(&offered, "zero"), vec!["ss10".to_string()]);
        assert_eq!(rivals_of(&offered, "ss10"), vec!["zero".to_string()]);
        assert!(rivals_of(&offered, "ss01").is_empty());
        assert!(rivals_of(&offered, "ss02:*").is_empty());
    }

    /// Asap offers its g and its zero, and with them asked for its „g" and
    /// „0" stand for the other drawings — every other letter as it was.
    #[test]
    fn asap_draws_its_alternates_when_asked() {
        use skrifa::MetadataProvider as _;
        let asap = std::fs::read("fonts/Asap/Asap-Variable.ttf").unwrap();
        let sets = style_sets(&asap);
        let ids: Vec<&str> = sets.iter().map(|s| s.id.as_str()).collect();
        assert!(ids.contains(&"ss01:g") && ids.contains(&"zero:0"), "{ids:?}");
        assert!(sets.iter().find(|s| s.id == "ss01:g").unwrap().shows.contains(&'ğ'));
        assert!(with_alternates(&asap, &Alternates::default(), &[]).is_none(), "nothing asked, nothing changed");

        let both = Alternates { sets: vec!["ss01:g".into(), "zero:0".into()] };
        let (altered, _) = with_alternates(&asap, &both, &[]).expect("a face");
        let glyph = |data: &[u8], c: char| skrifa::FontRef::new(data).unwrap().charmap().map(c).unwrap();
        let names = |data: &[u8], c: char| {
            let font = skrifa::FontRef::new(data).unwrap();
            skrifa::GlyphNames::new(&font).get(font.charmap().map(c).unwrap()).unwrap().as_str().to_string()
        };
        assert_eq!(names(&altered, 'g'), "g.ss01");
        assert_eq!(names(&altered, 'ğ'), "gbreve.ss01");
        assert_eq!(names(&altered, '0'), "zero.zero");
        for c in "abcY19 kessler".chars() {
            assert_eq!(glyph(&altered, c), glyph(&asap, c), "{c}");
        }
        assert!(axis_range(&altered, b"wght").is_some() && axis_range(&altered, b"wdth").is_some());

        let only_g = with_alternates(&asap, &Alternates { sets: vec!["ss01:g".into()] }, &[]).unwrap().0;
        assert_eq!(names(&only_g, '0'), "zero");
    }

    /// Muto's sets, where this copy has it: the open a under `ss01`, the
    /// other g under `ss02`, each letter of `ss04` on its own.
    #[test]
    fn muto_offers_its_sets() {
        use skrifa::MetadataProvider as _;
        let Ok(muto) = std::fs::read("fonts/Muto/MutoRegular.otf") else {
            return;
        };
        let sets = style_sets(&muto);
        let ids: Vec<&str> = sets.iter().map(|s| s.id.as_str()).collect();
        for id in ["ss01:a", "ss02:g", "ss03:t", "ss04:A", "ss04:w", "ss05:G", "ss06:J", "ss09:3", "ss13:"] {
            assert!(ids.contains(&id), "{id} in {ids:?}");
        }
        let a_only = with_alternates(&muto, &Alternates { sets: vec!["ss01:a".into()] }, &[]).unwrap().0;
        let glyph = |data: &[u8], c: char| skrifa::FontRef::new(data).unwrap().charmap().map(c).unwrap();
        assert_ne!(glyph(&a_only, 'a'), glyph(&muto, 'a'));
        assert_eq!(glyph(&a_only, 'g'), glyph(&muto, 'g'));
    }

    /// Muto's dotted zero, `ss10` on its slashed one, is offered as a swap of 0
    /// itself, and wins over the slashed one where both are on; its labels have
    /// its open a whatever else is on.
    #[test]
    fn muto_offers_its_dotted_zero() {
        use skrifa::MetadataProvider as _;
        let Ok(muto) = std::fs::read("fonts/Muto/MutoRegular.otf") else {
            return;
        };
        let ids: Vec<String> = style_sets(&muto).into_iter().map(|s| s.id).collect();
        for id in ["ss10:0", "zero:0"] {
            assert!(ids.iter().any(|i| i == id), "{id} in {ids:?}");
        }
        let face = skrifa::FontRef::new(&muto).unwrap();
        let names = skrifa::GlyphNames::new(&face);
        let named = |data: &[u8], c: char| {
            let glyph = skrifa::FontRef::new(data).unwrap().charmap().map(c).unwrap();
            names.get(glyph).unwrap().as_str().to_string()
        };
        let both = with_alternates(&muto, &Alternates { sets: vec!["zero:0".into(), "ss10:0".into()] }, &[]).unwrap().0;
        assert_eq!(named(&both, '0'), "zero.zero.ss10");
        let (labelled, forms) = with_alternates(&muto, &Alternates::default(), &label_sets_for("muto")).unwrap();
        let open = forms.iter().find(|(c, _)| *c == 'a').expect("a label form for a").1;
        assert_eq!(named(&labelled, open), "a.ss01");
        assert_eq!(named(&labelled, 'a'), "a");
    }

    /// The bold stays its step above the regular wherever the slider puts it,
    /// and inside the axis.
    #[test]
    fn the_bold_follows_the_weight() {
        let asap = std::fs::read("fonts/Asap/Asap-Variable.ttf").unwrap();
        let (least, most) = axis_range(&asap, b"wght").unwrap();
        assert_eq!(weights_at(&asap, 300.0), Some((300.0, 450.0)));
        assert_eq!(weights_at(&asap, 400.0), Some((400.0, 550.0)));
        assert_eq!(weights_at(&asap, most), Some((most - BOLD_STEP, most)));
        assert_eq!(weights_at(&asap, least - 50.0).unwrap().0, least);
    }

    /// The machine's own faces come after the folder's, once each, upright,
    /// with the bold beside the regular where the family has one — DejaVu,
    /// where the machine has it, the way the test above uses it.
    #[test]
    fn a_condensed_cut_is_not_the_regular() {
        assert!(regular_rank("DejaVuSans") < regular_rank("DejaVuSansCondensed"));
        assert!(regular_rank("Asap-Regular") < regular_rank("Asap-Condensed-Regular"));
    }

    #[test]
    fn system_faces_follow_the_folders_once_each() {
        let families = all_families(Some("fonts"), true);
        let mut keys: Vec<&str> = families.iter().map(|f| f.key.as_str()).collect();
        assert_eq!(keys.first(), Some(&"asap"), "the folder's come first");
        let count = keys.len();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), count, "no family twice");
        assert!(!families.iter().any(|f| f.key == "mplus1code"));
        assert!(!families.iter().any(|f| f.key == "d050000l" || f.key == "notocoloremoji"));

        if let Some(dejavu) = families.iter().find(|f| f.key == "dejavusans") {
            let faces = dejavu.proportional.as_ref().unwrap();
            assert!(faces.regular.ends_with("DejaVuSans.ttf"), "{faces:?}");
            assert!(faces.bold.as_ref().unwrap().ends_with("DejaVuSans-Bold.ttf"));
        }
        let without = all_families(Some("fonts"), false);
        assert!(without.len() <= families.len());
        assert!(
            without
                .iter()
                .filter(|f| !SYSTEM_OFFERED.contains(&f.key.as_str()) && f.key != SYSTEM_UI)
                .all(|f| f.faces(false).unwrap().regular.starts_with("fonts")),
            "without them, only the folder's (and muto where installed, and the system's interface face)"
        );
    }

    /// A real folder: two families, each with cuts, named
    /// the way their foundries name them. What the reader has to come out with
    /// is two families, four cuts, and the right file in each slot.
    #[test]
    fn the_fonts_folder_reads_as_two_families() {
        let dir = std::env::temp_dir().join("kessler-families-test");
        let _ = std::fs::remove_dir_all(&dir);
        for (folder, file) in [
            ("Lumen", "XY Lumen Regular.otf"),
            ("Lumen", "XY Lumen Bold.ttf"),
            ("Lumen", "XY Lumen Light.ttf"),
            ("Lumen", "XY Lumen Mono.ttf"),
            ("Kanto", "KantoRegular.otf"),
            ("Kanto", "KantoMedium.otf"),
            ("Kanto", "KantoBold.otf"),
            ("Kanto", "KantoMonoRegular.otf"),
            ("Kanto", "KantoMonoMedium.otf"),
            ("Kanto", "KantoMonoBold.otf"),
        ] {
            let at = dir.join(folder);
            std::fs::create_dir_all(&at).unwrap();
            let source = std::path::Path::new("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf");
            if !source.exists() {
                return;
            }
            std::fs::copy(source, at.join(file)).unwrap();
        }

        let families = families_in(&dir);
        let names: Vec<&str> = families.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["kanto", "lumen"], "one entry per family");

        let kanto = &families[0];
        let lumen = &families[1];

        let prop = kanto.proportional.as_ref().expect("kanto proportional");
        assert!(prop.regular.ends_with("KantoRegular.otf"), "{prop:?}");
        assert!(prop.bold.as_ref().unwrap().ends_with("KantoBold.otf"));
        let mono = kanto.mono.as_ref().expect("kanto mono");
        assert!(mono.regular.ends_with("KantoMonoRegular.otf"), "{mono:?}");
        assert!(mono.bold.as_ref().unwrap().ends_with("KantoMonoBold.otf"));

        let prop = lumen.proportional.as_ref().expect("lumen proportional");
        assert!(prop.regular.ends_with("XY Lumen Regular.otf"), "{prop:?}");
        assert!(prop.bold.as_ref().unwrap().ends_with("XY Lumen Bold.ttf"));
        let mono = lumen.mono.as_ref().expect("lumen mono");
        assert!(mono.regular.ends_with("XY Lumen Mono.ttf"), "{mono:?}");
        assert!(mono.bold.is_none(), "there is no mono bold in the folder");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Only Asap and Muto out of a folder that has another face in it too;
    /// a folder with none of them keeps what it has.
    #[test]
    fn the_menu_offers_its_own_faces() {
        let family = |key: &str| Family {
            name: key.into(),
            key: key.into(),
            proportional: Some(Faces { regular: format!("/a/{key}.ttf").into(), bold: None }),
            mono: None,
        };
        let keys = |families: Vec<Family>| families.into_iter().map(|f| f.key).collect::<Vec<_>>();
        let folder = vec![family("asap"), family("junicode"), family("lumen"), family("muto")];
        assert_eq!(keys(offered(folder)), vec!["asap", "muto"]);
        assert_eq!(keys(offered(vec![family("lumen")])), vec!["lumen"]);
    }

    /// The named typeface wins; anything else in the folder is the fallback,
    /// and an empty folder means egui's own face.
    #[test]
    fn the_named_typeface_is_the_one_installed() {
        let lumen = Family {
            name: "asap".into(),
            key: TYPEFACE.into(),
            proportional: Some(Faces {
                regular: "/a/XY Lumen Regular.otf".into(),
                bold: Some("/a/XY Lumen Bold.ttf".into()),
            }),
            mono: Some(Faces {
                regular: "/a/XY Lumen Mono.ttf".into(),
                bold: None,
            }),
        };
        let kanto = Family {
            name: "kanto".into(),
            key: "kanto".into(),
            proportional: Some(Faces {
                regular: "/a/KantoRegular.otf".into(),
                bold: None,
            }),
            mono: None,
        };

        let chosen = the_typeface(&[kanto.clone(), lumen.clone()], "").unwrap();
        assert_eq!(chosen.name, "asap");
        assert!(chosen.regular.ends_with("XY Lumen Regular.otf"));
        assert!(chosen.bold.unwrap().ends_with("XY Lumen Bold.ttf"));
        assert!(
            chosen.figures.unwrap().ends_with("XY Lumen Mono.ttf"),
            "the figures are set in the mono cut"
        );

        let chosen = the_typeface(std::slice::from_ref(&kanto), "").unwrap();
        assert_eq!(chosen.name, "kanto");
        assert!(
            chosen.figures.unwrap().ends_with("KantoRegular.otf"),
            "with no mono cut the figures are set like everything else"
        );

        assert!(the_typeface(&[], "").is_none());
        let both = [kanto.clone(), lumen.clone()];
        assert_eq!(the_typeface(&both, "kanto").unwrap().name, "kanto");
        assert_eq!(the_typeface(&both, TYPEFACE).unwrap().name, "asap");
        assert_eq!(the_typeface(&both, "helvetica").unwrap().name, "asap");
    }
}
