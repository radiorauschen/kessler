use crate::gui::*;

const THEME_FADE_SECS: f64 = 0.25;

const ZOOM_OUT_SECS: f64 = 0.06;
const ZOOM_IN_SECS: f64 = 0.14;

#[derive(Clone, Copy)]
pub enum ZoomDip {
    Out { since: f64, to: f32 },
    Held { to: f32 },
    In { since: f64 },
}

impl GuiApp {
    pub fn apply_theme(&mut self, ctx: &egui::Context, theme: Theme, brightness: Brightness) {
        let system = self.desktop_theme.or(ctx.system_theme());
        self.theme = brightness.resolve(theme, system);
        self.brightness = brightness;
        self.config.ui.ground = self.theme.name().to_string();
        self.config.ui.brightness = brightness.name().to_string();
        self.save_config();
        self.follow_system(ctx);
    }

    pub fn set_seed_colour(&mut self, role: usize, colour: egui::Color32, settled: bool) {
        let mut seed = theme::custom();
        seed.set(role, colour);
        theme::set_custom(seed);
        if settled {
            self.config.ui.custom_colours = seed.code();
            self.save_config();
        }
    }

    pub fn take_seed_code(&mut self, ctx: &egui::Context, code: &str) {
        match theme::Seed::parse(code) {
            Some(seed) => {
                theme::set_custom(seed);
                self.config.ui.custom_colours = seed.code();
                self.seed_entry = None;
                self.fading = Some((self.palette, ctx.input(|i| i.time)));
                self.apply_theme(ctx, Theme::Eigenes, self.brightness);
            }
            None => self.set_status(tr!("diesen code kann kessler nicht lesen")),
        }
    }

    pub fn take_seed_hex(&mut self, role: usize, text: &str) {
        match theme::Seed::parse_colour(text) {
            Some(colour) => {
                self.seed_hex = None;
                self.set_seed_colour(role, colour, true);
            }
            None => self.set_status(tr!("diese farbe kann kessler nicht lesen")),
        }
    }

    pub(in crate::gui) fn handle_seed_keys(&mut self, ctx: &egui::Context) {
        use egui::{Key, Modifiers};
        const NONE: Modifiers = Modifiers::NONE;
        let pressed = |m: Modifiers, k: Key| -> bool { ctx.input_mut(|i| i.consume_key(m, k)) };
        if pressed(NONE, Key::Escape) {
            self.seed_entry = None;
            self.seed_hex = None;
            return;
        }
        if pressed(NONE, Key::Enter) {
            if let Some((role, entry)) = self.seed_hex.as_ref() {
                let (role, text) = (*role, entry.text().to_string());
                self.take_seed_hex(role, &text);
            } else {
                let code = self.seed_entry.as_ref().map(|e| e.text().to_string()).unwrap_or_default();
                self.take_seed_code(ctx, &code);
            }
            return;
        }
        let typed = crate::gui::typed(ctx, false);
        let entry = match (self.seed_hex.as_mut(), self.seed_entry.as_mut()) {
            (Some((_, entry)), _) => entry,
            (None, Some(entry)) => entry,
            (None, None) => return,
        };
        for _ in 0..ctx.input_mut(|i| i.count_and_consume_key(NONE, Key::Backspace)) {
            entry.backspace();
        }
        if !typed.is_empty() {
            entry.insert(&typed);
        }
    }

    pub fn follow_system(&mut self, ctx: &egui::Context) {
        self.listen_to_the_desktop(ctx);
        let system = self.desktop_theme.or(ctx.system_theme());
        let resolved = self.brightness.resolve(self.theme, system);
        let now = ctx.input(|i| i.time);
        if resolved != self.resolved {
            self.resolved = resolved;
            if ctx.cumulative_pass_nr() > 2 && !self.reeling() {
                self.fading = Some((self.palette, now));
            }
        }
        let palette = self.resolved.palette();
        let palette = palette.coloured();
        let target = palette;
        if theme::set_light_ground(!target.is_dark()) {
            let key = self.config.ui.typeface.clone();
            self.set_typeface(ctx, &key);
        }
        let shown = match self.fading {
            Some((from, since)) => {
                let t = (((now - since) / THEME_FADE_SECS).clamp(0.0, 1.0)) as f32;
                if t >= 1.0 {
                    self.fading = None;
                    target
                } else {
                    ctx.request_repaint();
                    from.mix(&target, t * t * (3.0 - 2.0 * t))
                }
            }
            None => target,
        };
        if shown != self.palette || ctx.cumulative_pass_nr() == 0 {
            self.palette = shown;
            theme::install_style(ctx, &self.palette);
        }
        self.dress_the_frame(ctx);
        self.dress_the_icon(ctx);
        let lang = crate::lang::current();
        if self.titled != Some(lang) {
            self.titled = Some(lang);
            let title = if crate::DEV { format!("{} dev", crate::lang::app_name()) } else { crate::lang::app_name().to_string() };
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        }
    }

    pub(in crate::gui) fn dress_the_icon(&mut self, ctx: &egui::Context) {
        let palette = self.resolved.palette();
        let wanted = (palette.ground, palette.accent, self.typeface.clone(), crate::lang::current());
        if self.icon_dressed.as_ref() == Some(&wanted) {
            return;
        }
        let size = appicon::SIDE;
        let Some(letter) = appicon::letter(ctx, size) else {
            return;
        };
        self.icon_dressed = Some(wanted);
        let rgba = appicon::compose(size, appicon::Shape::here(), palette.ground, palette.accent, Some(&letter));
        if !crate::media::set_dock_icon(&appicon::premultiplied(&rgba), size, size) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Icon(Some(std::sync::Arc::new(egui::IconData {
                rgba,
                width: size as u32,
                height: size as u32,
            }))));
        }
    }

    pub(in crate::gui) fn listen_to_the_desktop(&mut self, ctx: &egui::Context) {
        if !cfg!(target_os = "linux") {
            return;
        }
        if let Some(news) = &self.desktop_news {
            while let Ok(theme) = news.try_recv() {
                self.desktop_theme = Some(theme);
            }
            return;
        }
        if self.brightness != Brightness::System {
            return;
        }
        self.desktop_theme = system_theme();
        let (tx, rx) = mpsc::channel();
        self.desktop_news = Some(rx);
        let wake = ctx.clone();
        std::thread::spawn(move || {
            use std::io::BufRead;
            #[cfg(target_os = "linux")]
            if crate::gui::portal::watch(&tx, &wake) {
                return;
            }
            let mut watching = std::process::Command::new("gsettings");
            watching
                .args(["monitor", "org.gnome.desktop.interface", "color-scheme"])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null());
            #[cfg(target_os = "linux")]
            {
                use std::os::unix::process::CommandExt;
                unsafe {
                    watching.pre_exec(|| {
                        libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                        Ok(())
                    });
                }
            }
            let Ok(mut child) = watching.spawn() else {
                return;
            };
            let Some(out) = child.stdout.take() else { return };
            for line in std::io::BufReader::new(out).lines().map_while(|l| l.ok()) {
                if tx.send(scheme_from(&line)).is_err() {
                    break;
                }
                wake.request_repaint();
            }
            let _ = child.kill();
            let _ = child.wait();
        });
    }

    pub(in crate::gui) fn dress_the_frame(&mut self, ctx: &egui::Context) {
        let wanted = frame_theme_for(self.brightness, &self.palette);
        let settling = ctx.cumulative_pass_nr() < 8;
        if self.frame_theme == Some(wanted) && !settling {
            return;
        }
        self.frame_theme = Some(wanted);
        ctx.send_viewport_cmd(egui::ViewportCommand::SetTheme(wanted));
    }

    pub fn toggle_light_dark(&mut self, ctx: &egui::Context) {
        let to = if self.resolved.palette().is_dark() { Brightness::Hell } else { Brightness::Dunkel };
        self.apply_theme(ctx, self.resolved, to);
        self.set_status(tr!("farben: {} · {}", crate::lang::t(self.theme.family()), crate::lang::t(to.name())));
    }

    pub fn next_scheme(&mut self, ctx: &egui::Context, back: bool) {
        let own = Theme::Eigenes.family();
        let families: Vec<Theme> =
            Theme::families().into_iter().filter(|t| t.family() != own || self.resolved.family() == own).collect();
        let at = families.iter().position(|t| t.family() == self.resolved.family()).unwrap_or(0);
        let step = if back { families.len() - 1 } else { 1 };
        let next = families[(at + step) % families.len()];
        let brightness = self.brightness;
        self.apply_theme(ctx, next.side(self.resolved.palette().is_dark()), brightness);
        self.set_status(tr!("farben: {}", crate::lang::t(next.family())));
    }

    pub fn toggle_fullscreen(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!now));
    }

    pub fn set_zoom(&mut self, ctx: &egui::Context, zoom: f32) {
        let to = crate::gui::menu::zoom_of(zoom);
        let now = ctx.input(|i| i.time);
        self.zoom_dip = Some(match self.zoom_dip {
            Some(ZoomDip::Out { since, .. }) => ZoomDip::Out { since, to },
            Some(ZoomDip::Held { .. }) => ZoomDip::Held { to },
            Some(ZoomDip::In { since }) => {
                let shown = 1.0 - ((now - since) / ZOOM_IN_SECS).clamp(0.0, 1.0);
                ZoomDip::Out { since: now - ZOOM_OUT_SECS * shown, to }
            }
            None => ZoomDip::Out { since: now, to },
        });
        ctx.request_repaint();
    }

    fn zoom_wanted(&self) -> f32 {
        match self.zoom_dip {
            Some(ZoomDip::Out { to, .. } | ZoomDip::Held { to }) => to,
            _ => crate::gui::menu::zoom_of(self.config.ui.zoom),
        }
    }

    pub fn zoom_dip(&mut self, ctx: &egui::Context) -> f32 {
        let Some(dip) = self.zoom_dip else {
            return 0.0;
        };
        let now = ctx.input(|i| i.time);
        let over = match dip {
            ZoomDip::Out { since, to } => {
                let gone = ((now - since) / ZOOM_OUT_SECS) as f32;
                if gone >= 1.0 {
                    ctx.set_zoom_factor(to);
                    self.config.ui.zoom = to;
                    self.save_config();
                    crate::gui::menu::rebalance(self);
                    self.zoom_dip = Some(ZoomDip::Held { to });
                    1.0
                } else {
                    gone.max(0.0)
                }
            }
            ZoomDip::Held { to } => {
                if (ctx.zoom_factor() - to).abs() < 0.001 {
                    self.zoom_dip = Some(ZoomDip::In { since: now });
                }
                1.0
            }
            ZoomDip::In { since } => {
                let back = ((now - since) / ZOOM_IN_SECS) as f32;
                if back >= 1.0 {
                    self.zoom_dip = None;
                    return 0.0;
                }
                1.0 - back
            }
        };
        ctx.request_repaint();
        over * over * (3.0 - 2.0 * over)
    }

    pub fn zoom_by(&mut self, ctx: &egui::Context, by: i32) {
        let zoom = match by {
            0 => 1.0,
            by => crate::gui::menu::zoom_step(self.zoom_wanted(), by),
        };
        self.set_zoom(ctx, zoom);
        self.set_status(tr!("grösse: {} %", (zoom * 100.0).round() as i32));
    }

    pub fn next_typeface(&mut self, ctx: &egui::Context, back: bool) {
        let count = self.families.len();
        if count < 2 {
            return;
        }
        let now = self.typeface.as_deref().unwrap_or_default();
        let at = self.families.iter().position(|f| f.name == now).unwrap_or(0);
        let to = if back { (at + count - 1) % count } else { (at + 1) % count };
        let next = self.families[to].key.clone();
        self.set_typeface(ctx, &next);
    }

    pub fn set_system_fonts(&mut self, ctx: &egui::Context, on: bool) {
        self.config.ui.system_fonts = on;
        self.families = theme::all_families(self.config.ui.font_dir.as_deref(), on);
        let now = self.typeface.as_deref().unwrap_or_default();
        if !self.families.iter().any(|f| f.name == now) {
            self.set_typeface(ctx, theme::PREFERRED);
        } else {
            self.save_config();
        }
        if on {
            let count = self.families.len();
            self.set_status(tr!("{} schriften", count));
        }
    }

    pub fn reset_settings(&mut self, ctx: &egui::Context) {
        let old = std::mem::take(&mut self.config.ui);
        let ui = &mut self.config.ui;
        ui.radio = old.radio;
        ui.ipod = old.ipod;
        ui.podcasts = old.podcasts;
        ui.music = old.music;
        ui.font_dir = old.font_dir;
        ui.font_fallback = old.font_fallback;
        ui.window = old.window;
        ui.book_at = old.book_at;
        crate::lang::set(crate::lang::Lang::chosen(&self.config.ui.language));
        self.apply_eq();
        theme::set_custom(theme::Seed::parse(&self.config.ui.custom_colours).unwrap_or(theme::Seed::DEFAULT));
        self.families = theme::all_families(self.config.ui.font_dir.as_deref(), self.config.ui.system_fonts);
        self.set_typeface(ctx, theme::PREFERRED);
        self.set_zoom(ctx, self.config.ui.zoom);
        let scheme = Theme::from_name(&self.config.ui.ground);
        let brightness = Brightness::from_config(&self.config.ui.brightness, &self.config.ui.ground);
        self.apply_theme(ctx, scheme, brightness);
        self.save_config();
        self.set_status(tr!("einstellungen zurückgesetzt"));
    }

    pub fn set_typeface(&mut self, ctx: &egui::Context, key: &str) {
        let Some(chosen) = theme::the_typeface(&self.families, key) else {
            return;
        };
        self.config.ui.typeface = chosen.key.clone();
        theme::set_face(&self.config.ui);
        let (source, fallback) = theme::install_fonts(
            ctx,
            Some(chosen.regular.as_str()),
            chosen.bold.as_deref(),
            chosen.figures.as_deref(),
            self.config.ui.font_fallback.as_deref(),
            theme::face_scale(&chosen.key),
        );
        self.font_source = source;
        self.font_fallback = fallback;
        self.typeface = Some(chosen.name);
        self.save_config();
        self.menu_heights = Default::default();
    }

    pub fn set_font_width(&mut self, ctx: &egui::Context, width: f32) {
        self.config.ui.font_width = width;
        let key = self.config.ui.typeface.clone();
        self.set_typeface(ctx, &key);
    }

    pub fn set_font_weight(&mut self, ctx: &egui::Context, weight: f32) {
        self.config.ui.font_weight = weight;
        let key = self.config.ui.typeface.clone();
        self.set_typeface(ctx, &key);
    }

    pub fn style_sets(&mut self) -> Vec<theme::StyleSet> {
        let Some(chosen) = theme::the_typeface(&self.families, &self.config.ui.typeface) else {
            return Vec::new();
        };
        if !matches!(&self.sets_offered, Some((path, _)) if *path == chosen.regular) {
            let sets = std::fs::read(&chosen.regular).map(|data| theme::style_sets(&data)).unwrap_or_default();
            self.sets_offered = Some((chosen.regular.clone(), sets));
        }
        self.sets_offered.as_ref().map(|(_, sets)| sets.clone()).unwrap_or_default()
    }

    pub fn toggle_style_set(&mut self, ctx: &egui::Context, tag: &str) {
        let rivals = theme::rivals_of(&self.style_sets(), tag);
        let key = theme::face_key(&self.config.ui.typeface).to_string();
        let sets = self.config.ui.font_sets.entry(key).or_default();
        if let Some(at) = sets.iter().position(|t| t == tag) {
            sets.remove(at);
        } else {
            sets.retain(|t| !rivals.contains(t));
            sets.push(tag.to_string());
            sets.sort();
        }
        let typeface = self.config.ui.typeface.clone();
        self.set_typeface(ctx, &typeface);
    }

    pub fn toggle_column(&mut self, column: columns::TrackColumn) -> Result<()> {
        self.columns.toggle(column);
        self.config.ui.track_columns = self.columns.names();
        self.save_config();
        if !self.columns.contains(column) && self.sort == column.sort_field() {
            self.sort = SortField::Artist;
            self.order = SortOrder::Ascending;
            self.remember_orders();
            return self.reload_dependent();
        }
        Ok(())
    }

    pub fn set_status(&mut self, message: impl Into<String>) {
        self.status = Some((message.into(), Instant::now(), STATUS_SECS));
    }

    pub fn set_notice(&mut self, message: impl Into<String>) {
        self.status = Some((message.into(), Instant::now(), NOTICE_SECS));
    }

    pub(in crate::gui) fn radio_scrobble_standing(&self) -> Option<OnAirScrobble> {
        if self.on_air.is_none() || !self.config.ui.radio_scrobble || self.scrobbler.is_none() {
            return None;
        }
        let now = self.aired.first()?;
        if now.scrobbled {
            return None;
        }
        if now.artist.is_none() {
            return Some(OnAirScrobble::Never);
        }
        let heard = (chrono::Local::now() - now.at).num_seconds().max(0);
        let needed = if now.from_start { super::radio::RADIO_SCROBBLE_AFTER } else { crate::lastfm::watch::ENOUGH_SECS as i64 };
        Some(match needed - heard {
            left if left > 0 => OnAirScrobble::Listening(left as u64),
            left if left > -COUNTS_SAID_SECS => OnAirScrobble::Counts,
            _ => return None,
        })
    }

    pub fn scrobble_standing(&self) -> Option<String> {
        use crate::lastfm::watch::Standing;
        let scrobbler = self.scrobbler.as_ref()?;
        if let Some(on_air) = self.radio_scrobble_standing() {
            return Some(match on_air {
                OnAirScrobble::Listening(left) => tr!("last.fm  noch {}", crate::gui::text::duration(left)),
                OnAirScrobble::Counts => tr!("last.fm  zählt beim nächsten lied").to_string(),
                OnAirScrobble::Never => tr!("last.fm  zählt nicht").to_string(),
            });
        }
        match self.scrobble_watch.standing() {
            Some(Standing::Counted) => Some(tr!("last.fm  gescrobbelt").to_string()),
            Some(Standing::Never) => Some(tr!("last.fm  zählt nicht").to_string()),
            Some(Standing::Listening { left }) => Some(tr!("last.fm  noch {}",
                crate::gui::text::duration(left.ceil() as u64)
            )),
            None => match scrobbler.waiting() {
                0 => None,
                1 => Some(tr!("last.fm  1 wartet").to_string()),
                n => Some(tr!("last.fm  {} warten", n)),
            },
        }
    }

    pub fn library_line(&self) -> Option<String> {
        if self.total_tracks == 0 || self.showing_radio() {
            return None;
        }
        let mut said = vec![
            tr!("{} titel", text::count(self.total_tracks as u64)),
            tr!("{} alben", text::count(self.total_albums as u64)),
            text::long_duration(self.total_seconds as u64),
        ];
        if self.total_bytes > 0 {
            said.push(crate::ipod::format_bytes(self.total_bytes));
        }
        Some(said.join(" · "))
    }

    pub fn scrobble_short(&self) -> Option<String> {
        use crate::lastfm::watch::Standing;
        let scrobbler = self.scrobbler.as_ref()?;
        if let Some(on_air) = self.radio_scrobble_standing() {
            return Some(match on_air {
                OnAirScrobble::Listening(left) => format!("last.fm  {}", crate::gui::text::duration(left)),
                OnAirScrobble::Counts => tr!("last.fm  zählt").to_string(),
                OnAirScrobble::Never => tr!("last.fm  nicht").to_string(),
            });
        }
        match self.scrobble_watch.standing() {
            Some(Standing::Counted) => Some("last.fm  ok".to_string()),
            Some(Standing::Never) => Some(tr!("last.fm  nicht").to_string()),
            Some(Standing::Listening { left }) => Some(format!(
                "last.fm  {}",
                crate::gui::text::duration(left.ceil() as u64)
            )),
            None => match scrobbler.waiting() {
                0 => None,
                n => Some(tr!("last.fm  {} warten", n)),
            },
        }
    }

    pub fn search_standing(&self) -> Option<String> {
        let query = self.filter.as_ref()?;
        if query.trim().is_empty() {
            return None;
        }
        let count = |n: usize| text::count(n as u64);
        let said: Vec<String> = [
            (self.artists.len(), tr!("{} künstler:innen", count(self.artists.len()))),
            (self.albums.len(), tr!("{} alben", count(self.albums.len()))),
            (self.tracks.len(), tr!("{} titel", count(self.tracks.len()))),
        ]
        .into_iter()
        .filter(|(n, _)| *n > 0)
        .map(|(_, said)| said)
        .collect();
        Some(if said.is_empty() {
            tr!("nichts gefunden").to_string()
        } else {
            said.join("     ")
        })
    }

    pub fn artist_hunt_standing(&self) -> Option<String> {
        let hunt = self.artist_hunt.as_ref()?;
        Some(tr!(
            "künstler:innen {} / {}",
            text::count(hunt.asked as u64),
            text::count(hunt.total as u64)
        ))
    }

    pub fn transfer_standing(&self) -> Option<String> {
        match self.transfer.as_ref().map(|t| &t.stage) {
            Some(crate::gui::Stage::Going { done, total, .. }) => Some(tr!(
                "ipod {} / {}",
                text::count(*done as u64),
                text::count(*total as u64)
            )),
            _ => None,
        }
    }

    pub fn status(&self) -> Option<&str> {
        self.status.as_ref().and_then(|(msg, at, secs)| {
            (at.elapsed() < std::time::Duration::from_secs(*secs)).then_some(msg.as_str())
        })
    }
}

const COUNTS_SAID_SECS: i64 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::gui) enum OnAirScrobble {
    Listening(u64),
    Counts,
    Never,
}
