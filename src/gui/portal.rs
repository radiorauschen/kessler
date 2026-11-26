//! Light or dark, as the Linux desktop says it — by way of the desktop
//! portal, which GNOME, KDE and the portals of sway and Hyprland all answer
//! alike (`org.freedesktop.appearance` `color-scheme`).
//!
//! `gsettings` is GNOME's own: under KDE or a tiling window manager it answers
//! nothing, or what GNOME would have said, so it is only asked where there is
//! no portal.

use std::sync::mpsc::Sender;

use eframe::egui;
use zbus::zvariant::Value;

const DESTINATION: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
const INTERFACE: &str = "org.freedesktop.portal.Settings";
const NAMESPACE: &str = "org.freedesktop.appearance";
const KEY: &str = "color-scheme";

/// The portal's answer as light or dark: 1 is „prefer dark", 2 „prefer
/// light", and 0 „no preference", which is how a desktop left as it came
/// answers — light, everywhere it is asked.
fn theme_of(code: u32) -> egui::Theme {
    if code == 1 {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    }
}

/// The number inside whatever the portal wraps it in: `ReadOne` hands back
/// the value, the older `Read` a value inside a value.
fn code_in(value: &Value<'_>) -> Option<u32> {
    match value {
        Value::U32(code) => Some(*code),
        Value::Value(inner) => code_in(inner),
        _ => None,
    }
}

fn proxy(connection: &zbus::blocking::Connection) -> zbus::Result<zbus::blocking::Proxy<'static>> {
    zbus::blocking::Proxy::new(connection, DESTINATION, PATH, INTERFACE)
}

/// What the desktop is set to now; `None` without a portal that knows.
pub fn read() -> Option<egui::Theme> {
    let connection = zbus::blocking::Connection::session().ok()?;
    let proxy = proxy(&connection).ok()?;
    let value: zbus::zvariant::OwnedValue = proxy
        .call("ReadOne", &(NAMESPACE, KEY))
        .or_else(|_| proxy.call("Read", &(NAMESPACE, KEY)))
        .ok()?;
    code_in(&value).map(theme_of)
}

/// Send every change of the desktop's setting, and wake the window for it,
/// for as long as the window listens. Returns at once with `false` when
/// there is no portal to listen to, so the caller can ask `gsettings`
/// instead; otherwise only when the window has gone.
pub fn watch(tx: &Sender<egui::Theme>, wake: &egui::Context) -> bool {
    let Ok(connection) = zbus::blocking::Connection::session() else {
        return false;
    };
    let Ok(proxy) = proxy(&connection) else {
        return false;
    };
    let readable: zbus::Result<zbus::zvariant::OwnedValue> = proxy
        .call("ReadOne", &(NAMESPACE, KEY))
        .or_else(|_| proxy.call("Read", &(NAMESPACE, KEY)));
    if readable.is_err() {
        return false;
    }
    let Ok(changes) = proxy.receive_signal("SettingChanged") else {
        return false;
    };
    for message in changes {
        let Ok((namespace, key, value)) =
            message.body().deserialize::<(String, String, zbus::zvariant::OwnedValue)>()
        else {
            continue;
        };
        if namespace != NAMESPACE || key != KEY {
            continue;
        }
        let Some(code) = code_in(&value) else { continue };
        if tx.send(theme_of(code)).is_err() {
            break;
        }
        wake.request_repaint();
    }
    true
}

/// The accent colour, under the same name: three shares of red, green and
/// blue between 0 and 1 — GNOME's and Ubuntu's accent colours, KDE's too.
/// Anything outside that is „none chosen".
const ACCENT_KEY: &str = "accent-color";

/// The colour inside whatever the portal wraps it in.
fn accent_in(value: &Value<'_>) -> Option<egui::Color32> {
    match value {
        Value::Value(inner) => accent_in(inner),
        Value::Structure(fields) => {
            let shares: Vec<f64> = fields
                .fields()
                .iter()
                .filter_map(|f| match f {
                    Value::F64(v) => Some(*v),
                    _ => None,
                })
                .collect();
            let [r, g, b] = shares.as_slice() else { return None };
            if [r, g, b].iter().any(|v| !(0.0..=1.0).contains(*v)) {
                return None;
            }
            let byte = |v: f64| (v * 255.0).round() as u8;
            Some(egui::Color32::from_rgb(byte(*r), byte(*g), byte(*b)))
        }
        _ => None,
    }
}

/// Read the desktop's accent now, and keep `theme`'s copy of it up to date
/// for as long as the program runs — on a thread of its own, the window woken
/// at each change. Nothing where there is no portal, or one that has no
/// accent to say.
pub fn follow_accent(wake: egui::Context) {
    let _ = std::thread::Builder::new().name("kessler-accent".into()).spawn(move || {
        let Ok(connection) = zbus::blocking::Connection::session() else { return };
        let Ok(proxy) = proxy(&connection) else { return };
        let now: zbus::Result<zbus::zvariant::OwnedValue> = proxy
            .call("ReadOne", &(NAMESPACE, ACCENT_KEY))
            .or_else(|_| proxy.call("Read", &(NAMESPACE, ACCENT_KEY)));
        let Ok(now) = now else { return };
        super::theme::set_desktop_accent(accent_in(&now));
        wake.request_repaint();
        let Ok(changes) = proxy.receive_signal("SettingChanged") else { return };
        for message in changes {
            let Ok((namespace, key, value)) =
                message.body().deserialize::<(String, String, zbus::zvariant::OwnedValue)>()
            else {
                continue;
            };
            if namespace != NAMESPACE || key != ACCENT_KEY {
                continue;
            }
            super::theme::set_desktop_accent(accent_in(&value));
            wake.request_repaint();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_portal_numbers_are_read() {
        assert_eq!(theme_of(1), egui::Theme::Dark);
        assert_eq!(theme_of(2), egui::Theme::Light);
        assert_eq!(theme_of(0), egui::Theme::Light);
        assert_eq!(code_in(&Value::U32(1)), Some(1));
        assert_eq!(code_in(&Value::Value(Box::new(Value::U32(2)))), Some(2));
        assert_eq!(code_in(&Value::Str("dark".into())), None);
    }

    #[test]
    fn the_accent_is_three_shares() {
        let orange = Value::from((0.914_f64, 0.329_f64, 0.125_f64));
        assert_eq!(accent_in(&orange), Some(egui::Color32::from_rgb(233, 84, 32)));
        let none = Value::from((-1.0_f64, -1.0_f64, -1.0_f64));
        assert_eq!(accent_in(&none), None);
    }
}
