use super::*;

pub(super) fn fourth_column(app: &mut GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette, width: f32) -> Rect {
    let holding = if app.split() {
        app.left_side().unwrap_or(app.fourth_was)
    } else if app.fourth_open() {
        app.fourth()
    } else {
        app.fourth_was
    };
    ui.scope_builder(egui::UiBuilder::new().scope_id(egui::Id::unique("fourth-column")), |ui| {
        match holding {
            crate::gui::Fourth::Device => device_column(app, ui, m, palette, width),
            crate::gui::Fourth::Lists => lists_panel(app, ui, m, palette, width),
            crate::gui::Fourth::Queue => queue_column(app, ui, m, palette, width),
            crate::gui::Fourth::Log => log_column(app, ui, m, palette, width),
            crate::gui::Fourth::Artist => artist_column(app, ui, m, palette, width),
            crate::gui::Fourth::Lyrics => lyrics_column(app, ui, m, palette, width),
            crate::gui::Fourth::Others => gaps_column(app, ui, m, palette, width),
            crate::gui::Fourth::Equalizer => eq_column(app, ui, m, palette, width),
        }
    })
    .inner
}

pub(super) fn side_column(app: &mut GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette, width: f32) -> Rect {
    let holding = app.right_side().unwrap_or(app.side_was);
    side_column_of(app, ui, m, palette, width, holding, "side-column")
}

pub(super) fn side_column_of(
    app: &mut GuiApp,
    ui: &mut Ui,
    m: Metrics,
    palette: &Palette,
    width: f32,
    holding: crate::gui::Fourth,
    id: &str,
) -> Rect {
    ui.scope_builder(egui::UiBuilder::new().scope_id(egui::Id::unique(id)), |ui| {
        match holding {
            crate::gui::Fourth::Device => device_column(app, ui, m, palette, width),
            crate::gui::Fourth::Log => log_column(app, ui, m, palette, width),
            crate::gui::Fourth::Artist => artist_column(app, ui, m, palette, width),
            crate::gui::Fourth::Lyrics => lyrics_column(app, ui, m, palette, width),
            crate::gui::Fourth::Others => gaps_column(app, ui, m, palette, width),
            crate::gui::Fourth::Equalizer => eq_column(app, ui, m, palette, width),
            crate::gui::Fourth::Lists => lists_panel(app, ui, m, palette, width),
            crate::gui::Fourth::Queue => queue_column(app, ui, m, palette, width),
        }
    })
    .inner
}

pub(super) fn list_column(app: &mut GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette, width: f32) -> Rect {
    let focused = app.focus == Column::Queue;
    let selected = app.queue_index;
    let follow = app.scroll_to == Some(Column::Queue) || app.reveal_all;
    let name = app.editing_name().unwrap_or(tr!("liste")).to_string();
    let needle = app.player.current_track().map(|t| t.id);
    let rows: Vec<(String, String, bool, bool)> = app
        .editing_tracks
        .iter()
        .map(|t| {
            (
                t.display_title().to_string(),
                t.shown_artist().to_string(),
                app.is_gone(&t.path),
                Some(t.id) == needle,
            )
        })
        .collect();
    let arrows = arrow_ids("listed", app.editing_tracks.iter().map(|t| t.id));

    let mut clicked: Option<usize> = None;
    let mut opened: Option<usize> = None;
    let mut menu_at: Option<(usize, egui::Pos2)> = None;
    let mut lifted: Option<usize> = None;
    let mut drop_at: Option<usize> = None;
    let mut drop_line: Option<f32> = None;
    let mut closed = false;
    let mut beside = false;
    let carrying = app.dragging.is_some();
    let pointer = ui.ctx().pointer_latest_pos();

    let taken = ui.allocate_ui(Vec2::new(width, ui.available_height()), |ui| {
        ui.vertical(|ui| {
            let trailing = match rows.len() {
                0 => String::new(),
                n => text::count(n as u64),
            };
            if heading(ui, &name, &trailing, None, Some(tr!("schliessen")), m, palette).clicked() {
                closed = true;
            }

            ui.add_space(HEAD_AIR);
            scrolled(ui, "queue", palette, |ui| {
                beside = ground_clicked(ui, "list");
                if rows.is_empty() {
                    say(
                        ui,
                        width,
                        tr!("leer — titel hierher ziehen"),
                        Role::Content,
                        palette.faint,
                        m,
                    );
                    return;
                }
                for (index, (title, artist, gone, sounding)) in rows.iter().enumerate() {
                    let marked = !app.unmarked && selected == index;
                    let state = RowState {
                        selected: marked,
                        focused: focused && marked,
                        playing: *sounding,
                        muted: false,
                        gone: *gone,
                        spent: false,
                        offering: false,
                        arrow: false,
                        pointed: false,
                    };
                    let response = queue_row(
                        ui,
                        egui::Id::unique(("listed", index)),
                        arrows[index],
                        width,
                        title,
                        artist,
                        state,
                        m,
                        palette,
                    );
                    keep_visible(ui, response.rect, m, follow && state.selected);
                    note_cursor_row(ui, "queue", response.rect, state.selected);
                    if carrying {
                        if let Some(at) = pointer {
                            if response.rect.x_range().contains(at.x)
                                && response.rect.y_range().contains(at.y)
                            {
                                let below = at.y > response.rect.center().y;
                                drop_at = Some(index + usize::from(below));
                                drop_line = Some(if below {
                                    response.rect.bottom()
                                } else {
                                    response.rect.top()
                                });
                            } else if index + 1 == rows.len()
                                && at.y > response.rect.bottom()
                                && response.rect.x_range().contains(at.x)
                            {
                                drop_at = Some(rows.len());
                                drop_line = Some(response.rect.bottom());
                            }
                        }
                    }
                    if response.drag_started() {
                        lifted = Some(index);
                    }
                    if response.double_clicked() {
                        opened = Some(index);
                    } else if response.clicked() {
                        clicked = Some(index);
                    } else if response.secondary_clicked() {
                        if let Some(at) = ui.ctx().pointer_latest_pos() {
                            menu_at = Some((index, at));
                        }
                    }
                }
            });
        });
    });

    if let Some(y) = drop_line {
        let line = Rect::from_min_size(
            egui::pos2(taken.response.rect.left(), y.round() - 1.0),
            Vec2::new(taken.response.rect.width(), 2.0),
        );
        ui.painter().rect_filled(line, 0.0, palette.ink);
    }
    if carrying && rows.is_empty() {
        if let Some(at) = pointer {
            if taken.response.rect.contains(at) {
                drop_at = Some(0);
            }
        }
    }

    if ui.input(|i| i.pointer.any_released()) {
        if let Some(at) = drop_at {
            if let Err(e) = app.drop_into_list(at) {
                app.set_status(format!("{e}"));
            }
        }
    }

    if beside {
        app.let_go();
    }
    if closed {
        app.stop_editing();
        return taken.response.rect;
    }
    if let Some(index) = lifted {
        app.dragging = Some(crate::gui::Dragged::Queued(index));
    }
    if let Some(index) = opened {
        if let Err(e) = app.play_from_list(index) {
            app.set_status(format!("{e}"));
        }
    } else if let Some(index) = clicked {
        if let Err(e) = app.select(Column::Queue, index) {
            app.set_status(format!("{e}"));
        }
    } else if let Some((index, at)) = menu_at {
        if let Err(e) = app.select(Column::Queue, index) {
            app.set_status(format!("{e}"));
        }
        app.row_menu = Some(crate::gui::RowMenu {
            at,
            about: About::Listed(index),
        });
    }
    taken.response.rect
}

pub(super) fn queue_to_come(len: usize, playing: Option<usize>) -> usize {
    match playing {
        Some(at) => len.saturating_sub(at + 1),
        None => len,
    }
}

pub(super) fn log_column(app: &mut GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette, width: f32) -> Rect {
    enum Line {
        Day(chrono::NaiveDate),
        Entry(usize),
    }

    let mut close = false;
    let top = ui.cursor().min;
    let height = ui.available_height();
    let today = chrono::Local::now().date_naive();
    let pitch = (m.row * LOG_PITCH).round();

    let mut lines: Vec<Line> = Vec::with_capacity(app.log.len() + 8);
    let mut day = None;
    let hidden = app.config.ui.log_hidden.clone();
    for at in 0..app.log.len() {
        let Some(entry) = app.log.nth_newest(at) else { continue };
        if hidden.iter().any(|h| h == entry.kind.word()) {
            continue;
        }
        let date = entry.at.date();
        if day != Some(date) {
            lines.push(Line::Day(date));
            day = Some(date);
        }
        lines.push(Line::Entry(at));
    }

    let kind_width = crate::gui::log::Kind::ALL
        .iter()
        .map(|k| text::line(ui, crate::lang::t(k.word()), Role::Note, palette.faint).size().x)
        .fold(0.0, f32::max);
    let time_width = text::line(ui, "00:00", Role::FiguresLabel, palette.faint).size().x;

    ui.allocate_ui(Vec2::new(width, height), |ui| {
        ui.set_width(width);
        ui.vertical(|ui| {
            let count = app.log.len();
            let trailing = if count == 0 {
                String::new()
            } else {
                text::count(count as u64)
            };
            let at_end = (count > 0).then_some(tr!("leeren")).filter(|word| {
                let named = text::line(ui, "log", Role::Label, palette.faint).size().x;
                let word = text::line(ui, word, Role::Label, palette.faint).size().x;
                named + HEADING_GAP + word <= width - m.marker_pad_x * 2.0
            });
            let head = heading_with(ui, "log", &trailing, None, Some(tr!("schliessen")), at_end, m, palette);
            if head.beside_clicked() && at_end.is_some() {
                app.log.clear();
            } else if head.clicked() {
                close = true;
            }
            ui.add_space(HEAD_AIR);
            if app.log.is_empty() {
                say(ui, width, tr!("noch nichts passiert"), Role::Content, palette.faint, m);
                return;
            }
            let mut area = without_a_bar(ui, "log").auto_shrink([false, false]);
            if std::mem::take(&mut app.log_from_newest) {
                area = area.vertical_scroll_offset(0.0);
            }
            let step = pitch + ui.spacing().item_spacing.y;
            let air = (pitch * LOG_DAY_AIR).round();
            let mut starts = Vec::with_capacity(lines.len());
            let mut y = 0.0;
            for (at, line) in lines.iter().enumerate() {
                if at > 0 && matches!(line, Line::Day(_)) {
                    y += air;
                }
                starts.push(y);
                y += step;
            }
            let total = (y - ui.spacing().item_spacing.y).max(0.0);
            let output = area
                .show_viewport(ui, |ui, viewport| {
                    ui.set_height(total);
                    let origin = ui.max_rect().min;
                    let first = starts.partition_point(|y| y + pitch < viewport.min.y);
                    let last = starts.partition_point(|y| *y <= viewport.max.y);
                    for at in first..last {
                        let rect = Rect::from_min_size(
                            origin + Vec2::new(0.0, starts[at]),
                            Vec2::new(width, pitch),
                        );
                        let response = ui.interact(rect, egui::Id::unique(("log-line", at)), Sense::hover());
                        let content = content_rect(rect, m);
                        match lines[at] {
                            Line::Day(date) => {
                                let said = crate::gui::log::day(date, today);
                                let g = text::line(ui, &said, Role::Label, palette.faint);
                                let line = text::row_line(ui, pitch, Role::Label);
                                ui.painter().galley(
                                    egui::pos2(content.left(), text::on_line(&g, rect.top(), line)),
                                    g,
                                    palette.faint,
                                );
                            }
                            Line::Entry(nth) => {
                                let Some(entry) = app.log.nth_newest(nth) else {
                                    continue;
                                };
                                let mut x = content.left();
                                let time = text::line(
                                    ui,
                                    &crate::gui::log::time(entry.at),
                                    Role::FiguresLabel,
                                    palette.faint,
                                );
                                let line = text::row_line(ui, pitch, Role::FiguresLabel);
                                ui.painter().galley(
                                    egui::pos2(x, text::on_line(&time, rect.top(), line)),
                                    time,
                                    palette.faint,
                                );
                                x += time_width + LOG_GAP;
                                let line = text::row_line(ui, pitch, Role::Note);
                                let kind =
                                    text::line(ui, crate::lang::t(entry.kind.word()), Role::Note, palette.mid);
                                ui.painter().galley(
                                    egui::pos2(x, text::on_line(&kind, rect.top(), line)),
                                    kind,
                                    palette.mid,
                                );
                                x += kind_width + LOG_GAP;
                                cell_text(
                                    ui,
                                    Rect::from_min_max(
                                        egui::pos2(x, rect.top()),
                                        egui::pos2(content.right(), rect.bottom()),
                                    ),
                                    egui::Id::unique(("log", nth)),
                                    &entry.text,
                                    Role::Note,
                                    palette.ink,
                                    response.hovered(),
                                    palette.ground,
                                    m,
                                );
                            }
                        }
                    }
                });
            let viewport = output.inner_rect;
            let above = output.state.clamped_offset().y;
            let below = (output.content_size.y - above - viewport.height()).max(0.0);
            paint_fade(ui, viewport, above, below, palette);
            paint_extent(ui, "log", viewport, above, output.content_size.y, palette);
        });
    });

    if close {
        app.log_open = false;
    }
    Rect::from_min_size(top, Vec2::new(width, height))
}

pub(super) const LOG_PITCH: f32 = 0.78;

pub(super) const LOG_GAP: f32 = 8.0;

pub(super) const LOG_DAY_AIR: f32 = 0.6;

pub(super) fn queue_column(app: &mut GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette, width: f32) -> Rect {
    const SPENT_SHOWN: usize = 5;

    let focused = app.focus == Column::Queue;
    let selected = app.queue_index;
    let follow = app.scroll_to == Some(Column::Queue) || app.reveal_all;
    let playing = app.player.queue_position().filter(|_| app.player.on_air().is_none());
    let picked = app.queue_picks();
    let carry_along = playing.is_some() && app.queue_followed != playing;
    if carry_along {
        app.queue_followed = playing;
    }
    let keep_at_top = playing.unwrap_or(0).saturating_sub(SPENT_SHOWN);
    let queued: Vec<(String, String, bool)> = app
        .player
        .queued()
        .iter()
        .map(|t| {
            (
                t.display_title().to_string(),
                t.shown_artist().to_string(),
                app.is_gone(&t.path),
            )
        })
        .collect();
    let arrows = arrow_ids("queued", app.player.queued().iter().map(|t| t.id));

    let mut clicked: Option<usize> = None;
    let mut opened: Option<usize> = None;
    let mut menu_at: Option<(usize, egui::Pos2)> = None;
    let mut lifted: Option<usize> = None;
    let mut drop_at: Option<usize> = None;
    let mut drop_line: Option<f32> = None;
    let carrying = app.dragging.is_some();
    let pointer = ui.ctx().pointer_latest_pos();
    let mut emptied = false;
    let mut close = false;
    let mut beside = false;

    let taken = ui.allocate_ui(Vec2::new(width, ui.available_height()), |ui| {
        ui.vertical(|ui| {
            let mut trailing = match queue_to_come(queued.len(), playing) {
                0 => String::new(),
                n => tr!("noch {}", text::count(n as u64)),
            };
            let modes_here = app.config.ui.modes_in_queue;
            let named = text::line(ui, tr!("warteschlange"), Role::Label, palette.faint).size().x;
            if modes_here && !trailing.is_empty() {
                let count = text::line(ui, &trailing, Role::Label, palette.faint).size().x;
                let inside = width - m.marker_pad_x * 2.0;
                if named + HEADING_GAP + QUEUE_MODES_WIDTH + HEADING_GAP + count > inside {
                    trailing.clear();
                }
            }
            let at_end = (!queued.is_empty())
                .then_some(tr!("leeren"))
                .filter(|word| {
                    let word = text::line(ui, word, Role::Label, palette.faint).size().x;
                    let modes = if modes_here { QUEUE_MODES_WIDTH + HEADING_GAP } else { 0.0 };
                    named + HEADING_GAP + modes + word <= width - m.marker_pad_x * 2.0
                });
            let head = heading_with(
                ui,
                tr!("warteschlange"),
                &trailing,
                None,
                Some(tr!("schliessen")),
                at_end,
                m,
                palette,
            );
            if head.beside_clicked() && at_end.is_some() {
                emptied = true;
            } else if head.clicked() {
                close = true;
            }
            if modes_here {
                let row = head.row.rect;
                let probe = text::line(ui, tr!("warteschlange"), Role::Label, palette.faint);
                let line = text::row_line(ui, m.row, Role::Label);
                let top = text::on_line(&probe, row.top(), line);
                let middle = text::ink_middle(ui, Role::Label, top);
                let left = content_rect(row, m).left() + named + HEADING_GAP;
                queue_mode_keys(app, ui, left, middle, palette);
            }

            ui.add_space(HEAD_AIR);
            scrolled(ui, "queue", palette, |ui| {
                beside = ground_clicked(ui, "queue");
                if queued.is_empty() {
                    prose(ui, width, tr!("nichts eingereiht — a reiht ein, oder titel hierher ziehen"), Role::Label, palette.faint, m);
                    return;
                }
                for (index, (title, artist, gone)) in queued.iter().enumerate() {
                    let marked = !app.unmarked && ((focused && selected == index) || picked.contains(&index));
                    let state = RowState {
                        selected: marked,
                        focused: focused && marked,
                        playing: playing == Some(index),
                        muted: false,
                        gone: *gone,
                        spent: playing.is_some_and(|at| index < at),
                        offering: false,
                        arrow: false,
                        pointed: false,
                    };
                    let response = queue_row(
                        ui,
                        egui::Id::unique(("queued", index)),
                        arrows[index],
                        width,
                        title,
                        artist,
                        state,
                        m,
                        palette,
                    );
                    keep_visible(ui, response.rect, m, follow && state.selected && !carry_along);
                    note_cursor_row(ui, "queue", response.rect, state.selected);
                    if carry_along && index == keep_at_top {
                        ui.scroll_to_rect(response.rect, Some(egui::Align::TOP));
                    }
                    if carrying {
                        if let Some(at) = pointer {
                            if response.rect.x_range().contains(at.x)
                                && response.rect.y_range().contains(at.y)
                            {
                                let below = at.y > response.rect.center().y;
                                drop_at = Some(index + usize::from(below));
                                drop_line = Some(if below {
                                    response.rect.bottom()
                                } else {
                                    response.rect.top()
                                });
                            } else if index + 1 == queued.len()
                                && at.y > response.rect.bottom()
                                && response.rect.x_range().contains(at.x)
                            {
                                drop_at = Some(queued.len());
                                drop_line = Some(response.rect.bottom());
                            }
                        }
                    }
                    if response.drag_started() {
                        lifted = Some(index);
                    }
                    if response.double_clicked() {
                        opened = Some(index);
                    } else if response.clicked() {
                        clicked = Some(index);
                    } else if response.secondary_clicked() {
                        if let Some(at) = ui.ctx().pointer_latest_pos() {
                            menu_at = Some((index, at));
                        }
                    }
                }
            });
        });
    });

    if let Some(y) = drop_line {
        let line = Rect::from_min_size(
            egui::pos2(taken.response.rect.left(), y.round() - 1.0),
            Vec2::new(taken.response.rect.width(), 2.0),
        );
        ui.painter().rect_filled(line, 0.0, palette.ink);
    }
    if carrying && queued.is_empty() {
        if let Some(at) = pointer {
            if taken.response.rect.contains(at) {
                drop_at = Some(0);
            }
        }
    }

    if ui.input(|i| i.pointer.any_released()) {
        if let Some(at) = drop_at {
            match app.drop_into_queue(at) {
                Ok(_) => {}
                Err(e) => app.set_status(format!("{e}")),
            }
        }
    }

    if beside {
        app.let_go();
    }
    if emptied {
        app.clear_queue();
    }
    if close {
        app.close_queue();
    }
    if let Some(index) = lifted {
        app.dragging = Some(crate::gui::Dragged::Queued(index));
    }
    if let Some(index) = opened {
        if let Err(e) = app.play_queued(index) {
            app.set_status(format!("{e}"));
        }
    } else if let Some(index) = clicked {
        let held = ui.input(|i| i.modifiers);
        app.pick_queued(index, held.shift, held.command || held.ctrl);
    } else if let Some((index, at)) = menu_at {
        if let Err(e) = app.select(Column::Queue, index) {
            app.set_status(format!("{e}"));
        }
        app.row_menu = Some(crate::gui::RowMenu {
            at,
            about: About::Queued(index),
        });
    }
    taken.response.rect
}

pub(super) fn action(ui: &mut Ui, width: f32, word: &str, m: Metrics, palette: &Palette) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, m.row), Sense::click());
    let content = content_rect(rect, m);
    if response.hovered() {
        ui.painter().rect_filled(
            rect.shrink2(Vec2::new(m.marker_pad_x - 4.0, m.marker_inset_y)),
            MARKER_RADIUS,
            palette.hover,
        );
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let colour = if response.hovered() { palette.ink } else { palette.mid };
    let g = text::line(ui, word, Role::Content, colour);
    let at = egui::pos2(
        content.left(),
        text::on_line(&g, rect.top(), text::row_line(ui, m.row, Role::Content)),
    );
    ui.painter().galley(at, g, colour);
    response.clicked()
}

fn arrow_ids(column: &str, tracks: impl Iterator<Item = i64>) -> Vec<egui::Id> {
    let mut seen: std::collections::HashMap<i64, usize> = std::collections::HashMap::new();
    tracks
        .map(|track| {
            let nth = seen.entry(track).or_default();
            let id = egui::Id::unique((column, "arrow", track, *nth));
            *nth += 1;
            id
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::arrow_ids;

    #[test]
    fn arrows_stay_with_their_tracks() {
        let before = arrow_ids("queued", [1, 2, 3].into_iter());
        let after = arrow_ids("queued", [9, 1, 2, 3].into_iter());
        assert_eq!(before[2], after[3]);
        let twice = arrow_ids("queued", [5, 5].into_iter());
        assert_ne!(twice[0], twice[1]);
    }
}
