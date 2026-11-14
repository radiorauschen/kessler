use eframe::egui;

use super::tagedit::Entry;
use super::GuiApp;

pub const APPLY_URL: &str = "https://www.last.fm/api/account/create";

pub struct KeyForm {
    pub entries: [Entry; 2],
    pub focus: usize,
}

pub const LABELS: [&str; 2] = ["schlüssel", "geheimnis"];

impl KeyForm {
    pub fn open(key: Option<&str>, secret: Option<&str>) -> Self {
        KeyForm {
            entries: [Entry::new(key.unwrap_or_default()), Entry::new(secret.unwrap_or_default())],
            focus: 0,
        }
    }

    pub fn step(&mut self) {
        self.focus = (self.focus + 1) % self.entries.len();
        self.entries[self.focus].end();
    }

    pub fn entry(&mut self) -> &mut Entry {
        &mut self.entries[self.focus]
    }

    pub fn values(&self) -> std::result::Result<(String, String), usize> {
        let key = tidy(self.entries[0].text());
        let secret = tidy(self.entries[1].text());
        if !looks_like_key(&key) {
            return Err(0);
        }
        if !looks_like_key(&secret) {
            return Err(1);
        }
        Ok((key, secret))
    }
}

fn tidy(text: &str) -> String {
    let text = text.trim();
    let text = text.rsplit(':').next().unwrap_or(text);
    text.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_lowercase()
}

fn looks_like_key(text: &str) -> bool {
    text.len() == 32 && text.chars().all(|c| c.is_ascii_hexdigit())
}

impl GuiApp {
    pub fn open_lastfm_keys(&mut self) {
        self.lastfm_keys = Some(KeyForm::open(
            self.config.lastfm.api_key.as_deref(),
            self.config.lastfm.api_secret.as_deref(),
        ));
    }

    pub fn save_lastfm_keys(&mut self) {
        let Some(form) = self.lastfm_keys.as_mut() else {
            return;
        };
        match form.values() {
            Err(field) => {
                form.focus = field;
                let which = crate::lang::t(LABELS[field]);
                self.set_status(tr!("{}: 32 zeichen, nur 0–9 und a–f", which));
            }
            Ok((key, secret)) => {
                self.lastfm_keys = None;
                let changed = self.config.lastfm.api_key.as_deref() != Some(key.as_str())
                    || self.config.lastfm.api_secret.as_deref() != Some(secret.as_str());
                if !changed {
                    return;
                }
                self.config.lastfm.api_key = Some(key);
                self.config.lastfm.api_secret = Some(secret);
                self.config.lastfm.session_key = None;
                self.config.lastfm.username = None;
                self.scrobbler = None;
                self.save_config();
                self.connect_lastfm();
            }
        }
    }

    pub(super) fn handle_lastfm_keys(&mut self, ctx: &egui::Context) {
        use egui::{Key, Modifiers};
        const NONE: Modifiers = Modifiers::NONE;
        let taken =
            |m: Modifiers, k: Key| -> usize { ctx.input_mut(|i| i.count_and_consume_key(m, k)) };
        let pressed = |m: Modifiers, k: Key| -> bool { ctx.input_mut(|i| i.consume_key(m, k)) };

        if pressed(NONE, Key::Escape) {
            self.lastfm_keys = None;
            return;
        }
        if pressed(NONE, Key::Enter) {
            self.save_lastfm_keys();
            return;
        }
        let cleared = pressed(Modifiers::COMMAND, Key::Backspace);
        let typed = super::typed(ctx, false);
        let Some(form) = self.lastfm_keys.as_mut() else {
            return;
        };
        if cleared {
            form.entry().clear();
        }
        for _ in 0..taken(NONE, Key::Tab)
            + taken(Modifiers::SHIFT, Key::Tab)
            + taken(NONE, Key::ArrowDown)
            + taken(NONE, Key::ArrowUp)
        {
            form.step();
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
        if !typed.is_empty() {
            form.entry().insert(&typed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn a_pasted_key_is_tidied_and_checked() {
        let mut form = KeyForm::open(None, None);
        form.entry().insert(&format!("API key: {}\n", KEY.to_uppercase()));
        form.step();
        form.entry().insert(&format!("  {KEY} "));
        assert_eq!(form.values(), Ok((KEY.to_string(), KEY.to_string())));
    }

    #[test]
    fn the_wrong_field_is_named() {
        let mut form = KeyForm::open(Some(KEY), Some("abc"));
        assert_eq!(form.values(), Err(1));
        form.entries[0] = Entry::new("xyz");
        assert_eq!(form.values(), Err(0));
    }

    #[test]
    fn tab_goes_round() {
        let mut form = KeyForm::open(None, None);
        form.step();
        assert_eq!(form.focus, 1);
        form.step();
        assert_eq!(form.focus, 0);
    }
}
