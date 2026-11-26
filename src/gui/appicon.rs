//! The application's icon, drawn while it runs: a tile and a k.
//!
//! The icon the system shows before kessler starts — in the Finder, the
//! Launchpad, a start menu — is fixed, black and white, and drawn by
//! `bundle/make-icon.py` in Asap. Once the
//! window is up it dresses the icon in what the window is dressed in: the k in
//! the typeface the window is set in, in the scheme's accent on its ground.
//!
//! The tile is the same shape the script draws — on a Mac straight sides
//! and superelliptic corners on Apple's grid, on GNOME and KDE their own
//! desktops' shapes, see [`Shape`] — so the icon changes colour when the
//! window opens and does not change shape.

use eframe::egui::{self, Color32};

use super::glyphs::Mask;

/// How big the icon is drawn, in pixels a side.
pub const SIDE: usize = 512;

/// How tall the k stands, as a share of the canvas, measured on its ink —
/// whatever the face — so it is as tall as the closed app's.
///
/// The shipped k is 0.545 of its canvas (`LETTER` in `bundle/make-icon.py`).
/// On Linux the desktop shows that picture whole, as it shows this one, so
/// 0.545 it is here too. On a Mac the closed app's icon is the layered one
/// (`kessler.icon`), and macOS 26 lays that layer over the tile itself
/// rather than over the canvas round it: its k comes out 0.545 of the
/// *tile*, which here is 824 of 1024. So 0.545 of the tile.
const LETTER: f32 = if cfg!(target_os = "macos") { 0.545 * 824.0 / 1024.0 } else { 0.545 };

/// How far the k is set by its weight rather than by its outline, across and
/// down: a k centred on its outline has its stem — most of its ink — left of
/// the middle and its bowl and legs low, and looks set too far left and too
/// low. Halfway between the two looks centred.
const BY_WEIGHT: f32 = 0.5;

/// How many looks each pixel of the tile's edge is given, each way.
const LOOKS: usize = 4;

/// The shape of the tile, after the desktop it is shown on. Everything in
/// canvas units, 0 … 1 across and down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// Apple's grid: 824 of 1024, straight sides and superelliptic corners
    /// — the numbers `make-icon.py` draws with. Also for every desktop that
    /// has no shape of its own.
    Mac,
    /// Adwaita's: a rounded square on GNOME's 128-unit grid, a little
    /// taller than wide, standing on an edge of itself a shade darker —
    /// the lip every GNOME application icon has under its face.
    Gnome,
    /// Breeze's: nearly the whole 48-unit grid, square, the corners only
    /// just turned.
    Kde,
}

impl Shape {
    /// The shape for the desktop this is running on.
    pub fn here() -> Shape {
        if !cfg!(target_os = "linux") {
            return Shape::Mac;
        }
        Self::for_desktop(&std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default())
    }

    /// From what `XDG_CURRENT_DESKTOP` says — a list, by colons.
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

    /// The tile's face, left, top, right, bottom, and how it is cut at the
    /// corners; and where, under the face, the lip ends.
    fn outline(self) -> (Rect, Corner, f32) {
        match self {
            Shape::Mac => {
                let inset = 100.0 / 1024.0;
                let face = Rect { left: inset, top: inset, right: 1.0 - inset, bottom: 1.0 - inset };
                (face, Corner::Super { reach: 0.352 * (1.0 - 2.0 * inset), power: 2.9 }, face.bottom)
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

    /// Where the letter stands: in the middle of the face.
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
    /// A quarter of a superellipse reaching this far along each side.
    Super { reach: f32, power: f32 },
    /// A quarter circle of this radius.
    Round(f32),
}

/// Whether a point, in canvas units, is inside `rect` cut at its corners.
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

/// How much of each pixel of a `size` canvas the tile covers, 0 … 1, and
/// how much of that is its lip.
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

/// The icon at `size` pixels in `shape`: the tile in `ground`, its lip a
/// shade darker, and `letter` — a mask already drawn at the size it is to
/// stand — in the middle of the face in `ink`. Four bytes a pixel, not
/// premultiplied.
pub fn compose(size: usize, shape: Shape, ground: Color32, ink: Color32, letter: Option<&Mask>) -> Vec<u8> {
    let tile = tile_cover(size, shape);
    let lip_colour = ground.lerp_to_gamma(Color32::BLACK, 0.3);
    let mut out = vec![0u8; size * size * 4];
    let (mx, my) = shape.middle();
    let (left, top) = letter.map_or((0, 0), |m| {
        let middle = m.width as f32 / 2.0;
        let across = middle + (weight_across(m) - middle) * BY_WEIGHT;
        let half = m.height as f32 / 2.0;
        let down = half + (weight_down(m) - half) * BY_WEIGHT;
        (
            (mx * size as f32 - across).round() as isize,
            (my * size as f32 - down).round() as isize,
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
        dress_mac(&mut out, &tile, size);
    }
    out
}

/// The tile's edge as the edge of a thick piece of glass, in canvas units of
/// 1024: where it faces the light — the upper left — and where it faces
/// away; each band is the tile less the tile blurred by that much, so it is
/// as wide as its number reaches in from the outline.
///
/// The light is caught twice, strongly on the edge facing it and more weakly
/// on the edge opposite, where it comes out of the glass again; a dark line
/// a little further in is the thickness the light bends through; and a broad
/// soft swell, lit on the same two sides and shaded on the others, is the
/// glass curving down to its edge. The same numbers `make-icon.py` uses.
const GLASS_LIGHT: (f32, f32) = (-0.55, -0.85);
/// How sharply the light falls off as the edge turns from it.
const GLASS_FACING: f32 = 1.6;
/// The catch-light: its band, and its strength facing the light and opposite.
const GLASS_CATCH: (f32, f32, f32) = (3.0, 0.95, 0.60);
/// Its softer spread, the same way round.
const GLASS_SPREAD: (f32, f32, f32) = (10.0, 0.35, 0.15);
/// A faint glow all round.
const GLASS_GLOW: (f32, f32) = (20.0, 0.06);
/// The dark line of the glass's thickness: between these two bands, this
/// strong.
const GLASS_THICK: (f32, f32, f32) = (6.0, 14.0, 0.40);
/// The shade between the catch-light and the thickness where the edge faces
/// neither way.
const GLASS_BETWEEN: (f32, f32, f32) = (4.0, 10.0, 0.12);
/// The swell: between these two bands; lit facing the light and opposite,
/// shaded elsewhere.
const GLASS_SWELL: (f32, f32) = (16.0, 44.0);
const GLASS_SWELL_LIT: (f32, f32) = (0.30, 0.18);
const GLASS_SWELL_SHADE: f32 = 0.12;

/// The shadow under the tile, in canvas units of 1024: how far down, how
/// soft, and how dark — as light as the system's own apps cast in the Dock.
const SHADOW_DOWN: f32 = 12.0;
const SHADOW_SOFT: f32 = 14.0;
const SHADOW_STRENGTH: f32 = 0.33;

/// The Mac's tile dressed as the Dock's other icons are: its edge a thick
/// piece of glass — see [`GLASS_LIGHT`] — and a soft shadow under it. The
/// ground inside the edge stays one colour and the letter stays flat.
fn dress_mac(out: &mut [u8], tile: &[(f32, f32)], size: usize) {
    let scale = size as f32 / 1024.0;
    let cover: Vec<f32> = tile.iter().map(|(c, _)| *c).collect();
    let blurred = |sigma: f32| -> Vec<f32> {
        let radius = ((sigma * scale).powi(2) + 0.25).sqrt() - 0.5;
        let radius = radius.round().max(1.0) as usize;
        let mut soft = cover.clone();
        for _ in 0..3 {
            soft = box_blur(&soft, size, radius);
        }
        soft
    };
    let band = |sigma: f32| -> Vec<f32> { cover.iter().zip(blurred(sigma)).map(|(c, s)| (c - s).clamp(0.0, 1.0)).collect() };
    let between = |a: &[f32], b: &[f32], at: usize| (b[at] - a[at]).clamp(0.0, 1.0);
    let soft = blurred(4.0);
    let light = {
        let l = GLASS_LIGHT.0.hypot(GLASS_LIGHT.1);
        (GLASS_LIGHT.0 / l, GLASS_LIGHT.1 / l)
    };
    let catch = band(GLASS_CATCH.0);
    let spread = band(GLASS_SPREAD.0);
    let glow = band(GLASS_GLOW.0);
    let (thin, thick) = (band(GLASS_THICK.0), band(GLASS_THICK.1));
    let (inside, outside) = (band(GLASS_BETWEEN.0), band(GLASS_BETWEEN.1));
    let (swell_in, swell_out) = (band(GLASS_SWELL.0), band(GLASS_SWELL.1));
    for y in 0..size {
        for x in 0..size {
            let at = y * size + x;
            let value = |dx: isize, dy: isize| {
                let (sx, sy) = ((x as isize + dx).clamp(0, size as isize - 1), (y as isize + dy).clamp(0, size as isize - 1));
                soft[sy as usize * size + sx as usize]
            };
            let (gx, gy) = ((value(1, 0) - value(-1, 0)) / 2.0, (value(0, 1) - value(0, -1)) / 2.0);
            let slope = gx.hypot(gy) + 1e-6;
            let (nx, ny) = (-gx / slope, -gy / slope);
            let towards = (nx * light.0 + ny * light.1).clamp(0.0, 1.0).powf(GLASS_FACING);
            let away = (-nx * light.0 - ny * light.1).clamp(0.0, 1.0).powf(GLASS_FACING);
            let swell = between(&swell_in, &swell_out, at);
            let lift = catch[at] * (GLASS_CATCH.1 * towards + GLASS_CATCH.2 * away)
                + spread[at] * (GLASS_SPREAD.1 * towards + GLASS_SPREAD.2 * away)
                + GLASS_GLOW.1 * glow[at]
                + swell * (GLASS_SWELL_LIT.0 * towards + GLASS_SWELL_LIT.1 * away);
            let aside = (1.0 - towards) * (1.0 - away);
            let drop = GLASS_BETWEEN.2 * between(&inside, &outside, at) * aside
                + GLASS_THICK.2 * between(&thin, &thick, at)
                + GLASS_SWELL_SHADE * swell * aside;
            if lift <= 0.0 && drop <= 0.0 {
                continue;
            }
            for c in 0..3 {
                let v = f32::from(out[at * 4 + c]) / 255.0;
                let v = (v + lift * (1.0 - v)) * (1.0 - drop);
                out[at * 4 + c] = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
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

/// One box blur of `radius`, across and then down; three in a row are near
/// enough to a Gaussian.
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

/// The k in the window's face, at the height the icon wants it at a `size`
/// canvas — measured on its ink, so a face with tall ascenders and one with
/// short come out the same height. `None` when the window is in egui's own
/// face.
///
/// In Japanese the icon is a ケ, the first of ケスラー, drawn from the Japanese face
/// kessler brings — its bold, as the Mac's k is — since none of the window's
/// own faces has a katakana to draw it from.
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

/// The same letter with its ink `ink` pixels tall, for the symbol in the menu
/// bar — the k in its box there.
pub fn small_letter(ctx: &egui::Context, ink: usize) -> Option<Mask> {
    letter(ctx, (ink as f32 / LETTER).round() as usize)
}

/// The bundled Japanese face for the icon: bold where there is one.
pub(super) fn japanese_face() -> Option<Vec<u8>> {
    let (regular, bold) = super::theme::bundled_fallback()?;
    std::fs::read(bold.unwrap_or(regular)).ok()
}

/// Where in Asap the icon's k is drawn: weight 700, width 85. The same place in
/// the shipped icon (`bundle/make-icon.py`) and the one the window draws,
/// whatever the window's own width and weight, so the k does not jump when the
/// app opens or closes. A face with axes of its own is set at the same place,
/// as far as its axes reach.
pub const ICON_AT: &[(&str, f32)] = &[("wdth", 85.0), ("wght", 700.0)];

/// How heavy the ケ is drawn, on the Japanese face's weight axis: its bold.
pub(super) const KE_WEIGHT: f32 = 700.0;

/// Once at the canvas' size to measure, once at the size that measures.
fn sized(size: usize, draw: impl Fn(f32) -> Option<Mask>) -> Option<Mask> {
    let probe = draw(size as f32)?;
    let ink = (probe.height as f32 - 2.0).max(1.0);
    draw(size as f32 * LETTER * size as f32 / ink)
}

/// The k alone, on a square `size` pixels across, where the tile sets it:
/// its cover, one byte a pixel. For the menu's head while nothing plays, the
/// k on the plate the pictures play on — in the face the window is set in, at
/// its width and its bold, so it reads as the window's own letter rather than
/// the icon's. A face without a weight axis gives the icon's k (from its bold
/// file); Japanese the ケ as on the icon. `None` as for [`letter`].
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

/// Where a mask's ink balances, across.
fn weight_down(mask: &Mask) -> f32 {
    let (mut sum, mut moment) = (0.0f32, 0.0f32);
    for (at, &c) in mask.cover.iter().enumerate() {
        let c = c as f32;
        sum += c;
        moment += c * ((at / mask.width) as f32 + 0.5);
    }
    if sum > 0.0 { moment / sum } else { mask.height as f32 / 2.0 }
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

/// The same pixels premultiplied, as the Mac wants them.
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

    /// No scheme dresses the running icon as the closed one is — a white k on
    /// black — or the Dock could not tell the two apart.
    #[test]
    fn no_scheme_looks_closed() {
        use super::super::theme::Theme;
        let near = |c: Color32, to: u8| [c.r(), c.g(), c.b()].iter().all(|&v| v.abs_diff(to) < 24);
        for theme in Theme::all() {
            let (ground, ink) = theme.icon_colours();
            assert!(!(near(ground, 0) && near(ink, 255)), "{theme:?} looks like the closed icon");
        }
    }

    /// Every scheme's k stands out on its icon's ground.
    #[test]
    fn every_icon_k_is_legible() {
        use super::super::theme::{contrast, Theme};
        for theme in Theme::all() {
            if matches!(theme, Theme::Eigenes | Theme::EigenesDunkel) {
                continue;
            }
            let (ground, ink) = theme.icon_colours();
            assert!(contrast(ground, ink) >= 3.0, "{theme:?}: {}", contrast(ground, ink));
        }
    }

    /// The Mac's tile is on Apple's grid: square in the middle, rounded at
    /// the corners, nothing outside.
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

    /// GNOME's stands on a lip; KDE's nearly fills the canvas.
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

    /// The letter stands in the middle in the ink; the rest is the ground.
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

    /// The Japanese icon's ケ can be drawn from the face kessler brings,
    /// and stands as tall as a k would.
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

    /// Not a test so much as a look: every shape in a few schemes, in Asap
    /// as the window first opens in it, and the last row bolder — written to
    /// `KESSLER_ICONS` (a PNG).
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

    /// Every scheme's icon in the Mac's shape, 1024 square, as the window
    /// draws it in Muto where the copy has it (else Asap) — the frames of the
    /// animated logo, written to `KESSLER_ICON_FRAMES` (a folder).
    #[test]
    #[ignore]
    fn icon_frames() {
        use super::super::theme::Theme;
        let dir = std::path::PathBuf::from(std::env::var("KESSLER_ICON_FRAMES").unwrap_or("/tmp/icon-frames".into()));
        std::fs::create_dir_all(&dir).unwrap();
        let muto = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/fonts/Muto/MutoBold.otf"));
        let size = 1024;
        let letter = match &muto {
            Ok(data) => sized(size, |p| super::super::glyphs::letter_in(data, 'k', p, &[])).unwrap(),
            Err(_) => {
                let asap = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/fonts/Asap/Asap-Variable.ttf")).unwrap();
                sized(size, |p| super::super::glyphs::letter_in(&asap, 'k', p, ICON_AT)).unwrap()
            }
        };
        for (n, theme) in Theme::all().iter().enumerate() {
            let (ground, accent) = theme.icon_colours();
            let px = compose(size, Shape::Mac, ground, accent, Some(&letter));
            let icon = image::RgbaImage::from_raw(size as u32, size as u32, px).unwrap();
            icon.save(dir.join(format!("{n:02}-{theme:?}.png"))).unwrap();
        }
    }
}
