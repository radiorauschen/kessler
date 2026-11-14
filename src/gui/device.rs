use std::path::PathBuf;

use crate::db::models::Track;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Body {
    #[default]
    Player,
    Shuffle1,
    Shuffle2,
    Shuffle3,
    Shuffle4,
}

impl Body {
    pub fn of(generation: crate::ipod::device_info::Generation) -> Self {
        use crate::ipod::device_info::Generation;
        match generation {
            Generation::Shuffle1 => Self::Shuffle1,
            Generation::Shuffle2 => Self::Shuffle2,
            Generation::Shuffle3 => Self::Shuffle3,
            Generation::Shuffle4 => Self::Shuffle4,
            _ => Self::Player,
        }
    }

    fn shape(self) -> Shape {
        match self {
            Self::Player => Shape {
                half: HALF,
                corner: CORNER,
                edge: EDGE,
                screen: true,
                control: Some((WHEEL_Y, WHEEL_R, BUTTON_R)),
                light: None,
            },
            Self::Shuffle1 => Shape {
                half: [0.15, 0.5, 0.05],
                corner: 0.1,
                edge: 0.035,
                screen: false,
                control: Some((0.2, 0.135, 0.05)),
                light: None,
            },
            Self::Shuffle2 => Shape {
                half: [0.33, 0.5, 0.07],
                corner: 0.09,
                edge: 0.045,
                screen: false,
                control: Some((0.0, 0.27, 0.1)),
                light: None,
            },
            Self::Shuffle3 => Shape {
                half: [0.195, 0.5, 0.075],
                corner: 0.08,
                edge: 0.05,
                screen: false,
                control: None,
                light: Some((0.38, 0.03)),
            },
            Self::Shuffle4 => Shape {
                half: [0.46, 0.5, 0.085],
                corner: 0.1,
                edge: 0.05,
                screen: false,
                control: Some((0.0, 0.33, 0.12)),
                light: None,
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Shape {
    half: [f32; 3],
    corner: f32,
    edge: f32,
    screen: bool,
    control: Option<(f32, f32, f32)>,
    light: Option<(f32, f32)>,
}

pub type Made = (u32, Body, Option<crate::gui::dither::Look>);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Turn([f32; 4]);

impl Default for Turn {
    fn default() -> Self {
        Turn::FACE_ON
    }
}

impl Turn {
    pub const FACE_ON: Turn = Turn([1.0, 0.0, 0.0, 0.0]);

    pub fn about(axis: [f32; 3], angle: f32) -> Turn {
        let axis = normalised(axis);
        let (s, c) = (angle / 2.0).sin_cos();
        Turn([c, axis[0] * s, axis[1] * s, axis[2] * s]).tidied()
    }

    fn then(self, then: Turn) -> Turn {
        let [a, b, c, d] = then.0;
        let [e, f, g, h] = self.0;
        Turn([
            a * e - b * f - c * g - d * h,
            a * f + b * e + c * h - d * g,
            a * g - b * h + c * e + d * f,
            a * h + b * g - c * f + d * e,
        ])
        .tidied()
    }

    fn undone(self) -> Turn {
        let [w, x, y, z] = self.0;
        Turn([w, -x, -y, -z])
    }

    fn tidied(self) -> Turn {
        let length = self.0.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-6);
        let sign = if self.0[0] < 0.0 { -1.0 } else { 1.0 };
        Turn(self.0.map(|v| v * sign / length))
    }

    pub fn apply(self, p: [f32; 3]) -> [f32; 3] {
        let [w, x, y, z] = self.0;
        let u = [x, y, z];
        let t = cross(u, p).map(|v| 2.0 * v);
        let ut = cross(u, t);
        [p[0] + w * t[0] + ut[0], p[1] + w * t[1] + ut[1], p[2] + w * t[2] + ut[2]]
    }

    fn toward(self, to: Turn, share: f32) -> Turn {
        let mut to = to.0;
        let mut cos = self.0.iter().zip(&to).map(|(a, b)| a * b).sum::<f32>();
        if cos < 0.0 {
            to = to.map(|v| -v);
            cos = -cos;
        }
        let (a, b) = if cos > 0.9995 {
            (1.0 - share, share)
        } else {
            let angle = cos.acos();
            let sin = angle.sin();
            (((1.0 - share) * angle).sin() / sin, (share * angle).sin() / sin)
        };
        Turn([0, 1, 2, 3].map(|i| a * self.0[i] + b * to[i])).tidied()
    }

    pub fn stood(self) -> [i32; 4] {
        self.0.map(|v| (v * 1000.0).round() as i32)
    }
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Handling {
    pub turn: Turn,
    let_go: Option<(f64, Turn)>,
    spun_at: Option<f64>,
}

pub const REST_SECS: f64 = 2.0;
pub const BACK_SECS: f64 = 1.2;
pub const SPIN_SECS: f64 = 4.0;

impl Handling {
    pub fn turn_by(&mut self, right: f32, down: f32) {
        let angle = (right * right + down * down).sqrt();
        if angle > 0.0 {
            self.turn = self.turn.then(Turn::about([down, right, 0.0], angle));
        }
        self.let_go = None;
    }

    pub fn let_go(&mut self, now: f64) {
        if !self.at_rest() {
            self.let_go = Some((now, self.turn));
        }
    }

    pub fn settle(&mut self, now: f64) -> Option<f64> {
        let (at, turn) = self.let_go?;
        let since = now - at;
        if since < REST_SECS {
            return Some(REST_SECS - since);
        }
        if since >= REST_SECS + BACK_SECS {
            *self = Self::default();
            return None;
        }
        let along = ease((since - REST_SECS) / BACK_SECS) as f32;
        self.turn = turn.toward(Turn::FACE_ON, along);
        Some(0.0)
    }

    pub fn spin(&mut self, now: f64) {
        let since = self.spun_at.map_or(0.0, |at| (now - at).clamp(0.0, 0.25));
        let angle = (since / SPIN_SECS * std::f64::consts::TAU) as f32;
        self.turn = self.turn.then(Turn::about([0.0, 1.0, 0.0], angle));
        self.spun_at = Some(now);
        self.let_go = None;
    }

    pub fn stop_spinning(&mut self, now: f64) {
        if self.spun_at.take().is_some() && !self.at_rest() {
            self.let_go = Some((now - REST_SECS, self.turn));
        }
    }

    pub fn at_rest(&self) -> bool {
        self.turn == Turn::FACE_ON
    }
}

fn ease(t: f64) -> f64 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

const HALF: [f32; 3] = [0.30, 0.50, 0.052];
const CORNER: f32 = 0.07;
const EDGE: f32 = 0.04;
const SCREEN: [f32; 2] = [0.235, 0.175];
const SCREEN_Y: f32 = 0.235;
const WHEEL_Y: f32 = -0.2;
const WHEEL_R: f32 = 0.19;
const BUTTON_R: f32 = 0.068;
const REFLECTION: f32 = 0.45;
const REFLECTION_WIDTH: f32 = 0.18;
const REFLECTION_AT: f32 = 0.55;
const REFLECTION_TRAVEL: f32 = 2.5;
const FILL: f32 = 0.8;

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalised(a: [f32; 3]) -> [f32; 3] {
    let length = dot(a, a).sqrt().max(1e-6);
    [a[0] / length, a[1] / length, a[2] / length]
}

fn distance(p: [f32; 3], shape: &Shape) -> f32 {
    let half = shape.half;
    let inset = shape.edge;
    let r = shape.corner - inset;
    let qx = p[0].abs() - (half[0] - inset - r);
    let qy = p[1].abs() - (half[1] - inset - r);
    let outline = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r;
    let wz = p[2].abs() - (half[2] - inset);
    let wx = outline;
    (wx.max(0.0).powi(2) + wz.max(0.0).powi(2)).sqrt() + wx.max(wz).min(0.0) - inset
}

fn normal(p: [f32; 3], shape: &Shape) -> [f32; 3] {
    const H: f32 = 0.0015;
    let d = |dx: f32, dy: f32, dz: f32| distance([p[0] + dx, p[1] + dy, p[2] + dz], shape);
    normalised([
        d(H, 0.0, 0.0) - d(-H, 0.0, 0.0),
        d(0.0, H, 0.0) - d(0.0, -H, 0.0),
        d(0.0, 0.0, H) - d(0.0, 0.0, -H),
    ])
}

fn brightness(
    p: [f32; 3],
    n: [f32; 3],
    view: [f32; 3],
    light: [f32; 3],
    shape: &Shape,
) -> f32 {
    let lambert = dot(n, light).max(0.0);
    let front = n[2] > 0.85 && p[2] > 0.0;
    if front {
        let (x, y) = (p[0], p[1]);
        if !shape.screen {
            return face(x, y, lambert, shape);
        }
        if x.abs() < SCREEN[0] && (y - SCREEN_Y).abs() < SCREEN[1] {
            let sheen = ((y - SCREEN_Y) / SCREEN[1] * 0.5 + 0.5).clamp(0.0, 1.0);
            let facing = dot(n, view);
            let mirrored = [2.0 * facing * n[0] - view[0], 2.0 * facing * n[1] - view[1]];
            let across = (x / SCREEN[0] + (y - SCREEN_Y) / SCREEN[1]) * 0.5;
            let at = across + (mirrored[0] + mirrored[1]) * REFLECTION_TRAVEL;
            let band = (-((at - REFLECTION_AT) / REFLECTION_WIDTH).powi(2)).exp();
            return 0.03 + 0.1 * sheen * lambert + REFLECTION * band;
        }
        return face(x, y, lambert, shape);
    }
    let towards = sub(light, view);
    let half = normalised(towards);
    let shine = dot(n, half).max(0.0).powf(18.0);
    let fall = (p[1] + shape.half[1]) / (2.0 * shape.half[1]);
    (0.08 + 0.25 * lambert + 0.22 * fall + 0.75 * shine).min(1.0)
}

fn face(x: f32, y: f32, lambert: f32, shape: &Shape) -> f32 {
    if let Some((middle, ring, button)) = shape.control {
        let from_wheel = (x * x + (y - middle).powi(2)).sqrt();
        if from_wheel < button - 0.01 {
            return 0.35 + 0.6 * lambert;
        }
        if from_wheel < button + 0.012 || (from_wheel - ring).abs() < 0.012 {
            return 0.02;
        }
        if from_wheel < ring {
            return 0.12 + 0.33 * lambert;
        }
    }
    if let Some((height, size)) = shape.light {
        if (x * x + (y - height).powi(2)).sqrt() < size {
            return 0.02;
        }
    }
    0.35 + 0.6 * lambert
}

pub fn picture(
    turn: Turn,
    width: usize,
    height: usize,
    body: Body,
) -> (Vec<u8>, Vec<bool>) {
    let shape = body.shape();
    let mut grey = vec![0u8; width * height];
    let mut covered = vec![false; width * height];
    let span = 1.0 / FILL;
    let scale = span / height.min(width).max(1) as f32;
    let back = turn.undone();
    let view = back.apply([0.0, 0.0, -1.0]);
    let light = back.apply(normalised([-0.5, 0.6, 0.8]));
    let toward_eye = [-view[0], -view[1], -view[2]];

    for row in 0..height {
        for column in 0..width {
            let sx = (column as f32 + 0.5 - width as f32 / 2.0) * scale;
            let sy = (height as f32 / 2.0 - row as f32 - 0.5) * scale;
            let origin = back.apply([sx, sy, 2.0]);
            let mut t = 0.0;
            let mut hit = None;
            for _ in 0..64 {
                let p = [
                    origin[0] + view[0] * t,
                    origin[1] + view[1] * t,
                    origin[2] + view[2] * t,
                ];
                let d = distance(p, &shape);
                if d < 0.0015 {
                    hit = Some(p);
                    break;
                }
                t += d;
                if t > 4.0 {
                    break;
                }
            }
            if let Some(p) = hit {
                let n = normal(p, &shape);
                let b = brightness(p, n, toward_eye, light, &shape);
                let at = row * width + column;
                grey[at] = (b.clamp(0.0, 1.0) * 255.0) as u8;
                covered[at] = true;
            }
        }
    }
    (grey, covered)
}

#[cfg(test)]
fn seen(p: [f32; 3], yaw: f32) -> [f32; 3] {
    Turn::about([0.0, 1.0, 0.0], yaw).apply(p)
}

#[derive(Debug, Clone, Default)]
pub struct Summary {
    pub for_device: (PathBuf, usize),
    pub tracks: usize,
    pub albums: usize,
    pub artists: usize,
    pub seconds: f64,
    pub music_bytes: u64,
    pub formats: Vec<(String, usize)>,
    pub serial: Option<String>,
    pub written: Option<String>,
    pub missing: usize,
    pub missing_bytes: u64,
    pub unplayable: usize,
    pub unplayable_formats: Vec<String>,
    pub newest_current: bool,
    pub retag: usize,
    pub sound_only: usize,
    pub respell: usize,
    pub reorder: bool,
    pub old_lists: usize,
}

fn fingerprint(track: &Track) -> String {
    format!(
        "{}\u{1}{}\u{1}{}\u{1}{}",
        track.display_artist().trim().to_lowercase(),
        track.album.trim().to_lowercase(),
        track.display_title().trim().to_lowercase(),
        track.duration_secs.round().max(0.0) as u64
    )
}

impl Summary {
    pub fn of(
        mount: PathBuf,
        on_device: &[Track],
        library: &[Track],
        plays: impl Fn(&str) -> bool,
        serial: Option<String>,
        written: Option<String>,
    ) -> Self {
        use std::collections::{BTreeMap, HashSet};

        let mut albums = HashSet::new();
        let mut artists = HashSet::new();
        let mut formats: BTreeMap<String, usize> = BTreeMap::new();
        let mut seconds = 0.0;
        let mut music_bytes = 0;
        let mut present = HashSet::new();
        for track in on_device {
            let artist = crate::db::sort_key(crate::db::grouping_artist(track));
            albums.insert((artist.clone(), crate::db::sort_key(&track.album)));
            artists.insert(artist);
            let format = match track.format.trim() {
                "" => tr!("andere").to_string(),
                known => known.to_lowercase(),
            };
            *formats.entry(format).or_default() += 1;
            seconds += track.duration_secs.max(0.0);
            music_bytes += track.file_size;
            present.insert(fingerprint(track));
        }
        let mut formats: Vec<(String, usize)> = formats.into_iter().collect();
        formats.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        let mut missing = 0;
        let mut missing_bytes = 0;
        let mut unplayable = 0;
        let mut unplayable_formats = Vec::new();
        let mut counted = HashSet::new();
        for track in library {
            let fp = fingerprint(track);
            if present.contains(&fp) || !counted.insert(fp) {
                continue;
            }
            if plays(&track.format) {
                missing += 1;
                missing_bytes += track.file_size;
            } else {
                unplayable += 1;
                let format = track.format.to_lowercase();
                if !unplayable_formats.contains(&format) {
                    unplayable_formats.push(format);
                }
            }
        }
        unplayable_formats.sort();

        Summary {
            for_device: (mount, on_device.len()),
            tracks: on_device.len(),
            albums: albums.len(),
            artists: artists.len(),
            seconds,
            music_bytes,
            formats,
            serial,
            written,
            missing,
            missing_bytes,
            unplayable,
            unplayable_formats,
            newest_current: true,
            retag: 0,
            sound_only: 0,
            respell: 0,
            reorder: false,
            old_lists: 0,
        }
    }
}

pub fn spoken_length(seconds: f64) -> String {
    let minutes = (seconds / 60.0).round() as u64;
    let (days, hours, mins) = (minutes / 1440, (minutes / 60) % 24, minutes % 60);
    let days_word = if days == 1 { tr!("tag") } else { tr!("tage") };
    match (days, hours) {
        (0, 0) => format!("{mins} min"),
        (0, _) => tr!("{} std {} min", hours, mins),
        (_, 0) => format!("{days} {days_word}"),
        _ => tr!("{} {} {} std", days, days_word, hours),
    }
}

pub fn formats_line(formats: &[(String, usize)]) -> String {
    formats
        .iter()
        .map(|(format, count)| format!("{format} {}", super::text::count(*count as u64)))
        .collect::<Vec<_>>()
        .join("  ·  ")
}

pub fn shares(total: u64, free: u64, music: u64) -> [f32; 3] {
    if total == 0 {
        return [0.0, 0.0, 1.0];
    }
    let used = total.saturating_sub(free);
    let music = music.min(used);
    let other = used - music;
    let whole = total as f64;
    [
        (music as f64 / whole) as f32,
        (other as f64 / whole) as f32,
        (free.min(total) as f64 / whole) as f32,
    ]
}

pub struct Removal {
    pub ids: Vec<u32>,
    pub what: String,
    pub bytes: u64,
    pub going: Option<std::sync::mpsc::Receiver<crate::error::Result<crate::ipod::sync::Removed>>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(artist: &str, album: &str, title: &str, format: &str, secs: f64) -> Track {
        Track {
            id: 0,
            path: PathBuf::from(format!("/m/{title}.{format}")),
            title: title.into(),
            artist: artist.into(),
            album_artist: String::new(),
            album: album.into(),
            genre: String::new(),
            year: None,
            track_number: None,
            disc_number: None,
            duration_secs: secs,
            bitrate: None,
            sample_rate: None,
            file_size: 4_000_000,
            format: format.into(),
            rating: 0,
            play_count: 0,
            last_played: None,
            date_added: Default::default(),
            date_modified: Default::default(),
            file_hash: None,
            has_artwork: false,
        }
    }

    #[test]
    fn the_player_is_drawn_and_turns() {
        let (w, h) = (80, 100);
        let (front, covered) = picture(Turn::FACE_ON, w, h, Body::Player);
        let share = covered.iter().filter(|c| **c).count() as f32 / (w * h) as f32;
        assert!(share > 0.2 && share < 0.7, "the player covers {share:.2} of the canvas");
        assert!(covered[(h / 2) * w + w / 2]);
        assert!(!covered[0] && !covered[w * h - 1]);

        let p = seen([0.0, SCREEN_Y, HALF[2]], 0.0);
        let scale = (1.0 / FILL) / w.min(h) as f32;
        let column = (p[0] / scale + w as f32 / 2.0) as usize;
        let row = (h as f32 / 2.0 - p[1] / scale) as usize;
        let screen = front[row * w + column];
        let face = front[(row + (0.2 / scale) as usize) * w + column];
        assert!(screen < 60, "the screen is {screen}");
        assert!(face > screen);

        let (back, _) = picture(Turn::about([0.0, 1.0, 0.0], std::f32::consts::PI), w, h, Body::Player);
        assert_ne!(back, front);
    }

    #[test]
    fn the_screen_mirrors_light_as_it_turns() {
        let (w, h) = (80, 100);
        let scale = (1.0 / FILL) / w.min(h) as f32;
        let screen: Vec<usize> = (0..w * h)
            .filter(|at| {
                let (x, y) = ((at % w) as f32 - w as f32 / 2.0, h as f32 / 2.0 - (at / w) as f32);
                (x * scale).abs() < SCREEN[0] * 0.9 && (y * scale - SCREEN_Y).abs() < SCREEN[1] * 0.9
            })
            .collect();
        let (face_on, _) = picture(Turn::FACE_ON, w, h, Body::Player);
        let brightest = screen.iter().map(|at| face_on[*at]).max().unwrap();
        let darkest = screen.iter().map(|at| face_on[*at]).min().unwrap();
        assert!(brightest > darkest + 60, "a band of light: {darkest} … {brightest}");
        let (turned, _) = picture(Turn::about([0.0, 1.0, 0.0], 0.15), w, h, Body::Player);
        let moved = screen.iter().filter(|at| face_on[**at].abs_diff(turned[**at]) > 30).count();
        assert!(moved > screen.len() / 10, "it moves: {moved} of {}", screen.len());
    }

    #[test]
    fn the_shuffles_are_other_shapes() {
        let (w, h) = (80, 80);
        let bodies = [Body::Player, Body::Shuffle1, Body::Shuffle2, Body::Shuffle3, Body::Shuffle4];
        let outlines: Vec<Vec<bool>> = bodies.iter().map(|b| picture(Turn::FACE_ON, w, h, *b).1).collect();
        for (i, a) in outlines.iter().enumerate() {
            assert!(a.iter().filter(|c| **c).count() > 200, "{:?} is drawn", bodies[i]);
            for b in &outlines[i + 1..] {
                assert_ne!(a, b);
            }
        }
        let across = |o: &Vec<bool>| o[(h / 2) * w..(h / 2 + 1) * w].iter().filter(|c| **c).count();
        assert!(across(&outlines[4]) > 2 * across(&outlines[1]));
    }

    fn off(turn: Turn) -> f32 {
        2.0 * turn.0[0].clamp(-1.0, 1.0).acos()
    }

    #[test]
    fn let_go_it_comes_back_face_on() {
        use std::f32::consts::PI;
        let mut held = Handling::default();
        assert!(held.at_rest());
        assert_eq!(held.settle(0.0), None);
        held.turn_by(1.5 * PI, 0.0);
        held.let_go(10.0);
        assert!((off(held.turn) - 0.5 * PI).abs() < 1e-3, "{}", off(held.turn));
        let was = held.turn;
        assert_eq!(held.settle(11.0), Some(1.0));
        assert_eq!(held.turn, was, "still as it was let go");
        let mut early = held;
        early.settle(12.0 + BACK_SECS * 0.1);
        assert!(off(early.turn) > 0.48 * PI, "slow to start: {}", off(early.turn));
        assert_eq!(held.settle(12.0 + BACK_SECS / 2.0), Some(0.0));
        assert!((off(held.turn) - 0.25 * PI).abs() < 1e-3, "half way at half the time");
        assert_eq!(held.settle(12.0 + BACK_SECS + 0.01), None);
        assert!(held.at_rest());
        held.turn_by(0.3, 0.0);
        assert_eq!(held.settle(100.0), None);
        assert!((off(held.turn) - 0.3).abs() < 1e-4);
    }

    #[test]
    fn it_spins_while_busy_and_comes_back() {
        use std::f32::consts::PI;
        let mut busy = Handling::default();
        busy.spin(0.0);
        assert!(busy.at_rest(), "the first step only starts the clock");
        let step = SPIN_SECS / 16.0;
        for i in 1..=4 {
            busy.spin(step * f64::from(i));
        }
        assert!((off(busy.turn) - PI / 2.0).abs() < 1e-3, "a quarter: {}", off(busy.turn));
        busy.spin(100.0);
        assert!(off(busy.turn) < PI / 2.0 + 0.5);
        busy.stop_spinning(101.0);
        assert_eq!(busy.settle(101.0), Some(0.0), "on its way back at once");
        assert_eq!(busy.settle(101.0 + BACK_SECS + 0.01), None);
        assert!(busy.at_rest());
        let mut still = Handling::default();
        still.stop_spinning(5.0);
        assert_eq!(still, Handling::default());
    }

    #[test]
    fn it_goes_the_way_the_hand_goes() {
        use std::f32::consts::FRAC_PI_2;
        let mut held = Handling::default();
        held.turn_by(0.0, FRAC_PI_2);
        let top = held.turn.apply([0.0, 1.0, 0.0]);
        assert!(top[2] > 0.99, "{top:?}");
        held.turn_by(0.3, 0.0);
        let top = held.turn.apply([0.0, 1.0, 0.0]);
        assert!(top[0] > 0.29, "{top:?}");
        let mut round = Handling::default();
        for (right, down) in [(0.0, 0.5), (0.5, 0.0), (0.0, -0.5), (-0.5, 0.0)] {
            round.turn_by(right, down);
        }
        let side = round.turn.apply([0.0, 1.0, 0.0]);
        assert!(side[0].abs() > 0.05, "{side:?}");
    }

    #[test]
    fn turns_undo_and_take_the_short_way() {
        let t = Turn::about([0.3, 1.0, -0.2], 2.0);
        let p = [0.1, 0.4, -0.7];
        let there = t.apply(p);
        let back = t.undone().apply(there);
        assert!((0..3).all(|i| (back[i] - p[i]).abs() < 1e-5));
        let far = Turn::about([0.0, 1.0, 0.0], 3.5);
        assert!(off(far) < std::f32::consts::PI, "written the short way: {}", off(far));
        assert_eq!(far.toward(Turn::FACE_ON, 1.0).stood(), Turn::FACE_ON.stood());
    }

    #[test]
    fn it_turns_over_the_top() {
        let (w, h) = (80, 100);
        let (face_on, _) = picture(Turn::FACE_ON, w, h, Body::Player);
        let (over, covered) = picture(Turn::about([1.0, 0.0, 0.0], 0.4), w, h, Body::Player);
        assert_ne!(face_on, over);
        assert!(covered[(h / 2) * w + w / 2]);
    }

    #[test]
    fn the_figures_count_what_is_there_and_what_is_not() {
        let device = vec![
            track("qluster", "echtzeit", "stein auf stein", "mp3", 181.0),
            track("qluster", "echtzeit", "beste freunde", "mp3", 414.0),
            track("popol vuh", "hosianna mantra", "ah!", "aac", 300.0),
        ];
        let library = vec![
            track("qluster", "echtzeit", "stein auf stein", "mp3", 181.0),
            track("qluster", "echtzeit", "verweile doch", "mp3", 315.0),
            track("qluster", "echtzeit", "verweile doch", "mp3", 315.0),
            track("phew", "phew", "closed", "flac", 200.0),
        ];
        let s = Summary::of(
            "/Volumes/X".into(),
            &device,
            &library,
            |f| f != "FLAC" && f != "flac",
            None,
            None,
        );
        assert_eq!(s.tracks, 3);
        assert_eq!(s.albums, 2);
        assert_eq!(s.artists, 2);
        assert_eq!(s.seconds, 895.0);
        assert_eq!(s.formats, vec![("mp3".into(), 2), ("aac".into(), 1)]);
        assert_eq!(s.missing, 1);
        assert_eq!(s.missing_bytes, 4_000_000);
        assert_eq!(s.unplayable, 1);
        assert_eq!(s.unplayable_formats, vec!["flac".to_string()]);
    }

    #[test]
    fn a_length_is_said_in_two_units() {
        assert_eq!(spoken_length(42.0 * 60.0), "42 min");
        assert_eq!(spoken_length(5.0 * 3600.0 + 12.0 * 60.0), "5 std 12 min");
        assert_eq!(spoken_length(86400.0), "1 tag");
        assert_eq!(spoken_length(3.0 * 86400.0 + 4.0 * 3600.0 + 59.0), "3 tage 4 std");
    }

    #[test]
    fn the_bar_adds_up() {
        let [music, other, free] = shares(100, 60, 30);
        assert_eq!((music, other, free), (0.3, 0.1, 0.6));
        let [music, other, _] = shares(100, 60, 70);
        assert_eq!((music, other), (0.4, 0.0));
        assert_eq!(shares(0, 0, 0), [0.0, 0.0, 1.0]);
    }
}
