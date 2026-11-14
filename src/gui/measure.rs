use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const SPAN: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Doing {
    pub playing: bool,
    pub focused: bool,
    pub unseen: bool,
    pub field_scope: bool,
    pub menu: bool,
    pub covers_waiting: bool,
    pub podcasts: bool,
    pub feeds: bool,
    pub fetching: bool,
    pub input: bool,
}

impl Doing {
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

pub struct Measure {
    path: PathBuf,
    began: Instant,
    cpu_then: Duration,
    frame_began: Option<Instant>,
    logic_took: Option<f32>,
    lapped: Option<Instant>,
    slowest: (f32, &'static str),
    asked: std::collections::HashMap<String, usize>,
    frames: Vec<f32>,
    logic: Vec<f32>,
    doing: Vec<Doing>,
    pub textures: String,
    view: String,
}

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

impl Measure {
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
            asked: std::collections::HashMap::new(),
            frames: Vec::new(),
            logic: Vec::new(),
            doing: Vec::new(),
            textures: String::new(),
            view: String::new(),
        })
    }

    pub fn note(&mut self, said: &str) {
        let at = chrono::Local::now().format("%H:%M:%S").to_string();
        self.write_note(&at, said, crate::player::watch::memory_now_mb());
    }

    pub fn noted(&mut self, note: &crate::player::watch::Note) {
        self.write_note(&note.at, &note.said, note.memory_mb);
    }

    fn write_note(&self, at: &str, said: &str, memory_mb: u64) {
        if let Ok(mut file) = std::fs::OpenOptions::new().append(true).open(&self.path) {
            let _ = writeln!(file, "{at}  {said}  · {memory_mb} mb");
        }
    }

    pub fn view_is(&mut self, view: &str) {
        if self.view != view {
            self.view = view.to_string();
            self.note(&format!("ansicht: {view}"));
        }
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    pub fn begin(&mut self) {
        let now = Instant::now();
        self.frame_began = Some(now);
        self.lapped = Some(now);
    }

    pub fn lap(&mut self, what: &'static str) {
        let now = Instant::now();
        if let Some(then) = self.lapped.replace(now) {
            let took = (now - then).as_secs_f32() * 1000.0;
            if took > self.slowest.0 {
                self.slowest = (took, what);
            }
        }
    }

    pub fn logic_done(&mut self) {
        if let Some(began) = self.frame_began {
            self.logic_took = Some(began.elapsed().as_secs_f32() * 1000.0);
        }
    }

    pub fn frame(&mut self, doing: Doing, asked: &[eframe::egui::RepaintCause]) {
        if !doing.input {
            for cause in asked {
                let file = cause.file.rsplit(['/', '\\']).next().unwrap_or(cause.file);
                *self.asked.entry(format!("{file}:{}", cause.line)).or_default() += 1;
            }
        }
        if let Some(began) = self.frame_began.take() {
            self.frames.push(began.elapsed().as_secs_f32() * 1000.0);
            self.logic.push(self.logic_took.take().unwrap_or(0.0));
            self.doing.push(doing);
        }
        if self.began.elapsed() >= SPAN {
            self.write();
        }
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
