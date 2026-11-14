use super::*;
use crate::podcast::{Chapter, Episode, Show};

pub(super) const ON_THE_RULE: &str = "chapter-on-the-rule";
pub(super) const IN_THE_LIST: &str = "chapter-in-the-list";

pub(super) fn point_at_chapter(ctx: &egui::Context, key: &str, chapter: Option<usize>) {
    let frame = ctx.cumulative_frame_nr();
    ctx.data_mut(|d| match chapter {
        Some(i) => {
            d.insert_temp(egui::Id::unique(key), (i, frame));
        }
        None => {
            d.remove_temp::<(usize, u64)>(egui::Id::unique(key));
        }
    });
}

pub(super) fn pointed_chapter(ctx: &egui::Context, key: &str) -> Option<usize> {
    let frame = ctx.cumulative_frame_nr();
    ctx.data(|d| d.get_temp::<(usize, u64)>(egui::Id::unique(key)))
        .filter(|(_, at)| frame.saturating_sub(*at) <= 1)
        .map(|(i, _)| i)
}

const MORE_WITHIN: f32 = 600.0;

enum Asked {
    Choose(Option<Show>),
    Follow(Show),
    Look(Episode, Show),
    Play(Episode, Show),
    Chapter(Episode, Show, f64),
    More,
    Older(Show),
    Fetched,
    Forget(Episode),
    Archive(Episode),
    Queue(Episode, Show),
    PlayQueued(Episode, Show),
    ClearQueue,
    Unspend(String),
    LookQueued(Episode, Show),
    ShowArchived,
    CheckNow,
    TurnOrder,
    Starred,
    Star(Episode, Show),
    Open,
    Share(Episode, Show),
    Heard(Episode, bool),
    Sleep,
    Menu(usize, egui::Pos2),
    Lift(Vec<(Episode, Show)>),
    LiftQueued(usize),
    Drop(usize),
    PickAlso(Episode, Show),
    PickThrough(Vec<(Episode, Show)>),
    ArchivePicked,
    QueuePicked,
    ForgetPicked,
    Unpick,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn podcast_row(app: &mut GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette, full_width: f32, narrow: bool, base: f32, reach: &dyn Fn(&Ui, usize, f32) -> f32) {
    let gap = m.column_gap;
    let min = crate::gui::theme::MIN_NAME_COLUMN;
    let carrying = matches!(app.dragging, Some(crate::gui::Dragged::Episodes(_)));
    let with_queue = (app.config.ui.podcast_queue || app.log_open || app.eq_open || app.podcasts.info_open || carrying) && crate::gui::theme::even_columns(4, full_width, gap) > min;
    let count = if with_queue { 4 } else { 3 };
    let widths_for = |count: usize| -> Vec<f32> {
        let even = crate::gui::theme::even_columns(count, full_width, gap);
        let mut widths = vec![even; count];
        for (i, width) in widths.iter_mut().enumerate().take(count - 1) {
            if let Some(share) = app.config.ui.podcast_shares.get(i).copied().filter(|s| *s > 0.0) {
                *width = (share * full_width).max(min);
            }
        }
        let others: f32 = widths[..count - 1].iter().sum::<f32>() + gap * (count - 1) as f32;
        widths[count - 1] = (full_width - others).max(min);
        widths
    };
    let (three, four) = (widths_for(3), widths_for(4));
    let widths = if with_queue { four.clone() } else { three.clone() };
    let open = slide(ui.ctx(), egui::Id::unique("podcast-queue-open"), with_queue, false);
    let mix = |i: usize| three[i] + (four[i] - three[i]) * open;
    let (first, episodes, info) = (mix(0), mix(1), mix(2));
    let queue = four[3];
    let queue_room = (full_width - first - episodes - info - gap * 3.0).max(0.0);
    let mut asked: Option<Asked> = None;
    if narrow {
        podcast_strip(app, ui, m, palette, full_width, base, &mut asked);
    } else {
        ui.data_mut(|d| d.remove_temp::<isize>(egui::Id::unique("podcast-strip-step")));
        let column = |ui: &mut Ui, n: usize, width: f32, body: &mut dyn FnMut(&mut Ui)| -> Rect {
            let height = reach(ui, n, width);
            ui.allocate_ui_with_layout(Vec2::new(width, height), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(width);
                ui.set_height(height);
                body(ui);
            })
            .response
            .rect
        };
        let mut taken: Vec<Rect> = Vec::new();
        let mut queue_seen: Option<Rect> = None;
        ui.horizontal_top(|ui| {
            taken.push(column(ui, 0, first, &mut |ui| shows_column(app, ui, m, palette, first, &mut asked)));
            ui.add_space(gap);
            taken.push(column(ui, 1, episodes, &mut |ui| episodes_column(app, ui, m, palette, episodes, &mut asked)));
            ui.add_space(gap);
            taken.push(column(ui, 2, info, &mut |ui| episode_column(app, ui, m, palette, info, &mut asked)));
            if open > 0.0 && queue_room > 1.0 {
                ui.add_space(gap);
                let height = reach(ui, 3, queue);
                let rect = ui.allocate_ui_with_layout(Vec2::new(queue_room, height), egui::Layout::top_down(egui::Align::Min), |ui| {
                    let seen = ui.max_rect();
                    ui.set_clip_rect(ui.clip_rect().intersect(seen));
                    ui.allocate_ui_with_layout(Vec2::new(queue, height), egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.set_width(queue);
                        ui.set_height(height);
                        if app.log_open && !carrying {
                            crate::gui::library::fourth::log_column(app, ui, m, palette, queue);
                        } else if app.eq_open && !carrying {
                            crate::gui::library::eq_col::eq_column(app, ui, m, palette, queue);
                        } else if app.podcasts.info_open && !carrying {
                            podcast_info_column(app, ui, m, palette, queue);
                        } else {
                            episode_queue_column(app, ui, m, palette, queue, &mut asked);
                        }
                    });
                })
                .response
                .rect;
                queue_seen = Some(rect);
                if with_queue && open >= 1.0 {
                    taken.push(rect);
                }
            }
        });

        if let Some(at) = ui.input(|i| i.pointer.primary_pressed().then(|| i.pointer.interact_pos()).flatten()) {
            if let Some(n) = taken.iter().take(3).position(|r| r.contains(at)) {
                app.podcasts.focus = n;
            } else if queue_seen.is_some_and(|r| r.contains(at)) {
                app.podcasts.focus = 3;
            }
        }
        let mut settled = false;
        for index in 0..taken.len().saturating_sub(1) {
            let (left, right) = (taken[index], taken[index + 1]);
            let Some(handle) = divider(ui, 300 + index, left, right, palette) else { continue };
            match handle {
                Handle::Reset => {
                    app.config.ui.podcast_shares.clear();
                    settled = true;
                }
                Handle::Width(width, done) => {
                    settled |= done;
                    let shares = &mut app.config.ui.podcast_shares;
                    if shares.len() < count - 1 {
                        shares.resize(count - 1, 0.0);
                    }
                    for (i, share) in shares.iter_mut().enumerate().take(index) {
                        *share = widths[i] / full_width;
                    }
                    let room = full_width - widths[..index].iter().sum::<f32>() - gap * (count - 1) as f32 - min * (count - 1 - index) as f32;
                    shares[index] = width.clamp(min, room.max(min)) / full_width;
                }
            }
        }
        if settled {
            app.save_config();
        }
    }
    if narrow {
        let to = match &asked {
            Some(Asked::Choose(_) | Asked::Fetched | Asked::Starred | Asked::Open) => Some(1),
            Some(Asked::Look(..) | Asked::LookQueued(..)) => Some(2),
            _ => None,
        };
        if let Some(to) = to {
            ui.data_mut(|d| d.insert_temp(egui::Id::unique(STRIP_AT), to));
        }
    }
    match asked {
        Some(Asked::Choose(show)) => app.choose_podcast(show),
        Some(Asked::Follow(show)) => app.follow_podcast(&show),
        Some(Asked::Look(episode, show)) => {
            app.podcasts.picked.clear();
            app.choose_episode(episode, show);
        }
        Some(Asked::LookQueued(episode, show)) => {
            app.podcasts.picked.clear();
            app.podcasts.queue_cursor = Some(episode.id.clone());
            app.choose_episode(episode, show);
        }
        Some(Asked::Play(episode, show)) => {
            app.choose_episode(episode.clone(), show.clone());
            app.fetch_and_play(episode, show, None);
        }
        Some(Asked::Chapter(episode, show, start)) => app.play_chapter(episode, show, start),
        Some(Asked::Fetched) => app.choose_fetched(),
        Some(Asked::Forget(episode)) => app.delete_episode_file(&episode),
        Some(Asked::Archive(episode)) => app.archive_episode(&episode),
        Some(Asked::Queue(episode, show)) => app.queue_episode(&episode, &show),
        Some(Asked::PlayQueued(episode, show)) => app.play_queued_episode(episode, show),
        Some(Asked::ClearQueue) => app.clear_episode_queue(),
        Some(Asked::Unspend(id)) => app.forget_spent(&id),
        Some(Asked::PickAlso(episode, show)) => {
            let panel = &mut app.podcasts;
            if panel.picked.is_empty() {
                if let Some(looked) = panel.episode.clone() {
                    panel.picked.push(looked);
                }
            }
            if let Some(at) = panel.picked.iter().position(|(e, _)| e.id == episode.id) {
                panel.picked.remove(at);
            } else {
                panel.picked.push((episode, show));
            }
        }
        Some(Asked::PickThrough(run)) => app.podcasts.picked = run,
        Some(Asked::ArchivePicked) => app.archive_picked(),
        Some(Asked::QueuePicked) => app.queue_picked(),
        Some(Asked::ForgetPicked) => app.forget_picked(),
        Some(Asked::Unpick) => app.podcasts.picked.clear(),
        Some(Asked::CheckNow) => app.check_for_episodes(),
        Some(Asked::TurnOrder) => app.turn_podcast_order(),
        Some(Asked::Starred) => app.choose_starred(),
        Some(Asked::Star(episode, show)) => app.star_episode(&episode, &show),
        Some(Asked::Open) => app.choose_open(),
        Some(Asked::Share(episode, show)) => app.share_episode(ui.ctx(), &episode, &show),
        Some(Asked::Heard(episode, heard)) => app.mark_heard(&episode, heard),
        Some(Asked::Sleep) => app.step_sleep(),
        Some(Asked::Lift(episodes)) => app.dragging = Some(crate::gui::Dragged::Episodes(episodes)),
        Some(Asked::LiftQueued(at)) => app.dragging = Some(crate::gui::Dragged::QueuedEpisode(at)),
        Some(Asked::Drop(at)) => app.drop_episodes(at),
        Some(Asked::Menu(at, pos)) => app.row_menu = Some(crate::gui::RowMenu { at: pos, about: crate::gui::About::Episode(at) }),
        Some(Asked::ShowArchived) => {
            app.config.ui.podcast_show_archived = !app.config.ui.podcast_show_archived;
            app.save_config();
        }
        Some(Asked::More) => app.podcasts.more += crate::gui::app::podcasts::EPISODES_PAGE,
        Some(Asked::Older(show)) => app.load_older(&show),
        None => {}
    }
}

fn show_page(app: &GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette, width: f32, show: &Show, feed: &crate::podcast::Feed) {
    let side = (width - m.marker_pad_x * 2.0).clamp(40.0, 176.0);
    let (area, _) = ui.allocate_exact_size(Vec2::new(width, side), Sense::hover());
    let picture = Rect::from_min_size(egui::pos2(content_rect(area, m).left(), area.top()), Vec2::splat(side));
    picture_at(app, ui, picture, palette, &crate::podcast::artwork_path(show), &show.title);
    ui.add_space(BLOCK_AIR * 2.0);
    paragraph(ui, width, &show.title, Role::ContentBold, palette.ink, m);
    let makers = if feed.people.is_empty() { feed.author.clone() } else { feed.people.join(", ") };
    if !makers.is_empty() {
        say(ui, width, &makers, Role::Label, palette.mid, m);
    }
    if let Some(said) = crate::podcast::feed::rhythm(&feed.episodes, chrono::Local::now().date_naive()) {
        say(ui, width, &said, Role::Label, palette.faint, m);
    }
    if !feed.link.is_empty() {
        ui.add_space(BLOCK_AIR);
        let shown = feed.link.trim_start_matches("https://").trim_start_matches("http://").trim_start_matches("www.").trim_end_matches('/');
        if link_action(ui, width, shown, m, palette) {
            crate::lastfm::open_in_browser(&feed.link);
        }
    }
    if !feed.about.is_empty() {
        ui.add_space(BLOCK_AIR * 2.0);
        prose(ui, width, &feed.about, Role::Label, palette.faint, m);
    }
    links_block(ui, width, &feed.about_links, m, palette);
}

fn links_block(ui: &mut Ui, width: f32, links: &[(String, String)], m: Metrics, palette: &Palette) {
    if links.is_empty() {
        return;
    }
    ui.add_space(BLOCK_AIR);
    for (words, url) in links {
        let shown = if words == url {
            url.trim_start_matches("https://").trim_start_matches("http://").trim_start_matches("www.").trim_end_matches('/').to_string()
        } else {
            words.clone()
        };
        if link_action(ui, width, &shown, m, palette) {
            crate::lastfm::open_in_browser(url);
        }
    }
}

const HEAD_ABOUT_CHARS: usize = 180;

fn show_head(app: &GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette, width: f32, show: &Show, feed: &crate::podcast::Feed) {
    let small = (m.row * 0.72).round();
    let side = (m.row + small * 2.0) * 1.6;
    let top = ui.cursor().min;
    let content = content_rect(Rect::from_min_size(top, Vec2::new(width, side)), m);
    let picture = Rect::from_min_size(content.min, Vec2::splat(side));
    picture_at(app, ui, picture, palette, &crate::podcast::artwork_path(show), &show.title);
    let left = picture.right() + ALBUM_HEAD_GAP;
    let beside = (content.right() - left).max(40.0);
    let column = Rect::from_min_size(egui::pos2(left - m.marker_pad_x, top.y), Vec2::new(beside + m.marker_pad_x * 2.0, side));
    let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(column).layout(egui::Layout::top_down(egui::Align::Min)));
    let w = column.width();
    paragraph(&mut inner, w, &show.title, Role::ContentBold, palette.ink, m);
    let makers = if feed.people.is_empty() { feed.author.clone() } else { feed.people.join(", ") };
    if !makers.is_empty() {
        say(&mut inner, w, &makers, Role::Label, palette.mid, m);
    }
    if let Some(said) = crate::podcast::feed::rhythm(&feed.episodes, chrono::Local::now().date_naive()) {
        say(&mut inner, w, &said, Role::Label, palette.faint, m);
    }
    if !feed.link.is_empty() {
        let shown = feed.link.trim_start_matches("https://").trim_start_matches("http://").trim_start_matches("www.").trim_end_matches('/');
        if link_action(&mut inner, w, shown, m, palette) {
            crate::lastfm::open_in_browser(&feed.link);
        }
    }
    for (_, url) in &feed.funding {
        let host = url.trim_start_matches("https://").trim_start_matches("http://").trim_start_matches("www.");
        let host = host.split('/').next().unwrap_or(host);
        if link_action(&mut inner, w, &tr!("unterstützen · {}", host), m, palette) {
            crate::lastfm::open_in_browser(url);
        }
    }
    let used = inner.min_rect().bottom().max(picture.bottom());
    ui.allocate_exact_size(Vec2::new(width, (used - top.y).max(side)), Sense::hover());
    if !feed.about.is_empty() {
        ui.add_space(BLOCK_AIR);
        let open_id = egui::Id::unique(("podcast-head-about", show.feed.as_str()));
        let open: bool = ui.data(|d| d.get_temp(open_id)).unwrap_or(false);
        let long = feed.about.chars().count() > HEAD_ABOUT_CHARS;
        let about = if open || !long {
            feed.about.clone()
        } else {
            let cut: String = feed.about.chars().take(HEAD_ABOUT_CHARS).collect();
            let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head).trim_end().to_string();
            format!("{cut} …")
        };
        prose(ui, width, &about, Role::Label, palette.faint, m);
        if open || !long {
            links_block(ui, width, &feed.about_links, m, palette);
        }
        if long && link_action(ui, width, if open { tr!("weniger") } else { tr!("mehr") }, m, palette) {
            ui.data_mut(|d| d.insert_temp(open_id, !open));
        }
    }
    ui.add_space(BLOCK_AIR * 2.0);
}

fn podcast_info_column(app: &GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette, width: f32) {
    let panel = &app.podcasts;
    still_heading(ui, tr!("informationen"), "", m, palette);
    ui.add_space(HEAD_AIR);
    let playing = panel.playing_from.clone().filter(|(e, _)| app.playing_this_episode(&e.id));
    let show = playing.as_ref().map(|(_, s)| s.clone()).or_else(|| panel.chosen.clone());
    let Some(show) = show else {
        prose(ui, width, tr!("es läuft gerade keine folge — hier steht dann, was über ihren podcast bekannt ist"), Role::Label, palette.faint, m);
        return;
    };
    scrolled(ui, "podcast-info", palette, |ui| {
        if let Some((episode, _)) = &playing {
            say(ui, width, tr!("läuft"), Role::Label, palette.faint, m);
            paragraph(ui, width, &episode.title, Role::Content, palette.ink, m);
            ui.add_space(BLOCK_AIR * 2.0);
        }
        match panel.feeds.get(&show.feed) {
            Some(feed) => show_page(app, ui, m, palette, width, &show, feed),
            None => say(ui, width, &format!("{} …", tr!("lade informationen")), Role::Label, palette.faint, m),
        }
    });
}

const STRIP_AT: &str = "podcast-strip-at";

fn podcast_strip(app: &mut GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette, room: f32, height: f32, asked: &mut Option<Asked>) {
    let gap = crate::gui::theme::SLIDE_GAP;
    let mut names: Vec<&'static str> = vec![tr!("podcasts"), tr!("folgen"), tr!("folge")];
    let side = if app.log_open {
        Some("log")
    } else if app.eq_open {
        Some("equalizer")
    } else if app.podcasts.info_open {
        Some(tr!("informationen"))
    } else if app.config.ui.podcast_queue {
        Some(tr!("warteschlange"))
    } else {
        None
    };
    names.extend(side);
    let count = names.len();
    let at_id = egui::Id::unique(STRIP_AT);
    let side_id = egui::Id::unique("podcast-strip-side");
    let was_side: Option<&'static str> = ui.data(|d| d.get_temp::<Option<&'static str>>(side_id)).flatten();
    let mut index: usize = ui.data(|d| d.get_temp(at_id)).unwrap_or(0);
    if (app.log_open || app.eq_open || app.podcasts.info_open) && was_side != side {
        index = count - 1;
    }
    let step: isize = ui.data_mut(|d| d.remove_temp::<isize>(egui::Id::unique("podcast-strip-step"))).unwrap_or(0);
    let index = (index as isize + step).clamp(0, count as isize - 1) as usize;
    ui.data_mut(|d| {
        d.insert_temp(at_id, index);
        d.insert_temp(side_id, side);
    });
    let offset = ui.ctx().animate_value_with_time(
        egui::Id::unique("podcast-slide"),
        crate::gui::theme::slide_offset(index, count, room, gap),
        0.22,
    );
    let steps_height = m.row * 1.5;
    let strip_height = (height - steps_height).max(m.row);
    let (area, _) = ui.allocate_exact_size(Vec2::new(room, strip_height), Sense::hover());
    let strip = Rect::from_min_size(egui::pos2(area.left() - offset, area.top()), Vec2::new(crate::gui::theme::strip_width(count, room, gap), strip_height));
    let mut inside = ui.new_child(egui::UiBuilder::new().max_rect(strip));
    inside.set_clip_rect(area.intersect(ui.clip_rect()));
    inside.horizontal_top(|ui| {
        let column = |ui: &mut Ui, body: &mut dyn FnMut(&mut Ui)| {
            ui.allocate_ui_with_layout(Vec2::new(room, strip_height), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(room);
                ui.set_height(strip_height);
                body(ui);
            });
        };
        column(ui, &mut |ui| shows_column(app, ui, m, palette, room, asked));
        ui.add_space(gap);
        column(ui, &mut |ui| episodes_column(app, ui, m, palette, room, asked));
        ui.add_space(gap);
        column(ui, &mut |ui| episode_column(app, ui, m, palette, room, asked));
        if side.is_some() {
            ui.add_space(gap);
            column(ui, &mut |ui| {
                if app.log_open {
                    crate::gui::library::fourth::log_column(app, ui, m, palette, room);
                } else if app.eq_open {
                    crate::gui::library::eq_col::eq_column(app, ui, m, palette, room);
                } else if app.podcasts.info_open {
                    podcast_info_column(app, ui, m, palette, room);
                } else {
                    episode_queue_column(app, ui, m, palette, room, asked);
                }
            });
        }
    });
    let line = Rect::from_min_size(area.left_bottom(), Vec2::new(room, steps_height));
    if let Some(to) = crate::gui::library::strip_steps(ui, line, &names, index, m, palette, "podcasts") {
        ui.data_mut(|d| d.insert_temp(at_id, to));
    }
}

pub fn step_podcast_strip(ctx: &egui::Context, way: isize) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("podcast-strip-step"), way));
}

fn shows_column(app: &GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette, width: f32, asked: &mut Option<Asked>) {
    let panel = &app.podcasts;
    let head = if panel.found.is_some() { tr!("gefunden") } else { "podcasts" };
    let refresh = if app.checking_for_episodes() { tr!("sehe nach…") } else { tr!("aktualisieren") };
    let offered = (panel.found.is_none() && !panel.kept.show.is_empty()).then_some(refresh);
    let mark = match app.config.ui.podcast_order.as_str() {
        "a-z" => "↓",
        "z-a" => "↑",
        _ => tr!("zuletzt"),
    };
    let count = panel.found.as_ref().map_or(panel.kept.show.len(), Vec::len);
    let trailing = if count == 0 { String::new() } else { text::count(count as u64) };
    if panel.found.is_some() {
        still_heading(ui, head, &trailing, m, palette);
    } else {
        let head = heading_with(ui, head, &trailing, Some(mark), None, offered, m, palette);
        if head.beside_clicked() && !app.checking_for_episodes() {
            *asked = Some(Asked::CheckNow);
        } else if head.clicked() {
            *asked = Some(Asked::TurnOrder);
        }
    }
    ui.add_space(HEAD_AIR);
    scrolled(ui, "podcast-shows", palette, |ui| {
        if panel.searching.is_some() {
            say(ui, width, &format!("{}…", tr!("suche")), Role::Label, palette.faint, m);
        }
        if let Some(found) = &panel.found {
            if found.is_empty() && panel.searching.is_none() {
                say(ui, width, tr!("nichts gefunden"), Role::Label, palette.faint, m);
            }
            for (i, show) in found.iter().enumerate() {
                let follows = panel.kept.follows(&show.feed);
                show_row(ui, ("podcast-found", i), width, show, follows, true, panel.chosen.as_ref() == Some(show), true, 0, m, palette, asked, &panel.follow);
            }
            return;
        }
        let all = panel.chosen.is_none() && !panel.only_fetched && !panel.only_starred && !panel.only_open;
        let here = panel.focus == 0;
        let state = RowState { selected: all, focused: all && here, arrow: all, playing: all, ..Default::default() };
        let new = panel.new_total();
        let figure = if new > 0 { tr!("{} neu", new) } else { String::new() };
        if name_row(ui, egui::Id::unique("podcast-newest"), width, tr!("alle"), &figure, Role::Label, state, m, palette).clicked() {
            *asked = Some(Asked::Choose(None));
        }
        let open = panel.open_count();
        let state = RowState { selected: panel.only_open, focused: panel.only_open && here, arrow: panel.only_open, playing: panel.only_open, ..Default::default() };
        let figure = if open > 0 { open.to_string() } else { String::new() };
        if name_row(ui, egui::Id::unique("podcast-open"), width, tr!("offen"), &figure, Role::Label, state, m, palette).clicked() {
            *asked = Some(Asked::Open);
        }
        let size = crate::podcast::cache_size();
        let mb = |bytes: u64| format!("{:.1}", bytes as f64 / 1_000_000.0);
        let (_, hidden) = panel.fetched_hidden(app.config.ui.podcast_show_archived);
        let figure = match (size, hidden) {
            (0, _) => String::new(),
            (size, 0) => format!("{} mb", mb(size)),
            (size, hidden) if hidden >= size => tr!("{} mb archiviert", mb(size)),
            (size, hidden) => tr!("{} mb · {} archiviert", size.div_ceil(1_000_000), hidden.div_ceil(1_000_000)),
        };
        let state = RowState { selected: panel.only_fetched, focused: panel.only_fetched && here, arrow: panel.only_fetched, playing: panel.only_fetched, ..Default::default() };
        if name_row(ui, egui::Id::unique("podcast-fetched"), width, tr!("geladen"), &figure, Role::Label, state, m, palette).clicked() {
            *asked = Some(Asked::Fetched);
        }
        let starred = panel.kept.starred.len();
        if starred > 0 || panel.only_starred {
            let state = RowState { selected: panel.only_starred, focused: panel.only_starred && here, arrow: panel.only_starred, playing: panel.only_starred, ..Default::default() };
            let figure = if starred > 0 { starred.to_string() } else { String::new() };
            if name_row(ui, egui::Id::unique("podcast-starred"), width, tr!("gemerkt"), &figure, Role::Label, state, m, palette).clicked() {
                *asked = Some(Asked::Starred);
            }
        }
        ui.add_space(BLOCK_AIR);
        if panel.kept.show.is_empty() {
            prose(ui, width, tr!("noch keinem podcast gefolgt — oben im suchfeld einen namen suchen oder eine feed-adresse einfügen"), Role::Label, palette.faint, m);
        }
        for (i, show) in app.shows_in_order().iter().enumerate() {
            let chosen = panel.chosen.as_ref().is_some_and(|c| c.feed == show.feed);
            let new = panel.new_by_show.get(&show.feed).copied().unwrap_or(0);
            show_row(ui, ("podcast-show", i), width, show, true, false, chosen, here, new, m, palette, asked, &panel.follow);
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn show_row(
    ui: &mut Ui,
    salt: (&str, usize),
    width: f32,
    show: &Show,
    follows: bool,
    found: bool,
    chosen: bool,
    here: bool,
    new: usize,
    m: Metrics,
    palette: &Palette,
    asked: &mut Option<Asked>,
    follow: &std::cell::RefCell<Option<String>>,
) {
    let ahead = row_ahead(ui, width, m);
    let over = super::pointer_rests_on(ui, ahead);
    let word = match (over, follows) {
        (true, true) => tr!("entfernen"),
        (true, false) => tr!("folgen"),
        (false, true) if found => tr!("gefolgt"),
        _ => "",
    };
    let count = tr!("{} neu", new);
    let figure = if word.is_empty() && new > 0 { count.as_str() } else { word };
    let state = RowState { selected: chosen, focused: chosen && here, arrow: chosen, playing: chosen, offering: over, ..Default::default() };
    let response = name_row(ui, egui::Id::unique(salt), width, &show.title, figure, Role::Label, state, m, palette);
    if follow.borrow().as_deref() == Some(show.feed.as_str()) {
        response.scroll_to_me(Some(egui::Align::Center));
        follow.replace(None);
    }
    point_at_word(ui, &response, word, over, m, palette);
    if response.clicked() {
        *asked = Some(if on_the_word(ui, &response, word, m, palette) && over { Asked::Follow(show.clone()) } else { Asked::Choose(Some(show.clone())) });
    }
}

fn episodes_column(app: &GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette, width: f32, asked: &mut Option<Asked>) {
    let panel = &app.podcasts;
    let chosen = panel.chosen.clone();
    let title = match &chosen {
        Some(show) => show.title.as_str(),
        None if panel.only_fetched => tr!("geladen"),
        None if panel.only_starred => tr!("gemerkt"),
        None if panel.only_open => tr!("offen"),
        None => tr!("alle"),
    };
    let follow = chosen.as_ref().map(|s| if panel.kept.follows(&s.feed) { tr!("nicht mehr folgen") } else { tr!("folgen") });
    let show_archived = app.config.ui.podcast_show_archived;
    let archived_word = if show_archived { tr!("archivierte ausblenden") } else { tr!("archivierte einblenden") };
    let head = if follow.is_some() {
        heading_with(ui, title, "", None, follow, Some(archived_word), m, palette)
    } else {
        still_heading_with(ui, title, "", Some(archived_word), m, palette)
    };
    if head.beside_clicked() {
        *asked = Some(Asked::ShowArchived);
    } else if head.clicked() {
        if let Some(show) = &chosen {
            *asked = Some(Asked::Follow(show.clone()));
        }
    }
    ui.add_space(HEAD_AIR);
    let reading = match &chosen {
        Some(show) => panel.reading_feed(&show.feed),
        None => !panel.reading.is_empty(),
    };
    let (list, total) = panel.episodes(show_archived);
    let below = scrolled(ui, "podcast-episodes", palette, |ui| {
        if let Some(show) = &chosen {
            if let Some(feed) = panel.feeds.get(&show.feed) {
                show_head(app, ui, m, palette, width, show, feed);
            }
        }
        if reading && list.is_empty() {
            say(ui, width, &format!("{}…", tr!("lese")), Role::Label, palette.faint, m);
        } else if list.is_empty() && panel.only_fetched {
            let (hidden, _) = panel.fetched_hidden(app.config.ui.podcast_show_archived);
            if hidden > 0 {
                let said = if hidden == 1 { tr!("1 geladene folge ist archiviert und ausgeblendet").to_string() } else { tr!("{} geladene folgen sind archiviert und ausgeblendet", hidden) };
                prose(ui, width, &said, Role::Label, palette.faint, m);
                if device_action(ui, width, tr!("archivierte einblenden"), m, palette) {
                    *asked = Some(Asked::ShowArchived);
                }
            } else {
                prose(ui, width, tr!("noch keine folge geladen — eine folge wird geladen, sobald sie spielt"), Role::Label, palette.faint, m);
            }
        }
        let looked = panel.episode.as_ref().map(|(e, _)| e.id.clone());
        let newest_first = list.windows(2).all(|w| crate::podcast::when(&w[0].0) >= crate::podcast::when(&w[1].0));
        let today = chrono::Local::now().date_naive();
        let mut last_age = None;
        for (i, (episode, show)) in list.iter().enumerate() {
            if newest_first || panel.only_open {
                let age = if panel.only_open && panel.kept.is_begun(&episode.id) { Age::Begun } else { Age::of(episode, today) };
                if last_age != Some(age) {
                    if last_age.is_some() {
                        ui.add_space(BLOCK_AIR);
                    }
                    say(ui, width, age.said(), Role::Label, palette.faint, m);
                    last_age = Some(age);
                }
            }
            let left = panel.kept.left(&episode.id);
            let heard = left.is_some_and(|l| l.done);
            let mut facts: Vec<String> = Vec::new();
            if chosen.is_none() {
                facts.push(show.title.clone());
            }
            match (episode.season, episode.number) {
                (Some(season), Some(number)) => facts.push(tr!("staffel {}, folge {}", season, number)),
                (None, Some(number)) => facts.push(tr!("folge {}", number)),
                _ => {}
            }
            facts.extend(day_of(episode));
            let heard_so_far = match (left, episode.seconds) {
                (Some(l), _) if l.done => tr!("fertig gehört").to_string(),
                (Some(l), Some(secs)) if l.at > 0.0 => tr!("noch {}", text::duration(secs.saturating_sub(l.at as u64))),
                (Some(l), None) if l.at > 0.0 => tr!("bei {}", text::duration(l.at as u64)),
                _ => tr!("noch nicht gehört").to_string(),
            };
            let coming = panel.fetching.as_ref().is_some_and(|(e, ..)| e.id == episode.id) && !app.playing_this_episode(&episode.id);
            let heard_so_far = if coming {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(400));
                format!("{}{}", tr!("lädt"), ".".repeat(1 + (ui.input(|i| i.time) / 0.4) as usize % 3))
            } else {
                heard_so_far
            };
            let picks_here = panel.focus != 3;
            let picked = picks_here && panel.picked.iter().any(|(e, _)| e.id == episode.id);
            let alone = panel.picked.is_empty() || !picks_here;
            let state = RowState {
                selected: picked || (alone && looked.as_deref() == Some(episode.id.as_str())),
                focused: panel.focus == 1 && (picked || (alone && looked.as_deref() == Some(episode.id.as_str()))),
                playing: app.playing_this_episode(&episode.id),
                muted: heard,
                spent: panel.kept.is_archived(&episode.id),
                ..Default::default()
            };
            let new = panel.is_new(episode);
            let offer = if panel.kept.is_archived(&episode.id) { tr!("zurückholen") } else { tr!("archivieren") };
            let (row, archive) = episode_row(app, ui, egui::Id::unique(("podcast-episode", i)), width, &episode.title, &facts.join(" · "), &heard_so_far, panel.kept.is_starred(&episode.id), show, &crate::podcast::artwork_path(show), new, !coming && crate::podcast::is_fetched(&episode.url), offer, state, m, palette);
            note_cursor_row(ui, "episode", row.rect, looked.as_deref() == Some(episode.id.as_str()));
            if panel.focus != 3 && panel.follow.borrow().as_deref() == Some(episode.id.as_str()) {
                row.scroll_to_me(Some(egui::Align::Center));
                panel.follow.replace(None);
            }
            if archive {
                *asked = Some(Asked::Archive(episode.clone()));
            } else if row.drag_started() {
                let lifted = if picked && panel.picked.len() > 1 { panel.picked.clone() } else { vec![(episode.clone(), show.clone())] };
                *asked = Some(Asked::Lift(lifted));
            } else if row.secondary_clicked() {
                if let Some(pos) = ui.ctx().pointer_latest_pos() {
                    *asked = Some(Asked::Menu(i, pos));
                }
            } else if row.double_clicked() {
                *asked = Some(Asked::Play(episode.clone(), show.clone()));
            } else if row.clicked() {
                let modifiers = ui.input(|input| input.modifiers);
                let from = looked.as_deref().and_then(|id| list.iter().position(|(e, _)| e.id == id));
                *asked = Some(match (modifiers.shift, modifiers.command || modifiers.ctrl, from) {
                    (true, _, Some(from)) => {
                        let (a, b) = (from.min(i), from.max(i));
                        Asked::PickThrough(list[a..=b].to_vec())
                    }
                    (false, true, _) => Asked::PickAlso(episode.clone(), show.clone()),
                    _ => Asked::Look(episode.clone(), show.clone()),
                });
            }
        }
        if let Some(show) = &chosen {
            let older = panel.feeds.get(&show.feed).map_or(0, |f| f.older);
            let filtered = panel.only_fetched || panel.only_starred || panel.only_open;
            if older > 0 && !filtered && list.len() >= total {
                ui.add_space(BLOCK_AIR);
                if panel.whole.as_deref() == Some(show.feed.as_str()) && reading {
                    say(ui, width, &format!("{}…", tr!("lese")), Role::Label, palette.faint, m);
                } else if device_action(ui, width, &tr!("ältere folgen laden ({})", older), m, palette) {
                    *asked = Some(Asked::Older(show.clone()));
                }
            }
        }
    });
    if below < MORE_WITHIN && list.len() < total {
        *asked = Some(Asked::More);
    }
}

fn episode_queue_column(app: &GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette, width: f32, asked: &mut Option<Asked>) {
    const SPENT_SHOWN: usize = 5;
    let panel = &app.podcasts;
    let queue: Vec<(Episode, Show)> = panel.kept.queue.iter().map(crate::podcast::Fetched::episode).collect();
    let spent: Vec<(Episode, Show)> = panel.kept.spent.iter().map(crate::podcast::Fetched::episode).collect();
    let clear = (!queue.is_empty() || !spent.is_empty()).then_some(tr!("leeren"));
    let count = if queue.is_empty() { String::new() } else { queue.len().to_string() };
    if still_heading_with(ui, tr!("warteschlange"), &count, clear, m, palette).beside_clicked() {
        *asked = Some(Asked::ClearQueue);
    }
    ui.add_space(HEAD_AIR);
    let area = ui.max_rect();
    let carrying = matches!(app.dragging, Some(crate::gui::Dragged::Episodes(_) | crate::gui::Dragged::QueuedEpisode(_)));
    let pointer = ui.ctx().pointer_latest_pos();
    let mut drop_at: Option<usize> = None;
    let mut drop_line: Option<(f32, f32, f32)> = None;
    scrolled(ui, "podcast-queue", palette, |ui| {
        let playing = panel.playing.as_ref().filter(|_| app.playing_episode()).and_then(|(id, _)| {
            let (episode, show) = app.player.current_track().map(|t| (t.title.clone(), t.album.clone()))?;
            Some((id.clone(), episode, show))
        });
        let followed_id = egui::Id::unique("podcast-queue-followed");
        let now_id = playing.as_ref().map(|(id, ..)| id.clone());
        let carry_along = now_id.is_some() && ui.data(|d| d.get_temp::<Option<String>>(followed_id)) != Some(now_id.clone());
        if carry_along {
            ui.data_mut(|d| d.insert_temp(followed_id, now_id.clone()));
        }
        let keep_at_top = spent.len().saturating_sub(SPENT_SHOWN);
        let in_picked = |id: &str| panel.focus == 3 && panel.picked.iter().any(|(e, _)| e.id == id);
        let looked = panel.queue_cursor.clone().filter(|_| panel.focus == 3);
        for (i, (episode, show)) in spent.iter().enumerate() {
            let ahead = row_ahead(ui, width, m);
            let over = super::pointer_rests_on(ui, ahead);
            let word = if over { tr!("entfernen") } else { "" };
            let figure = if word.is_empty() { episode.seconds.map(text::duration).unwrap_or_default() } else { word.to_string() };
            let here = in_picked(&episode.id) || (panel.picked.is_empty() || panel.focus != 3) && looked.as_deref() == Some(episode.id.as_str());
            let state = RowState { selected: here, focused: here && panel.focus == 3, offering: over, spent: true, ..Default::default() };
            let response = name_row(ui, egui::Id::unique(("podcast-spent", i)), width, &format!("{} · {}", show.title, episode.title), &figure, Role::Label, state, m, palette);
            if panel.focus == 3 && panel.follow.borrow().as_deref() == Some(episode.id.as_str()) {
                response.scroll_to_me(Some(egui::Align::Center));
                panel.follow.replace(None);
            }
            point_at_word(ui, &response, word, over, m, palette);
            if carry_along && i == keep_at_top {
                ui.scroll_to_rect(response.rect, Some(egui::Align::TOP));
            }
            if response.double_clicked() {
                *asked = Some(Asked::PlayQueued(episode.clone(), show.clone()));
            } else if response.clicked() {
                *asked = Some(if over && on_the_word(ui, &response, word, m, palette) { Asked::Unspend(episode.id.clone()) } else { Asked::LookQueued(episode.clone(), show.clone()) });
            }
        }
        if let Some((id, title, show)) = &playing {
            let here = in_picked(id) || ((panel.picked.is_empty() || panel.focus != 3) && looked.as_deref() == Some(id.as_str()));
            let state = RowState { playing: true, selected: here, focused: here && panel.focus == 3, ..Default::default() };
            let response = name_row(ui, egui::Id::unique("podcast-queue-now"), width, &format!("{show} · {title}"), "", Role::Label, state, m, palette);
            if panel.focus == 3 && panel.follow.borrow().as_deref() == Some(id.as_str()) {
                response.scroll_to_me(Some(egui::Align::Center));
                panel.follow.replace(None);
            }
            if carry_along && spent.is_empty() {
                ui.scroll_to_rect(response.rect, Some(egui::Align::TOP));
            }
        }
        if queue.is_empty() {
            if playing.is_some() {
                ui.add_space(BLOCK_AIR);
            }
            prose(ui, width, tr!("nichts eingereiht — „in die warteschlange“ unter einer folge, oder sie hierher ziehen"), Role::Label, palette.faint, m);
            return;
        }
        let looked = panel.queue_cursor.clone().filter(|_| panel.focus == 3);
        for (i, (episode, show)) in queue.iter().enumerate() {
            let ahead = row_ahead(ui, width, m);
            let over = super::pointer_rests_on(ui, ahead);
            let word = if over { tr!("entfernen") } else { "" };
            let coming = panel.fetching.as_ref().is_some_and(|(e, ..)| e.id == episode.id);
            let figure = if !word.is_empty() {
                word.to_string()
            } else if coming {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(400));
                format!("{}{}", tr!("lädt"), ".".repeat(1 + (ui.input(|i| i.time) / 0.4) as usize % 3))
            } else {
                episode.seconds.map(text::duration).unwrap_or_default()
            };
            let here = in_picked(&episode.id) || (panel.picked.is_empty() || panel.focus != 3) && looked.as_deref() == Some(episode.id.as_str());
            let state = RowState { selected: here, focused: here && panel.focus == 3, offering: over, muted: !here, ..Default::default() };
            let response = name_row(ui, egui::Id::unique(("podcast-queued", i)), width, &format!("{} · {}", show.title, episode.title), &figure, Role::Label, state, m, palette);
            if panel.focus == 3 && panel.follow.borrow().as_deref() == Some(episode.id.as_str()) {
                response.scroll_to_me(Some(egui::Align::Center));
                panel.follow.replace(None);
            }
            point_at_word(ui, &response, word, over, m, palette);
            if carrying {
                if let Some(at) = pointer {
                    let rect = response.rect;
                    if rect.x_range().contains(at.x) && rect.y_range().contains(at.y) {
                        let below = at.y > rect.center().y;
                        drop_at = Some(i + usize::from(below));
                        drop_line = Some((if below { rect.bottom() } else { rect.top() }, rect.left(), rect.width()));
                    } else if i + 1 == queue.len() && at.y > rect.bottom() && rect.x_range().contains(at.x) {
                        drop_at = Some(queue.len());
                        drop_line = Some((rect.bottom(), rect.left(), rect.width()));
                    }
                }
            }
            if response.drag_started() {
                *asked = Some(Asked::LiftQueued(i));
            } else if response.double_clicked() {
                *asked = Some(Asked::PlayQueued(episode.clone(), show.clone()));
            } else if response.clicked() {
                *asked = Some(if over && on_the_word(ui, &response, word, m, palette) { Asked::Queue(episode.clone(), show.clone()) } else { Asked::LookQueued(episode.clone(), show.clone()) });
            }
        }
        if let Some((y, left, wide)) = drop_line {
            ui.painter().rect_filled(Rect::from_min_size(egui::pos2(left, y.round() - 1.0), Vec2::new(wide, 2.0)), 0.0, palette.ink);
        }
    });
    if carrying && drop_at.is_none() && panel.kept.queue.is_empty() && pointer.is_some_and(|at| area.contains(at)) {
        drop_at = Some(0);
    }
    if ui.input(|i| i.pointer.any_released()) {
        if let Some(at) = drop_at {
            *asked = Some(Asked::Drop(at));
        }
    }
}

fn episode_column(app: &GuiApp, ui: &mut Ui, m: Metrics, palette: &Palette, width: f32, asked: &mut Option<Asked>) {
    let panel = &app.podcasts;
    still_heading(ui, tr!("folge"), "", m, palette);
    ui.add_space(HEAD_AIR);
    scrolled(ui, "podcast-episode", palette, |ui| {
        if panel.picked.len() > 1 {
            let n = panel.picked.len();
            paragraph(ui, width, &tr!("{} folgen", n), Role::ContentBold, palette.ink, m);
            let shows: std::collections::BTreeSet<&str> = panel.picked.iter().map(|(_, s)| s.title.as_str()).collect();
            say(ui, width, &shows.into_iter().collect::<Vec<_>>().join(" · "), Role::Label, palette.mid, m);
            ui.add_space(BLOCK_AIR);
            let all_archived = panel.picked.iter().all(|(e, _)| panel.kept.is_archived(&e.id));
            if device_action(ui, width, tr!("in die warteschlange"), m, palette) {
                *asked = Some(Asked::QueuePicked);
            }
            let word = if all_archived { tr!("nicht mehr archivieren") } else { tr!("archivieren") };
            if device_action(ui, width, word, m, palette) {
                *asked = Some(Asked::ArchivePicked);
            }
            if panel.picked.iter().any(|(e, _)| crate::podcast::cached(&e.url).is_file()) && device_action(ui, width, tr!("dateien löschen"), m, palette) {
                *asked = Some(Asked::ForgetPicked);
            }
            if device_action(ui, width, tr!("auswahl aufheben"), m, palette) {
                *asked = Some(Asked::Unpick);
            }
            return;
        }
        let Some((episode, show)) = panel.episode.clone() else {
            prose(ui, width, tr!("ein klick wählt eine folge, ein doppelklick spielt sie"), Role::Label, palette.faint, m);
            return;
        };
        let side = (width - m.marker_pad_x * 2.0).clamp(40.0, 176.0);
        let (area, _) = ui.allocate_exact_size(Vec2::new(width, side), Sense::hover());
        let picture = Rect::from_min_size(egui::pos2(content_rect(area, m).left(), area.top()), Vec2::splat(side));
        crate::podcast::fetch_episode_art(&episode.artwork);
        picture_at(app, ui, picture, palette, &crate::podcast::picture_for(&episode, &show), &show.title);
        ui.add_space(BLOCK_AIR * 2.0);
        paragraph(ui, width, &episode.title, Role::ContentBold, palette.ink, m);
        say(ui, width, &show.title, Role::Label, palette.mid, m);
        let mut facts: Vec<String> = Vec::new();
        if let Some(day) = day_of(&episode) {
            facts.push(day);
        }
        if let Some(secs) = episode.seconds {
            facts.push(text::duration(secs));
        }
        if crate::podcast::is_fetched(&episode.url) {
            facts.push(tr!("geladen").to_string());
        }
        if !facts.is_empty() {
            say(ui, width, &facts.join(" · "), Role::Label, palette.faint, m);
        }
        ui.add_space(BLOCK_AIR);
        let fetching = panel.fetching.as_ref().filter(|(e, ..)| e.id == episode.id);
        if let Some((_, _, _, (have, of))) = fetching {
            let how_far = if *of > 0 { format!("{} %", have * 100 / of) } else { format!("{:.1} mb", *have as f64 / 1_000_000.0) };
            say(ui, width, &tr!("lade … {}", how_far), Role::Label, palette.accent, m);
        } else if !app.playing_this_episode(&episode.id) {
            let left = panel.kept.left(&episode.id).filter(|l| !l.done && l.at > 0.0);
            let word = match left {
                Some(l) => tr!("weiter bei {}", text::duration(l.at as u64)),
                None => tr!("abspielen").to_string(),
            };
            if device_action(ui, width, &word, m, palette) {
                *asked = Some(Asked::Play(episode.clone(), show.clone()));
            }
            let queued = panel.kept.is_queued(&episode.id);
            let word = if queued { tr!("aus der warteschlange") } else { tr!("in die warteschlange") };
            if device_action(ui, width, word, m, palette) {
                *asked = Some(Asked::Queue(episode.clone(), show.clone()));
            }
            if crate::podcast::is_fetched(&episode.url) && device_action(ui, width, tr!("datei löschen"), m, palette) {
                *asked = Some(Asked::Forget(episode.clone()));
            }
        }
        let starred = panel.kept.is_starred(&episode.id);
        if device_action(ui, width, if starred { tr!("nicht mehr merken") } else { tr!("merken") }, m, palette) {
            *asked = Some(Asked::Star(episode.clone(), show.clone()));
        }
        let heard = panel.kept.left(&episode.id).is_some_and(|l| l.done);
        if device_action(ui, width, if heard { tr!("als ungehört markieren") } else { tr!("als gehört markieren") }, m, palette) {
            *asked = Some(Asked::Heard(episode.clone(), !heard));
        }
        if device_action(ui, width, tr!("teilen"), m, palette) {
            *asked = Some(Asked::Share(episode.clone(), show.clone()));
        }
        if app.playing_this_episode(&episode.id) && device_action(ui, width, &app.sleep_said(), m, palette) {
            *asked = Some(Asked::Sleep);
        }
        let word = if panel.kept.is_archived(&episode.id) { tr!("nicht mehr archivieren") } else { tr!("archivieren") };
        if device_action(ui, width, word, m, palette) {
            *asked = Some(Asked::Archive(episode.clone()));
        }
        let chapters: Vec<Chapter> = panel.chapters.as_ref().filter(|(id, _)| *id == episode.id).map(|(_, c)| c.clone()).unwrap_or_default();
        if !chapters.is_empty() {
            ui.add_space(BLOCK_AIR * 2.0);
            say(ui, width, tr!("kapitel"), Role::Label, palette.faint, m);
            let now = app.playing_this_episode(&episode.id).then(|| app.player.position().as_secs_f64());
            let on_rule = now.and_then(|_| pointed_chapter(ui.ctx(), ON_THE_RULE));
            let mut here = None;
            for (i, chapter) in chapters.iter().enumerate() {
                let next = chapters.get(i + 1).map_or(f64::INFINITY, |c| c.start);
                let state = RowState { playing: now.is_some_and(|t| t >= chapter.start && t < next), pointed: on_rule == Some(i), ..Default::default() };
                let response = chapter_row(ui, egui::Id::unique(("podcast-chapter", i)), width, i + 1, &chapter.title, &text::duration(chapter.start as u64), state, m, palette);
                if response.hovered() {
                    here = Some(i);
                }
                if response.clicked() {
                    *asked = Some(Asked::Chapter(episode.clone(), show.clone(), chapter.start));
                }
            }
            if now.is_some() {
                point_at_chapter(ui.ctx(), IN_THE_LIST, here);
            }
        } else if panel.chapters_reading.as_ref().is_some_and(|(id, _)| *id == episode.id) {
            ui.add_space(BLOCK_AIR);
            say(ui, width, &format!("{}…", tr!("kapitel")), Role::Label, palette.faint, m);
        }
        if !episode.about.is_empty() {
            ui.add_space(BLOCK_AIR * 2.0);
            prose(ui, width, &episode.about, Role::Label, palette.faint, m);
        }
        links_block(ui, width, &episode.links, m, palette);
        let inner = (width - m.marker_pad_x * 2.0).max(40.0);
        for url in &episode.images {
            crate::podcast::fetch_episode_art(url);
            let path = crate::podcast::episode_art_path(url);
            if path.is_file() {
                ui.add_space(BLOCK_AIR * 2.0);
                ui.horizontal(|ui| {
                    ui.add_space(m.marker_pad_x);
                    wide_picture(app, ui, inner, palette, &path);
                });
            }
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn chapter_row(ui: &mut Ui, id: egui::Id, width: f32, number: usize, title: &str, start: &str, state: RowState, m: Metrics, palette: &Palette) -> Response {
    use crate::gui::columns::TrackColumn;
    let (rect, response) = allocate_row(ui, width, m);
    if !ui.is_rect_visible(rect) {
        return response;
    }
    paint_marker(ui, rect, state, m, palette);
    paint_hover(ui, rect, &response, state, m, palette);
    let hovered = response.hovered() || state.pointed;
    let ground = behind(ui, state, hovered, palette);
    let content = content_rect(rect, m);
    let secondary = state.secondary(palette);

    let column = TrackColumn::Number;
    let number_colour = if state.pointed { palette.ink } else { secondary };
    let g = text::line_within(ui, &number.to_string(), column.role(), number_colour, column.width());
    let x = if column.right_aligned() { content.left() + column.width() - g.size().x } else { content.left() };
    let y = text::beside_content(ui, &g, column.role(), rect.top(), m.row);
    ui.painter().galley(egui::pos2(x, y), g, palette.ink);
    let left = content.left() + column.width() + crate::gui::columns::number_gap(CELL_GAP);

    let g = text::line(ui, start, Role::Label, secondary);
    let start_width = g.size().x;
    let y = text::beside_content(ui, &g, Role::Label, rect.top(), m.row);
    ui.painter().galley(egui::pos2(content.right() - start_width, y), g, palette.ink);

    let cell = Rect::from_min_max(egui::pos2(left, content.top()), egui::pos2((content.right() - start_width - 12.0).max(left + 1.0), content.bottom()));
    let lead = lead_arrow(ui, id, cell, text::row_line(ui, m.row, Role::Content), state.playing, state.primary(palette));
    cell_text(ui, Rect::from_min_max(egui::pos2(cell.left() + lead, cell.top()), cell.max), id, title, state.role(), state.primary(palette), state.selected || hovered, ground, m);
    response
}

const ARCHIVED_VEIL: f32 = 0.6;

const NEW_DOT: f32 = 2.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Age {
    Begun,
    Today,
    ThisWeek,
    Older,
}

impl Age {
    fn of(episode: &Episode, today: chrono::NaiveDate) -> Age {
        let Some(day) = local_day(episode) else {
            return Age::Older;
        };
        if day >= today {
            Age::Today
        } else if day > today - chrono::Days::new(7) {
            Age::ThisWeek
        } else {
            Age::Older
        }
    }

    fn said(self) -> &'static str {
        match self {
            Age::Begun => tr!("angefangen"),
            Age::Today => tr!("heute"),
            Age::ThisWeek => tr!("diese woche"),
            Age::Older => tr!("älter"),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn episode_row(
    app: &GuiApp,
    ui: &mut Ui,
    id: egui::Id,
    width: f32,
    title: &str,
    facts: &str,
    heard: &str,
    starred: bool,
    show: &Show,
    art: &std::path::Path,
    new: bool,
    fetched: bool,
    offer: &str,
    state: RowState,
    m: Metrics,
    palette: &Palette,
) -> (Response, bool) {
    let pad = m.marker_pad_x;
    let small = (m.row * 0.72).round();
    let side = m.row + small * 2.0;
    let height = side + pad * 2.0 + m.marker_inset_y;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click_and_drag());
    let response = response.on_hover_cursor(egui::CursorIcon::Default);
    let plate = Rect::from_min_size(rect.min, Vec2::new(rect.width(), height - m.marker_inset_y));
    let active = response.hovered();
    let ground = if state.selected && state.focused {
        marker_colour(palette)
    } else if active {
        hover_colour(ui, palette)
    } else {
        palette.ground
    };
    if ground != palette.ground {
        ui.painter().rect_filled(plate, MARKER_RADIUS, ground);
    }
    let content = content_rect(rect, m);
    let picture = Rect::from_min_size(egui::pos2(content.left(), rect.top() + pad), Vec2::splat(side));
    picture_at(app, ui, picture, palette, art, &show.title);
    if state.spent {
        ui.painter().rect_filled(picture.expand(0.5), MARKER_RADIUS, ground.gamma_multiply(ARCHIVED_VEIL));
    }
    let left = picture.right() + ALBUM_HEAD_GAP;
    let mut room = (content.right() - left).max(1.0);
    let mut on_word = false;
    if active && !offer.is_empty() {
        let probe = text::line(ui, offer, Role::Label, palette.mid);
        let word_width = probe.size().x;
        let cell = Rect::from_min_size(egui::pos2(content.right() - word_width, picture.top()), Vec2::new(word_width, m.row));
        on_word = response.hover_pos().is_some_and(|at| at.x >= cell.left() - 6.0 && at.y <= cell.bottom());
        if on_word {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let colour = if on_word { palette.ink } else { palette.mid };
        let g = text::line(ui, offer, Role::Label, colour);
        let y = text::beside_content(ui, &g, Role::Label, cell.top(), m.row);
        ui.painter().galley(egui::pos2(cell.left(), y), g, colour);
        room = (room - word_width - 12.0).max(1.0);
    }
    if new {
        let y = picture.top() + m.row / 2.0;
        ui.painter().circle_filled(egui::pos2(picture.right() + ALBUM_HEAD_GAP / 2.0, y), NEW_DOT, palette.accent);
    }
    let (title_colour, facts_colour) = if state.playing {
        (palette.accent, palette.mid)
    } else if state.spent {
        (palette.faint, palette.faint)
    } else if state.muted {
        (palette.mid, palette.faint)
    } else {
        (palette.ink, palette.mid)
    };
    let facts_room = (content.right() - left).max(1.0);
    let title_cell = Rect::from_min_size(egui::pos2(left, picture.top()), Vec2::new(room, m.row));
    let lead = lead_arrow(ui, id, title_cell, text::row_line(ui, m.row, Role::Content), state.playing, title_colour);
    let title_cell = Rect::from_min_max(egui::pos2(title_cell.left() + lead, title_cell.top()), title_cell.max);
    cell_text(ui, title_cell, id.with(0), title, Role::ContentBold, title_colour, active, ground, m);
    let facts_cell = Rect::from_min_size(egui::pos2(left, picture.top() + m.row), Vec2::new(facts_room, small));
    cell_text(ui, facts_cell, id.with(1), facts, Role::Label, facts_colour, active, ground, m);
    let mut x = left;
    let third = picture.top() + m.row + small;
    for (on, word) in [(starred, tr!("gemerkt"))] {
        if !on {
            continue;
        }
        let y = text::on_line(&text::line(ui, word, Role::Label, palette.accent), third, text::row_line(ui, small, Role::Content));
        let g = text::line(ui, word, Role::Label, palette.accent);
        let wide = g.size().x;
        ui.painter().galley(egui::pos2(x, y), g, palette.accent);
        let dot = text::line(ui, " · ", Role::Label, facts_colour);
        let dot_wide = dot.size().x;
        ui.painter().galley(egui::pos2(x + wide, y), dot, facts_colour);
        x += wide + dot_wide;
    }
    if fetched {
        let g = text::line(ui, &format!("{} · ", tr!("geladen")), Role::Label, palette.accent);
        let wide = g.size().x;
        let y = text::on_line(&g, third, text::row_line(ui, small, Role::Content));
        let word = text::line(ui, tr!("geladen"), Role::Label, palette.accent);
        ui.painter().galley(egui::pos2(x, y), word, palette.accent);
        let dot_x = x + text::line(ui, tr!("geladen"), Role::Label, palette.accent).size().x;
        let dot = text::line(ui, " · ", Role::Label, facts_colour);
        ui.painter().galley(egui::pos2(dot_x, y), dot, facts_colour);
        x += wide;
    }
    let cell = Rect::from_min_max(egui::pos2(x, third), egui::pos2(left + facts_room, third + small));
    cell_text(ui, cell, id.with(2), heard, Role::Label, facts_colour, active, ground, m);
    let word_clicked = on_word && response.clicked();
    (response, word_clicked)
}

fn day_of(episode: &Episode) -> Option<String> {
    Some(local_day(episode)?.format("%d.%m.%Y").to_string())
}

fn local_day(episode: &Episode) -> Option<chrono::NaiveDate> {
    let at = crate::podcast::when(episode);
    (at > 0).then(|| chrono::DateTime::from_timestamp(at, 0)).flatten().map(|d| d.with_timezone(&chrono::Local).date_naive())
}
