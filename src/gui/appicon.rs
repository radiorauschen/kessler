use eframe::egui::{self, Color32};

use super::glyphs::Mask;

pub const SIDE: usize = 512;

const LETTER: f32 = if cfg!(target_os = "macos") { 0.56 * 824.0 / 1024.0 } else { 0.56 };

const BY_WEIGHT: f32 = 0.5;

const LOOKS: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Mac,
    Gnome,
    Kde,
}

impl Shape {
    pub fn here() -> Shape {
        if !cfg!(target_os = "linux") {
            return Shape::Mac;
        }
        Self::for_desktop(&std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default())
    }

    pub fn for_desktop(desktops: &str) -> Shape {
        let said = desktops.to_ascii_uppercase();
        let names: Vec<&str> = said.split(':').collect();
        if names.contains(&"KDE") {
            Shape::Kde
        } else if names.iter().any(|n| matches!(*n, "GNOME" | "GNOME-CLASSIC" | "UNITY" | "BUDGIE")) {
            Shape::Gnome
        } else {
            Shape::Mac
        }
    }

    fn outline(self) -> (Rect, Corner, f32) {
        match self {
            Shape::Mac => {
                let inset = 100.0 / 1024.0;
                let face = Rect { left: inset, top: inset, right: 1.0 - inset, bottom: 1.0 - inset };
                (face, Corner::Super { reach: 0.30 * (1.0 - 2.0 * inset), power: 3.5 }, face.bottom)
            }
            Shape::Gnome => {
                let u = 1.0 / 128.0;
                let face = Rect { left: 8.0 * u, top: 8.0 * u, right: 120.0 * u, bottom: 112.0 * u };
                (face, Corner::Round(12.0 * u), 118.0 * u)
            }
            Shape::Kde => {
                let u = 1.0 / 48.0;
                let face = Rect { left: 3.0 * u, top: 3.0 * u, right: 45.0 * u, bottom: 45.0 * u };
                (face, Corner::Round(2.5 * u), face.bottom)
            }
        }
    }

    fn middle(self) -> (f32, f32) {
        let (face, _, _) = self.outline();
        ((face.left + face.right) / 2.0, (face.top + face.bottom) / 2.0)
    }
}

#[derive(Debug, Clone, Copy)]
struct Rect {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

#[derive(Debug, Clone, Copy)]
enum Corner {
    Super { reach: f32, power: f32 },
    Round(f32),
}

fn within(x: f32, y: f32, rect: Rect, corner: Corner) -> bool {
    if x < rect.left || x > rect.right || y < rect.top || y > rect.bottom {
        return false;
    }
    let reach = match corner {
        Corner::Super { reach, .. } | Corner::Round(reach) => reach,
    };
    let dx = (rect.left + reach - x).max(x - (rect.right - reach)).max(0.0);
    let dy = (rect.top + reach - y).max(y - (rect.bottom - reach)).max(0.0);
    let power = match corner {
        Corner::Super { power, .. } => power,
        Corner::Round(_) => 2.0,
    };
    (dx / reach).powf(power) + (dy / reach).powf(power) <= 1.0
}

fn tile_cover(size: usize, shape: Shape) -> Vec<(f32, f32)> {
    let (face, corner, lip) = shape.outline();
    let under = Rect { top: face.top + (lip - face.bottom), bottom: lip, ..face };
    let mut cover = vec![(0.0, 0.0); size * size];
    let looks = (LOOKS * LOOKS) as f32;
    for py in 0..size {
        for px in 0..size {
            let (mut faced, mut lipped) = (0, 0);
            for ly in 0..LOOKS {
                for lx in 0..LOOKS {
                    let x = (px as f32 + (lx as f32 + 0.5) / LOOKS as f32) / size as f32;
                    let y = (py as f32 + (ly as f32 + 0.5) / LOOKS as f32) / size as f32;
                    if within(x, y, face, corner) {
                        faced += 1;
                    } else if lip > face.bottom && within(x, y, under, corner) {
                        lipped += 1;
                    }
                }
            }
            cover[py * size + px] = ((faced + lipped) as f32 / looks, lipped as f32 / looks);
        }
    }
    cover
}

pub fn compose(size: usize, shape: Shape, ground: Color32, ink: Color32, letter: Option<&Mask>) -> Vec<u8> {
    let tile = tile_cover(size, shape);
    let lip_colour = ground.lerp_to_gamma(Color32::BLACK, 0.3);
    let mut out = vec![0u8; size * size * 4];
    let (mx, my) = shape.middle();
    let (left, top) = letter.map_or((0, 0), |m| {
        let middle = m.width as f32 / 2.0;
        let across = middle + (weight_across(m) - middle) * BY_WEIGHT;
        (
            (mx * size as f32 - across).round() as isize,
            (my * size as f32 - m.height as f32 / 2.0).round() as isize,
        )
    });
    for y in 0..size {
        for x in 0..size {
            let at = y * size + x;
            let (cover, lip) = tile[at];
            let base = if cover > 0.0 { ground.lerp_to_gamma(lip_colour, lip / cover) } else { ground };
            let ink_cover = letter
                .and_then(|m| {
                    let (lx, ly) = (x as isize - left, y as isize - top);
                    (lx >= 0 && ly >= 0 && (lx as usize) < m.width && (ly as usize) < m.height)
                        .then(|| m.cover[ly as usize * m.width + lx as usize] as f32 / 255.0)
                })
                .unwrap_or(0.0);
            let colour = base.lerp_to_gamma(ink, ink_cover);
            out[at * 4] = colour.r();
            out[at * 4 + 1] = colour.g();
            out[at * 4 + 2] = colour.b();
            out[at * 4 + 3] = (cover * 255.0).round() as u8;
        }
    }
    if shape == Shape::Mac {
        dress_mac(&mut out, &tile, size, ground);
    }
    out
}

const RIM: f32 = 4.0;

const RIM_STRENGTH: f32 = 0.5;

const LIGHT_GROUND: f32 = 0.6;

const RIM_DARK_TOP: f32 = 0.12;
const RIM_DARK_FOOT: f32 = 0.3;

fn luminance(c: Color32) -> f32 {
    (0.2126 * f32::from(c.r()) + 0.7152 * f32::from(c.g()) + 0.0722 * f32::from(c.b())) / 255.0
}

const SHADOW_DOWN: f32 = 12.0;
const SHADOW_SOFT: f32 = 14.0;
const SHADOW_STRENGTH: f32 = 0.5;

fn dress_mac(out: &mut [u8], tile: &[(f32, f32)], size: usize, ground: Color32) {
    let light_ground = luminance(ground) > LIGHT_GROUND;
    let scale = size as f32 / 1024.0;
    let cover: Vec<f32> = tile.iter().map(|(c, _)| *c).collect();
    let reach = (RIM * scale).round().max(1.0) as isize;
    let inner = min_filter(&cover, size, reach);
    for y in 0..size {
        let down = y as f32 / size as f32;
        let (rim_colour, strength) = if light_ground {
            (Color32::BLACK, RIM_DARK_TOP + (RIM_DARK_FOOT - RIM_DARK_TOP) * down)
        } else {
            (Color32::from_gray((255.0 * (1.0 - down)) as u8), RIM_STRENGTH)
        };
        for x in 0..size {
            let at = y * size + x;
            let rim = (cover[at] - inner[at]).clamp(0.0, 1.0) * strength;
            if rim > 0.0 {
                let now = Color32::from_rgb(out[at * 4], out[at * 4 + 1], out[at * 4 + 2]);
                let dressed = now.lerp_to_gamma(rim_colour, rim);
                out[at * 4] = dressed.r();
                out[at * 4 + 1] = dressed.g();
                out[at * 4 + 2] = dressed.b();
            }
        }
    }
    let down = (SHADOW_DOWN * scale).round() as usize;
    let mut shadow = vec![0.0f32; size * size];
    for y in down..size {
        shadow[y * size..(y + 1) * size].copy_from_slice(&cover[(y - down) * size..(y - down + 1) * size]);
    }
    let radius = (SHADOW_SOFT * scale / 1.7).round().max(1.0) as usize;
    for _ in 0..3 {
        shadow = box_blur(&shadow, size, radius);
    }
    for at in 0..size * size {
        let tile_alpha = cover[at];
        let shade = shadow[at] * SHADOW_STRENGTH * (1.0 - tile_alpha);
        let alpha = tile_alpha + shade;
        if alpha <= 0.0 || shade <= 0.0 {
            continue;
        }
        for c in 0..3 {
            out[at * 4 + c] = (f32::from(out[at * 4 + c]) * tile_alpha / alpha).round() as u8;
        }
        out[at * 4 + 3] = (alpha * 255.0).round().min(255.0) as u8;
    }
}

fn min_filter(values: &[f32], size: usize, reach: isize) -> Vec<f32> {
    let pass = |src: &[f32], across: bool| -> Vec<f32> {
        let mut dst = vec![0.0; src.len()];
        for y in 0..size as isize {
            for x in 0..size as isize {
                let mut least = f32::MAX;
                for d in -reach..=reach {
                    let (sx, sy) = if across { (x + d, y) } else { (x, y + d) };
                    let v = if sx < 0 || sy < 0 || sx >= size as isize || sy >= size as isize { 0.0 } else { src[sy as usize * size + sx as usize] };
                    least = least.min(v);
                }
                dst[y as usize * size + x as usize] = least;
            }
        }
        dst
    };
    pass(&pass(values, true), false)
}

fn box_blur(values: &[f32], size: usize, radius: usize) -> Vec<f32> {
    let width = (2 * radius + 1) as f32;
    let pass = |src: &[f32], across: bool| -> Vec<f32> {
        let mut dst = vec![0.0; src.len()];
        for line in 0..size {
            let get = |i: isize| -> f32 {
                if i < 0 || i >= size as isize {
                    return 0.0;
                }
                let i = i as usize;
                if across { src[line * size + i] } else { src[i * size + line] }
            };
            let mut sum: f32 = (-(radius as isize)..=radius as isize).map(get).sum();
            for i in 0..size {
                let at = if across { line * size + i } else { i * size + line };
                dst[at] = sum / width;
                sum += get(i as isize + radius as isize + 1) - get(i as isize - radius as isize);
            }
        }
        dst
    };
    pass(&pass(values, true), false)
}

pub fn letter(ctx: &egui::Context, size: usize) -> Option<Mask> {
    if crate::lang::current() == crate::lang::Lang::Ja {
        if let Some(data) = japanese_face() {
            if let Some(mask) = sized(size, |pixels| super::glyphs::letter_in(&data, 'ケ', pixels, &[("wght", KE_WEIGHT)])) {
                return Some(mask);
            }
        }
    }
    sized(size, |pixels| super::glyphs::letter(ctx, 'k', pixels))
}

pub fn small_letter(ctx: &egui::Context, ink: usize) -> Option<Mask> {
    letter(ctx, (ink as f32 / LETTER).round() as usize)
}

pub(super) fn japanese_face() -> Option<Vec<u8>> {
    let (regular, bold) = super::theme::bundled_fallback()?;
    std::fs::read(bold.unwrap_or(regular)).ok()
}

pub const ICON_AT: &[(&str, f32)] = &[("wdth", 85.0), ("wght", 700.0)];

pub(super) const KE_WEIGHT: f32 = 700.0;

fn sized(size: usize, draw: impl Fn(f32) -> Option<Mask>) -> Option<Mask> {
    let probe = draw(size as f32)?;
    let ink = (probe.height as f32 - 2.0).max(1.0);
    draw(size as f32 * LETTER * size as f32 / ink)
}

pub fn letter_square(ctx: &egui::Context, size: usize) -> Option<Vec<u8>> {
    let own = match super::theme::weights() {
        Some((_, bold)) if crate::lang::current() != crate::lang::Lang::Ja => {
            let at = [("wdth", super::theme::font_width()), ("wght", bold)];
            sized(size, |pixels| super::glyphs::window_letter(ctx, 'k', pixels, &at))
        }
        _ => None,
    };
    let mask = match own {
        Some(mask) => mask,
        None => letter(ctx, size)?,
    };
    let middle = mask.width as f32 / 2.0;
    let across = middle + (weight_across(&mask) - middle) * BY_WEIGHT;
    let left = (size as f32 / 2.0 - across).round() as isize;
    let top = (size as f32 / 2.0 - mask.height as f32 / 2.0).round() as isize;
    let mut out = vec![0u8; size * size];
    for y in 0..size {
        for x in 0..size {
            let (lx, ly) = (x as isize - left, y as isize - top);
            if lx >= 0 && ly >= 0 && (lx as usize) < mask.width && (ly as usize) < mask.height {
                out[y * size + x] = mask.cover[ly as usize * mask.width + lx as usize];
            }
        }
    }
    Some(out)
}

fn weight_across(mask: &Mask) -> f32 {
    let (mut sum, mut moment) = (0.0f32, 0.0f32);
    for (at, &c) in mask.cover.iter().enumerate() {
        let c = c as f32;
        sum += c;
        moment += c * ((at % mask.width) as f32 + 0.5);
    }
    if sum > 0.0 { moment / sum } else { mask.width as f32 / 2.0 }
}

pub fn premultiplied(rgba: &[u8]) -> Vec<u8> {
    rgba.as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| {
            let a = p[3] as u32;
            let m = |c: u8| ((c as u32 * a + 127) / 255) as u8;
            [m(p[0]), m(p[1]), m(p[2]), p[3]]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tile_is_on_the_grid() {
        let size = 128;
        let cover = tile_cover(size, Shape::Mac);
        let at = |x: usize, y: usize| cover[y * size + x].0;
        assert_eq!(at(64, 64), 1.0);
        assert_eq!(at(2, 64), 0.0, "outside the grid");
        assert_eq!(at(64, 13), 1.0, "the straight side");
        assert_eq!(at(14, 14), 0.0, "the corner is cut");
        assert!(at(20, 20) > 0.5, "but not by much");
    }

    #[test]
    fn each_desktop_has_its_shape() {
        let size = 128;
        let gnome = tile_cover(size, Shape::Gnome);
        assert_eq!(gnome[64 * size + 64], (1.0, 0.0));
        assert_eq!(gnome[115 * size + 64], (1.0, 1.0), "the lip under the face");
        assert_eq!(gnome[122 * size + 64].0, 0.0);
        let kde = tile_cover(size, Shape::Kde);
        assert_eq!(kde[9 * size + 64].0, 1.0, "close to the edge");
        assert_eq!(kde[0].0, 0.0);
        assert_eq!(Shape::for_desktop("ubuntu:GNOME"), Shape::Gnome);
        assert_eq!(Shape::for_desktop("KDE"), Shape::Kde);
        assert_eq!(Shape::for_desktop("LXDE-pi-wayfire"), Shape::Mac);
        assert_eq!(Shape::for_desktop(""), Shape::Mac);
    }

    #[test]
    fn the_letter_stands_in_the_middle() {
        let mask = Mask { width: 4, height: 4, cover: vec![255; 16] };
        let ground = Color32::from_rgb(0, 43, 54);
        let ink = Color32::from_rgb(181, 137, 0);
        let pixels = compose(64, Shape::Mac, ground, ink, Some(&mask));
        let px = |x: usize, y: usize| &pixels[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4];
        assert_eq!(px(31, 31), &[181, 137, 0, 255]);
        assert_eq!(px(20, 32), &[0, 43, 54, 255]);
        assert_eq!(px(0, 0)[3], 0);
        let pre = premultiplied(&[200, 100, 50, 128]);
        assert_eq!(pre, vec![100, 50, 25, 128]);
    }

    #[test]
    fn the_japanese_icon_has_its_ke() {
        let data = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/fonts/M PLUS 1 Code/MPLUS1Code-Variable.ttf"
        ))
        .unwrap();
        let mask = sized(256, |p| super::super::glyphs::letter_in(&data, 'ケ', p, &[("wght", KE_WEIGHT)])).expect("a ケ");
        let ink = mask.cover.iter().filter(|&&c| c > 128).count();
        assert!(ink > 1000, "{ink} inked");
    }

    #[test]
    #[ignore]
    fn icon_pictures() {
        use super::super::theme::Theme;
        let path = std::env::var("KESSLER_ICONS").unwrap_or("/tmp/icons.png".into());
        let font = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/fonts/Asap/Asap-Variable.ttf")).unwrap();
        let bold = font.clone();
        let size = 256;
        let letter = sized(size, |p| super::super::glyphs::letter_in(&font, 'k', p, ICON_AT)).unwrap();
        let heavy = sized(size, |p| super::super::glyphs::letter_in(&bold, 'k', p, &[("wdth", 85.0), ("wght", 900.0)])).unwrap();
        let japanese = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/fonts/M PLUS 1 Code/MPLUS1Code-Variable.ttf"
        ))
        .unwrap();
        let ke = sized(size, |p| super::super::glyphs::letter_in(&japanese, 'ケ', p, &[("wght", KE_WEIGHT)])).unwrap();
        let themes = [
            Theme::SolarisiertDunkel,
            Theme::MacosHell,
            Theme::Hudson,
            Theme::Koen,
            Theme::PcEngine,
            Theme::PaysageDHiver,
            Theme::PaysageDHiverHell,
        ];
        let rows = [
            (Shape::Mac, &letter),
            (Shape::Gnome, &letter),
            (Shape::Kde, &letter),
            (Shape::Mac, &heavy),
            (Shape::Mac, &ke),
        ];
        let mut sheet = image::RgbaImage::from_pixel((size * themes.len()) as u32, (size * rows.len()) as u32, image::Rgba([128, 128, 128, 255]));
        for (row, (shape, letter)) in rows.iter().enumerate() {
            for (col, theme) in themes.iter().enumerate() {
                let p = theme.palette();
                let px = compose(size, *shape, p.ground, p.accent, Some(letter));
                let icon = image::RgbaImage::from_raw(size as u32, size as u32, px).unwrap();
                image::imageops::overlay(&mut sheet, &icon, (col * size) as i64, (row * size) as i64);
            }
        }
        sheet.save(path).unwrap();
    }
}
