//! The first two columns — the names and the records — and the lists panel
//! that shares the first.

use super::*;

/// Where in the artist column the playing track's artist stands, if it is
/// there at all.
///
/// The track list says what is playing in the accent; the two columns to its
/// left now say *whose* and *which record* in the same ink, so the three read
/// as one line across the window — and a playing name can no longer be taken
/// for a hovered one, which is a grey plate and nothing more. Found by the
/// same keys the click on „was läuft" goes looking with, so the name that
/// lights up is the one that click would land on.
pub(super) fn playing_artist(app: &GuiApp) -> Option<usize> {
    let track = app.player.library_track()?;
    let key = crate::db::sort_key(crate::db::grouping_artist(track));
    app.artists
        .iter()
        .position(|a| crate::db::sort_key(&a.name) == key)
}

/// The same for the album column: this record by this artist, or — where the
/// column holds only one record of that name — that one.
pub(super) fn playing_album(app: &GuiApp) -> Option<usize> {
    let track = app.player.library_track()?;
    crate::gui::album_row_for(
        &app.albums,
        &crate::db::sort_key(&track.album),
        &crate::db::sort_key(crate::db::grouping_artist(track)),
    )
}

/// The letter a name stands under in the register: its first letter as the
/// shelf orders it — what comes before the first letter or digit left aside,
/// „ä" under „a" —, „#" for one beginning with a digit, and one head for every
/// name in another script, which has no letters of this alphabet to stand
/// under.
fn register_letter(name: &str) -> String {
    letter_of(&crate::db::shelf_key(name).1)
}

/// The letter a podcast or a station stands under: the first of its name as
/// written, which is what those columns sort by.
pub(super) fn plain_letter(name: &str) -> String {
    letter_of(&name.to_lowercase())
}

/// The register's letter for a key the list is sorted by.
fn letter_of(key: &str) -> String {
    match key.chars().next() {
        Some(c) if c.is_ascii_alphabetic() => c.to_ascii_lowercase().to_string(),
        Some(c) if c.is_ascii_digit() => "#".to_string(),
        Some(_) => tr!("andere").to_string(),
        None => String::new(),
    }
}

/// A register's head, where `letter` begins a new run of names: the letter in
/// the heads' tone, with a block's air before every head but the first, as
/// the podcasts' days stand over their episodes. Its place when drawn, so a
/// jump to the letter can scroll there.
pub(super) fn register_head(ui: &mut Ui, width: f32, letter: &str, last: &mut Option<String>, m: Metrics, palette: &Palette) -> Option<Rect> {
    if letter.is_empty() || last.as_deref() == Some(letter) {
        return None;
    }
    if last.is_some() {
        ui.add_space(BLOCK_AIR);
    }
    let top = ui.cursor().top();
    let role = if letter.chars().count() == 1 { Role::FiguresLabel } else { Role::Label };
    say(ui, width, letter, role, palette.heading(), m);
    *last = Some(letter.to_string());
    Some(Rect::from_x_y_ranges(ui.cursor().left()..=ui.cursor().left() + width, top..=ui.cursor().top()))
}

/// The tallest step between two letters of the bar: under a row, so a short
/// alphabet stays a compact column instead of spreading down the window.
const LETTER_BAR_STEP_MOST: f32 = 0.8;

/// A letter clicked in the bar, kept as the arrow's letter while the list
/// stays where the jump left it: near the end, the list cannot scroll its
/// names to the top, and the arrow would otherwise stand by the letter that
/// does. `seen` is the letter at the top once the jump has settled; the list
/// moved on from it, the choice is let go.
#[derive(Clone, Default)]
struct Chosen {
    letter: String,
    since: f64,
    seen: Option<String>,
}

fn letter_chosen_id() -> egui::Id {
    egui::Id::unique("artist-letter-chosen")
}

/// How long after a jump the list is taken to have arrived.
const LETTER_JUMP_SETTLES: f64 = 0.5;

/// The arrow's letter: the one clicked while the list has not moved since,
/// else the one whose names stand at the top.
fn chosen_or_top(ui: &Ui, top: Option<String>) -> Option<String> {
    let id = letter_chosen_id();
    let Some(mut chosen) = ui.data(|d| d.get_temp::<Chosen>(id)) else { return top };
    let now = ui.input(|i| i.time);
    if now - chosen.since < LETTER_JUMP_SETTLES {
        ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(LETTER_JUMP_SETTLES));
        return Some(chosen.letter);
    }
    match &chosen.seen {
        None => {
            chosen.seen = top;
            let letter = chosen.letter.clone();
            ui.data_mut(|d| d.insert_temp(id, chosen));
            Some(letter)
        }
        Some(seen) if top.as_ref() == Some(seen) => Some(chosen.letter),
        Some(_) => {
            ui.data_mut(|d| d.remove_temp::<Chosen>(id));
            top
        }
    }
}

/// Where the bar asks the names to go: read by the column on the next frame.
fn letter_jump_id() -> egui::Id {
    egui::Id::unique("artist-letter-jump")
}

/// The letter bar: the register's letters in the gap to the right of the
/// names, standing in the middle of the list's height, shown while the pointer
/// is over the column or the gap. It opens from its middle outwards with the
/// fade the lists have at their edges, and closes the same way. An arrow
/// pushes in before the letter whose names stand at the top — or before the one
/// under the pointer — as the rows' arrows do. A click on a letter, or the
/// pointer drawn down the bar with the button held, brings its names to the
/// top. Only letters the column has, in its order.
fn letter_bar(ui: &Ui, letters: &[String], now: Option<&str>, column: Rect, list_top: f32, m: Metrics, palette: &Palette) {
    if letters.is_empty() {
        return;
    }
    let gap = Rect::from_min_max(egui::pos2(column.right(), list_top), egui::pos2(column.right() + EXTENT_REACH, column.bottom()));
    let zone = column.union(gap);
    let pointer = ui.input(|i| i.pointer.hover_pos());
    let id = egui::Id::unique("artist-letter-bar");
    let held = ui.ctx().is_being_dragged(id);
    let near = held || pointer.is_some_and(|p| zone.contains(p));
    let shown = super::slide_for(ui.ctx(), egui::Id::unique("artist-letter-bar-shown"), near, false, LETTER_BAR_SLIDE);
    if shown <= 0.001 {
        return;
    }
    let sample = text::line(ui, "m", Role::Label, palette.heading());
    let least = sample.size().y;
    let height = (gap.bottom() - list_top).max(least);
    let room = ((height / least) as usize).max(1);
    let picked: Vec<&String> = if letters.len() <= room {
        letters.iter().collect()
    } else {
        (0..room).map(|i| &letters[i * letters.len() / room]).collect()
    };
    let step = (height / picked.len() as f32).min(m.row * LETTER_BAR_STEP_MOST);
    let tall = step * picked.len() as f32;
    let probe = text::line(ui, "m", Role::FiguresLabel, palette.heading());
    let arrow_probe = text::line(ui, "←", Role::Label, palette.ink);
    let edge = column.right() + (m.column_gap / 2.0 - LETTER_BAR_CLEAR).max(EXTENT_REACH);
    let box_width = LETTER_BAR_PAD_X * 2.0 + probe.size().x + LETTER_BAR_ARROW_GAP + arrow_probe.size().x;
    let flush = column.right() - m.marker_pad_x;
    let box_left_rest = flush + LETTER_BAR_PAD_X - box_width;
    let bar_top = list_top + (height - tall) / 2.0;
    let bar = Rect::from_min_max(egui::pos2(box_left_rest - LETTER_BAR_HIT_MORE, bar_top), egui::pos2(edge, bar_top + tall));
    let response = ui.interact(bar, id, Sense::click_and_drag());
    let at = |y: f32| (((y - bar.top()) / step).floor().max(0.0) as usize).min(picked.len() - 1);
    let under = pointer.filter(|p| bar.contains(*p) || held).map(|p| at(p.y));
    if response.clicked() || response.dragged() || response.drag_started() {
        if let Some(i) = ui.input(|i| i.pointer.interact_pos()).map(|p| at(p.y)) {
            let since = ui.input(|i| i.time);
            ui.data_mut(|d| {
                d.insert_temp(letter_jump_id(), picked[i].clone());
                d.insert_temp(letter_chosen_id(), Chosen { letter: picked[i].clone(), since, seen: None });
            });
            ui.ctx().request_repaint();
        }
    }
    if under.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let current = now.and_then(|now| {
        let whole = letters.iter().position(|l| l == now)?;
        picked.iter().rposition(|l| letters.iter().position(|w| w == *l).is_some_and(|p| p <= whole))
    });
    let pressing = held || response.is_pointer_button_down_on();
    let wanted = if pressing { under.or(current) } else { current };
    let settle_id = id.with("settled");
    let now_t = ui.input(|i| i.time);
    let (mut candidate, mut since, mut settled): (Option<usize>, f64, Option<usize>) =
        ui.data(|d| d.get_temp(settle_id)).unwrap_or((wanted, f64::NEG_INFINITY, wanted));
    if wanted != candidate {
        candidate = wanted;
        since = now_t;
    }
    let waited = now_t - since;
    if waited >= f64::from(LETTER_BAR_ARROW_DELAY) {
        settled = candidate;
    } else {
        ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(f64::from(LETTER_BAR_ARROW_DELAY) - waited));
    }
    ui.data_mut(|d| d.insert_temp(settle_id, (candidate, since, settled)));
    let target = settled;

    let dx = (edge - box_left_rest) * (1.0 - shown);
    let box_left = box_left_rest + dx;
    let painter = ui.painter().with_clip_rect(Rect::from_min_max(egui::pos2(box_left_rest - 1.0, gap.top() - 1.0), egui::pos2(edge, gap.bottom() + 1.0)));

    {
        let panel = Rect::from_min_max(
            egui::pos2(box_left - LETTER_BAR_PANEL_PAD, bar.top() - LETTER_BAR_PANEL_PAD),
            egui::pos2(edge + dx + LETTER_BAR_PANEL_RADIUS, bar.bottom() + LETTER_BAR_PANEL_PAD),
        );
        let corners = egui::CornerRadius {
            nw: LETTER_BAR_PANEL_RADIUS as u8,
            sw: LETTER_BAR_PANEL_RADIUS as u8,
            ne: 0,
            se: 0,
        };
        let ground = palette.ground.gamma_multiply(shown);
        let clipped = ui.painter().with_clip_rect(Rect::from_min_max(egui::pos2(column.left(), gap.top()), egui::pos2(edge, gap.bottom())));
        let shadow = egui::epaint::Shadow { offset: [0, 0], blur: LETTER_BAR_PANEL_SOFT as u8, spread: (LETTER_BAR_PANEL_SOFT / 2.0) as u8, color: ground };
        clipped.add(shadow.as_shape(panel, corners));
    }
    let ink_offset = text::ink_middle(ui, Role::FiguresLabel, 0.0);
    let ink = text::ink_height(ui, Role::FiguresLabel);
    let arrow_ink_offset = text::ink_middle(ui, Role::Label, 0.0);
    let room = LETTER_BAR_ARROW_GAP + arrow_probe.size().x;
    for (i, letter) in picked.iter().enumerate() {
        let lit = target == Some(i);
        let colour = if lit { palette.ink } else { palette.heading() };
        let said = if letter.chars().count() > 1 { "…" } else { letter.as_str() };
        let g = text::line(ui, said, Role::FiguresLabel, colour);
        let cell = Rect::from_min_size(egui::pos2(box_left, bar.top() + step * i as f32), Vec2::new(box_width, step));
        let pushed = ui.ctx().animate_bool_with_time(id.with(("arrow", i)), lit, ARROW_SECS);
        let slot = cell.right() - LETTER_BAR_PAD_X - probe.size().x - room * pushed;
        let middle = cell.center().y;
        if under == Some(i) {
            let plate = Rect::from_min_max(
                egui::pos2(slot - LETTER_BAR_PAD_X, middle - ink / 2.0 - LETTER_BAR_PAD_X),
                egui::pos2(cell.right(), middle + ink / 2.0 + LETTER_BAR_PAD_X),
            );
            painter.rect_filled(plate, crate::gui::theme::ROW_RADIUS, palette.hover);
        }
        let left = slot + (probe.size().x - g.size().x) / 2.0;
        painter.galley(egui::pos2(left, middle - ink_offset), g, colour);
        if pushed > 0.001 {
            let arrow = text::line(ui, "←", Role::Label, palette.ink);
            let rest = cell.right() - LETTER_BAR_PAD_X - arrow.size().x;
            let travel = LETTER_BAR_PAD_X + arrow.size().x;
            let x = rest + travel * (1.0 - pushed);
            let within = Rect::from_min_max(egui::pos2(rest - 1.0, cell.top()), egui::pos2(cell.right(), cell.bottom()));
            painter
                .with_clip_rect(within.intersect(painter.clip_rect()))
                .galley(egui::pos2(x, middle - arrow_ink_offset), arrow, palette.ink.gamma_multiply(pushed));
        }
    }
}

/// The air in a letter's box either side of the letter and its arrow.
const LETTER_BAR_PAD_X: f32 = 5.0;

/// How far the panel behind the letters reaches past the boxes, left, above
/// and below.
const LETTER_BAR_PANEL_PAD: f32 = 10.0;

/// The panel's left corners.
const LETTER_BAR_PANEL_RADIUS: f32 = 16.0;

/// How far the panel's soft edge reaches: thinning over this much, half of
/// it inside the panel's line and half outside.
const LETTER_BAR_PANEL_SOFT: f32 = 44.0;

/// How long the arrow's letter has to hold before the arrow goes to it.
const LETTER_BAR_ARROW_DELAY: f32 = 0.18;

/// How much further left than the boxes a press still counts.
const LETTER_BAR_HIT_MORE: f32 = 6.0;

/// How long the bar takes to slide in or out, the whole way.
const LETTER_BAR_SLIDE: f32 = 0.34;

/// How far short of the divider the bar stops.
const LETTER_BAR_CLEAR: f32 = 1.5;

/// Between the arrow and its letter.
const LETTER_BAR_ARROW_GAP: f32 = 2.0;

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

    let alphabetical = matches!(app.artist_sort, crate::gui::ListSort::Ascending | crate::gui::ListSort::Descending);
    let mut list_top = 0.0;
    let bar_on = app.config.ui.artist_letter_bar && alphabetical;
    let mut top_letter: Option<String> = None;

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
            list_top = ui.cursor().top();
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

                let register = app.config.ui.artist_letters && alphabetical;
                let mut jump: Option<String> = ui.data_mut(|d| d.remove_temp(letter_jump_id()));
                let mut last_letter: Option<String> = None;
                for (i, artist) in app.artists.iter().enumerate() {
                    let index = ArtistRow::Named(i).index();
                    let letter = if register || jump.is_some() || bar_on { register_letter(&artist.name) } else { String::new() };
                    let mut landing = None;
                    if register {
                        landing = register_head(ui, m.artists_width, &letter, &mut last_letter, m, palette);
                    }
                    if jump.as_deref() == Some(letter.as_str()) {
                        let rect = landing.unwrap_or_else(|| Rect::from_min_size(ui.cursor().min, Vec2::new(m.artists_width, m.row)));
                        ui.scroll_to_rect(rect.with_min_y(rect.top() - BLOCK_AIR), Some(egui::Align::TOP));
                        jump = None;
                    }
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
                    if bar_on && top_letter.is_none() && response.rect.bottom() > ui.clip_rect().top() {
                        top_letter = Some(letter.clone());
                    }
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

    ui.data_mut(|d| d.insert_temp(extent_yields_id("artists"), bar_on));
    if bar_on {
        let mut letters: Vec<String> = Vec::new();
        for artist in &app.artists {
            let letter = register_letter(&artist.name);
            if !letter.is_empty() && letters.last() != Some(&letter) {
                letters.push(letter);
            }
        }
        let now = chosen_or_top(ui, top_letter);
        letter_bar(ui, &letters, now.as_deref(), taken.response.rect, list_top, m, palette);
    }

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

/// What happened to the lists' names.
///
/// Rows by their place: a list's position in `lists`, and one past the last
/// for.
#[derive(Default)]
pub(super) struct ListRows {
    /// „zuletzt hinzugefügt", the list nobody made: clicked, or played.
    pub(super) newest_clicked: bool,
    pub(super) newest_opened: bool,
    pub(super) clicked: Option<usize>,
    pub(super) opened: Option<usize>,
    pub(super) menu: Option<(usize, egui::Pos2)>,
    pub(super) make: bool,
    /// Tracks let go of over a list, by its place.
    pub(super) dropped_on: Option<usize>,
    /// Tracks let go of over: a new list, made of them.
    pub(super) dropped_new: bool,
    /// A press somewhere other than the row a name is being typed in.
    pub(super) let_go: bool,
}

impl ListRows {
    /// Everything that happened to them, answered.
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

/// The lists' names, and at their foot the offer to make one.
///
/// A name being typed for a list is typed here, in the row it will stand in
/// turns into the line being written, a list being renamed into its new name.
/// The track list stays where it is.
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

/// A row lit up under tracks being carried to it.
pub(super) fn drop_light(ui: &Ui, rect: Rect, name: &str, m: Metrics, palette: &Palette) {
    ui.painter().rect_filled(
        rect.shrink2(Vec2::new(0.0, m.marker_inset_y)),
        crate::gui::theme::ROW_RADIUS,
        palette.marked(),
    );
    let g = text::line(ui, name, Role::Content, palette.ink);
    let at = egui::pos2(
        rect.left() + m.marker_pad_x,
        text::on_line(&g, rect.top(), text::row_line(ui, m.row, Role::Content)),
    );
    ui.painter().galley(at, g, palette.ink);
}

/// The line a list's name is being typed in: the marker, the name so far,
/// and the caret after it — the block the search field and the tag editor
/// use, in the accent.
pub(super) fn typing_row(app: &GuiApp, ui: &mut Ui, width: f32, m: Metrics, palette: &Palette) -> Rect {
    match app.prompt.as_ref() {
        Some(prompt) => typed_line(ui, width, &prompt.entry, app.scroll_to.is_some(), m, palette),
        None => {
            let (rect, response) = ui.allocate_exact_size(Vec2::new(width, m.row), Sense::click());
            let state = RowState { selected: true, focused: true, ..Default::default() };
            paint_marker(ui, rect, response.id, state, m, palette);
            rect
        }
    }
}

/// A row being written in place — a list's name, a station's: the marker,
/// the text so far, and the caret after it, as [`typing_row`] draws it.
pub(super) fn typed_line(
    ui: &mut Ui,
    width: f32,
    entry: &crate::gui::tagedit::Entry,
    scrolling: bool,
    m: Metrics,
    palette: &Palette,
) -> Rect {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, m.row), Sense::click());
    let state = RowState {
        selected: true,
        focused: true,
        ..Default::default()
    };
    paint_marker(ui, rect, response.id, state, m, palette);
    let content = content_rect(rect, m);
    let line = text::row_line(ui, m.row, Role::Content);
    let now = entry.text();
    let room = (content.width() - CARET_WIDTH - 2.0).max(1.0);
    let g = text::line_within(ui, now, Role::Content, palette.ink, room);
    let shown = g.size().x;
    let at = egui::pos2(content.left(), text::on_line(&g, rect.top(), line));
    if entry.is_marked() {
        paint_field_mark(ui, at, &g, rect, palette);
    }
    ui.painter().galley(at, g, palette.ink);
    if !entry.is_marked() && ui.input(|i| i.time).rem_euclid(1.06) < 0.62 {
        let before = &now[..entry.caret()];
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
    keep_visible(ui, rect, m, scrolling);
    let _ = response;
    rect
}

/// The lists, in the fourth column: their names at the top, and under them
/// the one that is open, to be read, filled and rearranged.
///
/// They take turns with the queue (`u`, `w`) rather than standing in a
/// column of their own, which moved every other column along each time they
/// came and went.
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

/// How much of the fourth column the lists' names may take before they
/// scroll, and the air between them and the open list.
pub(super) const LISTS_SHARE: f32 = 0.4;

/// The air between „zuletzt hinzugefügt" and the lists that are yours.
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

#[cfg(test)]
mod register_tests {
    /// The register's letters: first letter as the shelf has it, digits under
    /// „#", other scripts together.
    #[test]
    fn names_stand_under_their_first_letter() {
        assert_eq!(super::register_letter("Boards of Canada"), "b");
        assert_eq!(super::register_letter("(((Microwaves)))"), "m");
        assert_eq!(super::register_letter("Ääni"), "a");
        assert_eq!(super::register_letter("6955"), "#");
        assert_eq!(super::register_letter("工藤冬里"), super::register_letter("たゆたう"));
        assert_eq!(super::register_letter(""), "");
    }
}
