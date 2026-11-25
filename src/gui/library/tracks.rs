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
            .with(crate::gui::columns::TrackColumn::Bitrate, false)
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
        let plain = app.config.ui.heading_plain;
        let two_lines = plain || two_lines_beside(head_cover);
        let shelf_order = app.track_order() == (SortField::Artist, SortOrder::Ascending);
        let groups = if grouped || shelf_order { runs_of(ui, &app.tracks, &app.compiled) } else { Default::default() };
        let in_one_piece = || {
            let mut seen = std::collections::HashSet::new();
            groups.iter().all(|&(start, _)| {
                let t = &app.tracks[start];
                seen.insert((crate::db::sort_key(crate::db::grouping_artist(t)), crate::db::sort_key(&t.album)))
            })
        };
        let subheads = !grouped && groups.len() > 1 && in_one_piece();
        let groups = if grouped || subheads { groups } else { Default::default() };
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

            let stack = heads_stack(ui.ctx(), head_cover, width, m);
            below = scrolled(ui, "tracks", palette, |ui| {
            beside = ground_clicked(ui, "tracks");
            let mut next_group = 0;
            let seen = ui.clip_rect().expand(m.row * 4.0);
            let off_screen = |ui: &Ui, height: f32| {
                let top = ui.cursor().min.y;
                top > seen.bottom() || top + height < seen.top()
            };
            for (i, track) in app.tracks.iter().enumerate() {
                let mut head_top: Option<f32> = None;
                if subheads && groups.get(next_group).is_some_and(|&(start, _)| start == i) {
                    next_group += 1;
                    if i > 0 {
                        ui.add_space(ALBUM_HEAD_ABOVE);
                    }
                    head_top = Some(ui.cursor().top());
                    let album = if track.album.trim().is_empty() { tr!("ohne album") } else { track.album.as_str() };
                    let said = match crate::db::grouping_artist(track) {
                        artist if !named_artist && !artist.is_empty() => format!("{album} · {artist}"),
                        _ => album.to_string(),
                    };
                    say(ui, width, &said, Role::Label, palette.heading(), m);
                }
                if let Some(&(start, len)) = groups.get(next_group).filter(|_| grouped) {
                    if start == i {
                        next_group += 1;
                        if i > 0 {
                            ui.add_space(ALBUM_HEAD_ABOVE);
                        }
                        head_top = Some(ui.cursor().top());
                        let run = &app.tracks[start..(start + len).min(app.tracks.len())];
                        let track = run.iter().find(|t| !app.guests.contains(&t.id)).unwrap_or(track);
                        let compiled_run = run.iter().any(|t| app.compiled.contains(&t.id));
                        let artist = Some(crate::db::grouping_artist(track))
                            .filter(|a| !named_artist && !compiled_run && !a.is_empty());
                        let mut said = heading_facts(&on_heading, run);
                        if compiled_run {
                            said.insert(0, tr!("kompilation").to_string());
                        }
                        let under = facts_line(
                            &said,
                            app.config.ui.heading_count.then_some(len),
                            app.config.ui.merged_bitrate.then(|| heading_bitrate(run)).flatten().as_deref(),
                        );
                        let (under, artist) = if plain {
                            let mut line: Vec<String> = artist.map(str::to_string).into_iter().collect();
                            line.extend(said.iter().cloned());
                            if app.config.ui.heading_count {
                                line.push(if len == 1 { tr!("1 titel").to_string() } else { tr!("{} titel", text::count(len as u64)) });
                            }
                            line.extend(app.config.ui.merged_bitrate.then(|| heading_bitrate(run)).flatten());
                            (vec![line.join(" · ")], None)
                        } else if artist.is_none() && !two_lines {
                            (split_facts(ui, &under, words_room(head_cover, width, m)), artist)
                        } else {
                            (vec![under], artist)
                        };
                        let lines = head_lines(artist.is_some(), two_lines);
                        let height = album_head_height(head_cover, width, lines, under.len() - 1, stack, m);
                        if off_screen(ui, height) {
                            ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
                        } else {
                        let heard = heard_album.as_ref().is_some_and(|(artist, album)| {
                            run.iter().any(|t| {
                                crate::db::sort_key(crate::db::grouping_artist(t)) == *artist
                                    && crate::db::sort_key(&t.album) == *album
                            })
                        });
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
                            still: false,
                        };
                        let response = album_head(
                            ui,
                            &mut app.cover_cache,
                            width,
                            head_cover,
                            track,
                            &under,
                            artist,
                            two_lines,
                            plain,
                            heard,
                            marked,
                            stack,
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
                    keep_visible(ui, with_head(rect, head_top), m, follow && selected == i);
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
                    still: false,
                };
                let on_device = device_mark.filter(|_| on_device.contains(&track.id));
                let by = app.compiled.contains(&track.id).then(|| track.shown_artist());
                let response = track_row(ui, width, track, state, on_device, by, &columns, m, palette);
                keep_visible(ui, with_head(response.rect, head_top), m, follow && selected == i);
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
        if let Err(e) = app.fetch_more_recent() {
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

pub(super) fn heading_bitrate(run: &[crate::db::models::Track]) -> Option<String> {
    let rates = run.iter().filter_map(|t| t.bitrate).filter(|kbit| *kbit > 0);
    let (low, high) = rates.fold(None, |span: Option<(u32, u32)>, kbit| {
        Some(span.map_or((kbit, kbit), |(low, high)| (low.min(kbit), high.max(kbit))))
    })?;
    Some(if low == high { tr!("{} kbit/s", low) } else { tr!("{}–{} kbit/s", low, high) })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn head_lines(with_artist: bool, two_lines: bool) -> usize {
    if with_artist && !two_lines { 3 } else { 2 }
}

pub(crate) fn three_lines_tall() -> f32 {
    let row = crate::gui::theme::ROW_ROOMY;
    (row * (2.0 * crate::gui::theme::head_line() + LINE_INK)).ceil()
}

const LINE_INK: f32 = 0.7;

pub(crate) fn two_lines_beside(cover: Option<f32>) -> bool {
    cover.is_some_and(|size| size < three_lines_tall())
}

fn head_block(lines: usize, m: Metrics) -> f32 {
    m.row * (1.0 + crate::gui::theme::head_line() * lines.saturating_sub(1) as f32)
}

fn words_block(lines: usize, more: usize, m: Metrics) -> f32 {
    head_block(lines, m) + text::SMALL_LINE * more as f32
}

pub(super) fn facts_line(facts: &[String], count: Option<usize>, bitrate: Option<&str>) -> String {
    let mut under: Vec<String> = Vec::new();
    if let Some(count) = count {
        under.push(if count == 1 {
            tr!("1 titel").to_string()
        } else {
            tr!("{} titel", text::count(count as u64))
        });
    }
    under.extend(facts.iter().cloned());
    under.extend(bitrate.map(str::to_string));
    under.join(" · ")
}

pub(super) fn split_facts(ui: &Ui, said: &str, room: f32) -> Vec<String> {
    let wide = |words: &str| label_width(ui, words);
    if wide(said) <= room {
        return vec![said.to_string()];
    }
    let parts: Vec<&str> = said.split(" · ").collect();
    let fit = (1..parts.len()).rev().find(|&n| wide(&parts[..n].join(" · ")) <= room);
    match fit {
        Some(n) => vec![parts[..n].join(" · "), parts[n..].join(" · ")],
        None => vec![said.to_string()],
    }
}

fn label_width(ui: &Ui, words: &str) -> f32 {
    use std::cell::RefCell;
    use std::collections::HashMap;
    const KEPT_MOST: usize = 50_000;
    thread_local! {
        static WIDTHS: RefCell<(u64, HashMap<String, f32>)> = RefCell::new((0, HashMap::new()));
    }
    let probe = text::full_line(ui, "Xnmo Ab 0·9", Role::Label, Color32::WHITE).size().x;
    let stamp = (u64::from(probe.to_bits()) << 32) | u64::from(ui.ctx().pixels_per_point().to_bits());
    WIDTHS.with(|widths| {
        let mut widths = widths.borrow_mut();
        if widths.0 != stamp || widths.1.len() > KEPT_MOST {
            *widths = (stamp, HashMap::new());
        }
        if let Some(&width) = widths.1.get(words) {
            return width;
        }
        let width = text::full_line(ui, words, Role::Label, Color32::WHITE).size().x;
        widths.1.insert(words.to_string(), width);
        width
    })
}

fn words_room(cover: Option<f32>, width: f32, m: Metrics) -> f32 {
    let room = (width - m.marker_pad_x * 2.0).max(1.0);
    match head_cover_layout(cover, width, 1, m) {
        (size, false) if cover.is_some() => (room - size - ALBUM_HEAD_GAP).max(1.0),
        _ => room,
    }
}

pub(super) fn album_head_height(cover: Option<f32>, width: f32, lines: usize, more: usize, stack: f32, m: Metrics) -> f32 {
    let (size, _) = head_cover_layout(cover, width, lines, m);
    let block = words_block(lines, more, m);
    let beside = size.max(block);
    let under = size + ALBUM_HEAD_UNDER + block;
    let tall = if cover.is_some() { beside + (under - beside) * stack } else { beside };
    tall + head_pad_top(cover, m) + m.marker_pad_x + m.marker_inset_y
}

fn head_pad_top(cover: Option<f32>, m: Metrics) -> f32 {
    if cover.is_some() { m.marker_pad_x } else { 0.0 }
}

pub(super) fn heads_stack(ctx: &egui::Context, cover: Option<f32>, width: f32, m: Metrics) -> f32 {
    let Some(_) = cover else { return 0.0 };
    let (_, stacked) = head_cover_layout(cover, width, 3, m);
    super::slide(ctx, egui::Id::unique("heads-stacked"), stacked, false)
}

fn head_cover_layout(cover: Option<f32>, width: f32, lines: usize, m: Metrics) -> (f32, bool) {
    let room = (width - m.marker_pad_x * 2.0).max(1.0);
    match cover {
        Some(size) => {
            let size = size.min(room);
            (size, size >= STACKED_FROM || size > room / 3.0)
        }
        None => (words_block(lines, 0, m), false),
    }
}

const STACKED_FROM: f32 = 140.0;

pub(super) const ALBUM_HEAD_UNDER: f32 = 10.0;

pub(super) fn album_head(
    ui: &mut Ui,
    cache: &mut crate::gui::covers::CoverCache,
    width: f32,
    cover: Option<f32>,
    track: &crate::db::models::Track,
    under: &[String],
    artist: Option<&str>,
    two_lines: bool,
    plain: bool,
    heard: bool,
    marked: RowState,
    stack: f32,
    m: Metrics,
    palette: &Palette,
) -> Response {
    let lines = head_lines(artist.is_some(), two_lines);
    let more = under.len().saturating_sub(1);
    let (size, _) = head_cover_layout(cover, width, lines, m);
    let stack = if cover.is_some() { stack } else { 0.0 };
    let block = words_block(lines, more, m);
    let beside_tall = size.max(block);
    let tall = beside_tall * (1.0 - stack) + size * stack;
    let pad = head_pad_top(cover, m);
    let height = album_head_height(cover, width, lines, more, stack, m);
    let plate_height = height - m.marker_inset_y;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click_and_drag());
    let plate = Rect::from_min_size(rect.min, Vec2::new(rect.width(), plate_height));
    let plated = marked.plated();
    let corner = crate::gui::theme::ROW_RADIUS;
    if plated {
        ui.painter().rect_filled(plate, corner, marker_colour(palette));
    } else if response.hovered() {
        ui.painter().rect_filled(plate, corner, hover_colour(ui, palette));
    }
    let content = content_rect(rect, m);
    let picture = Rect::from_min_size(
        egui::pos2(content.left(), rect.top() + pad + (tall - size) / 2.0),
        Vec2::splat(size),
    );
    if cover.is_some() {
        crate::gui::covers::heading_cover(cache, ui, picture, track, heard, palette);
    }

    let name = if track.album.trim().is_empty() {
        tr!("ohne album")
    } else {
        track.album.as_str()
    };
    let title_ink = if heard { palette.accent } else { palette.ink };

    let first = under.first().map(String::as_str).unwrap_or("");

    let step = m.row * crate::gui::theme::head_line();
    let active = response.hovered();
    let ground = if plated {
        marker_colour(palette)
    } else if active {
        hover_colour(ui, palette)
    } else {
        palette.ground
    };
    let flac_ink = marked.secondary(palette);
    let row_line = text::row_line(ui, m.row, Role::Content);
    let x_top = row_line - text::x_rise(ui, Role::ContentBold);
    let last_baseline = row_line
        + if two_lines {
            step
        } else {
            let rows = 1 + usize::from(artist.is_some());
            match under.len() {
                0 => step * (rows - 1) as f32,
                n => step * rows as f32 + text::SMALL_LINE * (n - 1) as f32,
            }
        };
    let beside_top = if cover.is_some() && block < size {
        text::optically_beside(picture.top(), picture.bottom(), x_top, last_baseline)
    } else {
        rect.top() + pad + (beside_tall - block) / 2.0
    };
    let words = |ui: &mut Ui, under_cover: bool, alpha: f32, salt: u8| {
        let left = if cover.is_some() && !under_cover { picture.right() + ALBUM_HEAD_GAP } else { content.left() };
        let room = (content.right() - left).max(1.0);
        let top = if under_cover { picture.bottom() + ALBUM_HEAD_UNDER } else { beside_top };
        let title_colour = title_ink.gamma_multiply(alpha);
        let flac = flac_ink.gamma_multiply(alpha);
        let mid = palette.mid.gamma_multiply(alpha);
        let id = response.id.with(salt);
        let line = |at: usize| Rect::from_min_size(egui::pos2(left, top + step * at as f32), Vec2::new(room, m.row));
        cell_text(ui, line(0), id.with(0), name, Role::ContentBold, title_colour, active, ground, m);
        if two_lines {
            let cell = line(1);
            match artist {
                Some(artist) if !first.is_empty() => {
                    let said = text::line_within(ui, first, Role::Label, flac, (room * 2.0 / 3.0 - MARK_GAP).max(1.0));
                    let artist_room = (room - said.size().x - MARK_GAP).max(1.0);
                    let artist_cell = Rect::from_min_size(cell.min, Vec2::new(artist_room, m.row));
                    cell_text(ui, artist_cell, id.with(1), artist, Role::Content, mid, active, ground, m);
                    let ink = text::full_line(ui, artist, Role::Content, mid).size().x;
                    let at = egui::pos2(
                        left + ink.min(artist_room) + MARK_GAP,
                        text::on_line(&said, cell.top(), text::row_line(ui, m.row, Role::Content)),
                    );
                    ui.painter().galley(at, said, flac);
                }
                Some(artist) => cell_text(ui, cell, id.with(1), artist, Role::Content, mid, active, ground, m),
                None if plain => cell_text(ui, cell, id.with(1), first, Role::Content, mid, active, ground, m),
                None => cell_text(ui, cell, id.with(1), first, Role::Label, flac, active, ground, m),
            }
        } else {
            let mut at = 1;
            if let Some(artist) = artist {
                cell_text(ui, line(at), id.with(at), artist, Role::Content, mid, active, ground, m);
                at += 1;
            }
            for (k, words) in under.iter().enumerate() {
                let cell = line(at).translate(Vec2::new(0.0, text::SMALL_LINE * k as f32));
                cell_text(ui, cell, id.with(at + k), words, Role::Label, flac, active, ground, m);
            }
        }
    };
    if stack <= 0.0 {
        words(ui, false, 1.0, 0);
    } else if stack >= 1.0 {
        words(ui, true, 1.0, 0);
    } else {
        words(ui, false, (1.0 - 2.0 * stack).clamp(0.0, 1.0), 0);
        words(ui, true, (2.0 * stack - 1.0).clamp(0.0, 1.0), 1);
    }
    response
}

fn with_head(row: Rect, head_top: Option<f32>) -> Rect {
    head_top.map_or(row, |top| Rect::from_min_max(egui::pos2(row.left(), top.min(row.top())), row.max))
}
