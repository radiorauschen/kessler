//! The screen saver: the vectorscope, as big as the window allows.
//!
//! The field's scope is seven small figures side by side, one for each
//! register; the menu's is the same music as one figure, every register nested
//! inside the others. This is that one figure again, filling the window,
//! screened into dots in the accent on the ground — and with the room a window
//! has, it can afford what the field cannot: more waves of every note, so a
//! steady one lays its rings on each other and a moving one fans them out, and
//! a longer afterglow, so the last seconds hang in the dark as ghosts.
//!
//! It comes after so long without a key, a click, a movement or a turn of
//! the wheel, and the touch that ends it is used up doing so: the click that
//! wakes the window does not also start a record, the key does not also
//! skip one. Somebody half way through typing, or with a transfer running,
//! is somebody who is there — it never comes over either.
//!
//! While nothing plays, the still figure the icon is made of stands in the
//! middle instead, and under it — as under the music — what is playing, or what
//! was. Unless it is to come without music too: then it makes up vibrations of
//! its own and draws those, and nothing is played — they are numbers handed to
//! the scope, never sound.

use eframe::egui::{self, Color32, Rect, Sense, Vec2};

use super::scope::{Scope, Tuning};
use super::theme::{Palette, Role};
use super::{text, GuiApp};

/// How long it takes to come up, in seconds. Slowly: a picture that snaps
/// on is an alarm, one that rises is a mood.
const RISE: f64 = 2.5;

/// How long after it is shown on purpose — „jetzt zeigen" in the menu — a
/// movement of the pointer does not count: the hand is still on its way
/// away from the menu.
const GRACE: f64 = 1.5;

/// How big the figure is, as a share of the window's shorter side — the
/// menu's „klein", „mittel", „gross" — and, last, „fenster": as big as the
/// window's longer side, so the figure runs off every edge and fills it.
pub const SIZES: [f32; 4] = [0.5, 0.7, 0.86, 0.0];

/// The side of the figure's square for size `size` in `window`.
fn figure_side(window: Rect, size: u8) -> f32 {
    match SIZES.get(usize::from(size)).copied().unwrap_or(SIZES[2]) {
        share if share > 0.0 => window.width().min(window.height()) * share,
        _ => window.width().max(window.height()),
    }
}

/// The waiting times the menu offers, in minutes, „aus" first.
pub const AFTER: [u32; 4] = [0, 1, 5, 20];

/// The sizes of dot it offers, in device pixels: one is the finest the
/// screen has, three reads as a print from across the room.
pub const DOTS: [u8; 3] = [1, 2, 3];

/// The afterglows it offers: how much of its light a stroke keeps for each
/// thirtieth of a second. The field keeps half; here even the shortest
/// lingers longer, because there is room for it to.
pub const GLOWS: [f32; 3] = [0.55, 0.7, 0.82];

/// How strong the brighter part of the figure is drawn, as the exposure
/// that part is brought to: `1 - e^-s` of full ink — about 55 %, 75 % and
/// 90 %. „zart", „mittel", „kräftig" in the menu.
pub const STRENGTHS: [f32; 3] = [0.8, 1.4, 2.3];

/// Which lit pixels count as the brighter part: the ones above nine in ten.
const BRIGHT_SHARE: f32 = 0.9;

/// How long a frame stands while the window is looked at, in milliseconds.
const STEP: u64 = 16;

/// How much of the way to the wanted exposure it goes each thirtieth of a
/// second — most of the way in a second.
const EXPOSURE_EASE: f32 = 0.08;

/// The exposure is kept between these, so a near-silent stretch is not
/// blown up into noise and a wall of sound is not crushed to nothing.
const EXPOSURE_LEAST: f32 = 0.5;
const EXPOSURE_MOST: f32 = 400.0;

/// What the saver remembers between frames.
#[derive(Default)]
pub struct Saver {
    /// Whether it is showing.
    pub on: bool,
    /// When somebody last touched anything, in egui's seconds.
    touched_at: f64,
    /// When it came up.
    since: f64,
    /// Until when a movement of the pointer does not end it.
    grace_until: f64,
    scope: Option<Scope>,
    picture: Option<egui::TextureHandle>,
    /// The exposure it is drawn at, following the music — see [`figure`].
    exposure: f32,
    /// Brought up by hand from the menu, which it may be with nothing
    /// playing; otherwise it comes only with the music, and goes with it.
    by_hand: bool,
    /// The search field's quiet pictures as the saver draws them, with a scope
    /// of their own — see [`art`].
    art: Option<super::library::FieldArt>,
    art_scope: Scope,
    /// The drawings as the saver last made them, each at the size it was
    /// drawn at — see [`drawings`].
    drawn: Vec<Option<([usize; 2], egui::TextureHandle)>>,
}

impl Saver {
    /// Show it now, for the menu's „jetzt zeigen".
    pub fn show_now(&mut self, now: f64) {
        self.on = true;
        self.by_hand = true;
        self.since = now;
        self.grace_until = now + GRACE;
    }
}

/// The tuning for a given afterglow.
fn tuning(glow: f32) -> Tuning {
    Tuning {
        waves: 5.0,
        persist: glow,
        exposure: 0.9,
    }
}

/// Once a frame, before the keys are read: whether anybody is there, and so
/// whether the saver comes up, stays or goes. The touch that ends it is taken
/// out of the frame's input so nothing else acts on it.
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

/// Draw it over everything, when it is on. Called last.
pub fn draw(app: &mut GuiApp, ctx: &egui::Context, palette: &Palette) {
    if !app.saver.on {
        app.saver.picture = None;
        app.saver.scope = None;
        app.saver.art = None;
        app.saver.drawn.clear();
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
            if app.config.ui.saver_picture == DRAWINGS {
                wake = Some(drawings(app, ui, window, palette, rise, now - app.saver.since));
            } else if app.config.ui.saver_picture == WORDS {
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
            if app.config.ui.saver_picture != DRAWINGS {
                playing_line(app, ui, window, palette, rise);
            }
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

/// What colour the dots are, at one moment.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Colouring {
    /// One colour for every dot.
    Plain(Color32),
    /// From `from` on one side of the figure to `to` on the other, along
    /// the direction `angle` points (radians).
    Across { from: Color32, to: Color32, angle: f32 },
}

/// How long the wandering colour takes from one of the scheme's colours to
/// the next, in seconds: slow enough to be noticed only on looking again.
const WANDER_SECS: f64 = 12.0;

/// How long the gradient takes to turn once round, in seconds.
const TURN_SECS: f64 = 90.0;

impl Colouring {
    /// For the setting `saver_colours`, `t` seconds after the saver came.
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

    /// The colour of the dot at `x`, `y` of a square `side` across.
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

/// The scheme's own colours that read on its ground: the accent first, then
/// the ink, the receded accent, the middle tone and a halftone's own colour
/// where it has one. Those too close to the ground — the faint tone, the
/// marker — would be dots nobody sees, and are left out; so is a colour the
/// list has already, which would only make the wander stand still.
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

/// The colour `t` seconds into the wander round `colours`: each held a moment
/// and eased into the next.
fn wandering(colours: &[Color32], t: f64) -> Color32 {
    let n = colours.len();
    let step = (t.max(0.0) / WANDER_SECS) as usize;
    let within = ((t.max(0.0) / WANDER_SECS).fract()) as f32;
    let from = colours[step % n];
    let to = colours[(step + 1) % n];
    let f = ((within - 0.5) * 2.0).clamp(0.0, 1.0);
    from.lerp_to_gamma(to, f * f * (3.0 - 2.0 * f))
}

/// The music — or the made-up vibrations — as one figure in `square`,
/// screened, coloured as `colouring` says and faded by `ink`.
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

/// How many made-up frames make a second, as a sound card's would.
const VIBRATION_RATE: u32 = 44_100;

/// How many of them the scope is handed each time: as many as the tap keeps
/// of the music.
const VIBRATION_FRAMES: usize = 2048;

/// One made-up voice: a pitch that wanders a little, a right channel a
/// turning phase away from the left, and a slow breath in and out.
struct Voice {
    hz: f64,
    /// How far the pitch wanders, as a share of it, and over how long.
    wander: f64,
    wander_secs: f64,
    /// How long the right channel takes to turn once round against the
    /// left: a line, an ellipse, a ring, an ellipse the other way.
    turn_secs: f64,
    /// How long it takes to come and go, and where in that it starts.
    breath_secs: f64,
    breath_at: f64,
}

/// Three voices, two registers apart so each rings in its own and hardly
/// in its neighbours' — closer, and every register filled with the others'
/// leftovers, which the scope draws as a tangle. Each breathes at its own
/// pace, and the paces share no factor worth the name, so the rings they
/// make together do not come round again in any afternoon.
const VOICES: [Voice; 3] = [
    Voice { hz: 58.0, wander: 0.004, wander_secs: 29.0, turn_secs: 17.0, breath_secs: 41.0, breath_at: 0.2 },
    Voice { hz: 262.0, wander: 0.004, wander_secs: 37.0, turn_secs: 11.0, breath_secs: 29.0, breath_at: 0.55 },
    Voice { hz: 1245.0, wander: 0.004, wander_secs: 43.0, turn_secs: 23.0, breath_secs: 37.0, breath_at: 0.9 },
];

/// The made-up vibrations, the last [`VIBRATION_FRAMES`] of them up to `t`
/// seconds in, as left and right. Worked out afresh from `t` each time —
/// every pitch has a phase that can be written down for any moment — so
/// there is nothing to keep between frames and nothing to drift.
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

/// The exposure that brings the brighter part of what is lit to `strength`.
fn wanted_exposure(scope: &Scope, strength: f32) -> Option<f32> {
    let bright = scope.bright(BRIGHT_SHARE)?;
    Some((strength / bright.max(1e-4)).clamp(EXPOSURE_LEAST, EXPOSURE_MOST))
}

/// What the saver shows, by the names `[ui] saver_picture` uses: the
/// vectorscope, the search field's quiet pictures, or the words of the song.
pub const PICTURES: [&str; 9] = ["vektorskop", "schlange", "mandelbrot", "leben", "kugel", "zarathustra", "kessler saga", WORDS, DRAWINGS];

/// What the menu says for the saver's picture `name`.
pub fn said(name: &str) -> &'static str {
    if name == DRAWINGS {
        tr!("release note drawings")
    } else if name == WORDS {
        tr!("liedtext")
    } else {
        super::library::FieldPicture::from_name(name).said()
    }
}

/// How many rows of cells the quiet pictures are, filling the window: three
/// times the search field's, so a cell is about a fiftieth of the height.
const ART_ROWS: usize = 48;

/// How many texels a cell is across: enough for the scope to be drawn finer
/// than the cells it is then laid into, and for the snake's gaps.
const ART_TEXELS: usize = 4;

/// One of the search field's quiet pictures, filling the window in coarse cells
/// in the accent: the same snake, life, zoom, ball or coarse scope, moving on
/// the same ticks — a quarter of a second, a second on the air — and drawn only
/// on them. Returns how long until the next tick.
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

/// How big the words are, as a share of the window's height, for each of
/// the menu's sizes. Every line the same size, and the same weight: the line
/// being sung is told only by its colour, so nothing grows, shrinks or moves
/// and the words draw only when a line begins.
const WORD_SIZES: [f32; 4] = [0.05, 0.07, 0.1, 0.14];

/// How often it draws while it comes up, in milliseconds — the only time
/// the words are drawn more than once a line.
const RISE_STEP: u64 = 33;

/// How many lines stand above and below the one being sung.
const LINES_ABOUT: usize = 4;

/// The words of the song playing, filling the window: timed words with the line
/// being sung in the middle, in the accent, the ones before and after it fading
/// towards the edges; words without moments a page at a time, turned as the
/// song goes on. Returns how long until the words next change. Where there are
/// none — still being looked for, none found, or none sung — it says so in the
/// middle of the window rather than handing over to the figure.
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

/// Timed words around the line being sung — see [`lyrics`].
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

/// Where the words may stand: the window, less the top and the line at the
/// foot that says what is playing.
fn words_room(window: Rect) -> Rect {
    Rect::from_min_max(
        egui::pos2(window.left(), window.top() + FOOT_LINE),
        egui::pos2(window.right(), window.bottom() - FOOT_LINE - 40.0),
    )
}

/// Words without moments, a page at a time — see [`lyrics`].
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

/// How far above the foot of the window what is playing stands.
const FOOT_LINE: f32 = 48.0;

/// What is playing, quietly, at the foot: title and artist, centred.
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

/// The saver's picture that is felix's drawings: the one under „über" and
/// the one at the head of each public release's notes, one after the other.
/// What the config file calls it; the menu names it „release note drawings".
pub const DRAWINGS: &str = "release notes drawings";

/// The drawings, oldest first, and what release each belongs to, as the corner
/// says it. The first is the drawing under „über", there from the start; then
/// the one at the head of each public release's notes. A release that comes
/// with a drawing of its own adds it here — its mask made with
/// `bundle/make-drawing.py <scan> release-<version>.png`.
const RELEASE_DRAWINGS: &[(&str, &[u8])] = &[
    ("", super::menu::DRAWING),
    ("1.5.322", include_bytes!("release-1.5.322.png")),
    ("1.5.354", include_bytes!("release-1.5.354.png")),
    ("1.5.411", include_bytes!("release-1.5.411.png")),
    ("1.5.429", include_bytes!("release-1.5.429.png")),
];

/// How long each drawing stands, and how long one goes over into the next, in
/// seconds.
const DRAWING_HOLDS: f64 = 24.0;
const DRAWING_CROSSES: f64 = 3.0;

/// How tall a drawing stands, as a share of the window's height.
const DRAWING_HEIGHT: f32 = 0.72;

/// The mask of each drawing, read once.
fn drawing_masks() -> &'static Vec<image::GrayImage> {
    static MASKS: std::sync::OnceLock<Vec<image::GrayImage>> = std::sync::OnceLock::new();
    MASKS.get_or_init(|| {
        RELEASE_DRAWINGS
            .iter()
            .map(|(_, bytes)| image::load_from_memory(bytes).map(|p| p.to_luma8()).unwrap_or_default())
            .collect()
    })
}

/// Which drawing stands at `elapsed` seconds into the saver, the one after it,
/// and how far it has gone over to that one, 0 to 1. Starting on the newest.
fn drawing_at(elapsed: f64, count: usize) -> (usize, usize, f32) {
    let each = DRAWING_HOLDS + DRAWING_CROSSES;
    let step = (elapsed / each).floor().max(0.0) as usize;
    let into = elapsed - step as f64 * each;
    let now = (count - 1 + step) % count;
    let next = (now + 1) % count;
    let over = ((into - DRAWING_HOLDS) / DRAWING_CROSSES).clamp(0.0, 1.0) as f32;
    (now, next, over * over * (3.0 - 2.0 * over))
}

/// felix's drawings, one at a time in the middle of the window, in the
/// receded accent every drawing is drawn in, each going over into the next;
/// in the bottom right-hand corner which release it is from. Returns how long
/// until a frame is wanted again.
fn drawings(app: &mut GuiApp, ui: &egui::Ui, window: Rect, palette: &Palette, rise: f32, elapsed: f64) -> std::time::Duration {
    let masks = drawing_masks();
    let count = masks.len();
    if app.saver.drawn.len() != count {
        app.saver.drawn = vec![None; count];
    }
    let (now, next, over) = drawing_at(elapsed, count);
    let ppp = ui.ctx().pixels_per_point();
    let mut paint = |index: usize, strength: f32| {
        let mask = &masks[index];
        if strength <= 0.0 || mask.width() == 0 {
            return;
        }
        let aspect = mask.width() as f32 / mask.height() as f32;
        let height = (window.height() * DRAWING_HEIGHT).min(window.width() * 0.8 / aspect);
        let pixels = [(height * aspect * ppp).round().max(1.0) as usize, (height * ppp).round().max(1.0) as usize];
        let size = Vec2::new(pixels[0] as f32, pixels[1] as f32) / ppp;
        let texture = match &app.saver.drawn[index] {
            Some((at, texture)) if *at == pixels => texture.clone(),
            _ => {
                let small = image::imageops::resize(mask, pixels[0] as u32, pixels[1] as u32, image::imageops::FilterType::Lanczos3);
                let image = egui::ColorImage {
                    size: pixels,
                    pixels: small.pixels().map(|ink| Color32::from_white_alpha(ink.0[0])).collect(),
                    source_size: size,
                };
                let texture = ui.ctx().load_texture(format!("saver-drawing-{index}"), image, egui::TextureOptions::NEAREST);
                app.saver.drawn[index] = Some((pixels, texture.clone()));
                texture
            }
        };
        let centre = window.center() - Vec2::new(0.0, window.height() * 0.03);
        let corner = ((centre - size / 2.0).to_vec2() * ppp).round() / ppp;
        let rect = Rect::from_min_size(corner.to_pos2(), size);
        let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        ui.painter().image(texture.id(), rect, uv, palette.accent_faint.gamma_multiply(rise * strength));
    };
    paint(now, 1.0 - over);
    if next != now {
        paint(next, over);
    }
    let corner = |index: usize, strength: f32| {
        if strength <= 0.0 {
            return;
        }
        let (release, _) = RELEASE_DRAWINGS[index];
        let said = if index == 0 { tr!("initial release").to_string() } else { tr!("release {}", release) };
        let colour = palette.faint.gamma_multiply(rise * strength);
        let g = text::line(ui, &said, Role::Label, colour);
        let at = egui::pos2(window.right() - FOOT_LINE - g.size().x, window.bottom() - FOOT_LINE - g.size().y);
        ui.painter().galley(at, g, colour);
    };
    corner(now, 1.0 - over);
    if next != now {
        corner(next, over);
    }
    let each = DRAWING_HOLDS + DRAWING_CROSSES;
    let into = elapsed % each;
    if into < DRAWING_HOLDS {
        std::time::Duration::from_secs_f64(DRAWING_HOLDS - into + 0.01)
    } else {
        std::time::Duration::from_millis(33)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every drawing is there to read, and the saver starts on the newest,
    /// stands, goes over to the oldest and round again.
    #[test]
    fn the_release_drawings_go_round() {
        for (i, mask) in drawing_masks().iter().enumerate() {
            assert!(mask.width() > 100 && mask.height() > 100, "drawing {i}");
        }
        let n = RELEASE_DRAWINGS.len();
        assert_eq!(drawing_at(0.0, n), (n - 1, 0, 0.0));
        let (_, _, over) = drawing_at(DRAWING_HOLDS + DRAWING_CROSSES / 2.0, n);
        assert!((over - 0.5).abs() < 0.01);
        assert_eq!(drawing_at(DRAWING_HOLDS + DRAWING_CROSSES + 1.0, n).0, 0);
    }

    /// The wander holds each colour, then eases into the next, and comes
    /// round again.
    #[test]
    fn the_colour_wanders_round_the_scheme() {
        let colours = [Color32::RED, Color32::GREEN, Color32::BLUE];
        assert_eq!(wandering(&colours, 0.0), Color32::RED);
        assert_eq!(wandering(&colours, WANDER_SECS * 0.4), Color32::RED);
        assert_eq!(wandering(&colours, WANDER_SECS), Color32::GREEN);
        assert_ne!(wandering(&colours, WANDER_SECS * 1.75), Color32::GREEN);
        assert_eq!(wandering(&colours, WANDER_SECS * 3.0), Color32::RED);
    }

    /// Every scheme has colours to wander through, none of them lost on the
    /// ground and none twice.
    #[test]
    fn every_scheme_has_colours_to_wander_through() {
        for theme in super::super::theme::Theme::all() {
            let palette = theme.palette();
            let colours = scheme_colours(&palette);
            assert!(!colours.is_empty(), "{}", theme.name());
            assert!(!colours.contains(&palette.ground), "{}", theme.name());
        }
    }

    /// The gradient runs from one end to the other along its direction.
    #[test]
    fn the_gradient_runs_across_the_figure() {
        let across = Colouring::Across { from: Color32::BLACK, to: Color32::WHITE, angle: 0.0 };
        assert_eq!(across.at(0, 50, 100), Color32::BLACK);
        assert_eq!(across.at(100, 50, 100), Color32::WHITE);
        assert_eq!(Colouring::Plain(Color32::RED).at(3, 4, 10), Color32::RED);
    }

    /// A second of made-up music at `level` — a bass, a chord drifting in
    /// phase, a little noise — looked at the way the saver looks at it.
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

    /// A quiet record and a loud one come out about as strong: the exposure
    /// follows what is lit, not a number fixed for loud music.
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

    /// The made-up vibrations move, and they are never nothing.
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

    /// And a look at them, as the ones below: `KESSLER_VIBRATIONS=/tmp/v`
    /// writes /tmp/v-0.png, /tmp/v-1.png, … at moments some seconds apart.
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

    /// Not a test so much as a look, written to `KESSLER_SAVER_PNG`:
    /// `KESSLER_SAVER_PNG=/tmp/saver.png cargo test saver_picture -- --ignored`.
    /// `KESSLER_SAVER_LEVEL` sets how loud the made-up music is.
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
