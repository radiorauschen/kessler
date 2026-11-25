use std::time::Duration;

use eframe::egui::{self, Color32};

const EVERY: Duration = Duration::from_secs(3);

fn colour_of(code: i32) -> Option<Color32> {
    Some(match code {
        -1 => Color32::from_rgb(0x8E, 0x8E, 0x93),
        0 => Color32::from_rgb(0xFF, 0x3B, 0x30),
        1 => Color32::from_rgb(0xFF, 0x95, 0x00),
        2 => Color32::from_rgb(0xFF, 0xCC, 0x00),
        3 => Color32::from_rgb(0x34, 0xC7, 0x59),
        4 => Color32::from_rgb(0x00, 0x7A, 0xFF),
        5 => Color32::from_rgb(0xAF, 0x52, 0xDE),
        6 => Color32::from_rgb(0xFF, 0x2D, 0x55),
        _ => return None,
    })
}

fn read() -> Option<Color32> {
    let answer = std::process::Command::new("defaults").args(["read", "-g", "AppleAccentColor"]).output().ok()?;
    if !answer.status.success() {
        return None;
    }
    String::from_utf8_lossy(&answer.stdout).trim().parse().ok().and_then(colour_of)
}

pub fn follow(wake: egui::Context) {
    let _ = std::thread::Builder::new().name("kessler-accent".into()).spawn(move || {
        let mut last: Option<Option<Color32>> = None;
        loop {
            let now = read();
            if last != Some(now) {
                last = Some(now);
                super::theme::set_desktop_accent(now);
                wake.request_repaint();
            }
            std::thread::sleep(EVERY);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_numbers_are_the_systems_colours() {
        assert_eq!(colour_of(4), Some(Color32::from_rgb(0x00, 0x7A, 0xFF)));
        assert_eq!(colour_of(-1), Some(Color32::from_rgb(0x8E, 0x8E, 0x93)));
        assert_eq!(colour_of(7), None);
    }
}
