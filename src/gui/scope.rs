pub const BANDS: usize = 7;

const LOWEST: f32 = 55.0;
const HIGHEST: f32 = 6000.0;

const Q: f32 = 2.2;

const SETTLE: usize = 600;

const DARK: f32 = 1e-6;

const WAVES: f32 = 3.0;

const FEWEST: usize = 40;

const REACH: f32 = 1.5;

const NEST: f32 = 0.6;

const WIDEST: f32 = 7.0;

const PERSIST: f32 = 0.5;

const FRAME: f32 = 1.0 / 30.0;

const EXPOSURE: f32 = 1.0;

const DWELL: f32 = 0.2;

const FORGET: f32 = 0.4;

const FLOOR: f32 = 0.004;

const HUSH: f32 = 1e-4;

const SHARE: f32 = 1.3;

const FAINTEST: f32 = 0.03;

const TILT: f32 = 0.8;

#[derive(Debug, Clone)]
struct Figure {
    at: usize,
    points: Vec<(f32, f32)>,
    presence: f32,
}

#[derive(Debug, Clone, Copy, Default)]
struct Band {
    loud: f32,
}

#[derive(Debug, Clone, Default)]
pub struct Scope {
    bands: [Band; BANDS],
    whole: f32,
    light: Vec<f32>,
    width: usize,
    height: usize,
    lit_at: Option<f64>,
    tuning: Option<Tuning>,}

pub const EXPOSED: Tuning = Tuning {
    waves: WAVES,
    persist: 0.8,
    exposure: EXPOSURE * 0.4,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tuning {
    pub waves: f32,
    pub persist: f32,
    pub exposure: f32,
}

impl Scope {
    pub fn tuned(tuning: Tuning) -> Self {
        Scope {
            tuning: Some(tuning),
            ..Scope::default()
        }
    }

    pub fn retune(&mut self, tuning: Tuning) {
        self.tuning = Some(tuning);
    }

    pub fn untune(&mut self) {
        self.tuning = None;
    }

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

    fn time_since(&mut self, now: f64) -> f32 {
        let gone = self.lit_at.map_or(0.0, |then| (now - then).clamp(0.0, 1.0)) as f32;
        self.lit_at = Some(now);
        gone
    }

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

    pub fn grey(&self) -> Vec<u8> {
        self.grey_at(self.exposure())
    }

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

    fn splat(&mut self, x: f32, y: f32, amount: f32) {
        splat(&mut self.light, self.width, self.height, x, y, amount);
    }
}

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

fn pitch_of(at: usize) -> f32 {
    let across = at as f32 / (BANDS - 1) as f32;
    LOWEST * (HIGHEST / LOWEST).powf(across)
}

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
