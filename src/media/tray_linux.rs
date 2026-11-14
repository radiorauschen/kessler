use super::Command;
use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, OnceLock};
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Type, Value};

const ITEM_PATH: &str = "/StatusNotifierItem";
const MENU_PATH: &str = "/MenuBar";
const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const WATCHER_PATH: &str = "/StatusNotifierWatcher";

const TITLE: i32 = 1;
const PLAY: i32 = 3;
const PREVIOUS: i32 = 4;
const NEXT: i32 = 5;
const SHOW: i32 = 7;
const QUIT: i32 = 8;
const VOLUME: i32 = 9;
const LOUDER: i32 = 10;
const QUIETER: i32 = 11;
const SPACE: i32 = 12;
const SLEEP: i32 = 13;
const STEP: u8 = 10;
const LINES: [i32; 13] = [TITLE, 2, PLAY, PREVIOUS, NEXT, SLEEP, SPACE, VOLUME, LOUDER, QUIETER, 6, SHOW, QUIT];

type Pixmap = (i32, i32, Vec<u8>);

#[derive(Default, Clone)]
struct Words {
    play: String,
    pause: String,
    next: String,
    previous: String,
    show: String,
    quit: String,
    volume: String,
    louder: String,
    quieter: String,
    sleep: String,
}

#[derive(Default)]
struct Shared {
    commands: VecDeque<Command>,
    raw: Option<(Vec<u8>, usize, usize)>,
    light: bool,
    playing: bool,
    title: String,
    volume: u8,
    words: Words,
    revision: u32,
    shown: bool,
    held: bool,
}

fn shared() -> &'static Mutex<Shared> {
    static SHARED: OnceLock<Mutex<Shared>> = OnceLock::new();
    SHARED.get_or_init(Default::default)
}

static CONNECTION: OnceLock<zbus::blocking::Connection> = OnceLock::new();
static CONNECTING: std::sync::Once = std::sync::Once::new();

fn name() -> String {
    format!("org.kde.StatusNotifierItem-{}-1", std::process::id())
}

fn push(command: Command) {
    if let Ok(mut shared) = shared().lock() {
        shared.commands.push_back(command);
    }
    super::kessler_media_woken();
}

pub fn poll() -> Option<Command> {
    shared().lock().ok()?.commands.pop_front()
}

pub fn show(show: bool) {
    let Ok(mut held) = shared().lock().map(|mut s| {
        s.shown = show;
        s.held
    }) else {
        return;
    };
    match CONNECTION.get() {
        Some(connection) => {
            if show && !held {
                held = hold(connection);
            } else if !show && held {
                let _ = connection.release_name(name());
                held = false;
            }
            if let Ok(mut shared) = shared().lock() {
                shared.held = held;
            }
        }
        None if show => CONNECTING.call_once(|| {
            let spawned = std::thread::Builder::new().name("tray".into()).spawn(connect);
            if let Err(e) = spawned {
                eprintln!("kessler: kein symbol in der leiste ({e})");
            }
        }),
        None => {}
    }
}

fn connect() {
    let connection = zbus::blocking::connection::Builder::session()
        .and_then(|b| b.serve_at(ITEM_PATH, Item))
        .and_then(|b| b.serve_at(MENU_PATH, Menu))
        .and_then(|b| b.build());
    let connection = match connection {
        Ok(connection) => connection,
        Err(e) => {
            eprintln!("kessler: kein symbol in der leiste ({e})");
            return;
        }
    };
    let connection = CONNECTION.get_or_init(|| connection);
    if let Ok(mut shared) = shared().lock() {
        if shared.shown {
            shared.held = hold(connection);
        }
    }
    let Ok(bus) = zbus::blocking::fdo::DBusProxy::new(connection) else {
        return;
    };
    let Ok(changes) = bus.receive_name_owner_changed_with_args(&[(0, WATCHER)]) else {
        return;
    };
    for change in changes {
        let came = change.args().is_ok_and(|args| args.new_owner().is_some());
        let held = shared().lock().is_ok_and(|s| s.held);
        if came && held {
            register(connection);
        }
    }
}

fn hold(connection: &zbus::blocking::Connection) -> bool {
    if let Err(e) = connection.request_name(name()) {
        eprintln!("kessler: kein symbol in der leiste ({e})");
        return false;
    }
    register(connection);
    true
}

fn register(connection: &zbus::blocking::Connection) {
    let _ = connection.call_method(
        Some(WATCHER),
        WATCHER_PATH,
        Some(WATCHER),
        "RegisterStatusNotifierItem",
        &(name(),),
    );
}

pub fn image(rgba: &[u8], wide: usize, tall: usize) {
    if let Ok(mut shared) = shared().lock() {
        shared.raw = Some((rgba.to_vec(), wide, tall));
    }
    emit(|e| async move { Item::new_icon(&e).await });
}

pub fn ink(light: bool) {
    let changed = shared().lock().is_ok_and(|mut s| std::mem::replace(&mut s.light, light) != light);
    if changed {
        emit(|e| async move { Item::new_icon(&e).await });
    }
}

pub fn state(playing: bool, title: &str, volume: u8, words: &super::TrayWords) {
    let revision = {
        let Ok(mut shared) = shared().lock() else {
            return;
        };
        shared.playing = playing;
        shared.title = title.to_string();
        shared.volume = volume;
        shared.words = Words {
            play: words.play.into(),
            pause: words.pause.into(),
            next: words.next.into(),
            previous: words.previous.into(),
            show: words.show.into(),
            quit: words.quit.into(),
            volume: words.volume.into(),
            louder: words.louder.into(),
            quieter: words.quieter.into(),
            sleep: words.sleep.into(),
        };
        shared.revision += 1;
        shared.revision
    };
    emit(|e| async move { Item::new_tool_tip(&e).await });
    if let Some(connection) = CONNECTION.get() {
        if let Ok(menu) = connection.object_server().interface::<_, Menu>(MENU_PATH) {
            let _ = zbus::block_on(Menu::layout_updated(menu.signal_emitter(), revision, 0));
        }
    }
}

fn emit<F, Fut>(signal: F)
where
    F: FnOnce(SignalEmitter<'static>) -> Fut,
    Fut: std::future::Future<Output = zbus::Result<()>>,
{
    let Some(connection) = CONNECTION.get() else {
        return;
    };
    if !shared().lock().is_ok_and(|s| s.held) {
        return;
    }
    let Ok(item) = connection.object_server().interface::<_, Item>(ITEM_PATH) else {
        return;
    };
    let emitter = item.signal_emitter().to_owned();
    let _ = zbus::block_on(signal(emitter));
}

fn pixmap(raw: &[u8], wide: usize, tall: usize, light: bool) -> Pixmap {
    let side = wide.max(tall);
    let (left, top) = ((side - wide) / 2, (side - tall) / 2);
    let ink = if light { [0xF2, 0xF2, 0xF2] } else { [0x1C, 0x1C, 0x1C] };
    let mut out = vec![0u8; side * side * 4];
    for y in 0..tall {
        for x in 0..wide {
            let alpha = raw[(y * wide + x) * 4 + 3];
            let at = ((y + top) * side + x + left) * 4;
            out[at..at + 4].copy_from_slice(&[alpha, ink[0], ink[1], ink[2]]);
        }
    }
    (side as i32, side as i32, out)
}

struct Item;

#[zbus::interface(name = "org.kde.StatusNotifierItem")]
impl Item {
    fn activate(&self, _x: i32, _y: i32) {
        push(Command::Toggle);
    }

    fn secondary_activate(&self, _x: i32, _y: i32) {
        push(Command::Raise);
    }

    fn context_menu(&self, _x: i32, _y: i32) {}

    fn scroll(&self, _delta: i32, _orientation: String) {}

    #[zbus(property)]
    fn category(&self) -> String {
        "ApplicationStatus".into()
    }

    #[zbus(property)]
    fn id(&self) -> String {
        "kessler".into()
    }

    #[zbus(property)]
    fn title(&self) -> String {
        "kessler".into()
    }

    #[zbus(property)]
    fn status(&self) -> String {
        "Active".into()
    }

    #[zbus(property)]
    fn window_id(&self) -> i32 {
        0
    }

    #[zbus(property)]
    fn icon_name(&self) -> String {
        String::new()
    }

    #[zbus(property)]
    fn icon_pixmap(&self) -> Vec<Pixmap> {
        let Ok(shared) = shared().lock() else {
            return Vec::new();
        };
        match &shared.raw {
            Some((raw, wide, tall)) if raw.len() == wide * tall * 4 => vec![pixmap(raw, *wide, *tall, shared.light)],
            _ => Vec::new(),
        }
    }

    #[zbus(property)]
    fn overlay_icon_name(&self) -> String {
        String::new()
    }

    #[zbus(property)]
    fn overlay_icon_pixmap(&self) -> Vec<Pixmap> {
        Vec::new()
    }

    #[zbus(property)]
    fn attention_icon_name(&self) -> String {
        String::new()
    }

    #[zbus(property)]
    fn attention_icon_pixmap(&self) -> Vec<Pixmap> {
        Vec::new()
    }

    #[zbus(property)]
    fn attention_movie_name(&self) -> String {
        String::new()
    }

    #[zbus(property)]
    fn tool_tip(&self) -> (String, Vec<Pixmap>, String, String) {
        let title = shared().lock().map(|s| s.title.clone()).unwrap_or_default();
        (String::new(), Vec::new(), "kessler".into(), title)
    }

    #[zbus(property)]
    fn item_is_menu(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn menu(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from(MENU_PATH).expect("a fixed path")
    }

    #[zbus(signal)]
    async fn new_icon(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn new_tool_tip(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Type, Value, zbus::zvariant::OwnedValue)]
struct Entry {
    id: i32,
    properties: HashMap<String, OwnedValue>,
    children: Vec<OwnedValue>,
}

fn properties(id: i32, shared: &Shared) -> HashMap<String, OwnedValue> {
    let mut map = HashMap::new();
    let mut put = |key: &str, value: Value| {
        if let Ok(owned) = OwnedValue::try_from(value) {
            map.insert(key.to_string(), owned);
        }
    };
    let label = |text: &str| Value::from(text.replace('_', "__"));
    let words = &shared.words;
    match id {
        TITLE => {
            put("label", label(&shared.title));
            put("enabled", Value::from(false));
            put("visible", Value::from(!shared.title.is_empty()));
        }
        2 => {
            put("type", Value::from("separator"));
            put("visible", Value::from(!shared.title.is_empty()));
        }
        PLAY => put("label", label(if shared.playing { &words.pause } else { &words.play })),
        PREVIOUS => {
            put("label", label(&format!("←  {}", words.previous)));
            put("visible", Value::from(!words.previous.is_empty()));
        }
        NEXT => {
            put("label", label(&format!("→  {}", words.next)));
            put("visible", Value::from(!words.next.is_empty()));
        }
        6 | SPACE => put("type", Value::from("separator")),
        VOLUME => {
            put("label", label(&format!("{}  {}", words.volume, shared.volume)));
            put("enabled", Value::from(false));
        }
        LOUDER => {
            put("label", label(&format!("+  {}", words.louder)));
            put("enabled", Value::from(shared.volume < 100));
        }
        QUIETER => {
            put("label", label(&format!("−  {}", words.quieter)));
            put("enabled", Value::from(shared.volume > 0));
        }
        SLEEP => {
            put("label", label(&words.sleep));
            put("visible", Value::from(!words.sleep.is_empty()));
        }
        SHOW => put("label", label(&words.show)),
        QUIT => put("label", label(&words.quit)),
        _ => {}
    }
    map
}

fn layout(shared: &Shared) -> Entry {
    let children = LINES
        .iter()
        .filter_map(|&id| {
            let entry = Entry { id, properties: properties(id, shared), children: Vec::new() };
            OwnedValue::try_from(Value::from(entry)).ok()
        })
        .collect();
    let mut root = HashMap::new();
    if let Ok(submenu) = OwnedValue::try_from(Value::from("submenu")) {
        root.insert("children-display".to_string(), submenu);
    }
    Entry { id: 0, properties: root, children }
}

fn clicked(id: i32) {
    match id {
        PLAY => push(Command::Toggle),
        PREVIOUS => push(Command::Previous),
        NEXT => push(Command::Next),
        SHOW => push(Command::Raise),
        QUIT => push(Command::Quit),
        SLEEP => push(Command::Sleep),
        LOUDER | QUIETER => {
            let now = shared().lock().map(|s| s.volume).unwrap_or(0);
            let to = if id == LOUDER { now.saturating_add(STEP).min(100) } else { now.saturating_sub(STEP) };
            push(Command::Volume(to));
        }
        _ => {}
    }
}

struct Menu;

#[zbus::interface(name = "com.canonical.dbusmenu")]
impl Menu {
    fn get_layout(&self, parent_id: i32, _recursion_depth: i32, _property_names: Vec<String>) -> (u32, Entry) {
        let Ok(shared) = shared().lock() else {
            return (0, Entry { id: parent_id, properties: HashMap::new(), children: Vec::new() });
        };
        let whole = layout(&shared);
        let entry = if parent_id == 0 {
            whole
        } else {
            Entry { id: parent_id, properties: properties(parent_id, &shared), children: Vec::new() }
        };
        (shared.revision, entry)
    }

    fn get_group_properties(&self, ids: Vec<i32>, _property_names: Vec<String>) -> Vec<(i32, HashMap<String, OwnedValue>)> {
        let Ok(shared) = shared().lock() else {
            return Vec::new();
        };
        let ids = if ids.is_empty() { LINES.to_vec() } else { ids };
        ids.into_iter().map(|id| (id, properties(id, &shared))).collect()
    }

    fn get_property(&self, id: i32, name: String) -> zbus::fdo::Result<OwnedValue> {
        let shared = shared().lock().map_err(|_| zbus::fdo::Error::Failed("locked".into()))?;
        properties(id, &shared)
            .remove(&name)
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs(format!("no {name} on {id}")))
    }

    fn event(&self, id: i32, event_id: String, _data: OwnedValue, _timestamp: u32) {
        if event_id == "clicked" {
            clicked(id);
        }
    }

    fn event_group(&self, events: Vec<(i32, String, OwnedValue, u32)>) -> Vec<i32> {
        for (id, event_id, _, _) in &events {
            if event_id == "clicked" {
                clicked(*id);
            }
        }
        Vec::new()
    }

    fn about_to_show(&self, _id: i32) -> bool {
        false
    }

    fn about_to_show_group(&self, _ids: Vec<i32>) -> (Vec<i32>, Vec<i32>) {
        (Vec::new(), Vec::new())
    }

    #[zbus(property)]
    fn version(&self) -> u32 {
        3
    }

    #[zbus(property)]
    fn text_direction(&self) -> String {
        "ltr".into()
    }

    #[zbus(property)]
    fn status(&self) -> String {
        "normal".into()
    }

    #[zbus(property)]
    fn icon_theme_path(&self) -> Vec<String> {
        Vec::new()
    }

    #[zbus(signal)]
    async fn layout_updated(emitter: &SignalEmitter<'_>, revision: u32, parent: i32) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_picture_goes_out_square() {
        let mut raw = vec![0u8; 4 * 2 * 4];
        raw[3] = 255;
        let (wide, tall, bytes) = pixmap(&raw, 4, 2, true);
        assert_eq!((wide, tall, bytes.len()), (4, 4, 4 * 4 * 4));
        assert_eq!(&bytes[16..20], &[255, 0xF2, 0xF2, 0xF2]);
        assert_eq!(bytes[0], 0);
        let (_, _, dark) = pixmap(&raw, 4, 2, false);
        assert_eq!(&dark[16..20], &[255, 0x1C, 0x1C, 0x1C]);
    }

    #[test]
    fn the_menu_says_what_plays() {
        let mut shared = Shared { words: Words { play: "play".into(), pause: "pause".into(), ..Default::default() }, ..Default::default() };
        assert_eq!(layout(&shared).children.len(), LINES.len());
        let label = |shared: &Shared, id| String::try_from(properties(id, shared).remove("label").unwrap()).unwrap();
        assert_eq!(label(&shared, PLAY), "play");
        assert!(!bool::try_from(properties(TITLE, &shared).remove("visible").unwrap()).unwrap());
        shared.playing = true;
        shared.title = "olson — boards_of_canada".into();
        assert_eq!(label(&shared, PLAY), "pause");
        assert_eq!(label(&shared, TITLE), "olson — boards__of__canada");
    }
}
