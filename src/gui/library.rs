//! The library browser: three columns, no rules between them.
//!
//! Every row is drawn by hand rather than with egui's widgets, because the
//! widgets bring exactly what this design removes — fills, strokes, rounding,
//! hover plates. What is left is text on a ground, plus one rule.

use eframe::egui::{self, Color32, Rect, Response, Sense, Ui, Vec2};

use crate::db::models::{SortField, SortOrder};

use super::theme::{self, Metrics, Palette, Role, FADE_HEIGHT, MARKER_RADIUS, SCROLL_MARGIN_ROWS};

use super::columns::TrackColumn;

use super::{menu, text, About, ArtistRow, Column, GuiApp, PINNED};

mod rows;
mod masthead;
mod controls;
mod names;
mod tracks;
mod forms;
mod device_col;
mod fourth;
mod artist_col;
mod lyrics_col;
mod eq_col;
mod gaps_col;
mod radio_col;
mod switch;
mod picture;
mod field_art;
mod rowmenu;
mod meter;
mod podcast_col;

use rows::*;
use masthead::*;
use controls::*;
use names::*;
use tracks::*;
use forms::*;
use device_col::*;
use fourth::*;
use artist_col::*;
use lyrics_col::*;
use eq_col::*;
use gaps_col::*;
use radio_col::*;
pub(in crate::gui) use radio_col::{forget_decoded_pictures, heard_follow_id};
use switch::*;
use picture::*;
use rowmenu::*;
use podcast_col::*;
pub use podcast_col::step_podcast_strip;
pub(in crate::gui) use meter::{meter_rate, Meter, METER_RATES};
pub(in crate::gui) use meter::MeterDots;

pub(super) use rows::{DeviceMark, DEVICE_MARKS};
pub(in crate::gui) use rows::cursor_row;
pub(super) use rows::{damp_scrolling, lead_arrow, note_below, paint_extent, paint_fade, without_a_bar, ARROW_SECS};
pub(super) use masthead::PlayingAt;
pub(super) use controls::{control_plate, paint_control_hover, PLAY_WORDS};
pub(super) use switch::paint_underline;
pub(super) use field_art::{field_rate, field_tick, grey_for as field_grey, on_the_second, FieldArt, FieldPicture, Reading, FIELD_RATES};
pub(super) use picture::{live_mark, picture_step, scope_step, to_tick, EXPOSED_RATE, SECOND_RATE};

/// Space between two cells of the track list. The widths themselves belong to
/// the columns, in `columns.rs`.
const CELL_GAP: f32 = 20.0;

/// How long the volume rule is.
const VOLUME_WIDTH: f32 = 92.0;

/// The air between a column's head and the first row under it.
///
/// A head and a row were one row apart like any two rows, so the head read as
/// the first entry of the list rather than as its name. Six points is not a
/// gap you notice; it is a gap that stops you having to work out which line is
/// the label.
pub(super) const HEAD_AIR: f32 = 6.0;

/// The air that separates one block of rows in a column from the next.
///
/// Half a row: enough that the lists read as their own block between the two
/// pinned rows and the names, and not so much that the column looks broken in
/// two. This design separates with whitespace or with nothing — there is no
/// rule to draw here, and a heading over three rows would cost more lines than
/// it saves.
pub(super) const BLOCK_AIR: f32 = 8.0;

/// The corner a rule's ends are cut with: half its own thickness.
///
/// Which is the most a two-point bar can be given — the end becomes a
/// half-circle and no more, so the rounding is not a number somebody picked
/// but the only one that fits. It follows the rule when it
/// thickens under the pointer, because a bar that gets taller and keeps its
/// old corner starts to look square again.
///
/// Square with a face whose ends are square — see
/// [`super::theme::square_ends`].
fn capped(height: f32) -> u8 {
    if super::theme::square_ends() {
        return 0;
    }
    (height / 2.0).round().clamp(0.0, 255.0) as u8
}

/// How much a square-ended rule's corners are broken: half a point, a pixel
/// on a sharp screen — not rounded, only less hard.
const RULE_CORNER_CUT: f32 = 0.5;

/// A rule — the timeline, the volume, the knob on them, a list's extent
/// mark — and what of it is filled: its ends
/// cut as [`capped`] says, and with a square-ended face its corners broken by
/// [`RULE_CORNER_CUT`] rather than left sharp.
fn paint_rule(ui: &Ui, rect: Rect, colour: Color32) {
    if !super::theme::square_ends() {
        ui.painter().rect_filled(rect, capped(rect.height()), colour);
        return;
    }
    let cut = RULE_CORNER_CUT.min(rect.height() / 4.0).min(rect.width() / 4.0);
    if cut <= 0.0 {
        ui.painter().rect_filled(rect, 0, colour);
        return;
    }
    let (l, r, t, b) = (rect.left(), rect.right(), rect.top(), rect.bottom());
    let points = vec![
        egui::pos2(l + cut, t),
        egui::pos2(r - cut, t),
        egui::pos2(r, t + cut),
        egui::pos2(r, b - cut),
        egui::pos2(r - cut, b),
        egui::pos2(l + cut, b),
        egui::pos2(l, b - cut),
        egui::pos2(l, t + cut),
    ];
    ui.painter().add(egui::Shape::convex_polygon(points, colour, egui::Stroke::NONE));
}

/// A knob: a disc, or with a square-ended face an upright bar.
///
/// The bar keeps its width all the way and only its height swells, from
/// the line's own `line` thickness to [`BAR_TALL`]: it slides out of the
/// rule above and below, rather than growing out of a point.
fn knob_at(ui: &Ui, at: egui::Pos2, radius: f32, line: f32, colour: Color32) {
    if super::theme::square_ends() {
        let rest = line / 2.0;
        let t = ((radius - rest) / (KNOB - rest).max(0.01)).clamp(0.0, 1.0);
        let tall = line + (BAR_TALL - line) * t;
        paint_rule(ui, Rect::from_center_size(at, Vec2::new(BAR_WIDE, tall)), colour);
    } else {
        ui.painter().circle_filled(at, radius, colour);
    }
}

/// The knob that appears under the pointer on a rule, so that there is
/// something to take hold of rather than a hair to hit.
///
/// One size for both rules — the volume and the timeline are the same control
/// twice over, and a reader who has learnt one has learnt the other.
const KNOB: f32 = 5.0;

/// The bar a square-ended face has for a knob: narrow, and a little taller
/// than the disc it stands for.
const BAR_WIDE: f32 = 3.5;

const BAR_TALL: f32 = 12.0;

/// How thick a rule is when nobody is pointing at it, and when somebody is.
const RULE_THIN: f32 = 2.0;

const RULE_THICK: f32 = 4.0;

/// How long a rule takes to swell under the pointer, and to go down again.
///
/// Short enough to feel like an answer rather than an animation: much past a
/// sixth of a second and the line is still arriving after the eye has moved
/// on, much under and it may as well have jumped.
const SWELL_TIME: f32 = 0.13;

/// How long the knob takes: a little longer than its rule, so it is seen
/// sliding out of the line after the line has begun to swell rather than in
/// the same breath.
const KNOB_SWELL_TIME: f32 = 0.17;

/// The knob's size on a rule, on its own slightly slower clock — see
/// [`KNOB_SWELL_TIME`].
fn knob_swell(ui: &Ui, which: &'static str, hot: bool) -> f32 {
    let t = ui.ctx().animate_bool_with_time_and_easing(
        egui::Id::unique(("knob-swell", which)),
        hot,
        KNOB_SWELL_TIME,
        egui::emath::easing::cubic_out,
    );
    rule_swell(t).1
}

/// How far a rule is out of its resting state, 0 … 1.
///
/// `cubic_out` because this is a reply to a pointer: it has to leave at once
/// and may take its time settling. egui turns the curve round on the way back,
/// so the line sinks the way it rose.
fn swell(ui: &Ui, which: &'static str, hot: bool) -> f32 {
    ui.ctx().animate_bool_with_time_and_easing(
        egui::Id::unique(("swell", which)),
        hot,
        SWELL_TIME,
        egui::emath::easing::cubic_out,
    )
}

/// What a rule looks like that far out: how thick the line is, and how big the
/// knob on it is.
///
/// The two are one movement, which is the whole trick. The knob does not
/// appear on the line — at rest it *is* the line, a disc exactly as wide as
/// the line is thick and therefore invisible in it, and it swells out of it as
/// the line thickens. A snake that has swallowed an egg — that is exactly the
/// silhouette: one body, one bulge, no second object arriving
/// out of nowhere.
fn rule_swell(t: f32) -> (f32, f32) {
    let thickness = RULE_THIN + (RULE_THICK - RULE_THIN) * t;
    let knob = thickness / 2.0 + (KNOB - thickness / 2.0) * t;
    (thickness, knob)
}

/// How narrow the search field may become, and how tall.
///
/// How *wide* it is is no longer written down: it is as wide as the controls
/// at the other end of the masthead, so that the two ends weigh the same. See
/// `masthead`.
pub(in crate::gui) const MIN_FIELD: f32 = 118.0;

/// How tall the search field is: 28 points, and 22 on the line „super kompakt"
/// sets in the band beside the window's buttons. Set once a frame — see
/// `render`.
pub(in crate::gui) fn field_height() -> f32 {
    f32::from_bits(FIELD_HEIGHT_NOW.load(std::sync::atomic::Ordering::Relaxed))
}

const FIELD_HEIGHT: f32 = 28.0;

const FIELD_HEIGHT_FLAT: f32 = 22.0;

static FIELD_HEIGHT_NOW: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x41E0_0000);

fn set_field_height(height: f32) {
    FIELD_HEIGHT_NOW.store(height.to_bits(), std::sync::atomic::Ordering::Relaxed);
}

pub(in crate::gui) const FIELD_PAD_X: f32 = 10.0;

/// How far a soft edge reaches when a line of text runs past its cell.
const EDGE_FADE: f32 = 22.0;

/// How big a transport key in the corner is drawn. Here rather than beside
/// it, because the layout has to leave room for the keys before they are
/// drawn.
const KEY_SIZE: f32 = 22.0;

/// What a row says when the file behind it is no longer on the disk.
///
/// One word, in the lower case everything else here is in, and the same word
/// wherever it appears — the track column and the queue — so it reads as a
/// state of the row rather than as two different messages.
const GONE: &str = "fehlt";

/// How a row is currently standing.
#[derive(Debug, Clone, Copy, Default)]
struct RowState {
    selected: bool,
    /// Selected *and* this column has the keyboard.
    focused: bool,
    playing: bool,
    /// The "all" row and other rows that are present but not the point.
    muted: bool,
    /// The file behind this row was looked for and was not there.
    gone: bool,
    /// Already played: a queue row behind the needle. Quieter than muted —
    /// still there to be read, but the least thing in its column.
    spent: bool,
    /// The figure at the end is a word that does something — „merken",
    /// „entfernen" — set in the ink whatever the row's state, as every such
    /// word is.
    offering: bool,
    /// The arrow before the name though nothing of it plays: „alle",
    /// „zuletzt" and „kompilationen" while they are the view chosen, as that
    /// is what the columns are showing.
    arrow: bool,
    /// Pointed at from elsewhere, and so wearing the hover plate though the
    /// pointer is not on it: a chapter while the hand is on its stretch of the
    /// timeline.
    pointed: bool,
    /// Nothing to do on it: no plate under the pointer, which would offer a
    /// click that does nothing — a show in a station's programme.
    still: bool,
}

impl RowState {
    /// Whether the row wears its marker: only where the hand is, the chosen row
    /// of the column with the keyboard. The plates say where the hand is — the
    /// light one under the pointer, the darker one under the keyboard and the
    /// click — and the weight says what is chosen. Until then the other
    /// columns' chosen rows kept a fainter marker of their own, and a third
    /// plate said much the same as the other two.
    fn plated(self) -> bool {
        self.selected && self.focused
    }

    /// Weight says what is chosen — the artist whose albums are showing, the
    /// album whose tracks are, the track playing. Until then a plate set it
    /// regular, as saying it already; in a face whose weights are not one
    /// width, Muto's, the name grew and shrank as the hand went over it.
    ///
    /// It applies to the name only — the artist, the album, the title. Setting
    /// a track number or a running time in bold does not make the row easier to
    /// find, it just makes the column of figures look uneven.
    fn role(self) -> Role {
        if self.selected || self.playing {
            Role::ContentBold
        } else {
            Role::Content
        }
    }

    fn primary(self, p: &Palette) -> Color32 {
        let tone = self.primary_tone(p);
        if self.plated() {
            Palette::legible_on(tone, p.marked(), crate::gui::theme::ON_PLATE_LEAST)
        } else {
            tone
        }
    }

    fn primary_tone(self, p: &Palette) -> Color32 {
        if self.gone {
            p.mid
        } else if self.playing {
            p.accent
        } else if self.selected && !self.muted {
            p.ink
        } else if self.selected {
            p.mid
        } else if self.spent {
            p.faint
        } else if self.muted {
            p.mid
        } else {
            p.ink
        }
    }

    /// Marked, the playing row's figures — arrow, number, time — are the marked
    /// rows' own colour, and only its title says it plays: three colours in one
    /// block of marked rows read as three kinds of row. Unmarked, the receded
    /// accent finds it in the column.
    ///
    /// Marked means plated, the keyboard's marker behind it — not merely
    /// chosen: a chosen row in a column without the keyboard wears no plate,
    /// and the playing one there would have its artist in the queue in the
    /// second tone beside a title in the accent.
    fn secondary(self, p: &Palette) -> Color32 {
        let tone = self.secondary_tone(p);
        if self.plated() {
            Palette::legible_on(tone, p.marked(), crate::gui::theme::ON_PLATE_LEAST)
        } else {
            tone
        }
    }

    fn secondary_tone(self, p: &Palette) -> Color32 {
        if self.offering {
            p.mid
        } else if self.gone {
            p.faint
        } else if self.playing {
            p.accent_faint
        } else if (self.selected && !self.plated()) || self.muted {
            p.mid
        } else {
            p.faint
        }
    }
}

/// What is actually painted behind a row's text.
///
/// The marquee needs to know, because the soft edge it fades text into has to
/// be the same colour as whatever the row is sitting on.
fn behind(ui: &Ui, state: RowState, hovered: bool, palette: &Palette) -> Color32 {
    if state.plated() {
        marker_colour(palette)
    } else if hovered {
        hover_colour(ui, palette)
    } else {
        palette.ground
    }
}

/// How far one notch of a mouse wheel scrolls, in points.
///
/// A wheel says only „one line" and leaves the length of a line to the program;
/// egui makes it 40 points, whatever the rows are. On a Mac that feels right —
/// the system's own scrolling is like it, and a trackpad sends points of its
/// own anyway — but on Linux, where a notch moves about three lines everywhere
/// else, 40 points was not two of kessler's rows and a long list crawled. So
/// there five rows — a browser on the same machine moves about as far a notch,
/// as the rows are tall; on a Mac 40 as before. `[ui] scroll_speed` —
/// „langsam", „normal", „schnell" — takes two thirds of that or two thirds
/// more. A trackpad's points are not touched.
fn wheel_step(speed: &str, row: f32, mac: bool) -> f32 {
    let normal = if mac { 40.0 } else { 5.0 * row };
    let factor = match speed {
        "langsam" => 2.0 / 3.0,
        "schnell" => 5.0 / 3.0,
        _ => 1.0,
    };
    (normal * factor).round()
}

/// How much farther a trackpad's or a TrackPoint's own distances go: on Linux
/// they come as a few points a step, read one to one they crawled where every
/// other program moves twice as far; on a Mac the system's own scrolling
/// already feels right. `[ui] scroll_speed` scales it as it scales the wheel.
pub(in crate::gui) fn touch_factor(speed: &str, mac: bool) -> f32 {
    if mac {
        return 1.0;
    }
    let factor = match speed {
        "langsam" => 2.0 / 3.0,
        "schnell" => 5.0 / 3.0,
        _ => 1.0,
    };
    2.0 * factor
}

fn set_wheel_step(ctx: &egui::Context, speed: &str, row: f32) {
    let step = wheel_step(speed, row, cfg!(target_os = "macos"));
    if ctx.options(|o| o.input_options.line_scroll_speed) != step {
        ctx.options_mut(|o| o.input_options.line_scroll_speed = step);
    }
}

/// How long after the last turn of the wheel a list counts as still being
/// scrolled, and how long the hover then takes to go or to come back, in
/// seconds.
const SCROLL_SETTLES: f64 = 0.1;
const HOVER_FADE: f32 = 0.08;

/// The hover's plate, as strong as it is shown just now: gone while a list
/// is being scrolled, back as soon as it stops.
///
/// Scrolling under a still pointer moved the plate from row to row with every
/// line that passed under it, and the list looked restless. It goes quickly and
/// comes back quickly, so it is there again for the click that follows.
fn hover_colour(ui: &Ui, palette: &Palette) -> Color32 {
    palette.ground.lerp_to_gamma(palette.hover, hover_shown(ui.ctx()))
}

/// Whether the pointer is on `rect` and the list under it stands still:
/// what a row offers under the pointer — „merken", „folgen" — goes with
/// the hover's plate while the list is scrolled, instead of jumping from
/// row to row with every line that passes.
pub(super) fn pointer_rests_on(ui: &Ui, rect: Rect) -> bool {
    ui.rect_contains_pointer(rect) && hover_shown(ui.ctx()) > 0.5
}

pub(super) fn hover_shown(ctx: &egui::Context) -> f32 {
    let last = egui::Id::unique("last-scroll");
    let (now, scrolled) = ctx.input(|i| {
        (
            i.time,
            i.smooth_scroll_delta() != Vec2::ZERO
                || i.raw.events.iter().any(|e| matches!(e, egui::Event::MouseWheel { .. })),
        )
    });
    if scrolled {
        ctx.data_mut(|d| d.insert_temp(last, now));
    }
    let since = ctx.data(|d| d.get_temp::<f64>(last)).map(|at| now - at);
    let still = since.is_none_or(|s| s >= SCROLL_SETTLES);
    if let Some(s) = since.filter(|s| *s < SCROLL_SETTLES) {
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(SCROLL_SETTLES - s));
    }
    ctx.animate_bool_with_time(egui::Id::unique("hover-shown"), still, HOVER_FADE)
}

/// How strong a selected row's marker is.
///
/// All three columns keep their selection visible — you need to see what the
/// track list is a list *of*. But only the column holding the keyboard shows it
/// at full strength; in the others the marker sits back towards the ground.
/// Without that, three identical markers are on the screen at once and none of
/// them says where j and k will go next.
fn marker_colour(palette: &Palette) -> Color32 {
    palette.marked()
}

/// The marker behind a row: the whole row, held clear of the rows above and
/// below.
///
/// The row is allocated wider than its text by [`Metrics::marker_pad_x`] at
/// each side, and the text is inset by that much — so the marker has the same
/// air left and right, and neither end of it is clipped by the scroll area it
/// lives in. Growing the marker past the row instead would put the padding
/// outside the clip on the right, and only on the right.
fn marker_rect(rect: Rect, m: Metrics) -> Rect {
    Rect::from_min_max(
        egui::pos2(rect.left(), rect.top() + m.marker_inset_y),
        egui::pos2(rect.right(), rect.bottom() - m.marker_inset_y),
    )
}

/// Where a row's text goes: the row, minus the marker's air.
fn content_rect(rect: Rect, m: Metrics) -> Rect {
    Rect::from_min_max(
        egui::pos2(rect.left() + m.marker_pad_x, rect.top()),
        egui::pos2(rect.right() - m.marker_pad_x, rect.bottom()),
    )
}

/// Paint the highlighter stroke behind a selected row — coming and going
/// over [`HOVER_FADE`], as quickly as the hover, so a click does not snap
/// the plate in. `id` is the row's own.
fn paint_marker(ui: &Ui, rect: Rect, id: egui::Id, state: RowState, m: Metrics, palette: &Palette) {
    let shown = ui.ctx().animate_bool_with_time(id.with("plate"), state.plated(), HOVER_FADE);
    if shown <= 0.0 {
        return;
    }
    ui.painter()
        .rect_filled(marker_rect(rect, m), theme::ROW_RADIUS, marker_colour(palette).gamma_multiply(shown));
}

/// `to`, reached over [`HOVER_FADE`] from whatever the row with `id` was
/// last drawn in: a name turning to the accent as it starts to play, or to
/// the ink as it is chosen, crosses over rather than snapping. A row drawn
/// for the first time — scrolled into sight — stands in its colour at once.
fn eased_colour(ui: &Ui, id: egui::Id, to: Color32) -> Color32 {
    #[derive(Clone, Copy)]
    struct Fade {
        from: Color32,
        to: Color32,
        since: f64,
    }
    let key = id.with("ink");
    let now = ui.input(|i| i.time);
    let secs = f64::from(HOVER_FADE);
    let at = |f: Fade| {
        let t = ((now - f.since) / secs).clamp(0.0, 1.0) as f32;
        (f.from.lerp_to_gamma(f.to, t), t)
    };
    let fade = ui.data(|d| d.get_temp::<Fade>(key));
    let fade = match fade {
        Some(f) if f.to == to => f,
        Some(f) => Fade { from: at(f).0, to, since: now },
        None => Fade { from: to, to, since: now - secs },
    };
    ui.data_mut(|d| d.insert_temp(key, fade));
    let (shown, t) = at(fade);
    if t < 1.0 {
        ui.ctx().request_repaint();
    }
    shown
}

/// The faintest possible hover: it says "clickable", not "button".
fn paint_hover(
    ui: &Ui,
    rect: Rect,
    response: &Response,
    state: RowState,
    m: Metrics,
    palette: &Palette,
) {
    if ((response.hovered() && !state.still) || state.pointed) && !state.plated() {
        ui.painter()
            .rect_filled(marker_rect(rect, m), theme::ROW_RADIUS, hover_colour(ui, palette));
    }
}

/// The first column: the artists, or the radio's stations while it is open.
/// Everything switched off: in the middle of where the columns would stand,
/// where to switch them on again — a click on it opens the menu there. Not the
/// keys that do it at once (`1` `1` …): to somebody new to the window they said
/// nothing. How much of the room's height the drawing over „alles ist
/// ausgeschaltet" takes, and the least it is drawn at.
const DRAWING_SHARE: f32 = 0.55;
const DRAWING_LEAST: f32 = 120.0;

fn all_off_note(app: &mut GuiApp, ui: &mut Ui, palette: &Palette, width: f32, height: f32) {
    let (room, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let said = text::line(ui, tr!("alles ist ausgeschaltet"), Role::Content, palette.ink);
    let hint = text::line(ui, tr!("musik, radio oder podcasts unter menü → allgemein einschalten"), Role::Label, palette.mid);
    let gap = (said.size().y * 0.5).round();
    let drawn = (room.height() * DRAWING_SHARE).round();
    let drawn = if drawn >= DRAWING_LEAST { drawn } else { 0.0 };
    let under_drawing = if drawn > 0.0 { (said.size().y * 3.0).round() } else { 0.0 };
    let keys_written = tr!("oder [1] für musik, [2] für radio, [3] für podcasts");
    let keys_tall = text::line(ui, "x", Role::Label, palette.mid).size().y + 4.0;
    let keys_air = (gap * 0.6).round();
    let total = drawn + under_drawing + said.size().y + gap + hint.size().y + keys_air + keys_tall;
    let mut y = room.center().y - total / 2.0;
    if drawn > 0.0 {
        let space = Rect::from_min_size(egui::pos2(room.left(), y), Vec2::new(room.width(), drawn));
        crate::gui::menu::drawing_off(ui, space, palette);
        y += drawn + under_drawing;
    }
    let centred = |g: &std::sync::Arc<egui::Galley>, y: f32| egui::pos2((room.center().x - g.size().x / 2.0).round(), y.round());
    let said_at = centred(&said, y);
    y += said.size().y + gap;
    let hint_at = centred(&hint, y);
    let keys_middle = egui::pos2(room.center().x, (y + hint.size().y + keys_air + keys_tall / 2.0).round());
    let hint_rect = Rect::from_min_size(hint_at, hint.size()).union(Rect::from_min_size(said_at, said.size()));
    let response = ui.interact(hint_rect, egui::Id::unique("all-off-note"), Sense::click());
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let hint = if response.hovered() { text::line(ui, tr!("musik, radio oder podcasts unter menü → allgemein einschalten"), Role::Label, palette.ink) } else { hint };
    let painter = ui.painter();
    painter.galley(said_at, said, palette.ink);
    painter.galley(hint_at, hint, palette.ink);
    crate::gui::menu::keys_centred(ui, keys_written, keys_middle, palette.mid, palette);
    if response.clicked() {
        app.menu_tab = 0;
        app.menu_open = true;
    }
}

fn first_column(app: &mut GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette) -> Rect {
    if app.showing_radio() {
        radio_column(app, ui, m, palette)
    } else {
        artists_column(app, ui, m, palette)
    }
}

/// Where a column sliding in or out stands, from 0 (away) to 1 (open), with
/// `on` saying which way it is going.
///
/// egui's own `animate_bool` moved at one pace from start to stop, which
/// read as stiff, and moved on by at most one usual frame per frame drawn,
/// so a frame that took longer held the column back and it stuttered. This
/// one goes by the clock — however few frames there are, it is where it
/// should be — and eases out: off at speed, settling into place. Turned
/// round half way, it sets off from where it is, over the part of the way
/// that is left. `snap` puts it at its end at once.
fn slide(ctx: &egui::Context, id: egui::Id, on: bool, snap: bool) -> f32 {
    slide_for(ctx, id, on, snap, theme::FOURTH_SLIDE)
}

/// Where the foot's words stood when it was last drawn: where the
/// library's words end at the left, where what the status says (or the
/// clock) begins at the right — for the columns over them, see `render`.
#[derive(Clone, Copy, Default)]
pub(super) struct FootInk {
    pub(super) left_ends: Option<f32>,
    pub(super) right_begins: Option<f32>,
}

pub(super) fn foot_ink_id() -> egui::Id {
    egui::Id::unique("foot-ink")
}

/// The ground under what the status says at the foot, where the columns run
/// down to the window's edge: a field of the ground, soft at its edge, from a
/// little before the words to the window's corner, coming and going with
/// them — so a list under it fades away there instead of the column moving up
/// out of the way each time a word appears. Placed by where the words began
/// last frame: they are drawn after it.
fn foot_ground(ui: &Ui, window: Rect, top: f32, palette: &Palette) {
    let ink: FootInk = ui.data(|d| d.get_temp(foot_ink_id())).unwrap_or_default();
    let kept = egui::Id::unique("foot-ground-from");
    if let Some(begins) = ink.right_begins {
        ui.data_mut(|d| d.insert_temp(kept, begins));
    }
    let shown = ui.ctx().animate_bool_with_time(egui::Id::unique("foot-ground"), ink.right_begins.is_some(), FOOT_GROUND_FADE);
    let Some(begins) = ui.data(|d| d.get_temp::<f32>(kept)) else { return };
    if shown <= 0.0 {
        return;
    }
    let field = Rect::from_min_max(
        egui::pos2(begins - FOOT_GROUND_PAD, top - FOOT_GROUND_PAD),
        egui::pos2(window.right() + FOOT_GROUND_SOFT, window.bottom() + FOOT_GROUND_SOFT),
    );
    let ground = palette.ground.gamma_multiply(shown);
    let shadow = egui::epaint::Shadow { offset: [0, 0], blur: FOOT_GROUND_SOFT as u8, spread: (FOOT_GROUND_SOFT / 2.0) as u8, color: ground };
    ui.painter().with_clip_rect(window).add(shadow.as_shape(field, egui::CornerRadius::same(FOOT_GROUND_SOFT as u8)));
}

/// How far the ground under the foot's words reaches past them.
const FOOT_GROUND_PAD: f32 = 10.0;

/// How soft its edge is: thinning over this much, half inside, half out.
const FOOT_GROUND_SOFT: f32 = 40.0;

/// How long it takes to come and go.
const FOOT_GROUND_FADE: f32 = 0.18;

/// How far clear of the foot's words a column has to stand to run down
/// past them.
const FOOT_CLEAR: f32 = 12.0;

/// [`slide`] over `secs` for the whole way: the station's rule draws itself in
/// to the dot the way the columns open, only slower, so it can be watched.
fn slide_for(ctx: &egui::Context, id: egui::Id, on: bool, snap: bool, secs: f32) -> f32 {
    slide_to(ctx, id, if on { 1.0 } else { 0.0 }, snap, secs, egui::emath::easing::cubic_out)
}

/// The ground laid over `area` right after the view changed, and taken away
/// over the time egui fades the menu in: the new view comes up out of the
/// ground rather than standing there at once. `[ui] view_fade`.
fn view_fade(app: &GuiApp, ui: &Ui, area: Rect, palette: &Palette) {
    let view = app.view_shown();
    let id = egui::Id::unique("view-fade");
    let now = ui.input(|i| i.time);
    let before: Option<(crate::gui::View, f64)> = ui.data(|d| d.get_temp(id));
    let since = match before {
        Some((was, at)) if was == view => at,
        None => f64::NEG_INFINITY,
        Some(_) => now,
    };
    ui.data_mut(|d| d.insert_temp(id, (view, since)));
    if !app.config.ui.view_fade {
        return;
    }
    let secs = f64::from(ui.style().animation_time);
    let left = 1.0 - ((now - since) / secs).clamp(0.0, 1.0) as f32;
    if left <= 0.0 {
        return;
    }
    ui.ctx()
        .layer_painter(egui::LayerId::new(egui::Order::Middle, id))
        .rect_filled(area, 0.0, palette.ground.gamma_multiply(left));
    ui.ctx().request_repaint();
}

/// A window without a title bar grows from its edges and corners, as the bar's
/// frame would let it: a few points along each, the pointer saying so, and a
/// press there hands the resizing to the window system.
fn grow_from_the_edges(ui: &Ui, window: Rect) {
    use egui::{CursorIcon, ResizeDirection as D};
    const EDGE: f32 = 5.0;
    const CORNER: f32 = 14.0;
    let (l, r, t, b) = (window.left(), window.right(), window.top(), window.bottom());
    let zones = [
        (Rect::from_min_max(egui::pos2(l, t), egui::pos2(l + CORNER, t + CORNER)), D::NorthWest, CursorIcon::ResizeNorthWest),
        (Rect::from_min_max(egui::pos2(r - CORNER, t), egui::pos2(r, t + CORNER)), D::NorthEast, CursorIcon::ResizeNorthEast),
        (Rect::from_min_max(egui::pos2(l, b - CORNER), egui::pos2(l + CORNER, b)), D::SouthWest, CursorIcon::ResizeSouthWest),
        (Rect::from_min_max(egui::pos2(r - CORNER, b - CORNER), egui::pos2(r, b)), D::SouthEast, CursorIcon::ResizeSouthEast),
        (Rect::from_min_max(egui::pos2(l, t), egui::pos2(r, t + EDGE)), D::North, CursorIcon::ResizeVertical),
        (Rect::from_min_max(egui::pos2(l, b - EDGE), egui::pos2(r, b)), D::South, CursorIcon::ResizeVertical),
        (Rect::from_min_max(egui::pos2(l, t), egui::pos2(l + EDGE, b)), D::West, CursorIcon::ResizeHorizontal),
        (Rect::from_min_max(egui::pos2(r - EDGE, t), egui::pos2(r, b)), D::East, CursorIcon::ResizeHorizontal),
    ];
    for (i, (rect, direction, cursor)) in zones.into_iter().enumerate() {
        let edge = ui.interact(rect, egui::Id::unique(("window-edge", i)), Sense::drag());
        if edge.hovered() || edge.dragged() {
            ui.ctx().set_cursor_icon(cursor);
        }
        if edge.drag_started_by(egui::PointerButton::Primary) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
        }
    }
}

/// The window's three buttons drawn by kessler, where `[ui] window_buttons`
/// asks for them: plain circles where the Mac's own stand (hidden then, see
/// `macwindow`), in the scheme's colours. „einfarbig": all three in the third
/// tone, coming up to the accent under the pointer; „drei farben": the
/// accent, the heads' colour and the second tone, as the Mac's red, yellow
/// and green. Under the pointer they say what they do, in the face's own ×,
/// − and + (or lines drawn here where it has none), fading in and out with
/// the colour; in a window in the back they step back to the third tone.
/// Close puts the window away as the Mac's red button does here, the middle
/// one lays it in the Dock, the last makes it full screen (with `alt`, as big
/// as the screen).
fn own_window_buttons(app: &mut GuiApp, ui: &Ui, palette: &Palette, window: Rect) {
    use crate::gui::glyphs::Mark;
    let style = app.config.ui.window_buttons.clone();
    if style == "system" {
        return;
    }
    let Some((middles, across)) = theme::button_marks() else {
        return;
    };
    let radius = across / 2.0;
    let y = window.top() + theme::band_middle();
    let at = |i: usize| egui::pos2(window.left() + middles[i], y);
    let group = Rect::from_min_max(at(0), at(2)).expand(radius + 4.0);
    let hovered = ui.rect_contains_pointer(group);
    let focused = ui.input(|i| i.focused);
    let lit = ui.ctx().animate_bool_with_time(egui::Id::unique("window-buttons-lit"), hovered, BUTTONS_FADE);
    let shown = ui.ctx().animate_bool_with_time(egui::Id::unique("window-buttons-awake"), focused || hovered, BUTTONS_FADE);
    let colours = if style == "drei farben" {
        [palette.accent, palette.heading(), palette.mid].map(|c| palette.faint.lerp_to_gamma(c, shown))
    } else {
        [palette.faint.lerp_to_gamma(palette.accent, lit); 3]
    };
    let mut pressed = None;
    for (i, colour) in colours.into_iter().enumerate() {
        let centre = at(i);
        let rect = Rect::from_center_size(centre, Vec2::splat(across + 4.0));
        let response = ui.interact(rect, egui::Id::unique(("window-button", i)), Sense::click());
        let colour = if response.is_pointer_button_down_on() { colour.lerp_to_gamma(palette.ink, 0.3) } else { colour };
        ui.painter().circle_filled(centre, radius, colour);
        if lit > 0.0 {
            let ink = palette.ground.gamma_multiply(lit);
            let mark = [Mark::Close, Mark::Shrink, Mark::Grow][i];
            if !crate::gui::glyphs::paint_sized(ui, mark, centre, ink, across * 1.1) {
                drawn_button_mark(ui, i, centre, across, ink);
            }
        }
        if response.clicked() {
            pressed = Some(i);
        }
    }
    let ctx = ui.ctx().clone();
    match pressed {
        Some(0) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
        Some(1) => {
            crate::gui::macwindow::shrink_into_dock();
            ctx.request_repaint();
        }
        Some(_) if ctx.input(|i| i.modifiers.alt) => {
            crate::gui::macwindow::zoom();
            ctx.request_repaint();
        }
        Some(_) => app.toggle_fullscreen(&ctx),
        None => {}
    }
}

/// How long the window's own buttons take to come up to their colour under the
/// pointer, and to go back — see `own_window_buttons`.
const BUTTONS_FADE: f32 = 0.18;

/// ×, − or + as lines, for a face that has not got them.
fn drawn_button_mark(ui: &Ui, i: usize, centre: egui::Pos2, across: f32, ink: Color32) {
    let painter = ui.painter();
    let stroke = egui::Stroke::new((across / 9.0).max(1.0), ink);
    let k = across * 0.2;
    let flat = across * 0.26;
    let line = |a: Vec2, b: Vec2| painter.line_segment([centre + a, centre + b], stroke);
    match i {
        0 => {
            line(Vec2::new(-k, -k), Vec2::new(k, k));
            line(Vec2::new(-k, k), Vec2::new(k, -k));
        }
        1 => {
            line(Vec2::new(-flat, 0.0), Vec2::new(flat, 0.0));
        }
        _ => {
            line(Vec2::new(-flat, 0.0), Vec2::new(flat, 0.0));
            line(Vec2::new(0.0, -flat), Vec2::new(0.0, flat));
        }
    }
}

/// Between the clock at the foot and what the status says beside it.
const FOOT_CLOCK_GAP: f32 = 24.0;

/// The clock at the foot's right end, where `[ui] foot_clock` asks for one:
/// „uhr", the time; „datum", the date before it — as the language writes
/// them, faint as the library's counts at the other end. Drawn again when
/// the minute turns, and no oftener. Where it begins, `right` for none.
fn foot_clock(app: &GuiApp, ui: &Ui, palette: &Palette, right: f32, top: f32) -> f32 {
    if app.config.ui.columns_to_edge {
        return right;
    }
    let with_date = match app.config.ui.foot_clock.as_str() {
        "uhr" => false,
        "datum" => true,
        _ => return right,
    };
    let now = chrono::Local::now();
    let words = clock_words(now.naive_local(), false, with_date, crate::lang::current());
    let g = text::line(ui, &words, Role::Label, palette.faint);
    let x = right - g.size().x;
    ui.painter().galley(egui::pos2(x, top), g, palette.faint);
    let into_minute = f64::from(chrono::Timelike::second(&now)) + f64::from(chrono::Timelike::nanosecond(&now)) / 1e9;
    ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64((60.0 - into_minute).max(0.05)));
    x
}

/// Ask the columns of every view to ease to the widths they are given from
/// now on, as `0` sets them all even again: from what they were, over the
/// time the side's columns slide in.
pub(in crate::gui) fn ease_columns_from_now(ctx: &egui::Context) {
    let now = ctx.input(|i| i.time);
    ctx.data_mut(|d| d.insert_temp(columns_eased_id(), now));
}

fn columns_eased_id() -> egui::Id {
    egui::Id::unique("columns-eased-since")
}

/// A row's column widths as drawn: `target` itself, except for the moment
/// after [`ease_columns_from_now`], when they go there from where they were
/// last drawn, eased out as a slide is. `key` tells one row's widths from
/// another's — the library's, the radio's, the podcasts'.
pub(super) fn eased_widths(ctx: &egui::Context, key: &str, target: Vec<f32>) -> Vec<f32> {
    let last_id = egui::Id::unique(("columns-last", key));
    let from_id = egui::Id::unique(("columns-from", key));
    let now = ctx.input(|i| i.time);
    let since: Option<f64> = ctx.data(|d| d.get_temp(columns_eased_id()));
    let last: Option<Vec<f32>> = ctx.data(|d| d.get_temp(last_id));
    let secs = f64::from(theme::FOURTH_SLIDE) * 1.5;
    let drawn = match since.filter(|at| now - at < secs) {
        Some(at) => {
            let from = match ctx.data(|d| d.get_temp::<(f64, Vec<f32>)>(from_id)).filter(|(a, _)| *a == at) {
                Some((_, from)) => from,
                None => {
                    let from = last.clone().unwrap_or_else(|| target.clone());
                    ctx.data_mut(|d| d.insert_temp(from_id, (at, from.clone())));
                    from
                }
            };
            if from.len() == target.len() {
                let t = egui::emath::easing::cubic_out(((now - at) / secs).clamp(0.0, 1.0) as f32);
                ctx.request_repaint();
                from.iter().zip(&target).map(|(a, b)| a + (b - a) * t).collect()
            } else {
                target
            }
        }
        None => target,
    };
    ctx.data_mut(|d| d.insert_temp(last_id, drawn.clone()));
    drawn
}

/// [`slide_for`] to any point between 0 and 1, not only to the two ends: the
/// search field opens part of the way under the pointer and all of it for
/// typing.
fn slide_to(ctx: &egui::Context, id: egui::Id, to: f32, snap: bool, secs: f32, ease: fn(f32) -> f32) -> f32 {
    #[derive(Clone, Copy)]
    struct Slide {
        from: f32,
        to: f32,
        since: f64,
    }
    let (time, now) = ctx.input(|i| (i.time, i.time + f64::from(i.predicted_dt) / 2.0));
    let secs = f64::from(secs);
    let at = |s: Slide| {
        let length = secs * f64::from((s.to - s.from).abs());
        let done = if length > 0.0 { ((now - s.since) / length).clamp(0.0, 1.0) as f32 } else { 1.0 };
        (s.from + (s.to - s.from) * ease(done), done)
    };
    let held = ctx.data(|d| d.get_temp::<Slide>(id));
    let slide = match held {
        Some(s) if !snap && s.to == to => s,
        Some(s) if !snap => Slide { from: at(s).0, to, since: time },
        _ => Slide { from: to, to, since: time },
    };
    ctx.data_mut(|d| d.insert_temp(id, slide));
    let (value, done) = at(slide);
    if done < 1.0 {
        ctx.request_repaint();
    }
    value
}

pub fn render(app: &mut GuiApp, ui: &mut Ui) {
    app.drawn_part("vor dem zeichnen");
    let palette = app.palette;
    let window = ui.available_rect_before_wrap();
    if ui.interact(window, egui::Id::unique("window-floor"), Sense::click()).clicked() {
        app.let_go();
    }
    let mut m = Metrics::for_size(window.width(), window.height()).with_air().with_margins();
    if let Some((middles, across)) = theme::button_marks() {
        m = m.on_the_buttons(middles[0] - across / 2.0);
    }
    set_wheel_step(ui.ctx(), &app.config.ui.scroll_speed, m.row);
    masthead::set_window_width(window.width());
    let super_compact = PlayingAt::now(&app.config.ui) == PlayingAt::SuperCompact;
    let room = window.width() - (m.margin_x - m.marker_pad_x) * 2.0;
    app.side_fits = window.width() >= theme::SIDE_FROM;
    let split = app.split();
    let switched = split != app.was_split;
    if switched {
        ui.ctx().clear_animations();
        app.was_split = split;
    }

    let shares = {
        let c = &app.config.ui;
        theme::Shares {
            artists: c.artists_share,
            albums: c.albums_share,
            queue: c.queue_share,
            side: c.side_share,
            even: c.merged_even,
        }
    };

    let narrow = window.width()
        < theme::NARROW_UNTIL
            + if app.fourth_open() {
                theme::QUEUE_WIDTH + 48.0
            } else {
                0.0
            };
    app.narrow = narrow;

    let left_on = if split {
        app.left_side().is_some()
    } else {
        app.fourth_open()
    };
    let right_on = split && app.right_side().is_some();
    let under_radio = egui::Id::unique("library-under-radio");
    let radio_before = ui.data(|d| d.get_temp::<bool>(under_radio)).unwrap_or(false);
    let radio_now = app.showing_radio();
    ui.data_mut(|d| d.insert_temp(under_radio, radio_now));
    let on_device = egui::Id::unique("library-on-device");
    let device_before = ui.data(|d| d.get_temp::<bool>(on_device)).unwrap_or(false);
    let device_now = app.showing_ipod();
    ui.data_mut(|d| d.insert_temp(on_device, device_now));
    let under_podcasts = egui::Id::unique("library-under-podcasts");
    let podcasts_before = ui.data(|d| d.get_temp::<bool>(under_podcasts)).unwrap_or(false);
    let podcasts_now = app.showing_podcasts();
    ui.data_mut(|d| d.insert_temp(under_podcasts, podcasts_now));
    let switched = switched || radio_now || radio_before || podcasts_now || podcasts_before || device_now != device_before;
    let open = slide(ui.ctx(), egui::Id::unique("fourth-open"), left_on, switched);
    let side_open = slide(ui.ctx(), egui::Id::unique("side-open"), right_on, switched);
    let many = app.all_sides();
    let sides: Vec<(crate::gui::Fourth, f32)> = if many {
        crate::gui::Fourth::SIDES
            .into_iter()
            .map(|side| {
                let open = app.side_is_open(side);
                let t = slide(ui.ctx(), egui::Id::unique(("side-open", side.key())), open, switched);
                (side, t)
            })
            .filter(|(_, t)| *t > 0.0)
            .collect()
    } else {
        Vec::new()
    };
    let side_count: f32 = sides.iter().map(|(_, t)| t).sum();
    let side_open = if many { side_count.min(1.0) } else { side_open };
    let layout = app.names_layout();
    let with = theme::columns_with_side(room, m, shares, layout, 1.0, side_open);
    let side_full = theme::columns_with_side(room, m, shares, layout, open, 1.0).side;
    let columns = theme::columns_with_side(room, m, shares, layout, open, side_open);
    let even = many.then(|| {
        let names = layout.count() as f32;
        let between: f32 = sides.iter().skip(1).map(|(_, t)| t).sum();
        let gaps = m.column_gap * (names + open + side_open + between);
        ((room - gaps) / (names + 1.0 + open + side_count)).max(0.0)
    });
    let (with, side_full, columns) = match even {
        Some(each) => {
            let two = layout.count() > 1;
            let row = |left: f32, right: f32| theme::Columns {
                artists: each,
                albums: if two { each } else { 0.0 },
                tracks: each,
                queue: each * left,
                side: each * right,
            };
            (row(1.0, side_open), each, row(open, side_open))
        }
        None => (with, side_full, columns),
    };
    let columns = {
        let w = eased_widths(ui.ctx(), "library", vec![columns.artists, columns.albums, columns.tracks, columns.queue, columns.side]);
        theme::Columns { artists: w[0], albums: w[1], tracks: w[2], queue: w[3], side: w[4] }
    };
    let sliding = |t: f32| t > 0.0 && t < 1.0;
    app.fourth_sliding = sliding(open) || sliding(side_open) || sides.iter().any(|(_, t)| sliding(*t));
    if split {
        if let Some(left) = app.left_side() {
            app.fourth_was = left;
        }
        if let Some(right) = app.right_side() {
            app.side_was = right;
        }
    } else if app.fourth_open() {
        app.fourth_was = app.fourth();
    }
    m.artists_width = columns.artists;
    m.albums_width = columns.albums;

    let on_a_mac = cfg!(target_os = "macos");
    let compact = PlayingAt::now(&app.config.ui) == PlayingAt::Compact;
    let in_the_band = !narrow && (super_compact || (on_a_mac && !compact));
    let all_off = app.all_off();
    let menu_in_the_band = (on_a_mac && narrow) || (compact && !narrow) || all_off;
    let library_at_foot = app.config.ui.library_line
        && !app.config.ui.columns_to_edge
        && !app.showing_podcasts()
        && !all_off
        && app.config.ui.music;
    set_field_height(if in_the_band { FIELD_HEIGHT_FLAT } else { FIELD_HEIGHT });
    let off_the_band = m.margin_x - theme::BAND_INK_TOP;
    let top = if in_the_band && !on_a_mac {
        m.margin_x
    } else if in_the_band {
        theme::band_line_top(field_height())
    } else if menu_in_the_band && !on_a_mac {
        theme::margin_under_line() + off_the_band
    } else {
        theme::margin_top()
    };
    let barless = cfg!(target_os = "linux") && app.config.ui.window_bar == "ohne";
    if on_a_mac || barless {
        let band = Rect::from_min_size(window.min, Vec2::new(window.width(), theme::band_middle() * 2.0));
        let held = ui.interact(band, egui::Id::unique("window-band"), Sense::click_and_drag());
        if held.drag_started_by(egui::PointerButton::Primary) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        if held.double_clicked() {
            if on_a_mac {
                crate::gui::macwindow::zoom();
            } else {
                let maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
            }
            ui.ctx().request_repaint();
        }
    }
    if barless {
        grow_from_the_edges(ui, window);
    }
    if on_a_mac {
        own_window_buttons(app, ui, &palette, window);
    }
    if menu_in_the_band {
        let shift = if on_a_mac { 0.0 } else { off_the_band };
        menu_in_band(app, ui, &palette, window.top() + shift, window.right() - m.margin_x);
    }

    let status_at_foot = true;
    let bottom = if status_at_foot { theme::MARGIN_BOTTOM } else { m.margin_x };
    let outer = egui::Frame::NONE.inner_margin(egui::Margin {
        left: (m.margin_x - m.marker_pad_x) as i8,
        right: (m.margin_x - m.marker_pad_x) as i8,
        top: top as i8,
        bottom: bottom as i8,
    });

    let mut below_head = window.top();
    outer.show(ui, |ui| {
        let full_width = ui.available_width();

        let Masthead { field } = if all_off {
            Masthead { field: Rect::NOTHING }
        } else if narrow {
            masthead_narrow(app, ui, &palette, m, full_width, !menu_in_the_band)
        } else {
            masthead_compact(app, ui, &palette, m, full_width, in_the_band, !menu_in_the_band)
        };
        app.drawn_part("kopf");
        if narrow && !all_off {
            ui.add_space(32.0);
            narrow_playing(app, ui, &palette, m, full_width);
            ui.add_space(28.0);
        }
        below_head = ui.cursor().min.y;

        const COMPACT_FOOT: f32 = 44.0;
        let foot = if status_at_foot { COMPACT_FOOT } else { 0.0 };
        let into_foot = FOOT_WORDS_NEARER.min(foot);
        let columns_height = (ui.available_height() - foot + into_foot).max(120.0);
        let strip_height = if narrow { columns_height - into_foot + NARROW_INTO_FOOT.min(foot) } else { columns_height };
        let ink: FootInk = ui.data(|d| d.get_temp(foot_ink_id())).unwrap_or_default();
        let to_edge = app.config.ui.columns_to_edge;
        let deeper = if to_edge { foot + theme::MARGIN_BOTTOM - into_foot } else { 0.0 };
        let reach = |ui: &Ui, n: usize, width: f32| -> f32 {
            let from = ui.cursor().min.x;
            let under = ink.left_ends.is_some_and(|end| from < end + FOOT_CLEAR)
                || (!to_edge && ink.right_begins.is_some_and(|begin| from + width > begin - FOOT_CLEAR));
            let down = slide_for(ui.ctx(), egui::Id::unique(("column-down", n)), !under, false, theme::FOURTH_SLIDE);
            let deeper_now = deeper * down;
            ui.ctx().data_mut(|d| d.insert_temp(column_at_edge_id(), to_edge && down >= 0.99));
            columns_height + deeper_now
        };
        ui.data_mut(|d| d.remove_temp::<bool>(column_at_edge_id()));
        let queue_others = if app.merged() {
            m.artists_width + m.column_gap * 2.0
        } else {
            m.artists_width + m.albums_width + m.column_gap * 3.0
        };
        let queue_width = columns.queue;
        let queue_full = with.queue;
        let queue_gap = m.column_gap * open;
        let side_width = columns.side;
        let side_gap = m.column_gap * side_open;
        let between: f32 = sides.iter().skip(1).map(|(_, t)| m.column_gap * t).sum();
        let side_total = if many { side_full * side_count + between } else { side_width };
        let side_unit = side_full;
        let side_part = side_total + side_gap;

        let columns_band = Rect::from_min_size(
            ui.cursor().min,
            Vec2::new(full_width, columns_height),
        );

        let mut taken: Vec<Rect> = Vec::new();
        if app.showing_podcasts() {
            let podcasts_narrow = window.width() < theme::NARROW_UNTIL;
            let reach_column = |ui: &Ui, n: usize, width: f32| reach(ui, 20 + n, width);
            ui.allocate_ui(Vec2::new(full_width, columns_height + deeper), |ui| {
                let height = if podcasts_narrow { strip_height } else { columns_height };
                podcast_row(app, ui, m, &palette, full_width, podcasts_narrow, height, &reach_column);
            });
        } else if app.all_off() {
            ui.allocate_ui(Vec2::new(full_width, columns_height), |ui| {
                all_off_note(app, ui, &palette, full_width, columns_height);
            });
        } else if narrow && app.showing_radio() {
            radio_strip(app, ui, m, &palette, full_width, strip_height);
        } else if narrow {
            sliding_columns(app, ui, m, &palette, full_width, strip_height);
        } else if app.showing_radio() {
            let reach_column = |ui: &Ui, n: usize, width: f32| reach(ui, 40 + n, width);
            ui.allocate_ui(Vec2::new(full_width, columns_height + deeper), |ui| {
                radio_row(app, ui, m, &palette, full_width, &reach_column);
            });
        } else {
            ui.allocate_ui(Vec2::new(full_width, columns_height + deeper), |ui| {
                ui.horizontal_top(|ui| {
                    let h = reach(ui, 0, m.artists_width);
                    taken.push(ui.allocate_ui(Vec2::new(m.artists_width, h), |ui| first_column(app, ui, m, &palette)).inner);
                    app.drawn_part("erste spalte");
                    if !app.merged() {
                        ui.add_space(m.column_gap);
                        let h = reach(ui, 1, m.albums_width);
                        taken.push(ui.allocate_ui(Vec2::new(m.albums_width, h), |ui| albums_column(app, ui, m, &palette)).inner);
                        app.drawn_part("alben");
                    }
                    ui.add_space(m.column_gap);
                    let room = ui.available_width() - queue_width - queue_gap - side_part;
                    let h = reach(ui, 2, room);
                    taken.push(ui.allocate_ui(Vec2::new(room, h), |ui| tracks_column(app, ui, m, &palette)).inner);
                    app.drawn_part("titel");
                    if queue_width > 0.5 {
                        ui.add_space(queue_gap);
                        let h = reach(ui, 3, queue_width);
                        if open >= 1.0 {
                            taken.push(
                                ui.allocate_ui(Vec2::new(queue_width, h), |ui| fourth_column(app, ui, m, &palette, queue_width))
                                    .inner,
                            );
                        } else {
                            let (slot, _) = ui.allocate_exact_size(Vec2::new(queue_width, h), Sense::hover());
                            let whole = Rect::from_min_size(
                                slot.min,
                                Vec2::new(queue_full, slot.height()),
                            );
                            let mut inside = ui.new_child(egui::UiBuilder::new().max_rect(whole));
                            inside.set_clip_rect(slot.intersect(ui.clip_rect()));
                            fourth_column(app, &mut inside, m, &palette, queue_full);
                            paint_side_fade(ui, slot, 1.0 - open, &palette);
                            taken.push(slot);
                        }
                    }
                    app.drawn_part("vierte spalte");
                    if many && side_total > 0.5 {
                        ui.add_space(side_gap);
                        for (at, (side, t)) in sides.iter().enumerate() {
                            if at > 0 {
                                ui.add_space(m.column_gap * t);
                            }
                            let h = reach(ui, 4 + at, side_unit * t);
                            let (slot, _) = ui.allocate_exact_size(Vec2::new(side_unit * t, h), Sense::hover());
                            let whole = Rect::from_min_size(slot.min, Vec2::new(side_unit, slot.height()));
                            let mut inside = ui.new_child(egui::UiBuilder::new().max_rect(whole));
                            inside.set_clip_rect(slot.intersect(ui.clip_rect()));
                            let id = format!("side-column-{}", side.key());
                            side_column_of(app, &mut inside, m, &palette, side_unit, *side, &id);
                            if *t < 1.0 {
                                paint_side_fade(ui, slot, 1.0 - t, &palette);
                            }
                            if at == 0 {
                                taken.push(slot);
                            }
                        }
                    }
                    if !many && side_width > 0.5 {
                        ui.add_space(side_gap);
                        let h = reach(ui, 4, side_width);
                        if side_open >= 1.0 {
                            taken.push(
                                ui.allocate_ui(Vec2::new(side_width, h), |ui| side_column(app, ui, m, &palette, side_width)).inner,
                            );
                        } else {
                            let (slot, _) = ui.allocate_exact_size(Vec2::new(side_width, h), Sense::hover());
                            let whole = Rect::from_min_size(
                                slot.min,
                                Vec2::new(side_full, slot.height()),
                            );
                            let mut inside = ui.new_child(egui::UiBuilder::new().max_rect(whole));
                            inside.set_clip_rect(slot.intersect(ui.clip_rect()));
                            side_column(app, &mut inside, m, &palette, side_full);
                            paint_side_fade(ui, slot, 1.0 - side_open, &palette);
                            taken.push(slot);
                        }
                    }
                    app.drawn_part("seite");
                });
            });
        }

        let mut settled = false;
        let mut reset = false;
        let handles = if many { 0 } else { taken.len().saturating_sub(1) };
        for index in 0..handles {
            let (left, right) = (taken[index], taken[index + 1]);
            let Some(handle) = divider(ui, index, left, right, &palette) else {
                continue;
            };
            let width = match handle {
                Handle::Reset => {
                    reset = true;
                    continue;
                }
                Handle::Width(width, done) => {
                    settled |= done;
                    width
                }
            };
            if app.merged() && app.config.ui.merged_even {
                app.config.ui.merged_even = false;
                app.config.ui.artists_share = Some(columns.artists / full_width);
                app.config.ui.queue_share = Some(queue_full / full_width);
                app.config.ui.side_share = Some(side_full / full_width);
            }
            let names = app.names_layout().count();
            let lent = theme::queue_handed_round(
                full_width,
                m,
                app.config.ui.queue_share,
                app.names_layout(),
                app.fourth_open(),
            );
            let share = |wanted: f32, others: f32| {
                Some(theme::fit_column(wanted - lent, others, full_width) / full_width)
            };
            match index {
                0 => {
                    let beside = if app.merged() {
                        m.column_gap + queue_width + side_part
                    } else {
                        m.albums_width + m.column_gap * 2.0 + queue_width
                    };
                    app.config.ui.artists_share = share(width, beside);
                }
                i if i == names - 1 => {
                    app.config.ui.albums_share = share(
                        width,
                        m.artists_width + m.column_gap * 2.0 + queue_width,
                    );
                }
                i if i == names + 1 => {
                    let pair = queue_width + side_width;
                    let queue = width.clamp(theme::MIN_NAME_COLUMN, pair - theme::MIN_NAME_COLUMN);
                    app.config.ui.queue_share = Some(queue / full_width);
                    app.config.ui.side_share = Some((pair - queue) / full_width);
                }
                _ => {
                    let taken_left = queue_others - m.column_gap + width;
                    let wanted = share(full_width - taken_left - side_part, queue_others + side_part);
                    if queue_width > 0.5 {
                        app.config.ui.queue_share = wanted;
                    } else {
                        app.config.ui.side_share =
                            share(full_width - taken_left, queue_others);
                    }
                }
            }
        }
        if reset {
            let names = app.names_layout().count();
            let side = split && right_on;
            let count = names + 2 + usize::from(side);
            let each = theme::even_columns(count, full_width, m.column_gap);
            let others = each * (count - 2) as f32 + m.column_gap * (count - 1) as f32;
            let even = Some(theme::fit_column(each, others, full_width) / full_width);
            {
                app.config.ui.artists_share = even;
                if !app.merged() {
                    app.config.ui.albums_share = even;
                }
            }
            app.config.ui.queue_share = even;
            if side {
                app.config.ui.side_share = even;
            }
            if app.merged() {
                app.config.ui.merged_even = true;
            }
            settled = true;
        }
        if settled {
            app.save_config();
        }

        if app.filter.is_some() {
            let pressed = ui.input(|i| {
                (i.pointer.any_pressed() || i.pointer.any_click())
                    .then(|| {
                        i.pointer
                            .press_origin()
                            .or(i.pointer.interact_pos())
                            .or(i.pointer.latest_pos())
                    })
                    .flatten()
            });
            if let Some(at) = pressed.filter(|at| !field.contains(*at)) {
                let outcome = if columns_band.contains(at) {
                    app.stop_typing()
                } else {
                    app.close_filter()
                };
                if let Err(e) = outcome {
                    app.set_status(format!("{e}"));
                }
            }
        }

        if status_at_foot {
            let probe = text::line(ui, "x", Role::Label, palette.faint);
            let top = window.bottom() - m.margin_x - text::baseline(&probe);
            let right = window.right() - m.margin_x;
            if app.config.ui.columns_to_edge {
                foot_ground(ui, window, top, &palette);
            }
            let clock_from = foot_clock(app, ui, &palette, right, top);
            let status_right = if clock_from < right { clock_from - FOOT_CLOCK_GAP } else { right };
            let said_from = header_status(app, ui, &palette, window.left() + m.margin_x, status_right, top, true, library_at_foot);
            let said_from = if said_from < status_right { said_from } else { clock_from };
            if all_off {
                let mut x = window.left() + m.margin_x;
                let commit = env!("KESSLER_COMMIT");
                for (words, colour) in [(tr!("kessler"), palette.ink), (crate::VERSION_SHOWN, palette.mid), (commit, palette.faint)] {
                    if words.is_empty() {
                        continue;
                    }
                    let g = text::line(ui, words, Role::Label, colour);
                    let width = g.size().x;
                    ui.painter().galley(egui::pos2(x, top), g, colour);
                    x += width + m.column_gap / 2.0;
                }
            }
            let right_begins = (said_from < right - 0.5).then_some(said_from);
            ui.data_mut(|d| d.get_temp_mut_or_default::<FootInk>(foot_ink_id()).right_begins = right_begins);
        }
    });

    view_fade(app, ui, Rect::from_min_max(egui::pos2(window.left(), below_head), window.max), &palette);

    app.drawn_part("fuss");
    carried(app, ui);
    dropping(app, ui, window);
    row_menu(app, ui, window);
    app.drawn_part("ziehen und kontextmenü");
    menu::render(app, ui, window, m);
    menu::keys_sheet(app, ui, window, m);
    app.drawn_part("menü");
    super::saver::draw(app, ui.ctx(), &palette);
    let dip = app.zoom_dip(ui.ctx());
    if dip > 0.0 {
        ui.ctx()
            .layer_painter(egui::LayerId::new(egui::Order::Debug, egui::Id::unique("zoom-dip")))
            .rect_filled(ui.ctx().content_rect(), 0.0, palette.ground.gamma_multiply(dip));
    }

    if std::mem::take(&mut app.reveal_next) {
        ui.ctx().request_repaint();
    } else {
        app.scroll_to = None;
        app.reveal_all = false;
    }
}

/// How far a narrow window's strip of columns, with its line of arrows,
/// reaches down into the foot's band.
const NARROW_INTO_FOOT: f32 = 24.0;

/// How far into the foot's band a column goes that has the foot's words under
/// it — see `render`.
const FOOT_WORDS_NEARER: f32 = 28.0;

/// The columns as a strip that slides, for a window too narrow to hold them
/// side by side.
///
/// One column is read at a time and fills the window, as on an iPod; under it a
/// line names the columns on either side, with an arrow, and takes you there.
/// The strip itself is drawn wider than the window and pushed left — everything
/// outside is clipped by the window, so no column has to know it is off-screen.
fn sliding_columns(
    app: &mut GuiApp,
    ui: &mut Ui,
    m: Metrics,
    palette: &Palette,
    room: f32,
    height: f32,
) {
    let width = room;
    let gap = theme::SLIDE_GAP;

    let names = app.names_layout().count();
    let count = names + 1 + usize::from(app.fourth_open());
    let index = match app.focus {
        Column::Lists | Column::Artists => 0,
        Column::Albums if app.merged() => 0,
        Column::Albums => names.saturating_sub(1),
        Column::Tracks => names,
        Column::Queue => names + 1,
    }
    .min(count - 1);

    let offset = ui.ctx().animate_value_with_time(
        egui::Id::unique("column-slide"),
        theme::slide_offset(index, count, width, gap),
        0.22,
    );

    let mut m = m;
    m.artists_width = width;
    m.albums_width = width;
    m.column_gap = gap;

    let steps_height = m.row * 1.5;
    let strip_height = (height - steps_height).max(m.row);
    let strip_width = theme::strip_width(count, width, gap);
    let (area, _) = ui.allocate_exact_size(Vec2::new(room, strip_height), Sense::hover());
    let strip = Rect::from_min_size(
        egui::pos2(area.left() - offset, area.top()),
        Vec2::new(strip_width, strip_height),
    );

    ui.scope_builder(egui::UiBuilder::new().max_rect(strip), |ui| {
        ui.horizontal_top(|ui| {
            first_column(app, ui, m, palette);
            if !app.merged() {
                ui.add_space(gap);
                albums_column(app, ui, m, palette);
            }
            ui.add_space(gap);
            ui.allocate_ui(Vec2::new(width, ui.available_height()), |ui| {
                tracks_column(app, ui, m, palette)
            });
            if app.fourth_open() {
                ui.add_space(gap);
                fourth_column(app, ui, m, palette, width);
            }
        });
    });

    let line = Rect::from_min_size(area.left_bottom(), Vec2::new(room, steps_height));
    let order = strip_order(app, names);
    column_steps(app, ui, line, &order, index, m, palette);
}

/// The columns of the sliding strip, left to right: where each one takes the
/// keyboard, and what the line under the strip calls it.
fn strip_order(app: &GuiApp, names: usize) -> Vec<(Column, &'static str)> {
    let mut order: Vec<(Column, &'static str)> = Vec::new();
    {
        order.push((Column::Artists, tr!("künstler:innen")));
        if names > 1 {
            order.push((Column::Albums, tr!("alben")));
        }
    }
    order.push((Column::Tracks, if app.merged() { tr!("alben") } else { tr!("titel") }));
    if app.fourth_open() {
        let name = match app.fourth() {
            crate::gui::Fourth::Queue => tr!("warteschlange"),
            crate::gui::Fourth::Lists => tr!("listen"),
            crate::gui::Fourth::Device => "ipod",
            crate::gui::Fourth::Log => "log",
            crate::gui::Fourth::Artist => tr!("über künstler:in"),
            crate::gui::Fourth::Lyrics => tr!("liedtext"),
            crate::gui::Fourth::Others => tr!("andere künstler:innen"),
            crate::gui::Fourth::Equalizer => "equalizer",
        };
        order.push((Column::Queue, name));
    }
    order
}

/// The line under the sliding strip: the column to the left with an arrow
/// pointing there, at the left margin, and the one to the right at the right
/// margin. A click takes you there, as `h` and `l` do. Labels, in the mid tone,
/// brightening under the pointer: a way on, not a pair of buttons.
fn column_steps(
    app: &mut GuiApp,
    ui: &mut Ui,
    line: Rect,
    order: &[(Column, &'static str)],
    index: usize,
    m: Metrics,
    palette: &Palette,
) {
    let names: Vec<&'static str> = order.iter().map(|(_, name)| *name).collect();
    if let Some(to) = strip_steps(ui, line, &names, index, m, palette, "library") {
        if let Err(e) = app.focus_column(order[to].0) {
            app.set_status(format!("{e}"));
        }
    }
}

/// The line itself, for any strip of named columns — the library's, and the
/// radio's: which neighbour was clicked, if one was.
pub(super) fn strip_steps(
    ui: &mut Ui,
    line: Rect,
    names: &[&'static str],
    index: usize,
    m: Metrics,
    palette: &Palette,
    which: &'static str,
) -> Option<usize> {
    let inner = content_rect(Rect::from_min_max(egui::pos2(line.left(), line.top() + m.row * 0.5), line.max), m);
    let mut go: Option<usize> = None;
    let sides = [
        (index.checked_sub(1), true),
        (Some(index + 1).filter(|n| *n < names.len()), false),
    ];
    for (neighbour, left) in sides {
        let Some((at, name)) = neighbour.and_then(|n| names.get(n).map(|name| (n, name))) else {
            continue;
        };
        let words = if left { format!("← {name}") } else { format!("{name} →") };
        let probe = text::line(ui, &words, Role::Label, palette.mid);
        let size = probe.size();
        let x = if left { inner.left() } else { inner.right() - size.x };
        let hit = Rect::from_min_size(egui::pos2(x, inner.top()), Vec2::new(size.x, m.row));
        let response = ui.interact(hit, egui::Id::unique(("column-step", which, left)), Sense::click());
        controls::paint_control_hover(ui, hit.expand2(Vec2::new(4.0, 0.0)), &response, palette);
        let colour = if response.hovered() { palette.ink } else { palette.mid };
        let g = text::line(ui, &words, Role::Label, colour);
        let y = text::on_line(&g, hit.top(), text::row_line(ui, m.row, Role::Label));
        ui.painter().galley(egui::pos2(x, y), g, colour);
        if response.clicked() {
            go = Some(at);
        }
    }
    go
}

/// The gap between two columns, as something you can take hold of.
///
/// Nothing is drawn until the pointer is in the gap, and then it is a thin
/// rounded bar down the middle of it — the width the columns are already keeping
/// apart, made visible for as long as it is useful. It is not a border: the
/// moment the pointer leaves, the window has no lines in it again.
///
/// While it is being dragged, returns the new width for the column on its left
/// and whether the drag has just ended, which is when the setting is worth
/// writing to disk.
///
/// `left` and `right` are the rectangles the two columns actually occupied.
/// The line goes halfway between their facing edges — measured, not worked out
/// a second time from the margins, because those two answers turned out not to
/// agree on every machine.
fn divider(
    ui: &mut Ui,
    id: usize,
    left: Rect,
    right: Rect,
    palette: &Palette,
) -> Option<Handle> {
    /// How much of the gap answers the pointer. Wider than the line by a good
    /// margin: the line is a mark, the gap is the target.
    const GRAB: f32 = 11.0;
    /// A little thinner than the extent mark, rounded the same way, and in the
    /// marker's own grey — the tone the chosen row stands on — so it reads as
    /// part of the columns it moves rather than as a line drawn across them.
    const LINE: f32 = 2.0;
    /// How far the line stays clear of the top and bottom of the columns, so it
    /// reads as belonging to the gap rather than crossing the whole window.
    const INSET_Y: f32 = 20.0;
    const FADE_SECS: f32 = 0.12;

    let gap = right.left() - left.right();
    if gap <= 1.0 {
        return None;
    }
    let centre = (left.right() + right.left()) / 2.0;

    let grab = Rect::from_min_size(
        egui::pos2(centre - GRAB / 2.0, left.top()),
        Vec2::new(GRAB, left.height()),
    );
    let response = ui.interact(
        grab,
        egui::Id::unique(("divider", id)),
        Sense::click_and_drag(),
    );

    const HELD_SECS: f64 = 0.15;
    const HELD_BEYOND: f32 = 5.0;
    let held = egui::Id::unique(("divider-held", id));
    let now = ui.input(|i| i.time);
    let on = response.hovered() || response.dragged();
    if on {
        ui.data_mut(|d| d.insert_temp(held, now));
    }
    let lately = ui.data(|d| d.get_temp::<f64>(held)).is_some_and(|at| now - at < HELD_SECS);
    let near = ui.ctx().pointer_latest_pos().is_some_and(|p| grab.expand2(Vec2::new(HELD_BEYOND, 0.0)).contains(p));
    let active = on || (lately && near && !ui.input(|i| i.pointer.any_down()));
    if active && !on {
        ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(HELD_SECS));
    }
    if active {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeColumn);
        crate::gui::macwindow::column_cursor();
        if response.hovered() && !response.dragged() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
        }
    }
    let shown = ui
        .ctx()
        .animate_bool_with_time(egui::Id::unique(("divider-line", id)), active, FADE_SECS);
    if shown > 0.01 {
        let line = Rect::from_min_size(
            egui::pos2((centre - LINE / 2.0).round(), left.top() + INSET_Y),
            Vec2::new(LINE, (left.height() - INSET_Y * 2.0).max(0.0)),
        );
        let colour = if response.dragged() {
            palette.marker.lerp_to_gamma(palette.ink, 0.2)
        } else {
            palette.marker
        };
        ui.painter()
            .rect_filled(line, LINE / 2.0, colour.gamma_multiply(shown));
    }

    if response.double_clicked() {
        return Some(Handle::Reset);
    }
    if !response.dragged() && !response.drag_stopped() {
        return None;
    }
    let pointer = response.interact_pointer_pos()?;
    Some(Handle::Width(
        pointer.x - gap / 2.0 - left.left(),
        response.drag_stopped(),
    ))
}

/// What a hand on a divider asked for.
enum Handle {
    /// Pulled to here. The flag is the frame the drag ended on, which is the
    /// one worth writing a settings file for.
    Width(f32, bool),
    /// Pressed without pulling: share the row out evenly again.
    ///
    /// The same thing the menu's „spaltenbreiten zurücksetzen" does, within
    /// reach of the hand that made the mess — except that it goes to an even
    /// split rather than to the opening widths, because a click on the line
    /// between two columns is a question about those two columns.
    Reset,
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_wheel_notch_moves_five_rows_on_linux_and_as_before_on_a_mac() {
        assert_eq!(super::wheel_step("normal", 24.0, false), 120.0);
        assert_eq!(super::wheel_step("langsam", 24.0, false), 80.0);
        assert_eq!(super::wheel_step("schnell", 24.0, false), 200.0);
        assert_eq!(super::touch_factor("normal", false), 2.0);
        assert_eq!(super::touch_factor("schnell", true), 1.0, "a mac's trackpad is left as it is");
        assert_eq!(super::wheel_step("normal", 24.0, true), 40.0);
        assert_eq!(super::wheel_step("", 24.0, true), 40.0, "an old config without the key");
    }

    #[test]
    fn the_clock_writes_the_date_as_the_language_does() {
        use crate::lang::Lang;
        let at = chrono::NaiveDate::from_ymd_opt(2026, 3, 5).unwrap().and_hms_opt(14, 7, 0).unwrap();
        assert_eq!(masthead::clock_words(at, false, true, Lang::De), "05.03.2026  14:07");
        assert_eq!(masthead::clock_words(at, true, true, Lang::De), "05.03.2026  2:07 pm");
        assert_eq!(masthead::clock_words(at, false, true, Lang::En), "3/5/2026  14:07");
        assert_eq!(masthead::clock_words(at, true, true, Lang::En), "3/5/2026  2:07 pm");
        assert_eq!(masthead::clock_words(at, true, true, Lang::Ja), "2026年3月5日  午後2:07");
        let midnight = at.date().and_hms_opt(0, 5, 0).unwrap();
        assert_eq!(masthead::clock_words(midnight, true, true, Lang::En), "3/5/2026  12:05 am");
        assert_eq!(masthead::clock_words(at, false, false, Lang::De), "14:07");
        assert_eq!(masthead::clock_words(at, true, false, Lang::Ja), "午後2:07");
    }

    /// The cover's size chooses the heading's lines: two beside one smaller
    /// than three lines are tall, three beside a taller one or none.
    #[test]
    fn the_cover_chooses_two_lines_or_three() {
        use super::tracks::{three_lines_tall, two_lines_beside};
        let tall = three_lines_tall();
        assert!(tall > super::theme::ROW_ROOMY * 2.0 * super::theme::head_line(), "more than two line steps");
        assert!(two_lines_beside(Some(crate::config::HEADING_COVER_LEAST)));
        assert!(two_lines_beside(Some(tall - 1.0)));
        assert!(!two_lines_beside(Some(tall)));
        assert!(!two_lines_beside(Some(crate::config::HEADING_COVER_DEFAULT)));
        assert!(!two_lines_beside(None), "without a cover, three");
    }

    /// A record's heading says its bitrate once, or the span where its
    /// tracks differ; tracks without one do not count.
    #[test]
    fn a_record_says_its_bitrate_or_its_span() {
        let track = |bitrate: Option<u32>| crate::db::models::Track {
            id: 0,
            path: std::path::PathBuf::new(),
            title: "t".into(),
            artist: "cave".into(),
            album_artist: String::new(),
            album: "allways".into(),
            genre: String::new(),
            year: None,
            track_number: None,
            disc_number: None,
            duration_secs: 1.0,
            bitrate,
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
        let said = |run: &[crate::db::models::Track]| super::tracks::heading_bitrate(run);
        assert_eq!(said(&[track(Some(320)), track(Some(320))]).as_deref(), Some("320 kbit/s"));
        assert_eq!(
            said(&[track(Some(320)), track(None), track(Some(319)), track(Some(0))]).as_deref(),
            Some("319–320 kbit/s")
        );
        assert_eq!(said(&[track(None), track(Some(0))]), None);
        assert_eq!(said(&[]), None);
    }

    /// Records in the track list are the runs of one artist and one title,
    /// folded as the library folds names — a capital does not split one.
    #[test]
    fn a_record_is_a_run_of_its_own_tracks() {
        let track = |artist: &str, album: &str| crate::db::models::Track {
            id: 0,
            path: std::path::PathBuf::new(),
            title: "t".into(),
            artist: artist.into(),
            album_artist: String::new(),
            album: album.into(),
            genre: String::new(),
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
        let list = [
            track("aphex twin", "saw 85-92"),
            track("Aphex Twin", "SAW 85-92"),
            track("aphex twin", "richard d. james album"),
            track("autechre", "amber"),
            track("autechre", "amber"),
            track("autechre", "amber"),
        ];
        let none = std::collections::HashSet::new();
        assert_eq!(album_runs(&list, &none), vec![(0, 2), (2, 1), (3, 3)]);
        assert!(album_runs(&[], &none).is_empty());

        let mut sampler = [
            track("arktis", "psychedelic underground 2"),
            track("amon düül", "psychedelic underground 2"),
            track("guru guru", "psychedelic underground 2"),
            track("autechre", "amber"),
        ];
        for (id, t) in sampler.iter_mut().enumerate() {
            t.id = id as i64;
        }
        let compiled = [0, 1, 2].into_iter().collect();
        assert_eq!(album_runs(&sampler, &compiled), vec![(0, 3), (3, 1)]);
        assert_eq!(album_runs(&sampler, &none).len(), 4);
    }

    use super::*;

    /// What a narrow masthead gives up, and in what order.
    ///
    /// The bug this guards is the one that was actually on screen: at 360
    /// points the search field was drawn *over* the transport, because it kept
    /// its full width and is painted last.
    /// A pointer over the search field takes it back from the picture, and
    /// nothing else has to be true for that.
    ///
    /// The rule looks obvious written out, which is the reason to write it
    /// out: the version that reads just as reasonably — clear the field once
    /// somebody *clicks* it — is a field you have to click twice, and that is
    /// not visible in the code that would have it.
    #[test]
    fn a_pointer_over_the_field_takes_it_back() {
        assert!(!picture_has_the_field(false, false, false, false));
        assert!(!picture_has_the_field(false, true, false, false));

        assert!(picture_has_the_field(true, false, false, false));
        assert!(!picture_has_the_field(true, true, false, false));
        assert!(!picture_has_the_field(true, false, true, false));
        assert!(!picture_has_the_field(true, false, false, true));
    }

    /// The marker used to be grown outwards from the row. That put its
    /// right-hand padding outside the scroll area's clip rectangle, so the air
    /// showed on the left and was cut off on the right — which is exactly how
    /// it looked. Keeping the marker inside the row and insetting the text
    /// instead makes both sides the same by construction.
    #[test]
    fn the_marker_has_the_same_air_on_both_sides() {
        for m in [
            Metrics::default(),
            Metrics::for_size(820.0, 440.0),
            Metrics::for_size(2400.0, 1600.0),
        ] {
            let row = Rect::from_min_size(egui::pos2(100.0, 40.0), Vec2::new(300.0, m.row));
            let marker = marker_rect(row, m);
            let content = content_rect(row, m);

            let left = content.left() - marker.left();
            let right = marker.right() - content.right();
            assert_eq!(left, right, "the marker is lopsided");
            assert_eq!(left, m.marker_pad_x);

            assert!(marker.left() >= row.left());
            assert!(marker.right() <= row.right());

            assert!(marker.top() > row.top());
            assert!(marker.bottom() < row.bottom());
            assert!(marker.height() >= theme::SIZE_CONTENT);
        }
    }

    /// What the mark has to get right: it never leaves its column, it never
    /// shrinks to nothing however long the library is, and it reaches the
    /// bottom exactly when the list does.
    #[test]
    fn the_extent_mark_stays_inside_its_column() {
        let visible = 654.0;

        assert_eq!(extent_mark(visible, 400.0, 0.0), None);
        assert_eq!(extent_mark(visible, visible, 0.0), None);

        for content in [700.0, 1_200.0, 9_000.0, 147_600.0] {
            let max_offset = content - visible;
            for offset in [0.0, max_offset * 0.5, max_offset] {
                let (top, length) = extent_mark(visible, content, offset).unwrap();
                assert!(length >= EXTENT_MIN_LENGTH, "{content} gave {length}");
                assert!(top >= -0.01, "{content}/{offset} started at {top}");
                assert!(
                    top + length <= visible + 0.01,
                    "{content}/{offset} ran to {}",
                    top + length
                );
            }
        }

        let (top, _) = extent_mark(visible, 9_000.0, 0.0).unwrap();
        assert!(top.abs() < 0.01);
        let (top, length) = extent_mark(visible, 9_000.0, 9_000.0 - visible).unwrap();
        assert!((top + length - visible).abs() < 0.01);
    }

    /// Without the modes the header keeps the transport's three keys, ending
    /// where the five did, the same gap apart.
    #[test]
    fn the_header_keeps_three_keys_without_the_modes() {
        let all = header_cells(1000.0, 800.0, 50.0, true);
        let three = header_cells(1000.0, 800.0, 50.0, false);
        assert_eq!(all.len(), 5);
        assert_eq!(three.len(), 3);
        assert_eq!(three[2].right(), 1000.0);
        assert_eq!(three[1].width(), 50.0);
        assert_eq!(all[2].width(), three[2].width());
        assert_eq!(three[0].left(), all[0].left() + (all[4].right() - all[2].right()));
    }

    /// The five keys stand on one line, ending at the corner, in two groups:
    /// back, play and on, then repeat and shuffle further off, with the
    /// play key the only wide one.
    #[test]
    fn the_keys_line_up_in_the_corner() {
        let cells = key_cells(1000.0, 800.0, 50.0);
        assert_eq!(cells[4].right(), 1000.0);
        assert_eq!(cells[1].width(), 50.0);
        let apart: Vec<f32> = cells.windows(2).map(|pair| pair[1].left() - pair[0].right()).collect();
        assert_eq!(apart, vec![RING_STEP - RING_CELL, RING_STEP - RING_CELL, MODES_APART, RING_STEP - RING_CELL]);
        for cell in cells {
            assert_eq!(cell.bottom(), 800.0);
            assert_eq!(cell.height(), RING_CELL);
        }
        assert_eq!(1000.0 - cells[0].left(), RING_CELL * 4.0 + 50.0 + apart.iter().sum::<f32>());
    }

    /// Every place reads back as itself, and the places that have been taken
    /// out open on the nearest one that is left.
    #[test]
    fn the_play_key_says_what_pressing_it_does() {
        use crate::player::PlaybackState;
        assert_eq!(play_word(PlaybackState::Playing), ("pause", false));
        assert_eq!(play_word(PlaybackState::Paused), ("play", false));
        assert_eq!(play_word(PlaybackState::Stopped), ("play", false));
    }

    #[test]
    fn the_three_places_read_back() {
        for place in PlayingAt::ALL {
            let mut ui = crate::config::UiConfig::default();
            place.store(&mut ui);
            assert_eq!(PlayingAt::of(&ui), place, "{}", place.key());
        }
        let old = |at: &str| crate::config::UiConfig {
            playing_at: at.into(),
            ..Default::default()
        };
        for gone in ["kopfzeile", "kopfzeile 2", "fusszeile", "zeile", "cover", ""] {
            assert_eq!(PlayingAt::of(&old(gone)), PlayingAt::Compact, "{gone}");
        }
        assert_eq!(PlayingAt::of(&old("kompakt 2")), PlayingAt::Compact);
    }

    /// The head counts what is still to come, not the evening so far.
    #[test]
    fn the_queue_counts_what_is_left() {
        assert_eq!(queue_to_come(82, None), 82);
        assert_eq!(queue_to_come(82, Some(0)), 81);
        assert_eq!(queue_to_come(85, Some(3)), 81);
        assert_eq!(queue_to_come(5, Some(4)), 0);
        assert_eq!(queue_to_come(0, None), 0);

        let p = super::super::theme::Theme::MacosHell.palette();
        let spent = RowState {
            spent: true,
            ..Default::default()
        };
        assert_eq!(spent.primary(&p), p.faint);
        let chosen = RowState {
            selected: true,
            ..spent
        };
        assert_eq!(chosen.primary(&p), p.ink);
        assert_eq!(chosen.role(), Role::ContentBold);
        let view = RowState { muted: true, selected: true, ..Default::default() };
        assert_eq!(view.primary(&p), p.mid);
        assert_eq!(view.role(), Role::ContentBold);
    }

    /// The playing row's second tone is the receded accent, chosen or
    /// marked; a marked row's figures are those of any other row.
    #[test]
    fn the_playing_row_keeps_its_accent_when_marked() {
        let p = super::super::theme::Theme::RunebendingDunkel.palette();
        let playing = RowState { playing: true, ..Default::default() };
        assert_eq!(playing.secondary(&p), p.accent_faint);
        let chosen = RowState { selected: true, ..playing };
        assert_eq!(chosen.secondary(&p), p.accent_faint);
        let marked = RowState { focused: true, ..chosen };
        assert_eq!(marked.secondary_tone(&p), p.accent_faint);
        let marked_not_playing = RowState { playing: false, ..marked };
        assert_eq!(marked_not_playing.secondary_tone(&p), p.faint);
    }

    /// The plates say where the hand is, the weight what is chosen.
    #[test]
    fn the_hand_has_the_plate_and_the_choice_the_weight() {
        let chosen = RowState { selected: true, ..Default::default() };
        let marked = RowState { focused: true, ..chosen };
        assert!(marked.plated());
        assert_eq!(marked.role(), Role::ContentBold);
        assert!(!chosen.plated());
        assert_eq!(chosen.role(), Role::ContentBold);
        assert_eq!(chosen.role(), Role::ContentBold);
        let playing = RowState { playing: true, ..Default::default() };
        assert!(!playing.plated());
        assert_eq!(playing.role(), Role::ContentBold);
        assert_eq!(RowState::default().role(), Role::Content);
    }

    /// The two left heads, in both states the column can be in.
    ///
    /// The window opens with the numbers shown, so it opens with the arrow at
    /// „nr" — which is where one looks for it, and where the figures under
    /// it are actually counting up. Switch that column off and the order moves
    /// to „titel" rather than vanishing, which is the whole point of the rule:
    /// never two arrows on this side, never none.
    #[test]
    fn the_shelf_order_always_has_exactly_one_head() {
        for numbered in [true, false] {
            let (nr, title) = left_heads(numbered);
            let holding: Vec<Option<SortField>> =
                [nr, title].into_iter().filter(|h| h.is_some()).collect();
            assert_eq!(holding, vec![Some(SortField::Artist)]);
        }

        assert_eq!(left_heads(true).0, Some(SortField::Artist), "nr holds it");
        assert_eq!(left_heads(false).1, Some(SortField::Artist), "titel takes over");

        assert!(TrackColumn::DEFAULT.contains(&TrackColumn::Number));
    }

    /// The egg in the snake.
    ///
    /// At rest the knob is not hidden, not scaled to nothing, not skipped: it
    /// is exactly as wide as the line is thick, which is the same thing as
    /// being the line. That is what makes it swell *out* of the rule instead
    /// of appearing on top of it, and it is the one number in this that a
    /// later hand could quietly break — a knob that starts at zero pops, a
    /// knob that starts wider than the line is a dot sitting on it.
    #[test]
    fn the_knob_starts_as_the_line_and_swells_out_of_it() {
        let (thin, at_rest) = rule_swell(0.0);
        assert_eq!(thin, RULE_THIN);
        assert_eq!(at_rest, RULE_THIN / 2.0, "at rest the knob is the line");

        let (thick, knob) = rule_swell(1.0);
        assert_eq!(thick, RULE_THICK);
        assert_eq!(knob, KNOB);

        let mut last = (RULE_THIN, RULE_THIN / 2.0);
        for step in 1..=20 {
            let now = rule_swell(step as f32 / 20.0);
            assert!(now.0 >= last.0 && now.1 > last.1, "at {step}/20 it went back");
            assert!(now.0 <= RULE_THICK && now.1 <= KNOB, "at {step}/20 it overshot");
            assert!(now.1 >= now.0 / 2.0, "at {step}/20 the line outgrew the knob");
            last = now;
        }
    }

    /// A hover plate sits square on its word.
    ///
    /// The one this replaced was grown from the type's box, and a box in this
    /// face keeps room under the baseline for descenders *and* a line gap
    /// under that — so the plate carried three points more air below the word
    /// than above it. Three points at eleven point type: nothing anybody can
    /// name, everything anybody can see.
    #[test]
    fn a_hover_plate_sits_square_on_its_word() {
        let ink_middle = 40.0;
        let ink = 8.0;
        let plate = plate_around(ink_middle, ink, 100.0, 30.0);

        let above = (ink_middle - ink / 2.0) - plate.top();
        let below = plate.bottom() - (ink_middle + ink / 2.0);
        assert_eq!(above, below, "the word hangs in its plate");
        assert_eq!(above, CONTROL_PAD_Y);

        assert_eq!(plate.left(), 100.0 - CONTROL_PAD_X);
        assert_eq!(plate.right(), 130.0 + CONTROL_PAD_X);

        assert_eq!(plate.height(), 24.0);
    }

    /// The scope and the clock wake on one grid: run both for a minute of
    /// frames, each frame asking for the sooner of the two, and there are as
    /// many frames as the scope's alone — at every rate in the menu, on a
    /// screen of 60 and of 120 a second, with egui taking a frame of the
    /// screen's off every wait as it does.
    #[test]
    fn the_clock_does_not_add_frames_to_the_scope() {
        use super::picture::on_grid;
        for early in [1.0 / 60.0, 1.0 / 120.0, 0.0] {
            for rate in [20.0, 12.0, 8.0] {
                let mut now = 0.0137;
                let mut frames = 0;
                while now < 60.0 {
                    let scope = on_grid(now, 1.0 / rate, early).as_secs_f64();
                    let clock = on_grid(now, 0.25, early).as_secs_f64();
                    now += (scope.min(clock) - early).max(0.0) + 0.001;
                    frames += 1;
                }
                let wanted = (60.0 * rate) as i32;
                assert!((frames - wanted).abs() <= 1, "{rate} ({early}): {frames} frames, not {wanted}");
            }
        }
    }

    /// Woken a hair before a tick, the frame does not ask for that tick
    /// again a millisecond on.
    #[test]
    fn a_frame_just_before_a_tick_waits_for_the_next() {
        let wait = super::picture::on_grid(0.0499, 0.05, 0.0).as_secs_f64();
        assert!((wait - 0.0501).abs() < 1e-9, "{wait}");
    }
}
