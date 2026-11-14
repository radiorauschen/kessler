use std::f32::consts::PI;

const WINDOW: usize = 1024;

pub const BANDS: usize = 16;

const FALL: f32 = 0.86;

const HUSH: f32 = 0.004;

#[derive(Debug, Clone)]
pub struct Spectrum {
    pub bands: [f32; BANDS],
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

    pub fn silent(&self) -> bool {
        self.level < HUSH
    }
}

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
