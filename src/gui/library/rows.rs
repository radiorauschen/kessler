//! Rows: the pieces every column is drawn from — a name, a track, a queued
//! track, a heading — and text too long for its cell.

use super::*;

/// Where a line of text has crept to, and how long it has been there.
#[derive(Clone, Copy, Default)]
pub(super) struct Marquee {
    pub(super) offset: f32,
    pub(super) dwelt: f32,
}

/// How far to shift an over-long line this frame.
///
/// It waits a moment before setting off, so that sweeping the pointer down a
/// list does not set every row in motion; it stops at the end rather than
/// looping, because a line that keeps circling is harder to read than one that
/// simply finished; and it returns faster than it left, so letting go feels
/// like release rather than another wait.
pub(super) fn marquee(ui: &Ui, id: egui::Id, overflow: f32, active: bool) -> f32 {
    /// Seconds before an active line starts moving.
    const DWELL: f32 = 0.65;
    /// Reading pace, pixels per second.
    const AWAY: f32 = 34.0;
    /// Coming back, pixels per second.
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

/// A horizontal gradient, for the edge a scrolling line disappears behind.
///
/// Solid for the last stretch before the edge, and only then fading: a gradient
/// that was still half clear a few pixels from the cut left the letters there
/// showing through, and the line seemed to stop a little before the fade began.
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

/// How long a name takes to turn bold, or back, in seconds.
const WEIGHT_SECS: f32 = 0.12;

/// Draw one line of text inside a cell, scrolling it when it does not fit
/// and `active` (under the pointer).
///
/// Text that fits is simply drawn. Text that does not is clipped to the cell
/// and fades into the row's own background at whichever end still has more to
/// come, so a title never ends on a cut-off letter.
#[allow(clippy::too_many_arguments)]
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
    chosen_cell_text(ui, cell, id, label, role, colour, active, false, ground, m);
}

/// Which of a column's chosen lines may run this frame, see [`may_run`].
#[derive(Clone, Copy, Default)]
struct MarqueeTurn {
    frame: u64,
    taken: Option<egui::Id>,
    /// A line under the pointer was too long last frame, or this one.
    pointed_before: bool,
    pointed_now: bool,
}

/// Whether a chosen line that is too long may run.
///
/// One line at a time in a column: with a whole album picked every title set
/// off at once, and a column of moving lines is no longer read but watched.
/// The one under the pointer goes first; without it, the first chosen line
/// drawn — the topmost — runs and the others wait at their start.
fn may_run(ui: &Ui, id: egui::Id, pointed: bool) -> bool {
    let clip = ui.clip_rect();
    let key = egui::Id::unique(("marquee-turn", clip.left().to_bits(), clip.right().to_bits()));
    let frame = ui.ctx().cumulative_frame_nr();
    let mut turn: MarqueeTurn = ui.data(|d| d.get_temp(key)).unwrap_or_default();
    if turn.frame != frame {
        turn = MarqueeTurn { frame, taken: None, pointed_before: turn.pointed_now, pointed_now: false };
    }
    let runs = if pointed {
        turn.pointed_now = true;
        true
    } else if turn.pointed_before || turn.pointed_now {
        false
    } else {
        match turn.taken {
            None => {
                turn.taken = Some(id);
                true
            }
            Some(taken) => taken == id,
        }
    };
    ui.data_mut(|d| d.insert_temp(key, turn));
    runs
}

/// [`cell_text`] for a line that also runs while it is chosen — though only
/// one chosen line a column at a time, see [`may_run`].
#[allow(clippy::too_many_arguments)]
pub(super) fn chosen_cell_text(
    ui: &Ui,
    cell: Rect,
    id: egui::Id,
    label: &str,
    role: Role,
    colour: Color32,
    pointed: bool,
    chosen: bool,
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

    let active = (pointed || chosen) && may_run(ui, id, pointed);
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

/// How much of a list has gone under an edge before its fade is all there, in
/// points: the fade grows in, deeper and stronger, as the rows go under it,
/// rather than standing there whole at the first point scrolled.
const FADE_GROWS_OVER: f32 = 48.0;

/// How many steps the fade is drawn in: an eased curve from the ground to
/// nothing, so it has no edge of its own where it ends, as a straight one had.
const FADE_STEPS: usize = 8;

/// A soft edge where a list runs out, instead of a cut.
///
/// Each edge only appears when there is something hidden past it. A fade
/// drawn unconditionally would wash out the top row of a list that has not
/// been scrolled at all — the reader would think the first entry was somehow
/// disabled. It grows in over the first [`FADE_GROWS_OVER`] points that go
/// under the edge, deeper and stronger together, eased at both ends.
pub(in crate::gui) fn paint_fade(ui: &Ui, viewport: Rect, hidden_above: f32, hidden_below: f32, palette: &Palette) {
    let most = FADE_HEIGHT.min(viewport.height() / 3.0);
    if most <= 0.0 {
        return;
    }
    let grown = |hidden: f32| smooth(hidden / FADE_GROWS_OVER);
    let depth = |grown: f32| most * (0.35 + 0.65 * grown);
    let top = grown(hidden_above);
    fade_band(ui, viewport, viewport.top(), 1.0, depth(top), top, palette);
    let bottom = grown(hidden_below);
    fade_band(ui, viewport, viewport.bottom(), -1.0, depth(bottom), bottom, palette);
}

/// Eased at both ends, 0 … 1.
fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// One fade across `viewport`, from `edge` `inwards` (1 down, −1 up),
/// `depth` deep: the ground at the edge, as strong as `strength`, thinning
/// along an eased curve to nothing.
fn fade_band(ui: &Ui, viewport: Rect, edge: f32, inwards: f32, depth: f32, strength: f32, palette: &Palette) {
    if strength <= 0.01 || depth <= 0.0 {
        return;
    }
    let ground = palette.ground;
    let mut mesh = egui::Mesh::default();
    for step in 0..=FADE_STEPS {
        let along = step as f32 / FADE_STEPS as f32;
        let y = edge + inwards * depth * along;
        let colour = ground.gamma_multiply(strength * (1.0 - smooth(along)));
        mesh.colored_vertex(egui::pos2(viewport.left(), y), colour);
        mesh.colored_vertex(egui::pos2(viewport.right(), y), colour);
        if step > 0 {
            let at = (step as u32) * 2;
            mesh.add_triangle(at - 2, at - 1, at);
            mesh.add_triangle(at - 1, at, at + 1);
        }
    }
    ui.painter().add(egui::Shape::mesh(mesh));
}

/// The same fade as at the top and foot of a list, stood on its side at the
/// right edge of `slot` — what the fourth column slides in from behind and
/// out behind, so it comes and goes the way rows scroll in and out rather
/// than being cut off at the margin.
///
/// `gone` is how much of the column is still out of sight, 0 … 1. The fade
/// is full while most of it is, and eases away over the last quarter of the
/// way in: a fade left standing on a column that has arrived would sit over
/// the right-hand figures, and one that vanished in a frame would be a jump.
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

/// Where the row the keyboard stands on was drawn this frame, in `key`'s column
/// — for the right-click menu opened by a key, which opens beside it.
pub(in crate::gui) fn note_cursor_row(ui: &Ui, key: &'static str, rect: Rect, here: bool) {
    if here {
        ui.ctx().data_mut(|d| d.insert_temp(cursor_row_id(key), rect));
    }
}

/// Where it was last drawn, if it was.
pub(in crate::gui) fn cursor_row(ctx: &egui::Context, key: &'static str) -> Option<Rect> {
    ctx.data(|d| d.get_temp::<Rect>(cursor_row_id(key)))
}

fn cursor_row_id(key: &'static str) -> egui::Id {
    egui::Id::unique(("kessler-cursor-row", key))
}

/// Keep the selected row inside the viewport, with room to spare.
///
/// The rectangle handed to egui is the row grown by a few rows' worth of margin
/// top and bottom. egui then scrolls the least it can to bring that whole
/// rectangle into view — which means the list starts moving while the selection
/// is still well clear of the edge, instead of chasing it once it has already
/// left.
pub(super) fn keep_visible(ui: &Ui, rect: Rect, m: Metrics, wanted: bool) {
    if !wanted {
        return;
    }
    ui.scroll_to_rect(
        rect.expand2(Vec2::new(0.0, m.row * SCROLL_MARGIN_ROWS)),
        None,
    );
}

/// A scrolling area with no bar — and, just as importantly, no room kept for
/// one.
///
/// This window says how long a list is with [`paint_extent`], a mark outside
/// the column that appears under the pointer; there has never been a bar to
/// see. But egui was still *reserving* its width — ten points, a six-point bar
/// and its four-point margin — the moment a list grew long enough to scroll.
/// The rows are drawn at the column's full width regardless, so the column
/// ended up ten points wider than it asked to be, the gap to its right looked
/// ten points too wide, and those ten points came off the track list.
///
/// The menu had already run into exactly this and says so at its own scroll
/// area; what was missing was applying the same finding to the columns. It
/// only shows on a list long enough to scroll, which is why a test library of
/// five artists never caught it and a column of several thousand did, at a
/// glance.
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

/// How much of a push past a list's end gets through, on a trackpad or a
/// touch mouse. egui lets a list run a third of the column out and springs
/// it back from there; the system's own lists give a little and come back.
/// Rather than holding the list at some line — which stopped it dead there
/// and only then let it slide back — the push itself is made smaller before
/// egui sees it, so its own spring, eased, does all the moving.
const PAST_THE_END: f32 = 0.07;

/// Takes the push past the end down to [`PAST_THE_END`] of itself: all of it
/// once the list stands at an end, and the part of it that would carry the
/// list over one. Only for the list under the pointer, which is the one the
/// wheel moves.
fn damp_past_the_end(ui: &Ui, id: &str) {
    damp_scrolling(ui, area_id(ui, id), below_id(id), ui.available_rect_before_wrap());
}

/// The same for any scroll area: `area` the id egui keeps its state under,
/// `below` where the caller keeps how much was left under it last frame (see
/// [`note_below`]), `over` where the area will stand — the menu and the key
/// sheet take it so. Said rather than taken from the `Ui`: in a row laid out
/// side by side, as the menu's tabs and body are, what is left of the row is a
/// line tall until something fills it, and the pointer was never in it.
pub(in crate::gui) fn damp_scrolling(ui: &Ui, area: egui::Id, below: egui::Id, over: egui::Rect) {
    if !ui.rect_contains_pointer(over) {
        return;
    }
    let touch = matches!(
        ui.input(|i| i.scroll_source()),
        Some(egui::MouseWheelSource::Trackpad | egui::MouseWheelSource::Momentum)
    );
    let Some(state) = egui::scroll_area::State::load(ui.ctx(), area) else { return };
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
        match ui.data(|d| d.get_temp::<f32>(below)) {
            Some(below) => below,
            None => return,
        }
    };
    let damped = damp(push, room);
    if damped != push {
        ui.input_mut(|i| i.smooth_scroll_delta.y = damped);
    }
}

/// `push` with what goes past `room` cut to [`PAST_THE_END`]; a push that
/// would cross the end only reaches it this frame, so that the bounce comes
/// from the small push of the next and not the whole of this one.
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

/// Keep how much of a scroll area was left under it this frame, for
/// [`damp_scrolling`] the next.
pub(in crate::gui) fn note_below<R>(ui: &Ui, below: egui::Id, output: &egui::scroll_area::ScrollAreaOutput<R>) {
    let left = (output.content_size.y - output.state.clamped_offset().y - output.inner_rect.height()).max(0.0);
    ui.data_mut(|d| d.insert_temp(below, left));
}

/// Where a list keeps how much was left under it last frame.
fn below_id(id: &str) -> egui::Id {
    egui::Id::unique(("below", id))
}

/// The id a list's scroll area keeps its state under, as egui makes it.
fn area_id(ui: &Ui, id: &str) -> egui::Id {
    ui.make_persistent_id(egui::IdSalt::new(id))
}

/// Where a pull on a list's extent mark puts the list, for the scroll area
/// to take up the next frame: since egui 0.37 its offset is no longer set
/// from outside it.
fn pull_id(id: &str) -> egui::Id {
    egui::Id::unique(("extent-pull", id))
}

/// Wrap a column's scrolling area so its edges fade rather than cut.
/// Returns how far below the view the list goes on, in points.
pub(super) fn scrolled(ui: &mut Ui, id: &str, palette: &Palette, body: impl FnOnce(&mut Ui)) -> f32 {
    scrolled_with(ui, id, palette, LIST_END_AIR, body)
}

/// Air under the last row of a list, inside the scrolling.
///
/// Scrolled all the way down, a list used to end with its last row standing on
/// the very edge of the column, where every other column has its fade — so the
/// one at its end looked like the one whose fade was missing. With this much
/// under it the end reads as an end: the row is clear of the edge, and until it
/// is, the fade is over it like everywhere else.
pub(super) const LIST_END_AIR: f32 = FADE_HEIGHT;

/// What a list running down to the window's edge keeps under its last row
/// besides: room for the words the status puts in the corner.
const EDGE_END_AIR: f32 = 28.0;

/// The ground of a list, under its rows: a click on it, beside or below them,
/// lets everything marked go, as a click beside the files does in the Finder.
/// Laid before the rows, so each row over it keeps its own clicks; call it
/// first thing in a `scrolled` body.
///
/// Held to the column: a scroll area that only scrolls down clips its body to
/// the parent's width, so the clip alone ran across the whole window, and the
/// queue's ground, laid after the other columns, took every click meant for
/// their rows.
pub(super) fn ground_clicked(ui: &mut Ui, id: &str) -> bool {
    let ground = ui.max_rect().intersect(ui.clip_rect());
    ui.interact(ground, egui::Id::unique(("ground", id)), Sense::click()).clicked()
}

/// Whether the column being drawn runs down to the window's edge, where its
/// list needs no fade at the foot — see `render`'s `reach`.
pub(super) fn column_at_edge_id() -> egui::Id {
    egui::Id::unique("column-at-edge")
}

/// `scrolled`, with a say in the air under the last row — none, for a block
/// that is sized to what it holds.
pub(super) fn scrolled_with(ui: &mut Ui, id: &str, palette: &Palette, end_air: f32, body: impl FnOnce(&mut Ui)) -> f32 {
    let at_edge = ui.data_mut(|d| d.remove_temp::<bool>(column_at_edge_id())).unwrap_or(false);
    let end_air = if at_edge { end_air + EDGE_END_AIR } else { end_air };
    let output = without_a_bar(ui, id).auto_shrink([false, false]).show(ui, |ui| {
        body(ui);
        ui.add_space(end_air);
    });

    let viewport = output.inner_rect;
    let above = output.state.clamped_offset().y;
    let below = (output.content_size.y - above - viewport.height()).max(0.0);
    ui.data_mut(|d| d.insert_temp(below_id(id), below));
    paint_fade(ui, viewport, above, if at_edge { 0.0 } else { below }, palette);
    paint_extent(ui, id, viewport, above, output.content_size.y, palette);
    below
}

/// How much of a column there is, and where in it you are — but only while the
/// pointer is over that column.
///
/// The fade says "there is more", which is not the same as "there is four
/// times as much". This is the figure the fade cannot give, and it is drawn as
/// a mark rather than a control: it appears under the pointer and carries no
/// arrows or trough, so a list still has nothing in it but names.
///
/// But it can be taken hold of, the way a Mac's own scroller can: pulled, it
/// moves the list; pressed beside it, the list jumps so the mark stands under
/// the hand. Under the pointer it widens a little and darkens — the only sign
/// it is there to be held. A pull is taken up by the list's scroll area the
/// next frame — see [`without_a_bar`].
/// The extent mark's width under the hand.
const WIDE: f32 = 5.0;

/// How far past the column's own edge the extent mark sits. Outside rather
/// than over: every row runs to the edge of its column, and a mark laid on top
/// of the last figure of a row would be the one place in the window where two
/// things overlap. Five and a half is half way between a marked row's plate and
/// the divider; at three it stood close enough to the marked row's plate to
/// read as part of it.
pub(super) const EXTENT_OUTSIDE: f32 = 5.5;

/// What answers the pointer: the strip the mark runs in, a little wider than
/// the mark at its widest, and stopping short of the divider in the middle of
/// the gap. The artists' letter bar takes the same strip.
pub(super) const EXTENT_REACH: f32 = EXTENT_OUTSIDE + WIDE + 1.0;

/// Set for a list whose letter bar stands where its extent mark would.
pub(super) fn extent_yields_id(id: &str) -> egui::Id {
    egui::Id::unique(("extent-yields", id))
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
    const OUTSIDE: f32 = EXTENT_OUTSIDE;
    const REACH: f32 = EXTENT_REACH;
    const FADE_SECS: f32 = 0.12;

    if ui.data(|d| d.get_temp::<bool>(extent_yields_id(id))).unwrap_or(false) {
        return;
    }

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
    let colour = palette.mid.gamma_multiply(shown * (0.55 + 0.3 * lift));
    if crate::gui::theme::square_ends() {
        super::paint_rule(ui, mark, colour);
    } else {
        ui.painter().rect_filled(mark, width / 2.0, colour);
    }
}

/// Below this the mark stops reading as a mark and starts reading as dirt.
pub(super) const EXTENT_MIN_LENGTH: f32 = 18.0;

/// Where the extent mark sits and how long it is, as offsets from the top of
/// the viewport. `None` when there is nothing to say.
///
/// Length says how much of the list is in front of you, position says where
/// that piece sits — both as proportions of the same track, so a long library
/// gives a short mark that starts at the top and ends at the bottom, and that
/// reads correctly without a single number.
pub(super) fn extent_mark(visible: f32, content: f32, offset: f32) -> Option<(f32, f32)> {
    if content <= visible + 1.0 || visible <= EXTENT_MIN_LENGTH {
        return None;
    }
    let length = (visible / content * visible).clamp(EXTENT_MIN_LENGTH, visible);
    let travel = visible - length;
    let progress = (offset / (content - visible)).clamp(0.0, 1.0);
    Some((travel * progress, length))
}

/// Reserve one row and report what the mouse did with it.
pub(super) fn allocate_row(ui: &mut Ui, width: f32, m: Metrics) -> (Rect, Response) {
    ui.allocate_exact_size(Vec2::new(width, m.row), Sense::click_and_drag())
}

/// How much air a column head keeps between its name and the word on the
/// right, below which the right-hand word is left off entirely.
pub(super) const HEADING_GAP: f32 = 14.0;

/// What the hand is holding, at the end of it.
///
/// Small, and beside the pointer rather than under it: what you need to see
/// while carrying something is where it will land, and a picture of the thing
/// itself would sit exactly on top of that. So it is a count and nothing else
/// — enough to know a whole record came with you and not only the row you
/// pressed on.
///
/// It also ends a drag that was let go of nowhere in particular. The queue
/// ends the ones that land on it; everything else ends here, having done
/// nothing, which is what abandoning a drag should cost.
/// A word on a plate at the pointer.
///
/// The thing that hangs off the cursor while something is being dragged, made
/// into the one way this window says something *at the pointer* rather than in
/// its own place: the count while records are carried, the volume while the
/// rule is pushed, the second the timeline is about to jump to. All three are
/// answers to "what am I doing right now", all three are gone the moment the
/// hand lets go, and none of them belongs anywhere on the page.
///
/// Above the pointer for a rule, below-right for a drag: a hand pushing a
/// rule sideways covers what is under it, and a hand carrying records is
/// looking at where they will land.
///
/// A rule up in the band — „super kompakt", and the volume in „eigene zeile"
/// and „mit cover" — has no room above it: the plate ran into the window's top
/// edge, and under or beside the rule the pointer was in the way. Those rules
/// say it in their own words instead — see `badge_fits_above`. It keeps clear
/// of the sides, and is laid over everything.
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

/// [`badge`], always centred over `at` or under it — for a mark that belongs to
/// a place on a rule rather than to the pointer.
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

/// The air the badge's plate keeps around the ink, sideways and up and down.
const BADGE_PAD: Vec2 = Vec2::new(7.0, 5.0);

/// How far the badge stands from the pointer, or from the rule.
const BADGE_AWAY: f32 = 10.0;

/// How near the badge may come to the window's edges.
const BADGE_EDGE: f32 = 16.0;

/// Whether `said`, on its plate over a rule whose middle is at `y`, stays clear
/// of the window's top edge. Where it would not, the rule says it in its own
/// words: the volume where „vol" stands, the timeline in the time before it.
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

/// One row of the queue: the title, and whose it is if there is room left.
///
/// The other columns lay the figure on the right out first and give the name
/// what is left, because a year has to line up down the column. Here it is the
/// other way round. The title is what the row is *about* and takes the room it
/// needs; the artist gets the remainder, and is left off entirely when the
/// remainder is too small to say anything — a name cut down to "acid m…" is
/// four characters of noise standing where nothing would have read better.
///
/// So a queue of one record shows title after title with the space beside them
/// empty, and a queue you have mixed together shows whose each one is, without
/// either case having to be asked for.
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
    /// The air between the two.
    const GAP: f32 = 12.0;
    /// The most the artist is given, so a long name cannot crowd a long title.
    const ARTIST_SHARE: f32 = 0.4;

    let (rect, response) = allocate_row(ui, width, m);
    if !ui.is_rect_visible(rect) {
        return response;
    }
    paint_marker(ui, rect, response.id, state, m, palette);
    paint_hover(ui, rect, &response, state, m, palette);

    let hovered = response.hovered();
    let ground = behind(ui, state, hovered, palette);
    let content = content_rect(rect, m);

    let beside = if state.gone { crate::lang::t(GONE) } else { artist };
    let to_come = !(state.gone || state.spent || state.playing || state.selected);
    let title_colour = eased_colour(ui, response.id, if to_come { palette.mid } else { state.primary(palette) });
    let beside_colour = if to_come { palette.mid } else { state.secondary(palette) };

    let artist_width = if beside.is_empty() {
        0.0
    } else {
        let whole = text::full_line(ui, beside, Role::Content, beside_colour).size().x;
        if state.gone { whole } else { whole.min(content.width() * ARTIST_SHARE) }
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
    chosen_cell_text(
        ui,
        Rect::from_min_max(egui::pos2(cell.left() + lead, cell.top()), cell.max),
        id,
        title,
        state.role(),
        title_colour,
        hovered,
        state.selected,
        ground,
        m,
    );
    response
}

/// How much of a row the figure on its right may take before it is cut.
///
/// Under half: the name is what the row is about and has to keep the larger
/// share even when what stands beside it is long.
pub(super) const FIGURE_SHARE: f32 = 0.45;

/// How long the arrow takes to push in, or to go again, in seconds.
pub(in crate::gui) const ARROW_SECS: f32 = 0.16;

/// The arrow before an active row's name, pushing in from the left edge of
/// `cell` over [`ARROW_SECS`] and taking the name along — see
/// `UiConfig::arrows`. Returns how far the name stands in from where it
/// would without one: nothing while there is none, and the arrow and its gap
/// once it is in.
pub(in crate::gui) fn lead_arrow(ui: &Ui, id: egui::Id, cell: Rect, line: f32, on: bool, colour: Color32) -> f32 {
    push_arrow(ui, id, cell, line, on, colour)
}

/// The same arrow, whether arrows are asked for or not.
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

/// A row with a name on the left and, optionally, a figure on the right.
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
    paint_marker(ui, rect, response.id, state, m, palette);
    paint_hover(ui, rect, &response, state, m, palette);

    let hovered = (response.hovered() && !state.still) || state.pointed;
    let ground = behind(ui, state, hovered, palette);
    let content = content_rect(rect, m);

    let mut name_right = content.right();
    if !figure.is_empty() {
        let width_at_rest = text::line_within(ui, figure, figure_role, palette.mid, content.width() * FIGURE_SHARE).size().x;
        let on_word = state.offering
            && response.hover_pos().is_some_and(|at| at.x >= content.right() - width_at_rest - 6.0);
        let figure_colour = match on_word {
            true if state.plated() => Palette::legible_on(palette.ink, palette.marked(), crate::gui::theme::ON_PLATE_LEAST),
            true => palette.ink,
            false => state.secondary(palette),
        };
        let galley = text::line_within(
            ui,
            figure,
            figure_role,
            figure_colour,
            content.width() * FIGURE_SHARE,
        );
        let width = galley.size().x;
        let top = if state.offering {
            text::on_x_middle(ui, &galley, figure_role, marker_rect(rect, m).center().y)
        } else {
            text::beside_content(ui, &galley, figure_role, rect.top(), m.row)
        };
        let at = egui::pos2(content.right() - width, top);
        ui.painter().galley(at, galley, palette.ink);
        name_right = content.right() - width - 12.0;
    }

    let line = text::row_line(ui, m.row, Role::Content);
    let cell = Rect::from_min_max(content.left_top(), egui::pos2(name_right, content.bottom()));
    let ink = eased_colour(ui, response.id, state.primary(palette));
    let lead = lead_arrow(ui, id, cell, line, state.playing || state.arrow, ink);
    chosen_cell_text(
        ui,
        Rect::from_min_max(egui::pos2(cell.left() + lead, cell.top()), cell.max),
        id,
        name,
        state.role(),
        ink,
        hovered,
        state.selected,
        ground,
        m,
    );

    response
}

/// One small column beside a name in [`columned_row`]: its words, how wide
/// it is, the face it is set in, and whether it stands flush right, as
/// figures do.
pub(super) struct Cell<'a> {
    pub text: &'a str,
    pub width: f32,
    pub role: Role,
    pub right: bool,
}

/// Between a name and the columns beside it, and between two of them.
pub(super) const FOUND_GAP: f32 = 12.0;

/// A row with a name on the left and small columns at its end, each as wide
/// as `cells` says, so the rows of a list stand in columns as the track
/// list's do. Under the pointer an action word, when there is one, takes the
/// columns' place, as it does in [`name_row`]. `bold` sets the name bold, as
/// a chosen row's is, unless the keyboard's plate is under it.
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
    paint_marker(ui, rect, response.id, state, m, palette);
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
    let ink = eased_colour(ui, response.id, state.primary(palette));
    let lead = lead_arrow(ui, id, cell, line, state.playing, ink);
    chosen_cell_text(
        ui,
        Rect::from_min_max(egui::pos2(cell.left() + lead, cell.top()), cell.max),
        id,
        name,
        if bold && !state.plated() { Role::ContentBold } else { state.role() },
        ink,
        hovered,
        state.selected,
        ground,
        m,
    );
    response
}

/// The light plate behind a head's word under the pointer, let out past the
/// column's edge: it stands a little wider than the word, and the word at a
/// column's left or right end is nearer the edge than that — cut there, its
/// corner on that side was square.
fn plate_beyond(ui: &Ui, plate: Rect, palette: &Palette) {
    let mut painter = ui.painter().clone();
    painter.set_clip_rect(ui.clip_rect().union(plate));
    painter.rect_filled(plate, crate::gui::theme::ROW_RADIUS, palette.hover);
}

/// A column heading: a label on the left, optionally one on the right.
///
/// It is also the sort control: clicking steps the column on to its next order.
/// The only mark of that is the sign after the label — an arrow for a
/// direction, a word where no arrow would do ("zuletzt"). Nothing that would
/// read as a button.
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

/// A head that does nothing: no plate under the pointer, neither on its name
/// nor on the count after it.
pub(super) fn still_heading(ui: &mut Ui, label: &str, trailing: &str, m: Metrics, palette: &Palette) {
    head_drawn(ui, label, trailing, None, None, None, true, false, m, palette);
}

/// [`still_heading`], with a word at its far end that does something — the head
/// itself does nothing and wears no plate, the word does.
pub(super) fn still_heading_with(ui: &mut Ui, label: &str, trailing: &str, at_end: Option<&str>, m: Metrics, palette: &Palette) -> Head {
    head_drawn(ui, label, trailing, None, None, at_end, true, false, m, palette)
}

/// [`heading`], with a second offer at the far end: a word of its own there, in
/// place of `trailing` while the pointer is anywhere on the head. The queue's
/// „leeren" stands there, beside the „schliessen" its name turns into, the way
/// every other column's does. Until then the count turned into „leeren" and the
/// name stayed, and the queue was the one column a click on its name did not
/// close.
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

/// [`heading`], with a figure at its far end that only says something —
/// how many there are — and so neither lights under the pointer nor takes
/// a click of its own; the head itself still does what it does.
pub(super) fn heading_counted(ui: &mut Ui, label: &str, figure: &str, mark: Option<&str>, m: Metrics, palette: &Palette) -> Head {
    head_drawn(ui, label, figure, mark, None, None, false, true, m, palette)
}

/// Any head; a `still` one answers no pointer.
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
        plate_beyond(ui, plate, palette);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    ui.painter().galley(at, galley, colour);

    let far_word = |ui: &mut Ui, word: &str, hit: Option<&Response>| {
        let lit = hit.is_some_and(Response::hovered);
        let colour = if lit { palette.ink } else { palette.mid };
        let galley = text::line(ui, word, Role::Label, colour);
        let at = egui::pos2(content.right() - galley.size().x, text::on_line(&galley, rect.top(), line));
        if lit {
            plate_beyond(ui, control_plate(ui, at, galley.size().x), palette);
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

/// A column head: the whole line, and the word at its right end when that word
/// is a control of its own.
pub(super) struct Head {
    pub(super) row: Response,
    pub(super) beside: Option<Response>,
    /// The word a head offers in place of its name, where it offers one:
    /// then only it is clicked, not the rest of the head.
    word: Option<Response>,
}

impl Head {
    /// A click on the head itself — not on the word at its far end.
    pub(super) fn clicked(&self) -> bool {
        match &self.word {
            Some(word) => word.clicked(),
            None => self.row.clicked() && !self.beside_clicked(),
        }
    }

    /// A click on that word.
    pub(super) fn beside_clicked(&self) -> bool {
        self.beside.as_ref().is_some_and(Response::clicked)
    }

}

/// One track row: the title, plus whichever fields are switched on.
#[allow(clippy::too_many_arguments)]
/// The air between a title and the mark that follows it.
///
/// Wider than a word space: the mark is not part of the title, and set any
/// closer it reads as the last syllable of one.
pub(super) const MARK_GAP: f32 = 10.0;

/// The air between an arrow and the name it stands before.
pub(super) const PLAYING_ARROW_GAP: f32 = 5.0;

/// How the library marks a track that is on the plugged-in iPod already,
/// `[ui] device_mark`. Two to try, and none.
pub const DEVICE_MARKS: [&str; 3] = ["wort", "punkt", "aus"];

/// The mark a row carries for being on the device, see [`DEVICE_MARKS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceMark {
    /// „ipod" beside the title, a label like „flac".
    Word,
    /// A dot there instead.
    Dot,
}

impl DeviceMark {
    /// The mark the config asks for, `None` for none.
    pub fn of(said: &str) -> Option<Self> {
        match said {
            "punkt" => Some(Self::Dot),
            "aus" => None,
            _ => Some(Self::Word),
        }
    }
}

/// How big the dot is, across.
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
    paint_marker(ui, rect, response.id, state, m, palette);
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
    let ink = eased_colour(ui, egui::Id::unique(("track-ink", track.id)), state.primary(palette));
    let lead = lead_arrow(
        ui,
        egui::Id::unique(("track-arrow", track.id)),
        title_cell,
        text::row_line(ui, m.row, Role::Content),
        state.playing && !state.gone,
        ink,
    );
    chosen_cell_text(
        ui,
        Rect::from_min_max(egui::pos2(title_cell.left() + lead, title_cell.top()), title_cell.max),
        egui::Id::unique(("track-title", track.id)),
        track.display_title(),
        state.role(),
        ink,
        hovered,
        state.selected,
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

/// The mark behind a field's value while cmd+a has all of it, so is something
/// you see rather than something you remember having pressed. Not the rows'
/// marker, which the search field uses: the field being typed in already stands
/// on that, and the mark vanished into it. The ink, thinned, over whatever is
/// behind — the words stay in their own colour on it. `at` is where the value's
/// galley is drawn.
pub(super) fn paint_field_mark(ui: &Ui, at: egui::Pos2, value: &egui::Galley, rect: Rect, palette: &Palette) {
    let mark = Rect::from_min_max(
        egui::pos2(at.x - 3.0, rect.top() + 4.0),
        egui::pos2(at.x + value.size().x + 3.0, rect.bottom() - 4.0),
    );
    ui.painter()
        .rect_filled(mark, MARKER_RADIUS, palette.ink.gamma_multiply(0.18));
}

/// How many characters of the artist a compilation's track shows beside its
/// title.
const BY_CHARS: usize = 28;

/// `name`, cut to `most` characters with an ellipsis.
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
    /// The state the stretch is read from is the list's own: were the id
    /// made differently, nothing would ever be held.
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

    /// A push inside the list goes through whole; one that would cross the
    /// end stops on it; at the end only a little of it is left.
    #[test]
    fn a_push_past_the_end_is_made_small() {
        assert_eq!(super::damp(40.0, 100.0), 40.0);
        assert_eq!(super::damp(-40.0, 10.0), -10.0);
        assert!((super::damp(40.0, 0.0) - 40.0 * super::PAST_THE_END).abs() < 1e-4);
    }
}
