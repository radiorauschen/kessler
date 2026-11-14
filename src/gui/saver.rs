use eframe::egui::{self, Color32, Rect, Sense, Vec2};

use super::scope::{Scope, Tuning};
use super::theme::{Palette, Role};
use super::{text, GuiApp};

const RISE: f64 = 2.5;

const GRACE: f64 = 1.5;

pub const SIZES: [f32; 4] = [0.5, 0.7, 0.86, 0.0];

fn figure_side(window: Rect, size: u8) -> f32 {
    match SIZES.get(usize::from(size)).copied().unwrap_or(SIZES[2]) {
        share if share > 0.0 => window.width().min(window.height()) * share,
        _ => window.width().max(window.height()),
    }
}

pub const AFTER: [u32; 4] = [0, 1, 5, 20];

pub const DOTS: [u8; 3] = [1, 2, 3];

pub const GLOWS: [f32; 3] = [0.55, 0.7, 0.82];

pub const STRENGTHS: [f32; 3] = [0.8, 1.4, 2.3];

const BRIGHT_SHARE: f32 = 0.9;

const STEP: u64 = 16;

const EXPOSURE_EASE: f32 = 0.08;

const EXPOSURE_LEAST: f32 = 0.5;
const EXPOSURE_MOST: f32 = 400.0;

#[derive(Default)]
pub struct Saver {
    pub on: bool,
    touched_at: f64,
    since: f64,
    grace_until: f64,
    scope: Option<Scope>,
    picture: Option<egui::TextureHandle>,
    exposure: f32,
    by_hand: bool,
    art: Option<super::library::FieldArt>,
    art_scope: Scope,
}

impl Saver {
    pub fn show_now(&mut self, now: f64) {
        self.on = true;
        self.by_hand = true;
        self.since = now;
        self.grace_until = now + GRACE;
    }
}

fn tuning(glow: f32) -> Tuning {
    Tuning {
        waves: 5.0,
        persist: glow,
        exposure: 0.9,
    }
}

pub fn watch(app: &mut GuiApp, ctx: &egui::Context) {
    let now = ctx.input(|i| i.time);
    let (touched, moved) = ctx.input(|i| {
        let touched = i.events.iter().any(|e| {
            matches!(
                e,
                egui::Event::Key { pressed: true, .. }
                    | egui::Event::Text(_)
                    | egui::Event::PointerButton { .. }
                    | egui::Event::MouseWheel { .. }
                    | egui::Event::Zoom(_)
                    | egui::Event::Touch { .. }
                    | egui::Event::Paste(_)
            )
        });
        let moved = i.pointer.delta() != Vec2::ZERO;
        (touched, moved)
    });
    let transferring = matches!(
        app.transfer.as_ref().map(|t| &t.stage),
        Some(super::Stage::Going { .. })
    );
    let present = app.filter_typing || transferring || app.dragging.is_some();
    let playing = app.player.state() == crate::player::PlaybackState::Playing;
    let lively = playing || app.config.ui.saver_silent;

    if app.saver.on {
        let stopped = !lively && !app.saver.by_hand;
        let woken = touched || (moved && now >= app.saver.grace_until) || present || stopped;
        if woken {
            app.saver.on = false;
            app.saver.by_hand = false;
            app.saver.touched_at = now;
            ctx.input_mut(|i| {
                i.events.retain(|e| {
                    !matches!(
                        e,
                        egui::Event::Key { .. } | egui::Event::Text(_) | egui::Event::Paste(_)
                    )
                })
            });
        }
        return;
    }

    if touched || moved || present || !lively || app.saver.touched_at == 0.0 {
        app.saver.touched_at = now;
    }
    let minutes = app.config.ui.saver_after;
    if minutes == 0 {
        return;
    }
    let wait = f64::from(minutes) * 60.0;
    let idle = now - app.saver.touched_at;
    if idle >= wait {
        app.saver.on = true;
        app.saver.since = now;
        app.saver.grace_until = now;
    } else {
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(wait - idle + 0.05));
    }
}

pub fn draw(app: &mut GuiApp, ctx: &egui::Context, palette: &Palette) {
    if !app.saver.on {
        app.saver.picture = None;
        app.saver.scope = None;
        app.saver.art = None;
        app.saver.exposure = 0.0;
        return;
    }
    let now = ctx.input(|i| i.time);
    let window = ctx.content_rect();
    let rise = (((now - app.saver.since) / RISE).clamp(0.0, 1.0)) as f32;
    let rise = rise * rise * (3.0 - 2.0 * rise);

    let size = app.config.ui.saver_size;
    let mut wake: Option<std::time::Duration> = None;
    egui::Area::new(egui::Id::unique("saver"))
        .order(egui::Order::Tooltip)
        .fixed_pos(window.min)
        .interactable(true)
        .show(ctx, |ui| {
            let _ = ui.allocate_rect(window, Sense::click_and_drag());
            ui.ctx().set_cursor_icon(egui::CursorIcon::None);
            ui.painter()
                .rect_filled(window, 0.0, palette.ground.gamma_multiply(rise.max(0.02)));

            let side = figure_side(window, size);
            let lift = if SIZES.get(usize::from(size)) == Some(&0.0) { 0.0 } else { window.height() * 0.03 };
            let square = Rect::from_center_size(window.center() - Vec2::new(0.0, lift), Vec2::splat(side));
            let colouring = Colouring::of(app.config.ui.saver_colours, palette, now - app.saver.since);
            let ink = Color32::WHITE.gamma_multiply(rise);

            let hearing = app.player.state() == crate::player::PlaybackState::Playing && !app.spectrum.silent();
            let playing = app.player.state() == crate::player::PlaybackState::Playing;
            let picture = super::library::FieldPicture::from_name(&app.config.ui.saver_picture);
            if app.config.ui.saver_picture == WORDS {
                if playing {
                    wake = lyrics(app, ui, window, palette, rise, size);
                }
            } else if picture != super::library::FieldPicture::Scope {
                wake = art(app, ui, window, palette, rise, picture);
            }
            if wake.is_some() {
            } else if hearing {
                let frames = app.heard.recent_pairs();
                let rate = app.heard.rate();
                figure(app, ui, square, ink, colouring, &frames, rate);
            } else if app.config.ui.saver_silent {
                let now = ui.input(|i| i.time);
                let frames = vibrations(now - app.saver.since);
                figure(app, ui, square, ink, colouring, &frames, VIBRATION_RATE);
            } else {
                super::menu::screened_mark(app, ui, square.shrink(side * 0.12), palette.mid.gamma_multiply(rise * 0.7));
            }
            playing_line(app, ui, window, palette, rise);
        });

    if let Some(wake) = wake {
        let rising = now - app.saver.since < RISE;
        ctx.request_repaint_after(if rising { std::time::Duration::from_millis(RISE_STEP) } else { wake });
        return;
    }
    let step = if ctx.input(|i| i.focused) {
        std::time::Duration::from_millis(STEP)
    } else {
        super::library::picture_step(ctx, app.config.ui.scope_rate)
    };
    ctx.request_repaint_after(step);
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Colouring {
    Plain(Color32),
    Across { from: Color32, to: Color32, angle: f32 },
}

const WANDER_SECS: f64 = 12.0;

const TURN_SECS: f64 = 90.0;

impl Colouring {
    fn of(setting: u8, palette: &Palette, t: f64) -> Self {
        let colours = scheme_colours(palette);
        match setting {
            1 => Self::Plain(wandering(&colours, t)),
            2 => Self::Across {
                from: wandering(&colours, t),
                to: wandering(&colours, t + WANDER_SECS * colours.len() as f64 / 2.0),
                angle: (std::f64::consts::TAU * t / TURN_SECS) as f32,
            },
            _ => Self::Plain(palette.accent),
        }
    }

    fn at(self, x: usize, y: usize, side: usize) -> Color32 {
        match self {
            Self::Plain(colour) => colour,
            Self::Across { from, to, angle } => {
                let half = side as f32 / 2.0;
                let (dx, dy) = ((x as f32 - half) / half, (y as f32 - half) / half);
                let along = dx * angle.cos() + dy * angle.sin();
                let t = ((along + 1.0) / 2.0).clamp(0.0, 1.0);
                from.lerp_to_gamma(to, t * t * (3.0 - 2.0 * t))
            }
        }
    }
}

fn scheme_colours(palette: &Palette) -> Vec<Color32> {
    let mut colours: Vec<Color32> = Vec::new();
    let candidates = [Some(palette.accent), Some(palette.ink), Some(palette.accent_faint), Some(palette.mid), palette.lit_dots];
    for colour in candidates.into_iter().flatten() {
        let near = |a: Color32, b: Color32| {
            let d = |x: u8, y: u8| (i32::from(x) - i32::from(y)).abs();
            d(a.r(), b.r()) + d(a.g(), b.g()) + d(a.b(), b.b()) < 24
        };
        if !near(colour, palette.ground) && !colours.iter().any(|c| near(*c, colour)) {
            colours.push(colour);
        }
    }
    if colours.is_empty() {
        colours.push(palette.accent);
    }
    colours
}

fn wandering(colours: &[Color32], t: f64) -> Color32 {
    let n = colours.len();
    let step = (t.max(0.0) / WANDER_SECS) as usize;
    let within = ((t.max(0.0) / WANDER_SECS).fract()) as f32;
    let from = colours[step % n];
    let to = colours[(step + 1) % n];
    let f = ((within - 0.5) * 2.0).clamp(0.0, 1.0);
    from.lerp_to_gamma(to, f * f * (3.0 - 2.0 * f))
}

fn figure(
    app: &mut GuiApp,
    ui: &egui::Ui,
    square: Rect,
    ink: Color32,
    colouring: Colouring,
    frames: &[[f32; 2]],
    rate: u32,
) {
    let ppp = ui.ctx().pixels_per_point();
    let dots = f32::from(app.config.ui.saver_dots.clamp(1, 3));
    let px = ((square.width() * ppp) / dots).round().max(16.0) as usize;
    let side = px as f32 * dots / ppp;
    let corner = square.center() - Vec2::splat(side / 2.0);
    let square = Rect::from_min_size(
        egui::pos2((corner.x * ppp).round() / ppp, (corner.y * ppp).round() / ppp),
        Vec2::splat(side),
    );
    let glow = GLOWS[usize::from(app.config.ui.saver_glow).min(GLOWS.len() - 1)];
    let scope = app.saver.scope.get_or_insert_with(|| Scope::tuned(tuning(glow)));
    scope.retune(tuning(glow));
    let now = ui.input(|i| i.time);
    scope.look(frames, rate, px, px, 1, now);
    let strength = STRENGTHS[usize::from(app.config.ui.saver_strength).min(STRENGTHS.len() - 1)];
    if let Some(wanted) = wanted_exposure(scope, strength) {
        let was = if app.saver.exposure > 0.0 { app.saver.exposure } else { wanted };
        let gone = ui.input(|i| i.stable_dt).clamp(0.0, 0.1);
        let ease = 1.0 - (1.0 - EXPOSURE_EASE).powf(gone * 30.0);
        app.saver.exposure = was + (wanted - was) * ease;
    }
    let exposure = if app.saver.exposure > 0.0 { app.saver.exposure } else { tuning(glow).exposure };
    let grey = scope.grey_at(exposure);
    let method = super::dither::Screen::from_name(&app.config.ui.saver_dither_method);
    let screened = super::dither::screen(&grey, px as u32, px as u32, method);
    let image = egui::ColorImage {
        size: [px, px],
        pixels: screened
            .into_iter()
            .enumerate()
            .map(|(i, lit)| if lit { colouring.at(i % px, i / px, px) } else { Color32::TRANSPARENT })
            .collect(),
        source_size: Vec2::splat(px as f32),
    };
    let picture = match &mut app.saver.picture {
        Some(handle) if handle.size() == [px, px] => {
            handle.set(image, egui::TextureOptions::NEAREST);
            handle
        }
        slot => slot.insert(ui.ctx().load_texture("saver", image, egui::TextureOptions::NEAREST)),
    };
    ui.painter().image(
        picture.id(),
        square,
        Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        ink,
    );
}

const VIBRATION_RATE: u32 = 44_100;

const VIBRATION_FRAMES: usize = 2048;

struct Voice {
    hz: f64,
    wander: f64,
    wander_secs: f64,
    turn_secs: f64,
    breath_secs: f64,
    breath_at: f64,
}

const VOICES: [Voice; 3] = [
    Voice { hz: 58.0, wander: 0.004, wander_secs: 29.0, turn_secs: 17.0, breath_secs: 41.0, breath_at: 0.2 },
    Voice { hz: 262.0, wander: 0.004, wander_secs: 37.0, turn_secs: 11.0, breath_secs: 29.0, breath_at: 0.55 },
    Voice { hz: 1245.0, wander: 0.004, wander_secs: 43.0, turn_secs: 23.0, breath_secs: 37.0, breath_at: 0.9 },
];

fn vibrations(t: f64) -> Vec<[f32; 2]> {
    use std::f64::consts::TAU;
    let start = t - VIBRATION_FRAMES as f64 / f64::from(VIBRATION_RATE);
    (0..VIBRATION_FRAMES)
        .map(|i| {
            let s = start + i as f64 / f64::from(VIBRATION_RATE);
            let mut frame = [0.0f32; 2];
            for v in &VOICES {
                let cycles = v.hz * (s - v.wander * (TAU * s / v.wander_secs).cos() * v.wander_secs / TAU);
                let phase = TAU * cycles;
                let turn = TAU * s / v.turn_secs;
                let breath = 0.5 + 0.5 * (TAU * (s / v.breath_secs + v.breath_at)).sin();
                let level = (0.03 + 0.97 * breath.powi(4)) as f32;
                let left = phase.sin() as f32;
                let right = (phase + turn).sin() as f32;
                frame[0] += level * left;
                frame[1] += level * right;
            }
            frame
        })
        .collect()
}

fn wanted_exposure(scope: &Scope, strength: f32) -> Option<f32> {
    let bright = scope.bright(BRIGHT_SHARE)?;
    Some((strength / bright.max(1e-4)).clamp(EXPOSURE_LEAST, EXPOSURE_MOST))
}

pub const PICTURES: [&str; 8] = ["vektorskop", "schlange", "mandelbrot", "leben", "kugel", "zarathustra", "kessler saga", WORDS];

pub fn said(name: &str) -> &'static str {
    if name == WORDS {
        tr!("liedtext")
    } else {
        super::library::FieldPicture::from_name(name).said()
    }
}

const ART_ROWS: usize = 48;

const ART_TEXELS: usize = 4;

fn art(
    app: &mut GuiApp,
    ui: &egui::Ui,
    window: Rect,
    palette: &Palette,
    rise: f32,
    picture: super::library::FieldPicture,
) -> Option<std::time::Duration> {
    let cells = ((ART_ROWS as f32 * window.width() / window.height().max(1.0)).round() as usize).max(4);
    let (columns, rows) = (cells * ART_TEXELS, ART_ROWS * ART_TEXELS);
    let mut own = app.saver.art.take().unwrap_or_else(|| super::library::FieldArt::with_rows(ART_ROWS));
    std::mem::swap(&mut app.field_art, &mut own);
    std::mem::swap(&mut app.scope, &mut app.saver.art_scope);
    let grey = super::library::field_grey(app, ui, picture, (columns, rows), palette.screens_negative());
    std::mem::swap(&mut app.scope, &mut app.saver.art_scope);
    std::mem::swap(&mut app.field_art, &mut own);
    app.saver.art = Some(own);
    let grey = grey?;
    let image = egui::ColorImage {
        size: [columns, rows],
        pixels: grey
            .into_iter()
            .map(|g| if g > 127 { Color32::WHITE } else { Color32::TRANSPARENT })
            .collect(),
        source_size: Vec2::new(columns as f32, rows as f32),
    };
    let texture = match &mut app.saver.picture {
        Some(handle) if handle.size() == [columns, rows] => {
            handle.set(image, egui::TextureOptions::NEAREST);
            handle
        }
        slot => slot.insert(ui.ctx().load_texture("saver", image, egui::TextureOptions::NEAREST)),
    };
    let texel = window.height() / rows as f32;
    let area = Rect::from_center_size(window.center(), Vec2::new(columns as f32 * texel, window.height()));
    ui.painter().image(
        texture.id(),
        area,
        Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        palette.accent.gamma_multiply(rise),
    );
    let _ = picture;
    Some(super::library::to_tick(ui.ctx(), super::library::field_tick(app)))
}
pub const WORDS: &str = "liedtext";

const WORD_SIZES: [f32; 4] = [0.05, 0.07, 0.1, 0.14];

const RISE_STEP: u64 = 33;

const LINES_ABOUT: usize = 4;

fn lyrics(app: &mut GuiApp, ui: &egui::Ui, window: Rect, palette: &Palette, rise: f32, size: u8) -> Option<std::time::Duration> {
    app.follow_the_words();
    let size = window.height() * WORD_SIZES.get(usize::from(size)).copied().unwrap_or(WORD_SIZES[2]);
    let said = |ui: &egui::Ui, words: &str, again: f64| {
        let g = text::sized(ui, words, Role::Content, size, palette.faint.gamma_multiply(rise), window.width() * 0.84);
        ui.painter().galley(window.center() - g.size() / 2.0, g, palette.faint.gamma_multiply(rise));
        Some(std::time::Duration::from_secs_f64(again))
    };
    let Some(words) = app.lyrics.lyrics.clone() else {
        return if app.lyrics.looking.is_some() {
            said(ui, &format!("{} …", tr!("lade liedtext")), 1.0)
        } else {
            said(ui, tr!("kein liedtext gefunden"), 2.0)
        };
    };
    if words.instrumental || words.lines.iter().all(|l| l.text.is_empty()) {
        return said(ui, if words.instrumental { tr!("ohne worte") } else { tr!("kein liedtext gefunden") }, 2.0);
    }
    let room = window.width() * 0.84;
    let position = app.player.position().as_secs_f64();
    if words.timed() && app.on_air.is_none() {
        Some(timed(ui, &words, window, palette, rise, size, room, position))
    } else {
        let duration = app.player.duration().as_secs_f64();
        let along = if app.on_air.is_none() && duration > 0.0 { (position / duration).clamp(0.0, 1.0) } else { 0.0 };
        Some(paged(ui, &words, window, palette, rise, size, room, along, duration))
    }
}

#[allow(clippy::too_many_arguments)]
fn timed(
    ui: &egui::Ui,
    words: &crate::library::lyrics::Lyrics,
    window: Rect,
    palette: &Palette,
    rise: f32,
    size: f32,
    room: f32,
    position: f64,
) -> std::time::Duration {
    let lines = &words.lines;
    let sung = words.now(position);
    let centred = sung.unwrap_or(0);
    let focus = centred as i64;
    let first = (focus - LINES_ABOUT as i64).max(0) as usize;
    let last = ((focus + LINES_ABOUT as i64 + 1).max(0) as usize).min(lines.len());

    let mut laid = Vec::new();
    let mut y = 0.0f32;
    let mut sung_middle = None;
    for (at, entry) in lines.iter().enumerate().take(last).skip(first) {
        let far = ((at as i64 - focus).unsigned_abs() as f32 / (LINES_ABOUT as f32 + 0.5)).min(1.0);
        if entry.text.is_empty() {
            let gap = size * 0.6;
            if at == centred {
                sung_middle = Some(y + gap / 2.0);
            }
            y += gap;
            continue;
        }
        let colour = if Some(at) == sung { palette.accent } else { palette.mid.gamma_multiply(1.0 - far * 0.8) };
        let colour = colour.gamma_multiply(rise);
        let g = match entry.sung_to(position).filter(|_| Some(at) == sung) {
            Some(to) => text::sized_parts(
                ui,
                &[(&entry.text[..to], colour), (&entry.text[to..], palette.ink.gamma_multiply(rise))],
                Role::Content,
                size,
                room,
            ),
            None => text::sized(ui, &entry.text, Role::Content, size, colour, room),
        };
        let height = g.size().y;
        if at == centred {
            sung_middle = Some(y + height / 2.0);
        }
        laid.push((y, g));
        y += height + size * 0.35;
    }
    let middle = sung_middle.unwrap_or(0.0);
    let shift = window.center().y - middle;
    let room_for = words_room(window);
    let painter = ui.painter();
    for (top, g) in laid {
        let at = egui::pos2(window.center().x - g.rect.center().x, top + shift);
        if at.y >= room_for.top() && at.y + g.size().y <= room_for.bottom() {
            painter.galley(at, g, palette.mid);
        }
    }

    let next_word = sung.and_then(|i| lines[i].words.iter().map(|(at, _)| *at).find(|&at| at > position));
    let next_line = lines.iter().filter_map(|l| l.at).find(|&at| at > position);
    let next = match (next_word, next_line) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    };
    match next {
        Some(at) => std::time::Duration::from_secs_f64((at - position).max(0.02) + 0.01),
        None => std::time::Duration::from_secs(5),
    }
}

fn words_room(window: Rect) -> Rect {
    Rect::from_min_max(
        egui::pos2(window.left(), window.top() + FOOT_LINE),
        egui::pos2(window.right(), window.bottom() - FOOT_LINE - 40.0),
    )
}

#[allow(clippy::too_many_arguments)]
fn paged(
    ui: &egui::Ui,
    words: &crate::library::lyrics::Lyrics,
    window: Rect,
    palette: &Palette,
    rise: f32,
    size: f32,
    room: f32,
    along: f64,
    duration: f64,
) -> std::time::Duration {
    let tall = words_room(window).height() * 0.9;
    let colour = palette.ink.gamma_multiply(rise);
    let mut pages: Vec<Vec<std::sync::Arc<egui::Galley>>> = vec![Vec::new()];
    let mut height = 0.0f32;
    let gap = size * 0.3;
    for line in &words.lines {
        let page = pages.last_mut().expect("there is always a page");
        if line.text.is_empty() {
            if height > tall * 0.6 && !page.is_empty() {
                pages.push(Vec::new());
                height = 0.0;
            } else if !page.is_empty() {
                height += size * 0.6;
                page.push(text::sized(ui, " ", Role::Content, size * 0.6, colour, room));
            }
            continue;
        }
        let g = text::sized(ui, &line.text, Role::Content, size, colour, room);
        if height + g.size().y > tall && !page.is_empty() {
            pages.push(Vec::new());
            height = 0.0;
        }
        height += g.size().y + gap;
        pages.last_mut().expect("there is always a page").push(g);
    }
    pages.retain(|p| !p.is_empty());
    if pages.is_empty() {
        return std::time::Duration::from_secs(5);
    }
    let count = pages.len();
    let page = ((along * count as f64) as usize).min(count - 1);
    let lines = &pages[page];
    let total: f32 = lines.iter().map(|g| g.size().y + gap).sum::<f32>() - gap;
    let mut y = window.center().y - total / 2.0;
    let painter = ui.painter();
    for g in lines {
        let at = egui::pos2(window.center().x - g.rect.center().x, y);
        y += g.size().y + gap;
        painter.galley(at, g.clone(), colour);
    }
    if duration > 0.0 && page + 1 < count {
        let to_go = ((page + 1) as f64 / count as f64 - along) * duration;
        return std::time::Duration::from_secs_f64(to_go.clamp(0.05, 30.0) + 0.01);
    }
    std::time::Duration::from_secs(5)
}

const FOOT_LINE: f32 = 48.0;

fn playing_line(app: &GuiApp, ui: &egui::Ui, window: Rect, palette: &Palette, rise: f32) {
    let Some([title, artist, _]) = app.playing_words() else {
        return;
    };
    let said = format!("{title}   {artist}");
    let colour = palette.faint.gamma_multiply(rise);
    let g = text::line_within(ui, &said, Role::Label, colour, window.width() * 0.8);
    let at = egui::pos2(
        window.center().x - g.size().x / 2.0,
        window.bottom() - FOOT_LINE - g.size().y,
    );
    ui.painter().galley(at, g, colour);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_colour_wanders_round_the_scheme() {
        let colours = [Color32::RED, Color32::GREEN, Color32::BLUE];
        assert_eq!(wandering(&colours, 0.0), Color32::RED);
        assert_eq!(wandering(&colours, WANDER_SECS * 0.4), Color32::RED);
        assert_eq!(wandering(&colours, WANDER_SECS), Color32::GREEN);
        assert_ne!(wandering(&colours, WANDER_SECS * 1.75), Color32::GREEN);
        assert_eq!(wandering(&colours, WANDER_SECS * 3.0), Color32::RED);
    }

    #[test]
    fn every_scheme_has_colours_to_wander_through() {
        for theme in super::super::theme::Theme::all() {
            let palette = theme.palette();
            let colours = scheme_colours(&palette);
            assert!(!colours.is_empty(), "{}", theme.name());
            assert!(!colours.contains(&palette.ground), "{}", theme.name());
        }
    }

    #[test]
    fn the_gradient_runs_across_the_figure() {
        let across = Colouring::Across { from: Color32::BLACK, to: Color32::WHITE, angle: 0.0 };
        assert_eq!(across.at(0, 50, 100), Color32::BLACK);
        assert_eq!(across.at(100, 50, 100), Color32::WHITE);
        assert_eq!(Colouring::Plain(Color32::RED).at(3, 4, 10), Color32::RED);
    }

    fn looked_at(level: f32, side: usize) -> Scope {
        let rate = 44_100u32;
        let mut scope = Scope::tuned(tuning(GLOWS[1]));
        let mut seed = 1u32;
        let mut noise = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5
        };
        let mut t = 0.0f32;
        for frame in 0..45 {
            let frames: Vec<[f32; 2]> = (0..2048)
                .map(|i| {
                    let s = t + i as f32 / rate as f32;
                    let drift = 0.3 * (s * 0.7).sin();
                    let bass = 0.5 * (std::f32::consts::TAU * 55.0 * s).sin();
                    let mut l = bass;
                    let mut r = bass * 0.9;
                    for (f, a) in [(220.0, 0.25), (277.2, 0.2), (329.6, 0.18), (880.0, 0.08)] {
                        l += a * (std::f32::consts::TAU * f * s).sin();
                        r += a * (std::f32::consts::TAU * f * s + drift * f / 220.0).sin();
                    }
                    let hiss = 0.05 * noise();
                    [(l + hiss) * level, (r - hiss) * level]
                })
                .collect();
            scope.look(&frames, rate, side, side, 1, f64::from(frame) / 30.0);
            t += 1.0 / 30.0;
        }
        scope
    }

    fn ink_share(scope: &Scope, exposure: f32, side: usize) -> f32 {
        let grey = scope.grey_at(exposure);
        let dots = super::super::dither::screen(
            &grey,
            side as u32,
            side as u32,
            super::super::dither::Screen::from_name("geordnet"),
        );
        dots.iter().filter(|d| **d).count() as f32 / dots.len() as f32
    }

    #[test]
    fn a_quiet_record_is_drawn_as_strongly_as_a_loud_one() {
        let side = 300;
        let loud = looked_at(1.0, side);
        let quiet = looked_at(0.05, side);
        let strength = STRENGTHS[1];
        let a = ink_share(&loud, wanted_exposure(&loud, strength).unwrap(), side);
        let b = ink_share(&quiet, wanted_exposure(&quiet, strength).unwrap(), side);
        assert!(a > 0.01 && b > 0.01, "{a} {b}");
        assert!((a / b).max(b / a) < 2.0, "loud {a}, quiet {b}");
        let gentle = ink_share(&quiet, wanted_exposure(&quiet, STRENGTHS[0]).unwrap(), side);
        let strong = ink_share(&quiet, wanted_exposure(&quiet, STRENGTHS[2]).unwrap(), side);
        assert!(gentle < b && b < strong, "{gentle} {b} {strong}");
    }

    #[test]
    fn the_vibrations_move_and_never_stop() {
        let a = vibrations(10.0);
        let b = vibrations(10.5);
        assert_eq!(a.len(), VIBRATION_FRAMES);
        assert_ne!(a, b);
        for t in [0.0, 7.0, 60.0, 600.0] {
            let v = vibrations(t);
            let loud = v.iter().map(|[l, r]| l.abs() + r.abs()).fold(0.0f32, f32::max);
            assert!(loud > 0.1, "{t}: {loud}");
            assert!(v.iter().all(|[l, r]| l.is_finite() && r.is_finite()));
        }
    }

    #[test]
    #[ignore]
    fn vibration_pictures() {
        let stem = std::env::var("KESSLER_VIBRATIONS").unwrap_or("/tmp/vibrations".into());
        let side = 700usize;
        for (n, at) in [3.0, 17.0, 45.0, 90.0, 140.0, 230.0].into_iter().enumerate() {
            let mut scope = Scope::tuned(tuning(GLOWS[1]));
            for frame in 0..60 {
                let t = at + f64::from(frame) / 30.0;
                scope.look(&vibrations(t), VIBRATION_RATE, side, side, 1, t);
            }
            let exposure = wanted_exposure(&scope, STRENGTHS[1]).unwrap();
            let dots = super::super::dither::screen(
                &scope.grey_at(exposure),
                side as u32,
                side as u32,
                super::super::dither::Screen::from_name("geordnet"),
            );
            let mut image = image::RgbImage::new(side as u32, side as u32);
            for (i, lit) in dots.iter().enumerate() {
                let c = if *lit { [181, 137, 0] } else { [0, 43, 54] };
                image.put_pixel((i % side) as u32, (i / side) as u32, image::Rgb(c));
            }
            image.save(format!("{stem}-{n}.png")).unwrap();
        }
    }

    #[test]
    #[ignore]
    fn saver_picture() {
        let path = std::env::var("KESSLER_SAVER_PNG").unwrap_or("/tmp/saver.png".into());
        let level: f32 = std::env::var("KESSLER_SAVER_LEVEL")
            .ok()
            .and_then(|l| l.parse().ok())
            .unwrap_or(1.0);
        let side = 900usize;
        let scope = looked_at(level, side);
        let exposure = wanted_exposure(&scope, STRENGTHS[1]).unwrap();
        let grey = scope.grey_at(exposure);
        let dots = super::super::dither::screen(
            &grey,
            side as u32,
            side as u32,
            super::super::dither::Screen::from_name("geordnet"),
        );
        let mut image = image::RgbImage::new(side as u32, side as u32);
        for (i, lit) in dots.iter().enumerate() {
            let c = if *lit { [96, 160, 255] } else { [30, 30, 30] };
            image.put_pixel((i % side) as u32, (i / side) as u32, image::Rgb(c));
        }
        image.save(path).unwrap();
    }
}
