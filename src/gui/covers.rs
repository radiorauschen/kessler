use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;

use eframe::egui::{self, Color32, Rect, Ui};

use crate::library::artwork;

use super::dither::{self, Look};
use super::theme::{Palette, Role, MARKER_RADIUS};
use super::{text, GuiApp};

const LOADS_PER_FRAME: usize = 8;
const WAITING_AT_MOST: usize = 24;

const TEXTURE_BYTES: usize = 48 * 1024 * 1024;
const TEXTURES_AT_LEAST: usize = 16;
const TEXTURES_AT_MOST: usize = 64;

enum Slot {
    Ready(egui::TextureHandle),
    Waiting,
    Missing,
}

pub struct CoverCache {
    dir: PathBuf,
    dithered: bool,
    grain: u32,
    method: Look,
    negative: bool,
    slots: HashMap<u64, Slot>,
    order: VecDeque<u64>,
    wanted: HashSet<u64>,
    budget: usize,
    more_to_come: bool,
    reader: Option<std::sync::mpsc::Sender<Job>>,
    read: Option<std::sync::mpsc::Receiver<Read>>,
    waiting: usize,
    dressing: u64,
    fit: u32,
}

impl CoverCache {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            dithered: false,
            grain: 0,
            method: Look::default(),
            negative: false,
            slots: HashMap::new(),
            order: VecDeque::new(),
            wanted: HashSet::new(),
            budget: LOADS_PER_FRAME,
            more_to_come: false,
            reader: None,
            read: None,
            waiting: 0,
            dressing: 0,
            fit: artwork::TILE,
        }
    }

    pub fn waiting(&self) -> usize {
        self.waiting
    }

    pub fn clear(&mut self) {
        self.slots.clear();
        self.order.clear();
        self.waiting = 0;
        self.dressing += 1;
    }

    fn dress(&mut self, dithered: bool, grain: u32, method: Look, negative: bool, fit: u32) {
        let refit = !dithered && self.fit != fit;
        self.fit = fit;
        if self.dithered != dithered
            || self.grain != grain
            || self.method != method
            || self.negative != negative
            || refit
        {
            self.dithered = dithered;
            self.grain = grain;
            self.method = method;
            self.negative = negative;
            self.clear();
        }
    }

    fn begin_frame(&mut self) {
        self.budget = LOADS_PER_FRAME;
        self.more_to_come = false;
        self.wanted.clear();
    }

    fn trim(&mut self) {
        let side = if self.dithered { self.grain } else { self.fit };
        let each = (side.max(1) as usize).pow(2) * 4;
        let budget = (TEXTURE_BYTES / each).clamp(TEXTURES_AT_LEAST, TEXTURES_AT_MOST);
        while self.slots.len() > budget {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if self.wanted.contains(&oldest) {
                self.order.push_back(oldest);
                if self.order.len() <= self.wanted.len() {
                    break;
                }
                continue;
            }
            self.slots.remove(&oldest);
        }
    }

    fn texture(&mut self, ctx: &egui::Context, key: u64) -> Option<egui::TextureHandle> {
        self.receive(ctx);
        self.wanted.insert(key);

        if let Some(slot) = self.slots.get(&key) {
            return match slot {
                Slot::Ready(handle) => Some(handle.clone()),
                Slot::Waiting | Slot::Missing => None,
            };
        }
        if self.budget == 0 || self.waiting >= WAITING_AT_MOST {
            self.more_to_come = true;
            return None;
        }
        self.budget -= 1;

        let job = Job {
            key,
            path: artwork::cache_path_for(&self.dir, key),
            dithered: self.dithered,
            grain: self.grain,
            method: self.method,
            negative: self.negative,
            fit: self.fit,
            dressing: self.dressing,
            ctx: ctx.clone(),
        };
        if self.reader().send(job).is_err() {
            return None;
        }
        self.waiting += 1;
        self.slots.insert(key, Slot::Waiting);
        self.order.push_back(key);
        None
    }

    fn reader(&mut self) -> &std::sync::mpsc::Sender<Job> {
        if self.reader.is_none() {
            let (jobs, inbox) = std::sync::mpsc::channel::<Job>();
            let (outbox, read) = std::sync::mpsc::channel::<Read>();
            let _ = std::thread::Builder::new()
                .name("kessler-covers".into())
                .spawn(move || {
                    for job in inbox {
                        let image = load(&job.path, job.dithered, job.grain, job.method, job.negative, Some(job.fit));
                        if outbox.send(Read { key: job.key, dressing: job.dressing, dithered: job.dithered, image }).is_err() {
                            return;
                        }
                        job.ctx.request_repaint();
                    }
                });
            self.reader = Some(jobs);
            self.read = Some(read);
        }
        self.reader.as_ref().expect("just started")
    }

    fn receive(&mut self, ctx: &egui::Context) {
        let Some(read) = &self.read else {
            return;
        };
        let arrived: Vec<Read> = read.try_iter().collect();
        for cover in arrived {
            if cover.dressing != self.dressing {
                continue;
            }
            self.waiting = self.waiting.saturating_sub(1);
            if !matches!(self.slots.get(&cover.key), Some(Slot::Waiting)) {
                continue;
            }
            let slot = match cover.image {
                Some(image) => {
                    let filter = if cover.dithered {
                        egui::TextureOptions::NEAREST
                    } else {
                        egui::TextureOptions::LINEAR
                    };
                    Slot::Ready(ctx.load_texture(format!("cover-{:016x}", cover.key), image, filter))
                }
                None => Slot::Missing,
            };
            self.slots.insert(cover.key, slot);
        }
    }
}

struct Job {
    key: u64,
    path: PathBuf,
    dithered: bool,
    grain: u32,
    method: Look,
    negative: bool,
    fit: u32,
    dressing: u64,
    ctx: egui::Context,
}

struct Read {
    key: u64,
    dressing: u64,
    dithered: bool,
    image: Option<egui::ColorImage>,
}

fn load(
    path: &std::path::Path,
    dithered: bool,
    grain: u32,
    method: Look,
    negative: bool,
    fit: Option<u32>,
) -> Option<egui::ColorImage> {
    let decoded = image::ImageReader::open(path).ok()?.with_guessed_format().ok()?.decode().ok()?;
    if !dithered {
        let mut decoded = decoded.to_rgba8();
        let longest = decoded.width().max(decoded.height());
        if let Some(fit) = fit.filter(|&f| f > 0 && f < longest) {
            let side = |n: u32| ((n as u64 * fit as u64) / longest as u64).max(1) as u32;
            decoded = image::imageops::thumbnail(&decoded, side(decoded.width()), side(decoded.height()));
        }
        let size = [decoded.width() as usize, decoded.height() as usize];
        return Some(egui::ColorImage::from_rgba_unmultiplied(
            size,
            decoded.as_raw(),
        ));
    }
    Some(screened(&decoded, grain, method, negative))
}

pub(super) fn screened(
    source: &image::DynamicImage,
    grain: u32,
    method: Look,
    negative: bool,
) -> egui::ColorImage {
    let grain = grain.max(8);
    let mut grey = sized(&source.to_luma8(), grain);
    if negative {
        image::imageops::invert(&mut grey);
    }

    if method.tones > 2 {
        let layers = usize::from(method.tones) - 1;
        let levels = dither::screen_levels(grey.as_raw(), grain, grain, method.screen, method.tones);
        let side = grain as usize;
        let mut pixels = vec![Color32::TRANSPARENT; side * side * layers];
        for (i, level) in levels.iter().enumerate() {
            let (x, y) = (i % side, i / side);
            for layer in 0..usize::from(*level).min(layers) {
                pixels[y * side * layers + layer * side + x] = Color32::WHITE;
            }
        }
        return egui::ColorImage {
            size: [side * layers, side],
            pixels,
            source_size: egui::vec2(grain as f32 * layers as f32, grain as f32),
        };
    }
    let dots = dither::screen(grey.as_raw(), grain, grain, method.screen);
    let pixels = dots
        .into_iter()
        .map(|ink| if ink { Color32::WHITE } else { Color32::TRANSPARENT })
        .collect();

    egui::ColorImage {
        size: [grain as usize, grain as usize],
        pixels,
        source_size: egui::vec2(grain as f32, grain as f32),
    }
}

pub(super) fn screened_wide(source: &image::DynamicImage, width: u32, height: u32, method: Look, negative: bool) -> egui::ColorImage {
    let (width, height) = (width.max(8), height.max(8));
    let mut grey = image::imageops::resize(&source.to_luma8(), width, height, image::imageops::FilterType::Triangle);
    if negative {
        image::imageops::invert(&mut grey);
    }
    let (w, h) = (width as usize, height as usize);
    if method.tones > 2 {
        let layers = usize::from(method.tones) - 1;
        let levels = dither::screen_levels(grey.as_raw(), width, height, method.screen, method.tones);
        let mut pixels = vec![Color32::TRANSPARENT; w * h * layers];
        for (i, level) in levels.iter().enumerate() {
            let (x, y) = (i % w, i / w);
            for layer in 0..usize::from(*level).min(layers) {
                pixels[y * w * layers + layer * w + x] = Color32::WHITE;
            }
        }
        return egui::ColorImage { size: [w * layers, h], pixels, source_size: egui::vec2((w * layers) as f32, h as f32) };
    }
    let pixels = dither::screen(grey.as_raw(), width, height, method.screen)
        .into_iter()
        .map(|ink| if ink { Color32::WHITE } else { Color32::TRANSPARENT })
        .collect();
    egui::ColorImage { size: [w, h], pixels, source_size: egui::vec2(w as f32, h as f32) }
}

pub(super) fn paint_screened(
    ui: &Ui,
    rect: Rect,
    corner: u8,
    texture: egui::TextureId,
    under: &[Color32],
    ink: Color32,
) {
    let uv = |from: f32, to: f32| Rect::from_min_max(egui::pos2(from, 0.0), egui::pos2(to, 1.0));
    let layer = |colour: Color32, uv: Rect| {
        ui.painter()
            .add(egui::epaint::RectShape::filled(rect, corner, colour).with_texture(texture, uv));
    };
    let layers = under.len() + 1;
    let share = 1.0 / layers as f32;
    for (i, colour) in under.iter().chain(std::iter::once(&ink)).enumerate() {
        layer(*colour, uv(i as f32 * share, (i + 1) as f32 * share));
    }
}

fn sized(grey: &image::GrayImage, side: u32) -> image::GrayImage {
    let (width, height) = grey.dimensions();
    if width == 0 || height == 0 {
        return image::GrayImage::new(side, side);
    }
    if (width, height) == (side, side) {
        return grey.clone();
    }
    if side <= width && side <= height {
        return image::imageops::thumbnail(grey, side, side);
    }
    let raw = grey.as_raw();
    let w = width as usize;
    let at = |x: usize, y: usize| f32::from(raw[y * w + x]);
    let along = |i: u32, from: u32| -> (usize, usize, f32) {
        let p = ((i as f32 + 0.5) * from as f32 / side as f32 - 0.5).clamp(0.0, (from - 1) as f32);
        let lo = p.floor() as usize;
        (lo, (lo + 1).min(from as usize - 1), p - lo as f32)
    };
    let columns: Vec<_> = (0..side).map(|x| along(x, width)).collect();
    let mut out = image::GrayImage::new(side, side);
    for y in 0..side {
        let (y0, y1, fy) = along(y, height);
        for (x, &(x0, x1, fx)) in columns.iter().enumerate() {
            let top = at(x0, y0) + (at(x1, y0) - at(x0, y0)) * fx;
            let bottom = at(x0, y1) + (at(x1, y1) - at(x0, y1)) * fx;
            let v = top + (bottom - top) * fy;
            out.put_pixel(x as u32, y, image::Luma([v.round() as u8]));
        }
    }
    out
}

pub fn dress_for_headings(app: &mut GuiApp, ctx: &egui::Context, size: f32) {
    let ui = &app.config.ui;
    let grain = dither::grain_for(size, ctx.pixels_per_point(), ui.picture_fineness());
    let method = Look::of(ui);
    let dithered = ui.cover_dither;
    let negative = app.palette.screens_negative();
    let fit = ((size * ctx.pixels_per_point()).ceil() as u32).clamp(1, artwork::TILE);
    app.cover_cache.dress(dithered, grain, method, negative, fit);
    app.cover_cache.begin_frame();
}

pub fn after_headings(app: &mut GuiApp, ctx: &egui::Context) {
    if app.cover_cache.more_to_come {
        ctx.request_repaint_after(std::time::Duration::from_millis(30));
    }
    app.cover_cache.trim();
}

pub fn heading_cover(
    cache: &mut CoverCache,
    ui: &Ui,
    rect: Rect,
    track: &crate::db::models::Track,
    heard: bool,
    palette: &Palette,
) {
    let key = artwork::key(
        &crate::db::sort_key(crate::db::grouping_artist(track)),
        &crate::db::sort_key(&track.album),
    );
    match cache.texture(ui.ctx(), key) {
        Some(handle) => {
            let tint = match (cache.dithered, heard) {
                (false, _) => Color32::WHITE,
                (true, true) => palette.lit_dots(),
                (true, false) => palette.ink,
            };
            if cache.dithered {
                halftone_ground(ui, rect, palette);
            }
            let under = if cache.dithered { palette.tones_under(tint, cache.method.tones) } else { Vec::new() };
            paint_screened(ui, rect, MARKER_RADIUS, handle.id(), &under, tint);
        }
        None => blank(ui, rect, &track.album, palette, heard),
    }
}

pub(super) fn blank(ui: &Ui, rect: Rect, name: &str, palette: &Palette, lit: bool) {
    let (fill, letter) = blank_colours(palette, lit);
    ui.painter().rect_filled(rect, MARKER_RADIUS, fill);
    let Some(initial) = name.chars().find(|c| c.is_alphanumeric()) else {
        return;
    };
    let galley = text::line(ui, &initial.to_string(), Role::ContentBold, letter);
    let at = text::ink_centred(&galley, rect);
    ui.painter().galley(at, galley, letter);
}

pub(super) fn halftone_ground(ui: &Ui, rect: Rect, palette: &Palette) {
    ui.painter().rect_filled(rect, MARKER_RADIUS, palette.field);
}

fn blank_colours(palette: &Palette, lit: bool) -> (Color32, Color32) {
    if lit {
        (palette.field.lerp_to_gamma(palette.lit_dots(), 0.55), palette.ground)
    } else {
        (palette.field, palette.faint)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cover_is_sized_up_and_down_without_losing_its_shape() {
        let half = image::GrayImage::from_fn(400, 400, |x, _| image::Luma([if x < 200 { 255 } else { 0 }]));
        for side in [100, 336, 800] {
            let out = sized(&half, side);
            assert_eq!(out.dimensions(), (side, side));
            assert_eq!(out.get_pixel(2, side / 2)[0], 255);
            assert_eq!(out.get_pixel(side - 3, side / 2)[0], 0);
            let row: u32 = (0..side).map(|x| u32::from(out.get_pixel(x, 0)[0])).sum();
            let mean = row / side;
            assert!((120..=135).contains(&mean), "half light, half dark: {mean}");
        }
    }

    #[test]
    fn a_blank_tile_is_marked_too() {
        let palette = crate::gui::theme::Theme::MacosDunkel.palette();
        let (quiet, _) = blank_colours(&palette, false);
        let (lit, letter) = blank_colours(&palette, true);
        assert_eq!(quiet, palette.field);
        assert_ne!(lit, quiet, "a marked blank tile looks like any other");
        assert_ne!(letter, lit);
    }

    #[test]
    fn the_grain_follows_the_tile() {
        assert_eq!(dither::grain_for(168.0, 2.0, 0.5), 168);
        assert_eq!(dither::grain_for(120.0, 2.0, 0.5), 120);

        assert_eq!(dither::grain_for(169.0, 2.0, 0.5), 168);
        assert_eq!(dither::grain_for(171.0, 2.0, 0.5), 168);
    }

    #[test]
    fn a_light_cover_stays_light_on_a_light_ground() {
        let light = image::DynamicImage::ImageLuma8(image::GrayImage::from_pixel(64, 64, image::Luma([220])));
        let share = |negative: bool| {
            let dots = screened(&light, 64, Look::default(), negative);
            dots.pixels.iter().filter(|p| **p == Color32::WHITE).count() as f32 / dots.pixels.len() as f32
        };
        assert!(share(false) > 0.75, "on a dark ground: {}", share(false));
        assert!(share(true) < 0.25, "on a light ground: {}", share(true));
    }
}
