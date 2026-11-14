use super::*;

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

pub(super) fn volume_width(ui: &Ui, palette: &Palette) -> f32 {
    text::line(ui, tr!("vol"), Role::Label, palette.mid).size().x
}

pub(super) const CONTROL_GAP: f32 = 28.0;

pub(super) const CONTROL_PAD_X: f32 = 8.0;

pub(super) const CONTROL_PAD_Y: f32 = 8.0;

pub(in crate::gui) fn control_plate(ui: &Ui, at: egui::Pos2, width: f32) -> Rect {
    plate_around(
        text::ink_middle(ui, Role::Label, at.y),
        text::ink_height(ui, Role::Label),
        at.x,
        width,
    )
}

pub(super) fn plate_around(middle: f32, ink: f32, left: f32, width: f32) -> Rect {
    let half = ink / 2.0 + CONTROL_PAD_Y;
    Rect::from_min_max(
        egui::pos2(left - CONTROL_PAD_X, middle - half),
        egui::pos2(left + width + CONTROL_PAD_X, middle + half),
    )
}

pub(in crate::gui) fn paint_control_hover(
    ui: &Ui,
    plate: Rect,
    response: &Response,
    palette: &Palette,
) {
    if response.hovered() {
        ui.painter().rect_filled(plate, MARKER_RADIUS, palette.hover);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
}

pub(super) fn hover_ink(response: &Response, colour: Color32, palette: &Palette) -> Color32 {
    if response.hovered() && colour != palette.accent {
        palette.ink
    } else {
        colour
    }
}

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

    let hot = response.hovered() || response.dragged();
    let (height, knob) = rule_swell(swell(ui, "volume", hot));
    let rule = Rect::from_min_size(
        egui::pos2(strip.left(), strip.center().y - height / 2.0),
        Vec2::new(WIDTH, height),
    );
    ui.painter().rect_filled(rule, capped(height), palette.rule);

    let filled = WIDTH * app.player.volume() as f32 / 100.0;
    if filled > 0.0 {
        ui.painter().rect_filled(
            Rect::from_min_size(rule.min, Vec2::new(filled, height)),
            capped(height),
            palette.ink,
        );
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

    *right -= VOLUME_GAP;
    in_the_word
}

pub(super) const KEY_AIR: f32 = 5.0;

pub(super) const VOLUME_GAP: f32 = 10.0;

fn paint_chapters(ui: &Ui, rule: Rect, starts: &[f32], palette: &Palette, lit: Option<usize>, plated: bool) {
    if starts.len() < 2 {
        return;
    }
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

const MARK_WIDTH: f32 = 2.0;
const MARK_OUT: f32 = 2.5;

fn sharpest(palette: &Palette) -> Color32 {
    [palette.ink, palette.accent, palette.mid]
        .into_iter()
        .max_by(|a, b| crate::gui::theme::contrast(*a, palette.ground).total_cmp(&crate::gui::theme::contrast(*b, palette.ground)))
        .unwrap_or(palette.ink)
}

fn chapter_number(ui: &Ui, rule: Rect, x: f32, number: usize, plated: bool, palette: &Palette) {
    let said = number.to_string();
    if plated {
        let above = badge_fits_above(ui, rule.center().y, &said);
        badge_centred(ui, egui::pos2(x, rule.center().y), &said, above, palette);
        return;
    }
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

pub(super) fn progress_rule(
    app: &mut GuiApp,
    ui: &mut Ui,
    palette: &Palette,
    m: Metrics,
    width: f32,
) {
    const GRAB_HEIGHT: f32 = 14.0;
    const TIME_GAP: f32 = 14.0;

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

    let (height, knob) = rule_swell(swell(ui, "timeline", hot));
    let top = strip.center().y - height / 2.0;
    let rule = Rect::from_min_size(
        egui::pos2(strip.left(), top),
        Vec2::new(strip.width(), height),
    );
    ui.painter().rect_filled(rule, capped(height), palette.rule);

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
        ui.painter().rect_filled(filled, capped(height), colour);
    }
    if has_track {
        let starts = app.playing_chapter_starts();
        let share = held.map(|f| f as f32).or_else(|| {
            response.hovered().then(|| response.hover_pos().map(|at| fraction_at(at.x) as f32)).flatten()
        });
        let under = share.filter(|_| starts.len() > 1).map(|s| starts.iter().rposition(|start| *start <= s).unwrap_or(0));
        point_at_chapter(ui.ctx(), ON_THE_RULE, under);
        let lit = under.or_else(|| pointed_chapter(ui.ctx(), IN_THE_LIST));
        let plated = PlayingAt::of(&app.config.ui) != PlayingAt::SuperCompact;
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

pub(super) fn station_settled(ctx: &egui::Context) -> bool {
    let now = ctx.input(|i| i.time);
    match ctx.data(|d| d.get_temp::<(bool, f64)>(egui::Id::unique("station-settled"))) {
        Some((settled, at)) if now - at < 0.5 => settled,
        _ => true,
    }
}

fn wake_in(ui: &Ui, secs: f64) {
    let early = f64::from(ui.input(|i| i.predicted_dt));
    ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(secs.max(0.01) + early));
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Step {
    #[default]
    Appearing,
    Opening,
    Wandering,
    Closing,
    Waiting,
    Live,
    Leaving,
    Gone,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct Show {
    step: Step,
    since: f64,
    from: f32,
    round: f32,
    station: String,
}

impl Show {
    fn opening_secs(&self) -> f64 {
        (STATION_OPEN_SECS * f64::from(1.0 - self.from)).max(0.001)
    }

    fn closing_part(&self) -> f64 {
        STATION_CLOSE_SECS * f64::from(self.from)
    }

    fn leaving_secs(&self) -> f64 {
        self.closing_part() + STATION_GO_SECS
    }

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

fn ease_in_out(t: f32) -> f32 {
    0.5 - 0.5 * (t.clamp(0.0, 1.0) * std::f32::consts::PI).cos()
}

fn lerp_colour(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color32::from_rgba_unmultiplied(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()), mix(a.a(), b.a()))
}

const STATION_DOT: f32 = 7.0;
pub(super) fn station_rule_width(ui: &Ui, palette: &Palette) -> f32 {
    STATION_SPAN + STATION_LIVE_GAP + text::line(ui, tr!("live"), Role::Label, palette.mid).size().x
}

const STATION_LIVE_GAP: f32 = 8.0;
const STATION_SPAN: f32 = VOLUME_WIDTH;
const STATION_TUNE_THICK: f32 = 3.5;
const STATION_APPEAR_SECS: f64 = 0.5;
const STATION_OPEN_SECS: f64 = 0.9;
const STATION_TUNE_SECS: f64 = 2.0;
const STATION_CLOSE_SECS: f64 = 1.2;
const STATION_WAIT_SECS: f64 = 0.5;
const STATION_LIVE_SECS: f32 = 0.8;
const STATION_SQUARE_SECS: f64 = 0.3;
const STATION_GO_SECS: f64 = 0.5;
const STATION_PIECE: f32 = 0.3;
pub(super) const RING: [&str; 4] = ["←", "→", "↻", "⇵"];

pub(super) const REPEAT_ONE: &str = "①";

pub(super) const RING_CELL: f32 = KEY_SIZE;

pub(super) const RING_STEP: f32 = KEY_SIZE + 4.0;

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
        ui.painter().rect_filled(plate, MARKER_RADIUS, palette.hover);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum KeyMark {
    Back,
    On,
    Shuffle,
    Repeat,
    RepeatOne,
}

impl KeyMark {
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

pub(super) const MARK_STROKE: f32 = 1.1;

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

pub(in crate::gui) const PLAY_WORDS: [&str; 2] = ["play", "pause"];

pub(super) fn play_word(state: crate::player::PlaybackState) -> (&'static str, bool) {
    match state {
        crate::player::PlaybackState::Playing => (PLAY_WORDS[1], false),
        _ => (PLAY_WORDS[0], false),
    }
}

pub(super) fn on_air_nudge(ui: &Ui, palette: &Palette) -> f32 {
    let widest = PLAY_WORDS
        .iter()
        .map(|w| text::line(ui, w, Role::Label, palette.ink).size().x)
        .fold(0.0, f32::max);
    ((play_key_width(ui, palette) - widest) / 2.0 - KEY_AIR).max(0.0)
}

pub(super) const PLAY_PAD: f32 = 9.0;

pub(super) const MODES_APART: f32 = CONTROL_GAP - 2.0 * KEY_AIR;

pub(super) fn play_key_width(ui: &Ui, palette: &Palette) -> f32 {
    let widest = PLAY_WORDS
        .iter()
        .map(|w| text::line(ui, w, Role::Label, palette.ink).size().x)
        .fold(0.0, f32::max);
    (widest + PLAY_PAD * 2.0).max(RING_CELL)
}

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

pub(super) fn keys_width(ui: &Ui, palette: &Palette, modes: bool) -> f32 {
    let cells = header_cells(0.0, 0.0, play_key_width(ui, palette), modes);
    -cells[0].left()
}

pub(super) fn header_cells(right: f32, bottom: f32, play: f32, modes: bool) -> Vec<Rect> {
    let cells = key_cells(right, bottom, play);
    if modes {
        cells.to_vec()
    } else {
        let by = right - cells[2].right();
        cells[..3].iter().map(|cell| cell.translate(egui::vec2(by, 0.0))).collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Key {
    Plain,
    Play(bool),
    Switch(bool),
}

pub(super) fn key_colour(key: Key, palette: &Palette) -> Color32 {
    match key {
        Key::Play(true) | Key::Switch(true) => palette.accent,
        Key::Plain | Key::Play(false) | Key::Switch(false) => palette.mid,
    }
}

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

pub(super) const QUEUE_MODES_WIDTH: f32 = RING_CELL * 2.0 + (RING_STEP - RING_CELL);

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

const STATUS_APART: &str = "   ";

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
