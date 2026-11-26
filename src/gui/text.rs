//! Laying out a line of text under the design system's rules.
//!
//! Everything drawn goes through here, so lowercasing and label tracking
//! cannot be forgotten at a call site.

use std::sync::Arc;

use eframe::egui::{self, text::LayoutJob, Color32, Galley, TextFormat};

use super::theme::{lower, styled, Role};

/// Lay out one line: lowercased, in its role's face, tracked if it is a label.
pub fn line(ui: &egui::Ui, text: &str, role: Role, color: Color32) -> Arc<Galley> {
    line_tracked(ui, text, role, color, 0.0)
}

/// The same with `extra` points more between the letters than the role keeps —
/// the menu's head.
pub fn line_tracked(ui: &egui::Ui, text: &str, role: Role, color: Color32, extra: f32) -> Arc<Galley> {
    let mut job = LayoutJob::default();
    job.append(
        &styled(&lower(text), role),
        0.0,
        TextFormat {
            font_id: role.font(),
            color,
            extra_letter_spacing: role.tracking() + extra,
            ..Default::default()
        },
    );
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    ui.ctx().fonts_mut(|f| f.layout_job(job))
}

/// A name between regular and bold — `bold` from 0 to 1 — for one that is
/// turning bold or turning back. Only a face with a weight axis has anything
/// between the two; any other is regular below the middle and bold above it.
/// Never truncated, as [`full_line`].
///
/// The weight is taken in twelfths: each weight is a set of glyphs of its
/// own for egui to draw and keep, and twelve of them are as smooth as the
/// eye can tell over a tenth of a second.
pub fn full_line_weighted(ui: &egui::Ui, text: &str, bold: f32, color: Color32) -> Arc<Galley> {
    const STEPS: f32 = 12.0;
    let bold = (bold.clamp(0.0, 1.0) * STEPS).round() / STEPS;
    let Some((regular, heavy)) = super::theme::weights().filter(|_| bold > 0.0 && bold < 1.0) else {
        let role = if bold >= 0.5 { Role::ContentBold } else { Role::Content };
        return full_line(ui, text, role, color);
    };
    let mut job = LayoutJob::default();
    job.append(
        &styled(&lower(text), Role::Content),
        0.0,
        TextFormat {
            font_id: Role::Content.font(),
            color,
            extra_letter_spacing: Role::Content.tracking(),
            coords: egui::epaint::text::VariationCoords::new([(b"wght", regular + (heavy - regular) * bold)]),
            ..Default::default()
        },
    );
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.wrap.max_width = f32::INFINITY;
    ui.ctx().fonts_mut(|f| f.layout_job(job))
}

/// Every letter the window is likely to write, laid out once in each role's
/// face, so the glyphs are drawn into the font texture in one frame rather
/// than one size and letter at a time as lists scroll by. Each time the
/// texture fills it grows and goes to the graphics card again whole, and
/// that was the window standing still in its first half minute. Asked for
/// again where the face or the screen's scale changes.
pub fn prewarm(ctx: &egui::Context, capitals: bool) {
    const SAMPLE: &str = "abcdefghijklmnopqrstuvwxyzäöüßéèêàáâçñøåæœ0123456789 .,:;!?'\"„“”‘’»«()[]{}/\\|-–—_·•…→←↑↓&+=%#@*~^°";
    let roles = [
        Role::Content,
        Role::ContentBold,
        Role::Figures,
        Role::FiguresLabel,
        Role::Label,
        Role::Note,
        Role::LabelBold,
    ];
    let mut samples = vec![lower(SAMPLE)];
    if capitals {
        samples.push(SAMPLE.to_uppercase());
    }
    ctx.fonts_mut(|fonts| {
        for role in roles {
            for sample in &samples {
                let mut job = LayoutJob::default();
                job.append(
                    &styled(sample, role),
                    0.0,
                    TextFormat { font_id: role.font(), extra_letter_spacing: role.tracking(), ..Default::default() },
                );
                job.wrap.max_rows = 1;
                job.wrap.max_width = f32::INFINITY;
                let _ = fonts.layout_job(job);
            }
        }
    });
}

/// The same, but never truncated — the caller clips it.
///
/// This is what a line that scrolls is laid out with: truncating it first would
/// leave nothing to scroll to.
pub fn full_line(ui: &egui::Ui, text: &str, role: Role, color: Color32) -> Arc<Galley> {
    let mut job = LayoutJob::default();
    job.append(
        &styled(&lower(text), role),
        0.0,
        TextFormat {
            font_id: role.font(),
            color,
            extra_letter_spacing: role.tracking(),
            ..Default::default()
        },
    );
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.wrap.max_width = f32::INFINITY;
    ui.ctx().fonts_mut(|f| f.layout_job(job))
}

/// A line with its figures in the mono cut and its words in the label face —
/// the foot of the compact window, where „0:39" counts down every second and
/// „83 titel" sits beside it. In the proportional face a countdown changes
/// width as it goes and everything set against it moves; in the mono cut every
/// figure is as wide as every other.
///
/// Two faces in one galley line up by the bottom of their boxes, which is
/// not where their letters stand (see [`rich_line`]), so each run is its own
/// galley, set on the words' baseline.
pub struct Figured {
    runs: Vec<(Arc<Galley>, f32, f32)>,
    pub width: f32,
}

impl Figured {
    pub fn new(ui: &egui::Ui, text: &str, words: Role, figures: Role, color: Color32) -> Self {
        let chars: Vec<char> = text.chars().collect();
        let is_figure = |i: usize| {
            let c = chars[i];
            if c.is_ascii_digit() {
                return true;
            }
            matches!(c, ':' | '.' | ',')
                && i > 0
                && chars[i - 1].is_ascii_digit()
                && chars.get(i + 1).is_some_and(|n| n.is_ascii_digit())
        };
        let mut pieces: Vec<(String, bool)> = Vec::new();
        for (i, c) in chars.iter().enumerate() {
            let figure = is_figure(i);
            match pieces.last_mut() {
                Some((run, kind)) if *kind == figure => run.push(*c),
                _ => pieces.push((c.to_string(), figure)),
            }
        }
        let reference = baseline(&line(ui, "x", words, color));
        let mut runs = Vec::with_capacity(pieces.len());
        let mut x = 0.0;
        for (run, figure) in pieces {
            let g = full_line(ui, &run, if figure { figures } else { words }, color);
            let lift = reference - baseline(&g);
            let w = g.size().x;
            runs.push((g, x, lift));
            x += w;
        }
        Figured { runs, width: x }
    }

    /// Paint it with the words' box at `at`.
    pub fn paint(&self, painter: &egui::Painter, at: egui::Pos2, color: Color32) {
        for (g, x, lift) in &self.runs {
            painter.galley(egui::pos2(at.x + x, at.y + lift), g.clone(), color);
        }
    }
}

/// The air between two runs of one line.
pub const RUN_GAP: f32 = 14.0;

/// Several runs of text, differently set, that read as one line.
///
/// The now-playing line is a title, an artist and an album at three tonal
/// values — and the title in the bold cut, which is where this stopped being
/// one galley. egui lays the runs of a job out by the *bottom* of their boxes
/// (`TextFormat::valign` defaults to `Align::BOTTOM`), and two cuts of one
/// family no longer agree about where the bottom of a box is: the bold title
/// hung two and a half points below the artist beside it.
///
/// There is no setting for "line them up on their baselines" — the choice is
/// between top, centre and bottom of the box — so they come back as separate
/// galleys and are set on the row's own line by [`row_line`]. They still
/// scroll as one thing: one offset moves all of them, which is what made a
/// single galley worth having in the first place.
pub fn rich_line(ui: &egui::Ui, runs: &[(&str, Role, Color32)]) -> Vec<(Arc<Galley>, Color32)> {
    runs.iter()
        .filter(|(text, _, _)| !text.is_empty())
        .map(|(text, role, color)| (full_line(ui, text, *role, *color), *color))
        .collect()
}

/// How wide those runs are, set side by side with the air between them.
pub fn runs_width(runs: &[(Arc<Galley>, Color32)]) -> f32 {
    let ink: f32 = runs.iter().map(|(g, _)| g.size().x).sum();
    ink + RUN_GAP * runs.len().saturating_sub(1) as f32
}

/// Where a galley's first baseline sits inside it.
///
/// Two lines of type sit on one line when their *baselines* agree, and nothing
/// else about them has to. Two galleys drawn at the same top do not: egui
/// lines runs up by the bottom of their boxes, so the moment two faces
/// disagree about ascent and descent — which two cuts of one family readily
/// do, and which swapping a face for another of the same family will change
/// under you without a word — one of them drops.
///
/// „vol" and its figure did exactly that, by two and a half points, the day
/// the regular cut was swapped. Asked of the galley rather than reasoned
/// about: egui records each glyph's baseline, and that is the answer.
///
/// The line's own face's baseline, even where the first glyph is from the
/// fallback. egui puts a fallback glyph's baseline half way between the two
/// faces' rows, and `theme::baseline_nudge` moves its ink from there onto the
/// face's line — but only the ink: the recorded position stays where egui put
/// it. Reading that position off a line that begins in Japanese stood the line
/// by the fallback's baseline instead, undoing the nudge, by an amount that
/// differs between the regular and the bold cut wherever their metrics differ —
/// so a Japanese title jumped when its row was clicked into bold, in a face
/// whose two cuts disagree. Taken back to the face's own ascent
/// here, which is where a glyph of the face itself stands.
pub fn last_baseline(galley: &Galley) -> f32 {
    galley
        .rows
        .last()
        .and_then(|placed| {
            placed.row.glyphs.first().map(|g| {
                let from_fallback = g.font_ascent - g.font_face_ascent - 0.5 * (g.font_height - g.font_face_height);
                placed.pos.y + g.pos.y + from_fallback
            })
        })
        .unwrap_or(0.0)
}

/// From a galley's top to the baseline of its first line.
pub fn baseline(galley: &Galley) -> f32 {
    galley
        .rows
        .first()
        .and_then(|placed| {
            placed.row.glyphs.first().map(|g| {
                let from_fallback = g.font_ascent - g.font_face_ascent - 0.5 * (g.font_height - g.font_face_height);
                placed.pos.y + g.pos.y + from_fallback
            })
        })
        .unwrap_or(0.0)
}

/// The same, clipped to a column width.
pub fn line_within(
    ui: &egui::Ui,
    text: &str,
    role: Role,
    color: Color32,
    max_width: f32,
) -> Arc<Galley> {
    let mut job = LayoutJob::default();
    job.append(
        &styled(&lower(text), role),
        0.0,
        TextFormat {
            font_id: role.font(),
            color,
            extra_letter_spacing: role.tracking(),
            ..Default::default()
        },
    );
    job.wrap.max_width = max_width.max(1.0);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    ui.ctx().fonts_mut(|f| f.layout_job(job))
}

/// The same, but as it was written: capitals kept.
///
/// For the one place in the window where the case of a letter is the point —
/// the tag editor, where what is shown is what will be written into the file.
/// Lowercased there, „Björk" and „björk" look the same and a change of case is
/// invisible; and the caret, measured on lowercase letters, stood in the wrong
/// place as soon as a word had a capital in it.
pub fn cased_within(
    ui: &egui::Ui,
    text: &str,
    role: Role,
    color: Color32,
    max_width: f32,
) -> Arc<Galley> {
    let mut job = LayoutJob::default();
    job.append(
        text,
        0.0,
        TextFormat {
            font_id: role.font(),
            color,
            extra_letter_spacing: role.tracking(),
            ..Default::default()
        },
    );
    job.wrap.max_width = max_width.max(1.0);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    ui.ctx().fonts_mut(|f| f.layout_job(job))
}

/// How far apart the lines of prose stand, as a share of the size:
/// Asap brings 1.146 (its ascender and descender, no gap of its own).
pub const PARAGRAPH_LEADING: f32 = 1.32;

/// The same for the small face, a little more open: at the label's size, set
/// as tight as the reading size's prose, its lines ran into one another in a
/// podcast's few sentences.
pub const SMALL_LEADING: f32 = 1.5;

/// How far apart lines of the small face stand when they follow one another:
/// its size at [`SMALL_LEADING`], as its prose is set. The one step for every
/// block of small lines — a station's facts, a podcast's, the narrow window's
/// names, what stands under a podcast's or an episode's name.
pub const SMALL_LINE: f32 = super::theme::SIZE_LABEL * SMALL_LEADING;

/// Text that wraps onto as many lines as it needs.
///
/// The one place in the window where a line is allowed to become two: the list
/// of changes is prose, and prose cut off at the column edge with an ellipsis
/// is not a changelog. Everywhere else the single-line rule holds, because a
/// list whose rows are different heights has lost its rhythm.
pub fn paragraph(
    ui: &egui::Ui,
    text: &str,
    role: Role,
    color: Color32,
    max_width: f32,
) -> Arc<Galley> {
    wrapped(ui, text, role, color, max_width, None)
}

/// Prose to be read — what the information columns say about an artist, a
/// station, a show — with a little more air between its lines than the face
/// brings ([`PARAGRAPH_LEADING`] times the size): set as tight as a row, it
/// read as a block.
pub fn prose(ui: &egui::Ui, text: &str, role: Role, color: Color32, max_width: f32) -> Arc<Galley> {
    let leading = if role.font().size <= super::theme::SIZE_LABEL { SMALL_LEADING } else { PARAGRAPH_LEADING };
    wrapped(ui, text, role, color, max_width, Some(leading))
}

/// [`prose`] in parts of their own colours — what can be clicked in a text
/// among what is only read — and where each part stands in the galley, in
/// characters, to tell which the pointer is on.
pub fn prose_parts(ui: &egui::Ui, parts: &[(&str, Color32)], role: Role, max_width: f32) -> (Arc<Galley>, Vec<std::ops::Range<usize>>) {
    let font_id = role.font();
    let leading = if font_id.size <= super::theme::SIZE_LABEL { SMALL_LEADING } else { PARAGRAPH_LEADING };
    let mut job = LayoutJob::default();
    let mut ranges = Vec::with_capacity(parts.len());
    let mut at = 0;
    for (text, color) in parts {
        let small = lower(text);
        let shown = styled(&small, role);
        let count = shown.chars().count();
        ranges.push(at..at + count);
        at += count;
        job.append(
            &shown,
            0.0,
            TextFormat {
                font_id: font_id.clone(),
                color: *color,
                extra_letter_spacing: role.tracking(),
                line_height: Some(font_id.size * leading),
                ..Default::default()
            },
        );
    }
    job.wrap.max_width = max_width.max(1.0);
    (ui.ctx().fonts_mut(|f| f.layout_job(job)), ranges)
}

/// Words in a role's face at a size of their own, wrapped — the screen
/// saver's lyrics, as big as the window allows. `size` is taken in whole
/// steps of four points: each size is a set of glyphs of its own for egui to
/// keep, and a window being resized would otherwise leave dozens.
pub fn sized(ui: &egui::Ui, text: &str, role: Role, size: f32, color: Color32, max_width: f32) -> Arc<Galley> {
    sized_parts(ui, &[(text, color)], role, size, max_width)
}

/// The same in parts of their own colours — the words of a line already sung,
/// and the rest.
pub fn sized_parts(ui: &egui::Ui, parts: &[(&str, Color32)], role: Role, size: f32, max_width: f32) -> Arc<Galley> {
    let mut font_id = role.font();
    font_id.size = ((size / 4.0).round() * 4.0).max(8.0);
    let mut job = LayoutJob::default();
    for (text, color) in parts {
        job.append(
            &styled(&lower(text), role),
            0.0,
            TextFormat {
                line_height: Some(font_id.size * PARAGRAPH_LEADING),
                font_id: font_id.clone(),
                color: *color,
                ..Default::default()
            },
        );
    }
    job.wrap.max_width = max_width.max(1.0);
    job.halign = egui::Align::Center;
    ui.ctx().fonts_mut(|f| f.layout_job(job))
}

fn wrapped(
    ui: &egui::Ui,
    text: &str,
    role: Role,
    color: Color32,
    max_width: f32,
    leading: Option<f32>,
) -> Arc<Galley> {
    let mut job = LayoutJob::default();
    let font_id = role.font();
    let line_height = leading.map(|leading| font_id.size * leading);
    job.append(
        &styled(&lower(text), role),
        0.0,
        TextFormat {
            font_id,
            color,
            extra_letter_spacing: role.tracking(),
            line_height,
            ..Default::default()
        },
    );
    job.wrap.max_width = max_width.max(1.0);
    ui.ctx().fonts_mut(|f| f.layout_job(job))
}

/// A word shaped like the words this window is full of: lowercase, some of it
/// reaching to the ascender, none of it below the baseline.
///
/// Not "xnmoe", and not "xnmoebdfhkljgpy". The first is all x-height and the
/// second reaches down into the descender, and each of them centres a *part*
/// of a line rather than a line. This is what a word actually looks like — a
/// block of x-height with a few strokes standing up out of it — and the room
/// a marker keeps below the baseline is room it has to keep for the g in
/// "guru" whether the word in front of you has one or not.
const LOWERCASE_PROBE: &str = "xnmoebdfhkl";

/// How far a row's type has to be lifted to sit optically in the middle.
///
/// Centring a galley centres its *box*, which runs from the top of the
/// ascender space down to the bottom of the descender space — more room than
/// lowercase type ever uses at the top, so the word hangs low in its row. On
/// the one row with a marker behind it, that is plain to see.
///
/// The correction is measured off the face rather than guessed at: the probe
/// is laid out and asked where its ink really landed. That matters more than
/// it sounds — a correction measured on DejaVu does not fit another face. A
/// different typeface, or the mono cut, needs nothing new written here.
///
/// There is no dial and no fraction in it. A version of this did have one,
/// set halfway between "centre the x-height" and "centre down to the
/// baseline" — which is the same answer this arrives at by measuring a word
/// that has both, and a number somebody has to argue about is worse than a
/// word anybody can read.
///
/// It is one lift for every role, not one per role: it takes the whole row up
/// by the same amount and so leaves the arrangement inside the row alone.
/// Which face each piece of that row is set in is a separate question, and
/// [`row_line`] answers it.
pub fn x_height_lift(ui: &egui::Ui, height: f32) -> f32 {
    let galley = line(ui, LOWERCASE_PROBE, Role::Content, Color32::WHITE);
    let ink = galley.mesh_bounds;
    if !ink.height().is_finite() || ink.height() <= 0.0 {
        return 0.0;
    }
    let limit = height / 4.0;
    (ink.center().y - galley.size().y / 2.0).clamp(-limit, limit)
}

/// The one line a row of this height is written on, counted from its top.
///
/// A row is not a stack of boxes, it is a line of type: a title, the artist
/// beside it, the running time at the end, and on the row being played the
/// same title in the bold cut. They belong on one line, and two pieces of type
/// are on one line when their *baselines* agree.
///
/// Setting each piece by its box instead only ever worked by luck, and the
/// luck ran out with the typeface. Measured in the running window at fifteen
/// point, the box and the baseline within it:
///
/// | role         | box  | baseline |
/// |--------------|------|----------|
/// | content      | 19.5 | 12.0     |
/// | content bold | 21.0 | 16.0     |
/// | figures      | 20.0 | 16.0     |
/// | label        | 14.0 | 8.5      |
///
/// The regular carries seven and a half points below its baseline where the
/// bold carries five and the mono four: the Standard cut asks to be measured
/// by its typographic metrics, and those declare a line gap of 290/1000 that
/// lands under the line. Centre those three boxes in one row and you get three
/// lines, three and a half points apart. Cuts that happen to agree to the
/// unit hide it until the day one of them is swapped.
///
/// So the row is given one line, taken from the face it is read in, and
/// everything in it is set on that line by [`on_line`]. Where that line falls
/// is unchanged from before: the content role, centred and lifted, exactly
/// where it has always sat. It is the figures and the bold that come to it.
pub fn row_line(ui: &egui::Ui, height: f32, role: Role) -> f32 {
    let probe = line(ui, LOWERCASE_PROBE, role, Color32::WHITE);
    (height - probe.size().y) / 2.0 - x_height_lift(ui, height) + baseline(&probe)
}

/// Where to put a galley set in `role` at the end of a row whose words are
/// at content size. A word at content size stands on the row's line, as
/// everything in a column does; a smaller one — a count, an action word
/// such as „merken", a station's facts — has its ink on the middle of the
/// content's ink instead: on the row's line it sat low beside the name.
pub fn beside_content(ui: &egui::Ui, galley: &Galley, role: Role, top: f32, height: f32) -> f32 {
    let small = matches!(role, Role::Label | Role::LabelBold | Role::Note | Role::FiguresLabel);
    let line = row_line(ui, height, Role::Content);
    if !small {
        return on_line(galley, top, line);
    }
    let probe = self::line(ui, LOWERCASE_PROBE, Role::Content, Color32::WHITE);
    let middle = ink_middle(ui, Role::Content, on_line(&probe, top, line));
    middle - ink_middle(ui, role, 0.0)
}

/// The line to set type on so that it sits optically in the middle of a box.
///
/// A row and a box are two different problems. A row is one of many, and what
/// makes a column of them read evenly is that every row keeps the same room
/// under the baseline for the descenders some of them have — that is
/// [`row_line`]. A box with a fill behind it is on its own, and what the eye
/// checks is the word against the two edges of the fill, so here the *ink* is
/// centred and nothing is kept in reserve.
///
/// The search field was set by the row's rule and sat a point and a half low
/// against its own fill.
pub fn line_in_box(ui: &egui::Ui, height: f32, role: Role) -> f32 {
    let probe = line(ui, LOWERCASE_PROBE, role, Color32::WHITE);
    let ink = probe.mesh_bounds;
    if !ink.height().is_finite() || ink.height() <= 0.0 {
        return row_line(ui, height, role);
    }
    height / 2.0 - ink.center().y + baseline(&probe)
}

/// Where to set a galley so that its ink sits on a given middle.
///
/// For a figure beside a two-point rule this is the whole question: the eye
/// puts the number on the line, and the line runs through the middle of the
/// digits — not through the middle of a box that is keeping room for a „g"
/// the number does not have.
pub fn ink_on(galley: &Galley, middle: f32) -> f32 {
    let ink = galley.mesh_bounds;
    if !ink.height().is_finite() || ink.height() <= 0.0 {
        return middle - galley.size().y / 2.0;
    }
    middle - ink.center().y
}

/// What a galley's ink actually measures, for a plate cut to fit it.
pub fn ink_size(galley: &Galley) -> egui::Vec2 {
    let ink = galley.mesh_bounds;
    if !ink.height().is_finite() || ink.height() <= 0.0 || ink.width() <= 0.0 {
        return galley.size();
    }
    ink.size()
}

/// How far the top of a lowercase letter stands above the baseline in this
/// role: the x-height, taken off the face with an „x".
pub fn x_rise(ui: &egui::Ui, role: Role) -> f32 {
    let probe = line(ui, "x", role, Color32::WHITE);
    let ink = probe.mesh_bounds;
    if !ink.height().is_finite() || ink.height() <= 0.0 {
        return probe.size().y / 2.0;
    }
    baseline(&probe) - ink.top()
}

/// Where to put a galley set in `role` so that the middle of its x-height
/// stands at `middle`: for a word that comes up on a plate under the pointer
/// — „entfernen", „kopieren" — which is read as sitting in the middle of its
/// plate when its small letters do, not its box or its ascenders.
pub fn on_x_middle(ui: &egui::Ui, galley: &Galley, role: Role, middle: f32) -> f32 {
    middle - baseline(galley) + x_rise(ui, role) / 2.0
}

/// Where a block of lines beside a picture has to start for the eye to see
/// it in the middle of the picture's height, between `top` and `bottom`: as
/// much room from the picture's top edge to the x-height of the first line as
/// from the baseline of the last to the picture's bottom edge. The box of the
/// type is not what is seen — it has room for capitals and ascenders above
/// and for descenders below, and centred that way the words sat off the
/// middle. `x_top` and `last_baseline` are counted from the block's top.
pub fn optically_beside(top: f32, bottom: f32, x_top: f32, last_baseline: f32) -> f32 {
    (top + bottom - x_top - last_baseline) / 2.0
}

/// Where the eye finds the middle of a line of type drawn with its box top at
/// `top`.
///
/// Not the middle of the box: a box runs from the ascender space down to the
/// descender space, and its middle sits below the word. What a rule set beside
/// a word has to agree with is the *word* — so this is the middle of the ink,
/// measured off the face with the same probe [`x_height_lift`] uses.
pub fn ink_middle(ui: &egui::Ui, role: Role, top: f32) -> f32 {
    let probe = line(ui, LOWERCASE_PROBE, role, Color32::WHITE);
    let ink = probe.mesh_bounds;
    if !ink.height().is_finite() || ink.height() <= 0.0 {
        return top + probe.size().y / 2.0;
    }
    top + ink.center().y
}

/// How tall the ink of a word in this face actually stands.
///
/// The band the words of this window occupy — ascender down to baseline, taken
/// off the face with the same probe as everything else here. Not the box,
/// which runs from above the ascender to below the descender and, in the
/// Standard cut, a line gap below that again.
///
/// A plate drawn around the box is therefore not a plate around the word: it
/// keeps room the word does not use, all of it at the bottom, and so hangs
/// below it. See `library::control_plate`, which is where that showed.
pub fn ink_height(ui: &egui::Ui, role: Role) -> f32 {
    let probe = line(ui, LOWERCASE_PROBE, role, Color32::WHITE);
    let ink = probe.mesh_bounds;
    if !ink.height().is_finite() || ink.height() <= 0.0 {
        return probe.size().y;
    }
    ink.height()
}

/// Where a galley has to be put so that it stands on that line.
pub fn on_line(galley: &Galley, top: f32, line: f32) -> f32 {
    top + line - baseline(galley)
}

/// Format seconds as `m:ss`, or `h:mm:ss` past the hour.
pub fn duration(total_secs: u64) -> String {
    let h = total_secs / 3600;
    let m = (total_secs % 3600) / 60;
    let s = total_secs % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// A length too long for a clock: „45 min",. Two units at most — the third
/// would be a figure nobody reads.
pub fn long_duration(total_secs: u64) -> String {
    let minutes = total_secs / 60;
    let (days, hours, minutes) = (minutes / 1440, minutes / 60 % 24, minutes % 60);
    match (days, hours) {
        (0, 0) => tr!("{} min", minutes),
        (0, h) if minutes == 0 => tr!("{} std", h),
        (0, h) => tr!("{} std {} min", h, minutes),
        (1, 0) => tr!("1 tag").to_string(),
        (1, h) => tr!("1 tag {} std", h),
        (d, 0) => tr!("{} tage", d),
        (d, h) => tr!("{} tage {} std", d, h),
    }
}

/// A count, written as it is: „1139", no thousands mark. It was a thin space
/// until then, which Asap has not got: the space came out of the Japanese face
/// behind it, half an em wide, and „8 611" fell apart. Kept as one place to say
/// how a count is written.
pub fn count(n: u64) -> String {
    n.to_string()
}

/// A stored timestamp as a day worth reading.
///
/// The whole point of "zuletzt hinzugefügt" is the top of the list, so the top
/// of the list is where the words go: today, yesterday, and the few days after
/// that are what somebody is actually looking for. Further back a date is
/// shorter than a phrase and easier to compare, so it becomes one.
pub fn day(stamp: &str, today: chrono::NaiveDate) -> String {
    let Some(date) = stamp
        .get(..10)
        .and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
    else {
        return String::new();
    };
    match (today - date).num_days() {
        0 => tr!("heute").to_string(),
        1 => tr!("gestern").to_string(),
        days if days > 1 && days < 7 => tr!("vor {} tagen", days),
        _ => date.format("%d.%m.%y").to_string(),
    }
}

/// Where to put a galley so that its *ink* sits in the middle of a box.
///
/// Centring the galley is not the same thing, and for a single glyph it is
/// visibly not the same thing. The box runs from the ascender to the descender
/// whatever is in it, and it is as wide as the advance — so a mark that sits
/// high in the em, or narrow in its advance, comes out off-centre in both
/// directions at once. In a row of small buttons that reads as four glyphs
/// wandering about inside four identical plates.
///
/// [`x_height_lift`] is the answer to the same question for *words*, and the
/// wrong answer here: it lifts by the ink of a lowercase probe, because a row
/// of a list holds lowercase type. A guillemet is not lowercase, and `1` is a
/// figure — neither sits on the x-height, so lifting them by it moves them off
/// the middle rather than onto it.
///
/// So the glyph is laid out and asked where its ink really landed, and that is
/// what gets centred. Equal air on all four sides by construction, for every
/// glyph, in any typeface.
///
/// Measured off the screen at twice the scale, in the transport keys,
/// before and after: `«` had 19 pixels above it and 14 below — two and a half
/// points high — and now has 17 and 16, which is rounding. Sideways it was
/// already close, because the mono cut sets its glyphs fairly evenly in the
/// advance; it is the vertical that a lowercase correction ruins.
pub fn ink_centred(galley: &Galley, box_: egui::Rect) -> egui::Pos2 {
    let ink = galley.mesh_bounds;
    if !ink.width().is_finite() || !ink.height().is_finite() || ink.width() <= 0.0 {
        return box_.center() - galley.size() / 2.0;
    }
    box_.center() - ink.center().to_vec2()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A line that begins in the fallback stands on the same baseline as a
    /// line in the face itself, whatever the two faces' metrics.
    #[test]
    fn a_line_beginning_in_the_fallback_has_the_faces_baseline() {
        let read = |p: &str| std::fs::read(p).expect(p);
        let mut fonts = egui::FontDefinitions::empty();
        fonts.font_data.insert(
            "face".into(),
            std::sync::Arc::new(egui::FontData::from_owned(read(
                "fonts/Asap/Asap-Variable.ttf",
            ))),
        );
        fonts.font_data.insert(
            "fallback".into(),
            std::sync::Arc::new(
                egui::FontData::from_owned(read("fonts/M PLUS 1 Code/MPLUS1Code-Variable.ttf"))
                    .tweak(egui::FontTweak { y_offset_factor: 0.07, ..Default::default() }),
            ),
        );
        fonts
            .families
            .insert(egui::FontFamily::Proportional, vec!["face".into(), "fallback".into()]);
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts);
        ctx.run_ui(Default::default(), |_| {}).textures_delta.clear();
        let lay = |text: &str| {
            ctx.fonts_mut(|f| {
                f.layout_no_wrap(text.into(), egui::FontId::proportional(16.0), Color32::WHITE)
            })
        };
        let latin = baseline(&lay("abc"));
        assert!(latin > 0.0);
        assert!((baseline(&lay("坂本龍一")) - latin).abs() < 0.6);
        assert!((baseline(&lay("坂本 abc")) - latin).abs() < 0.6);
    }

    #[test]
    fn long_durations_keep_two_units() {
        assert_eq!(long_duration(0), "0 min");
        assert_eq!(long_duration(45 * 60 + 30), "45 min");
        assert_eq!(long_duration(3 * 3600), "3 std");
        assert_eq!(long_duration(6 * 3600 + 12 * 60), "6 std 12 min");
        assert_eq!(long_duration(86_400 + 59), "1 tag");
        assert_eq!(long_duration(86_400 + 5 * 3600 + 40 * 60), "1 tag 5 std");
        assert_eq!(long_duration(3 * 86_400 + 4 * 3600), "3 tage 4 std");
        assert_eq!(long_duration(10 * 86_400), "10 tage");
    }

    #[test]
    fn formats_durations() {
        assert_eq!(duration(0), "0:00");
        assert_eq!(duration(59), "0:59");
        assert_eq!(duration(291), "4:51");
        assert_eq!(duration(3600), "1:00:00");
        assert_eq!(duration(3661), "1:01:01");
    }

    #[test]
    fn writes_a_count_without_a_thousands_mark() {
        assert_eq!(count(7), "7");
        assert_eq!(count(142), "142");
        assert_eq!(count(2847), "2847");
        assert_eq!(count(1234567), "1234567");
    }

    #[test]
    fn recent_days_are_words_and_older_ones_are_dates() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 9, 11).unwrap();
        assert_eq!(day("2026-09-11 08:00:00", today), "heute");
        assert_eq!(day("2026-09-10 23:59:00", today), "gestern");
        assert_eq!(day("2026-09-08 12:00:00", today), "vor 3 tagen");
        assert_eq!(day("2026-09-05 12:00:00", today), "vor 6 tagen");
        assert_eq!(day("2026-09-04 12:00:00", today), "04.09.26");
        assert_eq!(day("2011-01-02 12:00:00", today), "02.01.11");
    }

    /// Nothing in, nothing out — an empty cell, never the word "invalid".
    #[test]
    fn an_unreadable_stamp_says_nothing() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 9, 11).unwrap();
        assert_eq!(day("", today), "");
        assert_eq!(day("irgendwas", today), "");
    }

    /// A clock or a file from the future must not read as "in three days".
    #[test]
    fn a_future_stamp_is_shown_as_a_date() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 9, 11).unwrap();
        assert_eq!(day("2027-01-01 00:00:00", today), "01.01.27");
    }

    /// The probe is the whole rule, so it has to keep its shape: x-height and
    /// ascenders, and nothing that reaches below the baseline. A stray g or y
    /// in it would quietly centre the descender space as well and put every
    /// row in the window a point too low again — which is the exact mistake
    /// this measurement exists to correct, and it would not show up anywhere
    /// but in somebody's eye.
    #[test]
    fn the_probe_reaches_up_but_not_down() {
        let has = |set: &str| LOWERCASE_PROBE.chars().any(|c| set.contains(c));
        assert!(has("acemnorsuvwxz"), "no x-height in the probe");
        assert!(has("bdfhklt"), "nothing reaching the ascender");
        assert!(!has("gjpqy"), "the probe reaches below the baseline");
        assert!(
            LOWERCASE_PROBE.chars().all(|c| c.is_ascii_lowercase()),
            "the probe has something in it that is not lowercase type"
        );
    }
}
