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

pub(super) use rows::{DeviceMark, DEVICE_MARKS};
pub(in crate::gui) use rows::cursor_row;
pub(super) use rows::{lead_arrow, paint_extent, paint_fade, without_a_bar, ARROW_SECS};
pub(super) use masthead::PlayingAt;
pub(super) use controls::{control_plate, paint_control_hover, PLAY_WORDS};
pub(super) use switch::paint_underline;
pub(super) use field_art::{field_rate, field_tick, grey_for as field_grey, on_the_second, FieldArt, FieldPicture, Reading, FIELD_RATES};
pub(super) use picture::{live_mark, picture_step, scope_step, to_tick, EXPOSED_RATE, SECOND_RATE};

const CELL_GAP: f32 = 20.0;

const VOLUME_WIDTH: f32 = 92.0;

pub(super) const HEAD_AIR: f32 = 6.0;

pub(super) const BLOCK_AIR: f32 = 8.0;

fn capped(height: f32) -> u8 {
    if super::theme::square_ends() {
        return 0;
    }
    (height / 2.0).round().clamp(0.0, 255.0) as u8
}

fn knob_at(ui: &Ui, at: egui::Pos2, radius: f32, line: f32, colour: Color32) {
    if super::theme::square_ends() {
        let rest = line / 2.0;
        let t = ((radius - rest) / (KNOB - rest).max(0.01)).clamp(0.0, 1.0);
        let tall = line + (BAR_TALL - line) * t;
        ui.painter().rect_filled(Rect::from_center_size(at, Vec2::new(BAR_WIDE, tall)), 0.0, colour);
    } else {
        ui.painter().circle_filled(at, radius, colour);
    }
}

const KNOB: f32 = 5.0;

const BAR_WIDE: f32 = 4.0;

const BAR_TALL: f32 = 12.0;

const RULE_THIN: f32 = 2.0;

const RULE_THICK: f32 = 4.0;

const SWELL_TIME: f32 = 0.13;

fn swell(ui: &Ui, which: &'static str, hot: bool) -> f32 {
    ui.ctx().animate_bool_with_time_and_easing(
        egui::Id::unique(("swell", which)),
        hot,
        SWELL_TIME,
        egui::emath::easing::cubic_out,
    )
}

fn rule_swell(t: f32) -> (f32, f32) {
    let thickness = RULE_THIN + (RULE_THICK - RULE_THIN) * t;
    let knob = thickness / 2.0 + (KNOB - thickness / 2.0) * t;
    (thickness, knob)
}

pub(in crate::gui) const MIN_FIELD: f32 = 118.0;

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

const EDGE_FADE: f32 = 22.0;

const KEY_SIZE: f32 = 22.0;

const GONE: &str = "fehlt";

#[derive(Debug, Clone, Copy, Default)]
struct RowState {
    selected: bool,
    focused: bool,
    playing: bool,
    muted: bool,
    gone: bool,
    spent: bool,
    offering: bool,
    arrow: bool,
    pointed: bool,
}

impl RowState {
    fn plated(self) -> bool {
        self.selected && self.focused
    }

    fn role(self) -> Role {
        if self.spent && !self.playing {
            Role::Content
        } else if self.selected || self.playing {
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
        } else if self.spent && self.selected {
            p.mid
        } else if self.spent {
            p.faint
        } else if self.muted {
            p.mid
        } else {
            p.ink
        }
    }

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

fn behind(ui: &Ui, state: RowState, hovered: bool, palette: &Palette) -> Color32 {
    if state.plated() {
        marker_colour(palette)
    } else if hovered {
        hover_colour(ui, palette)
    } else {
        palette.ground
    }
}

fn wheel_step(speed: &str, row: f32, mac: bool) -> f32 {
    let normal = if mac { 40.0 } else { 5.0 * row };
    let factor = match speed {
        "langsam" => 2.0 / 3.0,
        "schnell" => 5.0 / 3.0,
        _ => 1.0,
    };
    (normal * factor).round()
}

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

const SCROLL_SETTLES: f64 = 0.1;
const HOVER_FADE: f32 = 0.08;

fn hover_colour(ui: &Ui, palette: &Palette) -> Color32 {
    palette.ground.lerp_to_gamma(palette.hover, hover_shown(ui.ctx()))
}

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

fn marker_colour(palette: &Palette) -> Color32 {
    palette.marked()
}

fn marker_rect(rect: Rect, m: Metrics) -> Rect {
    Rect::from_min_max(
        egui::pos2(rect.left(), rect.top() + m.marker_inset_y),
        egui::pos2(rect.right(), rect.bottom() - m.marker_inset_y),
    )
}

fn content_rect(rect: Rect, m: Metrics) -> Rect {
    Rect::from_min_max(
        egui::pos2(rect.left() + m.marker_pad_x, rect.top()),
        egui::pos2(rect.right() - m.marker_pad_x, rect.bottom()),
    )
}

fn paint_marker(ui: &Ui, rect: Rect, state: RowState, m: Metrics, palette: &Palette) {
    if !state.plated() {
        return;
    }
    ui.painter()
        .rect_filled(marker_rect(rect, m), MARKER_RADIUS, marker_colour(palette));
}

fn paint_hover(
    ui: &Ui,
    rect: Rect,
    response: &Response,
    state: RowState,
    m: Metrics,
    palette: &Palette,
) {
    if (response.hovered() || state.pointed) && !state.plated() {
        ui.painter()
            .rect_filled(marker_rect(rect, m), MARKER_RADIUS, hover_colour(ui, palette));
    }
}

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

fn slide(ctx: &egui::Context, id: egui::Id, on: bool, snap: bool) -> f32 {
    slide_for(ctx, id, on, snap, theme::FOURTH_SLIDE)
}

#[derive(Clone, Copy, Default)]
pub(super) struct FootInk {
    pub(super) left_ends: Option<f32>,
    pub(super) right_begins: Option<f32>,
}

pub(super) fn foot_ink_id() -> egui::Id {
    egui::Id::unique("foot-ink")
}

const FOOT_CLEAR: f32 = 12.0;

fn slide_for(ctx: &egui::Context, id: egui::Id, on: bool, snap: bool, secs: f32) -> f32 {
    slide_to(ctx, id, if on { 1.0 } else { 0.0 }, snap, secs, egui::emath::easing::cubic_out)
}

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
    let palette = app.palette;
    let window = ui.available_rect_before_wrap();
    if ui.interact(window, egui::Id::unique("window-floor"), Sense::click()).clicked() {
        app.let_go();
    }
    let mut m = Metrics::for_size(window.width(), window.height()).with_air().with_margins();
    set_wheel_step(ui.ctx(), &app.config.ui.scroll_speed, m.row);
    let super_compact = PlayingAt::of(&app.config.ui) == PlayingAt::SuperCompact;
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
        let as_share = |share: Option<f32>, points: Option<f32>| {
            share.or_else(|| points.map(|p| p / room))
        };
        theme::Shares {
            artists: as_share(c.artists_share, c.artists_width),
            albums: as_share(c.albums_share, c.albums_width),
            queue: as_share(c.queue_share, c.queue_width),
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
    let compact = PlayingAt::of(&app.config.ui) == PlayingAt::Compact;
    let in_the_band = !narrow && (super_compact || (on_a_mac && !compact));
    let all_off = app.all_off();
    let menu_in_the_band = (on_a_mac && narrow) || (compact && !narrow) || all_off;
    let library_at_foot = app.config.ui.library_line && !app.showing_podcasts() && !all_off && app.config.ui.music;
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
    if on_a_mac {
        let band = Rect::from_min_size(window.min, Vec2::new(window.width(), crate::gui::macwindow::BY_HAND_MIDDLE * 2.0));
        let held = ui.interact(band, egui::Id::unique("window-band"), Sense::click_and_drag());
        if held.drag_started_by(egui::PointerButton::Primary) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        if held.double_clicked() {
            crate::gui::macwindow::zoom();
            ui.ctx().request_repaint();
        }
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

    outer.show(ui, |ui| {
        let full_width = ui.available_width();

        let Masthead { field } = if all_off {
            Masthead { field: Rect::NOTHING }
        } else if narrow {
            masthead_narrow(app, ui, &palette, m, full_width, !menu_in_the_band)
        } else {
            masthead_compact(app, ui, &palette, m, full_width, in_the_band, !menu_in_the_band)
        };
        if narrow && !all_off {
            ui.add_space(32.0);
            narrow_playing(app, ui, &palette, m, full_width);
            ui.add_space(28.0);
        }

        const COMPACT_FOOT: f32 = 44.0;
        let foot = if status_at_foot { COMPACT_FOOT } else { 0.0 };
        let columns_height = (ui.available_height() - foot).max(120.0);
        let ink: FootInk = ui.data(|d| d.get_temp(foot_ink_id())).unwrap_or_default();
        let deeper = (foot - (m.margin_x - theme::MARGIN_BOTTOM).max(0.0)).max(0.0);
        let reach = |ui: &Ui, n: usize, width: f32| -> f32 {
            let from = ui.cursor().min.x;
            let under = ink.left_ends.is_some_and(|end| from < end + FOOT_CLEAR)
                || ink.right_begins.is_some_and(|begin| from + width > begin - FOOT_CLEAR);
            let down = slide_for(ui.ctx(), egui::Id::unique(("column-down", n)), !under, false, theme::FOURTH_SLIDE);
            ui.ctx().data_mut(|d| d.insert_temp(column_deeper_id(), deeper * down));
            columns_height + deeper * down
        };
        ui.data_mut(|d| d.remove_temp::<f32>(column_deeper_id()));
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
                podcast_row(app, ui, m, &palette, full_width, podcasts_narrow, columns_height, &reach_column);
            });
        } else if app.all_off() {
            ui.allocate_ui(Vec2::new(full_width, columns_height), |ui| {
                all_off_note(app, ui, &palette, full_width, columns_height);
            });
        } else if narrow && app.showing_radio() {
            radio_strip(app, ui, m, &palette, full_width, columns_height);
        } else if narrow {
            sliding_columns(app, ui, m, &palette, full_width, columns_height);
        } else if app.showing_radio() {
            ui.allocate_ui(Vec2::new(full_width, columns_height), |ui| {
                radio_row(app, ui, m, &palette, full_width);
            });
        } else {
            ui.allocate_ui(Vec2::new(full_width, columns_height + deeper), |ui| {
                ui.horizontal_top(|ui| {
                    let h = reach(ui, 0, m.artists_width);
                    taken.push(ui.allocate_ui(Vec2::new(m.artists_width, h), |ui| first_column(app, ui, m, &palette)).inner);
                    if !app.merged() {
                        ui.add_space(m.column_gap);
                        let h = reach(ui, 1, m.albums_width);
                        taken.push(ui.allocate_ui(Vec2::new(m.albums_width, h), |ui| albums_column(app, ui, m, &palette)).inner);
                    }
                    ui.add_space(m.column_gap);
                    let room = ui.available_width() - queue_width - queue_gap - side_part;
                    let h = reach(ui, 2, room);
                    taken.push(ui.allocate_ui(Vec2::new(room, h), |ui| tracks_column(app, ui, m, &palette)).inner);
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
                app.config.ui.artists_width = None;
                app.config.ui.queue_share = Some(queue_full / full_width);
                app.config.ui.queue_width = None;
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
                    app.config.ui.artists_width = None;
                }
                i if i == names - 1 => {
                    app.config.ui.albums_share = share(
                        width,
                        m.artists_width + m.column_gap * 2.0 + queue_width,
                    );
                    app.config.ui.albums_width = None;
                }
                i if i == names + 1 => {
                    let pair = queue_width + side_width;
                    let queue = width.clamp(theme::MIN_NAME_COLUMN, pair - theme::MIN_NAME_COLUMN);
                    app.config.ui.queue_share = Some(queue / full_width);
                    app.config.ui.queue_width = None;
                    app.config.ui.side_share = Some((pair - queue) / full_width);
                }
                _ => {
                    let taken_left = queue_others - m.column_gap + width;
                    let wanted = share(full_width - taken_left - side_part, queue_others + side_part);
                    if queue_width > 0.5 {
                        app.config.ui.queue_share = wanted;
                        app.config.ui.queue_width = None;
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
                app.config.ui.artists_width = None;
                if !app.merged() {
                    app.config.ui.albums_share = even;
                    app.config.ui.albums_width = None;
                }
            }
            app.config.ui.queue_share = even;
            app.config.ui.queue_width = None;
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
            let said_from = header_status(app, ui, &palette, window.left() + m.margin_x, right, top, true, library_at_foot);
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

    carried(app, ui);
    dropping(app, ui, window);
    row_menu(app, ui, window);
    menu::render(app, ui, window, m);
    menu::keys_sheet(app, ui, window, m);
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

fn divider(
    ui: &mut Ui,
    id: usize,
    left: Rect,
    right: Rect,
    palette: &Palette,
) -> Option<Handle> {
    const GRAB: f32 = 11.0;
    const LINE: f32 = 2.0;
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

enum Handle {
    Width(f32, bool),
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

    #[test]
    fn a_pointer_over_the_field_takes_it_back() {
        assert!(!picture_has_the_field(false, false, false, false));
        assert!(!picture_has_the_field(false, true, false, false));

        assert!(picture_has_the_field(true, false, false, false));
        assert!(!picture_has_the_field(true, true, false, false));
        assert!(!picture_has_the_field(true, false, true, false));
        assert!(!picture_has_the_field(true, false, false, true));
    }

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
            assert!(!ui.header_playing);
        }
        let old = |at: &str| crate::config::UiConfig {
            playing_at: at.into(),
            header_playing: true,
            ..Default::default()
        };
        for gone in ["kopfzeile", "kopfzeile 2", "fusszeile", "zeile", "cover", ""] {
            assert_eq!(PlayingAt::of(&old(gone)), PlayingAt::Compact, "{gone}");
        }
        assert_eq!(PlayingAt::of(&old("kompakt 2")), PlayingAt::Compact);
    }

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
        assert_eq!(chosen.primary(&p), p.mid);
        assert_eq!(chosen.primary(&p), chosen.secondary(&p));
        assert_eq!(chosen.role(), Role::Content);
    }

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

    #[test]
    fn a_frame_just_before_a_tick_waits_for_the_next() {
        let wait = super::picture::on_grid(0.0499, 0.05, 0.0).as_secs_f64();
        assert!((wait - 0.0501).abs() < 1e-9, "{wait}");
    }
}
