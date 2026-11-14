use super::*;

const VERSE_AIR: f32 = 0.6;

pub(super) fn lyrics_column(app: &mut GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette, width: f32) -> Rect {
    app.follow_the_words();
    let top = ui.cursor().min;
    let height = ui.available_height();
    let mut close = false;
    let mut look = false;

    let playing = app.player.library_track().is_some_and(|t| {
        let artist = if t.artist.is_empty() { &t.album_artist } else { &t.artist };
        app.lyrics.of.as_ref().is_some_and(|(a, title)| a == artist && *title == t.title)
    });
    let now = app.player.position().as_secs_f64();
    let sung = app
        .lyrics
        .lyrics
        .as_ref()
        .filter(|_| playing)
        .and_then(|l| l.now(now));
    if playing && app.lyrics.lyrics.as_ref().is_some_and(|l| l.timed()) {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(150));
    }

    ui.allocate_ui(Vec2::new(width, height), |ui| {
        ui.set_width(width);
        ui.vertical(|ui| {
            let song = app.lyrics.of.clone();
            if heading(ui, tr!("liedtext"), "", None, Some(tr!("schliessen")), m, palette).clicked() {
                close = true;
            }
            ui.add_space(HEAD_AIR);
            let Some((artist, title)) = song.clone() else {
                say(ui, width, tr!("nichts ausgewählt"), Role::Content, palette.faint, m);
                return;
            };
            paragraph(ui, width, &title, Role::Content, palette.ink, m);
            paragraph(ui, width, &artist, Role::Content, palette.mid, m);
            let looking = song.as_ref().is_some_and(|s| app.lyrics.looking_for(s));

            scrolled(ui, "lyrics", palette, |ui| {
                ui.add_space(BLOCK_AIR * 2.0);
                if looking {
                    let dots = ".".repeat(1 + (ui.input(|i| i.time) / 0.4) as usize % 3);
                    say(ui, width, &format!("{}{}", tr!("lade liedtext"), dots), Role::Label, palette.faint, m);
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
                    return;
                }
                if let Some(why) = &app.lyrics.trouble {
                    prose(ui, width, why, Role::Label, palette.faint, m);
                    ui.add_space(BLOCK_AIR * 2.0);
                    if device_action(ui, width, tr!("neu suchen"), m, palette) {
                        look = true;
                    }
                    return;
                }
                let Some(words) = app.lyrics.lyrics.clone() else {
                    say(ui, width, tr!("kein liedtext gefunden"), Role::Content, palette.faint, m);
                    ui.add_space(BLOCK_AIR * 2.0);
                    if device_action(ui, width, tr!("neu suchen"), m, palette) {
                        look = true;
                    }
                    return;
                };
                say(ui, width, &tr!("aus {}", crate::lang::t(words.from.name())), Role::Label, palette.faint, m);
                if words.instrumental {
                    say(ui, width, tr!("ohne worte"), Role::Content, palette.mid, m);
                } else {
                    let timed = words.timed();
                    let kept = egui::Id::unique("lyrics-sung");
                    let role = if app.config.ui.lyrics_size == "klein" { Role::Label } else { Role::Content };
                    let follow = app.config.ui.lyrics_follow;
                    let was: Option<usize> = ui.data(|d| d.get_temp(kept)).flatten();
                    for (at, line) in words.lines.iter().enumerate() {
                        if line.text.is_empty() {
                            ui.add_space(m.row * VERSE_AIR);
                            continue;
                        }
                        let colour = match (timed, sung) {
                            (true, Some(now)) if now == at => palette.accent,
                            (true, _) => palette.mid,
                            (false, _) => palette.ink,
                        };
                        let room = (width - m.marker_pad_x * 2.0).max(60.0);
                        let g = text::paragraph(ui, &line.text, role, colour, room);
                        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, g.size().y + 4.0), Sense::hover());
                        ui.painter().galley(egui::pos2(rect.left() + m.marker_pad_x, rect.top() + 2.0), g, colour);
                        if follow && sung == Some(at) && was != sung {
                            ui.scroll_to_rect(rect, Some(egui::Align::Center));
                        }
                    }
                    ui.data_mut(|d| d.insert_temp(kept, sung));
                }
                if words.from != crate::library::lyrics::From::File {
                    ui.add_space(BLOCK_AIR * 2.0);
                    if device_action(ui, width, tr!("neu suchen"), m, palette) {
                        look = true;
                    }
                }
            });
        });
    });

    if close {
        app.toggle_lyrics();
    }
    if look {
        app.look_up_lyrics();
    }
    Rect::from_min_size(top, Vec2::new(width, height))
}
