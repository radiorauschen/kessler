use super::*;

#[derive(Clone, Copy, Default)]
pub(super) struct Marquee {
    pub(super) offset: f32,
    pub(super) dwelt: f32,
}

pub(super) fn marquee(ui: &Ui, id: egui::Id, overflow: f32, active: bool) -> f32 {
    const DWELL: f32 = 0.65;
    const AWAY: f32 = 34.0;
    const HOME: f32 = 260.0;

    if overflow <= 0.5 {
        return 0.0;
    }

    let dt = ui.input(|i| i.stable_dt).clamp(0.0, 0.1);
    let mut state: Marquee = ui.data(|d| d.get_temp(id)).unwrap_or_default();

    let next = if active {
        state.dwelt = (state.dwelt + dt).min(DWELL);
        if state.dwelt >= DWELL {
            state.offset = (state.offset + AWAY * dt).min(overflow);
        }
        if state.offset >= overflow {
            None
        } else if state.dwelt < DWELL {
            Some(std::time::Duration::from_secs_f32(DWELL - state.dwelt))
        } else {
            Some(std::time::Duration::from_secs_f32(1.0 / AWAY))
        }
    } else {
        state.dwelt = 0.0;
        state.offset = (state.offset - HOME * dt).max(0.0);
        (state.offset > 0.0).then_some(std::time::Duration::ZERO)
    };

    ui.data_mut(|d| d.insert_temp(id, state));
    match next {
        Some(std::time::Duration::ZERO) => ui.ctx().request_repaint(),
        Some(after) => ui.ctx().request_repaint_after(after),
        None => {}
    }
    state.offset
}

pub(super) fn edge_band(painter: &egui::Painter, rect: Rect, ground: Color32, solid_at_left: bool) {
    const SOLID: f32 = 0.35;
    let clear = Color32::TRANSPARENT;
    let solid = rect.width() * SOLID;
    let (xs, colours) = if solid_at_left {
        (
            [rect.left(), rect.left() + solid, rect.right()],
            [ground, ground, clear],
        )
    } else {
        (
            [rect.left(), rect.right() - solid, rect.right()],
            [clear, ground, ground],
        )
    };
    let mut mesh = egui::Mesh::default();
    for (x, colour) in xs.into_iter().zip(colours) {
        mesh.colored_vertex(egui::pos2(x, rect.top()), colour);
        mesh.colored_vertex(egui::pos2(x, rect.bottom()), colour);
    }
    for k in 0..2u32 {
        let at = k * 2;
        mesh.add_triangle(at, at + 1, at + 2);
        mesh.add_triangle(at + 1, at + 2, at + 3);
    }
    painter.add(egui::Shape::mesh(mesh));
}

#[allow(clippy::too_many_arguments)]
const WEIGHT_SECS: f32 = 0.12;

pub(super) fn cell_text(
    ui: &Ui,
    cell: Rect,
    id: egui::Id,
    label: &str,
    role: Role,
    colour: Color32,
    active: bool,
    ground: Color32,
    m: Metrics,
) {
    let galley = if matches!(role, Role::Content | Role::ContentBold) && super::theme::weights().is_some() {
        let bold = ui.ctx().animate_bool_with_time(id.with("weight"), role == Role::ContentBold, WEIGHT_SECS);
        text::full_line_weighted(ui, label, bold, colour)
    } else {
        text::full_line(ui, label, role, colour)
    };
    let size = galley.size();
    let on = if role == Role::Note { Role::Note } else { Role::Content };
    let y = text::on_line(&galley, cell.top(), text::row_line(ui, cell.height(), on));
    let overflow = size.x - cell.width();

    if overflow <= 0.5 {
        ui.painter()
            .galley(egui::pos2(cell.left(), y), galley, colour);
        return;
    }

    let offset = marquee(ui, id, overflow, active);
    let inset_left = if offset > 0.5 { 1.0 } else { 0.0 };
    let words = Rect::from_min_max(
        egui::pos2(cell.left() + inset_left, cell.top()),
        egui::pos2(cell.right() - 1.0, cell.bottom()),
    );
    ui.painter()
        .with_clip_rect(words.intersect(ui.clip_rect()))
        .galley(egui::pos2(cell.left() - offset, y), galley, colour);
    let painter = ui.painter().with_clip_rect(cell.expand2(Vec2::new(1.0, 0.0)).intersect(ui.clip_rect()));

    let fade = EDGE_FADE.min(cell.width() / 3.0);
    let band = Rect::from_min_max(
        egui::pos2(cell.left() - 1.0, cell.top() + m.marker_inset_y),
        egui::pos2(cell.right() + 1.0, cell.bottom() - m.marker_inset_y),
    );
    if offset > 0.5 {
        edge_band(
            &painter,
            Rect::from_min_size(band.left_top(), Vec2::new(fade, band.height())),
            ground,
            true,
        );
    }
    if offset < overflow - 0.5 {
        edge_band(
            &painter,
            Rect::from_min_size(
                egui::pos2(band.right() - fade, band.top()),
                Vec2::new(fade, band.height()),
            ),
            ground,
            false,
        );
    }
}

const QUICK_FADE: f32 = 6.0;

pub(in crate::gui) fn paint_fade(ui: &Ui, viewport: Rect, hidden_above: f32, hidden_below: f32, palette: &Palette) {
    let ground = palette.ground;
    let height = FADE_HEIGHT.min(viewport.height() / 3.0);
    if height <= 0.0 {
        return;
    }
    let strength = |hidden: f32| hidden / QUICK_FADE;

    let band = |rect: Rect, solid_at_top: bool, strength: f32| {
        if strength <= 0.01 {
            return;
        }
        let solid = ground.gamma_multiply(strength.clamp(0.0, 1.0));
        let clear = Color32::TRANSPARENT;
        let (top, bottom) = if solid_at_top { (solid, clear) } else { (clear, solid) };
        let mut mesh = egui::Mesh::default();
        mesh.colored_vertex(rect.left_top(), top);
        mesh.colored_vertex(rect.right_top(), top);
        mesh.colored_vertex(rect.left_bottom(), bottom);
        mesh.colored_vertex(rect.right_bottom(), bottom);
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(1, 2, 3);
        ui.painter().add(egui::Shape::mesh(mesh));
    };

    band(Rect::from_min_size(viewport.min, Vec2::new(viewport.width(), height)), true, strength(hidden_above));
    band(
        Rect::from_min_size(egui::pos2(viewport.left(), viewport.bottom() - height), Vec2::new(viewport.width(), height)),
        false,
        strength(hidden_below),
    );
}

pub(super) fn paint_side_fade(ui: &Ui, slot: Rect, gone: f32, palette: &Palette) {
    let width = FADE_HEIGHT.min(slot.width());
    let strength = (gone * 4.0).clamp(0.0, 1.0);
    if width <= 0.0 || strength <= 0.01 {
        return;
    }
    let band = Rect::from_min_max(egui::pos2(slot.right() - width, slot.top()), slot.max);
    let solid = palette.ground.gamma_multiply(strength);
    let clear = Color32::TRANSPARENT;
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(band.left_top(), clear);
    mesh.colored_vertex(band.right_top(), solid);
    mesh.colored_vertex(band.left_bottom(), clear);
    mesh.colored_vertex(band.right_bottom(), solid);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 2, 3);
    ui.painter().add(egui::Shape::mesh(mesh));
}

pub(in crate::gui) fn note_cursor_row(ui: &Ui, key: &'static str, rect: Rect, here: bool) {
    if here {
        ui.ctx().data_mut(|d| d.insert_temp(cursor_row_id(key), rect));
    }
}

pub(in crate::gui) fn cursor_row(ctx: &egui::Context, key: &'static str) -> Option<Rect> {
    ctx.data(|d| d.get_temp::<Rect>(cursor_row_id(key)))
}

fn cursor_row_id(key: &'static str) -> egui::Id {
    egui::Id::unique(("kessler-cursor-row", key))
}

pub(super) fn keep_visible(ui: &Ui, rect: Rect, m: Metrics, wanted: bool) {
    if !wanted {
        return;
    }
    ui.scroll_to_rect(
        rect.expand2(Vec2::new(0.0, m.row * SCROLL_MARGIN_ROWS)),
        None,
    );
}

pub(in crate::gui) fn without_a_bar(ui: &Ui, id: &str) -> egui::ScrollArea {
    let area = egui::ScrollArea::vertical()
        .id_salt(id)
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden);
    if let Some(at) = ui.data_mut(|d| d.remove_temp::<f32>(pull_id(id))) {
        return area.vertical_scroll_offset(at);
    }
    damp_past_the_end(ui, id);
    area
}

const PAST_THE_END: f32 = 0.15;

fn damp_past_the_end(ui: &Ui, id: &str) {
    if !ui.rect_contains_pointer(ui.available_rect_before_wrap()) {
        return;
    }
    let touch = matches!(
        ui.input(|i| i.scroll_source()),
        Some(egui::MouseWheelSource::Trackpad | egui::MouseWheelSource::Momentum)
    );
    let Some(state) = egui::scroll_area::State::load(ui.ctx(), area_id(ui, id)) else { return };
    let push = ui.input(|i| i.smooth_scroll_delta.y);
    if !touch || push == 0.0 {
        return;
    }
    let over = state.overscroll().y;
    let room = if push > 0.0 {
        if over < 0.0 { 0.0 } else { state.clamped_offset().y }
    } else if over > 0.0 {
        0.0
    } else {
        match ui.data(|d| d.get_temp::<f32>(below_id(id))) {
            Some(below) => below,
            None => return,
        }
    };
    let damped = damp(push, room);
    if damped != push {
        ui.input_mut(|i| i.smooth_scroll_delta.y = damped);
    }
}

fn damp(push: f32, room: f32) -> f32 {
    let room = room.max(0.0);
    if push.abs() <= room {
        push
    } else if room > 0.5 {
        room.copysign(push)
    } else {
        push * PAST_THE_END
    }
}

fn below_id(id: &str) -> egui::Id {
    egui::Id::unique(("below", id))
}

fn area_id(ui: &Ui, id: &str) -> egui::Id {
    ui.make_persistent_id(egui::IdSalt::new(id))
}

fn pull_id(id: &str) -> egui::Id {
    egui::Id::unique(("extent-pull", id))
}

pub(super) fn scrolled(ui: &mut Ui, id: &str, palette: &Palette, body: impl FnOnce(&mut Ui)) -> f32 {
    scrolled_with(ui, id, palette, LIST_END_AIR, body)
}

pub(super) const LIST_END_AIR: f32 = FADE_HEIGHT;

pub(super) fn ground_clicked(ui: &mut Ui, id: &str) -> bool {
    let ground = ui.max_rect().intersect(ui.clip_rect());
    ui.interact(ground, egui::Id::unique(("ground", id)), Sense::click()).clicked()
}

pub(super) fn column_deeper_id() -> egui::Id {
    egui::Id::unique("column-deeper")
}

pub(super) fn scrolled_with(ui: &mut Ui, id: &str, palette: &Palette, end_air: f32, body: impl FnOnce(&mut Ui)) -> f32 {
    let deeper = ui.data_mut(|d| d.remove_temp::<f32>(column_deeper_id())).unwrap_or(0.0);
    let output = without_a_bar(ui, id).auto_shrink([false, false]).show(ui, |ui| {
        body(ui);
        ui.add_space(end_air + deeper);
    });

    let viewport = output.inner_rect;
    let above = output.state.clamped_offset().y;
    let below = (output.content_size.y - above - viewport.height()).max(0.0);
    ui.data_mut(|d| d.insert_temp(below_id(id), below));
    paint_fade(ui, viewport, above, below, palette);
    paint_extent(ui, id, viewport, above, output.content_size.y, palette);
    below
}

pub(in crate::gui) fn paint_extent(
    ui: &Ui,
    id: &str,
    viewport: Rect,
    offset: f32,
    content: f32,
    palette: &Palette,
) {
    const WIDTH: f32 = 3.0;
    const WIDE: f32 = 5.0;
    const OUTSIDE: f32 = 5.5;
    const REACH: f32 = OUTSIDE + WIDE + 1.0;
    const FADE_SECS: f32 = 0.12;

    let visible = viewport.height();
    let Some((top, length)) = extent_mark(visible, content, offset) else {
        return;
    };
    let travel = visible - length;
    let furthest = content - visible;

    let strip = Rect::from_min_size(
        egui::pos2(viewport.right(), viewport.top()),
        Vec2::new(REACH, visible),
    );
    let response = ui.interact(strip, egui::Id::unique(("extent-hit", id)), Sense::click_and_drag());

    let hold = egui::Id::unique(("extent-hold", id));
    if response.drag_started() {
        if let Some(p) = response.interact_pointer_pos() {
            let mark_top = viewport.top() + top;
            let at = if (mark_top..=mark_top + length).contains(&p.y) {
                p.y - mark_top
            } else {
                length / 2.0
            };
            ui.data_mut(|d| d.insert_temp(hold, at));
        }
    }
    let held = if response.dragged() || response.drag_started() {
        Some(ui.data(|d| d.get_temp::<f32>(hold)).unwrap_or(length / 2.0))
    } else if response.clicked() {
        Some(length / 2.0)
    } else {
        None
    };
    let mut top = top;
    if let (Some(held), Some(p)) = (held, response.interact_pointer_pos()) {
        let at = (p.y - held - viewport.top()).clamp(0.0, travel.max(0.0));
        let wanted = if travel > 0.0 { at / travel * furthest } else { 0.0 };
        ui.data_mut(|d| d.insert_temp(pull_id(id), wanted));
        ui.ctx().request_repaint();
        top = at;
    }

    let hot = response.hovered() || response.dragged();
    let over = hot
        || ui
            .ctx()
            .pointer_latest_pos()
            .is_some_and(|p| viewport.contains(p));
    let shown =
        ui.ctx()
            .animate_bool_with_time(egui::Id::unique(("extent", id)), over, FADE_SECS);
    if shown <= 0.01 {
        return;
    }
    let lift = ui
        .ctx()
        .animate_bool_with_time(egui::Id::unique(("extent-hot", id)), hot, FADE_SECS);
    let width = WIDTH + (WIDE - WIDTH) * lift;

    let mark = Rect::from_min_size(
        egui::pos2(viewport.right() + OUTSIDE, viewport.top() + top),
        Vec2::new(width, length),
    );
    ui.painter().rect_filled(
        mark,
        if crate::gui::theme::square_ends() { 0.0 } else { width / 2.0 },
        palette.mid.gamma_multiply(shown * (0.55 + 0.3 * lift)),
    );
}

pub(super) const EXTENT_MIN_LENGTH: f32 = 18.0;

pub(super) fn extent_mark(visible: f32, content: f32, offset: f32) -> Option<(f32, f32)> {
    if content <= visible + 1.0 || visible <= EXTENT_MIN_LENGTH {
        return None;
    }
    let length = (visible / content * visible).clamp(EXTENT_MIN_LENGTH, visible);
    let travel = visible - length;
    let progress = (offset / (content - visible)).clamp(0.0, 1.0);
    Some((travel * progress, length))
}

pub(super) fn allocate_row(ui: &mut Ui, width: f32, m: Metrics) -> (Rect, Response) {
    ui.allocate_exact_size(Vec2::new(width, m.row), Sense::click_and_drag())
}

pub(super) const HEADING_GAP: f32 = 14.0;

pub(super) fn badge(ui: &Ui, at: egui::Pos2, said: &str, above: bool, palette: &Palette) {
    let galley = text::line(ui, said, Role::Label, palette.ground);
    let plate = text::ink_size(&galley) + BADGE_PAD * 2.0;
    let window = ui.ctx().content_rect();
    let mut corner = if above {
        egui::pos2(at.x - plate.x / 2.0, at.y - plate.y - BADGE_AWAY)
    } else {
        egui::pos2(at.x + 14.0, at.y + BADGE_AWAY)
    };
    corner.x = corner.x.min(window.right() - BADGE_EDGE - plate.x).max(window.left() + BADGE_EDGE);
    let plate = Rect::from_min_size(corner, plate);
    let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("badge")));
    painter.rect_filled(plate, MARKER_RADIUS, palette.ink);
    painter.galley(text::ink_centred(&galley, plate), galley, palette.ground);
}

pub(super) fn badge_centred(ui: &Ui, at: egui::Pos2, said: &str, above: bool, palette: &Palette) {
    if above {
        badge(ui, at, said, true, palette);
        return;
    }
    let galley = text::line(ui, said, Role::Label, palette.ground);
    let plate = text::ink_size(&galley) + BADGE_PAD * 2.0;
    let window = ui.ctx().content_rect();
    let mut corner = egui::pos2(at.x - plate.x / 2.0, at.y + BADGE_AWAY);
    corner.x = corner.x.min(window.right() - BADGE_EDGE - plate.x).max(window.left() + BADGE_EDGE);
    let plate = Rect::from_min_size(corner, plate);
    let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("badge")));
    painter.rect_filled(plate, MARKER_RADIUS, palette.ink);
    painter.galley(text::ink_centred(&galley, plate), galley, palette.ground);
}

const BADGE_PAD: Vec2 = Vec2::new(7.0, 5.0);

const BADGE_AWAY: f32 = 10.0;

const BADGE_EDGE: f32 = 16.0;

pub(super) fn badge_fits_above(ui: &Ui, y: f32, said: &str) -> bool {
    let galley = text::line(ui, said, Role::Label, Color32::WHITE);
    let tall = text::ink_size(&galley).y + BADGE_PAD.y * 2.0;
    y - tall - BADGE_AWAY >= ui.ctx().content_rect().top() + BADGE_EDGE
}

pub(super) fn carried(app: &mut GuiApp, ui: &Ui) {
    let Some(held) = app.dragging.as_ref() else {
        return;
    };
    let palette = app.palette;
    let episodes = matches!(held, crate::gui::Dragged::Episodes(_) | crate::gui::Dragged::QueuedEpisode(_));
    let said = match (episodes, held.count()) {
        (true, 1) => tr!("1 folge").to_string(),
        (true, n) => tr!("{} folgen", n),
        (false, 1) => tr!("1 titel").to_string(),
        (false, n) => tr!("{} titel", text::count(n as u64)),
    };
    if let Some(at) = ui.ctx().pointer_latest_pos() {
        badge(ui, at, &said, false, &palette);
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    }
    if ui.input(|i| i.pointer.any_released()) {
        app.dragging = None;
    }
    ui.ctx().request_repaint();
}

#[allow(clippy::too_many_arguments)]
pub(super) fn queue_row(
    ui: &mut Ui,
    id: egui::Id,
    arrow: egui::Id,
    width: f32,
    title: &str,
    artist: &str,
    state: RowState,
    m: Metrics,
    palette: &Palette,
) -> Response {
    const GAP: f32 = 12.0;
    const ARTIST_LEAST: f32 = 46.0;
    const ARTIST_SHARE: f32 = 0.4;

    let (rect, response) = allocate_row(ui, width, m);
    if !ui.is_rect_visible(rect) {
        return response;
    }
    paint_marker(ui, rect, state, m, palette);
    paint_hover(ui, rect, &response, state, m, palette);

    let hovered = response.hovered();
    let ground = behind(ui, state, hovered, palette);
    let content = content_rect(rect, m);

    let beside = if state.gone { crate::lang::t(GONE) } else { artist };
    let to_come = !(state.gone || state.spent || state.playing || state.selected);
    let title_colour = if to_come { palette.mid } else { state.primary(palette) };
    let beside_colour = if to_come { palette.mid } else { state.secondary(palette) };

    let wanted = text::full_line(ui, title, state.role(), title_colour)
        .size()
        .x;
    let spare = content.width() - wanted - GAP;
    let artist_width = if beside.is_empty() {
        0.0
    } else if state.gone {
        text::full_line(ui, beside, Role::Content, beside_colour)
            .size()
            .x
    } else if spare < ARTIST_LEAST {
        0.0
    } else {
        spare.min(content.width() * ARTIST_SHARE)
    };

    if artist_width > 0.0 {
        let galley = text::line_within(
            ui,
            beside,
            Role::Content,
            beside_colour,
            artist_width,
        );
        let at = egui::pos2(
            content.right() - galley.size().x,
            text::on_line(&galley, rect.top(), text::row_line(ui, m.row, Role::Content)),
        );
        ui.painter().galley(at, galley, beside_colour);
    }

    let title_right =
        content.right() - artist_width - if artist_width > 0.0 { GAP } else { 0.0 };
    let line = text::row_line(ui, m.row, Role::Content);
    let cell = Rect::from_min_max(content.left_top(), egui::pos2(title_right, content.bottom()));
    let lead = lead_arrow(ui, arrow, cell, line, state.playing, title_colour);
    cell_text(
        ui,
        Rect::from_min_max(egui::pos2(cell.left() + lead, cell.top()), cell.max),
        id,
        title,
        state.role(),
        title_colour,
        state.selected || hovered,
        ground,
        m,
    );
    response
}

pub(super) const FIGURE_SHARE: f32 = 0.45;

pub(in crate::gui) const ARROW_SECS: f32 = 0.16;

pub(in crate::gui) fn lead_arrow(ui: &Ui, id: egui::Id, cell: Rect, line: f32, on: bool, colour: Color32) -> f32 {
    push_arrow(ui, id, cell, line, on, colour)
}

fn push_arrow(ui: &Ui, id: egui::Id, cell: Rect, line: f32, on: bool, colour: Color32) -> f32 {
    let ctx = ui.ctx();
    let pass = ctx.cumulative_pass_nr();
    let was = ctx.data(|d| d.get_temp::<u64>(id.with("lead-arrow-was")));
    ctx.data_mut(|d| d.insert_temp(id.with("lead-arrow-was"), pass));
    let fresh = was.is_none_or(|was| pass.saturating_sub(was) > 1);
    let time = if fresh { 0.0 } else { ARROW_SECS };
    let shown = ctx.animate_bool_with_time(id.with("lead-arrow"), on, time);
    if shown <= 0.001 {
        return 0.0;
    }
    let arrow = text::line(ui, "→", Role::Content, colour);
    let room = arrow.size().x + PLAYING_ARROW_GAP;
    let x = cell.left() - room * (1.0 - shown);
    let at = egui::pos2(x, text::on_line(&arrow, cell.top(), line));
    ui.painter()
        .with_clip_rect(cell.intersect(ui.clip_rect()))
        .galley(at, arrow, colour.gamma_multiply(shown));
    room * shown
}

#[allow(clippy::too_many_arguments)]
pub(super) fn name_row(
    ui: &mut Ui,
    id: egui::Id,
    width: f32,
    name: &str,
    figure: &str,
    figure_role: Role,
    state: RowState,
    m: Metrics,
    palette: &Palette,
) -> Response {
    let (rect, response) = allocate_row(ui, width, m);
    if !ui.is_rect_visible(rect) {
        return response;
    }
    paint_marker(ui, rect, state, m, palette);
    paint_hover(ui, rect, &response, state, m, palette);

    let hovered = response.hovered() || state.pointed;
    let ground = behind(ui, state, hovered, palette);
    let content = content_rect(rect, m);

    let mut name_right = content.right();
    if !figure.is_empty() {
        let galley = text::line_within(
            ui,
            figure,
            figure_role,
            state.secondary(palette),
            content.width() * FIGURE_SHARE,
        );
        let width = galley.size().x;
        let at = egui::pos2(
            content.right() - width,
            text::beside_content(ui, &galley, figure_role, rect.top(), m.row),
        );
        ui.painter().galley(at, galley, palette.ink);
        name_right = content.right() - width - 12.0;
    }

    let line = text::row_line(ui, m.row, Role::Content);
    let cell = Rect::from_min_max(content.left_top(), egui::pos2(name_right, content.bottom()));
    let lead = lead_arrow(ui, id, cell, line, state.playing || state.arrow, state.primary(palette));
    cell_text(
        ui,
        Rect::from_min_max(egui::pos2(cell.left() + lead, cell.top()), cell.max),
        id,
        name,
        state.role(),
        state.primary(palette),
        state.selected || hovered,
        ground,
        m,
    );

    response
}

pub(super) struct Cell<'a> {
    pub text: &'a str,
    pub width: f32,
    pub role: Role,
    pub right: bool,
}

pub(super) const FOUND_GAP: f32 = 12.0;

#[allow(clippy::too_many_arguments)]
pub(super) fn columned_row(
    ui: &mut Ui,
    id: egui::Id,
    width: f32,
    name: &str,
    cells: &[Cell],
    word: &str,
    bold: bool,
    state: RowState,
    m: Metrics,
    palette: &Palette,
) -> Response {
    if !word.is_empty() {
        return name_row(ui, id, width, name, word, Role::Label, state, m, palette);
    }
    let (rect, response) = allocate_row(ui, width, m);
    if !ui.is_rect_visible(rect) {
        return response;
    }
    paint_marker(ui, rect, state, m, palette);
    paint_hover(ui, rect, &response, state, m, palette);
    let hovered = response.hovered();
    let ground = behind(ui, state, hovered, palette);
    let content = content_rect(rect, m);
    let mut right = content.right();
    for cell in cells.iter().rev() {
        let left = right - cell.width;
        let galley = text::line_within(ui, cell.text, cell.role, state.secondary(palette), cell.width);
        let x = if cell.right { right - galley.size().x } else { left };
        let at = egui::pos2(x, text::beside_content(ui, &galley, cell.role, rect.top(), m.row));
        ui.painter().galley(at, galley, palette.ink);
        right = left - FOUND_GAP;
    }
    let line = text::row_line(ui, m.row, Role::Content);
    let cell = Rect::from_min_max(content.left_top(), egui::pos2(right, content.bottom()));
    let lead = lead_arrow(ui, id, cell, line, state.playing, state.primary(palette));
    cell_text(
        ui,
        Rect::from_min_max(egui::pos2(cell.left() + lead, cell.top()), cell.max),
        id,
        name,
        if bold && !state.plated() { Role::ContentBold } else { state.role() },
        state.primary(palette),
        state.selected || hovered,
        ground,
        m,
    );
    response
}

#[allow(clippy::too_many_arguments)]
pub(super) fn heading(
    ui: &mut Ui,
    label: &str,
    trailing: &str,
    mark: Option<&str>,
    instead: Option<&str>,
    m: Metrics,
    palette: &Palette,
) -> Head {
    heading_with(ui, label, trailing, mark, instead, None, m, palette)
}

pub(super) fn still_heading(ui: &mut Ui, label: &str, trailing: &str, m: Metrics, palette: &Palette) {
    head_drawn(ui, label, trailing, None, None, None, true, false, m, palette);
}

pub(super) fn still_heading_with(ui: &mut Ui, label: &str, trailing: &str, at_end: Option<&str>, m: Metrics, palette: &Palette) -> Head {
    head_drawn(ui, label, trailing, None, None, at_end, true, false, m, palette)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn heading_with(
    ui: &mut Ui,
    label: &str,
    trailing: &str,
    mark: Option<&str>,
    instead: Option<&str>,
    at_end: Option<&str>,
    m: Metrics,
    palette: &Palette,
) -> Head {
    head_drawn(ui, label, trailing, mark, instead, at_end, false, false, m, palette)
}

pub(super) fn heading_counted(ui: &mut Ui, label: &str, figure: &str, mark: Option<&str>, m: Metrics, palette: &Palette) -> Head {
    head_drawn(ui, label, figure, mark, None, None, false, true, m, palette)
}

#[allow(clippy::too_many_arguments)]
fn head_drawn(
    ui: &mut Ui,
    label: &str,
    trailing: &str,
    mark: Option<&str>,
    instead: Option<&str>,
    at_end: Option<&str>,
    still: bool,
    quiet_end: bool,
    m: Metrics,
    palette: &Palette,
) -> Head {
    let sense = if still { Sense::hover() } else { Sense::click() };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), m.row), sense);
    let content = content_rect(rect, m);
    let line = text::row_line(ui, m.row, Role::Label);

    let pointer = ui.ctx().pointer_hover_pos().filter(|_| ui.rect_contains_pointer(rect));
    let end = at_end.filter(|_| pointer.is_some()).map(|word| {
        let width = text::line(ui, word, Role::Label, palette.faint).size().x;
        let cell = Rect::from_min_max(
            egui::pos2(content.right() - width - HEADING_GAP / 2.0, rect.top()),
            rect.right_bottom(),
        );
        (word, cell)
    });
    let on_end = end.is_some_and(|(_, cell)| pointer.is_some_and(|p| cell.contains(p)));
    let on_head = response.hovered() && !on_end && !still;

    let offering = instead.filter(|_| pointer.is_some());
    let shown = match (offering, mark) {
        (Some(word), _) => word.to_string(),
        (None, Some(mark)) => format!("{label} {mark}"),
        (None, None) => label.to_string(),
    };
    let shown_width = text::line(ui, &shown, Role::Label, palette.faint).size().x;
    let probe = text::line(ui, &shown, Role::Label, palette.faint);
    let at = egui::pos2(content.left(), text::on_line(&probe, rect.top(), line));
    let plate = control_plate(ui, at, shown_width);
    let word = offering.map(|_| ui.interact(plate, egui::Id::unique(("head-offer", label)), Sense::click()));
    let lit = match &word {
        Some(word) => word.hovered(),
        None => on_head,
    };
    let colour = match (offering, lit) {
        (_, true) => palette.ink,
        (Some(_), false) => palette.mid,
        (None, false) => palette.heading(),
    };
    let galley = text::line(ui, &shown, Role::Label, colour);
    if lit {
        ui.painter().rect_filled(plate, MARKER_RADIUS, palette.hover);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    ui.painter().galley(at, galley, colour);

    let far_word = |ui: &mut Ui, word: &str, hit: Option<&Response>| {
        let lit = hit.is_some_and(Response::hovered);
        let colour = if lit { palette.ink } else { palette.mid };
        let galley = text::line(ui, word, Role::Label, colour);
        let at = egui::pos2(content.right() - galley.size().x, text::on_line(&galley, rect.top(), line));
        if lit {
            ui.painter().rect_filled(control_plate(ui, at, galley.size().x), MARKER_RADIUS, palette.hover);
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        ui.painter().galley(at, galley, colour);
    };

    let mut beside = None;
    if let Some((word, cell)) = end {
        let hit = ui.interact(cell, egui::Id::unique(("head-end", label)), Sense::click());
        far_word(ui, word, Some(&hit));
        beside = Some(hit);
    } else if !trailing.is_empty() {
        let room = content.width() - shown_width - HEADING_GAP;
        let width = text::line(ui, trailing, Role::Label, palette.faint).size().x;
        if width <= room {
            let cell = Rect::from_min_max(
                egui::pos2(content.right() - width - 6.0, rect.top()),
                rect.right_bottom(),
            );
            let hit = (at_end.is_none() && !still && !quiet_end).then(|| {
                ui.interact(cell, egui::Id::unique(("head-trailing", label)), Sense::click())
            });
            far_word(ui, trailing, hit.as_ref());
            beside = hit;
        }
    }

    Head {
        row: response,
        beside,
        word,
    }
}

pub(super) struct Head {
    pub(super) row: Response,
    pub(super) beside: Option<Response>,
    word: Option<Response>,
}

impl Head {
    pub(super) fn clicked(&self) -> bool {
        match &self.word {
            Some(word) => word.clicked(),
            None => self.row.clicked() && !self.beside_clicked(),
        }
    }

    pub(super) fn beside_clicked(&self) -> bool {
        self.beside.as_ref().is_some_and(Response::clicked)
    }

}

#[allow(clippy::too_many_arguments)]
pub(super) const MARK_GAP: f32 = 10.0;

pub(super) const PLAYING_ARROW_GAP: f32 = 5.0;

pub const DEVICE_MARKS: [&str; 3] = ["wort", "punkt", "aus"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceMark {
    Word,
    Dot,
}

impl DeviceMark {
    pub fn of(said: &str) -> Option<Self> {
        match said {
            "punkt" => Some(Self::Dot),
            "aus" => None,
            _ => Some(Self::Word),
        }
    }
}

const DEVICE_DOT: f32 = 5.0;

pub(super) fn track_row(
    ui: &mut Ui,
    width: f32,
    track: &crate::db::models::Track,
    state: RowState,
    on_device: Option<DeviceMark>,
    by: Option<&str>,
    columns: &crate::gui::columns::TrackColumns,
    m: Metrics,
    palette: &Palette,
) -> Response {
    let (rect, response) = allocate_row(ui, width, m);
    if !ui.is_rect_visible(rect) {
        return response;
    }
    paint_marker(ui, rect, state, m, palette);
    paint_hover(ui, rect, &response, state, m, palette);

    let hovered = response.hovered();
    let ground = behind(ui, state, hovered, palette);
    let secondary = state.secondary(palette);
    let content = content_rect(rect, m);
    let gone_word = state
        .gone
        .then(|| text::full_line(ui, crate::lang::t(GONE), Role::Content, palette.mid));
    let format = crate::gui::columns::format_mark(&track.path).filter(|_| !state.gone);
    let word = match (format, on_device) {
        (Some(format), Some(DeviceMark::Word)) => Some(format!("{format}  ipod")),
        (None, Some(DeviceMark::Word)) => Some("ipod".to_string()),
        (format, _) => format.map(str::to_string),
    };
    let word = match (by.map(|name| short_name(name, BY_CHARS)), word) {
        (Some(name), Some(word)) => Some(format!("{name}  {word}")),
        (Some(name), None) => Some(name),
        (None, word) => word,
    };
    let mark = word.map(|word| text::full_line(ui, &word, Role::Label, secondary));
    let dot = on_device == Some(DeviceMark::Dot);
    let mark_room = mark
        .as_ref()
        .map(|g| g.size().x + MARK_GAP)
        .unwrap_or_default()
        + if dot { DEVICE_DOT + MARK_GAP } else { 0.0 };

    let title_width = (content.width()
        - mark_room
        - match &gone_word {
            Some(word) => {
                word.size().x
                    + CELL_GAP
                    + columns
                        .leading()
                        .map(|c| c.width() + crate::gui::columns::number_gap(CELL_GAP))
                        .unwrap_or_default()
            }
            None => columns.reserved(CELL_GAP),
        })
    .max(60.0);

    let cell = |ui: &Ui, column: TrackColumn, x: f32| {
        let value = column.value(track);
        let colour = if state.playing {
            secondary
        } else if column == TrackColumn::Length {
            palette.mid
        } else {
            secondary
        };
        let role = column.role();
        let galley = text::line_within(ui, &value, role, colour, column.width());
        let at = if column.right_aligned() {
            x + column.width() - galley.size().x
        } else {
            x
        };
        let at = egui::pos2(at, text::beside_content(ui, &galley, role, rect.top(), m.row));
        ui.painter().galley(at, galley, palette.ink);
    };

    let mut x = content.left();
    if let Some(leading) = columns.leading() {
        cell(ui, leading, x);
        x += leading.width() + crate::gui::columns::number_gap(CELL_GAP);
    }

    let spare = if gone_word.is_none() {
        columns
            .trailing()
            .next()
            .filter(|c| c.right_aligned())
            .map(|c| {
                let shown = text::line(ui, &c.value(track), c.role(), secondary).size().x;
                (c.width() - shown).max(0.0) + CELL_GAP - MARK_GAP
            })
            .unwrap_or_default()
    } else {
        0.0
    };
    let title_room = title_width + spare;
    let title_cell = Rect::from_min_size(egui::pos2(x, rect.top()), Vec2::new(title_room, m.row));
    let lead = lead_arrow(
        ui,
        egui::Id::unique(("track-arrow", track.id)),
        title_cell,
        text::row_line(ui, m.row, Role::Content),
        state.playing && !state.gone,
        state.primary(palette),
    );
    cell_text(
        ui,
        Rect::from_min_max(egui::pos2(title_cell.left() + lead, title_cell.top()), title_cell.max),
        egui::Id::unique(("track-title", track.id)),
        track.display_title(),
        state.role(),
        state.primary(palette),
        state.selected || hovered,
        ground,
        m,
    );
    if mark.is_some() || dot {
        let ink = text::full_line(ui, track.display_title(), state.role(), palette.ink)
            .size()
            .x;
        let mut after = x + lead + ink.min(title_room - lead) + MARK_GAP;
        if let Some(word) = mark {
            let at = egui::pos2(
                after,
                text::on_line(&word, rect.top(), text::row_line(ui, m.row, Role::Content)),
            );
            after += word.size().x + MARK_GAP;
            ui.painter().galley(at, word, secondary);
        }
        if dot {
            ui.painter().circle_filled(
                egui::pos2(after + DEVICE_DOT / 2.0, rect.center().y),
                DEVICE_DOT / 2.0,
                secondary,
            );
        }
    }
    x += title_width + mark_room + CELL_GAP;

    if let Some(word) = gone_word {
        let at = egui::pos2(
            content.right() - word.size().x,
            text::on_line(&word, rect.top(), text::row_line(ui, m.row, Role::Content)),
        );
        ui.painter().galley(at, word, palette.mid);
    } else {
        for column in columns.trailing() {
            cell(ui, column, x);
            x += column.width() + CELL_GAP;
        }
    }

    response
}

pub(super) fn paint_field_mark(ui: &Ui, at: egui::Pos2, value: &egui::Galley, rect: Rect, palette: &Palette) {
    let mark = Rect::from_min_max(
        egui::pos2(at.x - 3.0, rect.top() + 4.0),
        egui::pos2(at.x + value.size().x + 3.0, rect.bottom() - 4.0),
    );
    ui.painter()
        .rect_filled(mark, MARKER_RADIUS, palette.ink.gamma_multiply(0.18));
}

const BY_CHARS: usize = 28;

fn short_name(name: &str, most: usize) -> String {
    if name.chars().count() <= most {
        name.to_string()
    } else {
        let mut cut: String = name.chars().take(most.saturating_sub(1)).collect();
        cut.push('…');
        cut
    }
}

#[cfg(test)]
mod stretch_tests {
    use eframe::egui;
    #[test]
    fn the_stretch_reads_the_lists_own_state() {
        let ctx = egui::Context::default();
        let mut found = false;
        for _ in 0..2 {
            let mut out = ctx.run_ui(Default::default(), |ui| {
                super::without_a_bar(ui, "probe").show(ui, |ui| ui.label("x"));
                found = egui::scroll_area::State::load(ui.ctx(), super::area_id(ui, "probe")).is_some();
            });
            out.textures_delta.clear();
        }
        assert!(found);
    }

    #[test]
    fn a_push_past_the_end_is_made_small() {
        assert_eq!(super::damp(40.0, 100.0), 40.0);
        assert_eq!(super::damp(-40.0, 10.0), -10.0);
        assert!((super::damp(40.0, 0.0) - 40.0 * super::PAST_THE_END).abs() < 1e-4);
    }
}
