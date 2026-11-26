//! What the columns read, chosen in the masthead: `musik   radio   ipod`, words
//! in a row as the menu's tabs are. „radio" only once it is asked for in the
//! menu, „ipod" only while one is plugged in; with neither, there is nothing to
//! choose and nothing is drawn.
//!
//! All the words alike, in the second tone and the regular cut, each in its
//! place; the one open is underlined by a line as heavy as the face's own
//! strokes, sitting a little lower than an underline usually does, its ends
//! only just rounded, and the line wanders to the word chosen. Under the
//! pointer a word is in the ink, on the plate every control wears. A word newly
//! offered — an iPod plugged in — stands in the accent for a moment.
//!
//! until then `[ui] source_switch` also offered square brackets around the one
//! open, the others folding away behind it or showing only their first letters,
//! and before that a slash, a bar, round brackets, plates and a segmented
//! control.
//!
//! „ipod" opens as a place to put tracks down while they are carried from the
//! library.

use super::*;

/// Which the columns read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Source {
    Music,
    Radio,
    Device,
    /// The podcasts, while they are on under „allgemein".
    Podcasts,
}

/// What became of the switch this frame.
pub(super) enum Switched {
    Still,
    /// Another was chosen.
    To(Source),
    /// Tracks carried from the library were let go of over „ipod".
    Dropped,
}

/// How wide „ipod" opens while tracks are carried, as a place to put them
/// down: about as wide as a short title, so it reads as somewhere to aim at
/// rather than as a word that got bigger.
const DROP_WIDTH: f32 = 160.0;
/// The air either side of the words in the open place.
const DROP_AIR: f32 = 12.0;
/// How long it takes to open and to close again.
const DROP_SECS: f32 = 0.18;
/// The air between one word and the next: with only a line to tell the one
/// open, a little more than the brackets needed.
const WORD_AIR: f32 = 18.0;
/// How long a word newly offered — an iPod plugged in — stands in the
/// accent, fading back over the last part.
const NEWCOMER_SECS: f64 = 2.5;

/// How much of the accent tints the place to put tracks down, and how much
/// more while they are over it: one of the scheme's own colours, so it reads
/// as somewhere to aim at.
const DROP_TINT: f32 = 0.3;
const DROP_TINT_OVER: f32 = 0.55;

/// The line under the open word: as heavy as the face's own
/// strokes, a little lower than an underline usually sits, its ends only
/// just rounded — as fractions of how tall the ink stands.
const UNDERLINE_WEIGHT: f32 = 0.11;
const UNDERLINE_DROP: f32 = 0.3;
const UNDERLINE_ROUND: f32 = 0.35;
/// In Muto, whose labels are its medium, heavier to go with it and a point and
/// a half lower, its ends square as its rules'.
const UNDERLINE_WEIGHT_MEDIUM: f32 = 0.15;
const UNDERLINE_LOWER_MEDIUM: f32 = 1.5;
/// How long it takes to wander to another word.
const UNDERLINE_SECS: f32 = 0.26;

pub(in crate::gui) fn paint_underline(ui: &Ui, left: f32, width: f32, baseline: f32, ink: f32, tone: Color32) {
    let medium = crate::gui::theme::square_ends();
    let weight = (ink * if medium { UNDERLINE_WEIGHT_MEDIUM } else { UNDERLINE_WEIGHT }).max(1.0);
    let y = baseline + ink * UNDERLINE_DROP + if medium { UNDERLINE_LOWER_MEDIUM } else { 0.0 };
    let line = Rect::from_min_size(egui::pos2(left, y), egui::vec2(width, weight));
    ui.painter().rect_filled(line, if medium { 0.0 } else { weight * UNDERLINE_ROUND }, tone);
}

/// A value gliding by the clock to wherever it is asked to be, eased out,
/// over `secs` whatever the distance: the line under the open word.
fn glide(ctx: &egui::Context, id: egui::Id, to: f32, secs: f32) -> f32 {
    #[derive(Clone, Copy)]
    struct Glide {
        from: f32,
        to: f32,
        since: f64,
    }
    let now = ctx.input(|i| i.time);
    let at = |g: Glide| {
        let t = ((now - g.since) / f64::from(secs)).clamp(0.0, 1.0) as f32;
        (g.from + (g.to - g.from) * egui::emath::easing::cubic_out(t), t)
    };
    let held = ctx.data(|d| d.get_temp::<Glide>(id));
    let glide = match held {
        Some(g) if (g.to - to).abs() < 0.01 => g,
        Some(g) => Glide { from: at(g).0, to, since: now },
        None => Glide { from: to, to, since: now },
    };
    ctx.data_mut(|d| d.insert_temp(id, glide));
    let (value, t) = at(glide);
    if t < 1.0 {
        ctx.request_repaint();
    }
    value
}

/// The switch, laid out leftwards from `right`. The views the switch offers, in
/// the order of their keys, `1` to `4` (the podcasts before the ipod) — the
/// same list `1` `2` `3` go by.
pub(super) fn views_offered(app: &GuiApp) -> Vec<Source> {
    [
        app.config.ui.music.then_some(Source::Music),
        app.config.ui.radio.then_some(Source::Radio),
        app.config.ui.podcasts.then_some(Source::Podcasts),
        app.attached.is_some().then_some(Source::Device),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// How long the word of a view on its own stands after arriving in it,
/// before it fades, and how long the fading takes.
const LONE_SECS: f64 = 2.5;
const LONE_FADE_SECS: f64 = 0.6;
/// Under „super kompakt", once the lone word has faded, how long its room
/// takes to close: the line is short there, and a gap left for a word nobody
/// sees is room the rest of it can use.
const LONE_CLOSE_SECS: f64 = 0.4;

pub(super) fn source_switch(app: &GuiApp, ui: &Ui, right: &mut f32, top: f32, palette: &Palette) -> Switched {
    let offered = views_offered(app);
    let open_now = if app.showing_podcasts() {
        Source::Podcasts
    } else if app.showing_radio() {
        Source::Radio
    } else if app.showing_ipod() {
        Source::Device
    } else {
        Source::Music
    };
    if offered.len() < 2 {
        if offered.first() == Some(&open_now) {
            let gives_way = PlayingAt::now(&app.config.ui) == PlayingAt::SuperCompact;
            lone_word(ui, right, top, palette, open_now, gives_way);
        } else {
            ui.ctx().data_mut(|d| d.remove::<(Source, f64)>(egui::Id::unique("switch-lone-since")));
        }
        return Switched::Still;
    }
    ui.ctx().data_mut(|d| d.remove::<(Source, f64)>(egui::Id::unique("switch-lone-since")));
    let ctx = ui.ctx().clone();
    let now = ui.input(|i| i.time);
    let offered_id = egui::Id::unique("switch-offered");
    let newcomer_id = egui::Id::unique("switch-newcomer");
    let had: Option<Vec<Source>> = ctx.data(|d| d.get_temp(offered_id));
    if let Some(had) = &had {
        if let Some(new) = offered.iter().find(|s| !had.contains(s)) {
            ctx.data_mut(|d| d.insert_temp(newcomer_id, (*new, now)));
        }
    }
    ctx.data_mut(|d| d.insert_temp(offered_id, offered.clone()));
    let newcomer: Option<(Source, f64)> = ctx
        .data(|d| d.get_temp::<(Source, f64)>(newcomer_id))
        .filter(|(_, since)| now - since < NEWCOMER_SECS);
    if let Some((_, since)) = newcomer {
        ctx.request_repaint_after(std::time::Duration::from_secs_f64((since + NEWCOMER_SECS - now).max(0.0)));
    }

    let takes = matches!(app.dragging, Some(crate::gui::Dragged::Incoming(_)))
        && open_now != Source::Device
        && app.attached.is_some()
        && app.refuses_drops().is_none();
    let open = ui.ctx().animate_bool_with_time(egui::Id::unique("device-switch-open"), takes, DROP_SECS);
    let drop_word = tr!("hierher ziehen zum hinzufügen");
    let drop_size = text::line(ui, drop_word, Role::Label, palette.mid).size();

    let word_of = |source: Source| match source {
        Source::Music => tr!("musik"),
        Source::Radio => tr!("radio"),
        Source::Podcasts => "podcasts",
        Source::Device => "ipod",
    };
    let wide_of = |source: Source| {
        let cell = text::line(ui, word_of(source), Role::Label, palette.mid).size().x;
        if source == Source::Device {
            let opened = DROP_WIDTH.max(drop_size.x + 2.0 * DROP_AIR);
            cell + (opened - cell).max(0.0) * open
        } else {
            cell
        }
    };

    let end = *right;
    let mut cells: Vec<(Source, &str, Rect)> = Vec::new();
    for (at, source) in offered.iter().enumerate().rev() {
        let wide = wide_of(*source);
        *right -= wide;
        cells.push((*source, word_of(*source), control_plate(ui, egui::pos2(*right, top), wide)));
        if at > 0 {
            *right -= WORD_AIR;
        }
    }
    *right -= CONTROL_GAP;

    let mut switched = Switched::Still;
    let mut underline: Option<(f32, f32, Color32)> = None;
    for (source, word, plate) in cells {
        let response = ui.interact(plate, egui::Id::unique(("source-switch", word)), Sense::click());
        let is_open = source == open_now;
        let drop = source == Source::Device && open > 0.0;
        let over = drop && takes && ui.ctx().pointer_latest_pos().is_some_and(|at| plate.contains(at));
        if drop {
            let tint = if over { DROP_TINT_OVER } else { DROP_TINT };
            let fill = palette.field.lerp_to_gamma(palette.accent, tint);
            ui.painter().rect_filled(plate, crate::gui::theme::ROW_RADIUS, fill.gamma_multiply(open));
        } else if !is_open {
            paint_control_hover(ui, plate, &response, palette);
        }
        let colour = if response.hovered() || (drop && takes) {
            palette.ink
        } else if let Some((_, since)) = newcomer.filter(|(new, _)| *new == source) {
            let back = (((now - since) / NEWCOMER_SECS - 0.5) * 2.0).clamp(0.0, 1.0) as f32;
            palette.accent.lerp_to_gamma(palette.mid, back)
        } else {
            palette.mid
        };
        if !is_open && response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let painter = ui.painter().with_clip_rect(plate);
        let left = plate.left() + control_pad(ui, plate);
        let room = plate.width() - 2.0 * control_pad(ui, plate);
        let words = if source == Source::Device { vec![(word, 1.0 - open), (drop_word, open)] } else { vec![(word, 1.0)] };
        for (said, shown) in words {
            if shown > 0.0 {
                let tone = colour.gamma_multiply(shown);
                let galley = text::line(ui, said, Role::Label, tone);
                let said_width = galley.size().x;
                let start = left + (room - said_width) / 2.0;
                if is_open && said == word {
                    underline = Some((start, said_width, tone));
                }
                painter.galley(egui::pos2(start, top), galley, tone);
            }
        }
        if over && ui.input(|i| i.pointer.any_released()) {
            switched = Switched::Dropped;
        } else if response.clicked() && !is_open {
            switched = Switched::To(source);
        }
    }
    if let Some((left, width, tone)) = underline {
        let left = end - glide(&ctx, egui::Id::unique("switch-underline-from-end"), end - left, UNDERLINE_SECS);
        let width = glide(&ctx, egui::Id::unique("switch-underline-width"), width, UNDERLINE_SECS);
        let ink = text::ink_height(ui, Role::Label);
        let baseline = text::ink_middle(ui, Role::Label, top) + ink / 2.0;
        paint_underline(ui, left, width, baseline, ink, tone);
    }
    if open > 0.0 && open < 1.0 {
        ui.ctx().request_repaint();
    }
    switched
}

/// Only one view switched on, there is nothing to choose — but where the window
/// is should still be said. Its word, underlined as in the switch, for a moment
/// after arriving there — the window opening, a view switched off, `c` and a
/// digit — and then fading; under the pointer it comes back. Under „super
/// kompakt" its room closes after it instead, and it stays away.
fn lone_word(ui: &Ui, right: &mut f32, top: f32, palette: &Palette, view: Source, gives_way: bool) {
    let word = match view {
        Source::Music => tr!("musik"),
        Source::Radio => tr!("radio"),
        Source::Podcasts => "podcasts",
        Source::Device => "ipod",
    };
    let ctx = ui.ctx();
    let now = ui.input(|i| i.time);
    let since_id = egui::Id::unique("switch-lone-since");
    let since = match ctx.data(|d| d.get_temp::<(Source, f64)>(since_id)) {
        Some((was, at)) if was == view => at,
        _ => {
            ctx.data_mut(|d| d.insert_temp(since_id, (view, now)));
            now
        }
    };
    let left = now - since;
    let shown = if left < LONE_SECS { 1.0 } else { (1.0 - (left - LONE_SECS) / LONE_FADE_SECS).max(0.0) } as f32;
    let closing = left - LONE_SECS - LONE_FADE_SECS;
    let room = if !gives_way || closing <= 0.0 {
        1.0
    } else {
        1.0 - egui::emath::easing::cubic_in_out((closing / LONE_CLOSE_SECS).min(1.0) as f32)
    };
    if shown > 0.0 || (room > 0.0 && room < 1.0) {
        ctx.request_repaint();
    }
    let wide = text::line(ui, word, Role::Label, palette.mid).size().x;
    *right -= (wide + CONTROL_GAP) * room;
    if room < 1.0 {
        return;
    }
    let plate = control_plate(ui, egui::pos2(*right + CONTROL_GAP, top), wide);
    let over = ui.rect_contains_pointer(plate);
    let shown = shown.max(ctx.animate_bool_with_time(egui::Id::unique("switch-lone-over"), over, UNDERLINE_SECS));
    if shown <= 0.0 {
        return;
    }
    let tone = palette.mid.gamma_multiply(shown);
    let galley = text::line(ui, word, Role::Label, tone);
    let width = galley.size().x;
    let start = plate.left() + control_pad(ui, plate) + (plate.width() - 2.0 * control_pad(ui, plate) - width) / 2.0;
    ui.painter().galley(egui::pos2(start, top), galley, tone);
    let ink = text::ink_height(ui, Role::Label);
    let baseline = text::ink_middle(ui, Role::Label, top) + ink / 2.0;
    paint_underline(ui, start, width, baseline, ink, tone);
}

/// How far a plate reaches past its word on each side.
fn control_pad(ui: &Ui, plate: Rect) -> f32 {
    let probe = control_plate(ui, egui::pos2(0.0, plate.top()), 0.0);
    probe.width() / 2.0
}
