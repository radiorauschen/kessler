use std::sync::mpsc::Sender;

use eframe::egui;
use zbus::zvariant::Value;

const DESTINATION: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
const INTERFACE: &str = "org.freedesktop.portal.Settings";
const NAMESPACE: &str = "org.freedesktop.appearance";
const KEY: &str = "color-scheme";

fn theme_of(code: u32) -> egui::Theme {
    if code == 1 {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    }
}

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

pub fn read() -> Option<egui::Theme> {
    let connection = zbus::blocking::Connection::session().ok()?;
    let proxy = proxy(&connection).ok()?;
    let value: zbus::zvariant::OwnedValue = proxy
        .call("ReadOne", &(NAMESPACE, KEY))
        .or_else(|_| proxy.call("Read", &(NAMESPACE, KEY)))
        .ok()?;
    code_in(&value).map(theme_of)
}

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

const ACCENT_KEY: &str = "accent-color";

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
