use super::*;
use crate::player::eq;

const SWELLS: [&str; eq::COUNT] = ["eq0", "eq1", "eq2", "eq3", "eq4", "eq5", "eq6", "eq7", "eq8", "eq9"];

const NAMES: [&str; eq::COUNT] = ["31", "62", "125", "250", "500", "1k", "2k", "4k", "8k", "16k"];

const FIELD_LEAST: f32 = 150.0;
const FIELD_MOST: f32 = 280.0;

const NAME_GAP: f32 = 6.0;

const NAME_DROP: f32 = 6.0;

const WHEEL_DB: f32 = 0.5;

pub(super) fn eq_column(app: &mut GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette, width: f32) -> Rect {
    let top = ui.cursor().min;
    let height = ui.available_height();
    let mut close = false;
    let on = app.config.ui.eq_on;
    let mut gains = app.eq_gains();
    let mut changed: Option<bool> = None;
    let mut switch: Option<bool> = None;
    let mut pick_own: Option<Option<usize>> = None;

    ui.allocate_ui(Vec2::new(width, height), |ui| {
        ui.set_width(width);
        ui.vertical(|ui| {
            if heading(ui, "equalizer", "", None, Some(tr!("schliessen")), m, palette).clicked() {
                close = true;
            }
            ui.add_space(HEAD_AIR);

            let state = [(tr!("an"), on, false), (tr!("aus"), !on, false)];
            if let Some(at) = word_row(ui, width, &state, m, palette) {
                switch = Some(at == 0);
            }
            let slot = app.config.ui.eq_slot;
            let current = eq::preset_of(&gains).filter(|_| slot == 0);
            let mut presets: Vec<(&str, bool, bool)> =
                eq::PRESETS.iter().map(|(name, _)| (preset_word(name), on && current == Some(*name), false)).collect();
            presets.push((tr!("eigene 1"), on && slot == 1, app.eq_own(0).is_none()));
            presets.push((tr!("eigene 2"), on && slot == 2, app.eq_own(1).is_none()));
            if let Some(at) = word_row(ui, width, &presets, m, palette) {
                match at.checked_sub(eq::PRESETS.len()) {
                    None => {
                        gains = eq::PRESETS[at].1;
                        changed = Some(true);
                        pick_own = Some(None);
                    }
                    Some(own) => pick_own = Some(Some(own)),
                }
            }
            ui.add_space(BLOCK_AIR);

            let field_height = (ui.available_height() * 0.55).clamp(FIELD_LEAST, FIELD_MOST);
            let mut shown = if on { gains } else { [0.0; eq::COUNT] };
            if let Some(keep) = bands(ui, width, field_height, &mut shown, on, m, palette) {
                changed = Some(keep);
                gains = shown;
                if !on {
                    pick_own = Some(None);
                }
            }

            let room = eq::headroom_db(&gains);
            if on && room >= 0.05 {
                ui.add_space(BLOCK_AIR);
                prose(ui, width, &tr!("um {} db leiser, damit nichts übersteuert", decibels(room).trim_start_matches('+')), Role::Label, palette.faint, m);
            }
        });
    });

    match pick_own {
        Some(Some(own)) => app.choose_eq_own(own),
        Some(None) => app.config.ui.eq_slot = 0,
        None => {}
    }
    if let Some(on) = switch {
        app.set_eq_on(on);
    } else if let Some(keep) = changed {
        app.set_eq_gains(gains, keep);
    }
    if close {
        app.toggle_eq();
    }
    Rect::from_min_size(top, Vec2::new(width, height))
}

fn preset_word(name: &str) -> &'static str {
    match name {
        "mehr bass" => tr!("mehr bass"),
        "weniger bass" => tr!("weniger bass"),
        "mehr höhen" => tr!("mehr höhen"),
        "stimme" => tr!("stimme"),
        "loudness" => tr!("loudness"),
        _ => tr!("flach"),
    }
}

fn decibels(db: f32) -> String {
    let rounded = (db * 2.0).round() / 2.0;
    let said = if rounded.fract() == 0.0 { format!("{:+.0}", rounded) } else { format!("{:+.1}", rounded) };
    let said = if said == "+0" || said == "-0" { "0".to_string() } else { said };
    if crate::lang::current() == crate::lang::Lang::En {
        said
    } else {
        said.replace('.', ",")
    }
}

fn word_row(ui: &mut Ui, width: f32, words: &[(&str, bool, bool)], m: Metrics, palette: &Palette) -> Option<usize> {
    let gap = m.marker_pad_x * 2.0;
    let room = (width - m.marker_pad_x * 2.0).max(40.0);
    let galleys: Vec<_> = words.iter().map(|(w, ..)| text::line(ui, w, Role::Label, palette.mid)).collect();
    let mut lines: Vec<Vec<usize>> = vec![Vec::new()];
    let mut x = 0.0;
    for (i, g) in galleys.iter().enumerate() {
        let w = g.size().x;
        if x > 0.0 && x + w > room {
            lines.push(Vec::new());
            x = 0.0;
        }
        lines.last_mut().expect("one line at least").push(i);
        x += w + gap;
    }
    let mut clicked = None;
    for line in lines {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, m.row), Sense::hover());
        let content = content_rect(rect, m);
        let baseline = text::row_line(ui, m.row, Role::Label);
        let mut x = content.left();
        for i in line {
            let w = galleys[i].size().x;
            let cell = Rect::from_min_max(egui::pos2(x - m.marker_pad_x, rect.top()), egui::pos2(x + w + m.marker_pad_x, rect.bottom()));
            let response = ui.interact(cell, ui.scope_id().with(("eq-word", words[i].0)), Sense::click());
            let hot = response.hovered();
            if hot {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                ui.painter().rect_filled(cell.shrink2(Vec2::new(0.0, 3.0)), 3.0, palette.hover);
            }
            let chosen = words[i].1;
            let colour = if chosen || hot {
                palette.ink
            } else if words[i].2 {
                palette.faint
            } else {
                palette.mid
            };
            let g = text::line(ui, words[i].0, Role::Label, colour);
            let at = egui::pos2(x, text::on_line(&g, rect.top(), baseline));
            let foot = at.y + text::baseline(&g);
            ui.painter().galley(at, g, colour);
            if chosen {
                paint_underline(ui, x, w, foot, text::ink_height(ui, Role::Label), colour);
            }
            if response.clicked() {
                clicked = Some(i);
            }
            x += w + gap;
        }
    }
    clicked
}

#[allow(clippy::too_many_arguments)]
fn bands(ui: &mut Ui, width: f32, height: f32, gains: &mut [f32; eq::COUNT], on: bool, m: Metrics, palette: &Palette) -> Option<bool> {
    let slot_width = (width - m.marker_pad_x * 2.0) / eq::COUNT as f32;
    let widest = NAMES.iter().map(|n| text::line(ui, n, Role::Label, palette.faint).size().x).fold(0.0, f32::max);
    let turned = widest + NAME_GAP > slot_width;
    let label_height = if turned { widest + NAME_DROP * 2.0 } else { m.row };
    let (area, _) = ui.allocate_exact_size(Vec2::new(width, height + label_height), Sense::hover());
    let inner = Rect::from_min_max(
        egui::pos2(area.left() + m.marker_pad_x, area.top()),
        egui::pos2(area.right() - m.marker_pad_x, area.top() + height),
    );
    let travel = inner.shrink2(Vec2::new(0.0, 8.0));
    let x_of = |band: f32| {
        let slot = inner.width() / eq::COUNT as f32;
        inner.left() + slot * (band + 0.5)
    };
    let y_of = |db: f32| travel.center().y - db / eq::RANGE_DB * travel.height() / 2.0;
    let db_at = |y: f32| ((travel.center().y - y) / (travel.height() / 2.0) * eq::RANGE_DB).clamp(-eq::RANGE_DB, eq::RANGE_DB);
    let tone = |c: Color32| if on { c } else { c.gamma_multiply(0.45) };

    let zero = y_of(0.0);
    ui.painter().line_segment(
        [egui::pos2(inner.left(), zero), egui::pos2(inner.right(), zero)],
        egui::Stroke::new(1.0, palette.rule),
    );

    let ceiling = (travel.center().y - inner.top()) / (travel.height() / 2.0) * eq::RANGE_DB - 0.5;
    let first = eq::BANDS[0].log2();
    let span = eq::BANDS[eq::COUNT - 1].log2() - first;
    let points: Vec<egui::Pos2> = (0..=120)
        .map(|i| {
            let band = i as f32 / 120.0 * (eq::COUNT - 1) as f32;
            let hz = 2f32.powf(first + span * band / (eq::COUNT - 1) as f32);
            egui::pos2(x_of(band), y_of(soft_edge(eq::curve_db(gains, hz), ceiling)))
        })
        .collect();
    ui.painter().add(egui::Shape::line(points, egui::Stroke::new(1.5, tone(palette.accent).gamma_multiply(0.55))));

    let mut moved = None;
    for band in 0..eq::COUNT {
        let x = x_of(band as f32);
        let slot = inner.width() / eq::COUNT as f32;
        let grab = Rect::from_center_size(egui::pos2(x, inner.center().y), Vec2::new(slot.min(28.0), inner.height()));
        let response = ui.interact(grab, ui.scope_id().with(("eq-band", band)), Sense::click_and_drag());
        let hot = response.hovered() || response.dragged();
        let (thick, knob) = rule_swell(swell(ui, SWELLS[band], hot));

        if response.dragged() {
            if let Some(at) = response.interact_pointer_pos() {
                gains[band] = (db_at(at.y) * 2.0).round() / 2.0;
                moved = Some(false);
            }
        }
        if response.drag_stopped() {
            moved = Some(true);
        }
        if response.double_clicked() {
            gains[band] = 0.0;
            moved = Some(true);
        } else if response.clicked() {
            if let Some(at) = response.interact_pointer_pos() {
                gains[band] = (db_at(at.y) * 2.0).round() / 2.0;
                moved = Some(true);
            }
        }
        if response.hovered() {
            let wheel: f32 = ui.input(|i| {
                i.raw
                    .events
                    .iter()
                    .filter_map(|e| match e {
                        egui::Event::MouseWheel { delta, .. } => Some(delta.y),
                        _ => None,
                    })
                    .sum()
            });
            if wheel != 0.0 {
                gains[band] = (gains[band] + WHEEL_DB * wheel.signum()).clamp(-eq::RANGE_DB, eq::RANGE_DB);
                moved = Some(true);
            }
        }

        let rule = Rect::from_center_size(egui::pos2(x, travel.center().y), Vec2::new(thick, travel.height()));
        ui.painter().rect_filled(rule, capped(thick), palette.rule);
        let at = y_of(gains[band]);
        if gains[band] != 0.0 {
            let filled = Rect::from_min_max(egui::pos2(rule.left(), at.min(zero)), egui::pos2(rule.right(), at.max(zero)));
            ui.painter().rect_filled(filled, capped(thick), tone(palette.accent));
        }
        if knob > thick / 2.0 {
            knob_at(ui, egui::pos2(x, at), knob, thick, tone(palette.accent));
        }
        if hot {
            let said = format!("{} db", decibels(gains[band]));
            if at - travel.top() < 28.0 {
                badge(ui, egui::pos2(x, at + knob + 4.0), &said, false, palette);
            } else {
                badge(ui, egui::pos2(x, at - knob - 4.0), &said, true, palette);
            }
        }

        let colour = if hot { palette.ink } else { palette.faint };
        let g = text::line(ui, NAMES[band], Role::Label, colour);
        let w = g.size().x;
        if turned {
            let tall = g.size().y;
            let at = egui::pos2(x + tall / 2.0, area.top() + height + NAME_DROP);
            ui.painter().add(egui::epaint::TextShape::new(at, g, colour).with_angle(std::f32::consts::FRAC_PI_2));
        } else {
            let y = text::on_line(&g, area.top() + height, text::row_line(ui, label_height, Role::Label));
            ui.painter().galley(egui::pos2(x - w / 2.0, y), g, colour);
        }
    }
    moved
}

const KNEE: f32 = 10.0;

fn soft_edge(db: f32, ceiling: f32) -> f32 {
    let room = (ceiling - KNEE).max(0.5);
    let over = db.abs() - KNEE;
    if over <= 0.0 {
        return db;
    }
    db.signum() * (KNEE + room * (over / room).tanh())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_curve_is_true_below_the_knee_and_never_reaches_the_edge() {
        assert_eq!(soft_edge(6.0, 14.0), 6.0);
        assert_eq!(soft_edge(-10.0, 14.0), -10.0);
        let at_twelve = soft_edge(12.0, 14.0);
        assert!(at_twelve > 11.5 && at_twelve < 12.0, "{at_twelve}");
        assert!(soft_edge(40.0, 14.0) < 14.0);
        assert!(soft_edge(-40.0, 14.0) > -14.0);
        assert!(soft_edge(20.0, 14.0) > soft_edge(16.0, 14.0), "still rising");
    }
}
