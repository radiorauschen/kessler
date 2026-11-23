use super::*;

pub(super) const PICTURE_STEP: u64 = 50;

pub(in crate::gui) fn scope_step(rate: u32) -> u64 {
    match rate {
        SECOND_RATE | EXPOSED_RATE | 8 | 12 => 1000 / u64::from(rate),
        _ => PICTURE_STEP,
    }
}

pub(in crate::gui) const EXPOSED_RATE: u32 = 4;

pub(in crate::gui) const SECOND_RATE: u32 = 1;

pub(in crate::gui) fn is_exposed(rate: u32) -> bool {
    rate == EXPOSED_RATE || rate == SECOND_RATE
}

const EXPOSED_STEP: f64 = 0.05;

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

pub(super) const PICTURE_STEP_AWAY: u64 = 200;

pub(super) const PICTURE_STEP_HIDDEN: u64 = 1000;

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

pub(in crate::gui) fn on_grid(now: f64, period: f64, early: f64) -> std::time::Duration {
    let next = ((now / period).floor() + 1.0) * period;
    let mut wait = next - now;
    if wait < period * 0.25 {
        wait += period;
    }
    std::time::Duration::from_secs_f64(wait + early)
}

pub(in crate::gui) fn to_tick(ctx: &egui::Context, period: f64) -> std::time::Duration {
    let (now, early) = ctx.input(|i| (i.time, f64::from(i.predicted_dt)));
    on_grid(now, period, early)
}

pub(super) const FIELD_FADE: f32 = 0.16;

pub(super) const AWAY_FADE: f32 = 0.6;

pub(super) fn picture_has_the_field(drawing: bool, hovered: bool, typing: bool, worded: bool) -> bool {
    drawing && !hovered && !typing && !worded
}

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

const TOUCH_PICTURE: (usize, usize) = (400, 60);
const TOUCH_WHOLE: (usize, usize) = (1200, 60);

const TOUCH_INK: u8 = 200;

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

const TOUCH_SAID_EVERY: f64 = 5.0;

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

fn touch_bar_view(app: &GuiApp) -> Option<super::FieldPicture> {
    match app.config.ui.touchbar_view.as_str() {
        "aus" => None,
        "suchfeld" | "" => app.config.ui.field_spectrum.then(|| super::FieldPicture::from_name(&app.config.ui.field_picture)),
        name => Some(super::FieldPicture::from_name(name)),
    }
}

pub(super) const SCOPE_FIGURES: usize = 1;
