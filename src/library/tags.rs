use std::path::Path;

use lofty::config::WriteOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::prelude::*;
use lofty::probe::Probe;
use lofty::tag::{Tag, TagType};

use crate::db::models::Track;
use crate::library::metadata::Extras;
use crate::error::{KesslerError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Title,
    Artist,
    Album,
    AlbumArtist,
    Year,
    TrackNumber,
    DiscNumber,
    Genre,
    Composer,
    Grouping,
    Bpm,
    Comment,
}

impl Field {
    pub const ALL: [Field; 12] = [
        Field::Title,
        Field::Artist,
        Field::Album,
        Field::AlbumArtist,
        Field::Year,
        Field::TrackNumber,
        Field::DiscNumber,
        Field::Genre,
        Field::Composer,
        Field::Grouping,
        Field::Bpm,
        Field::Comment,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Field::Title => tr!("titel|name"),
            Field::Artist => tr!("künstler:in"),
            Field::Album => "album",
            Field::AlbumArtist => tr!("album-künstler:in"),
            Field::Year => tr!("jahr"),
            Field::TrackNumber => tr!("nr"),
            Field::DiscNumber => tr!("cd"),
            Field::Genre => "genre",
            Field::Composer => tr!("komponist:in"),
            Field::Grouping => tr!("gruppierung"),
            Field::Bpm => "bpm",
            Field::Comment => tr!("kommentar"),
        }
    }

    pub fn numeric(self) -> bool {
        matches!(self, Field::Year | Field::TrackNumber | Field::DiscNumber | Field::Bpm)
    }

    pub fn read(self, track: &Track, extras: &Extras) -> String {
        match self {
            Field::Composer => extras.composer.clone(),
            Field::Grouping => extras.grouping.clone(),
            Field::Bpm => extras.bpm.map(|n| n.to_string()).unwrap_or_default(),
            Field::Comment => extras.comment.clone(),
            Field::Title => track.title.clone(),
            Field::Artist => track.artist.clone(),
            Field::Album => track.album.clone(),
            Field::AlbumArtist => track.album_artist.clone(),
            Field::Genre => track.genre.clone(),
            Field::Year => track.year.map(|y| y.to_string()).unwrap_or_default(),
            Field::TrackNumber => track
                .track_number
                .map(|n| n.to_string())
                .unwrap_or_default(),
            Field::DiscNumber => track.disc_number.map(|n| n.to_string()).unwrap_or_default(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Edit {
    changes: Vec<(Field, String)>,
}

impl Edit {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, field: Field, value: impl Into<String>) {
        let value = value.into().trim().to_string();
        match self.changes.iter_mut().find(|(f, _)| *f == field) {
            Some(slot) => slot.1 = value,
            None => self.changes.push((field, value)),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    pub fn summary(&self) -> String {
        self.changes
            .iter()
            .map(|(field, value)| {
                if value.is_empty() {
                    tr!("{}: leer", field.label())
                } else {
                    format!("{}: {value}", field.label())
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    #[cfg(test)]
    pub fn changes(&self) -> &[(Field, String)] {
        &self.changes
    }

    #[cfg(test)]
    pub fn get(&self, field: Field) -> Option<&str> {
        self.changes
            .iter()
            .find(|(f, _)| *f == field)
            .map(|(_, v)| v.as_str())
    }
}

pub fn check(edit: &Edit) -> Result<()> {
    for (field, value) in &edit.changes {
        if field.numeric() && !value.is_empty() && number(value).is_none() {
            return Err(KesslerError::Metadata(tr!("{} braucht eine zahl, nicht „{}“",
                field.label(),
                value
            )));
        }
    }
    Ok(())
}

fn number(text: &str) -> Option<u32> {
    let text = text.trim();
    if text.is_empty() || !text.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

pub fn write(path: &Path, edit: &Edit) -> Result<()> {
    if edit.is_empty() {
        return Ok(());
    }
    check(edit)?;
    let edited: Vec<Field> = edit.changes.iter().map(|(f, _)| *f).collect();
    safely(path, Keep { edited: &edited, wrote: &edit.changes, cover: false }, |path| write_in_place(path, edit))
}

fn write_in_place(path: &Path, edit: &Edit) -> Result<()> {
    let mut file = open_for_writing(path)?;

    if edit.changes.iter().any(|(_, v)| !v.is_empty()) {
        add_main_tag(&mut file);
    }

    let kinds: Vec<TagType> = file.tags().iter().map(|t| t.tag_type()).collect();
    for kind in kinds {
        if let Some(tag) = file.tag_mut(kind) {
            for (field, value) in &edit.changes {
                apply(tag, *field, value);
            }
        }
    }

    save(file, path)
}

fn add_main_tag(file: &mut lofty::file::TaggedFile) {
    let primary = file.primary_tag_type();
    if file.tag(primary).is_none() {
        let mut fresh = file.first_tag().cloned().unwrap_or_else(|| Tag::new(primary));
        fresh.re_map(primary);
        file.insert_tag(fresh);
    }
}

fn save(mut file: lofty::file::TaggedFile, path: &Path) -> Result<()> {
    let id3v1 = file.tag_mut(TagType::Id3v1).map(|tag| {
        let words = [
            tag.title().map(|s| s.to_string()).unwrap_or_default(),
            tag.artist().map(|s| s.to_string()).unwrap_or_default(),
            tag.album().map(|s| s.to_string()).unwrap_or_default(),
            tag.comment().map(|s| s.to_string()).unwrap_or_default(),
        ];
        for (n, word) in words.iter().enumerate() {
            if word.is_empty() {
                continue;
            }
            let harmless: String = latin1(word, 30).iter().map(|&b| if b.is_ascii() { b as char } else { '?' }).collect();
            match n {
                0 => tag.set_title(harmless),
                1 => tag.set_artist(harmless),
                2 => tag.set_album(harmless),
                _ => tag.set_comment(harmless),
            }
        }
        words
    });
    let id3v2 = file.remove(TagType::Id3v2).map(lofty::id3::v2::Id3v2Tag::from);
    let unwritable = |e: lofty::error::LoftyError| KesslerError::Metadata(tr!("{} lässt sich nicht schreiben: {}", path.display(), e));
    file.save_to_path(path, WriteOptions::default()).map_err(unwritable)?;
    if let Some(tag) = id3v2 {
        tag.save_to_path(path, WriteOptions::default()).map_err(unwritable)?;
    }
    if let Some(words) = id3v1 {
        id3v1_words(path, &words)
            .map_err(|e| KesslerError::Metadata(tr!("{} lässt sich nicht schreiben: {}", path.display(), e)))?;
    }
    Ok(())
}

fn id3v1_words(path: &Path, words: &[String; 4]) -> std::io::Result<()> {
    use std::io::{Read, Seek, SeekFrom, Write};
    let mut file = std::fs::OpenOptions::new().read(true).write(true).open(path)?;
    let length = file.metadata()?.len();
    if length < 131 {
        return Ok(());
    }
    let mut end = [0u8; 131];
    file.seek(SeekFrom::Start(length - 131))?;
    file.read_exact(&mut end)?;
    if &end[3..6] != b"TAG" || &end[..3] == b"APE" {
        return Ok(());
    }
    for (word, (at, size)) in words.iter().zip([(3, 30), (33, 30), (63, 30), (97, 28)]) {
        let mut field = latin1(word, size);
        field.resize(size, 0);
        file.seek(SeekFrom::Start(length - 128 + at))?;
        file.write_all(&field)?;
    }
    Ok(())
}

fn latin1(text: &str, size: usize) -> Vec<u8> {
    text.chars()
        .map(|c| match c {
            '\u{0}'..='\u{ff}' => c as u8,
            '–' | '—' | '‒' | '−' => b'-',
            '“' | '”' | '„' => b'"',
            '‘' | '’' | '‚' => b'\'',
            _ => b'?',
        })
        .take(size)
        .collect()
}

fn open_for_writing(path: &Path) -> Result<lofty::file::TaggedFile> {
    let open = || {
        Probe::open(path)
            .map_err(|e| KesslerError::Metadata(tr!("{} lässt sich nicht öffnen: {}", path.display(), e)))
    };
    let strict = match open()?.read() {
        Ok(file) => return Ok(file),
        Err(e) => e,
    };
    let unreadable = || KesslerError::Metadata(tr!("{} lässt sich nicht lesen: {}", path.display(), strict));
    let raw = std::fs::read(path).map_err(|_| unreadable())?;
    let reads = |bytes: &[u8]| {
        Probe::new(std::io::Cursor::new(bytes))
            .guess_file_type()
            .is_ok_and(|p| p.read().is_ok())
    };
    let mended = super::id3::mend(&raw, reads).ok_or_else(unreadable)?;
    let beside = path.with_extension("kessler-mend");
    std::fs::write(&beside, &mended).and_then(|_| std::fs::rename(&beside, path)).map_err(|e| {
        let _ = std::fs::remove_file(&beside);
        KesslerError::Metadata(tr!("{} lässt sich nicht schreiben: {}", path.display(), e))
    })?;
    tracing::info!("Left a broken ID3v2 frame out of {}", path.display());
    open()?.read().map_err(|_| unreadable())
}

const PICTURE_SIZE: u32 = 1000;
const PICTURE_QUALITY: u8 = 90;

pub fn picture_for_tag(raw: &[u8]) -> Result<Vec<u8>> {
    super::artwork::shrink_to_jpeg(raw, PICTURE_SIZE, PICTURE_QUALITY)
        .ok_or_else(|| KesslerError::Metadata(tr!("das ist kein lesbares bild").into()))
}

pub fn write_picture(path: &Path, jpeg: &[u8]) -> Result<()> {
    safely(path, Keep { edited: &[], wrote: &[], cover: true }, |path| write_picture_in_place(path, jpeg))
}

#[derive(Clone, Copy)]
struct Keep<'a> {
    edited: &'a [Field],
    wrote: &'a [(Field, String)],
    cover: bool,
}

fn safely(path: &Path, keep: Keep, change: impl FnOnce(&Path) -> Result<()>) -> Result<()> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let beside = path.with_file_name(format!(".kessler-{name}"));
    let refused = |why: String| KesslerError::Metadata(tr!("{} bleibt unverändert: {}", path.display(), why));
    let before = crate::library::metadata::read_metadata(path).ok();
    let extras_before = crate::library::metadata::strict_extras(path);

    std::fs::copy(path, &beside).map_err(|e| refused(e.to_string()))?;
    let checked = change(&beside).and_then(|()| {
        let readable = Probe::open(&beside)
            .and_then(|p| p.read())
            .map_err(|e| refused(tr!("danach nicht mehr lesbar ({})", e)))?;
        let after = crate::library::metadata::read_metadata(&beside).map_err(|e| refused(e.to_string()))?;
        if let Some(before) = &before {
            let was = before.duration_secs;
            let is = readable.properties().duration().as_secs_f64();
            if was > 1.0 && (is - was).abs() > 1.0 {
                return Err(refused(tr!("die länge wäre danach {} statt {} sekunden", is.round() as i64, was.round() as i64)));
            }
            let extras_after = crate::library::metadata::read_extras(&beside);
            let none = Extras::default();
            let (was_extras, is_extras) = match &extras_before {
                Some(extras) => (extras, &extras_after),
                None => (&none, &none),
            };
            let changed = Field::ALL.iter().find(|field| {
                !keep.edited.contains(field) && field.read(before, was_extras) != field.read(&after, is_extras)
            });
            let lost = keep.wrote.iter().find(|(field, value)| {
                let typed = if field.numeric() { number(value).map(|n| n.to_string()).unwrap_or_default() } else { value.clone() };
                field.read(&after, &extras_after) != typed
            });
            if let Some((field, value)) = lost {
                return Err(refused(tr!("{} wäre danach „{}“ statt „{}“",
                    field.label(),
                    field.read(&after, &extras_after),
                    value
                )));
            }
            if let Some(field) = changed {
                return Err(refused(tr!("{} wäre danach „{}“ statt „{}“",
                    field.label(),
                    field.read(&after, is_extras),
                    field.read(before, was_extras)
                )));
            }
        }
        if keep.cover && !after.has_artwork {
            return Err(refused(tr!("dieses format nimmt kein cover auf").to_string()));
        }
        Ok(())
    });
    match checked {
        Ok(()) => std::fs::rename(&beside, path).map_err(|e| {
            let _ = std::fs::remove_file(&beside);
            refused(e.to_string())
        }),
        Err(e) => {
            let _ = std::fs::remove_file(&beside);
            tracing::warn!("{e}");
            Err(e)
        }
    }
}

fn write_picture_in_place(path: &Path, jpeg: &[u8]) -> Result<()> {
    let mut file = open_for_writing(path)?;

    add_main_tag(&mut file);
    let holds_pictures = |kind: TagType| {
        matches!(kind, TagType::Id3v2 | TagType::VorbisComments | TagType::Mp4Ilst | TagType::Ape)
    };

    let kinds: Vec<TagType> = file.tags().iter().map(|t| t.tag_type()).collect();
    for kind in kinds {
        if !holds_pictures(kind) {
            continue;
        }
        if let Some(tag) = file.tag_mut(kind) {
            tag.remove_picture_type(PictureType::CoverFront);
            tag.push_picture(Picture::new_unchecked(
                PictureType::CoverFront,
                Some(MimeType::Jpeg),
                None,
                jpeg.to_vec(),
            ));
        }
    }

    save(file, path)
}

fn apply(tag: &mut Tag, field: Field, value: &str) {
    let empty = value.is_empty();
    match field {
        Field::Title => {
            if empty {
                tag.remove_title()
            } else {
                tag.set_title(value.to_string())
            }
        }
        Field::Artist => {
            if empty {
                tag.remove_artist()
            } else {
                tag.set_artist(value.to_string())
            }
        }
        Field::Album => {
            if empty {
                tag.remove_album()
            } else {
                tag.set_album(value.to_string())
            }
        }
        Field::Genre => {
            if empty {
                tag.remove_genre()
            } else {
                tag.set_genre(value.to_string())
            }
        }
        Field::AlbumArtist => {
            if empty {
                tag.remove_key(&ItemKey::AlbumArtist);
            } else {
                tag.insert_text(ItemKey::AlbumArtist, value.to_string());
            }
        }
        Field::Year => match number(value) {
            Some(n) => {
                tag.set_year(n);
                if tag.get_string(&ItemKey::Year).is_some() {
                    tag.insert_text(ItemKey::Year, n.to_string());
                }
            }
            None => tag.remove_year(),
        },
        Field::TrackNumber => match number(value) {
            Some(n) => tag.set_track(n),
            None => tag.remove_track(),
        },
        Field::DiscNumber => match number(value) {
            Some(n) => tag.set_disk(n),
            None => tag.remove_disk(),
        },
        Field::Composer => text(tag, ItemKey::Composer, value),
        Field::Grouping => {
            tag.remove_key(&ItemKey::AppleId3v2ContentGroup);
            text(tag, ItemKey::ContentGroup, value);
        }
        Field::Bpm => {
            text(tag, ItemKey::IntegerBpm, value);
            text(tag, ItemKey::Bpm, value);
        }
        Field::Comment => {
            tag.retain(|item| !(item.key() == &ItemKey::Comment && item.description().is_empty()));
            if !empty {
                tag.push(lofty::tag::TagItem::new(ItemKey::Comment, lofty::tag::ItemValue::Text(value.to_string())));
            }
        }
    }
}

fn text(tag: &mut Tag, key: ItemKey, value: &str) {
    if value.is_empty() {
        tag.remove_key(&key);
    } else {
        tag.insert_text(key, value.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::super::fixtures::{bytes, FLAC, MP3};

    #[test]
    fn a_write_that_would_break_the_file_leaves_it_alone() {
        let path = scratch("safely", MP3, "mp3");
        write(&path, &Edit { changes: vec![(Field::Album, "Harvest".into())] }).unwrap();
        let before = std::fs::read(&path).unwrap();

        let refused = safely(&path, Keep { edited: &[], wrote: &[], cover: false }, |copy| {
            std::fs::write(copy, b"not a sound file any more").map_err(KesslerError::from)
        });
        assert!(refused.is_err(), "a broken copy must not be put in place");
        assert_eq!(std::fs::read(&path).unwrap(), before, "the original is untouched");
        let dir = path.parent().unwrap();
        let left: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with(".kessler-"))
            .collect();
        assert!(left.is_empty(), "the copy is cleared away: {left:?}");

        write(&path, &Edit { changes: vec![(Field::Album, "The Harvest".into())] }).unwrap();
        assert_eq!(crate::library::metadata::read_metadata(&path).unwrap().album, "The Harvest");
    }

    #[test]
    fn an_id3v1_tag_is_written_in_latin1() {
        let mut raw = bytes(MP3);
        let mut v1 = b"TAG".to_vec();
        for word in ["Kyema", "Eliane Radigue", "Trilogie"] {
            let mut field = word.as_bytes().to_vec();
            field.resize(30, 0);
            v1.extend_from_slice(&field);
        }
        v1.extend_from_slice(b"1998");
        v1.resize(128, 0);
        v1[127] = 255;
        raw.extend_from_slice(&v1);
        let path = scratch("id3v1", MP3, "mp3");
        std::fs::write(&path, raw).unwrap();

        let title = "Trilogie de la mort – für Für Kyema";
        let mut edit = Edit::new();
        edit.set(Field::Title, title);
        edit.set(Field::Artist, "Éliane Radigue");
        edit.set(Field::Composer, "Éliane Radigue");
        edit.set(Field::Comment, "für Kim");
        write(&path, &edit).unwrap();

        let after = crate::library::metadata::read_metadata(&path).unwrap();
        assert_eq!((after.title.as_str(), after.artist.as_str(), after.album.as_str()), (title, "Éliane Radigue", "Trilogie"));
        let extras = crate::library::metadata::read_extras(&path);
        assert_eq!((extras.composer.as_str(), extras.comment.as_str()), ("Éliane Radigue", "für Kim"));
        let written = std::fs::read(&path).unwrap();
        let v1 = &written[written.len() - 128..];
        assert_eq!(&v1[..3], b"TAG");
        assert_eq!(&v1[3..33], b"Trilogie de la mort - f\xfcr F\xfcr ");
        assert_eq!(&v1[33..48], b"\xc9liane Radigue\0");
        assert_eq!(&v1[97..104], b"f\xfcr Kim");
    }

    #[test]
    fn a_tag_with_a_broken_frame_can_still_be_written() {
        fn synchsafe(n: usize) -> [u8; 4] {
            [((n >> 21) & 0x7f) as u8, ((n >> 14) & 0x7f) as u8, ((n >> 7) & 0x7f) as u8, (n & 0x7f) as u8]
        }
        fn text(id: &[u8], words: &str) -> Vec<u8> {
            let mut data = vec![0u8];
            data.extend_from_slice(words.as_bytes());
            let mut out = id.to_vec();
            out.extend_from_slice(&synchsafe(data.len()));
            out.extend_from_slice(&[0, 0]);
            out.extend_from_slice(&data);
            out
        }
        let mut broken = b"TXXX".to_vec();
        broken.extend_from_slice(&synchsafe(4));
        broken.extend_from_slice(&[0, 0x01]);
        broken.extend_from_slice(&[0, 0, 0, 4]);
        let body = [text(b"TPE1", "Nagelfar"), text(b"TALB", "Virus West"), broken].concat();
        let mut raw = b"ID3".to_vec();
        raw.extend_from_slice(&[4, 0, 0]);
        raw.extend_from_slice(&synchsafe(body.len()));
        raw.extend_from_slice(&body);
        raw.extend_from_slice(&bytes(MP3));
        let path = scratch("broken-frame", MP3, "mp3");
        std::fs::write(&path, raw).unwrap();
        assert!(Probe::open(&path).unwrap().read().is_err());

        let mut edit = Edit::default();
        edit.changes.push((Field::Title, "Sturm der Katharsis".to_string()));
        write(&path, &edit).expect("written despite the broken frame");

        let file = Probe::open(&path).unwrap().read().expect("reads strictly now");
        let tag = file.primary_tag().unwrap();
        assert_eq!(tag.title().as_deref(), Some("Sturm der Katharsis"));
        assert_eq!(tag.artist().as_deref(), Some("Nagelfar"));
        assert_eq!(tag.album().as_deref(), Some("Virus West"));
    }

    fn scratch(name: &str, hex: &str, ext: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("kessler-tags-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join(format!("track.{ext}"));
        std::fs::write(&file, bytes(hex)).unwrap();
        file
    }

    fn read(path: &Path) -> Track {
        super::super::metadata::read_metadata(path).unwrap()
    }

    fn edit(pairs: &[(Field, &str)]) -> Edit {
        let mut edit = Edit::new();
        for (field, value) in pairs {
            edit.set(*field, *value);
        }
        edit
    }

    #[test]
    fn what_was_typed_is_what_the_file_says() {
        for (name, bytes, ext) in [("mp3", MP3, "mp3"), ("flac", FLAC, "flac")] {
            let path = scratch(name, bytes, ext);
            write(
                &path,
                &edit(&[
                    (Field::Title, "roygbiv"),
                    (Field::Artist, "boards of canada"),
                    (Field::Album, "music has the right to children"),
                    (Field::AlbumArtist, "boards of canada"),
                    (Field::Year, "1998"),
                    (Field::TrackNumber, "9"),
                    (Field::DiscNumber, "1"),
                    (Field::Genre, "electronic"),
                ]),
            )
            .unwrap();

            let track = read(&path);
            assert_eq!(track.title, "roygbiv", "{name}");
            assert_eq!(track.artist, "boards of canada", "{name}");
            assert_eq!(track.album, "music has the right to children", "{name}");
            assert_eq!(track.album_artist, "boards of canada", "{name}");
            assert_eq!(track.genre, "electronic", "{name}");
            assert_eq!(track.year, Some(1998), "{name}");
            assert_eq!(track.track_number, Some(9), "{name}");
            assert_eq!(track.disc_number, Some(1), "{name}");
        }
    }

    #[test]
    fn a_file_without_a_tag_gets_one() {
        let path = scratch("untagged", MP3, "mp3");
        assert_eq!(read(&path).title, "");
        write(&path, &edit(&[(Field::Title, "sixtyten")])).unwrap();
        assert_eq!(read(&path).title, "sixtyten");
    }

    #[test]
    fn the_fields_left_out_are_left_alone() {
        let path = scratch("partial", FLAC, "flac");
        write(
            &path,
            &edit(&[(Field::Title, "olson"), (Field::Artist, "boards of canada")]),
        )
        .unwrap();
        write(&path, &edit(&[(Field::Album, "geogaddi")])).unwrap();

        let track = read(&path);
        assert_eq!(track.title, "olson");
        assert_eq!(track.artist, "boards of canada");
        assert_eq!(track.album, "geogaddi");
    }

    #[test]
    fn an_emptied_field_is_removed() {
        let path = scratch("empty", MP3, "mp3");
        write(
            &path,
            &edit(&[(Field::Genre, "ambient"), (Field::Year, "2002")]),
        )
        .unwrap();
        assert_eq!(read(&path).genre, "ambient");

        write(&path, &edit(&[(Field::Genre, ""), (Field::Year, "")])).unwrap();
        let track = read(&path);
        assert_eq!(track.genre, "");
        assert_eq!(track.year, None);
    }

    #[test]
    fn a_year_that_is_not_a_number_writes_nothing() {
        let path = scratch("badyear", MP3, "mp3");
        write(&path, &edit(&[(Field::Title, "dayvan cowboy")])).unwrap();
        let before = std::fs::read(&path).unwrap();

        let bad = edit(&[(Field::Title, "chromakey dreamcoat"), (Field::Year, "neunzehn")]);
        assert!(write(&path, &bad).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(read(&path).title, "dayvan cowboy");
    }

    #[test]
    fn a_number_is_a_number_and_nothing_else() {
        assert_eq!(number("9"), Some(9));
        assert_eq!(number(" 9 "), Some(9));
        assert_eq!(number(""), None);
        assert_eq!(number("3/12"), None);
        assert_eq!(number("-1"), None);
        assert_eq!(number("١٩٩٨"), None);
    }

    fn picture(size: u32) -> Vec<u8> {
        let square = image::RgbImage::from_fn(size, size, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, 90])
        });
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(square)
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    #[test]
    fn a_cover_goes_into_the_file_and_comes_back() {
        for (name, hex, ext) in [("mp3cover", MP3, "mp3"), ("flaccover", FLAC, "flac")] {
            let path = scratch(name, hex, ext);
            let jpeg = picture_for_tag(&picture(300)).unwrap();
            write_picture(&path, &jpeg).unwrap();

            let back = super::super::artwork::embedded(&path).unwrap();
            assert_eq!(back, jpeg, "{name}");
            assert!(read(&path).has_artwork, "{name}");
        }
    }

    #[test]
    fn a_second_cover_replaces_the_first() {
        let path = scratch("recover", MP3, "mp3");
        write_picture(&path, &picture_for_tag(&picture(200)).unwrap()).unwrap();
        let second = picture_for_tag(&picture(360)).unwrap();
        write_picture(&path, &second).unwrap();

        let file = lofty::probe::Probe::open(&path).unwrap().read().unwrap();
        let tag = file.primary_tag().unwrap();
        assert_eq!(tag.pictures().len(), 1);
        assert_eq!(tag.pictures()[0].data(), second.as_slice());
    }

    #[test]
    fn a_cover_does_not_disturb_the_words() {
        let path = scratch("bothways", FLAC, "flac");
        write(&path, &edit(&[(Field::Title, "dayvan cowboy")])).unwrap();
        write_picture(&path, &picture_for_tag(&picture(200)).unwrap()).unwrap();
        assert_eq!(read(&path).title, "dayvan cowboy");

        write(&path, &edit(&[(Field::Album, "the campfire headphase")])).unwrap();
        assert!(read(&path).has_artwork);
    }

    #[test]
    fn something_that_is_not_a_picture_is_refused() {
        assert!(picture_for_tag(b"<html>404 not found</html>").is_err());
        assert!(picture_for_tag(&[]).is_err());
    }

    #[test]
    fn a_large_picture_is_brought_down_to_size() {
        let jpeg = picture_for_tag(&picture(1600)).unwrap();
        let back = image::load_from_memory(&jpeg).unwrap();
        assert_eq!(back.width().max(back.height()), PICTURE_SIZE);
        let small = picture_for_tag(&picture(250)).unwrap();
        assert_eq!(image::load_from_memory(&small).unwrap().width(), 250);
    }

    #[test]
    fn values_are_trimmed_on_the_way_in() {
        let mut e = Edit::new();
        e.set(Field::Album, "  geogaddi  ");
        assert_eq!(e.get(Field::Album), Some("geogaddi"));
    }

    #[test]
    fn the_last_value_of_a_field_wins() {
        let mut e = Edit::new();
        e.set(Field::Title, "olson");
        e.set(Field::Title, "1969");
        assert_eq!(e.changes().len(), 1);
        assert_eq!(e.get(Field::Title), Some("1969"));
    }

    #[test]
    fn a_year_written_twice_is_changed_in_both_places() {
        let path = scratch("year-twice", FLAC, "flac");
        {
            let mut file = Probe::open(&path).unwrap().read().unwrap();
            if file.tags().is_empty() {
                let kind = file.primary_tag_type();
                file.insert_tag(Tag::new(kind));
            }
            let tag = file.primary_tag_mut().unwrap();
            tag.insert_text(ItemKey::Year, "1990".into());
            tag.insert_text(ItemKey::RecordingDate, "1990-05-01".into());
            file.save_to_path(&path, WriteOptions::default()).unwrap();
        }
        assert_eq!(read(&path).year, Some(1990));

        write(&path, &edit(&[(Field::Year, "1998")])).unwrap();
        assert_eq!(read(&path).year, Some(1998));

        let file = Probe::open(&path).unwrap().read().unwrap();
        let tag = file.primary_tag().unwrap();
        assert_eq!(tag.get_string(&ItemKey::Year), Some("1998"));
        assert_eq!(tag.get_string(&ItemKey::RecordingDate), Some("1998-05-01"));
    }

    #[test]
    fn a_field_of_ones_own_is_kept() {
        let path = scratch("own-field", FLAC, "flac");
        {
            let mut own = lofty::ogg::VorbisComments::default();
            own.push("MYFIELD".into(), "kept".into());
            own.push("TITLE".into(), "before".into());
            own.save_to_path(&path, WriteOptions::default()).unwrap();
        }
        let found = |path: &std::path::Path| {
            let file = Probe::open(path).unwrap().read().unwrap();
            let own = file.primary_tag().unwrap().items().find_map(|item| match item.key() {
                ItemKey::Unknown(name) if name == "MYFIELD" => item.value().text().map(str::to_string),
                _ => None,
            });
            own
        };
        assert_eq!(found(&path).as_deref(), Some("kept"));

        write(&path, &edit(&[(Field::Title, "after")])).unwrap();
        assert_eq!(read(&path).title, "after");
        assert_eq!(found(&path).as_deref(), Some("kept"));
    }
}
