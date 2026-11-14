use zbus::zvariant::{OwnedValue, Value};

const NAME: &str = "org.mpris.MediaPlayer2.kessler";
const PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";

pub const COMMANDS: [(&str, &str); 6] = [
    ("play", "Play"),
    ("pause", "Pause"),
    ("toggle", "PlayPause"),
    ("next", "Next"),
    ("prev", "Previous"),
    ("stop", "Stop"),
];

fn proxy() -> Result<zbus::blocking::Proxy<'static>, String> {
    let connection = zbus::blocking::Connection::session().map_err(|e| format!("keine sitzung ({e})"))?;
    zbus::blocking::Proxy::new(&connection, NAME, PATH, PLAYER).map_err(|e| e.to_string())
}

fn said(error: zbus::Error) -> String {
    let text = error.to_string();
    if text.contains("ServiceUnknown") || text.contains("NameHasNoOwner") || text.contains("was not provided") {
        "kessler läuft nicht".into()
    } else {
        text
    }
}

pub fn send(command: &str) -> Result<(), String> {
    let (_, method) = COMMANDS
        .iter()
        .find(|(name, _)| *name == command)
        .ok_or_else(|| format!("unbekannt: {command}"))?;
    proxy()?.call_method(*method, &()).map(|_| ()).map_err(said)
}

pub fn now() -> Result<Option<String>, String> {
    let proxy = proxy()?;
    let status: String = proxy.get_property("PlaybackStatus").map_err(said)?;
    if status == "Stopped" {
        return Ok(None);
    }
    let metadata: std::collections::HashMap<String, OwnedValue> =
        proxy.get_property("Metadata").map_err(said)?;
    Ok(line(&metadata, status == "Paused"))
}

fn line(metadata: &std::collections::HashMap<String, OwnedValue>, paused: bool) -> Option<String> {
    let text = |key: &str| -> Option<String> {
        match metadata.get(key).map(|v| &**v) {
            Some(Value::Str(s)) => Some(s.to_string()),
            Some(Value::Array(items)) => items.iter().find_map(|item| match item {
                Value::Str(s) => Some(s.to_string()),
                _ => None,
            }),
            _ => None,
        }
    };
    let title = text("xesam:title").filter(|t| !t.is_empty())?;
    let mut said = match text("xesam:artist").filter(|a| !a.is_empty()) {
        Some(artist) => format!("{artist} — {title}"),
        None => title,
    }
    .to_lowercase();
    if paused {
        said.push_str(" (angehalten)");
    }
    Some(said)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owned(value: Value<'_>) -> OwnedValue {
        OwnedValue::try_from(value).expect("plain value")
    }

    #[test]
    fn what_plays_is_one_line() {
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("xesam:title".to_string(), owned(Value::from("Foil")));
        metadata.insert("xesam:artist".to_string(), owned(Value::from(vec!["Autechre".to_string()])));
        assert_eq!(line(&metadata, false).as_deref(), Some("autechre — foil"));
        assert_eq!(line(&metadata, true).as_deref(), Some("autechre — foil (angehalten)"));
        metadata.remove("xesam:artist");
        assert_eq!(line(&metadata, false).as_deref(), Some("foil"));
        assert_eq!(line(&std::collections::HashMap::new(), false), None);
        assert!(COMMANDS.iter().any(|(name, _)| *name == "toggle"));
    }
}
