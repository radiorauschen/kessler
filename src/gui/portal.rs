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
}
