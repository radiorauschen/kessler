use std::sync::Arc;

use eframe::egui::{self, text::LayoutJob, Color32, Galley, TextFormat};

use super::theme::{lower, styled, Role};

pub fn line(ui: &egui::Ui, text: &str, role: Role, color: Color32) -> Arc<Galley> {
    line_tracked(ui, text, role, color, 0.0)
}

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

    pub fn paint(&self, painter: &egui::Painter, at: egui::Pos2, color: Color32) {
        for (g, x, lift) in &self.runs {
            painter.galley(egui::pos2(at.x + x, at.y + lift), g.clone(), color);
        }
    }
}

pub const RUN_GAP: f32 = 14.0;

pub fn rich_line(ui: &egui::Ui, runs: &[(&str, Role, Color32)]) -> Vec<(Arc<Galley>, Color32)> {
    runs.iter()
        .filter(|(text, _, _)| !text.is_empty())
        .map(|(text, role, color)| (full_line(ui, text, *role, *color), *color))
        .collect()
}

pub fn runs_width(runs: &[(Arc<Galley>, Color32)]) -> f32 {
    let ink: f32 = runs.iter().map(|(g, _)| g.size().x).sum();
    ink + RUN_GAP * runs.len().saturating_sub(1) as f32
}

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

pub const PARAGRAPH_LEADING: f32 = 1.32;

pub fn paragraph(
    ui: &egui::Ui,
    text: &str,
    role: Role,
    color: Color32,
    max_width: f32,
) -> Arc<Galley> {
    wrapped(ui, text, role, color, max_width, None)
}

pub fn prose(ui: &egui::Ui, text: &str, role: Role, color: Color32, max_width: f32) -> Arc<Galley> {
    wrapped(ui, text, role, color, max_width, Some(PARAGRAPH_LEADING))
}

pub fn sized(ui: &egui::Ui, text: &str, role: Role, size: f32, color: Color32, max_width: f32) -> Arc<Galley> {
    sized_parts(ui, &[(text, color)], role, size, max_width)
}

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

const LOWERCASE_PROBE: &str = "xnmoebdfhkl";

pub fn x_height_lift(ui: &egui::Ui, height: f32) -> f32 {
    let galley = line(ui, LOWERCASE_PROBE, Role::Content, Color32::WHITE);
    let ink = galley.mesh_bounds;
    if !ink.height().is_finite() || ink.height() <= 0.0 {
        return 0.0;
    }
    let limit = height / 4.0;
    (ink.center().y - galley.size().y / 2.0).clamp(-limit, limit)
}

pub fn row_line(ui: &egui::Ui, height: f32, role: Role) -> f32 {
    let probe = line(ui, LOWERCASE_PROBE, role, Color32::WHITE);
    (height - probe.size().y) / 2.0 - x_height_lift(ui, height) + baseline(&probe)
}

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

pub fn line_in_box(ui: &egui::Ui, height: f32, role: Role) -> f32 {
    let probe = line(ui, LOWERCASE_PROBE, role, Color32::WHITE);
    let ink = probe.mesh_bounds;
    if !ink.height().is_finite() || ink.height() <= 0.0 {
        return row_line(ui, height, role);
    }
    height / 2.0 - ink.center().y + baseline(&probe)
}

pub fn ink_on(galley: &Galley, middle: f32) -> f32 {
    let ink = galley.mesh_bounds;
    if !ink.height().is_finite() || ink.height() <= 0.0 {
        return middle - galley.size().y / 2.0;
    }
    middle - ink.center().y
}

pub fn ink_size(galley: &Galley) -> egui::Vec2 {
    let ink = galley.mesh_bounds;
    if !ink.height().is_finite() || ink.height() <= 0.0 || ink.width() <= 0.0 {
        return galley.size();
    }
    ink.size()
}

pub fn ink_middle(ui: &egui::Ui, role: Role, top: f32) -> f32 {
    let probe = line(ui, LOWERCASE_PROBE, role, Color32::WHITE);
    let ink = probe.mesh_bounds;
    if !ink.height().is_finite() || ink.height() <= 0.0 {
        return top + probe.size().y / 2.0;
    }
    top + ink.center().y
}

pub fn ink_height(ui: &egui::Ui, role: Role) -> f32 {
    let probe = line(ui, LOWERCASE_PROBE, role, Color32::WHITE);
    let ink = probe.mesh_bounds;
    if !ink.height().is_finite() || ink.height() <= 0.0 {
        return probe.size().y;
    }
    ink.height()
}

pub fn on_line(galley: &Galley, top: f32, line: f32) -> f32 {
    top + line - baseline(galley)
}

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

pub fn count(n: u64) -> String {
    n.to_string()
}

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

    #[test]
    fn an_unreadable_stamp_says_nothing() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 9, 11).unwrap();
        assert_eq!(day("", today), "");
        assert_eq!(day("irgendwas", today), "");
    }

    #[test]
    fn a_future_stamp_is_shown_as_a_date() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 9, 11).unwrap();
        assert_eq!(day("2027-01-01 00:00:00", today), "01.01.27");
    }

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
