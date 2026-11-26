//! The covers on the records' headings in the track list, and beside what
//! is playing.
//!
//! Three things keep them cheap. The covers were shrunk to tiles once, during
//! the scan, so nothing here decodes a two-thousand-pixel JPEG. Only what is
//! on screen is ever turned into a texture, on a thread of its own, with a
//! ceiling on the memory they may hold between them. And a long list costs
//! the same as a short one.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;

use eframe::egui::{self, Color32, Rect, Ui};

use crate::library::artwork;

use super::dither::{self, Look};
use super::theme::{Palette, Role, MARKER_RADIUS};
use super::{text, GuiApp};

/// How many covers may be asked of the reading thread in one frame, and how
/// many may be waiting there at once.
///
/// Scrolling a long way lands on a screenful of tiles that have never been
/// loaded. They are read, sized and screened on a thread of their own — on the
/// window's, four a frame on a Retina screen held each frame up by a tenth of a
/// second and the window stood still while the wall filled. Until a cover is
/// there its tile is the blank one, and the covers arrive one by one.
const LOADS_PER_FRAME: usize = 8;
const WAITING_AT_MOST: usize = 24;

/// How much memory the held covers may take between them, in bytes, and
/// how few may be held whatever their size.
///
/// A ceiling on the bytes rather than on the count: a cover is grain × grain ×
/// 4 bytes, and the grain follows the heading's size and the screen — ninety
/// six covers were a few megabytes at the size the headings began at and a
/// quarter of a gigabyte at the largest on a Retina screen. What is on screen
/// is never let go of, so the floor only matters to a screenful of very big
/// ones.
///
/// And a ceiling on the count as well: with the wall gone a screen shows ten or
/// twenty headings, not hundreds of tiles, and covers of 48 points are so small
/// that the bytes alone let well over a thousand pile up over an evening's
/// browsing. A few screenfuls back are kept; further back is read again on the
/// way there, off the window's thread.
const TEXTURE_BYTES: usize = 48 * 1024 * 1024;
const TEXTURES_AT_LEAST: usize = 16;
const TEXTURES_AT_MOST: usize = 64;

/// What is known about one album's cover.
enum Slot {
    Ready(egui::TextureHandle),
    /// Asked of the reading thread, not back yet.
    Waiting,
    /// Looked for and not there. Remembered so it is not looked for again on
    /// every frame of a scroll.
    Missing,
}

/// The covers currently held in video memory.
pub struct CoverCache {
    dir: PathBuf,
    /// Whether the held textures are halftones or the covers themselves, and
    /// how coarse the halftone is. All three are different pictures, so
    /// changing any of them throws the textures away — which costs the two
    /// frames it takes to build a screenful again.
    dithered: bool,
    grain: u32,
    method: Look,
    /// Screened from the negative, for a light ground — see
    /// [`Palette::screens_negative`]. A different picture again.
    negative: bool,
    slots: HashMap<u64, Slot>,
    /// Load order, for deciding what to let go of first.
    order: VecDeque<u64>,
    /// Which covers this frame actually asked for; nothing visible is evicted.
    wanted: HashSet<u64>,
    budget: usize,
    /// Set when a frame ran out of budget, so another is asked for.
    more_to_come: bool,
    /// The reading thread, started at the first cover asked for, and what
    /// comes back from it.
    reader: Option<std::sync::mpsc::Sender<Job>>,
    read: Option<std::sync::mpsc::Receiver<Read>>,
    /// How many covers are with the reading thread.
    waiting: usize,
    /// Covers read and not yet on the graphics card, and how many more may
    /// go up this frame — see [`UPLOADS_PER_FRAME`].
    arrived: VecDeque<Read>,
    uploads: usize,
    /// Counted up whenever what is held is thrown away, so a cover read for
    /// the old dress is not taken for the new one.
    dressing: u64,
    /// How many pixels a side a heading's cover takes on the screen. A
    /// cover in its own colours is brought down to this before it goes to
    /// the graphics card: kept at the four hundred of the tile, each was
    /// 640 kb for a picture drawn a hundred pixels wide.
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
            arrived: VecDeque::new(),
            uploads: UPLOADS_PER_FRAME,
            dressing: 0,
            fit: artwork::TILE,
        }
    }

    /// How many covers are with the reading thread — for the measurement.
    pub fn waiting(&self) -> usize {
        self.waiting
    }

    /// Forget everything held. Used after a rescan, when covers that were
    /// missing may now be there.
    pub fn clear(&mut self) {
        self.slots.clear();
        self.order.clear();
        self.waiting = 0;
        self.dressing += 1;
    }

    /// Draw them this way from now on, throwing away what was built the other
    /// way.
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
        self.uploads = UPLOADS_PER_FRAME;
        self.more_to_come = false;
        self.wanted.clear();
    }

    /// Let go of the least recently loaded covers that are not on screen.
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

    /// The texture for one album, asking for it if there is budget left.
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

    /// The thread that reads covers, started the first time one is wanted.
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

    /// Take in what the reading thread has finished, as textures.
    fn receive(&mut self, ctx: &egui::Context) {
        let Some(read) = &self.read else {
            return;
        };
        self.arrived.extend(read.try_iter());
        while self.uploads > 0 {
            let Some(cover) = self.arrived.pop_front() else { break };
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
                    self.uploads -= 1;
                    Slot::Ready(ctx.load_texture(format!("cover-{:016x}", cover.key), image, filter))
                }
                None => Slot::Missing,
            };
            self.slots.insert(cover.key, slot);
        }
        if !self.arrived.is_empty() {
            self.more_to_come = true;
        }
    }
}

/// How many covers go up to the graphics card in one frame, at most.
const UPLOADS_PER_FRAME: usize = 4;

/// One cover asked of the reading thread.
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

/// And what it sends back.
struct Read {
    key: u64,
    dressing: u64,
    dithered: bool,
    image: Option<egui::ColorImage>,
}

/// Read one cover off disk into something egui can upload — in its own
/// colours no bigger than `fit` a side, where one is given.
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

/// A cover as a pattern of dots and nothing else.
///
/// The cover is first reduced to the number of dots wanted, then every dot is
/// decided by [`dither::screen`]. Reducing first is what makes the dots land
/// on whole screen pixels; deciding second is what makes them dots.
///
/// They come out **white and opaque, the gaps transparent**, so the colour is
/// not baked in: the tile tints it when it draws it. One texture per album,
/// any colour, and a scheme on the same side of light and dark costs nothing.
/// `negative` screens it from the negative, for a scheme whose ink is darker
/// than its ground — see [`Palette::screens_negative`].
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

/// [`screened`], for a picture that is not square: `width` × `height` dots.
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

/// The colour of the hairline round every picture, transparent for none —
/// set once a frame from the config (see [`set_outline`]), as the pictures
/// are painted from many places that do not all hold the config.
static OUTLINE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// The hairline's colour for what `[ui] picture_outline` names, in this
/// palette; transparent for "aus" and anything not known.
pub fn outline_colour(name: &str, palette: &Palette) -> Color32 {
    match name {
        "zart" => palette.faint,
        "tinte" => palette.ink,
        "akzent" => palette.accent_faint,
        _ => Color32::TRANSPARENT,
    }
}

/// The colour for this frame's pictures' hairline.
pub fn set_outline(colour: Color32) {
    OUTLINE.store(u32::from_le_bytes(colour.to_array()), std::sync::atomic::Ordering::Relaxed);
}

/// A hairline round a picture, or the tile standing in for one: one pixel of
/// the screen, inside its edge so it takes nothing from what stands beside
/// it, on the picture's own rounded corner.
fn outline(ui: &Ui, rect: Rect, corner: u8) {
    let [r, g, b, a] = OUTLINE.load(std::sync::atomic::Ordering::Relaxed).to_le_bytes();
    if a == 0 {
        return;
    }
    let width = 1.0 / ui.ctx().pixels_per_point();
    ui.painter().rect_stroke(
        rect,
        corner,
        egui::Stroke::new(width, Color32::from_rgba_premultiplied(r, g, b, a)),
        egui::StrokeKind::Inside,
    );
}

/// A screened picture in `rect`: its dots in `ink`, and — in three tones —
/// the second colour's under them, the two halves of its texture drawn one
/// over the other (see [`screened`]); the hairline round it where one is
/// asked for.
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
    outline(ui, rect, corner);
}

/// A grey picture brought to `side` × `side`, quickly: an average over the
/// pixels each new one covers on the way down, a straight line between
/// neighbours on the way up. `resize` with any of its filters took fifteen to
/// twenty milliseconds a cover to go from four hundred to eight hundred on a
/// Retina screen, four a frame, and the window stood still while the wall
/// filled; this is a millisecond or two, and the dots that come of it are the
/// same dots to the eye.
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

/// Get the cache ready for the album headings in the track list: covers at the
/// heading's size, screened as every picture is (darstellung → bilder). The
/// cache is theirs alone since the wall went — the two dressing it at two sizes
/// on the same frame was what kept it loading for ever.
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

/// And when they are drawn: let go of what is not on screen, and ask for
/// another frame if some covers had to wait for one.
pub fn after_headings(app: &mut GuiApp, ctx: &egui::Context) {
    if app.cover_cache.more_to_come {
        ctx.request_repaint_after(std::time::Duration::from_millis(30));
    }
    app.cover_cache.trim();
}

/// One album's cover in `rect`, for a heading in the track list: a halftone
/// in the ink — in the accent while it is the record being heard, the way the
/// wall marks it — or the cover itself; the blank tile when there is none.
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

/// The stand-in for an album with no cover.
///
/// Not a picture of a missing picture: the same tint the search field uses,
/// with the album's initial in it. It says "nothing here" quietly, and a wall
/// of them still reads as a grid rather than as damage.
///
/// And marked the way a cover is: the one you are on, or the one you are
/// hearing, takes the accent. A halftone cover turns the accent by itself; a
/// blank one has no dots to turn, and left in the field's grey it was the one
/// tile on the wall that did not answer a click.
pub(super) fn blank(ui: &Ui, rect: Rect, name: &str, palette: &Palette, lit: bool) {
    let (fill, letter) = blank_colours(palette, lit);
    ui.painter().rect_filled(rect, MARKER_RADIUS, fill);
    outline(ui, rect, MARKER_RADIUS);
    let Some(initial) = name.chars().find(|c| c.is_alphanumeric()) else {
        return;
    };
    let galley = text::line(ui, &initial.to_string(), Role::ContentBold, letter);
    let at = text::ink_centred(&galley, rect);
    ui.painter().galley(at, galley, letter);
}

/// The square a halftone stands on.
///
/// The dots are where a cover is light, so a cover that is mostly dark — a
/// black sleeve with a line drawing on it — came out as a few dots floating
/// on the ground, with no edge to say where the record is. The field a blank
/// tile stands on goes behind every halftone: a shade off the ground, enough
/// to keep the square a square, not enough to become a frame. No line — the
/// window has none to spare.
pub(super) fn halftone_ground(ui: &Ui, rect: Rect, palette: &Palette) {
    ui.painter().rect_filled(rect, MARKER_RADIUS, palette.field);
}

/// The fill and the letter of a blank tile. Marked, it is the accent laid
/// over the field rather than the accent itself: a solid square of it would
/// be the loudest thing on the wall, louder than a halftone in the same
/// colour, whose gaps let the ground through.
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

    /// Up and down, the picture keeps its shape: a light left half stays a
    /// light left half, and nothing comes out of the edges.
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

    /// A blank tile answers a click the way a cover does: marked, it takes
    /// the accent, and its letter stays readable on it.
    #[test]
    fn a_blank_tile_is_marked_too() {
        let palette = crate::gui::theme::Theme::MacosDunkel.palette();
        let (quiet, _) = blank_colours(&palette, false);
        let (lit, letter) = blank_colours(&palette, true);
        assert_eq!(quiet, palette.field);
        assert_ne!(lit, quiet, "a marked blank tile looks like any other");
        assert_ne!(letter, lit);
    }

    /// One dot per point at the middle of the slider — the size that makes
    /// the pattern land on whole screen pixels instead of beating against
    /// them. The rest of the slider's behaviour is tested where it lives.
    #[test]
    fn the_grain_follows_the_tile() {
        assert_eq!(dither::grain_for(168.0, 2.0, 0.5), 168);
        assert_eq!(dither::grain_for(120.0, 2.0, 0.5), 120);

        assert_eq!(dither::grain_for(169.0, 2.0, 0.5), 168);
        assert_eq!(dither::grain_for(171.0, 2.0, 0.5), 168);
    }

    /// A cover that is mostly light keeps being mostly light on a light
    /// ground: its dots — dark ink there — go where the picture is dark, so
    /// few of them. On a dark ground the same cover is mostly dots, light
    /// ink where it is light.
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
