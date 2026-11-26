//! The system's own transport controls.
//!
//! On a Mac: the media keys, the Now Playing panel in Control Centre — which
//! is also what AirPlay hands on to a HomePod or a television, cover and all —
//! and the menu the Dock icon offers on a right-click. The framework calls for all
//! three are in `macos.m`; what is here is the bridge and — more to the point
//! — the decision of *when* to tell the system anything, which is the part
//! that can be got wrong quietly and is therefore the part with tests.
//!
//! On Linux: the same keys and the panel's media controls, by way of MPRIS over
//! D-Bus — see `linux.rs`.
//!
//! Everywhere else this is three functions that do nothing, so the window can
//! call them without asking what it is running on.

use std::ffi::CString;

#[cfg(all(target_os = "linux", feature = "gui"))]
mod linux;
#[cfg(all(target_os = "linux", feature = "gui"))]
mod tray_linux;
#[cfg(all(target_os = "linux", feature = "gui"))]
pub mod remote;

/// What the system asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(any(kessler_media, kessler_tray)), allow(dead_code))]
pub enum Command {
    Play,
    Pause,
    Toggle,
    Next,
    Previous,
    Stop,
    /// The window to the front — from the symbol's menu on Linux; on a Mac
    /// the other side does it itself.
    #[cfg_attr(not(all(target_os = "linux", feature = "gui")), allow(dead_code))]
    Raise,
    /// The whole program, as ⌘Q — the same.
    #[cfg_attr(not(all(target_os = "linux", feature = "gui")), allow(dead_code))]
    Quit,
    /// The volume, in percent — from the symbol's menu: its slider on a Mac,
    /// „lauter" and „leiser" in a Linux panel.
    #[cfg_attr(not(any(kessler_tray, all(target_os = "linux", feature = "gui"))), allow(dead_code))]
    Volume(u8),
    /// The sleep timer one step on, from the symbol's menu while an episode
    /// plays.
    #[cfg_attr(not(any(kessler_tray, all(target_os = "linux", feature = "gui"))), allow(dead_code))]
    Sleep,
}

impl Command {
    /// The numbers are the bridge's whole vocabulary; they are written out on
    /// both sides and must not drift.
    #[cfg_attr(not(any(kessler_media, kessler_tray)), allow(dead_code))]
    fn from_code(code: i32) -> Option<Self> {
        match code {
            1 => Some(Self::Play),
            2 => Some(Self::Pause),
            3 => Some(Self::Toggle),
            4 => Some(Self::Next),
            5 => Some(Self::Previous),
            6 => Some(Self::Stop),
            7 => Some(Self::Sleep),
            _ => None,
        }
    }
}

/// What the system should say is playing.
#[derive(Debug, Clone, PartialEq)]
pub struct NowPlaying {
    pub playing: bool,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: f64,
    pub elapsed: f64,
    /// The album's cover, as the file the cover cache keeps it in — `None`
    /// when there is none (yet). A path rather than the picture, so the
    /// question "is this news" is one comparison and the picture is read
    /// once, on the other side, when the path changes.
    pub artwork: Option<std::path::PathBuf>,
}

/// How far the panel's own clock may be out before it is worth correcting.
///
/// The panel counts on by itself while the rate is 1.0, so an update every
/// frame would say nothing new sixty times a second. Only a jump — a seek, a
/// new track, a pause — is news.
const DRIFT: f64 = 2.0;

/// Whether the system needs telling.
///
/// Pure, and separate from the call itself, because "when" is the question
/// this module can actually get wrong: too often and it is talking to the
/// window server sixty times a second, too seldom and the panel shows the
/// track before last.
fn worth_sending(last: Option<&NowPlaying>, next: Option<&NowPlaying>, since: f64) -> bool {
    match (last, next) {
        (None, None) => false,
        (None, Some(_)) | (Some(_), None) => true,
        (Some(last), Some(next)) => {
            if last.playing != next.playing
                || last.title != next.title
                || last.artist != next.artist
                || last.album != next.album
                || last.artwork != next.artwork
                || (last.duration - next.duration).abs() > 0.5
            {
                return true;
            }
            let expected = if last.playing {
                last.elapsed + since
            } else {
                last.elapsed
            };
            (next.elapsed - expected).abs() > DRIFT
        }
    }
}

/// Keeps the system's idea of what is playing up to date.
#[derive(Debug, Default)]
pub struct Reporter {
    last: Option<NowPlaying>,
    sent: Option<std::time::Instant>,
}

impl Reporter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Called every frame; talks to the system only when there is something
    /// to say.
    pub fn report(&mut self, now: Option<NowPlaying>) {
        let since = self
            .sent
            .map(|at| at.elapsed().as_secs_f64())
            .unwrap_or(f64::MAX);
        if !worth_sending(self.last.as_ref(), now.as_ref(), since) {
            return;
        }
        update(now.as_ref());
        self.last = now;
        self.sent = Some(std::time::Instant::now());
    }
}

/// What to do when the system sends a command.
///
/// The window draws when something happens, and a key pressed on the keyboard
/// of a Mac while the pointer sits still is not, as far as the window is
/// concerned, something happening: the command lands in the ring on the other
/// side of the bridge and waits there until the next frame — which, with
/// nothing playing, may be the next time somebody moves the mouse.
///
/// So the command itself has to be the thing that happens.
type Waker = Box<dyn Fn() + Send + Sync>;

static WAKE: std::sync::OnceLock<std::sync::Mutex<Option<Waker>>> =
    std::sync::OnceLock::new();

fn waker() -> &'static std::sync::Mutex<Option<Waker>> {
    WAKE.get_or_init(|| std::sync::Mutex::new(None))
}

/// Register the nudge. The window passes its own repaint request.
pub fn on_wake(wake: impl Fn() + Send + Sync + 'static) {
    if let Ok(mut slot) = waker().lock() {
        *slot = Some(Box::new(wake));
    }
}

/// Called from the other side of the bridge, on whichever thread the system
/// used, the moment a command has been queued.
///
/// Nothing here may block: it runs inside a framework callback. Taking the
/// lock is the only thing it does besides the nudge itself, and the only other
/// taker is the one-off registration.
#[no_mangle]
pub extern "C" fn kessler_media_woken() {
    if let Ok(slot) = waker().lock() {
        if let Some(wake) = slot.as_ref() {
            wake();
        }
    }
}

#[cfg(kessler_media)]
extern "C" {
    fn kessler_media_start();
    fn kessler_hide_app();
    fn kessler_set_dock_icon(rgba: *const u8, width: i32, height: i32);
    fn kessler_media_poll() -> i32;
    fn kessler_media_update(
        state: i32,
        title: *const std::os::raw::c_char,
        artist: *const std::os::raw::c_char,
        album: *const std::os::raw::c_char,
        duration: f64,
        elapsed: f64,
        artwork: *const std::os::raw::c_char,
    );
}

/// Hide the whole application, as ⌘H does. Returns false where that cannot
/// be done — not a Mac, or a build without the bridge — so the caller can do
/// the next best thing.
pub fn hide_application() -> bool {
    #[cfg(kessler_media)]
    {
        unsafe { kessler_hide_app() };
        true
    }
    #[cfg(not(kessler_media))]
    {
        false
    }
}

/// Show `rgba` — premultiplied, `width` × `height` — as the Dock icon while
/// the application runs. False where that cannot be done this way, so the
/// caller can hand the window its icon instead.
pub fn set_dock_icon(rgba: &[u8], width: usize, height: usize) -> bool {
    #[cfg(kessler_media)]
    {
        if rgba.len() != width * height * 4 {
            return false;
        }
        unsafe { kessler_set_dock_icon(rgba.as_ptr(), width as i32, height as i32) };
        true
    }
    #[cfg(not(kessler_media))]
    {
        let _ = (rgba, width, height);
        false
    }
}

/// Register for the keys and hang the menu on the Dock icon.
///
/// Must be called once the application is up — the Dock asks the delegate for
/// its menu, and there is no delegate before the first frame.
pub fn start() {
    #[cfg(kessler_media)]
    unsafe {
        kessler_media_start()
    }
    #[cfg(all(target_os = "linux", feature = "gui"))]
    linux::start();
}

/// Whether a new track is said in a notification, and whether the window is
/// in front, when it need not be. Linux only, for now; called every frame.
pub fn set_notify(on: bool, in_front: bool) {
    #[cfg(all(target_os = "linux", feature = "gui"))]
    linux::set_notify(on, in_front);
    #[cfg(not(all(target_os = "linux", feature = "gui")))]
    let _ = (on, in_front);
}

/// The next command the system has for us, if any.
pub fn poll() -> Option<Command> {
    #[cfg(kessler_media)]
    unsafe {
        Command::from_code(kessler_media_poll())
    }
    #[cfg(all(target_os = "linux", feature = "gui"))]
    return linux::poll();
    #[cfg(not(any(kessler_media, all(target_os = "linux", feature = "gui"))))]
    None
}

/// Tell the system what is playing. Prefer [`Reporter`], which knows when.
fn update(now: Option<&NowPlaying>) {
    #[cfg(kessler_media)]
    {
        let (state, title, artist, album, duration, elapsed) = match now {
            Some(now) => (
                if now.playing { 1 } else { 2 },
                text(&now.title),
                text(&now.artist),
                text(&now.album),
                now.duration,
                now.elapsed,
            ),
            None => (0, text(""), text(""), text(""), 0.0, 0.0),
        };
        let artwork = text(
            &now.and_then(|n| n.artwork.as_ref())
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
        );
        unsafe {
            kessler_media_update(
                state,
                title.as_ptr(),
                artist.as_ptr(),
                album.as_ptr(),
                duration,
                elapsed,
                artwork.as_ptr(),
            )
        }
    }
    #[cfg(kessler_touchbar)]
    {
        let (state, title, artist) = match now {
            Some(now) => (if now.playing { 1 } else { 2 }, text(&now.title), text(&now.artist)),
            None => (0, text(""), text("")),
        };
        let artwork = text(
            &now.and_then(|n| n.artwork.as_ref())
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
        );
        unsafe { kessler_touchbar_update(state, title.as_ptr(), artist.as_ptr(), artwork.as_ptr()) }
    }
    #[cfg(all(target_os = "linux", feature = "gui"))]
    linux::update(now);
    #[cfg(not(any(kessler_media, all(target_os = "linux", feature = "gui"))))]
    let _ = now;
}

#[cfg(kessler_tray)]
extern "C" {
    fn kessler_tray_show(show: i32);
    fn kessler_tray_image(rgba: *const u8, wide: i32, tall: i32);
    #[allow(clippy::too_many_arguments)]
    fn kessler_tray_state(
        playing: i32,
        title: *const std::os::raw::c_char,
        play: *const std::os::raw::c_char,
        pause: *const std::os::raw::c_char,
        next: *const std::os::raw::c_char,
        previous: *const std::os::raw::c_char,
        show: *const std::os::raw::c_char,
        quit: *const std::os::raw::c_char,
        volume: i32,
        volume_word: *const std::os::raw::c_char,
        sleep: *const std::os::raw::c_char,
    );
    fn kessler_tray_poll() -> i32;
    fn kessler_tray_cover_points() -> i32;
    fn kessler_tray_cover(rgba: *const u8, side: i32, at: i32);
    /// Where the menu's slider was last let go of or moved to, in percent;
    /// -1 when it has not been touched since the last asking.
    fn kessler_tray_volume() -> i32;
}

/// Whether this build can put a symbol in the menu bar at all — on Linux in the
/// panel, see `tray_linux.rs`.
pub fn tray_available() -> bool {
    cfg!(any(kessler_tray, all(target_os = "linux", feature = "gui")))
}

/// Put the symbol in the menu bar, or take it away. From the main thread.
pub fn tray_show(show: bool) {
    #[cfg(kessler_tray)]
    unsafe {
        kessler_tray_show(i32::from(show))
    }
    #[cfg(all(target_os = "linux", feature = "gui"))]
    tray_linux::show(show);
    #[cfg(not(any(kessler_tray, all(target_os = "linux", feature = "gui"))))]
    let _ = show;
}

/// Light on a dark panel, or dark on a light one. Only on Linux: a Mac's menu
/// bar colours the symbol itself.
pub fn tray_ink(light: bool) {
    #[cfg(all(target_os = "linux", feature = "gui"))]
    tray_linux::ink(light);
    #[cfg(not(all(target_os = "linux", feature = "gui")))]
    let _ = light;
}

/// How tall the cover beside the title in the Mac's menu stands, in
/// points: three lines of the menu's text. From the main thread.
pub fn tray_cover_points() -> Option<u32> {
    #[cfg(kessler_tray)]
    unsafe {
        let points = kessler_tray_cover_points();
        if (16..=200).contains(&points) {
            return Some(points as u32);
        }
    }
    None
}

/// The cover in the Mac's menu under the symbol: `side` × `side` pixels,
/// four bytes each, twice its points; `at` 1 beside the title, 2 above it,
/// 0 — or no pixels — for none. From any thread: the other side copies the
/// pixels and sets them on the main thread.
pub fn tray_cover(rgba: &[u8], side: usize, at: i32) {
    #[cfg(kessler_tray)]
    {
        let side = if rgba.len() >= side * side * 4 { side } else { 0 };
        unsafe { kessler_tray_cover(rgba.as_ptr(), side as i32, at) }
    }
    #[cfg(not(kessler_tray))]
    let _ = (rgba, side, at);
}

/// The symbol's picture: `wide` × `tall` pixels — twice its points, for
/// Retina — four bytes a pixel, black with the coverage in alpha. From any
/// thread — the other side copies it and sets it on the main thread.
pub fn tray_image(rgba: &[u8], wide: usize, tall: usize) {
    #[cfg(kessler_tray)]
    {
        if rgba.len() == wide * tall * 4 {
            unsafe { kessler_tray_image(rgba.as_ptr(), wide as i32, tall as i32) }
        }
    }
    #[cfg(all(target_os = "linux", feature = "gui"))]
    if rgba.len() == wide * tall * 4 {
        tray_linux::image(rgba, wide, tall);
    }
    #[cfg(not(any(kessler_tray, all(target_os = "linux", feature = "gui"))))]
    let _ = (rgba, wide, tall);
}

/// The words of the symbol's menu, and whether something plays.
pub struct TrayWords<'a> {
    pub play: &'a str,
    pub pause: &'a str,
    pub next: &'a str,
    pub previous: &'a str,
    pub show: &'a str,
    pub quit: &'a str,
    /// „vol", before the slider and the figure.
    pub volume: &'a str,
    /// The two lines a Linux panel has instead of a slider.
    #[cfg_attr(not(all(target_os = "linux", feature = "gui")), allow(dead_code))]
    pub louder: &'a str,
    #[cfg_attr(not(all(target_os = "linux", feature = "gui")), allow(dead_code))]
    pub quieter: &'a str,
    /// What the sleep timer says, while an episode plays; empty leaves its
    /// row out.
    pub sleep: &'a str,
}

/// What the symbol's menu says: what is playing — `title`, one line, empty for
/// nothing; `about`, its title, artist and record a line each, for the Mac's
/// menu, which wraps them — whether it plays, the volume in percent, and its
/// words.
pub fn tray_state(playing: bool, title: &str, about: &str, volume: u8, words: &TrayWords) {
    #[cfg(kessler_tray)]
    {
        let _ = title;
        let strings =
            [about, words.play, words.pause, words.next, words.previous, words.show, words.quit, words.volume, words.sleep]
                .map(text);
        unsafe {
            kessler_tray_state(
                i32::from(playing),
                strings[0].as_ptr(),
                strings[1].as_ptr(),
                strings[2].as_ptr(),
                strings[3].as_ptr(),
                strings[4].as_ptr(),
                strings[5].as_ptr(),
                strings[6].as_ptr(),
                i32::from(volume),
                strings[7].as_ptr(),
                strings[8].as_ptr(),
            )
        }
    }
    #[cfg(all(target_os = "linux", feature = "gui"))]
    tray_linux::state(playing, title, volume, words);
    #[cfg(all(target_os = "linux", feature = "gui"))]
    let _ = about;
    #[cfg(not(any(kessler_tray, all(target_os = "linux", feature = "gui"))))]
    let _ = (
        playing,
        title,
        about,
        volume,
        words.play,
        words.pause,
        words.next,
        words.previous,
        words.show,
        words.quit,
        words.volume,
        words.louder,
        words.quieter,
        words.sleep,
    );
}

/// The next click on the symbol, as the command it stands for.
pub fn tray_poll() -> Option<Command> {
    #[cfg(kessler_tray)]
    unsafe {
        let volume = kessler_tray_volume();
        if (0..=100).contains(&volume) {
            return Some(Command::Volume(volume as u8));
        }
        Command::from_code(kessler_tray_poll())
    }
    #[cfg(all(target_os = "linux", feature = "gui"))]
    return tray_linux::poll();
    #[cfg(not(any(kessler_tray, all(target_os = "linux", feature = "gui"))))]
    None
}

/// A C string from anything, including text with a zero byte in it — which is
/// not a thing a tag should hold, but is a thing a broken file can.
#[cfg_attr(not(any(kessler_media, kessler_tray)), allow(dead_code))]
fn text(value: &str) -> CString {
    CString::new(value.replace('\0', " ")).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn playing(title: &str, elapsed: f64) -> NowPlaying {
        NowPlaying {
            playing: true,
            title: title.into(),
            artist: "boards of canada".into(),
            album: "geogaddi".into(),
            duration: 300.0,
            elapsed,
            artwork: Some("/covers/geogaddi.jpg".into()),
        }
    }

    /// The numbers on the wire are the one thing both halves of the bridge
    /// have to agree about.
    #[test]
    fn the_codes_are_fixed() {
        assert_eq!(Command::from_code(1), Some(Command::Play));
        assert_eq!(Command::from_code(2), Some(Command::Pause));
        assert_eq!(Command::from_code(3), Some(Command::Toggle));
        assert_eq!(Command::from_code(4), Some(Command::Next));
        assert_eq!(Command::from_code(5), Some(Command::Previous));
        assert_eq!(Command::from_code(6), Some(Command::Stop));
        assert_eq!(Command::from_code(7), Some(Command::Sleep));
        assert_eq!(Command::from_code(0), None);
        assert_eq!(Command::from_code(99), None);
    }

    /// The panel keeps its own clock, so a position that is only running on is
    /// not news. Saying it anyway would mean talking to the system on every
    /// frame for the rest of the track.
    #[test]
    fn a_position_that_merely_ran_on_is_not_news() {
        let before = playing("olson", 30.0);
        let after = playing("olson", 34.0);
        assert!(!worth_sending(Some(&before), Some(&after), 4.0));
        assert!(!worth_sending(Some(&before), Some(&after), 3.5));
    }

    /// A seek is exactly the case the panel cannot work out for itself.
    #[test]
    fn a_jump_is_news() {
        let before = playing("olson", 30.0);
        assert!(worth_sending(Some(&before), Some(&playing("olson", 120.0)), 1.0));
        assert!(worth_sending(Some(&before), Some(&playing("olson", 5.0)), 1.0));
    }

    /// So is a different track, and so is stopping.
    #[test]
    fn a_new_track_or_a_pause_is_news() {
        let before = playing("olson", 30.0);
        assert!(worth_sending(Some(&before), Some(&playing("1969", 0.0)), 0.1));

        let paused = NowPlaying {
            playing: false,
            ..playing("olson", 30.1)
        };
        assert!(worth_sending(Some(&before), Some(&paused), 0.1));
        assert!(worth_sending(Some(&before), None, 0.1));
        assert!(worth_sending(None, Some(&before), 0.1));
    }

    /// A cover that turns up while the track is playing — fetched from the
    /// net a few seconds in — has to reach the panel, and the AirPlay speaker
    /// behind it, without waiting for the next track. And one that goes has
    /// to go there too.
    #[test]
    fn a_cover_arriving_is_news() {
        let bare = NowPlaying {
            artwork: None,
            ..playing("olson", 30.0)
        };
        assert!(worth_sending(Some(&bare), Some(&playing("olson", 30.5)), 0.5));
        assert!(worth_sending(Some(&playing("olson", 30.0)), Some(&bare), 0.1));
        assert!(!worth_sending(Some(&playing("olson", 30.0)), Some(&playing("olson", 30.5)), 0.5));
    }

    /// A window with nothing playing must not keep saying so.
    #[test]
    fn silence_is_said_once() {
        assert!(!worth_sending(None, None, 10.0));
    }

    /// A paused track does not drift, so nothing about it is news until it
    /// moves.
    #[test]
    fn a_paused_track_stays_where_it_is() {
        let paused = NowPlaying {
            playing: false,
            ..playing("olson", 30.0)
        };
        let same = NowPlaying {
            playing: false,
            ..playing("olson", 30.0)
        };
        assert!(!worth_sending(Some(&paused), Some(&same), 600.0));
    }

    /// The bug this guards: the key was pressed, the
    /// command was waiting, and nothing moved until the mouse did.
    ///
    /// One test rather than two, because the thing it is about is a global:
    /// a second test registering its own nudge would land in this one's count.
    #[test]
    fn a_command_nudges_the_window() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        kessler_media_woken();

        let nudges = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&nudges);
        on_wake(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        });

        kessler_media_woken();
        kessler_media_woken();
        assert_eq!(nudges.load(Ordering::SeqCst), 2);
    }

    /// A zero byte in a tag would cut the title off at the bridge, or fail to
    /// convert at all.
    #[test]
    fn a_broken_title_still_crosses() {
        assert_eq!(text("olson").to_str(), Ok("olson"));
        assert_eq!(text("ol\0son").to_str(), Ok("ol son"));
        assert_eq!(text("").to_str(), Ok(""));
    }
}

#[cfg(kessler_route)]
extern "C" {
    fn kessler_route_start();
    fn kessler_route_poll() -> i32;
}

/// Whether this build can tell when the headphones are gone.
pub fn route_available() -> bool {
    cfg!(kessler_route)
}

/// Begin listening for where the sound goes. Once; later calls do nothing.
pub fn route_start() {
    #[cfg(kessler_route)]
    unsafe {
        kessler_route_start()
    }
}

/// A pause, once, when the sound has gone over from headphones to the
/// built-in speakers since the last asking.
pub fn route_poll() -> Option<Command> {
    #[cfg(kessler_route)]
    unsafe {
        Command::from_code(kessler_route_poll())
    }
    #[cfg(not(kessler_route))]
    None
}

#[cfg(kessler_touchbar)]
extern "C" {
    fn kessler_touchbar_show(show: i32, lower: i32, whole: i32, station: i32);
    fn kessler_touchbar_update(
        state: i32,
        title: *const std::os::raw::c_char,
        artist: *const std::os::raw::c_char,
        artwork: *const std::os::raw::c_char,
    );
    fn kessler_touchbar_poll() -> i32;
    fn kessler_touchbar_picture(rgba: *const u8, wide: i32, tall: i32);
    fn kessler_touchbar_picture_wide() -> i32;
}

/// The MacBooks with a Touch Bar, by the model the system names them: the
/// Pros of 2016 to 2020 that have one, and the 13-inch of 2022.
const TOUCH_BAR_MODELS: &[&str] = &[
    "MacBookPro13,2",
    "MacBookPro13,3",
    "MacBookPro14,2",
    "MacBookPro14,3",
    "MacBookPro15,1",
    "MacBookPro15,2",
    "MacBookPro15,3",
    "MacBookPro15,4",
    "MacBookPro16,1",
    "MacBookPro16,2",
    "MacBookPro16,3",
    "MacBookPro16,4",
    "MacBookPro17,1",
    "Mac14,7",
];

/// Whether `model` — as `hw.model` gives it — has a Touch Bar.
fn has_touch_bar(model: &str) -> bool {
    TOUCH_BAR_MODELS.contains(&model.trim())
}

/// The model this Mac names itself by, as `hw.model`.
#[cfg(kessler_touchbar)]
fn hardware_model() -> Option<String> {
    let name = std::ffi::CString::new("hw.model").ok()?;
    let mut buffer = [0u8; 64];
    let mut length = buffer.len();
    let read = unsafe {
        libc::sysctlbyname(name.as_ptr(), buffer.as_mut_ptr().cast(), &mut length, std::ptr::null_mut(), 0)
    };
    if read != 0 {
        return None;
    }
    let bytes = &buffer[..length.min(buffer.len())];
    let bytes = bytes.split(|&b| b == 0).next().unwrap_or(bytes);
    Some(String::from_utf8_lossy(bytes).into_owned())
}

/// Whether this Mac has a Touch Bar and this build can fill it — asked once.
pub fn touchbar_available() -> bool {
    #[cfg(kessler_touchbar)]
    {
        static HAS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *HAS.get_or_init(|| hardware_model().is_some_and(|model| has_touch_bar(&model)))
    }
    #[cfg(not(kessler_touchbar))]
    {
        let _ = has_touch_bar;
        false
    }
}

/// Fill the Touch Bar with what plays while the window is in front, or give
/// it back to the system. `lower` writes what plays small, as the window
/// does; `whole` lets the picture fill the bar beside the cover; `station`
/// leaves out the key back, which a station has no use for. From the main
/// thread.
pub fn touchbar_show(show: bool, lower: bool, whole: bool, station: bool) {
    #[cfg(kessler_touchbar)]
    unsafe {
        kessler_touchbar_show(i32::from(show), i32::from(lower), i32::from(whole), i32::from(station))
    }
    #[cfg(not(kessler_touchbar))]
    let _ = (show, lower, whole, station);
}

/// The search field's picture on the Touch Bar, as `rgba` (`wide` × `tall`
/// pixels, two to a point, unpremultiplied); `None` takes it away. From the
/// main thread.
pub fn touchbar_picture(picture: Option<(&[u8], usize, usize)>) {
    #[cfg(kessler_touchbar)]
    {
        let (rgba, wide, tall) = picture.unwrap_or((&[], 0, 0));
        if rgba.len() != wide * tall * 4 {
            return;
        }
        unsafe { kessler_touchbar_picture(rgba.as_ptr(), wide as i32, tall as i32) }
    }
    #[cfg(not(kessler_touchbar))]
    let _ = picture;
}

/// How wide the Touch Bar laid out the picture, in points — the room it
/// leaves between the title and the keys, or beside the cover. `None` before
/// it has laid one out. From the main thread.
pub fn touchbar_picture_wide() -> Option<usize> {
    #[cfg(kessler_touchbar)]
    {
        let wide = unsafe { kessler_touchbar_picture_wide() };
        usize::try_from(wide).ok().filter(|&w| w > 0)
    }
    #[cfg(not(kessler_touchbar))]
    None
}

/// The next key pressed on the Touch Bar, as the command it stands for.
pub fn touchbar_poll() -> Option<Command> {
    #[cfg(kessler_touchbar)]
    unsafe {
        Command::from_code(kessler_touchbar_poll())
    }
    #[cfg(not(kessler_touchbar))]
    None
}

#[cfg(test)]
mod touch_bar_tests {
    use super::*;

    #[test]
    fn only_the_macbooks_with_a_strip_have_one() {
        assert!(has_touch_bar("MacBookPro16,1"));
        assert!(has_touch_bar("Mac14,7\n"));
        assert!(!has_touch_bar("MacBookPro16,5"));
        assert!(!has_touch_bar("MacBookAir10,1"));
        assert!(!has_touch_bar("Mac14,2"));
    }
}
