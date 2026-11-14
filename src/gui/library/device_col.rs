use super::*;

pub(super) const DEVICE_ART: f32 = 132.0;

pub(super) fn device_column(app: &mut GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette, width: f32) -> Rect {
    app.refresh_device_summary();
    let (Some(device), Some(summary)) = (app.attached.as_ref(), app.device_summary.clone()) else {
        return ui.allocate_space(Vec2::new(width, 0.0)).1;
    };
    let name = crate::gui::theme::lower(&device.name);
    let model = device.model.clone();
    let capacity = device.capacity;
    let playlists = device.playlists;
    let refusal = device.refusal.clone();
    let generation = device.generation;

    let mut close = false;
    let mut transfer = false;
    let mut eject = false;
    let mut report = false;
    let taken = ui.allocate_ui(Vec2::new(width, ui.available_height()), |ui| {
        ui.vertical(|ui| {
            let titled = match name.trim() {
                "" | "ipod" => "ipod".to_string(),
                own => format!("ipod · {own}"),
            };
            if heading(ui, &titled, "", None, Some(tr!("schliessen")), m, palette).clicked() {
                close = true;
            }
            ui.add_space(HEAD_AIR);
            scrolled(ui, "device", palette, |ui| {
                let (area, _) =
                    ui.allocate_exact_size(Vec2::new(width, DEVICE_ART + 8.0), Sense::hover());
                let art = Rect::from_center_size(
                    egui::pos2(content_rect(area, m).center().x, area.center().y),
                    Vec2::splat(DEVICE_ART),
                );
                device_art(app, ui, art, palette);
                ui.add_space(BLOCK_AIR);

                let fact = |ui: &mut Ui, label: &str, value: &str| {
                    device_fact(ui, width, label, value, palette.ink, m, palette);
                };
                if refusal.is_none() {
                    name_row(app, ui, width, &name, m, palette);
                } else {
                    fact(ui, tr!("name"), &name);
                }
                fact(ui, tr!("modell"), &model);
                if let Some(serial) = &summary.serial {
                    fact(ui, tr!("seriennummer"), &crate::gui::theme::lower(serial));
                }
                ui.add_space(BLOCK_AIR);
                fact(ui, tr!("titel"), &text::count(summary.tracks as u64));
                fact(ui, tr!("alben"), &text::count(summary.albums as u64));
                fact(ui, tr!("künstler:innen"), &text::count(summary.artists as u64));
                if playlists > 0 && generation.has_playlists() {
                    fact(ui, "playlists", &text::count(playlists as u64));
                }
                fact(ui, tr!("spielzeit"), &crate::gui::device::spoken_length(summary.seconds));
                if !summary.formats.is_empty() {
                    fact(ui, tr!("formate"), &crate::gui::device::formats_line(&summary.formats));
                }
                if let Some(day) = &summary.written {
                    let today = chrono::Local::now().date_naive();
                    fact(ui, tr!("zuletzt beschrieben"), &text::day(day, today));
                }

                if let Some((total, free)) = capacity {
                    ui.add_space(BLOCK_AIR);
                    fact(
                        ui,
                        tr!("platz"),
                        &tr!("{} von {} frei",
                            crate::ipod::format_bytes(free),
                            crate::ipod::format_bytes(total)
                        ),
                    );
                    device_bar(ui, width, total, free, summary.music_bytes, m, palette);
                }

                ui.add_space(BLOCK_AIR);
                if app.transferring() {
                    ui.add_space(BLOCK_AIR * 0.5);
                    transfer_form(app, ui, m, palette);
                    return;
                }
                if app.removal.is_some() {
                    ui.add_space(BLOCK_AIR * 0.5);
                    removal_form(app, ui, m, palette, width);
                    return;
                }
                let missing = if summary.missing == 0 {
                    tr!("nichts").to_string()
                } else {
                    tr!("{} titel · {}",
                        text::count(summary.missing as u64),
                        crate::ipod::format_bytes(summary.missing_bytes)
                    )
                };
                fact(ui, tr!("noch nicht drauf"), &missing);
                if generation.has_playlists() {
                    let newest = if summary.newest_current {
                        tr!("aktuell")
                    } else {
                        tr!("nicht aktuell")
                    };
                    fact(ui, tr!("zuletzt hinzugefügt"), newest);
                }
                let tagged = summary.retag - summary.sound_only;
                if tagged > 0 {
                    fact(ui, "idtags", &tr!("{} geändert", text::count(tagged as u64)));
                }
                if summary.reorder {
                    fact(ui, tr!("menüs"), tr!("neu ordnen"));
                }
                if summary.respell > 0 {
                    fact(ui, tr!("schreibweisen"), &tr!("{} angleichen", text::count(summary.respell as u64)));
                }
                if summary.sound_only > 0 {
                    fact(ui, "sound check", &tr!("{} neu", text::count(summary.sound_only as u64)));
                }
                if summary.old_lists > 0 {
                    fact(
                        ui,
                        tr!("alte playlists"),
                        &tr!("{} · kommen weg", text::count(summary.old_lists as u64)),
                    );
                }
                if summary.unplayable > 0 {
                    let said = format!(
                        "{} ({})",
                        text::count(summary.unplayable as u64),
                        summary.unplayable_formats.join(", ")
                    );
                    let label = if app.converts_for_ipod() {
                        tr!("wird umgewandelt")
                    } else {
                        tr!("spielt es nicht")
                    };
                    device_fact(ui, width, label, &said, palette.mid, m, palette);
                }
                ui.add_space(6.0);
                if let Some(why) = &refusal {
                    say(ui, width, why, Role::Label, palette.faint, m);
                } else if summary.missing > 0
                    || !summary.newest_current
                    || summary.retag > 0
                    || summary.respell > 0
                    || summary.reorder
                    || summary.old_lists > 0
                {
                    transfer = device_action(ui, width, tr!("synchronisieren…"), m, palette);
                }
                if ui.input(|i| i.modifiers.alt)
                    && device_action(ui, width, tr!("bericht speichern"), m, palette)
                {
                    report = true;
                }
                if device_action(ui, width, tr!("auswerfen"), m, palette) {
                    eject = true;
                }
            });
        });
    });

    if close {
        app.close_device_panel();
        if app.showing_ipod() {
            if let Err(e) = app.use_device(false) {
                app.set_status(format!("{e}"));
            }
        }
    }
    if transfer {
        if let Err(e) = app.transfer_library() {
            app.set_status(format!("{e}"));
        }
    }
    if report {
        app.save_device_report();
    }
    if eject {
        match app.eject_device() {
            Ok(name) => app.set_status(tr!("{} kann abgezogen werden", name)),
            Err(e) => app.set_status(format!("{e}")),
        }
    }
    taken.response.rect
}

pub(super) fn device_fact(
    ui: &mut Ui,
    width: f32,
    label: &str,
    value: &str,
    colour: Color32,
    m: Metrics,
    palette: &Palette,
) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, m.row), Sense::hover());
    let content = content_rect(rect, m);
    let line = text::row_line(ui, m.row, Role::Content);
    let name = text::line(ui, label, Role::Content, palette.mid);
    let name_width = name.size().x;
    ui.painter().galley(
        egui::pos2(content.left(), text::on_line(&name, rect.top(), line)),
        name,
        palette.mid,
    );
    let room = (content.width() - name_width - 14.0).max(0.0);
    let figure = text::line_within(ui, value, Role::Content, colour, room);
    ui.painter().galley(
        egui::pos2(
            content.right() - figure.size().x,
            text::on_line(&figure, rect.top(), line),
        ),
        figure,
        colour,
    );
}

fn name_row(app: &mut GuiApp, ui: &mut Ui, width: f32, name: &str, m: Metrics, palette: &Palette) {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, m.row), Sense::click());
    let content = content_rect(rect, m);
    let line = text::row_line(ui, m.row, Role::Content);
    let typing = app.naming_device.as_ref();
    if typing.is_some() {
        ui.painter().rect_filled(marker_rect(rect, m), MARKER_RADIUS, palette.marked());
    } else {
        paint_hover(ui, rect, &response, RowState::default(), m, palette);
    }
    if response.hovered() {
        let icon = if typing.is_some() { egui::CursorIcon::Text } else { egui::CursorIcon::PointingHand };
        ui.ctx().set_cursor_icon(icon);
    }
    let label = text::line(ui, tr!("name"), Role::Content, palette.mid);
    let label_width = label.size().x;
    ui.painter().galley(egui::pos2(content.left(), text::on_line(&label, rect.top(), line)), label, palette.mid);
    let room = (content.width() - label_width - 14.0).max(0.0);
    match typing {
        None => {
            let figure = text::line_within(ui, name, Role::Content, palette.ink, room);
            ui.painter().galley(
                egui::pos2(content.right() - figure.size().x, text::on_line(&figure, rect.top(), line)),
                figure,
                palette.ink,
            );
            if response.clicked() {
                app.start_naming_device();
            }
        }
        Some(entry) => {
            let figure = text::cased_within(ui, entry.text(), Role::Content, palette.ink, room);
            let right = content.right();
            let at = egui::pos2(right - figure.size().x, text::on_line(&figure, rect.top(), line));
            if entry.is_marked() {
                let behind = Rect::from_min_size(
                    egui::pos2(at.x - 2.0, rect.top() + 3.0),
                    Vec2::new(figure.size().x + 4.0, m.row - 6.0),
                );
                ui.painter().rect_filled(behind, 2.0, palette.hover);
            } else if ui.input(|i| i.time).rem_euclid(1.06) < 0.62 {
                let after = &entry.text()[entry.caret()..];
                let run = text::cased_within(ui, after, Role::Content, palette.ink, f32::INFINITY).size().x;
                let bar = Rect::from_min_size(
                    egui::pos2(right - run + 0.5, rect.top() + 5.0),
                    Vec2::new(1.5, m.row - 10.0),
                );
                ui.painter().rect_filled(bar, 0.0, palette.ink);
            }
            ui.painter().galley(at, figure, palette.ink);
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(120));
            say(ui, width, tr!("enter speichert · esc lässt es"), Role::Label, palette.faint, m);
        }
    }
}

pub(super) fn device_bar(
    ui: &mut Ui,
    width: f32,
    total: u64,
    free: u64,
    music: u64,
    m: Metrics,
    palette: &Palette,
) {
    const HEIGHT: f32 = 4.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 14.0), Sense::hover());
    let content = content_rect(rect, m);
    let bar = Rect::from_center_size(
        egui::pos2(content.center().x, rect.center().y),
        Vec2::new(content.width(), HEIGHT),
    );
    ui.painter().rect_filled(bar, capped(HEIGHT), palette.rule);
    let [music_share, other_share, _] = crate::gui::device::shares(total, free, music);
    let used = Rect::from_min_size(
        bar.min,
        Vec2::new(bar.width() * (music_share + other_share), HEIGHT),
    );
    if used.width() > 0.5 {
        ui.painter().rect_filled(used, capped(HEIGHT), palette.mid);
    }
    let played = Rect::from_min_size(bar.min, Vec2::new(bar.width() * music_share, HEIGHT));
    if played.width() > 0.5 {
        ui.painter().rect_filled(played, capped(HEIGHT), palette.ink);
    }

    let used_bytes = total.saturating_sub(free);
    let said = tr!("musik {}   sonstiges {}",
        crate::ipod::format_bytes(music.min(used_bytes)),
        crate::ipod::format_bytes(used_bytes.saturating_sub(music)),
    );
    say(ui, width, &said, Role::Label, palette.faint, m);
}

pub(super) fn device_action(ui: &mut Ui, width: f32, word: &str, m: Metrics, palette: &Palette) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, m.row), Sense::click());
    let state = RowState::default();
    paint_hover(ui, rect, &response, state, m, palette);
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let colour = if response.hovered() { palette.ink } else { palette.mid };
    let g = text::line(ui, word, Role::Content, colour);
    ui.painter().galley(
        egui::pos2(
            content_rect(rect, m).left(),
            text::on_line(&g, rect.top(), text::row_line(ui, m.row, Role::Content)),
        ),
        g,
        colour,
    );
    response.clicked()
}

pub(super) fn link_action(ui: &mut Ui, width: f32, word: &str, m: Metrics, palette: &Palette) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, m.row), Sense::click());
    let state = RowState::default();
    paint_hover(ui, rect, &response, state, m, palette);
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let left = content_rect(rect, m).left();
    let line = text::row_line(ui, m.row, Role::Content);
    let colour = if response.hovered() { palette.ink } else { palette.mid };
    let arrow = text::line(ui, LINK_ARROW, Role::Content, colour);
    let after = left + arrow.size().x + LINK_ARROW_GAP;
    let room = (content_rect(rect, m).right() - after).max(20.0);
    let word = text::line(ui, &link_words(ui, word, room), Role::Content, colour);
    ui.painter().galley(egui::pos2(left, text::on_line(&arrow, rect.top(), line)), arrow, colour);
    ui.painter().galley(egui::pos2(after, text::on_line(&word, rect.top(), line)), word, colour);
    response.clicked()
}

pub(super) const LINK_ARROW: &str = "›";
pub(super) const LINK_ARROW_GAP: f32 = 3.0;
const LINK_MOST: usize = 32;

pub(super) fn link_words(ui: &Ui, word: &str, room: f32) -> String {
    let mut chars: Vec<char> = word.chars().collect();
    if chars.len() > LINK_MOST {
        chars.truncate(LINK_MOST - 1);
        chars.push('…');
    }
    let mut shown: String = chars.iter().collect();
    while chars.len() > 2 && text::line(ui, &shown, Role::Content, Color32::WHITE).size().x > room {
        chars.truncate(chars.len() - 2);
        chars.push('…');
        shown = chars.iter().collect();
    }
    shown
}

const SPIN_STEP: std::time::Duration = std::time::Duration::from_millis(40);

pub(super) fn device_art(app: &mut GuiApp, ui: &Ui, rect: Rect, palette: &Palette) {
    use crate::gui::device;
    let look = app.config.ui.cover_dither.then(|| crate::gui::dither::Look::of(&app.config.ui));
    let fineness = if look.is_some() { app.config.ui.picture_fineness() } else { 0.5 };
    let grain = crate::gui::dither::grain_for(rect.width(), ui.ctx().pixels_per_point(), fineness);
    let body = app.attached.as_ref().map(|a| device::Body::of(a.generation)).unwrap_or_default();

    let held = ui.interact(rect, egui::Id::unique("device-art-hand"), Sense::drag());
    if held.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        let moved = held.drag_delta();
        let across = std::f32::consts::PI / rect.width().max(1.0);
        app.device_handling.turn_by(moved.x * across, moved.y * across);
    } else if held.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
    let now = ui.input(|i| i.time);
    let writing = matches!(
        app.transfer.as_ref().map(|t| &t.stage),
        Some(crate::gui::Stage::Going { .. })
    );
    if writing && !held.dragged() {
        app.device_handling.spin(now);
        ui.ctx().request_repaint_after(SPIN_STEP);
    } else if !writing {
        app.device_handling.stop_spinning(now);
    }
    if held.drag_stopped() && !writing {
        app.device_handling.let_go(now);
    }
    if let Some(wait) = app.device_handling.settle(now) {
        ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(wait));
    }
    let pose = app.device_handling;
    let stood = pose.turn.stood();
    let seed = stood.iter().fold(0u32, |seed, v| seed.wrapping_mul(31).wrapping_add(*v as u32));

    let fresh = !matches!(
        &app.device_art,
        Some((at, (g, b, l), _)) if *at == stood && *g == grain && *b == body && *l == look
    );
    if fresh {
        let size = grain as usize;
        let (grey, covered) = device::picture(pose.turn, size, size, body);
        use crate::gui::dither::{self, Screen};
        let ink: Vec<u8> = match look {
            None => grey.iter().zip(&covered).map(|(g, inside)| if *inside { *g } else { 0 }).collect(),
            Some(look) if look.tones > 2 => {
                let levels = match look.screen {
                    Screen::Noise => dither::noise_levels(&grey, grain, grain, seed, look.tones),
                    method => dither::screen_levels(&grey, grain, grain, method, look.tones),
                };
                levels.into_iter().zip(covered).map(|(l, inside)| if inside { l } else { 0 }).collect()
            }
            Some(look) => {
                let dots = match look.screen {
                    Screen::Noise => dither::noise(&grey, grain, grain, seed),
                    method => dither::screen(&grey, grain, grain, method),
                };
                dots.into_iter().zip(covered).map(|(on, inside)| if on && inside { 255 } else { 0 }).collect()
            }
        };
        let tones = look.filter(|l| l.tones > 2).map(|l| {
            let mut tones = vec![Color32::TRANSPARENT];
            tones.extend(palette.tones_under(palette.ink, l.tones));
            tones.push(palette.ink);
            tones
        });
        let pixels: Vec<Color32> = match &tones {
            Some(tones) => ink.iter().map(|level| tones[usize::from(*level).min(tones.len() - 1)]).collect(),
            None => ink.iter().map(|a| Color32::from_white_alpha(*a)).collect(),
        };
        let image = egui::ColorImage::new([size, size], pixels);
        let options = if look.is_some() {
            egui::TextureOptions::NEAREST
        } else {
            egui::TextureOptions::LINEAR
        };
        let made = (grain, body, look);
        match app.device_art.take() {
            Some((_, _, mut handle)) => {
                handle.set(image, options);
                app.device_art = Some((stood, made, handle));
            }
            None => {
                let handle = ui.ctx().load_texture("device-art", image, options);
                app.device_art = Some((stood, made, handle));
            }
        }
    }
    if let Some((_, made, handle)) = &app.device_art {
        let tint = if made.2.is_some_and(|l| l.tones > 2) { Color32::WHITE } else { palette.ink };
        ui.painter().add(
            egui::epaint::RectShape::filled(rect, 0.0, tint).with_texture(
                handle.id(),
                Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            ),
        );
    }
}

pub(super) fn removal_form(app: &mut GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette, width: f32) {
    let Some(removal) = app.removal.as_ref() else { return };
    let going = removal.going.is_some();
    let (what, bytes) = (removal.what.clone(), removal.bytes);
    let fact = |ui: &mut Ui, label: &str, value: &str| {
        device_fact(ui, width, label, value, palette.ink, m, palette);
    };
    say(ui, width, tr!("vom ipod entfernen"), Role::Label, palette.faint, m);
    ui.add_space(4.0);
    fact(ui, tr!("was"), &what);
    fact(ui, tr!("wird frei"), &crate::ipod::format_bytes(bytes));
    ui.add_space(6.0);
    if going {
        say(ui, width, tr!("entferne…"), Role::Content, palette.accent, m);
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
        return;
    }
    paragraph(
        ui,
        width,
        tr!("die dateien werden vom ipod gelöscht, die datenbank vorher gesichert. in deiner bibliothek bleibt alles."),
        Role::Label,
        palette.faint,
        m,
    );
    ui.add_space(6.0);
    if device_action(ui, width, tr!("entfernen"), m, palette) {
        app.run_removal();
    }
    if device_action(ui, width, tr!("abbrechen"), m, palette) {
        app.removal = None;
    }
}
