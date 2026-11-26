//! The picture behind the search field: a vectorscope for every register.
//!
//! A vectorscope draws two signals against each other — one across, one up —
//! and lets the beam trace whatever shape the pair of them makes. Fed the
//! left channel and the right, it is how a studio looks at a mix: a lone
//! sine is a ring, a mono voice a line, a wide chorus a cloud. It is also,
//! drawn as lines that glow and fade, the most *musical* picture there is:
//! every shape in it is the actual wave, not a reading of it.
//!
//! A search field is not the shape of a scope, though. It is ten times as wide
//! as it is tall, and one figure stretched across it is a flat scribble — most
//! of a mix sits in the middle, so its figure is a line, and a line ten times
//! as wide as it is tall says nothing at all. So the field holds a row of
//! them instead, one for each register of the music: the lowest at the left,
//! the highest at the right, the way a keyboard lies. Each one sees only its
//! own band, and each draws the left of it against the right of it a quarter
//! of a wave later — which turns a note into a ring whatever the mix is doing,
//! mono included, and turns a noise into a tangle. So a bass drum blooms a
//! big slow loop at the left edge, a bass line holds one steady, a voice knots
//! the middle and a cymbal throws a burst of short strokes at the right. The
//! neighbours overlap, so the row is one weave of lines rather than a shelf of
//! objects.
//!
//! Each figure is scaled against how loud *its own band* has been over the
//! last second or so, not against the loudest thing in the track. A hi-hat is
//! a hundredth of a bass drum's energy; measured on one scale it would never
//! be seen. Measured on its own, every register fills its place when it is
//! busy and shrinks when it rests — which is exactly the rhythm somebody
//! looking at the field is looking for.
//!
//! What is drawn is light, not a line. Every sample leaves the same amount of
//! it, spread along the stroke to the next: where the beam goes slowly, the
//! stroke is bright, and where it jumps, it is faint — the way a real scope's
//! phosphor behaves, and the reason a scope's picture has depth. The light
//! fades by half every thirtieth of a second, so a figure leaves its last few
//! shapes behind it as ghosts, and the grey that comes out is screened into
//! dots like everything else in this window that is a picture.
//!
//! Costs nothing while nothing plays and nobody looks, and at thirty frames a
//! second is about a hundred thousand multiplications a frame — a filter pass
//! over two thousand frames for each of seven bands, and the strokes.

/// How many registers, and so how many figures across the field.
pub const BANDS: usize = 7;

/// The middle of the lowest band and of the highest, in hertz.
///
/// A kick drum's body sits at the bottom of that, a hi-hat's hiss around the
/// top. Above six kilohertz a quarter of a wave is under two samples, and the
/// figure stops being a shape and becomes a smudge.
const LOWEST: f32 = 55.0;
const HIGHEST: f32 = 6000.0;

/// How narrow each band is: about two thirds of an octave wide, with the
/// bands an octave apart. Narrow enough that a note rings in its own figure
/// and not in the two beside it — broader, and one low note drew three
/// figures — and a mix is never so thin that it falls between two of them.
const Q: f32 = 2.2;

/// How many frames each filter is given to settle before anything it says is
/// drawn. The lowest band rings up over some thirteen milliseconds.
const SETTLE: usize = 600;

/// Light below this is dark: at the most exposure the saver uses it would
/// not make a single step of grey.
const DARK: f32 = 1e-6;

/// How many waves of its own pitch a figure draws.
///
/// Three: one wave is a ring, and a ring is all the same shape; a dozen is a
/// ball of wool. Three are enough to see whether the note is steady — the
/// rings lie on each other — or moving, when they fan out.
const WAVES: f32 = 3.0;

/// And never fewer frames than this, or the highest bands draw a comma.
const FEWEST: usize = 40;

/// How far a figure may reach past its own place, as a share of it. Past one
/// it overlaps its neighbours; the bass drum is meant to.
const REACH: f32 = 1.5;

/// How far the innermost of the registers sharing a place is drawn in from
/// the outermost: at 0.6, the highest reaches two fifths as far as the lowest.
const NEST: f32 = 0.6;

/// How much wider than tall a figure may be. A figure in a place of its own
/// is a little over twice as wide as the field is tall; one figure across the
/// whole field could be ten times, and a ring ten times as wide as it is tall
/// is a line. Seven keeps it an ellipse and still lets it take most of the
/// field.
const WIDEST: f32 = 7.0;

/// How much of the light is left after a thirtieth of a second.
const PERSIST: f32 = 0.5;

/// The thirtieth of a second that [`PERSIST`] is measured over.
const FRAME: f32 = 1.0 / 30.0;

/// How much light a stroke leaves, before it is bent into grey.
const EXPOSURE: f32 = 1.0;

/// How much a fast stroke is dimmed for being fast: a real phosphor would say
/// all of it, and a real phosphor at this size would show the bass and hide
/// everything else. A third keeps the depth and keeps the strokes.
const DWELL: f32 = 0.2;

/// How fast a band forgets how loud it was, a second at a time.
const FORGET: f32 = 0.4;

/// A register this far under the music as a whole is not scaled up to be
/// seen: it is quiet because the music has nothing there, not because it is
/// resting between two notes.
const FLOOR: f32 = 0.004;

/// Below this, nothing is drawn: it is the noise of the file, not music.
const HUSH: f32 = 1e-4;

/// How steeply brightness follows a register's share of the music. Over one,
/// the loudest register stands out from the others more than it does to the
/// ear, which in a picture this small is what makes it legible.
const SHARE: f32 = 1.3;

/// Below this share of the music, a register is not drawn at all. Every band
/// hears a little of every other through its filter's skirts, and scaled up
/// to fill its place that little is a full-sized figure — of a note that is
/// not there. A floor under the brightness kept it on screen: one high note
/// drew the bass rings too, faint, but with thirty times the strokes.
const FAINTEST: f32 = 0.03;

/// How much a register's loudness is lifted for every octave it sits above
/// the lowest, before the registers are compared. Music loses energy as it
/// goes up; half a power of the pitch is what the ear hears as level, and a
/// little more is what keeps a hi-hat on the field at all.
const TILT: f32 = 0.8;

/// One register's figure: its points, each −1 … 1 across and up, and how much
/// light it is drawn with.
#[derive(Debug, Clone)]
struct Figure {
    at: usize,
    points: Vec<(f32, f32)>,
    presence: f32,
}

/// What one register remembers of its own loudness.
#[derive(Debug, Clone, Copy, Default)]
struct Band {
    /// How loud it has been lately — its loudest, forgotten at [`FORGET`].
    loud: f32,
}

/// The light on the field, and what the figures remember.
#[derive(Debug, Clone, Default)]
pub struct Scope {
    bands: [Band; BANDS],
    /// How loud the music as a whole has been lately, root-mean-square.
    whole: f32,
    /// One value a device pixel, row by row from the top.
    light: Vec<f32>,
    width: usize,
    height: usize,
    /// When the light was last faded.
    lit_at: Option<f64>,
    /// Set for the screen saver, where there is room for more: more waves
    /// of each note, a longer afterglow and more light a sample. `None` is
    /// the field's own [`WAVES`], [`PERSIST`] and [`EXPOSURE`].
    tuning: Option<Tuning>,}

/// The field's scope long-exposed, for a picture drawn four times a second
/// rather than twenty: the light lasts long enough that a quarter of a second
/// of strokes is still on the plate when it is shown — a fifth of the first is
/// left at the last — and each stroke leaves less, so the plate is no brighter
/// for holding more of them.
pub const EXPOSED: Tuning = Tuning {
    waves: WAVES,
    persist: 0.8,
    exposure: EXPOSURE * 0.4,
};

/// A scope drawn bigger than the field — see [`Scope::tuned`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tuning {
    pub waves: f32,
    pub persist: f32,
    pub exposure: f32,
}

impl Scope {
    /// A scope with its own number of waves, afterglow and exposure.
    pub fn tuned(tuning: Tuning) -> Self {
        Scope {
            tuning: Some(tuning),
            ..Scope::default()
        }
    }

    /// Change the tuning, keeping the light that is already there.
    pub fn retune(&mut self, tuning: Tuning) {
        self.tuning = Some(tuning);
    }

    /// Back to the field's own tuning.
    pub fn untune(&mut self) {
        self.tuning = None;
    }

    /// When the light was last laid down, on the window's clock.
    pub fn lit_at(&self) -> Option<f64> {
        self.lit_at
    }

    fn waves(&self) -> f32 {
        self.tuning.map_or(WAVES, |t| t.waves)
    }

    fn persist(&self) -> f32 {
        self.tuning.map_or(PERSIST, |t| t.persist)
    }

    fn exposure(&self) -> f32 {
        self.tuning.map_or(EXPOSURE, |t| t.exposure)
    }

    /// Fade the light for the time gone by, and draw what the card played.
    ///
    /// `frames` is what the tap kept, oldest first; `rate` is how many of them
    /// make a second. `width` and `height` are the field in device pixels —
    /// the light is started over if they change, rather than stretched.
    pub fn look(
        &mut self,
        frames: &[[f32; 2]],
        rate: u32,
        width: usize,
        height: usize,
        places: usize,
        now: f64,
    ) {
        if (width, height) != (self.width, self.height) {
            self.width = width;
            self.height = height;
            self.light = vec![0.0; width * height];
        }
        let gone = self.time_since(now);
        let fade = self.persist().powf(gone / FRAME);
        for value in &mut self.light {
            *value *= fade;
            if *value < DARK {
                *value = 0.0;
            }
        }
        if width < 8 || height < 8 {
            return;
        }
        let persist = self.persist();
        let share = if gone > 0.0 && persist < 1.0 {
            ((1.0 - fade) / (1.0 - persist)).clamp(0.25, 4.0)
        } else {
            1.0
        };
        for (points, light) in self.placed(frames, rate, width as f32, height as f32, places, gone) {
            for pair in points.windows(2) {
                self.stroke(pair[0], pair[1], light * share);
            }
        }
    }

    /// Seconds since the last look, at most one, and now the last look.
    fn time_since(&mut self, now: f64) -> f32 {
        let gone = self.lit_at.map_or(0.0, |then| (now - then).clamp(0.0, 1.0)) as f32;
        self.lit_at = Some(now);
        gone
    }

    /// Every register's figure, placed in a field `width` × `height`, and the
    /// light it is drawn with.
    fn placed(
        &mut self,
        frames: &[[f32; 2]],
        rate: u32,
        width: f32,
        height: f32,
        places: usize,
        gone: f32,
    ) -> Vec<(Vec<(f32, f32)>, f32)> {
        let places = places.clamp(1, BANDS);
        let place = width / places as f32;
        let up = height * 0.5 - 1.5;
        let across = (place * 0.5 * REACH).min(width * 0.5 - 2.0).min(up * WIDEST);
        let figures = self.figures(frames, rate, gone);
        figures
            .into_iter()
            .map(|figure| {
                let (at, nest) = placed(figure.at, places);
                let middle = (place * (at as f32 + 0.5), height * 0.5);
                let reach = (across * nest, up * nest);
                let points: Vec<(f32, f32)> = figure
                    .points
                    .iter()
                    .map(|(x, y)| (middle.0 + x * reach.0, middle.1 - y * reach.1))
                    .collect();
                (points, figure.presence * nest * nest)
            })
            .collect()
    }

    /// What each register draws this time, in its own square of −1 … 1.
    ///
    /// Apart from the drawing so it can be tested without pixels: which band
    /// a note lands in, how big a figure is, and how bright, are all decided
    /// here.
    fn figures(&mut self, frames: &[[f32; 2]], rate: u32, gone: f32) -> Vec<Figure> {
        if rate == 0 || frames.len() <= SETTLE + FEWEST {
            return Vec::new();
        }
        let forget = FORGET.powf(gone);
        let whole = (frames.iter().map(|&[l, r]| l * l + r * r).sum::<f32>()
            / (2 * frames.len()) as f32)
            .sqrt();
        self.whole = whole.max(self.whole * forget);

        let mut left = vec![0.0; frames.len()];
        let mut right = vec![0.0; frames.len()];
        let mut out: Vec<(Figure, f32)> = Vec::with_capacity(BANDS);
        for at in 0..BANDS {
            let pitch = pitch_of(at);
            if pitch * 2.2 >= rate as f32 {
                continue;
            }
            let filter = Biquad::band(pitch, Q, rate as f32);
            filter.run(frames.iter().map(|f| f[0]), &mut left);
            filter.run(frames.iter().map(|f| f[1]), &mut right);

            let wave = rate as f32 / pitch;
            let drawn = ((wave * self.waves()) as usize)
                .max(FEWEST)
                .min(frames.len() - SETTLE);
            let from = frames.len() - drawn;
            let behind = wave * 0.25;

            let mut loudest: f32 = 0.0;
            let raw: Vec<(f32, f32)> = (from..frames.len())
                .map(|i| {
                    let x = left[i];
                    let y = later(&right, i as f32 - behind);
                    loudest = loudest.max(x.abs()).max(y.abs());
                    (x, y)
                })
                .collect();
            let band = &mut self.bands[at];
            band.loud = (loudest * 0.4).max(band.loud * forget);
            if loudest < HUSH {
                continue;
            }
            let gain = 1.0 / (1.8 * band.loud.max(FLOOR * self.whole).max(HUSH));
            let points = raw
                .into_iter()
                .map(|(x, y)| ((x * gain).tanh(), (y * gain).tanh()))
                .collect();
            let tilted = loudest * (pitch / LOWEST).powf(TILT);
            let filled = ((loudest * gain).tanh() / 0.88).min(1.0);
            out.push((
                Figure {
                    at,
                    points,
                    presence: filled,
                },
                tilted,
            ));
        }

        let top = out.iter().map(|(_, t)| *t).fold(0.0_f32, f32::max);
        out.into_iter()
            .map(|(mut figure, tilted)| {
                figure.presence *= (tilted / top.max(HUSH)).powf(SHARE).min(1.0);
                figure
            })
            .filter(|figure| figure.presence >= FAINTEST)
            .collect()
    }

    /// The light as grey, a byte a device pixel — ready to be screened.
    pub fn grey(&self) -> Vec<u8> {
        self.grey_at(self.exposure())
    }

    /// The same at an exposure given outright.
    pub fn grey_at(&self, exposure: f32) -> Vec<u8> {
        self.light
            .iter()
            .map(|&v| {
                if v == 0.0 {
                    0
                } else {
                    ((1.0 - (-v * exposure).exp()) * 255.0) as u8
                }
            })
            .collect()
    }

    /// How bright the brighter part of the picture is: the light that
    /// `share` of the lit pixels stay under, from a sample of them — for an
    /// exposure that follows the music rather than a fixed one. `None` while
    /// nothing is lit.
    pub fn bright(&self, share: f32) -> Option<f32> {
        const EVERY: usize = 5;
        let mut lit: Vec<f32> = self
            .light
            .iter()
            .step_by(EVERY)
            .copied()
            .filter(|v| *v > 1e-4)
            .collect();
        if lit.len() < 16 {
            return None;
        }
        let at = ((lit.len() - 1) as f32 * share.clamp(0.0, 1.0)) as usize;
        let (_, value, _) = lit.select_nth_unstable_by(at, |a, b| a.total_cmp(b));
        Some(*value)
    }

    /// One sample's worth of light, laid along the way to the next.
    fn stroke(&mut self, from: (f32, f32), to: (f32, f32), presence: f32) {
        let length = (to.0 - from.0).hypot(to.1 - from.1);
        let steps = (length / 0.7).ceil().max(1.0);
        let each = presence * steps.powf(-DWELL);
        let n = steps as usize;
        for step in 0..n {
            let t = (step as f32 + 0.5) / steps;
            self.splat(from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t, each);
        }
    }

    /// Light at a point between pixels, shared among the four around it.
    fn splat(&mut self, x: f32, y: f32, amount: f32) {
        splat(&mut self.light, self.width, self.height, x, y, amount);
    }
}

/// Light at a point between pixels, shared among the four around it — in a
/// buffer of `width` × `height` values, row by row. Shared with the line in
/// [`super::pen`], which lays its light the same way.
pub(super) fn splat(light: &mut [f32], width: usize, height: usize, x: f32, y: f32, amount: f32) {
    let (fx, fy) = (x.floor(), y.floor());
    let (ax, ay) = (x - fx, y - fy);
    for (dx, dy, share) in [
        (0, 0, (1.0 - ax) * (1.0 - ay)),
        (1, 0, ax * (1.0 - ay)),
        (0, 1, (1.0 - ax) * ay),
        (1, 1, ax * ay),
    ] {
        let (px, py) = (fx as i64 + dx, fy as i64 + dy);
        if px < 0 || py < 0 || px >= width as i64 || py >= height as i64 {
            continue;
        }
        light[py as usize * width + px as usize] += amount * share;
    }
}

/// The middle of band `at`, spaced evenly by octave from [`LOWEST`] to
/// [`HIGHEST`].
fn pitch_of(at: usize) -> f32 {
    let across = at as f32 / (BANDS - 1) as f32;
    LOWEST * (HIGHEST / LOWEST).powf(across)
}

/// Which of `places` a register is drawn in, and how far out it reaches in
/// there, 0 … 1.
///
/// Seven places is a figure each. Fewer, and neighbouring registers share one
/// — laid inside each other rather than on top of each other: the lowest of
/// them outermost, reaching all the way, and each one above it a ring further
/// in. So one place, the whole field for one figure, is a big eye of seven
/// rings with the bass drum at its rim and the cymbals at its pupil, and its
/// lines cross wherever two registers are both busy.
fn placed(band: usize, places: usize) -> (usize, f32) {
    let places = places.clamp(1, BANDS);
    let at = band * places / BANDS;
    let sharing: Vec<usize> = (0..BANDS).filter(|b| b * places / BANDS == at).collect();
    let within = sharing.iter().position(|b| *b == band).unwrap_or(0);
    let nest = if sharing.len() > 1 {
        1.0 - NEST * within as f32 / (sharing.len() - 1) as f32
    } else {
        1.0
    };
    (at, nest)
}

/// A sample between two samples, by straight line. `at` below nought is
/// silence — before the window began.
fn later(samples: &[f32], at: f32) -> f32 {
    if at < 0.0 {
        return 0.0;
    }
    let i = at.floor() as usize;
    let t = at - i as f32;
    let a = samples.get(i).copied().unwrap_or(0.0);
    let b = samples.get(i + 1).copied().unwrap_or(a);
    a + (b - a) * t
}

/// A band-pass filter: the cookbook one, with its peak at unity so every band
/// comes out on the same scale as it went in.
#[derive(Debug, Clone, Copy)]
struct Biquad {
    b0: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

impl Biquad {
    fn band(pitch: f32, q: f32, rate: f32) -> Self {
        let w = std::f32::consts::TAU * pitch / rate;
        let alpha = w.sin() / (2.0 * q);
        let a0 = 1.0 + alpha;
        Self {
            b0: alpha / a0,
            b2: -alpha / a0,
            a1: -2.0 * w.cos() / a0,
            a2: (1.0 - alpha) / a0,
        }
    }

    /// Filter from a standing start into `out`, which is as long as the input.
    fn run(&self, input: impl Iterator<Item = f32>, out: &mut [f32]) {
        let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
        for (x, slot) in input.zip(out.iter_mut()) {
            let y = self.b0 * x + self.b2 * x2 - self.a1 * y1 - self.a2 * y2;
            x2 = x1;
            x1 = x;
            y2 = y1;
            y1 = y;
            *slot = y;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The same tone, drawn for two seconds at thirty frames a second and
    /// at twenty, comes out as bright: fewer frames, each with more light.
    #[test]
    fn the_light_does_not_depend_on_the_frame_rate() {
        let shone = |fps: f64| {
            let mut scope = Scope::default();
            let frames = tone(pitch_of(3), 0.5, 0.0);
            let n = (2.0 * fps) as usize;
            for i in 0..=n {
                scope.look(&frames, RATE, W, H, 1, i as f64 / fps);
            }
            scope.light.iter().sum::<f32>()
        };
        let (thirty, twenty) = (shone(30.0), shone(20.0));
        assert!((twenty / thirty - 1.0).abs() < 0.1, "30: {thirty}, 20: {twenty}");
    }

    const RATE: u32 = 44_100;
    const W: usize = 600;
    const H: usize = 56;

    fn tone(pitch: f32, amplitude: f32, width: f32) -> Vec<[f32; 2]> {
        (0..2048)
            .map(|i| {
                let t = i as f32 / RATE as f32;
                let s = (std::f32::consts::TAU * pitch * t).sin() * amplitude;
                let r = (std::f32::consts::TAU * pitch * t + width).sin() * amplitude;
                [s, r]
            })
            .collect()
    }

    fn lit(scope: &Scope) -> Vec<(usize, usize, f32)> {
        let w = scope.width;
        scope
            .light
            .iter()
            .enumerate()
            .filter(|(_, v)| **v > 0.05)
            .map(|(i, v)| (i % w, i / w, *v))
            .collect()
    }

    #[test]
    fn silence_draws_nothing() {
        let mut scope = Scope::default();
        scope.look(&vec![[0.0; 2]; 2048], RATE, W, H, BANDS, 0.0);
        assert!(scope.grey().iter().all(|g| *g == 0));
    }

    fn figure(figures: &[Figure], at: usize) -> &Figure {
        figures.iter().find(|f| f.at == at).expect("the band drew nothing")
    }

    fn size(figure: &Figure) -> f32 {
        figure
            .points
            .iter()
            .map(|(x, y)| x.hypot(*y))
            .fold(0.0_f32, f32::max)
    }

    /// A note is brightest in the place of its own pitch, and the registers
    /// two away from it are drawn faint: low at the left, high at the right.
    #[test]
    fn a_note_is_brightest_in_the_place_of_its_pitch() {
        for at in 0..BANDS {
            let mut scope = Scope::default();
            let figures = scope.figures(&tone(pitch_of(at), 0.5, 0.0), RATE, 0.0);
            let brightest = figures
                .iter()
                .max_by(|a, b| a.presence.total_cmp(&b.presence))
                .unwrap();
            assert_eq!(brightest.at, at, "a note of band {at} lit band {}", brightest.at);
            for other in figures.iter().filter(|f| f.at.abs_diff(at) >= 2) {
                assert!(
                    other.presence < 0.35,
                    "band {} is drawn at {} for a note two bands away",
                    other.at,
                    other.presence
                );
            }
        }
    }

    /// And the note's figure is a ring: the same distance from its middle
    /// all the way round, which is the whole difference between a
    /// vectorscope and a blob.
    #[test]
    fn a_note_is_a_ring() {
        let mut scope = Scope::default();
        let figures = scope.figures(&tone(pitch_of(2), 0.5, 0.0), RATE, 0.0);
        let ring = figure(&figures, 2);
        let radii: Vec<f32> = ring.points.iter().map(|(x, y)| x.hypot(*y)).collect();
        let (small, big) = radii
            .iter()
            .fold((f32::MAX, 0.0_f32), |(lo, hi), r| (lo.min(*r), hi.max(*r)));
        assert!(big > 0.6, "the ring is too small to see: {big}");
        assert!(small > big * 0.75, "not a ring: from {small} to {big}");
    }

    /// And drawn as a ring on the field: light all round, none in the middle.
    #[test]
    fn the_ring_is_hollow_on_the_field() {
        let mut scope = Scope::default();
        scope.look(&tone(pitch_of(2), 0.5, 0.0), RATE, W, H, BANDS, 0.0);
        let place = W as f32 / BANDS as f32;
        let (cx, cy) = ((place * 2.5) as usize, H / 2);
        let middle = scope.light[cy * W + cx];
        let rim = lit(&scope).iter().map(|p| p.2).fold(0.0_f32, f32::max);
        assert!(rim > 0.2, "the ring is too faint to see: {rim}");
        assert!(middle < rim * 0.1, "the middle of the ring is lit: {middle} of {rim}");
    }

    /// What a picture of music is for: a louder moment is a bigger figure.
    /// Measured against the band's own recent loudness — so after a stretch
    /// of the same note, a quieter one shrinks, and a second later, once the
    /// loud one is forgotten, it has grown back to fill its place.
    #[test]
    fn a_quieter_moment_is_a_smaller_figure() {
        let pitch = pitch_of(1);
        let mut scope = Scope::default();
        let loud = size(figure(&scope.figures(&tone(pitch, 0.6, 0.0), RATE, 0.0), 1));
        let quiet = size(figure(&scope.figures(&tone(pitch, 0.1, 0.0), RATE, 0.1), 1));
        assert!(quiet * 2.0 < loud, "loud reached {loud}, quiet {quiet}");
        let mut later = 0.0;
        for _ in 0..30 {
            later = size(figure(&scope.figures(&tone(pitch, 0.1, 0.0), RATE, 0.1), 1));
        }
        assert!(later > loud * 0.9, "the quiet note never grew back: {later} of {loud}");
    }

    /// The light goes: a figure that is not drawn again has faded to nothing
    /// in a fraction of a second, and to half in a thirtieth.
    #[test]
    fn the_light_fades() {
        let mut scope = Scope::default();
        scope.look(&tone(pitch_of(3), 0.5, 0.0), RATE, W, H, BANDS, 0.0);
        let before: f32 = scope.light.iter().sum();
        scope.look(&[], RATE, W, H, BANDS, FRAME as f64);
        let after: f32 = scope.light.iter().sum();
        assert!((after / before - PERSIST).abs() < 0.01);
        scope.look(&[], RATE, W, H, BANDS, 0.5);
        assert!(scope.grey().iter().all(|g| *g < 2));
    }

    /// Two channels out of step draw a different figure from two in step:
    /// the stereo of the mix is in the picture.
    #[test]
    fn the_stereo_changes_the_figure() {
        let mut narrow = Scope::default();
        narrow.look(&tone(pitch_of(2), 0.5, 0.0), RATE, W, H, BANDS, 0.0);
        let mut wide = Scope::default();
        wide.look(&tone(pitch_of(2), 0.5, 1.2), RATE, W, H, BANDS, 0.0);
        let differ = narrow
            .grey()
            .iter()
            .zip(wide.grey())
            .filter(|(a, b)| (**a as i32 - *b as i32).abs() > 40)
            .count();
        assert!(differ > 50, "only {differ} pixels differ");
    }

    #[test]
    fn nothing_is_drawn_outside_the_field() {
        let mut scope = Scope::default();
        let loud: Vec<[f32; 2]> = (0..2048)
            .map(|i| {
                let s = if (i / 7) % 2 == 0 { 1.0 } else { -1.0 };
                [s, -s]
            })
            .collect();
        scope.look(&loud, RATE, W, H, BANDS, 0.0);
        assert_eq!(scope.light.len(), W * H);
    }

    /// Every register has a place however many places there are, the low ones
    /// to the left of the high ones, and no place is left empty.
    #[test]
    fn every_register_has_a_place_in_order() {
        for places in 1..=BANDS {
            let mut last = 0;
            let mut used = vec![false; places];
            for band in 0..BANDS {
                let (at, nest) = placed(band, places);
                assert!(at < places, "band {band} of {places} places went to {at}");
                assert!(at >= last, "band {band} of {places} places is left of the one below it");
                assert!(nest > 0.3 && nest <= 1.0, "band {band} reaches {nest}");
                used[at] = true;
                last = at;
            }
            assert!(used.iter().all(|u| *u), "{places} places leave one empty");
        }
        for band in 0..BANDS {
            assert_eq!(placed(band, BANDS), (band, 1.0));
        }
        let reaches: Vec<f32> = (0..BANDS).map(|b| placed(b, 1).1).collect();
        assert_eq!(reaches[0], 1.0);
        assert!(reaches.windows(2).all(|w| w[1] < w[0]), "{reaches:?}");
    }

    /// With one place, a high note is drawn small in the middle of the
    /// field, and a low one reaches out towards its edges.
    #[test]
    fn one_big_figure_has_the_bass_at_its_rim() {
        let spread = |band: usize| {
            let mut scope = Scope::default();
            scope.look(&tone(pitch_of(band), 0.5, 0.0), RATE, W, H, 1, 0.0);
            let all = lit(&scope);
            let top = all.iter().map(|p| p.2).fold(0.0_f32, f32::max);
            let lit: Vec<usize> = all.iter().filter(|p| p.2 > top * 0.5).map(|p| p.0).collect();
            let (lo, hi) = (*lit.iter().min().unwrap(), *lit.iter().max().unwrap());
            ((lo + hi) / 2, hi - lo)
        };
        let (low_middle, low) = spread(0);
        let (high_middle, high) = spread(BANDS - 1);
        assert!(low_middle.abs_diff(W / 2) < 8 && high_middle.abs_diff(W / 2) < 8);
        assert!(high * 2 < low, "the high ring spans {high}, the low one {low}");
    }

    /// Not a test: draws a few seconds of a real recording, frame by frame,
    /// so the picture can be looked at without a window.
    ///
    /// `cargo test --release scope::tests::look -- --ignored` with
    /// `SCOPE_RAW` pointing at interleaved f32 stereo at 44.1 kHz.
    #[test]
    #[ignore]
    fn look() {
        let Ok(path) = std::env::var("SCOPE_RAW") else {
            return;
        };
        let start: f32 = std::env::var("SCOPE_AT").ok().and_then(|s| s.parse().ok()).unwrap_or(20.0);
        let bytes = std::fs::read(path).unwrap();
        let samples: Vec<f32> = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|&c| f32::from_le_bytes(c))
            .collect();
        let frames: Vec<[f32; 2]> = samples.as_chunks::<2>().0.to_vec();
        let places: usize = std::env::var("SCOPE_PLACES").ok().and_then(|s| s.parse().ok()).unwrap_or(BANDS);
        let mut scope = Scope::default();
        let shown = 12;
        let mut sheet: Vec<u8> = Vec::new();
        for frame in 0..90 {
            let t = start + frame as f32 * FRAME;
            let end = (t * RATE as f32) as usize;
            scope.look(&frames[end - 2048..end], RATE, W, H, places, t as f64);
            if frame >= 90 - shown * 3 && frame % 3 == 0 {
                let dots = super::super::dither::screen(
                    &scope.grey(),
                    W as u32,
                    H as u32,
                    super::super::dither::Screen::Ordered,
                );
                sheet.extend(dots.iter().map(|d| if *d { 20u8 } else { 235 }));
                sheet.extend(std::iter::repeat_n(255u8, W * 6));
            }
        }
        let rows = sheet.len() / W;
        let mut out = format!("P5\n{W} {rows}\n255\n").into_bytes();
        out.extend(sheet);
        std::fs::write("/tmp/scope/sheet.pgm", out).unwrap();
    }
}
