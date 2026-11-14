use super::*;

pub(super) struct Masthead {
    pub(super) field: Rect,
}

pub(super) fn masthead_narrow(
    app: &mut GuiApp,
    ui: &mut Ui,
    palette: &Palette,
    m: Metrics,
    width: f32,
    with_menu: bool,
) -> Masthead {
    let (row, _) = ui.allocate_exact_size(Vec2::new(width, field_height()), Sense::hover());
    let rect = content_rect(row, m);
    let probe = text::line(ui, "x", Role::Label, palette.mid);
    let on_the_field = text::on_line(&probe, row.top(), text::line_in_box(ui, field_height(), Role::Label));
    let right = header_controls(app, ui, palette, rect.right(), on_the_field, rect.right(), with_menu);
    let right = if right >= rect.right() { right + SEARCH_GAP } else { right };
    let field = search_field(app, ui, palette, rect, right, f32::MAX);
    Masthead { field }
}

pub(super) fn narrow_playing(app: &mut GuiApp, ui: &mut Ui, palette: &Palette, m: Metrics, width: f32) {
    let (block, _) = ui.allocate_exact_size(Vec2::new(width, NARROW_LINE * 3.0), Sense::hover());
    let names = content_rect(block, m);
    let line = text::row_line(ui, NARROW_LINE, Role::Label);
    playing_line(
        app,
        ui,
        palette,
        names,
        line,
        Playing {
            title: Role::LabelBold,
            artist: Role::Label,
            rest: Role::Label,
            idle_word: true,
            stacked: true,
        },
    );
    ui.add_space(NARROW_GAP);
    narrow_rule(app, ui, palette, m, width);
}

pub(super) fn narrow_rule(app: &mut GuiApp, ui: &mut Ui, palette: &Palette, m: Metrics, width: f32) {
    let (line, _) = ui.allocate_exact_size(Vec2::new(width, field_height()), Sense::hover());
    let inner = content_rect(line, m);
    let probe = text::line(ui, "x", Role::Label, palette.mid);
    let top = text::on_line(&probe, line.top(), text::line_in_box(ui, field_height(), Role::Label));
    let keys_left = header_corner(app, ui, palette, inner.right() + KEY_AIR, top);
    let rule_right = keys_left - NARROW_GAP;
    if rule_right - inner.left() > 80.0 {
        let middle = text::ink_middle(ui, Role::Label, top);
        let strip = Rect::from_min_max(
            egui::pos2(inner.left() - m.marker_pad_x, middle - 7.0),
            egui::pos2(rule_right + m.marker_pad_x, middle + 9.0),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(strip), |ui| {
            progress_rule(app, ui, palette, m, strip.width());
        });
    }
}

const NARROW_GAP: f32 = 12.0;

const NARROW_LINE: f32 = 16.0;

const SEARCH_GAP: f32 = 12.0;

const SEARCH_CARET: Vec2 = Vec2::new(4.0, 12.0);

pub(super) fn header_controls(
    app: &mut GuiApp,
    ui: &mut Ui,
    palette: &Palette,
    edge: f32,
    top: f32,
    keep_left: f32,
    with_menu: bool,
) -> f32 {
    let mut right = edge;
    if with_menu {
        let open = app.menu_open;
        if menu::trigger(ui, egui::pos2(edge, top), palette, open) {
            app.menu_open = !open;
        }
        right -= menu::trigger_width(ui) + CONTROL_GAP;
    }

    if with_menu {
        run_source_switch(app, ui, &mut right, top, palette);
    }

    let volume_fits = VOLUME_WIDTH + VOLUME_GAP + volume_width(ui, palette) + CONTROL_GAP
        <= right - keep_left;

    if volume_fits {
        let figure = volume_rule(app, ui, &mut right, top, palette);
        if transport_volume(ui, &mut right, top, palette, figure) {
            app.toggle_eq();
        }
    }

    if app.is_scanning() {
        let (done, total) = app.scan_counts();
        let said = if total > 0 {
            tr!("lese ein  {} / {}  {}%",
                text::count(done as u64),
                text::count(total as u64),
                (app.scan_fraction().unwrap_or(0.0) * 100.0).round() as u32
            )
        } else {
            tr!("zähle dateien…").to_string()
        };
        let g = text::line(ui, &said, Role::Label, palette.accent);
        right -= g.size().x;
        ui.painter()
            .galley(egui::pos2(right, top), g, palette.accent);
        right -= 20.0;
    }
    right
}

pub(super) const HEADER_GAP: f32 = CONTROL_GAP;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayingAt {
    Compact,
    SuperCompact,
}

impl PlayingAt {
    pub const ALL: [PlayingAt; 2] = [Self::Compact, Self::SuperCompact];

    pub fn compact(self) -> bool {
        matches!(self, Self::Compact | Self::SuperCompact)
    }

    pub fn of(ui: &crate::config::UiConfig) -> Self {
        match ui.playing_at.trim() {
            "super kompakt" => Self::SuperCompact,
            _ => Self::Compact,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Compact => "kompakt",
            Self::SuperCompact => "super kompakt",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Compact => "kompakt",
            Self::SuperCompact => "super kompakt",
        }
    }

    pub fn next(self) -> Self {
        let at = Self::ALL.iter().position(|p| *p == self).unwrap_or(0);
        Self::ALL[(at + 1) % Self::ALL.len()]
    }

    pub fn store(self, ui: &mut crate::config::UiConfig) {
        ui.playing_at = self.key().to_string();
        ui.header_playing = false;
    }
}

pub(super) fn search_field(
    app: &mut GuiApp,
    ui: &mut Ui,
    palette: &Palette,
    row: Rect,
    limit: f32,
    wanted: f32,
) -> Rect {
    let least = wanted.min(MIN_FIELD);
    let wide = wanted.clamp(least, (limit - row.left() - SEARCH_GAP).max(least));
    let field = Rect::from_min_size(row.left_top(), Vec2::new(wide, field_height()));
    let response = ui.interact(field, egui::Id::unique("search-field"), Sense::click());
    let fill = if app.filter_typing {
        palette.field.lerp_to_gamma(palette.ink, 0.06)
    } else {
        palette.field
    };
    ui.painter().rect_filled(field, MARKER_RADIUS, fill);

    let showing = ui.ctx().animate_bool_with_time(
        egui::Id::unique("field-picture"),
        picture_has_the_field(
            app.config.ui.field_spectrum && !app.spectrum.silent(),
            response.hovered(),
            app.filter_typing,
            app.filter.as_ref().is_some_and(|f| !f.trim().is_empty()),
        ),
        FIELD_FADE,
    );
    let in_front = ui.ctx().animate_bool_with_time(
        egui::Id::unique("field-picture-in-front"),
        ui.input(|i| i.focused),
        AWAY_FADE,
    );
    let showing = showing * in_front;
    spectrum_strip(app, ui, field, palette, showing);
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
    }

    let left = field.left() + FIELD_PAD_X;
    let written_on = text::line_in_box(ui, field_height(), Role::Label);

    let Some(query) = app.filter.clone() else {
        let hint = if response.hovered() {
            palette.ink
        } else {
            palette.mid
        };
        let hint = hint.gamma_multiply(1.0 - showing);
        let word = if app.showing_radio() {
            tr!("stationen suchen")
        } else if app.showing_podcasts() {
            tr!("podcasts suchen")
        } else if app.showing_ipod() {
            tr!("suchen")
        } else {
            tr!("musik suchen")
        };
        let g = text::line(ui, word, Role::Label, hint);
        let at = egui::pos2(left, text::on_line(&g, field.top(), written_on));
        ui.painter().galley(at, g, hint);
        if response.clicked() {
            app.open_filter();
        }
        return field;
    };

    if response.clicked() {
        app.open_filter();
        app.filter_marked = false;
    }

    let words = if app.filter_typing {
        palette.ink
    } else {
        palette.mid
    };
    let typed = text::line_within(
        ui,
        &query,
        Role::Label,
        words,
        wide - FIELD_PAD_X * 2.0 - 10.0,
    );
    let typed_width = typed.size().x;
    let at = egui::pos2(left, text::on_line(&typed, field.top(), written_on));
    if app.filter_marked {
        let mark = Rect::from_min_max(
            egui::pos2(at.x - 3.0, field.top() + 5.0),
            egui::pos2(at.x + typed_width + 3.0, field.bottom() - 5.0),
        );
        ui.painter().rect_filled(mark, MARKER_RADIUS, palette.marker);
    }
    ui.painter().galley(at, typed, words);

    if app.filter_typing && !app.filter_marked && ui.input(|i| i.time).rem_euclid(1.06) < 0.62 {
        let before: String = match app.filter_caret {
            Some(at) => query.chars().take(at).collect(),
            None => query.clone(),
        };
        let before_width = if app.filter_caret.is_some() {
            text::line(ui, &before, Role::Label, words).size().x.min(typed_width)
        } else {
            typed_width
        };
        let caret = Rect::from_min_size(
            egui::pos2(left + before_width + 2.0, field.center().y - SEARCH_CARET.y / 2.0),
            SEARCH_CARET,
        );
        ui.painter().rect_filled(caret, 0.0, palette.ink);
        if let Some(under) = app.filter_caret.and_then(|at| query.chars().nth(at)) {
            let g = text::line(ui, &under.to_string(), Role::Label, fill);
            let at = egui::pos2(left + before_width, text::on_line(&g, field.top(), written_on));
            ui.painter().with_clip_rect(caret).galley(at, g, fill);
        }
    }
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(120));

    field
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Playing {
    pub(super) title: Role,
    pub(super) artist: Role,
    pub(super) rest: Role,
    pub(super) idle_word: bool,
    pub(super) stacked: bool,
}

pub(super) const COMPACT_FIELD_SHUT: f32 = MIN_FIELD;

pub(super) const SUPER_COMPACT_FIELD_SHUT: f32 = MIN_FIELD - 8.0;

pub(super) const COMPACT_FIELD_REACH: f32 = 9.0;

pub(super) const COMPACT_FIELD_FURTHER: f32 = 120.0;

pub(super) const COMPACT_FIELD_HOVER: f32 = 0.8;
pub(super) const COMPACT_FIELD_HOVER_SHARE: f32 = 0.75;
pub(super) const COMPACT_FIELD_SECS: f32 = 0.6;
pub(super) const COMPACT_FIELD_DELAY: f64 = 0.1;

pub(super) const COMPACT_LINE_BELOW: f32 = 34.0;

const AFTER_THE_BUTTONS: f32 = 28.0;

fn clear_of_the_buttons(ui: &Ui, rect: Rect) -> Rect {
    let end = crate::gui::theme::buttons_end();
    if end <= 0.0 {
        return rect;
    }
    let clear = ui.ctx().content_rect().left() + end + AFTER_THE_BUTTONS;
    Rect::from_min_max(egui::pos2(rect.left().max(clear), rect.top()), rect.max)
}

fn run_source_switch(app: &mut GuiApp, ui: &mut Ui, right: &mut f32, top: f32, palette: &Palette) {
    match source_switch(app, ui, right, top, palette) {
        Switched::To(Source::Podcasts) => {
            app.show_podcasts();
            ui.ctx().request_discard("the masthead's switch chose the podcasts");
        }
        Switched::To(to) => {
            app.read_from(to == Source::Radio, to == Source::Device);
            ui.ctx().request_discard("the masthead's switch chose another shelf");
        }
        Switched::Dropped => {
            if let Some(crate::gui::Dragged::Incoming(tracks)) = app.dragging.take() {
                let what = match tracks.len() {
                    1 => tracks.first().map(|t| t.display_title().to_string()).unwrap_or_default(),
                    n => tr!("{} titel", text::count(n as u64)),
                };
                if let Err(e) = app.plan_transfer(tracks, what, Vec::new()) {
                    app.set_status(format!("{e}"));
                }
            }
        }
        Switched::Still => {}
    }

}

pub(super) fn menu_in_band(app: &mut GuiApp, ui: &mut Ui, palette: &Palette, window_top: f32, right: f32) {
    let row_top = window_top + crate::gui::theme::line_on_band_middle(FIELD_HEIGHT_FLAT);
    let probe = text::line(ui, "x", Role::Label, palette.mid);
    let top = text::on_line(&probe, row_top, text::line_in_box(ui, FIELD_HEIGHT_FLAT, Role::Label));
    let open = app.menu_open;
    if menu::trigger(ui, egui::pos2(right, top), palette, open) {
        app.menu_open = !open;
    }
    let mut right = right - menu::trigger_width(ui) - CONTROL_GAP;
    run_source_switch(app, ui, &mut right, top, palette);
}

pub(super) const COMPACT_LEAST: f32 = 300.0;

pub(super) const COMPACT_NAMES_SHARE: f32 = 0.5;

pub(super) const SUPER_COMPACT_RULE_AIR: (f32, f32) = (7.0, 2.0);

pub(super) fn header_corner(app: &mut GuiApp, ui: &mut Ui, palette: &Palette, right: f32, top: f32) -> f32 {
    let right = right + KEY_AIR;
    let middle = text::ink_middle(ui, Role::Label, top);
    let corner = egui::pos2(right, middle + KEY_SIZE / 2.0);
    let on_air = app.on_air.is_some();
    let modes = !app.config.ui.modes_in_queue && !on_air && !app.playing_episode();
    transport_keys(app, ui, corner, palette, modes);
    if on_air {
        let cells = header_cells(right, 0.0, play_key_width(ui, palette), false);
        return cells[2].right() - cells[1].width() + on_air_nudge(ui, palette);
    }
    right - keys_width(ui, palette, modes)
}

#[allow(clippy::too_many_arguments)]
const STATUS_FADE: f32 = 0.12;

pub(super) fn header_status(
    app: &mut GuiApp,
    ui: &mut Ui,
    palette: &Palette,
    left: f32,
    right: f32,
    top: f32,
    short: bool,
    library: bool,
) -> f32 {
    const GAP: f32 = 24.0;
    let room = (right - left).max(0.0);

    let said = match app.status() {
        Some(news) => Some((news.to_string(), palette.accent)),
        None => {
            let standing: Vec<String> = if short {
                [
                    app.transfer_standing(),
                    app.artist_hunt_standing(),
                    app.measuring_standing(),
                    app.podcast_standing(),
                    app.scrobble_short(),
                ]
                    .into_iter()
                    .flatten()
                    .collect()
            } else {
                [
                    app.search_standing(),
                    app.transfer_standing(),
                    app.artist_hunt_standing(),
                    app.measuring_standing(),
                    app.podcast_standing(),
                    app.scrobble_standing(),
                ]
                    .into_iter()
                    .flatten()
                    .collect()
            };
            (!standing.is_empty()).then(|| (standing.join("   "), palette.faint))
        }
    };
    let said_room = room;
    let present = said.is_some();
    let fade = ui.ctx().animate_bool_with_time(egui::Id::unique("status-said-fade"), present, STATUS_FADE);
    let last = egui::Id::unique("status-said-last");
    if let Some(said) = &said {
        ui.data_mut(|d| d.insert_temp(last, said.clone()));
    }
    let said = said.or_else(|| (fade > 0.0).then(|| ui.data(|d| d.get_temp::<(String, Color32)>(last))).flatten());
    let said = said.filter(|_| said_room > 40.0).map(|(text, colour)| {
        (status_words(ui, &text, colour, said_room, short), colour)
    });
    let said_width = said.as_ref().map(|(g, _)| g.width()).unwrap_or(0.0);
    let sleep = app.sleep_standing().map(|words| {
        let at_rest = status_words(ui, &words, palette.faint, f32::INFINITY, short);
        let longer = status_words(ui, tr!("+5 min"), palette.ink, f32::INFINITY, short);
        let off = status_words(ui, tr!("beenden|timer"), palette.ink, f32::INFINITY, short);
        (at_rest, longer, off)
    });
    let sleep_width = sleep.as_ref().map_or(0.0, |(rest, longer, off)| rest.width().max(longer.width() + SLEEP_GAP + off.width()));
    let sleep_width = if sleep_width > 0.0 && sleep_width + if said_width > 0.0 { GAP } else { 0.0 } + said_width <= said_room { sleep_width } else { 0.0 };
    let total = said_width + if sleep_width > 0.0 { sleep_width + if said_width > 0.0 { GAP } else { 0.0 } } else { 0.0 };

    let library = (short && library)
        .then(|| app.library_line())
        .flatten()
        .map(|words| status_words(ui, &words, palette.faint, f32::INFINITY, true))
        .filter(|g| g.width() + GAP + total <= room);
    let left_ends = library.as_ref().map(|g| left + g.width());
    if short {
        ui.data_mut(|d| d.get_temp_mut_or_default::<super::FootInk>(super::foot_ink_id()).left_ends = left_ends);
    }
    if let Some(g) = library {
        g.paint(ui.painter(), egui::pos2(left, top), palette.faint);
    }
    if total <= 0.0 {
        return right;
    }

    let x = right - total;
    let said_x = right - said_width;
    if let Some((g, colour)) = said {
        let width = g.width();
        let plate = super::controls::control_plate(ui, egui::pos2(said_x, top), width);
        let response = ui.interact(plate, egui::Id::unique("status-said"), Sense::click());
        super::controls::paint_control_hover(ui, plate, &response, palette);
        let ink = super::controls::hover_ink(&response, colour, palette).gamma_multiply(fade);
        g.paint(ui.painter(), egui::pos2(said_x, top), ink);
        if response.clicked() {
            app.toggle_log();
        }
    }
    if let (Some((at_rest, longer, off)), true) = (sleep, sleep_width > 0.0) {
        let ends = x + sleep_width;
        let region = super::controls::control_plate(ui, egui::pos2(x, top), sleep_width);
        if ui.rect_contains_pointer(region) {
            let off_x = ends - off.width();
            let longer_x = off_x - SLEEP_GAP - longer.width();
            for (g, at, id, which) in [(longer, longer_x, "sleep-longer", true), (off, off_x, "sleep-off", false)] {
                let plate = super::controls::control_plate(ui, egui::pos2(at, top), g.width());
                let response = ui.interact(plate, egui::Id::unique(id), Sense::click());
                super::controls::paint_control_hover(ui, plate, &response, palette);
                let ink = super::controls::hover_ink(&response, palette.mid, palette);
                g.paint(ui.painter(), egui::pos2(at, top), ink);
                if response.clicked() {
                    if which {
                        app.sleep_longer();
                    } else {
                        app.sleep_off();
                    }
                }
            }
        } else {
            let width = at_rest.width();
            at_rest.paint(ui.painter(), egui::pos2(ends - width, top), palette.faint);
        }
    }
    right - total
}

const SLEEP_GAP: f32 = 12.0;

pub(super) fn clock_words(at: chrono::NaiveDateTime, twelve: bool, with_date: bool, lang: crate::lang::Lang) -> String {
    use chrono::{Datelike as _, Timelike as _};
    use crate::lang::Lang;
    let (day, month, year) = (at.day(), at.month(), at.year());
    let date = match lang {
        Lang::De => format!("{day:02}.{month:02}.{year}"),
        Lang::En => format!("{month}/{day}/{year}"),
        Lang::Ja => format!("{year}年{month}月{day}日"),
    };
    let (hour, minute) = (at.hour(), at.minute());
    let time = if twelve {
        let (pm, hour) = at.hour12();
        match lang {
            Lang::Ja => format!("{}{hour}:{minute:02}", if pm { "午後" } else { "午前" }),
            _ => format!("{hour}:{minute:02} {}", if pm { "pm" } else { "am" }),
        }
    } else {
        format!("{hour}:{minute:02}")
    };
    if with_date { format!("{date}  {time}") } else { time }
}

pub(super) fn masthead_compact(
    app: &mut GuiApp,
    ui: &mut Ui,
    palette: &Palette,
    m: Metrics,
    width: f32,
    in_the_band: bool,
    with_menu: bool,
) -> Masthead {
    let (row, _) = ui.allocate_exact_size(Vec2::new(width, field_height()), Sense::hover());
    let rect = content_rect(row, m);
    let rect = if in_the_band { clear_of_the_buttons(ui, rect) } else { rect };
    let super_compact = PlayingAt::of(&app.config.ui) == PlayingAt::SuperCompact;
    let words_line = text::line_in_box(ui, field_height(), Role::Label);
    let probe = text::line(ui, "x", Role::Label, palette.mid);
    let top = text::on_line(&probe, row.top(), words_line);

    let right = header_controls(app, ui, palette, rect.right(), top, rect.left() + COMPACT_LEAST, with_menu);
    let group = (rect.right() - right - CONTROL_GAP).max(0.0);
    let corner_left = header_corner(app, ui, palette, right, top);
    let limit = corner_left - HEADER_GAP;

    let shut = if super_compact { SUPER_COMPACT_FIELD_SHUT } else { COMPACT_FIELD_SHUT };
    let mut words = vec![tr!("musik suchen")];
    if app.config.ui.radio {
        words.push(tr!("stationen suchen"));
    }
    if app.config.ui.podcasts {
        words.push(tr!("podcasts suchen"));
    }
    let widest = words.iter().map(|w| text::line(ui, w, Role::Label, palette.mid).size().x).fold(0.0, f32::max);
    let shut = shut.max(widest + 2.0 * FIELD_PAD_X);
    let field = {
        let open_id = egui::Id::unique("compact-field-open");
        let was: f32 = ui.data(|d| d.get_temp(open_id)).unwrap_or(shut);
        let over = ui.rect_contains_pointer(Rect::from_min_size(
            egui::pos2(rect.left(), row.top()),
            Vec2::new(was, field_height()),
        ));
        let typing = app.filter_typing || app.filter.is_some();
        let further = if super_compact { 0.0 } else { COMPACT_FIELD_FURTHER };
        let open = group.max(shut) + COMPACT_FIELD_REACH + further;
        let since_id = egui::Id::unique("compact-field-over-since");
        let now = ui.input(|i| i.time);
        let since: Option<f64> = ui.data(|d| d.get_temp(since_id));
        let since = match (over, since) {
            (true, Some(at)) => Some(at),
            (true, None) => Some(now),
            (false, _) => None,
        };
        ui.data_mut(|d| match since {
            Some(at) => {
                d.insert_temp(since_id, at);
            }
            None => d.remove::<f64>(since_id),
        });
        let rested = since.is_some_and(|at| now - at >= COMPACT_FIELD_DELAY);
        if over && !rested {
            let left = COMPACT_FIELD_DELAY - since.map_or(0.0, |at| now - at);
            ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(left.max(0.0)));
        }
        let target = if typing || rested {
            let wide = (shut + (open - shut) * COMPACT_FIELD_HOVER) * COMPACT_FIELD_HOVER_SHARE;
            ((wide - shut) / (open - shut).max(1.0)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let opened = slide_to(
            ui.ctx(),
            egui::Id::unique("compact-field-width"),
            target,
            false,
            COMPACT_FIELD_SECS,
            egui::emath::easing::cubic_in_out,
        );
        let shown = shut + (open - shut) * opened;
        ui.data_mut(|d| d.insert_temp(open_id, shown));
        search_field(app, ui, palette, rect, limit, shown)
    };
    let names_left = field.right() + HEADER_GAP;

    let names = Playing {
        title: Role::LabelBold,
        artist: Role::Label,
        rest: Role::Label,
        idle_word: false,
        stacked: false,
    };
    let natural = app
        .player
        .current_track()
        .map(|track| {
            text::runs_width(&text::rich_line(
                ui,
                &[
                    (track.display_title(), names.title, palette.ink),
                    (track.shown_artist(), names.artist, palette.mid),
                    (&track.album, names.rest, palette.faint),
                ],
            ))
        })
        .or_else(|| {
            on_air_words(app, ui, 3).map(|words| {
                let parts = [(names.title, palette.ink), (names.artist, palette.mid), (names.rest, palette.faint)];
                let runs: Vec<_> = words.iter().map(|(w, at)| (w.as_str(), parts[*at].0, parts[*at].1)).collect();
                text::runs_width(&text::rich_line(ui, &runs))
            })
        })
        .unwrap_or(0.0);
    let station_only = app.player.current_track().is_none() && app.on_air.is_some();
    let names_room = if station_only {
        (limit - names_left - HEADER_GAP - station_rule_width(ui, palette) - m.marker_pad_x * 2.0).max(0.0)
    } else {
        ((limit - names_left) * COMPACT_NAMES_SHARE).max(0.0)
    };
    let names_width = natural.min(names_room).ceil();
    let names_width = if station_only {
        ui.ctx().animate_value_with_time(egui::Id::unique("on-air-names-width"), names_width, ON_AIR_FADE_SECS).ceil()
    } else {
        names_width
    };
    if names_width > 0.0 {
        let cell = Rect::from_min_max(
            egui::pos2(names_left, row.top()),
            egui::pos2(names_left + names_width, row.bottom()),
        );
        playing_line(app, ui, palette, cell, words_line, names);
    }

    let (air_left, air_right) = if super_compact && !station_only { SUPER_COMPACT_RULE_AIR } else { (0.0, 0.0) };
    let rule_left = if names_width > 0.0 {
        names_left + names_width + HEADER_GAP + air_left
    } else {
        names_left
    };
    let long = limit - rule_left - air_right;
    if long > 80.0 {
        let middle = text::ink_middle(ui, Role::Label, top);
        let strip = Rect::from_min_size(
            egui::pos2(rule_left - m.marker_pad_x, middle - 7.0),
            Vec2::new(long + m.marker_pad_x * 2.0, 16.0),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(strip), |ui| {
            progress_rule(app, ui, palette, m, strip.width());
        });
    }

    ui.add_space(COMPACT_LINE_BELOW);
    Masthead { field }
}

const ON_AIR_FADE_SECS: f32 = 0.3;

fn on_air_words(app: &GuiApp, ui: &Ui, dots: usize) -> Option<Vec<(String, usize)>> {
    let station = app.on_air.as_ref()?;
    if app.player.tuning() || !super::controls::station_settled(ui.ctx()) {
        return Some(vec![(station.name.clone(), 0), (format!("{}{}", tr!("verbinde"), ".".repeat(dots)), 2)]);
    }
    let Some(aired) = app.aired.first() else {
        if let Some(show) = app.show_on_now() {
            return Some(vec![(show, 0), (station.name.clone(), 2)]);
        }
        return Some(vec![(station.name.clone(), 0), (tr!("radio").to_string(), 2)]);
    };
    let mut words = vec![(aired.title.clone(), 0)];
    if let Some(artist) = &aired.artist {
        words.push((artist.clone(), 1));
    }
    words.push((station.name.clone(), 2));
    Some(words)
}

#[derive(Clone, Default)]
struct OnAirFade {
    key: Vec<(String, usize)>,
    words: Vec<(String, usize)>,
    was: Option<Vec<(String, usize)>>,
    since: f64,
    seen: f64,
}

pub(super) fn playing_line(
    app: &mut GuiApp,
    ui: &mut Ui,
    palette: &Palette,
    cell: Rect,
    line: f32,
    how: Playing,
) {
    let Some(track) = app.player.current_track() else {
        let dots = 1 + (ui.input(|i| i.time) / 0.4) as usize % 3;
        if let Some(words) = on_air_words(app, ui, dots) {
            let key = on_air_words(app, ui, 0).unwrap_or_default();
            let fade_id = egui::Id::unique("now-on-air-fade");
            let now = ui.input(|i| i.time);
            let mut fade = ui.data(|d| d.get_temp::<OnAirFade>(fade_id)).unwrap_or_default();
            if now - fade.seen > 0.5 {
                fade.key.clear();
                fade.was = None;
            }
            fade.seen = now;
            if fade.key != key {
                fade.was = (!fade.key.is_empty()).then(|| std::mem::take(&mut fade.words));
                fade.key = key;
                fade.since = now;
            }
            fade.words = words.clone();
            let t = ((now - fade.since) as f32 / ON_AIR_FADE_SECS).clamp(0.0, 1.0);
            let shown = 0.5 - 0.5 * (t * std::f32::consts::PI).cos();
            if t >= 1.0 {
                fade.was = None;
            }
            let parts = [(how.title, palette.ink), (how.artist, palette.mid), (how.rest, palette.faint)];
            let over = ui.rect_contains_pointer(cell);
            if let Some(was) = &fade.was {
                let runs: Vec<_> = was.iter().map(|(w, at)| (w.as_str(), parts[*at].0, parts[*at].1.gamma_multiply(1.0 - shown))).collect();
                let runs = text::rich_line(ui, &runs);
                set_runs(ui, palette, cell, line, &runs, egui::Id::unique("now-on-air-was"), false);
                ui.ctx().request_repaint();
            }
            let alpha = if fade.was.is_some() { shown } else { 1.0 };
            let runs: Vec<_> = words.iter().map(|(w, at)| (w.as_str(), parts[*at].0, parts[*at].1.gamma_multiply(alpha))).collect();
            let runs = text::rich_line(ui, &runs);
            set_runs(ui, palette, cell, line, &runs, egui::Id::unique("now-on-air"), over);
            if words.iter().any(|(w, _)| w.starts_with(tr!("verbinde"))) {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(400));
            }
            ui.data_mut(|d| d.insert_temp(fade_id, fade));
            let hit = ui.interact(cell, egui::Id::unique("now-on-air-hit"), Sense::click());
            if hit.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if hit.clicked() {
                app.show_on_air();
            }
            return;
        }
        if how.idle_word {
            let idle = text::line(ui, tr!("nichts ausgewählt"), how.rest, palette.mid);
            let at = egui::pos2(cell.left(), text::on_line(&idle, cell.top(), line));
            ui.painter().galley(at, idle, palette.mid);
        }
        return;
    };

    let hit = ui.interact(cell, egui::Id::unique("now-playing-hit"), Sense::click());
    if hit.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let reveal = hit.clicked();

    if how.stacked {
        let pitch = cell.height() / 3.0;
        let parts = [
            (track.display_title(), how.title, palette.ink),
            (track.shown_artist(), how.artist, palette.mid),
            (track.album.as_str(), how.rest, palette.faint),
        ];
        for (index, part) in parts.into_iter().enumerate() {
            let row = Rect::from_min_size(
                egui::pos2(cell.left(), cell.top() + pitch * index as f32),
                Vec2::new(cell.width(), pitch),
            );
            let runs = text::rich_line(ui, &[part]);
            let hovered = hit.hovered() && ui.rect_contains_pointer(row);
            let id = egui::Id::unique(("now-playing", track.id, index));
            set_runs(ui, palette, row, line, &runs, id, hovered);
        }
        if reveal {
            found_it(app);
        }
        return;
    }

    let runs = text::rich_line(
        ui,
        &[
            (track.display_title(), how.title, palette.ink),
            (track.shown_artist(), how.artist, palette.mid),
            (&track.album, how.rest, palette.faint),
        ],
    );
    let id = egui::Id::unique(("now-playing", track.id));
    set_runs(ui, palette, cell, line, &runs, id, hit.hovered());
    if reveal {
        found_it(app);
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn set_runs(
    ui: &Ui,
    palette: &Palette,
    cell: Rect,
    line: f32,
    runs: &[(std::sync::Arc<egui::Galley>, Color32)],
    id: egui::Id,
    hovered: bool,
) {
    let overflow = text::runs_width(runs) - cell.width();

    let start = cell.left();

    let set = |painter: &egui::Painter, offset: f32| {
        let mut x = start - offset;
        for (galley, colour) in runs {
            let width = galley.size().x;
            let at = egui::pos2(x, text::on_line(galley, cell.top(), line));
            painter.galley(at, galley.clone(), *colour);
            x += width + text::RUN_GAP;
        }
    };

    if overflow <= 0.5 {
        set(ui.painter(), 0.0);
        return;
    }

    let offset = marquee(ui, id, overflow, hovered);

    let painter = ui.painter().with_clip_rect(cell.intersect(ui.clip_rect()));
    set(&painter, offset);

    let fade = EDGE_FADE.min(cell.width() / 3.0);
    if offset > 0.5 {
        edge_band(
            &painter,
            Rect::from_min_size(cell.left_top(), Vec2::new(fade, cell.height())),
            palette.ground,
            true,
        );
    }
    if offset < overflow - 0.5 {
        edge_band(
            &painter,
            Rect::from_min_size(
                egui::pos2(cell.right() - fade, cell.top()),
                Vec2::new(fade, cell.height()),
            ),
            palette.ground,
            false,
        );
    }
}

pub(super) fn found_it(app: &mut GuiApp) {
    if app.playing_episode() {
        app.show_playing_episode();
        return;
    }
    if app.showing_radio() {
        app.toggle_radio_view();
    }
    app.leave_podcasts();
    if let Err(e) = app.reveal_playing() {
        app.set_status(format!("{e}"));
    }
    app.reveal_next = true;
}
