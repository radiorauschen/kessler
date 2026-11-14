#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    #[default]
    Ordered,
    Halftone,
    Diffusion,
    Atkinson,
    Noise,
    BlueNoise,
}

impl Screen {
    pub const ALL: [Screen; 6] = [
        Screen::Ordered,
        Screen::Halftone,
        Screen::Diffusion,
        Screen::Atkinson,
        Screen::Noise,
        Screen::BlueNoise,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Screen::Ordered => "geordnet",
            Screen::Halftone => "halbton",
            Screen::Diffusion => "streuung",
            Screen::Atkinson => "atkinson",
            Screen::Noise => "rauschen",
            Screen::BlueNoise => "blaues rauschen",
        }
    }

    pub fn about(self) -> &'static str {
        match self {
            Screen::Ordered => tr!("gleichmässiges gewebe"),
            Screen::Halftone => tr!("wie gedruckt, punkte wachsen"),
            Screen::Diffusion => tr!("ohne muster, fotografisch"),
            Screen::Atkinson => tr!("hart, mac 1984"),
            Screen::Noise => tr!("korn, ohne ordnung"),
            Screen::BlueNoise => tr!("verstreut, ohne klumpen"),
        }
    }

    pub fn from_name(name: &str) -> Screen {
        let name = name.trim().to_lowercase();
        Screen::ALL
            .into_iter()
            .find(|s| s.name() == name)
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Look {
    pub screen: Screen,
    pub tones: u8,
}

impl Look {
    pub fn of(ui: &crate::config::UiConfig) -> Look {
        Look {
            screen: Screen::from_name(&ui.dither_method),
            tones: ui.dither_tones.clamp(2, 4),
        }
    }
}

#[rustfmt::skip]
const BAYER: [[u8; 8]; 8] = [
    [ 0, 32,  8, 40,  2, 34, 10, 42],
    [48, 16, 56, 24, 50, 18, 58, 26],
    [12, 44,  4, 36, 14, 46,  6, 38],
    [60, 28, 52, 20, 62, 30, 54, 22],
    [ 3, 35, 11, 43,  1, 33,  9, 41],
    [51, 19, 59, 27, 49, 17, 57, 25],
    [15, 47,  7, 39, 13, 45,  5, 37],
    [63, 31, 55, 23, 61, 29, 53, 21],
];

#[rustfmt::skip]
const CLUSTER: [[u8; 8]; 8] = [
    [24, 10, 12, 26, 35, 47, 49, 37],
    [ 8,  0,  2, 14, 45, 59, 61, 51],
    [22,  6,  4, 16, 43, 57, 63, 53],
    [30, 20, 18, 28, 33, 41, 55, 39],
    [34, 46, 48, 36, 25, 11, 13, 27],
    [44, 58, 60, 50,  9,  1,  3, 15],
    [42, 56, 62, 52, 23,  7,  5, 17],
    [32, 40, 54, 38, 31, 21, 19, 29],
];

fn spread(cell: u8) -> i16 {
    cell as i16 * 4 + 2
}

const BLUE_SIDE: usize = 64;

fn blue(x: u32, y: u32) -> i16 {
    let tile = blue_tile();
    i16::from(tile[(y as usize % BLUE_SIDE) * BLUE_SIDE + x as usize % BLUE_SIDE])
}

pub fn blue_share(x: usize, y: usize) -> f32 {
    (f32::from(blue(x as u32, y as u32)) + 0.5) / 255.0
}

fn blue_tile() -> &'static [u8] {
    static TILE: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    TILE.get_or_init(|| {
        const N: usize = BLUE_SIDE * BLUE_SIDE;
        let sigma2 = 2.0 * 1.5f32 * 1.5;
        let kernel: Vec<f32> = (0..N)
            .map(|i| {
                let (dx, dy) = (i % BLUE_SIDE, i / BLUE_SIDE);
                let (dx, dy) = (dx.min(BLUE_SIDE - dx) as f32, dy.min(BLUE_SIDE - dy) as f32);
                (-(dx * dx + dy * dy) / sigma2).exp()
            })
            .collect();
        let offset = |a: usize, b: usize| {
            let (ax, ay, bx, by) = (a % BLUE_SIDE, a / BLUE_SIDE, b % BLUE_SIDE, b / BLUE_SIDE);
            ((ay + BLUE_SIDE - by) % BLUE_SIDE) * BLUE_SIDE + (ax + BLUE_SIDE - bx) % BLUE_SIDE
        };
        let add = |energy: &mut [f32], at: usize, sign: f32| {
            for (p, e) in energy.iter_mut().enumerate() {
                *e += sign * kernel[offset(p, at)];
            }
        };
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
        let mut dots = vec![false; N];
        let mut energy = vec![0.0f32; N];
        let mut placed = 0;
        while placed < N / 10 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let at = (seed % N as u64) as usize;
            if !dots[at] {
                dots[at] = true;
                add(&mut energy, at, 1.0);
                placed += 1;
            }
        }
        let tightest = |dots: &[bool], energy: &[f32]| (0..N).filter(|&i| dots[i]).max_by(|&a, &b| energy[a].total_cmp(&energy[b]));
        let emptiest = |dots: &[bool], energy: &[f32]| (0..N).filter(|&i| !dots[i]).min_by(|&a, &b| energy[a].total_cmp(&energy[b]));
        for _ in 0..N {
            let (Some(from), Some(to)) = (tightest(&dots, &energy), emptiest(&dots, &energy)) else { break };
            dots[from] = false;
            add(&mut energy, from, -1.0);
            if to == from {
                dots[from] = true;
                add(&mut energy, from, 1.0);
                break;
            }
            dots[to] = true;
            add(&mut energy, to, 1.0);
        }
        let mut rank = vec![0usize; N];
        {
            let (mut dots, mut energy) = (dots.clone(), energy.clone());
            for r in (0..placed).rev() {
                let Some(at) = tightest(&dots, &energy) else { break };
                dots[at] = false;
                add(&mut energy, at, -1.0);
                rank[at] = r;
            }
        }
        for r in placed..N {
            let Some(at) = emptiest(&dots, &energy) else { break };
            dots[at] = true;
            add(&mut energy, at, 1.0);
            rank[at] = r;
        }
        rank.into_iter().map(|r| (r * 255 / N) as u8).collect()
    })
}

fn scattered(x: u32, y: u32) -> i16 {
    let mut h = x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0xC2B2_AE35);
    h ^= h >> 13;
    (h % 255) as i16
}

pub fn noise(grey: &[u8], width: u32, height: u32, seed: u32) -> Vec<bool> {
    let count = (width as usize).saturating_mul(height as usize);
    if grey.len() < count || count == 0 {
        return vec![false; count];
    }
    let salt = seed.wrapping_mul(0x27D4_EB2F).rotate_left(11) ^ 0x1656_67B1;
    thresholded(grey, width, height, |x, y| scattered(x ^ salt, y.wrapping_add(salt.rotate_left(16))))
}

pub fn noise_levels(grey: &[u8], width: u32, height: u32, seed: u32, tones: u8) -> Vec<u8> {
    let count = (width as usize).saturating_mul(height as usize);
    if grey.len() < count || count == 0 {
        return vec![0; count];
    }
    let salt = seed.wrapping_mul(0x27D4_EB2F).rotate_left(11) ^ 0x1656_67B1;
    levelled(grey, width, height, tones, |x, y| scattered(x ^ salt, y.wrapping_add(salt.rotate_left(16))))
}

pub fn screen_levels(grey: &[u8], width: u32, height: u32, method: Screen, tones: u8) -> Vec<u8> {
    let count = (width as usize).saturating_mul(height as usize);
    if grey.len() < count || count == 0 {
        return vec![0; count];
    }
    match method {
        Screen::Ordered => levelled(grey, width, height, tones, |x, y| {
            spread(BAYER[(y % 8) as usize][(x % 8) as usize])
        }),
        Screen::Halftone => levelled(grey, width, height, tones, |x, y| {
            spread(CLUSTER[(y % 8) as usize][(x % 8) as usize])
        }),
        Screen::Noise => levelled(grey, width, height, tones, scattered),
        Screen::BlueNoise => levelled(grey, width, height, tones, blue),
        Screen::Diffusion => diffused_levels(grey, width, height, tones, FLOYD_STEINBERG, 16),
        Screen::Atkinson => diffused_levels(grey, width, height, tones, ATKINSON, 8),
    }
}

fn levelled(grey: &[u8], width: u32, height: u32, tones: u8, threshold: impl Fn(u32, u32) -> i16) -> Vec<u8> {
    let top = i16::from(tones.clamp(2, 4)) - 1;
    let mut out = Vec::with_capacity((width * height) as usize);
    for y in 0..height {
        for x in 0..width {
            let stretched = grey[(y * width + x) as usize] as i16 * top;
            let step = (stretched / 255).min(top);
            let within = stretched - step * 255;
            let up = step < top && within > threshold(x, y);
            out.push(step as u8 + up as u8);
        }
    }
    out
}

fn diffused_levels(grey: &[u8], width: u32, height: u32, tones: u8, spill: Spill, whole: i32) -> Vec<u8> {
    let top = i32::from(tones.clamp(2, 4)) - 1;
    let mut work: Vec<i32> = grey[..(width * height) as usize]
        .iter()
        .map(|v| *v as i32)
        .collect();
    let mut out = vec![0u8; (width * height) as usize];
    for y in 0..height as i32 {
        for x in 0..width as i32 {
            let at = (y * width as i32 + x) as usize;
            let old = work[at];
            let level = ((old * top + 127) / 255).clamp(0, top);
            out[at] = level as u8;
            let error = old - level * 255 / top;
            for (dx, dy, share) in spill {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                    continue;
                }
                let to = (ny * width as i32 + nx) as usize;
                work[to] += error * share / whole;
            }
        }
    }
    out
}

pub fn screen(grey: &[u8], width: u32, height: u32, method: Screen) -> Vec<bool> {
    let count = (width as usize).saturating_mul(height as usize);
    if grey.len() < count || count == 0 {
        return vec![false; count];
    }

    match method {
        Screen::Ordered => thresholded(grey, width, height, |x, y| {
            spread(BAYER[(y % 8) as usize][(x % 8) as usize])
        }),
        Screen::Halftone => thresholded(grey, width, height, |x, y| {
            spread(CLUSTER[(y % 8) as usize][(x % 8) as usize])
        }),
        Screen::Noise => thresholded(grey, width, height, scattered),
        Screen::BlueNoise => thresholded(grey, width, height, blue),
        Screen::Diffusion => diffused(grey, width, height, FLOYD_STEINBERG, 16),
        Screen::Atkinson => diffused(grey, width, height, ATKINSON, 8),
    }
}

fn thresholded(
    grey: &[u8],
    width: u32,
    height: u32,
    threshold: impl Fn(u32, u32) -> i16,
) -> Vec<bool> {
    let mut out = Vec::with_capacity((width * height) as usize);
    for y in 0..height {
        for x in 0..width {
            let value = grey[(y * width + x) as usize] as i16;
            out.push(value > threshold(x, y));
        }
    }
    out
}

type Spill = &'static [(i32, i32, i32)];

const FLOYD_STEINBERG: Spill = &[(1, 0, 7), (-1, 1, 3), (0, 1, 5), (1, 1, 1)];

const ATKINSON: Spill = &[
    (1, 0, 1),
    (2, 0, 1),
    (-1, 1, 1),
    (0, 1, 1),
    (1, 1, 1),
    (0, 2, 1),
];

fn diffused(grey: &[u8], width: u32, height: u32, spill: Spill, whole: i32) -> Vec<bool> {
    let mut work: Vec<i32> = grey[..(width * height) as usize]
        .iter()
        .map(|v| *v as i32)
        .collect();
    let mut out = vec![false; (width * height) as usize];

    for y in 0..height as i32 {
        for x in 0..width as i32 {
            let at = (y * width as i32 + x) as usize;
            let old = work[at];
            let ink = old > 127;
            out[at] = ink;
            let error = old - if ink { 255 } else { 0 };

            for (dx, dy, share) in spill {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                    continue;
                }
                let to = (ny * width as i32 + nx) as usize;
                work[to] += error * share / whole;
            }
        }
    }

    out
}

pub fn grain_for(tile: f32, pixels_per_point: f32, fineness: f32) -> u32 {
    let scale = 2f32.powf((fineness.clamp(0.0, 1.0) - 0.5) * 5.0);
    let finest = (tile * pixels_per_point.max(1.0)).clamp(GRAIN_MIN, GRAIN_MAX);
    let wanted = (tile * scale).clamp(GRAIN_MIN, finest);
    let step = match wanted {
        w if w < 64.0 => 2.0,
        w if w < 128.0 => 4.0,
        w if w < 320.0 => 8.0,
        _ => 16.0,
    };
    ((wanted / step).round() * step).clamp(GRAIN_MIN, finest) as u32
}

const GRAIN_MIN: f32 = 24.0;
const GRAIN_MAX: f32 = 768.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_tones_keep_the_tone() {
        let field = |v: u8| vec![v; 64 * 64];
        for method in Screen::ALL {
            let share = |v: u8, level: u8| {
                let out = screen_levels(&field(v), 64, 64, method, 3);
                out.iter().filter(|l| **l == level).count() as f32 / out.len() as f32
            };
            assert_eq!(share(0, 0), 1.0, "{method:?}");
            assert_eq!(share(255, 2), 1.0, "{method:?}");
            assert!(share(128, 1) > 0.9, "{method:?}");
            let light = share(192, 1) + share(192, 2);
            assert!((light - 1.0).abs() < 0.02, "{method:?}");
            assert!((share(192, 2) - 0.5).abs() < 0.1, "{method:?}");
            assert!((share(64, 1) - 0.5).abs() < 0.1, "{method:?}");
        }
        let a = noise_levels(&field(64), 64, 64, 1, 3);
        assert_ne!(a, noise_levels(&field(64), 64, 64, 2, 3));
        assert!(a.iter().all(|l| *l < 2));
    }

    #[test]
    fn four_tones_keep_the_tone() {
        let field = |v: u8| vec![v; 64 * 64];
        for method in Screen::ALL {
            let share = |v: u8, level: u8| {
                let out = screen_levels(&field(v), 64, 64, method, 4);
                out.iter().filter(|l| **l == level).count() as f32 / out.len() as f32
            };
            assert_eq!(share(0, 0), 1.0, "{method:?}");
            assert_eq!(share(255, 3), 1.0, "{method:?}");
            assert!(share(85, 1) > 0.9, "{method:?}");
            assert!(share(170, 2) > 0.9, "{method:?}");
        }
    }

    #[test]
    fn every_frame_has_its_own_grain() {
        let grey = vec![128u8; 64 * 64];
        let a = noise(&grey, 64, 64, 1);
        let b = noise(&grey, 64, 64, 2);
        assert_eq!(a, noise(&grey, 64, 64, 1));
        let same = a.iter().zip(&b).filter(|(x, y)| x == y).count();
        assert!(same < 64 * 64 * 3 / 4, "{same}");
        let ink = |d: &[bool]| d.iter().filter(|i| **i).count() as f32 / d.len() as f32;
        assert!((ink(&a) - 0.5).abs() < 0.05 && (ink(&b) - 0.5).abs() < 0.05);
    }

    fn flat(value: u8, n: u32) -> Vec<u8> {
        vec![value; (n * n) as usize]
    }

    fn set(dots: &[bool]) -> f32 {
        dots.iter().filter(|d| **d).count() as f32 / dots.len() as f32
    }

    #[test]
    fn every_method_keeps_the_tone() {
        for method in Screen::ALL {
            let dots = screen(&flat(128, 64), 64, 64, method);
            let share = set(&dots);
            assert!(
                (share - 0.5).abs() < 0.12,
                "{}: mid grey came out {:.0}% ink",
                method.name(),
                share * 100.0
            );
        }
    }

    #[test]
    fn black_is_empty_and_white_is_full() {
        for method in Screen::ALL {
            let black = screen(&flat(0, 32), 32, 32, method);
            assert_eq!(set(&black), 0.0, "{} made ink out of black", method.name());

            let white = screen(&flat(255, 32), 32, 32, method);
            assert_eq!(set(&white), 1.0, "{} left white empty", method.name());
        }
    }

    #[test]
    fn the_same_picture_always_gives_the_same_dots() {
        let picture: Vec<u8> = (0..64 * 64).map(|i| ((i * 7) % 256) as u8).collect();
        for method in Screen::ALL {
            let once = screen(&picture, 64, 64, method);
            let again = screen(&picture, 64, 64, method);
            assert_eq!(once, again, "{} is not repeatable", method.name());
        }
    }

    #[test]
    fn every_method_keeps_the_tone_of_a_whole_picture() {
        let width = 96u32;
        let picture: Vec<u8> = (0..width * width)
            .map(|i| {
                let (x, y) = ((i % width) as f32, (i / width) as f32);
                let ramp = (x + y) / (2.0 * width as f32);
                let blob = (-(((x - 30.0).powi(2) + (y - 60.0).powi(2)) / 400.0)).exp();
                ((ramp * 0.8 + blob * 0.5).clamp(0.0, 1.0) * 255.0) as u8
            })
            .collect();
        let brightness =
            picture.iter().map(|v| *v as f32).sum::<f32>() / (255.0 * picture.len() as f32);

        for method in Screen::ALL {
            let dots = screen(&picture, width, width, method);
            let ink = set(&dots);
            assert!(
                (ink - brightness).abs() < 0.04,
                "{}: {:.1}% dots for a picture {:.1}% bright",
                method.name(),
                ink * 100.0,
                brightness * 100.0
            );
        }
    }

    #[test]
    fn light_and_dark_stay_where_they_were() {
        let width = 64u32;
        let ramp: Vec<u8> = (0..width * width)
            .map(|i| ((i % width as usize as u32) * 4) as u8)
            .collect();

        for method in Screen::ALL {
            let dots = screen(&ramp, width, width, method);
            let left: usize = (0..width)
                .map(|y| dots[(y * width) as usize..(y * width + 16) as usize].iter().filter(|d| **d).count())
                .sum();
            let right: usize = (0..width)
                .map(|y| {
                    dots[(y * width + width - 16) as usize..(y * width + width) as usize]
                        .iter()
                        .filter(|d| **d)
                        .count()
                })
                .sum();
            assert!(
                right > left * 2,
                "{}: the bright end is not brighter ({left} vs {right})",
                method.name()
            );
        }
    }

    #[test]
    fn the_ordered_ones_repeat_and_the_others_do_not() {
        let grey = flat(128, 64);
        for method in [Screen::Ordered, Screen::Halftone] {
            let dots = screen(&grey, 64, 64, method);
            let first = &dots[0..8];
            let eight_along = &dots[8..16];
            assert_eq!(first, eight_along, "{} should repeat every 8", method.name());
        }
    }

    #[test]
    fn a_picture_of_nothing_is_not_a_crash() {
        assert!(screen(&[], 0, 0, Screen::Ordered).is_empty());
        assert_eq!(screen(&[1, 2, 3], 8, 8, Screen::Diffusion).len(), 64);
    }

    #[test]
    fn names_round_trip_and_rubbish_falls_back() {
        for method in Screen::ALL {
            assert_eq!(Screen::from_name(method.name()), method);
            assert_eq!(Screen::from_name(&method.name().to_uppercase()), method);
        }
        assert_eq!(Screen::from_name("was auch immer"), Screen::Ordered);
        assert_eq!(Screen::from_name(""), Screen::Ordered);
    }

    #[test]
    fn the_middle_of_the_slider_is_one_dot_per_point() {
        let tile = 192.0;
        let middle = grain_for(tile, 2.0, 0.5);
        assert!(
            (middle as f32 - tile).abs() <= tile / 16.0,
            "middle gave {middle} for a tile of {tile}"
        );
    }

    #[test]
    fn the_slider_runs_from_coarse_to_fine_without_turning_back() {
        let mut last = 0;
        for step in 0..=20 {
            let grain = grain_for(192.0, 2.0, step as f32 / 20.0);
            assert!(grain >= last, "{grain} came after {last}");
            last = grain;
        }
        assert!(grain_for(192.0, 2.0, 0.0) < 64, "the coarse end is not coarse");
        assert!(
            grain_for(192.0, 2.0, 1.0) >= 384,
            "on a retina screen the fine end is one dot per pixel"
        );
    }

    #[test]
    fn nothing_finer_than_the_screen_can_show() {
        assert_eq!(grain_for(200.0, 1.0, 1.0), 200);
        assert_eq!(grain_for(200.0, 1.0, 0.8), 200);
        assert_eq!(grain_for(200.0, 2.0, 1.0), 400);
        assert_eq!(grain_for(200.0, 1.0, 0.0), grain_for(200.0, 2.0, 0.0));
    }

    #[test]
    fn the_slider_never_asks_for_the_impossible() {
        for fineness in [0.0, 0.25, 0.5, 0.75, 1.0] {
            for tile in [40.0, 120.0, 400.0, 4000.0] {
                let grain = grain_for(tile, 2.0, fineness);
                assert!(
                    (GRAIN_MIN as u32..=GRAIN_MAX as u32).contains(&grain),
                    "tile {tile} at {fineness} gave {grain}"
                );
            }
        }
    }

    #[test]
    fn blue_noise_is_even_and_spread() {
        let tile = super::blue_tile();
        let mut counts = [0usize; 256];
        for &t in tile {
            counts[usize::from(t)] += 1;
        }
        assert!(counts[..255].iter().all(|&c| (15..=17).contains(&c)), "{counts:?}");
        let side = super::BLUE_SIDE;
        let lit = |x: usize, y: usize| tile[(y % side) * side + x % side] < 26;
        let touching = (0..side * side)
            .filter(|&i| {
                let (x, y) = (i % side, i / side);
                lit(x, y) && (lit(x + 1, y) || lit(x, y + 1))
            })
            .count();
        let dots = (0..side * side).filter(|&i| lit(i % side, i / side)).count();
        assert!(touching * 20 < dots, "{touching} of {dots} dots touch another");
    }
}
