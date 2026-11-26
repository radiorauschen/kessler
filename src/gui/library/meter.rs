//! „zeiger": two moving-needle instruments behind the search field, as a tuner
//! of the seventies had them under a strip of glass — on the left how loud it
//! is (on a station, how much of it the network has brought ahead: its field
//! strength), on the right where the sound stands between the two speakers,
//! the needle in the middle while it is balanced.
//!
//! The field is the window; behind it an arc of scale for each, a word under
//! it in Misaki, and a needle turning on a pivot below the field's edge, so
//! only its upper part is seen.
//! The needles are not set to the value but pulled towards it, a spring and a
//! damper on a little mass: they take a moment to get there, go a hair past
//! and settle — which is what such a needle is.

use super::*;

/// How far a needle swings either side of upright, in radians.
const SWING: f32 = 0.62;

/// How quickly a needle answers, as the frequency it would ring at
/// undamped, and how much it is damped: a little under critical, so it goes
/// a hair past and comes back, as a moving coil does.
const RING_HZ: f32 = 1.7;
const DAMPING: f32 = 0.72;

/// The loudness the left needle reads, in dB below full scale: the bottom of
/// its scale and the top.
const QUIETEST_DB: f32 = -36.0;
const LOUDEST_DB: f32 = 0.0;

/// How far the right needle leans for a difference between the channels: a
/// tenth louder on one side is already a clear lean. Most music is balanced,
/// and a needle that hardly moves says nothing.
const LEAN: f32 = 4.0;

/// How long a step of the needles' motion is at most, in seconds: frames that
/// come late are made up in steps, so a stall is not a jump.
const STEP: f32 = 1.0 / 120.0;

/// Where an arc stands in its room: half as wide as the room by this much,
/// its top this far down; the word under it this far down; the needle
/// reaching this far past it — all against the room's height but the first.
const ARC_HALF_WIDE: f32 = 0.38;
const ARC_TOP: f32 = 0.42;
const WORD_AT: f32 = 0.6;
const NEEDLE_PAST: f32 = 0.24;

/// The ticks standing on each arc, as parts of its swing either side of
/// upright, and how tall they are against the room's height: five along the
/// level's arc, one in the middle of the balance's.
const LEVEL_TICKS: &[f32] = &[-1.0, -0.5, 0.0, 0.5, 1.0];
const MIDDLE_TICKS: &[f32] = &[0.0];
const TICK_TALL: f32 = 0.16;

/// Over how much of its width the needle's edge fades out: a little, so it
/// is still a line of the dots' own fineness.
const NEEDLE_SOFT: f32 = 0.6;

/// How far the needles are from the field towards the ink: as far as the
/// field's other pictures are drawn — see `picture::spectrum_strip`.
const NEEDLE_INK: f32 = 0.72;

/// How often the window draws while a needle moves, as the period of a clock
/// counted from the window's start — the clock the time and the rule under
/// the title tick on, four times a second: `[ui] meter_rate` a second, each
/// of them a whole number of steps to the quarter second, so each of the
/// clock's frames is one of the needles', and none comes in between. See
/// `picture::on_grid`.
pub(in crate::gui) const METER_RATES: [u32; 4] = [24, 12, 8, 4];

/// The needles' steps a second for `asked`: the nearest of [`METER_RATES`]
/// not above it, so a file that says 10 is read as 8.
pub(in crate::gui) fn meter_rate(asked: u32) -> u32 {
    METER_RATES.iter().copied().find(|&rate| rate <= asked).unwrap_or(4)
}

/// One needle: where it points, nought to one across its scale, and how fast
/// it is going.
#[derive(Default, Clone, Copy)]
struct Needle {
    at: f32,
    speed: f32,
}

impl Needle {
    /// Pulled towards `to` for `dt` seconds.
    fn pull(&mut self, to: f32, dt: f32) {
        let omega = std::f32::consts::TAU * RING_HZ;
        let mut left = dt.min(0.25);
        while left > 0.0 {
            let h = left.min(STEP);
            let force = omega * omega * (to - self.at) - 2.0 * DAMPING * omega * self.speed;
            self.speed += force * h;
            self.at = (self.at + self.speed * h).clamp(-0.05, 1.05);
            left -= h;
        }
    }

    fn moving(&self, to: f32) -> bool {
        self.speed.abs() > 0.002 || (to - self.at).abs() > 0.002
    }
}

/// The two needles and when they were last moved.
pub(in crate::gui) struct Meter {
    level: Needle,
    middle: Needle,
    last: Option<f64>,
    /// The scale and the needles as last screened, kept: a texture made
    /// afresh each frame is an allocation on the graphics card each frame.
    scale_picture: Option<egui::TextureHandle>,
    word_picture: Option<egui::TextureHandle>,
    needle_picture: Option<egui::TextureHandle>,
    /// What the scale and the words were last drawn for: they stand still,
    /// and are drawn again only when the field's size, the screen or the
    /// word changes — the language included, which the words are in.
    still_for: Option<(usize, usize, String, bool, crate::lang::Lang)>,
    /// Where the needles pointed when last drawn, in thousandths of a
    /// radian: a frame where neither has moved draws nothing new.
    needles_at: Option<[i32; 2]>,
    /// The needles' picture, kept between frames: only the strips the
    /// needles were in, and are in now, are drawn again.
    needle_grey: Vec<u8>,
}

impl Default for Meter {
    fn default() -> Self {
        Self {
            level: Needle::default(),
            middle: Needle { at: 0.5, speed: 0.0 },
            last: None,
            scale_picture: None,
            word_picture: None,
            needle_picture: None,
            still_for: None,
            needles_at: None,
            needle_grey: Vec::new(),
        }
    }
}

/// The two channels' loudness over the last moment, as root mean squares.
fn channels(pairs: &[[f32; 2]]) -> (f32, f32) {
    if pairs.is_empty() {
        return (0.0, 0.0);
    }
    let (l, r) = pairs.iter().fold((0.0, 0.0), |(l, r), [a, b]| (l + a * a, r + b * b));
    let n = pairs.len() as f32;
    ((l / n).sqrt(), (r / n).sqrt())
}

/// Where the left needle should point for a loudness, nought to one.
fn level_reading(rms: f32) -> f32 {
    let db = 20.0 * rms.max(1e-6).log10();
    ((db - QUIETEST_DB) / (LOUDEST_DB - QUIETEST_DB)).clamp(0.0, 1.0)
}

/// Where the right needle should point for the two channels, nought to one,
/// a half in the middle.
fn middle_reading(left: f32, right: f32) -> f32 {
    let sum = left + right;
    if sum < 1e-4 {
        return 0.5;
    }
    (0.5 + (right - left) / sum * LEAN / 2.0).clamp(0.0, 1.0)
}

/// Where the left needle should point on a station: how much the network has
/// brought ahead of what is heard, half its room a full scale.
fn field_reading(held: std::time::Duration) -> f32 {
    (held.as_secs_f32() / (crate::player::ahead::LIVE_ROOM.as_secs_f32() * 0.5)).clamp(0.0, 1.0)
}

/// Draw the two instruments in `field`, `showing` of the way in.
pub(in crate::gui) fn draw(app: &mut GuiApp, ui: &Ui, field: Rect, palette: &Palette, showing: f32) {
    let now = ui.input(|i| i.time);
    let dt = app.meter.last.map_or(0.0, |was| (now - was) as f32);
    app.meter.last = Some(now);
    let playing = app.player.state() == crate::player::PlaybackState::Playing;
    let station = app.player.current_track().is_none() && app.on_air.is_some();
    let (left, right) = if playing { channels(&app.heard.recent_pairs()) } else { (0.0, 0.0) };
    let level_to = match (playing, station) {
        (false, _) => 0.0,
        (true, true) => field_reading(crate::player::ahead::station_held()),
        (true, false) => level_reading((left * left + right * right).sqrt() / std::f32::consts::SQRT_2),
    };
    let middle_to = if playing { middle_reading(left, right) } else { 0.5 };
    app.meter.level.pull(level_to, dt);
    app.meter.middle.pull(middle_to, dt);

    let gap = field.width() * 0.06;
    let half = (field.width() - gap) / 2.0;
    let left_room = Rect::from_min_size(field.min, Vec2::new(half, field.height()));
    let right_room = Rect::from_min_size(egui::pos2(field.right() - half, field.top()), Vec2::new(half, field.height()));
    let dials = [
        Instrument::new(left_room, app.meter.level.at, LEVEL_TICKS),
        Instrument::new(right_room, app.meter.middle.at, MIDDLE_TICKS),
    ];
    let level_word = if station { tr!("feldstärke") } else { tr!("pegel") };
    let words = [level_word, tr!("mitte|zeiger")];

    let ppp = ui.ctx().pixels_per_point();
    let (width, height) = ((field.width() * ppp).round() as usize, (field.height() * ppp).round() as usize);
    if width < 8 || height < 8 {
        return;
    }
    let cell = (ppp.round() as usize).max(1);
    let point = cell as f32 / ppp;
    let at_cell = |x: usize, y: usize| {
        let (cx, cy) = (x / cell * cell + cell / 2, y / cell * cell + cell / 2);
        field.min + Vec2::new(cx as f32 + 0.5, cy as f32 + 0.5) / ppp
    };
    let ctx = ui.ctx().clone();
    let texture = |slot: &mut Option<egui::TextureHandle>, image: egui::ColorImage, name: &str| -> egui::TextureId {
        match slot {
            Some(handle) if handle.size() == [width, height] => {
                handle.set(image, egui::TextureOptions::NEAREST);
                handle.id()
            }
            slot => slot.insert(ctx.load_texture(name, image, egui::TextureOptions::NEAREST)).id(),
        }
    };

    let still_for = (width, height, app.config.ui.dither_method.clone(), station, crate::lang::current());
    if app.meter.still_for.as_ref() != Some(&still_for) || app.meter.scale_picture.is_none() || app.meter.word_picture.is_none() {
        let spelled = spell(&dials, words, field, point);
        let mut scale_grey = vec![0u8; width * height];
        let mut word_grey = vec![0u8; width * height];
        for y in 0..height {
            for x in 0..width {
                let at = y * width + x;
                if dials.iter().any(|d| d.on_scale(at_cell(x, y), point)) {
                    scale_grey[at] = 255;
                }
                if spelled_at(&spelled, (x / cell) as i64, (y / cell) as i64) {
                    word_grey[at] = 255;
                }
            }
        }
        let method = crate::gui::dither::Screen::from_name(&app.config.ui.dither_method);
        let screened = |grey: &[u8]| egui::ColorImage {
            size: [width, height],
            pixels: crate::gui::dither::screen(grey, width as u32, height as u32, method)
                .into_iter()
                .map(|ink| if ink { Color32::WHITE } else { Color32::TRANSPARENT })
                .collect(),
            source_size: Vec2::new(width as f32, height as f32),
        };
        texture(&mut app.meter.scale_picture, screened(&scale_grey), "meter-scale");
        texture(&mut app.meter.word_picture, screened(&word_grey), "meter-words");
        app.meter.still_for = Some(still_for);
        app.meter.needles_at = None;
        app.meter.needle_grey.clear();
    }
    let (Some(scale_id), Some(word_id)) = (app.meter.scale_picture.as_ref().map(|t| t.id()), app.meter.word_picture.as_ref().map(|t| t.id())) else {
        return;
    };

    let needles_at = [dials[0].angle, dials[1].angle].map(|a| (a * 1000.0).round() as i32);
    let fresh = app.meter.needle_grey.len() != width * height;
    if fresh || app.meter.needles_at != Some(needles_at) || app.meter.needle_picture.is_none() {
        if fresh {
            app.meter.needle_grey = vec![0u8; width * height];
        }
        let was: Vec<Instrument> = match app.meter.needles_at.filter(|_| !fresh) {
            Some(at) => dials.iter().zip(at).map(|(d, a)| d.pointing(a as f32 / 1000.0)).collect(),
            None => Vec::new(),
        };
        let grey = &mut app.meter.needle_grey;
        let to_pixels = |r: Rect| {
            let x0 = (((r.left() - field.left()) * ppp).floor().max(0.0) as usize).min(width);
            let x1 = (((r.right() - field.left()) * ppp).ceil().max(0.0) as usize).min(width);
            let y0 = (((r.top() - field.top()) * ppp).floor().max(0.0) as usize).min(height);
            let y1 = (((r.bottom() - field.top()) * ppp).ceil().max(0.0) as usize).min(height);
            (x0..x1, y0..y1)
        };
        let strips: Vec<Rect> = if fresh { vec![field] } else { was.iter().chain(dials.iter()).map(|d| d.needle_bounds(point)).collect() };
        for strip in &strips {
            let (xs, ys) = to_pixels(*strip);
            for y in ys {
                for x in xs.clone() {
                    let p = at_cell(x, y);
                    let cover = dials.iter().map(|d| d.needle_cover(p, point)).fold(0.0, f32::max);
                    grey[y * width + x] = (cover * 255.0).round() as u8;
                }
            }
        }
        let soft = egui::ColorImage {
            size: [width, height],
            pixels: grey.iter().map(|&a| Color32::from_rgba_premultiplied(a, a, a, a)).collect(),
            source_size: Vec2::new(width as f32, height as f32),
        };
        texture(&mut app.meter.needle_picture, soft, "meter-needles");
        app.meter.needles_at = Some(needles_at);
    }
    let Some(needle_id) = app.meter.needle_picture.as_ref().map(|t| t.id()) else {
        return;
    };

    let painter = ui.painter().with_clip_rect(field);
    let all = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    let scale = palette.field.lerp_to_gamma(palette.ink, 0.32).gamma_multiply(showing);
    let word_colour = palette.field.lerp_to_gamma(palette.ink, 0.45).gamma_multiply(showing);
    let needle = palette.field.lerp_to_gamma(palette.ink, NEEDLE_INK).gamma_multiply(showing);
    for (id, colour) in [(scale_id, scale), (word_id, word_colour), (needle_id, needle)] {
        painter.add(egui::epaint::RectShape::filled(field, MARKER_RADIUS, colour).with_texture(id, all));
    }

    let watched = ui.input(|i| i.focused);
    if watched && (playing || app.meter.level.moving(level_to) || app.meter.middle.moving(middle_to)) {
        let period = 1.0 / f64::from(meter_rate(app.config.ui.meter_rate));
        ui.ctx().request_repaint_after(super::picture::to_tick(ui.ctx(), period));
    } else {
        app.meter.last = None;
    }
}

/// The words in Misaki, a dot of it a cell of `point`, under each arc: each
/// word's columns, and the cell its first column's top stands in.
fn spell(dials: &[Instrument], words: [&str; 2], field: Rect, point: f32) -> Vec<(Vec<u8>, i64, i64)> {
    dials
        .iter()
        .zip(words)
        .map(|(dial, word)| {
            let columns = crate::misaki::columns(&word.to_lowercase());
            let wide = columns.len() as i64;
            let left = ((dial.room.center().x - field.left()) / point) as i64 - wide / 2;
            let top = ((dial.room.top() + dial.room.height() * WORD_AT - field.top()) / point) as i64;
            (columns, left, top)
        })
        .collect()
}

/// Whether a dot of the words is at the cell (`column`, `row`).
fn spelled_at(spelled: &[(Vec<u8>, i64, i64)], column: i64, row: i64) -> bool {
    spelled.iter().any(|(columns, left, top)| {
        let (cx, cy) = (column - left, row - top);
        (0..8).contains(&cy) && cx >= 0 && columns.get(cx as usize).is_some_and(|byte| byte & (1 << cy) != 0)
    })
}

/// The needles for a screen of their own — the Touch Bar's, each pixel lit
/// or dark — moving by their own clock, wherever they are drawn.
pub(in crate::gui) struct MeterDots {
    level: Needle,
    middle: Needle,
    last: Option<std::time::Instant>,
}

impl MeterDots {
    pub(in crate::gui) fn new() -> Self {
        Self { level: Needle::default(), middle: Needle { at: 0.5, speed: 0.0 }, last: None }
    }

    /// The two instruments `width` × `height` pixels, a pixel a cell, for
    /// what was heard last; `station` reads the left one as field strength.
    pub(in crate::gui) fn paint(&mut self, pairs: &[[f32; 2]], station: bool, width: usize, height: usize) -> Vec<bool> {
        let now = std::time::Instant::now();
        let dt = self.last.map_or(0.0, |was| now.duration_since(was).as_secs_f32());
        self.last = Some(now);
        let (left, right) = channels(pairs);
        let level_to = if station {
            field_reading(crate::player::ahead::station_held())
        } else {
            level_reading((left * left + right * right).sqrt() / std::f32::consts::SQRT_2)
        };
        self.level.pull(level_to, dt);
        self.middle.pull(middle_reading(left, right), dt);
        let field = Rect::from_min_size(egui::pos2(0.0, 0.0), Vec2::new(width as f32, height as f32));
        let gap = field.width() * 0.06;
        let half = (field.width() - gap) / 2.0;
        let dials = [
            Instrument::new(Rect::from_min_size(field.min, Vec2::new(half, field.height())), self.level.at, LEVEL_TICKS),
            Instrument::new(Rect::from_min_size(egui::pos2(field.right() - half, 0.0), Vec2::new(half, field.height())), self.middle.at, MIDDLE_TICKS),
        ];
        let level_word = if station { tr!("feldstärke") } else { tr!("pegel") };
        let spelled = spell(&dials, [level_word, tr!("mitte|zeiger")], field, 1.0);
        (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .map(|(x, y)| {
                let p = egui::pos2(x as f32 + 0.5, y as f32 + 0.5);
                dials.iter().any(|d| d.on_scale(p, 1.0) || d.needle_cover(p, 1.0) >= 0.5) || spelled_at(&spelled, x as i64, y as i64)
            })
            .collect()
    }
}

/// One instrument in its room: where its pivot is, how long its arc's
/// radius, where its ticks stand and where its needle points.
#[derive(Clone, Copy)]
struct Instrument {
    room: Rect,
    pivot: egui::Pos2,
    radius: f32,
    ticks: &'static [f32],
    angle: f32,
}

impl Instrument {
    fn new(room: Rect, at: f32, ticks: &'static [f32]) -> Self {
        let radius = (room.width() * ARC_HALF_WIDE) / SWING.sin();
        let pivot = egui::pos2(room.center().x, room.top() + room.height() * ARC_TOP + radius);
        Self { room, pivot, radius, ticks, angle: -SWING + 2.0 * SWING * at }
    }

    /// Whether `p` is on the scale: the arc, or one of its ticks standing on
    /// it — each a `thick` line, as thin as the needle.
    fn on_scale(&self, p: egui::Pos2, thick: f32) -> bool {
        if !self.room.contains(p) {
            return false;
        }
        let d = p - self.pivot;
        let (angle, r) = (d.x.atan2(-d.y), d.length());
        let arc = (r - self.radius).abs() <= thick * 0.5 && angle.abs() <= SWING;
        let tall = self.room.height() * TICK_TALL;
        let tick = (self.radius..=self.radius + tall).contains(&r)
            && self.ticks.iter().any(|t| ((angle - t * SWING) * r).abs() <= thick * 0.5);
        arc || tick
    }

    /// The same instrument, its needle at `angle`.
    fn pointing(&self, angle: f32) -> Self {
        Self { angle, ..*self }
    }

    /// The box the needle lies in, with room for its soft edge.
    fn needle_bounds(&self, thick: f32) -> Rect {
        let along = Vec2::new(self.angle.sin(), -self.angle.cos());
        let foot = self.pivot + along * (self.radius - self.room.height() * 2.0);
        let tip = self.pivot + along * (self.radius + self.room.height() * NEEDLE_PAST);
        Rect::from_two_pos(foot, tip).expand(thick * (1.0 + NEEDLE_SOFT) + 1.0)
    }

    /// How much of the cell at `p` the needle covers, nought to one: a line
    /// `thick` wide from below the field up past the arc, its edges softened
    /// over [`NEEDLE_SOFT`] of that, so it does not stair-step as it turns.
    fn needle_cover(&self, p: egui::Pos2, thick: f32) -> f32 {
        let soft = thick * NEEDLE_SOFT;
        if !self.room.expand(thick + soft).contains(p) {
            return 0.0;
        }
        let along = Vec2::new(self.angle.sin(), -self.angle.cos());
        let d = p - self.pivot;
        let t = d.dot(along);
        let across = (d - along * t).length();
        let tip = self.radius + self.room.height() * NEEDLE_PAST;
        let side = (1.0 - (across - thick * 0.5) / soft).clamp(0.0, 1.0);
        let end = (1.0 - (t - tip) / soft).clamp(0.0, 1.0);
        side.min(end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ticks stand on the arc, at their places and nowhere between.
    #[test]
    fn the_ticks_stand_on_the_arc() {
        let room = Rect::from_min_size(egui::pos2(0.0, 0.0), Vec2::new(100.0, 28.0));
        let dial = Instrument::new(room, 0.0, LEVEL_TICKS);
        let up = |angle: f32, r: f32| dial.pivot + Vec2::new(angle.sin(), -angle.cos()) * r;
        let above = dial.radius + room.height() * TICK_TALL * 0.6;
        assert!(dial.on_scale(up(0.0, above), 1.0));
        assert!(dial.on_scale(up(SWING * 0.5, above), 1.0));
        assert!(!dial.on_scale(up(SWING * 0.25, above), 1.0));
        assert!(dial.on_scale(up(SWING * 0.25, dial.radius), 1.0));
    }

    /// The needle covers its middle wholly, fades at its edge, and is gone a
    /// little way off.
    #[test]
    fn the_needle_is_soft_only_at_its_edge() {
        let room = Rect::from_min_size(egui::pos2(0.0, 0.0), Vec2::new(100.0, 28.0));
        let upright = Instrument::new(room, 0.5, MIDDLE_TICKS);
        let x = room.center().x;
        let y = room.top() + room.height() * 0.6;
        assert_eq!(upright.needle_cover(egui::pos2(x, y), 1.0), 1.0);
        let edge = upright.needle_cover(egui::pos2(x + 0.75, y), 1.0);
        assert!(edge > 0.0 && edge < 1.0, "{edge}");
        assert_eq!(upright.needle_cover(egui::pos2(x + 3.0, y), 1.0), 0.0);
    }

    /// A needle pulled to a place gets there, goes only a little past it,
    /// and settles.
    #[test]
    fn a_needle_overshoots_a_little_and_settles() {
        let mut needle = Needle::default();
        let mut most: f32 = 0.0;
        for _ in 0..120 {
            needle.pull(0.6, 1.0 / 60.0);
            most = most.max(needle.at);
        }
        assert!(most > 0.6 && most < 0.66, "went to {most}");
        assert!((needle.at - 0.6).abs() < 0.005 && !needle.moving(0.6));
    }

    /// The needles' frames fall on the clock's: every quarter second is one
    /// of their ticks.
    #[test]
    fn the_needles_tick_with_the_clock() {
        for rate in METER_RATES {
            let per_quarter = 0.25 * f64::from(rate);
            assert!((per_quarter - per_quarter.round()).abs() < 1e-9, "{rate}");
        }
        assert_eq!(meter_rate(10), 8);
        assert_eq!(meter_rate(0), 4);
        assert_eq!(meter_rate(100), 24);
    }

    /// Loud reads high, quiet low; balanced stands in the middle, one side
    /// louder leans to it.
    #[test]
    fn the_readings_go_the_right_way() {
        assert!(level_reading(0.5) > level_reading(0.05));
        assert_eq!(level_reading(0.0), 0.0);
        assert_eq!(middle_reading(0.2, 0.2), 0.5);
        assert!(middle_reading(0.1, 0.2) > 0.5);
        assert!(middle_reading(0.2, 0.1) < 0.5);
        assert_eq!(middle_reading(0.0, 0.0), 0.5);
    }
}
