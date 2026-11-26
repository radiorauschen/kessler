//! Every colour scheme, one after the other, on the view as it stands,
//! caught and written out as one short film — each scheme light, then
//! dark, then the next — to show what the schemes look like at a glance,
//! on a project page for instance. As a GIF (`alt` + `shift` + `g`), or as
//! an MP4 through ffmpeg (`alt` + `shift` + `m`), which keeps the window's
//! own sharpness and every colour; behind ⌥ under „über" as well.

use crate::gui::*;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

/// How long each scheme stands before it is caught: the colours are put
/// in at once, but covers screened in them and the status line's words
/// take a few frames to arrive.
const SETTLE_SECS: f64 = 0.45;

/// How long each half of a scheme stands in the film, in milliseconds.
const SHOWN_MS: u32 = 800;

/// The widest a GIF's frame is kept, in pixels: wider than any project
/// page shows a picture, and a GIF of a window at twice the size runs to
/// tens of megabytes.
const GIF_WIDEST: u32 = 1280;

/// The widest an MP4's frame is kept: a window on a large screen at twice
/// the size, as it stands; only larger than that is brought down.
const MP4_WIDEST: u32 = 2880;

/// How hard the GIF encoder works for its 256 colours: 1 is the best and
/// slowest, 30 the fastest. Text stays clean at 10.
const QUANTISE_SPEED: i32 = 10;

/// What the film is written as.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Film {
    Gif,
    Mp4,
}

impl Film {
    fn extension(self) -> &'static str {
        match self {
            Self::Gif => "gif",
            Self::Mp4 => "mp4",
        }
    }
}

/// One frame.
type Still = image::RgbaImage;

/// The run, while it goes.
pub struct Reel {
    film: Film,
    /// What to put back afterwards.
    theme: Theme,
    brightness: Brightness,
    /// The schemes still to come, each light half followed by its dark one.
    ahead: VecDeque<Theme>,
    /// The scheme on screen now, since when, and whether its picture has
    /// been asked for.
    now: Option<(Theme, f64, bool)>,
    /// The GIF's frames, kept here: small, and the encoder wants them all.
    stills: Vec<Still>,
    /// The MP4's frames go to disk as they come, each on a thread of its
    /// own — at full size a few dozen of them would fill a gigabyte.
    frames: PathBuf,
    saving: Vec<std::thread::JoinHandle<std::result::Result<(), String>>>,
}

/// The file written, or what went wrong, from the thread that writes it.
pub type Written = mpsc::Receiver<std::result::Result<PathBuf, String>>;

impl GuiApp {
    /// Whether the schemes are being run through: the colours then change
    /// at once rather than fading, and the keyboard waits.
    pub(in crate::gui) fn reeling(&self) -> bool {
        self.reel.is_some()
    }

    /// Start, or stop if it is running.
    pub(in crate::gui) fn toggle_reel(&mut self, film: Film) {
        if self.reel.is_some() {
            self.end_reel();
            self.set_status(tr!("farbschemen aufnehmen: abgebrochen"));
            return;
        }
        if self.reel_written.is_some() {
            return;
        }
        if film == Film::Mp4 && crate::ipod::sync::transcoder().is_none() {
            self.set_status(tr!("mp4 bräuchte ffmpeg: brew install ffmpeg"));
            return;
        }
        let own = Theme::Eigenes.family();
        let ahead = Theme::families()
            .into_iter()
            .filter(|t| t.family() != own || self.resolved.family() == own)
            .flat_map(|t| [t.side(false), t.side(true)])
            .collect();
        self.menu_open = false;
        self.reel = Some(Box::new(Reel {
            film,
            theme: self.theme,
            brightness: self.brightness,
            ahead,
            now: None,
            stills: Vec::new(),
            frames: std::env::temp_dir().join(format!("kessler-reel-{}", std::process::id())),
            saving: Vec::new(),
        }));
    }

    /// One frame of the run: put the next scheme up, ask for its picture
    /// once it has settled, take the picture when it comes, and at the end
    /// hand the pictures to a thread to be written.
    pub(in crate::gui) fn tend_the_reel(&mut self, ctx: &egui::Context) {
        self.take_reel_written(ctx);
        let Some(reel) = self.reel.as_mut() else { return };
        ctx.request_repaint();
        let time = ctx.input(|i| i.time);
        let caught = CAUGHT.lock().ok().and_then(|mut held| held.take());
        match reel.now {
            None => {
                let Some(next) = reel.ahead.pop_front() else {
                    let reel = self.reel.take().expect("checked above");
                    self.write_reel(*reel);
                    return;
                };
                reel.now = Some((next, time, false));
                self.theme = next;
                let dark = next.palette().is_dark();
                self.brightness = if dark { Brightness::Dunkel } else { Brightness::Hell };
                let half = if dark { "dunkel" } else { "hell" };
                self.set_status(tr!(
                    "farben: {} · {}",
                    crate::lang::t(next.family()),
                    crate::lang::t(half)
                ));
            }
            Some((theme, since, false)) if time - since >= SETTLE_SECS => {
                reel.now = Some((theme, since, true));
                let ask = ctx.clone();
                ctx.request_screenshot(move |image| {
                    if let Ok(mut held) = CAUGHT.lock() {
                        *held = Some(image);
                    }
                    ask.request_repaint();
                });
            }
            Some((_, _, true)) => {
                if let Some(image) = caught {
                    match reel.film {
                        Film::Gif => reel.stills.push(still_of(&image, GIF_WIDEST)),
                        Film::Mp4 => {
                            let path = reel.frames.join(format!("{:04}.png", reel.saving.len()));
                            reel.saving.push(std::thread::spawn(move || {
                                if let Some(dir) = path.parent() {
                                    std::fs::create_dir_all(dir).map_err(|e| format!("{e}"))?;
                                }
                                still_of(&image, MP4_WIDEST).save(&path).map_err(|e| format!("{e}"))
                            }));
                        }
                    }
                    reel.now = None;
                }
            }
            Some(_) => {}
        }
    }

    /// The scheme there was before, put back as it was.
    fn end_reel(&mut self) {
        if let Some(reel) = self.reel.take() {
            self.theme = reel.theme;
            self.brightness = reel.brightness;
            let (frames, saving) = (reel.frames, reel.saving);
            std::thread::spawn(move || {
                for saved in saving {
                    let _ = saved.join();
                }
                let _ = std::fs::remove_dir_all(frames);
            });
        }
    }

    /// Put the colours back and write the film on a thread of its own: a
    /// few dozen frames of a whole window take a while to encode.
    fn write_reel(&mut self, reel: Reel) {
        self.theme = reel.theme;
        self.brightness = reel.brightness;
        self.set_status(tr!("farbschemen aufnehmen: wird geschrieben …"));
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        let desktop = home.join("Desktop");
        let dir = if desktop.is_dir() { desktop } else { home };
        let stamp = chrono::Local::now().format("%Y-%m-%d-%H%M").to_string();
        let path = dir.join(format!("kessler-farbschemen-{stamp}.{}", reel.film.extension()));
        let (tell, written) = mpsc::channel();
        self.reel_written = Some(written);
        let Reel { film, stills, frames, saving, .. } = reel;
        std::thread::spawn(move || {
            let done = match film {
                Film::Gif => write_gif(&path, stills).map_err(|e| format!("{e}")),
                Film::Mp4 => write_mp4(&path, &frames, saving),
            };
            let _ = tell.send(done.map(|()| path));
        });
    }

    /// Whether the thread is done, and if so, say so and show the file.
    fn take_reel_written(&mut self, ctx: &egui::Context) {
        let Some(written) = &self.reel_written else { return };
        let answer = match written.try_recv() {
            Ok(answer) => answer,
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(250));
                return;
            }
            Err(mpsc::TryRecvError::Disconnected) => Err(tr!("farbschemen aufnehmen: abgebrochen").to_string()),
        };
        self.reel_written = None;
        match answer {
            Ok(path) => {
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                self.set_notice(tr!("gespeichert: {}", name));
                let _ = crate::gui::reveal::show(&path);
            }
            Err(e) => self.set_status(e),
        }
    }
}

/// A picture of the window, brought down to `widest` if it is wider.
/// The window's picture, once it has been taken: since egui 0.37 it comes
/// to a callback rather than among the frame's events.
static CAUGHT: std::sync::Mutex<Option<std::sync::Arc<egui::ColorImage>>> = std::sync::Mutex::new(None);

fn still_of(image: &egui::ColorImage, widest: u32) -> Still {
    let [w, h] = image.size;
    let bytes: Vec<u8> = image.pixels.iter().flat_map(|c| [c.r(), c.g(), c.b(), 255]).collect();
    let full = Still::from_raw(w as u32, h as u32, bytes).unwrap_or_default();
    if full.width() <= widest {
        return full;
    }
    let height = (full.height() as u64 * widest as u64 / full.width() as u64).max(1) as u32;
    image::imageops::resize(&full, widest, height, image::imageops::FilterType::CatmullRom)
}

/// The frames as a GIF that goes round for ever.
fn write_gif(path: &Path, stills: Vec<Still>) -> image::ImageResult<()> {
    use image::codecs::gif::{GifEncoder, Repeat};
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut encoder = GifEncoder::new_with_speed(file, QUANTISE_SPEED);
    encoder.set_repeat(Repeat::Infinite)?;
    let delay = image::Delay::from_numer_denom_ms(SHOWN_MS, 1);
    encoder.encode_frames(stills.into_iter().map(|s| image::Frame::from_parts(s, 0, 0, delay)))
}

/// The frames as an MP4: the PNGs once they are all on disk, strung
/// together by ffmpeg in H.264 — the format every browser and GitHub play —
/// at a quality where text stays crisp, in even sizes as the format wants.
fn write_mp4(
    path: &Path,
    frames: &Path,
    saving: Vec<std::thread::JoinHandle<std::result::Result<(), String>>>,
) -> std::result::Result<(), String> {
    let made = (|| {
        for saved in saving {
            saved.join().map_err(|_| "png".to_string())??;
        }
        let ffmpeg = crate::ipod::sync::transcoder()
            .ok_or_else(|| tr!("mp4 bräuchte ffmpeg: brew install ffmpeg").to_string())?;
        let output = std::process::Command::new(ffmpeg)
            .args(mp4_arguments(frames, path))
            .stdin(std::process::Stdio::null())
            .output()
            .map_err(|e| format!("ffmpeg: {e}"))?;
        if output.status.success() {
            Ok(())
        } else {
            let said = String::from_utf8_lossy(&output.stderr);
            Err(format!("ffmpeg: {}", said.lines().last().unwrap_or_default()))
        }
    })();
    let _ = std::fs::remove_dir_all(frames);
    made
}

/// What ffmpeg is told: one picture every [`SHOWN_MS`], played at 30
/// frames a second, sharp (`crf` 16, tuned for still pictures), in the
/// colours every player knows (`yuv420p`), ready to play while it loads.
fn mp4_arguments(frames: &Path, path: &Path) -> Vec<std::ffi::OsString> {
    let rate = format!("1000/{SHOWN_MS}");
    let mut arguments: Vec<std::ffi::OsString> = [
        "-y", "-loglevel", "error", "-framerate", &rate, "-i",
    ]
    .iter()
    .map(Into::into)
    .collect();
    arguments.push(frames.join("%04d.png").into_os_string());
    for word in [
        "-vf",
        "scale=trunc(iw/2)*2:trunc(ih/2)*2:flags=lanczos,fps=30",
        "-c:v",
        "libx264",
        "-preset",
        "slow",
        "-tune",
        "stillimage",
        "-crf",
        "16",
        "-pix_fmt",
        "yuv420p",
        "-movflags",
        "+faststart",
    ] {
        arguments.push(word.into());
    }
    arguments.push(path.as_os_str().to_owned());
    arguments
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wide_window_is_brought_down_and_a_narrow_one_kept() {
        let wide = egui::ColorImage::new([2880, 1800], vec![egui::Color32::RED; 2880 * 1800]);
        let still = still_of(&wide, GIF_WIDEST);
        assert_eq!((still.width(), still.height()), (GIF_WIDEST, 800));
        assert_eq!(still_of(&wide, MP4_WIDEST).dimensions(), (2880, 1800));
        let narrow = egui::ColorImage::new([400, 300], vec![egui::Color32::BLUE; 400 * 300]);
        assert_eq!(still_of(&narrow, GIF_WIDEST).dimensions(), (400, 300));
    }

    #[test]
    fn the_frames_come_out_as_a_gif() {
        let dir = std::env::temp_dir().join(format!("kessler-reel-gif-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("probe.gif");
        let stills = vec![Still::from_pixel(8, 6, image::Rgba([10, 20, 30, 255])); 3];
        write_gif(&path, stills).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.starts_with(b"GIF89a"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn ffmpeg_is_asked_for_one_picture_every_shown_time() {
        let said: Vec<String> = mp4_arguments(Path::new("/f"), Path::new("/o.mp4"))
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let rate = said.iter().position(|a| a == "-framerate").unwrap();
        assert_eq!(said[rate + 1], format!("1000/{SHOWN_MS}"));
        assert!(said.contains(&"/f/%04d.png".to_string()));
        assert_eq!(said.last().unwrap(), "/o.mp4");
    }

    #[test]
    #[ignore = "needs ffmpeg"]
    fn the_frames_come_out_as_an_mp4() {
        let dir = std::env::temp_dir().join(format!("kessler-reel-mp4-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("probe.mp4");
        let frames = dir.join("frames");
        std::fs::create_dir_all(&frames).unwrap();
        for (at, red) in [10u8, 200].into_iter().enumerate() {
            Still::from_pixel(65, 41, image::Rgba([red, 20, 30, 255])).save(frames.join(format!("{at:04}.png"))).unwrap();
        }
        write_mp4(&path, &frames, Vec::new()).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[4..8], b"ftyp");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
