//! How the window looks and what its status line says.

use crate::gui::*;

/// How long a change of colours takes to fade from one to the other.
const THEME_FADE_SECS: f64 = 0.25;

/// A change of size is made behind a dip to the ground, so quickly it reads as
/// a blink: the window fades out, changes size where nobody sees it, and fades
/// back in. Out fast, in a little slower, in seconds.
const ZOOM_OUT_SECS: f64 = 0.06;
const ZOOM_IN_SECS: f64 = 0.14;

/// Where a change of size stands — see [`GuiApp::zoom_dip`].
#[derive(Clone, Copy)]
pub enum ZoomDip {
    /// Fading out, since then, to this size.
    Out { since: f64, to: f32 },
    /// Set, and waiting for the first frame drawn at it: that frame is the
    /// one that lays every face out afresh at the new size, and it stays
    /// hidden.
    Held { to: f32 },
    /// Fading back in, since then.
    In { since: f64 },
}

impl GuiApp {
    /// Switch to a colour scheme and to light, dark or the system's, and
    /// remember both. The scheme is kept as the half being painted, so a
    /// config read by an older kessler still opens on it.
    pub fn apply_theme(&mut self, ctx: &egui::Context, theme: Theme, brightness: Brightness) {
        let system = self.desktop_theme.or(ctx.system_theme());
        self.theme = brightness.resolve(theme, system);
        self.brightness = brightness;
        self.config.ui.ground = self.theme.name().to_string();
        self.config.ui.brightness = brightness.name().to_string();
        self.save_config();
        self.follow_system(ctx);
    }

    /// One of one's own scheme's four colours, as a rule in the menu moves it:
    /// painted at once (the palette is worked out from the seed every frame),
    /// written to the config once the hand lets go.
    pub fn set_seed_colour(&mut self, role: usize, colour: egui::Color32, settled: bool) {
        let mut seed = theme::custom();
        seed.set(role, colour);
        theme::set_custom(seed);
        if settled {
            self.config.ui.custom_colours = seed.code();
            self.save_config();
        }
    }

    /// Take a scheme somebody passed on by its code, and show it.
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

    /// Take one colour typed as hex, for the colour it was typed under.
    pub fn take_seed_hex(&mut self, role: usize, text: &str) {
        match theme::Seed::parse_colour(text) {
            Some(colour) => {
                self.seed_hex = None;
                self.set_seed_colour(role, colour, true);
            }
            None => self.set_status(tr!("diese farbe kann kessler nicht lesen")),
        }
    }

    /// The keys while a code is pasted into the menu, or a colour typed as
    /// hex: escape leaves, enter takes it.
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

    /// Repaint in whatever the choice currently comes out as.
    ///
    /// Called every frame, because "systemfarben" has to notice the Mac being
    /// switched between light and dark while the window is open — and because
    /// doing nothing when nothing changed costs one comparison.
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
        self.dress_the_bar(ctx);
        self.dress_the_icon(ctx);
        let lang = crate::lang::current();
        if self.titled != Some(lang) {
            self.titled = Some(lang);
            let title = if crate::DEV { format!("{} dev", crate::lang::app_name()) } else { crate::lang::app_name().to_string() };
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        }
    }

    /// The icon in the window's colours and its typeface — see
    /// [`appicon`]. Drawn when something it is made of has changed,
    /// and not before the window has a face to draw the k in.
    ///
    /// Drawn on a thread of its own and handed to the system on a later
    /// frame: a tile of 512 points a side with its letter takes a good tenth
    /// of a second, and drawn here it held up the frame light turned to dark
    /// in — the change stood still before it began to fade.
    pub(in crate::gui) fn dress_the_icon(&mut self, ctx: &egui::Context) {
        let size = appicon::SIDE;
        if let Some(coming) = &self.icon_coming {
            match coming.try_recv() {
                Ok(rgba) => {
                    self.icon_coming = None;
                    if !crate::media::set_dock_icon(&appicon::premultiplied(&rgba), size, size) {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Icon(Some(std::sync::Arc::new(egui::IconData {
                            rgba,
                            width: size as u32,
                            height: size as u32,
                        }))));
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.icon_coming = None,
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        let (ground, accent) = self.resolved.icon_colours();
        let wanted = (ground, accent, self.typeface.clone(), crate::lang::current());
        if self.icon_dressed.as_ref() == Some(&wanted) {
            return;
        }
        if appicon::small_letter(ctx, 16).is_none() {
            return;
        }
        self.icon_dressed = Some(wanted);
        let (send, take) = std::sync::mpsc::channel();
        self.icon_coming = Some(take);
        let ctx = ctx.clone();
        let shape = appicon::Shape::here();
        std::thread::Builder::new()
            .name("kessler-icon".into())
            .spawn(move || {
                let Some(letter) = appicon::letter(&ctx, size) else { return };
                let rgba = appicon::compose(size, shape, ground, accent, Some(&letter));
                if send.send(rgba).is_ok() {
                    ctx.request_repaint();
                }
            })
            .ok();
    }

    /// On Linux, keep up with GNOME's light-or-dark setting.
    ///
    /// The window system there does not tell a program which way round the
    /// desktop is, so „systemfarben" would sit in the light for ever. GNOME
    /// keeps it in one setting, and `gsettings monitor` says each time it
    /// changes — one sleeping child process, started the first time the
    /// scheme follows the system and not before.
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

    /// Tell the window frame which way round the window is.
    ///
    /// The bar with the three buttons and the program's name is drawn by the
    /// operating system, not by this program, and it follows the window's own
    /// appearance rather than anything painted inside it. A dark scheme under
    /// a light title bar is the one seam in the window that cannot be painted
    /// over, so the window itself gets told.
    ///
    /// „systemfarben" hands back `SystemDefault` rather than whichever colour
    /// it happens to have resolved to, and that is not a shortcut: pinning the
    /// window's appearance stops it following the system, and the system theme
    /// this program reads is the *window's*. Pinning it would answer its own
    /// question from then on, and a Mac switched to light in the evening would
    /// leave the window sitting in the dark.
    /// On Linux, the title bar on or off as `[ui] window_bar` asks: told to
    /// the window when it changes, not every frame.
    pub(in crate::gui) fn dress_the_bar(&mut self, ctx: &egui::Context) {
        if !cfg!(target_os = "linux") {
            return;
        }
        let wanted = self.config.ui.window_bar != "ohne";
        if self.bar_worn != Some(wanted) {
            self.bar_worn = Some(wanted);
            ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(wanted));
        }
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

    /// From light to dark and back, and nothing else. Every scheme
    /// has its other half, so this stays in the scheme; it leaves „system" for
    /// the side that is not showing.
    pub fn toggle_light_dark(&mut self, ctx: &egui::Context) {
        let to = if self.resolved.palette().is_dark() { Brightness::Hell } else { Brightness::Dunkel };
        self.apply_theme(ctx, self.resolved, to);
        self.set_status(tr!("farben: {} · {}", crate::lang::t(self.theme.family()), crate::lang::t(to.name())));
    }

    /// The next scheme in the menu's order, round again at the end, in the half
    /// that is showing: light stays light and dark stays dark, so leafing
    /// through is not a flicker between the two.
    ///
    /// `back` steps the other way, for `alt` + `shift` + `f`.
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

    /// The whole screen and back. The operating system draws the change
    /// and keeps the state; this only asks for the other one.
    pub fn toggle_fullscreen(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!now));
    }

    /// Draw the whole window at this size and keep it (menu → grösse, and
    /// `cmd` + `+` `-` `0`).
    ///
    /// Not at once: behind a short dip to the ground — see [`ZoomDip`].
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

    /// The size the window is at, or is about to be at.
    fn zoom_wanted(&self) -> f32 {
        match self.zoom_dip {
            Some(ZoomDip::Out { to, .. } | ZoomDip::Held { to }) => to,
            _ => crate::gui::menu::zoom_of(self.config.ui.zoom),
        }
    }

    /// Run the dip a frame on, and say how much of the ground lies over the
    /// window now, 0 to 1.
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

    /// A notch bigger or smaller, from the keyboard, and say where it is.
    pub fn zoom_by(&mut self, ctx: &egui::Context, by: i32) {
        let zoom = match by {
            0 => 1.0,
            by => crate::gui::menu::zoom_step(self.zoom_wanted(), by),
        };
        self.set_zoom(ctx, zoom);
        self.set_status(tr!("grösse: {} %", (zoom * 100.0).round() as i32));
    }

    /// Switch one optional track column on or off.
    /// The next typeface in the `fonts` folder, set at once: the faces are
    /// read and handed to egui again, and the transport's marks are drawn
    /// again out of the new face on the next frame.
    ///
    /// `back` steps the other way, for the long list the machine's own faces
    /// make (⇧-click).
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

    /// Offer this machine's own typefaces in the menu too, or stop. Stopping
    /// while one of them is set goes back to the one the window opens in, since
    /// the list no longer holds it.
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

    /// Every setting of the menu back to how kessler first opens: the `[ui]`
    /// table, but for the parts switched on under „allgemein" and where the
    /// fonts are looked for. The music's folders, last.fm, the iPod and the
    /// player stay.
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

    /// Set the window in the family `key` names — see [`theme::the_typeface`].
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

    /// Set the typeface this wide on its width axis, and keep it (menu →
    /// darstellung → breite) — the faces installed again at the new width.
    pub fn set_font_width(&mut self, ctx: &egui::Context, width: f32) {
        self.config.ui.font_width = width;
        let key = self.config.ui.typeface.clone();
        self.set_typeface(ctx, &key);
    }

    /// Set the regular this heavy on the weight axis, the bold with it, and
    /// keep it (menu → schrift → gewicht).
    pub fn set_font_weight(&mut self, ctx: &egui::Context, weight: f32) {
        self.config.ui.font_weight = weight;
        let key = self.config.ui.typeface.clone();
        self.set_typeface(ctx, &key);
    }

    /// The alternate forms the face in use offers — see
    /// [`theme::style_sets`] — read from its file once per face.
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

    /// The face's alternate `tag` on or off (menu → schrift → alternativen),
    /// and the faces installed again with it.
    ///
    /// Two alternates of the same character are one or the other, never both:
    /// switching one on switches off the other.
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

    /// The same, for something that happened without anybody asking.
    ///
    /// A message about what you just did may go as soon as you have read it —
    /// you knew what it would say. A scrobble is the other kind: it reports
    /// something the program did on its own, at the bottom edge of a window
    /// whose owner is looking at the middle, and four seconds is not enough to
    /// catch it.
    pub fn set_notice(&mut self, message: impl Into<String>) {
        self.status = Some((message.into(), Instant::now(), NOTICE_SECS));
    }

    /// Where the song on the air stands with last.fm, by the radio's own rule:
    /// a song heard from its start counts once it has been heard thirty
    /// seconds, when the next one comes or after four minutes; the one under
    /// way when the station was tuned in, or one without an artist, never.
    /// `None` while no station plays or nothing goes to last.fm from the radio.
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
        let heard = now.heard_secs(chrono::Local::now(), self.radio_paused_since);
        let needed = if now.from_start { super::radio::RADIO_SCROBBLE_AFTER } else { crate::lastfm::watch::ENOUGH_SECS as i64 };
        Some(match needed - heard {
            left if left > 0 => OnAirScrobble::Listening(left as u64),
            left if left > -COUNTS_SAID_SECS => OnAirScrobble::Counts,
            _ => return None,
        })
    }

    /// Where the track being played stands with Last.fm, in words.
    ///
    /// The one thing the program did entirely out of sight: a scrobble is
    /// earned four minutes in and reported for twelve seconds, which is a
    /// report you see only by chance. Said as a standing line, the rule
    /// explains itself — the figure counts down, and then it says it counted.
    ///
    /// Nothing at all without an account: a line about a service nobody signed
    /// in to is noise, and the menu already says so where it belongs.
    pub fn scrobble_standing(&self) -> Option<String> {
        self.scrobble_said(false)
    }

    /// The line itself, long or short. Paused, nothing: a count standing
    /// still at the foot read as a clock gone wrong. It comes back, where it
    /// stopped, when the music plays on.
    fn scrobble_said(&self, short: bool) -> Option<String> {
        use crate::lastfm::watch::Standing;
        let scrobbler = self.scrobbler.as_ref()?;
        let paused = self.player.state() == crate::player::PlaybackState::Paused;
        let counting = |left: u64| tr!("last.fm  zählt in {}", crate::gui::text::duration(left));
        if !paused {
            if let Some(on_air) = self.radio_scrobble_standing() {
                return Some(match on_air {
                    OnAirScrobble::Listening(left) => counting(left),
                    OnAirScrobble::Counts if short => tr!("last.fm  zählt").to_string(),
                    OnAirScrobble::Counts => tr!("last.fm  zählt beim nächsten lied").to_string(),
                    OnAirScrobble::Never if short => tr!("last.fm  nicht").to_string(),
                    OnAirScrobble::Never => tr!("last.fm  zählt nicht").to_string(),
                });
            }
            match self.scrobble_watch.standing() {
                Some(Standing::Counted) => return Some(tr!("last.fm  gezählt").to_string()),
                Some(Standing::Never) if short => return Some(tr!("last.fm  nicht").to_string()),
                Some(Standing::Never) => return Some(tr!("last.fm  zählt nicht").to_string()),
                Some(Standing::Listening { left }) => return Some(counting(left.ceil() as u64)),
                None => {}
            }
        }
        match scrobbler.waiting() {
            0 => None,
            1 if !short => Some(tr!("last.fm  1 wartet").to_string()),
            n => Some(tr!("last.fm  {} warten", n)),
        }
    }

    /// The whole library in one line, for the foot of the compact window.
    /// Nothing while it is empty.
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

    /// The same, short enough for the compact header line: the time left and
    /// nothing else while a track is being listened to — „last.fm 2:10".
    pub fn scrobble_short(&self) -> Option<String> {
        self.scrobble_said(true)
    }

    /// What the words in the search field found, for as long as they stand
    /// there.
    ///
    /// The foot of the window is where this program says what is true right
    /// now — last.fm, the device, a scan — and a search is exactly that kind
    /// of fact: true while the words are in the field, gone when they are not.
    /// So it is a standing line rather than news, and anything that just
    /// happened still takes the line for its few seconds.
    ///
    /// All three figures, because the words are searched for in all three
    /// columns at once and „47" alone would not say of what. A column that
    /// found nothing is left out rather than printed as a nought: the line is
    /// read at a glance, and three noughts to skip over is three too many.
    pub fn search_standing(&self) -> Option<String> {
        if self.showing_podcasts() {
            return None;
        }
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

    /// How far the run through the artists has got, while it runs — for
    /// the status line, where it stands until it is done.
    pub fn artist_hunt_standing(&self) -> Option<String> {
        let hunt = self.artist_hunt.as_ref()?;
        Some(tr!(
            "künstler:innen {} / {}",
            text::count(hunt.asked as u64),
            text::count(hunt.total as u64)
        ))
    }

    /// How far a copy to the iPod has got, while it runs — for the status
    /// line, so it can be followed with the log lying over the iPod's
    /// column.
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

    /// The status line, while it is still fresh.
    pub fn status(&self) -> Option<&str> {
        self.status.as_ref().and_then(|(msg, at, secs)| {
            (at.elapsed() < std::time::Duration::from_secs(*secs)).then_some(msg.as_str())
        })
    }
}

/// Where the song on the air stands with last.fm — see
/// [`GuiApp::radio_scrobble_standing`]. How long „last.fm zählt" stands once a
/// station's song has been heard long enough.
const COUNTS_SAID_SECS: i64 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::gui) enum OnAirScrobble {
    /// Seconds still to hear before it counts.
    Listening(u64),
    /// Heard long enough: it goes when the next song comes, or at four
    /// minutes.
    Counts,
    Never,
}
