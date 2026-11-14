use super::*;

pub(super) fn playing_artist(app: &GuiApp) -> Option<usize> {
    let track = app.player.library_track()?;
    let key = crate::db::sort_key(crate::db::grouping_artist(track));
    app.artists
        .iter()
        .position(|a| crate::db::sort_key(&a.name) == key)
}

pub(super) fn playing_album(app: &GuiApp) -> Option<usize> {
    let track = app.player.library_track()?;
    crate::gui::album_row_for(
        &app.albums,
        &crate::db::sort_key(&track.album),
        &crate::db::sort_key(crate::db::grouping_artist(track)),
    )
}

pub(super) fn artists_column(app: &mut GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette) -> Rect {
    let focused = app.focus == Column::Artists && !app.untouched;
    let selected = app.artist_index;
    let sounding = playing_artist(app);
    let follow = app.scroll_to == Some(Column::Artists) || app.reveal_all;
    let mark = app.artist_sort.mark();
    let counts = app.config.ui.artist_counts;
    let show_all = app.config.ui.show_all_artists;
    let mut clicked: Option<usize> = None;
    let mut opened: Option<usize> = None;
    let mut lifted: Option<usize> = None;
    let mut turn = false;
    let mut menu: Option<(usize, usize, egui::Pos2)> = None;

    let taken = ui.allocate_ui(Vec2::new(m.artists_width, ui.available_height()), |ui| {
        ui.vertical(|ui| {
            let total = app.config.ui.artist_total.then(|| text::count(app.artists.len() as u64));
            let trailing = match &total {
                Some(total) => total.as_str(),
                None if counts => tr!("titel"),
                None => "",
            };
            let head = match total {
                Some(_) => heading_counted(ui, tr!("künstler:innen"), trailing, Some(mark), m, palette),
                None => heading(ui, tr!("künstler:innen"), trailing, Some(mark), None, m, palette),
            };
            if head.clicked() {
                turn = true;
            }
            ui.add_space(HEAD_AIR);
            scrolled(ui, "artists", palette, |ui| {
                for (index, row) in PINNED.iter().enumerate() {
                    let (id, label) = match row {
                        ArtistRow::All if !show_all => continue,
                        ArtistRow::All => ("artist-all", tr!("alle")),
                        ArtistRow::Recent if !app.drawn(Column::Artists, index) => continue,
                        ArtistRow::Recent => ("artist-recent", tr!("zuletzt")),
                        ArtistRow::Compilations if !app.drawn(Column::Artists, index) => continue,
                        ArtistRow::Compilations => ("artist-compilations", tr!("kompilationen")),
                        ArtistRow::Named(_) => continue,
                    };
                    let state = RowState {
                        selected: selected == index,
                        focused: focused && selected == index,
                        arrow: selected == index,
                        muted: true,
                        ..Default::default()
                    };
                    let response = name_row(
                        ui,
                        egui::Id::unique(id),
                        m.artists_width,
                        label,
                        "",
                        Role::Figures,
                        state,
                        m,
                        palette,
                    );
                    keep_visible(ui, response.rect, m, follow && state.selected);
                    note_cursor_row(ui, "artists", response.rect, state.selected);
                    if response.double_clicked() {
                        opened = Some(index);
                    } else if response.clicked() {
                        clicked = Some(index);
                    }
                }

                ui.add_space(BLOCK_AIR);

                for (i, artist) in app.artists.iter().enumerate() {
                    let index = ArtistRow::Named(i).index();
                    let state = RowState {
                        selected: selected == index,
                        focused: focused && selected == index,
                        playing: sounding == Some(i),
                        ..Default::default()
                    };
                    let response = name_row(
                        ui,
                        egui::Id::unique(("artist", &artist.name)),
                        m.artists_width,
                        artist.shown_name(),
                        &if counts {
                            text::count(artist.track_count as u64)
                        } else {
                            String::new()
                        },
                        Role::Figures,
                        state,
                        m,
                        palette,
                    );
                    keep_visible(ui, response.rect, m, follow && state.selected);
                    note_cursor_row(ui, "artists", response.rect, state.selected);
                    if response.drag_started() {
                        lifted = Some(i);
                    } else if response.double_clicked() {
                        opened = Some(index);
                    } else if response.clicked() {
                        clicked = Some(index);
                    } else if response.secondary_clicked() {
                        if let Some(at) = response.interact_pointer_pos() {
                            menu = Some((index, i, at));
                        }
                    }
                }
            });
        });
    });

    if let Some((index, i, at)) = menu {
        if let Err(e) = app.select(Column::Artists, index) {
            app.set_status(format!("{e}"));
        }
        app.row_menu = Some(crate::gui::RowMenu { at, about: About::Artist(i) });
    }
    if turn {
        if let Err(e) = app.cycle_artist_order() {
            app.set_status(format!("{e}"));
        }
    } else if let Some(at) = lifted {
        match app.artist_as_tracks(at) {
            Ok(tracks) => app.start_drag(tracks),
            Err(e) => app.set_status(format!("{e}")),
        }
    } else if let Some(index) = opened {
        match app.artist_row(index) {
            crate::gui::ArtistRow::Named(at) => {
                if let Err(e) = app.play_artist(at) {
                    app.set_status(format!("{e}"));
                }
                if let Err(e) = crate::gui::timed("die spalten daneben neu laden", || {
                    app.select(Column::Artists, index)
                }) {
                    app.set_status(format!("{e}"));
                }
            }
            _ => {
                if let Err(e) = app
                    .select(Column::Artists, index)
                    .and_then(|()| app.select(Column::Tracks, 0))
                    .and_then(|()| app.play_column())
                {
                    app.set_status(format!("{e}"));
                }
            }
        }
        app.focus = Column::Artists;
    } else if let Some(index) = clicked {
        if let Err(e) = app.select(Column::Artists, index) {
            app.set_status(format!("{e}"));
        }
        if app.narrow {
            app.focus = Column::Albums;
        }
    }
    taken.response.rect
}

#[derive(Default)]
pub(super) struct ListRows {
    pub(super) newest_clicked: bool,
    pub(super) newest_opened: bool,
    pub(super) clicked: Option<usize>,
    pub(super) opened: Option<usize>,
    pub(super) menu: Option<(usize, egui::Pos2)>,
    pub(super) make: bool,
    pub(super) dropped_on: Option<usize>,
    pub(super) dropped_new: bool,
    pub(super) let_go: bool,
}

impl ListRows {
    pub(super) fn settle(&self, app: &mut GuiApp) {
        if self.let_go {
            app.cancel_prompt();
        }
        if let Some(at) = self.dropped_on {
            if let Some(crate::gui::Dragged::Incoming(tracks)) = app.dragging.take() {
                if let Err(e) = app.add_to_list(at, &tracks) {
                    app.set_status(format!("{e}"));
                }
            }
        }
        if self.dropped_new {
            if let Some(crate::gui::Dragged::Incoming(tracks)) = app.dragging.take() {
                app.ask_for_a_list(tracks);
            }
        } else if self.make {
            app.ask_for_a_list(Vec::new());
        }
        if let Some((at, where_to)) = self.menu {
            app.row_menu = Some(crate::gui::RowMenu {
                at: where_to,
                about: About::List(at),
            });
        }
        if self.newest_clicked || self.newest_opened {
            let recent = crate::gui::ArtistRow::Recent.index();
            let shown = app.use_device(false).and_then(|()| app.select(Column::Artists, recent));
            let played = if self.newest_opened {
                shown
                    .and_then(|()| app.select(Column::Tracks, 0))
                    .and_then(|()| app.play_column())
            } else {
                shown
            };
            if let Err(e) = played {
                app.set_status(format!("{e}"));
            }
            app.scroll_to = Some(Column::Artists);
            return;
        }
        if let Some(at) = self.opened {
            if let Err(e) = app.select(Column::Lists, at).and_then(|()| app.play_list(at)) {
                app.set_status(format!("{e}"));
            }
        } else if let Some(at) = self.clicked {
            if let Err(e) = app.select(Column::Lists, at) {
                app.set_status(format!("{e}"));
            }
        }
    }
}

pub(super) fn list_rows(app: &GuiApp, ui: &mut Ui, width: f32, m: Metrics, palette: &Palette) -> ListRows {
    let mut out = ListRows::default();
    let focused = app.focus == Column::Lists;
    let cursor = app.lists_index;
    let open = app.editing.and_then(|id| app.lists.iter().position(|(list, _)| list.id == id));
    let follow = app.scroll_to == Some(Column::Lists) || app.reveal_all;
    let carrying = matches!(app.dragging, Some(crate::gui::Dragged::Incoming(_)));
    let pointer = ui.ctx().pointer_latest_pos();
    let released = ui.input(|i| i.pointer.any_released());
    let pressed_at = ui.input(|i| {
        i.pointer
            .any_pressed()
            .then(|| i.pointer.press_origin())
            .flatten()
    });

    let typing = app
        .prompt
        .as_ref()
        .filter(|_| app.prompt_inline())
        .map(|prompt| match prompt.ask {
            crate::gui::Ask::Rename(id) => app.lists.iter().position(|(list, _)| list.id == id),
            _ => Some(app.lists.len()),
        });
    let mut typed_in: Option<Rect> = None;

    let newest = (crate::config::RECENT_SHOWN as u64).min(u64::from(app.total_tracks));
    if newest > 0 && app.config.ui.show_recent {
        let state = RowState {
            selected: app.showing_recent() && !app.showing_ipod(),
            muted: true,
            ..Default::default()
        };
        let response = name_row(
            ui,
            egui::Id::unique("list-newest"),
            width,
            tr!("zuletzt hinzugefügt"),
            &text::count(newest),
            Role::Figures,
            state,
            m,
            palette,
        );
        if response.double_clicked() {
            out.newest_opened = true;
        } else if response.clicked() {
            out.newest_clicked = true;
        }
        ui.add_space(NEWEST_AIR);
    }

    for (i, (list, held)) in app.lists.iter().enumerate() {
        if typing == Some(Some(i)) {
            typed_in = Some(typing_row(app, ui, width, m, palette));
            continue;
        }
        let is_open = open == Some(i);
        let state = RowState {
            selected: is_open || (focused && cursor == i),
            focused: focused && cursor == i,
            playing: is_open,
            ..Default::default()
        };
        let named = list.name.clone();
        let response = name_row(
            ui,
            egui::Id::unique(("list", list.id)),
            width,
            &named,
            &text::count(*held as u64),
            Role::Figures,
            state,
            m,
            palette,
        );
        keep_visible(ui, response.rect, m, follow && state.selected);
        note_cursor_row(ui, "lists", response.rect, state.selected);
        let over = carrying && pointer.is_some_and(|at| response.rect.contains(at));
        if over {
            drop_light(ui, response.rect, &list.name, m, palette);
            if released {
                out.dropped_on = Some(i);
            }
        }
        if response.double_clicked() {
            out.opened = Some(i);
        } else if response.clicked() {
            out.clicked = Some(i);
        }
        if response.secondary_clicked() {
            if let Some(at) = response.interact_pointer_pos() {
                out.menu = Some((i, at));
            }
        }
    }

    if typing == Some(Some(app.lists.len())) {
        typed_in = Some(typing_row(app, ui, width, m, palette));
    } else {
        let at = app.lists.len();
        let state = RowState {
            selected: focused && cursor == at,
            focused: focused && cursor == at,
            ..Default::default()
        };
        let response = name_row(
            ui,
            egui::Id::unique("list-new"),
            width,
            tr!("neue liste…"),
            "",
            Role::Figures,
            state,
            m,
            palette,
        );
        let over = carrying && pointer.is_some_and(|at| response.rect.contains(at));
        if over {
            drop_light(ui, response.rect, tr!("neue liste…"), m, palette);
            if released {
                out.dropped_new = true;
            }
        } else {
            let colour = palette.ink;
            let g = text::line(ui, tr!("neue liste…"), Role::Content, colour);
            let at = egui::pos2(
                content_rect(response.rect, m).left(),
                text::on_line(&g, response.rect.top(), text::row_line(ui, m.row, Role::Content)),
            );
            ui.painter().galley(at, g, colour);
        }
        keep_visible(ui, response.rect, m, follow && state.selected);
        note_cursor_row(ui, "lists", response.rect, state.selected);
        if response.clicked() || response.double_clicked() {
            out.make = true;
        }
    }

    if let (Some(rect), Some(at)) = (typed_in, pressed_at) {
        if !rect.contains(at) {
            out.let_go = true;
        }
    }
    out
}

pub(super) fn drop_light(ui: &Ui, rect: Rect, name: &str, m: Metrics, palette: &Palette) {
    ui.painter().rect_filled(
        rect.shrink2(Vec2::new(0.0, m.marker_inset_y)),
        MARKER_RADIUS,
        palette.marked(),
    );
    let g = text::line(ui, name, Role::Content, palette.ink);
    let at = egui::pos2(
        rect.left() + m.marker_pad_x,
        text::on_line(&g, rect.top(), text::row_line(ui, m.row, Role::Content)),
    );
    ui.painter().galley(at, g, palette.ink);
}

pub(super) fn typing_row(app: &GuiApp, ui: &mut Ui, width: f32, m: Metrics, palette: &Palette) -> Rect {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, m.row), Sense::click());
    let state = RowState {
        selected: true,
        focused: true,
        ..Default::default()
    };
    paint_marker(ui, rect, state, m, palette);
    let Some(prompt) = app.prompt.as_ref() else {
        return rect;
    };
    let content = content_rect(rect, m);
    let line = text::row_line(ui, m.row, Role::Content);
    let now = prompt.entry.text();
    let room = (content.width() - CARET_WIDTH - 2.0).max(1.0);
    let g = text::line_within(ui, now, Role::Content, palette.ink, room);
    let shown = g.size().x;
    let at = egui::pos2(content.left(), text::on_line(&g, rect.top(), line));
    if prompt.entry.is_marked() {
        paint_field_mark(ui, at, &g, rect, palette);
    }
    ui.painter().galley(at, g, palette.ink);
    if !prompt.entry.is_marked() && ui.input(|i| i.time).rem_euclid(1.06) < 0.62 {
        let before = &now[..prompt.entry.caret()];
        let run = text::line(ui, before, Role::Content, palette.ink).size().x.min(shown);
        let caret = Rect::from_min_size(
            egui::pos2(
                (content.left() + run + 1.0).min(content.right() - CARET_WIDTH),
                rect.top() + 6.0,
            ),
            Vec2::new(CARET_WIDTH, m.row - 12.0),
        );
        ui.painter().rect_filled(caret, 0.0, palette.ink);
    }
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(120));
    keep_visible(ui, rect, m, app.scroll_to.is_some());
    let _ = response;
    rect
}

pub(super) fn lists_panel(app: &mut GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette, width: f32) -> Rect {
    let mut close = false;
    let top = ui.cursor().min;
    let height = ui.available_height();

    ui.allocate_ui(Vec2::new(width, height), |ui| {
        ui.set_width(width);
        ui.vertical(|ui| {
            let trailing = if app.lists.is_empty() {
                String::new()
            } else {
                text::count(app.lists.len() as u64)
            };
            if heading(ui, tr!("listen"), &trailing, None, Some(tr!("schliessen")), m, palette).clicked() {
                close = true;
            }
            ui.add_space(HEAD_AIR);

            let wanted = (app.lists.len() + 2) as f32 * m.row + NEWEST_AIR;
            let room = (ui.available_height() * LISTS_SHARE).max(m.row * 3.0);
            let names_height = wanted.min(room);
            let mut rows = ListRows::default();
            ui.allocate_ui(Vec2::new(width, names_height), |ui| {
                ui.set_height(names_height);
                scrolled_with(ui, "lists", palette, 0.0, |ui| {
                    rows = list_rows(app, ui, width, m, palette);
                });
            });
            rows.settle(app);

            ui.add_space(LISTS_GAP);
            let deleting = app
                .prompt
                .as_ref()
                .is_some_and(|p| matches!(p.ask, crate::gui::Ask::Delete(_)));
            if deleting {
                prompt_form(app, ui, m, palette);
            } else if app.editing.is_some() {
                list_column(app, ui, m, palette, width);
            } else if !app.lists.is_empty() {
                say(
                    ui,
                    width,
                    tr!("zum öffnen eine liste anklicken"),
                    Role::Content,
                    palette.faint,
                    m,
                );
            }
        });
    });

    if close {
        app.close_lists();
    }
    Rect::from_min_size(top, Vec2::new(width, height))
}

pub(super) const LISTS_SHARE: f32 = 0.4;

pub(super) const NEWEST_AIR: f32 = 10.0;

pub(super) const LISTS_GAP: f32 = 30.0;

pub(super) fn albums_column(app: &mut GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette) -> Rect {
    if app.editor.is_some() && app.editor_column() == Column::Albums {
        return ui
            .allocate_ui(Vec2::new(m.albums_width, ui.available_height()), |ui| {
                tag_form(app, ui, m, palette)
            })
            .inner;
    }
    let sounding = playing_album(app);
    let focused = app.focus == Column::Albums;
    let selected = app.album_index;
    let follow = app.scroll_to == Some(Column::Albums) || app.reveal_all;
    let mark = app.album_sort.mark();
    let by_date = app.album_sort == crate::gui::ListSort::Added;
    let today = chrono::Local::now().date_naive();
    let mut clicked: Option<usize> = None;
    let mut opened: Option<usize> = None;
    let mut lifted: Option<usize> = None;
    let mut menu_at: Option<(usize, egui::Pos2)> = None;
    let mut turn = false;
    let mut turn_years = false;

    let taken = ui.allocate_ui(Vec2::new(m.albums_width, ui.available_height()), |ui| {
        ui.vertical(|ui| {
            let trailing = match (by_date, app.album_sort) {
                (true, _) => tr!("hinzugefügt").to_string(),
                (false, crate::gui::ListSort::Year) => format!("{} ↓", tr!("jahr")),
                (false, crate::gui::ListSort::YearBack) => format!("{} ↑", tr!("jahr")),
                _ => tr!("jahr").to_string(),
            };
            let head = heading(ui, tr!("alben"), &trailing, Some(mark), None, m, palette);
            if head.clicked() {
                turn = true;
            }
            if head.beside_clicked() && !by_date {
                turn_years = true;
            }
            ui.add_space(HEAD_AIR);
            scrolled(ui, "albums", palette, |ui| {
                let all = RowState {
                    selected: selected == 0,
                    focused: focused && selected == 0,
                    muted: true,
                    ..Default::default()
                };
                let response = name_row(
                    ui,
                    egui::Id::unique("album-all"),
                    m.albums_width,
                    tr!("alle"),
                    "",
                    Role::Figures,
                    all,
                    m,
                    palette,
                );
                keep_visible(ui, response.rect, m, follow && all.selected);
                note_cursor_row(ui, "albums", response.rect, all.selected);
                if response.double_clicked() {
                    opened = Some(0);
                } else if response.clicked() {
                    clicked = Some(0);
                }

                for (i, album) in app.albums.iter().enumerate() {
                    let index = crate::gui::AlbumRow::Named(i).row();
                    let state = RowState {
                        selected: selected == index,
                        focused: focused && selected == index,
                        playing: sounding == Some(i),
                        ..Default::default()
                    };
                    let (figure, figure_role) = if by_date {
                        (text::day(&album.added, today), Role::Content)
                    } else {
                        (
                            album.year.map(|y| y.to_string()).unwrap_or_default(),
                            Role::Figures,
                        )
                    };
                    let response = name_row(
                        ui,
                        egui::Id::unique(("album", &album.name, &album.artist)),
                        m.albums_width,
                        album.shown_name(),
                        &figure,
                        figure_role,
                        state,
                        m,
                        palette,
                    );
                    keep_visible(ui, response.rect, m, follow && state.selected);
                    note_cursor_row(ui, "albums", response.rect, state.selected);
                    if response.drag_started() {
                        lifted = Some(i);
                    } else if response.double_clicked() {
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

    if turn {
        if let Err(e) = app.cycle_album_order() {
            app.set_status(format!("{e}"));
        }
    } else if turn_years {
        if let Err(e) = app.cycle_album_years() {
            app.set_status(format!("{e}"));
        }
    } else if let Some(at) = lifted {
        match app.album_as_tracks(at) {
            Ok(tracks) => app.start_drag(tracks),
            Err(e) => app.set_status(format!("{e}")),
        }
    } else if let Some(index) = opened {
        let started = match app.play_album(index) {
            Ok(started) => started,
            Err(e) => {
                app.set_status(format!("{e}"));
                false
            }
        };
        if let Err(e) = app.select(Column::Albums, index) {
            app.set_status(format!("{e}"));
        }
        if started {
            app.focus = Column::Tracks;
        }
    } else if let Some(index) = clicked {
        if let Err(e) = app.select(Column::Albums, index) {
            app.set_status(format!("{e}"));
        }
        if app.narrow {
            app.focus = Column::Tracks;
        }
    } else if let Some((index, at)) = menu_at {
        if let Err(e) = app.select(Column::Albums, index) {
            app.set_status(format!("{e}"));
        }
        app.row_menu = Some(crate::gui::RowMenu {
            at,
            about: About::Album(index - 1),
        });
    }
    taken.response.rect
}
