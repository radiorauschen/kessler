use super::*;

pub(super) const TAG_LABEL_WIDTH: f32 = 118.0;

pub(super) const CARET_WIDTH: f32 = 7.0;

pub(super) fn tag_form(app: &mut GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette) -> Rect {
    let width = ui.available_width();
    let blink = ui.input(|i| i.time).rem_euclid(1.06) < 0.62;
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(120));

    let mut focus_on: Option<usize> = None;

    let taken = ui.vertical(|ui| {
        let Some(editor) = app.editor.as_ref() else {
            return;
        };
        let count = editor.tracks.len();
        let trailing = if count == 1 {
            tr!("1 titel").to_string()
        } else {
            tr!("{} titel", text::count(count as u64))
        };
        heading(ui, tr!("idtag bearbeiten"), &trailing, None, None, m, palette);

        scrolled(ui, "tag-form", palette, |ui| {
            for (i, row) in editor.rows.iter().enumerate() {
                let focused = editor.focus == i;
                let state = RowState {
                    selected: focused,
                    focused,
                    ..Default::default()
                };
                let (rect, response) = allocate_row(ui, width, m);
                paint_marker(ui, rect, state, m, palette);
                paint_hover(ui, rect, &response, state, m, palette);
                if response.clicked() {
                    focus_on = Some(i);
                }

                let content = content_rect(rect, m);
                let y = |h: f32| rect.top() + (m.row - h) / 2.0;

                let label_colour = if focused { palette.ink } else { palette.faint };
                let g = text::line(ui, crate::lang::t(row.field.label()), Role::Label, label_colour);
                ui.painter()
                    .galley(egui::pos2(content.left(), y(g.size().y)), g, label_colour);

                let left = content.left() + TAG_LABEL_WIDTH;
                let room = (content.right() - left).max(20.0);

                if row.mixed {
                    let aside = if focused { CARET_WIDTH + 7.0 } else { 0.0 };
                    let g = text::line_within(
                        ui,
                        tr!("verschieden"),
                        Role::Content,
                        palette.faint,
                        room - aside,
                    );
                    ui.painter()
                        .galley(egui::pos2(left + aside, y(g.size().y)), g, palette.faint);
                } else {
                    let colour = if row.touched { palette.ink } else { palette.mid };
                    let g =
                        text::cased_within(ui, row.entry.text(), Role::Content, colour, room);
                    let at = egui::pos2(left, y(g.size().y));
                    if focused && row.entry.is_marked() {
                        paint_field_mark(ui, at, &g, rect, palette);
                    }
                    ui.painter().galley(at, g, colour);
                }

                if focused && blink && !row.entry.is_marked() {
                    let before = &row.entry.text()[..row.entry.caret()];
                    let at = if row.mixed {
                        0.0
                    } else {
                        text::cased_within(ui, before, Role::Content, palette.ink, f32::INFINITY)
                            .size()
                            .x
                    };
                    let caret = Rect::from_min_size(
                        egui::pos2(
                            (left + at + 1.0).min(content.right() - CARET_WIDTH),
                            rect.top() + 6.0,
                        ),
                        Vec2::new(CARET_WIDTH, m.row - 12.0),
                    );
                    ui.painter().rect_filled(caret, 0.0, palette.ink);
                }
            }

            ui.add_space(16.0);
            let (rect, _) = ui.allocate_exact_size(Vec2::new(width, m.row), Sense::hover());
            let content = content_rect(rect, m);
            let colour = if editor.changed() {
                palette.mid
            } else {
                palette.faint
            };
            let g = text::line_within(
                ui,
                tr!("tab wechselt     enter speichert     esc bricht ab"),
                Role::Label,
                colour,
                content.width(),
            );
            let at = egui::pos2(
                content.left(),
                text::on_line(&g, rect.top(), text::row_line(ui, m.row, Role::Label)),
            );
            ui.painter().galley(at, g, colour);

            ui.add_space(6.0);
            for (i, title) in editor.titles.iter().take(TAG_TITLES_SHOWN).enumerate() {
                let shown = if i == TAG_TITLES_SHOWN - 1 && count > TAG_TITLES_SHOWN {
                    tr!("… und {} weitere", text::count((count - i) as u64))
                } else {
                    title.clone()
                };
                let (rect, _) = ui.allocate_exact_size(Vec2::new(width, m.row), Sense::hover());
                let content = content_rect(rect, m);
                let g =
                    text::cased_within(ui, &shown, Role::Content, palette.faint, content.width());
                let at = egui::pos2(
                    content.left(),
                    text::on_line(&g, rect.top(), text::row_line(ui, m.row, Role::Content)),
                );
                ui.painter().galley(at, g, palette.faint);
            }
        });
    });

    if let Some(i) = focus_on {
        if let Some(editor) = app.editor.as_mut() {
            editor.focus = i;
            editor.end();
        }
    }
    taken.response.rect
}

pub(super) const TAG_TITLES_SHOWN: usize = 5;

pub(super) const COVER_WISH: f32 = 150.0;

pub(super) const COVER_GAP: f32 = 10.0;

pub(super) fn cover_layout(inner: f32) -> (usize, f32) {
    let across = ((inner + COVER_GAP) / (COVER_WISH + COVER_GAP)).floor().max(2.0) as usize;
    let tile = (inner - COVER_GAP * (across as f32 - 1.0)) / across as f32;
    (across, tile.max(24.0))
}

pub(super) fn say(ui: &mut Ui, width: f32, words: &str, role: Role, colour: Color32, m: Metrics) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, m.row), Sense::hover());
    let content = content_rect(rect, m);
    let g = text::line_within(ui, words, role, colour, content.width());
    let at = egui::pos2(
        content.left(),
        text::on_line(&g, rect.top(), text::row_line(ui, m.row, Role::Content)),
    );
    ui.painter().galley(at, g, colour);
}

pub(super) fn prompt_form(app: &mut GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette) -> Rect {
    let width = ui.available_width();
    let blink = ui.input(|i| i.time).rem_euclid(1.06) < 0.62;
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(120));

    let mut answer = false;
    let mut close = false;

    let taken = ui.vertical(|ui| {
        let Some(prompt) = app.prompt.as_ref() else {
            return;
        };
        let head = heading(ui, prompt.heading(), tr!("schliessen"), None, None, m, palette);
        if head.beside_clicked() {
            close = true;
        }

        if prompt.typing() {
            let (rect, response) = ui.allocate_exact_size(Vec2::new(width, m.row), Sense::click());
            let content = content_rect(rect, m);
            let line = text::row_line(ui, m.row, Role::Content);
            let text_now = prompt.entry.text();
            let g = text::line_within(ui, text_now, Role::Content, palette.ink, content.width());
            let at = egui::pos2(content.left(), text::on_line(&g, rect.top(), line));
            if prompt.entry.is_marked() {
                paint_field_mark(ui, at, &g, rect, palette);
            }
            ui.painter().galley(at, g, palette.ink);
            if blink && !prompt.entry.is_marked() {
                let before = &text_now[..prompt.entry.caret()];
                let run = text::line(ui, before, Role::Content, palette.ink).size().x;
                let caret = Rect::from_min_size(
                    egui::pos2(
                        (content.left() + run + 1.0).min(content.right() - CARET_WIDTH),
                        rect.top() + 6.0,
                    ),
                    Vec2::new(CARET_WIDTH, m.row - 12.0),
                );
                ui.painter().rect_filled(caret, 0.0, palette.ink);
            }
            if response.clicked() {
            }
        } else {
            say(ui, width, prompt.entry.text(), Role::ContentBold, palette.ink, m);
            say(
                ui,
                width,
                tr!("die titel bleiben in der bibliothek"),
                Role::Content,
                palette.mid,
                m,
            );
        }

        ui.add_space(m.row * 0.6);
        if action(ui, width, prompt.word(), m, palette) {
            answer = true;
        }
        say(
            ui,
            width,
            tr!("enter · esc bricht ab"),
            Role::Label,
            palette.faint,
            m,
        );
    });

    if answer {
        if let Err(e) = app.answer_prompt() {
            app.set_status(format!("{e}"));
        }
    }
    if close {
        app.cancel_prompt();
    }
    taken.response.rect
}

pub(super) fn transfer_form(app: &mut GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette) -> Rect {
    use crate::gui::Stage;
    use crate::ipod::format_bytes;

    let width = ui.available_width();
    let mut close = false;
    let mut go: Option<bool> = None;

    let taken = ui.vertical(|ui| {
        let Some(transfer) = app.transfer.as_ref() else {
            return;
        };
        let running = matches!(transfer.stage, Stage::Going { .. });
        let trailing = if running { "" } else { tr!("schliessen") };
        let head = heading(ui, tr!("auf den ipod"), trailing, None, None, m, palette);
        if head.beside_clicked() && !running {
            close = true;
        }

        let label = |ui: &mut Ui, word: &str, value: &str, palette: &Palette| {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(width, m.row), Sense::hover());
            let content = content_rect(rect, m);
            let line = text::row_line(ui, m.row, Role::Content);
            let g = text::line(ui, word, Role::Label, palette.faint);
            let at = egui::pos2(content.left(), text::on_line(&g, rect.top(), line));
            ui.painter().galley(at, g, palette.faint);
            let left = content.left() + TAG_LABEL_WIDTH;
            let g = text::line_within(
                ui,
                value,
                Role::Content,
                palette.ink,
                (content.right() - left).max(40.0),
            );
            let at = egui::pos2(left, text::on_line(&g, rect.top(), line));
            ui.painter().galley(at, g, palette.ink);
        };

        say(ui, width, &transfer.what, Role::ContentBold, palette.ink, m);
        ui.add_space(m.row * 0.4);

        match &transfer.stage {
            Stage::Asking => {
                let plan = &transfer.plan;
                if !plan.to_copy.is_empty() {
                    label(
                        ui,
                        tr!("überspielen"),
                        &tr!("{} titel, {}",
                            text::count(plan.to_copy.len() as u64),
                            format_bytes(plan.bytes_to_copy)
                        ),
                        palette,
                    );
                }
                for list in &plan.lists {
                    ui.add_space(m.row * 0.3);
                    say(ui, width, &format!("„{}“", list.name), Role::Content, palette.ink, m);
                    say(
                        ui,
                        width,
                        &tr!("playlist, neu geschrieben · {} titel",
                            text::count(list.tracks.len() as u64)
                        ),
                        Role::Label,
                        palette.faint,
                        m,
                    );
                    ui.add_space(m.row * 0.3);
                }
                let tagged = plan.retag.len() - plan.sound_only;
                if tagged > 0 {
                    label(ui, "idtags", &tr!("{} titel neu getaggt", text::count(tagged as u64)), palette);
                }
                if plan.sound_only > 0 {
                    label(
                        ui,
                        "sound check",
                        &tr!("{} titel", text::count(plan.sound_only as u64)),
                        palette,
                    );
                }
                if !plan.respell.is_empty() {
                    label(
                        ui,
                        tr!("schreibweisen"),
                        &tr!("{} titel angeglichen", text::count(plan.respell.len() as u64)),
                        palette,
                    );
                }
                if plan.reorder {
                    label(ui, tr!("menüs"), tr!("neu ordnen"), palette);
                }
                if !plan.drop_tracks.is_empty() {
                    label(ui, tr!("podcasts"), &tr!("{} gehörte folgen kommen weg", text::count(plan.drop_tracks.len() as u64)), palette);
                }
                if !plan.drop_lists.is_empty() {
                    ui.add_space(m.row * 0.3);
                    say(
                        ui,
                        width,
                        &tr!("{} alte playlists kommen weg:", text::count(plan.drop_lists.len() as u64)),
                        Role::Label,
                        palette.faint,
                        m,
                    );
                    for name in &plan.drop_lists {
                        say(ui, width, &format!("„{name}“"), Role::Content, palette.mid, m);
                    }
                    say(
                        ui,
                        width,
                        tr!("die musik darin bleibt auf dem ipod"),
                        Role::Label,
                        palette.faint,
                        m,
                    );
                    ui.add_space(m.row * 0.3);
                }
                if plan.already_present > 0 {
                    label(
                        ui,
                        tr!("schon da"),
                        &tr!("{} titel", text::count(plan.already_present as u64)),
                        palette,
                    );
                }
                let converting = transfer.can_convert && app.config.ipod_convert;
                if converting && !plan.incompatible.is_empty() {
                    label(
                        ui,
                        tr!("umwandeln"),
                        &tr!("{} titel — {} → {}",
                            text::count(plan.incompatible.len() as u64),
                            plan.incompatible_formats().join(", "),
                            crate::ipod::sync::conversion_word(&plan.incompatible_formats())
                        ),
                        palette,
                    );
                } else if !plan.incompatible.is_empty() {
                    label(
                        ui,
                        tr!("geht nicht"),
                        &tr!("{} titel — {}",
                            text::count(plan.incompatible.len() as u64),
                            plan.incompatible_formats().join(", ")
                        ),
                        palette,
                    );
                    if !transfer.can_convert {
                        label(ui, tr!("umwandeln"), tr!("bräuchte ffmpeg"), palette);
                        label(ui, "", "brew install ffmpeg", palette);
                    }
                }
                if !plan.missing_files.is_empty() {
                    label(
                        ui,
                        tr!("nicht gefunden"),
                        &tr!("{} dateien", text::count(plan.missing_files.len() as u64)),
                        palette,
                    );
                }
                if let Some(free) = transfer.free {
                    let after = free.saturating_sub(plan.bytes_to_copy);
                    label(ui, tr!("danach frei"), &format_bytes(after), palette);
                }

                ui.add_space(m.row * 0.6);
                let work = !plan.is_empty() || (converting && !plan.incompatible.is_empty());
                if !work && !transfer.can_convert {
                    say(ui, width, tr!("nichts zu überspielen"), Role::Content, palette.faint, m);
                } else if converting {
                    if action(ui, width, tr!("überspielen"), m, palette) {
                        go = Some(true);
                    }
                } else {
                    if work && action(ui, width, tr!("überspielen"), m, palette) {
                        go = Some(false);
                    }
                    if transfer.can_convert
                        && action(ui, width, tr!("mit umwandeln"), m, palette)
                    {
                        go = Some(true);
                    }
                }
            }
            Stage::Going {
                done,
                total,
                doing,
                failed,
                ..
            } => {
                label(
                    ui,
                    tr!("läuft"),
                    &tr!("{} von {}", done, total),
                    palette,
                );
                if !doing.is_empty() {
                    say(ui, width, doing, Role::Content, palette.mid, m);
                }
                if *failed > 0 {
                    label(
                        ui,
                        tr!("nicht gegangen"),
                        &tr!("{} titel", text::count(*failed as u64)),
                        palette,
                    );
                }
                ui.add_space(m.row * 0.4);
                say(
                    ui,
                    width,
                    tr!("die alte datenbank liegt als sicherung daneben"),
                    Role::Label,
                    palette.faint,
                    m,
                );
            }
            Stage::Done(Ok(said)) => {
                let room = (width - m.marker_pad_x * 2.0).max(60.0);
                let g = text::paragraph(ui, said, Role::Content, palette.ink, room);
                let (rect, _) = ui.allocate_exact_size(
                    Vec2::new(width, g.size().y + m.row * 0.4),
                    Sense::hover(),
                );
                ui.painter()
                    .galley(egui::pos2(rect.left() + m.marker_pad_x, rect.top()), g, palette.ink);
            }
            Stage::Done(Err(why)) => {
                let room = (width - m.marker_pad_x * 2.0).max(60.0);
                let g = text::paragraph(ui, why, Role::Content, palette.accent, room);
                let (rect, _) = ui.allocate_exact_size(
                    Vec2::new(width, g.size().y + m.row * 0.4),
                    Sense::hover(),
                );
                ui.painter()
                    .galley(egui::pos2(rect.left() + m.marker_pad_x, rect.top()), g, palette.accent);
            }
        }
    });

    if let Some(convert) = go {
        app.run_transfer(convert);
    }
    if close {
        app.close_transfer();
    }
    taken.response.rect
}

pub(super) fn details_form(app: &mut GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette) -> Rect {
    use crate::library::details::Line;
    let width = ui.available_width();
    let mut close = false;

    let taken = ui.vertical(|ui| {
        let Some(details) = app.details.as_ref() else {
            return;
        };
        let head = heading(ui, tr!("informationen"), tr!("schliessen"), None, None, m, palette);
        if head.beside_clicked() {
            close = true;
        }

        let widest = details
            .lines
            .iter()
            .filter_map(|line| match line {
                Line::Field { label, .. } => {
                    Some(text::line(ui, label, Role::Label, palette.faint).size().x)
                }
                Line::Section(_) => None,
            })
            .fold(0.0_f32, f32::max);
        let label_width = (widest + 18.0).clamp(TAG_LABEL_WIDTH, width * 0.5);

        scrolled(ui, "details", palette, |ui| {
            say(ui, width, &details.title, Role::ContentBold, palette.ink, m);

            for line in &details.lines {
                match line {
                    Line::Section(name) => {
                        ui.add_space(m.row * 0.5);
                        say(ui, width, name, Role::Label, palette.faint, m);
                    }
                    Line::Field { label, value } => {
                        let left = m.marker_pad_x + label_width;
                        let room = (width - left - m.marker_pad_x).max(40.0);
                        let answer =
                            text::paragraph(ui, value, Role::Content, palette.ink, room);
                        let wrapped = answer.size().y > m.row;
                        let height = if wrapped {
                            answer.size().y + m.row * 0.3
                        } else {
                            m.row
                        };

                        let (rect, _) =
                            ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
                        let content = content_rect(rect, m);
                        let line = text::row_line(ui, m.row, Role::Content);

                        let g = text::line(ui, label, Role::Label, palette.faint);
                        let at = egui::pos2(content.left(), text::on_line(&g, rect.top(), line));
                        ui.painter().galley(at, g, palette.faint);
                        let at = egui::pos2(
                            content.left() + label_width,
                            text::on_line(&answer, rect.top(), line),
                        );
                        ui.painter().galley(at, answer, palette.ink);
                    }
                }
            }
            ui.add_space(m.row);
        });
    });

    if close {
        app.close_details();
    }
    taken.response.rect
}

pub(super) fn cover_form(app: &mut GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette) -> Rect {
    use crate::gui::coverpick::State;
    let width = ui.available_width();

    if let Some(State::Offering { found, .. }) = app.cover_pick.as_mut().map(|p| &mut p.state) {
        for (i, one) in found.iter_mut().enumerate() {
            if one.texture.is_some() {
                continue;
            }
            let Some(bytes) = &one.thumb else { continue };
            let Ok(picture) = image::load_from_memory(bytes) else {
                one.thumb = None;
                continue;
            };
            let picture = picture.to_rgba8();
            let size = [picture.width() as usize, picture.height() as usize];
            let colours = egui::ColorImage::from_rgba_unmultiplied(size, picture.as_raw());
            one.texture = Some(ui.ctx().load_texture(
                format!("cover-candidate-{i}"),
                colours,
                egui::TextureOptions::LINEAR,
            ));
        }
    }

    let hint = tr!("pfeile wählen     enter übernimmt     esc bricht ab");

    let mut take: Option<usize> = None;
    let mut look_at: Option<usize> = None;
    let mut columns_fitted = cover_layout(width - m.marker_pad_x * 2.0).0;

    let taken = ui.vertical(|ui| {
        let Some(picker) = app.cover_pick.as_ref() else {
            return;
        };
        let about = format!("{} — {}", picker.artist, picker.album);
        heading(ui, tr!("cover suchen"), &about, None, None, m, palette);

        scrolled(ui, "cover-form", palette, |ui| match &picker.state {
            State::Looking(_) => {
                say(ui, width, tr!("sucht …"), Role::Content, palette.mid, m);
                say(
                    ui,
                    width,
                    tr!("archiv und itunes werden gefragt"),
                    Role::Label,
                    palette.faint,
                    m,
                );
            }
            State::Fetching(_) => {
                say(ui, width, tr!("lädt das bild …"), Role::Content, palette.mid, m);
            }
            State::Trouble(said) => {
                say(ui, width, said, Role::Content, palette.mid, m);
                ui.add_space(8.0);
                say(ui, width, hint, Role::Label, palette.faint, m);
            }
            State::Offering { found, focus } => {
                let inner = width - m.marker_pad_x * 2.0;
                let (across, tile) = cover_layout(inner);
                columns_fitted = across;
                let rows = found.len().div_ceil(across);
                let step = tile + COVER_GAP + m.row;
                let (area, _) =
                    ui.allocate_exact_size(Vec2::new(width, rows as f32 * step), Sense::hover());

                for (index, one) in found.iter().enumerate() {
                    let (row, column) = (index / across, index % across);
                    let at = egui::pos2(
                        area.left() + m.marker_pad_x + column as f32 * (tile + COVER_GAP),
                        area.top() + row as f32 * step,
                    );
                    let cell = Rect::from_min_size(at, Vec2::new(tile, tile + m.row));
                    let response = ui.interact(
                        cell,
                        egui::Id::unique(("cover-candidate", index)),
                        Sense::click(),
                    );
                    let chosen = index == *focus;
                    if chosen || response.hovered() {
                        ui.painter().rect_filled(
                            cell.expand(4.0),
                            MARKER_RADIUS,
                            if chosen { palette.marked() } else { palette.hover },
                        );
                    }

                    let square = Rect::from_min_size(at, Vec2::new(tile, tile));
                    match &one.texture {
                        Some(texture) => {
                            let size = texture.size_vec2();
                            let scale = (tile / size.x).min(tile / size.y);
                            let drawn = Rect::from_center_size(square.center(), size * scale);
                            let mut mesh = egui::Mesh::with_texture(texture.id());
                            mesh.add_rect_with_uv(
                                drawn,
                                Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                                Color32::WHITE,
                            );
                            ui.painter().add(egui::Shape::mesh(mesh));
                        }
                        None => {
                            ui.painter().rect_filled(square, MARKER_RADIUS, palette.hover);
                            let g = text::line(ui, tr!("kein bild"), Role::Label, palette.faint);
                            ui.painter()
                                .galley(square.center() - g.size() / 2.0, g, palette.faint);
                        }
                    }

                    let label = match one.candidate.year {
                        Some(year) => format!("{}  {year}", one.candidate.source),
                        None => one.candidate.source.to_string(),
                    };
                    let colour = if chosen { palette.ink } else { palette.faint };
                    let g = text::line_within(ui, &label, Role::Label, colour, tile);
                    let at = egui::pos2(
                        at.x,
                        text::on_line(&g, square.bottom(), text::row_line(ui, m.row, Role::Label)),
                    );
                    ui.painter().galley(at, g, colour);

                    if response.double_clicked() {
                        take = Some(index);
                    } else if response.clicked() {
                        look_at = Some(index);
                    }
                }

                ui.add_space(14.0);
                if let Some(one) = found.get(*focus) {
                    say(
                        ui,
                        width,
                        &format!("{} — {}", one.candidate.artist, one.candidate.album),
                        Role::Content,
                        palette.ink,
                        m,
                    );
                }
                ui.add_space(6.0);
                say(ui, width, hint, Role::Label, palette.faint, m);
            }
        });
    });

    if let Some(picker) = app.cover_pick.as_mut() {
        picker.across = columns_fitted;
        if let (Some(index), State::Offering { focus, .. }) = (look_at, &mut picker.state) {
            *focus = index;
        }
        if let Some(index) = take {
            picker.take(index);
        }
        if picker.is_busy() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(120));
        }
    }
    taken.response.rect
}
