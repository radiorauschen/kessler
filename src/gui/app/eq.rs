use crate::gui::*;
use crate::player::eq;

impl GuiApp {
    pub(in crate::gui) fn apply_eq(&mut self) {
        eq::SETTINGS.set(self.config.ui.eq_on, eq::gains_from_text(&self.config.ui.eq_gains));
    }

    pub(in crate::gui) fn eq_gains(&self) -> [f32; eq::COUNT] {
        eq::gains_from_text(&self.config.ui.eq_gains)
    }

    pub(in crate::gui) fn eq_own(&self, slot: usize) -> Option<[f32; eq::COUNT]> {
        let text = if slot == 0 { &self.config.ui.eq_own_1 } else { &self.config.ui.eq_own_2 };
        (!text.trim().is_empty()).then(|| eq::gains_from_text(text))
    }

    pub(in crate::gui) fn choose_eq_own(&mut self, slot: usize) {
        let gains = self.eq_own(slot).unwrap_or_else(|| self.eq_gains());
        self.config.ui.eq_slot = slot as u8 + 1;
        self.write_eq_own(slot, &gains);
        self.set_eq_gains(gains, true);
    }

    fn write_eq_own(&mut self, slot: usize, gains: &[f32; eq::COUNT]) {
        let text = eq::gains_to_text(gains);
        if slot == 0 {
            self.config.ui.eq_own_1 = text;
        } else {
            self.config.ui.eq_own_2 = text;
        }
    }

    pub fn toggle_eq(&mut self) {
        if self.eq_open {
            self.eq_open = false;
            self.follow_the_playing();
            return;
        }
        if !self.all_sides() {
            if self.device_shown() {
                self.close_device_panel();
            }
            self.log_open = false;
            self.artist_open = false;
            self.artist_form = None;
            self.lyrics_open = false;
            self.gaps.open = false;
        }
        self.eq_open = true;
        self.podcasts.info_open = false;
        self.leave_the_left();
    }

    pub(in crate::gui) fn set_eq_on(&mut self, on: bool) {
        self.config.ui.eq_on = on;
        self.apply_eq();
        self.save_config();
    }

    pub(in crate::gui) fn set_eq_gains(&mut self, gains: [f32; eq::COUNT], keep: bool) {
        self.config.ui.eq_gains = eq::gains_to_text(&gains);
        if let Some(slot) = (self.config.ui.eq_slot as usize).checked_sub(1).filter(|s| *s < 2) {
            self.write_eq_own(slot, &gains);
        }
        self.config.ui.eq_on = true;
        self.apply_eq();
        if keep {
            self.save_config();
        }
    }
}
