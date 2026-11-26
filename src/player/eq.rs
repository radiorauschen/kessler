//! The equalizer: ten bands an octave apart, from 31 Hz to 16 kHz.
//!
//! Each band is a peaking filter (Robert Bristow-Johnson's cookbook), an
//! octave wide, raised or lowered by up to [`RANGE_DB`]. All ten run one after
//! the other on every channel, in the same tap that levels and listens — see
//! `audio::Tap` — so the music, the radio and the podcasts all go through
//! them, and the scope draws what is heard.
//!
//! What sets it apart from a row of sliders that only add:
//!
//! - **headroom**: whatever the curve raises, the whole is first lowered by
//!   the curve's highest point, so a raised band can never push the sound
//!   past full scale and break up. The curve's shape is what is heard, not
//!   its height;
//! - **no clicks**: a change walks to the new setting over some tens of
//!   milliseconds, the filters worked out afresh every few frames on the way
//!   there, rather than jumping in the middle of a wave;
//! - **nothing when flat**: a band at 0 dB is left out, and with all of them
//!   there and the walk over, the tap passes the sound through untouched.
//!
//! The setting itself lives in [`SETTINGS`], one for the whole program: the
//! window writes it, every tap reads it, a change is heard at once.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// The bands' middles, in hertz: octaves, as a graphic equalizer has them.
pub const BANDS: [f32; 10] = [31.0, 62.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0];

/// How many bands.
pub const COUNT: usize = BANDS.len();

/// How far a band goes either way, in decibels.
pub const RANGE_DB: f32 = 12.0;

/// How wide each band is: an octave, give or take, so neighbours meet
/// rather than leave a dip between them or pile up.
const Q: f64 = std::f64::consts::SQRT_2;

/// How long a change takes to be heard in full, in seconds — long enough not
/// to click, short enough to feel immediate under the hand.
const WALK_SECS: f32 = 0.06;

/// How many frames between two steps of the walk. The filters are worked out
/// again at each step, so this is also how often that happens while one is
/// under way.
const STEP_FRAMES: u32 = 32;

/// The most channels a tap keeps filter memory for; past that, a channel
/// shares the last one's — nothing kessler plays has more than two.
const CHANNELS: usize = 8;

/// The equalizer's setting, shared by the window and every tap.
pub struct Settings {
    on: AtomicBool,
    gains: [AtomicU32; COUNT],
    /// Counted up at every change, so a tap need only compare one number
    /// per frame to know whether to look again.
    version: AtomicU32,
}

impl Settings {
    pub const fn new() -> Self {
        Self {
            on: AtomicBool::new(false),
            gains: [const { AtomicU32::new(0) }; COUNT],
            version: AtomicU32::new(0),
        }
    }

    /// Switch it, and set every band, in decibels — clamped to the range.
    pub fn set(&self, on: bool, gains: [f32; COUNT]) {
        self.on.store(on, Ordering::Relaxed);
        for (slot, gain) in self.gains.iter().zip(gains) {
            let gain = if gain.is_finite() { gain.clamp(-RANGE_DB, RANGE_DB) } else { 0.0 };
            slot.store(gain.to_bits(), Ordering::Relaxed);
        }
        self.version.fetch_add(1, Ordering::Release);
    }

    pub fn on(&self) -> bool {
        self.on.load(Ordering::Relaxed)
    }

    pub fn gains(&self) -> [f32; COUNT] {
        std::array::from_fn(|i| f32::from_bits(self.gains[i].load(Ordering::Relaxed)))
    }

    /// What the taps should be walking to: the gains while it is on, flat
    /// while it is off.
    fn wanted(&self) -> [f32; COUNT] {
        if self.on() {
            self.gains()
        } else {
            [0.0; COUNT]
        }
    }

    fn version(&self) -> u32 {
        self.version.load(Ordering::Acquire)
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self::new()
    }
}

/// The one setting the program has.
pub static SETTINGS: Settings = Settings::new();

/// The ready-made settings, by the word the window shows for them, in
/// decibels from the lowest band to the highest.
pub const PRESETS: [(&str, [f32; COUNT]); 6] = [
    ("flach", [0.0; COUNT]),
    ("mehr bass", [6.0, 5.0, 3.5, 1.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
    ("weniger bass", [-6.0, -5.0, -3.5, -1.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
    ("mehr höhen", [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 2.5, 4.0, 5.0]),
    ("stimme", [-4.0, -3.0, -1.5, 0.0, 1.0, 2.5, 3.0, 2.0, 0.0, -1.0]),
    ("loudness", [5.0, 4.0, 2.0, 0.0, -1.0, -1.0, 0.0, 1.5, 3.0, 3.5]),
];

/// Which preset `gains` is, if any.
pub fn preset_of(gains: &[f32; COUNT]) -> Option<&'static str> {
    PRESETS
        .iter()
        .find(|(_, p)| p.iter().zip(gains).all(|(a, b)| (a - b).abs() < 0.05))
        .map(|(name, _)| *name)
}

/// The gains as the config keeps them: ten numbers, one decimal each.
pub fn gains_to_text(gains: &[f32; COUNT]) -> String {
    gains.iter().map(|g| format!("{:.1}", g)).collect::<Vec<_>>().join(" ")
}

/// Read back; anything missing or unreadable is flat.
pub fn gains_from_text(text: &str) -> [f32; COUNT] {
    let mut out = [0.0; COUNT];
    for (slot, word) in out.iter_mut().zip(text.split(|c: char| c.is_whitespace() || c == ',').filter(|w| !w.is_empty())) {
        if let Ok(value) = word.parse::<f32>() {
            if value.is_finite() {
                *slot = value.clamp(-RANGE_DB, RANGE_DB);
            }
        }
    }
    out
}

/// One biquad's coefficients, divided through by a0.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Coeffs {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
}

impl Coeffs {
    const THROUGH: Self = Self { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 };

    /// A peaking filter at `hz`, raised by `gain_db`, at `rate` frames a
    /// second.
    fn peaking(hz: f32, gain_db: f32, rate: u32) -> Self {
        if gain_db == 0.0 || rate == 0 {
            return Self::THROUGH;
        }
        let rate = f64::from(rate);
        let hz = f64::from(hz).min(rate * 0.45);
        let a = 10f64.powf(f64::from(gain_db) / 40.0);
        let w = 2.0 * std::f64::consts::PI * hz / rate;
        let alpha = w.sin() / (2.0 * Q);
        let cos = w.cos();
        let a0 = 1.0 + alpha / a;
        Self {
            b0: (1.0 + alpha * a) / a0,
            b1: (-2.0 * cos) / a0,
            b2: (1.0 - alpha * a) / a0,
            a1: (-2.0 * cos) / a0,
            a2: (1.0 - alpha / a) / a0,
        }
    }

    /// What it does to a sine at `hz`, in decibels.
    fn response_db(&self, hz: f64, rate: f64) -> f64 {
        let w = 2.0 * std::f64::consts::PI * hz / rate;
        let (c1, s1) = (w.cos(), w.sin());
        let (c2, s2) = ((2.0 * w).cos(), (2.0 * w).sin());
        let num_re = self.b0 + self.b1 * c1 + self.b2 * c2;
        let num_im = -(self.b1 * s1 + self.b2 * s2);
        let den_re = 1.0 + self.a1 * c1 + self.a2 * c2;
        let den_im = -(self.a1 * s1 + self.a2 * s2);
        let num = num_re * num_re + num_im * num_im;
        let den = (den_re * den_re + den_im * den_im).max(1e-300);
        10.0 * (num / den).max(1e-300).log10()
    }
}

/// The rate the curve is drawn for: what most music is at.
const DRAWN_RATE: u32 = 48_000;

/// What all ten bands together do to a sine at `hz`, in decibels — the curve
/// the window draws, before the headroom is taken off.
pub fn curve_db(gains: &[f32; COUNT], hz: f32) -> f32 {
    let rate = f64::from(DRAWN_RATE);
    BANDS
        .iter()
        .zip(gains)
        .filter(|(_, g)| **g != 0.0)
        .map(|(band, g)| Coeffs::peaking(*band, *g, DRAWN_RATE).response_db(f64::from(hz), rate))
        .sum::<f64>() as f32
}

/// How far the whole is lowered first so the curve's highest point stays at
/// full scale: its highest point, where that is above nought, in decibels.
pub fn headroom_db(gains: &[f32; COUNT]) -> f32 {
    if gains.iter().all(|g| *g <= 0.0) {
        return 0.0;
    }
    (0..=180)
        .map(|i| 20.0 * 10f32.powf(i as f32 / 60.0))
        .map(|hz| curve_db(gains, hz))
        .fold(0.0_f32, f32::max)
}

fn decibels_to_factor(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// One tap's equalizer: the filters, their memory, and the walk to whatever
/// the setting says.
pub struct Running {
    settings: &'static Settings,
    seen: u32,
    rate: u32,
    /// Where each band is now and where it is going, in decibels.
    now: [f32; COUNT],
    wanted: [f32; COUNT],
    /// The factor everything is lowered by first, now and wanted.
    pre_now: f32,
    pre_wanted: f32,
    coeffs: [Coeffs; COUNT],
    /// Per channel, per band, the transposed direct form's two numbers.
    memory: [[[f64; 2]; COUNT]; CHANNELS],
    /// Frames until the next step of the walk.
    until_step: u32,
    /// How far a band moves in one step, in decibels.
    step_db: f32,
}

impl Running {
    pub fn new(settings: &'static Settings, rate: u32) -> Self {
        let wanted = settings.wanted();
        let pre = decibels_to_factor(-headroom_db(&wanted));
        let steps = (WALK_SECS * rate.max(1) as f32 / STEP_FRAMES as f32).max(1.0);
        Self {
            settings,
            seen: settings.version(),
            rate,
            now: wanted,
            wanted,
            pre_now: pre,
            pre_wanted: pre,
            coeffs: std::array::from_fn(|i| Coeffs::peaking(BANDS[i], wanted[i], rate)),
            memory: [[[0.0; 2]; COUNT]; CHANNELS],
            until_step: STEP_FRAMES,
            step_db: 2.0 * RANGE_DB / steps,
        }
    }

    /// Whether anything is being done to the sound at all.
    fn idle(&self) -> bool {
        self.pre_now == 1.0 && self.pre_wanted == 1.0 && self.now.iter().chain(&self.wanted).all(|g| *g == 0.0)
    }

    /// Forget what the filters were ringing with: after a seek, the old
    /// place's tail does not belong in front of the new one.
    pub fn reset(&mut self) {
        self.memory = [[[0.0; 2]; COUNT]; CHANNELS];
    }

    /// Called once a frame, before its first sample: look at the setting,
    /// and take the next step of the walk when one is due.
    fn frame(&mut self) {
        let version = self.settings.version();
        if version != self.seen {
            self.seen = version;
            self.wanted = self.settings.wanted();
            self.pre_wanted = decibels_to_factor(-headroom_db(&self.wanted));
        }
        if self.until_step > 1 {
            self.until_step -= 1;
            return;
        }
        self.until_step = STEP_FRAMES;
        let mut moved = false;
        for (((now, wanted), coeffs), hz) in self.now.iter_mut().zip(self.wanted).zip(self.coeffs.iter_mut()).zip(BANDS) {
            if *now != wanted {
                let next = *now + (wanted - *now).clamp(-self.step_db, self.step_db);
                *now = next;
                *coeffs = Coeffs::peaking(hz, next, self.rate);
                moved = true;
            }
        }
        if self.pre_now != self.pre_wanted {
            let now = 20.0 * self.pre_now.log10();
            let wanted = 20.0 * self.pre_wanted.log10();
            let next = now + (wanted - now).clamp(-self.step_db, self.step_db);
            self.pre_now = if (next - wanted).abs() < 1e-4 { self.pre_wanted } else { decibels_to_factor(next) };
        }
        if moved && self.now.iter().all(|g| *g == 0.0) {
            self.reset();
        }
    }

    /// One sample of channel `channel`; `channel` 0 starts a frame.
    #[inline]
    pub fn process(&mut self, sample: f32, channel: u16) -> f32 {
        if channel == 0 {
            self.frame();
        }
        if self.idle() {
            return sample;
        }
        let memory = &mut self.memory[usize::from(channel).min(CHANNELS - 1)];
        let mut y = f64::from(sample * self.pre_now);
        for ((now, c), z) in self.now.iter().zip(&self.coeffs).zip(memory.iter_mut()) {
            if *now == 0.0 {
                continue;
            }
            let x = y;
            y = c.b0 * x + z[0];
            z[0] = c.b1 * x - c.a1 * y + z[1];
            z[1] = c.b2 * x - c.a2 * y;
        }
        y as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaked() -> &'static Settings {
        Box::leak(Box::new(Settings::new()))
    }

    /// A sine at `hz` through `running`, mono; the level of the last half,
    /// once the filters have settled, as a peak.
    fn peak_through(running: &mut Running, hz: f32, rate: u32) -> f32 {
        let n = rate as usize;
        let mut peak = 0.0f32;
        for i in 0..n {
            let x = (2.0 * std::f32::consts::PI * hz * i as f32 / rate as f32).sin() * 0.25;
            let y = running.process(x, 0);
            if i > n / 2 {
                peak = peak.max(y.abs());
            }
        }
        peak
    }

    fn db(factor: f32) -> f32 {
        20.0 * factor.log10()
    }

    #[test]
    fn off_or_flat_leaves_every_sample_as_it_was() {
        let settings = leaked();
        let mut running = Running::new(settings, 44_100);
        for i in 0..1000 {
            let x = (i as f32 * 0.37).sin();
            assert_eq!(running.process(x, (i % 2) as u16), x);
        }
        settings.set(false, [6.0; COUNT]);
        for i in 0..10_000 {
            let x = (i as f32 * 0.37).sin();
            assert_eq!(running.process(x, (i % 2) as u16), x, "off is off, whatever the gains say");
        }
    }

    #[test]
    fn a_raised_band_is_heard_raised_and_its_neighbours_less() {
        let settings = leaked();
        let mut gains = [0.0; COUNT];
        gains[5] = 9.0;
        settings.set(true, gains);
        let rate = 48_000;
        let mut running = Running::new(settings, rate);
        let at = db(peak_through(&mut running, 1000.0, rate) / 0.25);
        running.reset();
        let far = db(peak_through(&mut running, 100.0, rate) / 0.25);
        assert!(at.abs() < 0.5, "at the band {at}");
        assert!((far + 9.0).abs() < 1.0, "far from it, only the headroom {far}");
    }

    #[test]
    fn the_headroom_keeps_a_raised_curve_under_full_scale() {
        let gains = PRESETS.iter().find(|(n, _)| *n == "loudness").unwrap().1;
        let room = headroom_db(&gains);
        assert!(room > 4.0 && room < 7.0, "{room}");
        let settings = leaked();
        settings.set(true, gains);
        let rate = 44_100;
        for hz in [30.0, 60.0, 1000.0, 10_000.0] {
            let mut running = Running::new(settings, rate);
            let peak = peak_through(&mut running, hz, rate) / 0.25;
            assert!(peak <= 1.01, "{hz} Hz came out at {peak}");
        }
        assert_eq!(headroom_db(&[-3.0; COUNT]), 0.0, "only cuts: nothing to make room for");
    }

    #[test]
    fn a_change_walks_rather_than_jumps() {
        let settings = leaked();
        let rate = 48_000;
        let mut running = Running::new(settings, rate);
        let tone = |i: usize| (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / rate as f32).sin() * 0.5;
        let mut last = 0.0f32;
        for i in 0..2000 {
            last = running.process(tone(i), 0);
        }
        let mut gains = [0.0; COUNT];
        gains[5] = -12.0;
        settings.set(true, gains);
        let mut biggest_jump = 0.0f32;
        for i in 2000..2000 + rate as usize / 5 {
            let y = running.process(tone(i), 0);
            biggest_jump = biggest_jump.max((y - last).abs());
            last = y;
        }
        assert!(biggest_jump < 0.08, "{biggest_jump}");
        let settled = db(peak_through(&mut running, 1000.0, rate) / 0.25);
        assert!((settled + 12.0).abs() < 0.5, "{settled}");
    }

    #[test]
    fn the_drawn_curve_agrees_with_what_is_heard() {
        let mut gains = [0.0; COUNT];
        gains[2] = 6.0;
        gains[7] = -6.0;
        assert!((curve_db(&gains, 125.0) - 6.0).abs() < 0.3);
        assert!((curve_db(&gains, 4000.0) + 6.0).abs() < 0.3);
        assert!(curve_db(&gains, 1000.0).abs() < 1.0);
    }

    #[test]
    fn gains_go_to_text_and_back() {
        let gains = PRESETS[4].1;
        assert_eq!(gains_from_text(&gains_to_text(&gains)), gains);
        assert_eq!(gains_from_text(""), [0.0; COUNT]);
        assert_eq!(gains_from_text("40 x -3")[..3], [RANGE_DB, 0.0, -3.0]);
        assert_eq!(preset_of(&gains), Some("stimme"));
        assert_eq!(preset_of(&[1.0; COUNT]), None);
    }
}
