use crate::gui::*;
use crate::podcast::{Episode, Show};

#[derive(Clone, PartialEq)]
enum ShowRow {
    All,
    Open,
    Fetched,
    Starred,
    Podcast(Show),
}

impl GuiApp {
    fn podcast_rows(&self) -> Vec<ShowRow> {
        if let Some(found) = &self.podcasts.found {
            return found.iter().cloned().map(ShowRow::Podcast).collect();
        }
        let mut rows = vec![ShowRow::All, ShowRow::Open, ShowRow::Fetched];
        if !self.podcasts.kept.starred.is_empty() || self.podcasts.only_starred {
            rows.push(ShowRow::Starred);
        }
        rows.extend(self.shows_in_order().into_iter().map(ShowRow::Podcast));
        rows
    }

    fn podcast_row_now(&self, rows: &[ShowRow]) -> usize {
        let panel = &self.podcasts;
        let now = if panel.only_open {
            Some(ShowRow::Open)
        } else if panel.only_fetched {
            Some(ShowRow::Fetched)
        } else if panel.only_starred {
            Some(ShowRow::Starred)
        } else {
            panel.chosen.clone().map(ShowRow::Podcast)
        };
        match now {
            Some(row) => rows.iter().position(|r| *r == row).unwrap_or(0),
            None if panel.found.is_some() => 0,
            None => 0,
        }
    }

    fn choose_podcast_row(&mut self, row: ShowRow) {
        match row {
            ShowRow::All => self.choose_podcast(None),
            ShowRow::Open => self.choose_open(),
            ShowRow::Fetched => self.choose_fetched(),
            ShowRow::Starred => self.choose_starred(),
            ShowRow::Podcast(show) => self.choose_podcast(Some(show)),
        }
    }

    fn podcast_list_here(&self) -> Vec<(Episode, Show)> {
        match self.podcasts.focus {
            3 => self.podcast_queue_rows(),
            _ => self.podcasts.episodes(self.config.ui.podcast_show_archived).0,
        }
    }

    fn podcast_queue_rows(&self) -> Vec<(Episode, Show)> {
        let kept = &self.podcasts.kept;
        let mut rows: Vec<(Episode, Show)> = kept.spent.iter().map(crate::podcast::Fetched::episode).collect();
        if self.playing_episode() {
            if let Some(now) = self.podcasts.playing_from.clone() {
                rows.push(now);
            }
        }
        rows.extend(kept.queue.iter().map(crate::podcast::Fetched::episode));
        rows
    }

    fn take_out_of_episode_queue(&mut self, id: &str) {
        let before = self.podcast_queue_rows();
        let Some(at) = before.iter().position(|(e, _)| e.id == id) else { return };
        if self.podcasts.kept.spent.iter().any(|s| s.id == id) {
            self.forget_spent(id);
        } else if let Some(row) = self.podcasts.kept.queue.iter().find(|q| q.id == id) {
            let (episode, show) = row.episode();
            self.queue_episode(&episode, &show);
        } else {
            return;
        }
        let after = self.podcast_queue_rows();
        if let Some((episode, show)) = after.get(at.min(after.len().saturating_sub(1))).cloned() {
            self.podcast_cursor_to(episode, show);
        } else {
            self.podcasts.queue_cursor = None;
        }
    }

    fn take_all_out_of_episode_queue(&mut self, ids: &[String]) {
        let before = self.podcast_queue_rows();
        let first = before.iter().position(|(e, _)| ids.contains(&e.id));
        let mut kept = crate::podcast::Kept::read();
        kept.spent.retain(|s| !ids.contains(&s.id));
        kept.queue.retain(|q| !ids.contains(&q.id));
        kept.write();
        self.podcasts.kept = kept;
        let after = self.podcast_queue_rows();
        match first.and_then(|at| after.get(at.min(after.len().saturating_sub(1))).cloned()) {
            Some((episode, show)) => self.podcast_cursor_to(episode, show),
            None => self.podcasts.queue_cursor = None,
        }
        self.set_notice(tr!("{} aus der warteschlange", before.len().saturating_sub(after.len())));
    }

    fn podcast_cursor_id(&self) -> Option<String> {
        if self.podcasts.focus == 3 {
            self.podcasts.queue_cursor.clone()
        } else {
            self.podcasts.episode.as_ref().map(|(e, _)| e.id.clone())
        }
    }

    fn podcast_cursor_to(&mut self, episode: Episode, show: Show) {
        self.podcasts.follow.replace(Some(episode.id.clone()));
        if self.podcasts.focus == 3 {
            self.podcasts.queue_cursor = Some(episode.id);
        } else {
            self.choose_episode(episode, show);
        }
    }

    fn podcast_cursor_row(&self) -> Option<(Episode, Show)> {
        let id = self.podcast_cursor_id()?;
        if self.podcasts.focus == 3 {
            self.podcast_queue_rows().into_iter().find(|(e, _)| e.id == id)
        } else {
            self.podcasts.episode.clone()
        }
    }

    fn podcast_looked_at(&self, list: &[(Episode, Show)]) -> Option<usize> {
        let id = self.podcast_cursor_id()?;
        list.iter().position(|(e, _)| e.id == id)
    }

    pub(in crate::gui) fn podcast_step(&mut self, by: i32) {
        if self.podcasts.focus == 0 {
            let rows = self.podcast_rows();
            if rows.is_empty() {
                return;
            }
            let now = self.podcast_row_now(&rows) as i64;
            let to = (now + i64::from(by)).clamp(0, rows.len() as i64 - 1) as usize;
            if to as i64 != now {
                if let ShowRow::Podcast(show) = &rows[to] {
                    self.podcasts.follow.replace(Some(show.feed.clone()));
                }
                self.choose_podcast_row(rows[to].clone());
            }
            return;
        }
        let list = self.podcast_list_here();
        if list.is_empty() {
            return;
        }
        let to = match self.podcast_looked_at(&list) {
            Some(at) => (at as i64 + i64::from(by)).clamp(0, list.len() as i64 - 1) as usize,
            None if by < 0 => list.len() - 1,
            None => 0,
        };
        self.podcasts.picked.clear();
        let (episode, show) = list[to].clone();
        self.podcast_cursor_to(episode, show);
        self.podcast_reach(to, list.len());
    }

    fn podcast_reach(&mut self, at: usize, len: usize) {
        if self.podcasts.focus == 1 && at + 3 >= len {
            let (_, total) = self.podcasts.episodes(self.config.ui.podcast_show_archived);
            if len < total {
                self.podcasts.more += crate::gui::app::podcasts::EPISODES_PAGE;
            }
        }
    }

    fn podcast_pick_by(&mut self, by: i32) {
        if self.podcasts.focus != 1 && self.podcasts.focus != 3 {
            return;
        }
        let list = self.podcast_list_here();
        let Some(from) = self.podcast_looked_at(&list) else {
            self.podcast_step(by);
            return;
        };
        let ends: Vec<usize> = self.podcasts.picked.iter().filter_map(|(e, _)| list.iter().position(|(l, _)| l.id == e.id)).collect();
        let moving = match (ends.iter().min(), ends.iter().max()) {
            (Some(&lo), Some(&hi)) => {
                if lo == from {
                    hi
                } else {
                    lo
                }
            }
            _ => from,
        };
        let to = (moving as i64 + i64::from(by)).clamp(0, list.len() as i64 - 1) as usize;
        let (a, b) = (from.min(to), from.max(to));
        self.podcasts.follow.replace(Some(list[to].0.id.clone()));
        self.podcasts.picked = list[a..=b].to_vec();
        self.podcast_reach(to, list.len());
    }

    fn podcast_columns(&self) -> Vec<usize> {
        let mut columns = vec![0, 1];
        if self.config.ui.podcast_queue && !self.podcasts.info_open && !self.log_open && !self.eq_open {
            columns.push(3);
        }
        columns
    }

    fn podcast_focus_by(&mut self, by: i32) {
        let columns = self.podcast_columns();
        let now = columns.iter().position(|&c| c == self.podcasts.focus).unwrap_or(0) as i64;
        let to = (now + i64::from(by)).clamp(0, columns.len() as i64 - 1) as usize;
        if self.podcasts.focus != columns[to] {
            self.podcasts.picked.clear();
        }
        self.podcasts.focus = columns[to];
        if self.podcasts.focus != 0 {
            let list = self.podcast_list_here();
            if self.podcast_looked_at(&list).is_none() {
                let first = if self.podcasts.focus == 3 {
                    let spent = self.podcasts.kept.spent.len();
                    list.get(spent).or(list.last()).cloned()
                } else {
                    list.first().cloned()
                };
                if let Some((episode, show)) = first {
                    self.podcasts.picked.clear();
                    self.podcast_cursor_to(episode, show);
                }
            }
        }
    }

    pub(in crate::gui) fn handle_podcast_keys(&mut self, ctx: &egui::Context) {
        use egui::{Key, Modifiers};
        const NONE: Modifiers = Modifiers::NONE;
        const SHIFT: Modifiers = Modifiers::SHIFT;
        let taken = |m: Modifiers, k: Key| ctx.input_mut(|i| i.count_and_consume_key(m, k));
        let pressed = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_key(m, k));

        if !self.podcasts.picked.is_empty() && pressed(NONE, Key::Escape) {
            self.podcasts.picked.clear();
            return;
        }

        if self.podcasts.focus == 3 {
            let looked_id = self.podcasts.queue_cursor.clone();
            let later = taken(SHIFT, Key::J) as i32 - taken(SHIFT, Key::K) as i32;
            let wider = taken(SHIFT, Key::ArrowDown) as i32 - taken(SHIFT, Key::ArrowUp) as i32;
            if later != 0 {
                if self.podcasts.picked.len() > 1 {
                    let ids: Vec<String> = self.podcasts.picked.iter().map(|(e, _)| e.id.clone()).collect();
                    self.carry_queued_episodes(&ids, later);
                } else if let Some(id) = &looked_id {
                    self.carry_queued_episode(id, later);
                    self.podcasts.follow.replace(Some(id.clone()));
                }
            }
            if wider != 0 {
                self.podcast_pick_by(wider);
            }
            if pressed(SHIFT, Key::D) {
                let now = ctx.input(|i| i.time);
                match self.clear_asked_at {
                    Some(then) if now - then < QUIT_WINDOW_SECS => {
                        self.clear_asked_at = None;
                        self.clear_episode_queue();
                    }
                    _ => {
                        self.clear_asked_at = Some(now);
                        self.set_status(tr!("noch einmal shift + d zum leeren"));
                    }
                }
            }
            if pressed(NONE, Key::Backspace) || pressed(NONE, Key::Delete) || pressed(NONE, Key::D) || pressed(Modifiers::COMMAND, Key::Backspace) {
                if self.podcasts.picked.len() > 1 {
                    let picked = std::mem::take(&mut self.podcasts.picked);
                    let ids: Vec<String> = picked.iter().map(|(e, _)| e.id.clone()).collect();
                    self.take_all_out_of_episode_queue(&ids);
                } else if let Some(id) = looked_id {
                    self.take_out_of_episode_queue(&id);
                }
            }
        }

        let wider = taken(SHIFT, Key::ArrowDown) + taken(SHIFT, Key::J);
        let narrower = taken(SHIFT, Key::ArrowUp) + taken(SHIFT, Key::K);
        if wider != narrower {
            self.podcast_pick_by(wider as i32 - narrower as i32);
        }
        if pressed(SHIFT, Key::G) {
            self.g_pressed_at = None;
            self.podcast_step(i32::MAX / 2);
        } else if pressed(NONE, Key::G) {
            let now = ctx.input(|i| i.time);
            match self.g_pressed_at.take() {
                Some(then) if now - then < GG_WINDOW_SECS => self.podcast_step(i32::MIN / 2),
                _ => self.g_pressed_at = Some(now),
            }
        }
        let down = taken(NONE, Key::ArrowDown) + taken(NONE, Key::J);
        let up = taken(NONE, Key::ArrowUp) + taken(NONE, Key::K);
        if down != up {
            self.podcast_step(down as i32 - up as i32);
        }
        let right = taken(NONE, Key::ArrowRight) + taken(NONE, Key::L);
        let left = taken(NONE, Key::ArrowLeft) + taken(NONE, Key::H);
        if right != left {
            let by = right as i32 - left as i32;
            self.podcast_focus_by(by);
            crate::gui::library::step_podcast_strip(ctx, by as isize);
        }
        if pressed(NONE, Key::Tab) {
            self.podcast_focus_by(1);
        }

        if self.podcasts.focus == 3 && pressed(Modifiers::COMMAND, Key::A) {
            self.podcasts.picked = self.podcast_queue_rows();
        }
        if pressed(Modifiers::COMMAND, Key::A) {
            if self.podcasts.focus == 0 {
                self.podcasts.focus = 1;
            }
            let list = self.podcasts.listed(self.config.ui.podcast_show_archived);
            self.podcasts.picked = list.into_iter().map(|(e, s)| (e.into_owned(), s.into_owned())).collect();
            if self.podcasts.episode.is_none() {
                if let Some((episode, show)) = self.podcasts.picked.first().cloned() {
                    self.choose_episode(episode, show);
                }
            }
        }

        let looked = self.podcast_cursor_row();
        let several = self.podcasts.picked.len() > 1;
        if pressed(NONE, Key::Enter) {
            match (self.podcasts.focus, looked.clone()) {
                (0, _) => self.podcast_focus_by(1),
                (3, Some((episode, _))) if self.playing_this_episode(&episode.id) => {}
                (3, Some((episode, show))) => self.play_queued_episode(episode, show),
                (_, Some((episode, show))) => self.fetch_and_play(episode, show, None),
                _ => {}
            }
        }
        if self.podcasts.focus == 0 {
            return;
        }
        if pressed(NONE, Key::A) || pressed(SHIFT, Key::A) {
            if several {
                self.queue_picked();
            } else if let Some((episode, show)) = &looked {
                self.queue_episode(episode, show);
            }
        }
        if pressed(NONE, Key::Backspace) || pressed(NONE, Key::Delete) || pressed(NONE, Key::D) || pressed(Modifiers::COMMAND, Key::Backspace) {
            if several {
                self.archive_picked();
            } else if let Some((episode, _)) = &looked {
                self.archive_episode(episode);
            }
        }
    }
}
