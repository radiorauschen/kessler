use eframe::egui::{self, Rect, Sense, Ui, Vec2};

use super::text;
use super::columns::TrackColumn;
use super::dither::Screen;
use super::theme::{Brightness, Palette, Role, Theme};
use super::GuiApp;

fn row_height() -> f32 {
    super::theme::ROW_ROOMY + super::theme::ROW_AIR
}

const HANDOVER: f64 = 0.08;

const RECENTRE_FRAMES: u8 = 3;

const SCRIM: &str = "sheet-scrim";

const COLUMN: f32 = 352.0;
const PADDING: f32 = 24.0;

const HOVER_BLEED: f32 = 6.0;

fn plate(rect: Rect) -> Rect {
    let inset = (super::theme::ROW_ROOMY * 0.07).round().max(1.0);
    rect.expand2(Vec2::new(HOVER_BLEED, -inset))
}

const WIDTH: f32 = COLUMN + PADDING * 2.0;

const MOST_COLUMNS: usize = 3;

fn sheet_size(widest: f32, gap: f32) -> (f32, usize) {
    let across = |n: usize| COLUMN * n as f32 + gap * (n - 1) as f32 + PADDING * 2.0;
    for n in (2..=MOST_COLUMNS).rev() {
        if widest >= across(n) {
            return (across(n), n);
        }
    }
    (widest.min(WIDTH), 1)
}

const SHORTCUTS: &[(&str, &[(&str, &str)])] = &[
    (
        "wiedergabe",
        &[
            ("[leertaste]", "abspielen, anhalten"),
            ("[enter]", "auswahl abspielen, öffnen"),
            ("[n] / [p]", "nächster, voriger titel"),
            ("[shift] + [←] [→]", "10 sekunden spulen"),
            ("[,] / [.]", "eine minute spulen"),
            ("[+] / [−]", "lautstärke"),
            ("[s] / [r]", "zufall, wiederholen"),
            ("[x] + [←] [→] [↑] [↓]", "schlange im suchfeld steuern"),
        ],
    ),
    (
        "bewegen",
        &[
            ("[j] [k] / [↓] [↑]", "zeile hoch, runter"),
            ("[h] [l] / [←] [→]", "spalte wechseln"),
            ("[tab]", "spalte wechseln"),
            ("[g] [g] / [shift] + [g]", "an den anfang, ans ende"),
            ("[shift] + [↓] [↑]", "mehrere markieren"),
            ("[cmd] + [a]", "alles markieren"),
            ("[esc]", "nichts mehr markiert"),
        ],
    ),
    (
        "ansichten",
        &[
            ("[1] [2] [3] [4]", "musik, radio, podcasts, ipod"),
            ("[c] + [1] [2] [3]", "musik, radio, podcasts ausschalten"),
            ("[1] [2] [3]", "ausgeschaltet: einschalten und öffnen"),
        ],
    ),
    (
        "suchen",
        &[
            ("[/]", "suchen, esc beendet"),
            ("[←] [→]", "im suchfeld: cursor bewegen"),
            ("[alt] + [←] [→]", "im suchfeld: wortweise"),
            ("[cmd] + [←] [→] / [pos1] [ende]", "im suchfeld: an den anfang, ans ende"),
            ("[enter] / [tab]", "aus der suche in die liste"),
        ],
    ),
    (
        "radio",
        &[
            ("[enter]", "gehörte zeile merken"),
            ("[d] / [backspace]", "gemerktes lied vergessen"),
        ],
    ),
    (
        "podcasts",
        &[
            ("[a]", "in die warteschlange, wieder heraus"),
            ("[d] / [backspace]", "archivieren, zurückholen"),
            ("[w]", "warteschlange zeigen"),
            ("[i]", "der podcast, der läuft"),
        ],
    ),
    (
        "warteschlange und listen",
        &[
            ("[a] / [shift] + [a]", "einreihen: ans ende, als nächstes"),
            ("[w]", "warteschlange zeigen"),
            ("[u]", "listen statt warteschlange"),
            ("[shift] + [u]", "nächste liste öffnen"),
            ("[d] / [backspace]", "titel herausnehmen, liste löschen"),
            ("[cmd] + [backspace]", "titel herausnehmen"),
            ("[shift] + [d] zweimal", "warteschlange leeren"),
            ("[shift] + [j] [k]", "in der warteschlange: titel verschieben"),
            ("[shift] + [↓] [↑]", "in der warteschlange: mehrere wählen"),
        ],
    ),
    (
        "bearbeiten",
        &[
            ("[e] / [cmd] + [i]", "idtag bearbeiten"),
            ("rechtsklick", "titel-, album- und listenmenü"),
            ("[v] / [shift] + [f10]", "dieses menü per tastatur, ↑ ↓ wählen, enter"),
            ("[←] [→]", "im menü auf den sternen: bewerten"),
            ("ziehen", "in die warteschlange, in eine liste, auf den ipod oben"),
        ],
    ),
    (
        "cover wählen",
        &[
            ("[h] [j] [k] [l] / [←] [↓] [↑] [→]", "zum nächsten cover"),
            ("[enter]", "dieses nehmen"),
            ("[esc]", "schliessen"),
        ],
    ),
    (
        "fenster",
        &[
            ("[t]", "log: was kessler getan hat"),
            ("[i]", "wer das ist: künstler:in"),
            ("[o]", "andere künstler:innen: ähnlich wie deine, aber noch nicht in der bibliothek"),
            ("[y]", "liedtext"),
            ("[z] / rechtsklick auf vol", "equalizer: tiefen, mitten und höhen einstellen"),
            ("[shift] + [t]", "hell, dunkel"),
            ("[shift] + [f]", "nächstes farbschema"),
            ("[alt] + [shift] + [f]", "voriges farbschema"),
            ("[f] / [f11]", "vollbild"),
            ("[b]", "bildschirmschoner jetzt"),
            ("[cmd] + [+] [−] [0]", "alles grösser, kleiner, wie das system"),
            ("[m]", "menü öffnen: alle einstellungen"),
            ("[?]", "alle tasten auf einen blick"),
            ("[q] [q]", "beenden"),
        ],
    ),
    (
        "im menü",
        &[
            ("[h] [l] / [←] [→]", "zwischen reitern und zeilen wechseln"),
            ("[j] [k] / [↓] [↑]", "reiter oder zeile hoch, runter"),
            ("[enter]", "einstellung ändern"),
            ("[h] [l]", "regler schieben"),
            ("[pos1] / [ende]", "erste, letzte zeile"),
            ("[bild↑] / [bild↓]", "zehn zeilen hoch, runter"),
            ("[shift] + [←] [→]", "reiter wechseln"),
            ("[1] – [8]", "zu einem reiter springen"),
            ("[esc] / [m]", "menü schliessen"),
        ],
    ),
];

#[derive(Debug, PartialEq)]
enum KeyPiece<'a> {
    Key(&'a str),
    Between(&'a str),
}

fn key_pieces(written: &str) -> Vec<KeyPiece<'_>> {
    let mut pieces = Vec::new();
    let mut rest = written;
    while !rest.is_empty() {
        match rest.find('[') {
            Some(0) => {
                let end = rest.find(']').unwrap_or(rest.len());
                pieces.push(KeyPiece::Key(&rest[1..end]));
                rest = rest.get(end + 1..).unwrap_or("");
            }
            Some(at) => {
                let between = rest[..at].trim();
                if !between.is_empty() {
                    pieces.push(KeyPiece::Between(between));
                }
                rest = &rest[at..];
            }
            None => {
                let between = rest.trim();
                if !between.is_empty() {
                    pieces.push(KeyPiece::Between(between));
                }
                rest = "";
            }
        }
    }
    pieces
}

fn key_ink_middle(ui: &Ui) -> f32 {
    let probe = text::line(ui, "1hk", Role::Label, egui::Color32::WHITE);
    let ink = probe.mesh_bounds;
    if ink.height().is_finite() && ink.height() > 0.0 {
        ink.center().y
    } else {
        probe.size().y / 2.0
    }
}

pub(in crate::gui) fn keys_centred(ui: &Ui, written: &str, centre: egui::Pos2, words_ink: egui::Color32, palette: &Palette) -> f32 {
    const PAD_Y: f32 = 2.0;
    const RADIUS: u8 = 3;
    const BETWEEN: f32 = 6.0;
    let pieces: Vec<(bool, std::sync::Arc<egui::Galley>)> = key_pieces(written)
        .into_iter()
        .map(|piece| match piece {
            KeyPiece::Key(key) => (true, text::line(ui, key, Role::Label, palette.ink)),
            KeyPiece::Between(words) => (false, text::line(ui, words, Role::Label, words_ink)),
        })
        .collect();
    let line_tall = pieces.iter().map(|(_, g)| g.size().y).fold(0.0, f32::max);
    let plate_tall = line_tall + PAD_Y * 2.0;
    let wide = |(key, g): &(bool, std::sync::Arc<egui::Galley>)| if *key { (g.size().x + 10.0).max(plate_tall) } else { g.size().x };
    let total: f32 = pieces.iter().map(wide).sum::<f32>() + BETWEEN * pieces.len().saturating_sub(1) as f32;
    let words_top = centre.y - line_tall / 2.0;
    let plate_middle = text::ink_middle(ui, Role::Label, words_top);
    let mut x = (centre.x - total / 2.0).round();
    for piece in &pieces {
        let (key, g) = piece;
        let size = g.size();
        let w = wide(piece);
        if *key {
            let plate = Rect::from_min_size(egui::pos2(x, (plate_middle - plate_tall / 2.0).round()), Vec2::new(w, plate_tall));
            ui.painter().rect_filled(plate, RADIUS, palette.hover);
            let top = (plate.center().y - key_ink_middle(ui)).round();
            let left = (plate.center().x - size.x / 2.0).round();
            ui.painter().galley(egui::pos2(left, top), g.clone(), palette.ink);
        } else {
            ui.painter().galley(egui::pos2(x, words_top), g.clone(), words_ink);
        }
        x += w + BETWEEN;
    }
    plate_tall
}

#[derive(Clone, Copy, PartialEq)]
enum Shine {
    Plain,
    Dim,
    Lit,
}

fn key_line(ui: &mut Ui, written: &str, what: &str, palette: &Palette, whole: bool, shine: Shine) {
    const PAD_X: f32 = 5.0;
    let lit = shine == Shine::Lit;
    let (key_ink, words_ink) = match shine {
        Shine::Dim => (palette.faint, palette.faint),
        Shine::Lit => (palette.ink, palette.ink),
        Shine::Plain => (palette.ink, palette.mid),
    };
    let words_role = if lit { Role::ContentBold } else { Role::Content };
    const PAD_Y: f32 = 2.0;
    const BETWEEN: f32 = 4.0;
    const RADIUS: u8 = 3;
    let width = ui.available_width();
    let keys_width = key_pieces(written)
        .into_iter()
        .map(|piece| match piece {
            KeyPiece::Key(key) => text::line(ui, key, Role::Label, palette.ink).size().x + PAD_X * 2.0 + BETWEEN,
            KeyPiece::Between(words) => text::line(ui, words, Role::Label, palette.faint).size().x + BETWEEN,
        })
        .sum::<f32>();
    let indent = if whole { 0.0 } else { INDENT };
    let room = (width - indent - keys_width - 12.0).max(1.0);
    let mut lines: Vec<String> = Vec::new();
    for word in what.split(' ') {
        let fits = |candidate: &str| text::line(ui, candidate, Role::Content, palette.mid).size().x <= room;
        match lines.last_mut() {
            Some(last) if !whole || fits(&format!("{last} {word}")) => {
                last.push(' ');
                last.push_str(word);
            }
            _ => lines.push(word.to_string()),
        }
    }
    let height = row_height() * lines.len().max(1) as f32;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    if lit {
        let plate = rect.expand2(Vec2::new(HOVER_BLEED, 0.0));
        ui.painter().rect_filled(plate, super::theme::MARKER_RADIUS, palette.hover);
    }
    let rect = Rect::from_min_size(rect.min, Vec2::new(width, row_height()));
    let middle = rect.center().y;
    let mut x = rect.left() + indent;
    for piece in key_pieces(written) {
        match piece {
            KeyPiece::Key(key) => {
                let g = text::line(ui, key, Role::Label, key_ink);
                let size = g.size();
                let plate = Rect::from_min_size(
                    egui::pos2(x, middle - size.y / 2.0 - PAD_Y),
                    Vec2::new(size.x + PAD_X * 2.0, size.y + PAD_Y * 2.0),
                );
                let plate_ink = match shine {
                    Shine::Dim => palette.hover.gamma_multiply(0.5),
                    Shine::Lit => palette.marked(),
                    Shine::Plain => palette.hover,
                };
                ui.painter().rect_filled(plate, RADIUS, plate_ink);
                let top = (plate.center().y - key_ink_middle(ui)).round();
                ui.painter().galley(egui::pos2(plate.left() + PAD_X, top), g, key_ink);
                x = plate.right() + BETWEEN;
            }
            KeyPiece::Between(words) => {
                let g = text::line(ui, words, Role::Label, palette.faint);
                let size = g.size();
                ui.painter()
                    .galley(egui::pos2(x, middle - size.y / 2.0), g, palette.faint);
                x += size.x + BETWEEN;
            }
        }
    }
    let line = text::row_line(ui, row_height(), Role::Content);
    for (n, said) in lines.iter().enumerate() {
        let g = text::line_within(ui, said, words_role, words_ink, (rect.right() - x - 12.0).max(1.0));
        let top = rect.top() + row_height() * n as f32;
        let at = egui::pos2(rect.right() - g.size().x, text::on_line(&g, top, line));
        ui.painter().galley(at, g, words_ink);
    }
}

#[cfg(test)]
fn every_shortcut() -> impl Iterator<Item = &'static (&'static str, &'static str)> {
    SHORTCUTS.iter().flat_map(|(_, keys)| keys.iter())
}

fn key_answers(asked: &str, group: &str, written: &str, what: &str) -> bool {
    let words = |text: &str| -> Vec<String> {
        crate::db::sort_key(text)
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .map(str::to_string)
            .collect()
    };
    let keys: Vec<String> = key_pieces(written)
        .into_iter()
        .filter_map(|piece| match piece {
            KeyPiece::Key(key) => Some(crate::db::sort_key(key)),
            KeyPiece::Between(_) => None,
        })
        .collect();
    let said: Vec<String> = words(what).into_iter().chain(words(group)).collect();
    let asked = crate::db::sort_key(asked);
    let mut any = false;
    for word in asked.split_whitespace() {
        any = true;
        let is_key = keys
            .iter()
            .any(|key| key == word || (word.chars().count() > 1 && key.starts_with(word)));
        let in_words = word.chars().any(char::is_alphanumeric) && said.iter().any(|w| w.starts_with(word));
        if !is_key && !in_words {
            return false;
        }
    }
    any
}

type KeyShown<'a> = (&'a str, &'a str, Shine);

fn keys_field(ui: &mut Ui, asked: &str, palette: &Palette) {
    use super::library::{field_height, FIELD_PAD_X, MIN_FIELD};
    let tall = field_height();
    let (row, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), tall), Sense::hover());
    let written_on = text::line_in_box(ui, tall, Role::Label);
    let shown = if asked.is_empty() {
        text::line(ui, tr!("tippen zum suchen"), Role::Label, palette.mid)
    } else {
        text::cased_within(ui, asked, Role::Label, palette.ink, (row.width() - 120.0).max(1.0))
    };
    let caret_room = if asked.is_empty() { 0.0 } else { 4.0 };
    let wide = (shown.size().x + caret_room + FIELD_PAD_X * 2.0).max(MIN_FIELD).min(row.width());
    let field = Rect::from_min_size(row.min, Vec2::new(wide, tall));
    ui.painter().rect_filled(field, super::theme::MARKER_RADIUS, palette.field.lerp_to_gamma(palette.ink, 0.06));
    let at = egui::pos2(field.left() + FIELD_PAD_X, text::on_line(&shown, field.top(), written_on));
    let caret_x = at.x + shown.size().x + 2.0;
    let glyph_tall = shown.size().y;
    if asked.is_empty() {
        ui.painter().galley(at, shown, palette.mid);
        return;
    }
    ui.painter().galley(at, shown, palette.ink);
    let blink = ui.input(|i| i.time).rem_euclid(1.06) < 0.62;
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(120));
    if blink {
        ui.painter().line_segment(
            [egui::pos2(caret_x, at.y), egui::pos2(caret_x, at.y + glyph_tall)],
            egui::Stroke::new(1.0, palette.ink),
        );
    }
}

const KEYS_FIELD_AIR: f32 = 8.0;

fn caption_in(ui: &mut Ui, word: &str, palette: &Palette, indent: f32) {
    const ABOVE: f32 = 6.0;
    let g = text::line(ui, word, Role::Label, palette.faint);
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), row_height() + ABOVE), Sense::hover());
    let line = text::row_line(ui, row_height(), Role::Label);
    let at = egui::pos2(rect.left() + indent, text::on_line(&g, rect.top() + ABOVE, line));
    ui.painter().galley(at, g, palette.faint);
}

const INDENT: f32 = 20.0;

#[derive(Clone, Copy, PartialEq)]
enum Line {
    Reading,
    Setting,
    Choice { active: bool },
    Listed,
}

#[derive(Clone, Copy, Default)]
pub struct MenuKeys {
    pub cursor: Option<usize>,
    pub press: Press,
    pub follow: bool,
    seen: usize,
    pub count: usize,
    pub on_tabs: bool,
    pub tabbed: bool,
    pub on_slider: bool,
    slider_seen: bool,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Press {
    #[default]
    Nothing,
    Act,
    Less,
    More,
}

fn menu_keys_id() -> egui::Id {
    egui::Id::unique("menu-keys")
}

pub fn with_menu_keys(ctx: &egui::Context, change: impl FnOnce(&mut MenuKeys)) {
    ctx.data_mut(|d| change(d.get_temp_mut_or_default(menu_keys_id())));
}

fn keyed(ui: &Ui, rect: Rect, clicked: bool) -> (bool, Press) {
    let mut out = (false, Press::Nothing);
    let mut follow = false;
    with_menu_keys(ui.ctx(), |keys| {
        let at = keys.seen;
        keys.seen += 1;
        if clicked {
            keys.cursor = Some(at);
            keys.on_tabs = false;
        }
        if keys.cursor == Some(at) {
            out = (true, std::mem::take(&mut keys.press));
            follow = std::mem::take(&mut keys.follow);
        }
    });
    if follow {
        ui.scroll_to_rect(rect.expand2(Vec2::new(0.0, row_height() * 2.0)), None);
    }
    out
}

fn heard_row(ui: &mut Ui, value: &str, tail: Option<std::sync::Arc<egui::Galley>>, palette: &Palette) -> egui::Response {
    let Some(tail) = tail else {
        return row(ui, tr!("gehört"), value, true, palette);
    };
    let (rect, mut response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), row_height()), Sense::click());
    let (on, press) = keyed(ui, rect, response.clicked());
    if on && matches!(press, Press::Act | Press::More) {
        response.flags.set(egui::response::Flags::FAKE_PRIMARY_CLICKED, true);
    }
    if on {
        ui.painter().rect_filled(plate(rect), super::theme::MARKER_RADIUS, palette.marked());
    }
    let written_on = text::row_line(ui, row_height(), Role::Content);
    let label_g = text::line(ui, tr!("gehört"), Role::Content, palette.mid);
    let at = egui::pos2(rect.left(), text::on_line(&label_g, rect.top(), written_on));
    ui.painter().galley(at, label_g, palette.mid);
    let tail_at = egui::pos2(rect.right() - tail.size().x, text::on_line(&tail, rect.top(), written_on));
    let value_g = text::line(ui, value, Role::Content, palette.ink);
    let value_at = egui::pos2(tail_at.x - 6.0 - value_g.size().x, text::on_line(&value_g, rect.top(), written_on));
    ui.painter().galley(value_at, value_g, palette.ink);
    ui.painter().galley(tail_at, tail, palette.mid);
    response
}

fn row(
    ui: &mut Ui,
    label: &str,
    value: &str,
    actionable: bool,
    palette: &Palette,
) -> egui::Response {
    let kind = if actionable { Line::Setting } else { Line::Reading };
    line(ui, label, value, kind, palette, None, false)
}

fn toggle(ui: &mut Ui, label: &str, on: bool, words: (&str, &str), palette: &Palette) -> egui::Response {
    let value = if on { words.0 } else { words.1 };
    line(ui, crate::lang::t(label), crate::lang::t(value), Line::Setting, palette, None, on)
}

fn explanation(ui: &mut Ui, words: &str, palette: &Palette) {
    let width = ui.available_width();
    let g = text::prose(ui, words, Role::Label, palette.faint, width);
    ui.add_space(4.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, g.size().y), egui::Sense::hover());
    ui.painter().galley(rect.min, g, palette.faint);
    ui.add_space(8.0);
}

const AN_AUS: (&str, &str) = ("an", "aus");

fn row_with_hint(
    ui: &mut Ui,
    label: &str,
    value: &str,
    hint: &str,
    palette: &Palette,
) -> egui::Response {
    line(ui, label, value, Line::Setting, palette, Some(hint), false)
}

fn choice(ui: &mut Ui, label: &str, value: &str, active: bool, palette: &Palette) -> egui::Response {
    line(ui, label, value, Line::Choice { active }, palette, None, false)
}

fn set_in<R>(ui: &mut Ui, rows: impl FnOnce(&mut Ui) -> R) -> R {
    let mut room = ui.available_rect_before_wrap();
    room.min.x += INDENT;
    ui.scope_builder(egui::UiBuilder::new().max_rect(room), rows).inner
}

fn line(
    ui: &mut Ui,
    label: &str,
    value: &str,
    kind: Line,
    palette: &Palette,
    hint: Option<&str>,
    lit: bool,
) -> egui::Response {
    let actionable = !matches!(kind, Line::Reading | Line::Listed);
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), row_height()),
        if actionable {
            Sense::click()
        } else {
            Sense::hover()
        },
    );

    if actionable && response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let mut response = response;
    let keyboard = if actionable {
        let (on, press) = keyed(ui, rect, response.clicked());
        if on && matches!(press, Press::Act | Press::More) {
            response.flags.set(egui::response::Flags::FAKE_PRIMARY_CLICKED, true);
        }
        on
    } else {
        false
    };

    let active = matches!(kind, Line::Choice { active: true });
    if keyboard || (actionable && response.hovered()) {
        ui.painter().rect_filled(
            plate(rect),
            super::theme::MARKER_RADIUS,
            if keyboard { palette.marked() } else { palette.hover },
        );
    }

    let indent = if matches!(kind, Line::Choice { .. } | Line::Listed) {
        INDENT
    } else {
        0.0
    };
    let label_role = if active { Role::ContentBold } else { Role::Content };
    let label_colour = if active { palette.ink } else { palette.mid };
    let label_g = text::line(ui, label, label_role, label_colour);
    let value_colour = if actionable || lit { palette.ink } else { palette.mid };
    let value_role = if lit { Role::ContentBold } else { Role::Content };
    let hint_g = hint.map(|word| text::line(ui, word, Role::Label, palette.ink));
    let held = hint_g.as_ref().map(|g| g.size().x + 16.0).unwrap_or(0.0);
    let value_g = text::line_within(
        ui,
        value,
        value_role,
        value_colour,
        (rect.width() - label_g.size().x - indent - 20.0 - held).max(1.0),
    );

    let pad = 0.0;
    let written_on = text::row_line(ui, row_height(), Role::Content);
    let at = egui::pos2(
        rect.left() + indent + pad,
        text::on_line(&label_g, rect.top(), written_on),
    );
    if matches!(kind, Line::Choice { .. }) {
        let shown = ui.ctx().animate_bool_with_time(
            response.id.with("lead-arrow"),
            active,
            super::library::ARROW_SECS,
        );
        if shown > 0.001 {
            let arrow = text::line(ui, "→", Role::Content, label_colour);
            let travel = INDENT - 6.0;
            let x = at.x - 6.0 - arrow.size().x - travel * (1.0 - shown) * 0.5;
            let arrow_at = egui::pos2(x, text::on_line(&arrow, rect.top(), written_on));
            ui.painter().galley(arrow_at, arrow, label_colour.gamma_multiply(shown));
        }
    }
    ui.painter().galley(at, label_g, label_colour);
    let at = egui::pos2(
        rect.right() - value_g.size().x - pad - held,
        text::on_line(&value_g, rect.top(), written_on),
    );
    ui.painter().galley(at, value_g, value_colour);

    if let Some(galley) = hint_g {
        if response.hovered() {
            let at = egui::pos2(
                rect.right() - galley.size().x - pad,
                text::on_line(&galley, rect.top(), written_on),
            );
            ui.painter().galley(at, galley, palette.ink);
        }
    }

    response
}

fn slider(
    ui: &mut Ui,
    label: &str,
    value: f32,
    reading: &str,
    palette: &Palette,
) -> (Option<f32>, bool) {
    const RULE: f32 = 92.0;

    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), row_height()),
        Sense::click_and_drag(),
    );
    let (keyboard, press) = keyed(ui, rect, response.clicked() || response.drag_started());
    if keyboard {
        with_menu_keys(ui.ctx(), |keys| keys.slider_seen = true);
        ui.painter().rect_filled(plate(rect), super::theme::MARKER_RADIUS, palette.marked());
    }
    if response.hovered() {
        if !keyboard {
            ui.painter().rect_filled(plate(rect), super::theme::MARKER_RADIUS, palette.hover);
        }
        ui.ctx()
            .set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }

    let label_g = text::line(ui, label, Role::Content, palette.mid);
    let written_on = text::row_line(ui, row_height(), Role::Content);
    ui.painter().galley(
        egui::pos2(rect.left(), text::on_line(&label_g, rect.top(), written_on)),
        label_g,
        palette.mid,
    );

    let strip = Rect::from_min_size(
        egui::pos2(rect.right() - RULE, rect.top()),
        Vec2::new(RULE, row_height()),
    );
    let reading_g = text::line(ui, reading, Role::Label, palette.faint);
    let reading_top = strip.center().y - text::ink_middle(ui, Role::Label, 0.0);
    ui.painter().galley(
        egui::pos2(strip.left() - reading_g.size().x - 12.0, reading_top),
        reading_g,
        palette.faint,
    );

    let height = if response.hovered() || response.dragged() {
        4.0
    } else {
        2.0
    };
    let rule = Rect::from_min_size(
        egui::pos2(strip.left(), strip.center().y - height / 2.0),
        Vec2::new(RULE, height),
    );
    ui.painter().rect_filled(rule, 0.0, palette.rule);
    let filled = RULE * value.clamp(0.0, 1.0);
    if filled > 0.0 {
        ui.painter().rect_filled(
            Rect::from_min_size(rule.min, Vec2::new(filled, height)),
            0.0,
            palette.ink,
        );
    }

    const STEP: f32 = 0.1;
    let stepped = match press {
        Press::Less => Some((value - STEP).clamp(0.0, 1.0)),
        Press::More => Some((value + STEP).clamp(0.0, 1.0)),
        _ => None,
    };
    let moved = if response.dragged() || response.clicked() {
        response
            .interact_pointer_pos()
            .map(|pointer| ((pointer.x - strip.left()) / RULE).clamp(0.0, 1.0))
    } else {
        stepped
    };

    (moved, response.drag_stopped() || response.clicked() || stepped.is_some())
}

fn music_folders(app: &mut GuiApp, ui: &mut Ui, palette: &Palette) {
    let folders: Vec<std::path::PathBuf> = app.config.music_dirs.clone();
    let mut forget: Option<usize> = None;

    if folders.is_empty() {
        row(ui, tr!("ordner"), tr!("keine eingetragen"), false, palette);
    }
    for (index, dir) in folders.iter().enumerate() {
        let label = if index == 0 { tr!("ordner") } else { "" };
        let shown = shorten(dir);
        let asked = app.forget_asked == Some(index);
        if row_with_hint(ui, label, &shown, tr!("entfernen"), palette).clicked() {
            app.forget_asked = if asked { None } else { Some(index) };
        }
        if asked {
            if choice(ui, tr!("ordner entfernen?"), tr!("ja"), false, palette).clicked() {
                forget = Some(index);
                app.forget_asked = None;
            }
            if choice(ui, "", tr!("nein, behalten"), false, palette).clicked() {
                app.forget_asked = None;
            }
        }
    }

    let picking = app.is_picking();
    let add = if picking {
        tr!("wartet auf den dialog…")
    } else {
        tr!("ordner hinzufügen…")
    };
    if row(ui, "", add, !picking, palette).clicked() {
        app.ask_for_folder();
    }

    if let Some(index) = forget {
        if let Err(e) = app.forget_folder(index) {
            app.set_status(format!("{e}"));
        }
    }
}

fn shorten(path: &std::path::Path) -> String {
    let text = path.display().to_string();
    let Some(home) = std::env::var_os("HOME") else {
        return text;
    };
    let home = home.to_string_lossy();
    match text.strip_prefix(home.as_ref()) {
        Some(rest) => format!("~{rest}"),
        None => text,
    }
}

fn heading(ui: &mut Ui, label: &str, palette: &Palette) {
    if TAB_WORD.with(|word| word.get() == label) {
        return;
    }
    let g = text::line(ui, label, Role::Label, palette.faint);
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), row_height()), Sense::hover());
    let at = egui::pos2(
        rect.left(),
        text::on_line(&g, rect.top(), text::row_line(ui, row_height(), Role::Label)),
    );
    ui.painter().galley(at, g, palette.faint);
}

pub fn render(app: &mut GuiApp, ui: &Ui, window: Rect, m: super::theme::Metrics) {
    let mut fading = 1.0;
    if !app.menu_open {
        app.menu_leaving = None;
    }
    if let Some(since) = app.menu_leaving {
        let gone = ((ui.input(|i| i.time) - since) / HANDOVER) as f32;
        if gone >= 1.0 {
            app.menu_leaving = None;
            app.menu_open = false;
            app.keys_sheet_open = true;
            app.keys_query.clear();
        } else {
            fading = 1.0 - gone;
            ui.ctx().request_repaint();
        }
    }
    if !app.menu_open {
        ui.ctx().data_mut(|d| d.remove::<(Vec2, u8)>(egui::Id::unique("menu-centred-for")));
        app.forget_asked = None;
        app.backup_said = None;
        app.menu_top = None;
        if app.theme_list_open
            || app.licenses_open
            || app.picture_screen_list_open
            || app.saver_screen_list_open
        {
            app.theme_list_open = false;
            app.licenses_open = false;
            app.picture_screen_list_open = false;
            app.saver_screen_list_open = false;
            rebalance(app);
        }
        return;
    }
    let opening = app.menu_top.is_none();
    let clicked = ui.input(|i| i.pointer.primary_clicked());
    with_menu_keys(ui.ctx(), |keys| {
        if opening || clicked {
            *keys = MenuKeys { count: keys.count, seen: keys.seen, tabbed: keys.tabbed, ..MenuKeys::default() };
        }
        keys.count = keys.seen;
        keys.seen = 0;
        keys.on_slider = std::mem::take(&mut keys.slider_seen);
    });

    let palette = app.palette;
    let ctx = ui.ctx().clone();

    let veil = egui::Color32::from_black_alpha(if palette.is_dark() { 168 } else { 112 });
    egui::Area::new(egui::Id::unique(SCRIM))
        .order(egui::Order::Middle)
        .fixed_pos(window.min)
        .show(&ctx, |ui| {
            let (rect, response) = ui.allocate_exact_size(window.size(), Sense::click());
            ui.painter().rect_filled(rect, 0.0, veil);
            if response.clicked() {
                app.menu_open = false;
            }
        });

    let edge_x = m.margin_x - m.marker_pad_x;
    let top = window.top() + super::theme::sheet_inset_top(edge_x, super::theme::titlebar());
    let foot = window.bottom() - edge_x;
    let widest = (window.width() - edge_x * 2.0).max(240.0);

    let tabbed_width = TAB_LIST + TAB_GAP + COLUMN + PADDING * 2.0;
    let tabbed = widest >= tabbed_width;
    with_menu_keys(&ctx, |keys| {
        keys.tabbed = tabbed;
        if opening {
            keys.on_tabs = tabbed;
        }
    });
    let sheet = if tabbed { tabbed_width } else { widest.min(WIDTH) };
    let x = (window.center().x - sheet / 2.0).max(window.left() + edge_x);
    let id = egui::Id::unique("menu");
    let middle = window.center().y;
    let half = (middle - top).min(foot - middle).max(60.0);
    let least = top.max(middle - half);
    let centring = egui::Id::unique("menu-centred-for");
    let (centred_for, frames): (Vec2, u8) = ctx.data(|d| d.get_temp(centring)).unwrap_or((Vec2::ZERO, 0));
    let frames = if centred_for == window.size() { frames.saturating_add(1) } else { 0 };
    ctx.data_mut(|d| d.insert_temp(centring, (window.size(), frames)));
    let settling = app.menu_top.is_none() || frames < RECENTRE_FRAMES;
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(SETTLING), settling));
    if settling {
        ctx.request_repaint();
    }
    let top = match ctx.memory(|mem| mem.area_rect(id)).map(|r| r.height()) {
        Some(height) => (middle - height / 2.0).max(least),
        None => {
            ctx.request_discard("the menu sheet is measured before it is centred");
            least
        }
    };
    app.menu_top = Some(top);
    let room = (middle + half - top).max(120.0);

    egui::Area::new(id)
        .order(egui::Order::Foreground)
        .fixed_pos(egui::pos2(x, top))
        .constrain(false)
        .show(&ctx, |ui| {
            if fading < 1.0 {
                ui.multiply_opacity(fading);
                ui.disable();
            }
            ui.set_max_height(room);
            ui.set_max_width(sheet);
            egui::Frame::NONE
                .fill(palette.ground)
                .corner_radius(4)
                .inner_margin(egui::Margin {
                    left: (PADDING - HOVER_BLEED) as i8,
                    right: (PADDING - HOVER_BLEED) as i8,
                    top: PADDING as i8,
                    bottom: PADDING as i8,
                })
                .show(ui, |ui| {
                    ui.set_width(sheet - (PADDING - HOVER_BLEED) * 2.0);
                    let switched = keep_tab(app, ui);
                    let head = sheet_head(app, ui, &palette);

                    let measured = egui::Id::unique("menu-content-height");
                    let measured_now: f32 = ui.data(|d| d.get_temp(measured)).unwrap_or(room);
                    let held_id = egui::Id::unique("menu-sheet-height");
                    let growing_id = egui::Id::unique("menu-sheet-growing");
                    let mut growing: u8 = ui.data(|d| d.get_temp(growing_id)).unwrap_or(0);
                    if switched {
                        growing = 3;
                    }
                    let held: Option<f32> = ui.data(|d| d.get_temp(held_id));
                    let natural = match held {
                        Some(held) if !settling && growing > 0 => held.max(measured_now),
                        Some(held) if !settling => held,
                        _ => measured_now,
                    };
                    ui.data_mut(|d| {
                        d.insert_temp(held_id, natural);
                        d.insert_temp(growing_id, growing.saturating_sub(1));
                    });
                    let room = room - PADDING * 2.0 - head;

                    let content = |app: &mut GuiApp, ui: &mut Ui, height: f32, width: f32| {
                        let output = egui::ScrollArea::vertical()
                            .id_salt(("menu-body", app.menu_tab))
                            .max_height(height)
                            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                            .show(ui, |ui| {
                                ui.set_width(width + HOVER_BLEED * 2.0);
                                egui::Frame::NONE
                                    .inner_margin(egui::Margin::symmetric(HOVER_BLEED as i8, 0))
                                    .show(ui, |ui| tab_content(app, ui, &palette, height))
                                    .inner
                            });
                        let viewport = output.inner_rect;
                        let above = output.state.clamped_offset().y;
                        let below = (output.content_size.y - above - viewport.height()).max(0.0);
                        super::library::paint_fade(ui, viewport, above, below, &palette);
                        output.content_size.y.max(output.inner)
                    };
                    let grown = if tabbed {
                        let height = natural.max(tab_list_height()).min(room).max(60.0);
                        let mut grown = 0.0;
                        ui.horizontal_top(|ui| {
                            ui.add_space(HOVER_BLEED);
                            tab_list(app, ui, &palette, height);
                            ui.add_space(TAB_GAP - HOVER_BLEED);
                            grown = content(app, ui, height, COLUMN);
                        });
                        grown.max(tab_list_height())
                    } else {
                        let top = ui.cursor().top();
                        tab_bar(app, ui, &palette);
                        ui.add_space(row_height());
                        let used = ui.cursor().top() - top;
                        let height = natural.min(room - used).max(60.0);
                        content(app, ui, height, sheet - PADDING * 2.0)
                    };
                    ui.data_mut(|d| d.insert_temp(measured, grown));
                    if (grown - measured_now).abs() > 0.5 {
                        ui.ctx().request_repaint();
                    }
                });
        });
}

pub fn keys_sheet(app: &mut GuiApp, ui: &Ui, window: Rect, m: super::theme::Metrics) {
    if !app.keys_sheet_open || app.menu_open {
        return;
    }
    let palette = app.palette;
    let ctx = ui.ctx().clone();

    let veil = egui::Color32::from_black_alpha(if palette.is_dark() { 168 } else { 112 });
    egui::Area::new(egui::Id::unique(SCRIM))
        .order(egui::Order::Middle)
        .fixed_pos(window.min)
        .show(&ctx, |ui| {
            let (rect, response) = ui.allocate_exact_size(window.size(), Sense::click());
            ui.painter().rect_filled(rect, 0.0, veil);
            if response.clicked() {
                app.keys_sheet_open = false;
            }
        });

    let edge_x = m.margin_x - m.marker_pad_x;
    let widest = (window.width() - edge_x * 2.0).max(240.0);
    let (sheet, columns) = sheet_size(widest, m.column_gap);
    let id = egui::Id::unique("keys-sheet");
    let height = ctx.memory(|mem| mem.area_rect(id)).map(|r| r.height());
    if height.is_none() {
        ctx.request_discard("the key sheet is measured before it is centred");
    }
    let top = window.top() + super::theme::sheet_inset_top(edge_x, super::theme::titlebar());
    let foot = window.bottom() - edge_x;
    let middle = window.center().y;
    let half = (middle - top).min(foot - middle).max(60.0);
    let top = top.max(middle - half);
    let foot = middle + half;
    let y = height.map(|h| (middle - h / 2.0).max(top)).unwrap_or(top);
    let x = (window.center().x - sheet / 2.0).max(window.left() + edge_x);

    egui::Area::new(id)
        .order(egui::Order::Foreground)
        .fixed_pos(egui::pos2(x, y))
        .constrain(false)
        .show(&ctx, |ui| {
            ui.set_max_height(foot - top);
            ui.set_max_width(sheet);
            egui::Frame::NONE
                .fill(palette.ground)
                .corner_radius(4)
                .inner_margin(egui::Margin::same(PADDING as i8))
                .show(ui, |ui| {
                    ui.set_width(sheet - PADDING * 2.0);
                    let corner = egui::pos2(ui.max_rect().right(), ui.cursor().top());
                    keys_field(ui, &app.keys_query, &palette);
                    ui.add_space(KEYS_FIELD_AIR);
                    let room = (foot - top - PADDING * 2.0 - super::library::field_height() - KEYS_FIELD_AIR).max(120.0);
                    let asked_id = egui::Id::unique("keys-sheet-asked");
                    let changed = ctx.data(|d| d.get_temp::<String>(asked_id)).as_deref() != Some(app.keys_query.as_str());
                    ctx.data_mut(|d| d.insert_temp(asked_id, app.keys_query.clone()));
                    let mut area = egui::ScrollArea::vertical()
                        .id_salt("keys-sheet-body")
                        .max_height(room)
                        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden);
                    if changed {
                        area = area.vertical_scroll_offset(0.0);
                    }
                    let output = area.show(ui, |ui| {
                    let asked = std::mem::take(&mut app.keys_sheet_scroll);
                    if asked != 0.0 {
                        ui.scroll_with_delta(Vec2::new(0.0, -asked));
                    }
                    let query = app.keys_query.trim();
                    let shown: Vec<(&str, Vec<KeyShown>)> = SHORTCUTS
                        .iter()
                        .map(|(group, keys)| {
                            let group_said = crate::lang::t(group);
                            let lines = keys
                                .iter()
                                .map(|(written, what)| {
                                    let (written, what) = (crate::lang::t(written), crate::lang::t(what));
                                    let shine = if query.is_empty() {
                                        Shine::Plain
                                    } else if key_answers(query, group_said, written, what) {
                                        Shine::Lit
                                    } else {
                                        Shine::Dim
                                    };
                                    (written, what, shine)
                                })
                                .collect::<Vec<_>>();
                            (group_said, lines)
                        })
                        .collect();
                    let spacing = ui.spacing().item_spacing.x;
                    ui.spacing_mut().item_spacing.x = m.column_gap;
                    let total: usize = shown.iter().map(|(_, keys)| keys.len() + 1).sum();
                    let per = total.div_ceil(columns.max(1));
                    ui.columns(columns.max(1), |cells| {
                        let mut at = 0;
                        let mut filled = 0;
                        for (group, keys) in &shown {
                            let half = keys.len().div_ceil(2);
                            if filled > 0 && filled + half > per && at + 1 < cells.len() {
                                at += 1;
                                filled = 0;
                            }
                            caption_in(&mut cells[at], group, &palette, 0.0);
                            for (written, what, shine) in keys {
                                key_line(&mut cells[at], written, what, &palette, true, *shine);
                            }
                            filled += keys.len() + 1;
                        }
                    });
                    ui.spacing_mut().item_spacing.x = spacing;
                        });
                    let viewport = output.inner_rect;
                    let above = output.state.clamped_offset().y;
                    let below = (output.content_size.y - above - viewport.height()).max(0.0);
                    super::library::paint_fade(ui, viewport, above, below, &palette);

                    let word = text::line(ui, tr!("schliessen"), Role::Label, palette.mid);
                    let width = word.size().x;
                    let ink = word.mesh_bounds;
                    let at = egui::pos2(corner.x - ink.max.x, corner.y - ink.min.y);
                    let close = super::library::control_plate(ui, at, width);
                    let response = ui.interact(close, egui::Id::unique("keys-sheet-close"), Sense::click());
                    super::library::paint_control_hover(ui, close, &response, &palette);
                    let colour = if response.hovered() { palette.ink } else { palette.mid };
                    ui.painter().galley(at, word, colour);
                    if response.clicked() {
                        app.keys_sheet_open = false;
                    }
                });
        });
}

fn general_section(app: &mut GuiApp, ui: &mut Ui, palette: &Palette) {
    heading(ui, tr!("allgemein"), palette);
    let setting = app.config.ui.language.clone();
    let said = match crate::lang::Lang::of_key(&setting) {
        Some(lang) => lang.name().to_string(),
        None => tr!("system · {}", crate::lang::Lang::of_system().name()),
    };
    if row(ui, tr!("sprache"), &said, true, palette).clicked() {
        let next = match setting.as_str() {
            "de" => "en",
            "en" => "ja",
            "ja" => "",
            _ => "de",
        };
        app.config.ui.language = next.to_string();
        crate::lang::set(crate::lang::Lang::chosen(next));
        app.save_config();
    }
    let speed = app.config.ui.scroll_speed.clone();
    let said = match speed.as_str() {
        "langsam" => tr!("langsam"),
        "schnell" => tr!("schnell"),
        _ => tr!("normal"),
    };
    if row(ui, tr!("mausrad"), said, true, palette).clicked() {
        app.config.ui.scroll_speed = match speed.as_str() {
            "langsam" => "normal",
            "normal" => "schnell",
            _ => "langsam",
        }
        .to_string();
        app.save_config();
    }

    if row(ui, tr!("tasten"), tr!("alle zeigen"), true, palette).clicked() && app.menu_leaving.is_none() {
        app.menu_leaving = Some(ui.input(|i| i.time));
    }
    let mut open: Option<String> = None;
    let asking = app.update.as_mut().is_some_and(super::update::Check::tend);
    if asking {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
    }
    let answer = app.update.as_ref().and_then(|u| u.answer.clone());
    let said = match &answer {
        _ if asking => format!("{}{}", tr!("sehe nach"), ".".repeat(1 + (ui.input(|i| i.time) / 0.4) as usize % 3)),
        Some(super::update::Answer::Newer { version, .. }) => tr!("{} ist da — öffnen", version),
        Some(super::update::Answer::Current) => tr!("das ist die neueste").to_string(),
        Some(super::update::Answer::Unseen(_)) => tr!("nicht zu sehen").to_string(),
        None => tr!("nachsehen").to_string(),
    };
    if row(ui, tr!("neue version"), &said, true, palette).clicked() && !asking {
        match answer {
            Some(super::update::Answer::Newer { page, .. }) if !page.is_empty() => open = Some(page),
            _ => app.update = Some(super::update::Check::start()),
        }
    }
    if let Some(url) = open {
        if !crate::lastfm::open_in_browser(&url) {
            app.set_status(tr!("der browser liess sich nicht öffnen"));
        }
    }

    ui.add_space(row_height());
    heading(ui, tr!("funktionen"), palette);
    let music = app.config.ui.music;
    if toggle(ui, tr!("musik"), music, AN_AUS, palette).clicked() {
        app.set_music(!music);
    }
    let radio = app.config.ui.radio;
    if toggle(ui, "radio", radio, AN_AUS, palette).clicked() {
        app.set_radio(!radio);
    }
    let ipod = app.config.ui.ipod;
    if toggle(ui, "ipod", ipod, AN_AUS, palette).clicked() {
        app.config.ui.ipod = !ipod;
        app.save_config();
    }
    let podcasts = app.config.ui.podcasts;
    if toggle(ui, "podcasts", podcasts, AN_AUS, palette).clicked() {
        app.set_podcasts(!podcasts);
    }

    ui.add_space(row_height());
    let asked_id = egui::Id::unique("reset-settings-asked");
    let now = ui.input(|i| i.time);
    let asked = ui.data(|d| d.get_temp::<f64>(asked_id)).is_some_and(|at| now - at < RESET_WINDOW_SECS);
    let word = if asked { tr!("wirklich?") } else { "" };
    if row(ui, tr!("einstellungen zurücksetzen"), word, true, palette).clicked() {
        if asked {
            ui.data_mut(|d| d.remove_temp::<f64>(asked_id));
            app.reset_settings(ui.ctx());
        } else {
            ui.data_mut(|d| d.insert_temp(asked_id, now));
            ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(RESET_WINDOW_SECS));
        }
    }
    explanation(ui, tr!("setzt darstellung, spalten, bilder, schrift und die übrigen einstellungen des menüs zurück. musikordner, last.fm, ipod und die funktionen oben bleiben."), palette);
}

const RESET_WINDOW_SECS: f64 = 4.0;

fn podcasts_section(app: &mut GuiApp, ui: &mut Ui, palette: &Palette) {
    heading(ui, "podcasts", palette);
    if !app.config.ui.podcasts {
        return;
    }
    const BACKS: [u32; 4] = [10, 15, 30, 60];
    const AHEADS: [u32; 5] = [10, 15, 30, 45, 60];
    let back = app.config.ui.podcast_back;
    if row(ui, tr!("zurückspulen"), &format!("{back} s"), true, palette).clicked() {
        let at = BACKS.iter().position(|b| *b == back).map_or(0, |i| (i + 1) % BACKS.len());
        app.config.ui.podcast_back = BACKS[at];
        app.save_config();
    }
    let ahead = app.config.ui.podcast_ahead;
    if row(ui, tr!("vorspulen"), &format!("{ahead} s"), true, palette).clicked() {
        let at = AHEADS.iter().position(|a| *a == ahead).map_or(0, |i| (i + 1) % AHEADS.len());
        app.config.ui.podcast_ahead = AHEADS[at];
        app.save_config();
    }
    const KEEPS: [&str; 7] = ["gehört", "7", "30", "90", "180", "365", "immer"];
    let keep = app.config.ui.podcast_keep.clone();
    let said = match keep.as_str() {
        "7" => tr!("7 tagen"),
        "30" => tr!("1 monat"),
        "90" => tr!("3 monaten"),
        "180" => tr!("6 monaten"),
        "365" => tr!("1 jahr"),
        "immer" => tr!("nie"),
        _ => tr!("dem hören"),
    };
    if row(ui, tr!("geladene löschen nach"), said, true, palette).clicked() {
        let at = KEEPS.iter().position(|k| *k == keep).map_or(0, |i| (i + 1) % KEEPS.len());
        app.config.ui.podcast_keep = KEEPS[at].to_string();
        app.save_config();
        app.archive_episodes();
    }
    let shown = if app.config.ui.podcast_show_archived { tr!("ausgegraut") } else { tr!("ausgeblendet") };
    if row(ui, tr!("archivierte folgen"), shown, true, palette).clicked() {
        app.config.ui.podcast_show_archived = !app.config.ui.podcast_show_archived;
        app.save_config();
    }
    let forgets = if app.config.ui.podcast_archive_forgets { tr!("an") } else { tr!("aus") };
    if row(ui, tr!("archivierte dateien löschen"), forgets, true, palette).clicked() {
        app.config.ui.podcast_archive_forgets = !app.config.ui.podcast_archive_forgets;
        app.save_config();
        if app.config.ui.podcast_archive_forgets {
            app.forget_archived();
        }
    }
    const CHECKS: [&str; 5] = ["15", "30", "60", "180", "hand"];
    let check = app.config.ui.podcast_check.clone();
    let said = match check.as_str() {
        "15" => tr!("alle 15 minuten"),
        "60" => tr!("jede stunde"),
        "180" => tr!("alle 3 stunden"),
        "hand" => tr!("nur von hand"),
        _ => tr!("alle 30 minuten"),
    };
    if row(ui, tr!("nach neuen folgen sehen"), said, true, palette).clicked() {
        let at = CHECKS.iter().position(|c| *c == check).map_or(0, |i| (i + 1) % CHECKS.len());
        app.config.ui.podcast_check = CHECKS[at].to_string();
        app.save_config();
    }
    let now = if app.checking_for_episodes() { tr!("sehe nach…") } else { tr!("jetzt nachsehen") };
    if row(ui, "", now, true, palette).clicked() && !app.checking_for_episodes() {
        app.check_for_episodes();
    }
    explanation(ui, tr!("„archivieren“ nimmt eine folge aus den listen — ihre datei bleibt, außer „archivierte dateien löschen“ ist an. „datei löschen“ gibt nur den platz frei, die folge bleibt in der liste. „geladene löschen nach“ räumt dateien von selbst weg, ob archiviert oder nicht."), palette);
    let size = crate::podcast::cache_size();
    if size > 0 && row(ui, tr!("alle geladenen löschen"), &format!("{:.1} mb", size as f64 / 1_000_000.0), true, palette).clicked() {
        app.forget_all_episodes();
    }
    if row(ui, "", tr!("bilder neu laden"), true, palette).clicked() {
        app.reload_podcast_pictures();
    }
    if row(ui, "", tr!("podcasts importieren…"), true, palette).clicked() {
        app.import_podcasts();
    }
    if row(ui, "", tr!("podcasts exportieren…"), true, palette).clicked() {
        app.export_podcasts();
    }
}

static RADIO_ON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static IPOD_ON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
static PODCASTS_ON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub(in crate::gui) fn note_features(ui: &crate::config::UiConfig) {
    RADIO_ON.store(ui.radio, std::sync::atomic::Ordering::Relaxed);
    IPOD_ON.store(ui.ipod, std::sync::atomic::Ordering::Relaxed);
    PODCASTS_ON.store(ui.podcasts, std::sync::atomic::Ordering::Relaxed);
}

type Section = fn(&mut GuiApp, &mut Ui, &Palette);

pub const SECTION_COUNT: usize = SECTIONS.len();

const SECTIONS: &[Section] = &[
    library_section,
    fill_section,
    look_section,
    header_section,
    pictures_section,
    frugal_section,
    saver_section,
    artists_section,
    albums_section,
    tracks_section,
    ipod_section,
    playback_section,
    lastfm_section,
    menubar_section,
    radio_section,
    sides_section,
    general_section,
    podcasts_section,
    about_section,
];

const ABOUT: usize = 18;

const IPOD: usize = 10;

const MENUBAR: usize = 13;

const RADIO: usize = 14;

const SIDES: usize = 15;

const GENERAL: usize = 16;

const PODCASTS: usize = 17;

fn shown(at: usize) -> bool {
    match at {
        IPOD => IPOD_ON.load(std::sync::atomic::Ordering::Relaxed),
        RADIO => RADIO_ON.load(std::sync::atomic::Ordering::Relaxed),
        PODCASTS => PODCASTS_ON.load(std::sync::atomic::Ordering::Relaxed),
        MENUBAR => crate::media::tray_available(),
        _ => true,
    }
}

pub(in crate::gui) const TABS: &[(&str, &[usize])] = &[
    ("allgemein", &[GENERAL, 6, 5]),
    ("bibliothek", &[0, 1]),
    ("darstellung", &[2, 3, 4, MENUBAR]),
    ("spalten", &[7, 8, 9, SIDES]),
    ("wiedergabe", &[11, 12]),
    ("radio", &[RADIO]),
    ("podcasts", &[PODCASTS]),
    ("ipod", &[IPOD]),
    ("über", &[ABOUT]),
];

const TAB_LIST: f32 = 168.0;

const TAB_GAP: f32 = PADDING * 2.0;

pub(in crate::gui) fn tabs_shown() -> Vec<usize> {
    (0..TABS.len())
        .filter(|&t| TABS[t].1.iter().any(|&at| shown(at)))
        .collect()
}

pub(in crate::gui) fn step_tab(app: &mut GuiApp, by: isize) {
    let tabs = tabs_shown();
    if tabs.is_empty() {
        return;
    }
    let at = tabs.iter().position(|&t| t == app.menu_tab).unwrap_or(0) as isize;
    let n = tabs.len() as isize;
    app.menu_tab = tabs[((at + by) % n + n) as usize % tabs.len()];
}

fn keep_tab(app: &mut GuiApp, ui: &Ui) -> bool {
    let shown_id = egui::Id::unique("menu-tab-shown");
    let was: Option<usize> = ui.data(|d| d.get_temp(shown_id));
    let switched = was.is_some_and(|w| w != app.menu_tab);
    if switched {
        with_menu_keys(ui.ctx(), |keys| {
            if keys.cursor.is_some() {
                keys.cursor = Some(0);
                keys.follow = true;
            }
        });
    }
    ui.data_mut(|d| d.insert_temp(shown_id, app.menu_tab));
    let tabs = tabs_shown();
    if !tabs.contains(&app.menu_tab) {
        app.menu_tab = tabs.first().copied().unwrap_or(0);
    }
    switched
}

fn tab_list_height() -> f32 {
    row_height() * tabs_shown().len() as f32
}

fn tab(app: &mut GuiApp, ui: &mut Ui, palette: &Palette, t: usize, width: Option<f32>) {
    let on = t == app.menu_tab;
    let down_the_side = width.is_some();
    let arrow_room = if on && down_the_side {
        text::line(ui, "→", Role::Content, palette.ink).size().x + 5.0
    } else {
        0.0
    };
    let wide = width
        .unwrap_or_else(|| text::line(ui, crate::lang::t(TABS[t].0), Role::Content, palette.ink).size().x + arrow_room);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(wide, row_height()), Sense::click());
    let hovered = response.hovered();
    let keyboard = on && down_the_side && ui.data(|d| d.get_temp::<MenuKeys>(menu_keys_id())).is_some_and(|k| k.on_tabs);
    let colour = if on { palette.ink } else { palette.mid };
    let role = if on && down_the_side { Role::ContentBold } else { Role::Content };
    let g = text::line(ui, crate::lang::t(TABS[t].0), role, colour);
    if keyboard {
        ui.painter().rect_filled(plate(rect), super::theme::MARKER_RADIUS, palette.marked());
    } else if hovered {
        ui.painter().rect_filled(plate(rect), super::theme::MARKER_RADIUS, palette.hover);
    }
    let line = text::row_line(ui, row_height(), Role::Content);
    let lead = super::library::lead_arrow(ui, response.id, rect, line, on && down_the_side, colour);
    let at = egui::pos2(rect.left() + lead, text::on_line(&g, rect.top(), line));
    let word = g.size().x;
    let baseline = at.y + text::baseline(&g);
    ui.painter().galley(at, g, colour);
    if on && !down_the_side {
        super::library::paint_underline(ui, at.x, word, baseline, text::ink_height(ui, Role::Content), colour);
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if response.clicked() {
        app.menu_tab = t;
        if down_the_side {
            with_menu_keys(ui.ctx(), |keys| {
                keys.on_tabs = true;
                keys.cursor = None;
            });
        }
    }
}

fn tab_list(app: &mut GuiApp, ui: &mut Ui, palette: &Palette, height: f32) {
    ui.allocate_ui_with_layout(Vec2::new(TAB_LIST, height), egui::Layout::top_down(egui::Align::Min), |ui| {
        ui.set_width(TAB_LIST);
        ui.set_height(height);
        for t in tabs_shown() {
            tab(app, ui, palette, t, Some(TAB_LIST));
        }
    });
}

fn tab_bar(app: &mut GuiApp, ui: &mut Ui, palette: &Palette) {
    egui::Frame::NONE.inner_margin(egui::Margin::symmetric(HOVER_BLEED as i8, 0)).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(HOVER_BLEED * 4.0, 0.0);
            for t in tabs_shown() {
                tab(app, ui, palette, t, None);
            }
        });
    });
}

fn tab_content(app: &mut GuiApp, ui: &mut Ui, palette: &Palette, view: f32) -> f32 {
    let settling = ui.data(|d| d.get_temp::<bool>(egui::Id::unique(SETTLING))).unwrap_or(false);
    let forgotten = tabs_shown()
        .into_iter()
        .any(|t| TABS[t].1.iter().filter(|&&at| shown(at)).all(|&at| app.menu_heights[at] <= 0.0));
    if settling || forgotten {
        measure_hidden_tabs(app, ui, palette);
    }
    let tallest = (0..TABS.len())
        .map(|t| {
            let shown: Vec<f32> = TABS[t].1.iter().map(|&at| app.menu_heights[at]).filter(|h| *h > 0.0).collect();
            shown.iter().sum::<f32>() + row_height() * shown.len().saturating_sub(1) as f32
        })
        .fold(0.0, f32::max);
    let shown = tallest.min(view);
    ui.vertical(|ui| {
        ui.set_min_height(shown);
        let top = ui.cursor().top();
        run_tab(app, ui, palette, app.menu_tab);
        if TABS[app.menu_tab].1.contains(&ABOUT) {
            let below = ui.cursor().top() + row_height() * 2.0;
            let kept_id = egui::Id::unique("menu-drawing-height");
            if app.licenses_open {
                let kept: f32 = ui.data(|d| d.get_temp(kept_id)).unwrap_or(0.0);
                let room = Rect::from_min_max(
                    egui::pos2(ui.max_rect().left(), below),
                    egui::pos2(ui.max_rect().right(), below + kept),
                );
                drawing(ui, room, palette);
                ui.add_space(row_height() * 2.0 + kept);
            } else {
                let room = Rect::from_min_max(
                    egui::pos2(ui.max_rect().left(), below),
                    egui::pos2(ui.max_rect().right(), top + shown),
                );
                let drawn = drawing(ui, room, palette);
                ui.data_mut(|d| d.insert_temp(kept_id, drawn));
            }
        }
    });
    tallest
}

const DRAWING: &[u8] = include_bytes!("drawing.png");

fn drawing(ui: &Ui, room: Rect, palette: &Palette) -> f32 {
    static MASK: std::sync::OnceLock<image::GrayImage> = std::sync::OnceLock::new();
    ink_drawing(ui, room, palette, &MASK, DRAWING, "menu-drawing", 0.46)
}

const DRAWING_OFF: &[u8] = include_bytes!("drawing-off.png");

pub(in crate::gui) fn drawing_off(ui: &Ui, room: Rect, palette: &Palette) -> f32 {
    static MASK: std::sync::OnceLock<image::GrayImage> = std::sync::OnceLock::new();
    ink_drawing(ui, room, palette, &MASK, DRAWING_OFF, "all-off-drawing", 0.5)
}

fn ink_drawing(
    ui: &Ui,
    room: Rect,
    palette: &Palette,
    mask: &'static std::sync::OnceLock<image::GrayImage>,
    bytes: &[u8],
    key: &'static str,
    middle_at: f32,
) -> f32 {
    let colour = palette.accent_faint;
    let mask = mask.get_or_init(|| {
        image::load_from_memory(bytes).map(|picture| picture.to_luma8()).unwrap_or_default()
    });
    if mask.width() == 0 || room.width() <= 0.0 {
        return 0.0;
    }
    let aspect = mask.width() as f32 / mask.height() as f32;
    let height = room.height().min(room.width() / aspect);
    if height < 1.0 {
        return 0.0;
    }
    let ppp = ui.ctx().pixels_per_point();
    let pixels = [
        (height * aspect * ppp).round().max(1.0) as usize,
        (height * ppp).round().max(1.0) as usize,
    ];
    let size = Vec2::new(pixels[0] as f32, pixels[1] as f32) / ppp;
    let left = (room.center().x - size.x * middle_at).clamp(room.left(), (room.right() - size.x).max(room.left()));
    let corner = ((egui::vec2(left, room.max.y - size.y)) * ppp).round() / ppp;
    let rect = Rect::from_min_size(corner.to_pos2(), size);

    let id = egui::Id::unique(key);
    let kept: Option<([usize; 2], egui::TextureHandle)> = ui.data(|d| d.get_temp(id));
    let texture = match kept {
        Some((at, texture)) if at == pixels => texture,
        _ => {
            let small = image::imageops::resize(
                mask,
                pixels[0] as u32,
                pixels[1] as u32,
                image::imageops::FilterType::Lanczos3,
            );
            let image = egui::ColorImage {
                size: pixels,
                pixels: small.pixels().map(|ink| egui::Color32::from_white_alpha(ink.0[0])).collect(),
                source_size: size,
            };
            let texture = ui.ctx().load_texture(key, image, egui::TextureOptions::NEAREST);
            ui.data_mut(|d| d.insert_temp(id, (pixels, texture.clone())));
            texture
        }
    };
    ui.painter().image(
        texture.id(),
        rect,
        Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        colour,
    );
    size.y
}

const SETTLING: &str = "menu-settling";

fn measure_hidden_tabs(app: &mut GuiApp, ui: &mut Ui, palette: &Palette) {
    let kept: MenuKeys = ui.data(|d| d.get_temp(menu_keys_id())).unwrap_or_default();
    with_menu_keys(ui.ctx(), |keys| {
        keys.cursor = None;
        keys.press = Press::Nothing;
        keys.follow = false;
    });
    let width = ui.available_width();
    let away = Rect::from_min_size(egui::pos2(ui.max_rect().left(), -100_000.0), Vec2::new(width, 100_000.0));
    for t in tabs_shown() {
        if t == app.menu_tab {
            continue;
        }
        let mut hidden = ui.new_child(
            egui::UiBuilder::new()
                .id_salt(("menu-measure", t))
                .max_rect(away)
                .layout(egui::Layout::top_down(egui::Align::Min))
                .invisible(),
        );
        run_tab(app, &mut hidden, palette, t);
    }
    ui.data_mut(|d| d.insert_temp(menu_keys_id(), kept));
}

pub(in crate::gui) fn rebalance(app: &mut GuiApp) {
    app.menu_heights = [0.0; SECTION_COUNT];
}

thread_local! {
    static TAB_WORD: std::cell::Cell<&'static str> = const { std::cell::Cell::new("") };
}

fn run_tab(app: &mut GuiApp, ui: &mut Ui, palette: &Palette, t: usize) {
    TAB_WORD.with(|word| word.set(crate::lang::t(TABS[t].0)));
    run(app, ui, palette, TABS[t].1.iter().copied());
    TAB_WORD.with(|word| word.set(""));
}

fn run(app: &mut GuiApp, ui: &mut Ui, palette: &Palette, which: impl IntoIterator<Item = usize>) {
    let mut first = true;
    for at in which {
        if !shown(at) {
            continue;
        }
        if !first {
            ui.add_space(row_height());
        }
        first = false;
        let top = ui.cursor().top();
        SECTIONS[at](app, ui, palette);
        app.menu_heights[at] = ui.cursor().top() - top;
    }
}

fn library_section(app: &mut GuiApp, ui: &mut Ui, palette: &Palette) {
    heading(ui, tr!("bibliothek"), palette);

    music_folders(app, ui, palette);
    row(
        ui,
        tr!("titel"),
        &tr!("{} in {} alben",
            text::count(app.total_tracks as u64),
            text::count(app.total_albums as u64)
        ),
        false,
        palette,
    );

    let scanning = app.is_scanning();
    let rescan_label = match (scanning, app.scan_counts()) {
        (false, _) => tr!("jetzt neu einlesen").to_string(),
        (true, (_, 0)) => tr!("zähle dateien…").to_string(),
        (true, (done, total)) => format!(
            "{} / {}",
            text::count(done as u64),
            text::count(total as u64)
        ),
    };
    if row(ui, tr!("einlesen"), &rescan_label, !scanning, palette).clicked() {
        app.start_rescan();
    }

    set_in(ui, |ui| {
        if toggle(ui, "beim start einlesen", app.config.watch_dirs, AN_AUS, palette).clicked() {
            app.config.watch_dirs = !app.config.watch_dirs;
            app.save_config();
        }
    });

    let adding = app.is_adding();
    let add_label = match (adding, app.add_counts()) {
        (false, _) => tr!("hinzufügen…").to_string(),
        (true, (_, 0)) => tr!("sucht dateien…").to_string(),
        (true, (done, total)) => format!(
            "{} / {}",
            text::count(done as u64),
            text::count(total as u64)
        ),
    };
    if row(ui, tr!("musik"), &add_label, !adding, palette).clicked() {
        app.ask_for_music();
    }

    set_in(ui, |ui| {
        let how = if app.config.add_move {
            tr!("verschieben")
        } else {
            tr!("kopieren")
        };
        if row(ui, tr!("dabei"), how, true, palette).clicked() {
            app.config.add_move = !app.config.add_move;
            app.save_config();
        }

        if crate::library::import::music_app_available() {
            if toggle(ui, "und an music.app", app.config.add_to_music, ("ja", "nein"), palette).clicked() {
                app.config.add_to_music = !app.config.add_to_music;
                app.save_config();
            }
        }
    });

    artist_info_rows(app, ui, palette);
}

fn fill_section(app: &mut GuiApp, ui: &mut Ui, palette: &Palette) {
    heading(ui, tr!("idtags ergänzen"), palette);
    let scanning = app.is_scanning();

    if row(ui, tr!("cover"), tr!("in den dateien suchen"), !scanning, palette).clicked() {
        app.look_for_covers_again();
    }

    for what in [super::Hunted::Years, super::Hunted::Genres] {
        let running = app.tag_hunt.as_ref().map(|h| (h.what, h.asked, h.total));
        let (said, open) = match running {
            Some((now, asked, total)) if now == what => (
                tr!("{} / {} — anhalten",
                    text::count(asked as u64),
                    text::count(total as u64)
                ),
                true,
            ),
            Some((now, _, _)) => (tr!("wartet auf die {}", crate::lang::t(now.word())), false),
            None => (tr!("fehlende suchen…").to_string(), !scanning && app.artist_hunt.is_none()),
        };
        if row(ui, crate::lang::t(what.word()), &said, open, palette).clicked() && open {
            if running.is_some() {
                app.stop_tag_hunt();
            } else if let Err(e) = app.start_tag_hunt(what) {
                app.set_status(format!("{e}"));
            }
        }
    }
}

fn look_section(app: &mut GuiApp, ui: &mut Ui, palette: &Palette) {
    heading(ui, tr!("darstellung"), palette);

    let pending = egui::Id::unique("zoom-pending");
    let zoom = ui
        .data(|d| d.get_temp::<f32>(pending))
        .unwrap_or_else(|| zoom_of(app.config.ui.zoom));
    let said = if (zoom - 1.0).abs() < 0.001 {
        tr!("wie das system").to_string()
    } else {
        format!("{} %", (zoom * 100.0).round() as i32)
    };
    let (moved, settled) = slider(ui, tr!("grösse"), zoom_notch(zoom), &said, palette);
    if let Some(at) = moved {
        let step = ZOOMS[(at * (ZOOMS.len() - 1) as f32).round() as usize];
        ui.data_mut(|d| d.insert_temp(pending, step));
    }
    if settled {
        let chosen = ui.data_mut(|d| d.remove_temp::<f32>(pending)).unwrap_or(zoom);
        app.set_zoom(ui.ctx(), chosen);
        rebalance(app);
    }

    let open = app.theme_list_open;
    let chosen = crate::lang::t(app.theme.family());
    if row(ui, tr!("farben"), chosen, true, palette).clicked() {
        app.theme_list_open = !open;
        if open {
            rebalance(app);
        }
    }
    if open {
        theme_list(app, ui, palette);
    }
    if app.theme.family() == Theme::Eigenes.family() {
        own_scheme(app, ui, palette);
    }
    let brightness = app.brightness;
    let said = match brightness {
        Brightness::System => format!(
            "{} · {}",
            crate::lang::t(brightness.name()),
            crate::lang::t(if app.resolved.palette().is_dark() { "dunkel" } else { "hell" })
        ),
        fixed => crate::lang::t(fixed.name()).to_string(),
    };
    if row(ui, tr!("hell, dunkel"), &said, true, palette).clicked() {
        let ctx = ui.ctx().clone();
        app.apply_theme(&ctx, app.theme, brightness.next());
    }
    let all_columns = app.config.ui.all_columns;
    if toggle(ui, tr!("alle spalten nebeneinander"), all_columns, AN_AUS, palette).clicked() {
        app.config.ui.all_columns = !all_columns;
        app.save_config();
    }

    if app.families.len() > 1 {
        if let Some(name) = app.typeface.clone() {
            if row(ui, tr!("schrift"), &name, true, palette).clicked() {
                let back = ui.input(|i| i.modifiers.shift);
                app.next_typeface(ui.ctx(), back);
            }
        }
    }
    if let Some((least, most)) = super::theme::width_axis() {
        set_in(ui, |ui| font_width_row(app, ui, palette, least, most));
    }
    if let Some((least, most)) = super::theme::weight_axis() {
        set_in(ui, |ui| font_weight_row(app, ui, palette, least, most));
    }
    let sets: Vec<_> = app
        .style_sets()
        .into_iter()
        .filter(|s| !s.letter.is_some_and(|c| c.is_ascii_uppercase()))
        .filter(|s| s.letter.is_some())
        .collect();
    if !sets.is_empty() {
        set_in(ui, |ui| style_sets_rows(app, ui, palette, &sets));
    }
    let hinted = super::theme::Hinting::from_name(&app.config.ui.font_hinting);
    let said = match hinted {
        super::theme::Hinting::Full => tr!("voll"),
        super::theme::Hinting::Light => tr!("leicht"),
        super::theme::Hinting::Off => tr!("aus"),
    };
    if row(ui, tr!("hinting"), said, true, palette).clicked() {
        let all = super::theme::Hinting::ALL;
        let at = all.iter().position(|h| *h == hinted).map_or(0, |i| (i + 1) % all.len());
        app.config.ui.font_hinting = all[at].name().to_string();
        let typeface = app.config.ui.typeface.clone();
        app.set_typeface(ui.ctx(), &typeface);
        app.save_config();
    }
    let system = app.config.ui.system_fonts;
    if system || ui.input(|i| i.modifiers.alt) {
        let said = if system { tr!("an") } else { tr!("aus") };
        if row(ui, tr!("systemschriften"), said, true, palette).clicked() {
            app.set_system_fonts(ui.ctx(), !system);
        }
    }
    let capitals = app.config.ui.capitals;
    if capitals || ui.input(|i| i.modifiers.alt) {
        let said = if capitals { tr!("an") } else { tr!("aus") };
        if row(ui, tr!("grossbuchstaben"), said, true, palette).clicked() {
            app.config.ui.capitals = !capitals;
            crate::gui::theme::set_capitals(!capitals);
            app.save_config();
            ui.ctx().request_discard("capitals switched");
        }
    }
    if app.typeface.is_none() {
        let why = if app.font_source.has_bold() {
            tr!("fonts-ordner nicht gefunden")
        } else {
            tr!("fonts-ordner nicht gefunden — kein fetter schnitt")
        };
        row(ui, tr!("schrift"), why, false, palette);
    }
    if app.font_fallback.is_none() {
        row(ui, tr!("ersatzzeichen"), tr!("keine gefunden"), false, palette);
    }
}

fn header_section(app: &mut GuiApp, ui: &mut Ui, palette: &Palette) {
    heading(ui, tr!("kopfzeile"), palette);

    let place = super::library::PlayingAt::of(&app.config.ui);
    if row(ui, tr!("was läuft"), crate::lang::t(place.name()), true, palette).clicked() {
        place.next().store(&mut app.config.ui);
        app.save_config();
    }
    if place.compact() {
        set_in(ui, |ui| {
            let shown = app.config.ui.library_line;
            let said = if shown { tr!("unten links") } else { tr!("aus") };
            if row(ui, tr!("bibliothekszahlen"), said, true, palette).clicked() {
                app.config.ui.library_line = !shown;
                app.save_config();
            }
        });
    }
    let in_queue = app.config.ui.modes_in_queue;
    let said = if in_queue { tr!("über der warteschlange") } else { tr!("in der kopfzeile") };
    if row(ui, tr!("zufall · wiederholen"), said, true, palette).clicked() {
        app.config.ui.modes_in_queue = !in_queue;
        app.save_config();
    }
    let strip = app.config.ui.field_spectrum;
    let picture = super::library::FieldPicture::from_name(&app.config.ui.field_picture);
    let said = if strip { picture.said() } else { tr!("nichts") };
    if row(ui, tr!("im suchfeld"), said, true, palette).clicked() {
        let next = if strip { picture.next() } else { Some(super::library::FieldPicture::Scope) };
        app.config.ui.field_spectrum = next.is_some();
        if let Some(next) = next {
            app.config.ui.field_picture = next.name().to_string();
        }
        app.save_config();
    }
    set_in(ui, |ui| {
        let scope = super::library::FieldPicture::from_name(&app.config.ui.field_picture) == super::library::FieldPicture::Scope;
        if app.config.ui.field_spectrum && scope {
            let rate = app.config.ui.scope_rate;
            let exposed = super::library::EXPOSED_RATE;
            let second = super::library::SECOND_RATE;
            let said = if rate == exposed {
                tr!("{} bilder/s, belichtet", exposed)
            } else if rate == second {
                tr!("1 bild/s, mit der uhr").to_string()
            } else {
                tr!("{} bilder/s", 1000 / super::library::scope_step(rate))
            };
            if row(ui, tr!("bildrate"), &said, true, palette).clicked() {
                app.config.ui.scope_rate = match rate {
                    12 => 8,
                    8 => exposed,
                    r if r == exposed => second,
                    r if r == second => 20,
                    _ => 12,
                };
                app.save_config();
            }
        }
        let quiet = super::library::FieldPicture::from_name(&app.config.ui.field_picture) != super::library::FieldPicture::Scope;
        if app.config.ui.field_spectrum && quiet {
            let rate = super::library::field_rate(app.config.ui.field_rate);
            let said = tr!("{} schritte/s", rate);
            if row(ui, tr!("bildrate"), &said, true, palette).clicked() {
                let rates = super::library::FIELD_RATES;
                let at = rates.iter().position(|&r| r == rate).map_or(2, |i| (i + 1) % rates.len());
                app.config.ui.field_rate = rates[at];
                app.save_config();
            }
        }
    });
}

fn radio_section(app: &mut GuiApp, ui: &mut Ui, palette: &Palette) {
    heading(ui, tr!("radio"), palette);
    if !app.config.ui.radio {
        return;
    }
    let logos = app.config.ui.radio_logos;
    if toggle(ui, tr!("logos der stationen laden"), logos, AN_AUS, palette).clicked() {
        app.config.ui.radio_logos = !logos;
        app.save_config();
    }
    let found = app.config.ui.radio_found.clone();
    let said = match found.as_str() {
        "spalten" => tr!("in spalten"),
        "aus" => tr!("aus"),
        _ => tr!("rechts daneben"),
    };
    if row(ui, tr!("angaben bei suchtreffern"), said, true, palette).clicked() {
        app.config.ui.radio_found = match found.as_str() {
            "spalten" => "aus",
            "aus" => "rechts",
            _ => "spalten",
        }
        .to_string();
        app.save_config();
    }
    if row(ui, tr!("gemerkte stationen"), tr!("als m3u speichern"), true, palette).clicked() {
        app.export_stations();
    }
    set_in(ui, |ui| {
        if row(ui, tr!("stationen aus datei"), tr!("hinzufügen…"), true, palette).clicked() {
            app.import_stations();
        }
    });
    let held = ui.input(|i| i.modifiers.alt);
    if held && row(ui, tr!("beispielsender"), tr!("hinzufügen"), true, palette).clicked() {
        crate::radio::offer_again();
        if app.radio.is_some() {
            app.radio = Some(Box::new(crate::radio::Radio::open()));
        }
        app.set_status(tr!("radio: beispielsender hinzugefügt"));
    }
    if app.scrobbler.is_some() {
        let scrobble = app.config.ui.radio_scrobble;
        if toggle(ui, tr!("zu last.fm scrobbeln"), scrobble, AN_AUS, palette).clicked() {
            app.config.ui.radio_scrobble = !scrobble;
            app.save_config();
        }
        let love = app.config.ui.radio_love;
        if toggle(ui, tr!("gemerkte lieder bei last.fm lieben"), love, AN_AUS, palette).clicked() {
            app.config.ui.radio_love = !love;
            app.save_config();
        }
    }
    row(ui, tr!("verzeichnis"), "radio-browser.info", false, palette);
}

fn menubar_section(app: &mut GuiApp, ui: &mut Ui, palette: &Palette) {
    let named = if cfg!(target_os = "linux") { tr!("systemleiste") } else { tr!("menüleiste") };
    heading(ui, named, palette);
    let mode = super::menubar::Mode::from_name(&app.config.ui.menubar);
    if toggle(ui, "anzeige", mode != super::menubar::Mode::Off, AN_AUS, palette).clicked() {
        app.config.ui.menubar = mode.next().name().to_string();
        app.save_config();
    }
    if cfg!(target_os = "linux") && mode != super::menubar::Mode::Off {
        let ink = app.config.ui.menubar_ink.clone();
        let said = match ink.as_str() {
            "hell" => tr!("hell"),
            "dunkel" => tr!("dunkel"),
            _ => tr!("wie das system"),
        };
        if row(ui, tr!("zeichenfarbe"), said, true, palette).clicked() {
            let next = match ink.as_str() {
                "hell" => "dunkel",
                "dunkel" => "system",
                _ => "hell",
            };
            app.config.ui.menubar_ink = next.to_string();
            app.save_config();
        }
    }
    if cfg!(target_os = "macos") && mode != super::menubar::Mode::Off {
        let cover = super::menubar::CoverAt::from_name(&app.config.ui.menubar_cover);
        if row(ui, tr!("cover im menü"), cover.said(), true, palette).clicked() {
            app.config.ui.menubar_cover = cover.next().name().to_string();
            app.save_config();
        }
        if cover != super::menubar::CoverAt::Off {
            set_in(ui, |ui| {
                let screened = app.config.ui.menubar_cover_screened;
                if toggle(ui, tr!("gerastert"), screened, AN_AUS, palette).clicked() {
                    app.config.ui.menubar_cover_screened = !screened;
                    app.save_config();
                }
            });
        }
    }
}

fn frugal_section(app: &mut GuiApp, ui: &mut Ui, palette: &Palette) {
    heading(ui, tr!("sparsam"), palette);

    let cap = app.config.ui.frame_cap;
    let said = if cap == 0 { tr!("wie der bildschirm").to_string() } else { tr!("höchstens {} bilder/s", cap) };
    if row(ui, tr!("bildrate"), &said, true, palette).clicked() {
        app.config.ui.frame_cap = match cap {
            60 => 30,
            30 => 0,
            _ => 60,
        };
        app.save_config();
    }
}

fn font_width_row(app: &mut GuiApp, ui: &mut Ui, palette: &Palette, least: f32, most: f32) {
    let pending = egui::Id::unique("font-width-pending");
    let width = ui
        .data(|d| d.get_temp::<f32>(pending))
        .unwrap_or(app.config.ui.font_width)
        .clamp(least, most);
    let said = if (width - 100.0).abs() < 0.5 {
        tr!("wie gezeichnet").to_string()
    } else {
        format!("{} %", width.round() as i32)
    };
    let (moved, settled) = slider(ui, tr!("breite"), (width - least) / (most - least), &said, palette);
    if let Some(at) = moved {
        let chosen = (least + at * (most - least)).round();
        ui.data_mut(|d| d.insert_temp(pending, chosen));
    }
    if settled {
        let chosen = ui.data_mut(|d| d.remove_temp::<f32>(pending)).unwrap_or(width);
        app.set_font_width(ui.ctx(), chosen);
        rebalance(app);
    }
}

fn style_sets_rows(app: &mut GuiApp, ui: &mut Ui, palette: &Palette, sets: &[super::theme::StyleSet]) {
    let on = app.config.ui.sets_of(super::theme::face_key(&app.config.ui.typeface));
    let unfolded = app.sets_open;
    let chosen: Vec<String> = sets.iter().filter(|s| on.contains(&s.id)).map(set_letter).collect();
    let said = if unfolded { tr!("schliessen").to_string() } else if chosen.is_empty() { tr!("keine").to_string() } else { chosen.join(", ") };
    if row(ui, tr!("alternativen"), &said, true, palette).clicked() {
        app.sets_open = !unfolded;
        rebalance(app);
    }
    if !unfolded {
        return;
    }
    let mut flip: Option<String> = None;
    for set in sets {
        if choice(ui, &set_letter(set), &set.tag, on.contains(&set.id), palette).clicked() {
            flip = Some(set.id.clone());
        }
    }
    if let Some(id) = flip {
        app.toggle_style_set(ui.ctx(), &id);
        rebalance(app);
    }
}

fn set_letter(set: &super::theme::StyleSet) -> String {
    match set.letter {
        Some(c) if c.is_ascii_uppercase() => format!("{} {}", tr!("gross|buchstaben"), c.to_ascii_lowercase()),
        Some(c) if c.is_ascii_lowercase() => format!("{} {}", tr!("klein|buchstaben"), c),
        Some(c) => c.to_string(),
        None if set.id.ends_with(":*") => {
            let mut first: Vec<String> = Vec::new();
            for c in set.shows.iter().filter(|c| c.is_ascii_lowercase()) {
                if first.len() == 5 {
                    first.push("…".into());
                    break;
                }
                first.push(c.to_string());
            }
            first.join(" ")
        }
        None => set.shows.iter().take(4).map(|c| c.to_string()).collect::<Vec<_>>().join(" "),
    }
}

fn font_weight_row(app: &mut GuiApp, ui: &mut Ui, palette: &Palette, least: f32, most: f32) {
    let pending = egui::Id::unique("font-weight-pending");
    let weight = ui
        .data(|d| d.get_temp::<f32>(pending))
        .unwrap_or(app.config.ui.font_weight)
        .clamp(least, most);
    let said = if (weight - 400.0).abs() < 0.5 {
        tr!("wie gezeichnet").to_string()
    } else {
        format!("{}", weight.round() as i32)
    };
    let (moved, settled) = slider(ui, tr!("gewicht"), (weight - least) / (most - least).max(1.0), &said, palette);
    if let Some(at) = moved {
        let chosen = ((least + at * (most - least)) / 10.0).round() * 10.0;
        ui.data_mut(|d| d.insert_temp(pending, chosen));
    }
    if settled {
        let chosen = ui.data_mut(|d| d.remove_temp::<f32>(pending)).unwrap_or(weight);
        app.set_font_weight(ui.ctx(), chosen);
        rebalance(app);
    }
}

fn artists_section(app: &mut GuiApp, ui: &mut Ui, palette: &Palette) {
    heading(ui, tr!("künstler:innen"), palette);

    if toggle(ui, "alle-zeile", app.config.ui.show_all_artists, ("sichtbar", "versteckt"), palette)
        .clicked()
    {
        app.config.ui.show_all_artists = !app.config.ui.show_all_artists;
        app.save_config();
    }

    let recent = app.config.ui.show_recent;
    if toggle(ui, tr!("zuletzt-zeile"), recent, ("sichtbar", "versteckt"), palette).clicked() {
        app.config.ui.show_recent = !recent;
        app.save_config();
        if recent && app.showing_recent() {
            app.artist_index = app.home_row();
            if let Err(e) = app.reload_dependent() {
                app.set_status(format!("{e}"));
            }
        }
    }

    if recent {
        let on_recent = app.config.ui.start_on_recent;
        let said = if on_recent { tr!("zuletzt") } else { tr!("alle") };
        if row(ui, tr!("beim öffnen zeigen"), said, true, palette).clicked() {
            app.config.ui.start_on_recent = !on_recent;
            app.save_config();
        }
    }

    if toggle(ui, "kompilationen-zeile", app.config.ui.show_compilations, ("sichtbar", "versteckt"), palette)
        .clicked()
    {
        app.config.ui.show_compilations = !app.config.ui.show_compilations;
        app.save_config();
        if !app.config.ui.show_compilations && app.showing_compilations() {
            app.artist_index = app.home_row();
        }
        if let Err(e) = app.reload_all() {
            app.set_status(format!("{e}"));
        }
    }

    let counts = app.config.ui.artist_counts;
    if toggle(ui, "titelzahl", counts, AN_AUS, palette).clicked() {
        app.config.ui.artist_counts = !counts;
        app.save_config();
    }
    let total = app.config.ui.artist_total;
    if toggle(ui, tr!("anzahl oben rechts"), total, AN_AUS, palette).clicked() {
        app.config.ui.artist_total = !total;
        app.save_config();
    }

}

fn albums_section(app: &mut GuiApp, ui: &mut Ui, palette: &Palette) {
    heading(ui, tr!("alben"), palette);

    let merged = app.merged();
    let view = if merged { tr!("mit titelliste kombiniert") } else { tr!("eigene spalte") };
    if row(ui, tr!("ansicht"), view, true, palette).clicked() {
        let done = app.set_albums_in_tracks(!merged);
        if let Err(e) = done {
            app.set_status(format!("{e}"));
        }
    }

    set_in(ui, |ui| view_rows(app, ui, palette));
    if merged {
        set_in(ui, |ui| track_column_rows(app, ui, palette));
    }

    let whole = app.config.ui.compilations_whole;
    let said = if whole { tr!("ganzes album") } else { tr!("nur eigene titel") };
    if row(ui, tr!("sampler"), said, true, palette).clicked() {
        app.config.ui.compilations_whole = !whole;
        app.save_config();
        if let Err(e) = app.reload_dependent() {
            app.set_status(format!("{e}"));
        }
    }
}

fn view_rows(app: &mut GuiApp, ui: &mut Ui, palette: &Palette) {
    if app.merged() {
        heading_cover_rows(app, ui, palette);
    }
}

fn tracks_section(app: &mut GuiApp, ui: &mut Ui, palette: &Palette) {
    if app.merged() {
        return;
    }
    heading(ui, tr!("titelliste"), palette);
    track_column_rows(app, ui, palette);
}

fn track_column_rows(app: &mut GuiApp, ui: &mut Ui, palette: &Palette) {
    let merged = app.merged();
    for column in TrackColumn::ALL {
        let on = match column {
            TrackColumn::Album if merged => continue,
            TrackColumn::Number if merged => app.config.ui.merged_number,
            TrackColumn::Length if merged => app.config.ui.merged_length,
            _ => app.columns.contains(column),
        };
        let clicked = toggle(ui, column.name(), on, AN_AUS, palette).clicked();
        if column == TrackColumn::Number && on {
            set_in(ui, |ui| {
                let right = app.config.ui.numbers_right;
                let said = if right { tr!("rechtsbündig") } else { tr!("linksbündig") };
                if row(ui, tr!("nummern"), said, true, palette).clicked() {
                    app.config.ui.numbers_right = !right;
                    app.save_config();
                }
            });
        }
        if column == TrackColumn::Length && on {
            set_in(ui, |ui| {
                let small = app.config.ui.small_length;
                let said = if small { tr!("klein") } else { tr!("gross") };
                if row(ui, tr!("schriftgrösse"), said, true, palette).clicked() {
                    app.config.ui.small_length = !small;
                    app.save_config();
                }
            });
        }
        if clicked {
            match column {
                TrackColumn::Number if merged => {
                    app.config.ui.merged_number = !on;
                    app.save_config();
                }
                TrackColumn::Length if merged => {
                    app.config.ui.merged_length = !on;
                    app.save_config();
                }
                _ => {
                    if let Err(e) = app.toggle_column(column) {
                        app.set_status(format!("{e}"));
                    }
                }
            }
        }
    }
}

fn ipod_section(app: &mut GuiApp, ui: &mut Ui, palette: &Palette) {
    let refusal = app.attached.as_ref().map(|a| a.refusal.clone());
    if refusal.is_none() {
        heading(ui, "ipod", palette);
        row(ui, tr!("gerät"), tr!("nicht angeschlossen"), false, palette);
        connect_row(app, ui, palette);
        plays_row(app, ui, palette);
        convert_row(app, ui, palette);
        podcasts_row(app, ui, palette);
        mark_row(app, ui, palette);
        mount_row(app, ui, palette);
        return;
    }
    if let Some(refusal) = refusal {
        heading(ui, "ipod", palette);

        match refusal {
            Some(why) => {
                row(ui, tr!("schreiben"), &why, false, palette);
            }
            None => {
                if row(ui, tr!("überspielen"), tr!("ganze bibliothek…"), true, palette).clicked() {
                    if let Err(e) = app.transfer_library() {
                        app.set_status(format!("{e}"));
                    }
                }
                set_in(ui, |ui| convert_row(app, ui, palette));
                set_in(ui, |ui| podcasts_row(app, ui, palette));
                connect_row(app, ui, palette);
                mark_row(app, ui, palette);
                mount_row(app, ui, palette);
                plays_row(app, ui, palette);
                let said = app.backup_said.clone();
                if row(
                    ui,
                    tr!("datenbank"),
                    said.as_deref().unwrap_or(tr!("sichern")),
                    true,
                    palette,
                )
                .clicked()
                {
                    app.backup_device();
                }
            }
        }

        if row(ui, tr!("auswerfen"), tr!("jetzt"), true, palette).clicked() {
            app.menu_open = false;
            match app.eject_device() {
                Ok(name) => app.set_status(tr!("{} kann abgezogen werden", name)),
                Err(e) => app.set_status(format!("{e}")),
            }
        }
    }
}

fn connect_row(app: &mut GuiApp, ui: &mut Ui, palette: &Palette) {
    const STEPS: [&str; 3] = ["nichts", "fragen", "abgleichen"];
    let now = STEPS.iter().position(|s| *s == app.config.ipod_on_connect).unwrap_or(0);
    let lit = now > 0;
    let said = if STEPS[now] == "abgleichen" { tr!("synchronisieren") } else { crate::lang::t(STEPS[now]) };
    if line(ui, tr!("beim anschliessen"), said, Line::Setting, palette, None, lit).clicked() {
        let back = ui.input(|i| i.modifiers.shift);
        let next = if back { (now + STEPS.len() - 1) % STEPS.len() } else { (now + 1) % STEPS.len() };
        app.config.ipod_on_connect = STEPS[next].to_string();
        app.save_config();
    }
}

fn plays_row(app: &mut GuiApp, ui: &mut Ui, palette: &Palette) {
    let on = app.config.ipod_take_plays;
    let said = match (on, app.scrobbler.is_some()) {
        (false, _) => tr!("aus"),
        (true, true) => tr!("in bibliothek und last.fm"),
        (true, false) => tr!("in die bibliothek"),
    };
    if line(ui, tr!("gespielt & bewertet"), said, Line::Setting, palette, None, on).clicked() {
        app.config.ipod_take_plays = !on;
        app.save_config();
    }
}

fn convert_row(app: &mut GuiApp, ui: &mut Ui, palette: &Palette) {
    if crate::ipod::sync::transcoder_available() {
        let on = app.config.ipod_convert;
        if toggle(ui, tr!("flac umwandeln"), on, AN_AUS, palette).clicked() {
            app.config.ipod_convert = !on;
            app.save_config();
            app.device_summary = None;
        }
    } else {
        row(ui, tr!("flac umwandeln"), tr!("bräuchte ffmpeg"), false, palette);
    }
}

fn podcasts_row(app: &mut GuiApp, ui: &mut Ui, palette: &Palette) {
    if !app.config.ui.podcasts {
        return;
    }
    let on = app.config.ipod_podcasts;
    if toggle(ui, tr!("podcasts als playlist (experiment)"), on, AN_AUS, palette).clicked() {
        app.config.ipod_podcasts = !on;
        app.save_config();
        app.device_summary = None;
    }
}

fn mount_row(app: &mut GuiApp, ui: &mut Ui, palette: &Palette) {
    if !cfg!(target_os = "linux") {
        return;
    }
    let on = app.config.ipod_mount;
    if toggle(ui, tr!("ipod selbst einhängen"), on, AN_AUS, palette).clicked() {
        app.config.ipod_mount = !on;
        app.save_config();
    }
}

fn mark_row(app: &mut GuiApp, ui: &mut Ui, palette: &Palette) {
    let marks = super::library::DEVICE_MARKS;
    let mark = app.config.ui.device_mark.clone();
    let at = marks.iter().position(|n| *n == mark).unwrap_or(0);
    let said = match marks[at] {
        "wort" => tr!("wort „ipod“"),
        "punkt" => tr!("punkt"),
        _ => tr!("aus"),
    };
    if row(ui, tr!("markierung in der bibliothek"), said, true, palette).clicked() {
        app.config.ui.device_mark = marks[(at + 1) % marks.len()].to_string();
        app.save_config();
    }
}

fn playback_section(app: &mut GuiApp, ui: &mut Ui, palette: &Palette) {
    heading(ui, tr!("wiedergabe"), palette);

    let gapless = app.player.is_gapless();
    if toggle(ui, "lückenlos", gapless, AN_AUS, palette).clicked() {
        app.player.set_gapless(!gapless);
        app.config.player.gapless = !gapless;
        app.save_config();
    }
    if cfg!(target_os = "linux") {
        let on = app.config.ui.notify;
        if toggle(ui, tr!("benachrichtigung"), on, AN_AUS, palette).clicked() {
            app.config.ui.notify = !on;
            app.save_config();
        }
    }

    let level = crate::player::leveling::Level::from_name(&app.config.player.leveling);
    let on = level != crate::player::leveling::Level::Off;
    if line(ui, tr!("lautstärke angleichen"), crate::lang::t(level.name()), Line::Setting, palette, None, on)
        .clicked()
    {
        app.cycle_leveling();
    }
    if on {
        let running = app.measuring.as_ref().map(|m| (m.done, m.total));
        let (known, all) = app.loudness_count;
        let (said, open) = match running {
            Some((done, total)) => (
                tr!("{} / {} — anhalten", text::count(done as u64), text::count(total as u64)),
                true,
            ),
            None if known >= all => (tr!("alle {} gemessen", text::count(all as u64)), false),
            None => (
                tr!("{} von {} — messen…", text::count(known as u64), text::count(all as u64)),
                true,
            ),
        };
        if row(ui, "", &said, open, palette).clicked() && open {
            if running.is_some() {
                app.stop_measuring();
            } else {
                app.start_measuring();
            }
        }
    }

    if app.player.audio_unavailable_reason().is_some() {
        row(ui, tr!("audio"), tr!("kein ausgabegerät"), false, palette);
    }

}

fn lastfm_section(app: &mut GuiApp, ui: &mut Ui, palette: &Palette) {
    heading(ui, "last.fm", palette);

    let account: Option<(String, usize)> = app.scrobbler.as_ref().map(|scrobbler| {
        let who = if scrobbler.user().is_empty() {
            tr!("verbunden").to_string()
        } else {
            scrobbler.user().to_string()
        };
        (who, scrobbler.waiting())
    });

    match account {
        Some((who, waiting)) => {
            row(ui, tr!("konto"), &who, false, palette);
            if row(ui, "", tr!("profil im browser…"), true, palette).clicked() {
                app.open_lastfm_profile();
            }
            if row(ui, "", tr!("anderes konto verbinden…"), true, palette).clicked() {
                app.switch_lastfm();
            }
            if row(ui, "", tr!("abmelden"), true, palette).clicked() {
                app.disconnect_lastfm();
            }

            let on = app.config.lastfm.now_playing;
            if toggle(ui, "„läuft gerade“ senden", on, AN_AUS, palette).clicked() {
                app.config.lastfm.now_playing = !on;
                app.save_config();
                app.set_status(tr!("wirkt beim nächsten start"));
            }

            if waiting > 0 {
                row(
                    ui,
                    tr!("wartend"),
                    &tr!("{} noch nicht gesendet", text::count(waiting as u64)),
                    false,
                    palette,
                );
            }
        }
        None => {
            if app.lastfm_keys.is_some() {
                key_fields(app, ui, palette);
                return;
            }
            let ready = app.config.lastfm.api_key.is_some()
                && app.config.lastfm.api_secret.is_some();
            match (ready, app.is_connecting()) {
                (_, true) => {
                    row(ui, tr!("konto"), tr!("warte auf bestätigung…"), false, palette);
                    row(ui, "", tr!("im browser auf last.fm zustimmen"), false, palette);
                }
                (true, false) => {
                    if row(ui, tr!("konto"), tr!("verbinden…"), true, palette).clicked() {
                        app.connect_lastfm();
                    }
                    set_in(ui, |ui| {
                        if row(ui, tr!("schlüssel"), tr!("ändern…"), true, palette).clicked() {
                            app.open_lastfm_keys();
                        }
                    });
                }
                (false, false) => {
                    if row(ui, tr!("konto"), tr!("verbinden…"), true, palette).clicked() {
                        app.open_lastfm_keys();
                    }
                }
            }
        }
    }
}

fn key_fields(app: &mut GuiApp, ui: &mut Ui, palette: &Palette) {
    use super::lastfmkeys::LABELS;
    let mut focus_on = None;
    let (mut save, mut cancel) = (false, false);
    {
        let Some(form) = app.lastfm_keys.as_ref() else {
            return;
        };
        for (i, label) in LABELS.iter().enumerate() {
            let focused = form.focus == i;
            let masked = i == 1 && !focused;
            if entry_field(ui, crate::lang::t(label), &form.entries[i], focused, masked, palette).clicked() {
                focus_on = Some(i);
            }
        }
        let paste = if cfg!(target_os = "macos") { "⌘v" } else { tr!("strg+v") };
        row(ui, "", &tr!("einfügen mit {} · tab wechselt", paste), false, palette);
        if row(ui, "", tr!("bei last.fm beantragen…"), true, palette).clicked() {
            crate::lastfm::open_in_browser(super::lastfmkeys::APPLY_URL);
        }
        if row(ui, "", tr!("speichern und verbinden"), true, palette).clicked() {
            save = true;
        }
        if row(ui, "", tr!("abbrechen"), true, palette).clicked() {
            cancel = true;
        }
    }
    if let Some(i) = focus_on {
        if let Some(form) = app.lastfm_keys.as_mut() {
            form.focus = i;
            form.entries[i].end();
        }
    }
    if save {
        app.save_lastfm_keys();
    }
    if cancel {
        app.lastfm_keys = None;
    }
}

fn entry_field(
    ui: &mut Ui,
    label: &str,
    entry: &super::tagedit::Entry,
    focused: bool,
    masked: bool,
    palette: &Palette,
) -> egui::Response {
    let blink = ui.input(|i| i.time).rem_euclid(1.06) < 0.62;
    if focused {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(120));
    }
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), row_height() * 2.0), Sense::click());
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
    }
    if focused {
        ui.painter()
            .rect_filled(rect, super::theme::MARKER_RADIUS, palette.marked());
    }
    let written_on = text::row_line(ui, row_height(), Role::Content);
    let label_colour = if focused { palette.ink } else { palette.mid };
    let label_g = text::line(ui, label, Role::Content, label_colour);
    ui.painter().galley(
        egui::pos2(rect.left(), text::on_line(&label_g, rect.top(), written_on)),
        label_g,
        label_colour,
    );

    let shown: String = if masked {
        "•".repeat(entry.text().chars().count().min(32))
    } else {
        entry.text().to_string()
    };
    let (value, colour) = if shown.is_empty() && !focused {
        (tr!("leer").to_string(), palette.faint)
    } else {
        (shown, palette.ink)
    };
    let left = rect.left() + INDENT;
    let room = (rect.right() - left - CARET_ROOM).max(1.0);
    let second = rect.top() + row_height();
    let marked = focused && entry.is_marked();
    let value_g = text::cased_within(ui, &value, Role::Content, colour, room);
    let at = egui::pos2(left, text::on_line(&value_g, second, written_on));
    if marked {
        let behind = Rect::from_min_size(
            egui::pos2(left - 2.0, second + 3.0),
            Vec2::new(value_g.size().x + 4.0, row_height() - 6.0),
        );
        ui.painter().rect_filled(behind, 2.0, palette.hover);
    }
    ui.painter().galley(at, value_g, colour);
    if focused && blink && !marked {
        let before = &entry.text()[..entry.caret()];
        let run = text::cased_within(ui, before, Role::Content, palette.ink, f32::INFINITY)
            .size()
            .x;
        let bar = Rect::from_min_size(
            egui::pos2((left + run + 1.0).min(rect.right() - CARET_ROOM), second + 5.0),
            Vec2::new(1.5, row_height() - 10.0),
        );
        ui.painter().rect_filled(bar, 0.0, palette.ink);
    }
    response
}

const CARET_ROOM: f32 = 3.0;

fn artist_info_rows(app: &mut GuiApp, ui: &mut Ui, palette: &Palette) {
    let running = app.artist_hunt.as_ref().map(|h| (h.asked, h.total, h.again));
    artist_hunt_row(app, ui, palette, running, false);
    set_in(ui, |ui| {
        artist_hunt_row(app, ui, palette, running, true);
        if row(ui, tr!("exportieren"), tr!("als markdown…"), app.exporting_artists.is_none(), palette).clicked() {
            app.export_artists();
        }
    });
}

fn artist_hunt_row(
    app: &mut GuiApp,
    ui: &mut Ui,
    palette: &Palette,
    running: Option<(usize, usize, bool)>,
    again: bool,
) {
    let (label, idle) = if again {
        (tr!("aktualisieren"), tr!("alle neu suchen…"))
    } else {
        (tr!("über künstler:innen"), tr!("fehlende suchen…"))
    };
    let (said, open) = match running {
        Some((asked, total, which)) if which == again => (
            tr!("{} / {} — anhalten", text::count(asked as u64), text::count(total as u64)),
            true,
        ),
        Some(_) => (idle.to_string(), false),
        None => (idle.to_string(), app.tag_hunt.is_none()),
    };
    if row(ui, label, &said, open, palette).clicked() && open {
        if running.is_some() {
            app.stop_artist_hunt();
        } else if let Err(e) = app.start_artist_hunt(again) {
            app.set_status(format!("{e}"));
        }
    }
}

const LICENSE: (&str, &str) = ("gpl-3.0", "https://www.gnu.org/licenses/gpl-3.0.html");
const GITHUB: (&str, &str) = ("radiorauschen/kessler", "https://github.com/radiorauschen/kessler");
type Part = fn(&mut crate::config::UiConfig) -> &mut bool;

fn sides_section(app: &mut GuiApp, ui: &mut Ui, palette: &Palette) {
    heading(ui, tr!("informationen"), palette);
    let parts: [(&str, Part); 6] = [
        ("bild", |ui| &mut ui.info_picture),
        ("steckbrief", |ui| &mut ui.info_facts),
        ("wikipedia", |ui| &mut ui.info_wikipedia),
        ("bibliothek und radio", |ui| &mut ui.info_library),
        ("mitglieder, gruppen", |ui| &mut ui.info_ties),
        ("ähnliche künstler:innen", |ui| &mut ui.info_similar),
    ];
    for (label, field) in parts {
        let on = *field(&mut app.config.ui);
        if toggle(ui, label, on, ("zeigen", "aus"), palette).clicked() {
            *field(&mut app.config.ui) = !on;
            app.save_config();
        }
        if label == "bild" && on {
            let wide = app.config.ui.info_picture_wide;
            if toggle(ui, "bildbreite", wide, ("spaltenbreite", "fest"), palette).clicked() {
                app.config.ui.info_picture_wide = !wide;
                app.save_config();
            }
        }
    }

    ui.add_space(row_height());
    heading(ui, tr!("liedtext"), palette);
    let small = app.config.ui.lyrics_size == "klein";
    if row(ui, tr!("schrift"), if small { tr!("klein") } else { tr!("normal") }, true, palette).clicked() {
        app.config.ui.lyrics_size = if small { "normal" } else { "klein" }.into();
        app.save_config();
    }
    let follow = app.config.ui.lyrics_follow;
    if toggle(ui, "mitlaufen", follow, AN_AUS, palette).clicked() {
        app.config.ui.lyrics_follow = !follow;
        app.save_config();
    }

    ui.add_space(row_height());
    heading(ui, "log", palette);
    for kind in crate::gui::log::Kind::ALL {
        let word = kind.word();
        let shown = !app.config.ui.log_hidden.iter().any(|h| h == word);
        if toggle(ui, word, shown, ("zeigen", "aus"), palette).clicked() {
            if shown {
                app.config.ui.log_hidden.push(word.to_string());
            } else {
                app.config.ui.log_hidden.retain(|h| h != word);
            }
            app.save_config();
        }
    }
    if row(ui, tr!("exportieren"), tr!("als markdown"), true, palette).clicked() {
        app.export_log();
    }

    ui.add_space(row_height());
    heading(ui, tr!("andere künstler:innen"), palette);
    const COUNTS: [u32; 4] = [0, 10, 25, 50];
    let now = app.config.ui.others_shown;
    let said = if now == 0 { tr!("alle").to_string() } else { text::count(u64::from(now)) };
    if row(ui, tr!("zeigen"), &said, true, palette).clicked() {
        let at = COUNTS.iter().position(|c| *c == now).unwrap_or(0);
        app.config.ui.others_shown = COUNTS[(at + 1) % COUNTS.len()];
        app.save_config();
    }
}

fn about_section(app: &mut GuiApp, ui: &mut Ui, palette: &Palette) {
    heading(ui, tr!("über"), palette);

    let mut open: Option<String> = None;
    if row(ui, tr!("entwickler"), PUBLISHER.0, true, palette).clicked() {
        open = Some(PUBLISHER.1.to_string());
    }
    if row(ui, "github", GITHUB.0, true, palette).clicked() {
        open = Some(GITHUB.1.to_string());
    }
    row(ui, tr!("danke an"), tr!("mattis, matthias, leo, niklas und rade"), false, palette);
    let heard = app.listened.heard;
    use crate::gui::listened::said as time;
    let asking_id = egui::Id::unique("menu-heard-asking");
    let now = ui.input(|i| i.time);
    let asked: Option<f64> = ui.data(|d| d.get_temp(asking_id)).filter(|at| now - at < 5.0);
    let under = Rect::from_min_size(ui.cursor().min, Vec2::new(ui.available_width(), row_height()));
    let pointed = ui.rect_contains_pointer(under);
    let said = if asked.is_some() {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
        tr!("sicher? noch einmal klicken").to_string()
    } else if pointed {
        tr!("auf null setzen").to_string()
    } else {
        time(heard.total())
    };
    let parts = crate::gui::listened::parts(&heard);
    let room = under.width()
        - text::line(ui, tr!("gehört"), Role::Content, palette.mid).size().x
        - 20.0
        - text::line(ui, &time(heard.total()), Role::Content, palette.ink).size().x;
    let tail_g = text::line(ui, &parts, Role::Label, palette.mid);
    let fits = tail_g.size().x + 6.0 <= room;
    let tail = (fits && asked.is_none() && !pointed).then_some(tail_g);
    if heard_row(ui, &said, tail, palette).clicked() {
        if asked.is_some() {
            app.listened.reset();
            ui.data_mut(|d| d.remove::<f64>(asking_id));
        } else {
            ui.data_mut(|d| d.insert_temp(asking_id, now));
        }
    } else if asked.is_none() {
        ui.data_mut(|d| d.remove::<f64>(asking_id));
    }
    if !fits {
        for (name, secs) in [(tr!("musik"), heard.music), (tr!("radio"), heard.radio), (tr!("podcasts"), heard.podcasts)] {
            line(ui, name, &time(secs), Line::Listed, palette, None, false);
        }
    }
    if app.families.iter().any(|f| f.key == "muto") && row(ui, "muto", tr!("eine schrift von rade matic"), true, palette).clicked() {
        open = Some(FOUNDRY.to_string());
    }
    if let Some(url) = open {
        if !crate::lastfm::open_in_browser(&url) {
            app.set_status(tr!("der browser liess sich nicht öffnen"));
        }
    }
    let unfolded = app.licenses_open;
    if row(ui, tr!("lizenzen"), if unfolded { tr!("schliessen") } else { tr!("zeigen") }, true, palette).clicked() {
        app.licenses_open = !unfolded;
        rebalance(app);
    }
    if unfolded {
        licenses(app, ui, palette);
    }

    let held = ui.input(|i| i.modifiers.alt);
    if app.measure.is_some() || held {
        measure_row(app, ui, palette);
    }
    if held && row(ui, tr!("farbschemen als gif"), "alt + shift + g", true, palette).clicked() {
        app.toggle_reel(super::app::Film::Gif);
    }
    if held && row(ui, tr!("farbschemen als mp4"), "alt + shift + m", true, palette).clicked() {
        app.toggle_reel(super::app::Film::Mp4);
    }

    ui.add_space(row_height());
    let motto: Vec<_> = [tr!("»warum großschreiben,"), tr!("wenn man nicht groß sprechen kann?«")]
        .iter()
        .map(|line| text::line(ui, line, Role::Label, palette.faint))
        .collect();
    let height: f32 = motto.iter().map(|g| g.size().y).sum();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    let mut y = rect.top();
    for g in motto {
        let (width, step) = (g.size().x, g.size().y);
        ui.painter().galley(egui::pos2(rect.center().x - width / 2.0, y), g, palette.faint);
        y += step;
    }
}

fn measure_row(app: &mut GuiApp, ui: &mut Ui, palette: &Palette) {
    let said = match &app.measure {
        Some(_) => tr!("läuft"),
        None => tr!("aus"),
    };
    if row(ui, tr!("messen"), said, true, palette).clicked() {
        match app.measure.take() {
            Some(measure) => {
                let _ = super::reveal::show(measure.path());
                app.set_status(tr!("messung gespeichert: {}", measure.path().display().to_string()));
            }
            None => {
                let path = crate::config::Config::data_dir().join("kessler-messung.log");
                match super::measure::Measure::start(path) {
                    Ok(measure) => app.measure = Some(measure),
                    Err(e) => app.set_status(format!("{e}")),
                }
            }
        }
    }
}

fn sheet_head(app: &mut GuiApp, ui: &mut Ui, palette: &Palette) -> f32 {
    let row = row_height();
    let tall = row * 2.0;
    let (whole, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), tall), Sense::hover());
    let rect = whole.shrink2(Vec2::new(HOVER_BLEED, 0.0));

    let figure = Rect::from_min_size(rect.min, Vec2::splat(tall));
    let pressed = ui.interact(figure, egui::Id::unique("menu-mark"), Sense::click());
    if pressed.hovered() && (app.player.current_track().is_some() || app.on_air.is_some()) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if pressed.clicked() {
        app.toggle_pause();
    }
    if !still_mark(app, ui, figure, palette, false) && !super::library::live_mark(app, ui, figure, palette) {
        still_mark(app, ui, figure, palette, true);
    }

    let line = text::row_line(ui, row, Role::Content);
    let left = figure.right() + FIGURE_GAP;
    let name = probe_name(ui, palette);
    let name_top = text::on_line(&name, rect.top(), line);
    let mut y = name_top + name.size().y;
    ui.painter().galley(egui::pos2(left, name_top), name, palette.ink);
    let shown = if app.drawn_with.is_empty() {
        crate::VERSION_SHOWN.to_string()
    } else {
        format!("{} · {}", crate::VERSION_SHOWN, app.drawn_with)
    };
    let number = text::line_tracked(ui, &shown, Role::Label, palette.mid, HEAD_TRACKING);
    let step = number.size().y;
    ui.painter().galley(egui::pos2(left, y), number, palette.mid);
    y += step;
    let commit = env!("KESSLER_COMMIT");
    if !commit.is_empty() {
        let g = text::line_tracked(ui, commit, Role::Label, palette.faint, HEAD_TRACKING);
        ui.painter().galley(egui::pos2(left, y), g, palette.faint);
    }

    let word = text::line(ui, tr!("schliessen"), Role::Label, palette.mid);
    let width = word.size().x;
    let ink = word.mesh_bounds;
    let at = egui::pos2(rect.right() - ink.max.x, rect.top() - ink.min.y);
    let close = super::library::control_plate(ui, at, width);
    let response = ui.interact(close, egui::Id::unique("menu-close"), Sense::click());
    super::library::paint_control_hover(ui, close, &response, palette);
    let colour = if response.hovered() { palette.ink } else { palette.mid };
    ui.painter().galley(at, word, colour);
    if response.clicked() {
        app.menu_open = false;
    }

    let probe = text::line(ui, "x", Role::Content, palette.mid);
    let words_top = text::on_line(&probe, 0.0, line);
    let ink_top = text::ink_middle(ui, Role::Content, words_top) - text::ink_height(ui, Role::Content) / 2.0;
    let air = (PADDING + HEAD_EXTRA - ink_top).round().max(0.0);
    ui.add_space(air);
    tall + air + ui.spacing().item_spacing.y
}

fn probe_name(ui: &Ui, palette: &Palette) -> std::sync::Arc<egui::Galley> {
    text::line_tracked(ui, tr!("kessler"), Role::Content, palette.ink, HEAD_TRACKING)
}

const HEAD_TRACKING: f32 = 0.4;

const HEAD_EXTRA: f32 = 6.0;

const FIGURE_GAP: f32 = 16.0;

pub(super) fn icon_curve(across: f32, steps: usize) -> Vec<(f32, f32)> {
    let reach = 0.33 * across / 0.70;
    (0..steps)
        .map(|i| {
            let t = 3.0 * std::f32::consts::TAU * i as f32 / (steps - 1) as f32;
            let k = t / std::f32::consts::TAU;
            let r = 1.0 - 0.17 * k;
            let x = r * t.cos();
            let y = r * 0.62 * (t + 0.25 * (k * 2.1).sin()).sin();
            let a = 45f32.to_radians() + 0.10 * k;
            let (xr, yr) = (x * a.cos() - y * a.sin(), x * a.sin() + y * a.cos());
            (xr * reach, -yr * reach)
        })
        .collect()
}

fn still_mark(app: &mut GuiApp, ui: &Ui, room: Rect, palette: &Palette, always: bool) -> bool {
    if !always && app.player.state() == crate::player::PlaybackState::Playing {
        return false;
    }
    let plate = Rect::from_center_size(room.center(), Vec2::splat(room.width().min(room.height())));
    let ppp = ui.ctx().pixels_per_point();
    let side = (plate.width() * ppp).round().max(8.0) as usize;
    let face = (
        app.config.ui.typeface.clone(),
        super::theme::font_width().to_bits(),
        super::theme::weights().map(|(_, bold)| bold.to_bits()),
    );
    let key = (side, crate::lang::current(), face);
    if !matches!(&app.mark_paused, Some((made, _)) if *made == key) {
        let Some(cover) = super::appicon::letter_square(ui.ctx(), side) else {
            return false;
        };
        let image = egui::ColorImage {
            size: [side, side],
            pixels: cover.into_iter().map(egui::Color32::from_white_alpha).collect(),
            source_size: Vec2::splat(side as f32),
        };
        let texture = ui.ctx().load_texture("mark-paused", image, egui::TextureOptions::LINEAR);
        app.mark_paused = Some((key, texture));
    }
    ui.painter().rect_filled(plate, super::theme::MARKER_RADIUS, palette.field);
    if let Some((_, texture)) = &app.mark_paused {
        ui.painter().image(
            texture.id(),
            plate,
            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            palette.accent,
        );
    }
    true
}

pub(super) fn screened_mark(app: &mut GuiApp, ui: &Ui, room: Rect, colour: egui::Color32) {
    const BEAM: f32 = 0.3;
    let ppp = ui.ctx().pixels_per_point();
    let side = (room.width() * ppp).round().max(8.0) as usize;
    let method = Screen::from_name(&app.config.ui.dither_method);
    let key = (side, method);
    let fresh = !matches!(&app.mark_still, Some((made, _)) if *made == key);
    if fresh {
        let mut light = vec![0.0f32; side * side];
        let middle = side as f32 / 2.0;
        let points = icon_curve(side as f32, 2400);
        for pair in points.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let steps = ((b.0 - a.0).hypot(b.1 - a.1) / 0.5).ceil().max(1.0) as usize;
            for step in 0..steps {
                let t = step as f32 / steps as f32;
                super::scope::splat(
                    &mut light,
                    side,
                    side,
                    middle + a.0 + (b.0 - a.0) * t,
                    middle + a.1 + (b.1 - a.1) * t,
                    BEAM,
                );
            }
        }
        let grey: Vec<u8> = light
            .iter()
            .map(|v| ((1.0 - (-v * 2.2).exp()) * 255.0) as u8)
            .collect();
        let dots = super::dither::screen(&grey, side as u32, side as u32, method);
        let image = egui::ColorImage {
            size: [side, side],
            pixels: dots
                .into_iter()
                .map(|ink| if ink { egui::Color32::WHITE } else { egui::Color32::TRANSPARENT })
                .collect(),
            source_size: Vec2::splat(side as f32),
        };
        let texture = ui
            .ctx()
            .load_texture("mark-still", image, egui::TextureOptions::NEAREST);
        app.mark_still = Some((key, texture));
    }
    if let Some((_, texture)) = &app.mark_still {
        ui.painter().image(
            texture.id(),
            room,
            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            colour,
        );
    }
}

fn saver_section(app: &mut GuiApp, ui: &mut Ui, palette: &Palette) {
    heading(ui, tr!("bildschirmschoner"), palette);
    let after = app.config.ui.saver_after;
    let on = after != 0;
    if toggle(ui, "bildschirmschoner", on, AN_AUS, palette).clicked() {
        app.config.ui.saver_after = if on { 0 } else { SAVER_FIRST };
        app.save_config();
    }
    if on {
        set_in(ui, |ui| saver_details(app, ui, palette));
    }
    if row(ui, "", tr!("jetzt zeigen"), true, palette).clicked() {
        let now = ui.input(|i| i.time);
        app.saver.show_now(now);
    }
}

const SAVER_FIRST: u32 = 5;

fn saver_details(app: &mut GuiApp, ui: &mut Ui, palette: &Palette) {
    use super::saver::{AFTER, DOTS, GLOWS, SIZES};
    let after = app.config.ui.saver_after;
    let said = match after {
        1 => tr!("nach 1 minute").to_string(),
        n => tr!("nach {} minuten", n),
    };
    if row(ui, tr!("startet"), &said, true, palette).clicked() {
        let waits: Vec<u32> = AFTER.iter().copied().filter(|m| *m != 0).collect();
        let at = waits.iter().position(|m| *m == after).map_or(0, |i| i + 1);
        app.config.ui.saver_after = waits[at % waits.len()];
        app.save_config();
    }
    let pictures = super::saver::PICTURES;
    let at = pictures.iter().position(|p| *p == app.config.ui.saver_picture).unwrap_or(0);
    if row(ui, tr!("zeigt"), super::saver::said(pictures[at]), true, palette).clicked() {
        app.config.ui.saver_picture = pictures[(at + 1) % pictures.len()].to_string();
        app.save_config();
    }
    let shown = pictures[at];
    let scope = super::library::FieldPicture::from_name(shown) == super::library::FieldPicture::Scope && shown != super::saver::WORDS;
    let words = shown == super::saver::WORDS;
    if !(scope || words) {
        return;
    }
    let silent = app.config.ui.saver_silent;
    let said = if silent { tr!("auch ohne musik") } else { tr!("nur bei musik") };
    if scope && row(ui, tr!("wann"), said, true, palette).clicked() {
        app.config.ui.saver_silent = !silent;
        app.save_config();
    }
    let size = usize::from(app.config.ui.saver_size).min(SIZES.len() - 1);
    let said = match size {
        0 => tr!("klein"),
        1 => tr!("mittel"),
        2 => tr!("gross"),
        _ => tr!("ganzes fenster"),
    };
    if row(ui, tr!("grösse"), said, true, palette).clicked() {
        app.config.ui.saver_size = ((size + 1) % SIZES.len()) as u8;
        app.save_config();
    }
    if !scope {
        return;
    }
    let dots = app.config.ui.saver_dots.clamp(1, 3);
    let said = match dots {
        1 => tr!("fein"),
        2 => tr!("mittel"),
        _ => tr!("grob"),
    };
    if row(ui, tr!("punkte"), said, true, palette).clicked() {
        let at = DOTS.iter().position(|d| *d == dots).map_or(0, |i| i + 1);
        app.config.ui.saver_dots = DOTS[at % DOTS.len()];
        app.save_config();
    }
    let glow = usize::from(app.config.ui.saver_glow).min(GLOWS.len() - 1);
    let said = match glow {
        0 => tr!("kurz"),
        1 => tr!("mittel"),
        _ => tr!("lang"),
    };
    if row(ui, tr!("nachleuchten"), said, true, palette).clicked() {
        app.config.ui.saver_glow = ((glow + 1) % GLOWS.len()) as u8;
        app.save_config();
    }
    let strength = usize::from(app.config.ui.saver_strength).min(super::saver::STRENGTHS.len() - 1);
    let said = match strength {
        0 => tr!("zart"),
        1 => tr!("mittel"),
        _ => tr!("kräftig"),
    };
    if row(ui, tr!("deckkraft"), said, true, palette).clicked() {
        app.config.ui.saver_strength = ((strength + 1) % super::saver::STRENGTHS.len()) as u8;
        app.save_config();
    }
    let colours = usize::from(app.config.ui.saver_colours).min(2);
    let said = match colours {
        0 => tr!("akzent"),
        1 => tr!("wandernd"),
        _ => tr!("verlauf"),
    };
    if row(ui, tr!("farben"), said, true, palette).clicked() {
        app.config.ui.saver_colours = ((colours + 1) % 3) as u8;
        app.save_config();
    }
    let chosen = Screen::from_name(&app.config.ui.saver_dither_method);
    let open = app.saver_screen_list_open;
    if row(ui, tr!("verfahren"), crate::lang::t(chosen.name()), true, palette).clicked() {
        app.saver_screen_list_open = !open;
    }
    if open {
        if let Some(method) = screen_list(ui, palette, chosen) {
            app.config.ui.saver_dither_method = method.name().to_string();
            app.save_config();
        }
    }
}

pub(super) const ZOOMS: [f32; 8] = [0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75];

pub(super) fn zoom_of(configured: f32) -> f32 {
    if configured.is_finite() && (0.5..=2.0).contains(&configured) {
        configured
    } else {
        1.0
    }
}

fn zoom_notch(zoom: f32) -> f32 {
    let at = ZOOMS
        .iter()
        .enumerate()
        .min_by(|a, b| (a.1 - zoom).abs().total_cmp(&(b.1 - zoom).abs()))
        .map(|(at, _)| at)
        .unwrap_or(0);
    at as f32 / (ZOOMS.len() - 1) as f32
}

pub(super) fn zoom_step(current: f32, by: i32) -> f32 {
    let at = (zoom_notch(current) * (ZOOMS.len() - 1) as f32).round() as i32;
    ZOOMS[(at + by).clamp(0, ZOOMS.len() as i32 - 1) as usize]
}

fn screen_list(ui: &mut Ui, palette: &Palette, current: Screen) -> Option<Screen> {
    let mut picked: Option<Screen> = None;
    for method in Screen::ALL {
        if choice(ui, crate::lang::t(method.name()), crate::lang::t(method.about()), method == current, palette).clicked() {
            picked = Some(method);
        }
    }
    picked
}

fn pictures_section(app: &mut GuiApp, ui: &mut Ui, palette: &Palette) {
    heading(ui, tr!("bilder"), palette);
    let dithered = app.config.ui.cover_dither;
    let value = if dithered { tr!("gerastert") } else { tr!("im original") };
    if row(ui, tr!("anzeige"), value, true, palette).clicked() {
        app.config.ui.cover_dither = !dithered;
        app.save_config();
    }
    if !dithered {
        example_picture_row(app, ui, palette);
        return;
    }
    let chosen = Screen::from_name(&app.config.ui.dither_method);
    let open = app.picture_screen_list_open;
    if row(ui, tr!("raster"), crate::lang::t(chosen.name()), true, palette).clicked() {
        app.picture_screen_list_open = !open;
    }
    if open {
        if let Some(method) = screen_list(ui, palette, chosen) {
            app.config.ui.dither_method = method.name().to_string();
            app.save_config();
        }
    }
    let tones = app.config.ui.dither_tones.clamp(2, 4);
    let said = match tones {
        2 => tr!("zwei"),
        3 => tr!("drei"),
        _ => tr!("vier"),
    };
    if row(ui, tr!("töne"), said, true, palette).clicked() {
        app.config.ui.dither_tones = if tones >= 4 { 2 } else { tones + 1 };
        app.save_config();
    }
    let fineness = app.config.ui.picture_fineness();
    let (moved, settled) = slider(ui, tr!("feinheit"), fineness, coarseness(fineness), palette);
    if let Some(moved) = moved {
        app.config.ui.dither_fineness = moved;
    }
    if settled {
        app.save_config();
    }
    example_picture_row(app, ui, palette);
}

const EXAMPLE_SIDE: f32 = 148.0;

fn example_picture() -> &'static image::DynamicImage {
    static PICTURE: std::sync::OnceLock<image::DynamicImage> = std::sync::OnceLock::new();
    PICTURE.get_or_init(|| {
        const SIDE: u32 = 400;
        type Colour = [f32; 3];
        let between = |a: Colour, b: Colour, t: f32| -> Colour { [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t.clamp(0.0, 1.0)) };
        let colour_at = |x: f32, y: f32| -> Colour {
            if (0.60..=0.90).contains(&x) && (0.58..=0.88).contains(&y) {
                return [0.05, 0.27, 0.30];
            }
            let (top, left, right, foot) = ((0.27, 0.50), 0.08, 0.46, 0.88);
            if y <= foot && y >= top.1 {
                let down = (y - top.1) / (foot - top.1);
                if x >= top.0 - (top.0 - left) * down && x <= top.0 + (right - top.0) * down {
                    return [0.98, 0.85, 0.42];
                }
            }
            let (cx, cy, r) = (0.50, 0.40, 0.27);
            let (dx, dy) = ((x - cx) / r, (y - cy) / r);
            if dx * dx + dy * dy <= 1.0 {
                let dz = (1.0 - dx * dx - dy * dy).sqrt();
                let lit = (-0.45 * dx - 0.55 * dy + 0.70 * dz).max(0.0).powf(1.3);
                let body = between([0.22, 0.03, 0.12], [0.98, 0.42, 0.30], lit);
                return between(body, [1.0, 0.95, 0.88], (lit - 0.82) / 0.18);
            }
            between([0.08, 0.10, 0.30], [0.99, 0.78, 0.60], x * 0.45 + y * 0.55)
        };
        let picture = image::RgbImage::from_fn(SIDE, SIDE, |px, py| {
            let mut sum = [0.0f32; 3];
            for i in 0..3 {
                for j in 0..3 {
                    let x = (px as f32 + (i as f32 + 0.5) / 3.0) / SIDE as f32;
                    let y = (py as f32 + (j as f32 + 0.5) / 3.0) / SIDE as f32;
                    let colour = colour_at(x, y);
                    for (sum, part) in sum.iter_mut().zip(colour) {
                        *sum += part;
                    }
                }
            }
            image::Rgb(sum.map(|part| (part / 9.0 * 255.0).round().clamp(0.0, 255.0) as u8))
        });
        image::DynamicImage::ImageRgb8(picture)
    })
}

fn example_picture_row(app: &mut GuiApp, ui: &mut Ui, palette: &Palette) {
    ui.add_space(row_height() * 0.5);
    let side = EXAMPLE_SIDE.min(ui.available_width());
    let (room, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), side), Sense::hover());
    let rect = Rect::from_min_size(room.min, Vec2::splat(side));
    let look = super::dither::Look::of(&app.config.ui);
    let dithered = app.config.ui.cover_dither;
    let negative = dithered && palette.screens_negative();
    let ppp = ui.ctx().pixels_per_point();
    let grain = if dithered {
        super::dither::grain_for(side, ppp, app.config.ui.picture_fineness())
    } else {
        (side * ppp).round() as u32
    };
    let made = (dithered, grain, look.screen.name(), look.tones, negative);
    let kept = egui::Id::unique("example-picture");
    type Kept = ((bool, u32, &'static str, u8, bool), egui::TextureHandle);
    let texture = match ui.data(|d| d.get_temp::<Kept>(kept)).filter(|(was, _)| *was == made) {
        Some((_, texture)) => texture,
        None => {
            let image = if dithered {
                super::covers::screened(example_picture(), grain, look, negative)
            } else {
                let picture = example_picture().to_rgb8();
                egui::ColorImage::from_rgb([picture.width() as usize, picture.height() as usize], picture.as_raw())
            };
            let options = if dithered { egui::TextureOptions::NEAREST } else { egui::TextureOptions::LINEAR };
            let texture = ui.ctx().load_texture("example-picture", image, options);
            ui.data_mut(|d| d.insert_temp::<Kept>(kept, (made, texture.clone())));
            texture
        }
    };
    if dithered {
        super::covers::halftone_ground(ui, rect, palette);
        let under = palette.tones_under(palette.ink, look.tones);
        super::covers::paint_screened(ui, rect, super::theme::MARKER_RADIUS, texture.id(), &under, palette.ink);
    } else {
        super::covers::paint_screened(ui, rect, super::theme::MARKER_RADIUS, texture.id(), &[], egui::Color32::WHITE);
    }
}

fn heading_cover_rows(app: &mut GuiApp, ui: &mut Ui, palette: &Palette) {
    let shown = app.config.ui.heading_covers;
    if toggle(ui, tr!("cover"), shown, AN_AUS, palette).clicked() {
        app.config.ui.heading_covers = !shown;
        app.save_config();
    }
    if shown {
        heading_cover_size(app, ui, palette);
    }
    let count = app.config.ui.heading_count;
    if toggle(ui, tr!("titelzahl"), count, AN_AUS, palette).clicked() {
        app.config.ui.heading_count = !count;
        app.save_config();
    }
}

fn heading_cover_size(app: &mut GuiApp, ui: &mut Ui, palette: &Palette) {
    use crate::config::{HEADING_COVER_LEAST, HEADING_COVER_MOST};
    let span = HEADING_COVER_MOST - HEADING_COVER_LEAST;
    let size = app.config.ui.heading_cover();
    let reading = format!("{} pt", size.round() as i32);
    let (moved, settled) = slider(ui, tr!("covergrösse"), (size - HEADING_COVER_LEAST) / span, &reading, palette);
    if let Some(moved) = moved {
        app.config.ui.heading_cover_size = (HEADING_COVER_LEAST + moved * span).round();
    }
    if settled {
        app.save_config();
    }
}

fn coarseness(fineness: f32) -> &'static str {
    match fineness {
        f if f < 0.15 => tr!("sehr grob"),
        f if f < 0.35 => tr!("grob"),
        f if f < 0.65 => tr!("mittel"),
        f if f < 0.85 => tr!("fein"),
        _ => tr!("sehr fein"),
    }
}

fn theme_list(app: &mut GuiApp, ui: &mut Ui, palette: &Palette) {
    let current = app.theme;
    let mut chosen: Option<Theme> = None;

    let own_shown = ui.input(|i| i.modifiers.alt) || current.family() == Theme::Eigenes.family();
    let groups: Vec<(&str, &[Theme])> = Theme::GROUPS
        .iter()
        .copied()
        .filter(|(_, schemes)| own_shown || !schemes.iter().any(|t| t.family() == Theme::Eigenes.family()))
        .collect();
    let rows: Vec<usize> = groups.iter().map(|(_, s)| s.len() + 1).collect();
    let total: usize = rows.iter().sum();
    let split = (0..=groups.len())
        .min_by_key(|&k| {
            let left: usize = rows[..k].iter().sum();
            left.max(total - left)
        })
        .unwrap_or(groups.len());
    const APART: f32 = 24.0;
    let top = ui.cursor().min;
    let width = ui.available_width();
    let half = ((width - APART) / 2.0).max(1.0);
    let mut bottom = top.y;
    for (column, range) in [(0.0, 0..split), (1.0, split..groups.len())] {
        let room = Rect::from_min_size(
            egui::pos2(top.x + column * (half + APART), top.y),
            Vec2::new(half, f32::INFINITY),
        );
        let mut col = ui.new_child(egui::UiBuilder::new().max_rect(room).layout(*ui.layout()));
        for (n, (caption, schemes)) in groups[range].iter().enumerate() {
            if n > 0 {
                col.add_space(row_height() / 2.0);
            }
            set_in(&mut col, |ui| heading(ui, crate::lang::t(caption), palette));
            for &theme in schemes.iter() {
                let on = theme.family() == current.family();
                if choice(&mut col, crate::lang::t(theme.family()), "", on, palette).clicked() {
                    chosen = Some(theme);
                }
            }
        }
        bottom = bottom.max(col.min_rect().bottom());
    }
    ui.allocate_rect(Rect::from_min_max(top, egui::pos2(top.x + width, bottom)), Sense::hover());

    if let Some(theme) = chosen {
        let ctx = ui.ctx().clone();
        app.apply_theme(&ctx, theme, app.brightness);
    }
}

fn own_scheme(app: &mut GuiApp, ui: &mut Ui, palette: &Palette) {
    use super::theme::{custom, from_rules, rules_of, set_custom};
    let dark = palette.is_dark();
    let shown = custom().side(dark);
    if shown != custom() {
        set_custom(shown);
    }
    let names = [tr!("grund"), tr!("schrift|farbe"), tr!("akzent"), tr!("köpfe")];
    for (role, name) in names.into_iter().enumerate() {
        let colour = custom().get(role);
        let said = format!("#{:02x}{:02x}{:02x}", colour.r(), colour.g(), colour.b());
        let open = app.seed_role_open == Some(role);
        if choice(ui, name, &said, open, palette).clicked() {
            app.seed_role_open = if open { None } else { Some(role) };
            app.seed_hex = None;
            rebalance(app);
        }
        if !open {
            continue;
        }
        let held = egui::Id::unique(("seed-rules", role));
        let rules = ui
            .data(|d| d.get_temp::<([f32; 3], egui::Color32)>(held))
            .filter(|(_, was)| *was == colour)
            .map(|(rules, _)| rules)
            .unwrap_or_else(|| rules_of(colour));
        let readings = [
            format!("{} %", (rules[0] * 100.0).round() as i32),
            format!("{} %", (rules[1] * 100.0).round() as i32),
            format!("{}°", (rules[2] * 360.0).round() as i32),
        ];
        let labels = [tr!("helligkeit"), tr!("buntheit"), tr!("farbton")];
        for k in 0..3 {
            let (moved, settled) =
                set_in(ui, |ui| set_in(ui, |ui| slider(ui, labels[k], rules[k], &readings[k], palette)));
            if moved.is_some() || settled {
                let mut now = rules;
                if let Some(at) = moved {
                    now[k] = at;
                }
                let made = from_rules(now);
                ui.data_mut(|d| d.insert_temp(held, (now, made)));
                app.set_seed_colour(role, made, settled);
            }
        }
        let (mut take, mut leave) = (false, false);
        set_in(ui, |ui| {
            set_in(ui, |ui| match app.seed_hex.as_ref() {
                Some((_, entry)) => {
                    entry_field(ui, tr!("hex"), entry, true, false, palette);
                    take = row(ui, "", tr!("übernehmen"), true, palette).clicked();
                    leave = row(ui, "", tr!("abbrechen"), true, palette).clicked();
                }
                None => {
                    if row(ui, tr!("hex"), tr!("eintippen…"), true, palette).clicked() {
                        app.seed_entry = None;
                        app.seed_hex = Some((role, super::tagedit::Entry::new(said.clone())));
                        if let Some((_, entry)) = app.seed_hex.as_mut() {
                            entry.mark_all();
                        }
                    }
                }
            })
        });
        if take {
            let text = app.seed_hex.as_ref().map(|(_, e)| e.text().to_string()).unwrap_or_default();
            app.take_seed_hex(role, &text);
        }
        if leave {
            app.seed_hex = None;
        }
    }
    let code = custom().code();
    if set_in(ui, |ui| row(ui, tr!("code"), &code, true, palette).clicked()) {
        ui.ctx().copy_text(code);
        app.set_status(tr!("code kopiert"));
    }
    let mut take = false;
    let mut leave = false;
    match app.seed_entry.as_ref() {
        None => {
            if set_in(ui, |ui| row(ui, "", tr!("code einfügen…"), true, palette).clicked()) {
                app.seed_hex = None;
                app.seed_entry = Some(super::tagedit::Entry::new(""));
            }
        }
        Some(entry) => set_in(ui, |ui| {
            entry_field(ui, tr!("code"), entry, true, false, palette);
            let paste = if cfg!(target_os = "macos") { "⌘v" } else { tr!("strg+v") };
            row(ui, "", &tr!("einfügen mit {} · enter übernimmt", paste), false, palette);
            take = row(ui, "", tr!("übernehmen"), true, palette).clicked();
            leave = row(ui, "", tr!("abbrechen"), true, palette).clicked();
        }),
    }
    if leave {
        app.seed_entry = None;
    }
    if take {
        let code = app.seed_entry.as_ref().map(|e| e.text().to_string()).unwrap_or_default();
        let ctx = ui.ctx().clone();
        app.take_seed_code(&ctx, &code);
    }
}

const FOUNDRY: &str = "https://www.radematic.com";

const PUBLISHER: (&str, &str) = ("windpark soft", "https://windparkbooks.de/soft/");

const FACE_LICENSES: [(&str, &str, &str); 3] = [
    ("asap", "Asap", "sil open font license 1.1"),
    ("m plus 1 code", "M PLUS 1 Code", "sil open font license 1.1"),
    ("muto", "Muto", "nur in kessler, rade matic"),
];

const OTHER_LICENSES: [(&str, &str); 5] = [
    ("zeichnung", "cc by-nc-nd 4.0, felix h-w"),
    ("egui-schriften", "ubuntu font licence, ofl, mit"),
    ("symphonia", "mpl-2.0"),
    ("sqlite", "gemeinfrei"),
    ("rust-bibliotheken", "mit, apache-2.0, bsd, zlib"),
];

fn licenses(app: &mut GuiApp, ui: &mut Ui, palette: &Palette) {
    if choice(ui, "kessler", LICENSE.0, false, palette).clicked() && !crate::lastfm::open_in_browser(LICENSE.1) {
        app.set_status(tr!("der browser liess sich nicht öffnen"));
    }
    let folder = super::theme::font_folder(app.config.ui.font_dir.as_deref());
    for (name, dir, licence) in FACE_LICENSES {
        let here = folder.as_ref().map(|f| f.join(dir)).filter(|d| d.is_dir());
        let Some(here) = here else {
            continue;
        };
        let text = ["OFL.txt", "LICENSE-muto.md", "LICENSE.txt", "LICENSE"].iter().map(|f| here.join(f)).find(|f| f.is_file());
        if choice(ui, name, crate::lang::t(licence), false, palette).clicked() {
            if let Some(text) = text {
                if let Err(e) = super::reveal::show(&text) {
                    app.set_status(e);
                }
            }
        }
    }
    for (name, licence) in OTHER_LICENSES {
        line(ui, crate::lang::t(name), crate::lang::t(licence), Line::Listed, palette, None, false);
    }
}

const DOT_RADIUS: f32 = 0.065;
const DOT_STEP: f32 = 0.24;

pub fn trigger(ui: &mut Ui, at: egui::Pos2, palette: &Palette, open: bool) -> bool {
    let width = trigger_width(ui);
    let left = egui::pos2(at.x - width, at.y);
    let plate = super::library::control_plate(ui, left, width);
    let response = ui.interact(plate, egui::Id::unique("menu-trigger"), Sense::click());
    let colour = if open {
        palette.accent
    } else if response.hovered() {
        palette.ink
    } else {
        palette.mid
    };
    super::library::paint_control_hover(ui, plate, &response, palette);
    let h = plate.height();
    for i in [-1.0, 0.0, 1.0] {
        let at = plate.center() + egui::vec2(i * DOT_STEP * h, 0.0);
        ui.painter().circle_filled(at, DOT_RADIUS * h, colour);
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response.clicked()
}

pub fn trigger_width(ui: &Ui) -> f32 {
    let probe = super::library::control_plate(ui, egui::pos2(0.0, 0.0), 0.0);
    let h = probe.height();
    (h - probe.width()).max(2.0 * (DOT_STEP + DOT_RADIUS) * h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_size_walks_its_notches_and_nonsense_is_the_systems() {
        assert!(ZOOMS.windows(2).all(|w| w[0] < w[1]));
        assert!(ZOOMS.contains(&1.0));
        let mut at = ZOOMS[0];
        for want in &ZOOMS[1..] {
            at = zoom_step(at, 1);
            assert_eq!(at, *want);
        }
        assert_eq!(zoom_step(at, 1), at, "held at the largest");
        assert_eq!(zoom_step(ZOOMS[0], -1), ZOOMS[0], "held at the smallest");
        assert_eq!(zoom_step(1.37, 1), 1.5, "a size set by hand moves on from the nearest");
        assert_eq!(zoom_notch(ZOOMS[0]), 0.0);
        assert_eq!(zoom_notch(ZOOMS[ZOOMS.len() - 1]), 1.0);
        for nonsense in [0.0, -1.0, f32::NAN, f32::INFINITY, 9.0] {
            assert_eq!(zoom_of(nonsense), 1.0, "{nonsense}");
        }
        assert_eq!(zoom_of(1.25), 1.25);
        assert_eq!(crate::config::UiConfig::default().zoom, 1.0);
    }

    #[test]
    fn the_playing_line_steps_through_every_place() {
        use super::super::library::PlayingAt;
        let mut ui = crate::config::UiConfig::default();
        assert_eq!(PlayingAt::of(&ui), PlayingAt::Compact);
        let mut seen = Vec::new();
        for _ in 0..PlayingAt::ALL.len() {
            let at = PlayingAt::of(&ui);
            seen.push(at);
            at.next().store(&mut ui);
        }
        assert_eq!(seen, PlayingAt::ALL.to_vec());
        assert_eq!(PlayingAt::of(&ui), PlayingAt::Compact);

    }

    #[test]
    fn the_sheet_takes_as_many_columns_as_fit() {
        let gap = 48.0;
        let two = COLUMN * 2.0 + gap + PADDING * 2.0;
        let three = COLUMN * 3.0 + gap * 2.0 + PADDING * 2.0;

        assert_eq!(sheet_size(three, gap), (three, 3));
        assert_eq!(sheet_size(three + 400.0, gap), (three, 3), "and no more, and no wider");
        assert_eq!(sheet_size(three - 1.0, gap), (two, 2));
        assert_eq!(sheet_size(two, gap), (two, 2));

        assert_eq!(sheet_size(two - 1.0, gap), (WIDTH, 1));
        assert_eq!(sheet_size(WIDTH, gap), (WIDTH, 1));

        assert_eq!(sheet_size(300.0, gap), (300.0, 1));
    }

    #[test]
    fn a_search_on_the_key_sheet_finds_keys_and_words() {
        assert!(key_answers("n", "wiedergabe", "[n] / [p]", "nächster, voriger titel"));
        assert!(!key_answers("n", "wiedergabe", "[s] / [r]", "zufall, wiederholen"));
        assert!(key_answers("Zufall", "wiedergabe", "[s] / [r]", "zufall, wiederholen"));
        assert!(key_answers("wieder", "wiedergabe", "[,] / [.]", "eine minute spulen"));
        assert!(key_answers("shi spul", "wiedergabe", "[shift] + [←] [→]", "10 sekunden spulen"));
        assert!(!key_answers("shi zufall", "wiedergabe", "[shift] + [←] [→]", "10 sekunden spulen"));
        assert!(key_answers("?", "fenster", "[?]", "alle tasten auf einen blick"));
        assert!(!key_answers("  ", "fenster", "[?]", "alle tasten auf einen blick"));
    }

    #[test]
    fn every_shortcut_is_documented() {
        let listed: Vec<String> = every_shortcut().map(|(k, _)| k.replace(['[', ']'], "")).collect();
        for expected in [
            "leertaste",
            "enter",
            "n / p",
            "shift + ← →",
            "shift + t",
            "m",
            "q q",
            "?",
            "a / shift + a",
            ", / .",
            "cmd + + − 0",
            "shift + f",
            "g g / shift + g",
            "f / f11",
            "e / cmd + i",
            "y",
            "z / rechtsklick auf vol",
            "o",
            "b",
        ] {
            assert!(
                listed.iter().any(|k| k == expected),
                "the menu does not mention {expected}"
            );
        }
        assert!(every_shortcut().all(|(_, what)| !what.is_empty()));
    }

    #[test]
    fn a_written_shortcut_comes_apart_into_keys_and_words() {
        use KeyPiece::{Between, Key};
        assert_eq!(key_pieces("[n] / [p]"), vec![Key("n"), Between("/"), Key("p")]);
        assert_eq!(
            key_pieces("[shift] + [←] [→]"),
            vec![Key("shift"), Between("+"), Key("←"), Key("→")]
        );
        assert_eq!(key_pieces("[+] / [−]"), vec![Key("+"), Between("/"), Key("−")]);
        assert_eq!(key_pieces("[/]"), vec![Key("/")]);
        assert_eq!(key_pieces("rechtsklick"), vec![Between("rechtsklick")]);
        for (keys, _) in every_shortcut() {
            assert!(!key_pieces(keys).is_empty(), "{keys} comes out as nothing");
        }
    }

    #[test]
    fn shortcut_text_stays_lowercase() {
        for (group, _) in SHORTCUTS {
            assert_eq!(*group, group.to_lowercase(), "{group} carries capitals");
        }
        for (keys, what) in every_shortcut() {
            assert_eq!(*keys, keys.to_lowercase(), "{keys} carries capitals");
            assert_eq!(*what, what.to_lowercase(), "{what} carries capitals");
        }
    }
}
