use super::*;
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::gui) enum FieldPicture {
    Scope,
    Snake,
    Zoom,
    Life,
    Ball,
    Book,
    Saga,
    Meter,
}

impl FieldPicture {
    pub(in crate::gui) fn name(self) -> &'static str {
        match self {
            Self::Scope => "vektorskop",
            Self::Snake => "schlange",
            Self::Zoom => "mandelbrot",
            Self::Life => "leben",
            Self::Ball => "kugel",
            Self::Book => "zarathustra",
            Self::Saga => "kessler saga",
            Self::Meter => "zeiger",
        }
    }

    pub(in crate::gui) fn from_name(name: &str) -> Self {
        match name.trim() {
            "schlange" => Self::Snake,
            "mandelbrot" => Self::Zoom,
            "leben" | "cover" | "lochstreifen" => Self::Life,
            "kugel" => Self::Ball,
            "zarathustra" => Self::Book,
            "kessler saga" => Self::Saga,
            "zeiger" => Self::Meter,
            _ => Self::Scope,
        }
    }

    pub(in crate::gui) fn said(self) -> &'static str {
        match self {
            Self::Scope => tr!("vektorskop"),
            Self::Snake => tr!("schlange"),
            Self::Zoom => tr!("mandelbrot"),
            Self::Life => tr!("leben"),
            Self::Ball => tr!("kugel"),
            Self::Book => tr!("zarathustra"),
            Self::Saga => tr!("kessler saga"),
            Self::Meter => tr!("zeiger"),
        }
    }

    pub(in crate::gui) fn next(self) -> Option<Self> {
        match self {
            Self::Scope => Some(Self::Snake),
            Self::Snake => Some(Self::Zoom),
            Self::Zoom => Some(Self::Life),
            Self::Life => Some(Self::Ball),
            Self::Ball => Some(Self::Book),
            Self::Book => Some(Self::Saga),
            Self::Saga => Some(Self::Meter),
            Self::Meter => None,
        }
    }
}

const TICK: f64 = 0.25;

pub(in crate::gui) const FIELD_RATES: [u32; 4] = [1, 2, 4, 8];

pub(in crate::gui) fn field_rate(asked: u32) -> u32 {
    match asked {
        0..=1 => 1,
        2..=3 => 2,
        4..=5 => 4,
        _ => 8,
    }
}

pub(in crate::gui) fn field_tick(app: &GuiApp) -> f64 {
    if app.config.ui.scope_rate == super::SECOND_RATE {
        1.0
    } else {
        1.0 / f64::from(field_rate(app.config.ui.field_rate))
    }
}

pub(in crate::gui) fn on_the_second(app: &GuiApp) -> bool {
    let quiet = !matches!(FieldPicture::from_name(&app.config.ui.field_picture), FieldPicture::Scope | FieldPicture::Meter);
    app.config.ui.field_spectrum && quiet && field_rate(app.config.ui.field_rate) <= 1
}

pub(in crate::gui) fn asks_for_frames(app: &GuiApp) -> bool {
    app.player.current_track().is_none() || (field_tick(app) - TICK).abs() > 1e-9
}

fn tick_for(app: &GuiApp) -> f64 {
    field_tick(app)
}

#[derive(Default)]
pub(in crate::gui) struct FieldArt {
    snake: Option<Snake>,
    cover: Option<Cover>,
    zoom: Option<Zoom>,
    life: Option<Life>,
    ball: Option<Ball>,
    saga: Option<(i64, (usize, usize), Vec<u8>)>,
    saga_heard: SagaHeard,
    rows: usize,
    cell: usize,
    wall: bool,
}

impl FieldArt {
    pub(in crate::gui) fn with_rows(rows: usize) -> Self {
        Self { rows, ..Self::default() }
    }

    pub(in crate::gui) fn wall(self) -> Self {
        Self { wall: true, ..self }
    }

    pub(in crate::gui) fn rows(&self) -> usize {
        if self.rows == 0 { GRID_ROWS } else { self.rows }
    }

    pub(in crate::gui) fn cell(&self) -> Option<usize> {
        (self.cell > 0).then_some(self.cell)
    }

    pub(in crate::gui) fn steer(&mut self, way: (i32, i32)) {
        if let Some(snake) = self.snake.as_mut() {
            snake.steered = Some(way);
        }
    }

    pub(in crate::gui) fn let_go(&mut self) {
        if let Some(snake) = self.snake.as_mut() {
            snake.steered = None;
        }
    }
}

pub(in crate::gui) fn playing_key(app: &GuiApp) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    match (app.player.current_track(), app.on_air.as_ref()) {
        (Some(track), _) => track.id.hash(&mut hasher),
        (None, Some(station)) => station.url.hash(&mut hasher),
        (None, None) => 0u8.hash(&mut hasher),
    }
    hasher.finish()
}

fn loudness(heard: &crate::player::audio::Heard) -> f32 {
    let pairs = heard.recent_pairs();
    if pairs.is_empty() {
        return 0.0;
    }
    let sum: f32 = pairs.iter().map(|[l, r]| ((l + r) / 2.0).powi(2)).sum();
    (sum / pairs.len() as f32).sqrt()
}

pub(in crate::gui) struct Moment {
    pub key: u64,
    pub now: f64,
    pub tick: f64,
    pub level: f32,
    pub progress: Option<f64>,
    pub cover: Option<std::path::PathBuf>,
    pub negative: bool,
}

#[derive(Default)]
pub(in crate::gui) struct Drawn {
    pub grey: Option<Vec<u8>>,
    pub save: bool,
    pub opening: bool,
}

pub(in crate::gui) fn grey_for(
    app: &mut GuiApp,
    ui: &Ui,
    picture: FieldPicture,
    (width, height): (usize, usize),
    negative: bool,
) -> Option<Vec<u8>> {
    let key = playing_key(app);
    let duration = app.player.duration().as_secs_f64();
    let on_air = app.player.current_track().is_none();
    let seeding = picture == FieldPicture::Life && !app.field_art.life.as_ref().is_some_and(|life| life.fits(key, width, height));
    let moment = Moment {
        key,
        now: ui.input(|i| i.time),
        tick: tick_for(app),
        level: loudness(&app.heard),
        progress: (!on_air && duration > 0.0).then(|| app.player.position().as_secs_f64() / duration),
        cover: if seeding { app.now_playing_info().and_then(|now| now.artwork) } else { None },
        negative,
    };
    let drawn = grey_of(&mut app.field_art, &mut app.reading, &mut app.config.ui.book_at, picture, (width, height), &moment);
    if drawn.save {
        app.save_config();
    }
    if drawn.opening {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
    }
    drawn.grey
}

pub(in crate::gui) fn grey_of(
    art: &mut FieldArt,
    reading: &mut Reading,
    book_at: &mut usize,
    picture: FieldPicture,
    (width, height): (usize, usize),
    moment: &Moment,
) -> Drawn {
    let (key, now, tick, level) = (moment.key, moment.now, moment.tick, moment.level);
    let tick_now = (now / tick).floor() as i64;
    let rows = art.rows();
    art.cell = (height / rows.max(1)).max(1);
    let grey = match picture {
        FieldPicture::Scope | FieldPicture::Meter => None,
        FieldPicture::Snake => {
            let snake = match &mut art.snake {
                Some(snake) if snake.fits(key, width, height) => snake,
                slot => slot.insert(Snake::new(key, width, height, rows * SNAKE_ROWS / GRID_ROWS)),
            };
            snake.walk_to(tick_now, level);
            Some(snake.grey())
        }
        FieldPicture::Zoom => {
            let ticked = |seconds: f64| (seconds / tick).floor() * tick;
            let depth = match moment.progress {
                Some(progress) => progress.clamp(0.0, 1.0),
                None => (ticked(now) % ZOOM_AIRED_SECS) / ZOOM_AIRED_SECS,
            };
            let zoom = art.zoom.get_or_insert_with(Zoom::default);
            Some(zoom.grey(width, height, depth, rows))
        }
        FieldPicture::Life => {
            let fits = art.life.as_ref().is_some_and(|life| life.fits(key, width, height));
            if !fits {
                let cover = art.cover.get_or_insert_with(Cover::default);
                cover.load(moment.cover.as_deref());
                let seed = cover.image.clone();
                art.life = Some(Life::new(key, width, height, rows, seed.as_ref(), moment.negative));
            }
            art.life.as_mut().map(|life| {
                life.live_to(tick_now, level);
                life.grey()
            })
        }
        FieldPicture::Ball => {
            let ball = art.ball.get_or_insert_with(|| Ball::new(key));
            ball.fit(width, height, rows);
            ball.roll_to(tick_now);
            Some(ball.grey(moment.negative))
        }
        FieldPicture::Book => {
            let grid = Grid::page(width, height, rows * BOOK_ROWS / GRID_ROWS);
            let setting = if art.wall { Setting::Wall } else { Setting::Justified };
            let (at, save) = read_on(reading, book_at, tick_now, tick, grid.line(), setting);
            return Drawn { grey: Some(book_page(at, reading.skip, &grid, setting)), save, opening: false };
        }
        FieldPicture::Saga => {
            art.saga_heard.note(tick_now, level);
            if let Some(grey) = saga_opening(reading, now, (width, height), rows) {
                return Drawn { grey: Some(grey), save: false, opening: true };
            }
            if let Some((was, size, grey)) = &art.saga {
                if *was == tick_now && *size == (width, height) {
                    return Drawn { grey: Some(grey.clone()), ..Drawn::default() };
                }
            }
            let grid = Grid::whole(width, height, rows * SAGA_ROWS / GRID_ROWS);
            let began = *reading.entered.get_or_insert(tick_now);
            let entry = Entry { began, most: (ENTRY_SECS / tick).floor().max(1.0) as i64 };
            let grey = saga(&grid, tick_now, &art.saga_heard, Some(entry));
            art.saga = Some((tick_now, (width, height), grey.clone()));
            Some(grey)
        }
    };
    Drawn { grey, ..Drawn::default() }
}

static BOOK: &str = include_str!("zarathustra.de.txt");

const BOOK_ROWS: usize = 24;

const LINE_ROWS: usize = 8;

const BOOK_SAVE_EVERY: u32 = 480;

#[derive(Default)]
pub(in crate::gui) struct Reading {
    skip: usize,
    tick: Option<i64>,
    unsaved: u32,
    lines: u64,
    opening: Option<f64>,
    opened: bool,
    entered: Option<i64>,
    flows: Vec<Flow>,
}

struct Flow {
    columns: usize,
    setting: Setting,
    at: usize,
    lines: u64,
    tick: i64,
}

const FLOW_KEPT_TICKS: i64 = 8;

fn book_place(at: usize) -> usize {
    if at < BOOK.len() && BOOK.is_char_boundary(at) { at } else { 0 }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Setting {
    Justified,
    Wall,
}

fn letters(text: &str) -> Vec<Vec<u8>> {
    text.chars()
        .map(|c| {
            let mut buffer = [0u8; 4];
            crate::misaki::columns(c.encode_utf8(&mut buffer))
        })
        .collect()
}

fn wide(word: &[Vec<u8>]) -> usize {
    word.iter().map(Vec::len).sum()
}

fn line_from(at: usize, columns: usize, setting: Setting) -> (Vec<u8>, usize) {
    let mut i = book_place(at);
    let mut words: Vec<Vec<Vec<u8>>> = Vec::new();
    let mut used = 0;
    let skip_spaces = |i: &mut usize| {
        while BOOK[*i..].starts_with(' ') {
            *i += 1;
        }
    };
    skip_spaces(&mut i);
    if setting == Setting::Wall {
        let mut out: Vec<u8> = Vec::with_capacity(columns);
        for (at, c) in BOOK[i..].char_indices() {
            let mut buffer = [0u8; 4];
            let letter = crate::misaki::columns(c.encode_utf8(&mut buffer));
            if !out.is_empty() && out.len() + letter.len() > columns {
                return (out, i + at);
            }
            out.extend(letter);
        }
        return (out, 0);
    }
    let next = loop {
        if i >= BOOK.len() {
            break 0;
        }
        let rest = &BOOK[i..];
        let end = rest.find(' ').unwrap_or(rest.len());
        let word = &rest[..end];
        let set = letters(word);
        let space = if words.is_empty() { 0 } else { WORD_SPACE };
        if used + space + wide(&set) <= columns {
            used += space + wide(&set);
            words.push(set);
            i += end;
            skip_spaces(&mut i);
            continue;
        }
        let room = columns.saturating_sub(used + space);
        let front = hyphen_points(word).into_iter().rev().find_map(|at| {
            let mut set = letters(&word[..at]);
            set.extend(letters("-"));
            (wide(&set) <= room).then_some((at, set))
        });
        if let Some((at, set)) = front {
            words.push(set);
            break i + at;
        }
        if !words.is_empty() {
            break i;
        }
        let mut kept = Vec::new();
        let mut taken = 0;
        for (letter, c) in set.into_iter().zip(word.chars()) {
            if taken > 0 && wide(&kept) + letter.len() > columns {
                break;
            }
            kept.push(letter);
            taken += c.len_utf8();
        }
        words.push(kept);
        break i + taken;
    };
    (set_across(&words, columns), next)
}

const WORD_SPACE: usize = 4;

fn set_across(words: &[Vec<Vec<u8>>], columns: usize) -> Vec<u8> {
    let inked: usize = words.iter().map(|word| wide(word)).sum();
    let gaps = words.len().saturating_sub(1);
    let spare = columns.saturating_sub(inked + gaps * WORD_SPACE);
    let mut out = Vec::with_capacity(columns);
    for (n, word) in words.iter().enumerate() {
        if n > 0 {
            let wider = usize::from(n - 1 < spare % gaps);
            out.extend(std::iter::repeat_n(0, WORD_SPACE + spare / gaps + wider));
        }
        out.extend(word.iter().flatten());
    }
    out
}

fn hyphen_points(word: &str) -> Vec<usize> {
    let letters: Vec<(usize, char)> = word.char_indices().collect();
    let first = letters.iter().position(|(_, c)| c.is_alphabetic());
    let last = letters.iter().rposition(|(_, c)| c.is_alphabetic());
    let (Some(first), Some(last)) = (first, last) else { return Vec::new() };
    let core = &letters[first..=last];
    if core.iter().any(|(_, c)| !c.is_alphabetic()) {
        return Vec::new();
    }
    let is_vowel = |c: char| "aeiouyäöüAEIOUYÄÖÜ".contains(c);
    let lower: Vec<char> = core.iter().map(|(_, c)| c.to_lowercase().next().unwrap_or(*c)).collect();
    let mut units: Vec<(usize, bool)> = Vec::new();
    let mut n = 0;
    while n < lower.len() {
        let vowel = is_vowel(lower[n]);
        let mut len = 1;
        if vowel {
            while n + len < lower.len() && is_vowel(lower[n + len]) {
                len += 1;
            }
        } else {
            let rest: String = lower[n..].iter().take(3).collect();
            if rest.starts_with("sch") {
                len = 3;
            } else if ["ch", "ck", "ph", "th", "qu"].iter().any(|pair| rest.starts_with(pair)) {
                len = 2;
            }
        }
        units.push((n, vowel));
        n += len;
    }
    let mut points = Vec::new();
    for u in 1..units.len() {
        let (mut start, vowel) = units[u];
        let before_vowel = units.get(u + 1).is_some_and(|(_, v)| *v);
        if vowel || !before_vowel || !units[..u].iter().any(|(_, v)| *v) {
            continue;
        }
        let single = |u: usize| units.get(u + 1).map_or(lower.len(), |(n, _)| *n) - units[u].0 == 1;
        if u >= 2
            && single(u)
            && matches!(lower[start], 'r' | 'l')
            && single(u - 1)
            && "bdgkptf".contains(lower[units[u - 1].0])
            && !units[u - 2].1
        {
            start = units[u - 1].0;
        }
        if start >= 2 && lower.len() - start >= 2 && points.last() != Some(&core[start].0) {
            points.push(core[start].0);
        }
    }
    points
}

fn read_on(reading: &mut Reading, book_at: &mut usize, tick_now: i64, tick: f64, columns: usize, setting: Setting) -> (usize, bool) {
    let was = reading.tick.replace(tick_now);
    let first = was.is_some_and(|was| tick_now > was);
    if let Some(was) = was.filter(|_| first) {
        let per_tick = (tick / TICK).round().max(1.0) as usize;
        let mut skip = reading.skip + (tick_now - was).min(4) as usize * per_tick;
        while skip >= LINE_ROWS {
            skip -= LINE_ROWS;
            reading.lines += 1;
        }
        reading.skip = skip;
    }
    let lines = reading.lines;
    let begins = book_place(*book_at);
    let flows = &mut reading.flows;
    let n = match flows.iter().position(|flow| flow.columns == columns && flow.setting == setting) {
        Some(n) => n,
        None => {
            flows.push(Flow { columns, setting, at: begins, lines, tick: tick_now });
            flows.len() - 1
        }
    };
    let flow = &mut flows[n];
    if tick_now - flow.tick > FLOW_KEPT_TICKS || tick_now < flow.tick {
        flow.at = begins;
        flow.lines = lines;
    }
    flow.tick = tick_now;
    while flow.lines < lines {
        flow.lines += 1;
        flow.at = line_from(flow.at, columns, setting).1;
    }
    let at = flow.at;
    let mut save = false;
    if first {
        *book_at = at;
        reading.unsaved += 1;
        if reading.unsaved >= BOOK_SAVE_EVERY {
            reading.unsaved = 0;
            save = true;
        }
    }
    (at, save)
}

fn book_page(at: usize, skip: usize, grid: &Grid, setting: Setting) -> Vec<u8> {
    let mut lines = Vec::new();
    let mut next = at;
    while lines.len() * LINE_ROWS < grid.rows + skip {
        let (line, after) = line_from(next, grid.line(), setting);
        lines.push(line);
        next = after;
    }
    grid.paint(|column, row| {
        let down = row + skip;
        let dot = down % LINE_ROWS;
        lines.get(down / LINE_ROWS).and_then(|line| line.get(column)).is_some_and(|byte| byte & (1 << dot) != 0)
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Grid {
    cell: usize,
    columns: usize,
    rows: usize,
    left: isize,
    top: isize,
    size: (usize, usize),
}

pub(super) const GRID_ROWS: usize = 16;

impl Grid {
    fn of(width: usize, height: usize, rows: usize) -> Self {
        let cell = (height / rows.max(4)).max(1);
        let columns = width.div_ceil(cell).max(4);
        let rows = height.div_ceil(cell).max(4);
        Self {
            cell,
            columns,
            rows,
            left: (width as isize - (columns * cell) as isize) / 2,
            top: (height as isize - (rows * cell) as isize) / 2,
            size: (width, height),
        }
    }

    fn whole(width: usize, height: usize, rows: usize) -> Self {
        let cell = (height / rows.max(4)).max(1);
        let columns = (width / cell).max(4);
        let rows = (height / cell).max(4);
        Self {
            cell,
            columns,
            rows,
            left: (width.saturating_sub(columns * cell) / 2) as isize,
            top: (height.saturating_sub(rows * cell) / 2) as isize,
            size: (width, height),
        }
    }

    fn page(width: usize, height: usize, rows: usize) -> Self {
        let whole = Self::whole(width, height, rows);
        let columns = whole.columns.saturating_sub(2).max(4);
        Self {
            columns,
            rows: height.div_ceil(whole.cell).max(4),
            left: (width as isize - (columns * whole.cell) as isize) / 2,
            top: 0,
            ..whole
        }
    }

    fn line(&self) -> usize {
        self.columns + 1
    }

    fn paint(&self, lit: impl Fn(usize, usize) -> bool) -> Vec<u8> {
        let (width, height) = self.size;
        let mut grey = vec![0u8; width * height];
        for row in 0..self.rows {
            for column in 0..self.columns {
                if !lit(column, row) {
                    continue;
                }
                let x0 = self.left + (column * self.cell) as isize;
                let y0 = self.top + (row * self.cell) as isize;
                let across = x0.max(0) as usize..((x0 + self.cell as isize).max(0) as usize).min(width);
                for y in y0.max(0) as usize..((y0 + self.cell as isize).max(0) as usize).min(height) {
                    grey[y * width + across.start..y * width + across.end.max(across.start)].fill(255);
                }
            }
        }
        grey
    }
}

const BAYER: [[f32; 4]; 4] = [[0.0, 8.0, 2.0, 10.0], [12.0, 4.0, 14.0, 6.0], [3.0, 11.0, 1.0, 9.0], [15.0, 7.0, 13.0, 5.0]];

fn bayer(column: usize, row: usize) -> f32 {
    (BAYER[row % 4][column % 4] + 0.5) / 16.0
}

#[derive(Default)]
struct Cover {
    path: Option<std::path::PathBuf>,
    image: Option<image::GrayImage>,
}

impl Cover {
    fn load(&mut self, path: Option<&std::path::Path>) {
        if self.path.as_deref() == path {
            return;
        }
        self.path = path.map(|p| p.to_path_buf());
        self.image = path
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|bytes| image::load_from_memory(&bytes).ok())
            .map(|picture| picture.to_luma8());
    }
}

struct Snake {
    key: u64,
    size: (usize, usize),
    cell: usize,
    columns: i32,
    rows: i32,
    body: VecDeque<(i32, i32)>,
    heading: (i32, i32),
    food: (i32, i32),
    grow: usize,
    stepped: Option<i64>,
    steps: u64,
    calm: f32,
    seed: u64,
    steered: Option<(i32, i32)>,
    asked: usize,
}

const SNAKE_ROWS: usize = 6;

const SNAKE_FIRST: usize = 3;
const SNAKE_MOST_SHARE: f32 = 0.4;

const SNAKE_GROWS_EVERY: u64 = 16;

impl Snake {
    fn new(key: u64, width: usize, height: usize, rows: usize) -> Self {
        let cell = (height / rows.max(2)).max(1);
        let columns = (width / cell).max(4) as i32;
        let rows_asked = rows;
        let rows = (height / cell).max(2) as i32;
        let middle = (columns / 2, rows / 2);
        let body = (0..SNAKE_FIRST as i32).map(|i| (middle.0 - i, middle.1)).collect();
        let mut snake = Self {
            key,
            size: (width, height),
            cell,
            columns,
            rows,
            asked: rows_asked,
            body,
            heading: (1, 0),
            food: (0, 0),
            grow: 0,
            stepped: None,
            steps: 0,
            calm: 0.0,
            seed: key | 1,
            steered: None,
        };
        snake.place_food();
        snake
    }

    fn fits(&self, key: u64, width: usize, height: usize) -> bool {
        self.key == key && self.size == (width, height)
    }

    fn random(&mut self) -> u64 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        self.seed
    }

    fn place_food(&mut self) {
        let cells = (self.columns * self.rows) as u64;
        for _ in 0..64 {
            let at = (self.random() % cells) as i32;
            let food = (at % self.columns, at / self.columns);
            if !self.body.contains(&food) {
                self.food = food;
                return;
            }
        }
    }

    fn free(&self, at: (i32, i32)) -> bool {
        let inside = (0..self.columns).contains(&at.0) && (0..self.rows).contains(&at.1);
        let tail = if self.grow == 0 { self.body.back() } else { None };
        inside && (!self.body.contains(&at) || tail == Some(&at))
    }

    fn walk_to(&mut self, tick: i64, level: f32) {
        let behind = match self.stepped {
            Some(was) if tick > was => (tick - was).min(4),
            Some(_) => 0,
            None => 1,
        };
        self.stepped = Some(tick);
        for _ in 0..behind {
            self.step(level);
        }
    }

    fn step(&mut self, level: f32) {
        let loud = level > 0.02 && level > self.calm * 1.35;
        self.calm = self.calm * 0.85 + level * 0.15;
        let head = self.body[0];
        let (dx, dy) = self.heading;
        if let Some(wanted) = self.steered {
            let way = if wanted == (-dx, -dy) { (dx, dy) } else { wanted };
            if !self.free((head.0 + way.0, head.1 + way.1)) {
                let steered = self.steered;
                *self = Snake::new(self.key, self.size.0, self.size.1, self.asked);
                self.steered = steered;
                return;
            }
            self.go(way);
            return;
        }
        let ways = [(dx, dy), (-dy, dx), (dy, -dx)];
        let free: Vec<(i32, i32)> = ways.into_iter().filter(|&(x, y)| self.free((head.0 + x, head.1 + y))).collect();
        if free.is_empty() {
            *self = Snake::new(self.key, self.size.0, self.size.1, self.asked);
            return;
        }
        let towards = |&(x, y): &(i32, i32)| (head.0 + x - self.food.0).abs() + (head.1 + y - self.food.1).abs();
        let turns: Vec<(i32, i32)> = free.iter().copied().filter(|&w| w != (dx, dy)).collect();
        let way = if loud && !turns.is_empty() {
            turns[(self.random() % turns.len() as u64) as usize]
        } else {
            *free.iter().min_by_key(|w| (towards(w), **w != (dx, dy))).unwrap_or(&free[0])
        };
        self.go(way);
    }

    fn go(&mut self, way: (i32, i32)) {
        let head = self.body[0];
        self.heading = way;
        let next = (head.0 + way.0, head.1 + way.1);
        self.body.push_front(next);
        self.steps += 1;
        let most = ((self.columns * self.rows) as f32 * SNAKE_MOST_SHARE) as usize;
        if self.grow > 0 && self.body.len() <= most {
            self.grow -= 1;
        } else {
            self.body.pop_back();
        }
        if next == self.food {
            self.grow += 2;
            self.place_food();
        }
        if self.steps.is_multiple_of(SNAKE_GROWS_EVERY) {
            self.grow += 1;
        }
    }

    fn grey(&self) -> Vec<u8> {
        let (width, height) = self.size;
        let mut grey = vec![0u8; width * height];
        let cell = self.cell;
        let left = (width - self.columns as usize * cell) / 2;
        let top = (height - self.rows as usize * cell) / 2;
        let mut fill = |at: (i32, i32), inset: usize| {
            let x0 = left + at.0 as usize * cell + inset;
            let y0 = top + at.1 as usize * cell + inset;
            let side = cell.saturating_sub(2 * inset).max(1);
            for y in y0..(y0 + side).min(height) {
                for x in x0..(x0 + side).min(width) {
                    grey[y * width + x] = 255;
                }
            }
        };
        let gap = usize::from(cell >= 4);
        for &at in &self.body {
            fill(at, gap);
        }
        fill(self.food, (cell / 3).max(gap));
        grey
    }
}

const ZOOM_AT: (f64, f64) = (-0.101_096_363_845_62, 0.956_286_510_809_14);
const ZOOM_WIDEST: f64 = 3.5;
const ZOOM_DEEPEST: f64 = 2e-11;

const ZOOM_AIRED_SECS: f64 = 600.0;

#[derive(Default)]
struct Zoom {
    made: Option<(u64, usize, usize, usize, Vec<u8>)>,
}

impl Zoom {
    fn grey(&mut self, width: usize, height: usize, depth: f64, rows: usize) -> Vec<u8> {
        let stamp = depth.to_bits();
        if let Some((was, w, h, r, grey)) = &self.made {
            if *was == stamp && *w == width && *h == height && *r == rows {
                return grey.clone();
            }
        }
        let grey = mandelbrot(width, height, depth, rows);
        self.made = Some((stamp, width, height, rows, grey.clone()));
        grey
    }
}

fn escapes(grid: &Grid, depth: f64) -> (Vec<u32>, u32) {
    let span = ZOOM_WIDEST * (ZOOM_DEEPEST / ZOOM_WIDEST).powf(depth);
    let most = (100.0 + 40.0 * (ZOOM_WIDEST / span).log10()).clamp(100.0, 600.0) as u32;
    let step = span / grid.columns as f64;
    let mut rounds = vec![most; grid.columns * grid.rows];
    for row in 0..grid.rows {
        let ci = ZOOM_AT.1 + (row as f64 + 0.5 - grid.rows as f64 / 2.0) * step;
        for column in 0..grid.columns {
            let cr = ZOOM_AT.0 + (column as f64 + 0.5 - grid.columns as f64 / 2.0) * step;
            let (mut x, mut y, mut n) = (0.0f64, 0.0f64, 0u32);
            while n < most && x * x + y * y <= 4.0 {
                let next = x * x - y * y + cr;
                y = 2.0 * x * y + ci;
                x = next;
                n += 1;
            }
            rounds[row * grid.columns + column] = n;
        }
    }
    (rounds, most)
}

fn mandelbrot(width: usize, height: usize, depth: f64, rows: usize) -> Vec<u8> {
    let grid = Grid::of(width, height, rows * 2);
    let (rounds, most) = escapes(&grid, depth);
    let mut sorted: Vec<u32> = rounds.iter().copied().filter(|&n| n < most).collect();
    sorted.sort_unstable();
    let tone: Vec<f32> = rounds
        .iter()
        .map(|&n| {
            if n >= most {
                return 1.0;
            }
            let place = sorted.partition_point(|&m| m < n) as f32 / sorted.len().max(1) as f32;
            let wave = 0.5 - 0.5 * (place * ZOOM_RINGS * std::f32::consts::PI).cos();
            let steep = wave * wave * (3.0 - 2.0 * wave);
            steep * (0.45 + 0.55 * place)
        })
        .collect();
    grid.paint(|column, row| tone[row * grid.columns + column] > crate::gui::dither::blue_share(column, row))
}

const ZOOM_RINGS: f32 = 5.0;

struct Life {
    key: u64,
    grid: Grid,
    columns: usize,
    rows: usize,
    cells: Vec<bool>,
    before: Vec<bool>,
    lived: Option<i64>,
    calm: f32,
    seed: u64,
}

impl Life {
    fn new(key: u64, width: usize, height: usize, rows: usize, cover: Option<&image::GrayImage>, negative: bool) -> Self {
        let grid = Grid::of(width, height, rows);
        let (columns, rows) = (grid.columns, grid.rows);
        let mut life = Self {
            key,
            grid,
            columns,
            rows,
            cells: vec![false; columns * rows],
            before: Vec::new(),
            lived: None,
            calm: 0.0,
            seed: key | 1,
        };
        match cover {
            Some(cover) => life.seed_with(cover, negative),
            None => life.scatter(),
        }
        life
    }

    fn fits(&self, key: u64, width: usize, height: usize) -> bool {
        self.key == key && self.grid.size == (width, height)
    }

    fn random(&mut self) -> u64 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        self.seed
    }

    fn seed_with(&mut self, cover: &image::GrayImage, negative: bool) {
        let (cw, ch) = (cover.width() as usize, cover.height() as usize);
        if cw == 0 || ch == 0 {
            return self.scatter();
        }
        let scale = cw as f32 / self.columns as f32;
        let top = (ch as f32 - self.rows as f32 * scale).max(0.0) / 2.0;
        for row in 0..self.rows {
            for column in 0..self.columns {
                let x = ((column as f32 + 0.5) * scale) as u32;
                let y = (top + (row as f32 + 0.5) * scale) as u32;
                let luma = f32::from(cover.get_pixel(x.min(cw as u32 - 1), y.min(ch as u32 - 1))[0]) / 255.0;
                let ink = if negative { 1.0 - luma } else { luma };
                self.cells[row * self.columns + column] = ink > bayer(column, row);
            }
        }
    }

    fn scatter(&mut self) {
        for at in 0..self.cells.len() {
            self.cells[at] = self.random().is_multiple_of(5);
        }
    }

    fn glider(&mut self) {
        const SHAPE: [(usize, usize); 5] = [(1, 0), (2, 1), (0, 2), (1, 2), (2, 2)];
        let x0 = (self.random() % self.columns as u64) as usize;
        let y0 = (self.random() % self.rows as u64) as usize;
        let flip = self.random();
        for (dx, dy) in SHAPE {
            let dx = if flip & 1 == 0 { dx } else { 2 - dx };
            let dy = if flip & 2 == 0 { dy } else { 2 - dy };
            let at = ((y0 + dy) % self.rows) * self.columns + (x0 + dx) % self.columns;
            self.cells[at] = true;
        }
    }

    fn live_to(&mut self, tick: i64, level: f32) {
        let behind = match self.lived {
            Some(was) if tick > was => (tick - was).min(4),
            Some(_) => 0,
            None => 0,
        };
        self.lived = Some(tick);
        for _ in 0..behind {
            self.generation(level);
        }
    }

    fn generation(&mut self, level: f32) {
        let (columns, rows) = (self.columns, self.rows);
        let mut next = vec![false; self.cells.len()];
        for row in 0..rows {
            for column in 0..columns {
                let mut around = 0;
                for dy in [rows - 1, 0, 1] {
                    for dx in [columns - 1, 0, 1] {
                        if (dx, dy) != (0, 0) && self.cells[((row + dy) % rows) * columns + (column + dx) % columns] {
                            around += 1;
                        }
                    }
                }
                let alive = self.cells[row * columns + column];
                next[row * columns + column] = around == 3 || (alive && around == 2);
            }
        }
        let still = next == self.before || !next.iter().any(|&c| c);
        self.before = std::mem::replace(&mut self.cells, next);
        let loud = level > 0.02 && level > self.calm * 1.35;
        self.calm = self.calm * 0.85 + level * 0.15;
        if still {
            for _ in 0..6 {
                self.glider();
            }
        } else if loud {
            self.glider();
        }
    }

    fn grey(&self) -> Vec<u8> {
        self.grid.paint(|column, row| self.cells[row * self.columns + column])
    }
}

struct Ball {
    grid: Option<Grid>,
    at: (i32, i32),
    way: (i32, i32),
    screen: usize,
    rolled: Option<i64>,
}

const BALL_CELLS: i32 = 8;

const BALL_SCREENS: usize = 7;

impl Ball {
    fn new(key: u64) -> Self {
        let way = (if key & 1 == 0 { 1 } else { -1 }, if key & 2 == 0 { 1 } else { -1 });
        Self { grid: None, at: ((key >> 8 & 0xff) as i32, (key >> 16 & 0xf) as i32), way, screen: (key >> 24) as usize % BALL_SCREENS, rolled: None }
    }

    fn side(&self) -> i32 {
        self.grid.map_or(BALL_CELLS, |grid| BALL_CELLS.min(grid.rows as i32 - 1).min(grid.columns as i32 - 1))
    }

    fn fit(&mut self, width: usize, height: usize, rows: usize) {
        if self.grid.is_some_and(|grid| grid.size == (width, height)) {
            return;
        }
        let grid = Grid::of(width, height, rows);
        self.grid = Some(grid);
        let side = self.side();
        self.at.0 = self.at.0.clamp(0, grid.columns as i32 - side);
        self.at.1 = self.at.1.clamp(0, grid.rows as i32 - side);
    }

    fn roll_to(&mut self, tick: i64) {
        let behind = match self.rolled {
            Some(was) if tick > was => (tick - was).min(4),
            _ => 0,
        };
        self.rolled = Some(tick);
        for _ in 0..behind {
            self.roll();
        }
    }

    fn roll(&mut self) {
        let Some(grid) = self.grid else { return };
        let side = self.side();
        let room = (grid.columns as i32 - side, grid.rows as i32 - side);
        let mut bounced = false;
        if !(0..=room.0).contains(&(self.at.0 + self.way.0)) {
            self.way.0 = -self.way.0;
            bounced = true;
        }
        if !(0..=room.1).contains(&(self.at.1 + self.way.1)) {
            self.way.1 = -self.way.1;
            bounced = true;
        }
        self.at = ((self.at.0 + self.way.0).clamp(0, room.0.max(0)), (self.at.1 + self.way.1).clamp(0, room.1.max(0)));
        if bounced {
            self.screen = (self.screen + 1) % BALL_SCREENS;
        }
    }

    fn light(side: i32, x: i32, y: i32) -> Option<f32> {
        let across = |v: i32| (v as f32 + 0.5) / side as f32 * 2.0 - 1.0;
        let (dx, dy) = (across(x), across(y));
        let out = dx * dx + dy * dy;
        if out > 1.0 {
            return None;
        }
        let dz = (1.0 - out).sqrt();
        let (lx, ly, lz) = (-0.45f32, -0.55f32, 0.70f32);
        let length = (lx * lx + ly * ly + lz * lz).sqrt();
        let lit = ((dx * lx + dy * ly + dz * lz) / length).max(0.0);
        Some(lit)
    }

    fn on_rim(side: i32, x: i32, y: i32) -> bool {
        Self::light(side, x, y).is_some()
            && [(-1, 0), (1, 0), (0, -1), (0, 1)].iter().any(|(dx, dy)| Self::light(side, x + dx, y + dy).is_none())
    }

    fn threshold(screen: usize, x: usize, y: usize) -> f32 {
        const CLUSTER: [[f32; 4]; 4] = [[12.0, 5.0, 6.0, 13.0], [4.0, 0.0, 1.0, 7.0], [11.0, 3.0, 2.0, 8.0], [15.0, 10.0, 9.0, 14.0]];
        match screen {
            0 => bayer(x, y),
            1 => ((y % 4) as f32 + 0.5) / 4.0,
            2 => ((x % 4) as f32 + 0.5) / 4.0,
            3 => (((x + y) % 4) as f32 + 0.5) / 4.0,
            4 => (CLUSTER[y % 4][x % 4] + 0.5) / 16.0,
            5 => {
                let mut h = (x as u32).wrapping_mul(0x9e37_79b1) ^ (y as u32).wrapping_mul(0x85eb_ca77);
                h ^= h >> 15;
                h = h.wrapping_mul(0x2c1b_3c6d);
                h ^= h >> 12;
                ((h % 16) as f32 + 0.5) / 16.0
            }
            _ => if (x + y).is_multiple_of(2) { 0.3 } else { 0.7 },
        }
    }

    fn grey(&self, negative: bool) -> Vec<u8> {
        let Some(grid) = self.grid else { return Vec::new() };
        let side = self.side();
        grid.paint(|column, row| {
            let (x, y) = (column as i32 - self.at.0, row as i32 - self.at.1);
            if !(0..side).contains(&x) || !(0..side).contains(&y) {
                return false;
            }
            if Self::on_rim(side, x, y) {
                return true;
            }
            Self::light(side, x, y).is_some_and(|light| {
                let ink = 0.2 + 0.8 * if negative { 1.0 - light } else { light };
                ink > Self::threshold(self.screen, x as usize, y as usize)
            })
        })
    }
}

static SAGA: &str = include_str!("saga.txt");

const SAGA_ROWS: usize = 28;
const FIGURE_WIDE: usize = 15;
const FIGURE_TALL: usize = 26;
const FLOOR: i64 = SAGA_ROWS as i64 - 1;

struct Figure {
    ink: Vec<bool>,
    hole: Vec<bool>,
}

impl Figure {
    fn at(&self, x: i64, y: i64) -> (bool, bool) {
        if !(0..FIGURE_WIDE as i64).contains(&x) || !(0..FIGURE_TALL as i64).contains(&y) {
            return (false, false);
        }
        let at = y as usize * FIGURE_WIDE + x as usize;
        (self.ink[at], self.hole[at])
    }

    fn clears(&self, x: i64, y: i64) -> bool {
        let (ink, hole) = self.at(x, y);
        ink || hole || (-1..=1).any(|dy| (-1..=1).any(|dx| self.at(x + dx, y + dy).0))
    }
}

fn figures() -> &'static [Figure] {
    static FRAMES: std::sync::OnceLock<Vec<Figure>> = std::sync::OnceLock::new();
    FRAMES.get_or_init(|| {
        let rows: Vec<&str> = SAGA
            .lines()
            .filter(|line| !line.is_empty() && line.chars().all(|c| c == '#' || c == '.'))
            .collect();
        rows.chunks(FIGURE_TALL)
            .filter(|frame| frame.len() == FIGURE_TALL)
            .map(|frame| {
                let ink: Vec<bool> = frame
                    .iter()
                    .flat_map(|row| (0..FIGURE_WIDE).map(move |x| row.as_bytes().get(x) == Some(&b'#')))
                    .collect();
                let mut outside = vec![false; ink.len()];
                let mut open: Vec<(usize, usize)> = (0..FIGURE_WIDE)
                    .flat_map(|x| [(x, 0), (x, FIGURE_TALL - 1)])
                    .chain((0..FIGURE_TALL).flat_map(|y| [(0, y), (FIGURE_WIDE - 1, y)]))
                    .collect();
                while let Some((x, y)) = open.pop() {
                    let at = y * FIGURE_WIDE + x;
                    if ink[at] || outside[at] {
                        continue;
                    }
                    outside[at] = true;
                    if x > 0 {
                        open.push((x - 1, y));
                    }
                    if x + 1 < FIGURE_WIDE {
                        open.push((x + 1, y));
                    }
                    if y > 0 {
                        open.push((x, y - 1));
                    }
                    if y + 1 < FIGURE_TALL {
                        open.push((x, y + 1));
                    }
                }
                let hole = ink.iter().zip(&outside).map(|(&ink, &out)| !ink && !out).collect();
                Figure { ink, hole }
            })
            .collect()
    })
}

fn mix(place: i64, salt: u64) -> u64 {
    let mut h = (place as u64).wrapping_add(salt.wrapping_mul(0x9e37_79b9_7f4a_7c15));
    h = (h ^ (h >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    h = (h ^ (h >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    h ^ (h >> 31)
}

#[derive(Default)]
pub(in crate::gui) struct SagaHeard {
    levels: VecDeque<(i64, f32)>,
}

const SAGA_HEARD_KEPT: usize = 4096;

impl SagaHeard {
    fn note(&mut self, tick: i64, level: f32) {
        if self.levels.back().is_some_and(|&(was, _)| was >= tick) {
            return;
        }
        self.levels.push_back((tick, level));
        while self.levels.len() > SAGA_HEARD_KEPT {
            self.levels.pop_front();
        }
    }

    fn at(&self, tick: i64) -> Option<f32> {
        let at = self.levels.partition_point(|&(was, _)| was <= tick);
        at.checked_sub(1).map(|i| self.levels[i].1)
    }
}

const SAGA_STRETCH: i64 = 160;

#[derive(Clone, Copy)]
struct Plane {
    num: i64,
    den: i64,
    salt: u64,
}

const FAR: Plane = Plane { num: 1, den: 4, salt: 1 };
const SKY: Plane = Plane { num: 1, den: 3, salt: 8 };
const HIGH: Plane = Plane { num: 1, den: 6, salt: 12 };
const MIDDLE: Plane = Plane { num: 1, den: 2, salt: 2 };
const LANDMARKS: Plane = Plane { num: 1, den: 2, salt: 9 };
const GROUND: Plane = Plane { num: 1, den: 1, salt: 4 };
const FRONT: Plane = Plane { num: 2, den: 1, salt: 6 };

struct Scene<'a> {
    tick: i64,
    columns: i64,
    heard: &'a SagaHeard,
    since: Option<i64>,
    turn: Turn,
    into: i64,
    span: i64,
}

impl Scene<'_> {
    fn ramp(&self) -> f32 {
        eased(self.into, self.span)
    }

    fn quiet_at(&self, came: i64) -> f32 {
        match turn_at(came, self.since) {
            (Turn::Still, into, span) => eased(into, span),
            _ => 0.0,
        }
    }

    fn zoom(&self) -> f32 {
        if self.turn != Turn::Wide {
            return 1.0;
        }
        let from_edge = (self.into - ZOOM_WAIT).min(self.span - 1 - ZOOM_WAIT - self.into);
        if from_edge < 0 {
            return 1.0;
        }
        (1.0 + 0.5 * (from_edge / ZOOM_STEP) as f32).min(FARTHEST)
    }

    fn offset(&self, plane: Plane) -> i64 {
        (self.tick * plane.num).div_euclid(plane.den)
    }

    fn came_in(&self, plane: Plane, world: i64) -> i64 {
        ((world - self.columns) * plane.den).div_euclid(plane.num)
    }

    fn fullness(&self, plane: Plane, world: i64) -> f32 {
        let came = self.came_in(plane, world);
        if self.since.is_some_and(|since| came < since) {
            return 0.0;
        }
        let stretch = if mix(came.div_euclid(SAGA_STRETCH), 40).is_multiple_of(3) { 0.12 } else { 1.0 };
        let music = self.heard.at(came).map_or(0.7, |level| (0.2 + level * 5.0).clamp(0.2, 1.0));
        let still = 1.0 - 0.96 * self.quiet_at(came);
        stretch * music * still
    }

    fn keeps(&self, plane: Plane, chunk: i64, wide: i64, likely: f32) -> bool {
        let chance = (mix(chunk, plane.salt + 90) % 1000) as f32 / 1000.0;
        chance < likely * self.fullness(plane, chunk * wide)
    }

    fn loud_at(&self, plane: Plane, world: i64) -> f32 {
        let came = self.came_in(plane, world);
        if self.since.is_some_and(|since| came < since) {
            return 0.0;
        }
        self.heard.at(came).unwrap_or(0.0)
    }
}

fn chunk_of(scene: &Scene, plane: Plane, x: i64, wide: i64) -> (i64, i64, u64, i64) {
    let world = x + scene.offset(plane);
    let chunk = world.div_euclid(wide);
    (chunk, world.rem_euclid(wide), mix(chunk, plane.salt), world)
}

fn hazed(scene: &Scene, world: i64, y: i64) -> bool {
    scene.turn == Turn::Still && (mix(world * 64 + y, 76) % 1000) as f32 / 1000.0 < scene.ramp() * 0.97
}

fn far(scene: &Scene, x: i64, y: i64) -> bool {
    const WIDE: i64 = 16;
    let (chunk, x, h, world) = chunk_of(scene, FAR, x, WIDE);
    if !scene.keeps(FAR, chunk, WIDE, 0.8) || hazed(scene, world, y) {
        return false;
    }
    let lit = match h % 6 {
        4 => {
            let steps = 3 + (h >> 8) as i64 % 4;
            let step = (FLOOR - y).div_euclid(3);
            y <= FLOOR && step < steps && (x - 7).abs() <= 7 - step
        }
        5 => {
            let top = 6 + (h >> 8) as i64 % 8;
            let stack = ((3..6).contains(&x) || (10..12).contains(&x)) && y >= top;
            let up = top - y;
            let smoke = (1..=5).contains(&up) && x == 4 + (up + scene.tick.div_euclid(4)).rem_euclid(3) && (up + scene.tick.div_euclid(2)).rem_euclid(2) == 0;
            if smoke {
                return true;
            }
            stack
        }
        0 => {
            let top = 3 + (h >> 8) as i64 % 12;
            if (6..8).contains(&x) {
                y >= top
            } else {
                let row = y - top - 1;
                let capsule = row.div_euclid(3);
                let left = (h >> (16 + capsule.rem_euclid(24))) & 1 == 0;
                let span = if left { 2..6 } else { 8..12 };
                row >= 0 && span.contains(&x) && (h >> 40) as i64 % 5 + 2 > capsule.min(7) - 2 && {
                    let (inside_x, inside_y) = (x - span.start, row.rem_euclid(3));
                    !(inside_y == 1 && inside_x == if left { 1 } else { 2 })
                }
            }
        }
        1 => {
            let reach = 4 + (h >> 8) as i64 % 3;
            let (dx, base) = (x - 7, FLOOR - 2 - (h >> 12) as i64 % 4);
            let dome = (dx * dx + (y - base) * (y - base) * 2) <= reach * reach * 2 && y <= base;
            let drum = dx.abs() <= reach && y > base;
            dome || drum
        }
        2 => {
            let top = 1 + (h >> 8) as i64 % 6;
            if y < top {
                return false;
            }
            let half = 1 + (y - top) / 7;
            let (left, right) = (7 - half, 7 + half);
            x == left || x == right || ((y - top) % 3 == 0 && (left..=right).contains(&x)) || (x - left == (y - top) % (2 * half + 1))
        }
        _ => {
            let top = 6 + (h >> 8) as i64 % 10;
            let wide = 6 + (h >> 12) as i64 % 6;
            (2..2 + wide).contains(&x) && y >= top && !((x - 2) % 2 == 1 && (y - top) % 2 == 1 && y < FLOOR - 1)
        }
    };
    lit && (world + y).rem_euclid(2) == 0
}

fn birds(scene: &Scene, x: i64, y: i64) -> bool {
    const WIDE: i64 = 28;
    let (chunk, x, h, world) = chunk_of(scene, SKY, x, WIDE);
    if scene.loud_at(SKY, world) < 0.12 || !h.is_multiple_of(2) || scene.quiet_at(scene.came_in(SKY, world)) > 0.3 {
        return false;
    }
    let (bx, by) = (6 + (h >> 8) as i64 % 16, 2 + (h >> 16) as i64 % 8);
    let up = (scene.tick + chunk).rem_euclid(2) == 0;
    let wing = if up { by - 1 } else { by };
    (x == bx && y == by) || ((x - bx).abs() == 1 && y == wing) || ((x - bx).abs() == 2 && up && y == by - 2)
}

fn clouds(scene: &Scene, x: i64, y: i64) -> bool {
    const WIDE: i64 = 48;
    let (chunk, x, h, world) = chunk_of(scene, HIGH, x, WIDE);
    if !scene.keeps(HIGH, chunk, WIDE, 0.45) || hazed(scene, world, y) {
        return false;
    }
    let (at, base) = (4 + (h >> 8) as i64 % 12, 3 + (h >> 16) as i64 % 5);
    let wide = 12 + (h >> 24) as i64 % 10;
    let x = x - at;
    let lump = (y == base && (0..wide).contains(&x))
        || (y == base - 1 && (2..wide - 3).contains(&x))
        || (y == base - 2 && (5..wide - 7).contains(&x));
    lump && (world + y).rem_euclid(2) == 0
}

fn middle(scene: &Scene, x: i64, y: i64) -> bool {
    const WIDE: i64 = 22;
    const POLE: i64 = 10;
    const KINDS: u64 = 9;
    let (chunk, x, h, world) = chunk_of(scene, MIDDLE, x, WIDE);
    let mast = |chunk: i64| mix(chunk, MIDDLE.salt) % KINDS == 1 && scene.keeps(MIDDLE, chunk, WIDE, 0.85);
    let wired = {
        let from = (world - POLE).div_euclid(WIDE);
        let t = (world - POLE).rem_euclid(WIDE) as f32 / WIDE as f32;
        y == 8 + (16.0 * t * (1.0 - t)).round() as i64 && mast(from) && mast(from + 1)
    };
    if wired {
        return true;
    }
    if !scene.keeps(MIDDLE, chunk, WIDE, 0.85) {
        return false;
    }
    match h % KINDS {
        0 => {
            let wide = 9 + (h >> 8) as i64 % 9;
            let tall = 8 + (h >> 16) as i64 % 8;
            let top = FLOOR - tall;
            let x = x - 2;
            if !(0..=wide).contains(&x) || y > FLOOR {
                return false;
            }
            if y < top {
                let middle = wide / 2;
                return (x - middle).abs() == y - (top - middle) && y >= top - middle;
            }
            let post = x % 4 == 0 || x == wide;
            let rail = (y - top) % 4 == 0 || y == FLOOR;
            let (bay_x, bay_y) = (x / 4, (y - top) / 4);
            let braced = (h >> (24 + (bay_x * 3 + bay_y) % 32)) & 1 == 1;
            let brace = braced && {
                let (in_x, in_y) = (x % 4, (y - top) % 4);
                if (bay_x + bay_y) % 2 == 0 { in_x == in_y } else { in_x == 3 - in_y }
            };
            post || rail || brace
        }
        1 => (x == POLE && y >= 6) || (y == 7 && (POLE - 2..=POLE + 2).contains(&x)),
        2 => {
            let middle = 4 + (h >> 8) as i64 % 12;
            let reach = 3 + (h >> 16) as i64 % 5;
            let d = (x - middle) as f32 / reach as f32;
            let high = (reach as f32 * 0.8 * (1.0 - d * d)).round() as i64;
            y > FLOOR - high && y <= FLOOR && !mix(world * 64 + y, 3).is_multiple_of(5)
        }
        3 => {
            let step = 2 + (h >> 8) as i64 % 2;
            let x = x - 2;
            if !(0..16).contains(&x) {
                return false;
            }
            let top = FLOOR - 3 - (x / 3) * step;
            y == top || (y > top && ((y - top) % 3 == 0 || (x + (y - top) / 3 * 2) % 4 == 0))
        }
        6 => {
            let top = FLOOR - 13;
            let post = (x == 5 || x == 15) && y > top;
            let upper = y == top && (3..=17).contains(&x);
            let cap = y == top - 1 && (2..=18).contains(&x);
            let lower = y == top + 3 && (5..=15).contains(&x);
            post || upper || cap || lower
        }
        7 => {
            let (cx, cy) = (10, FLOOR - 13);
            let (dx, dy) = (x - cx, y - cy);
            let pole = x == cx && y > cy;
            let upright = (scene.tick.div_euclid(2) + chunk).rem_euclid(2) == 0;
            let sails = if upright {
                (dx == 0 && dy.abs() <= 4) || (dy == 0 && dx.abs() <= 4)
            } else {
                dx.abs() == dy.abs() && dx.abs() <= 3
            };
            pole || sails
        }
        8 => {
            let deck = FLOOR - 10;
            let bay = x.rem_euclid(11);
            let pier = bay < 2 && y > deck;
            let haunch = (bay - 6).abs() - 2;
            y == deck || y == deck + 1 || pier || (y > deck + 1 && y <= deck + 1 + haunch.max(0))
        }
        4 => {
            let (left, right, top) = (3, 15, FLOOR - 13 + (h >> 8) as i64 % 4);
            let broken = top + 3 + (h >> 12) as i64 % 6;
            let pier = |px: i64| (px..px + 2).contains(&x);
            let arch = {
                let (cx, r) = ((left + right + 1) as f32 / 2.0, (right - left - 1) as f32 / 2.0);
                let d = ((x as f32 + 0.5 - cx).powi(2) + ((y - top - 6) as f32).powi(2) * 1.4).sqrt();
                y <= top + 6 && (r - 1.0..=r + 0.6).contains(&d)
            };
            (pier(left) && y >= top + 4) || (pier(right - 1) && y >= broken) || (arch && x < right - 3)
        }
        _ => {
            let x = x - 1;
            if !(0..20).contains(&x) {
                return false;
            }
            let tall = 6 + (mix(world, 11) % 4) as i64;
            (x % 2 == 0 && y >= FLOOR - tall) || y == FLOOR - 4
        }
    }
}

fn landmarks(scene: &Scene, x: i64, y: i64) -> bool {
    const WIDE: i64 = 120;
    let (chunk, x, h, _) = chunk_of(scene, LANDMARKS, x, WIDE);
    if !scene.keeps(LANDMARKS, chunk, WIDE, 0.4) {
        return false;
    }
    let x = x - 40;
    if !(0..40).contains(&x) {
        return false;
    }
    if h % 8 == 7 {
        return if (h >> 20) % 2 == 0 { pediment_front(x, y) } else { ball_on_cube(x, y) };
    }
    match h % 6 {
        4 => {
            let dx = x - 19;
            let bowl = 13 - dx * dx / 12;
            let dish = dx.abs() <= 11 && (y == bowl || y == bowl + 1);
            let mast = (18..=20).contains(&x) && y > 14 && (x != 19 || y % 3 == 0);
            let feed = (x == 19 && (5..13).contains(&y)) || (y == 4 && dx.abs() <= 1);
            let stays = (5..13).contains(&y) && dx.abs() == (y - 5) && y % 2 == 1;
            dish || mast || feed || stays
        }
        5 => {
            let dx = x - 14;
            let nose = (2..7).contains(&y) && dx.abs() <= (y - 2) / 2;
            let body = (7..FLOOR - 3).contains(&y) && dx.abs() <= 2 && (dx.abs() == 2 || y % 5 == 2);
            let window = y == 9 && dx == 0;
            let fins = (FLOOR - 4..=FLOOR).contains(&y) && dx.abs() <= 2 + (y - (FLOOR - 4)) && dx.abs() >= 2;
            let tower = (22..=24).contains(&x) && y >= 4 && (x != 23 || y % 3 == 0);
            let arms = (y == 9 || y == 17) && (17..22).contains(&x);
            nose || body || window || fins || tower || arms
        }
        0 => {
            let shaft = (8..10).contains(&x) || (18..20).contains(&x);
            if shaft {
                return y >= 2;
            }
            let row = y - 3;
            if row < 0 {
                return false;
            }
            let (cx, cy) = (x.div_euclid(5), row.div_euclid(4));
            let (ix, iy) = (x.rem_euclid(5), row.rem_euclid(4));
            let placed = !mix(cx * 64 + cy, h).is_multiple_of(3) && (0..6).contains(&cx);
            placed && iy < 3 && ix < 4 && !(iy == 1 && (1..3).contains(&ix))
        }
        1 => {
            let (tank_top, tank_bottom) = (3, 11);
            let tank = (6..20).contains(&x) && (tank_top..=tank_bottom).contains(&y) && (x == 6 || x == 19 || y == tank_top || y == tank_bottom || (y - tank_top) % 3 == 0);
            let t = (y - tank_bottom) as f32 / (FLOOR - tank_bottom) as f32;
            let legs = y > tank_bottom && y <= FLOOR && {
                let spread = (t * 4.0).round() as i64;
                x == 7 - spread || x == 18 + spread || (y - tank_bottom) % 5 == 0 && (7 - spread..=18 + spread).contains(&x)
            };
            tank || legs
        }
        2 => {
            let tower = (4..6).contains(&x) && y >= 2 && (y % 3 != 0 || x == 4);
            let jib = (y == 2 || (y == 3 && x % 3 == 0)) && (0..36).contains(&x);
            let hook_at = 22 + (h >> 8) as i64 % 12;
            let line_to = 8 + (h >> 16) as i64 % 12;
            let line = x == hook_at && (3..=line_to).contains(&y);
            let hook = (y == line_to + 1 && (hook_at - 1..=hook_at + 1).contains(&x)) || (y == line_to + 2 && x == hook_at + 1);
            tower || jib || line || hook
        }
        _ => {
            let (cx, floors) = (14, 4 + (h >> 8) as i64 % 2);
            (0..floors).any(|f| {
                let eave = FLOOR - 5 - f * 5;
                let half = 10 - f * 2;
                let roof = y == eave && (x - cx).abs() <= half || (y == eave - 1 && (x - cx).abs() <= half - 2);
                let wall = y > eave && y <= eave + 4 && (x - cx).abs() == half - 3;
                roof || wall
            }) || (x == cx && (FLOOR - 5 - floors * 5 - 3..FLOOR - 5 - floors * 5).contains(&y))
        }
    }
}

fn ground(scene: &Scene, x: i64, y: i64) -> bool {
    const WIDE: i64 = 7;
    let (chunk, x, h, world) = chunk_of(scene, GROUND, x, WIDE);
    if y == FLOOR && mix(world, 5).is_multiple_of(4) {
        let still = scene.quiet_at(scene.came_in(GROUND, world)) > 0.5 && mix(world, 6).is_multiple_of(3);
        if still || scene.fullness(GROUND, world) > 0.3 {
            return true;
        }
    }
    if !scene.keeps(GROUND, chunk, WIDE, 0.5) {
        return false;
    }
    if h % 6 == 5 {
        let at = 2 + (h >> 8) as i64 % 3;
        return (x == at && (FLOOR - 1..=FLOOR).contains(&y)) || (y == FLOOR - 2 && (x - at).abs() <= 1);
    }
    if h % 6 == 3 {
        let at = 2 + (h >> 8) as i64 % 3;
        let tall = |dx: i64| if dx == 0 { 3 } else { 2 };
        return (-1..=1).any(|dx| x == at + dx && y > FLOOR - tall(dx) && y <= FLOOR);
    }
    let start = (h >> 8) as i64 % 3;
    let wide = 2 + (h >> 16) as i64 % 3;
    let tall = 1 + (h >> 24) as i64 % 2;
    let x = x - start;
    let corner = (x == 0 || x == wide - 1) && y == FLOOR - tall + 1 && tall > 1;
    (0..wide).contains(&x) && y > FLOOR - tall && y <= FLOOR && !corner
}

fn front(scene: &Scene, x: i64, y: i64) -> bool {
    const WIDE: i64 = 64;
    let (chunk, x, h, world) = chunk_of(scene, FRONT, x, WIDE);
    if !scene.keeps(FRONT, chunk, WIDE, 0.35) {
        return false;
    }
    if (h >> 8) % 3 == 2 {
        let board = (6..=11).contains(&y) && (12..=30).contains(&x);
        let rim = y == 6 || y == 11 || x == 12 || x == 30;
        let writing = y == 8 && (15..=27).contains(&x) && x % 4 != 2;
        ((20..23).contains(&x) && y > 11) || (board && (rim || writing))
    } else if (h >> 8) % 3 == 0 {
        (20..23).contains(&x) && y >= 10 && !(x == 22 && (y - 10) % 5 == 2)
    } else {
        let reach = 4 + (h >> 16) as i64 % 4;
        let (dx, dy) = (x - 30, y - (FLOOR + 1));
        dx * dx + dy * dy * 2 <= reach * reach * 2 && !mix(world * 64 + y, 7).is_multiple_of(7)
    }
}

fn pediment_front(x: i64, y: i64) -> bool {
    let (left, right, eave) = (6, 32, FLOOR - 15);
    let middle = (left + right) / 2;
    if !(left..=right).contains(&x) || y > FLOOR {
        return false;
    }
    if y < eave {
        let up = eave - y;
        let edge = (x - middle).abs() == (right - middle) - 2 * up || (x - middle).abs() == (right - middle) - 2 * up - 1;
        let gap = (x - middle).abs() <= 3;
        let ball = (x - middle).pow(2) + (y - (eave - 6)).pow(2) * 2 <= 4;
        return (edge && !gap && up <= 6) || ball;
    }
    let wall = x == left || x == right || y == eave || y == eave + 1;
    let windows = (x - left) % 4 == 2 && (y - eave) % 4 == 3 && y < FLOOR - 5 && (x - middle).abs() > 2;
    let door = {
        let (dx, dy) = (x - middle, y - (FLOOR - 4));
        (dx.abs() == 3 && dy >= 0) || (dy < 0 && dx * dx + dy * dy * 3 >= 7 && dx * dx + dy * dy * 3 <= 12)
    };
    wall || windows || door
}

fn ball_on_cube(x: i64, y: i64) -> bool {
    let cube = (14..=26).contains(&x) && (FLOOR - 9..=FLOOR).contains(&y) && (x == 14 || x == 26 || y == FLOOR - 9 || (x + y) % 4 == 0);
    let (dx, dy) = (x - 20, y - (FLOOR - 15));
    let ring = dx * dx + dy * dy;
    let ball = (20..=30).contains(&ring) || (ring <= 4 && dx < 0 && dy < 0);
    let shaft = (4..=6).contains(&x) && (FLOOR - 16..=FLOOR).contains(&y) && (x != 5 || y % 2 == 0);
    let capital = (1..=9).contains(&x) && (y == FLOOR - 18 || (y == FLOOR - 17 && (2..=8).contains(&x)));
    let base = (3..=7).contains(&x) && y == FLOOR;
    cube || ball || shaft || capital || base
}

const ERA: i64 = 480;

const TURN_EASE: i64 = 40;

const ZOOM_WAIT: i64 = 30;
const ZOOM_STEP: i64 = 3;
const FARTHEST: f32 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Turn {
    Plain,
    Wide,
    Companion,
    Night,
    Rain,
    Snow,
    Still,
}

fn drawn_turn(era: i64) -> Turn {
    match mix(era, 70) % 100 {
        0..=4 => Turn::Wide,
        5..=11 => Turn::Companion,
        12..=17 => Turn::Night,
        18..=21 => Turn::Rain,
        22..=24 => Turn::Snow,
        25..=28 => Turn::Still,
        _ => Turn::Plain,
    }
}

fn turn_of(era: i64) -> Turn {
    if drawn_turn(era - 1) == Turn::Still || drawn_turn(era - 2) == Turn::Still {
        Turn::Still
    } else {
        drawn_turn(era)
    }
}

fn turn_at(tick: i64, since: Option<i64>) -> (Turn, i64, i64) {
    let era = tick.div_euclid(ERA);
    let counts = |era: i64| !since.is_some_and(|since| era * ERA < since);
    if !counts(era) {
        return (Turn::Plain, tick.rem_euclid(ERA), ERA);
    }
    let turn = turn_of(era);
    if turn != Turn::Still {
        return (turn, tick.rem_euclid(ERA), ERA);
    }
    let (mut first, mut last) = (era, era);
    while era - first < 8 && counts(first - 1) && turn_of(first - 1) == Turn::Still {
        first -= 1;
    }
    while last - era < 8 && turn_of(last + 1) == Turn::Still {
        last += 1;
    }
    (turn, tick - first * ERA, (last - first + 1) * ERA)
}

fn eased(into: i64, span: i64) -> f32 {
    let from_edge = into.min(span - 1 - into).max(0);
    (from_edge as f32 / TURN_EASE as f32).min(1.0)
}

fn creature(look: u64, step: i64, x: i64, y: i64) -> bool {
    let long = 5 + (look % 6) as i64;
    let legs = 1 + ((look >> 4) % 3) as i64;
    let body = FLOOR - legs;
    let head = long;
    let ear_up = (look >> 8).is_multiple_of(2);
    let snout = !(look >> 9).is_multiple_of(3);
    let tail = (look >> 12) % 3;
    if y > FLOOR {
        return false;
    }
    let trunk = (body - 1..=body).contains(&y) && (0..long).contains(&x) && !(y == body - 1 && (x == 0 || x == long - 1) && long > 6);
    let skull = (body - 3..=body - 1).contains(&y) && (head - 1..=head + 1).contains(&x);
    let nose = snout && y == body - 1 && x == head + 2;
    let eye = y == body - 3 && x == head + 1;
    let ear = if ear_up { y == body - 4 && x == head - 1 } else { (body - 3..=body - 2).contains(&y) && x == head - 2 };
    let tail_at = match tail {
        0 => (y == body - 2 && x == -1) || (y == body - 3 && x == -2),
        1 => y == body && x == -1 || (y == body + 1 && x == -2 && legs > 1),
        _ => {
            let up = step.rem_euclid(2) == 0;
            (y == body - 1 && x == -1) || (y == if up { body - 2 } else { body - 1 } && x == -2)
        }
    };
    let legs_at = y > body && {
        let stride = step.rem_euclid(2) == 0;
        let low = y == FLOOR && legs > 1;
        let feet: [i64; 4] = if stride && low { [0, 2, long - 3, long - 1] } else { [1, 1, long - 2, long - 2] };
        feet.contains(&x)
    };
    (trunk || skull || nose || ear || tail_at || legs_at) && !eye
}

fn creatures(scene: &Scene, x: i64, y: i64) -> bool {
    const WIDE: i64 = 300;
    for (plane, facing_right) in [(TOWARD, false), (ALONG, true)] {
        let (chunk, x, h, _) = chunk_of(scene, plane, x, WIDE);
        if !scene.keeps(plane, chunk, WIDE, 0.07) {
            continue;
        }
        let x = x - 40;
        let step = if facing_right { scene.tick.div_euclid(2) } else { scene.tick };
        let x = if facing_right { x } else { 14 - x };
        if (-3..20).contains(&x) && creature(h, step, x, y) {
            return true;
        }
    }
    false
}

const TOWARD: Plane = Plane { num: 3, den: 2, salt: 23 };
const ALONG: Plane = Plane { num: 1, den: 2, salt: 24 };

fn companion(scene: &Scene, ahead: i64, x: i64, y: i64) -> bool {
    if scene.turn != Turn::Companion {
        return false;
    }
    const RUN: i64 = 36;
    let look = mix(scene.tick.div_euclid(ERA), 71);
    let into = scene.into;
    let at = if into < RUN {
        -16 + (ahead + 16) * into / RUN
    } else if into > ERA - RUN {
        ahead + (scene.columns + 4 - ahead) * (into - (ERA - RUN)) / RUN
    } else {
        ahead
    };
    let step = if (RUN..=ERA - RUN).contains(&into) { scene.tick.div_euclid(2) } else { scene.tick };
    creature(look, step, x - at, y)
}

fn night(scene: &Scene, x: i64, y: i64) -> bool {
    let night = scene.turn == Turn::Night;
    if !(night || scene.turn == Turn::Still) || y >= FLOOR - 6 {
        return false;
    }
    let ramp = scene.ramp();
    let moon = night && {
        let (cx, cy) = (scene.columns * 4 / 5, 5);
        let (dx, dy) = (x - cx, y - cy);
        let (sx, sy) = (dx + 2, dy + 1);
        ramp > 0.4 && dx * dx + dy * dy <= 20 && sx * sx + sy * sy > 16
    };
    let every = if night { 97 } else { 151 };
    let world = x + scene.offset(HIGH);
    let star = |wx: i64, wy: i64| -> Option<bool> {
        let h = mix(wx * 64 + wy, 72);
        let there = h.is_multiple_of(every)
            && wy < FLOOR - 7
            && ((h >> 8) % 1000) as f32 / 1000.0 < ramp
            && (scene.tick + (h >> 20) as i64).rem_euclid(9) != 0;
        there.then_some((h >> 32).is_multiple_of(4))
    };
    let shines = star(world, y).is_some()
        || [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|&(dx, dy)| star(world + dx, y + dy) == Some(true));
    moon || shines
}

fn weather(scene: &Scene, x: i64, y: i64) -> bool {
    if y >= FLOOR {
        return false;
    }
    let ramp = scene.ramp();
    match scene.turn {
        Turn::Rain => {
            let lane = x + y.div_euclid(2);
            let h = mix(lane, 73);
            let falls = (y - scene.tick * 3 + (h >> 8) as i64 % 40).rem_euclid(14);
            h.is_multiple_of(5) && falls < 3 && ((h >> 16) % 1000) as f32 / 1000.0 < ramp
        }
        Turn::Snow => {
            let drift = (y / 5 + scene.tick.div_euclid(6) + x / 9).rem_euclid(3) - 1;
            let lane = x - drift;
            let h = mix(lane, 74);
            let falls = (y - scene.tick.div_euclid(2) + (h >> 8) as i64 % 40).rem_euclid(13);
            h.is_multiple_of(7) && falls == 0 && ((h >> 16) % 1000) as f32 / 1000.0 < ramp
        }
        _ => false,
    }
}

const GIANT: Plane = Plane { num: 1, den: 1, salt: 21 };
const GIANT_UNIT: i64 = 4;

fn giants(scene: &Scene, x: i64, y: i64) -> bool {
    const WIDE: i64 = 64;
    if y > FLOOR {
        return false;
    }
    let world = (x + scene.offset(GIANT)).div_euclid(GIANT_UNIT);
    let row = FLOOR - (FLOOR - y).div_euclid(GIANT_UNIT);
    let chunk = world.div_euclid(WIDE);
    let h = mix(chunk, GIANT.salt);
    if (h >> 4).is_multiple_of(2) {
        return false;
    }
    postmodern(h, world.rem_euclid(WIDE) - 10, row)
}

fn postmodern(h: u64, x: i64, y: i64) -> bool {
    match (h >> 8) % 6 {
        0 => {
            let (left, right, top) = (0, 22, 2 + (h >> 16) as i64 % 3);
            let middle = (left + right) / 2;
            let shaft = top + 6;
            if !(left..=right).contains(&x) {
                return false;
            }
            if y < shaft {
                let up = shaft - y;
                let reach = (right - middle) - 2 * (up - 1);
                let edge = (x - middle).abs() == reach || (x - middle).abs() == reach - 1 || y == shaft - 1;
                let notch = (x - middle).pow(2) + (y - top - 1).pow(2) <= 8;
                return y >= top && edge && !notch;
            }
            let side = x == left || x == right;
            let mullion = (x - left) % 3 == 0 && y < FLOOR - 6;
            let band = (y - shaft) % 5 == 0;
            let (dx, dy) = (x - middle, y - (FLOOR - 2));
            let arch = (16..=26).contains(&(dx * dx + dy * dy)) && dy <= 0 || (dx.abs() == 4 && dy > 0);
            side || mullion || band || arch
        }
        1 => {
            let (left, right, top) = (0, 26, FLOOR - 20);
            if !(left..=right).contains(&x) || y < top {
                return false;
            }
            let (dx, dy) = (x - 13, y - (top + 8));
            let ring = (25..=40).contains(&(dx * dx + dy * dy));
            let key = dy < -6 && dy > -9 && dx.abs() <= (-dy - 5);
            let stripes = y > top + 15 && y % 2 == 0;
            let edge = x == left || x == right || y == top;
            ring || key || stripes || edge
        }
        2 => {
            (0..3).any(|i| {
                let (middle, half, top) = (22, 20 - i * 5, FLOOR - 18 + i * 5);
                let dx = x - middle;
                if dx.abs() > half || y < top - 2 {
                    return false;
                }
                let curve = top + (dx * dx * 3) / (half * half).max(1);
                let beam = y == curve - 2 || y == curve - 1;
                let column = y > curve && dx.rem_euclid(4) == 0;
                beam || column
            })
        }
        3 => {
            let steps = 5;
            let step = (FLOOR - y).div_euclid(4);
            let ziggurat = y <= FLOOR && step < steps && (x - 14).abs() <= 14 - step * 3;
            let face = ziggurat && ((FLOOR - y) % 4 == 0 || (x - 14).abs() == 14 - step * 3 || (x - 14).rem_euclid(3) == 0);
            let (dx, dy) = (x - 14, y - (FLOOR - 23));
            let ball = (5..=10).contains(&(dx * dx + dy * dy));
            let column = (36..=38).contains(&x) && y >= FLOOR - 18 && (x != 37 || y % 2 == 0);
            let capital = (34..=40).contains(&x) && y == FLOOR - 19;
            face || ball || column || capital
        }
        4 => {
            let cube = (4..=18).contains(&x) && (FLOOR - 10..=FLOOR).contains(&y) && (x == 4 || x == 18 || y == FLOOR - 10 || (x - y) % 5 == 0);
            let drum = (7..=15).contains(&x) && (FLOOR - 18..FLOOR - 10).contains(&y) && (x == 7 || x == 15 || y == FLOOR - 18 || y == FLOOR - 17);
            let cone = (FLOOR - 25..FLOOR - 18).contains(&y) && (x - 11).abs() == (y - (FLOOR - 25)) / 2;
            let arch = (22..=34).contains(&x) && {
                let (dx, dy) = (x - 28, y - (FLOOR - 6));
                (dy <= 0 && (30..=42).contains(&(dx * dx + dy * dy * 2))) || ((dx.abs() == 5 || dx.abs() == 6) && dy > 0)
            };
            cube || drum || cone || arch
        }
        _ => {
            let (left, right, top) = (2, 30, FLOOR - 24);
            let frame = (left..=right).contains(&x)
                && (top..=FLOOR).contains(&y)
                && (x <= left + 1 || x >= right - 1 || y <= top + 1);
            let (dx, dy) = (x - 16, y - (top + 11));
            let turned = (dx.abs() + dy.abs() == 7 || dx.abs() + dy.abs() == 8) && y < FLOOR - 3;
            let stairs = y > FLOOR - 3 && (left - 3..=right + 3).contains(&x) && (FLOOR - y) <= ((x - 16).abs() - 12).clamp(0, 3);
            frame || turned || stairs
        }
    }
}

const OPENING_DROP: f64 = 2.4;
const OPENING_HOLD: f64 = 1.2;

fn saga_opening(reading: &mut Reading, now: f64, (width, height): (usize, usize), rows: usize) -> Option<Vec<u8>> {
    if reading.opened {
        return None;
    }
    let began = *reading.opening.get_or_insert(now);
    let gone = now - began;
    if gone >= OPENING_DROP + OPENING_HOLD {
        reading.opened = true;
        return None;
    }
    let grid = Grid::whole(width, height, rows * SAGA_ROWS / GRID_ROWS);
    Some(opening_page(&grid, (gone / OPENING_DROP).min(1.0)))
}

fn opening_page(grid: &Grid, down: f64) -> Vec<u8> {
    let word = crate::misaki::columns("kessler");
    let all = word.iter().fold(0u8, |all, byte| all | byte);
    let first = i64::from(all.trailing_zeros().min(7));
    let ink_rows = 8 - i64::from(all.leading_zeros().min(7)) - first;
    let k = (grid.rows / FIGURE_TALL).max(1) as i64;
    let (columns, rows) = (grid.columns as i64 / k, grid.rows as i64 / k);
    let left = (columns - (word.len() as i64 - 1)) / 2;
    let rest = (rows - ink_rows) / 2;
    let top = (-ink_rows as f64 + (rest + ink_rows) as f64 * down).round() as i64;
    grid.paint(|column, row| {
        let (x, y) = (column as i64 / k - left, row as i64 / k - top);
        (0..ink_rows).contains(&y) && x >= 0 && word.get(x as usize).is_some_and(|byte| byte & (1 << (y + first)) != 0)
    })
}

#[derive(Clone, Copy)]
struct Entry {
    began: i64,
    most: i64,
}

const ENTRY_SECS: f64 = 12.0;

fn saga(grid: &Grid, tick: i64, heard: &SagaHeard, entry: Option<Entry>) -> Vec<u8> {
    let frames = figures();
    let Some(figure) = frames.get(tick.rem_euclid(frames.len().max(1) as i64) as usize) else {
        return grid.paint(|_, _| false);
    };
    let k = (grid.rows / FIGURE_TALL).max(1);
    let columns = grid.columns.div_ceil(k) as i64;
    let (rows, k_rows) = (grid.rows as i64, k as i64);
    let top = if SAGA_ROWS as i64 * k_rows <= rows {
        (rows - SAGA_ROWS as i64 * k_rows) / 2
    } else if (FIGURE_TALL as i64 + 1) * k_rows <= rows {
        rows - SAGA_ROWS as i64 * k_rows
    } else {
        (rows - FIGURE_TALL as i64 * k_rows) / 2 - k_rows
    };
    let middle_left = (columns - FIGURE_WIDE as i64) / 2;
    let (left, since) = match entry.filter(|entry| tick >= entry.began) {
        Some(entry) => {
            let way = middle_left + FIGURE_WIDE as i64;
            let pace = (way + entry.most - 1).div_euclid(entry.most).max(1);
            let arrives = entry.began + (way + pace - 1).div_euclid(pace);
            (middle_left - ((arrives - tick).max(0) * pace).min(way), Some(arrives))
        }
        None => (middle_left, None),
    };
    let figure_top = 1;
    let (turn, into, span) = turn_at(tick, since);
    let scene = Scene { tick, columns, heard, since, turn, into, span };
    let zoom = scene.zoom();
    let centre = left as f32 + FIGURE_WIDE as f32 / 2.0;
    let ahead = left + FIGURE_WIDE as i64 + 2;
    let behind = |x: i64, y: i64| {
        ground(&scene, x, y)
            || creatures(&scene, x, y)
            || middle(&scene, x, y)
            || landmarks(&scene, x, y)
            || birds(&scene, x, y)
            || clouds(&scene, x, y)
            || far(&scene, x, y)
            || night(&scene, x, y)
            || weather(&scene, x, y)
    };
    grid.paint(|column, row| {
        let y = (row as i64 - top).div_euclid(k as i64);
        if !(0..SAGA_ROWS as i64).contains(&y) {
            return false;
        }
        let x = (column / k) as i64;
        if zoom > 1.0 {
            let span = |from: f32, to: f32| (from.floor() as i64, (to.ceil() as i64).max(from.floor() as i64 + 1));
            let (x0, x1) = (centre + (x as f32 - centre) * zoom, centre + (x as f32 + 1.0 - centre) * zoom);
            let floor_edge = FLOOR as f32 + 1.0;
            let (y0, y1) = (floor_edge - (FLOOR - y + 1) as f32 * zoom, floor_edge - (FLOOR - y) as f32 * zoom);
            let small = |x: i64, y: i64| {
                let (x0, x1) = (centre + (x as f32 - centre) * zoom, centre + (x as f32 + 1.0 - centre) * zoom);
                let (y0, y1) = (floor_edge - (FLOOR - y + 1) as f32 * zoom, floor_edge - (FLOOR - y) as f32 * zoom);
                let ((sx0, sx1), (sy0, sy1)) = (span(x0, x1), span(y0, y1));
                let inked = (sy0..sy1).flat_map(|sy| (sx0..sx1).map(move |sx| (sx, sy))).filter(|&(sx, sy)| figure.at(sx - left, sy - figure_top).0).count() as i64;
                inked > 0 && inked * 4 >= (sx1 - sx0) * (sy1 - sy0)
            };
            if small(x, y) {
                return true;
            }
            if (-1..=1).any(|dy| (-1..=1).any(|dx| small(x + dx, y + dy))) {
                return false;
            }
            let (cx, cy) = (((x0 + x1) / 2.0).floor() as i64, ((y0 + y1) / 2.0).floor() as i64);
            let between = (mix(cx * 131 + cy, 75) % 1000) as f32 / 1000.0 > (zoom - 1.0) / 1.5;
            return giants(&scene, cx, cy)
                || ground(&scene, cx, cy)
                || creatures(&scene, cx, cy)
                || birds(&scene, cx, cy)
                || clouds(&scene, cx, cy)
                || (between && behind(cx, cy));
        }
        let (fx, fy) = (x - left, y - figure_top);
        if front(&scene, x, y) {
            return true;
        }
        if figure.at(fx, fy).0 {
            return true;
        }
        if figure.clears(fx, fy) {
            return false;
        }
        if companion(&scene, ahead, x, y) {
            return true;
        }
        if (-1..=1).any(|dy| (-1..=1).any(|dx| companion(&scene, ahead, x + dx, y + dy))) {
            return false;
        }
        behind(x, y)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_book_is_set_in_lines_and_comes_round() {
        assert!(BOOK.starts_with("Als Zarathustra dreissig Jahr alt war"));
        assert_eq!(book_place(1), 1);
        let inside = BOOK.char_indices().find(|(_, c)| c.len_utf8() > 1).unwrap().0 + 1;
        assert_eq!(book_place(inside), 0);
        assert_eq!(book_place(BOOK.len() + 5), 0);
        let (line, next) = line_from(0, 64, Setting::Justified);
        assert_eq!(line.len(), 64);
        assert!(BOOK[next..].starts_with("dreissig"));
        let (line, next) = line_from(0, 88, Setting::Justified);
        assert_eq!(line.len(), 88);
        assert!(BOOK[next..].starts_with("sig "), "{}", &BOOK[next..next + 10]);
        let (line, next) = line_from(4, 8, Setting::Justified);
        assert_eq!(line.len(), 8);
        assert!(next > 4);
        let last = BOOK.rfind(' ').unwrap() + 1;
        assert_eq!(line_from(last, 400, Setting::Justified).1, 0);
        let grid = Grid::page(160, 24, BOOK_ROWS);
        let grey = book_page(0, 0, &grid, Setting::Justified);
        assert!(grey.contains(&255));
        assert!(grey[160 * 7..160 * 8].iter().all(|&g| g == 0));
    }

    #[test]
    fn german_is_hyphenated_by_its_rules_of_thumb() {
        let cut = |word: &str| -> Vec<String> {
            let mut parts = Vec::new();
            let mut from = 0;
            for at in hyphen_points(word) {
                parts.push(word[from..at].to_string());
                from = at;
            }
            parts.push(word[from..].to_string());
            parts
        };
        assert_eq!(cut("dreissig"), ["dreis", "sig"]);
        assert_eq!(cut("Zarathustra"), ["Za", "ra", "thus", "tra"]);
        assert_eq!(cut("Menschen"), ["Men", "schen"]);
        assert_eq!(cut("Zucker"), ["Zu", "cker"]);
        assert_eq!(cut("Höhle:"), ["Höh", "le:"]);
        assert_eq!(cut("ich"), ["ich"]);
        assert_eq!(cut("„Was"), ["„Was"]);
    }

    #[test]
    fn a_line_is_set_full_out() {
        let word = |n: usize| vec![vec![1u8; 4]; n];
        let words = vec![word(1), word(1), word(1)];
        let line = set_across(&words, 21);
        assert_eq!(line.len(), 21);
        assert_eq!(line[4..9], [0; 5]);
        assert_eq!(line[9..13], [1; 4]);
        assert_eq!(line[13..17], [0; 4]);
        assert_eq!(line[17..21], [1; 4]);
        assert_eq!(set_across(&[word(3)], 20).len(), 12);
        let (line, next) = line_from(0, 84, Setting::Wall);
        assert_eq!(line.len(), 84);
        assert!(BOOK[next..].starts_with("sig "), "{}", &BOOK[next..next + 8]);
    }

    #[test]
    fn the_pictures_go_round_by_name() {
        let mut picture = FieldPicture::Scope;
        let mut seen = 0;
        while let Some(next) = picture.next() {
            assert_eq!(FieldPicture::from_name(picture.name()), picture);
            picture = next;
            seen += 1;
        }
        assert_eq!(seen, 7);
        assert_eq!(FieldPicture::from_name("lochstreifen"), FieldPicture::Life);
        assert_eq!(FieldPicture::from_name(""), FieldPicture::Scope);
    }

    #[test]
    fn the_snake_stays_in_the_field_and_off_itself() {
        let mut snake = Snake::new(7, 220, 52, SNAKE_ROWS);
        for tick in 0..5000 {
            let level = if tick % 7 == 0 { 0.5 } else { 0.05 };
            snake.walk_to(tick, level);
            let mut seen = std::collections::HashSet::new();
            for &at in &snake.body {
                assert!((0..snake.columns).contains(&at.0) && (0..snake.rows).contains(&at.1));
                assert!(seen.insert(at), "the snake ran into itself at tick {tick}");
            }
        }
        assert_eq!(snake.grey().len(), 220 * 52);
    }

    #[test]
    fn the_snake_moves_only_on_ticks() {
        let mut snake = Snake::new(3, 120, 30, SNAKE_ROWS);
        snake.walk_to(10, 0.1);
        let before = snake.body.clone();
        snake.walk_to(10, 0.1);
        assert_eq!(snake.body, before);
        snake.walk_to(11, 0.1);
        assert_ne!(snake.body, before);
    }

    #[test]
    fn the_snake_goes_where_it_is_steered() {
        let mut snake = Snake::new(5, 120, 30, SNAKE_ROWS);
        snake.steered = Some((0, -1));
        snake.walk_to(0, 0.0);
        snake.walk_to(1, 0.0);
        assert_eq!(snake.heading, (0, -1));
        snake.steered = Some((0, 1));
        for tick in 2..40 {
            snake.walk_to(tick, 0.0);
            assert!(snake.body.iter().all(|&(x, y)| x >= 0 && y >= 0 && x < snake.columns && y < snake.rows));
        }
    }

    #[test]
    fn the_zoom_shows_the_edge_all_the_way_down() {
        for depth in [0.0, 0.5, 1.0] {
            let grey = mandelbrot(80, 20, depth, GRID_ROWS);
            assert!(grey.iter().any(|&g| g > 40), "nothing at depth {depth}");
        }
        let mut zoom = Zoom::default();
        let a = zoom.grey(40, 10, 0.3, GRID_ROWS);
        assert_eq!(zoom.grey(40, 10, 0.3, GRID_ROWS), a);
    }

    #[test]
    fn the_figure_walks_in_and_the_world_comes_after() {
        let grid = Grid::whole(240, 28, SAGA_ROWS);
        let heard = SagaHeard::default();
        let entry = Entry { began: 1000, most: 48 };
        let inked = |grey: &[u8]| (0..240).filter(|&x| (0..28).any(|y| grey[y * 240 + x] == 255)).collect::<Vec<_>>();
        let first = inked(&saga(&grid, 1002, &heard, Some(entry)));
        assert!(!first.is_empty() && first.iter().all(|&x| x < FIGURE_WIDE), "{first:?}");
        let way = inked(&saga(&grid, 1020, &heard, Some(entry)));
        assert!(way[0] > first[0] && way[way.len() - 1] - way[0] < FIGURE_WIDE, "{way:?}");
        let middle = (240 - FIGURE_WIDE) / 2;
        let there = inked(&saga(&grid, 1000 + 48, &heard, Some(entry)));
        assert!(there[0] >= middle && there[there.len() - 1] < middle + FIGURE_WIDE, "{there:?}");
        let later = inked(&saga(&grid, 1000 + 48 + 400, &heard, Some(entry)));
        assert!(later.len() > there.len());
        assert_eq!(saga(&grid, 9000, &heard, Some(entry)), saga(&grid, 9000, &heard, None));
    }

    #[test]
    fn the_word_comes_down_to_the_middle() {
        let grid = Grid::whole(300, 52, SAGA_ROWS);
        assert!(!opening_page(&grid, 0.0).contains(&255), "above the field");
        let grey = opening_page(&grid, 1.0);
        let inked = |x: usize, y: usize| grey[y * 300 + x] == 255;
        let rows: Vec<usize> = (0..52).filter(|&y| (0..300).any(|x| inked(x, y))).collect();
        let columns: Vec<usize> = (0..300).filter(|&x| (0..52).any(|y| inked(x, y))).collect();
        let (top, foot) = (rows[0], 51 - rows[rows.len() - 1]);
        let (left, right) = (columns[0], 299 - columns[columns.len() - 1]);
        assert!(top.abs_diff(foot) <= 2, "{top} above, {foot} below");
        assert!(left.abs_diff(right) <= 2, "{left} left, {right} right");
        let half = opening_page(&grid, 0.5);
        assert!(half.contains(&255) && half != grey);
    }

    #[test]
    fn the_page_has_even_margins_and_the_cells_reach_the_edge() {
        for (width, height) in [(238, 56), (239, 56), (241, 57), (119, 28), (300, 52), (333, 61)] {
            let grid = Grid::page(width, height, BOOK_ROWS);
            let grey = book_page(0, 0, &grid, Setting::Justified);
            let inked = |x: usize| (0..height).any(|y| grey[y * width + x] == 255);
            let left = (0..width).find(|&x| inked(x)).unwrap();
            let right = width - 1 - (0..width).rev().find(|&x| inked(x)).unwrap();
            assert!(left.abs_diff(right) <= 1, "{width}×{height}: {left} left, {right} right");
            assert!(left >= grid.cell, "{width}×{height}: {left}");
            let all = Grid::of(width, height, GRID_ROWS).paint(|_, _| true);
            assert!(all.iter().all(|&g| g == 255), "{width}×{height}");
        }
    }

    #[test]
    fn life_starts_from_the_cover_and_goes_on() {
        let cover = image::GrayImage::from_fn(64, 64, |x, _| image::Luma([if x < 32 { 0 } else { 255 }]));
        let mut life = Life::new(9, 96, 32, GRID_ROWS, Some(&cover), false);
        let half = life.columns / 2;
        let left: usize = (0..life.rows).map(|r| (0..half).filter(|&c| life.cells[r * life.columns + c]).count()).sum();
        let right: usize = (0..life.rows).map(|r| (half..life.columns).filter(|&c| life.cells[r * life.columns + c]).count()).sum();
        assert!(right > left, "the light half of the cover is the live one");
        life.live_to(0, 0.0);
        for tick in 1..400 {
            life.live_to(tick, 0.01);
            assert!(life.cells.iter().any(|&c| c), "life died out at {tick}");
        }
    }

    #[test]
    fn the_ball_bounces_and_changes_its_screen() {
        let mut ball = Ball::new(42);
        ball.fit(120, 32, GRID_ROWS);
        let grid = ball.grid.unwrap();
        let first = ball.screen;
        let mut screens = std::collections::HashSet::new();
        for tick in 0..400 {
            ball.roll_to(tick);
            screens.insert(ball.screen);
            assert!(ball.at.0 >= 0 && ball.at.0 + ball.side() <= grid.columns as i32);
            assert!(ball.at.1 >= 0 && ball.at.1 + ball.side() <= grid.rows as i32);
        }
        assert!(screens.len() > 3, "screened only {screens:?}, from {first}");
        assert!(ball.grey(false).iter().any(|&g| g > 0));
        ball.fit(40, 32, GRID_ROWS);
        assert!(ball.at.0 + ball.side() <= ball.grid.unwrap().columns as i32);
    }

    #[test]
    fn the_saga_walks_in_the_middle_while_the_world_goes_by() {
        let frames = figures();
        assert_eq!(frames.len(), 15);
        assert!(frames.iter().all(|f| f.ink.len() == FIGURE_WIDE * FIGURE_TALL));
        assert!(frames[0].hole.iter().any(|&h| h), "the face is inside the figure");
        let grid = Grid::whole(200, 28, SAGA_ROWS);
        let heard = SagaHeard::default();
        let one = saga(&grid, 0, &heard, None);
        let left = (grid.columns as i64 - FIGURE_WIDE as i64) / 2;
        let (fx, fy) = (0..FIGURE_WIDE * FIGURE_TALL)
            .find(|&at| frames[0].ink[at])
            .map(|at| ((at % FIGURE_WIDE) as i64, (at / FIGURE_WIDE) as i64))
            .unwrap();
        assert_eq!(one[(fy as usize + 1) * 200 + (fx + left) as usize], 255);
        let later = saga(&grid, 40, &heard, None);
        assert_ne!(one, later);
        let grid = Grid::whole(300, 52, SAGA_ROWS);
        let grey = saga(&grid, 0, &heard, None);
        let middle = grid.columns / 2;
        let inked: Vec<usize> = (0..52).filter(|&y| (middle - 16..middle + 16).any(|x| grey[y * 300 + x] == 255)).collect();
        assert!(inked.first().is_some_and(|&y| y <= 1), "{inked:?}");
        assert!(inked.last().is_some_and(|&y| y >= 50), "{inked:?}");
    }

    #[test]
    fn the_turns_are_rare() {
        let mut seen = std::collections::HashMap::new();
        for era in 0..20_000 {
            *seen.entry(turn_of(era)).or_insert(0u32) += 1;
        }
        let share = |turn| f64::from(seen.get(&turn).copied().unwrap_or(0)) / 20_000.0;
        assert!(share(Turn::Plain) > 0.55, "{seen:?}");
        for turn in [Turn::Wide, Turn::Companion, Turn::Night, Turn::Rain, Turn::Snow, Turn::Still] {
            assert!((0.015..0.15).contains(&share(turn)), "{turn:?}: {seen:?}");
        }
    }

    #[test]
    fn a_still_stretch_is_long_and_nearly_empty() {
        let era = (1..).find(|&e| drawn_turn(e) == Turn::Still && turn_of(e - 1) != Turn::Still).unwrap();
        let (turn, into, span) = turn_at(era * ERA + ERA + 5, None);
        assert_eq!(turn, Turn::Still);
        assert!(span >= 3 * ERA, "{span}");
        assert_eq!(into, ERA + 5);
        let heard = SagaHeard::default();
        let grid = Grid::whole(300, 28, SAGA_ROWS);
        let middle = (300 - FIGURE_WIDE) / 2;
        let inked = |tick: i64| {
            let grey = saga(&grid, tick, &heard, None);
            (0..28 * 300).filter(|&at| grey[at] == 255 && !(middle - 2..middle + FIGURE_WIDE + 2).contains(&(at % 300))).count()
        };
        let plain = (1..).find(|&e| turn_of(e) == Turn::Plain && turn_of(e - 1) == Turn::Plain).unwrap();
        let busy = inked(plain * ERA + ERA / 2);
        let still = inked(era * ERA + span * 2 / 3);
        assert!(still * 5 < busy, "{still} inked in the still, {busy} in a plain part");
    }

    #[test]
    fn the_view_draws_back_and_comes_close_again() {
        let heard = SagaHeard::default();
        let era = (1..).find(|&e| turn_of(e) == Turn::Wide).unwrap();
        let scene = |into: i64| Scene { tick: era * ERA + into, columns: 300, heard: &heard, since: None, turn: Turn::Wide, into, span: ERA };
        assert_eq!(scene(0).zoom(), 1.0);
        assert_eq!(scene(ERA / 2).zoom(), FARTHEST);
        assert_eq!(scene(ERA - 1).zoom(), 1.0);
        let grid = Grid::whole(300, 28, SAGA_ROWS);
        let middle = (300 - FIGURE_WIDE) / 2;
        let tall = |grey: &[u8]| (0..28).filter(|&y| (middle..middle + FIGURE_WIDE).any(|x| grey[y * 300 + x] == 255)).count();
        let close = tall(&saga(&grid, era * ERA, &heard, None));
        let far = tall(&saga(&grid, era * ERA + ERA / 2, &heard, None));
        assert!(close >= FIGURE_TALL, "{close}");
        assert!((3..=10).contains(&far), "{far} rows tall, drawn back");
    }

    #[test]
    fn the_companion_walks_beside_the_figure() {
        let heard = SagaHeard::default();
        let era = (1..).find(|&e| turn_of(e) == Turn::Companion).unwrap();
        let scene = |into: i64| Scene { tick: era * ERA + into, columns: 300, heard: &heard, since: None, turn: Turn::Companion, into, span: ERA };
        let ahead = 160;
        let beside = |into: i64| (ahead - 4..ahead + 20).any(|x| (0..SAGA_ROWS as i64).any(|y| companion(&scene(into), ahead, x, y)));
        assert!(!beside(0));
        assert!(beside(ERA / 2));
    }

    #[test]
    fn the_creatures_walk() {
        for look in 0..64u64 {
            let ink = |step: i64| -> Vec<bool> { (FLOOR - 9..=FLOOR).flat_map(|y| (-4..18).map(move |x| creature(look, step, x, y))).collect() };
            let (one, two) = (ink(0), ink(1));
            assert!(one.iter().filter(|&&lit| lit).count() > 12, "{look}");
            assert!((-4..18).any(|x| creature(look, 0, x, FLOOR)), "{look} stands on the ground");
            if (look >> 4) % 3 > 0 {
                assert_ne!(one, two, "{look}");
            }
        }
    }

}
