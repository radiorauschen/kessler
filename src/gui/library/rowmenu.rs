use super::*;

pub(super) fn dropping(app: &mut GuiApp, ui: &Ui, window: Rect) {
    if app.hovering == 0 {
        return;
    }
    let count = app.hovering;
    let palette = app.palette;
    let ctx = ui.ctx().clone();

    let veil = Color32::from_black_alpha(if palette.is_dark() { 168 } else { 112 });
    let said = if count == 1 {
        tr!("loslassen, um es aufzunehmen").to_string()
    } else {
        tr!("loslassen, um {} sachen aufzunehmen", text::count(count as u64))
    };
    let how = if app.config.add_move {
        tr!("musik wird in die bibliothek verschoben · ein bild wird das cover")
    } else {
        tr!("musik wird in die bibliothek kopiert · ein bild wird das cover")
    };

    egui::Area::new(egui::Id::unique("drop-veil"))
        .order(egui::Order::Foreground)
        .fixed_pos(window.min)
        .interactable(false)
        .show(&ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(window.size(), Sense::hover());
            ui.painter().rect_filled(rect, 0.0, veil);

            let first = text::line(ui, &said, Role::Content, palette.ink);
            let second = text::line(ui, how, Role::Label, palette.mid);
            const PAD_X: f32 = 28.0;
            const PAD_Y: f32 = 22.0;
            const GAP: f32 = 10.0;
            let width = first.size().x.max(second.size().x) + PAD_X * 2.0;
            let height = first.size().y + GAP + second.size().y + PAD_Y * 2.0;
            let plate = Rect::from_center_size(rect.center(), Vec2::new(width, height));
            ui.painter().rect_filled(plate, 4.0, palette.ground);

            let mid = plate.center().x;
            ui.painter().galley(
                egui::pos2(mid - first.size().x / 2.0, plate.top() + PAD_Y),
                first.clone(),
                palette.ink,
            );
            ui.painter().galley(
                egui::pos2(
                    mid - second.size().x / 2.0,
                    plate.top() + PAD_Y + first.size().y + GAP,
                ),
                second,
                palette.mid,
            );
        });
}

pub(super) fn show_file(app: &mut GuiApp, path: Option<std::path::PathBuf>) -> crate::error::Result<()> {
    if let Some(path) = path {
        if let Err(why) = crate::gui::reveal::show(&path) {
            app.set_status(why);
        }
    }
    Ok(())
}

pub(super) fn row_menu(app: &mut GuiApp, ui: &Ui, window: Rect) {
    let Some(menu) = app.row_menu.as_ref() else {
        return;
    };
    let (at, about) = (menu.at, menu.about);
    let palette = app.palette;
    let ctx = ui.ctx().clone();
    const WIDTH: f32 = 200.0;
    const ROW: f32 = 24.0;
    const PAD: f32 = 10.0;

    egui::Area::new(egui::Id::unique("row-menu-scrim"))
        .order(egui::Order::Middle)
        .fixed_pos(window.min)
        .show(&ctx, |ui| {
            let (_, response) = ui.allocate_exact_size(window.size(), Sense::click());
            if response.clicked() || response.secondary_clicked() {
                app.row_menu = None;
            }
        });

    let count = app.track_selection.len();
    let to_device = app.attached.is_some() && !app.showing_ipod();
    let on_device = app.attached.is_some() && app.showing_ipod();
    let entries: Vec<(&str, bool)> = match about {
        About::Tracks => vec![
            (tr!("abspielen"), count == 1),
            (tr!("als nächstes"), true),
            (tr!("ans ende"), true),
            (tr!("in eine liste…"), true),
            (tr!("auf den ipod…"), to_device),
            (tr!("idtag bearbeiten"), true),
            (tr!("informationen anzeigen"), count == 1),
            (crate::lang::t(crate::gui::reveal::WORD), count >= 1),
            (tr!("über künstler:in"), count >= 1),
        ]
        .into_iter()
        .chain(on_device.then_some((tr!("vom ipod entfernen…"), count >= 1)))
        .collect(),
        About::Album(_) => vec![
            (tr!("als nächstes"), true),
            (tr!("ans ende"), true),
            (tr!("in eine liste…"), true),
            (tr!("auf den ipod…"), to_device),
            (tr!("cover suchen…"), true),
            (tr!("cover aus datei…"), true),
            (tr!("idtag bearbeiten"), true),
            (crate::lang::t(crate::gui::reveal::WORD), true),
            (tr!("über künstler:in"), true),
        ]
        .into_iter()
        .chain(on_device.then_some((tr!("vom ipod entfernen…"), true)))
        .collect(),
        About::Queued(_) => vec![
            (tr!("abspielen"), true),
            (tr!("entfernen"), true),
            (tr!("warteschlange leeren"), true),
            (tr!("als liste sichern…"), !app.player.queued().is_empty()),
            (crate::lang::t(crate::gui::reveal::WORD), true),
            (tr!("über künstler:in"), true),
        ],
        About::Listed(_) => vec![
            (tr!("abspielen"), true),
            (tr!("aus der liste"), true),
            (tr!("liste schliessen"), true),
            (crate::lang::t(crate::gui::reveal::WORD), true),
            (tr!("über künstler:in"), true),
        ],
        About::List(_) => vec![
            (tr!("abspielen"), true),
            (tr!("bearbeiten"), true),
            (tr!("in die warteschlange"), true),
            (tr!("umbenennen…"), true),
            (tr!("löschen…"), true),
        ],
        About::Station(at) => {
            let station = app.radio_listed().get(at).cloned();
            let kept = station.as_ref().is_some_and(|s| app.radio.as_ref().is_some_and(|r| r.keeps(s)));
            vec![
                (tr!("abspielen"), true),
                (tr!("ähnliche stationen"), true),
                (if kept { tr!("entfernen") } else { tr!("hinzufügen") }, true),
            ]
        }
        About::Episode(at) => app.episode_menu(at),
        About::Artist(_) => vec![(tr!("abspielen"), true), (tr!("über künstler:in"), true)]
            .into_iter()
            .chain(on_device.then_some((tr!("vom ipod entfernen…"), true)))
            .collect(),
    };
    let love = tr!("bei last.fm lieben");
    let to_love: Option<(String, String)> = app.scrobbler.as_ref().and_then(|_| {
        let track = match about {
            About::Tracks if count == 1 => app.selected_tracks().into_iter().next().cloned(),
            About::Queued(index) => app.player.queued().get(index).cloned(),
            About::Listed(index) => app.editing_tracks.get(index).cloned(),
            _ => None,
        }?;
        let artist = Some(track.artist.clone()).filter(|a| !a.is_empty())?;
        (!track.title.is_empty()).then(|| (artist, track.title.clone()))
    });
    let mut entries = entries;
    if to_love.is_some() {
        entries.push((love, true));
    }
    let rated: Option<(Vec<i64>, u8)> = {
        let tracks: Vec<crate::db::models::Track> = match about {
            About::Tracks => app.selected_tracks().into_iter().cloned().collect(),
            About::Queued(index) => app.player.queued().get(index).cloned().into_iter().collect(),
            About::Listed(index) => app.editing_tracks.get(index).cloned().into_iter().collect(),
            _ => Vec::new(),
        };
        let tracks: Vec<_> = tracks.into_iter().filter(|t| t.id >= 0).collect();
        tracks.first().map(|first| {
            let shared = if tracks.iter().all(|t| t.rating == first.rating) { first.rating } else { 0 };
            (tracks.iter().map(|t| t.id).collect(), shared)
        })
    };
    let height = ROW * (entries.len() + usize::from(rated.is_some())) as f32 + PAD * 2.0;
    let x = at.x.min(window.right() - WIDTH - 8.0).max(window.left());
    let y = at.y.min(window.bottom() - height - 8.0).max(window.top());

    let shade = palette.ground.to_srgba_unmultiplied();
    let down = if palette.is_dark() { 0.35 } else { 0.55 };
    let shadow = egui::epaint::Shadow {
        offset: [0, 10],
        blur: 40,
        spread: 0,
        color: Color32::from_rgba_unmultiplied(
            (shade[0] as f32 * down) as u8,
            (shade[1] as f32 * down) as u8,
            (shade[2] as f32 * down) as u8,
            if palette.is_dark() { 150 } else { 70 },
        ),
    };

    let (fill, lit) = (palette.field, palette.marked());

    let stars_at = rated.as_ref().map(|_| entries.len());
    let can = |at: usize| entries.get(at).map_or(Some(at) == stars_at, |(_, on)| *on);
    let places = entries.len() + usize::from(stars_at.is_some());
    if app.row_menu_step != 0 && (0..places).any(can) {
        let n = places as i32;
        let forward = app.row_menu_step > 0;
        let mut at = match app.row_menu_focus {
            Some(at) => at as i32,
            None if forward => -1,
            None => n,
        };
        for _ in 0..app.row_menu_step.unsigned_abs() {
            loop {
                at = (at + if forward { 1 } else { -1 }).rem_euclid(n);
                if can(at as usize) {
                    break;
                }
            }
        }
        app.row_menu_focus = Some(at as usize);
    }
    app.row_menu_step = 0;
    let keyed = app.row_menu_focus.filter(|at| can(*at));
    let on_stars = keyed.is_some() && keyed == stars_at;
    let side = std::mem::take(&mut app.row_menu_side);
    if on_stars && side != 0 {
        if let Some((ids, shared)) = &rated {
            let to = (i32::from(*shared) + side).clamp(0, 5) as u8;
            app.rate_tracks(ids, to);
        }
    }
    let entered = std::mem::take(&mut app.row_menu_enter);
    if entered && on_stars {
        app.row_menu = None;
        return;
    }
    let mut chosen: Option<usize> = if entered { keyed } else { None };
    let mut rate_to: Option<u8> = None;
    egui::Area::new(egui::Id::unique("row-menu"))
        .order(egui::Order::Foreground)
        .fixed_pos(egui::pos2(x, y))
        .constrain(false)
        .show(&ctx, |ui| {
            egui::Frame::NONE
                .fill(fill)
                .corner_radius(4)
                .shadow(shadow)
                .inner_margin(egui::Margin::same(PAD as i8))
                .show(ui, |ui| {
                    ui.set_width(WIDTH - PAD * 2.0);
                    for (i, (label, enabled)) in entries.iter().enumerate() {
                        let (rect, response) = ui.allocate_exact_size(
                            Vec2::new(ui.available_width(), ROW),
                            if *enabled { Sense::click() } else { Sense::hover() },
                        );
                        if (response.hovered() || keyed == Some(i)) && *enabled {
                            ui.painter().rect_filled(rect, MARKER_RADIUS, lit);
                        }
                        let colour = if *enabled { palette.ink } else { palette.faint };
                        let g = text::line(ui, label, Role::Content, colour);
                        let at = egui::pos2(
                            rect.left() + 6.0,
                            text::on_line(&g, rect.top(), text::row_line(ui, ROW, Role::Content)),
                        );
                        ui.painter().galley(at, g, colour);
                        if response.clicked() {
                            chosen = Some(i);
                        }
                    }
                    if let Some((_, shared)) = &rated {
                        if on_stars {
                            let under = Rect::from_min_size(ui.cursor().min, Vec2::new(ui.available_width(), ROW));
                            ui.painter().rect_filled(under, MARKER_RADIUS, lit);
                        }
                        rate_to = star_row(ui, ROW, *shared, &palette);
                    }
                });
        });
    if let (Some(stars), Some((ids, _))) = (rate_to, &rated) {
        app.row_menu = None;
        app.rate_tracks(ids, stars);
        return;
    }

    if let (Some((artist, title)), true) = (&to_love, chosen.is_some_and(|i| entries.get(i).is_some_and(|e| e.0 == love))) {
        app.row_menu = None;
        if let Some(scrobbler) = &app.scrobbler {
            scrobbler.love(artist, title);
        }
        let said = tr!("{} – {} bei last.fm geliebt", artist, title);
        app.note(crate::gui::log::Kind::Lastfm, said.clone());
        app.set_notice(said);
        return;
    }

    let outcome = match (about, chosen) {
        (About::Tracks, Some(0)) => {
            app.row_menu = None;
            app.play_selected()
        }
        (About::Tracks, Some(1)) | (About::Album(_), Some(0)) => {
            app.row_menu = None;
            app.enqueue_selection(true)
        }
        (About::Tracks, Some(2)) | (About::Album(_), Some(1)) => {
            app.row_menu = None;
            app.enqueue_selection(false)
        }
        (About::Tracks, Some(3)) => {
            app.row_menu = None;
            app.ask_for_a_list(app.selection_as_tracks());
            Ok(())
        }
        (About::Tracks, Some(4)) => {
            let tracks = app.selection_as_tracks();
            let what = match tracks.len() {
                1 => tracks
                    .first()
                    .map(|t| t.display_title().to_string())
                    .unwrap_or_default(),
                n => tr!("{} titel", text::count(n as u64)),
            };
            app.plan_transfer(tracks, what, Vec::new())
        }
        (About::Tracks, Some(5)) => {
            app.open_tag_editor();
            Ok(())
        }
        (About::Tracks, Some(6)) => {
            app.open_details();
            Ok(())
        }
        (About::Album(index), Some(2)) => {
            app.row_menu = None;
            match app.album_as_tracks(index) {
                Ok(tracks) => {
                    app.ask_for_a_list(tracks);
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }
        (About::Album(index), Some(3)) => {
            let what = app
                .albums
                .get(index)
                .map(|a| format!("{} — {}", a.shown_name(), a.artist))
                .unwrap_or_else(|| tr!("eine platte").to_string());
            match app.album_as_tracks(index) {
                Ok(tracks) => app.plan_transfer(tracks, what, Vec::new()),
                Err(e) => Err(e),
            }
        }
        (About::Album(index), Some(4)) => app.search_cover(index),
        (About::Album(index), Some(5)) => app.cover_from_file(index),
        (About::Album(index), Some(6)) => app.edit_album_tags(index),
        (About::Tracks, Some(7)) => {
            app.row_menu = None;
            let first = app.selection_as_tracks().into_iter().next();
            show_file(app, first.map(|t| t.path))
        }
        (About::Album(index), Some(7)) => {
            app.row_menu = None;
            match app.album_as_tracks(index) {
                Ok(tracks) => show_file(app, tracks.into_iter().next().map(|t| t.path)),
                Err(e) => Err(e),
            }
        }
        (About::Queued(index), Some(4)) => {
            app.row_menu = None;
            let path = app.player.queued().get(index).map(|t| t.path.clone());
            show_file(app, path)
        }
        (About::Listed(index), Some(3)) => {
            app.row_menu = None;
            let path = app.editing_tracks.get(index).map(|t| t.path.clone());
            show_file(app, path)
        }
        (About::Queued(index), Some(0)) => {
            app.row_menu = None;
            app.play_queued(index)
        }
        (About::Queued(_), Some(1)) => {
            app.row_menu = None;
            app.drop_queued();
            Ok(())
        }
        (About::Queued(_), Some(2)) => {
            app.row_menu = None;
            app.clear_queue();
            Ok(())
        }
        (About::Queued(_), Some(3)) => {
            app.row_menu = None;
            app.queue_as_list();
            Ok(())
        }
        (About::Listed(index), Some(0)) => {
            app.row_menu = None;
            app.play_from_list(index)
        }
        (About::Listed(index), Some(1)) => {
            app.row_menu = None;
            app.drop_from_list(index)
        }
        (About::Listed(_), Some(2)) => {
            app.row_menu = None;
            app.stop_editing();
            Ok(())
        }
        (About::List(at), Some(0)) => {
            app.row_menu = None;
            app.play_list(at)
        }
        (About::List(at), Some(1)) => {
            app.row_menu = None;
            app.edit_list(at)
        }
        (About::List(at), Some(2)) => {
            app.row_menu = None;
            app.list_into_queue(at)
        }
        (About::List(at), Some(3)) => {
            app.row_menu = None;
            app.ask_to_rename_list(at);
            Ok(())
        }
        (About::List(at), Some(4)) => {
            app.row_menu = None;
            app.ask_to_delete_list(at);
            Ok(())
        }
        (About::Episode(at), Some(n)) => {
            app.row_menu = None;
            app.episode_menu_chosen(at, n);
            Ok(())
        }
        (About::Station(at), Some(n)) => {
            app.row_menu = None;
            let station = app.radio_listed().get(at).cloned();
            if n == 1 && app.query().is_some() {
                if let Err(e) = app.close_filter() {
                    app.set_status(format!("{e}"));
                }
            }
            if let (Some(station), Some(radio)) = (station, app.radio.as_mut()) {
                match n {
                    0 => {
                        radio.cursor = at;
                        app.play_station(station);
                    }
                    1 => radio.find_similar(&station),
                    _ => radio.toggle_kept(&station),
                }
            }
            Ok(())
        }
        (About::Artist(at), Some(0)) => {
            app.row_menu = None;
            app.play_artist(at)
        }
        (about, Some(n)) if entries.get(n).is_some_and(|(label, _)| *label == tr!("vom ipod entfernen…")) => {
            app.row_menu = None;
            let taken = match about {
                About::Tracks => Ok(app.selection_as_tracks()),
                About::Album(index) => app.album_as_tracks(index),
                About::Artist(at) => app.artist_as_tracks(at),
                _ => Ok(Vec::new()),
            };
            match taken {
                Ok(tracks) => {
                    let what = match (about, tracks.len()) {
                        (About::Album(index), _) => app
                            .albums
                            .get(index)
                            .map(|a| format!("{} — {}", a.shown_name(), a.artist))
                            .unwrap_or_default(),
                        (About::Artist(at), n) => app
                            .artists
                            .get(at)
                            .map(|a| tr!("{} · {} titel", a.shown_name(), text::count(n as u64)))
                            .unwrap_or_default(),
                        (_, 1) => tracks[0].display_title().to_string(),
                        (_, n) => tr!("{} titel", text::count(n as u64)),
                    };
                    app.ask_to_remove(tracks, what);
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }
        (about, Some(n)) if entries.get(n).is_some_and(|(label, _)| *label == tr!("über künstler:in")) => {
            app.row_menu = None;
            let sung_by = |t: &crate::db::models::Track| {
                if t.artist.trim().is_empty() { t.album_artist.clone() } else { t.artist.clone() }
            };
            let name = match about {
                About::Tracks => app.selection_as_tracks().first().map(sung_by),
                About::Album(index) => app.albums.get(index).map(|a| a.artist.clone()),
                About::Queued(index) => app.player.queued().get(index).map(sung_by),
                About::Listed(index) => app.editing_tracks.get(index).map(sung_by),
                About::Artist(at) => app.artists.get(at).map(|a| a.name.clone()),
                About::List(_) | About::Station(_) | About::Episode(_) => None,
            };
            if let Some(name) = name {
                app.about_artist(&name);
            }
            Ok(())
        }
        _ => Ok(()),
    };
    if let Err(e) = outcome {
        app.set_status(format!("{e}"));
    }
}

fn star_row(ui: &mut Ui, row: f32, rating: u8, palette: &Palette) -> Option<u8> {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), row), Sense::hover());
    let probe = text::line(ui, "★", Role::Content, palette.ink);
    let cell = (probe.size().x + 8.0).max(18.0);
    let line = text::row_line(ui, row, Role::Content);
    let cells: Vec<Rect> = (0..5)
        .map(|i| Rect::from_min_size(egui::pos2(rect.left() + 6.0 - 4.0 + cell * i as f32, rect.top()), Vec2::new(cell, row)))
        .collect();
    let mut over: Option<u8> = None;
    let mut clicked: Option<u8> = None;
    for (i, cell_rect) in cells.iter().enumerate() {
        let response = ui.interact(*cell_rect, egui::Id::unique(("row-menu-star", i)), Sense::click());
        if response.hovered() {
            over = Some(i as u8 + 1);
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if response.clicked() {
            let stars = i as u8 + 1;
            clicked = Some(if stars == rating { 0 } else { stars });
        }
    }
    let lit = over.unwrap_or(rating);
    for (i, cell_rect) in cells.iter().enumerate() {
        let colour = if (i as u8) < lit { palette.ink } else { palette.faint };
        let g = text::line(ui, "★", Role::Content, colour);
        let at = egui::pos2(cell_rect.left() + 4.0, text::on_line(&g, cell_rect.top(), line));
        ui.painter().galley(at, g, colour);
    }
    clicked
}
