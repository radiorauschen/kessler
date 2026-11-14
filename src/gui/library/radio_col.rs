use super::*;

pub(super) fn radio_column(app: &mut GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette) -> Rect {
    let width = m.artists_width;
    let taken = ui.allocate_ui(Vec2::new(width, ui.available_height()), |ui| {
        ui.vertical(|ui| {
            stations_head(app, ui, m, palette);
            without_a_bar(ui, "radio").auto_shrink([false, false]).show(ui, |ui| {
                station_rows(app, ui, m, palette, width);
                station_brief(app, ui, m, palette, width);
                heard_rows(app, ui, m, palette, width, true);
            });
        });
    });
    let_go_elsewhere(app, ui, taken.response.rect);
    taken.response.rect
}

fn let_go_elsewhere(app: &mut GuiApp, ui: &Ui, column: Rect) {
    let clicked_out = ui.input(|i| i.pointer.primary_clicked() && i.pointer.interact_pos().is_some_and(|at| !column.contains(at)));
    if clicked_out {
        if let Some(radio) = app.radio.as_mut() {
            radio.resting = true;
        }
    }
}

pub(super) fn radio_row(app: &mut GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette, full_width: f32) {
    let gap = m.column_gap;
    let min = crate::gui::theme::MIN_NAME_COLUMN;
    let side = radio_side(app);
    let last_id = egui::Id::unique("radio-side-last");
    if let Some(side) = side {
        ui.data_mut(|d| d.insert_temp(last_id, side));
    }
    let open = slide(ui.ctx(), egui::Id::unique("radio-side-open"), side.is_some(), false);
    let drawn = side.or_else(|| ui.data(|d| d.get_temp::<crate::gui::Fourth>(last_id))).filter(|_| open > 0.0);
    let even = |columns| crate::gui::theme::even_columns(columns, full_width, gap);
    let stations = app
        .config
        .ui
        .radio_stations_share
        .map_or_else(|| even(3) + (even(4) - even(3)) * open, |share| share * full_width)
        .clamp(min, (full_width - gap * 3.0 - min * 3.0).max(min));
    let rest = (full_width - stations - gap).max(160.0);
    let side_full = app
        .config
        .ui
        .radio_side_share
        .map_or_else(|| even(4), |share| share * rest)
        .clamp(min.min(rest / 3.0), (rest - (min + gap) * 2.0).max(rest / 3.0));
    let pair = (rest - (gap + side_full) * open).max(160.0);
    let inner = (pair - gap).max(160.0);
    let page_share = app.config.ui.radio_page_share.unwrap_or(0.5);
    let page = (inner * page_share).clamp(min.min(inner / 2.0), (inner - min).max(inner / 2.0));
    let heard_width = (inner - page).max(80.0);
    let many = app.all_sides();
    let mut sides: Vec<(crate::gui::Fourth, f32, f32)> = match drawn {
        Some(side) if !many => vec![(side, open, side_full)],
        _ => Vec::new(),
    };
    let (stations, page, heard_width) = if many {
        let slid: Vec<(crate::gui::Fourth, f32)> = crate::gui::Fourth::SIDES
            .into_iter()
            .filter(|side| !matches!(side, crate::gui::Fourth::Device))
            .map(|side| {
                let shown = app.side_is_open(side);
                (side, slide(ui.ctx(), egui::Id::unique(("radio-side-open", side.key())), shown, false))
            })
            .filter(|(_, t)| *t > 0.0)
            .collect();
        let count: f32 = slid.iter().map(|(_, t)| t).sum();
        let unit = ((full_width - gap * (2.0 + count)) / (3.0 + count)).max(80.0);
        sides = slid.into_iter().map(|(side, t)| (side, t, unit)).collect();
        (unit, unit, unit)
    } else {
        (stations, page, heard_width)
    };
    let mut taken: Vec<Rect> = Vec::new();
    ui.horizontal_top(|ui| {
        taken.push(stations_column(app, ui, m, palette, stations));
        ui.add_space(gap);
        let station = ui.allocate_ui(Vec2::new(page, ui.available_height()), |ui| station_page(app, ui, m, palette, page));
        taken.push(station.response.rect);
        ui.add_space(gap);
        let has_programme = app.programme.is_some();
        let head = if has_programme { tr!("programm") } else { tr!("gerade gehört") };
        let heard_rect = ui.allocate_ui(Vec2::new(heard_width, ui.available_height()), |ui| {
            ui.vertical(|ui| {
                still_heading(ui, head, "", m, palette);
                ui.add_space(HEAD_AIR);
                without_a_bar(ui, "radio-heard").auto_shrink([false, false]).show(ui, |ui| {
                    heard_rows(app, ui, m, palette, heard_width, false);
                });
            });
        });
        taken.push(heard_rect.response.rect);
        for (side, open, side_full) in sides {
            let side_width = side_full * open;
            ui.add_space(gap * open);
            let (slot, _) = ui.allocate_exact_size(Vec2::new(side_width, ui.available_height()), Sense::hover());
            let whole = Rect::from_min_size(slot.min, Vec2::new(side_full, slot.height()));
            let mut inside = ui.new_child(egui::UiBuilder::new().max_rect(whole));
            inside.set_clip_rect(slot.intersect(ui.clip_rect()));
            side_column(app, &mut inside, m, palette, side_full, side);
            if open < 1.0 {
                paint_side_fade(ui, slot, 1.0 - open, palette);
            } else {
                taken.push(slot);
            }
        }
    });

    let mut settled = false;
    let handles = if many { 0 } else { taken.len().saturating_sub(1) };
    for index in 0..handles {
        let (left, right) = (taken[index], taken[index + 1]);
        let Some(handle) = divider(ui, 100 + index, left, right, palette) else { continue };
        match handle {
            Handle::Reset => {
                app.config.ui.radio_stations_share = None;
                app.config.ui.radio_side_share = None;
                app.config.ui.radio_page_share = None;
                settled = true;
            }
            Handle::Width(width, done) => {
                settled |= done;
                if index == 0 {
                    app.config.ui.radio_stations_share = Some(width.max(min) / full_width);
                } else if index == 1 {
                    app.config.ui.radio_page_share = Some(width.clamp(min, (inner - min).max(min)) / inner.max(1.0));
                } else {
                    let both = (left.left() - taken[1].left() + width).clamp(min * 2.0 + gap, (rest - gap - min).max(min * 2.0 + gap));
                    app.config.ui.radio_side_share = Some((rest - both - gap) / rest.max(1.0));
                }
            }
        }
    }
    if settled {
        app.save_config();
    }
}

fn side_column(app: &mut GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette, width: f32, side: crate::gui::Fourth) {
    match side {
        crate::gui::Fourth::Artist if app.shown_artist().is_none() => nobody_named(app, ui, m, palette, width),
        side => {
            side_column_of(app, ui, m, palette, width, side, &format!("radio-side-{}", side.key()));
        }
    }
}

pub(super) fn radio_strip(app: &mut GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette, room: f32, height: f32) {
    let gap = crate::gui::theme::SLIDE_GAP;
    let side = radio_side(app);
    let programme = if app.programme.is_some() { tr!("programm") } else { tr!("gerade gehört") };
    let mut names: Vec<&'static str> = vec![tr!("stationen"), tr!("sender"), programme];
    if let Some(side) = side {
        names.push(side_name(side));
    }
    let count = names.len();
    let at_id = egui::Id::unique("radio-strip-at");
    let side_id = egui::Id::unique("radio-strip-side");
    let was_side: Option<crate::gui::Fourth> = ui.data(|d| d.get_temp::<Option<crate::gui::Fourth>>(side_id)).flatten();
    let mut index: usize = ui.data(|d| d.get_temp(at_id)).unwrap_or(0);
    if side.is_some() && side != was_side {
        index = count - 1;
    }
    let index = index.min(count - 1);
    ui.data_mut(|d| {
        d.insert_temp(at_id, index);
        d.insert_temp(side_id, side);
    });
    let offset = ui.ctx().animate_value_with_time(
        egui::Id::unique("radio-slide"),
        crate::gui::theme::slide_offset(index, count, room, gap),
        0.22,
    );
    let mut m = m;
    m.artists_width = room;
    let steps_height = m.row * 1.5;
    let strip_height = (height - steps_height).max(m.row);
    let (area, _) = ui.allocate_exact_size(Vec2::new(room, strip_height), Sense::hover());
    let strip = Rect::from_min_size(
        egui::pos2(area.left() - offset, area.top()),
        Vec2::new(crate::gui::theme::strip_width(count, room, gap), strip_height),
    );
    let mut inside = ui.new_child(egui::UiBuilder::new().max_rect(strip));
    inside.set_clip_rect(area.intersect(ui.clip_rect()));
    inside.horizontal_top(|ui| {
        stations_column(app, ui, m, palette, room);
        ui.add_space(gap);
        ui.allocate_ui(Vec2::new(room, strip_height), |ui| station_page(app, ui, m, palette, room));
        ui.add_space(gap);
        ui.allocate_ui(Vec2::new(room, strip_height), |ui| {
            ui.vertical(|ui| {
                still_heading(ui, programme, "", m, palette);
                ui.add_space(HEAD_AIR);
                without_a_bar(ui, "radio-heard").auto_shrink([false, false]).show(ui, |ui| {
                    heard_rows(app, ui, m, palette, room, false);
                });
            });
        });
        if let Some(side) = side {
            ui.add_space(gap);
            ui.allocate_ui(Vec2::new(room, strip_height), |ui| side_column(app, ui, m, palette, room, side));
        }
    });
    let line = Rect::from_min_size(area.left_bottom(), Vec2::new(room, steps_height));
    if let Some(to) = strip_steps(ui, line, &names, index, m, palette, "radio") {
        ui.data_mut(|d| d.insert_temp(at_id, to));
    }
}

fn side_name(side: crate::gui::Fourth) -> &'static str {
    match side {
        crate::gui::Fourth::Queue => tr!("warteschlange"),
        crate::gui::Fourth::Lists => tr!("listen"),
        crate::gui::Fourth::Device => "ipod",
        crate::gui::Fourth::Log => "log",
        crate::gui::Fourth::Artist => tr!("informationen"),
        crate::gui::Fourth::Lyrics => tr!("liedtext"),
        crate::gui::Fourth::Others => tr!("andere künstler:innen"),
        crate::gui::Fourth::Equalizer => "equalizer",
    }
}

fn going_dots(ui: &Ui) -> String {
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
    ".".repeat(1 + (ui.input(|i| i.time) / 0.4) as usize % 3)
}

fn radio_side(app: &GuiApp) -> Option<crate::gui::Fourth> {
    app.right_side().filter(|side| !matches!(side, crate::gui::Fourth::Device))
}

fn stations_column(app: &mut GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette, width: f32) -> Rect {
    let height = ui.available_height();
    let taken = ui.allocate_ui(Vec2::new(width, height), |ui| {
        ui.vertical(|ui| {
            stations_head(app, ui, m, palette);
            without_a_bar(ui, "radio").auto_shrink([false, false]).show(ui, |ui| {
                station_rows(app, ui, m, palette, width);
            });
        });
    });
    let rect = taken.response.rect;
    let_go_elsewhere(app, ui, rect);
    rect
}

fn stations_head(app: &mut GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette) {
    let count = app.radio_listed().len();
    let trailing = if count == 0 { String::new() } else { text::count(count as u64) };
    let similar = app.query().is_none().then(|| app.radio.as_ref().and_then(|r| r.similar.as_ref().map(|s| s.to.clone()))).flatten();
    if let Some(to) = similar {
        if heading(ui, &tr!("ähnlich wie {}", to), &trailing, None, Some(tr!("schliessen")), m, palette).clicked() {
            if let Some(radio) = app.radio.as_mut() {
                radio.similar = None;
                radio.cursor = 0;
            }
        }
    } else {
        let label = match app.query() {
            Some(_) => tr!("stationen · suche").to_string(),
            None => tr!("stationen").to_string(),
        };
        match found_layout(app, ui, ui.available_width(), m) {
            Found::Columns(widths) => {
                let top = ui.cursor().top();
                let right = ui.cursor().left() + ui.available_width() - m.marker_pad_x;
                still_heading(ui, &label, "", m, palette);
                found_heads(ui, top, right, widths, m, palette);
            }
            _ => still_heading(ui, &label, &trailing, m, palette),
        }
    }
    ui.add_space(HEAD_AIR);
}

enum Found {
    Nothing,
    Beside,
    Columns([f32; 4]),
}

const FOUND_ROLE: Role = Role::Note;

const FOUND_HEADS: [&str; 4] = ["genre", "land", "format", "kbit/s"];

const FOUND_FIGURES: usize = 3;

const FOUND_NAME_LEAST: f32 = 100.0;

fn found_layout(app: &GuiApp, ui: &Ui, width: f32, m: Metrics) -> Found {
    let searching = app.query().is_some() || app.radio.as_ref().is_some_and(|r| r.similar.is_some());
    if !searching {
        return Found::Nothing;
    }
    if app.query().is_some_and(|q| crate::radio::is_address(&q)) {
        return Found::Beside;
    }
    match app.config.ui.radio_found.as_str() {
        "aus" => Found::Nothing,
        "spalten" => {
            let wide = |words: &[&str], role: Role| {
                words.iter().map(|w| text::line(ui, w, role, Color32::WHITE).size().x).fold(0.0, f32::max).ceil()
            };
            let land = wide(&[crate::lang::t(FOUND_HEADS[1])], Role::Label).max(wide(&["mm"], FOUND_ROLE));
            let codec = wide(&[crate::lang::t(FOUND_HEADS[2])], Role::Label).max(wide(&["aac+"], FOUND_ROLE));
            let kbit = wide(&[FOUND_HEADS[3]], Role::Label).max(wide(&["320"], Role::FiguresLabel));
            let left = width - 2.0 * m.marker_pad_x - land - codec - kbit - 4.0 * FOUND_GAP;
            let genre = (left * 0.35).clamp(48.0, 120.0);
            if left - genre >= FOUND_NAME_LEAST {
                Found::Columns([genre, land, codec, kbit])
            } else {
                Found::Beside
            }
        }
        _ => Found::Beside,
    }
}

fn found_heads(ui: &Ui, top: f32, right: f32, widths: [f32; 4], m: Metrics, palette: &Palette) {
    let line = text::row_line(ui, m.row, Role::Label);
    let mut end = right;
    for (at, word) in FOUND_HEADS.iter().enumerate().rev() {
        let word = if at == FOUND_FIGURES { *word } else { crate::lang::t(word) };
        let galley = text::line(ui, word, Role::Label, palette.heading());
        let x = if at == FOUND_FIGURES { end - galley.size().x } else { end - widths[at] };
        ui.painter().galley(egui::pos2(x, text::on_line(&galley, top, line)), galley, palette.heading());
        end -= widths[at] + FOUND_GAP;
    }
}

fn station_rows(app: &mut GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette, width: f32) {
    let Some(radio) = app.radio.as_ref() else { return };
    let searching = app.query().is_some() || radio.similar.is_some();
    let shown = app.radio_listed();
    let kept: Vec<bool> = shown.iter().map(|s| radio.keeps(s)).collect();
    let cursor = radio.cursor.min(shown.len().saturating_sub(1));
    let focused = app.focus == Column::Artists && app.radio_heard_keys.is_none();
    let busy = radio.busy() || app.query().is_some_and(|q| q != radio.asked);
    let on_air = app.on_air.as_ref().map(|s| s.url.clone());
    if shown.is_empty() {
        let said = match (searching, busy) {
            (true, true) => format!("{}{}", tr!("sucht …").trim_end_matches(['…', ' ']), going_dots(ui)),
            (true, false) => tr!("nichts gefunden").to_string(),
            (false, _) => {
                prose(ui, width, tr!("noch keine stationen — oben im suchfeld einen namen suchen oder eine stream-adresse einfügen"), Role::Label, palette.faint, m);
                return;
            }
        };
        say(ui, width, &said, Role::Content, palette.faint, m);
        return;
    }
    let mut play: Option<usize> = None;
    let mut keep: Option<usize> = None;
    let carried_id = egui::Id::unique("radio-station-carried");
    let carried = ui.data(|d| d.get_temp::<usize>(carried_id)).filter(|_| !searching);
    let pointer = ui.input(|i| i.pointer.interact_pos());
    let mut drop_at: Option<(usize, f32)> = None;
    let mut menu: Option<(usize, egui::Pos2)> = None;
    let found = found_layout(app, ui, width, m);
    for (at, station) in shown.iter().enumerate() {
        let ahead = row_ahead(ui, width, m);
        let over = super::pointer_rests_on(ui, ahead) && carried.is_none();
        let word = match (over, kept[at]) {
            (true, true) => tr!("entfernen"),
            (true, false) => tr!("merken"),
            (false, true) if searching => tr!("gemerkt"),
            _ => "",
        };
        let state = RowState {
            playing: on_air.as_deref() == Some(station.url.as_str()),
            selected: at == cursor && (focused || app.radio_heard_keys.is_some()) && !radio.resting,
            focused: focused && !radio.resting,
            offering: over,
            spent: searching && station.silent,
            ..RowState::default()
        };
        let id = egui::Id::unique(("radio-station", &station.url));
        let facts = station.found_facts();
        let silent = tr!("stumm");
        let genre = if station.silent { silent } else { facts.genre };
        let response = match found {
            Found::Columns(widths) => {
                let cells = [
                    Cell { text: genre, width: widths[0], role: FOUND_ROLE, right: false },
                    Cell { text: facts.land, width: widths[1], role: FOUND_ROLE, right: false },
                    Cell { text: facts.codec, width: widths[2], role: FOUND_ROLE, right: false },
                    Cell { text: facts.bitrate, width: widths[3], role: Role::FiguresLabel, right: true },
                ];
                let word = if over { word } else { "" };
                columned_row(ui, id, width, &station.name, &cells, word, kept[at], state, m, palette)
            }
            Found::Beside if !over => {
                let sound = [facts.codec, facts.bitrate].into_iter().filter(|f| !f.is_empty()).collect::<Vec<_>>().join(" ");
                let said: Vec<&str> =
                    [word, genre, facts.land, sound.as_str()].into_iter().filter(|f| !f.is_empty()).collect();
                name_row(ui, id, width, &station.name, &said.join(" · "), FOUND_ROLE, state, m, palette)
            }
            _ => name_row(ui, id, width, &station.name, word, Role::Label, state, m, palette),
        };
        note_cursor_row(ui, "station", response.rect, at == cursor);
        point_at_word(ui, &response, word, over, m, palette);
        if !searching && response.drag_started() {
            ui.data_mut(|d| d.insert_temp(carried_id, at));
        }
        if let (Some(_), Some(p)) = (carried, pointer) {
            let rect = response.rect;
            if p.y >= rect.top() && p.y < rect.bottom() || (at == 0 && p.y < rect.top()) || (at + 1 == shown.len() && p.y >= rect.bottom()) {
                let below = p.y >= rect.center().y;
                drop_at = Some(if below { (at + 1, rect.bottom()) } else { (at, rect.top()) });
            }
        }
        if response.secondary_clicked() {
            if let Some(at_pointer) = response.interact_pointer_pos() {
                menu = Some((at, at_pointer));
            }
        }
        if response.clicked() {
            if on_the_word(ui, &response, word, m, palette) {
                keep = Some(at);
            } else {
                play = Some(at);
            }
        }
    }
    if let Some(from) = carried {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        if let Some((_, y)) = drop_at.filter(|(to, _)| *to != from && *to != from + 1) {
            let line = Rect::from_min_size(egui::pos2(ui.min_rect().left(), y.round() - 1.0), Vec2::new(width, 2.0));
            ui.painter().rect_filled(line, 0.0, palette.ink);
        }
        if ui.input(|i| i.pointer.any_released()) {
            ui.data_mut(|d| d.remove_temp::<usize>(carried_id));
            if let (Some((to, _)), Some(radio)) = (drop_at, app.radio.as_mut()) {
                radio.move_kept(from, to);
            }
        }
    }
    if let Some((index, at)) = menu {
        if let Some(radio) = app.radio.as_mut() {
            radio.cursor = index;
        }
        app.row_menu = Some(crate::gui::RowMenu { at, about: crate::gui::About::Station(index) });
    }
    if let Some(at) = keep {
        if let Some(radio) = app.radio.as_mut() {
            radio.toggle_kept(&shown[at]);
        }
    } else if let Some(at) = play {
        if let Some(radio) = app.radio.as_mut() {
            radio.cursor = at;
            radio.resting = false;
        }
        app.play_station(shown[at].clone());
    }
}

pub(in crate::gui) fn heard_follow_id() -> egui::Id {
    egui::Id::unique("radio-heard-follow")
}

enum Heard {
    Keep(usize),
    About(String),
}

fn heard_rows(app: &mut GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette, width: f32, headed: bool) {
    let searching = app.query().is_some();
    let Some(radio) = app.radio.as_ref() else { return };
    let one_column = headed;
    let hidden = searching && one_column;
    let aired: Vec<crate::radio::Aired> = if hidden { Vec::new() } else { app.aired.clone() };
    let songs: Vec<crate::radio::Song> = if hidden { Vec::new() } else { radio.songs.clone() };
    let songs_kept: Vec<bool> = aired.iter().map(|a| radio.keeps_song(a)).collect();
    let twelve = false;
    let programme = app.programme.as_ref().map(|p| (p.past.clone(), p.coming.clone(), p.waiting())).filter(|_| !hidden);
    let mut asked: Option<Heard> = None;
    let mut forget_song: Option<usize> = None;

    let now = chrono::Local::now();
    let lang = crate::lang::current();
    let at = |time: chrono::DateTime<chrono::Local>| clock_words(time.naive_local(), twelve, false, lang);
    let left_of = |show: &crate::radio::programme::Show| {
        let left = (show.end - now).num_minutes().max(1);
        if left >= 60 { tr!("noch {} h {} min", left / 60, left % 60) } else { tr!("noch {} min", left) }
    };
    let loading = |ui: &mut Ui| {
        let dots = ".".repeat(1 + (ui.input(|i| i.time) / 0.4) as usize % 3);
        say(ui, width, &format!("{}{}", tr!("lade programm"), dots), Role::Label, palette.faint, m);
    };
    let silent = app.player.on_air_introduced().is_some_and(|i| !i.titles);
    let why = match (app.on_air.is_some(), silent) {
        (true, true) => tr!("der sender sagt nicht, was läuft"),
        (true, false) => tr!("der sender hat noch nicht gesagt, was läuft"),
        (false, _) => tr!("es läuft gerade kein sender"),
    };
    let said_why = aired.is_empty() && !one_column;

    let mut programme_drawn = false;
    if let Some((past, coming, waiting)) = &programme {
        if headed {
            ui.add_space(m.row);
            still_heading(ui, tr!("programm"), "", m, palette);
            ui.add_space(HEAD_AIR);
        }
        if *waiting {
            loading(ui);
        }
        for show in past {
            let state = RowState { spent: true, ..RowState::default() };
            let id = egui::Id::unique(("radio-show", show.start.timestamp(), &show.title));
            name_row(ui, id, width, &show.title, &at(show.start), Role::FiguresLabel, state, m, palette);
        }
        for (i, show) in coming.iter().take(PROGRAMME_AHEAD).enumerate() {
            let on = i == 0 && show.start <= now;
            let (end, role) = if on { (left_of(show), Role::Label) } else { (at(show.start), Role::FiguresLabel) };
            let state = RowState { playing: on, ..RowState::default() };
            let id = egui::Id::unique(("radio-show", show.start.timestamp(), &show.title));
            name_row(ui, id, width, &show.title, &end, role, state, m, palette);
        }
        programme_drawn = true;
    }
    if (programme_drawn || headed) && !aired.is_empty() {
        ui.add_space(m.row);
        still_heading(ui, tr!("gerade gehört"), "", m, palette);
        ui.add_space(HEAD_AIR);
    }
    let said_why = said_why && !programme_drawn;
    if said_why {
        say(ui, width, why, Role::Content, palette.faint, m);
    }
    let keys_at = app.radio_heard_keys;
    let follow = ui.data_mut(|d| d.remove_temp::<bool>(heard_follow_id())).unwrap_or(false);
    for i in 0..aired.len() {
        let (_, heard) = aired_row(ui, width, m, palette, &aired[i], i, songs_kept[i], twelve, keys_at == Some(i), follow);
        if heard.is_some() {
            asked = heard;
        }
    }
    if app.on_air.is_some() && !programme_drawn && !aired.is_empty() {
        ui.add_space(BLOCK_AIR);
        say(ui, width, tr!("der sender sagt nicht, was als nächstes kommt"), Role::Label, palette.faint, m);
    }

    if !songs.is_empty() {
        if programme_drawn || headed || !aired.is_empty() || said_why {
            ui.add_space(m.row);
        }
        still_heading(ui, tr!("gemerkte lieder"), &text::count(songs.len() as u64), m, palette);
        ui.add_space(HEAD_AIR);
        for (at, song) in songs.iter().enumerate() {
            let words = if song.artist.is_empty() {
                song.title.clone()
            } else {
                format!("{} — {}", song.artist, song.title)
            };
            let ahead = row_ahead(ui, width, m);
            let end = if super::pointer_rests_on(ui, ahead) { tr!("entfernen") } else { "" };
            let id = egui::Id::unique(("radio-song", at, &song.title));
            let here = keys_at == Some(aired.len() + at);
            let state = RowState { offering: !end.is_empty(), selected: here, focused: here, ..RowState::default() };
            let response = name_row(ui, id, width, &words, end, Role::Label, state, m, palette);
            if here && follow {
                response.scroll_to_me(None);
            }
            point_at_word(ui, &response, end, !end.is_empty(), m, palette);
            if response.clicked() {
                if on_the_word(ui, &response, end, m, palette) {
                    forget_song = Some(at);
                } else if !song.artist.is_empty() {
                    asked = Some(Heard::About(song.artist.clone()));
                }
            }
        }
    }

    match asked {
        Some(Heard::About(name)) => app.about_artist(&name),
        Some(Heard::Keep(at)) => app.keep_aired(&aired[at]),
        None => {}
    }
    if let Some(at) = forget_song {
        if let Some(radio) = app.radio.as_mut() {
            radio.forget_song(&songs[at]);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn aired_row(
    ui: &mut Ui,
    width: f32,
    m: Metrics,
    palette: &Palette,
    line: &crate::radio::Aired,
    at: usize,
    kept: bool,
    twelve: bool,
    keyed: bool,
    follow: bool,
) -> (Rect, Option<Heard>) {
    let words = match &line.artist {
        Some(artist) => format!("{artist} — {}", line.title),
        None => line.title.clone(),
    };
    let ahead = row_ahead(ui, width, m);
    let over = super::pointer_rests_on(ui, ahead);
    let when = clock_words(line.at.naive_local(), twelve, false, crate::lang::current());
    let (end, role) = match (over, kept) {
        (true, true) => (tr!("entfernen").to_string(), Role::Label),
        (true, false) => (tr!("merken").to_string(), Role::Label),
        (false, true) => (tr!("gemerkt").to_string(), Role::Label),
        (false, false) => (when, Role::FiguresLabel),
    };
    let state = RowState { playing: at == 0, spent: at > 0, offering: over, selected: keyed, focused: keyed, ..RowState::default() };
    let id = egui::Id::unique(("radio-aired", at, &line.said));
    let response = name_row(ui, id, width, &words, &end, role, state, m, palette);
    if keyed && follow {
        response.scroll_to_me(None);
    }
    point_at_word(ui, &response, &end, over, m, palette);
    let mut heard = None;
    if response.clicked() {
        if over && on_the_word(ui, &response, &end, m, palette) {
            heard = Some(Heard::Keep(at));
        } else if let Some(artist) = &line.artist {
            heard = Some(Heard::About(artist.clone()));
        }
    }
    (response.rect, heard)
}

struct Known {
    station: crate::radio::Station,
    intro: crate::player::stream::Introduction,
    site: String,
    wiki: Option<crate::radio::AboutText>,
    asking: bool,
}

impl Known {
    fn of(app: &mut GuiApp, station: crate::radio::Station) -> Known {
        let intro = (app.player.on_air() == Some(station.url.as_str()))
            .then(|| app.player.on_air_introduced())
            .flatten()
            .unwrap_or_default();
        let site = if station.homepage.is_empty() { intro.url.clone() } else { station.homepage.clone() };
        let lang = crate::lang::current().key();
        let (wiki, asking) = match app.radio.as_mut() {
            Some(radio) => (radio.about(&site).and_then(|a| a.in_lang(lang)).cloned(), radio.asking_about(&site)),
            None => (None, false),
        };
        Known { station, intro, site, wiki, asking }
    }

    fn facts(&self) -> String {
        let genre = if self.station.tags.is_empty() { self.intro.genre.to_lowercase() } else { self.station.tags.clone() };
        let kbit = if self.intro.bitrate.is_empty() { self.station.bitrate.as_str() } else { self.intro.bitrate.as_str() };
        let bitrate = if kbit.is_empty() { String::new() } else { tr!("{} kbit/s", kbit) };
        let votes = match self.station.votes {
            0 => String::new(),
            1 => tr!("1 stimme im verzeichnis").to_string(),
            n => tr!("{} stimmen im verzeichnis", text::count(u64::from(n))),
        };
        let silent = if self.station.silent { tr!("zuletzt stumm, laut verzeichnis").to_string() } else { String::new() };
        let sent_as_hls = self.station.hls || self.intro.hls || crate::player::hls::is_hls(&self.station.url, "");
        let hls = if sent_as_hls { "hls" } else { "" };
        let said = |parts: &[&str]| parts.iter().filter(|p| !p.trim().is_empty()).copied().collect::<Vec<_>>().join(" · ");
        [
            said(&[&self.station.country, &self.station.language]),
            genre,
            said(&[&self.station.codec, hls, &bitrate]),
            votes,
            silent,
        ]
            .into_iter()
            .filter(|line| !line.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn words(&self, ui: &mut Ui, width: f32, most: usize, m: Metrics, palette: &Palette) {
        let lang = crate::lang::current().key();
        let cut = |text: &str| brief(text, most);
        if let Some(wiki) = &self.wiki {
            ui.add_space(BLOCK_AIR * 2.0);
            let said = if wiki.lang == lang { tr!("aus {}", "wikipedia") } else { tr!("aus {} ({})", "wikipedia", wiki.lang) };
            say(ui, width, &said, Role::Label, palette.faint, m);
            prose(ui, width, &cut(&wiki.text), Role::Content, palette.mid, m);
        } else if self.asking {
            ui.add_space(BLOCK_AIR * 2.0);
            let dots = ".".repeat(1 + (ui.input(|i| i.time) / 0.4) as usize % 3);
            say(ui, width, &format!("{}{}", tr!("lade informationen"), dots), Role::Label, palette.faint, m);
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        } else if !self.intro.description.is_empty() {
            ui.add_space(BLOCK_AIR * 2.0);
            say(ui, width, tr!("vom sender selbst"), Role::Label, palette.faint, m);
            prose(ui, width, &cut(&self.intro.description), Role::Content, palette.mid, m);
        }
    }

    fn links(&self, ui: &mut Ui, width: f32, m: Metrics, palette: &Palette) -> Option<String> {
        let mut links: Vec<(String, String)> = Vec::new();
        if let Some(wiki) = &self.wiki {
            links.push((tr!("wikipedia").to_string(), wiki.page.clone()));
        }
        if !self.site.is_empty() {
            let shown = self.site.trim_start_matches("https://").trim_start_matches("http://");
            let shown = shown.split(['?', '#']).next().unwrap_or(shown);
            let shown = shown.trim_start_matches("www.").trim_end_matches('/');
            links.push((shown.to_string(), self.site.clone()));
        }
        if links.is_empty() {
            return None;
        }
        ui.add_space(BLOCK_AIR * 2.0);
        say(ui, width, tr!("mehr lesen"), Role::Label, palette.faint, m);
        let mut clicked = None;
        for (word, url) in &links {
            if link_action(ui, width, word, m, palette) {
                clicked = Some(url.clone());
            }
        }
        clicked
    }
}

fn station_page(app: &mut GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette, width: f32) {
    let listed = app.radio_listed();
    let at = app.radio.as_ref().map_or(0, |r| r.cursor);
    let Some(station) = app.on_air.clone().or_else(|| listed.get(at).cloned()) else {
        ui.vertical(|ui| {
            still_heading(ui, tr!("sender"), "", m, palette);
            ui.add_space(HEAD_AIR);
            say(ui, width, tr!("nichts ausgewählt"), Role::Content, palette.faint, m);
        });
        return;
    };
    let known = Known::of(app, station.clone());
    let mut open_url = None;
    ui.allocate_ui(Vec2::new(width, ui.available_height()), |ui| {
        ui.set_width(width);
        ui.vertical(|ui| {
            still_heading(ui, tr!("sender"), "", m, palette);
            ui.add_space(HEAD_AIR);
            without_a_bar(ui, "radio-station-page").auto_shrink([false, false]).show(ui, |ui| {
                let inner = (width - m.marker_pad_x * 2.0).max(40.0);
                let side = STATION_PAGE_PICTURE.min(inner / 3.0);
                let beside = (inner - side - BRIEF_GAP).max(40.0);
                let name = text::paragraph(ui, &station.name, Role::ContentBold, palette.ink, beside);
                let facts = text::prose(ui, &known.facts(), Role::Label, palette.mid, beside);
                let block = name.size().y + NAME_TO_FACTS + facts.size().y;
                let inset = m.marker_inset_y;
                let height = side.max(block) + inset;
                let (area, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
                let area = Rect::from_min_max(egui::pos2(area.left(), area.top() + inset), area.max);
                let left = content_rect(area, m).left();
                let rect = Rect::from_min_size(egui::pos2(left, area.top()), Vec2::splat(side));
                station_picture(app, ui, rect, palette, &station);
                let at = egui::pos2(left + side + BRIEF_GAP, area.top() + ((side - block) / 2.0).max(0.0));
                let below = at + Vec2::new(0.0, name.size().y + NAME_TO_FACTS);
                ui.painter().galley(at, name, palette.ink);
                ui.painter().galley(below, facts, palette.mid);
                known.words(ui, width, usize::MAX, m, palette);
                open_url = known.links(ui, width, m, palette);
            });
        });
    });
    if let Some(url) = open_url {
        if !crate::lastfm::open_in_browser(&url) {
            app.set_status(tr!("der browser liess sich nicht öffnen"));
        }
    }
}

const HEARD_HERE: usize = 8;

fn nobody_named(app: &mut GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette, width: f32) {
    let mut close = false;
    let show = app.show_on_now();
    let about = app.show_about();
    let picture = app.show_picture();
    let links = app.show_links();
    let station = app.on_air.as_ref().map(|s| s.name.clone()).unwrap_or_default();
    let said = app.aired.first().map(|a| a.said.clone());
    let before = if show.is_none() { app.heard_on_station(&station, HEARD_HERE) } else { Vec::new() };
    let kept: Vec<String> = app
        .radio
        .as_ref()
        .map(|r| {
            let mut artists: Vec<String> = Vec::new();
            for song in r.songs.iter().filter(|s| s.station == station && !s.artist.is_empty()) {
                if !artists.iter().any(|a| crate::db::sort_key(a) == crate::db::sort_key(&song.artist)) {
                    artists.push(song.artist.clone());
                }
            }
            artists
        })
        .unwrap_or_default();
    let mut look_up: Option<String> = None;
    let mut open_url = None;
    ui.allocate_ui(Vec2::new(width, ui.available_height()), |ui| {
        ui.set_width(width);
        ui.vertical(|ui| {
            if heading(ui, tr!("informationen"), "", None, Some(tr!("schliessen")), m, palette).clicked() {
                close = true;
            }
            ui.add_space(HEAD_AIR);
            let Some(show) = show else {
                let said_here = if station.is_empty() {
                    tr!("es läuft gerade kein sender")
                } else {
                    tr!("der sender nennt gerade keine künstler:in")
                };
                say(ui, width, said_here, Role::Content, palette.faint, m);
                if let Some(said) = said.as_deref().filter(|s| !s.is_empty()) {
                    ui.add_space(BLOCK_AIR * 2.0);
                    say(ui, width, tr!("der sender sagt gerade"), Role::Label, palette.faint, m);
                    paragraph(ui, width, said, Role::Content, palette.ink, m);
                }
                for (head, artists) in [(tr!("zuletzt hier gehört"), &before), (tr!("gemerkt von hier"), &kept)] {
                    if artists.is_empty() {
                        continue;
                    }
                    ui.add_space(BLOCK_AIR * 2.0);
                    say(ui, width, head, Role::Label, palette.faint, m);
                    for artist in artists.iter() {
                        if link_action(ui, width, &crate::gui::theme::lower(artist), m, palette) {
                            look_up = Some(artist.clone());
                        }
                    }
                }
                return;
            };
            paragraph(ui, width, &show, Role::Content, palette.ink, m);
            if let Some(path) = picture {
                ui.add_space(BLOCK_AIR);
                let inner = (width - m.marker_pad_x * 2.0).max(40.0);
                let side = picture_side(inner, app.config.ui.info_picture_wide);
                let (area, _) = ui.allocate_exact_size(Vec2::new(width, side), Sense::hover());
                let rect = Rect::from_min_size(egui::pos2(content_rect(area, m).left(), area.top()), Vec2::splat(side));
                picture_at(app, ui, rect, palette, &path, &show);
            }
            if let Some(about) = about.as_deref().filter(|a| !a.is_empty()) {
                ui.add_space(BLOCK_AIR);
                prose(ui, width, about, Role::Content, palette.mid, m);
            }
            if !links.is_empty() {
                ui.add_space(BLOCK_AIR * 2.0);
                say(ui, width, tr!("mehr lesen"), Role::Label, palette.faint, m);
                for (words, url) in &links {
                    if link_action(ui, width, &crate::gui::theme::lower(words), m, palette) {
                        open_url = Some(url.clone());
                    }
                }
            }
        });
    });
    if close {
        app.toggle_artist();
    }
    if let Some(artist) = look_up {
        app.about_artist(&artist);
    }
    if let Some(url) = open_url {
        if !crate::lastfm::open_in_browser(&url) {
            app.set_status(tr!("der browser liess sich nicht öffnen"));
        }
    }
}

fn station_brief(app: &mut GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette, width: f32) {
    let Some(station) = app.on_air.clone() else {
        return;
    };
    let known = Known::of(app, station.clone());

    ui.add_space(m.row);
    still_heading(ui, &station.name, "", m, palette);
    ui.add_space(HEAD_AIR);

    let inner = (width - m.marker_pad_x * 2.0).max(40.0);
    let side = STATION_PICTURE.min(inner / 3.0);
    let beside = (inner - side - BRIEF_GAP).max(40.0);
    let on_air = app.on_air.as_ref().is_some_and(|s| s.url == station.url);
    let show = app.show_on_now().filter(|_| on_air);
    let show = show.map(|show| text::paragraph(ui, &show, Role::Content, palette.ink, beside));
    let facts = text::prose(ui, &known.facts(), Role::Label, palette.mid, beside);
    let above = show.as_ref().map_or(0.0, |g| g.size().y + 2.0);
    let height = side.max(above + facts.size().y);
    let (area, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let left = content_rect(area, m).left();
    let picture = Rect::from_min_size(egui::pos2(left, area.top()), Vec2::splat(side));
    station_picture(app, ui, picture, palette, &station);
    let at = egui::pos2(left + side + BRIEF_GAP, area.top());
    if let Some(show) = show {
        ui.painter().galley(at, show, palette.ink);
    }
    ui.painter().galley(at + Vec2::new(0.0, above), facts, palette.mid);

    known.words(ui, width, BRIEF_CHARS, m, palette);
    if let Some(url) = known.links(ui, width, m, palette) {
        if !crate::lastfm::open_in_browser(&url) {
            app.set_status(tr!("der browser liess sich nicht öffnen"));
        }
    }
}

const PROGRAMME_AHEAD: usize = 4;

const STATION_PAGE_PICTURE: f32 = 88.0;
const NAME_TO_FACTS: f32 = 4.0;
const STATION_PICTURE: f32 = 56.0;
const BRIEF_GAP: f32 = 12.0;
const BRIEF_CHARS: usize = 280;

fn brief(text: &str, most: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= most {
        return text.to_string();
    }
    let mut kept = String::new();
    for sentence in text.split_inclusive(". ") {
        if !kept.is_empty() && kept.chars().count() + sentence.chars().count() > most {
            break;
        }
        kept.push_str(sentence);
    }
    let kept = kept.trim_end().to_string();
    if kept.chars().count() <= most + most / 3 {
        return kept;
    }
    let mut cut: String = kept.chars().take(most).collect();
    if let Some(at) = cut.rfind(' ') {
        cut.truncate(at);
    }
    format!("{}…", cut.trim_end_matches([',', ';', ':']))
}

fn station_picture(app: &GuiApp, ui: &Ui, rect: Rect, palette: &Palette, station: &crate::radio::Station) {
    match crate::radio::picture_path(station) {
        Some(path) => picture_at(app, ui, rect, palette, &path, &station.name),
        None => crate::gui::covers::blank(ui, rect, &station.name, palette, false),
    }
}

pub(super) fn picture_at(app: &GuiApp, ui: &Ui, rect: Rect, palette: &Palette, path: &std::path::Path, name: &str) {
    let dithered = app.config.ui.cover_dither;
    let grain = crate::gui::dither::grain_for(rect.width(), ui.ctx().pixels_per_point(), app.config.ui.picture_fineness());
    let method = crate::gui::dither::Look::of(&app.config.ui);
    let negative = palette.screens_negative();
    let generation = PICTURES_GENERATION.load(std::sync::atomic::Ordering::Relaxed);
    let id = egui::Id::unique(("station-picture", path, grain, dithered, format!("{method:?}"), negative, generation));
    let mut held = held_texture(id).map(Some);
    if held.is_none() {
        let loaded = decoded_picture(ui.ctx(), path).map(|decoded| {
            let image = if dithered {
                crate::gui::covers::screened(&decoded, grain, method, negative)
            } else {
                let rgba = decoded.to_rgba8();
                egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw())
            };
            ui.ctx().load_texture("station-picture", image, egui::TextureOptions::NEAREST)
        });
        if let Some(handle) = &loaded {
            keep_texture(id, handle.clone());
        }
        held = Some(loaded);
    }
    match held.flatten() {
        Some(handle) => {
            let tint = if dithered { palette.ink } else { Color32::WHITE };
            if dithered {
                crate::gui::covers::halftone_ground(ui, rect, palette);
            }
            let under = if dithered { palette.tones_under(tint, method.tones) } else { Vec::new() };
            crate::gui::covers::paint_screened(ui, rect, MARKER_RADIUS, handle.id(), &under, tint);
        }
        None => crate::gui::covers::blank(ui, rect, name, palette, false),
    }
}

pub(super) fn wide_picture(app: &GuiApp, ui: &mut Ui, width: f32, palette: &Palette, path: &std::path::Path) {
    let Some(decoded) = decoded_picture(ui.ctx(), path) else { return };
    if decoded.width() < 32 || decoded.height() < 32 {
        return;
    }
    let height = (width * decoded.height() as f32 / decoded.width() as f32).min(width * 1.5);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let dithered = app.config.ui.cover_dither;
    let grain = crate::gui::dither::grain_for(rect.width(), ui.ctx().pixels_per_point(), app.config.ui.picture_fineness());
    let method = crate::gui::dither::Look::of(&app.config.ui);
    let negative = palette.screens_negative();
    let generation = PICTURES_GENERATION.load(std::sync::atomic::Ordering::Relaxed);
    let id = egui::Id::unique(("wide-picture", path, grain, dithered, format!("{method:?}"), negative, generation));
    let held = held_texture(id);
    let handle = held.unwrap_or_else(|| {
        let image = if dithered {
            let tall = (grain as f32 * height / width).round() as u32;
            crate::gui::covers::screened_wide(&decoded, grain, tall, method, negative)
        } else {
            let rgba = decoded.to_rgba8();
            egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw())
        };
        let handle = ui.ctx().load_texture("wide-picture", image, egui::TextureOptions::NEAREST);
        keep_texture(id, handle.clone());
        handle
    });
    let tint = if dithered { palette.ink } else { Color32::WHITE };
    if dithered {
        crate::gui::covers::halftone_ground(ui, rect, palette);
    }
    let under = if dithered { palette.tones_under(tint, method.tones) } else { Vec::new() };
    crate::gui::covers::paint_screened(ui, rect, MARKER_RADIUS, handle.id(), &under, tint);
}

const PICTURE_KEPT: u32 = 512;

const PICTURE_AGAIN_SECS: u64 = 2;

const PICTURE_UNUSED_SECS: u64 = 30;

static TEXTURES: std::sync::Mutex<Option<std::collections::HashMap<egui::Id, (egui::TextureHandle, std::time::Instant)>>> = std::sync::Mutex::new(None);

static PRUNED_AT: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

fn held_texture(id: egui::Id) -> Option<egui::TextureHandle> {
    prune_pictures();
    let mut textures = TEXTURES.lock().unwrap_or_else(|e| e.into_inner());
    let (handle, used) = textures.get_or_insert_with(Default::default).get_mut(&id)?;
    *used = std::time::Instant::now();
    Some(handle.clone())
}

fn keep_texture(id: egui::Id, handle: egui::TextureHandle) {
    TEXTURES.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Default::default).insert(id, (handle, std::time::Instant::now()));
}

fn prune_pictures() {
    let now = std::time::Instant::now();
    {
        let mut at = PRUNED_AT.lock().unwrap_or_else(|e| e.into_inner());
        if at.is_some_and(|at| now.duration_since(at).as_secs() < 5) {
            return;
        }
        *at = Some(now);
    }
    let unused = |used: std::time::Instant| now.duration_since(used).as_secs() >= PICTURE_UNUSED_SECS;
    if let Some(textures) = TEXTURES.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        textures.retain(|_, (_, used)| !unused(*used));
    }
    if let Some(decoded) = DECODED.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        decoded.retain(|_, d| !matches!(d, Decoding::Done(_, used) if unused(*used)));
    }
}

enum Decoding {
    Under,
    Done(std::sync::Arc<image::DynamicImage>, std::time::Instant),
    Missing(std::time::Instant),
}

static DECODED: std::sync::Mutex<Option<std::collections::HashMap<std::path::PathBuf, Decoding>>> = std::sync::Mutex::new(None);

fn decoded_picture(ctx: &egui::Context, path: &std::path::Path) -> Option<std::sync::Arc<image::DynamicImage>> {
    let mut decoded = DECODED.lock().unwrap_or_else(|e| e.into_inner());
    let known = decoded.get_or_insert_with(Default::default);
    match known.get_mut(path) {
        Some(Decoding::Done(image, used)) => {
            *used = std::time::Instant::now();
            return Some(image.clone());
        }
        Some(Decoding::Under) => return None,
        Some(Decoding::Missing(at)) if at.elapsed().as_secs() < PICTURE_AGAIN_SECS => {
            ctx.request_repaint_after(std::time::Duration::from_secs(PICTURE_AGAIN_SECS));
            return None;
        }
        _ => {}
    }
    known.insert(path.to_path_buf(), Decoding::Under);
    drop(decoded);
    decode_later(path.to_path_buf(), ctx.clone());
    None
}

const DECODERS: usize = 2;

static TO_DECODE: std::sync::Mutex<Option<std::sync::mpsc::Sender<(std::path::PathBuf, egui::Context)>>> = std::sync::Mutex::new(None);

fn decode_later(path: std::path::PathBuf, ctx: egui::Context) {
    let mut sender = TO_DECODE.lock().unwrap_or_else(|e| e.into_inner());
    let send = sender.get_or_insert_with(|| {
        let (send, take) = std::sync::mpsc::channel::<(std::path::PathBuf, egui::Context)>();
        let take = std::sync::Arc::new(std::sync::Mutex::new(take));
        for _ in 0..DECODERS {
            let take = std::sync::Arc::clone(&take);
            std::thread::spawn(move || loop {
                let next = take.lock().unwrap_or_else(|e| e.into_inner()).recv();
                let Ok((path, ctx)) = next else { return };
                decode_now(path, &ctx);
            });
        }
        send
    });
    let _ = send.send((path, ctx));
}

fn decode_now(path: std::path::PathBuf, ctx: &egui::Context) {
    let mut bytes = std::fs::read(&path).ok();
    if let Some(read) = &bytes {
        let podcasts = path.parent().and_then(|p| p.file_name()).is_some_and(|n| n == "podcast-art");
        if podcasts {
            if let Some(small) = crate::podcast::small_picture(read) {
                let part = path.with_extension("part");
                if std::fs::write(&part, &small).is_ok() && std::fs::rename(&part, &path).is_ok() {
                    bytes = Some(small);
                }
            }
        }
    }
    let image = bytes.and_then(|bytes| image::load_from_memory(&bytes).ok()).map(|image| {
        if image.width().max(image.height()) > PICTURE_KEPT {
            image.thumbnail(PICTURE_KEPT, PICTURE_KEPT)
        } else {
            image
        }
    });
    let said = match image {
        Some(image) => Decoding::Done(std::sync::Arc::new(image), std::time::Instant::now()),
        None => Decoding::Missing(std::time::Instant::now()),
    };
    DECODED.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Default::default).insert(path, said);
    ctx.request_repaint();
}

pub(in crate::gui) fn forget_decoded_pictures() {
    *DECODED.lock().unwrap_or_else(|e| e.into_inner()) = None;
    *TEXTURES.lock().unwrap_or_else(|e| e.into_inner()) = None;
    PICTURES_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

static PICTURES_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub(super) fn row_ahead(ui: &Ui, width: f32, m: Metrics) -> Rect {
    Rect::from_min_size(ui.cursor().min + Vec2::new(0.0, 0.5), Vec2::new(width, (m.row - 1.0).max(1.0)))
}

pub(super) fn point_at_word(ui: &Ui, response: &Response, word: &str, offered: bool, m: Metrics, palette: &Palette) {
    if !offered || word.is_empty() {
        return;
    }
    let word_width = text::line(ui, word, Role::Label, palette.mid).size().x;
    if response.hover_pos().is_some_and(|at| at.x >= content_rect(response.rect, m).right() - word_width - 6.0) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
}

pub(super) fn on_the_word(ui: &Ui, response: &Response, word: &str, m: Metrics, palette: &Palette) -> bool {
    if word.is_empty() {
        return false;
    }
    let word_width = text::line(ui, word, Role::Label, palette.mid).size().x;
    response
        .interact_pointer_pos()
        .is_some_and(|at| at.x >= content_rect(response.rect, m).right() - word_width - 6.0)
}

#[cfg(test)]
mod tests {
    use super::brief;

    #[test]
    fn a_brief_keeps_whole_sentences() {
        let text = "NTS is a radio station. It broadcasts from London. It began in 2011 in Dalston.";
        assert_eq!(brief(text, 200), text);
        assert_eq!(brief(text, 60), "NTS is a radio station. It broadcasts from London.");
        assert_eq!(brief(text, 10), "NTS is a…");
    }
}
