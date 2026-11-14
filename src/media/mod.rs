use std::ffi::CString;

#[cfg(all(target_os = "linux", feature = "gui"))]
mod linux;
#[cfg(all(target_os = "linux", feature = "gui"))]
mod tray_linux;
#[cfg(all(target_os = "linux", feature = "gui"))]
pub mod remote;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(any(kessler_media, kessler_tray)), allow(dead_code))]
pub enum Command {
    Play,
    Pause,
    Toggle,
    Next,
    Previous,
    Stop,
    #[cfg_attr(not(all(target_os = "linux", feature = "gui")), allow(dead_code))]
    Raise,
    #[cfg_attr(not(all(target_os = "linux", feature = "gui")), allow(dead_code))]
    Quit,
    #[cfg_attr(not(any(kessler_tray, all(target_os = "linux", feature = "gui"))), allow(dead_code))]
    Volume(u8),
    #[cfg_attr(not(any(kessler_tray, all(target_os = "linux", feature = "gui"))), allow(dead_code))]
    Sleep,
}

impl Command {
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

#[derive(Debug, Clone, PartialEq)]
pub struct NowPlaying {
    pub playing: bool,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: f64,
    pub elapsed: f64,
    pub artwork: Option<std::path::PathBuf>,
}

const DRIFT: f64 = 2.0;

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

#[derive(Debug, Default)]
pub struct Reporter {
    last: Option<NowPlaying>,
    sent: Option<std::time::Instant>,
}

impl Reporter {
    pub fn new() -> Self {
        Self::default()
    }

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

type Waker = Box<dyn Fn() + Send + Sync>;

static WAKE: std::sync::OnceLock<std::sync::Mutex<Option<Waker>>> =
    std::sync::OnceLock::new();

fn waker() -> &'static std::sync::Mutex<Option<Waker>> {
    WAKE.get_or_init(|| std::sync::Mutex::new(None))
}

pub fn on_wake(wake: impl Fn() + Send + Sync + 'static) {
    if let Ok(mut slot) = waker().lock() {
        *slot = Some(Box::new(wake));
    }
}

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

pub fn start() {
    #[cfg(kessler_media)]
    unsafe {
        kessler_media_start()
    }
    #[cfg(all(target_os = "linux", feature = "gui"))]
    linux::start();
}

pub fn set_notify(on: bool, in_front: bool) {
    #[cfg(all(target_os = "linux", feature = "gui"))]
    linux::set_notify(on, in_front);
    #[cfg(not(all(target_os = "linux", feature = "gui")))]
    let _ = (on, in_front);
}

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
    fn kessler_tray_volume() -> i32;
}

pub fn tray_available() -> bool {
    cfg!(any(kessler_tray, all(target_os = "linux", feature = "gui")))
}

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

pub fn tray_ink(light: bool) {
    #[cfg(all(target_os = "linux", feature = "gui"))]
    tray_linux::ink(light);
    #[cfg(not(all(target_os = "linux", feature = "gui")))]
    let _ = light;
}

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

pub fn tray_cover(rgba: &[u8], side: usize, at: i32) {
    #[cfg(kessler_tray)]
    {
        let side = if rgba.len() >= side * side * 4 { side } else { 0 };
        unsafe { kessler_tray_cover(rgba.as_ptr(), side as i32, at) }
    }
    #[cfg(not(kessler_tray))]
    let _ = (rgba, side, at);
}

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

pub struct TrayWords<'a> {
    pub play: &'a str,
    pub pause: &'a str,
    pub next: &'a str,
    pub previous: &'a str,
    pub show: &'a str,
    pub quit: &'a str,
    pub volume: &'a str,
    #[cfg_attr(not(all(target_os = "linux", feature = "gui")), allow(dead_code))]
    pub louder: &'a str,
    #[cfg_attr(not(all(target_os = "linux", feature = "gui")), allow(dead_code))]
    pub quieter: &'a str,
    pub sleep: &'a str,
}

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

    #[test]
    fn a_position_that_merely_ran_on_is_not_news() {
        let before = playing("olson", 30.0);
        let after = playing("olson", 34.0);
        assert!(!worth_sending(Some(&before), Some(&after), 4.0));
        assert!(!worth_sending(Some(&before), Some(&after), 3.5));
    }

    #[test]
    fn a_jump_is_news() {
        let before = playing("olson", 30.0);
        assert!(worth_sending(Some(&before), Some(&playing("olson", 120.0)), 1.0));
        assert!(worth_sending(Some(&before), Some(&playing("olson", 5.0)), 1.0));
    }

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

    #[test]
    fn silence_is_said_once() {
        assert!(!worth_sending(None, None, 10.0));
    }

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

    #[test]
    fn a_broken_title_still_crosses() {
        assert_eq!(text("olson").to_str(), Ok("olson"));
        assert_eq!(text("ol\0son").to_str(), Ok("ol son"));
        assert_eq!(text("").to_str(), Ok(""));
    }
}
