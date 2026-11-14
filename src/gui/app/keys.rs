use crate::gui::*;

impl GuiApp {
    pub(in crate::gui) fn handle_keys(&mut self, ctx: &egui::Context) -> Result<()> {
        use egui::{Key, Modifiers};

        let taken = |ctx: &egui::Context, m: Modifiers, k: Key| -> usize {
            ctx.input_mut(|i| i.count_and_consume_key(m, k))
        };
        let pressed = |ctx: &egui::Context, m: Modifiers, k: Key| -> bool {
            ctx.input_mut(|i| i.consume_key(m, k))
        };

        const NONE: Modifiers = Modifiers::NONE;
        const SHIFT: Modifiers = Modifiers::SHIFT;

        if self.reeling() {
            if pressed(ctx, NONE, Key::Escape)
                || pressed(ctx, Modifiers::ALT | SHIFT, Key::G)
                || pressed(ctx, Modifiers::ALT | SHIFT, Key::M)
            {
                self.toggle_reel(app::Film::Gif);
            }
            return Ok(());
        }

        if self.row_menu.is_some() {
            let down = taken(ctx, NONE, Key::ArrowDown) + taken(ctx, NONE, Key::J) + taken(ctx, NONE, Key::Tab);
            let up = taken(ctx, NONE, Key::ArrowUp) + taken(ctx, NONE, Key::K) + taken(ctx, SHIFT, Key::Tab);
            self.row_menu_step += down as i32 - up as i32;
            let right = taken(ctx, NONE, Key::ArrowRight) + taken(ctx, NONE, Key::L);
            let left = taken(ctx, NONE, Key::ArrowLeft) + taken(ctx, NONE, Key::H);
            self.row_menu_side += right as i32 - left as i32;
            if pressed(ctx, NONE, Key::Enter) {
                self.row_menu_enter = true;
            }
            if pressed(ctx, NONE, Key::Escape) || pressed(ctx, NONE, Key::V) || pressed(ctx, NONE, Key::Menu) || pressed(ctx, SHIFT, Key::F10) {
                self.row_menu = None;
            }
            return Ok(());
        }
        self.row_menu_focus = None;
        self.row_menu_step = 0;
        self.row_menu_enter = false;
        self.row_menu_side = 0;

        if self.naming_device.is_some() && !(self.device_open && self.attached.is_some()) {
            self.naming_device = None;
        }
        if self.naming_device.is_some() && !self.menu_open {
            self.handle_naming_keys(ctx);
            return Ok(());
        }
        if self.menu_open {
            if self.lastfm_keys.is_some() {
                self.handle_lastfm_keys(ctx);
                return Ok(());
            }
            if self.seed_entry.is_some() || self.seed_hex.is_some() {
                self.handle_seed_keys(ctx);
                return Ok(());
            }
            if pressed(ctx, NONE, Key::Escape) || pressed(ctx, NONE, Key::M) {
                self.menu_open = false;
                return Ok(());
            }
            use crate::gui::menu::{step_tab, with_menu_keys, Press};
            if pressed(ctx, SHIFT, Key::H) || pressed(ctx, SHIFT, Key::ArrowLeft) || pressed(ctx, NONE, Key::OpenBracket) {
                step_tab(self, -1);
            }
            if pressed(ctx, SHIFT, Key::L) || pressed(ctx, SHIFT, Key::ArrowRight) || pressed(ctx, NONE, Key::CloseBracket) {
                step_tab(self, 1);
            }
            let digits = [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9];
            for (n, key) in digits.into_iter().enumerate() {
                if pressed(ctx, NONE, key) {
                    if let Some(&t) = crate::gui::menu::tabs_shown().get(n) {
                        self.menu_tab = t;
                    }
                }
            }
            let down = (taken(ctx, NONE, Key::J) + taken(ctx, NONE, Key::ArrowDown) + taken(ctx, NONE, Key::Tab)) as isize;
            let up = (taken(ctx, NONE, Key::K) + taken(ctx, NONE, Key::ArrowUp) + taken(ctx, SHIFT, Key::Tab)) as isize;
            let (h, l) = (taken(ctx, NONE, Key::H) as isize, taken(ctx, NONE, Key::L) as isize);
            let (arrow_left, arrow_right) =
                (taken(ctx, NONE, Key::ArrowLeft) as isize, taken(ctx, NONE, Key::ArrowRight) as isize);
            let mut by = down - up + 10 * taken(ctx, NONE, Key::PageDown) as isize - 10 * taken(ctx, NONE, Key::PageUp) as isize;
            if pressed(ctx, NONE, Key::Home) {
                by = isize::MIN / 2;
            }
            if pressed(ctx, NONE, Key::End) {
                by = isize::MAX / 2;
            }
            let act = pressed(ctx, NONE, Key::Enter) || pressed(ctx, NONE, Key::Space);
            if by == 0 && h == 0 && l == 0 && arrow_left == 0 && arrow_right == 0 && !act {
                return Ok(());
            }
            let mut tab_by = 0;
            with_menu_keys(ctx, |keys| {
                let last = keys.count.saturating_sub(1) as isize;
                let (left, right) = if keys.tabbed {
                    (h + arrow_left, l + arrow_right)
                } else {
                    tab_by = arrow_right - arrow_left;
                    (h, l)
                };
                if keys.tabbed && keys.on_tabs {
                    tab_by = down - up;
                    if right > 0 || act {
                        keys.on_tabs = false;
                        keys.cursor = Some(0);
                        keys.follow = true;
                    }
                } else if let Some(at) = keys.cursor {
                    if keys.on_slider && (left > 0 || right > 0) {
                        keys.press = if left > 0 { Press::Less } else { Press::More };
                    } else if keys.tabbed && left > 0 {
                        keys.on_tabs = true;
                        keys.cursor = None;
                    } else if by != 0 {
                        keys.cursor = Some((at as isize).saturating_add(by).clamp(0, last.max(0)) as usize);
                        keys.follow = true;
                    } else if right > 0 || act {
                        keys.press = Press::Act;
                    }
                } else if keys.tabbed && left > 0 {
                    keys.on_tabs = true;
                } else if by != 0 {
                    let from = if by > 0 { -1 } else { last + 1 };
                    keys.cursor = Some(from.saturating_add(by).clamp(0, last.max(0)) as usize);
                    keys.follow = true;
                } else if left > 0 || right > 0 || act {
                    keys.cursor = Some(0);
                    keys.follow = true;
                }
            });
            if tab_by != 0 {
                step_tab(self, tab_by);
            }
            ctx.request_repaint();
            return Ok(());
        }

        let typed_question = ctx.input(|i| {
            i.events
                .iter()
                .any(|e| matches!(e, egui::Event::Text(t) if t == "?"))
        });
        if self.keys_sheet_open {
            if pressed(ctx, NONE, Key::Escape) {
                if self.keys_query.is_empty() {
                    self.keys_sheet_open = false;
                } else {
                    self.keys_query.clear();
                }
                return Ok(());
            }
            if typed_question && self.keys_query.is_empty() {
                self.keys_sheet_open = false;
                return Ok(());
            }
            for _ in 0..ctx.input_mut(|i| i.count_and_consume_key(NONE, Key::Backspace)) {
                self.keys_query.pop();
            }
            let typed = crate::gui::typed(ctx, false);
            if !typed.is_empty() {
                self.keys_query.push_str(&typed);
                ctx.request_repaint();
            }
            const STEP: f32 = 3.0 * 21.0;
            let down = taken(ctx, NONE, Key::ArrowDown);
            let up = taken(ctx, NONE, Key::ArrowUp);
            if down != up {
                self.keys_sheet_scroll += (down as f32 - up as f32) * STEP;
                ctx.request_repaint();
            }
            return Ok(());
        }

        if self.editor.is_some() {
            return self.handle_edit_keys(ctx);
        }
        if self.artist_form.is_some() {
            self.handle_artist_form_keys(ctx);
            return Ok(());
        }

        if self.prompt.is_some() {
            return self.handle_prompt_keys(ctx);
        }

        if self.details.is_some() && pressed(ctx, NONE, Key::Escape) {
            self.close_details();
            return Ok(());
        }

        if self.transferring() && pressed(ctx, NONE, Key::Escape) {
            self.close_transfer();
            return Ok(());
        }

        if self.cover_pick.is_some() {
            if pressed(ctx, NONE, Key::Escape) {
                self.cover_pick = None;
                return Ok(());
            }
            let across = self.cover_pick.as_ref().map(|p| p.across).unwrap_or(3) as i32;
            let right = taken(ctx, NONE, Key::ArrowRight) + taken(ctx, NONE, Key::L);
            let left = taken(ctx, NONE, Key::ArrowLeft) + taken(ctx, NONE, Key::H);
            let down = taken(ctx, NONE, Key::ArrowDown) + taken(ctx, NONE, Key::J);
            let up = taken(ctx, NONE, Key::ArrowUp) + taken(ctx, NONE, Key::K);
            let step = right as i32 - left as i32 + (down as i32 - up as i32) * across;
            let take = pressed(ctx, NONE, Key::Enter);
            if let Some(picker) = self.cover_pick.as_mut() {
                if step != 0 {
                    picker.step(step);
                }
                if take {
                    picker.take_focused();
                }
            }
            return Ok(());
        }

        if self.filter.is_some() && self.filter_typing {
            return self.handle_filter_keys(ctx);
        }

        let typed_slash = ctx.input(|i| {
            i.events
                .iter()
                .any(|e| matches!(e, egui::Event::Text(t) if t == "/"))
        });
        if typed_slash || pressed(ctx, Modifiers::COMMAND, Key::F) {
            self.open_filter();
            return Ok(());
        }
        if typed_question {
            self.keys_sheet_open = true;
            self.keys_query.clear();
            return Ok(());
        }

        let snake = self.config.ui.field_spectrum
            && library::FieldPicture::from_name(&self.config.ui.field_picture) == library::FieldPicture::Snake;
        if snake && ctx.input(|i| i.key_down(Key::X)) {
            pressed(ctx, NONE, Key::X);
            for (key, way) in [(Key::ArrowUp, (0, -1)), (Key::ArrowDown, (0, 1)), (Key::ArrowLeft, (-1, 0)), (Key::ArrowRight, (1, 0))] {
                if pressed(ctx, NONE, key) {
                    self.field_art.steer(way);
                }
            }
            return Ok(());
        }
        self.field_art.let_go();

        if self.showing_podcasts() {
            self.handle_podcast_keys(ctx);
        }
        if self.showing_radio() {
            self.handle_radio_keys(ctx);
            self.focus = Column::Artists;
        }
        let music = !self.showing_radio() && !self.showing_podcasts();

        if pressed(ctx, NONE, Key::V) || pressed(ctx, NONE, Key::Menu) || pressed(ctx, SHIFT, Key::F10) {
            self.open_row_menu_by_key(ctx);
            return Ok(());
        }

        if pressed(ctx, NONE, Key::Escape) {
            let similar = self.radio.as_mut().and_then(|r| r.similar.take().map(|_| r.cursor = 0));
            if similar.is_none() {
                self.let_go();
            }
        }
        let marked = !self.unmarked;

        let forward = taken(ctx, SHIFT, Key::ArrowRight) + taken(ctx, SHIFT, Key::L);
        let backward = taken(ctx, SHIFT, Key::ArrowLeft) + taken(ctx, SHIFT, Key::H);
        if forward > 0 || backward > 0 {
            let delta = (forward as f64 - backward as f64) * SEEK_STEP_SECS;
            if !self.player.seek_to(
                self.player.position().as_secs_f64() + delta,
            ) {
                self.set_status(tr!("dieses format lässt sich nicht spulen"));
            }
        }

        let minutes = ctx.input(|i| {
            i.events.iter().fold(0i32, |n, e| match e {
                egui::Event::Text(t) if t == "." => n + 1,
                egui::Event::Text(t) if t == "," => n - 1,
                _ => n,
            })
        });
        if minutes != 0 {
            let to = self.player.position().as_secs_f64() + minutes as f64 * SEEK_MINUTE_SECS;
            if !self.player.seek_to(to) {
                self.set_status(tr!("dieses format lässt sich nicht spulen"));
            }
        }

        let arrow_down = taken(ctx, SHIFT, Key::ArrowDown);
        let arrow_up = taken(ctx, SHIFT, Key::ArrowUp);
        let j = taken(ctx, SHIFT, Key::J);
        let k = taken(ctx, SHIFT, Key::K);
        let (wider, narrower) = (arrow_down + j, arrow_up + k);
        if music && self.focus == Column::Queue {
            if self.editing.is_some() {
                if marked && (wider > 0 || narrower > 0) {
                    self.carry_queued(wider as i32 - narrower as i32)?;
                }
            } else {
                if arrow_down > 0 || arrow_up > 0 {
                    let len = self.player.queued().len() as i32;
                    if len > 0 {
                        let mut picks = self.queue_picks();
                        if picks.is_empty() {
                            picks.push(self.queue_index);
                        }
                        let (lo, hi) = (picks[0] as i32, picks[picks.len() - 1] as i32);
                        let step = arrow_down as i32 - arrow_up as i32;
                        let moving_end = if self.queue_index as i32 == lo { hi } else { lo };
                        let to = (moving_end + step).clamp(0, len - 1) as usize;
                        self.pick_queued(to, true, false);
                        self.scroll_to = Some(Column::Queue);
                    }
                }
                if marked && (j > 0 || k > 0) {
                    self.carry_queued(j as i32 - k as i32)?;
                }
            }
        }
        if music && (wider > 0 || narrower > 0) && self.focus == Column::Tracks && !self.tracks.is_empty()
        {
            let step = wider as i32 - narrower as i32;
            let to = (self.track_index as i32 + step).clamp(0, self.tracks.len() as i32 - 1);
            self.extend_track_selection(to as usize);
            self.scroll_to = Some(Column::Tracks);
        }

        if pressed(ctx, Modifiers::ALT | SHIFT, Key::M) {
            self.toggle_reel(app::Film::Mp4);
        }
        if pressed(ctx, Modifiers::ALT | SHIFT, Key::G) {
            self.toggle_reel(app::Film::Gif);
        } else if pressed(ctx, SHIFT, Key::G) {
            self.g_pressed_at = None;
            self.move_selection(i32::MAX / 2)?;
        } else if pressed(ctx, NONE, Key::G) {
            let now = ctx.input(|i| i.time);
            match self.g_pressed_at.take() {
                Some(then) if now - then < GG_WINDOW_SECS => {
                    self.move_selection(i32::MIN / 2)?;
                }
                _ => self.g_pressed_at = Some(now),
            }
        }
        let down = taken(ctx, NONE, Key::J) + taken(ctx, NONE, Key::ArrowDown);
        let up = taken(ctx, NONE, Key::K) + taken(ctx, NONE, Key::ArrowUp);
        let right = taken(ctx, NONE, Key::L) + taken(ctx, NONE, Key::ArrowRight);
        let left = taken(ctx, NONE, Key::H) + taken(ctx, NONE, Key::ArrowLeft);

        if down > 0 || up > 0 {
            if self.focus == Column::Queue {
                self.queue_picked.clear();
            }
            self.move_selection(down as i32 - up as i32)?;
        }
        if self.showing_podcasts() && (left > 0 || right > 0) {
            crate::gui::library::step_podcast_strip(ctx, right as isize - left as isize);
        } else {
            if left > 0 {
                self.focus_column(self.focus.left())?;
            }
            if right > 0 {
                self.focus_column(self.focus.right())?;
            }
        }
        if pressed(ctx, NONE, Key::Tab) {
            let first = Column::Artists;
            let to = match self.focus {
                Column::Tracks if self.queue_open() || self.lists_open() => Column::Queue,
                Column::Tracks | Column::Queue | Column::Lists => first,
                _ => Column::Tracks,
            };
            self.focus_column(to)?;
        }

        if marked && pressed(ctx, NONE, Key::Enter) {
            if self.focus == Column::Queue {
                let at = self.queue_index;
                if self.editing.is_some() {
                    self.play_from_list(at)?;
                } else if at < self.player.queued().len() {
                    self.play_queued(at)?;
                }
            } else {
                self.play_selected()?;
            }
        }
        if pressed(ctx, NONE, Key::Space) {
            self.toggle_pause();
        }
        if pressed(ctx, NONE, Key::N) {
            self.skip(true)?;
        }
        if pressed(ctx, NONE, Key::P) {
            self.skip(false)?;
        }
        let now = ctx.input(|i| i.time);
        if pressed(ctx, NONE, Key::C) {
            self.c_pressed_at = Some(now);
            self.set_status(tr!("ausschalten: 1 musik · 2 radio · 3 podcasts"));
        }
        let digit = [Key::Num1, Key::Num2, Key::Num3].into_iter().position(|k| pressed(ctx, NONE, k)).map(|n| n as u8 + 1);
        if let Some(n) = digit {
            let held = ctx.input(|i| i.key_down(Key::C));
            let closing = self.c_pressed_at.take().is_some_and(|then| now - then < GG_WINDOW_SECS) || held;
            if closing && !self.view_on(n) {
                self.set_status(already_off(n));
            } else if closing {
                self.switch_view(n, false);
            } else {
                if !self.view_on(n) {
                    self.switch_view(n, true);
                }
                self.open_view(n);
            }
        }
        if pressed(ctx, NONE, Key::Num4) && self.attached.is_some() {
            self.read_from(false, true);
        }
        let bigger = taken(ctx, Modifiers::COMMAND, Key::Plus) + taken(ctx, Modifiers::COMMAND, Key::Equals);
        let smaller = taken(ctx, Modifiers::COMMAND, Key::Minus);
        if bigger != smaller {
            self.zoom_by(ctx, bigger as i32 - smaller as i32);
        }
        if pressed(ctx, Modifiers::COMMAND, Key::Num0) {
            self.zoom_by(ctx, 0);
        }
        for _ in 0..taken(ctx, NONE, Key::Plus) + taken(ctx, NONE, Key::Equals) {
            self.player.volume_up();
        }
        for _ in 0..taken(ctx, NONE, Key::Minus) {
            self.player.volume_down();
        }
        if pressed(ctx, NONE, Key::S) {
            self.player.toggle_shuffle();
        }
        if pressed(ctx, NONE, Key::R) {
            self.player.toggle_repeat();
        }

        if pressed(ctx, NONE, Key::M) {
            self.menu_open = true;
        }
        if self.showing_podcasts() && pressed(ctx, NONE, Key::W) {
            if self.podcasts.info_open || self.log_open {
                self.podcasts.info_open = false;
                self.log_open = false;
                self.config.ui.podcast_queue = true;
            } else {
                self.config.ui.podcast_queue = !self.config.ui.podcast_queue;
            }
            self.save_config();
        } else if !self.showing_radio() && !self.showing_ipod() && pressed(ctx, NONE, Key::W) {
            self.toggle_queue();
        }
        if pressed(ctx, Modifiers::ALT | SHIFT, Key::F) {
            self.next_scheme(ctx, true);
        } else if pressed(ctx, SHIFT, Key::F) {
            self.next_scheme(ctx, false);
        } else if pressed(ctx, NONE, Key::F)
            || pressed(ctx, NONE, Key::F11)
            || pressed(ctx, Modifiers::COMMAND | Modifiers::CTRL, Key::F)
        {
            self.toggle_fullscreen(ctx);
        }
        if pressed(ctx, SHIFT, Key::T) {
            self.toggle_light_dark(ctx);
        } else if pressed(ctx, NONE, Key::T) {
            self.toggle_log();
        }
        if self.showing_podcasts() && pressed(ctx, NONE, Key::I) {
            self.podcast_info();
        } else if pressed(ctx, NONE, Key::I) {
            self.toggle_artist();
        }
        if pressed(ctx, NONE, Key::O) {
            self.toggle_gaps();
        }
        if pressed(ctx, NONE, Key::B) {
            let now = ctx.input(|i| i.time);
            self.saver.show_now(now);
        }
        if pressed(ctx, NONE, Key::Y) {
            self.toggle_lyrics();
        }
        if pressed(ctx, NONE, Key::Z) {
            self.toggle_eq();
        }
        if self.showing_radio() || self.showing_podcasts() {
        } else if pressed(ctx, SHIFT, Key::U) {
            self.go_to_lists()?;
        } else if pressed(ctx, NONE, Key::U) {
            self.toggle_lists()?;
        }
        if !music {
        } else if pressed(ctx, Modifiers::COMMAND, Key::A) {
            match self.focus {
                Column::Tracks | Column::Albums => self.select_all_tracks(),
                Column::Queue if self.editing.is_none() => self.pick_all_queued(),
                _ => {}
            }
        } else if marked && pressed(ctx, SHIFT, Key::A) {
            self.enqueue_selection(true)?;
        } else if marked && pressed(ctx, NONE, Key::A) {
            self.enqueue_selection(false)?;
        }
        if music && self.focus == Column::Queue && self.editing.is_none() && pressed(ctx, SHIFT, Key::D) {
            let now = ctx.input(|i| i.time);
            match self.clear_asked_at {
                Some(then) if now - then < QUIT_WINDOW_SECS => {
                    self.clear_asked_at = None;
                    self.clear_queue();
                }
                _ => {
                    self.clear_asked_at = Some(now);
                    self.set_status(tr!("noch einmal shift + d zum leeren"));
                }
            }
        }
        if music
            && self.focus == Column::Lists
            && self.lists_index < self.lists.len()
            && (pressed(ctx, NONE, Key::Backspace)
                || pressed(ctx, NONE, Key::Delete)
                || pressed(ctx, NONE, Key::D))
        {
            self.ask_to_delete_list(self.lists_index);
        }
        if music
            && marked
            && self.focus == Column::Queue
            && (pressed(ctx, NONE, Key::Backspace)
                || pressed(ctx, Modifiers::COMMAND, Key::Backspace)
                || pressed(ctx, NONE, Key::Delete)
                || pressed(ctx, NONE, Key::D))
        {
            match self.editing {
                Some(_) => self.drop_from_list(self.queue_index)?,
                None => self.drop_queued(),
            }
        }
        if music && marked && (pressed(ctx, Modifiers::COMMAND, Key::I) || pressed(ctx, NONE, Key::E)) {
            self.open_tag_editor();
        }
        if pressed(ctx, NONE, Key::Q) {
            let now = ctx.input(|i| i.time);
            match self.quit_asked_at {
                Some(then) if now - then < QUIT_WINDOW_SECS => {
                    self.quitting = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                _ => {
                    self.quit_asked_at = Some(now);
                    self.set_status(tr!("noch einmal q zum beenden"));
                }
            }
        }

        Ok(())
    }

    pub(in crate::gui) fn handle_prompt_keys(&mut self, ctx: &egui::Context) -> Result<()> {
        use egui::{Key, Modifiers};

        const NONE: Modifiers = Modifiers::NONE;
        let taken =
            |m: Modifiers, k: Key| -> usize { ctx.input_mut(|i| i.count_and_consume_key(m, k)) };
        let pressed =
            |m: Modifiers, k: Key| -> bool { ctx.input_mut(|i| i.consume_key(m, k)) };

        if pressed(NONE, Key::Escape) {
            self.cancel_prompt();
            return Ok(());
        }
        if pressed(NONE, Key::Enter) {
            return self.answer_prompt();
        }

        let cleared = pressed(Modifiers::COMMAND, Key::Backspace);
        let mark = pressed(Modifiers::COMMAND, Key::A) || pressed(Modifiers::CTRL, Key::A);
        let (copy, cut) = copy_or_cut(ctx);
        let backspaces = taken(NONE, Key::Backspace);
        let deletes = taken(NONE, Key::Delete);
        let lefts = taken(NONE, Key::ArrowLeft);
        let rights = taken(NONE, Key::ArrowRight);
        let home = taken(NONE, Key::Home) + taken(NONE, Key::ArrowUp);
        let end = taken(NONE, Key::End) + taken(NONE, Key::ArrowDown);

        let typed: String = typed(ctx, false);

        let Some(prompt) = self.prompt.as_mut() else {
            return Ok(());
        };
        if !prompt.typing() {
            return Ok(());
        }
        if cleared {
            prompt.entry.clear();
        }
        if mark {
            prompt.entry.mark_all();
        }
        if (copy || cut) && prompt.entry.is_marked() {
            ctx.copy_text(prompt.entry.text().to_string());
            if cut {
                prompt.entry.clear();
            }
        }
        for _ in 0..backspaces {
            prompt.entry.backspace();
        }
        for _ in 0..deletes {
            prompt.entry.delete();
        }
        for _ in 0..lefts {
            prompt.entry.left();
        }
        for _ in 0..rights {
            prompt.entry.right();
        }
        if home > 0 {
            prompt.entry.home();
        }
        if end > 0 {
            prompt.entry.end();
        }
        if !typed.is_empty() {
            prompt.entry.insert(&typed);
        }
        Ok(())
    }

    pub(in crate::gui) fn handle_edit_keys(&mut self, ctx: &egui::Context) -> Result<()> {
        use egui::{Key, Modifiers};

        const NONE: Modifiers = Modifiers::NONE;
        const SHIFT: Modifiers = Modifiers::SHIFT;

        let taken =
            |m: Modifiers, k: Key| -> usize { ctx.input_mut(|i| i.count_and_consume_key(m, k)) };
        let pressed =
            |m: Modifiers, k: Key| -> bool { ctx.input_mut(|i| i.consume_key(m, k)) };

        if pressed(NONE, Key::Escape) {
            self.cancel_tag_editor();
            return Ok(());
        }
        if pressed(NONE, Key::Enter) || pressed(Modifiers::COMMAND, Key::Enter) {
            return self.save_tags();
        }

        let cleared = pressed(Modifiers::COMMAND, Key::Backspace);
        let mark = pressed(Modifiers::COMMAND, Key::A) || pressed(Modifiers::CTRL, Key::A);
        let (copy, cut) = copy_or_cut(ctx);

        let Some(editor) = self.editor.as_mut() else {
            return Ok(());
        };

        if cleared {
            editor.clear();
        }
        if mark {
            editor.mark_all();
        }
        if copy || cut {
            if let Some(marked) = editor.marked() {
                ctx.copy_text(marked.to_string());
                if cut {
                    editor.clear();
                }
            }
        }

        for _ in 0..taken(NONE, Key::Tab) + taken(NONE, Key::ArrowDown) {
            editor.step(1);
        }
        for _ in 0..taken(SHIFT, Key::Tab) + taken(NONE, Key::ArrowUp) {
            editor.step(-1);
        }
        for _ in 0..taken(NONE, Key::ArrowLeft) {
            editor.left();
        }
        for _ in 0..taken(NONE, Key::ArrowRight) {
            editor.right();
        }
        if pressed(NONE, Key::Home) {
            editor.home();
        }
        if pressed(NONE, Key::End) {
            editor.end();
        }
        for _ in 0..taken(NONE, Key::Backspace) {
            editor.backspace();
        }
        for _ in 0..taken(NONE, Key::Delete) {
            editor.delete();
        }

        let typed = typed(ctx, false);
        if !typed.is_empty() {
            editor.typed(&typed);
        }
        Ok(())
    }

    pub(in crate::gui) fn handle_artist_form_keys(&mut self, ctx: &egui::Context) {
        use egui::{Key, Modifiers};
        const NONE: Modifiers = Modifiers::NONE;
        const SHIFT: Modifiers = Modifiers::SHIFT;
        let taken =
            |m: Modifiers, k: Key| -> usize { ctx.input_mut(|i| i.count_and_consume_key(m, k)) };
        let pressed =
            |m: Modifiers, k: Key| -> bool { ctx.input_mut(|i| i.consume_key(m, k)) };

        if pressed(NONE, Key::Escape) {
            self.artist_form = None;
            return;
        }
        let newline = pressed(SHIFT, Key::Enter);
        if !newline && (pressed(NONE, Key::Enter) || pressed(Modifiers::COMMAND, Key::Enter)) {
            self.save_artist_form();
            return;
        }
        let cleared = pressed(Modifiers::COMMAND, Key::Backspace);
        let mark = pressed(Modifiers::COMMAND, Key::A) || pressed(Modifiers::CTRL, Key::A);
        let (copy, cut) = copy_or_cut(ctx);
        let Some(form) = self.artist_form.as_mut() else {
            return;
        };
        if newline && form.focus == artist::NOTE {
            form.entry().insert("\n");
        }
        if cleared {
            form.entry().clear();
        }
        if mark {
            form.entry().mark_all();
        }
        if (copy || cut) && form.entry().is_marked() {
            ctx.copy_text(form.entry().text().to_string());
            if cut {
                form.entry().clear();
            }
        }
        for _ in 0..taken(NONE, Key::Tab) + taken(NONE, Key::ArrowDown) {
            form.step(1);
        }
        for _ in 0..taken(SHIFT, Key::Tab) + taken(NONE, Key::ArrowUp) {
            form.step(-1);
        }
        for _ in 0..taken(NONE, Key::ArrowLeft) {
            form.entry().left();
        }
        for _ in 0..taken(NONE, Key::ArrowRight) {
            form.entry().right();
        }
        if pressed(NONE, Key::Home) {
            form.entry().home();
        }
        if pressed(NONE, Key::End) {
            form.entry().end();
        }
        for _ in 0..taken(NONE, Key::Backspace) {
            form.entry().backspace();
        }
        for _ in 0..taken(NONE, Key::Delete) {
            form.entry().delete();
        }
        let typed = typed(ctx, form.focus == artist::NOTE);
        if !typed.is_empty() {
            form.entries[form.focus].insert(&typed);
        }
    }

    pub(in crate::gui) fn handle_filter_keys(&mut self, ctx: &egui::Context) -> Result<()> {
        use egui::{Key, Modifiers};

        const NONE: Modifiers = Modifiers::NONE;
        const SHIFT: Modifiers = Modifiers::SHIFT;

        let taken = |m: Modifiers, k: Key| -> usize {
            ctx.input_mut(|i| i.count_and_consume_key(m, k))
        };
        let pressed =
            |m: Modifiers, k: Key| -> bool { ctx.input_mut(|i| i.consume_key(m, k)) };

        if pressed(NONE, Key::Escape) {
            return self.close_filter();
        }

        if pressed(Modifiers::COMMAND, Key::A) || pressed(Modifiers::CTRL, Key::A) {
            self.filter_marked = self.filter.as_deref().is_some_and(|q| !q.is_empty());
            return Ok(());
        }

        let forward = taken(SHIFT, Key::ArrowRight);
        let backward = taken(SHIFT, Key::ArrowLeft);
        if forward > 0 || backward > 0 {
            let delta = (forward as f64 - backward as f64) * SEEK_STEP_SECS;
            let to = self.player.position().as_secs_f64() + delta;
            if !self.player.seek_to(to) {
                self.set_status(tr!("dieses format lässt sich nicht spulen"));
            }
        }

        let down = taken(NONE, Key::ArrowDown);
        let up = taken(NONE, Key::ArrowUp);
        if down > 0 || up > 0 {
            if self.showing_podcasts() {
                self.podcasts.focus = 0;
                self.podcast_step(down as i32 - up as i32);
            } else {
                self.move_selection(down as i32 - up as i32)?;
            }
        }

        let length = self.filter.as_deref().map_or(0, |q| q.chars().count());
        let at = self.filter_caret.unwrap_or(length).min(length);
        let chars: Vec<char> = self.filter.as_deref().unwrap_or("").chars().collect();
        let word_left = |from: usize| {
            let mut i = from;
            while i > 0 && chars[i - 1] == ' ' {
                i -= 1;
            }
            while i > 0 && chars[i - 1] != ' ' {
                i -= 1;
            }
            i
        };
        let word_right = |from: usize| {
            let mut i = from;
            while i < chars.len() && chars[i] == ' ' {
                i += 1;
            }
            while i < chars.len() && chars[i] != ' ' {
                i += 1;
            }
            i
        };
        let mut caret = at;
        if pressed(Modifiers::COMMAND, Key::ArrowLeft) || pressed(NONE, Key::Home) {
            caret = 0;
        }
        if pressed(Modifiers::COMMAND, Key::ArrowRight) || pressed(NONE, Key::End) {
            caret = length;
        }
        for _ in 0..taken(Modifiers::ALT, Key::ArrowLeft) {
            caret = word_left(caret);
        }
        for _ in 0..taken(Modifiers::ALT, Key::ArrowRight) {
            caret = word_right(caret);
        }
        let (left, right) = (taken(NONE, Key::ArrowLeft), taken(NONE, Key::ArrowRight));
        caret = (caret + right).saturating_sub(left).min(length);
        if caret != at {
            self.filter_marked = false;
            self.filter_caret = Some(caret);
        }

        if pressed(NONE, Key::Enter) || pressed(NONE, Key::Tab) {
            self.filter_typing = false;
            if self.focus != Column::Tracks && !self.tracks.is_empty() {
                self.focus_column(Column::Tracks)?;
            }
            return Ok(());
        }

        let mut query = self.filter.clone().unwrap_or_default();
        let before = query.clone();
        let mut close = false;

        let marked = self.filter_marked;
        let typed: String = typed(ctx, false);
        let rubbed_out = taken(NONE, Key::Backspace);
        let rubbed_ahead = taken(NONE, Key::Delete);
        let mut chars: Vec<char> = query.chars().collect();
        let mut caret = self.filter_caret.unwrap_or(chars.len()).min(chars.len());
        if marked && (rubbed_out > 0 || rubbed_ahead > 0 || !typed.is_empty()) {
            chars.clear();
            caret = 0;
            self.filter_marked = false;
        } else {
            for _ in 0..rubbed_out {
                if caret == 0 {
                    if chars.is_empty() {
                        close = true;
                    }
                } else {
                    chars.remove(caret - 1);
                    caret -= 1;
                }
            }
            for _ in 0..rubbed_ahead {
                if caret < chars.len() {
                    chars.remove(caret);
                }
            }
        }
        for c in typed.chars() {
            chars.insert(caret, c);
            caret += 1;
        }
        if !typed.is_empty() {
            self.filter_marked = false;
        }
        query = chars.iter().collect();
        self.filter_caret = (caret < chars.len()).then_some(caret);

        if close && query.is_empty() {
            return self.close_filter();
        }
        if query != before {
            self.filter = Some(query);
            self.after_filter_changed()?;
        }
        Ok(())
    }
}

fn already_off(n: u8) -> &'static str {
    match n {
        1 => tr!("musik ist schon aus — 1 schaltet sie ein"),
        2 => tr!("radio ist schon aus — 2 schaltet es ein"),
        _ => tr!("podcasts sind schon aus — 3 schaltet sie ein"),
    }
}

fn off_note(n: u8) -> &'static str {
    match n {
        1 => tr!("musik ausgeschaltet — 1 schaltet sie wieder ein"),
        2 => tr!("radio ausgeschaltet — 2 schaltet es wieder ein"),
        _ => tr!("podcasts ausgeschaltet — 3 schaltet sie wieder ein"),
    }
}

impl GuiApp {
    fn view_on(&self, n: u8) -> bool {
        match n {
            1 => self.config.ui.music,
            2 => self.config.ui.radio,
            _ => self.config.ui.podcasts,
        }
    }

    fn switch_view(&mut self, n: u8, on: bool) {
        match n {
            1 => self.set_music(on),
            2 => self.set_radio(on),
            _ => self.set_podcasts(on),
        }
        let said = match (n, on) {
            (1, true) => tr!("musik eingeschaltet"),
            (2, true) => tr!("radio eingeschaltet"),
            (_, true) => tr!("podcasts eingeschaltet"),
            _ => off_note(n),
        };
        self.set_status(said);
    }

    fn open_view(&mut self, n: u8) {
        match n {
            1 => self.read_from(false, false),
            2 => self.read_from(true, false),
            _ if !self.showing_podcasts() => self.show_podcasts(),
            _ => {}
        }
    }

    fn open_row_menu_by_key(&mut self, ctx: &egui::Context) {
        use crate::gui::library::cursor_row;
        use crate::gui::{About, RowMenu};
        let asked: Option<(About, &'static str)> = if self.showing_podcasts() {
            let (list, _) = self.podcasts.episodes(self.config.ui.podcast_show_archived);
            let looked = self.podcasts.episode.as_ref().map(|(e, _)| e.id.clone());
            let at = looked.and_then(|id| list.iter().position(|(e, _)| e.id == id));
            match (self.podcasts.focus, at) {
                (1, Some(at)) => Some((About::Episode(at), "episode")),
                _ => None,
            }
        } else if self.showing_radio() {
            let cursor = self.radio.as_ref().map(|r| r.cursor);
            match (self.radio_heard_keys, cursor) {
                (None, Some(at)) if at < self.radio_listed().len() => Some((About::Station(at), "station")),
                _ => None,
            }
        } else {
            let first_name = crate::gui::ArtistRow::Named(0).index();
            match self.focus {
                Column::Artists if self.artist_index >= first_name && self.artist_index - first_name < self.artists.len() => {
                    Some((About::Artist(self.artist_index - first_name), "artists"))
                }
                Column::Albums if self.album_index >= 1 && self.album_index - 1 < self.albums.len() => Some((About::Album(self.album_index - 1), "albums")),
                Column::Tracks if !self.tracks.is_empty() => {
                    self.unmarked = false;
                    Some((About::Tracks, "tracks"))
                }
                Column::Queue if self.editing.is_some() && self.queue_index < self.editing_tracks.len() => {
                    Some((About::Listed(self.queue_index), "queue"))
                }
                Column::Queue if self.editing.is_none() && self.queue_index < self.player.queued().len() => {
                    Some((About::Queued(self.queue_index), "queue"))
                }
                Column::Lists if self.lists_index < self.lists.len() => Some((About::List(self.lists_index), "lists")),
                _ => None,
            }
        };
        let Some((about, key)) = asked else {
            self.set_status(tr!("hier gibt es kein menü"));
            return;
        };
        let at = match cursor_row(ctx, key) {
            Some(row) => egui::pos2(row.left() + (row.width() * 0.25).min(80.0), row.bottom()),
            None => ctx.content_rect().center(),
        };
        self.row_menu = Some(RowMenu { at, about });
        self.row_menu_focus = None;
        self.row_menu_step = 1;
        self.row_menu_enter = false;
    }
}
