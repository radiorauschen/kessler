//! The controls in the masthead: the switches, the volume, the progress
//! rule, the transport keys and the words the status line uses.

use super::*;

/// The volume, in the transport line: a word and a figure, not a control —
/// the rule beside it is what changes them.
///
/// Two galleys rather than one, because they are two different things. „vol"
/// is a word and is set like every other word up there; the number is a
/// number, and set in the mono cut for the reason every figure in this window
/// is: it changes under your hand, and in a proportional face a 1 is narrower
/// than a 7, so the word beside it would shuffle sideways at every step.
///
/// `figure`, while the rule is pushed and there is no room for the badge
/// over it, stands in the word's place, set to end where the word ends.
///
/// True when the word was right-clicked: it opens the equalizer as the
/// rule beside it does, the word being the bigger thing to hit.
pub(super) fn transport_volume(ui: &Ui, right: &mut f32, top: f32, palette: &Palette, figure: Option<u8>) -> bool {
    let word = text::line(ui, tr!("vol"), Role::Label, palette.mid);
    let end = *right;
    *right -= word.size().x;
    match figure {
        Some(figure) => {
            let g = text::line(ui, &figure.to_string(), Role::FiguresLabel, palette.ink);
            let y = text::ink_on(&g, text::ink_middle(ui, Role::Label, top));
            ui.painter().galley(egui::pos2(end - g.size().x, y), g, palette.ink);
        }
        None => ui.painter().galley(egui::pos2(*right, top), word.clone(), palette.mid),
    }
    let hit = Rect::from_min_size(egui::pos2(*right, top), word.size()).expand(3.0);
    *right -= CONTROL_GAP;
    ui.interact(hit, egui::Id::unique("volume-word"), Sense::click()).secondary_clicked()
}

/// What „vol" takes, measured the way it is drawn.
pub(super) fn volume_width(ui: &Ui, palette: &Palette) -> f32 {
    text::line(ui, tr!("vol"), Role::Label, palette.mid).size().x
}

/// How far apart two header controls stand, measured text to text.
///
/// Wide enough that their plates keep clear air between them: each plate
/// reaches `CONTROL_PAD_X` past its own word, so the gap left over is this
/// minus twice that, and it has to stay larger than the padding itself or the
/// plate looks closer to its neighbour than to the word it belongs to.
/// Twenty-eight: the twelve left between two plates is still more than the
/// eight a plate reaches past its word.
pub(super) const CONTROL_GAP: f32 = 28.0;

/// How far a hover plate reaches past the word it sits behind: sideways from
/// the type's own box, above and below from its **ink**.
///
/// Eight points every way, and the two numbers are the same number for a
/// reason — what the eye checks on a plate this small is whether the word sits
/// in the middle of it, and it cannot do that if the four gaps are set from
/// two different edges.
pub(super) const CONTROL_PAD_X: f32 = 8.0;

pub(super) const CONTROL_PAD_Y: f32 = 8.0;

/// The rectangle a header control occupies: its word, plus the air around it
/// that the pointer counts as part of it.
///
/// Measured from the ink, not from the box. The box keeps room above the
/// ascender and below the descender — and in some faces a line gap below that
/// again. Grown from it, the plate carried three points more air under the
/// word than over it: nothing you can name at a glance, everything you can see
/// once somebody says it.
///
/// The band is taken from the face with the usual probe rather than from the
/// word in front of us, so `liste`, `raster` and `menü` get plates of one
/// height that line up with each other — a word with an umlaut must not sit in
/// a taller plate than the word beside it.
pub(in crate::gui) fn control_plate(ui: &Ui, at: egui::Pos2, width: f32) -> Rect {
    plate_around(
        text::ink_middle(ui, Role::Label, at.y),
        text::ink_height(ui, Role::Label),
        at.x,
        width,
    )
}

/// The plate itself, once the face has been asked where its ink is.
///
/// Apart from the measuring so that it can be read, and checked, without a
/// window: `middle` is where the word's ink sits and `ink` how tall it stands.
/// The whole of the rule is that the air above the ink and the air below it
/// are one number — which is what a plate grown from the box could not do.
pub(super) fn plate_around(middle: f32, ink: f32, left: f32, width: f32) -> Rect {
    let half = ink / 2.0 + CONTROL_PAD_Y;
    Rect::from_min_max(
        egui::pos2(left - CONTROL_PAD_X, middle - half),
        egui::pos2(left + width + CONTROL_PAD_X, middle + half),
    )
}

/// The faintest possible hover, on a control in the header.
///
/// Exactly what a row in a list gets: same fill, same corner. Two different
/// ways of saying "you can click this" in one window is one too many.
pub(in crate::gui) fn paint_control_hover(
    ui: &Ui,
    plate: Rect,
    response: &Response,
    palette: &Palette,
) {
    if response.hovered() {
        ui.painter().rect_filled(plate, crate::gui::theme::ROW_RADIUS, palette.hover);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
}

/// The words of a text link: in the ink under the pointer, as the header's
/// controls are, and in their own tone otherwise. News keeps the accent,
/// which says what it is.
pub(super) fn hover_ink(response: &Response, colour: Color32, palette: &Palette) -> Color32 {
    if response.hovered() && colour != palette.accent {
        palette.ink
    } else {
        colour
    }
}

/// The volume rule, shown beside its label while it is open.
///
/// Same language as the progress rule — two pixels of mark, a taller strip to
/// grab — because they are the same kind of thing and there is no reason for a
/// second one.
///
/// What it hands back is the figure for „vol" to say in its place, while the
/// rule is pushed with no room for the badge over it.
pub(super) fn volume_rule(app: &mut GuiApp, ui: &Ui, right: &mut f32, top: f32, palette: &Palette) -> Option<u8> {
    const WIDTH: f32 = VOLUME_WIDTH;
    const GRAB_HEIGHT: f32 = 16.0;

    let middle = text::ink_middle(ui, Role::Label, top);
    *right -= WIDTH;
    let strip = Rect::from_min_size(
        egui::pos2(*right, middle - GRAB_HEIGHT / 2.0),
        Vec2::new(WIDTH, GRAB_HEIGHT),
    );
    let response = ui.interact(strip, egui::Id::unique("volume-rule"), Sense::click_and_drag());
    if response.secondary_clicked() {
        app.toggle_eq();
    }

    if response.dragged() || response.clicked() {
        if let Some(pointer) = response.interact_pointer_pos() {
            let fraction = ((pointer.x - strip.left()) / WIDTH).clamp(0.0, 1.0);
            app.player.set_volume((fraction * 100.0).round() as u8);
        }
    }

    let mut turned = false;
    if response.hovered() {
        let amount: f32 = ui.input(|i| {
            i.raw
                .events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::MouseWheel { unit, delta, .. } => Some(
                        (delta.y + delta.x)
                            * match unit {
                                egui::MouseWheelUnit::Point => 1.0,
                                egui::MouseWheelUnit::Line => VOLUME_SCROLL_LINE,
                                egui::MouseWheelUnit::Page => VOLUME_SCROLL_POINTS * 10.0,
                            },
                    ),
                    _ => None,
                })
                .sum()
        });
        if amount != 0.0 {
            let carry_id = egui::Id::unique("volume-scroll-carry");
            let carry: f32 = ui.data(|d| d.get_temp(carry_id)).unwrap_or(0.0) + amount / VOLUME_SCROLL_POINTS;
            let steps = carry.trunc();
            ui.data_mut(|d| d.insert_temp(carry_id, carry - steps));
            if steps != 0.0 {
                let was = i32::from(app.player.volume());
                app.player.set_volume((was + steps as i32).clamp(0, 100) as u8);
                let now = ui.input(|i| i.time);
                ui.data_mut(|d| d.insert_temp(egui::Id::unique("volume-scrolled-at"), now));
            }
        }
        if let Some(at) = ui.data(|d| d.get_temp::<f64>(egui::Id::unique("volume-scrolled-at"))) {
            let left = VOLUME_SCROLL_SHOWN - (ui.input(|i| i.time) - at);
            if left > 0.0 {
                turned = true;
                ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(left));
            }
        }
    }

    let hot = response.hovered() || response.dragged();
    let (height, _) = rule_swell(swell(ui, "volume", hot));
    let knob = knob_swell(ui, "volume", hot);
    let rule = Rect::from_min_size(
        egui::pos2(strip.left(), strip.center().y - height / 2.0),
        Vec2::new(WIDTH, height),
    );
    paint_rule(ui, rule, palette.rule);

    let filled = WIDTH * app.player.volume() as f32 / 100.0;
    if filled > 0.0 {
        paint_rule(ui, Rect::from_min_size(rule.min, Vec2::new(filled, height)), palette.ink);
    }

    if knob > height / 2.0 {
        knob_at(ui, egui::pos2(rule.left() + filled, rule.center().y), knob, height, palette.ink);
    }

    let mut in_the_word = None;
    if response.dragged() || response.clicked() {
        if let Some(at) = response.interact_pointer_pos() {
            let said = app.player.volume().to_string();
            if badge_fits_above(ui, rule.center().y, &said) {
                badge(ui, egui::pos2(at.x, rule.center().y), &said, true, palette);
            } else {
                in_the_word = Some(app.player.volume());
            }
        }
    }
    if turned && !(response.dragged() || response.clicked()) {
        let said = app.player.volume().to_string();
        if badge_fits_above(ui, rule.center().y, &said) {
            badge(ui, egui::pos2(rule.left() + filled, rule.center().y), &said, true, palette);
        } else {
            in_the_word = Some(app.player.volume());
        }
    }

    *right -= VOLUME_GAP;
    in_the_word
}

/// How many points of scrolling over the volume rule make one step of it.
const VOLUME_SCROLL_POINTS: f32 = 8.0;

/// How many points a notch of a wheel counts for over the volume rule: two
/// steps of it.
const VOLUME_SCROLL_LINE: f32 = 2.0 * VOLUME_SCROLL_POINTS;

/// How long the figure stays after the volume rule was scrolled, in seconds.
const VOLUME_SCROLL_SHOWN: f64 = 0.8;

/// How far a key's mark stands in from the edge of its cell.
pub(super) const KEY_AIR: f32 = 5.0;

/// The air between „vol" and its rule.
pub(super) const VOLUME_GAP: f32 = 10.0;

/// The progress rule: two pixels to look at, a taller strip to grab.
///
/// It is drawn whether or not anything is playing — empty, it is the one line
/// the design keeps, and it holds the header together as a block. The exception
/// to "no lines" is that this one always turns out to be carrying something:
/// how far into a track you are, or that there is no track. The chapters on the
/// timeline `rule`, the starts as shares of the way: heard and not heard as
/// with music, a mark where each begins — no shade for every other chapter.
///
/// `lit`, the chapter pointed at — on the rule or in the list — stands out a
/// shade more, its number by it.
fn paint_chapters(ui: &Ui, rule: Rect, starts: &[f32], palette: &Palette, lit: Option<usize>, plated: bool) {
    if starts.len() < 2 {
        return;
    }
    /// How wide the cut where a chapter begins is.
    const CUT: f32 = 1.5;
    let x = |share: f32| rule.left() + rule.width() * share;
    for (i, &start) in starts.iter().enumerate() {
        let from = x(start);
        if i > 0 && from > rule.left() + CUT && from < rule.right() - CUT {
            let mark = Rect::from_center_size(egui::pos2(from, rule.center().y), Vec2::new(MARK_WIDTH, rule.height() + MARK_OUT * 2.0));
            ui.painter().rect_filled(mark, 0.0, sharpest(palette));
        }
    }
    let Some(i) = lit.filter(|i| *i < starts.len()) else { return };
    let from = x(starts[i]);
    let to = starts.get(i + 1).map_or(rule.right(), |&next| x(next));
    let inset = if i > 0 { MARK_WIDTH / 2.0 } else { 0.0 };
    let part = Rect::from_min_max(egui::pos2(from + inset, rule.top()), egui::pos2(to - MARK_WIDTH / 2.0, rule.bottom()));
    ui.painter().rect_filled(part, 0.0, palette.ink.gamma_multiply(0.25));
    chapter_number(ui, rule, from, i + 1, plated, palette);
}

/// How wide a chapter's mark on the timeline is, and how far it stands out
/// of the rule above and below.
const MARK_WIDTH: f32 = 2.0;
const MARK_OUT: f32 = 2.5;

/// The tone of the scheme that stands out most from its ground — the ink, the
/// accent or the second tone, whichever it is in this scheme — for the
/// chapters' marks.
fn sharpest(palette: &Palette) -> Color32 {
    [palette.ink, palette.accent, palette.mid]
        .into_iter()
        .max_by(|a, b| crate::gui::theme::contrast(*a, palette.ground).total_cmp(&crate::gui::theme::contrast(*b, palette.ground)))
        .unwrap_or(palette.ink)
}

/// The number of a chapter by the rule at `x`, where it begins. `plated` — the
/// compact header — on the same plate the volume's figure wears while it is
/// pulled, above the rule where the window has room, else under it; in the
/// super compact one, small, in the scheme's sharpest tone on the ground.
fn chapter_number(ui: &Ui, rule: Rect, x: f32, number: usize, plated: bool, palette: &Palette) {
    let said = number.to_string();
    if plated {
        let above = badge_fits_above(ui, rule.center().y, &said);
        badge_centred(ui, egui::pos2(x, rule.center().y), &said, above, palette);
        return;
    }
    /// The air between the rule and the figure, and around the figure.
    const AWAY: f32 = 5.0;
    const PAD: Vec2 = Vec2::new(4.0, 2.0);
    let tone = sharpest(palette);
    let g = text::line(ui, &said, Role::FiguresLabel, tone);
    let ink = text::ink_size(&g);
    let above = rule.top() - AWAY - ink.y - PAD.y * 2.0 >= ui.ctx().content_rect().top();
    let top = if above { rule.top() - AWAY - ink.y - PAD.y * 2.0 } else { rule.bottom() + AWAY };
    let size = ink + PAD * 2.0;
    let left = (x - size.x / 2.0).clamp(rule.left(), (rule.right() - size.x).max(rule.left()));
    let plate = Rect::from_min_size(egui::pos2(left, top), size);
    let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("chapter-number")));
    painter.rect_filled(plate, MARKER_RADIUS, palette.ground);
    painter.galley(text::ink_centred(&g, plate), g, tone);
}

/// The air between a time and the end of the rule it belongs to.
const TIME_GAP: f32 = 14.0;

/// The shortest stretch of rule worth drawing after the time: less, and it
/// read as a stray dash beside the figures.
const RULE_STROKE_LEAST: f32 = 40.0;

/// How much of the line the time and its rule need at the least: the time at
/// its widest, its gap, and a rule worth the name. Without a track, the time
/// is not there and the rule alone is asked for.
pub(super) fn rule_least(app: &GuiApp, ui: &Ui, palette: &Palette, m: Metrics) -> f32 {
    let time = if app.player.current_track().is_some() {
        let whole = text::duration(app.player.duration().as_secs());
        text::line(ui, &format!("{whole} / {whole}"), Role::FiguresLabel, palette.mid).size().x + TIME_GAP
    } else {
        0.0
    };
    time + RULE_STROKE_LEAST + m.marker_pad_x * 2.0
}

pub(super) fn progress_rule(
    app: &mut GuiApp,
    ui: &mut Ui,
    palette: &Palette,
    m: Metrics,
    width: f32,
) {
    const GRAB_HEIGHT: f32 = 14.0;

    let (row, response) =
        ui.allocate_exact_size(Vec2::new(width, GRAB_HEIGHT), Sense::click_and_drag());
    let mut strip = content_rect(row, m);

    let has_track = app.player.current_track().is_some();
    let hot = has_track && (response.hovered() || response.dragged());
    let scrubbing = egui::Id::unique("scrubbing");
    let in_the_time = has_track && response.dragged() && !badge_fits_above(ui, strip.center().y, "0:00");
    let pushed_to = in_the_time
        .then(|| ui.data(|d| d.get_temp::<f32>(scrubbing)))
        .flatten()
        .map(|f| (f64::from(f) * app.player.duration().as_secs_f64()).max(0.0) as u64);
    let (at_secs, at_colour) = match pushed_to {
        Some(secs) => (secs, palette.ink),
        None => (app.player.position().as_secs(), palette.mid),
    };

    if has_track {
        let whole = text::duration(app.player.duration().as_secs());
        let said = format!("{} / {whole}", text::duration(at_secs));
        let widest = text::line(ui, &format!("{whole} / {whole}"), Role::FiguresLabel, palette.mid)
            .size()
            .x;
        let g = text::line(ui, &said, Role::FiguresLabel, at_colour);
        let y = text::ink_on(&g, strip.center().y);
        ui.painter()
            .galley(egui::pos2(strip.left(), y), g, at_colour);
        strip = Rect::from_min_max(
            egui::pos2(strip.left() + widest + TIME_GAP, strip.top()),
            strip.right_bottom(),
        );
    }

    if !has_track && app.on_air.is_none() {
        ui.data_mut(|d| d.remove_temp::<Show>(egui::Id::unique("station-show")));
        if app.showing_radio() {
            return;
        }
    }
    if !has_track && app.on_air.is_some() {
        let (end, on) = station_rule(app, ui, strip, palette);
        let shown = slide_for(ui.ctx(), egui::Id::unique("station-live"), on, false, STATION_LIVE_SECS);
        if shown > 0.0 {
            let colour = palette.mid.gamma_multiply(shown);
            let g = text::line(ui, tr!("live"), Role::Label, colour);
            let y = text::ink_on(&g, strip.center().y);
            ui.painter().galley(egui::pos2(end + STATION_LIVE_GAP, y), g, colour);
        }
        return;
    }

    let fraction_at = |x: f32| ((x - strip.left()) / strip.width().max(1.0)).clamp(0.0, 1.0) as f64;

    let held = response
        .dragged()
        .then(|| response.interact_pointer_pos().map(|at| fraction_at(at.x)))
        .flatten();

    if let Some(fraction) = held {
        ui.data_mut(|d| d.insert_temp(scrubbing, fraction as f32));
    }
    let landed = if response.drag_stopped() {
        let last = ui.data(|d| d.get_temp::<f32>(scrubbing)).map(f64::from);
        ui.data_mut(|d| d.remove_temp::<f32>(scrubbing));
        last.or_else(|| response.interact_pointer_pos().map(|at| fraction_at(at.x)))
    } else if response.clicked() {
        response.interact_pointer_pos().map(|at| fraction_at(at.x))
    } else {
        None
    };
    if has_track {
        if let Some(fraction) = landed {
            if !app.player.seek_to_fraction(fraction) {
                app.set_status(tr!("dieses format lässt sich nicht spulen"));
            }
        }
    }

    let (height, _) = rule_swell(swell(ui, "timeline", hot));
    let knob = knob_swell(ui, "timeline", hot);
    let top = strip.center().y - height / 2.0;
    let rule = Rect::from_min_size(
        egui::pos2(strip.left(), top),
        Vec2::new(strip.width(), height),
    );
    paint_rule(ui, rule, palette.rule);

    let progress = held
        .map(|f| f as f32)
        .unwrap_or_else(|| app.player.progress().clamp(0.0, 1.0) as f32);
    let (fraction, colour) = match (has_track, app.scan_fraction()) {
        (true, _) => (progress, palette.accent),
        (false, Some(scan)) => (scan, palette.mid),
        (false, None) => (0.0, palette.accent),
    };
    if fraction > 0.0 {
        let filled = Rect::from_min_size(rule.min, Vec2::new(rule.width() * fraction, height));
        paint_rule(ui, filled, colour);
    }
    if has_track {
        let starts = app.playing_chapter_starts();
        let share = held.map(|f| f as f32).or_else(|| {
            response.hovered().then(|| response.hover_pos().map(|at| fraction_at(at.x) as f32)).flatten()
        });
        let under = share.filter(|_| starts.len() > 1).map(|s| starts.iter().rposition(|start| *start <= s).unwrap_or(0));
        point_at_chapter(ui.ctx(), ON_THE_RULE, under);
        let lit = under.or_else(|| pointed_chapter(ui.ctx(), IN_THE_LIST));
        let plated = PlayingAt::now(&app.config.ui) != PlayingAt::SuperCompact;
        paint_chapters(ui, rule, &starts, palette, lit, plated);
    }

    if knob > height / 2.0 {
        let at = egui::pos2(rule.left() + rule.width() * progress, rule.center().y);
        knob_at(ui, at, knob, height, colour);
    }

    if has_track && !in_the_time {
        if let (Some(fraction), Some(pointer)) = (held, response.interact_pointer_pos()) {
            let seconds = fraction * app.player.duration().as_secs_f64();
            badge(
                ui,
                egui::pos2(pointer.x, rule.center().y),
                &text::duration(seconds.max(0.0) as u64),
                true,
                palette,
            );
        }
    }
}

/// The rule while a station plays: it shows that a connection is being made,
/// and then that one stands — a little rule, `STATION_SPAN` wide, as wide as
/// the volume's, that draws itself in to a dot that pulses.
///
/// One movement after the other, each eased and in its own time:
/// see [`Step`]. Paused, it draws itself out to the little rule again, still
/// and faint, and wanders there once play tunes the station in anew. Returns
/// where it ends, for „live" after it, and whether „live" is to stand.
fn station_rule(app: &GuiApp, ui: &Ui, strip: Rect, palette: &Palette) -> (f32, bool) {
    let tuning = app.player.tuning();
    let playing = app.player.state() == crate::player::PlaybackState::Playing;
    let now = ui.input(|i| i.time);
    let id = egui::Id::unique("station-show");
    let url = app.on_air.as_ref().map(|s| s.url.clone()).unwrap_or_default();
    let mut show = ui.data(|d| d.get_temp::<Show>(id)).unwrap_or(Show { step: Step::Appearing, since: now, from: 0.0, round: 0.0, station: url.clone() });
    let paused = !playing && !tuning;
    if paused && !matches!(show.step, Step::Leaving | Step::Gone) {
        show = Show { step: Step::Leaving, since: now, from: show.out(now), round: show.roundness(now), station: url.clone() };
    } else if !paused && matches!(show.step, Step::Leaving | Step::Gone) {
        show = Show { step: Step::Appearing, since: now, from: 0.0, round: 0.0, station: url.clone() };
    }
    let wants_out = tuning;
    let another = show.station != url;
    if (wants_out || another) && matches!(show.step, Step::Closing | Step::Waiting | Step::Live) {
        show = Show { step: Step::Opening, since: now, from: show.out(now), round: show.roundness(now), station: url.clone() };
    }
    show.station = url;
    loop {
        let t = now - show.since;
        let next = match show.step {
            Step::Appearing if t >= STATION_APPEAR_SECS => Step::Opening,
            Step::Opening if t >= show.opening_secs() => Step::Wandering,
            Step::Wandering if !wants_out && t >= STATION_TUNE_SECS => Step::Closing,
            Step::Closing if t >= STATION_CLOSE_SECS => Step::Waiting,
            Step::Waiting if t >= STATION_WAIT_SECS => Step::Live,
            Step::Leaving if t >= show.leaving_secs() => Step::Gone,
            _ => break,
        };
        let done = match show.step {
            Step::Appearing => STATION_APPEAR_SECS,
            Step::Opening => show.opening_secs(),
            Step::Wandering => t,
            Step::Closing => STATION_CLOSE_SECS,
            Step::Leaving => show.leaving_secs(),
            _ => STATION_WAIT_SECS,
        };
        let from = if next == Step::Opening { 0.0 } else { 1.0 };
        show = Show { step: next, since: show.since + done, from, round: 0.0, station: show.station };
    }
    ui.data_mut(|d| d.insert_temp(id, show.clone()));
    let square = super::super::theme::square_ends();
    let settled = matches!(show.step, Step::Live | Step::Leaving | Step::Gone) || (square && show.step == Step::Waiting);
    ui.data_mut(|d| d.insert_temp(egui::Id::unique("station-settled"), (settled, now)));

    let t = now - show.since;
    if show.step == Step::Gone {
        return (strip.left(), false);
    }
    let out = show.out(now);
    let grown = match show.step {
        Step::Appearing => ease_in_out((t / STATION_APPEAR_SECS) as f32),
        Step::Leaving => 1.0 - ease_in_out(((t - show.closing_part()) / STATION_GO_SECS) as f32),
        _ => 1.0,
    };
    let width = STATION_DOT + (STATION_SPAN.min(strip.width()) - STATION_DOT).max(0.0) * out;
    let height = STATION_DOT + (STATION_TUNE_THICK - STATION_DOT) * out;
    let shape = Rect::from_min_size(egui::pos2(strip.left(), strip.center().y - height / 2.0), Vec2::new(width, height));
    let still = if playing || tuning { palette.accent } else { palette.mid };

    let round = if square { show.roundness(now) } else { 1.0 };

    if out <= 0.0 {
        let centre = shape.center();
        let side = STATION_DOT * grown;
        match show.step {
            Step::Appearing | Step::Closing | Step::Leaving => ui.ctx().request_repaint(),
            Step::Waiting if square => ui.ctx().request_repaint(),
            Step::Waiting => wake_in(ui, STATION_WAIT_SECS - t),
            _ => {}
        }
        let colour = still.gamma_multiply(grown);
        if square {
            let dot = Rect::from_center_size(centre, Vec2::splat(side));
            rounded_box(ui.painter(), dot, round * side / 2.0, colour);
        } else {
            ui.painter().circle_filled(centre, side / 2.0, colour);
        }
        let live = show.step == Step::Live || (square && show.step == Step::Waiting);
        return (shape.right(), live);
    }

    let ground = lerp_colour(still, palette.rule, out);
    if square {
        rounded_box(ui.painter(), shape, round * height.min(width) / 2.0, ground);
    } else {
        ui.painter().rect_filled(shape, capped(height.min(width)), ground);
    }
    let moving = show.step == Step::Wandering && (playing || tuning);
    if moving {
        let phase = (t % STATION_TUNE_SECS) / STATION_TUNE_SECS;
        let wave = (0.5 - 0.5 * (phase * std::f64::consts::TAU).cos()) as f32;
        let piece = (shape.width() * STATION_PIECE).max(height * 4.0).min(shape.width());
        let left = shape.left() + (shape.width() - piece) * wave;
        let travelling = Rect::from_min_size(egui::pos2(left, shape.top()), Vec2::new(piece, height));
        ui.painter().rect_filled(travelling, capped(height), palette.accent);
    }
    if moving || matches!(show.step, Step::Opening | Step::Closing | Step::Leaving) {
        ui.ctx().request_repaint();
    }
    (shape.right(), false)
}

/// A box with corners rounded by `radius`, any fraction of a point, as one
/// outline — so that a colour faded in or out is even all over, with no
/// overlaps.
fn rounded_box(painter: &egui::Painter, rect: Rect, radius: f32, colour: Color32) {
    let r = radius.clamp(0.0, rect.width().min(rect.height()) / 2.0);
    if r < 0.05 {
        painter.rect_filled(rect, 0, colour);
        return;
    }
    const STEPS: usize = 8;
    let inner = rect.shrink(r);
    let corners = [
        (inner.right_top(), -90.0_f32),
        (inner.right_bottom(), 0.0),
        (inner.left_bottom(), 90.0),
        (inner.left_top(), 180.0),
    ];
    let mut points = Vec::with_capacity(4 * (STEPS + 1));
    for (centre, from) in corners {
        for i in 0..=STEPS {
            let angle = (from + 90.0 * i as f32 / STEPS as f32).to_radians();
            points.push(centre + r * Vec2::new(angle.cos(), angle.sin()));
        }
    }
    painter.add(egui::Shape::convex_polygon(points, colour, egui::Stroke::NONE));
}

/// Whether the station's rule has done its work, so its line may say what
/// the station plays — see `station_rule`. A rule not drawn for a moment
/// (another layout) holds nothing back.
pub(super) fn station_settled(ctx: &egui::Context) -> bool {
    let now = ctx.input(|i| i.time);
    match ctx.data(|d| d.get_temp::<(bool, f64)>(egui::Id::unique("station-settled"))) {
        Some((settled, at)) if now - at < 0.5 => settled,
        _ => true,
    }
}

/// Wake the window `secs` from now, on time: egui takes a frame of the
/// screen's off every wait it is asked for (see `picture::on_grid`), and
/// woken that much early the rule would ask again for the few milliseconds
/// left and draw the same picture twice.
fn wake_in(ui: &Ui, secs: f64) {
    let early = f64::from(ui.input(|i| i.predicted_dt));
    ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(secs.max(0.01) + early));
}

/// The movements of the rule while a station plays, one after the other.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Step {
    /// A small ball comes up, in `STATION_APPEAR_SECS`.
    #[default]
    Appearing,
    /// It pushes out to the right into the little rule.
    Opening,
    /// A piece travels to and fro on it while the station is tuned in —
    /// at least once.
    Wandering,
    /// Tuned in: the rule draws itself in to the ball again.
    Closing,
    /// A moment still.
    Waiting,
    /// The ball stands still, with „live" beside it.
    Live,
    /// Paused: the rule draws in, then the ball shrinks away.
    Leaving,
    /// Nothing drawn until play comes again.
    Gone,
}

/// Where the rule is in its movements: which, since when, — opening — from
/// how far out it began, and for which station (by its address).
#[derive(Debug, Clone, Default, PartialEq)]
struct Show {
    step: Step,
    since: f64,
    from: f32,
    /// How round it was when this step began, 0 square and 1 the dot — for
    /// a square-ended face only.
    round: f32,
    station: String,
}

impl Show {
    /// Opening takes the time the rest of the way takes.
    fn opening_secs(&self) -> f64 {
        (STATION_OPEN_SECS * f64::from(1.0 - self.from)).max(0.001)
    }

    /// Leaving, how long the rule takes to draw in: as long as the rest of
    /// the way takes.
    fn closing_part(&self) -> f64 {
        STATION_CLOSE_SECS * f64::from(self.from)
    }

    /// The whole of leaving: drawing in, then the ball going.
    fn leaving_secs(&self) -> f64 {
        self.closing_part() + STATION_GO_SECS
    }

    /// How round the shape is, 0 square and 1 the dot, for a square-ended
    /// face: square until the station is there, rounding while it waits,
    /// round once live; drawn out again, it squares off first, and leaving
    /// it keeps what it had.
    fn roundness(&self, now: f64) -> f32 {
        let t = now - self.since;
        match self.step {
            Step::Appearing | Step::Wandering | Step::Closing => 0.0,
            Step::Opening => self.round * (1.0 - ease_in_out((t / STATION_SQUARE_SECS) as f32)),
            Step::Waiting => ease_in_out((t / STATION_WAIT_SECS) as f32),
            Step::Live => 1.0,
            Step::Leaving | Step::Gone => self.round,
        }
    }

    /// How far out the rule is, 0 the ball and 1 the little rule.
    fn out(&self, now: f64) -> f32 {
        let t = now - self.since;
        match self.step {
            Step::Appearing | Step::Waiting | Step::Live | Step::Gone => 0.0,
            Step::Leaving if self.closing_part() <= 0.0 => 0.0,
            Step::Leaving => self.from * (1.0 - ease_in_out((t / self.closing_part()) as f32)),
            Step::Opening => self.from + (1.0 - self.from) * ease_in_out((t / self.opening_secs()) as f32),
            Step::Wandering => 1.0,
            Step::Closing => 1.0 - ease_in_out((t / STATION_CLOSE_SECS) as f32),
        }
    }
}

/// Eased at both ends, 0 to 1.
fn ease_in_out(t: f32) -> f32 {
    0.5 - 0.5 * (t.clamp(0.0, 1.0) * std::f32::consts::PI).cos()
}

fn lerp_colour(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color32::from_rgba_unmultiplied(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()), mix(a.a(), b.a()))
}

/// How big the dot is before „live", and the air between them.
const STATION_DOT: f32 = 7.0;
/// How long a station's rule is: the dot and its little line, the gap and
/// „live" — what the compact line keeps for it.
pub(super) fn station_rule_width(ui: &Ui, palette: &Palette) -> f32 {
    STATION_SPAN + STATION_LIVE_GAP + text::line(ui, tr!("live"), Role::Label, palette.mid).size().x
}

const STATION_LIVE_GAP: f32 = 8.0;
/// How wide the little rule the dot draws out to is: the volume's width.
const STATION_SPAN: f32 = VOLUME_WIDTH;
/// How thick the little rule is: about half the dot.
const STATION_TUNE_THICK: f32 = 3.5;
/// The movements' times: the ball coming up, pushing out to the rule, one turn
/// of the travelling piece (and the least it wanders), drawing in, the moment
/// still, and „live" fading in.
const STATION_APPEAR_SECS: f64 = 0.5;
const STATION_OPEN_SECS: f64 = 0.9;
const STATION_TUNE_SECS: f64 = 2.0;
const STATION_CLOSE_SECS: f64 = 1.2;
const STATION_WAIT_SECS: f64 = 0.5;
const STATION_LIVE_SECS: f32 = 0.8;
/// With a square-ended face, how quickly the dot squares off when a new
/// station draws it out again.
const STATION_SQUARE_SECS: f64 = 0.3;
/// Paused, how long the ball takes to shrink away once the rule is in.
const STATION_GO_SECS: f64 = 0.5;
/// How much of the little rule the travelling piece takes.
const STATION_PIECE: f32 = 0.3;
/// The four things you do to a record while it is playing, set as type.
///
/// Set, not drawn. This window has no pictograms anywhere else, and these are
/// letterforms out of the typeface it is already set in: two arrows point back
/// and forward, a circular pair comes back round to where it started, and two
/// more pass each other going opposite ways. Repeat before shuffle: the round
/// mark between the straight ones, rather than two flat pairs side by side.
///
/// All four are arrows, and that was measured rather than felt: with `«` and
/// `»` in the first two seats the row held two sizes of mark, 11 by 12 screen
/// dots against 20 by 21 for the arrows beside them — punctuation dropped
/// between pictures. `←` and `→` are drawn by the same hand at the same weight
/// as `⇵` and `↻`, and the four read as one set.
///
/// They come from the proportional cut: marks like `≈` and `∞` from a mono
/// cut without circular arrows had to be *read into* rather than recognised.
///
/// The mono cut had one argument in its favour and it has since expired: two
/// of these glyphs change with the state behind them, and a proportional face
/// would have re-spaced the row under the pointer. Since the row became
/// buttons on a fixed grid, nothing can move — a glyph sits in the middle of
/// its own button and the button does not care how wide it is.
pub(super) const RING: [&str; 4] = ["←", "→", "↻", "⇵"];

/// The one glyph that says which of three states it is in, rather than only
/// whether it is on: the whole queue goes round, one track is „①" — the same
/// circle, with the number inside it.
pub(super) const REPEAT_ONE: &str = "①";

/// How wide one key is, and how far apart two stand. The keys are the transport
/// in the corner.
///
/// Round ones were tried, and then not kept: a row of circles reads as a
/// string of beads rather than as a row of keys. The rounded rectangle is the
/// window's own plate, the one the drag badge wears.
pub(super) const RING_CELL: f32 = KEY_SIZE;

pub(super) const RING_STEP: f32 = KEY_SIZE + 4.0;

/// One key of the transport, set the way every other control in the header
/// is: a mark on the ground, and a plate behind it only while the pointer is
/// on it — the same faint plate `vol`, `liste` and `menü` wear.
///
/// `letters` is the caller's: see [`key_colour`].
pub(super) fn paint_key(
    ui: &Ui,
    plate: Rect,
    label: &str,
    role: Role,
    letters: Color32,
    hovered: bool,
    palette: &Palette,
) {
    if hovered {
        ui.painter().rect_filled(plate, crate::gui::theme::ROW_RADIUS, palette.hover);
    }
    let letters = if hovered && letters != palette.accent { palette.ink } else { letters };

    if role != Role::Label {
        if let Some(mark) = KeyMark::of(label) {
            let colour = letters;
            if !crate::gui::glyphs::paint(ui, mark.glyph(), plate.center(), colour) {
                paint_mark(ui, mark, plate.center(), colour);
            }
            return;
        }
    }
    let g = text::line(ui, label, role, letters);
    let mut at = text::ink_centred(&g, plate);
    if role == Role::Label {
        at.y = plate.center().y - text::ink_middle(ui, role, 0.0);
    }
    ui.painter().galley(at, g, letters);
}

/// The five marks the transport keys carry.
///
/// Out of the chosen face, read by glyph name and set a step above the label
/// size — see `glyphs`. As plain characters they came out at the size of the
/// lists, a third heavier than the words beside them, and the one repeat
/// worth having is a drawing no character reaches. Where the face has not
/// got them, they are drawn here instead: every stroke one width, the width
/// of the label face's stems.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum KeyMark {
    Back,
    On,
    Shuffle,
    Repeat,
    RepeatOne,
}

impl KeyMark {
    /// The same mark, as the face draws it.
    pub(super) fn glyph(self) -> crate::gui::glyphs::Mark {
        use crate::gui::glyphs::Mark;
        match self {
            Self::Back => Mark::Back,
            Self::On => Mark::On,
            Self::Shuffle => Mark::Shuffle,
            Self::Repeat => Mark::Repeat,
            Self::RepeatOne => Mark::RepeatOne,
        }
    }

    pub(super) fn of(label: &str) -> Option<Self> {
        match label {
            "←" => Some(Self::Back),
            "→" => Some(Self::On),
            "⇵" => Some(Self::Shuffle),
            "↻" => Some(Self::Repeat),
            "①" => Some(Self::RepeatOne),
            _ => None,
        }
    }
}

/// How thick a drawn mark's strokes are: the label face's stem.
pub(super) const MARK_STROKE: f32 = 1.1;

/// Draw one of the transport's marks, centred on `at`.
pub(super) fn paint_mark(ui: &Ui, mark: KeyMark, at: egui::Pos2, colour: Color32) {
    let painter = ui.painter();
    let stroke = egui::Stroke::new(MARK_STROKE, colour);
    let p = |x: f32, y: f32| egui::pos2(at.x + x, at.y + y);
    let arrow = |from: egui::Pos2, to: egui::Pos2| {
        painter.line_segment([from, to], stroke);
        let along = (to - from).normalized();
        let across = egui::vec2(-along.y, along.x);
        const HEAD: f32 = 2.9;
        painter.line_segment([to, to - along * HEAD + across * HEAD], stroke);
        painter.line_segment([to, to - along * HEAD - across * HEAD], stroke);
    };
    match mark {
        KeyMark::Back => arrow(p(4.5, 0.0), p(-4.5, 0.0)),
        KeyMark::On => arrow(p(-4.5, 0.0), p(4.5, 0.0)),
        KeyMark::Shuffle => {
            arrow(p(-3.2, -4.5), p(-3.2, 4.5));
            arrow(p(3.2, 4.5), p(3.2, -4.5));
        }
        KeyMark::Repeat | KeyMark::RepeatOne => {
            const R: f32 = 4.6;
            let start = -std::f32::consts::FRAC_PI_2 + 0.55;
            let end = start + std::f32::consts::TAU - 1.1;
            let steps = 28;
            let points: Vec<egui::Pos2> = (0..=steps)
                .map(|i| {
                    let a = start + (end - start) * i as f32 / steps as f32;
                    p(R * a.cos(), R * a.sin())
                })
                .collect();
            let tip = *points.last().unwrap_or(&at);
            painter.add(egui::Shape::line(points, stroke));
            let along = egui::vec2(-end.sin(), end.cos());
            let across = egui::vec2(-along.y, along.x);
            const HEAD: f32 = 2.8;
            painter.line_segment([tip, tip - along * HEAD + across * HEAD], stroke);
            painter.line_segment([tip, tip - along * HEAD - across * HEAD], stroke);
            if mark == KeyMark::RepeatOne {
                let g = text::line(ui, "1", Role::Label, colour);
                let at = text::ink_centred(&g, Rect::from_center_size(at, Vec2::splat(R * 2.0)));
                painter.galley(at, g, colour);
            }
        }
    }
}

/// The two words the play key can say. It says what pressing it will do, as
/// every player's key does: „pause" while the music plays, „play" while it is
/// held or has not started yet. Both in the ink: „pause" lit in the accent
/// while „play" was not would read as two different kinds of key; what is
/// playing is shown by the track in the accent, and by the moving timeline.
///
/// It used to say what the player *was* doing, the way the repeat key's glyph
/// does, and read as the wrong word: a key marked „play" while the music plays
/// looks like a key that has not been pressed.
pub(in crate::gui) const PLAY_WORDS: [&str; 2] = ["play", "pause"];

/// What the play key says for `state`, and whether it is lit.
pub(super) fn play_word(state: crate::player::PlaybackState) -> (&'static str, bool) {
    match state {
        crate::player::PlaybackState::Playing => (PLAY_WORDS[1], false),
        _ => (PLAY_WORDS[0], false),
    }
}

/// How far play, standing alone in →'s place while a station plays, moves on
/// past the cell's edge: its word in the middle of a cell made for the wider of
/// its two words stood that much further from „vol" than the marks of the other
/// keys do. Moved, the wider word's edge stands where a mark's would, one
/// control's gap from „vol". Measured on the wider word, not the one shown:
/// measured on the one shown, the key jumped sideways each time play turned
/// into pause.
pub(super) fn on_air_nudge(ui: &Ui, palette: &Palette) -> f32 {
    let widest = PLAY_WORDS
        .iter()
        .map(|w| text::line(ui, w, Role::Label, palette.ink).size().x)
        .fold(0.0, f32::max);
    ((play_key_width(ui, palette) - widest) / 2.0 - KEY_AIR).max(0.0)
}

/// The air either side of the word on the play key.
pub(super) const PLAY_PAD: f32 = 9.0;

/// The air between on and repeat: repeat and shuffle are a group of their
/// own, as far from the transport as one control stands from the next —
/// [`CONTROL_GAP`] from mark to mark.
pub(super) const MODES_APART: f32 = CONTROL_GAP - 2.0 * KEY_AIR;

/// How wide the play key is: the wider of its two words, plus its air, and
/// never narrower than the other keys. Fixed, so the arrows either side do not
/// move when it changes its word.
pub(super) fn play_key_width(ui: &Ui, palette: &Palette) -> f32 {
    let widest = PLAY_WORDS
        .iter()
        .map(|w| text::line(ui, w, Role::Label, palette.ink).size().x)
        .fold(0.0, f32::max);
    (widest + PLAY_PAD * 2.0).max(RING_CELL)
}

/// Where the five keys stand, left to right: back, play, on, repeat, shuffle.
/// The row ends at `right`, the corner of the header. Two groups: the
/// transport, then the modes, further off.
///
/// Lifted out so the arithmetic can be checked without a window.
pub(super) fn key_cells(right: f32, bottom: f32, play: f32) -> [Rect; 5] {
    let widths = [RING_CELL, play, RING_CELL, RING_CELL, RING_CELL];
    let gap = RING_STEP - RING_CELL;
    let after = [gap, gap, MODES_APART, gap];
    let mut cells = [Rect::NOTHING; 5];
    let mut x = right;
    for (index, width) in widths.iter().enumerate().rev() {
        if index < after.len() {
            x -= after[index];
        }
        cells[index] = Rect::from_min_max(
            egui::pos2(x - width, bottom - RING_CELL),
            egui::pos2(x, bottom),
        );
        x -= width;
    }
    cells
}

/// How much of the header's line the keys take, from the right margin — all
/// five, or the three of the transport when `modes` (repeat and shuffle)
/// stand over the queue instead.
pub(super) fn keys_width(ui: &Ui, palette: &Palette, modes: bool) -> f32 {
    let cells = header_cells(0.0, 0.0, play_key_width(ui, palette), modes);
    -cells[0].left()
}

/// The keys' cells in the header, ending at `right`: all five, or — without
/// the modes — the first three moved along to end there themselves.
pub(super) fn header_cells(right: f32, bottom: f32, play: f32, modes: bool) -> Vec<Rect> {
    let cells = key_cells(right, bottom, play);
    if modes {
        cells.to_vec()
    } else {
        let by = right - cells[2].right();
        cells[..3].iter().map(|cell| cell.translate(egui::vec2(by, 0.0))).collect()
    }
}

/// What a key is, for its colour.
///
/// under the pointer in the ink, on its plate; on — playing, lit, chosen — in
/// the accent. One colour for each state, wherever the control is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Key {
    /// Back and on.
    Plain,
    /// The play key, on while the music plays.
    Play(bool),
    /// Repeat and shuffle, on while lit.
    Switch(bool),
}

/// The colour of a key's letters at rest — see [`Key`]; [`paint_key`]
/// turns them to the ink under the pointer.
pub(super) fn key_colour(key: Key, palette: &Palette) -> Color32 {
    match key {
        Key::Play(true) | Key::Switch(true) => palette.accent,
        Key::Plain | Key::Play(false) | Key::Switch(false) => palette.mid,
    }
}

/// Repeat and shuffle as keys: what each shows, and whether it is lit.
pub(super) fn mode_keys(app: &GuiApp) -> [(&'static str, Role, bool); 2] {
    use crate::db::models::RepeatMode;
    [
        match app.player.repeat_mode() {
            RepeatMode::Off => (RING[2], Role::Content, false),
            RepeatMode::All => (RING[2], Role::Content, true),
            RepeatMode::One => (REPEAT_ONE, Role::Content, true),
        },
        (RING[3], Role::Content, app.player.is_shuffle()),
    ]
}

/// Repeat and shuffle at the top of the queue's column, the way Apple Music
/// keeps them over „als nächstes": two keys in a row starting at `left`,
/// their middle on `middle`. Returns where they end.
pub(super) fn queue_mode_keys(app: &mut GuiApp, ui: &mut Ui, left: f32, middle: f32, palette: &Palette) -> f32 {
    let mut x = left;
    for (index, (label, role, on)) in mode_keys(app).into_iter().enumerate() {
        let plate = Rect::from_min_size(
            egui::pos2(x, middle - RING_CELL / 2.0),
            Vec2::splat(RING_CELL),
        );
        let response = ui.interact(plate, egui::Id::unique(("queue-mode", index)), Sense::click());
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        paint_key(ui, plate, label, role, key_colour(Key::Switch(on), palette), response.hovered(), palette);
        if response.clicked() {
            if index == 0 {
                app.player.toggle_repeat();
            } else {
                app.player.toggle_shuffle();
            }
        }
        x = plate.right() + (RING_STEP - RING_CELL);
    }
    x - (RING_STEP - RING_CELL)
}

/// How wide the two keys over the queue are.
pub(super) const QUEUE_MODES_WIDTH: f32 = RING_CELL * 2.0 + (RING_STEP - RING_CELL);

/// The transport, standing in the corner all the time: back, play, on,
/// repeat, shuffle — the play key between the two arrows.
pub(super) fn transport_keys(
    app: &mut GuiApp,
    ui: &mut Ui,
    corner: egui::Pos2,
    palette: &Palette,
    modes: bool,
) {
    let cells = header_cells(corner.x, corner.y, play_key_width(ui, palette), modes);
    let state = app.player.state();
    let [repeat, shuffle] = mode_keys(app);
    let switch = |(label, role, on): (&'static str, Role, bool)| (label.to_string(), role, Key::Switch(on));
    let (back, on) = if app.playing_episode() {
        (
            (format!("−{}", app.config.ui.podcast_back), Role::FiguresLabel),
            (format!("+{}", app.config.ui.podcast_ahead), Role::FiguresLabel),
        )
    } else {
        ((RING[0].to_string(), Role::Content), (RING[1].to_string(), Role::Content))
    };
    let keys: [(String, Role, Key); 5] = [
        (back.0, back.1, Key::Plain),
        {
            let (word, lit) = play_word(state);
            (word.to_string(), Role::Label, Key::Play(lit))
        },
        (on.0, on.1, Key::Plain),
        switch(repeat),
        switch(shuffle),
    ];

    let on_air = app.on_air.is_some();
    let on_by = cells[2].right() - cells[1].right() + on_air_nudge(ui, palette);
    let mut pressed = None;
    for (index, ((label, role, key), plate)) in keys.into_iter().zip(cells).enumerate() {
        if on_air && (index == 0 || index == 2) {
            continue;
        }
        let plate = if on_air && index == 1 { plate.translate(egui::vec2(on_by, 0.0)) } else { plate };
        let response = ui.interact(plate, egui::Id::unique(("key", index)), Sense::click());
        if response.clicked() {
            pressed = Some(index);
        }
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        paint_key(ui, plate, &label, role, key_colour(key, palette), response.hovered(), palette);
    }

    let outcome = match pressed {
        Some(0) => app.skip(false),
        Some(1) => {
            app.toggle_pause();
            Ok(())
        }
        Some(2) => app.skip(true),
        Some(3) => {
            app.player.toggle_repeat();
            Ok(())
        }
        Some(4) => {
            app.player.toggle_shuffle();
            Ok(())
        }
        _ => Ok(()),
    };
    if let Err(e) = outcome {
        app.set_status(format!("{e}"));
    }
}

/// A piece of the status line: set in one face and cut to its room, or —
/// at the foot of the compact window — with its figures in the mono cut.
pub(super) enum StatusWords {
    Plain(std::sync::Arc<egui::Galley>),
    Figured(text::Figured),
}

impl StatusWords {
    pub(super) fn width(&self) -> f32 {
        match self {
            Self::Plain(g) => g.size().x,
            Self::Figured(f) => f.width,
        }
    }

    pub(super) fn paint(self, painter: &egui::Painter, at: egui::Pos2, colour: Color32) {
        match self {
            Self::Plain(g) => painter.galley(at, g, colour),
            Self::Figured(f) => f.paint(painter, at, colour),
        }
    }
}

/// What stands between who speaks in the status line and what they say.
const STATUS_APART: &str = "   ";

/// `words` for the status line, in `colour`, in at most `room`. With
/// `figures`, its figures are in the mono cut — unless that is too long for
/// the room, when it is cut in the one face like everywhere else: a figured
/// line has no way to end in an ellipsis.
pub(super) fn status_words(ui: &Ui, words: &str, colour: Color32, room: f32, figures: bool) -> StatusWords {
    let spaced = words.replace("  ", STATUS_APART);
    let words = spaced.as_str();
    if figures {
        let figured = text::Figured::new(ui, words, Role::Label, Role::FiguresLabel, colour);
        if figured.width <= room {
            return StatusWords::Figured(figured);
        }
    }
    StatusWords::Plain(text::line_within(ui, words, Role::Label, colour, room.min(1.0e6)))
}
