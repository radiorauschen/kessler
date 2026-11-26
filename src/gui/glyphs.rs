//! The transport's marks, out of the typeface the window is set in.
//!
//! Five of the marks this window uses are not letters, and some of the best
//! of them cannot be reached by typing a character: a face's best repeat may
//! be its second drawing of `↻` (`uni21BB.alt`, say) — two arrows chasing
//! each other round, the queue going round — and egui sets a character's
//! first drawing and nothing else. It has no stylistic sets.
//!
//! So these are read out of the font file by name and drawn here: the outline
//! is walked, flattened into straight edges and filled into a small mask at
//! the screen's own resolution, then handed to the painter as a texture and
//! tinted like any other mark. What comes out is the type designer's drawing
//! at the size of the label words beside it — the same stems, the same ends,
//! the same hand — which neither a fallback face nor lines drawn here could be.
//!
//! Where the face has an arrow but not the shuffle or the repeat — many have
//! `←` and `→` and neither of the other two — those two are built here out of
//! the face's own arrow: its stem, its head and the thickness of the head's
//! arms are measured off the drawing and set again, two lanes for the
//! shuffle, two arrows chasing round for the repeat. See [`compose`].
//!
//! Where the face has not got even that (none at all), the caller is told so
//! and draws its own.

use std::collections::HashMap;
use std::sync::Arc;

use eframe::egui::{self, Color32, Context, Pos2, Rect, TextureHandle, Vec2};
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{FontRef, GlyphNames, MetadataProvider};

/// One of the marks, and how it is found in a face.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mark {
    Back,
    On,
    Shuffle,
    Repeat,
    RepeatOne,
    /// The three of the window's own buttons, where kessler draws them:
    /// close, into the Dock, full screen — the face's ×, − and +.
    Close,
    Shrink,
    Grow,
}

impl Mark {
    /// The glyph names to try, best first, and the character to fall back on
    /// when a face names its glyphs differently or not at all.
    fn wanted(self) -> (&'static [&'static str], char) {
        match self {
            Self::Back => (&["arrowleft", "uni2190"], '←'),
            Self::On => (&["arrowright", "uni2192"], '→'),
            Self::Shuffle => (&["uni21F5"], '⇵'),
            Self::Repeat => (&["uni21BB.alt", "uni21BB"], '↻'),
            Self::RepeatOne => (&["uni2460"], '①'),
            Self::Close => (&["multiply", "uni00D7"], '×'),
            Self::Shrink => (&["minus", "uni2212"], '−'),
            Self::Grow => (&["plus", "uni002B"], '+'),
        }
    }

    /// A quarter turn for the shuffle: `⇵` stands two lanes side by side,
    /// one down and one up; turned, it is two lanes one above the other, one
    /// going each way along the line the row reads in — `⇆`, and a shuffle
    /// that looks like the other arrows in the row, which all lie flat.
    /// The glyph looks the same turned either way.
    fn turned(self) -> bool {
        self == Self::Shuffle
    }
}

/// How big the marks are set, in points per em.
///
/// A step above the label words (eleven) and below the lists (fifteen). At
/// eleven the arrows were drawn as small as a lowercase x, and the stems
/// were exactly the words' own; at fifteen the stems are a third heavier than
/// the words, which is how the row looked two weights at once. Twelve and a
/// half is where the arrows are plainly marks and the stems still read as the
/// same pen — measured against the word `pause`.
pub const MARK_SIZE: f32 = 12.5;

/// The face the marks are read from, as the context holds it, and how much
/// smaller than their size it is set — the same as the words beside them.
#[derive(Clone)]
struct Face(Arc<Vec<u8>>, f32);

/// The masks already drawn, keyed by mark and screen scale. `None` means the
/// face has not got that glyph, which is worth remembering too.
#[derive(Clone, Default)]
struct Drawn(HashMap<(Mark, u32, u32), Option<TextureHandle>>);

fn face_id() -> egui::Id {
    egui::Id::unique("kessler-mark-face")
}

fn drawn_id() -> egui::Id {
    egui::Id::unique("kessler-mark-textures")
}

/// Hand the marks the file the window's regular face was read from — or
/// nothing, when the window is set in egui's own face. Every mark drawn from
/// the face before is forgotten.
pub fn use_face(ctx: &Context, data: Option<Vec<u8>>, scale: f32) {
    ctx.data_mut(|d| {
        match data {
            Some(data) => {
                d.insert_temp(face_id(), Face(Arc::new(data), scale));
            }
            None => d.remove::<Face>(face_id()),
        }
        d.insert_temp(drawn_id(), Drawn::default());
    });
}

/// The face the icon's k is read from — see `appicon` — and its id.
#[derive(Clone)]
struct IconFace(Arc<Vec<u8>>, Place);

/// A place in a variable face's design space: axis tags and values.
pub type Place = Vec<(&'static str, f32)>;

fn icon_face_id() -> egui::Id {
    egui::Id::unique("kessler-icon-face")
}

/// Hand the icon the file its k is to be read from, and where in a variable
/// face's design space to read it — or nothing.
pub fn use_icon_face(ctx: &Context, data: Option<(Vec<u8>, Place)>) {
    ctx.data_mut(|d| match data {
        Some((data, at)) => {
            d.insert_temp(icon_face_id(), IconFace(Arc::new(data), at));
        }
        None => {
            d.remove::<IconFace>(icon_face_id());
        }
    });
}

/// Paint `mark` centred on `at` in `colour`. False when the face has not got
/// it, so the caller can draw a mark of its own there instead.
pub fn paint(ui: &egui::Ui, mark: Mark, at: Pos2, colour: Color32) -> bool {
    paint_sized(ui, mark, at, colour, MARK_SIZE)
}

/// The same at `size` points per em rather than the transport's — its ink
/// centred on `at`, as every mark here is.
pub fn paint_sized(ui: &egui::Ui, mark: Mark, at: Pos2, colour: Color32, size: f32) -> bool {
    let ctx = ui.ctx();
    let scale = ctx.pixels_per_point();
    let key = (mark, scale.to_bits(), size.to_bits());
    let known = ctx.data(|d| d.get_temp::<Drawn>(drawn_id()).and_then(|m| m.0.get(&key).cloned()));
    let texture = match known {
        Some(texture) => texture,
        None => {
            let face = ctx.data(|d| d.get_temp::<Face>(face_id()));
            let texture = face.and_then(|face| {
                let mask = draw(&face.0, mark, size * face.1 * scale)?;
                let image = egui::ColorImage::new(
                    [mask.width, mask.height],
                    mask.cover.iter().map(|&a| Color32::from_white_alpha(a)).collect(),
                );
                Some(ctx.load_texture(
                    format!("mark-{mark:?}-{scale}-{size}"),
                    image,
                    egui::TextureOptions::LINEAR,
                ))
            });
            ctx.data_mut(|d| {
                d.get_temp_mut_or_default::<Drawn>(drawn_id())
                    .0
                    .insert(key, texture.clone())
            });
            texture
        }
    };
    let Some(texture) = texture else {
        return false;
    };
    let size = texture.size_vec2() / scale;
    let corner = ((at - size / 2.0).to_vec2() * scale).round() / scale;
    let rect = Rect::from_min_size(corner.to_pos2(), size);
    ui.painter().image(
        texture.id(),
        rect,
        Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        colour,
    );
    true
}

/// A filled glyph: how much of each screen dot it covers, row by row.
#[derive(Debug, Clone, PartialEq)]
pub struct Mask {
    pub width: usize,
    pub height: usize,
    pub cover: Vec<u8>,
}

/// One letter of the icon's face — see [`use_icon_face`] — filled at
/// `pixels` per em: the icon is a k in that face. `None` when the window is
/// set in egui's own face, or the face has no such letter.
pub fn letter(ctx: &Context, character: char, pixels: f32) -> Option<Mask> {
    let face = ctx.data(|d| d.get_temp::<IconFace>(icon_face_id()))?;
    letter_in(&face.0, character, pixels, &face.1)
}

/// One letter of the face the window is set in — the regular file — at `at`
/// in its design space: the menu's head draws its k there while nothing
/// plays, in the face, width and weight the window has, not the icon's.
pub fn window_letter(ctx: &Context, character: char, pixels: f32, at: &[(&str, f32)]) -> Option<Mask> {
    let face = ctx.data(|d| d.get_temp::<Face>(face_id()))?;
    letter_in(&face.0, character, pixels, at)
}

/// The same out of a font file's bytes — at `at` in a variable face's design
/// space, for the axes it names: the Japanese icon's ケ is drawn bold out of M
/// PLUS 1 Code, which is one variable file, and Asap's k at the window's width
/// and bold.
pub fn letter_in(data: &[u8], character: char, pixels: f32, at: &[(&str, f32)]) -> Option<Mask> {
    let font = FontRef::new(data).ok()?;
    let location = font.axes().location(at.iter().copied());
    let mut edges = outline_at(&font, &[], character, pixels, (&location).into())?;
    for edge in &mut edges {
        edge.0.y = -edge.0.y;
        edge.1.y = -edge.1.y;
    }
    Some(fill(&edges))
}

/// Find `mark` in the face and fill it at `pixels` per em. `None` when the
/// file is not a font or has no such glyph.
pub fn draw(data: &[u8], mark: Mark, pixels: f32) -> Option<Mask> {
    let font = FontRef::new(data).ok()?;
    let (names, character) = mark.wanted();
    let mut edges = match outline_of(&font, names, character, pixels) {
        Some(mut edges) => {
            if mark.turned() {
                turn(&mut edges);
            }
            edges
        }
        None => compose(&font, mark, pixels)?,
    };
    for edge in &mut edges {
        edge.0.y = -edge.0.y;
        edge.1.y = -edge.1.y;
    }
    Some(fill(&edges))
}

/// A glyph's outline as straight edges, at `pixels` per em, the right way up
/// (y up the page). Found by one of `names`, else by `character`.
fn outline_of(font: &FontRef, names: &[&str], character: char, pixels: f32) -> Option<Vec<(Vec2, Vec2)>> {
    outline_at(font, names, character, pixels, LocationRef::default())
}

/// The same at a place in the face's design space.
fn outline_at(
    font: &FontRef,
    names: &[&str],
    character: char,
    pixels: f32,
    location: LocationRef,
) -> Option<Vec<(Vec2, Vec2)>> {
    let by_name = GlyphNames::new(font);
    let glyph = names
        .iter()
        .find_map(|wanted| by_name.iter().find(|(_, name)| name.as_str() == *wanted))
        .map(|(id, _)| id)
        .or_else(|| font.charmap().map(character))?;
    outline_glyph(font, glyph, pixels, location)
}

/// One glyph's outline as straight edges, by its id.
fn outline_glyph(font: &FontRef, glyph: skrifa::GlyphId, pixels: f32, location: LocationRef) -> Option<Vec<(Vec2, Vec2)>> {
    let outline = font.outline_glyphs().get(glyph)?;
    let mut pen = Flattener::default();
    outline
        .draw(
            DrawSettings::unhinted(Size::new(pixels), location),
            &mut pen,
        )
        .ok()?;
    pen.close();
    (!pen.edges.is_empty()).then_some(pen.edges)
}

/// A mark the face has not got, built out of what it has.
///
/// Out of its own `→`, where there is one: the stem, the head and the arms
/// measured off the drawing. Where there is not even that, out of its hyphen:
/// the hyphen's bar is the stem's thickness and height on the line, the head
/// is set at forty-five degrees in the same thickness, and every corner is
/// softened by as much as the face softens its own.
///
/// `None` for a face that has neither, or one whose arrow is not a stem and
/// a two-armed head.
fn compose(font: &FontRef, mark: Mark, pixels: f32) -> Option<Vec<(Vec2, Vec2)>> {
    let (pen, winding) = match outline_of(font, &["arrowright", "uni2192"], '→', pixels) {
        Some(arrow) => (Pen::measure(&arrow)?, area(&arrow).signum()),
        None => {
            let hyphen = outline_of(font, &["hyphen", "uni002D"], '-', pixels)?;
            (Pen::from_hyphen(&hyphen, pixels)?, area(&hyphen).signum())
        }
    };
    let mut shapes = match mark {
        Mark::Back => vec![pen.arrow(pen.tip, pen.tail, pen.middle, pen.head)],
        Mark::On => vec![pen.arrow(pen.tail, pen.tip, pen.middle, pen.head)],
        Mark::Shuffle => pen.shuffle(),
        Mark::Repeat => pen.repeat().concat(),
        Mark::RepeatOne => pen.one(),
        Mark::Close | Mark::Shrink | Mark::Grow => return None,
    };
    if pen.round > 0.0 {
        shapes = shapes.iter().map(|shape| rounded(shape, pen.round)).collect();
    }
    if mark == Mark::RepeatOne {
        shapes.push(pen.ring());
    }
    let edges: Vec<(Vec2, Vec2)> = shapes
        .into_iter()
        .flat_map(|mut shape| {
            if area_of(&shape).signum() != winding {
                shape.reverse();
            }
            closed(&shape)
        })
        .collect();
    Some(edges)
}

/// What an arrow is drawn with, measured off the face's own `→`: a straight
/// stem and a head of two arms at forty-five degrees.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Pen {
    /// Where the tail starts and the tip ends, across.
    tail: f32,
    tip: f32,
    /// The height of the stem's middle.
    middle: f32,
    /// How thick the stem is.
    stem: f32,
    /// How thick the head's arms are, measured across the page.
    arm: f32,
    /// How far the head reaches back from the tip, and so how far up and
    /// down.
    head: f32,
    /// How much every corner is softened — nothing for an arrow measured off
    /// the face, which has its own corners already.
    round: f32,
}

impl Pen {
    /// Measure an arrow pointing right. `None` when it is not built the way
    /// this expects — a filled triangle for a head, say — because a measure
    /// that does not fit would only draw something wrong.
    fn measure(edges: &[(Vec2, Vec2)]) -> Option<Pen> {
        let (low, high) = bounds(edges);
        let middle = (low.y + high.y) / 2.0;
        let head = high.y - middle;
        let (tail, tip) = (low.x, high.x);
        let near_tail = tail + (tip - tail) * 0.08;
        let stem = crossings_down(edges, near_tail)
            .into_iter()
            .find(|(a, b)| *a <= middle && middle <= *b)
            .map(|(a, b)| b - a)?;
        let arm = crossings_across(edges, middle + head / 2.0)
            .last()
            .map(|(a, b)| b - a)?;
        let fits = stem > 0.0 && arm > 0.0 && arm < head && stem < head && tip - tail > head;
        fits.then_some(Pen { tail, tip, middle, stem, arm, head, round: 0.0 })
    }

    /// A pen for a face with no arrow at all, out of its hyphen: the bar's
    /// thickness is the stem, its middle the height the arrow stands at, and
    /// the arrow as long and its head as deep as a drawn arrow's in proportion
    /// to the em. The corners are softened by a sixth of the stem, which is
    /// about what a softly drawn face does to its own.
    fn from_hyphen(hyphen: &[(Vec2, Vec2)], em: f32) -> Option<Pen> {
        let (low, high) = bounds(hyphen);
        let stem = high.y - low.y;
        if !(stem > 0.0 && stem < em * 0.3) {
            return None;
        }
        Some(Pen {
            tail: em * 0.05,
            tip: em * 0.68,
            middle: (low.y + high.y) / 2.0,
            stem,
            arm: stem * std::f32::consts::SQRT_2,
            head: em * 0.26,
            round: stem / 6.0,
        })
    }

    /// An arrow like the face's own, with a head `head` deep, its stem's
    /// middle at `middle`, from `tail` to `tip` — pointing right, or left when
    /// `tip` is the smaller.
    fn arrow(&self, tail: f32, tip: f32, middle: f32, head: f32) -> Vec<Vec2> {
        let s = if tip >= tail { 1.0 } else { -1.0 };
        let (d, t) = (self.arm, self.stem / 2.0);
        let p = |along: f32, up: f32| Vec2::new(tip - s * along, middle + up);
        vec![
            p(head, -head),
            p(0.0, 0.0),
            p(head, head),
            p(head + d / 2.0, head - d / 2.0),
            p(d + t, t),
            Vec2::new(tail, middle + t),
            Vec2::new(tail, middle - t),
            p(d + t, -t),
            p(head + d / 2.0, -head + d / 2.0),
        ]
    }

    /// The head alone, its tip at `tip` pointing along `towards`.
    fn head_alone(&self, tip: Vec2, towards: Vec2, head: f32) -> Vec<Vec2> {
        let d = self.arm;
        let along = towards.normalized();
        let across = Vec2::new(-along.y, along.x);
        let p = |back: f32, up: f32| tip - along * back + across * up;
        vec![
            p(head, -head),
            p(0.0, 0.0),
            p(head, head),
            p(head + d / 2.0, head - d / 2.0),
            p(d, 0.0),
            p(head + d / 2.0, -head + d / 2.0),
        ]
    }

    /// How tall a composed mark stands: a little more than the arrow, as a
    /// face's own shuffle and repeat tend to stand a little taller than its
    /// arrows.
    fn height(&self) -> f32 {
        self.head * 2.0 * 1.1
    }

    /// The head of a lane of the shuffle, and of either arrow of the repeat:
    /// small enough that two lanes stand in about the arrow's height.
    fn lane_head(&self) -> f32 {
        ((self.height() - 1.5 * self.stem) / 3.0).max(self.arm * 1.3)
    }

    /// Two lanes that cross, both going right: one from the top left down to
    /// the bottom right, unbroken, and one from the bottom left up to the top
    /// right, broken where it passes under the first. Straight at both ends,
    /// an S between,
    /// and the face's own head on each.
    fn shuffle(&self) -> Vec<Vec<Vec2>> {
        let t = self.stem;
        let head = (self.lane_head() * 0.9).max(self.arm * 1.3);
        let apart = 2.0 * head + 0.8 * t;
        let top = self.middle + apart / 2.0;
        let bottom = self.middle - apart / 2.0;
        let width = self.tip - self.tail;
        let bend_from = self.tail + width * 0.2;
        let bend_to = self.tail + width * 0.66;
        let end = self.tip - self.arm;
        let lane = |from: f32, to: f32| -> Vec<Vec2> {
            let mut line = vec![Vec2::new(self.tail, from)];
            let pull = (bend_to - bend_from) * 0.5;
            let (p0, p1, p2, p3) = (
                Vec2::new(bend_from, from),
                Vec2::new(bend_from + pull, from),
                Vec2::new(bend_to - pull, to),
                Vec2::new(bend_to, to),
            );
            let steps = 48;
            for i in 0..=steps {
                let s = i as f32 / steps as f32;
                let u = 1.0 - s;
                line.push(p0 * (u * u * u) + p1 * (3.0 * u * u * s) + p2 * (3.0 * u * s * s) + p3 * (s * s * s));
            }
            line.push(Vec2::new(end, to));
            line
        };
        let over = lane(top, bottom);
        let under = lane(bottom, top);
        let crossing = Vec2::new((bend_from + bend_to) / 2.0, self.middle);
        let clear = 1.8 * t;
        let side = |left: bool| -> Vec<Vec2> {
            under
                .iter()
                .copied()
                .filter(|p| (p.x < crossing.x) == left && (*p - crossing).length() >= clear)
                .collect()
        };
        let (before, after) = (side(true), side(false));
        vec![
            stroke(&over, t),
            self.head_alone(Vec2::new(self.tip, bottom), Vec2::new(1.0, 0.0), head),
            stroke(&before, t),
            stroke(&after, t),
            self.head_alone(Vec2::new(self.tip, top), Vec2::new(1.0, 0.0), head),
        ]
    }

    /// How far apart the repeat's two lanes run, middle to middle, and how
    /// deep their heads are: far enough apart that a side has room between
    /// its corner and the head below it.
    fn lanes(&self) -> (f32, f32) {
        let head = (self.lane_head() * 0.9).max(self.arm * 1.3);
        ((head + 0.8 * self.stem) / 0.65, head)
    }

    /// Two arrows chasing each other round, clockwise: the shuffle's two
    /// lanes, each turning at its tail into the side of a loop. The upper
    /// one comes up the left side, round the corner and along to its head at
    /// the right; the lower one is the same turned half round. Each side
    /// stops short of the other arrow's head by most of a stem's breadth.
    fn repeat(&self) -> Vec<Vec<Vec<Vec2>>> {
        let t = self.stem;
        let (apart, head) = self.lanes();
        let corner = apart * 0.35;
        let top = self.middle + apart / 2.0;
        let bottom = self.middle - apart / 2.0;
        let side = self.tail + t / 2.0;
        let stops = bottom + head + 0.8 * t;
        let mut line = vec![Vec2::new(side, stops)];
        let bend = Vec2::new(side + corner, top - corner);
        let steps = 12;
        for i in 0..=steps {
            let a = std::f32::consts::PI - std::f32::consts::FRAC_PI_2 * i as f32 / steps as f32;
            line.push(bend + Vec2::new(a.cos(), a.sin()) * corner);
        }
        line.push(Vec2::new(self.tip - self.arm, top));
        let upper = vec![
            stroke(&line, t),
            self.head_alone(Vec2::new(self.tip, top), Vec2::new(1.0, 0.0), head),
        ];
        let turn = |p: Vec2| Vec2::new(self.tail + self.tip - p.x, 2.0 * self.middle - p.y);
        let lower: Vec<Vec<Vec2>> = upper.iter().map(|s| s.iter().map(|p| turn(*p)).collect()).collect();
        vec![upper, lower]
    }
}

impl Pen {
    /// Where the ring of the repeat-one stands, and how big it is round the
    /// middle of its line: as tall as an arrow.
    fn ring_size(&self) -> (Vec2, f32) {
        (Vec2::new((self.tail + self.tip) / 2.0, self.middle), self.head)
    }

    /// The ring of the repeat-one: a circle the stem's thickness, its outer
    /// edge as tall as an arrow.
    fn ring(&self) -> Vec<Vec2> {
        let (centre, outer) = self.ring_size();
        let middle = outer - self.stem / 2.0;
        let steps = 48;
        let at = |i: usize, r: f32| {
            let a = std::f32::consts::TAU * i as f32 / steps as f32;
            centre + Vec2::new(a.cos(), a.sin()) * r
        };
        let mut shape: Vec<Vec2> = (0..=steps).map(|i| at(i, middle + self.stem / 2.0)).collect();
        shape.extend((0..=steps).rev().map(|i| at(i, middle - self.stem / 2.0)));
        shape
    }

    /// The „1" in the ring: a stem and a flag, drawn in the pen's own
    /// thickness — the face's own „1", scaled down to fit, came out thinner
    /// than the ring round it.
    fn one(&self) -> Vec<Vec<Vec2>> {
        let (centre, outer) = self.ring_size();
        let tall = 2.0 * (outer - self.stem) * 0.58;
        let (top, bottom) = (centre.y + tall / 2.0, centre.y - tall / 2.0);
        let x = centre.x + self.stem * 0.2;
        let flag = tall * 0.3;
        vec![
            stroke(&[Vec2::new(x, top + self.stem * 0.3), Vec2::new(x, bottom)], self.stem),
            stroke(&[Vec2::new(x - flag, top - flag * 0.7), Vec2::new(x, top)], self.stem),
        ]
    }
}

/// Soften every corner of a closed shape: each point is cut back along both
/// its edges by up to `by` — never more than half an edge — and the cut
/// bridged with a curve through where the corner was. Points on a straight
/// run, or on a gentle curve, are left as they are.
fn rounded(points: &[Vec2], by: f32) -> Vec<Vec2> {
    let n = points.len();
    let mut out = Vec::with_capacity(n * 4);
    for i in 0..n {
        let (before, at, after) = (points[(i + n - 1) % n], points[i], points[(i + 1) % n]);
        let (into, out_of) = (at - before, after - at);
        let turn = into.normalized().dot(out_of.normalized());
        if turn > 0.94 || into.length() < 1e-4 || out_of.length() < 1e-4 {
            out.push(at);
            continue;
        }
        let back = by.min(into.length() / 2.0).min(out_of.length() / 2.0);
        let (from, to) = (at - into.normalized() * back, at + out_of.normalized() * back);
        for step in 0..=4 {
            let t = step as f32 / 4.0;
            let u = 1.0 - t;
            out.push(from * (u * u) + at * (2.0 * u * t) + to * (t * t));
        }
    }
    out
}

/// A line `width` thick along the points, as one closed shape: each point
/// moved half the width out to either side, square at the two ends.
fn stroke(points: &[Vec2], width: f32) -> Vec<Vec2> {
    let n = points.len();
    let normal = |i: usize| {
        let before = points[i.saturating_sub(1)];
        let after = points[(i + 1).min(n - 1)];
        let along = (after - before).normalized();
        Vec2::new(-along.y, along.x)
    };
    let mut out: Vec<Vec2> = (0..n).map(|i| points[i] + normal(i) * (width / 2.0)).collect();
    out.extend((0..n).rev().map(|i| points[i] - normal(i) * (width / 2.0)));
    out
}

/// The smallest box round the edges.
fn bounds(edges: &[(Vec2, Vec2)]) -> (Vec2, Vec2) {
    let (mut low, mut high) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
    for (a, b) in edges {
        low = low.min(a.min(*b));
        high = high.max(a.max(*b));
    }
    (low, high)
}

/// The runs of ink along a line across the page at `y`, by the non-zero
/// rule, left to right.
fn crossings_across(edges: &[(Vec2, Vec2)], y: f32) -> Vec<(f32, f32)> {
    runs(
        edges
            .iter()
            .filter(|(a, b)| (a.y <= y) != (b.y <= y))
            .map(|(a, b)| (a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x), if b.y > a.y { 1 } else { -1 })),
    )
}

/// The same down the page at `x`, bottom to top.
fn crossings_down(edges: &[(Vec2, Vec2)], x: f32) -> Vec<(f32, f32)> {
    runs(
        edges
            .iter()
            .filter(|(a, b)| (a.x <= x) != (b.x <= x))
            .map(|(a, b)| (a.y + (x - a.x) / (b.x - a.x) * (b.y - a.y), if b.x > a.x { 1 } else { -1 })),
    )
}

fn runs(crossings: impl Iterator<Item = (f32, i32)>) -> Vec<(f32, f32)> {
    let mut crossings: Vec<_> = crossings.collect();
    crossings.sort_by(|p, q| p.0.total_cmp(&q.0));
    let (mut winding, mut from, mut out) = (0, 0.0, Vec::new());
    for (at, turn) in crossings {
        let was = winding;
        winding += turn;
        if was == 0 && winding != 0 {
            from = at;
        } else if was != 0 && winding == 0 {
            out.push((from, at));
        }
    }
    out
}

/// Twice the signed area the edges enclose; its sign says which way they
/// wind.
fn area(edges: &[(Vec2, Vec2)]) -> f32 {
    edges.iter().map(|(a, b)| a.x * b.y - b.x * a.y).sum()
}

fn area_of(points: &[Vec2]) -> f32 {
    area(&closed(points))
}

/// A closed polygon's edges.
fn closed(points: &[Vec2]) -> Vec<(Vec2, Vec2)> {
    (0..points.len())
        .map(|i| (points[i], points[(i + 1) % points.len()]))
        .collect()
}

/// A quarter turn.
fn turn(edges: &mut [(Vec2, Vec2)]) {
    for edge in edges {
        edge.0 = Vec2::new(-edge.0.y, edge.0.x);
        edge.1 = Vec2::new(-edge.1.y, edge.1.x);
    }
}

/// Walks an outline into straight edges, in the outline's own units.
#[derive(Default)]
struct Flattener {
    edges: Vec<(Vec2, Vec2)>,
    start: Vec2,
    at: Vec2,
}

impl Flattener {
    fn to(&mut self, point: Vec2) {
        if point != self.at {
            self.edges.push((self.at, point));
        }
        self.at = point;
    }
}

/// How many straight pieces a curve is cut into. The marks are a dozen dots
/// tall; eight pieces a curve is finer than any dot.
const PIECES: usize = 8;

impl OutlinePen for Flattener {
    fn move_to(&mut self, x: f32, y: f32) {
        self.close();
        self.start = Vec2::new(x, y);
        self.at = self.start;
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.to(Vec2::new(x, y));
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let (p0, p1, p2) = (self.at, Vec2::new(cx0, cy0), Vec2::new(x, y));
        for i in 1..=PIECES {
            let t = i as f32 / PIECES as f32;
            let u = 1.0 - t;
            self.to(p0 * (u * u) + p1 * (2.0 * u * t) + p2 * (t * t));
        }
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let (p0, p1, p2, p3) = (
            self.at,
            Vec2::new(cx0, cy0),
            Vec2::new(cx1, cy1),
            Vec2::new(x, y),
        );
        for i in 1..=PIECES {
            let t = i as f32 / PIECES as f32;
            let u = 1.0 - t;
            self.to(
                p0 * (u * u * u) + p1 * (3.0 * u * u * t) + p2 * (3.0 * u * t * t) + p3 * (t * t * t),
            );
        }
    }

    fn close(&mut self) {
        let start = self.start;
        self.to(start);
    }
}

/// How many lines each row of dots is looked at along. Across, a span's ends
/// are measured exactly; down, sixteen looks are sixteen shades.
const LOOKS: usize = 16;

/// Fill closed edges into a mask by the non-zero rule, with one clear dot all
/// round.
fn fill(edges: &[(Vec2, Vec2)]) -> Mask {
    let (mut low, mut high) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
    for (a, b) in edges {
        low = low.min(a.min(*b));
        high = high.max(a.max(*b));
    }
    fill_within(edges, low, high)
}

/// The same between `low` and `high`, wherever the edges reach: ink outside
/// is lost.
fn fill_within(edges: &[(Vec2, Vec2)], low: Vec2, high: Vec2) -> Mask {
    let shift = Vec2::splat(1.0) - low;
    let width = (high.x - low.x).ceil() as usize + 2;
    let height = (high.y - low.y).ceil() as usize + 2;
    let mut light = vec![0.0f32; width * height];
    let mut crossings: Vec<(f32, i32)> = Vec::new();
    for row in 0..height {
        for look in 0..LOOKS {
            let y = row as f32 + (look as f32 + 0.5) / LOOKS as f32;
            crossings.clear();
            for (a, b) in edges {
                let (a, b) = (*a + shift, *b + shift);
                if (a.y <= y) == (b.y <= y) {
                    continue;
                }
                let x = a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x);
                crossings.push((x, if b.y > a.y { 1 } else { -1 }));
            }
            crossings.sort_by(|p, q| p.0.total_cmp(&q.0));
            let mut winding = 0;
            let mut from = 0.0;
            for &(x, turn) in &crossings {
                let was = winding;
                winding += turn;
                if was == 0 && winding != 0 {
                    from = x;
                } else if was != 0 && winding == 0 {
                    span(&mut light[row * width..(row + 1) * width], from, x);
                }
            }
        }
    }
    let cover = light
        .iter()
        .map(|l| ((l / LOOKS as f32).clamp(0.0, 1.0) * 255.0).round() as u8)
        .collect();
    Mask { width, height, cover }
}

/// Add one look's worth of a span from `from` to `to` to a row of dots.
fn span(row: &mut [f32], from: f32, to: f32) {
    let from = from.max(0.0);
    let to = to.min(row.len() as f32);
    if to <= from {
        return;
    }
    let (first, last) = (from.floor() as usize, (to.ceil() as usize).min(row.len()));
    for (x, dot) in row.iter_mut().enumerate().take(last).skip(first) {
        let left = from.max(x as f32);
        let right = to.min(x as f32 + 1.0);
        if right > left {
            *dot += right - left;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(from: f32, to: f32) -> Vec<(Vec2, Vec2)> {
        let p = |x, y| Vec2::new(x, y);
        vec![
            (p(from, from), p(to, from)),
            (p(to, from), p(to, to)),
            (p(to, to), p(from, to)),
            (p(from, to), p(from, from)),
        ]
    }

    #[test]
    fn a_square_fills_its_dots_and_no_others() {
        let mask = fill(&square(0.0, 4.0));
        assert_eq!((mask.width, mask.height), (6, 6), "one clear dot all round");
        for y in 0..6 {
            for x in 0..6 {
                let inside = (1..5).contains(&x) && (1..5).contains(&y);
                assert_eq!(mask.cover[y * 6 + x], if inside { 255 } else { 0 }, "{x},{y}");
            }
        }
    }

    #[test]
    fn half_a_dot_is_half_covered() {
        let mask = fill(&square(0.5, 3.0));
        let row = &mask.cover[2 * mask.width..3 * mask.width];
        assert_eq!(&row[..3], &[0, 255, 255], "{row:?}");
        assert!((120..=136).contains(&row[3]), "{row:?}");
    }

    #[test]
    fn a_hole_stays_open() {
        let mut ring = square(0.0, 8.0);
        let inner: Vec<_> = square(2.0, 6.0).into_iter().map(|(a, b)| (b, a)).collect();
        ring.extend(inner);
        let mask = fill(&ring);
        let at = |x: usize, y: usize| mask.cover[y * mask.width + x];
        assert_eq!(at(1, 1), 255);
        assert_eq!(at(5, 5), 0, "the middle is the hole");
    }

    /// A face that has all five, where one is installed: DejaVu has the
    /// arrows and the circled one, though no second repeat.
    fn some_face() -> Option<Vec<u8>> {
        [
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/dejavu/DejaVuSans.ttf",
        ]
        .iter()
        .find_map(|p| std::fs::read(p).ok())
    }

    #[test]
    fn every_mark_comes_out_of_a_face_that_has_it() {
        let Some(face) = some_face() else { return };
        for mark in [Mark::Back, Mark::On, Mark::Shuffle, Mark::Repeat, Mark::RepeatOne] {
            let mask = draw(&face, mark, 25.0).unwrap_or_else(|| panic!("{mark:?}"));
            assert!(mask.cover.contains(&255), "{mark:?} has ink");
            assert!(mask.width < 40 && mask.height < 40, "{mark:?} {mask:?}");
        }
    }

    #[test]
    fn a_turn_lays_a_tall_mark_down() {
        let mut tall = square(0.0, 2.0);
        for edge in &mut tall {
            edge.0.y *= 5.0;
            edge.1.y *= 5.0;
        }
        let before = fill(&tall);
        turn(&mut tall);
        let after = fill(&tall);
        assert_eq!((before.width, before.height), (after.height, after.width));
    }

    #[test]
    fn only_the_shuffle_is_turned() {
        for mark in [Mark::Back, Mark::On, Mark::Repeat, Mark::RepeatOne] {
            assert!(!mark.turned(), "{mark:?}");
        }
        assert!(Mark::Shuffle.turned());
    }

    /// An arrow drawn as a face draws its `→` (font units): the shape
    /// [`Pen::arrow`] has to give back when it is measured and drawn again.
    fn drawn_arrow() -> Vec<(Vec2, Vec2)> {
        let p = |x: f32, y: f32| Vec2::new(x, y);
        closed(&[
            p(500.0, 50.0),
            p(800.0, 350.0),
            p(500.0, 650.0),
            p(430.0, 580.0),
            p(610.0, 400.0),
            p(40.0, 400.0),
            p(40.0, 300.0),
            p(610.0, 300.0),
            p(430.0, 120.0),
        ])
    }

    #[test]
    fn the_arrow_is_measured_and_drawn_again_as_it_was() {
        let pen = Pen::measure(&drawn_arrow()).expect("a stem and a head");
        assert!((pen.stem - 100.0).abs() < 0.5, "{pen:?}");
        assert!((pen.arm - 140.0).abs() < 0.5, "{pen:?}");
        assert!((pen.head - 300.0).abs() < 0.5, "{pen:?}");
        let again = pen.arrow(pen.tail, pen.tip, pen.middle, pen.head);
        let wanted = [
            (500.0, 50.0),
            (800.0, 350.0),
            (500.0, 650.0),
            (430.0, 580.0),
            (610.0, 400.0),
            (40.0, 400.0),
            (40.0, 300.0),
            (610.0, 300.0),
            (430.0, 120.0),
        ];
        for (got, (x, y)) in again.iter().zip(wanted) {
            assert!((got.x - x).abs() < 0.5 && (got.y - y).abs() < 0.5, "{got:?} ≠ {x},{y}");
        }
    }

    #[test]
    fn a_filled_triangle_is_not_measured() {
        let p = |x: f32, y: f32| Vec2::new(x, y);
        let triangle = closed(&[p(0.0, 0.0), p(100.0, 50.0), p(0.0, 100.0)]);
        assert_eq!(Pen::measure(&triangle), None);
    }

    /// Both built marks come out with ink, wound one way, about as tall as
    /// the arrow and no wider than it plus its head.
    #[test]
    fn the_shuffle_and_the_repeat_are_built_out_of_the_arrow() {
        let arrow = drawn_arrow();
        let pen = Pen::measure(&arrow).unwrap();
        for shapes in [pen.shuffle(), pen.repeat().concat()] {
            let winding = area(&arrow).signum();
            let edges: Vec<_> = shapes
                .into_iter()
                .flat_map(|mut s| {
                    if area_of(&s).signum() != winding {
                        s.reverse();
                    }
                    closed(&s)
                })
                .collect();
            let (low, high) = bounds(&edges);
            assert!(high.y - low.y < pen.height() * 1.25, "{low:?} {high:?}");
            assert!(high.y - low.y > pen.head * 2.0 * 0.9, "{low:?} {high:?}");
            let scaled: Vec<_> = edges.iter().map(|(a, b)| (*a * 0.05, *b * 0.05)).collect();
            assert!(fill(&scaled).cover.contains(&255));
        }
    }

    /// Not a check: writes what the built marks look like at the size the
    /// window sets them, and a hundred times that, to look at.
    /// `cargo test -- --ignored built_marks_picture`
    #[test]
    #[ignore]
    fn built_marks_picture() {
        let pen = Pen::measure(&drawn_arrow()).unwrap();
        let winding = area(&drawn_arrow()).signum();
        for (name, shapes) in [("shuffle", pen.shuffle()), ("repeat", pen.repeat().concat()), ("arrow", vec![pen.arrow(pen.tail, pen.tip, pen.middle, pen.head)])] {
            for (size, scale) in [("big", 0.4f32), ("small", 25.0 / 1000.0)] {
                let edges: Vec<_> = shapes
                    .clone()
                    .into_iter()
                    .flat_map(|mut s| {
                        if area_of(&s).signum() != winding {
                            s.reverse();
                        }
                        closed(&s)
                    })
                    .map(|(a, b)| (Vec2::new(a.x, -a.y) * scale, Vec2::new(b.x, -b.y) * scale))
                    .collect();
                let mask = fill(&edges);
                let img = image::GrayImage::from_raw(
                    mask.width as u32,
                    mask.height as u32,
                    mask.cover.iter().map(|c| 255 - c).collect(),
                )
                .unwrap();
                img.save(std::env::temp_dir().join(format!("mark-{name}-{size}.png"))).unwrap();
            }
        }
    }

    /// A face with no arrow at all still gets all five marks, out of its
    /// hyphen — Barlow's bar, as it is in the font (Regular, font units,
    /// without its rounded corners) — and every one of them has ink.
    #[test]
    fn the_marks_come_out_of_a_hyphen() {
        let p = |x: f32, y: f32| Vec2::new(x, y);
        let hyphen = closed(&[p(66.0, 280.0), p(383.0, 280.0), p(383.0, 342.0), p(66.0, 342.0)]);
        let pen = Pen::from_hyphen(&hyphen, 1000.0).expect("a pen");
        assert!((pen.stem - 62.0).abs() < 0.5, "{pen:?}");
        assert!((pen.middle - 311.0).abs() < 0.5, "{pen:?}");
        assert!(pen.round > 0.0, "a pen made up from a hyphen softens its corners");
        let winding = area(&hyphen).signum();
        let all = [
            vec![pen.arrow(pen.tip, pen.tail, pen.middle, pen.head)],
            vec![pen.arrow(pen.tail, pen.tip, pen.middle, pen.head)],
            pen.shuffle(),
            pen.repeat().concat(),
            [pen.one(), vec![pen.ring()]].concat(),
        ];
        for shapes in all {
            let edges: Vec<_> = shapes
                .iter()
                .map(|s| rounded(s, pen.round))
                .flat_map(|mut s| {
                    if area_of(&s).signum() != winding {
                        s.reverse();
                    }
                    closed(&s)
                })
                .map(|(a, b)| (a * 0.03, b * 0.03))
                .collect();
            assert!(fill(&edges).cover.contains(&255));
        }
    }

    /// Softening a corner takes the point off it and keeps the shape where
    /// it was.
    #[test]
    fn a_rounded_square_loses_its_corners_only() {
        let p = |x: f32, y: f32| Vec2::new(x, y);
        let square = [p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0), p(0.0, 10.0)];
        let soft = rounded(&square, 2.0);
        assert!(!soft.contains(&p(0.0, 0.0)), "the corner is still there");
        let (low, high) = bounds(&closed(&soft));
        assert!(low.x >= -0.01 && low.y >= -0.01 && high.x <= 10.01 && high.y <= 10.01);
        assert!(soft.contains(&p(2.0, 0.0)) && soft.contains(&p(10.0, 8.0)));
    }

    #[test]
    fn not_a_font_is_nothing() {
        assert_eq!(draw(b"not a font", Mark::Back, 20.0), None);
    }
}
