use super::*;

const MORE_RECENT_WITHIN: f32 = 600.0;

pub(super) fn left_heads(numbered: bool) -> (Option<SortField>, Option<SortField>) {
    if numbered {
        (Some(SortField::Artist), None)
    } else {
        (None, Some(SortField::Artist))
    }
}

pub(super) fn tracks_column(app: &mut GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette) -> Rect {
    if app.editor.is_some() && app.editor_column() == Column::Tracks {
        return tag_form(app, ui, m, palette);
    }
    if app.cover_pick.is_some() {
        return cover_form(app, ui, m, palette);
    }
    if app.details.is_some() {
        return details_form(app, ui, m, palette);
    }
    if app.prompt.is_some() && !app.prompt_inline() && !app.lists_open() {
        return prompt_form(app, ui, m, palette);
    }
    let focused = app.focus == Column::Tracks;
    let shown = !app.unmarked && !matches!(app.focus, Column::Queue | Column::Lists);
    let selected = app.track_index;
    let picked = app.track_selection.clone();
    let more_than_the_cursor = picked.len() > 1 || app.record_picked;
    let playing_id = app.player.current_track().map(|t| t.id);
    let follow = app.scroll_to == Some(Column::Tracks) || app.reveal_all;
    let width = ui.available_width();
    let sort = app.sort;
    let order = app.order;
    let merged = app.merged();
    let columns = if merged {
        app.columns
            .for_rows()
            .with(crate::gui::columns::TrackColumn::Number, app.config.ui.merged_number)
            .with(crate::gui::columns::TrackColumn::Length, app.config.ui.merged_length)
    } else {
        app.columns.clone()
    }
    .fitting(width - m.marker_pad_x * 2.0, CELL_GAP);
    let on_heading: Vec<crate::gui::columns::TrackColumn> = app.columns.on_heading().collect();
    let device_mark = DeviceMark::of(&app.config.ui.device_mark).filter(|_| !app.showing_ipod());
    let on_device = if device_mark.is_some() { app.tracks_on_device() } else { std::sync::Arc::default() };

    let mut clicked: Option<usize> = None;
    let mut double_clicked: Option<usize> = None;
    let mut lifted: Option<usize> = None;
    let mut sort_by: Option<SortField> = None;
    enum Pick {
        One,
        Through,
        Also,
    }
    let mut pick = Pick::One;
    let mut menu_at: Option<(usize, egui::Pos2)> = None;
    enum HeadAction {
        Pick(usize, usize),
        Play(usize, usize),
        Lift(usize, usize),
        Menu(usize, egui::Pos2),
    }
    let mut head_action: Option<HeadAction> = None;
    let mut beside = false;
    let mut below = f32::INFINITY;

    let taken = ui.vertical(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, m.row), Sense::hover());
        let content = content_rect(rect, m);
        let title_width = (content.width() - columns.reserved(CELL_GAP)).max(60.0);

        let mut head = |ui: &mut Ui, label: &str, cell_width: f32, right: bool, field: Option<SortField>, x: f32| {
            let cell =
                Rect::from_min_size(egui::pos2(x, rect.top()), Vec2::new(cell_width, m.row));
            let sense = if field.is_some() { Sense::click() } else { Sense::hover() };
            let response = ui.interact(cell, egui::Id::unique(("track-head", label)), sense);
            if response.clicked() {
                sort_by = field;
            }

            let active = field == Some(sort);
            let offering = field.is_some() && response.hovered();
            let said = crate::lang::t(label);
            let shown = match (active, order) {
                (true, SortOrder::Ascending) => format!("{said} ↓"),
                (true, SortOrder::Descending) => format!("{said} ↑"),
                (false, _) if offering => format!("{said} ↓"),
                (false, _) => said.to_string(),
            };
            let colour = if offering && response.hovered() { palette.ink } else { palette.heading() };
            let g = text::line(ui, &shown, Role::Label, colour);
            let at = if right {
                x + cell_width - g.size().x
            } else {
                x
            };
            let at = egui::pos2(at, text::on_line(&g, rect.top(), text::row_line(ui, m.row, Role::Label)));
            if offering {
                paint_control_hover(ui, control_plate(ui, at, g.size().x), &response, palette);
            }
            ui.painter().galley(at, g, colour);
        };

        let (number_head, title_head) = left_heads(columns.leading().is_some());

        if merged {
            let line = text::row_line(ui, m.row, Role::Label);
            let name = text::line(ui, tr!("alben"), Role::Label, palette.heading());
            let at = egui::pos2(content.left(), text::on_line(&name, rect.top(), line));
            let name_width = name.size().x;
            ui.painter().galley(at, name, palette.heading());
            let records = runs_of(ui, &app.tracks, &app.compiled).len();
            let records_said = if records == 1 { tr!("album") } else { tr!("alben") };
            let own = app.tracks.len() - app.guests.len();
            let count = if own == 1 {
                tr!("{} {} · 1 titel", text::count(records as u64), records_said)
            } else {
                tr!("{} {} · {} titel",
                    text::count(records as u64),
                    records_said,
                    text::count(own as u64),
                )
            };
            let g = text::line(ui, &count, Role::Label, palette.mid);
            if !app.tracks.is_empty() && name_width + HEADING_GAP + g.size().x <= content.width() {
                let at = egui::pos2(
                    content.right() - g.size().x,
                    text::on_line(&g, rect.top(), line),
                );
                ui.painter().galley(at, g, palette.mid);
            }
        }

        let mut x = content.left();
        if merged {
        } else if let Some(leading) = columns.leading() {
            let field = number_head.filter(|_| !merged);
            head(ui, leading.name(), leading.width(), leading.right_aligned(), field, x);
            x += leading.width() + crate::gui::columns::number_gap(CELL_GAP);
        }
        if !merged {
            head(ui, "titel|name", title_width, false, title_head, x);
            x += title_width + CELL_GAP;
            for column in columns.trailing() {
                head(ui, column.name(), column.width(), column.right_aligned(), Some(column.sort_field()), x);
                x += column.width() + CELL_GAP;
            }
        }

        ui.add_space(HEAD_AIR);

        let grouped = merged;
        let head_cover = app.config.ui.heading_covers.then(|| app.config.ui.heading_cover());
        let groups = if grouped { runs_of(ui, &app.tracks, &app.compiled) } else { Default::default() };
        let named_artist = app.selected_artist().is_some();
        let heard_album = app.player.library_track().map(|t| {
            (
                crate::db::sort_key(crate::db::grouping_artist(t)),
                crate::db::sort_key(&t.album),
            )
        });
        if let (true, Some(size)) = (grouped, head_cover) {
            crate::gui::covers::dress_for_headings(app, ui.ctx(), size);
        }

            below = scrolled(ui, "tracks", palette, |ui| {
            beside = ground_clicked(ui, "tracks");
            let mut next_group = 0;
            let seen = ui.clip_rect().expand(m.row * 4.0);
            let off_screen = |ui: &Ui, height: f32| {
                let top = ui.cursor().min.y;
                top > seen.bottom() || top + height < seen.top()
            };
            for (i, track) in app.tracks.iter().enumerate() {
                if let Some(&(start, len)) = groups.get(next_group) {
                    if start == i {
                        next_group += 1;
                        if i > 0 {
                            ui.add_space(ALBUM_HEAD_ABOVE);
                        }
                        let height = album_head_height(head_cover, width, m);
                        if off_screen(ui, height) {
                            ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
                        } else {
                        let run = &app.tracks[start..(start + len).min(app.tracks.len())];
                        let track = run.iter().find(|t| !app.guests.contains(&t.id)).unwrap_or(track);
                        let heard = heard_album.as_ref().is_some_and(|(artist, album)| {
                            run.iter().any(|t| {
                                crate::db::sort_key(crate::db::grouping_artist(t)) == *artist
                                    && crate::db::sort_key(&t.album) == *album
                            })
                        });
                        let mut said = heading_facts(&on_heading, run);
                        let compiled_run = run.iter().any(|t| app.compiled.contains(&t.id));
                        if compiled_run {
                            said.insert(0, tr!("kompilation").to_string());
                        }
                        let taken = |at: usize| (more_than_the_cursor && picked.contains(&at)) || (focused && selected == at);
                        let whole = shown
                            && (start..start + run.len()).all(taken)
                            && (run.len() > 1 || app.record_picked);
                        let marked = RowState {
                            selected: whole,
                            focused: focused && whole,
                            playing: false,
                            muted: false,
                            gone: false,
                            spent: false,
                            offering: false,
                            arrow: false,
                            pointed: false,
                        };
                        let response = album_head(
                            ui,
                            &mut app.cover_cache,
                            width,
                            head_cover,
                            track,
                            &said,
                            app.config.ui.heading_count.then_some(len),
                            !named_artist && !compiled_run,
                            heard,
                            marked,
                            m,
                            palette,
                        );
                        if response.drag_started() {
                            head_action = Some(HeadAction::Lift(start, len));
                        } else if response.double_clicked() {
                            head_action = Some(HeadAction::Play(start, len));
                        } else if response.clicked() {
                            head_action = Some(HeadAction::Pick(start, len));
                        } else if response.secondary_clicked() {
                            if let Some(at) = ui.ctx().pointer_latest_pos() {
                                head_action = Some(HeadAction::Menu(start, at));
                            }
                        }
                        }
                    }
                }
                if off_screen(ui, m.row) {
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, m.row), Sense::hover());
                    keep_visible(ui, rect, m, follow && selected == i);
                    continue;
                }
                let marked = shown && ((more_than_the_cursor && picked.contains(&i)) || (focused && selected == i));
                let state = RowState {
                    selected: marked,
                    focused: focused && marked,
                    playing: playing_id == Some(track.id),
                    muted: false,
                    gone: app.is_gone(&track.path),
                    spent: app.guests.contains(&track.id),
                    offering: false,
                    arrow: false,
                    pointed: false,
                };
                let on_device = device_mark.filter(|_| on_device.contains(&track.id));
                let by = app.compiled.contains(&track.id).then(|| track.shown_artist());
                let response = track_row(ui, width, track, state, on_device, by, &columns, m, palette);
                keep_visible(ui, response.rect, m, follow && selected == i);
                note_cursor_row(ui, "tracks", response.rect, selected == i);
                if response.drag_started() {
                    lifted = Some(i);
                }
                if response.double_clicked() {
                    double_clicked = Some(i);
                } else if response.clicked() {
                    clicked = Some(i);
                    let modifiers = ui.input(|input| input.modifiers);
                    pick = if modifiers.shift {
                        Pick::Through
                    } else if modifiers.command || modifiers.ctrl {
                        Pick::Also
                    } else {
                        Pick::One
                    };
                } else if response.secondary_clicked() {
                    if let Some(at) = ui.ctx().pointer_latest_pos() {
                        menu_at = Some((i, at));
                    }
                }
            }
        });
    });

    if app.merged() && app.config.ui.heading_covers {
        crate::gui::covers::after_headings(app, ui.ctx());
    }

    if below < MORE_RECENT_WITHIN && app.more_recent_to_fetch() {
        app.recent_more += crate::config::RECENT_SHOWN;
        if let Err(e) = app.reload_dependent() {
            app.set_status(format!("{e}"));
        }
    }

    if let Some(action) = head_action {
        match action {
            HeadAction::Pick(start, len) => {
                if let Err(e) = app.select(Column::Tracks, start) {
                    app.set_status(format!("{e}"));
                }
                app.extend_track_selection(start + len - 1);
                app.record_picked = true;
            }
            HeadAction::Play(start, len) => {
                let run = app.tracks[start..(start + len).min(app.tracks.len())].to_vec();
                let _ = app.select(Column::Tracks, start);
                if let Err(e) = app.add_and_play(run, tr!("dieses album")) {
                    app.set_status(format!("{e}"));
                }
            }
            HeadAction::Lift(start, len) => {
                let carried = app.dragged_record(start, len);
                app.start_drag(carried);
            }
            HeadAction::Menu(start, at) => {
                let found = app.tracks[start.min(app.tracks.len())..]
                    .iter()
                    .find(|t| !app.guests.contains(&t.id))
                    .and_then(|track| {
                    let artist = crate::db::sort_key(crate::db::grouping_artist(track));
                    let album = crate::db::sort_key(&track.album);
                    app.albums.iter().position(|a| {
                        crate::db::sort_key(&a.name) == album
                            && crate::db::sort_key(&a.artist) == artist
                    })
                });
                if let Some(index) = found {
                    app.row_menu = Some(crate::gui::RowMenu {
                        at,
                        about: About::Album(index),
                    });
                }
            }
        }
    } else if let Some(field) = sort_by {
        if let Err(e) = app.sort_tracks_by(field) {
            app.set_status(format!("{e}"));
        }
    } else if let Some(index) = lifted {
        let carried = app.dragged_tracks(index);
        app.start_drag(carried);
    } else if let Some(index) = double_clicked {
        let _ = app.select(Column::Tracks, index);
        if let Err(e) = app.play_selected() {
            app.set_status(format!("{e}"));
        }
    } else if let Some(index) = clicked {
        match pick {
            Pick::Through => app.extend_track_selection(index),
            Pick::Also => app.toggle_track_selection(index),
            Pick::One => {
                if let Err(e) = app.select(Column::Tracks, index) {
                    app.set_status(format!("{e}"));
                }
            }
        }
    } else if let Some((index, at)) = menu_at {
        app.unmarked = false;
        if !app.track_selection.contains(&index) {
            if let Err(e) = app.select(Column::Tracks, index) {
                app.set_status(format!("{e}"));
            }
        }
        app.row_menu = Some(crate::gui::RowMenu {
            at,
            about: About::Tracks,
        });
    } else if beside {
        app.let_go();
    }
    taken.response.rect
}

pub(super) const ALBUM_HEAD_ABOVE: f32 = 10.0;

pub(super) const ALBUM_HEAD_GAP: f32 = 14.0;

fn runs_of(
    ui: &Ui,
    tracks: &[crate::db::models::Track],
    compiled: &std::collections::HashSet<i64>,
) -> std::sync::Arc<Vec<(usize, usize)>> {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for track in tracks {
        (track.id, &track.album, &track.artist, &track.album_artist).hash(&mut hasher);
    }
    compiled.len().hash(&mut hasher);
    compiled.iter().fold(0i64, |sum, id| sum.wrapping_add(*id)).hash(&mut hasher);
    let stamp = hasher.finish();
    let id = egui::Id::unique("album-runs");
    if let Some((was, runs)) = ui.data(|d| d.get_temp::<(u64, std::sync::Arc<Vec<(usize, usize)>>)>(id)) {
        if was == stamp {
            return runs;
        }
    }
    let runs = std::sync::Arc::new(album_runs(tracks, compiled));
    ui.data_mut(|d| d.insert_temp(id, (stamp, runs.clone())));
    runs
}

pub(super) fn album_runs(
    tracks: &[crate::db::models::Track],
    compiled: &std::collections::HashSet<i64>,
) -> Vec<(usize, usize)> {
    let key = |t: &crate::db::models::Track| {
        (
            if compiled.contains(&t.id) {
                None
            } else {
                Some(crate::db::sort_key(crate::db::grouping_artist(t)))
            },
            crate::db::sort_key(&t.album),
        )
    };
    let mut runs: Vec<(usize, usize)> = Vec::new();
    let mut last = None;
    for (i, track) in tracks.iter().enumerate() {
        let this = key(track);
        if last.as_ref() == Some(&this) {
            if let Some(run) = runs.last_mut() {
                run.1 += 1;
            }
        } else {
            runs.push((i, 1));
            last = Some(this);
        }
    }
    runs
}

pub(super) fn heading_facts(
    on: &[crate::gui::columns::TrackColumn],
    run: &[crate::db::models::Track],
) -> Vec<String> {
    use crate::gui::columns::TrackColumn;
    let Some(first) = run.first() else {
        return Vec::new();
    };
    on.iter()
        .filter_map(|column| {
            let said = match column {
                TrackColumn::Year => run
                    .iter()
                    .find_map(|t| t.year)
                    .map(|y| y.to_string())
                    .unwrap_or_default(),
                TrackColumn::Genre => run
                    .iter()
                    .map(|t| t.genre.trim())
                    .find(|g| !g.is_empty())
                    .unwrap_or_default()
                    .to_string(),
                TrackColumn::Added => run
                    .iter()
                    .max_by_key(|t| t.date_added)
                    .map(|t| column.value(t))
                    .unwrap_or_default(),
                other => other.value(first),
            };
            (!said.is_empty()).then_some(said)
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub(super) fn album_head_height(cover: Option<f32>, width: f32, m: Metrics) -> f32 {
    let (size, stacked) = head_cover_layout(cover, width, m);
    let words = if stacked { ALBUM_HEAD_UNDER + m.row * (1.0 + crate::gui::theme::head_line()) } else { 0.0 };
    size + words + m.marker_pad_x * 2.0 + m.marker_inset_y
}

fn head_cover_layout(cover: Option<f32>, width: f32, m: Metrics) -> (f32, bool) {
    let room = (width - m.marker_pad_x * 2.0).max(1.0);
    match cover {
        Some(size) => {
            let size = size.min(room);
            (size, size > room / 3.0)
        }
        None => (m.row * 2.0, false),
    }
}

const ALBUM_HEAD_UNDER: f32 = 10.0;

pub(super) fn album_head(
    ui: &mut Ui,
    cache: &mut crate::gui::covers::CoverCache,
    width: f32,
    cover: Option<f32>,
    track: &crate::db::models::Track,
    facts: &[String],
    count: Option<usize>,
    with_artist: bool,
    heard: bool,
    marked: RowState,
    m: Metrics,
    palette: &Palette,
) -> Response {
    let (size, stacked) = head_cover_layout(cover, width, m);
    let pad = m.marker_pad_x;
    let height = album_head_height(cover, width, m);
    let plate_height = height - m.marker_inset_y;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click_and_drag());
    let plate = Rect::from_min_size(rect.min, Vec2::new(rect.width(), plate_height));
    let plated = marked.plated();
    if plated {
        ui.painter().rect_filled(plate, MARKER_RADIUS, marker_colour(palette));
    } else if response.hovered() {
        ui.painter().rect_filled(plate, MARKER_RADIUS, hover_colour(ui, palette));
    }
    let content = content_rect(rect, m);
    let picture = Rect::from_min_size(
        egui::pos2(content.left(), rect.top() + pad),
        Vec2::splat(size),
    );
    if cover.is_some() {
        crate::gui::covers::heading_cover(cache, ui, picture, track, heard, palette);
    }
    let left = if cover.is_some() && !stacked { picture.right() + ALBUM_HEAD_GAP } else { content.left() };
    let room = (content.right() - left).max(1.0);
    let name = if track.album.trim().is_empty() {
        tr!("ohne album")
    } else {
        track.album.as_str()
    };
    let title_colour = if heard { palette.accent } else { palette.ink };

    let mut under: Vec<String> = Vec::new();
    let artist = crate::db::grouping_artist(track);
    if with_artist && !artist.is_empty() {
        under.push(artist.to_string());
    }
    under.extend(facts.iter().cloned());
    if let Some(count) = count {
        under.push(if count == 1 {
            tr!("1 titel").to_string()
        } else {
            tr!("{} titel", text::count(count as u64))
        });
    }
    let under = under.join(" · ");

    let step = m.row * crate::gui::theme::head_line();
    let block = m.row + step;
    let top = if stacked { picture.bottom() + ALBUM_HEAD_UNDER } else { picture.top() + (size - block) / 2.0 };
    let active = response.hovered();
    let ground = if plated {
        marker_colour(palette)
    } else if active {
        hover_colour(ui, palette)
    } else {
        palette.ground
    };
    for (at, (words, role, colour)) in [
        (name, Role::ContentBold, title_colour),
        (under.as_str(), Role::Content, palette.mid),
    ]
    .into_iter()
    .enumerate()
    {
        let cell = Rect::from_min_size(egui::pos2(left, top + step * at as f32), Vec2::new(room, m.row));
        cell_text(ui, cell, response.id.with(at), words, role, colour, active, ground, m);
    }
    response
}
