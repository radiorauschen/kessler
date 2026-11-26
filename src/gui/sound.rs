//! The music as a picture: a few bands, read off the samples themselves.
//!
//! Not the tags, not the position in the track — the sound on its way to the
//! card, tapped in [`crate::player::audio`] and turned into something a window
//! can draw. The search field asks it whether anything is playing, and draws
//! its own pictures from the samples — see [`crate::gui::scope`] and
//! [`crate::gui::pen`].
//!
//! Why a spectrum and not a level: a level is one number, and one number moves
//! with the loudness knob as much as with the music. A handful of bands shows
//! what the loudness is *made of* — a bass drum in the first two, a voice in
//! the middle, a cymbal in the last — and that is what somebody watching a
//! picture of music is actually looking for.
//!
//! Everything here is small on purpose. The transform is a radix-2 FFT over
//! 1024 samples, which is some ten thousand multiplications a frame — less
//! than one cover is dithered for, and it happens only while something is
//! playing and something is drawing.

use std::f32::consts::PI;

/// How many samples go into one transform. A power of two, and about a
/// fortieth of a second at CD rate.
const WINDOW: usize = 1024;

/// How many bands come out.
///
/// Sixteen, because that is what fits behind a search field as dots you can
/// count, and because the ear has no use for more of them at this size.
pub const BANDS: usize = 16;

/// How fast a band may fall.
///
/// Rising is instant — a beat has to land on the frame it happened in — and
/// falling takes about a fifth of a second, which is what makes the picture
/// read as one movement rather than as a flicker. The same asymmetry every
/// VU meter has had since they were made of metal.
const FALL: f32 = 0.86;

/// Below this, there is nothing to see.
const HUSH: f32 = 0.004;

/// The bands, as they stand now.
///
/// Kept by the window rather than worked out fresh: the falling is a memory of
/// what the bands were a frame ago.
#[derive(Debug, Clone)]
pub struct Spectrum {
    pub bands: [f32; BANDS],
    /// How loud it is overall, 0 … 1, with the same falling.
    pub level: f32,
}

impl Default for Spectrum {
    fn default() -> Self {
        Self {
            bands: [0.0; BANDS],
            level: 0.0,
        }
    }
}

impl Spectrum {
    /// Read the samples and move the bands towards what they say.
    ///
    /// `samples` is what the tap kept, oldest first. Fewer than a window's
    /// worth — at the very start of a track, or with nothing playing — leaves
    /// the picture falling towards nothing rather than jumping to it.
    pub fn hear(&mut self, samples: &[f32]) {
        let wanted = bands_of(samples);
        for (band, fresh) in self.bands.iter_mut().zip(wanted) {
            *band = if fresh > *band { fresh } else { *band * FALL };
        }
        let loudest = self.bands.iter().copied().fold(0.0_f32, f32::max);
        self.level = if loudest > self.level {
            loudest
        } else {
            self.level * FALL
        };
    }

    /// Whether there is anything to see.
    pub fn silent(&self) -> bool {
        self.level < HUSH
    }
}

/// The band energies of one window of samples, each 0 … 1.
///
/// The bands are laid out by octave rather than evenly: the ear hears pitch
/// that way, and sixteen even slices of nought to twenty kilohertz would put
/// everything anybody can hum in the first two of them.
fn bands_of(samples: &[f32]) -> [f32; BANDS] {
    let mut out = [0.0; BANDS];
    if samples.len() < WINDOW {
        return out;
    }
    let from = samples.len() - WINDOW;

    let mut real: Vec<f32> = Vec::with_capacity(WINDOW);
    let mut imag = vec![0.0; WINDOW];
    for (i, sample) in samples[from..].iter().enumerate() {
        let w = 0.5 - 0.5 * (2.0 * PI * i as f32 / WINDOW as f32).cos();
        real.push(sample * w);
    }
    fft(&mut real, &mut imag);

    let half = WINDOW / 2;
    for (band, slot) in out.iter_mut().enumerate() {
        let (lo, hi) = edges(band, half);
        let mut sum = 0.0;
        for bin in lo..hi {
            sum += (real[bin] * real[bin] + imag[bin] * imag[bin]).sqrt();
        }
        let mean = sum / (hi - lo).max(1) as f32;
        *slot = ((1.0 + mean * 220.0).ln() / 6.0).clamp(0.0, 1.0);
    }
    out
}

/// Which bins belong to a band: an octave each, near enough.
///
/// Apart from `bands_of` so the layout can be checked without a transform —
/// the bands have to cover every bin, in order, without a gap or an overlap,
/// and an off-by-one here is invisible in the picture and wrong in the sound.
fn edges(band: usize, half: usize) -> (usize, usize) {
    let scale = |b: usize| -> usize {
        let t = b as f32 / BANDS as f32;
        let top = half as f32;
        (top.powf(t)).round() as usize
    };
    let lo = scale(band).max(1);
    let hi = scale(band + 1).max(lo + 1).min(half);
    (lo, hi.max(lo + 1))
}

/// An in-place radix-2 Cooley–Tukey transform.
///
/// Written out rather than pulled in: it is thirty lines, it is the one piece
/// of arithmetic in this program that every textbook agrees on, and a
/// dependency for it would be a dependency on the audio path.
fn fft(real: &mut [f32], imag: &mut [f32]) {
    let n = real.len();
    debug_assert!(n.is_power_of_two() && imag.len() == n);

    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            real.swap(i, j);
            imag.swap(i, j);
        }
    }

    let mut len = 2;
    while len <= n {
        let angle = -2.0 * PI / len as f32;
        let (wr, wi) = (angle.cos(), angle.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0_f32, 0.0_f32);
            for k in 0..len / 2 {
                let (i, j) = (start + k, start + k + len / 2);
                let tr = cr * real[j] - ci * imag[j];
                let ti = cr * imag[j] + ci * real[j];
                real[j] = real[i] - tr;
                imag[j] = imag[i] - ti;
                real[i] += tr;
                imag[i] += ti;
                let next = (cr * wr - ci * wi, cr * wi + ci * wr);
                cr = next.0;
                ci = next.1;
            }
        }
        len <<= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sine at a known pitch lands in the band that pitch belongs to, and
    /// in no other.
    ///
    /// The test that makes the picture worth drawing: without it a spectrum
    /// is a row of bars moving plausibly, and plausible is exactly what
    /// nobody can check by looking.
    #[test]
    fn a_tone_lands_in_its_own_band() {
        let rate = 44_100.0;
        for hertz in [110.0_f32, 1_000.0, 6_000.0] {
            let samples: Vec<f32> = (0..WINDOW * 2)
                .map(|i| 0.2 * (2.0 * PI * hertz * i as f32 / rate).sin())
                .collect();
            let bands = bands_of(&samples);

            let loudest = bands
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
                .map(|(at, _)| at)
                .unwrap();

            let bin = (hertz / (rate / WINDOW as f32)).round() as usize;
            let wanted = (0..BANDS)
                .find(|band| {
                    let (lo, hi) = edges(*band, WINDOW / 2);
                    (lo..hi).contains(&bin)
                })
                .expect("every bin belongs to a band");
            assert_eq!(
                loudest, wanted,
                "{hertz} hz landed in band {loudest}, not {wanted}"
            );
        }
    }

    /// Silence draws nothing, and the bands cover every bin exactly once.
    #[test]
    fn the_bands_cover_the_whole_range_in_order() {
        assert_eq!(bands_of(&[0.0; WINDOW * 2]), [0.0; BANDS]);

        let half = WINDOW / 2;
        let mut last = 0;
        for band in 0..BANDS {
            let (lo, hi) = edges(band, half);
            assert!(hi > lo, "band {band} is empty");
            assert!(lo >= last, "band {band} starts before the one before it");
            last = lo;
        }
        assert_eq!(edges(BANDS - 1, half).1, half, "the last band reaches the top");
    }

    /// A beat lands on the frame it happened in, and falls away over a few.
    #[test]
    fn a_beat_lands_at_once_and_fades_slowly() {
        let mut spectrum = Spectrum::default();
        let rate = 44_100.0;
        let loud: Vec<f32> = (0..WINDOW * 2)
            .map(|i| (2.0 * PI * 110.0 * i as f32 / rate).sin())
            .collect();

        spectrum.hear(&loud);
        let struck = spectrum.level;
        assert!(struck > 0.2, "a tone at full scale drew {struck}");

        spectrum.hear(&[0.0; WINDOW * 2]);
        assert!(spectrum.level < struck);
        assert!(
            spectrum.level > struck * 0.5,
            "one silent frame took it from {struck} to {}",
            spectrum.level
        );
        for _ in 2..82 {
            spectrum.hear(&[0.0; WINDOW * 2]);
        }
        assert!(spectrum.silent(), "it never settled: {}", spectrum.level);
    }
}
