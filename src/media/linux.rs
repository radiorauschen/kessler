use super::{Command, NowPlaying};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};

const NAME: &str = "org.mpris.MediaPlayer2.kessler";
const PATH: &str = "/org/mpris/MediaPlayer2";

#[derive(Default)]
struct Shared {
    commands: VecDeque<Command>,
    now: Option<NowPlaying>,
    at: Option<Instant>,
}

fn shared() -> &'static Mutex<Shared> {
    static SHARED: OnceLock<Mutex<Shared>> = OnceLock::new();
    SHARED.get_or_init(Default::default)
}

static CONNECTION: OnceLock<zbus::blocking::Connection> = OnceLock::new();

fn push(command: Command) {
    if let Ok(mut shared) = shared().lock() {
        shared.commands.push_back(command);
    }
    super::kessler_media_woken();
}

pub fn start() {
    let spawned = std::thread::Builder::new()
        .name("mpris".into())
        .spawn(|| {
            let connection = zbus::blocking::connection::Builder::session()
                .and_then(|b| b.name(NAME))
                .and_then(|b| b.serve_at(PATH, Root))
                .and_then(|b| b.serve_at(PATH, Player))
                .and_then(|b| b.build());
            match connection {
                Ok(connection) => {
                    let _ = CONNECTION.set(connection);
                    announce(false);
                }
                Err(e) => eprintln!("kessler: keine medientasten ({e})"),
            }
        });
    if let Err(e) = spawned {
        eprintln!("kessler: keine medientasten ({e})");
    }
}

pub fn poll() -> Option<Command> {
    shared().lock().ok()?.commands.pop_front()
}

static NOTIFY: AtomicBool = AtomicBool::new(false);
static IN_FRONT: AtomicBool = AtomicBool::new(true);
static SHOWN: AtomicU32 = AtomicU32::new(0);

pub fn set_notify(on: bool, in_front: bool) {
    NOTIFY.store(on, Ordering::Relaxed);
    IN_FRONT.store(in_front, Ordering::Relaxed);
}

fn notify(now: &NowPlaying) {
    let Some(connection) = CONNECTION.get().cloned() else {
        return;
    };
    let summary = now.title.to_lowercase();
    let body = [now.artist.as_str(), now.album.as_str()]
        .iter()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_lowercase())
        .collect::<Vec<_>>()
        .join(" — ");
    let image = now.artwork.as_deref().map(file_url);
    let _ = std::thread::Builder::new().name("notify".into()).spawn(move || {
        let mut hints: HashMap<&str, Value> = HashMap::new();
        if let Some(image) = image {
            hints.insert("image-path", Value::from(image));
        }
        hints.insert("category", Value::from("x-gnome.music"));
        let replaces = SHOWN.load(Ordering::Relaxed);
        let reply = connection.call_method(
            Some("org.freedesktop.Notifications"),
            "/org/freedesktop/Notifications",
            Some("org.freedesktop.Notifications"),
            "Notify",
            &("kessler", replaces, "kessler", summary, body, Vec::<&str>::new(), hints, -1i32),
        );
        if let Ok(reply) = reply {
            if let Ok(id) = reply.body().deserialize::<u32>() {
                SHOWN.store(id, Ordering::Relaxed);
            }
        }
    });
}

pub fn update(now: Option<&NowPlaying>) {
    let jumped = {
        let Ok(mut shared) = shared().lock() else {
            return;
        };
        let new_track = now.is_some_and(|next| {
            next.playing
                && !matches!(&shared.now, Some(last) if last.title == next.title
                    && last.artist == next.artist && last.album == next.album)
        });
        if new_track && NOTIFY.load(Ordering::Relaxed) && !IN_FRONT.load(Ordering::Relaxed) {
            if let Some(now) = now {
                notify(now);
            }
        }
        let jumped = matches!((&shared.now, now), (Some(last), Some(next))
            if last.title == next.title && last.album == next.album && last.playing == next.playing);
        shared.now = now.cloned();
        shared.at = Some(Instant::now());
        jumped
    };
    announce(jumped);
}

fn announce(jumped: bool) {
    let Some(connection) = CONNECTION.get() else {
        return;
    };
    let Ok(player) = connection.object_server().interface::<_, Player>(PATH) else {
        return;
    };
    let emitter = player.signal_emitter();
    let iface = player.get();
    let _ = zbus::block_on(async {
        iface.playback_status_changed(emitter).await?;
        iface.metadata_changed(emitter).await?;
        if jumped {
            Player::seeked(emitter, position()).await?;
        }
        Ok::<_, zbus::Error>(())
    });
}

fn position() -> i64 {
    let Ok(shared) = shared().lock() else {
        return 0;
    };
    let Some(now) = &shared.now else {
        return 0;
    };
    let since = match (now.playing, shared.at) {
        (true, Some(at)) => at.elapsed().as_secs_f64(),
        _ => 0.0,
    };
    micros((now.elapsed + since).min(now.duration.max(0.0)))
}

fn micros(seconds: f64) -> i64 {
    (seconds.max(0.0) * 1_000_000.0) as i64
}

fn metadata(now: Option<&NowPlaying>) -> HashMap<String, OwnedValue> {
    let mut map = HashMap::new();
    let Some(now) = now else {
        put(&mut map, "mpris:trackid", ObjectPath::from_static_str_unchecked("/org/mpris/MediaPlayer2/TrackList/NoTrack"));
        return map;
    };
    put(&mut map, "mpris:trackid", track_id(now));
    put(&mut map, "mpris:length", micros(now.duration));
    put(&mut map, "xesam:title", now.title.clone());
    put(&mut map, "xesam:artist", vec![now.artist.clone()]);
    put(&mut map, "xesam:album", now.album.clone());
    if let Some(path) = &now.artwork {
        put(&mut map, "mpris:artUrl", file_url(path));
    }
    map
}

fn put<'a>(map: &mut HashMap<String, OwnedValue>, key: &str, value: impl Into<Value<'a>>) {
    if let Ok(owned) = OwnedValue::try_from(value.into()) {
        map.insert(key.to_string(), owned);
    }
}

fn track_id(now: &NowPlaying) -> ObjectPath<'static> {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (&now.title, &now.artist, &now.album).hash(&mut hasher);
    ObjectPath::try_from(format!("/org/kessler/track/t{:016x}", hasher.finish()))
        .unwrap_or_else(|_| ObjectPath::from_static_str_unchecked("/org/kessler/track"))
}

fn file_url(path: &std::path::Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut url = String::from("file://");
    for &byte in path.as_os_str().as_bytes() {
        if byte.is_ascii_alphanumeric() || b"/-_.~".contains(&byte) {
            url.push(byte as char);
        } else {
            url.push_str(&format!("%{byte:02X}"));
        }
    }
    url
}

struct Root;

#[zbus::interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    fn raise(&self) {}

    fn quit(&self) {}

    #[zbus(property)]
    fn can_quit(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn can_raise(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn identity(&self) -> String {
        "kessler".into()
    }

    #[zbus(property)]
    fn desktop_entry(&self) -> String {
        "kessler".into()
    }

    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<String> {
        Vec::new()
    }

    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<String> {
        Vec::new()
    }
}

struct Player;

#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl Player {
    fn next(&self) {
        push(Command::Next);
    }

    fn previous(&self) {
        push(Command::Previous);
    }

    fn pause(&self) {
        push(Command::Pause);
    }

    fn play_pause(&self) {
        push(Command::Toggle);
    }

    fn stop(&self) {
        push(Command::Stop);
    }

    fn play(&self) {
        push(Command::Play);
    }

    fn seek(&self, _offset: i64) {}

    fn set_position(&self, _track: ObjectPath<'_>, _position: i64) {}

    fn open_uri(&self, _uri: String) {}

    #[zbus(signal)]
    async fn seeked(emitter: &SignalEmitter<'_>, position: i64) -> zbus::Result<()>;

    #[zbus(property)]
    fn playback_status(&self) -> String {
        let playing = shared().lock().ok().and_then(|s| s.now.as_ref().map(|n| n.playing));
        match playing {
            Some(true) => "Playing",
            Some(false) => "Paused",
            None => "Stopped",
        }
        .into()
    }

    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        let now = shared().lock().ok().and_then(|s| s.now.clone());
        metadata(now.as_ref())
    }

    #[zbus(property)]
    fn position(&self) -> i64 {
        position()
    }

    #[zbus(property)]
    fn rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn volume(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_play(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_pause(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_seek(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cover_path_becomes_a_url() {
        assert_eq!(
            file_url(std::path::Path::new("/home/user/.cache/kessler/covers/über alles.jpg")),
            "file:///home/user/.cache/kessler/covers/%C3%BCber%20alles.jpg"
        );
    }

    #[test]
    fn a_track_keeps_its_id() {
        let track = NowPlaying {
            playing: true,
            title: "olson".into(),
            artist: "boards of canada".into(),
            album: "music has the right to children".into(),
            duration: 91.0,
            elapsed: 3.0,
            artwork: None,
        };
        let later = NowPlaying { elapsed: 60.0, playing: false, ..track.clone() };
        let other = NowPlaying { title: "roygbiv".into(), ..track.clone() };
        assert_eq!(track_id(&track), track_id(&later));
        assert_ne!(track_id(&track), track_id(&other));

        let map = metadata(Some(&track));
        for key in ["mpris:trackid", "mpris:length", "xesam:title", "xesam:artist", "xesam:album"] {
            assert!(map.contains_key(key), "{key}");
        }
        assert!(!map.contains_key("mpris:artUrl"), "no cover, no url");
        assert!(metadata(None).contains_key("mpris:trackid"));
    }
}
