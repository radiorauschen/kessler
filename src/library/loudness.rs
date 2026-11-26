//! How loud a track is, so that shuffle does not jump between a quiet
//! record from 1972 and a loud one from 2008.
//!
//! ReplayGain 2.0, which is what foobar2000, Swinsian and the rest write:
//! loudness as EBU R 128 measures it — K-weighted, in 400 ms blocks, gated
//! at −70 LUFS and again 10 LU under the first result — and a gain that
//! brings it to −18 LUFS, held down where the peak would clip.
//!
//! Where a file already carries ReplayGain tags, those are taken as they
//! are: somebody chose them, and reading four tags is a thousand times
//! quicker than decoding the file. Everything else is decoded once and
//! measured; the answer is kept in the database, beside the track, and
//! read from there from then on.

use std::path::Path;

use lofty::prelude::*;
use lofty::probe::Probe;

use crate::error::{KesslerError, Result};

/// The loudness ReplayGain 2.0 brings everything to, in LUFS.
pub const TARGET: f64 = -18.0;

/// What is known about one track's loudness.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Loudness {
    /// Integrated loudness in LUFS — for tags, worked back from the gain.
    pub lufs: f64,
    /// The highest sample, 1.0 being full scale.
    pub peak: f64,
    /// The gain for the track alone, in dB.
    pub track_gain: f64,
    /// The gain for its record, when known.
    pub album_gain: Option<f64>,
    pub album_peak: Option<f64>,
    /// Read from the file's tags rather than measured.
    pub from_tags: bool,
}

/// The gain that brings `lufs` to the target, within what ReplayGain allows.
pub fn gain_for(lufs: f64) -> f64 {
    (TARGET - lufs).clamp(-51.0, 51.0)
}

/// The file's own ReplayGain tags, if it has a track gain.
pub fn from_tags(path: &Path) -> Option<Loudness> {
    let tagged = Probe::open(path).ok()?.read().ok()?;
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag())?;
    let number = |key: &ItemKey| -> Option<f64> {
        let text = tag.get_string(key)?;
        text.split_whitespace().next()?.replace(',', ".").parse::<f64>().ok()
    };
    let track_gain = number(&ItemKey::ReplayGainTrackGain)?;
    let peak = number(&ItemKey::ReplayGainTrackPeak).unwrap_or(1.0);
    Some(Loudness {
        lufs: TARGET - track_gain,
        peak: if peak > 0.0 { peak } else { 1.0 },
        track_gain,
        album_gain: number(&ItemKey::ReplayGainAlbumGain),
        album_peak: number(&ItemKey::ReplayGainAlbumPeak),
        from_tags: true,
    })
}

/// How long the measuring rests after each second of music it has read,
/// while music is playing: it then reads at most some sixty times faster
/// than the music plays, instead of as fast as the disk will go. Reading
/// flat out from the disk the music is playing from, it could keep the
/// sound waiting for its next block — a click.
const PACE: std::time::Duration = std::time::Duration::from_millis(15);

/// Decode the file and measure it — see [`measure`]. `playing`, when
/// given, says whether music is playing, and the reading is paced while
/// it is.
pub fn measure_file(path: &Path, playing: Option<&std::sync::atomic::AtomicBool>) -> Result<Loudness> {
    use rodio::Source;
    let source = crate::player::audio::open(path)
        .map_err(|why| KesslerError::Playback(format!("{}: {why}", path.display())))?;
    let channels = usize::from(source.channels().get());
    let rate = source.sample_rate().get();
    let second = (rate as usize * channels).max(1);
    let source = source.enumerate().map(|(at, sample)| {
        if at > 0 && at % second == 0 {
            if let Some(playing) = playing {
                if playing.load(std::sync::atomic::Ordering::Relaxed) {
                    std::thread::sleep(PACE);
                }
            }
        }
        sample
    });
    let Some((lufs, peak)) = measure(source, channels, rate) else {
        return Ok(Loudness {
            lufs: TARGET,
            peak: 1.0,
            track_gain: 0.0,
            album_gain: None,
            album_peak: None,
            from_tags: false,
        });
    };
    Ok(Loudness {
        lufs,
        peak,
        track_gain: gain_for(lufs),
        album_gain: None,
        album_peak: None,
        from_tags: false,
    })
}

/// A record's gain and peak from its tracks' loudness, peak and length:
/// the tracks' energy, each weighted by how long it plays. (R 128 would gate
/// the blocks of the whole record together; weighted by length the answer
/// is within a few tenths of that, and needs nothing kept but one number a
/// track.)
pub fn album(tracks: &[(f64, f64, f64)]) -> Option<(f64, f64)> {
    let seconds: f64 = tracks.iter().map(|(_, _, s)| s.max(0.0)).sum();
    if tracks.is_empty() || seconds <= 0.0 {
        return None;
    }
    let energy: f64 = tracks
        .iter()
        .map(|(lufs, _, s)| s.max(0.0) * 10f64.powf(lufs / 10.0))
        .sum::<f64>()
        / seconds;
    let peak = tracks.iter().map(|(_, p, _)| *p).fold(0.0, f64::max);
    Some((gain_for(10.0 * energy.log10()), peak))
}

/// The factor a gain comes to on the samples, held down so the peak does
/// not go over full scale.
pub fn factor(gain_db: f64, peak: f64) -> f32 {
    let wanted = 10f64.powf(gain_db / 20.0);
    let ceiling = if peak > 0.0 { 1.0 / peak } else { wanted };
    wanted.min(ceiling.max(0.0)) as f32
}

/// A biquad, run in direct form I.
#[derive(Clone, Copy, Default)]
struct Biquad {
    b: [f64; 3],
    a: [f64; 3],
    x: [f64; 2],
    y: [f64; 2],
}

impl Biquad {
    fn run(&mut self, input: f64) -> f64 {
        let out = self.b[0] * input + self.b[1] * self.x[0] + self.b[2] * self.x[1]
            - self.a[1] * self.y[0]
            - self.a[2] * self.y[1];
        self.x = [input, self.x[0]];
        self.y = [out, self.y[0]];
        out
    }
}

/// The two stages of R 128's K-weighting — a shelf that lifts the top, a
/// high-pass under 40 Hz — for any sample rate, as libebur128 derives them.
fn k_weighting(rate: u32) -> [Biquad; 2] {
    use std::f64::consts::PI;
    let rate = f64::from(rate.max(1));

    let f0 = 1681.974450955533;
    let g = 3.999843853973347;
    let q = 0.7071752369554196;
    let k = (PI * f0 / rate).tan();
    let vh = 10f64.powf(g / 20.0);
    let vb = vh.powf(0.4996667741545416);
    let a0 = 1.0 + k / q + k * k;
    let shelf = Biquad {
        b: [(vh + vb * k / q + k * k) / a0, 2.0 * (k * k - vh) / a0, (vh - vb * k / q + k * k) / a0],
        a: [1.0, 2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
        ..Biquad::default()
    };

    let f0 = 38.13547087602444;
    let q = 0.5003270373238773;
    let k = (PI * f0 / rate).tan();
    let a0 = 1.0 + k / q + k * k;
    let high_pass = Biquad {
        b: [1.0, -2.0, 1.0],
        a: [1.0, 2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
        ..Biquad::default()
    };
    [shelf, high_pass]
}

/// Integrated loudness (LUFS) and sample peak of interleaved samples, or
/// `None` for something shorter than one block or silent throughout.
///
/// The first two channels are measured, each weighted 1 — which is what
/// R 128 does with stereo, and with mono counts the one channel once.
pub fn measure(samples: impl Iterator<Item = f32>, channels: usize, rate: u32) -> Option<(f64, f64)> {
    let channels = channels.max(1);
    let heard = channels.min(2);
    let step = (rate as usize / 10).max(1);
    let mut filters: Vec<[Biquad; 2]> = (0..heard).map(|_| k_weighting(rate)).collect();
    let mut steps: Vec<f64> = Vec::new();
    let mut sum = 0.0;
    let mut in_step = 0usize;
    let mut peak = 0f64;
    for (at, sample) in samples.enumerate() {
        let channel = at % channels;
        let s = f64::from(sample);
        peak = peak.max(s.abs());
        if channel < heard {
            let [shelf, high] = &mut filters[channel];
            let y = high.run(shelf.run(s));
            sum += y * y;
        }
        if channel == channels - 1 {
            in_step += 1;
            if in_step == step {
                steps.push(sum / step as f64);
                sum = 0.0;
                in_step = 0;
            }
        }
    }
    let blocks: Vec<f64> = steps.windows(4).map(|w| w.iter().sum::<f64>() / 4.0).collect();
    let lufs = |z: f64| -0.691 + 10.0 * z.log10();
    let above = |gate: f64| -> Vec<f64> {
        blocks.iter().copied().filter(|z| *z > 0.0 && lufs(*z) > gate).collect()
    };
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    let first = above(-70.0);
    if first.is_empty() {
        return None;
    }
    let relative = lufs(mean(&first)) - 10.0;
    let second = above(relative.max(-70.0));
    if second.is_empty() {
        return None;
    }
    Some((lufs(mean(&second)), peak))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(amplitude: f32, hz: f32, rate: u32, seconds: f32, channels: usize) -> Vec<f32> {
        let frames = (rate as f32 * seconds) as usize;
        let mut out = Vec::with_capacity(frames * channels);
        for i in 0..frames {
            let v = amplitude * (2.0 * std::f32::consts::PI * hz * i as f32 / rate as f32).sin();
            for _ in 0..channels {
                out.push(v);
            }
        }
        out
    }

    /// EBU Tech 3341's first case: a 1 kHz sine at −23 dBFS in both
    /// channels reads −23 LUFS — at 48 kHz and at 44.1 kHz.
    #[test]
    fn a_sine_at_minus_23_reads_minus_23() {
        let amplitude = 10f32.powf(-23.0 / 20.0);
        for rate in [48_000, 44_100] {
            let (lufs, peak) = measure(sine(amplitude, 1000.0, rate, 5.0, 2).into_iter(), 2, rate).unwrap();
            assert!((lufs + 23.0).abs() < 0.1, "{rate}: {lufs}");
            assert!((peak - f64::from(amplitude)).abs() < 1e-3);
        }
    }

    /// Silence is gated away entirely: it has no loudness to speak of.
    #[test]
    fn silence_is_not_measured() {
        assert!(measure(vec![0.0; 96_000].into_iter(), 2, 48_000).is_none());
        let mut both = sine(0.5, 1000.0, 48_000, 3.0, 2);
        let alone = measure(both.clone().into_iter(), 2, 48_000).unwrap().0;
        both.extend(std::iter::repeat_n(0.0, 48_000 * 2 * 3));
        let with_silence = measure(both.into_iter(), 2, 48_000).unwrap().0;
        assert!((alone - with_silence).abs() < 0.3, "{alone} {with_silence}");
    }

    #[test]
    fn a_loud_track_is_turned_down_and_a_quiet_one_up_but_never_to_clip() {
        assert!((gain_for(-9.0) + 9.0).abs() < 1e-9);
        assert!((gain_for(-24.0) - 6.0).abs() < 1e-9);
        assert!((factor(6.0, 0.9) - 1.0 / 0.9).abs() < 1e-6);
        assert!((factor(-6.0, 1.0) - 0.501_187).abs() < 1e-5);
    }

    /// Two tracks of a record: the long loud one counts for more than the
    /// short quiet one, and the record's peak is the higher of the two.
    #[test]
    fn a_record_is_its_tracks_weighted_by_length() {
        let (gain, peak) = album(&[(-10.0, 0.9, 300.0), (-20.0, 0.5, 60.0)]).unwrap();
        assert!(gain < gain_for(-10.0) + 1.0 && gain > gain_for(-10.0), "{gain}");
        assert_eq!(peak, 0.9);
        assert!(album(&[]).is_none());
    }
}
