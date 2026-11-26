//! The tag editor's state.
//!
//! Only what is being typed and what it means; the drawing is in
//! [`super::library`] and the writing in [`crate::library::tags`].
//!
//! The one idea worth stating: a field is *touched* or it is not. Opening the
//! editor on twelve tracks of one record shows one album name and twelve
//! titles as "verschieden", and pressing enter must write neither of them —
//! only what the person actually typed. Everything here follows from that.

use crate::db::models::Track;
use crate::library::metadata::Extras;
use crate::library::tags::{Edit, Field};

/// Where the word before byte `at` begins: back over the spaces, then over
/// the word — as option and an arrow or backspace go on a Mac.
pub fn word_before(text: &str, at: usize) -> usize {
    let head = &text[..at];
    let trimmed = head.trim_end();
    trimmed.rfind(char::is_whitespace).map_or(0, |i| i + trimmed[i..].chars().next().map_or(1, char::len_utf8))
}

/// Where the word after byte `at` ends: over the spaces, then over the word.
pub fn word_after(text: &str, at: usize) -> usize {
    let tail = &text[at..];
    let skipped = tail.len() - tail.trim_start().len();
    let rest = &tail[skipped..];
    at + skipped + rest.find(char::is_whitespace).unwrap_or(rest.len())
}

/// A line of text with a caret in it.
///
/// Small on purpose. Words go at once with option, there is no undo, and the
/// only selection is all of it: this sits in a field the size of a track title,
/// where the whole value is visible and retyping it is faster than anything
/// finer. Cmd+a marks the whole value the way it does in every other field on a
/// Mac — the next letter replaces it, backspace takes it away, cmd+c copies it,
/// and an arrow puts the caret at that end instead (until then it emptied the
/// field without a word).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Entry {
    text: String,
    /// Byte offset into `text`, always on a character boundary.
    caret: usize,
    /// All of `text` is marked, and the next key acts on all of it.
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

    /// Mark everything, the way cmd+a does. Nothing to mark in an empty field.
    pub fn mark_all(&mut self) {
        self.marked = !self.text.is_empty();
        self.caret = self.text.len();
    }

    pub fn is_marked(&self) -> bool {
        self.marked
    }

    /// Let go of the mark and leave the text as it is.
    pub fn unmark(&mut self) {
        self.marked = false;
    }

    /// If everything is marked, take it all away and say so.
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

    /// Delete backwards. Returns whether there was anything to delete.
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

    /// Delete the word before the caret and the spaces between — option and
    /// backspace on a Mac. Whether there was anything to delete.
    pub fn backspace_word(&mut self) -> bool {
        if self.take_marked() {
            return true;
        }
        let from = word_before(&self.text, self.caret);
        if from == self.caret {
            return false;
        }
        self.text.replace_range(from..self.caret, "");
        self.caret = from;
        true
    }

    /// Delete the word after the caret and the spaces before it — option and
    /// delete.
    pub fn delete_word(&mut self) {
        if self.take_marked() {
            return;
        }
        let to = word_after(&self.text, self.caret);
        self.text.replace_range(self.caret..to, "");
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

/// One field of the form.
#[derive(Debug, Clone)]
pub struct Row {
    pub field: Field,
    pub entry: Entry,
    /// The tracks disagree about this field, and nothing has been typed yet.
    /// Shown as "verschieden", and left alone when saving.
    pub mixed: bool,
    /// Something has been typed here, so this field will be written.
    pub touched: bool,
}

/// The editor, while it is open.
#[derive(Debug, Clone)]
pub struct Editor {
    /// The tracks being changed, by database id — indexes into a list that a
    /// reload could renumber underneath us.
    pub tracks: Vec<i64>,
    /// Their titles, for the line that says what is being edited.
    pub titles: Vec<String>,
    pub rows: Vec<Row>,
    /// Which row has the keyboard.
    pub focus: usize,
}

/// The [`Extras`] of the tracks about to be edited, read from their files.
/// Not for a selection of thousands: reading every file would hold the
/// window for seconds, and an empty list lets [`Editor::open`] treat those
/// fields as unknown.
pub fn extras_of(tracks: &[&Track]) -> Vec<Extras> {
    const AT_MOST: usize = 400;
    if tracks.len() > AT_MOST {
        return Vec::new();
    }
    tracks.iter().map(|t| crate::library::metadata::read_extras(&t.path)).collect()
}

impl Editor {
    /// Open the editor on a set of tracks.
    ///
    /// A field every track agrees on is shown and can be edited as one value.
    /// A field they disagree on comes up empty and marked.
    ///
    /// `extras` are the files' own [`Extras`], one per track; without them
    /// (see [`extras_of`]) those fields come up as if they differed, so they
    /// are written only where something is typed into them.
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

    /// Move to another field, wrapping at both ends: the form is a ring, and
    /// tab at the bottom going back to the top is what everyone expects.
    pub fn step(&mut self, delta: i32) {
        self.row_mut().entry.unmark();
        let len = self.rows.len() as i32;
        self.focus = (((self.focus as i32 + delta) % len + len) % len) as usize;
    }

    /// Type into the focused field.
    ///
    /// Number fields take digits and nothing else. Refusing the key is kinder
    /// than accepting it and refusing the save five fields later.
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

    /// Delete backwards in the focused field. Emptying a mixed field counts as
    /// typing: it means "make them all empty", which is a real thing to want.
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

    /// The word before the caret in the focused field — option-backspace.
    pub fn backspace_word(&mut self) {
        let row = self.row_mut();
        if row.entry.backspace_word() || row.mixed {
            row.touched = true;
            row.mixed = false;
        }
    }

    /// The word after the caret in the focused field — option-delete.
    pub fn delete_word(&mut self) {
        let row = self.row_mut();
        if !row.entry.is_empty() {
            row.entry.delete_word();
            row.touched = true;
        }
        row.mixed = false;
    }

    /// Mark all of the focused field — cmd+a.
    pub fn mark_all(&mut self) {
        self.row_mut().entry.mark_all();
    }

    /// What is marked in the focused field, for cmd+c.
    pub fn marked(&self) -> Option<&str> {
        let row = self.row();
        row.entry.is_marked().then(|| row.entry.text())
    }

    /// Empty the focused field in one go.
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

    /// What to write: the touched fields, and only those.
    pub fn edit(&self) -> Edit {
        let mut edit = Edit::new();
        for row in self.rows.iter().filter(|r| r.touched) {
            edit.set(row.field, row.entry.text());
        }
        edit
    }

    /// Whether anything would be written at all.
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

    /// Option-backspace takes the word before the caret with the spaces
    /// after it, option-delete the word after with the spaces before.
    #[test]
    fn a_word_goes_at_once() {
        let mut entry = Entry::new("another green  world");
        assert!(entry.backspace_word());
        assert_eq!(entry.text(), "another green  ");
        assert!(entry.backspace_word());
        assert_eq!(entry.text(), "another ");
        entry.home();
        entry.delete_word();
        assert_eq!(entry.text(), " ");
        let mut entry = Entry::new("björk début");
        entry.left();
        entry.left();
        assert!(entry.backspace_word());
        assert_eq!(entry.text(), "björk ut");
        assert_eq!(word_after("a  bc d", 1), 5);
        assert_eq!(word_before("a  bc d", 5), 3);
        let mut empty = Entry::new("");
        assert!(!empty.backspace_word());
    }

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

    /// The point of the whole thing: correcting one field across a record must
    /// not overwrite the twelve fields that differ legitimately.
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

    /// Opening the editor and pressing enter writes nothing at all.
    #[test]
    fn an_untouched_form_is_an_empty_edit() {
        let mut e = editor(&[track(1, "olson", "boa", "geogaddi", Some(2002))]);
        assert!(!e.changed());
        assert!(e.edit().is_empty());
        e.step(1);
        e.step(1);
        assert!(e.edit().is_empty());
    }

    /// Only the field that was typed in gets written.
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

    /// A number field that accepted letters would only fail at the save, by
    /// which time the person has moved on.
    #[test]
    fn a_number_field_takes_digits_only() {
        let mut e = editor(&[track(1, "olson", "boa", "geogaddi", None)]);
        focus(&mut e, Field::Year);
        e.typed("zweitausend");
        assert_eq!(row(&e, Field::Year).entry.text(), "");
        e.typed("2oo2");
        assert_eq!(row(&e, Field::Year).entry.text(), "22");
    }

    /// Emptying a field the tracks disagree about means "make them all empty",
    /// which has to be distinguishable from never having touched it.
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

    /// Tab at the last field goes back to the first.
    #[test]
    fn the_form_is_a_ring() {
        let mut e = editor(&[track(1, "olson", "boa", "geogaddi", None)]);
        assert_eq!(e.focus, 0);
        e.step(-1);
        assert_eq!(e.focus, Field::ALL.len() - 1);
        e.step(1);
        assert_eq!(e.focus, 0);
    }

    /// Caret arithmetic on bytes, with text that is not one byte per letter.
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

    /// Backspace at the start of an empty field must not panic or wrap.
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
