use std::sync::atomic::Ordering;
use std::sync::Arc;

use eframe::egui;

use super::glyphs::Mask;

const BOX_WIDE: usize = 32;
const BOX_TALL: usize = 32;

const BOX_RADIUS: f32 = 7.0;

const LETTER_INK: usize = 19;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Off,
    Letter,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Self::Off => "aus",
            Self::Letter => "k",
        }
    }

    pub fn from_name(name: &str) -> Self {
        match name.trim() {
            "k" | "skop" | "laufschrift" | "still" | "bewegt" => Self::Letter,
            _ => Self::Off,
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::Letter,
            Self::Letter => Self::Off,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverAt {
    Beside,
    Above,
    Off,
}

impl CoverAt {
    pub fn name(self) -> &'static str {
        match self {
            Self::Beside => "daneben",
            Self::Above => "oben",
            Self::Off => "aus",
        }
    }

    pub fn from_name(name: &str) -> Self {
        match name.trim() {
            "oben" => Self::Above,
            "aus" => Self::Off,
            _ => Self::Beside,
        }
    }

    pub fn said(self) -> &'static str {
        match self {
            Self::Beside => tr!("neben dem titel"),
            Self::Above => tr!("über dem titel"),
            Self::Off => tr!("aus"),
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Beside => Self::Above,
            Self::Above => Self::Off,
            Self::Off => Self::Beside,
        }
    }

    fn number(self) -> i32 {
        match self {
            Self::Off => 0,
            Self::Beside => 1,
            Self::Above => 2,
        }
    }

    fn points(self) -> u32 {
        match self {
            Self::Beside => crate::media::tray_cover_points().unwrap_or(BESIDE_POINTS),
            Self::Above => 184,
            Self::Off => 0,
        }
    }
}

const BESIDE_POINTS: u32 = 53;

#[derive(Debug, Clone, PartialEq)]
pub struct CoverWish {
    pub path: Option<std::path::PathBuf>,
    pub at: CoverAt,
    pub screen: Option<CoverScreen>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CoverScreen {
    pub look: super::dither::Look,
    pub fineness: f32,
    pub ground: egui::Color32,
    pub ink: egui::Color32,
    pub third: egui::Color32,
    pub negative: bool,
}

fn cover_pixels(path: &std::path::Path, points: u32, screen: Option<CoverScreen>) -> Option<(Vec<u8>, usize)> {
    let picture = image::ImageReader::open(path).ok()?.with_guessed_format().ok()?.decode().ok()?;
    let side = (points * 2) as usize;
    let mut rgba = vec![0u8; side * side * 4];
    match screen {
        Some(screen) => {
            let grain = super::dither::grain_for(points as f32, 2.0, screen.fineness);
            let dots = super::covers::screened(&picture, grain, screen.look, screen.negative);
            let [wide, tall] = dots.size;
            let grain = tall.max(1);
            let three = wide == grain * 2;
            for y in 0..side {
                for x in 0..side {
                    let (gx, gy) = (x * grain / side, y * grain / side);
                    let lit = |column: usize| dots.pixels[gy * wide + column].a() > 0;
                    let colour = if three {
                        match (lit(gx), lit(grain + gx)) {
                            (_, true) => screen.ink,
                            (true, false) => screen.third,
                            _ => screen.ground,
                        }
                    } else if lit(gx) {
                        screen.ink
                    } else {
                        screen.ground
                    };
                    let at = (y * side + x) * 4;
                    rgba[at..at + 4].copy_from_slice(&[colour.r(), colour.g(), colour.b(), 255]);
                }
            }
        }
        None => {
            let sized = picture
                .resize_to_fill(side as u32, side as u32, image::imageops::FilterType::CatmullRom)
                .to_rgba8();
            rgba.copy_from_slice(sized.as_raw());
        }
    }
    round_corners(&mut rgba, side, (super::theme::MARKER_RADIUS as f32 * 2.0).max(4.0));
    Some((rgba, side))
}

fn round_corners(rgba: &mut [u8], side: usize, radius: f32) {
    let reach = radius.ceil() as usize;
    for y in 0..reach.min(side) {
        for x in 0..reach.min(side) {
            let (dx, dy) = (radius - x as f32 - 0.5, radius - y as f32 - 0.5);
            let outside = (dx.max(0.0).hypot(dy.max(0.0)) - radius + 0.5).clamp(0.0, 1.0);
            if outside == 0.0 {
                continue;
            }
            for (cx, cy) in [(x, y), (side - 1 - x, y), (x, side - 1 - y), (side - 1 - x, side - 1 - y)] {
                let a = (cy * side + cx) * 4 + 3;
                rgba[a] = (f32::from(rgba[a]) * (1.0 - outside)).round() as u8;
            }
        }
    }
}

#[derive(Clone)]
struct Picture {
    wide: usize,
    tall: usize,
    rgba: Vec<u8>,
}

impl Picture {
    fn show(&self) {
        crate::media::tray_image(&self.rgba, self.wide, self.tall);
    }
}

#[derive(Default)]
pub struct MenuBar {
    shown: Option<(Mode, crate::lang::Lang)>,
    said: Option<(bool, String, u8, bool, String)>,
    inked: Option<bool>,
    covered: Option<CoverWish>,
    cover_round: Arc<std::sync::atomic::AtomicU64>,
}

impl MenuBar {
    pub fn cover(&mut self, wish: CoverWish) {
        if !cfg!(target_os = "macos") || !crate::media::tray_available() || self.covered.as_ref() == Some(&wish) {
            return;
        }
        self.covered = Some(wish.clone());
        let points = wish.at.points();
        let round = self.cover_round.fetch_add(1, Ordering::Relaxed) + 1;
        let rounds = self.cover_round.clone();
        let made = move || {
            let pixels = match (&wish.path, wish.at) {
                (Some(path), at) if at != CoverAt::Off => cover_pixels(path, points, wish.screen),
                _ => None,
            };
            if rounds.load(Ordering::Relaxed) != round {
                return;
            }
            match pixels {
                Some((rgba, side)) => crate::media::tray_cover(&rgba, side, wish.at.number()),
                None => crate::media::tray_cover(&[], 0, 0),
            }
        };
        let _ = std::thread::Builder::new().name("kessler-tray-cover".into()).spawn(made);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn sync(
        &mut self,
        ctx: &egui::Context,
        mode: Mode,
        light: bool,
        playing: bool,
        title: &str,
        about: &str,
        volume: u8,
        on_air: bool,
        sleep: &str,
    ) {
        if !crate::media::tray_available() {
            return;
        }
        if self.inked != Some(light) {
            crate::media::tray_ink(light);
            self.inked = Some(light);
        }
        let now = (mode, crate::lang::current());
        if self.shown != Some(now) {
            crate::media::tray_show(mode != Mode::Off);
            if mode != Mode::Off {
                match super::appicon::small_letter(ctx, LETTER_INK) {
                    Some(letter) => boxed(&letter, BOX_WIDE),
                    None => cut_out(&[], 0, 0, false, BOX_WIDE),
                }
                .show();
                self.said = None;
            }
            self.shown = Some(now);
        }
        if mode == Mode::Off {
            return;
        }
        let now = (playing, about.to_string(), volume, on_air, sleep.to_string());
        if self.said.as_ref() != Some(&now) {
            crate::media::tray_state(
                playing,
                title,
                about,
                volume,
                &crate::media::TrayWords {
                    play: super::library::PLAY_WORDS[0],
                    pause: super::library::PLAY_WORDS[1],
                    next: if on_air { "" } else { tr!("nächster titel") },
                    previous: if on_air { "" } else { tr!("voriger titel") },
                    show: tr!("kessler zeigen"),
                    quit: tr!("kessler beenden"),
                    volume: tr!("vol"),
                    louder: tr!("lauter"),
                    quieter: tr!("leiser"),
                    sleep,
                },
            );
            self.said = Some(now);
        }
    }
}

fn boxed(letter: &Mask, box_wide: usize) -> Picture {
    cut_out(&letter.cover, letter.width, letter.height, true, box_wide)
}

fn cut_out(cover: &[u8], wide: usize, tall: usize, by_weight: bool, box_wide: usize) -> Picture {
    let across = if by_weight { (weight_across(cover, wide) + wide as f32 / 2.0) / 2.0 } else { wide as f32 / 2.0 };
    let left = (box_wide as f32 / 2.0 - across).round() as isize;
    let top = ((BOX_TALL as f32 - tall as f32) / 2.0).round() as isize;
    let mut rgba = vec![0u8; box_wide * BOX_TALL * 4];
    for y in 0..BOX_TALL {
        for x in 0..box_wide {
            let (cx, cy) = (x as isize - left, y as isize - top);
            let cut = if cx >= 0 && cy >= 0 && (cx as usize) < wide && (cy as usize) < tall {
                cover[cy as usize * wide + cx as usize] as f32 / 255.0
            } else {
                0.0
            };
            let alpha = box_cover(x, y, box_wide) * (1.0 - cut);
            rgba[(y * box_wide + x) * 4 + 3] = (alpha * 255.0).round() as u8;
        }
    }
    Picture { wide: box_wide, tall: BOX_TALL, rgba }
}

fn box_cover(x: usize, y: usize, wide: usize) -> f32 {
    const LOOKS: usize = 4;
    let (left, top, right, bottom) = (0.0, 0.0, wide as f32, BOX_TALL as f32);
    let r = BOX_RADIUS;
    let mut inside = 0;
    for i in 0..LOOKS {
        for j in 0..LOOKS {
            let px = x as f32 + (i as f32 + 0.5) / LOOKS as f32;
            let py = y as f32 + (j as f32 + 0.5) / LOOKS as f32;
            if px < left || px > right || py < top || py > bottom {
                continue;
            }
            let dx = (left + r - px).max(px - (right - r)).max(0.0);
            let dy = (top + r - py).max(py - (bottom - r)).max(0.0);
            if dx * dx + dy * dy <= r * r {
                inside += 1;
            }
        }
    }
    inside as f32 / (LOOKS * LOOKS) as f32
}

fn weight_across(cover: &[u8], wide: usize) -> f32 {
    let (mut sum, mut moment) = (0.0f32, 0.0f32);
    for (at, &c) in cover.iter().enumerate() {
        sum += c as f32;
        moment += c as f32 * ((at % wide) as f32 + 0.5);
    }
    if sum > 0.0 { moment / sum } else { wide as f32 / 2.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_cover_is_screened_in_the_window_colours() {
        let dir = std::env::temp_dir().join(format!("kessler-tray-cover-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cover");
        let picture = image::RgbImage::from_fn(300, 300, |x, y| image::Rgb([(x % 256) as u8, (y % 256) as u8, 128]));
        picture.save_with_format(&path, image::ImageFormat::Png).unwrap();
        let (ground, ink, third) =
            (egui::Color32::from_rgb(10, 20, 30), egui::Color32::from_rgb(200, 210, 220), egui::Color32::from_rgb(250, 0, 0));
        let screen = CoverScreen {
            look: crate::gui::dither::Look { screen: crate::gui::dither::Screen::Ordered, tones: 3 },
            fineness: 0.5,
            ground,
            ink,
            third,
            negative: false,
        };
        let (rgba, side) = cover_pixels(&path, 44, Some(screen)).unwrap();
        assert_eq!(side, 88);
        assert_eq!(rgba.len(), side * side * 4);
        assert_eq!(rgba[3], 0, "the corner is cut away");
        let mut seen = std::collections::HashSet::new();
        for pixel in rgba.chunks(4).filter(|p| p[3] == 255) {
            seen.insert([pixel[0], pixel[1], pixel[2]]);
        }
        let allowed = [ground, ink, third].map(|c| [c.r(), c.g(), c.b()]);
        assert!(seen.iter().all(|c| allowed.contains(c)), "{seen:?}");
        assert_eq!(seen.len(), 3);
        let (plain, side) = cover_pixels(&path, 184, None).unwrap();
        assert_eq!(side, 368);
        assert_eq!(plain[(side * side / 2 + side / 2) * 4 + 3], 255);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_two_steps_go_round_and_come_back_by_name() {
        let mut mode = Mode::Off;
        for _ in 0..2 {
            assert_eq!(Mode::from_name(mode.name()), mode);
            mode = mode.next();
        }
        assert_eq!(mode, Mode::Off);
        assert_eq!(Mode::from_name(""), Mode::Off);
        for gone in ["skop", "laufschrift", "still", "bewegt"] {
            assert_eq!(Mode::from_name(gone), Mode::Letter);
        }
    }

    #[test]
    fn the_box_has_its_corners_and_its_hole() {
        assert_eq!(box_cover(BOX_WIDE / 2, BOX_TALL / 2, BOX_WIDE), 1.0);
        assert_eq!(box_cover(0, 0, BOX_WIDE), 0.0);
        assert_eq!(box_cover(0, 1, BOX_WIDE), 0.0);
        assert_eq!(box_cover(BOX_WIDE / 2, 0, BOX_WIDE), 1.0);
        assert_eq!(box_cover(BOX_WIDE / 2, BOX_TALL - 1, BOX_WIDE), 1.0);
        let hole = vec![255u8; 4 * 4];
        let picture = cut_out(&hole, 4, 4, false, BOX_WIDE);
        assert_eq!(picture.rgba.len(), BOX_WIDE * BOX_TALL * 4);
        let alpha = |x: usize, y: usize| picture.rgba[(y * BOX_WIDE + x) * 4 + 3];
        assert_eq!(alpha(BOX_WIDE / 2, BOX_TALL / 2), 0);
        assert_eq!(alpha(BOX_WIDE / 2, 4), 255);
    }
}
