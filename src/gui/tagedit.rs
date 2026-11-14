use crate::db::models::Track;
use crate::library::metadata::Extras;
use crate::library::tags::{Edit, Field};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Entry {
    text: String,
    caret: usize,
    marked: bool,
}

impl Entry {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let caret = text.len();
        Self {
            text,
            caret,
            marked: false,
        }
    }

    pub fn mark_all(&mut self) {
        self.marked = !self.text.is_empty();
        self.caret = self.text.len();
    }

    pub fn is_marked(&self) -> bool {
        self.marked
    }

    pub fn unmark(&mut self) {
        self.marked = false;
    }

    fn take_marked(&mut self) -> bool {
        if !self.marked {
            return false;
        }
        self.marked = false;
        self.text.clear();
        self.caret = 0;
        true
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn caret(&self) -> usize {
        self.caret
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn insert(&mut self, s: &str) {
        self.take_marked();
        self.text.insert_str(self.caret, s);
        self.caret += s.len();
    }

    pub fn backspace(&mut self) -> bool {
        if self.take_marked() {
            return true;
        }
        match self.text[..self.caret].chars().next_back() {
            Some(c) => {
                let from = self.caret - c.len_utf8();
                self.text.replace_range(from..self.caret, "");
                self.caret = from;
                true
            }
            None => false,
        }
    }

    pub fn delete(&mut self) {
        if self.take_marked() {
            return;
        }
        if let Some(c) = self.text[self.caret..].chars().next() {
            let to = self.caret + c.len_utf8();
            self.text.replace_range(self.caret..to, "");
        }
    }

    pub fn left(&mut self) {
        if self.marked {
            self.marked = false;
            self.caret = 0;
            return;
        }
        if let Some(c) = self.text[..self.caret].chars().next_back() {
            self.caret -= c.len_utf8();
        }
    }

    pub fn right(&mut self) {
        if self.marked {
            self.marked = false;
            self.caret = self.text.len();
            return;
        }
        if let Some(c) = self.text[self.caret..].chars().next() {
            self.caret += c.len_utf8();
        }
    }

    pub fn home(&mut self) {
        self.marked = false;
        self.caret = 0;
    }

    pub fn end(&mut self) {
        self.marked = false;
        self.caret = self.text.len();
    }

    pub fn clear(&mut self) {
        self.marked = false;
        self.text.clear();
        self.caret = 0;
    }
}

#[derive(Debug, Clone)]
pub struct Row {
    pub field: Field,
    pub entry: Entry,
    pub mixed: bool,
    pub touched: bool,
}

#[derive(Debug, Clone)]
pub struct Editor {
    pub tracks: Vec<i64>,
    pub titles: Vec<String>,
    pub rows: Vec<Row>,
    pub focus: usize,
}

pub fn extras_of(tracks: &[&Track]) -> Vec<Extras> {
    const AT_MOST: usize = 400;
    if tracks.len() > AT_MOST {
        return Vec::new();
    }
    tracks.iter().map(|t| crate::library::metadata::read_extras(&t.path)).collect()
}

impl Editor {
    pub fn open(tracks: &[&Track], extras: &[Extras]) -> Self {
        let known = extras.len() == tracks.len();
        let none = Extras::default();
        let rows = Field::ALL
            .iter()
            .map(|&field| {
                let from_file = matches!(field, Field::Composer | Field::Grouping | Field::Bpm | Field::Comment);
                let mut values = tracks
                    .iter()
                    .enumerate()
                    .map(|(i, t)| field.read(t, extras.get(i).unwrap_or(&none)));
                let first = values.next().unwrap_or_default();
                let mixed = (from_file && !known) || values.any(|v| v != first);
                Row {
                    field,
                    entry: Entry::new(if mixed { String::new() } else { first }),
                    mixed,
                    touched: false,
                }
            })
            .collect();

        Self {
            tracks: tracks.iter().map(|t| t.id).collect(),
            titles: tracks.iter().map(|t| t.display_title().to_string()).collect(),
            rows,
            focus: 0,
        }
    }

    pub fn row(&self) -> &Row {
        &self.rows[self.focus.min(self.rows.len() - 1)]
    }

    fn row_mut(&mut self) -> &mut Row {
        let at = self.focus.min(self.rows.len() - 1);
        &mut self.rows[at]
    }

    pub fn step(&mut self, delta: i32) {
        self.row_mut().entry.unmark();
        let len = self.rows.len() as i32;
        self.focus = (((self.focus as i32 + delta) % len + len) % len) as usize;
    }

    pub fn typed(&mut self, text: &str) {
        let numeric = self.row().field.numeric();
        let kept: String = text
            .chars()
            .filter(|c| !c.is_control() && (!numeric || c.is_ascii_digit()))
            .collect();
        if kept.is_empty() {
            return;
        }
        let row = self.row_mut();
        row.entry.insert(&kept);
        row.touched = true;
        row.mixed = false;
    }

    pub fn backspace(&mut self) {
        let row = self.row_mut();
        if row.entry.backspace() || row.mixed {
            row.touched = true;
            row.mixed = false;
        }
    }

    pub fn delete(&mut self) {
        let row = self.row_mut();
        if !row.entry.is_empty() {
            row.entry.delete();
            row.touched = true;
        }
        row.mixed = false;
    }

    pub fn mark_all(&mut self) {
        self.row_mut().entry.mark_all();
    }

    pub fn marked(&self) -> Option<&str> {
        let row = self.row();
        row.entry.is_marked().then(|| row.entry.text())
    }

    pub fn clear(&mut self) {
        let row = self.row_mut();
        row.entry.clear();
        row.touched = true;
        row.mixed = false;
    }

    pub fn left(&mut self) {
        self.row_mut().entry.left();
    }

    pub fn right(&mut self) {
        self.row_mut().entry.right();
    }

    pub fn home(&mut self) {
        self.row_mut().entry.home();
    }

    pub fn end(&mut self) {
        self.row_mut().entry.end();
    }

    pub fn edit(&self) -> Edit {
        let mut edit = Edit::new();
        for row in self.rows.iter().filter(|r| r.touched) {
            edit.set(row.field, row.entry.text());
        }
        edit
    }

    pub fn changed(&self) -> bool {
        self.rows.iter().any(|r| r.touched)
    }
}

#[cfg(test)]
mod marking {
    use super::Entry;

    #[test]
    fn a_marked_value_goes_at_the_next_letter() {
        let mut entry = Entry::new("the man from s.l.a.p.p.y.");
        entry.mark_all();
        assert!(entry.is_marked());
        entry.insert("spear-core");
        assert_eq!(entry.text(), "spear-core");
        assert!(!entry.is_marked());
        assert_eq!(entry.caret(), "spear-core".len());
    }

    #[test]
    fn backspace_takes_all_of_a_marked_value() {
        let mut entry = Entry::new("aida's call");
        entry.mark_all();
        assert!(entry.backspace());
        assert!(entry.is_empty());
        assert!(!entry.backspace());
    }

    #[test]
    fn an_arrow_lets_go_at_its_own_end() {
        let mut entry = Entry::new("1978");
        entry.mark_all();
        entry.left();
        assert!(!entry.is_marked());
        assert_eq!((entry.text(), entry.caret()), ("1978", 0));
        entry.mark_all();
        entry.right();
        assert_eq!((entry.text(), entry.caret()), ("1978", 4));
    }

    #[test]
    fn an_empty_field_has_nothing_to_mark() {
        let mut entry = Entry::new("");
        entry.mark_all();
        assert!(!entry.is_marked());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: i64, title: &str, artist: &str, album: &str, year: Option<i32>) -> Track {
        Track {
            id,
            path: std::path::PathBuf::from(format!("/music/{id}.mp3")),
            title: title.into(),
            artist: artist.into(),
            album_artist: String::new(),
            album: album.into(),
            genre: String::new(),
            year,
            track_number: None,
            disc_number: None,
            duration_secs: 100.0,
            bitrate: None,
            sample_rate: None,
            file_size: 0,
            format: "mp3".into(),
            rating: 0,
            play_count: 0,
            last_played: None,
            date_added: chrono::Local::now().naive_local(),
            date_modified: chrono::Local::now().naive_local(),
            file_hash: None,
            has_artwork: false,
        }
    }

    fn editor(tracks: &[Track]) -> Editor {
        let tracks: Vec<&Track> = tracks.iter().collect();
        let extras = vec![Extras::default(); tracks.len()];
        Editor::open(&tracks, &extras)
    }

    fn row(editor: &Editor, field: Field) -> &Row {
        editor.rows.iter().find(|r| r.field == field).unwrap()
    }

    fn focus(editor: &mut Editor, field: Field) {
        editor.focus = editor.rows.iter().position(|r| r.field == field).unwrap();
    }

    #[test]
    fn what_the_tracks_disagree_about_is_marked_not_guessed() {
        let e = editor(&[
            track(1, "olson", "boards of canada", "mhtrtc", Some(1998)),
            track(2, "roygbiv", "boards of canada", "mhtrtc", Some(1998)),
        ]);
        assert!(row(&e, Field::Title).mixed);
        assert_eq!(row(&e, Field::Title).entry.text(), "");
        assert!(!row(&e, Field::Artist).mixed);
        assert_eq!(row(&e, Field::Artist).entry.text(), "boards of canada");
        assert_eq!(row(&e, Field::Year).entry.text(), "1998");
    }

    #[test]
    fn an_untouched_form_is_an_empty_edit() {
        let mut e = editor(&[track(1, "olson", "boa", "geogaddi", Some(2002))]);
        assert!(!e.changed());
        assert!(e.edit().is_empty());
        e.step(1);
        e.step(1);
        assert!(e.edit().is_empty());
    }

    #[test]
    fn only_what_was_typed_is_written() {
        let mut e = editor(&[
            track(1, "olson", "boa", "geogaddi", Some(2002)),
            track(2, "1969", "boa", "geogaddi", Some(2002)),
        ]);
        focus(&mut e, Field::Album);
        e.clear();
        e.typed("geogaddi (remastered)");

        let edit = e.edit();
        assert_eq!(edit.changes().len(), 1);
        assert_eq!(edit.get(Field::Album), Some("geogaddi (remastered)"));
        assert_eq!(edit.get(Field::Title), None);
    }

    #[test]
    fn a_number_field_takes_digits_only() {
        let mut e = editor(&[track(1, "olson", "boa", "geogaddi", None)]);
        focus(&mut e, Field::Year);
        e.typed("zweitausend");
        assert_eq!(row(&e, Field::Year).entry.text(), "");
        e.typed("2oo2");
        assert_eq!(row(&e, Field::Year).entry.text(), "22");
    }

    #[test]
    fn emptying_a_mixed_field_counts_as_an_answer() {
        let mut e = editor(&[
            track(1, "olson", "boa", "geogaddi", Some(2002)),
            track(2, "1969", "boa", "geogaddi", Some(1998)),
        ]);
        focus(&mut e, Field::Year);
        assert!(row(&e, Field::Year).mixed);
        e.backspace();
        assert!(row(&e, Field::Year).touched);
        assert_eq!(e.edit().get(Field::Year), Some(""));
    }

    #[test]
    fn the_form_is_a_ring() {
        let mut e = editor(&[track(1, "olson", "boa", "geogaddi", None)]);
        assert_eq!(e.focus, 0);
        e.step(-1);
        assert_eq!(e.focus, Field::ALL.len() - 1);
        e.step(1);
        assert_eq!(e.focus, 0);
    }

    #[test]
    fn the_caret_walks_letters_not_bytes() {
        let mut entry = Entry::new("tänzer");
        assert_eq!(entry.caret(), 7);
        entry.left();
        entry.left();
        entry.left();
        entry.left();
        assert_eq!(entry.caret(), 3);
        entry.backspace();
        assert_eq!(entry.text(), "tnzer");
        entry.insert("ä");
        assert_eq!(entry.text(), "tänzer");
        entry.home();
        entry.delete();
        assert_eq!(entry.text(), "änzer");
        entry.end();
        assert_eq!(entry.caret(), entry.text().len());
    }

    #[test]
    fn deleting_nothing_does_nothing() {
        let mut entry = Entry::new("");
        assert!(!entry.backspace());
        entry.delete();
        entry.left();
        assert_eq!(entry.caret(), 0);
        assert_eq!(entry.text(), "");
    }
}
