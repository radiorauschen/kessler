use super::*;

pub(super) fn artist_column(app: &mut GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette, width: f32) -> Rect {
    app.follow_the_artist();
    let top = ui.cursor().min;
    let height = ui.available_height();
    let mut close = false;
    let mut look = false;
    let mut edit = false;
    let mut open_url: Option<String> = None;
    let mut go_to: Option<usize> = None;
    let mut copied: Option<String> = None;
    let mut visit: Option<String> = None;
    let mut back = false;
    let mut to_gaps = false;

    ui.allocate_ui(Vec2::new(width, height), |ui| {
        ui.set_width(width);
        ui.vertical(|ui| {
            let name = app.artist.name.clone();
            if heading(ui, tr!("informationen"), "", None, Some(tr!("schliessen")), m, palette).clicked() {
                close = true;
            }
            ui.add_space(HEAD_AIR);
            if let Some(before) = app.artist_trail.last() {
                if device_action(ui, width, &tr!("← {}", before), m, palette) {
                    back = true;
                }
            } else if app.gaps.came_from && device_action(ui, width, &tr!("← {}", tr!("andere künstler:innen")), m, palette) {
                to_gaps = true;
            }
            let Some(name) = name else {
                say(ui, width, tr!("niemand ausgewählt"), Role::Content, palette.faint, m);
                return;
            };
            paragraph(ui, width, &name, Role::Content, palette.ink, m);
            ui.add_space(BLOCK_AIR);
            if app.artist_form.is_some() {
                artist_form(app, ui, m, palette, width);
                return;
            }
            let key = app.artist.key;
            let looking = app.artist.looking_for(key);
            let info = app.artist.info.clone().unwrap_or_default();

            let shows = {
                let ui = &app.config.ui;
                (ui.info_picture, ui.info_facts, ui.info_wikipedia, ui.info_library, ui.info_ties, ui.info_similar)
            };
            let (show_picture, show_facts, show_wikipedia, show_library, show_ties, show_similar) = shows;
            scrolled(ui, "artist", palette, |ui| {
                if show_picture {
                    let inner = (width - m.marker_pad_x * 2.0).max(40.0);
                    let side = picture_side(inner, app.config.ui.info_picture_wide);
                    let (area, _) = ui.allocate_exact_size(Vec2::new(width, side), Sense::hover());
                    let rect = Rect::from_min_size(
                        egui::pos2(content_rect(area, m).left(), area.top()),
                        Vec2::splat(side),
                    );
                    artist_picture(app, ui, rect, palette, &name);
                    ui.add_space(BLOCK_AIR * 2.0);
                }

                let fact = |ui: &mut Ui, label: &str, value: &str| {
                    if !value.trim().is_empty() {
                        device_fact(ui, width, label, value, palette.ink, m, palette);
                    }
                };
                if show_facts {
                    fact(ui, tr!("art"), crate::lang::t(&info.found.kind));
                    fact(ui, tr!("herkunft"), info.origin());
                    fact(ui, tr!("aktiv"), info.active());
                    fact(ui, tr!("stil"), &info.style());
                }
                if show_facts && !info.found.disambiguation.is_empty() {
                    ui.add_space(BLOCK_AIR * 2.0);
                    say(ui, width, &tr!("aus {}", "musicbrainz"), Role::Label, palette.faint, m);
                    paragraph(ui, width, &info.found.disambiguation, Role::Content, palette.mid, m);
                }

                let trouble = app.artist.trouble.as_ref().filter(|(k, _)| *k == key);
                if looking {
                    ui.add_space(BLOCK_AIR);
                    let dots = ".".repeat(1 + (ui.input(|i| i.time) / 0.4) as usize % 3);
                    say(ui, width, &format!("{}{}", tr!("lade informationen"), dots), Role::Label, palette.faint, m);
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
                } else if let Some((_, why)) = trouble {
                    ui.add_space(BLOCK_AIR);
                    prose(ui, width, why, Role::Label, palette.faint, m);
                } else if !info.looked_up.is_empty() && !info.recognised && info.own.is_empty() {
                    ui.add_space(BLOCK_AIR);
                    prose(
                        ui,
                        width,
                        tr!("nichts gefunden — unter „bearbeiten“ lässt sich selbst eintragen, was man weiss"),
                        Role::Label,
                        palette.faint,
                        m,
                    );
                }

                let (about, from, other) = info.found.about_in(crate::lang::current().key());
                let waiting = looking && other.is_some();
                if show_wikipedia && !about.is_empty() && !waiting {
                    ui.add_space(BLOCK_AIR * 2.0);
                    if !from.is_empty() {
                        let said = match other {
                            Some(lang) => tr!("aus {} ({})", from, lang),
                            None => tr!("aus {}", from),
                        };
                        say(ui, width, &said, Role::Label, palette.faint, m);
                    }
                    prose(ui, width, about, Role::Content, palette.mid, m);
                }
                let on_radio = app.artist.name.clone().and_then(|name| app.heard_on_radio(&name));
                let radio_fact = |ui: &mut Ui, heard: &crate::radio::heard::Hearing| {
                    let today = chrono::Local::now().date_naive();
                    let when = text::day(&heard.day, today);
                    fact(ui, tr!("im radio"), &format!("{when} · {}", heard.station));
                };
                let on_radio = on_radio.filter(|_| show_library);
                let mine = app.artist.mine.clone().filter(|_| show_library);
                if mine.is_none() {
                    if let Some(heard) = &on_radio {
                        ui.add_space(BLOCK_AIR * 2.0);
                        say(ui, width, tr!("zuletzt gehört"), Role::Label, palette.faint, m);
                        radio_fact(ui, heard);
                    }
                }
                if let Some(mine) = mine {
                    let today = chrono::Local::now().date_naive();
                    ui.add_space(BLOCK_AIR * 2.0);
                    say(ui, width, tr!("in deiner bibliothek"), Role::Label, palette.faint, m);
                    fact(
                        ui,
                        tr!("alben"),
                        &text::count(mine.albums as u64),
                    );
                    fact(
                        ui,
                        tr!("titel"),
                        &format!(
                            "{} · {}",
                            text::count(mine.tracks as u64),
                            crate::gui::device::spoken_length(mine.seconds)
                        ),
                    );
                    if mine.plays > 0 {
                        fact(ui, tr!("gespielt"), &tr!("{} mal", text::count(mine.plays)));
                    }
                    fact(ui, tr!("seit"), &text::day(&mine.since, today));
                    if !mine.last_played.is_empty() {
                        fact(ui, tr!("zuletzt|gespielt"), &text::day(&mine.last_played, today));
                    }
                    if let Some(heard) = &on_radio {
                        radio_fact(ui, heard);
                    }
                }

                if !info.own.note.is_empty() {
                    ui.add_space(BLOCK_AIR * 2.0);
                    say(ui, width, tr!("notiz"), Role::Label, palette.faint, m);
                    prose(ui, width, &info.own.note, Role::Content, palette.ink, m);
                }

                let in_library = |name: &str| {
                    let plain = crate::library::artistinfo::plain(name);
                    app.artists
                        .iter()
                        .position(|a| crate::library::artistinfo::plain(&a.name) == plain)
                };
                for (heading, ties) in [
                    (tr!("mitglieder"), &info.found.members),
                    (tr!("in gruppen"), &info.found.groups),
                ] {
                    if !show_ties || ties.is_empty() {
                        continue;
                    }
                    ui.add_space(BLOCK_AIR * 2.0);
                    say(ui, width, heading, Role::Label, palette.faint, m);
                    for tie in ties.iter().take(ARTIST_TIES) {
                        match in_library(&tie.name) {
                            Some(at) => {
                                if device_action(ui, width, &tie.name, m, palette) {
                                    go_to = Some(at);
                                }
                            }
                            None => {
                                let colour = if tie.current { palette.ink } else { palette.mid };
                                match other_row(ui, width, &tie.name, colour, m, palette) {
                                    Some(Other::Visit) => visit = Some(tie.name.clone()),
                                    Some(Other::Copy) => copied = Some(tie.name.clone()),
                                    None => {}
                                }
                            }
                        }
                        let mut aside = tie.years.clone();
                        if !tie.also.is_empty() {
                            if !aside.is_empty() {
                                aside.push_str(" · ");
                            }
                            aside.push_str(&tr!("auch in {}", tie.also.join(", ")));
                        }
                        if !aside.is_empty() {
                            prose(ui, width, &aside, Role::Label, palette.faint, m);
                        }
                    }
                }

                if show_similar && !info.found.similar.is_empty() {
                    ui.add_space(BLOCK_AIR * 2.0);
                    say(ui, width, tr!("ähnliche künstler:innen"), Role::Label, palette.faint, m);
                    for other in info.found.similar.iter().take(ARTIST_SIMILAR) {
                        let plain = crate::library::artistinfo::plain(other);
                        let here = app
                            .artists
                            .iter()
                            .position(|a| crate::library::artistinfo::plain(&a.name) == plain);
                        match here {
                            Some(at) => {
                                if device_action(ui, width, other, m, palette) {
                                    go_to = Some(at);
                                }
                            }
                            None => match other_row(ui, width, other, palette.mid, m, palette) {
                                Some(Other::Visit) => visit = Some(other.clone()),
                                Some(Other::Copy) => copied = Some(other.clone()),
                                None => {}
                            },
                        }
                    }
                }

                let mut links = info.found.links.clone();
                if !info.own.link.trim().is_empty() {
                    links.insert(
                        0,
                        crate::library::artistinfo::Link {
                            label: tr!("eigener link").to_string(),
                            url: info.own.link.trim().to_string(),
                        },
                    );
                }
                let encoded = crate::library::coverart::encode(&name);
                let has = |label: &str, links: &[crate::library::artistinfo::Link]| {
                    links.iter().any(|l| l.label == label)
                };
                if !has("last.fm", &links) {
                    links.push(crate::library::artistinfo::Link {
                        label: "last.fm".into(),
                        url: format!("https://www.last.fm/music/{}", encoded.replace("%20", "+")),
                    });
                }
                if !has("bandcamp", &links) {
                    links.push(crate::library::artistinfo::Link {
                        label: tr!("bandcamp suchen").to_string(),
                        url: format!("https://bandcamp.com/search?q={encoded}&item_type=b"),
                    });
                }
                if !has("discogs", &links) {
                    links.push(crate::library::artistinfo::Link {
                        label: tr!("discogs suchen").to_string(),
                        url: format!("https://www.discogs.com/search/?q={encoded}&type=artist"),
                    });
                }
                ui.add_space(BLOCK_AIR * 2.0);
                say(ui, width, tr!("mehr lesen"), Role::Label, palette.faint, m);
                for link in &links {
                    if link_action(ui, width, &link.label, m, palette) {
                        open_url = Some(link.url.clone());
                    }
                }

                ui.add_space(BLOCK_AIR * 2.0);
                if device_action(ui, width, tr!("bearbeiten"), m, palette) {
                    edit = true;
                }
                if !looking && device_action(ui, width, tr!("neu suchen"), m, palette) {
                    look = true;
                }
                if !info.looked_up.is_empty() {
                    let today = chrono::Local::now().date_naive();
                    ui.add_space(BLOCK_AIR);
                    say(
                        ui,
                        width,
                        &tr!("gesucht {}", text::day(&info.looked_up, today)),
                        Role::Label,
                        palette.faint,
                        m,
                    );
                }
            });
        });
    });

    if close {
        app.toggle_artist();
    }
    if look {
        app.look_up_artist();
    }
    if edit {
        app.edit_artist();
    }
    if let Some(url) = open_url {
        if !crate::lastfm::open_in_browser(&url) {
            app.set_status(tr!("der browser liess sich nicht öffnen"));
        }
    }
    if let Some(name) = copied {
        ui.ctx().copy_text(name.clone());
        app.set_status(tr!("„{}“ kopiert", name));
    }
    if let Some(name) = visit {
        if let Some(now) = app.artist.name.clone() {
            app.artist_trail.push(now);
        }
        app.about_artist(&name);
    }
    if back {
        if let Some(before) = app.artist_trail.pop() {
            app.about_artist(&before);
        }
    }
    if to_gaps {
        app.show_gaps();
    }
    if let Some(at) = go_to {
        if let Err(e) = app.select(Column::Artists, ArtistRow::Named(at).index()) {
            app.set_status(format!("{e}"));
        }
        app.scroll_to = Some(Column::Artists);
    }
    Rect::from_min_size(top, Vec2::new(width, height))
}

pub(super) const ARTIST_PICTURE: f32 = 176.0;

pub(super) fn picture_side(inner: f32, wide: bool) -> f32 {
    if wide {
        inner
    } else {
        inner.min(ARTIST_PICTURE)
    }
}

pub(super) const ARTIST_SIMILAR: usize = 8;

pub(super) const ARTIST_TIES: usize = 12;

pub(super) fn artist_picture(app: &mut GuiApp, ui: &Ui, rect: Rect, palette: &Palette, name: &str) {
    use crate::library::artistinfo;
    let key = app.artist.key;
    let dithered = app.config.ui.cover_dither;
    let grain = crate::gui::dither::grain_for(
        rect.width(),
        ui.ctx().pixels_per_point(),
        app.config.ui.picture_fineness(),
    );
    let stamp = app.artist.stamp;
    let negative = palette.screens_negative();
    let method = crate::gui::dither::Look::of(&app.config.ui);
    let stamp = (stamp, negative, dithered, method);
    let fresh = !matches!(
        &app.artist.picture,
        Some((k, g, s, _)) if *k == key && *g == grain && *s == stamp
    );
    if fresh {
        app.artist.picture = None;
        let path = artistinfo::picture_path(&app.config.artists_dir(), key);
        if let Ok(decoded) = image::open(&path) {
            let image = if dithered {
                crate::gui::covers::screened(&decoded, grain, method, negative)
            } else {
                let rgba = decoded.to_rgba8();
                egui::ColorImage::from_rgba_unmultiplied(
                    [rgba.width() as usize, rgba.height() as usize],
                    rgba.as_raw(),
                )
            };
            let handle = ui
                .ctx()
                .load_texture("artist-picture", image, egui::TextureOptions::NEAREST);
            app.artist.picture = Some((key, grain, stamp, handle));
        }
    }
    match &app.artist.picture {
        Some((_, _, _, handle)) => {
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

pub(super) fn paragraph(ui: &mut Ui, width: f32, words: &str, role: Role, colour: Color32, m: Metrics) {
    let room = (width - m.marker_pad_x * 2.0).max(60.0);
    let g = text::paragraph(ui, words, role, colour, room);
    place_wrapped(ui, width, g, colour, m);
}

pub(super) fn prose(ui: &mut Ui, width: f32, words: &str, role: Role, colour: Color32, m: Metrics) {
    let room = (width - m.marker_pad_x * 2.0).max(60.0);
    let g = text::prose(ui, words, role, colour, room);
    place_wrapped(ui, width, g, colour, m);
}

fn place_wrapped(ui: &mut Ui, width: f32, g: std::sync::Arc<egui::Galley>, colour: Color32, m: Metrics) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, g.size().y + 4.0), Sense::hover());
    ui.painter()
        .galley(egui::pos2(rect.left() + m.marker_pad_x, rect.top() + 2.0), g, colour);
}

pub(super) fn artist_form(app: &mut GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette, width: f32) {
    use crate::gui::artist::{LABELS, NOTE};
    let blink = ui.input(|i| i.time).rem_euclid(1.06) < 0.62;
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(120));
    let mut focus_on: Option<usize> = None;
    let mut save = false;
    let mut cancel = false;
    let mut pick = false;
    {
        let Some(form) = app.artist_form.as_ref() else {
            return;
        };
        for (i, label) in LABELS.iter().enumerate() {
            let focused = form.focus == i;
            let state = RowState {
                selected: focused,
                focused,
                ..Default::default()
            };
            let text = form.entries[i].text();
            let lines: Vec<&str> = if i == NOTE { text.split('\n').collect() } else { vec![text] };
            let rows = lines.len().max(1);
            let (rect, response) = ui.allocate_exact_size(
                Vec2::new(width, m.row * rows as f32),
                Sense::click(),
            );
            paint_marker(ui, rect, state, m, palette);
            paint_hover(ui, rect, &response, state, m, palette);
            if response.clicked() {
                focus_on = Some(i);
            }
            let content = content_rect(rect, m);
            let label_colour = if focused { palette.ink } else { palette.faint };
            let g = text::line(ui, crate::lang::t(label), Role::Label, label_colour);
            let first = text::row_line(ui, m.row, Role::Content);
            ui.painter().galley(
                egui::pos2(content.left(), text::on_line(&g, rect.top(), first)),
                g,
                label_colour,
            );
            let left = content.left() + TAG_LABEL_WIDTH;
            let room = (content.right() - left).max(20.0);
            let caret = form.entries[i].caret();
            let before = &text[..caret];
            let caret_line = before.matches('\n').count();
            let caret_col = before.rsplit('\n').next().unwrap_or("");
            let marked = focused && form.entries[i].is_marked();
            for (n, line) in lines.iter().enumerate() {
                let top = rect.top() + m.row * n as f32;
                let g = text::cased_within(ui, line, Role::Content, palette.ink, room);
                let at = egui::pos2(left, text::on_line(&g, top, first));
                if marked {
                    let row = Rect::from_min_size(egui::pos2(rect.left(), top), Vec2::new(rect.width(), m.row));
                    paint_field_mark(ui, at, &g, row, palette);
                }
                ui.painter().galley(at, g, palette.ink);
                if focused && blink && !marked && n == caret_line {
                    let at = text::cased_within(ui, caret_col, Role::Content, palette.ink, f32::INFINITY)
                        .size()
                        .x;
                    let bar = Rect::from_min_size(
                        egui::pos2((left + at + 1.0).min(content.right() - CARET_WIDTH), top + 6.0),
                        Vec2::new(CARET_WIDTH, m.row - 12.0),
                    );
                    ui.painter().rect_filled(bar, 0.0, palette.ink);
                }
            }
        }
        ui.add_space(16.0);
        say(
            ui,
            width,
            tr!("tab wechselt     enter speichert     esc bricht ab"),
            Role::Label,
            palette.mid,
            m,
        );
        if form.focus == NOTE {
            say(ui, width, tr!("shift-enter: neue zeile"), Role::Label, palette.faint, m);
        }
        ui.add_space(6.0);
        if device_action(ui, width, tr!("bild aus datei…"), m, palette) {
            pick = true;
        }
        if device_action(ui, width, tr!("speichern"), m, palette) {
            save = true;
        }
        if device_action(ui, width, tr!("abbrechen"), m, palette) {
            cancel = true;
        }
    }
    if let Some(i) = focus_on {
        if let Some(form) = app.artist_form.as_mut() {
            form.focus = i;
            form.entries[i].end();
        }
    }
    if pick {
        app.artist_picture_from_file();
    }
    if save {
        app.save_artist_form();
    } else if cancel {
        app.artist_form = None;
    }
}

enum Other {
    Visit,
    Copy,
}

fn other_row(ui: &mut Ui, width: f32, name: &str, colour: Color32, m: Metrics, palette: &Palette) -> Option<Other> {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, m.row), Sense::click());
    paint_hover(ui, rect, &response, RowState::default(), m, palette);
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let content = content_rect(rect, m);
    let line = text::row_line(ui, m.row, Role::Content);
    let hint = text::line(ui, tr!("kopieren"), Role::Label, palette.faint);
    let hint_at = egui::pos2(content.right() - hint.size().x, text::beside_content(ui, &hint, Role::Label, rect.top(), m.row));
    let hint_rect = Rect::from_min_max(
        egui::pos2(hint_at.x - 6.0, rect.top()),
        egui::pos2(rect.right(), rect.bottom()),
    );
    let over_hint = response.hover_pos().is_some_and(|p| hint_rect.contains(p));
    let room = content.width() - if response.hovered() { hint.size().x + 12.0 } else { 0.0 };
    let colour = if response.hovered() && !over_hint { palette.ink } else { colour };
    let g = text::line_within(ui, name, Role::Content, colour, room.max(20.0));
    ui.painter()
        .galley(egui::pos2(content.left(), text::on_line(&g, rect.top(), line)), g, colour);
    if response.hovered() {
        let tint = if over_hint { palette.ink } else { palette.mid };
        ui.painter().galley(hint_at, hint, tint);
    }
    if !response.clicked() {
        return None;
    }
    let at = response.interact_pointer_pos();
    Some(if at.is_some_and(|p| hint_rect.contains(p)) { Other::Copy } else { Other::Visit })
}
