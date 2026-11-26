//! The picture that stands in for a cover when the music draws its own:
//! the spectrum strip and the live mark.

use super::*;

/// The music, as dots, inside the search field.
///
/// The quiet half of the two pictures this window draws of the sound — the
/// other one takes the whole screen. This one has to sit *behind* a word
/// somebody is reading and a caret somebody is watching, so it is built to be
/// glanced at and not looked at: the same ordered screen the covers are
/// dithered with, in the rule's own colour, a little over a third of the
/// field's height, hugging its floor.
///
/// It is drawn only while something is playing, and it settles rather than
/// stopping when the music does — see [`sound::Spectrum`]. Nothing here is
/// interactive and nothing here moves the layout: the field is the same size
/// with and without it, so a window with the strip switched off is the same
/// window.
/// How often a moving picture asks to be drawn, in milliseconds — with the
/// window in front, and with it behind.
///
/// Twenty frames a second while somebody is at it — thirty cost a quarter of
/// a core on a Mac. The light fades by the clock, not by the frame, so the
/// afterglow lasts as long as it did; it is drawn in fewer steps.
///
/// Five while the window does not have the keyboard, and that is the one that
/// saves something. This program is made to stand open all evening behind
/// whatever you are actually doing, and every one of these frames is a *whole
/// window* redrawn — all four columns, every row of type — because that is
/// what an immediate-mode interface costs to move one dot. Three of those
/// frames a second instead of thirty, for a picture nobody is looking at, is
/// the whole of it.
///
/// Not stopped, though. „Not focused" is not „not visible": the window may be
/// sitting in plain sight beside the one being typed in, and a picture that
/// stops dead whenever you click elsewhere says the music stopped.
pub(super) const PICTURE_STEP: u64 = 50;

/// The step in front for a rate from the menu: twenty, twelve or eight a
/// second, anything else the twenty it was.
pub(in crate::gui) fn scope_step(rate: u32) -> u64 {
    match rate {
        SECOND_RATE | EXPOSED_RATE | 8 | 12 => 1000 / u64::from(rate),
        _ => PICTURE_STEP,
    }
}

/// The rate that stands for the long-exposed scope: four pictures a second,
/// each a quarter of a second laid down at once — see [`look_in`].
pub(in crate::gui) const EXPOSED_RATE: u32 = 4;

/// The rate for the scope in step with the clock: one picture a second, the
/// whole second laid down at once, drawn in the frame the clock and the rule
/// draw anyway — the window is then drawn once a second while music plays, as
/// it is in the background. The position does not ask for frames of its own
/// then either; see `GuiApp::update`.
pub(in crate::gui) const SECOND_RATE: u32 = 1;

/// Whether `rate` lays down what it missed rather than the last moment.
pub(in crate::gui) fn is_exposed(rate: u32) -> bool {
    rate == EXPOSED_RATE || rate == SECOND_RATE
}

/// The steps a long exposure is laid down in: the twentieth of a second the
/// scope draws at when it is not exposed, so each step is what one of those
/// frames would have drawn.
const EXPOSED_STEP: f64 = 0.05;

/// Lay what was heard down on `scope`: the last moment of it, as always — or,
/// long-exposed, everything since the last picture, step by step, so a picture
/// four times a second holds the strokes of the five it stands for. The audio
/// costs next to nothing; what the window costs is each frame of it redrawn,
/// and there are a fifth as many.
pub(in crate::gui) fn look_in(
    scope: &mut crate::gui::scope::Scope,
    heard: &crate::player::audio::Heard,
    (width, height, places): (usize, usize, usize),
    now: f64,
    exposed: bool,
) {
    let rate = heard.rate();
    if !exposed {
        scope.untune();
        scope.look(&heard.recent_pairs(), rate, width, height, places, now);
        return;
    }
    scope.retune(crate::gui::scope::EXPOSED);
    let gone = scope.lit_at().map_or(EXPOSED_STEP, |then| (now - then).clamp(0.0, 1.0));
    let steps = ((gone / EXPOSED_STEP).round() as usize).clamp(1, 20);
    let per = (f64::from(rate) * EXPOSED_STEP) as usize;
    let window = heard.recent_pairs().len();
    let frames = heard.last_pairs(window + per * (steps - 1));
    for step in 0..steps {
        let back = steps - 1 - step;
        let end = frames.len().saturating_sub(per * back);
        let start = end.saturating_sub(window);
        scope.look(&frames[start..end], rate, width, height, places, now - EXPOSED_STEP * back as f64);
    }
}

/// Five a second behind another window — ten cost an eighth of a core for a
/// window nobody was in. The search field's picture fades out there
/// altogether (see
/// `masthead`); this is left to the screen saver, which is only ever on
/// while somebody is meant to be watching it.
pub(super) const PICTURE_STEP_AWAY: u64 = 200;

/// While the window cannot be seen at all — hidden with ⌘H, minimised, or
/// covered by another window — a second: the picture is there again within one
/// when the window comes back, and the frames between were drawn for nobody.
/// Ten a second cost a tenth of a core on a Mac for a window put away.
pub(super) const PICTURE_STEP_HIDDEN: u64 = 1000;

/// How long a moving picture should wait before asking for its next frame.
/// `rate`: the frames a second chosen in the menu for the window in front
/// (`[ui] scope_rate`); the steps away from the front are slower anyway.
pub(in crate::gui) fn picture_step(ctx: &egui::Context, rate: u32) -> std::time::Duration {
    let (watched, unseen) = ctx.input(|i| {
        let viewport = i.viewport();
        (
            i.focused,
            viewport.occluded == Some(true) || viewport.minimized == Some(true),
        )
    });
    if watched && !unseen {
        if rate == SECOND_RATE {
            return std::time::Duration::from_millis(1100);
        }
        let period = match rate {
            EXPOSED_RATE | 8 | 12 => 1.0 / f64::from(rate),
            _ => PICTURE_STEP as f64 / 1000.0,
        };
        return to_tick(ctx, period);
    }
    std::time::Duration::from_millis(if unseen { PICTURE_STEP_HIDDEN } else { PICTURE_STEP_AWAY })
}

/// How long until the next tick of a clock that ticks every `period` seconds
/// from the window's start.
///
/// The scope and the clock in the masthead both ask for frames while the window
/// is in front, and each used to count from the frame before: every frame of
/// the clock put the scope's next one back by its own width, and the two came
/// in between each other — twenty-eight frames a second for twenty, fourteen
/// for twelve. Counted from the same start, a quarter second is five, three or
/// two of the scope's steps, and the clock's frame is one of the scope's.
///
/// Woken a hair before a tick, the next one would be a millisecond away — so a
/// tick closer than a quarter of the period is skipped.
///
/// `early`: what egui takes off every wait it is asked for, a frame of the
/// screen's, given back here so the frame comes on the tick. Taken off, the
/// window woke 17 ms before each tick of the scope's 50, asked again for the
/// tick itself, and drew 28 frames a second for 20; the 83 ms of twelve a
/// second skipped the near tick and came out right.
pub(in crate::gui) fn on_grid(now: f64, period: f64, early: f64) -> std::time::Duration {
    let next = ((now / period).floor() + 1.0) * period;
    let mut wait = next - now;
    if wait < period * 0.25 {
        wait += period;
    }
    std::time::Duration::from_secs_f64(wait + early)
}

/// [`on_grid`] for this frame of the window.
pub(in crate::gui) fn to_tick(ctx: &egui::Context, period: f64) -> std::time::Duration {
    let (now, early) = ctx.input(|i| (i.time, f64::from(i.predicted_dt)));
    on_grid(now, period, early)
}

/// How long the field takes to change hands, in seconds.
///
/// Short enough that a pointer arriving at the field finds the word already
/// there, long enough that the two are seen to trade places rather than to
/// blink.
pub(super) const FIELD_FADE: f32 = 0.16;

/// How long the picture takes to go when the window leaves the front, and
/// to come back when it returns, in seconds.
pub(super) const AWAY_FADE: f32 = 0.6;

/// Whether the picture has the search field, or somebody wanting to type has.
///
/// The rule in one place, and the one thing about it worth writing down: a
/// pointer over the field wins whatever else is true. Somebody moving the
/// mouse there is already on their way to typing, and a field that only clears
/// itself once you have clicked is a field you have to click twice.
///
/// Nor while there are words in it, typed and then left: they stay on show
/// until the field is empty again, or the picture runs over what is being
/// searched for.
pub(super) fn picture_has_the_field(drawing: bool, hovered: bool, typing: bool, worded: bool) -> bool {
    drawing && !hovered && !typing && !worded
}

/// `showing` is how much of the field the picture has, nought to one — see
/// [`picture_has_the_field`], which decides it, and the crossfade in
/// [`search_field`], which is the other half of it.
pub(super) fn spectrum_strip(app: &mut GuiApp, ui: &Ui, field: Rect, palette: &Palette, showing: f32) {
    if !app.config.ui.field_spectrum || app.spectrum.silent() || showing <= 0.004 {
        return;
    }

    let chosen = super::FieldPicture::from_name(&app.config.ui.field_picture);
    if chosen == super::FieldPicture::Meter {
        super::meter::draw(app, ui, field, palette, showing);
        return;
    }
    let whole = chosen != super::FieldPicture::Scope;
    let inset = MARKER_RADIUS as f32;
    let room = if whole {
        field
    } else {
        Rect::from_min_max(
            egui::pos2(field.left() + inset, field.top() + 1.0),
            egui::pos2(field.right() - inset, field.bottom() - 1.0),
        )
    };
    let ppp = ui.ctx().pixels_per_point();
    let width = (room.width() * ppp).round().max(0.0) as usize;
    let height = (room.height() * ppp).round().max(0.0) as usize;
    if width < 8 || height < 8 {
        return;
    }

    let now = ui.input(|i| i.time);

    let ink = palette
        .field
        .lerp_to_gamma(palette.ink, 0.72)
        .gamma_multiply(showing);

    let grey = if chosen == super::FieldPicture::Scope {
        let exposed = is_exposed(app.config.ui.scope_rate);
        look_in(&mut app.scope, &app.heard, (width, height, SCOPE_FIGURES), now, exposed);
        app.scope.grey()
    } else {
        let negative = palette.screens_negative();
        match super::field_art::grey_for(app, ui, chosen, (width, height), negative) {
            Some(grey) => grey,
            None => return,
        }
    };
    let method = crate::gui::dither::Screen::from_name(&app.config.ui.dither_method);
    let dots = crate::gui::dither::screen(&grey, width as u32, height as u32, method);
    let image = egui::ColorImage {
        size: [width, height],
        pixels: dots
            .into_iter()
            .map(|ink| if ink { Color32::WHITE } else { Color32::TRANSPARENT })
            .collect(),
        source_size: Vec2::new(width as f32, height as f32),
    };
    let picture = match &mut app.scope_picture {
        Some(handle) if handle.size() == [width, height] => {
            handle.set(image, egui::TextureOptions::NEAREST);
            handle
        }
        slot => slot.insert(ui.ctx().load_texture("scope", image, egui::TextureOptions::NEAREST)),
    };

    let all = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    if whole {
        ui.painter()
            .with_clip_rect(field)
            .add(egui::epaint::RectShape::filled(room, MARKER_RADIUS, ink).with_texture(picture.id(), all));
    } else {
        ui.painter().with_clip_rect(field).image(picture.id(), room, all, ink);
    }

    if chosen == super::FieldPicture::Scope {
        ui.ctx().request_repaint_after(picture_step(ui.ctx(), app.config.ui.scope_rate));
    } else if super::field_art::asks_for_frames(app) {
        ui.ctx().request_repaint_after(to_tick(ui.ctx(), super::field_art::field_tick(app)));
    }
}

/// The scope at the foot of the menu, live: one figure with every register
/// inside it, screened like the field's and drawn in the accent — the menu's
/// one picture, and small, so the one place the accent may move.
///
/// Returns false, and draws nothing, while nothing is sounding; the menu then
/// draws the still figure the icon is made of in its place.
pub(in crate::gui) fn live_mark(app: &mut GuiApp, ui: &Ui, room: Rect, palette: &Palette) -> bool {
    if app.spectrum.silent() {
        return false;
    }
    if !app.config.ui.field_spectrum {
        return false;
    }
    let chosen = super::FieldPicture::from_name(&app.config.ui.field_picture);
    let plate = Rect::from_center_size(room.center(), Vec2::splat(room.width().min(room.height())));
    ui.painter().rect_filled(plate, MARKER_RADIUS, palette.field);
    let room = plate.shrink(MARKER_RADIUS as f32);
    let ppp = ui.ctx().pixels_per_point();
    let side = (room.width().min(room.height()) * ppp).round().max(0.0) as usize;
    if side < 16 {
        return false;
    }
    let now = ui.input(|i| i.time);
    let grey = if chosen == super::FieldPicture::Scope {
        let exposed = is_exposed(app.config.ui.scope_rate);
        look_in(&mut app.mark_scope, &app.heard, (side, side, 1), now, exposed);
        app.mark_scope.grey()
    } else {
        let rows = app.field_art.cell().map_or(super::field_art::GRID_ROWS, |cell| (side / cell).max(4));
        if app.mark_art.rows() != rows {
            app.mark_art = super::FieldArt::with_rows(rows).wall();
        }
        std::mem::swap(&mut app.field_art, &mut app.mark_art);
        std::mem::swap(&mut app.scope, &mut app.mark_scope);
        let negative = palette.screens_negative();
        let grey = super::field_art::grey_for(app, ui, chosen, (side, side), negative);
        std::mem::swap(&mut app.field_art, &mut app.mark_art);
        std::mem::swap(&mut app.scope, &mut app.mark_scope);
        match grey {
            Some(grey) => grey,
            None => return false,
        }
    };
    let method = crate::gui::dither::Screen::from_name(&app.config.ui.dither_method);
    let dots = crate::gui::dither::screen(&grey, side as u32, side as u32, method);
    let image = egui::ColorImage {
        size: [side, side],
        pixels: dots
            .into_iter()
            .map(|ink| if ink { Color32::WHITE } else { Color32::TRANSPARENT })
            .collect(),
        source_size: Vec2::splat(side as f32),
    };
    let picture = match &mut app.mark_picture {
        Some(handle) if handle.size() == [side, side] => {
            handle.set(image, egui::TextureOptions::NEAREST);
            handle
        }
        slot => slot.insert(ui.ctx().load_texture("mark", image, egui::TextureOptions::NEAREST)),
    };
    let square = Rect::from_center_size(room.center(), Vec2::splat(side as f32 / ppp));
    ui.painter().image(
        picture.id(),
        square,
        Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        palette.accent,
    );
    if chosen == super::FieldPicture::Scope {
        ui.ctx().request_repaint_after(picture_step(ui.ctx(), app.config.ui.scope_rate));
    } else if super::field_art::asks_for_frames(app) {
        ui.ctx().request_repaint_after(to_tick(ui.ctx(), super::field_art::field_tick(app)));
    }
    true
}

/// The picture's size on the Touch Bar, in its pixels: two to a point, as
/// tall as the bar, and as wide as its view, fixed beside what plays and
/// beside the cover — see `src/media/touchbar.m`.
const TOUCH_PICTURE: (usize, usize) = (400, 60);
const TOUCH_WHOLE: (usize, usize) = (1200, 60);

/// What the dots are drawn in on the Touch Bar: a light grey on its black,
/// quieter than the system's white labels beside it.
const TOUCH_INK: u8 = 200;

/// The Touch Bar, kept in step once a frame: set up again when it is turned
/// on or off, when the picture is to fill it or not, when a station starts or
/// stops — no key back for a station — and when names change case; then its
/// picture, as [`touch_bar_view`] says. Only while the window is in front,
/// which is the only time the bar is the window's; taken away while nothing
/// sounds.
pub(in crate::gui) fn touch_bar_picture(app: &mut GuiApp, ui: &Ui) {
    if !crate::media::touchbar_available() {
        return;
    }
    let chosen = touch_bar_view(app);
    let on = app.config.ui.touchbar;
    let sounding = !app.spectrum.silent() && ui.input(|i| i.focused);
    let whole = app.config.ui.touchbar_whole && chosen.is_some() && sounding;
    let station = app.player.current_track().is_none() && app.on_air.is_some();
    let mode = (on, !app.config.ui.capitals, whole, station);
    if app.touch_mode != Some(mode) {
        crate::media::touchbar_show(on, !app.config.ui.capitals, whole, station);
        if app.touch_mode.is_none_or(|was| was.2 != whole) {
            app.touch_sent = None;
        }
        app.touch_mode = Some(mode);
    }
    let wanted = on && sounding;
    let Some(chosen) = chosen.filter(|_| wanted) else {
        let why = if !on {
            "aus"
        } else if chosen.is_none() {
            "kein bild gewählt"
        } else if app.spectrum.silent() {
            "kein bild: es klingt nichts"
        } else {
            "kein bild: das fenster ist nicht vorn"
        };
        say_touch(app, ui, why.to_string(), None);
        if app.touch_sent.take().is_some() {
            crate::media::touchbar_picture(None);
        }
        return;
    };
    let (width, height) = if whole { TOUCH_WHOLE } else { TOUCH_PICTURE };
    let laid = crate::media::touchbar_picture_wide();
    let state = if app.measure.is_some() {
        format!(
            "{} · {}×{} px · {} · die leiste gibt {}",
            chosen.name(),
            width,
            height,
            if whole { "ganze leiste" } else { "neben dem titel" },
            laid.map_or_else(|| "noch keine breite".to_string(), |points| format!("{points} pt")),
        )
    } else {
        String::new()
    };
    let now = ui.input(|i| i.time);
    let dots = if chosen == super::FieldPicture::Meter {
        let station = app.player.current_track().is_none() && app.on_air.is_some();
        let small = app.touch_meter.paint(&app.heard.recent_pairs(), station, width / 2, height / 2);
        (0..width * height).map(|at| small[(at / width / 2) * (width / 2) + (at % width) / 2]).collect()
    } else {
        let grey = if chosen == super::FieldPicture::Scope {
            let exposed = is_exposed(app.config.ui.scope_rate);
            look_in(&mut app.touch_scope, &app.heard, (width, height, SCOPE_FIGURES), now, exposed);
            app.touch_scope.grey()
        } else {
            let rows = app.field_art.cell().map_or(super::field_art::GRID_ROWS, |cell| (height / cell).max(4));
            if app.touch_art.rows() != rows {
                app.touch_art = super::FieldArt::with_rows(rows);
            }
            std::mem::swap(&mut app.field_art, &mut app.touch_art);
            std::mem::swap(&mut app.scope, &mut app.touch_scope);
            let grey = super::field_art::grey_for(app, ui, chosen, (width, height), false);
            std::mem::swap(&mut app.field_art, &mut app.touch_art);
            std::mem::swap(&mut app.scope, &mut app.touch_scope);
            match grey {
                Some(grey) => grey,
                None => {
                    say_touch(app, ui, format!("{state} · das bild gibt nichts her"), None);
                    if app.touch_sent.take().is_some() {
                        crate::media::touchbar_picture(None);
                    }
                    return;
                }
            }
        };
        let method = crate::gui::dither::Screen::from_name(&app.config.ui.dither_method);
        crate::gui::dither::screen(&grey, width as u32, height as u32, method)
    };
    let print = {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        dots.hash(&mut hasher);
        hasher.finish()
    };
    if app.measure.is_some() {
        let lit = dots.iter().filter(|&&lit| lit).count();
        say_touch(app, ui, state, Some((lit, dots.len(), app.touch_sent != Some(print))));
    }
    if app.touch_sent != Some(print) {
        app.touch_sent = Some(print);
        let rgba: Vec<u8> = dots
            .iter()
            .flat_map(|&lit| if lit { [TOUCH_INK, TOUCH_INK, TOUCH_INK, 255] } else { [0; 4] })
            .collect();
        crate::media::touchbar_picture(Some((&rgba, width, height)));
    }
    match chosen {
        super::FieldPicture::Scope => ui.ctx().request_repaint_after(picture_step(ui.ctx(), app.config.ui.scope_rate)),
        super::FieldPicture::Meter => {
            let period = 1.0 / f64::from(super::meter_rate(app.config.ui.meter_rate));
            ui.ctx().request_repaint_after(to_tick(ui.ctx(), period));
        }
        _ if super::field_art::asks_for_frames(app) => {
            ui.ctx().request_repaint_after(to_tick(ui.ctx(), super::field_art::field_tick(app)));
        }
        _ => {}
    }
}

/// How often, while the Touch Bar shows a picture, the measurement says how
/// many were sent and how much of the last was lit, in seconds.
const TOUCH_SAID_EVERY: f64 = 5.0;

/// What the Touch Bar does, into the measurement, for whoever looks into a
/// picture that does not show there: a line when what it shows changes —
/// which picture, how big, how wide the bar laid it out — and while it
/// shows one, every few seconds how many were sent and how many of the
/// last one's dots were lit. A picture all dark on the bar with dots lit
/// here is the bar's side; none lit is the picture's. `shown` is the lit
/// dots, all of them, and whether this one is sent; none while nothing is.
fn say_touch(app: &mut GuiApp, ui: &Ui, state: String, shown: Option<(usize, usize, bool)>) {
    if app.measure.is_none() {
        return;
    }
    let now = ui.input(|i| i.time);
    let (said, since, sent) = &mut app.touch_said;
    let mut lines = Vec::new();
    if *said != state {
        lines.push(format!("touch bar: {state}"));
        *said = state;
        *since = now;
        *sent = 0;
    }
    if let Some((lit, all, new)) = shown {
        *sent += u32::from(new);
        if now - *since >= TOUCH_SAID_EVERY {
            lines.push(format!("touch bar: {} bilder in {:.0} s gesendet · {lit} von {all} punkten an", sent, now - *since));
            *since = now;
            *sent = 0;
        }
    }
    if let Some(measure) = app.measure.as_mut() {
        for line in lines {
            measure.note(&line);
        }
    }
}

/// Which picture the Touch Bar shows: the search field's, whatever it is
/// (none while the field shows none), one of them by name, or none.
fn touch_bar_view(app: &GuiApp) -> Option<super::FieldPicture> {
    match app.config.ui.touchbar_view.as_str() {
        "aus" => None,
        "suchfeld" | "" => app.config.ui.field_spectrum.then(|| super::FieldPicture::from_name(&app.config.ui.field_picture)),
        name => Some(super::FieldPicture::from_name(name)),
    }
}

/// How many figures the scope in the search field draws: one, wide, with every
/// register inside it. There were up to seven side by side, and a setting for
/// how many; one is what it is now.
pub(super) const SCOPE_FIGURES: usize = 1;
