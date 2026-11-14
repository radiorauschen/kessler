use super::*;

const NAMERS_SHOWN: usize = 4;

pub(super) fn gaps_column(app: &mut GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette, width: f32) -> Rect {
    let top = ui.cursor().min;
    let height = ui.available_height();
    let mut close = false;
    let mut visit: Option<String> = None;
    let mut hunt = false;
    let mut flip_view = false;
    let tree = app.config.ui.others_tree;

    ui.allocate_ui(Vec2::new(width, height), |ui| {
        ui.set_width(width);
        ui.vertical(|ui| {
            if heading(ui, tr!("andere künstler:innen"), "", None, Some(tr!("schliessen")), m, palette).clicked() {
                close = true;
            }
            ui.add_space(HEAD_AIR);
            prose(
                ui,
                width,
                tr!("wen deine künstler:innen oft als ähnlich nennen, und wer dir noch fehlt"),
                Role::Label,
                palette.faint,
                m,
            );
            let other_view = if tree { tr!("als liste zeigen") } else { tr!("verzweigt zeigen") };
            if device_action(ui, width, other_view, m, palette) {
                flip_view = true;
            }
            let Some(counted) = app.gaps.counted.clone() else {
                let dots = ".".repeat(1 + (ui.input(|i| i.time) / 0.4) as usize % 3);
                say(ui, width, &format!("{}{}", tr!("zähle"), dots), Role::Label, palette.faint, m);
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
                return;
            };
            if app.gaps.counting.is_some() {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
            }

            scrolled(ui, "gaps", palette, |ui| {
                ui.add_space(BLOCK_AIR * 2.0);
                if counted.gaps.is_empty() {
                    let said = if counted.known == 0 {
                        tr!("noch nichts über ähnliche künstler:innen gesammelt")
                    } else {
                        tr!("keine gefunden")
                    };
                    say(ui, width, said, Role::Content, palette.faint, m);
                }
                let most = match app.config.ui.others_shown {
                    0 => usize::MAX,
                    n => n as usize,
                };
                if tree {
                    let shown = counted.gaps.len().min(most);
                    let wide = |name: &str, role: Role| text::line(ui, name, role, palette.ink).size().x;
                    let roots = (0..shown).map(|at| wide(&counted.gaps[at].name, Role::ContentBold)).fold(0.0, f32::max);
                    let namers = (0..shown)
                        .flat_map(|at| branches(&counted.gaps, at).into_iter().map(|b| b.name.to_string()).collect::<Vec<_>>())
                        .map(|name| wide(&name, Role::Content))
                        .fold(0.0, f32::max);
                    for at in 0..shown {
                        if let Some(name) = gap_tree(ui, width, &counted.gaps, at, (roots, namers), m, palette) {
                            visit = Some(name);
                        }
                        ui.add_space(BLOCK_AIR * 2.0);
                    }
                }
                for gap in counted.gaps.iter().take(if tree { 0 } else { most }) {
                    if gap_row(ui, width, &gap.name, gap.by.len(), m, palette) {
                        visit = Some(gap.name.clone());
                    }
                    let mut namers: Vec<&str> = gap.by.iter().take(NAMERS_SHOWN).map(|(n, _)| n.as_str()).collect();
                    let more = gap.by.len().saturating_sub(NAMERS_SHOWN);
                    let more = (more > 0).then(|| tr!("+ {}", more));
                    if let Some(more) = &more {
                        namers.push(more);
                    }
                    prose(ui, width, &namers.join(", "), Role::Label, palette.faint, m);
                    ui.add_space(BLOCK_AIR);
                }

                ui.add_space(BLOCK_AIR * 2.0);
                say(
                    ui,
                    width,
                    &tr!("aus {} von {} künstler:innen",
                        text::count(counted.known as u64),
                        text::count(counted.artists as u64)
                    ),
                    Role::Label,
                    palette.faint,
                    m,
                );
                if counted.unmeasured > 0 {
                    prose(
                        ui,
                        width,
                        &tr!("{} davon ohne mass und mit nur fünf namen — „alle neu suchen…“ im menü holt es nach",
                            text::count(counted.unmeasured as u64)
                        ),
                        Role::Label,
                        palette.faint,
                        m,
                    );
                }
                if let Some(standing) = app.artist_hunt_standing() {
                    say(ui, width, &standing, Role::Label, palette.mid, m);
                } else if counted.known < counted.artists {
                    ui.add_space(BLOCK_AIR);
                    if device_action(ui, width, tr!("fehlende suchen…"), m, palette) {
                        hunt = true;
                    }
                }
            });
        });
    });

    if close {
        app.toggle_gaps();
    }
    if flip_view {
        app.config.ui.others_tree = !tree;
        app.save_config();
    }
    if let Some(name) = visit {
        app.visit_gap(&name);
    }
    if hunt {
        if let Err(e) = app.start_artist_hunt(false) {
            app.set_status(format!("{e}"));
        }
    }
    Rect::from_min_size(top, Vec2::new(width, height))
}

fn gap_row(ui: &mut Ui, width: f32, name: &str, named: usize, m: Metrics, palette: &Palette) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, m.row), Sense::click());
    paint_hover(ui, rect, &response, RowState::default(), m, palette);
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let content = content_rect(rect, m);
    let line = text::row_line(ui, m.row, Role::Content);
    let figure = text::line(ui, &named.to_string(), Role::Figures, palette.mid);
    let figure_at = egui::pos2(content.right() - figure.size().x, text::on_line(&figure, rect.top(), line));
    let room = (content.width() - figure.size().x - 12.0).max(20.0);
    let g = text::line_within(ui, name, Role::Content, palette.ink, room);
    ui.painter()
        .galley(egui::pos2(content.left(), text::on_line(&g, rect.top(), line)), g, palette.ink);
    ui.painter().galley(figure_at, figure, palette.mid);
    response.clicked()
}

const TREE_NAMERS: usize = 4;
const TREE_FURTHER: usize = 3;

struct Branch<'a> {
    name: &'a str,
    like: Option<f32>,
    further: Vec<&'a str>,
}

fn branches(gaps: &[crate::library::gaps::Gap], at: usize) -> Vec<Branch<'_>> {
    gaps[at]
        .by
        .iter()
        .take(TREE_NAMERS)
        .map(|(namer, like)| Branch {
            name: namer.as_str(),
            like: *like,
            further: gaps
                .iter()
                .enumerate()
                .filter(|(i, g)| *i != at && g.by.iter().any(|(n, _)| n == namer))
                .map(|(_, g)| g.name.as_str())
                .take(TREE_FURTHER)
                .collect(),
        })
        .collect()
}

fn gap_tree(ui: &mut Ui, width: f32, gaps: &[crate::library::gaps::Gap], at: usize, widest: (f32, f32), m: Metrics, palette: &Palette) -> Option<String> {
    let tree = branches(gaps, at);
    let rows_of = |b: &Branch| b.further.len().max(1);
    let rows: usize = tree.iter().map(rows_of).sum::<usize>().max(1);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, m.row * rows as f32), Sense::hover());
    let content = content_rect(rect, m);
    let brace = (m.row * 0.5).round();
    let gap = 4.0;
    let column = ((content.width() - 2.0 * (brace + gap * 2.0)) / 3.0).max(20.0);
    let (root_column, namer_column) = (widest.0.min(column), widest.1.min(column));
    let air = gap * 3.0;
    let x_root = content.left();
    let x_first_brace = x_root + root_column + air;
    let x_namers = x_first_brace + brace + gap;
    let x_second_brace = x_namers + namer_column + air;
    let x_further = x_second_brace + brace + gap;
    let stroke = (Role::Content.font().size * 0.085).max(1.0);
    let line = text::row_line(ui, m.row, Role::Content);
    let mut clicked = None;

    let mut name_at = |ui: &mut Ui, name: &str, x: f32, top: f32, room: f32, role: Role, colour: Color32, salt: (usize, usize)| {
        let g = text::line_within(ui, name, role, colour, room);
        let cell = Rect::from_min_size(egui::pos2(x, top), Vec2::new(g.size().x.min(room), m.row));
        let response = ui.interact(cell, ui.scope_id().with(("gap-tree", at, salt)), Sense::click());
        let colour = if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            let plate = cell.expand2(Vec2::new(m.marker_pad_x / 2.0, -m.marker_inset_y));
            ui.painter().rect_filled(plate, MARKER_RADIUS, hover_colour(ui, palette));
            palette.ink
        } else {
            colour
        };
        ui.painter().galley(egui::pos2(x, text::on_line(&g, top, line)), g, colour);
        if response.clicked() {
            clicked = Some(name.to_string());
        }
    };

    let middle = rect.top() + m.row * rows as f32 / 2.0;
    name_at(ui, &gaps[at].name, x_root, middle - m.row / 2.0, root_column, Role::ContentBold, palette.ink, (0, 0));
    if tree.is_empty() {
        return clicked;
    }
    let tone_of = |b: &Branch| match b.like {
        Some(like) if like >= 0.5 => palette.ink,
        Some(_) => palette.faint,
        None => palette.mid,
    };
    let first_tone = tree.iter().max_by(|a, b| a.like.unwrap_or(-1.0).total_cmp(&b.like.unwrap_or(-1.0))).map_or(palette.mid, tone_of);
    paint_brace(ui, x_first_brace, rect.top(), rect.top() + m.row * rows as f32, brace, stroke, first_tone, m);
    let mut top = rect.top();
    for (i, branch) in tree.iter().enumerate() {
        let span = m.row * rows_of(branch) as f32;
        let colour = tone_of(branch);
        name_at(ui, branch.name, x_namers, top + (span - m.row) / 2.0, namer_column, Role::Content, colour, (1, i));
        if !branch.further.is_empty() {
            paint_brace(ui, x_second_brace, top, top + span, brace, stroke, palette.mid, m);
            for (j, further) in branch.further.iter().enumerate() {
                let room = (content.right() - x_further).max(20.0);
                name_at(ui, further, x_further, top + m.row * j as f32, room, Role::Content, palette.mid, (2 + i, j));
            }
        }
        top += span;
    }
    clicked
}

#[allow(clippy::too_many_arguments)]
fn paint_brace(ui: &Ui, x: f32, top: f32, bottom: f32, width: f32, stroke: f32, colour: Color32, m: Metrics) {
    let inset = m.marker_inset_y.max(stroke);
    let (top, bottom) = (top + inset, bottom - inset);
    let middle = (top + bottom) / 2.0;
    let pen = egui::Stroke::new(stroke, colour);
    if bottom - top < m.row * 1.2 {
        ui.painter().line_segment([egui::pos2(x, middle), egui::pos2(x + width, middle)], pen);
        return;
    }
    let stem = x + width / 2.0;
    let r = (width / 2.0).min((bottom - top) / 4.0);
    let arc = |centre: egui::Pos2, from: f32, to: f32| -> Vec<egui::Pos2> {
        (0..=6)
            .map(|k| {
                let a = from + (to - from) * k as f32 / 6.0;
                egui::pos2(centre.x + r * a.cos(), centre.y + r * a.sin())
            })
            .collect()
    };
    use std::f32::consts::{FRAC_PI_2, PI};
    let mut points = vec![egui::pos2(x + width, top + stroke / 2.0)];
    points.extend(arc(egui::pos2(stem + r, top + stroke / 2.0 + r), -FRAC_PI_2, -PI));
    points.extend(arc(egui::pos2(stem - r, middle - r), 0.0, FRAC_PI_2));
    points.extend(arc(egui::pos2(stem - r, middle + r), -FRAC_PI_2, 0.0));
    points.extend(arc(egui::pos2(stem + r, bottom - stroke / 2.0 - r), PI, FRAC_PI_2));
    points.push(egui::pos2(x + width, bottom - stroke / 2.0));
    ui.painter().add(egui::Shape::line(points, pen));
}
