//! A measurement of the window, written down to be sent along with a report
//! of something slow.
//!
//! Off unless the menu turns it on (über → `messen`). While it runs, every
//! five seconds one line goes to `kessler-messung.log` beside the library:
//! how many frames were drawn and how long they took, how much of a core the
//! program used and how much memory it has held at most, and what was going
//! on — music, the window in front or out of sight, the scope in
//! the search field, the menu, the menu bar. That is enough to
//! tell which of them a number belongs to, on a machine nobody here can see.

use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// How long one line covers.
const SPAN: Duration = Duration::from_secs(5);

/// What was going on during a frame — see [`Measure::frame`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Doing {
    pub playing: bool,
    pub focused: bool,
    pub unseen: bool,
    pub field_scope: bool,
    pub menu: bool,
    pub covers_waiting: bool,
    /// The podcasts' view open, feeds being read, an episode being fetched.
    pub podcasts: bool,
    pub feeds: bool,
    pub fetching: bool,
    /// The frame had something from the hand to answer: the pointer moved,
    /// a key, a click, a scroll. Frames without are the program's own.
    pub input: bool,
}

impl Doing {
    /// The words for one line, those that were true at least once.
    fn said(seen: &[Doing]) -> String {
        let any = |f: fn(&Doing) -> bool| seen.iter().any(f);
        let mut words = Vec::new();
        for (on, word) in [
            (any(|d| d.playing), "musik"),
            (any(|d| d.focused), "vorne"),
            (any(|d| !d.focused && !d.unseen), "hinten"),
            (any(|d| d.unseen), "unsichtbar"),
            (any(|d| d.field_scope), "skop"),
            (any(|d| d.menu), "menü"),
            (any(|d| d.covers_waiting), "cover-laden"),
            (any(|d| d.podcasts), "podcasts"),
            (any(|d| d.feeds), "feeds-lesen"),
            (any(|d| d.fetching), "folge-laden"),
        ] {
            if on {
                words.push(word);
            }
        }
        words.join(" ")
    }
}

/// The measurement while it runs.
pub struct Measure {
    path: PathBuf,
    began: Instant,
    cpu_then: Duration,
    frame_began: Option<Instant>,
    /// How long the frame's `logic` took — the player, the database, what
    /// is told to the system — apart from the drawing after it.
    logic_took: Option<f32>,
    /// Where the last lap ended: the frame's start, then each [`Measure::lap`].
    lapped: Option<Instant>,
    /// The longest step of `logic` in the span, and its name. A frame of 70 ms
    /// in the logic said nothing about which part of it.
    slowest: (f32, &'static str),
    /// Every step of this frame, logic and drawing, for the line a slow
    /// frame gets.
    steps: Vec<(&'static str, f32)>,
    /// What was pressed this frame — keys and buttons — for the same line.
    pressed: String,
    /// What asked for the frames that nobody's hand asked for, counted by the
    /// place in the code — the answer to.
    asked: std::collections::HashMap<String, usize>,
    /// How soon each request for this frame wanted it, in the order egui
    /// kept their places — see [`watch_repaints`].
    soon: Vec<Duration>,
    frames: Vec<f32>,
    logic: Vec<f32>,
    doing: Vec<Doing>,
    /// What the textures hold, as the window last said — see
    /// [`textures_said`]: where the memory goes that is not the program's
    /// own heap.
    pub textures: String,
    /// Which view and list the window showed last, so a change of it is
    /// said once — see [`Measure::view_is`].
    view: String,
}

/// The textures egui holds — every cover, picture and the glyphs' atlas —
/// in megabytes, how many, the atlas's size and the biggest of the rest.
/// Each lives twice: on the graphics card, and egui keeps the atlas in
/// memory as well.
pub fn textures_said(ctx: &eframe::egui::Context) -> String {
    let manager = ctx.tex_manager();
    let manager = manager.read();
    let mut bytes = 0usize;
    let mut biggest = (0usize, String::new());
    for (_, meta) in manager.allocated() {
        let size = meta.size[0] * meta.size[1] * meta.bytes_per_pixel;
        bytes += size;
        if size > biggest.0 && !meta.name.contains("font") {
            biggest = (size, meta.name.clone());
        }
    }
    let [w, h] = ctx.fonts(|f| f.font_image_size());
    format!(
        "texturen {:.0} mb in {} (schrift {}×{}, grösste {} {:.1} mb)",
        bytes as f64 / 1e6,
        manager.num_allocated(),
        w,
        h,
        if biggest.1.is_empty() { "-" } else { &biggest.1 },
        biggest.0 as f64 / 1e6,
    )
}

/// How soon every request for a frame wanted it, in the order they came —
/// the same order egui keeps their places in. Taken at each frame's start.
static SOON: std::sync::Mutex<Vec<Duration>> = std::sync::Mutex::new(Vec::new());

/// Requests kept at most between two frames, where nobody takes them.
const SOON_MOST: usize = 4096;

/// A request that wants a frame within this is one that keeps the window
/// drawing; one that wants it in a quarter of a second or two seconds is not.
const QUICK: Duration = Duration::from_millis(50);

/// A frame that takes longer than this, in milliseconds, gets a line of its
/// own: which steps took the time, and what was pressed. The five-second
/// line only says that one was slow, not where.
const SLOW_FRAME: f32 = 40.0;

/// Have egui say how soon each request wanted its frame. egui keeps the
/// place of every request, whether it wants the frame now or in two seconds;
/// counting them all, a place that asked for two seconds every frame stood
/// in the log as often as the one that asked for the next frame, and hid it.
pub fn watch_repaints(ctx: &eframe::egui::Context) {
    static WATCHING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if WATCHING.swap(true, std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    ctx.set_repaint_observer(Box::new(|info: eframe::egui::RequestRepaintInfo| {
        let mut soon = SOON.lock().unwrap_or_else(|e| e.into_inner());
        if soon.len() < SOON_MOST {
            soon.push(info.delay);
        }
    }));
}

impl Measure {
    /// Start one, the file begun with a line saying so.
    pub fn start(path: PathBuf) -> std::io::Result<Self> {
        let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&path)?;
        writeln!(
            file,
            "# kessler {} · {} · {} — sekunden, bilder, bilder/s (davon ohne eingabe), ms je bild (mitte / 95 % / längstes), davon logik längstes (welcher schritt), cpu %, speicher jetzt (höchstens) mb, texturen, was lief, [wer ohne eingabe neu zeichnen liess]",
            crate::VERSION_SHOWN,
            std::env::consts::OS,
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        )?;
        Ok(Measure {
            path,
            began: Instant::now(),
            cpu_then: cpu_time(),
            frame_began: None,
            logic_took: None,
            lapped: None,
            slowest: (0.0, ""),
            steps: Vec::new(),
            pressed: String::new(),
            asked: std::collections::HashMap::new(),
            soon: Vec::new(),
            frames: Vec::new(),
            logic: Vec::new(),
            doing: Vec::new(),
            textures: String::new(),
            view: String::new(),
        })
    }

    /// A line of its own, at once, with the time — what the sound of an episode
    /// is, between the five-second lines.
    pub fn note(&mut self, said: &str) {
        let at = chrono::Local::now().format("%H:%M:%S").to_string();
        self.write_note(&at, said, crate::player::watch::memory_now_mb());
    }

    /// A line said elsewhere, with the time and memory of when it was said
    /// — not of when it is written, which for what came before the
    /// measurement began is all the same moment.
    pub fn noted(&mut self, note: &crate::player::watch::Note) {
        self.write_note(&note.at, &note.said, note.memory_mb);
    }

    fn write_note(&self, at: &str, said: &str, memory_mb: u64) {
        if let Ok(mut file) = std::fs::OpenOptions::new().append(true).open(&self.path) {
            let _ = writeln!(file, "{at}  {said}  · {memory_mb} mb");
        }
    }

    /// The view the window shows now: said in a line of its own when it
    /// is another than before.
    pub fn view_is(&mut self, view: &str) {
        if self.view != view {
            self.view = view.to_string();
            self.note(&format!("ansicht: {view}"));
        }
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// The frame begins: the first thing `logic` does.
    pub fn begin(&mut self) {
        self.soon = std::mem::take(&mut *SOON.lock().unwrap_or_else(|e| e.into_inner()));
        let now = Instant::now();
        self.frame_began = Some(now);
        self.lapped = Some(now);
        self.steps.clear();
        self.pressed.clear();
    }

    /// One step of `logic` is done: the time since the last one is put down
    /// under this name, if it is the longest of the span.
    pub fn lap(&mut self, what: &'static str) {
        let now = Instant::now();
        if let Some(then) = self.lapped.replace(now) {
            let took = (now - then).as_secs_f32() * 1000.0;
            self.steps.push((what, took));
            if took > self.slowest.0 {
                self.slowest = (took, what);
            }
        }
    }

    /// One part of the drawing is done: put down for this frame only. The
    /// span's line keeps its longest step of the logic alone.
    pub fn drawn(&mut self, what: &'static str) {
        let now = Instant::now();
        if let Some(then) = self.lapped.replace(now) {
            self.steps.push((what, (now - then).as_secs_f32() * 1000.0));
        }
    }

    /// The frame's logic is done: the last thing `logic` does.
    pub fn logic_done(&mut self) {
        if let Some(began) = self.frame_began {
            self.logic_took = Some(began.elapsed().as_secs_f32() * 1000.0);
        }
    }

    /// The frame is drawn: the last thing `ui` does. Writes a line when the
    /// span is full.
    /// `asked` is what requested this frame, when no hand did: egui keeps
    /// the place of every `request_repaint`.
    pub fn frame(&mut self, doing: Doing, asked: &[eframe::egui::RepaintCause]) {
        if !doing.input {
            let known = self.soon.len() == asked.len();
            for (i, cause) in asked.iter().enumerate() {
                if known && self.soon[i] > QUICK {
                    continue;
                }
                let file = cause.file.rsplit(['/', '\\']).next().unwrap_or(cause.file);
                *self.asked.entry(format!("{file}:{}", cause.line)).or_default() += 1;
            }
        }
        if let Some(began) = self.frame_began.take() {
            let took = began.elapsed().as_secs_f32() * 1000.0;
            if took > SLOW_FRAME {
                self.say_slow(took);
            }
            self.frames.push(took);
            self.logic.push(self.logic_took.take().unwrap_or(0.0));
            self.doing.push(doing);
        }
        if self.began.elapsed() >= SPAN {
            self.write();
        }
    }

    /// What was pressed this frame, as it is put down if the frame is slow.
    pub fn pressed(&mut self, said: String) {
        self.pressed = said;
    }

    /// A slow frame's own line: its three longest steps, the rest together,
    /// and what was pressed.
    fn say_slow(&mut self, took: f32) {
        let mut steps = self.steps.clone();
        steps.sort_by(|a, b| b.1.total_cmp(&a.1));
        let named: f32 = steps.iter().take(3).map(|(_, ms)| ms).sum();
        let mut said: Vec<String> = steps.iter().take(3).map(|(what, ms)| format!("{what} {ms:.0}")).collect();
        let rest = took - named;
        if rest >= 1.0 {
            said.push(format!("rest {rest:.0}"));
        }
        let pressed = if self.pressed.is_empty() { String::new() } else { format!(" · gedrückt: {}", self.pressed) };
        self.note(&format!("langsames bild: {took:.0} ms — {}{pressed}", said.join(", ")));
    }

    fn write(&mut self) {
        let seconds = self.began.elapsed().as_secs_f32();
        let cpu = cpu_time();
        let used = cpu.saturating_sub(self.cpu_then).as_secs_f32() / seconds.max(0.001) * 100.0;
        let mut sorted = self.frames.clone();
        sorted.sort_by(f32::total_cmp);
        let at = |share: f32| {
            sorted
                .get(((sorted.len() as f32 - 1.0) * share).round().max(0.0) as usize)
                .copied()
                .unwrap_or(0.0)
        };
        let quiet = self.doing.iter().filter(|d| !d.input).count();
        let logic = self.logic.iter().copied().fold(0.0f32, f32::max);
        let mut asked: Vec<(&String, &usize)> = self.asked.iter().collect();
        asked.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        let asked = asked
            .iter()
            .take(3)
            .map(|(place, n)| format!("{place} ×{n}"))
            .collect::<Vec<_>>()
            .join(", ");
        let line = format!(
            "{:.1}  {}  {:.1}/s ({:.1}/s)  {:.1} / {:.1} / {:.1} ms  {:.1} ms ({})  {:.0} %  {} ({}) mb  {}  {}  [{}]",
            seconds,
            sorted.len(),
            sorted.len() as f32 / seconds.max(0.001),
            quiet as f32 / seconds.max(0.001),
            at(0.5),
            at(0.95),
            sorted.last().copied().unwrap_or(0.0),
            logic,
            if self.slowest.1.is_empty() { "-" } else { self.slowest.1 },
            used,
            crate::player::watch::memory_now_mb(),
            peak_memory_mb(),
            self.textures,
            Doing::said(&self.doing),
            asked,
        );
        if let Ok(mut file) = std::fs::OpenOptions::new().append(true).open(&self.path) {
            let _ = writeln!(file, "{line}");
        }
        self.began = Instant::now();
        self.cpu_then = cpu;
        self.slowest = (0.0, "");
        self.asked.clear();
        self.frames.clear();
        self.logic.clear();
        self.doing.clear();
    }
}

/// The processor time this program has used so far, all its threads.
fn cpu_time() -> Duration {
    #[cfg(unix)]
    {
        let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
        if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) } == 0 {
            let time = |t: libc::timeval| {
                Duration::from_secs(t.tv_sec as u64) + Duration::from_micros(t.tv_usec as u64)
            };
            return time(usage.ru_utime) + time(usage.ru_stime);
        }
    }
    Duration::ZERO
}

/// The most memory the program has held at once, in megabytes. A Mac
/// counts it in bytes, Linux in kilobytes.
fn peak_memory_mb() -> u64 {
    #[cfg(unix)]
    {
        let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
        if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) } == 0 {
            let peak = usage.ru_maxrss as u64;
            return if cfg!(target_os = "macos") { peak / 1024 / 1024 } else { peak / 1024 };
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One line a span, with the frames counted and what went on named.
    #[test]
    fn a_span_is_written_as_one_line() {
        let path = std::env::temp_dir().join(format!("kessler-messung-{}.log", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut measure = Measure::start(path.clone()).unwrap();
        let asked = [eframe::egui::RepaintCause::new()];
        for _ in 0..3 {
            measure.begin();
            std::thread::sleep(Duration::from_millis(2));
            measure.lap("abholen");
            measure.lap("player");
            measure.logic_done();
            measure.frame(
                Doing { playing: true, focused: true, field_scope: true, ..Doing::default() },
                &asked,
            );
        }
        measure.write();
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2, "{text}");
        assert!(lines[0].starts_with("# kessler"));
        assert!(lines[1].contains("  3  "), "{}", lines[1]);
        assert!(lines[1].contains("/s ("), "{}", lines[1]);
        assert!(lines[1].contains("ms (abholen)"), "{}", lines[1]);
        assert!(lines[1].contains("musik vorne skop  [measure.rs:"), "{}", lines[1]);
        assert!(lines[1].ends_with(" ×3]"), "{}", lines[1]);
        let _ = std::fs::remove_file(&path);
    }

    /// A change of view is said once, with the memory held at that moment;
    /// the same view again says nothing.
    #[test]
    fn a_change_of_view_is_said_once_with_the_memory() {
        let path = std::env::temp_dir().join(format!("kessler-messung-ansicht-{}.log", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut measure = Measure::start(path.clone()).unwrap();
        measure.view_is("musik");
        measure.view_is("musik");
        measure.view_is("podcasts · alle");
        let text = std::fs::read_to_string(&path).unwrap();
        let said: Vec<&str> = text.lines().filter(|l| l.contains("ansicht:")).collect();
        assert_eq!(said.len(), 2, "{text}");
        assert!(said[1].contains("ansicht: podcasts · alle  · ") && said[1].ends_with(" mb"), "{}", said[1]);
        if cfg!(any(target_os = "linux", target_os = "macos")) {
            assert!(crate::player::watch::memory_now_mb() > 0);
        }
        let _ = std::fs::remove_file(&path);
    }
}
