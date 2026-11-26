//! Copying tracks from the local library onto an iPod.
//!
//! Deliberately **additive**: tracks already on the device are kept, whether or
//! not they are still in the library. Nothing is deleted from an iPod. Removing
//! tracks means deleting files a user may have no other copy of, and that is
//! not something to get subtly wrong on the first version.
//!
//! The order of operations matters. Files are copied first and the database is
//! rewritten last, via a temporary file that is renamed into place — so an
//! interrupted sync leaves the old, working database intact and at worst a few
//! unreferenced files on the device.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::db::models::Track;
use crate::error::{KesslerError, Result};

use super::device_info::{DeviceInfo, Generation};
use super::itunesdb::types::{IpodTrack, ItunesDb};
use super::itunesdb::writer;
use super::IpodDevice;

/// Identity used to decide whether a library track is already on the device.
///
/// Paths are useless here — the same song has a different path on each side —
/// and file hashes would mean reading every byte of both libraries.
///
/// The fourth component is *duration*, not file size. A FLAC transcoded to MP3
/// on its way to the iPod arrives at a completely different size, and matching
/// on size would copy it again on every single sync. Duration survives the
/// conversion. Whole seconds, because encoders disagree in the milliseconds.
fn fingerprint(artist: &str, album: &str, title: &str, seconds: u64) -> String {
    format!(
        "{}\u{1}{}\u{1}{}\u{1}{}",
        tidy(artist).to_lowercase(),
        tidy(album).to_lowercase(),
        tidy(title).to_lowercase(),
        seconds
    )
}

fn library_fingerprint(track: &Track) -> String {
    fingerprint(
        track.display_artist(),
        &track.album,
        track.display_title(),
        track.duration_secs.round().max(0.0) as u64,
    )
}

fn ipod_fingerprint(track: &IpodTrack) -> String {
    fingerprint(
        &track.artist,
        &track.album,
        &track.title,
        ((track.length_ms as f64) / 1000.0).round() as u64,
    )
}

/// What a sync would do, worked out before anything is touched.
#[derive(Debug, Default)]
pub struct SyncPlan {
    /// Library tracks that are not on the device yet.
    pub to_copy: Vec<Track>,
    /// Library tracks already present, matched by fingerprint.
    pub already_present: usize,
    /// Tracks on the device that the library does not have. These are kept.
    pub device_only: usize,
    /// Their ids on the device, for the report.
    pub device_only_ids: Vec<u32>,
    /// Of those, the ones whose names the library spells another way — case,
    /// accents, spaces — with the spelling they get: artist, album artist,
    /// album, genre. Nothing else of theirs is touched.
    pub respell: Vec<(u32, [String; 4])>,
    /// Library tracks whose file has gone missing since the last scan.
    pub missing_files: Vec<PathBuf>,
    /// Tracks in a format this iPod cannot play. Copying them would waste
    /// space on files the device never lists.
    pub incompatible: Vec<Track>,
    /// Bytes the copy would add to the device.
    pub bytes_to_copy: u64,
    /// Playlists the sync writes on the device because the one there is not
    /// what it should be, or is not there at all.
    pub lists: Vec<DeviceList>,
    /// Tracks already on the device whose tags the library has changed since:
    /// the device's track id and the library's track, whose tags it gets.
    pub retag: Vec<(u32, Track)>,
    /// Tracks written by an older Kessler, found again, and the library id
    /// their persistent id will carry from now on — see [`KESSLER_MARK`].
    pub rekey: Vec<(u32, i64)>,
    /// Sound Check for the library's tracks, by library id, where their
    /// loudness is known — see [`soundcheck`]. Written with a copy, and a
    /// track already on the device whose value differs is retagged for it.
    pub soundcheck: std::collections::HashMap<i64, u32>,
    /// Of `retag`, how many only for Sound Check — their tags are as they
    /// were.
    pub sound_only: usize,
    /// The device's own playlists, by name, that go: every one but the
    /// library's and those Kessler keeps. Only a sync of the whole library
    /// fills this in.
    pub drop_lists: Vec<String>,
    /// Episodes of podcasts kessler put on the device that are no longer
    /// among the ones wanted there — heard, archived, their file let go of:
    /// the device's ids of them. They go from the database and their files
    /// from the disk — see [`plan_podcasts`]. The one thing a sync deletes.
    pub drop_tracks: Vec<u32>,
    /// The device's database is not in the order its menus need — written
    /// by an older kessler, or by someone else — and is written again even with
    /// nothing to copy, so the 3G lists each artist and record once (see
    /// `writer::index_mhod`).
    pub reorder: bool,
}

/// A playlist Kessler keeps on the device by itself — „zuletzt hinzugefügt".
///
/// Written whole each time it changes, in place of the one of the same name:
/// the device's copy is a picture of the library's newest tracks, not a list
/// anybody edits there. `also_named` are the names it may have been written
/// under before — the other language's — so a window switched to English does
/// not leave a second list behind on the device.
#[derive(Debug, Clone)]
pub struct DeviceList {
    pub name: String,
    pub also_named: Vec<String>,
    /// In the order they are to stand, newest first.
    pub tracks: Vec<Track>,
}

impl DeviceList {
    fn answers_to(&self, name: &str) -> bool {
        self.name == name || self.also_named.iter().any(|n| n == name)
    }
}

impl SyncPlan {
    pub fn is_empty(&self) -> bool {
        self.to_copy.is_empty()
            && self.lists.is_empty()
            && self.retag.is_empty()
            && self.respell.is_empty()
            && self.drop_lists.is_empty()
            && self.drop_tracks.is_empty()
            && !self.reorder
    }

    /// Take only the first few tracks of the plan.
    ///
    /// For the first write to a device nobody has written to before: copy one
    /// album, unplug it, look at the screen. A plan that has been cut short is
    /// still a whole plan — the byte count follows it, or the interface would
    /// promise a gigabyte and copy a song.
    pub fn first(mut self, most: usize) -> Self {
        if self.to_copy.len() <= most {
            return self;
        }
        self.to_copy.truncate(most);
        self.bytes_to_copy = self.to_copy.iter().map(|t| t.file_size).sum();
        self
    }

    /// Distinct source formats among the tracks this device cannot play.
    pub fn incompatible_formats(&self) -> Vec<String> {
        let mut formats: Vec<String> = self
            .incompatible
            .iter()
            .map(|t| t.format.to_lowercase())
            .collect();
        formats.sort();
        formats.dedup();
        formats
    }
}

/// The high half of the persistent id of a track Kessler copies: the low
/// half is the library's own id for it. So a track can be found again on the
/// device after its tags have changed in the library — by who it is, not by
/// what it is called. („KESS" in ASCII.)
pub const KESSLER_MARK: u64 = 0x4B45_5353_0000_0000;

/// The high half older Kessler versions stamped their copies with, the low
/// half being the device's track id — which says nothing about the library.
/// Such a track, found again by its tags, is re-stamped with [`KESSLER_MARK`].
const OLD_MARK: u64 = 0xC7A7_E01D_0000_0000;

const MARK_MASK: u64 = 0xFFFF_FFFF_0000_0000;

/// The high half of the persistent id of a podcast's episode kessler
/// copies („KPOD"): what tells the episodes it put there, and may take off
/// again, from everything else on the device.
pub const PODCAST_MARK: u64 = 0x4B50_4F44_0000_0000;

/// The episodes kessler put on the device that are not in `list` any more
/// go: into [`SyncPlan::drop_tracks`]. Only its own, by [`PODCAST_MARK`] —
/// an episode somebody else put there is nobody's to delete.
pub fn plan_podcasts(plan: &mut SyncPlan, list: &DeviceList, device_db: &ItunesDb) {
    let wanted: HashSet<String> = list.tracks.iter().map(library_fingerprint).collect();
    plan.drop_tracks = device_db
        .tracks
        .iter()
        .filter(|track| track.dbid & MARK_MASK == PODCAST_MARK && !wanted.contains(&ipod_fingerprint(track)))
        .map(|track| track.id)
        .collect();
}

/// The library id a device track carries, if Kessler copied it.
fn library_id_of(track: &IpodTrack) -> Option<i64> {
    (track.dbid & MARK_MASK == KESSLER_MARK).then_some((track.dbid & 0xFFFF_FFFF) as i64)
}

/// Which of `library` a device track is: by the library id kessler stamped
/// it with, else by its tags and length. For plays heard on the iPod — see
/// `playcounts`.
pub fn library_index(track: &IpodTrack, library: &[Track]) -> Option<usize> {
    if let Some(id) = library_id_of(track) {
        if let Some(at) = library.iter().position(|t| t.id == id) {
            return Some(at);
        }
    }
    let wanted = ipod_fingerprint(track);
    library.iter().position(|t| library_fingerprint(t) == wanted)
}

/// A name as it is compared when looking for a retagged track: lower case,
/// letters and figures only.
fn plain(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Whether two titles are one title spelt two ways: the same once plain, or
/// a few letters apart — a typo put right, a „(remastered)" taken off.
fn near(a: &str, b: &str) -> bool {
    let (a, b) = (plain(a), plain(b));
    if a == b || (!a.is_empty() && !b.is_empty() && (a.starts_with(&b) || b.starts_with(&a))) {
        return true;
    }
    let most = (a.chars().count().max(b.chars().count()) / 5).max(2);
    edit_distance(&a, &b) <= most
}

fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut last = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let was = row[j + 1];
            row[j + 1] = (row[j + 1] + 1).min(row[j] + 1).min(last + usize::from(ca != *cb));
            last = was;
        }
    }
    row[b.len()]
}

/// The library's tags as the device stores them — with nothing where the
/// library has nothing, rather than the stand-ins it shows in the window (the
/// file's name), which would overwrite a real name on the device. A name as it
/// goes onto the device: trimmed, each run of spaces one space, and nothing
/// left that cannot be seen. The iPod groups by the exact text, so „mr oizo "
/// with a space at its end, or with a zero-width character a tag editor left in
/// it, stood beside „mr oizo" as an artist of its own.
fn tidy(text: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    let seen: String = text
        .nfc()
        .filter(|c| !matches!(c, '\u{200B}'..='\u{200D}' | '\u{FEFF}' | '\u{00AD}'))
        .collect();
    seen.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// One spelling for every name, as the library's own columns show it.
///
/// The library takes „Qluster" and „qluster" — or „Amon Düül" and „Amon Duul" —
/// for one artist (`crate::db::sort_key`), and shows her under the spelling
/// most of her tracks carry. The iPod compares names as written: every spelling
/// stood in its menus as an artist of its own, her records split between them
/// (found with the report). So a name goes onto the device the way the library
/// shows it — the most frequent spelling, ties alphabetically, as
/// `Database::get_artists` chooses — for artists (and album artists, which are
/// the same names), albums and genres. The files keep theirs.
pub struct Spellings {
    chosen: std::collections::HashMap<(u8, String), String>,
}

const NAMES: u8 = 0;
const ALBUMS: u8 = 1;
const GENRES: u8 = 2;

impl Spellings {
    pub fn of(tracks: &[Track]) -> Self {
        use std::collections::HashMap;
        let mut counts: HashMap<(u8, String), HashMap<String, usize>> = HashMap::new();
        let mut count = |pool: u8, text: &str| {
            let written = tidy(text);
            if written.is_empty() {
                return;
            }
            let key = crate::db::sort_key(&written);
            *counts.entry((pool, key)).or_default().entry(written).or_default() += 1;
        };
        for track in tracks {
            count(NAMES, &track.artist);
            count(NAMES, &track.album_artist);
            count(ALBUMS, &track.album);
            count(GENRES, &track.genre);
        }
        let chosen = counts
            .into_iter()
            .filter_map(|(key, spellings)| {
                let best = spellings
                    .into_iter()
                    .max_by(|(a, n), (b, m)| n.cmp(m).then_with(|| b.cmp(a)))?
                    .0;
                Some((key, best))
            })
            .collect();
        Spellings { chosen }
    }

    fn spell(&self, pool: u8, text: &str) -> String {
        let written = tidy(text);
        self.chosen
            .get(&(pool, crate::db::sort_key(&written)))
            .cloned()
            .unwrap_or(written)
    }

    /// The track with its names spelled the one way.
    pub fn applied(&self, track: &Track) -> Track {
        let mut track = track.clone();
        track.artist = self.spell(NAMES, &track.artist);
        track.album_artist = self.spell(NAMES, &track.album_artist);
        track.album = self.spell(ALBUMS, &track.album);
        track.genre = self.spell(GENRES, &track.genre);
        track
    }
}

fn device_tags(track: &Track) -> (String, String, String, String, String, u32, u32, u32) {
    let title = if track.title.is_empty() { "" } else { track.display_title() };
    let artist = if track.artist.is_empty() && track.album_artist.is_empty() {
        ""
    } else {
        track.display_artist()
    };
    (
        tidy(title),
        tidy(artist),
        tidy(&track.album),
        tidy(&track.album_artist),
        tidy(&track.genre),
        track.year.unwrap_or(0).max(0) as u32,
        track.track_number.unwrap_or(0),
        track.disc_number.unwrap_or(0),
    )
}

/// Whether the device's entry says something other than the library does.
/// A field the library leaves empty is not held against the device: an
/// empty genre in the library is no reason to take one off the iPod.
fn tags_differ(device: &IpodTrack, library: &Track) -> bool {
    !tag_changes(device, library).is_empty()
}

/// The fields [`tags_differ`] finds: each one's name, as the device has it,
/// and as the library would write it. For the report, so a retag that comes
/// back after every sync says which field will not settle.
pub fn tag_changes(device: &IpodTrack, library: &Track) -> Vec<(&'static str, String, String)> {
    let (title, artist, album, album_artist, genre, year, number, disc) = device_tags(library);
    let mut changes = Vec::new();
    for (field, mine, theirs) in [
        ("titel", title, &device.title),
        ("künstler:in", artist, &device.artist),
        ("album", album, &device.album),
        ("albumkünstler:in", album_artist, &device.album_artist),
        ("genre", genre, &device.genre),
    ] {
        let same = mine == *theirs || {
            use unicode_normalization::UnicodeNormalization;
            mine.nfc().eq(theirs.nfc())
        };
        if !same && (!mine.is_empty() || blank(theirs)) {
            changes.push((field, theirs.clone(), mine));
        }
    }
    for (field, mine, theirs) in [
        ("jahr", year, device.year),
        ("nummer", number, device.track_number),
        ("cd", disc, device.disc_number),
    ] {
        if mine != 0 && mine != theirs {
            changes.push((field, theirs.to_string(), mine.to_string()));
        }
    }
    changes
}

/// Spaces and nothing else: no name, but an entry of its own in the device's
/// menus beside the empty one. The library's empty field clears it.
fn blank(text: &str) -> bool {
    !text.is_empty() && text.trim().is_empty()
}

/// Give a device track the library's tags, field by field, leaving what the
/// library has nothing for — see [`tags_differ`].
fn take_tags(device: &mut IpodTrack, library: &Track) {
    let (title, artist, album, album_artist, genre, year, number, disc) = device_tags(library);
    let text = |into: &mut String, mine: String| {
        if !mine.is_empty() || blank(into) {
            *into = mine;
        }
    };
    text(&mut device.title, title);
    text(&mut device.artist, artist);
    text(&mut device.album, album);
    text(&mut device.album_artist, album_artist);
    text(&mut device.genre, genre);
    let figure = |into: &mut u32, mine: u32| {
        if mine != 0 {
            *into = mine;
        }
    };
    figure(&mut device.year, year);
    figure(&mut device.track_number, number);
    figure(&mut device.disc_number, disc);
    device.date_modified = Some(library.date_modified.and_utc().timestamp());
}

/// Work out what would be copied, without touching the device.
///
/// `generation` decides which formats are usable; tracks in anything else are
/// set aside rather than copied.
///
/// A library track is looked for on the device three ways, in order: by the
/// library id Kessler stamped its copy with; by its tags and length (the
/// fingerprint); and — for one whose tags have changed since it was copied — by
/// its length and most of its tags, when exactly one track on the device that
/// nothing else has claimed fits. The last is done after the first two for
/// every track, so a record's other songs have already claimed theirs. A track
/// found by either of the last two whose tags differ is retagged on the device
/// rather than copied again.
pub fn plan(
    library: &[Track],
    device_db: &ItunesDb,
    generation: Generation,
    soundcheck: &std::collections::HashMap<i64, u32>,
) -> SyncPlan {
    plan_in(library, library, device_db, generation, soundcheck)
}

/// [`plan`] for some of the library — a selection, an artist — with the
/// names spelled as the `whole` of it spells them, so sending one record
/// does not write a spelling the next whole sync would take back.
pub fn plan_in(
    library: &[Track],
    whole: &[Track],
    device_db: &ItunesDb,
    generation: Generation,
    soundcheck: &std::collections::HashMap<i64, u32>,
) -> SyncPlan {
    let spellings = Spellings::of(whole);
    let library: Vec<Track> = library.iter().map(|t| spellings.applied(t)).collect();
    let library = library.as_slice();
    let tracks = &device_db.tracks;
    let (found, claimed) = match_library(library, tracks);
    let mut plan = SyncPlan { soundcheck: soundcheck.clone(), ..SyncPlan::default() };
    let master = device_db.playlists.iter().find(|p| p.is_master).map(|p| &p.track_ids);
    let laid_out = device_db
        .playlists
        .iter()
        .find(|p| p.is_master)
        .is_some_and(|p| p.indexed == writer::INDEX_FIELDS && p.jumps == 0)
        && device_db.tracks.windows(2).all(|w| w[0].id < w[1].id);
    let listed: Vec<u32> = tracks.iter().map(|t| t.id).collect();
    plan.reorder = !generation.is_shuffle()
        && !tracks.is_empty()
        && (master != Some(&writer::master_playlist("", device_db).track_ids)
            || master != Some(&listed)
            || !laid_out
            || tracks.iter().any(|t| t.unchecked));

    let mut seen_in_library: HashSet<String> = HashSet::new();
    let mut retagged: HashSet<usize> = HashSet::new();
    let agreed: HashSet<usize> = library
        .iter()
        .zip(&found)
        .filter_map(|(track, at)| {
            let at = (*at)?;
            let device = &tracks[at];
            let same_sound = soundcheck.get(&track.id).is_none_or(|value| *value == device.soundcheck);
            (same_sound && !tags_differ(device, track)).then_some(at)
        })
        .collect();
    for (i, track) in library.iter().enumerate() {
        if let Some(at) = found[i] {
            plan.already_present += 1;
            let device = &tracks[at];
            if agreed.contains(&at) {
                if device.dbid & MARK_MASK == OLD_MARK {
                    plan.rekey.push((device.id, track.id));
                }
                continue;
            }
            let heard_otherwise = soundcheck
                .get(&track.id)
                .is_some_and(|value| *value != device.soundcheck);
            let tags = tags_differ(device, track);
            if !generation.is_shuffle() && (tags || heard_otherwise) && retagged.insert(at) {
                plan.retag.push((device.id, track.clone()));
                if !tags {
                    plan.sound_only += 1;
                }
            }
            if device.dbid & MARK_MASK == OLD_MARK {
                plan.rekey.push((device.id, track.id));
            }
            continue;
        }
        if !seen_in_library.insert(library_fingerprint(track)) {
            continue;
        }
        if !track.path.is_file() {
            plan.missing_files.push(track.path.clone());
            continue;
        }
        if !generation.plays(&track.format) {
            plan.incompatible.push(track.clone());
            continue;
        }

        plan.bytes_to_copy += track.file_size;
        plan.to_copy.push(track.clone());
    }
    plan.device_only_ids =
        claimed.iter().zip(tracks).filter(|(c, _)| !**c).map(|(_, t)| t.id).collect();
    plan.device_only = plan.device_only_ids.len();
    if !generation.is_shuffle() {
        plan.respell = respelled(&spellings, tracks, &claimed);
    }

    plan
}

/// The tracks nothing in the library claims whose names the library spells
/// another way, with the library's spelling — or, for a name the library does
/// not have, the name tidied. „Ground Zero" and „ground zero" were two artists
/// in the device's menus as long as one of them was a track only the device
/// has.
fn respelled(spellings: &Spellings, tracks: &[IpodTrack], claimed: &[bool]) -> Vec<(u32, [String; 4])> {
    tracks
        .iter()
        .zip(claimed)
        .filter(|(_, claimed)| !**claimed)
        .filter_map(|(track, _)| {
            let names = [
                spellings.spell(NAMES, &track.artist),
                spellings.spell(NAMES, &track.album_artist),
                spellings.spell(ALBUMS, &track.album),
                spellings.spell(GENRES, &track.genre),
            ];
            let now = [&track.artist, &track.album_artist, &track.album, &track.genre];
            (names.iter().zip(now).any(|(new, old)| new != old)).then_some((track.id, names))
        })
        .collect()
}

/// The library's tracks that are on the device already, by their ids — what a
/// sync would leave be. For the mark beside a title in the library while a
/// device is plugged in.
pub fn on_device(library: &[Track], device_db: &ItunesDb) -> HashSet<i64> {
    let (found, _) = match_library(library, &device_db.tracks);
    library
        .iter()
        .zip(found)
        .filter_map(|(track, at)| at.map(|_| track.id))
        .collect()
}

/// Each library track's device track, if it has one, and which device
/// tracks were claimed — see [`plan`] for how they are found.
fn match_library(library: &[Track], tracks: &[IpodTrack]) -> (Vec<Option<usize>>, Vec<bool>) {
    use std::collections::HashMap;
    let mut by_fingerprint: HashMap<String, usize> = HashMap::new();
    let mut by_library: HashMap<i64, usize> = HashMap::new();
    let mut by_length: HashMap<u64, Vec<usize>> = HashMap::new();
    for (at, track) in tracks.iter().enumerate() {
        by_fingerprint.entry(ipod_fingerprint(track)).or_insert(at);
        if let Some(id) = library_id_of(track) {
            by_library.entry(id).or_insert(at);
        }
        by_length
            .entry(((track.length_ms as f64) / 1000.0).round() as u64)
            .or_default()
            .push(at);
    }
    let mut claimed = vec![false; tracks.len()];

    let mut found: Vec<Option<usize>> = vec![None; library.len()];
    for (i, track) in library.iter().enumerate() {
        let at = by_library
            .get(&track.id)
            .or_else(|| by_fingerprint.get(&library_fingerprint(track)))
            .copied();
        if let Some(at) = at {
            found[i] = Some(at);
            claimed[at] = true;
        }
    }
    for (i, track) in library.iter().enumerate() {
        if found[i].is_some() {
            continue;
        }
        let seconds = track.duration_secs.round().max(0.0) as u64;
        let fits: Vec<usize> = by_length
            .get(&seconds)
            .into_iter()
            .flatten()
            .copied()
            .filter(|&at| !claimed[at])
            .filter(|&at| {
                let d = &tracks[at];
                let artist = plain(&d.artist) == plain(track.display_artist());
                let album = plain(&d.album) == plain(&track.album);
                let title_same = plain(&d.title) == plain(track.display_title());
                (artist && album && near(&d.title, track.display_title()))
                    || (title_same && (artist || album))
            })
            .collect();
        if let [at] = fits[..] {
            found[i] = Some(at);
            claimed[at] = true;
        }
    }
    (found, claimed)
}

/// Name the device's own playlists for removal: every one but the library
/// (the master) and the ones Kessler keeps, which are written anew anyway.
/// Podcasts are the device's business and stay.
pub fn plan_dropping_lists(plan: &mut SyncPlan, device_db: &ItunesDb, keep: &[DeviceList]) {
    plan.drop_lists = foreign_lists(device_db, keep);
}

/// The device's playlists that are neither the library nor one of `keep`.
pub fn foreign_lists(device_db: &ItunesDb, keep: &[DeviceList]) -> Vec<String> {
    device_db
        .playlists
        .iter()
        .filter(|p| !p.is_master && !p.is_podcast)
        .filter(|p| !keep.iter().any(|k| k.answers_to(&p.name)))
        .map(|p| p.name.clone())
        .collect()
}

/// Add `list` to the plan unless the device already holds it as it should be.
///
/// Current means: a playlist of its name is there, and it lists exactly the
/// tracks it should, in order, every one of which is already on the device.
/// A track of it that the plan is about to copy makes it out of date by
/// definition — the copy is new, so the list on the device cannot have it.
pub fn plan_list(
    plan: &mut SyncPlan,
    list: DeviceList,
    device_db: &ItunesDb,
    generation: Generation,
    converts: bool,
) {
    if !list_is_current(&list, device_db, generation, converts) {
        plan.lists.push(list);
    }
}

/// Whether the device's copy of `list` is what it should be.
///
/// `converts`: whether what this device cannot play is converted on the way,
/// so that it too belongs on the device.
pub fn list_is_current(
    list: &DeviceList,
    device_db: &ItunesDb,
    generation: Generation,
    converts: bool,
) -> bool {
    let ids = device_ids(device_db.tracks.iter());
    let mut wanted: Vec<u32> = Vec::with_capacity(list.tracks.len());
    for track in &list.tracks {
        match ids.get(&library_fingerprint(track)) {
            Some(id) if !wanted.contains(id) => wanted.push(*id),
            Some(_) => {}
            None if track.path.is_file() && (converts || generation.plays(&track.format)) => {
                return false
            }
            None => {}
        }
    }
    device_db
        .playlists
        .iter()
        .find(|p| !p.is_master && list.answers_to(&p.name))
        .is_some_and(|p| p.track_ids == wanted)
}

/// Each device track's id by its fingerprint, the first one where there are
/// two of the same.
fn device_ids<'a>(tracks: impl Iterator<Item = &'a IpodTrack>) -> std::collections::HashMap<String, u32> {
    let mut ids = std::collections::HashMap::new();
    for track in tracks {
        ids.entry(ipod_fingerprint(track)).or_insert(track.id);
    }
    ids
}

/// Put `lists` into the device's playlists, each in place of any of its
/// names, by the tracks the device now holds. One that was there keeps its id,
/// so the device sees the same list with new contents.
fn write_lists(db: &mut ItunesDb, lists: &[DeviceList]) {
    if lists.is_empty() {
        return;
    }
    let ids = device_ids(db.tracks.iter());
    let mut next = db.playlists.iter().map(|p| p.id).max().unwrap_or(1) + 1;
    for list in lists {
        let mut track_ids: Vec<u32> = Vec::with_capacity(list.tracks.len());
        for track in &list.tracks {
            if let Some(id) = ids.get(&library_fingerprint(track)) {
                if !track_ids.contains(id) {
                    track_ids.push(*id);
                }
            }
        }
        let kept = db
            .playlists
            .iter()
            .find(|p| !p.is_master && list.answers_to(&p.name))
            .map(|p| p.id);
        db.playlists.retain(|p| p.is_master || !list.answers_to(&p.name));
        let id = kept.unwrap_or_else(|| {
            next += 1;
            next - 1
        });
        db.playlists.push(super::itunesdb::types::IpodPlaylist {
            id,
            name: list.name.clone(),
            is_master: false,
            is_podcast: false,
            track_ids,
            timestamp: Some(writer::now_unix()),
            indexed: Vec::new(),
            jumps: 0,
        });
    }
}

/// Progress reported while a sync runs.
#[derive(Debug, Clone)]
pub enum SyncProgress {
    BackedUp(PathBuf),
    Copying {
        index: usize,
        total: usize,
        title: String,
    },
    CopyFailed {
        path: PathBuf,
        error: String,
    },
    Transcoding {
        index: usize,
        total: usize,
        title: String,
    },
    WritingDatabase,
    Done {
        copied: usize,
        failed: usize,
        /// Playlists written — see [`DeviceList`].
        lists: usize,
        /// Tracks given the library's tags, and playlists taken off.
        retagged: usize,
        dropped: usize,
        /// Stopped before every track was across: what was copied is in the
        /// database, the rest is not.
        stopped: bool,
    },
}

/// Apply a plan to a device.
///
/// Refuses outright if the hardware requires a database signature Kessler
/// cannot produce, or if the tracks would not fit.
pub fn apply(
    device: &IpodDevice,
    info: &DeviceInfo,
    plan: SyncPlan,
    transcode: bool,
    target: Target,
    stop: &std::sync::atomic::AtomicBool,
    mut progress: impl FnMut(SyncProgress),
) -> Result<()> {
    if let Err(why) = info.writable() {
        return Err(KesslerError::Ipod(why));
    }

    const HEADROOM: u64 = 16 * 1024 * 1024;
    if let Some((_, available)) = device.capacity() {
        if plan.bytes_to_copy + HEADROOM > available {
            return Err(KesslerError::Ipod(format!(
                "not enough space: {} to copy, {} free",
                super::format_bytes(plan.bytes_to_copy),
                super::format_bytes(available)
            )));
        }
    }

    let mut existing = device.read_database()?;
    let played = super::playcounts::read(device).unwrap_or_default();
    super::playcounts::fold(&mut existing, &played);

    if device.has_database() {
        let backup = backup_database(device)?;
        progress(SyncProgress::BackedUp(backup));
    }

    let folders = info.generation.music_folders().max(1);
    let mut next_id = existing.tracks.iter().map(|t| t.id).max().unwrap_or(0) + 1;
    let mut new_tracks: Vec<IpodTrack> = Vec::new();
    let mut failed = 0usize;

    let mut queue: Vec<(&Track, bool)> = plan.to_copy.iter().map(|t| (t, false)).collect();
    if transcode {
        queue.extend(plan.incompatible.iter().map(|t| (t, true)));
    }
    let total = queue.len();

    let scratch = std::env::temp_dir().join(format!("{SCRATCH_PREFIX}{}", writer::now_unix()));
    let mut stopped = false;

    for (index, (track, needs_transcode)) in queue.iter().enumerate() {
        let track = *track;
        if stop.load(std::sync::atomic::Ordering::Relaxed) {
            stopped = true;
            break;
        }

        let source = if *needs_transcode {
            progress(SyncProgress::Transcoding {
                index: index + 1,
                total,
                title: track.display_title().to_string(),
            });
            match convert_for_device(track, &scratch, info.generation, target) {
                Ok(p) => p,
                Err(e) => {
                    progress(SyncProgress::CopyFailed {
                        path: track.path.clone(),
                        error: e.to_string(),
                    });
                    failed += 1;
                    continue;
                }
            }
        } else {
            track.path.clone()
        };

        progress(SyncProgress::Copying {
            index: index + 1,
            total,
            title: track.display_title().to_string(),
        });

        let relative = match allocate_path(device, &source, next_id, folders) {
            Ok(p) => p,
            Err(e) => {
                progress(SyncProgress::CopyFailed {
                    path: track.path.clone(),
                    error: e.to_string(),
                });
                failed += 1;
                continue;
            }
        };

        let wanted = std::fs::metadata(&source).map(|m| m.len()).unwrap_or(0);
        if let Some((_, available)) = device.capacity() {
            if wanted + HEADROOM > available {
                progress(SyncProgress::CopyFailed {
                    path: track.path.clone(),
                    error: format!(
                        "not enough space: {} to copy, {} free",
                        super::format_bytes(wanted),
                        super::format_bytes(available)
                    ),
                });
                failed += 1;
                continue;
            }
        }

        let destination = device.resolve(&relative);
        let made = destination.parent().map_or(Ok(()), std::fs::create_dir_all);
        let copied = made.and_then(|()| std::fs::copy(&source, &destination));
        if *needs_transcode {
            let _ = std::fs::remove_file(&source);
        }

        if let Err(e) = copied {
            progress(SyncProgress::CopyFailed {
                path: track.path.clone(),
                error: e.to_string(),
            });
            failed += 1;
            let _ = std::fs::remove_file(&destination);
            continue;
        }

        let on_device_size = std::fs::metadata(&destination).map(|m| m.len()).unwrap_or(0);
        let mut copied = to_ipod_track(track, next_id, relative, on_device_size);
        copied.soundcheck = plan.soundcheck.get(&track.id).copied().unwrap_or(0);
        if *needs_transcode {
            let (bitrate, rate) = converted_rates(&destination, &source, track, target);
            copied.bitrate = bitrate;
            copied.sample_rate = rate;
            if target == Target::Alac && bitrate >= 500 {
                copied.filetype = "Apple Lossless audio file".into();
            }
        }
        new_tracks.push(copied);
        next_id += 1;
    }

    let _ = std::fs::remove_dir_all(&scratch);
    progress(SyncProgress::WritingDatabase);

    let mut kept = existing.tracks;
    for (id, library) in &plan.retag {
        if let Some(track) = kept.iter_mut().find(|t| t.id == *id) {
            take_tags(track, library);
            if let Some(value) = plan.soundcheck.get(&library.id) {
                track.soundcheck = *value;
            }
        }
    }
    for (id, [artist, album_artist, album, genre]) in &plan.respell {
        if let Some(track) = kept.iter_mut().find(|t| t.id == *id) {
            track.artist = artist.clone();
            track.album_artist = album_artist.clone();
            track.album = album.clone();
            track.genre = genre.clone();
        }
    }
    for (id, library_id) in &plan.rekey {
        if let Some(track) = kept.iter_mut().find(|t| t.id == *id) {
            track.dbid = KESSLER_MARK | (*library_id as u64 & 0xFFFF_FFFF);
        }
    }
    let going: HashSet<u32> = plan.drop_tracks.iter().copied().collect();
    let mut gone_files: Vec<PathBuf> = Vec::new();
    kept.retain(|track| {
        if going.contains(&track.id) {
            gone_files.extend(track.path.clone());
            false
        } else {
            true
        }
    });
    let mut db = ItunesDb {
        version: existing.version,
        db_id: existing.db_id,
        language: existing.language,
        library_id: existing.library_id,
        tracks: kept,
        playlists: Vec::new(),
    };
    let copied = new_tracks.len();
    db.tracks.extend(new_tracks);

    let valid: HashSet<u32> = db.tracks.iter().map(|t| t.id).collect();
    let master_name = existing
        .playlists
        .iter()
        .find(|p| p.is_master)
        .map(|p| p.name.clone())
        .unwrap_or_else(|| device.name.clone());

    let mut playlists = vec![writer::master_playlist(&master_name, &db)];
    for pl in existing
        .playlists
        .iter()
        .filter(|p| !p.is_master && !plan.drop_lists.contains(&p.name))
    {
        let mut pl = pl.clone();
        pl.track_ids.retain(|id| valid.contains(id));
        playlists.push(pl);
    }
    db.playlists = playlists;
    write_lists(&mut db, &plan.lists);

    write_database(device, info, &db)?;
    let _ = super::playcounts::forget(device);
    for relative in &gone_files {
        let text = relative.to_string_lossy().replace(':', "/");
        let _ = std::fs::remove_file(device.mount_point.join(text.trim_start_matches('/')));
    }
    progress(SyncProgress::Done {
        copied,
        failed,
        lists: plan.lists.len(),
        retagged: plan.retag.len(),
        dropped: plan.drop_lists.len(),
        stopped,
    });

    Ok(())
}

/// The files in the device's music folders that its database does not list —
/// what a sync broke off before the database was written left behind, or
/// another program's leftovers — with their sizes. Nothing on a device
/// without a database: everything there would be unlisted. Hidden files
/// (the Mac's `._` companions) are left alone.
pub fn unlisted_files(device: &IpodDevice) -> Result<Vec<(PathBuf, u64)>> {
    if !device.has_database() {
        return Ok(Vec::new());
    }
    let db = device.read_database()?;
    let listed: HashSet<String> = db
        .tracks
        .iter()
        .filter_map(|t| t.path.as_ref())
        .map(|p| p.to_string_lossy().replace(':', "/").trim_start_matches('/').to_lowercase())
        .collect();
    let music = device.mount_point.join("iPod_Control").join("Music");
    let mut found = Vec::new();
    let Ok(folders) = std::fs::read_dir(&music) else { return Ok(found) };
    for folder in folders.flatten() {
        let folder_name = folder.file_name().to_string_lossy().into_owned();
        if folder_name.starts_with('.') || !folder.path().is_dir() {
            continue;
        }
        let Ok(files) = std::fs::read_dir(folder.path()) else { continue };
        for file in files.flatten() {
            let name = file.file_name().to_string_lossy().into_owned();
            let Ok(meta) = file.metadata() else { continue };
            if name.starts_with('.') || !meta.is_file() {
                continue;
            }
            let relative = format!("ipod_control/music/{folder_name}/{name}").to_lowercase();
            if !listed.contains(&relative) {
                found.push((file.path(), meta.len()));
            }
        }
    }
    Ok(found)
}

/// Take the unlisted files off the device — see [`unlisted_files`] — and say
/// how many went and how many bytes they were.
pub fn remove_unlisted(device: &IpodDevice) -> Result<(usize, u64)> {
    let mut gone = (0, 0);
    for (path, size) in unlisted_files(device)? {
        if std::fs::remove_file(&path).is_ok() {
            gone.0 += 1;
            gone.1 += size;
        }
    }
    Ok(gone)
}

/// What taking tracks off the device came to.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Removed {
    pub tracks: usize,
    pub bytes: u64,
    /// Files that stayed on the device although their entries went.
    pub files_left: usize,
}

/// Take tracks out of a database — out of the library and out of every
/// playlist — and say how many went, which files they were, as the device
/// stores them, and how many bytes.
fn drop_tracks(db: &mut ItunesDb, ids: &HashSet<u32>) -> (usize, Vec<PathBuf>, u64) {
    let before = db.tracks.len();
    let mut files = Vec::new();
    let mut bytes = 0u64;
    db.tracks.retain(|t| {
        if ids.contains(&t.id) {
            files.extend(t.path.clone());
            bytes += u64::from(t.size_bytes);
            false
        } else {
            true
        }
    });
    for list in &mut db.playlists {
        list.track_ids.retain(|id| !ids.contains(id));
    }
    (before - db.tracks.len(), files, bytes)
}

/// Take tracks off the device, by the device's own ids.
///
/// The database is backed up first and written before any file goes: a
/// failure half way leaves files nothing lists, which cost space, rather
/// than entries whose file is gone, which the iPod stumbles over.
pub fn remove(device: &IpodDevice, info: &DeviceInfo, ids: &[u32]) -> Result<Removed> {
    if let Err(why) = info.writable() {
        return Err(KesslerError::Ipod(why));
    }
    let mut db = device.read_database()?;
    let ids: HashSet<u32> = ids.iter().copied().collect();
    let played = super::playcounts::read(device).unwrap_or_default();
    super::playcounts::fold(&mut db, &played);
    let (tracks, files, bytes) = drop_tracks(&mut db, &ids);
    if tracks == 0 {
        return Ok(Removed::default());
    }
    if device.has_database() {
        backup_database(device)?;
    }
    write_database(device, info, &db)?;
    let _ = super::playcounts::forget(device);
    let mut files_left = 0;
    for relative in &files {
        let text = relative.to_string_lossy().replace(':', "/");
        let path = device.mount_point.join(text.trim_start_matches('/'));
        if std::fs::remove_file(&path).is_err() && path.exists() {
            files_left += 1;
        }
    }
    Ok(Removed { tracks, bytes, files_left })
}

/// Give the device a new name.
///
/// The name an iPod goes by — under settings → about, and in iTunes — is the
/// name of its master playlist, so that is what changes. The volume keeps its
/// own label: renaming a mounted disk unmounts it on the Mac and wants root on
/// Linux, and a FAT label could not hold most names anyway.
///
/// Written like every other change: the device's own play counts folded in
/// first, the old database put aside, the new one read back before it goes
/// in.
pub fn rename(device: &IpodDevice, info: &DeviceInfo, name: &str) -> Result<()> {
    if let Err(why) = info.writable() {
        return Err(KesslerError::Ipod(why));
    }
    let name = name.trim();
    if name.is_empty() {
        return Err(KesslerError::Ipod("a name cannot be empty".into()));
    }
    let mut db = device.read_database()?;
    let played = super::playcounts::read(device).unwrap_or_default();
    super::playcounts::fold(&mut db, &played);
    match db.playlists.iter_mut().find(|p| p.is_master) {
        Some(master) => master.name = name.to_string(),
        None => {
            let master = writer::master_playlist(name, &db);
            db.playlists.insert(0, master);
        }
    }
    if device.has_database() {
        backup_database(device)?;
    }
    write_database(device, info, &db)?;
    let _ = super::playcounts::forget(device);
    Ok(())
}

/// Copy the current database aside, timestamped, and return where it went.
pub fn backup_database(device: &IpodDevice) -> Result<PathBuf> {
    let source = device.itunesdb_path();
    let dir = source
        .parent()
        .ok_or_else(|| KesslerError::Ipod("iTunesDB has no parent directory".into()))?;

    let stamp = writer::now_unix();
    let destination = dir.join(format!("iTunesDB.kessler-backup.{stamp}"));

    std::fs::copy(&source, &destination).map_err(|e| {
        KesslerError::Ipod(format!(
            "could not back up the existing database to {}: {e}",
            destination.display()
        ))
    })?;
    prune_backups(dir, BACKUPS_KEPT);

    Ok(destination)
}

/// How many backups of the database stay on the device. Each is as large as
/// the database — megabytes for a full disk — and one is left at every sync.
const BACKUPS_KEPT: usize = 10;

/// Take all but the newest `keep` of kessler's backups out of `dir`.
fn prune_backups(dir: &Path, keep: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut backups: Vec<(i64, PathBuf)> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let stamp = name.strip_prefix("iTunesDB.kessler-backup.")?.parse().ok()?;
            Some((stamp, e.path()))
        })
        .collect();
    backups.sort_by_key(|b| std::cmp::Reverse(b.0));
    for (_, path) in backups.into_iter().skip(keep) {
        let _ = std::fs::remove_file(path);
    }
}

/// Write the database through a temporary file, then rename it into place.
///
/// A rename within the same directory is atomic on every filesystem an iPod
/// uses, so the device never sees a partially written database.
///
/// The iPod classic and the nano 3G and 4G get it signed — see `hash58`.
///
/// A shuffle gets its `iTunesSD` written from the same database right after
/// — see `itunessd` — since that, not the database, is what it plays from.
fn write_database(device: &IpodDevice, info: &DeviceInfo, db: &ItunesDb) -> Result<()> {
    let generation = info.generation;
    let final_path = device.itunesdb_path();
    let temp_path = final_path.with_extension("kessler-tmp");

    let bytes = match info.signing() {
        Some(guid) => writer::write_signed(db, &guid),
        None => writer::write(db),
    };
    if let Some(dir) = final_path.parent() {
        std::fs::create_dir_all(dir)?;
    }

    super::itunesdb::parse(&bytes).map_err(|e| {
        KesslerError::Ipod(format!(
            "refusing to install a database kessler cannot read back: {e}"
        ))
    })?;

    let written = (|| -> std::io::Result<()> {
        use std::io::Write;
        let mut file = std::fs::File::create(&temp_path)?;
        file.write_all(&bytes)?;
        file.sync_all()
    })();
    if let Err(e) = written {
        let _ = std::fs::remove_file(&temp_path);
        return Err(KesslerError::Ipod(format!("could not write the new database: {e}")));
    }
    std::fs::rename(&temp_path, &final_path).map_err(|e| {
        let _ = std::fs::remove_file(&temp_path);
        KesslerError::Ipod(format!("could not install the new database: {e}"))
    })?;

    if let (Some(layout), Some(dir)) = (super::itunessd::Layout::of(generation), final_path.parent()) {
        super::itunessd::install(dir, layout, db)?;
    }

    Ok(())
}

/// Where ffmpeg is, if it is anywhere Kessler can find it.
///
/// `PATH` first, and then the handful of places a package manager actually
/// puts it. The second half is not belt and braces. **A macOS application
/// launched from the Finder inherits almost no `PATH`** — `/usr/bin:/bin:` and
/// the two sbins, nothing else — so Homebrew's `/opt/homebrew/bin` is
/// invisible to it. The same build found ffmpeg when run from a terminal and
/// not when double-clicked, and what that looked like from the outside was the
/// conversion feature simply not existing: the offer to convert is only made
/// when there is something to convert *with*, so it silently never appeared.
///
/// Looked for once and remembered: finding it means running it, and the menu
/// asks every frame it is open. An ffmpeg installed while the program runs is
/// found at the next start.
pub fn transcoder() -> Option<PathBuf> {
    static FOUND: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    FOUND.get_or_init(look_for_transcoder).clone()
}

fn look_for_transcoder() -> Option<PathBuf> {
    let known = [
        "/opt/homebrew/bin/ffmpeg",
        "/usr/local/bin/ffmpeg",
        "/opt/local/bin/ffmpeg",
        "/usr/bin/ffmpeg",
    ];
    found_among(Path::new("ffmpeg"), &known)
}

/// The first of these that answers `-version`.
///
/// Apart from [`transcoder`] so the search can be tested without depending on
/// what happens to be installed on the machine running the tests.
fn found_among(on_path: &Path, known: &[&str]) -> Option<PathBuf> {
    let runs = |what: &Path| -> bool {
        std::process::Command::new(what)
            .arg("-version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    };
    if runs(on_path) {
        return Some(on_path.to_path_buf());
    }
    known
        .iter()
        .map(PathBuf::from)
        .find(|at| at.is_file() && runs(at))
}

/// macOS's own converter, which every Mac has: Core Audio reads FLAC (and WAV,
/// AIFF, Apple Lossless) and writes AAC, which every iPod since the third
/// generation plays. So on a Mac nothing has to be installed to put a FLAC on
/// an iPod. It does not read Ogg Vorbis; that still needs ffmpeg.
pub fn afconvert() -> Option<PathBuf> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let at = PathBuf::from("/usr/bin/afconvert");
    at.is_file().then_some(at)
}

/// Whether there is anything to convert with.
pub fn transcoder_available() -> bool {
    afconvert().is_some() || transcoder().is_some()
}

/// The start of the scratch folders' names in the system's temporary folder.
pub const SCRATCH_PREFIX: &str = "kessler-transcode-";

/// What a file the device cannot play is converted to, as chosen under menu →
/// ipod: AAC at a bit rate, Apple Lossless, or MP3 at 320 kbit/s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Aac(u32),
    Alac,
    Mp3,
}

impl Target {
    /// Every choice, by its name in the config file and the menu.
    pub const NAMES: [&'static str; 5] = ["aac 256", "aac 320", "aac 192", "alac", "mp3 320"];

    /// The choice a name stands for; AAC at 256 kbit/s for anything else.
    pub fn from_name(name: &str) -> Self {
        match name.trim() {
            "aac 320" => Self::Aac(320),
            "aac 192" => Self::Aac(192),
            "alac" => Self::Alac,
            "mp3 320" => Self::Mp3,
            _ => Self::Aac(256),
        }
    }
}

/// What a file of this format becomes on the way to a device that plays
/// AAC and Apple Lossless, here, as `target` asks: `aac`, `alac` or `mp3` —
/// or nothing, when nothing here can read it.
pub fn converts_to(format: &str, target: Target) -> Option<&'static str> {
    choose(format, target, true, true, afconvert().is_some(), transcoder().is_some())
}

/// What `target` becomes for a device that plays `m4a` and `alac` or not,
/// with the converters there are. afconvert reads no Vorbis or Opus; a device
/// that plays no Apple Lossless gets AAC; one that plays no AAC, MP3; and MP3
/// needs ffmpeg, without which AAC it is.
fn choose(format: &str, target: Target, m4a: bool, alac: bool, afconvert: bool, ffmpeg: bool) -> Option<&'static str> {
    let vorbis = matches!(format.to_ascii_lowercase().as_str(), "ogg" | "oga" | "opus");
    let apple = afconvert && !vorbis;
    let wanted = match target {
        _ if !m4a => "mp3",
        Target::Alac if alac => "alac",
        Target::Alac | Target::Aac(_) => "aac",
        Target::Mp3 if ffmpeg => "mp3",
        Target::Mp3 => "aac",
    };
    match wanted {
        "mp3" if ffmpeg => Some("mp3"),
        "aac" | "alac" if apple || ffmpeg => Some(wanted),
        _ => None,
    }
}

/// The targets for a set of formats, for a line in the window: „aac",
/// „aac, mp3".
pub fn conversion_word(formats: &[String], target: Target) -> String {
    let mut words: Vec<&str> = formats.iter().filter_map(|f| converts_to(f, target)).collect();
    words.sort();
    words.dedup();
    words.join(", ")
}

/// Convert one file for the device, by whichever converter takes it —
/// see [`choose`]: afconvert for AAC and Apple Lossless where it can, ffmpeg
/// for the rest.
fn convert_for_device(track: &Track, scratch: &Path, generation: Generation, target: Target) -> Result<PathBuf> {
    let apple = afconvert().is_some() && !matches!(track.format.to_ascii_lowercase().as_str(), "ogg" | "oga" | "opus");
    let kbps = match target {
        Target::Aac(kbps) => kbps,
        _ => 256,
    };
    match choose(&track.format, target, generation.plays("m4a"), generation.plays("alac"), afconvert().is_some(), transcoder().is_some()) {
        Some("aac") if apple => transcode_with_afconvert(track, scratch, false, kbps),
        Some("alac") if apple => transcode_with_afconvert(track, scratch, true, kbps),
        Some("aac") => transcode_with_ffmpeg(&track.path, scratch, Codec::Aac(kbps)),
        Some("alac") => transcode_with_ffmpeg(&track.path, scratch, Codec::Alac),
        Some(_) => transcode_with_ffmpeg(&track.path, scratch, Codec::Mp3),
        None => Err(KesslerError::Ipod(format!(
            "{}: nothing here converts {} (ffmpeg would: brew install ffmpeg)",
            track.path.display(),
            track.format
        ))),
    }
}

/// The bit rate and sample rate of a converted copy, for its line in the
/// database: AAC and MP3 at the rate they were made at, Apple Lossless by its
/// size over its length; at 44.1 kHz where it was made so.
fn converted_rates(on_device: &Path, made: &Path, track: &Track, target: Target) -> (u32, u32) {
    let m4a = made.extension().is_some_and(|e| e.eq_ignore_ascii_case("m4a"));
    if !m4a {
        return (320, track.sample_rate.unwrap_or(44_100));
    }
    match target {
        Target::Alac => {
            let bytes = std::fs::metadata(on_device).map(|m| m.len()).unwrap_or(0);
            let seconds = track.duration_secs.max(1.0) as u64;
            ((bytes * 8 / 1000 / seconds) as u32, 44_100)
        }
        Target::Aac(kbps) => (kbps, 44_100),
        Target::Mp3 => (256, 44_100),
    }
}

/// The form a copy on the device is in: `mp3`, `alac` or `aac`, and its bit
/// rate. Apple Lossless by its kind's word, else by a rate no AAC has.
fn device_form(track: &IpodTrack) -> (&'static str, u32) {
    let ending = track
        .path
        .as_ref()
        .and_then(|p| p.to_string_lossy().rsplit('.').next().map(str::to_ascii_lowercase))
        .unwrap_or_default();
    if ending == "mp3" {
        ("mp3", track.bitrate)
    } else if track.filetype.contains("Lossless") || track.bitrate >= 500 {
        ("alac", 0)
    } else {
        ("aac", track.bitrate)
    }
}

/// The device's converted copies — of library tracks this model cannot play
/// as they are — that are not in the form now chosen: AAC at another rate,
/// AAC where Apple Lossless is chosen, and so on. Each with the library track
/// it is a copy of, for „neu umwandeln": converted again, and the old copy
/// taken off.
pub fn stale_conversions(library: &[Track], device_db: &ItunesDb, generation: Generation, target: Target) -> Vec<(u32, Track)> {
    let (found, _) = match_library(library, &device_db.tracks);
    let (apple, ffmpeg) = (afconvert().is_some(), transcoder().is_some());
    let mut stale = Vec::new();
    for (i, at) in found.iter().enumerate() {
        let Some(at) = *at else { continue };
        let track = &library[i];
        if generation.plays(&track.format) || !track.path.is_file() {
            continue;
        }
        let Some(wanted) = choose(&track.format, target, generation.plays("m4a"), generation.plays("alac"), apple, ffmpeg) else {
            continue;
        };
        let device = &device_db.tracks[at];
        let (form, rate) = device_form(device);
        let kbps = match target {
            Target::Aac(kbps) => kbps,
            _ => 256,
        };
        let off = form != wanted || (wanted == "aac" && rate.abs_diff(kbps) > 16);
        if off {
            stale.push((device.id, track.clone()));
        }
    }
    stale
}

/// The scratch folders a sync left behind — the program quit or stopped
/// mid-way — that are older than an hour: none of this run's, which takes
/// minutes at most per track. Asked once at the start.
pub fn sweep_scratch() {
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with(SCRATCH_PREFIX) {
            continue;
        }
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|at| at.elapsed().ok())
            .is_some_and(|age| age.as_secs() > 3600);
        if old {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// Convert to AAC at `kbps` or to Apple Lossless (`lossless`) with afconvert,
/// then write the library's tags into it — afconvert carries the sound across
/// and nothing else. At 44.1 kHz whatever the source's rate, and Apple
/// Lossless in 16 bits: the older iPods go no higher, and a 96 kHz or 24-bit
/// FLAC would otherwise be refused or silent.
fn transcode_with_afconvert(track: &Track, scratch: &Path, lossless: bool, kbps: u32) -> Result<PathBuf> {
    std::fs::create_dir_all(scratch)?;
    let stem = track
        .path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "track".into());
    let destination = scratch.join(format!("{stem}.m4a"));
    let afconvert = afconvert()
        .ok_or_else(|| KesslerError::Ipod("afconvert is not on this machine".into()))?;
    let mut command = std::process::Command::new(&afconvert);
    command.arg("-f").arg("m4af");
    if lossless {
        command.arg("-d").arg("alac@44100/1");
    } else {
        command.arg("-d").arg("aac@44100").arg("-b").arg((kbps * 1000).to_string()).arg("-q").arg("127");
    }
    let output = command
        .arg(&track.path)
        .arg(&destination)
        .output()
        .map_err(|e| KesslerError::Ipod(format!("could not run afconvert on {}: {e}", track.path.display())))?;
    if !output.status.success() || !destination.is_file() {
        let said = String::from_utf8_lossy(&output.stderr);
        let _ = std::fs::remove_file(&destination);
        return Err(KesslerError::Ipod(format!(
            "afconvert failed on {}: {}",
            track.path.display(),
            said.lines().last().unwrap_or("unknown error")
        )));
    }
    if let Err(e) = tag_like(&destination, track) {
        tracing::debug!("tags into {}: {e}", destination.display());
    }
    Ok(destination)
}

/// Write the library's tags for `track` into a freshly converted file.
fn tag_like(path: &Path, track: &Track) -> std::result::Result<(), lofty::error::LoftyError> {
    use lofty::config::WriteOptions;
    use lofty::file::{AudioFile, TaggedFileExt};
    use lofty::prelude::*;
    use lofty::tag::Tag;
    let mut file = lofty::read_from_path(path)?;
    let kind = file.primary_tag_type();
    if file.tag(kind).is_none() {
        file.insert_tag(Tag::new(kind));
    }
    let Some(tag) = file.tag_mut(kind) else { return Ok(()) };
    let text = |tag: &mut Tag, key: ItemKey, value: &str| {
        if !value.is_empty() {
            tag.insert_text(key, value.to_string());
        }
    };
    text(tag, ItemKey::TrackTitle, &track.title);
    text(tag, ItemKey::TrackArtist, &track.artist);
    text(tag, ItemKey::AlbumTitle, &track.album);
    text(tag, ItemKey::AlbumArtist, &track.album_artist);
    text(tag, ItemKey::Genre, &track.genre);
    if let Some(year) = track.year.filter(|y| *y > 0) {
        tag.set_year(year as u32);
    }
    if let Some(number) = track.track_number.filter(|n| *n > 0) {
        tag.set_track(number);
    }
    if let Some(disc) = track.disc_number.filter(|n| *n > 0) {
        tag.set_disk(disc);
    }
    file.save_to_path(path, WriteOptions::default())
}

/// What ffmpeg makes.
#[derive(Debug, Clone, Copy)]
enum Codec {
    Aac(u32),
    Alac,
    Mp3,
}

/// Convert one file with ffmpeg in `scratch`, keeping its tags: MP3 at 320
/// kbit/s, AAC at a bit rate, or Apple Lossless in 16 bits — the last two at
/// 44.1 kHz, as afconvert makes them.
///
/// Shelling out to ffmpeg rather than linking an encoder: the alternative is
/// bundling LAME, and a music library manager should not ship a codec when
/// every machine that plays music already has one available.
fn transcode_with_ffmpeg(source: &Path, scratch: &Path, codec: Codec) -> Result<PathBuf> {
    std::fs::create_dir_all(scratch)?;

    let stem = source
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "track".into());
    let ending = if matches!(codec, Codec::Mp3) { "mp3" } else { "m4a" };
    let destination = scratch.join(format!("{stem}.{ending}"));

    let ffmpeg = transcoder().ok_or_else(|| {
        KesslerError::Ipod("ffmpeg is needed for this conversion and was not found (brew install ffmpeg)".into())
    })?;
    let mut command = std::process::Command::new(&ffmpeg);
    command
        .arg("-nostdin")
        .arg("-loglevel")
        .arg("error")
        .arg("-y")
        .arg("-i")
        .arg(source)
        .arg("-map_metadata")
        .arg("0")
        .arg("-vn");
    match codec {
        Codec::Mp3 => {
            command.arg("-id3v2_version").arg("3").arg("-codec:a").arg("libmp3lame").arg("-b:a").arg("320k");
        }
        Codec::Aac(kbps) => {
            command.arg("-codec:a").arg("aac").arg("-b:a").arg(format!("{kbps}k")).arg("-ar").arg("44100");
        }
        Codec::Alac => {
            command.arg("-codec:a").arg("alac").arg("-sample_fmt").arg("s16p").arg("-ar").arg("44100");
        }
    }
    let output = command.arg(&destination).output().map_err(|e| {
        KesslerError::Ipod(format!("could not run ffmpeg (needed to convert {}): {e}", source.display()))
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let _ = std::fs::remove_file(&destination);
        return Err(KesslerError::Ipod(format!(
            "ffmpeg failed on {}: {}",
            source.display(),
            stderr.lines().last().unwrap_or("unknown error")
        )));
    }

    if !destination.is_file() {
        return Err(KesslerError::Ipod(format!(
            "ffmpeg reported success but wrote nothing for {}",
            source.display()
        )));
    }

    Ok(destination)
}

/// Pick a free `iPod_Control/Music/F##/XXXX.ext` for a new file.
fn allocate_path(
    device: &IpodDevice,
    source: &Path,
    seed: u32,
    folders: u32,
) -> Result<PathBuf> {
    let extension = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("mp3")
        .to_ascii_lowercase();

    let folder = format!("F{:02}", seed % folders);

    for attempt in 0..1_000u32 {
        let name = four_letter_name(seed.wrapping_mul(2_654_435_761).wrapping_add(attempt));
        let relative = PathBuf::from("iPod_Control")
            .join("Music")
            .join(&folder)
            .join(format!("{name}.{extension}"));
        if !device.resolve(&relative).exists() {
            return Ok(relative);
        }
    }

    Err(KesslerError::Ipod(format!(
        "could not find a free filename in iPod_Control/Music/{folder}"
    )))
}

fn four_letter_name(mut value: u32) -> String {
    let mut name = String::with_capacity(4);
    for _ in 0..4 {
        name.push((b'A' + (value % 26) as u8) as char);
        value /= 26;
    }
    name
}

/// Turn a library track into its on-device form.
/// Sound Check's number for a track gain in dB, held down so a quiet track
/// with a high peak is not turned up past full scale.
pub fn soundcheck(gain_db: f64, peak: f64) -> u32 {
    let ceiling = if peak > 0.0 { -20.0 * peak.log10() } else { gain_db };
    let gain = gain_db.min(ceiling);
    (1000.0 * 10f64.powf(-gain / 10.0)).round().clamp(1.0, u32::MAX as f64) as u32
}

/// Sound Check for every library track whose loudness is known.
pub fn soundcheck_table(db: &crate::db::Database) -> std::collections::HashMap<i64, u32> {
    db.loudness_rows()
        .map(|rows| {
            rows.into_iter()
                .filter_map(|r| r.known.map(|k| (r.id, soundcheck(k.track_gain, k.peak))))
                .collect()
        })
        .unwrap_or_default()
}

/// The words iTunes writes for a file's kind (`mhod` 6), as libgpod's users
/// fill them in too; every track that came from iTunes has one.
fn filetype_word(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("mp3") => "MPEG audio file",
        Some("m4a" | "aac" | "mp4") => "AAC audio file",
        Some("wav") => "WAV audio file",
        Some("aif" | "aiff") => "AIFF audio file",
        _ => "",
    }
}

fn to_ipod_track(track: &Track, id: u32, relative: PathBuf, size_on_device: u64) -> IpodTrack {
    IpodTrack {
        id,
        dbid: if track.id < 0 { PODCAST_MARK } else { KESSLER_MARK } | (track.id as u64 & 0xFFFF_FFFF),
        title: tidy(track.display_title()),
        artist: tidy(track.display_artist()),
        album: tidy(&track.album),
        album_artist: tidy(&track.album_artist),
        genre: tidy(&track.genre),
        composer: String::new(),
        comment: String::new(),
        filetype: filetype_word(&relative).into(),
        path: Some(relative),
        track_number: track.track_number.unwrap_or(0),
        total_tracks: 0,
        disc_number: track.disc_number.unwrap_or(0),
        total_discs: 0,
        year: track.year.unwrap_or(0).max(0) as u32,
        length_ms: (track.duration_secs * 1000.0) as u32,
        size_bytes: size_on_device.min(u32::MAX as u64) as u32,
        bitrate: track.bitrate.unwrap_or(0),
        sample_rate: track.sample_rate.unwrap_or(0),
        rating: track.rating.min(5) * 20,
        play_count: track.play_count,
        skip_count: 0,
        last_played: track.last_played.map(|t| t.and_utc().timestamp()),
        date_added: Some(track.date_added.and_utc().timestamp()),
        date_modified: Some(track.date_modified.and_utc().timestamp()),
        compilation: false,
        has_artwork: false,
        artwork_size: 0,
        soundcheck: 0,
        unchecked: false,
    }
}

#[cfg(test)]
mod limit_tests {
    use super::*;

    fn track(size: u64) -> Track {
        Track {
            id: 0,
            path: PathBuf::from("/music/x.mp3"),
            title: "x".into(),
            artist: String::new(),
            album_artist: String::new(),
            album: String::new(),
            genre: String::new(),
            year: None,
            track_number: None,
            disc_number: None,
            duration_secs: 100.0,
            bitrate: None,
            sample_rate: None,
            file_size: size,
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

    /// A plan cut short has to be honest about what it will now copy — the
    /// figure the person reads before saying yes.
    #[test]
    fn a_shortened_plan_counts_its_own_bytes() {
        let plan = SyncPlan {
            to_copy: vec![track(100), track(200), track(400)],
            bytes_to_copy: 700,
            ..Default::default()
        };
        let two = plan.first(2);
        assert_eq!(two.to_copy.len(), 2);
        assert_eq!(two.bytes_to_copy, 300);

        let all = SyncPlan {
            to_copy: vec![track(100)],
            bytes_to_copy: 100,
            ..Default::default()
        }
        .first(9);
        assert_eq!(all.to_copy.len(), 1);
        assert_eq!(all.bytes_to_copy, 100);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A database from before the indices is written again once, even with
    /// nothing to copy; written, it is in order and left alone.
    #[test]
    fn a_database_out_of_order_is_written_again_once() {
        let mut db = ItunesDb {
            tracks: vec![device_track(1, "Can", "Rare Two", "a", 1), device_track(2, "Air", "Moon Safari", "b", 1)],
            ..Default::default()
        };
        db.playlists = vec![crate::ipod::itunesdb::types::IpodPlaylist {
            id: 1,
            name: "ipod".into(),
            is_master: true,
            is_podcast: false,
            track_ids: vec![1, 2],
            timestamp: None,
            indexed: Vec::new(),
            jumps: 0,
        }];
        let before = plan(&[], &db, G, &Default::default());
        assert!(before.reorder);
        assert!(!before.is_empty());
        db.playlists = vec![writer::master_playlist("ipod", &db)];
        let again = crate::ipod::itunesdb::parse(&writer::write(&db)).unwrap();
        let after = plan(&[], &again, G, &Default::default());
        assert!(!after.reorder);
        assert!(after.is_empty());
    }

    /// Only the newest backups stay; anything else in the folder is left.
    #[test]
    fn old_backups_are_taken_away() {
        let dir = std::env::temp_dir().join(format!("kessler-backups-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for stamp in 1..=5 {
            std::fs::write(dir.join(format!("iTunesDB.kessler-backup.{stamp}")), b"x").unwrap();
        }
        std::fs::write(dir.join("iTunesDB"), b"db").unwrap();
        prune_backups(&dir, 2);
        let mut left: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(left, vec!["iTunesDB", "iTunesDB.kessler-backup.4", "iTunesDB.kessler-backup.5"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A track with its names untidy in the library is the same track as
    /// its tidied copy on the device, so it is neither copied again nor
    /// leaves its playlist out of date.
    #[test]
    fn an_untidy_name_is_found_tidied_on_the_device() {
        assert_eq!(
            fingerprint("Mr  Oizo ", "Analog Worms Attack", "Flat\u{200B} Beat", 211),
            fingerprint("Mr Oizo", "Analog Worms Attack", "Flat Beat", 211)
        );
    }

    /// A master playlist in order over a track list that is not — as older
    /// kessler wrote them — is written again, once.
    #[test]
    fn a_track_list_out_of_the_master_order_is_written_again() {
        let mut db = ItunesDb {
            tracks: vec![device_track(1, "Can", "Rare Two", "a", 1), device_track(2, "Air", "Moon Safari", "b", 1)],
            ..Default::default()
        };
        db.playlists = vec![writer::master_playlist("ipod", &db)];
        assert_eq!(db.playlists[0].track_ids, vec![2, 1], "the master list in the artists' order");
        assert!(plan(&[], &db, G, &Default::default()).reorder, "the track list is not");
        let again = crate::ipod::itunesdb::parse(&writer::write(&db)).unwrap();
        assert!(!plan(&[], &again, G, &Default::default()).reorder);
    }

    /// A device whose track numbers do not rise down the list is written again,
    /// and the written one has them rising.
    #[test]
    fn track_numbers_rise_down_the_list() {
        let mut db = ItunesDb {
            tracks: vec![device_track(9, "Can", "Rare Two", "a", 1), device_track(2, "Air", "Moon Safari", "b", 1)],
            ..Default::default()
        };
        db.playlists = vec![writer::master_playlist("ipod", &db)];
        let again = crate::ipod::itunesdb::parse(&writer::write(&db)).unwrap();
        assert!(again.tracks.windows(2).all(|w| w[0].id < w[1].id));
        assert_eq!(again.tracks[0].artist, "Air");
        assert_eq!(again.playlists[0].track_ids, again.tracks.iter().map(|t| t.id).collect::<Vec<_>>());
        assert!(!plan(&[], &again, G, &Default::default()).reorder);
        let mut swapped = again.clone();
        swapped.tracks.swap(0, 1);
        swapped.playlists[0].track_ids.swap(0, 1);
        assert!(plan(&[], &swapped, G, &Default::default()).reorder);
    }

    /// A copied track says what kind of file it is, as iTunes's do.
    #[test]
    fn a_copied_track_names_its_kind() {
        assert_eq!(filetype_word(Path::new("iPod_Control/Music/F01/ABCD.mp3")), "MPEG audio file");
        assert_eq!(filetype_word(Path::new("x/ABCD.M4A")), "AAC audio file");
        assert_eq!(filetype_word(Path::new("x/ABCD.flac")), "");
    }

    /// A device whose tracks kessler wrote unticked is written again.
    #[test]
    fn unticked_tracks_are_written_again() {
        let mut db = ItunesDb { tracks: vec![device_track(1, "Can", "Rare Two", "a", 1)], ..Default::default() };
        db.playlists = vec![writer::master_playlist("ipod", &db)];
        assert!(!plan(&[], &db, G, &Default::default()).reorder);
        db.tracks[0].unchecked = true;
        assert!(plan(&[], &db, G, &Default::default()).reorder);
    }

    /// What goes onto the device is trimmed and has nothing invisible left,
    /// so one artist is one name there.
    #[test]
    fn names_go_onto_the_device_tidied() {
        assert_eq!(tidy("Mr Oizo "), "Mr Oizo");
        assert_eq!(tidy(" Mr  Oizo"), "Mr Oizo");
        assert_eq!(tidy("Mr\u{200B} Oizo\u{FEFF}"), "Mr Oizo");
        assert_eq!(tidy("Mr\u{00A0}Oizo"), "Mr Oizo");
        assert_eq!(tidy(""), "");
    }
    use chrono::NaiveDateTime;

    fn library_track(id: i64, artist: &str, album: &str, title: &str, size: u64) -> Track {
        library_track_fmt(id, artist, album, title, size, "mp3", 291.0)
    }

    fn library_track_fmt(
        id: i64,
        artist: &str,
        album: &str,
        title: &str,
        size: u64,
        format: &str,
        duration_secs: f64,
    ) -> Track {
        Track {
            id,
            path: PathBuf::from(format!("/music/{title}.{format}")),
            title: title.into(),
            artist: artist.into(),
            album_artist: artist.into(),
            album: album.into(),
            genre: "Electronic".into(),
            year: Some(1992),
            track_number: Some(1),
            disc_number: Some(1),
            duration_secs,
            bitrate: Some(320),
            sample_rate: Some(44_100),
            file_size: size,
            format: format.into(),
            rating: 5,
            play_count: 3,
            last_played: None,
            date_added: NaiveDateTime::default(),
            date_modified: NaiveDateTime::default(),
            file_hash: None,
            has_artwork: false,
        }
    }

    fn device_track(id: u32, artist: &str, album: &str, title: &str, size: u32) -> IpodTrack {
        IpodTrack {
            id,
            artist: artist.into(),
            album: album.into(),
            title: title.into(),
            size_bytes: size,
            length_ms: 291_000,
            ..Default::default()
        }
    }

    const G: Generation = Generation::Third;

    /// An episode goes onto the device under its own mark; once it is no
    /// longer among the ones wanted, it — and only it — goes again: a
    /// library track, and an episode somebody else put there, stay.
    #[test]
    fn an_episode_is_marked_and_goes_when_it_is_heard() {
        let mut episode = library_track(-7, "the show", "the show", "episode one", 10);
        episode.genre = "podcast".into();
        let copied = to_ipod_track(&episode, 5, PathBuf::from("iPod_Control/Music/F00/AAAA.mp3"), 10);
        assert_eq!(copied.dbid & MARK_MASK, PODCAST_MARK);
        assert_eq!(to_ipod_track(&library_track(7, "a", "b", "c", 1), 6, PathBuf::from("x.mp3"), 1).dbid & MARK_MASK, KESSLER_MARK);

        let mut heard = device_track(5, "the show", "the show", "episode one", 10);
        heard.dbid = PODCAST_MARK | 1;
        let mut wanted = device_track(6, "the show", "the show", "episode two", 10);
        wanted.dbid = PODCAST_MARK | 2;
        let mut song = device_track(7, "Can", "Tago Mago", "Halleluhwah", 10);
        song.dbid = KESSLER_MARK | 7;
        let foreign = device_track(8, "another show", "another show", "episode nine", 10);
        let device = ItunesDb { tracks: vec![heard, wanted, song, foreign], ..Default::default() };

        let list = DeviceList {
            name: "podcasts".into(),
            also_named: Vec::new(),
            tracks: vec![library_track(-8, "the show", "the show", "episode two", 10)],
        };
        let mut plan = SyncPlan::default();
        plan_podcasts(&mut plan, &list, &device);
        assert_eq!(plan.drop_tracks, vec![5]);
        assert!(!plan.is_empty());
    }

    /// ffmpeg is looked for where it lives, not only where `PATH` points.
    ///
    /// The bug this exists for was invisible from the inside: run from a
    /// terminal the search succeeded, run from a double-clicked application it
    /// did not, and the only symptom was that the offer to convert never
    /// appeared. A macOS bundle inherits `/usr/bin:/bin` and the two sbins and
    /// nothing else, so everything a package manager installs is out of sight.
    #[test]
    fn the_transcoder_is_looked_for_off_the_path_too() {
        let dir = std::env::temp_dir().join("kessler-ffmpeg-search");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let nothing = dir.join("definitely-not-a-program");
        assert_eq!(found_among(&nothing, &[]), None);
        let absent = dir.join("absent");
        assert_eq!(
            found_among(&nothing, &[absent.to_str().unwrap()]),
            None,
            "a name that is not on the disk was taken for a program"
        );

        let real = std::path::Path::new("/bin/true");
        if real.is_file() {
            assert_eq!(
                found_among(&nothing, &["/bin/true"]),
                Some(std::path::PathBuf::from("/bin/true")),
                "a program that is there and runs was not found"
            );
            assert_eq!(
                found_among(std::path::Path::new("true"), &["/bin/true"]),
                Some(std::path::PathBuf::from("true"))
            );
        }

        let looked = [
            "/opt/homebrew/bin/ffmpeg",
            "/usr/local/bin/ffmpeg",
            "/opt/local/bin/ffmpeg",
            "/usr/bin/ffmpeg",
        ];
        for at in looked {
            assert!(at.ends_with("/ffmpeg"), "{at} is not an ffmpeg");
        }
    }

    #[test]
    fn a_transcoded_copy_still_counts_as_present() {
        let library = vec![library_track_fmt(
            1,
            "Aphex Twin",
            "SAW 85-92",
            "Xtal",
            30_000_000,
            "flac",
            291.0,
        )];
        let device = ItunesDb {
            tracks: vec![device_track(10, "Aphex Twin", "SAW 85-92", "Xtal", 3_000_000)],
            ..Default::default()
        };
        assert_eq!(plan(&library, &device, G, &Default::default()).already_present, 1);
    }

    /// The library's tracks on the device are named by their library ids,
    /// found as a sync finds them; the rest are not.
    #[test]
    fn on_device_names_what_a_sync_would_leave_be() {
        let library = vec![
            library_track(1, "Aphex Twin", "SAW 85-92", "Xtal", 3_000_000),
            library_track(2, "Aphex Twin", "SAW 85-92", "Tha", 3_000_000),
        ];
        let device = ItunesDb {
            tracks: vec![device_track(10, "Aphex Twin", "SAW 85-92", "Xtal", 3_000_000)],
            ..Default::default()
        };
        assert_eq!(on_device(&library, &device), HashSet::from([1]));
        assert!(on_device(&library, &ItunesDb::default()).is_empty());
    }

    #[test]
    fn a_different_length_is_a_different_track() {
        let library = vec![library_track_fmt(
            1, "Aphex Twin", "SAW 85-92", "Xtal", 3_000_000, "mp3", 120.0,
        )];
        let device = ItunesDb {
            tracks: vec![device_track(10, "Aphex Twin", "SAW 85-92", "Xtal", 3_000_000)],
            ..Default::default()
        };
        assert_eq!(plan(&library, &device, G, &Default::default()).already_present, 0);
    }

    #[test]
    fn sub_second_encoder_drift_still_matches() {
        let library = vec![library_track_fmt(1, "A", "B", "C", 100, "flac", 291.02)];
        let device = ItunesDb {
            tracks: vec![IpodTrack {
                id: 1,
                artist: "A".into(),
                album: "B".into(),
                title: "C".into(),
                length_ms: 290_980,
                ..Default::default()
            }],
            ..Default::default()
        };
        assert_eq!(plan(&library, &device, G, &Default::default()).already_present, 1);
    }

    #[test]
    fn flac_is_set_aside_for_a_third_generation_ipod() {
        let mut flac = library_track_fmt(1, "A", "B", "C", 100, "flac", 200.0);
        flac.path = std::env::temp_dir().join("kessler-format-test.flac");
        std::fs::write(&flac.path, b"not really a flac").unwrap();

        let plan = plan(&[flac.clone()], &ItunesDb::default(), Generation::Third, &Default::default());
        assert_eq!(plan.incompatible.len(), 1);
        assert!(plan.to_copy.is_empty());
        assert_eq!(plan.incompatible_formats(), vec!["flac"]);
        assert_eq!(plan.bytes_to_copy, 0);

        let _ = std::fs::remove_file(&flac.path);
    }

    #[test]
    fn mp3_is_accepted_for_a_third_generation_ipod() {
        let mut mp3 = library_track_fmt(1, "A", "B", "C", 4_242, "mp3", 200.0);
        mp3.path = std::env::temp_dir().join("kessler-format-test.mp3");
        std::fs::write(&mp3.path, b"not really an mp3").unwrap();

        let plan = plan(&[mp3.clone()], &ItunesDb::default(), Generation::Third, &Default::default());
        assert!(plan.incompatible.is_empty());
        assert_eq!(plan.to_copy.len(), 1);
        assert_eq!(plan.bytes_to_copy, 4_242);

        let _ = std::fs::remove_file(&mp3.path);
    }

    #[test]
    fn plans_only_the_tracks_that_are_missing() {
        let library = vec![
            library_track(1, "Aphex Twin", "SAW 85-92", "Xtal", 3_000_000),
            library_track(2, "Aphex Twin", "SAW 85-92", "Tha", 4_000_000),
        ];
        let device = ItunesDb {
            tracks: vec![device_track(10, "Aphex Twin", "SAW 85-92", "Xtal", 3_000_000)],
            ..Default::default()
        };

        let plan = plan(&library, &device, G, &Default::default());
        assert_eq!(plan.already_present, 1);
        assert_eq!(plan.missing_files.len(), 1);
        assert!(plan.to_copy.is_empty());
    }

    #[test]
    fn matching_ignores_case_and_surrounding_space() {
        let library = vec![library_track(1, "  Aphex Twin ", "SAW 85-92", "XTAL", 3_000_000)];
        let device = ItunesDb {
            tracks: vec![device_track(10, "aphex twin", "saw 85-92", "xtal", 3_000_000)],
            ..Default::default()
        };
        assert_eq!(plan(&library, &device, G, &Default::default()).already_present, 1);
    }

    #[test]
    fn counts_device_only_tracks() {
        let library = vec![library_track(1, "Aphex Twin", "SAW 85-92", "Xtal", 3_000_000)];
        let device = ItunesDb {
            tracks: vec![
                device_track(10, "Aphex Twin", "SAW 85-92", "Xtal", 3_000_000),
                device_track(11, "Boards of Canada", "Geogaddi", "1969", 5_000_000),
            ],
            ..Default::default()
        };
        let plan = plan(&library, &device, G, &Default::default());
        assert_eq!(plan.already_present, 1);
        assert_eq!(plan.device_only, 1);
    }

    #[test]
    fn duplicate_library_entries_are_copied_once() {
        let library = vec![
            library_track(1, "A", "B", "C", 100),
            library_track(2, "A", "B", "C", 100),
        ];
        let plan = plan(&library, &ItunesDb::default(), G, &Default::default());
        assert_eq!(plan.missing_files.len(), 1);
    }

    #[test]
    fn generates_distinct_filenames() {
        let a = four_letter_name(1);
        let b = four_letter_name(2);
        assert_eq!(a.len(), 4);
        assert!(a.chars().all(|c| c.is_ascii_uppercase()));
        assert_ne!(a, b);
    }

    #[test]
    fn converts_a_library_track_to_device_form() {
        let track = library_track(1, "Aphex Twin", "SAW 85-92", "Xtal", 3_000_000);
        let converted =
            to_ipod_track(&track, 7, PathBuf::from("iPod_Control/Music/F00/AAAA.mp3"), 3_000_000);

        assert_eq!(converted.id, 7);
        assert_eq!(converted.title, "Xtal");
        assert_eq!(converted.length_ms, 291_000);
        assert_eq!(converted.size_bytes, 3_000_000);
        assert_eq!(converted.rating, 100);
        assert_eq!(converted.sample_rate, 44_100);
    }

    /// The newest tracks as a playlist on the device: written when it is not
    /// there, left alone when it is right, written again in place — same id,
    /// new contents — when the library has moved on.
    #[test]
    fn the_newest_tracks_are_kept_as_a_playlist_on_the_device() {
        let dir = std::env::temp_dir().join(format!("kessler-list-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut a = library_track(1, "A", "X", "one", 1_000);
        let mut b = library_track(2, "B", "Y", "two", 1_000);
        for t in [&mut a, &mut b] {
            t.path = dir.join(format!("{}.mp3", t.id));
            std::fs::write(&t.path, b"x").unwrap();
        }
        let on_device = |t: &Track, id: u32| {
            to_ipod_track(t, id, PathBuf::from(format!("iPod_Control/Music/F00/{id}.mp3")), 1_000)
        };
        let mut db = ItunesDb {
            version: 0x13,
            db_id: 5,
            language: [0, 0],
            library_id: 0,
            tracks: vec![on_device(&a, 1), on_device(&b, 2)],
            playlists: Vec::new(),
        };
        db.playlists = vec![writer::master_playlist("iPod", &db)];
        let gen = Generation::Third;
        let list = |tracks: Vec<Track>| DeviceList {
            name: "zuletzt hinzugefügt".into(),
            also_named: vec!["recently added".into()],
            tracks,
        };

        assert!(!list_is_current(&list(vec![b.clone(), a.clone()]), &db, gen, false));
        write_lists(&mut db, &[list(vec![b.clone(), a.clone()])]);
        let written = db.playlists.iter().find(|p| !p.is_master).unwrap().clone();
        assert_eq!(written.track_ids, vec![2, 1]);
        assert!(list_is_current(&list(vec![b.clone(), a.clone()]), &db, gen, false));

        db.playlists.last_mut().unwrap().name = "recently added".into();
        assert!(list_is_current(&list(vec![b.clone(), a.clone()]), &db, gen, false));

        let mut c = library_track(3, "C", "Z", "three", 1_000);
        c.path = dir.join("3.mp3");
        std::fs::write(&c.path, b"x").unwrap();
        assert!(!list_is_current(&list(vec![c.clone(), b.clone()]), &db, gen, false));
        db.tracks.push(on_device(&c, 3));
        write_lists(&mut db, &[list(vec![c.clone(), b.clone()])]);
        let lists: Vec<_> = db.playlists.iter().filter(|p| !p.is_master).collect();
        assert_eq!(lists.len(), 1, "one list, not a second beside the first");
        assert_eq!(lists[0].id, written.id);
        assert_eq!(lists[0].name, "zuletzt hinzugefügt");
        assert_eq!(lists[0].track_ids, vec![3, 2]);

        let reparsed = super::super::itunesdb::parse(&writer::write(&db)).unwrap();
        let back = reparsed.playlists.iter().find(|p| !p.is_master).unwrap();
        assert_eq!(back.track_ids, vec![3, 2]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn device_of(tracks: &[(&Track, u32)]) -> ItunesDb {
        ItunesDb {
            version: 0x13,
            db_id: 1,
            language: [0, 0],
            library_id: 0,
            tracks: tracks
                .iter()
                .map(|(t, id)| to_ipod_track(t, *id, PathBuf::from(format!("iPod_Control/Music/F00/{id}.mp3")), 1))
                .collect(),
            playlists: Vec::new(),
        }
    }

    fn existing(path: &str, t: &mut Track) {
        t.path = std::env::temp_dir().join(path);
        std::fs::write(&t.path, b"x").unwrap();
    }

    /// An album name put right in the library: the track is found on the
    /// device by its length and the rest of its tags, and retagged there
    /// rather than copied a second time.
    #[test]
    fn a_retagged_track_is_retagged_on_the_device_not_copied_again() {
        let mut was = library_track(1, "Boards of Canada", "Music Has the Rigth", "Roygbiv", 3_000);
        let mut other = library_track(2, "Boards of Canada", "Music Has the Rigth", "Turquoise Hexagon Sun", 3_000);
        other.duration_secs = 305.0;
        let mut device = device_of(&[(&was, 7), (&other, 8)]);
        for t in &mut device.tracks {
            t.dbid = 0x1234_0000 + t.id as u64;
        }
        existing("kessler-retag-1.mp3", &mut was);
        existing("kessler-retag-2.mp3", &mut other);
        was.album = "Music Has the Right to Children".into();
        was.genre = "Electronic".into();
        let plan = plan(&[was.clone(), other.clone()], &device, Generation::Third, &Default::default());
        assert!(plan.to_copy.is_empty(), "{:?}", plan.to_copy);
        assert_eq!(plan.retag.len(), 1);
        assert_eq!(plan.retag[0].0, 7);
        let mut on_device = device.tracks[0].clone();
        take_tags(&mut on_device, &plan.retag[0].1);
        assert_eq!(on_device.album, "Music Has the Right to Children");
        assert!(!tags_differ(&on_device, &was));
    }

    /// Sound Check: its number, and a track whose loudness has become known
    /// since it was copied is retagged for it, not copied again.
    #[test]
    fn sound_check_reaches_the_tracks_already_there() {
        assert_eq!(soundcheck(-6.0, 0.5), 3981);
        assert_eq!(soundcheck(6.0, 0.9), 810);
        let mut track = library_track(5, "A", "B", "C", 3_000);
        let device = device_of(&[(&track, 9)]);
        existing("kessler-soundcheck.mp3", &mut track);
        let loud: std::collections::HashMap<i64, u32> = [(5, 3981)].into_iter().collect();
        let plan = plan(&[track.clone()], &device, Generation::Third, &loud);
        assert!(plan.to_copy.is_empty());
        assert_eq!(plan.retag.len(), 1);
        assert_eq!(plan.sound_only, 1);
        assert_eq!(plan.soundcheck.get(&5), Some(&3981));
        let mut there = device.clone();
        there.tracks[0].soundcheck = 3981;
        assert!(plan_again(&track, &there, &loud).retag.is_empty());
    }

    fn plan_again(track: &Track, device: &ItunesDb, loud: &std::collections::HashMap<i64, u32>) -> SyncPlan {
        plan(std::slice::from_ref(track), device, Generation::Third, loud)
    }

    /// Removed tracks go from the library and from every list, and their
    /// files are named so they can be deleted.
    #[test]
    fn removed_tracks_leave_the_lists_too() {
        let a = library_track(1, "A", "B", "One", 3_000);
        let b = library_track(2, "A", "B", "Two", 3_000);
        let mut device = device_of(&[(&a, 7), (&b, 8)]);
        device.playlists.push(crate::ipod::itunesdb::types::IpodPlaylist { name: "liste".into(), track_ids: vec![8, 7], ..Default::default() });
        let ids: HashSet<u32> = [7].into_iter().collect();
        let (count, files, _) = drop_tracks(&mut device, &ids);
        assert_eq!((count, files.len()), (1, 1));
        assert_eq!(device.tracks.iter().map(|t| t.id).collect::<Vec<_>>(), vec![8]);
        assert!(device.playlists.iter().all(|p| !p.track_ids.contains(&7)));
        assert!(device.playlists.iter().any(|p| p.track_ids == vec![8]));
    }

    /// A device without a database — fresh from the Finder's restore — is an
    /// iPod all the same, reads as an empty library, and gets its database (and
    /// the folder for it) when first written, with no backup of nothing (as
    /// libgpod's `itdb_init_ipod`).
    #[test]
    fn a_device_without_a_database_is_an_empty_ipod() {
        let root = crate::library::fixtures::scratch("fresh-ipod");
        assert!(IpodDevice::at(&root).is_err(), "not without iPod_Control");
        std::fs::create_dir_all(root.join("iPod_Control").join("Device")).unwrap();
        let device = IpodDevice::at(&root).unwrap();
        assert!(!device.has_database());
        let empty = device.read_database().unwrap();
        assert!(empty.tracks.is_empty() && empty.playlists.is_empty());
        assert_ne!(empty.db_id, 0);
        let info = DeviceInfo { generation: Generation::Third, model_number: None, serial: None, guid: None, db_version: None, signed_database: false, later_signature: false };
        rename(&device, &info, "kessler").unwrap();
        assert!(device.has_database());
        let written = device.read_database().unwrap();
        assert_eq!(written.master_playlist().unwrap().name, "kessler");
        let itunes = root.join("iPod_Control").join("iTunes");
        assert!(std::fs::read_dir(&itunes).unwrap().flatten().all(|e| !e.file_name().to_string_lossy().contains("backup")));
    }

    /// Taking a track off a shuffle writes its iTunesSD anew from the database,
    /// without the track: the shuffle plays from that file, not from the
    /// database.
    #[test]
    fn a_shuffle_gets_its_itunessd_with_the_database() {
        let a = library_track(1, "A", "B", "One", 3_000);
        let b = library_track(2, "A", "B", "Two", 3_000);
        let mut db = device_of(&[(&a, 7), (&b, 8)]);
        db.playlists.push(writer::master_playlist("shuffle", &db));
        let root = crate::library::fixtures::scratch("shuffle-sync");
        let itunes = root.join("iPod_Control").join("iTunes");
        std::fs::create_dir_all(itunes.join("..").join("Music").join("F00")).unwrap();
        std::fs::create_dir_all(&itunes).unwrap();
        std::fs::write(itunes.join("iTunesDB"), writer::write(&db)).unwrap();
        std::fs::write(itunes.join("iTunesPState"), b"old").unwrap();
        let device = IpodDevice::at(&root).unwrap();
        let info = DeviceInfo { generation: Generation::Shuffle2, model_number: None, serial: None, guid: None, db_version: None, signed_database: false, later_signature: false };
        remove(&device, &info, &[7]).unwrap();
        let written = std::fs::read(itunes.join("iTunesSD")).unwrap();
        assert_eq!(written, super::super::itunessd::classic(&device.read_database().unwrap()));
        assert_eq!(written[2], 1, "one track left");
        assert!(!itunes.join("iTunesPState").exists());
    }

    /// One song twice in the library — two releases, other year and
    /// number — and once on the device: left as it is while one of the two
    /// agrees with it, instead of retagged back and forth every sync.
    #[test]
    fn one_song_in_two_releases_does_not_flip_the_device() {
        let a = library_track(1, "Toukaseibunshi", "Cryptogramm", "Cryptogramm", 3_000);
        let mut b = a.clone();
        b.id = 2;
        b.year = Some(2009);
        b.track_number = Some(4);
        let db = device_of(&[(&a, 7)]);
        let planned = plan(&[a.clone(), b.clone()], &db, G, &Default::default());
        assert!(planned.retag.is_empty(), "{:?}", planned.retag.iter().map(|(id, t)| (*id, t.year)).collect::<Vec<_>>());
        let planned = plan(&[b, a], &db, G, &Default::default());
        assert!(planned.retag.is_empty(), "in either order");
    }

    /// An album or genre of spaces alone is cleared by the library's empty
    /// one; a real one the library lacks stays.
    #[test]
    fn a_name_of_spaces_is_cleared() {
        let mut a = library_track(1, "A", "", "One", 3_000);
        a.genre = String::new();
        let mut db = device_of(&[(&a, 7)]);
        db.tracks[0].album = "                              ".into();
        db.tracks[0].genre = "Noise".into();
        let changes = tag_changes(&db.tracks[0], &a);
        assert_eq!(changes, vec![("album", "                              ".to_string(), String::new())]);
        let mut track = db.tracks[0].clone();
        take_tags(&mut track, &a);
        assert_eq!((track.album.as_str(), track.genre.as_str()), ("", "Noise"));
    }

    /// A track only the device has keeps everything but how its names are
    /// spelled: those the library has go as the library spells them, the rest
    /// tidied. Nothing to change, nothing planned.
    #[test]
    fn a_track_only_on_the_device_is_spelled_as_the_library_does() {
        let a = library_track(1, "Ground Zero", "Consume Next", "One", 3_000);
        let mut db = device_of(&[(&a, 7)]);
        let mut only = device_track(8, "ground zero ", "Null & Void", "Two", 3_000);
        only.genre = "  ".into();
        db.tracks.push(only.clone());
        let mut stranger = device_track(9, "Masonna", "Inner Mind Mystique", "Three", 3_000);
        stranger.album_artist = "Masonna".into();
        stranger.genre = "Noise".into();
        db.tracks.push(stranger);
        let planned = plan(std::slice::from_ref(&a), &db, G, &Default::default());
        assert_eq!(planned.device_only, 2);
        assert_eq!(
            planned.respell,
            vec![(8, ["Ground Zero".to_string(), String::new(), "Null & Void".to_string(), String::new()])]
        );
        assert!(!planned.is_empty());
        assert!(plan(&[a], &db, Generation::Shuffle2, &Default::default()).respell.is_empty());
    }

    /// One artist in two spellings goes onto the device in one, the library's:
    /// the iPod would list each spelling as an artist of its own. A track
    /// already there in the other spelling is retagged; a selection is spelled
    /// as the whole library is.
    #[test]
    fn one_name_goes_over_in_one_spelling() {
        let mut a = library_track(1, "Qluster", "Echtzeit", "One", 3_000);
        let mut b = library_track(2, "Qluster", "Echtzeit", "Two", 3_000);
        let mut c = library_track(3, "qluster", "Antworten", "Three", 3_000);
        c.genre = "Ambient".into();
        let mut d = library_track(4, "  Qluster ", "antworten", "Four", 3_000);
        d.genre = "ambient".into();
        for (name, t) in [("1", &mut a), ("2", &mut b), ("3", &mut c), ("4", &mut d)] {
            existing(&format!("kessler-spelling-{name}.mp3"), t);
        }
        let library = vec![a.clone(), b.clone(), c.clone(), d.clone()];
        let plan = plan(&library, &ItunesDb::default(), G, &Default::default());
        let artists: HashSet<&str> = plan.to_copy.iter().map(|t| t.artist.as_str()).collect();
        assert_eq!(artists, ["Qluster"].into_iter().collect());
        let albums: HashSet<&str> = plan.to_copy.iter().filter(|t| t.id >= 3).map(|t| t.album.as_str()).collect();
        assert_eq!(albums, ["Antworten"].into_iter().collect());

        let device = device_of(&[(&c, 7)]);
        assert_eq!(device.tracks[0].artist, "qluster");
        let again = super::plan(&library, &device, G, &Default::default());
        assert_eq!(again.retag.iter().map(|(id, t)| (*id, t.artist.as_str())).collect::<Vec<_>>(), vec![(7, "Qluster")]);
        let alone = plan_in(&[c.clone()], &library, &ItunesDb::default(), G, &Default::default());
        assert_eq!(alone.to_copy[0].artist, "Qluster");
    }

    /// Renamed, the iPod is the same iPod under another name: its master list
    /// is called so, everything on it stays where it was, and the old database
    /// is kept aside.
    #[test]
    fn a_renamed_ipod_keeps_everything_but_its_name() {
        let a = library_track(1, "A", "B", "One", 3_000);
        let b = library_track(2, "A", "B", "Two", 3_000);
        let mut db = device_of(&[(&a, 7), (&b, 8)]);
        db.playlists.push(writer::master_playlist("iPod", &db));
        db.playlists.push(crate::ipod::itunesdb::types::IpodPlaylist { name: "liste".into(), track_ids: vec![8, 7], ..Default::default() });
        let root = crate::library::fixtures::scratch("rename");
        let itunes = root.join("iPod_Control").join("iTunes");
        std::fs::create_dir_all(&itunes).unwrap();
        std::fs::write(itunes.join("iTunesDB"), writer::write(&db)).unwrap();
        let device = IpodDevice::at(&root).unwrap();
        let info = DeviceInfo { generation: Generation::Third, model_number: None, serial: None, guid: None, db_version: None, signed_database: false, later_signature: false };
        assert!(rename(&device, &info, "   ").is_err());
        rename(&device, &info, "  mein ipod ").unwrap();
        let after = device.read_database().unwrap();
        assert_eq!(after.master_playlist().map(|p| p.name.as_str()), Some("mein ipod"));
        assert_eq!(after.tracks.iter().map(|t| t.id).collect::<Vec<_>>(), vec![7, 8]);
        assert_eq!(after.user_playlists().map(|p| p.track_ids.clone()).collect::<Vec<_>>(), vec![vec![8, 7]]);
        let kept = std::fs::read_dir(&itunes)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("iTunesDB.kessler-backup."))
            .count();
        assert_eq!(kept, 1);
    }

    /// A converted file gets the library's tags: afconvert brings the sound
    /// and nothing else.
    #[test]
    fn a_converted_file_is_tagged_like_the_library() {
        use lofty::file::TaggedFileExt;
        use lofty::prelude::*;
        let dir = crate::library::fixtures::scratch("tag-like");
        let path = dir.join("converted.m4a");
        crate::library::fixtures::place(&path, crate::library::fixtures::M4A);
        let mut track = library_track(3, "Boards of Canada", "Geogaddi", "Music Is Math", 1);
        track.year = Some(2002);
        track.track_number = Some(6);
        tag_like(&path, &track).unwrap();
        let file = lofty::read_from_path(&path).unwrap();
        let tag = file.primary_tag().unwrap();
        assert_eq!(tag.title().as_deref(), Some("Music Is Math"));
        assert_eq!(tag.artist().as_deref(), Some("Boards of Canada"));
        assert_eq!(tag.album().as_deref(), Some("Geogaddi"));
        assert_eq!(tag.track(), Some(6));
    }

    /// A title taken apart — „u" and its dots — is written composed, and one
    /// on the device in either form is not brought up to date again.
    #[test]
    fn a_name_taken_apart_is_written_composed_and_not_again() {
        let apart = "Fu\u{0308}r Felix";
        let together = "F\u{00FC}r Felix";
        assert_eq!(tidy(apart), together);
        let mut library = library_track(1, "Julia Holter", "Album", apart, 200);
        library.title = apart.into();
        let mut device = IpodTrack { title: apart.into(), artist: "Julia Holter".into(), album: "Album".into(), ..IpodTrack::default() };
        device.album_artist = library.album_artist.clone();
        device.genre = library.genre.clone();
        device.year = library.year.unwrap_or(0) as u32;
        device.track_number = library.track_number.unwrap_or(0);
        device.disc_number = library.disc_number.unwrap_or(0);
        assert!(tag_changes(&device, &library).iter().all(|(field, ..)| *field != "titel"));
        device.title = together.into();
        assert!(tag_changes(&device, &library).iter().all(|(field, ..)| *field != "titel"));
    }

    /// What a file becomes is what was chosen, as far as the device plays it
    /// and something here makes it: afconvert reads everything but Vorbis,
    /// ffmpeg everything; Apple Lossless only where the device plays it, MP3
    /// only with ffmpeg, MP3 alone for a device without AAC.
    #[test]
    fn the_target_is_what_was_chosen_as_far_as_it_goes() {
        let aac = Target::Aac(256);
        assert_eq!(choose("flac", aac, true, true, true, false), Some("aac"));
        assert_eq!(choose("flac", aac, true, true, false, true), Some("aac"));
        assert_eq!(choose("ogg", aac, true, true, true, false), None);
        assert_eq!(choose("ogg", aac, true, true, true, true), Some("aac"));
        assert_eq!(choose("flac", aac, true, true, false, false), None);
        assert_eq!(choose("flac", Target::Alac, true, true, true, false), Some("alac"));
        assert_eq!(choose("flac", Target::Alac, true, false, true, false), Some("aac"));
        assert_eq!(choose("flac", Target::Mp3, true, true, true, true), Some("mp3"));
        assert_eq!(choose("flac", Target::Mp3, true, true, true, false), Some("aac"));
        assert_eq!(choose("flac", aac, false, false, true, true), Some("mp3"));
        assert_eq!(choose("flac", aac, false, false, true, false), None);
        assert_eq!(Target::from_name("aac 320"), Target::Aac(320));
        let copy = |path: &str, filetype: &str, bitrate: u32| IpodTrack {
            path: Some(PathBuf::from(path)),
            filetype: filetype.into(),
            bitrate,
            ..IpodTrack::default()
        };
        assert_eq!(device_form(&copy(":iPod_Control:Music:F00:A.mp3", "MPEG audio file", 320)), ("mp3", 320));
        assert_eq!(device_form(&copy(":iPod_Control:Music:F00:B.m4a", "AAC audio file", 256)), ("aac", 256));
        assert_eq!(device_form(&copy(":iPod_Control:Music:F00:C.m4a", "Apple Lossless audio file", 900)), ("alac", 0));
        assert_eq!(device_form(&copy(":iPod_Control:Music:F00:D.m4a", "AAC audio file", 870)), ("alac", 0));
        assert_eq!(Target::from_name("was"), Target::Aac(256));
    }

    /// A track Kessler copied carries its library id, and is found by it
    /// however much its tags have changed.
    #[test]
    fn a_kessler_copy_is_found_by_its_library_id() {
        let mut track = library_track(42, "Artist", "Album", "Title", 3_000);
        let device = device_of(&[(&track, 3)]);
        assert_eq!(library_id_of(&device.tracks[0]), Some(42));
        existing("kessler-retag-3.mp3", &mut track);
        track.artist = "Someone Else Entirely".into();
        track.album_artist = track.artist.clone();
        track.title = "A Different Name".into();
        let plan = plan(&[track], &device, Generation::Third, &Default::default());
        assert!(plan.to_copy.is_empty());
        assert_eq!(plan.retag.len(), 1);
    }

    /// A play heard on the iPod finds its library track by the id kessler
    /// stamped the copy with, and a copy from elsewhere by its tags.
    #[test]
    fn a_device_track_finds_its_library_track() {
        let a = library_track(7, "Artist", "Album", "Title", 3_000);
        let b = library_track(8, "Other", "Record", "Song", 4_000);
        let device = device_of(&[(&a, 1)]);
        assert_eq!(library_index(&device.tracks[0], &[b.clone(), a.clone()]), Some(1));
        let mut foreign = device.tracks[0].clone();
        foreign.dbid = 0x1234;
        foreign.artist = "Other".into();
        foreign.album = "Record".into();
        foreign.title = "Song".into();
        foreign.length_ms = 291_000;
        assert_eq!(library_index(&foreign, &[a.clone(), b.clone()]), Some(1));
        foreign.title = "Nothing Like It".into();
        assert_eq!(library_index(&foreign, &[a, b]), None);
    }

    /// A song of the same length on the same record is another song, and is
    /// not mistaken for this one while that one has its own.
    #[test]
    fn another_song_of_the_same_length_is_not_taken_for_this_one() {
        let a = library_track(1, "Artist", "Album", "First Song", 3_000);
        let mut b = library_track(2, "Artist", "Album", "Second Song", 3_000);
        let device = device_of(&[(&a, 1)]);
        existing("kessler-retag-4.mp3", &mut b);
        let plan = plan(&[a.clone(), b.clone()], &device, Generation::Third, &Default::default());
        assert_eq!(plan.to_copy.len(), 1);
        assert_eq!(plan.to_copy[0].id, 2);
        assert!(plan.retag.is_empty());
    }

    /// The library leaving a field empty takes nothing off the device.
    #[test]
    fn an_empty_field_in_the_library_is_not_a_change() {
        let mut track = library_track(1, "Artist", "Album", "Title", 3_000);
        let mut device = to_ipod_track(&track, 1, PathBuf::from("x.mp3"), 1);
        device.genre = "Ambient".into();
        device.year = 1998;
        track.genre = String::new();
        track.year = None;
        assert!(!tags_differ(&device, &track));
        take_tags(&mut device, &track);
        assert_eq!(device.genre, "Ambient");
        assert_eq!(device.year, 1998);
    }

    /// A whole sync takes off every playlist but the library and Kessler's own.
    #[test]
    fn the_old_playlists_are_named_for_removal() {
        use super::super::itunesdb::types::IpodPlaylist;
        let list = |name: &str, master: bool, podcast: bool| IpodPlaylist {
            id: 1,
            name: name.into(),
            is_master: master,
            is_podcast: podcast,
            track_ids: Vec::new(),
            timestamp: None,
            indexed: Vec::new(),
            jumps: 0,
        };
        let device = ItunesDb {
            playlists: vec![
            list("iPod", true, false),
            list("On-The-Go 1", false, false),
            list("recently added", false, false),
            list("Podcasts", false, true),
            list("lange stücke", false, false),
            ],
            ..Default::default()
        };
        let keep = [DeviceList {
            name: "zuletzt hinzugefügt".into(),
            also_named: vec!["recently added".into()],
            tracks: Vec::new(),
        }];
        assert_eq!(foreign_lists(&device, &keep), vec!["On-The-Go 1", "lange stücke"]);
    }

    #[test]
    fn a_written_sync_database_reads_back() {
        let track = library_track(1, "Björk", "Homogénic", "Jóga", 3_000_000);
        let converted =
            to_ipod_track(&track, 1, PathBuf::from("iPod_Control/Music/F00/AAAA.mp3"), 3_000_000);

        let mut db = ItunesDb {
            version: 0x13,
            db_id: 5,
            language: [0, 0],
            library_id: 0,
            tracks: vec![converted],
            playlists: Vec::new(),
        };
        db.playlists = vec![writer::master_playlist("iPod", &db)];

        let reparsed = super::super::itunesdb::parse(&writer::write(&db)).unwrap();
        assert_eq!(reparsed.tracks[0].title, "Jóga");
        assert_eq!(reparsed.tracks[0].artist, "Björk");
        assert_eq!(reparsed.master_playlist().unwrap().track_ids, vec![1]);
    }
}
