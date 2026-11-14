use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::db::models::Track;
use crate::error::{KesslerError, Result};

use super::device_info::{DeviceInfo, Generation};
use super::itunesdb::types::{IpodTrack, ItunesDb};
use super::itunesdb::writer;
use super::IpodDevice;

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

#[derive(Debug, Default)]
pub struct SyncPlan {
    pub to_copy: Vec<Track>,
    pub already_present: usize,
    pub device_only: usize,
    pub device_only_ids: Vec<u32>,
    pub respell: Vec<(u32, [String; 4])>,
    pub missing_files: Vec<PathBuf>,
    pub incompatible: Vec<Track>,
    pub bytes_to_copy: u64,
    pub lists: Vec<DeviceList>,
    pub retag: Vec<(u32, Track)>,
    pub rekey: Vec<(u32, i64)>,
    pub soundcheck: std::collections::HashMap<i64, u32>,
    pub sound_only: usize,
    pub drop_lists: Vec<String>,
    pub drop_tracks: Vec<u32>,
    pub reorder: bool,
}

#[derive(Debug, Clone)]
pub struct DeviceList {
    pub name: String,
    pub also_named: Vec<String>,
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

    pub fn first(mut self, most: usize) -> Self {
        if self.to_copy.len() <= most {
            return self;
        }
        self.to_copy.truncate(most);
        self.bytes_to_copy = self.to_copy.iter().map(|t| t.file_size).sum();
        self
    }

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

pub const KESSLER_MARK: u64 = 0x4B45_5353_0000_0000;

const OLD_MARK: u64 = 0xC7A7_E01D_0000_0000;

const MARK_MASK: u64 = 0xFFFF_FFFF_0000_0000;

pub const PODCAST_MARK: u64 = 0x4B50_4F44_0000_0000;

pub fn plan_podcasts(plan: &mut SyncPlan, list: &DeviceList, device_db: &ItunesDb) {
    let wanted: HashSet<String> = list.tracks.iter().map(library_fingerprint).collect();
    plan.drop_tracks = device_db
        .tracks
        .iter()
        .filter(|track| track.dbid & MARK_MASK == PODCAST_MARK && !wanted.contains(&ipod_fingerprint(track)))
        .map(|track| track.id)
        .collect();
}

fn library_id_of(track: &IpodTrack) -> Option<i64> {
    (track.dbid & MARK_MASK == KESSLER_MARK).then_some((track.dbid & 0xFFFF_FFFF) as i64)
}

pub fn library_index(track: &IpodTrack, library: &[Track]) -> Option<usize> {
    if let Some(id) = library_id_of(track) {
        if let Some(at) = library.iter().position(|t| t.id == id) {
            return Some(at);
        }
    }
    let wanted = ipod_fingerprint(track);
    library.iter().position(|t| library_fingerprint(t) == wanted)
}

fn plain(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

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

fn tidy(text: &str) -> String {
    let seen: String = text
        .chars()
        .filter(|c| !matches!(c, '\u{200B}'..='\u{200D}' | '\u{FEFF}' | '\u{00AD}'))
        .collect();
    seen.split_whitespace().collect::<Vec<_>>().join(" ")
}

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

fn tags_differ(device: &IpodTrack, library: &Track) -> bool {
    !tag_changes(device, library).is_empty()
}

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
        if mine != *theirs && (!mine.is_empty() || blank(theirs)) {
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

fn blank(text: &str) -> bool {
    !text.is_empty() && text.trim().is_empty()
}

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

pub fn plan(
    library: &[Track],
    device_db: &ItunesDb,
    generation: Generation,
    soundcheck: &std::collections::HashMap<i64, u32>,
) -> SyncPlan {
    plan_in(library, library, device_db, generation, soundcheck)
}

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

pub fn on_device(library: &[Track], device_db: &ItunesDb) -> HashSet<i64> {
    let (found, _) = match_library(library, &device_db.tracks);
    library
        .iter()
        .zip(found)
        .filter_map(|(track, at)| at.map(|_| track.id))
        .collect()
}

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

pub fn plan_dropping_lists(plan: &mut SyncPlan, device_db: &ItunesDb, keep: &[DeviceList]) {
    plan.drop_lists = foreign_lists(device_db, keep);
}

pub fn foreign_lists(device_db: &ItunesDb, keep: &[DeviceList]) -> Vec<String> {
    device_db
        .playlists
        .iter()
        .filter(|p| !p.is_master && !p.is_podcast)
        .filter(|p| !keep.iter().any(|k| k.answers_to(&p.name)))
        .map(|p| p.name.clone())
        .collect()
}

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

fn device_ids<'a>(tracks: impl Iterator<Item = &'a IpodTrack>) -> std::collections::HashMap<String, u32> {
    let mut ids = std::collections::HashMap::new();
    for track in tracks {
        ids.entry(ipod_fingerprint(track)).or_insert(track.id);
    }
    ids
}

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
        lists: usize,
        retagged: usize,
        dropped: usize,
    },
}

pub fn apply(
    device: &IpodDevice,
    info: &DeviceInfo,
    plan: SyncPlan,
    transcode: bool,
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

    let scratch = std::env::temp_dir().join(format!("kessler-transcode-{}", writer::now_unix()));

    for (index, (track, needs_transcode)) in queue.iter().enumerate() {
        let track = *track;

        let source = if *needs_transcode {
            progress(SyncProgress::Transcoding {
                index: index + 1,
                total,
                title: track.display_title().to_string(),
            });
            match convert_for_device(track, &scratch, info.generation) {
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
            let aac = source.extension().is_some_and(|e| e.eq_ignore_ascii_case("m4a"));
            copied.bitrate = if aac { 256 } else { 320 };
            copied.sample_rate = if aac { 44_100 } else { copied.sample_rate };
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

    write_database(device, info.generation, &db)?;
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
    });

    Ok(())
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Removed {
    pub tracks: usize,
    pub bytes: u64,
    pub files_left: usize,
}

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
    write_database(device, info.generation, &db)?;
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
    write_database(device, info.generation, &db)?;
    let _ = super::playcounts::forget(device);
    Ok(())
}

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

const BACKUPS_KEPT: usize = 10;

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

fn write_database(device: &IpodDevice, generation: Generation, db: &ItunesDb) -> Result<()> {
    let final_path = device.itunesdb_path();
    let temp_path = final_path.with_extension("kessler-tmp");

    let bytes = writer::write(db);
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

pub fn afconvert() -> Option<PathBuf> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let at = PathBuf::from("/usr/bin/afconvert");
    at.is_file().then_some(at)
}

pub fn transcoder_available() -> bool {
    afconvert().is_some() || transcoder().is_some()
}

pub fn converts_to(format: &str) -> Option<&'static str> {
    choose(format, afconvert().is_some(), transcoder().is_some())
}

fn choose(format: &str, afconvert: bool, ffmpeg: bool) -> Option<&'static str> {
    let vorbis = matches!(format.to_ascii_lowercase().as_str(), "ogg" | "oga" | "opus");
    if afconvert && !vorbis {
        Some("aac")
    } else if ffmpeg {
        Some("mp3")
    } else {
        None
    }
}

pub fn conversion_word(formats: &[String]) -> String {
    let mut words: Vec<&str> = formats.iter().filter_map(|f| converts_to(f)).collect();
    words.sort();
    words.dedup();
    words.join(", ")
}

fn convert_for_device(track: &Track, scratch: &Path, generation: Generation) -> Result<PathBuf> {
    let target = if generation.plays("m4a") {
        converts_to(&track.format)
    } else {
        choose(&track.format, false, transcoder().is_some())
    };
    match target {
        Some("aac") => transcode_to_aac(track, scratch),
        Some(_) => transcode_to_mp3(&track.path, scratch),
        None => Err(KesslerError::Ipod(format!(
            "{}: nothing here converts {} (ffmpeg would: brew install ffmpeg)",
            track.path.display(),
            track.format
        ))),
    }
}

fn transcode_to_aac(track: &Track, scratch: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(scratch)?;
    let stem = track
        .path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "track".into());
    let destination = scratch.join(format!("{stem}.m4a"));
    let afconvert = afconvert()
        .ok_or_else(|| KesslerError::Ipod("afconvert is not on this machine".into()))?;
    let output = std::process::Command::new(&afconvert)
        .arg("-f")
        .arg("m4af")
        .arg("-d")
        .arg("aac@44100")
        .arg("-b")
        .arg("256000")
        .arg("-q")
        .arg("127")
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

fn transcode_to_mp3(source: &Path, scratch: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(scratch)?;

    let stem = source
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "track".into());
    let destination = scratch.join(format!("{stem}.mp3"));

    let ffmpeg = transcoder().ok_or_else(|| {
        KesslerError::Ipod(
            "ffmpeg is needed to convert to MP3 and was not found (brew install ffmpeg)".into(),
        )
    })?;
    let output = std::process::Command::new(&ffmpeg)
        .arg("-nostdin")
        .arg("-loglevel")
        .arg("error")
        .arg("-y")
        .arg("-i")
        .arg(source)
        .arg("-map_metadata")
        .arg("0")
        .arg("-id3v2_version")
        .arg("3")
        .arg("-codec:a")
        .arg("libmp3lame")
        .arg("-b:a")
        .arg("320k")
        .arg(&destination)
        .output()
        .map_err(|e| {
            KesslerError::Ipod(format!(
                "could not run ffmpeg (needed to convert {}): {e}",
                source.display()
            ))
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

pub fn soundcheck(gain_db: f64, peak: f64) -> u32 {
    let ceiling = if peak > 0.0 { -20.0 * peak.log10() } else { gain_db };
    let gain = gain_db.min(ceiling);
    (1000.0 * 10f64.powf(-gain / 10.0)).round().clamp(1.0, u32::MAX as f64) as u32
}

pub fn soundcheck_table(db: &crate::db::Database) -> std::collections::HashMap<i64, u32> {
    db.loudness_rows()
        .map(|rows| {
            rows.into_iter()
                .filter_map(|r| r.known.map(|k| (r.id, soundcheck(k.track_gain, k.peak))))
                .collect()
        })
        .unwrap_or_default()
}

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

    #[test]
    fn an_untidy_name_is_found_tidied_on_the_device() {
        assert_eq!(
            fingerprint("Mr  Oizo ", "Analog Worms Attack", "Flat\u{200B} Beat", 211),
            fingerprint("Mr Oizo", "Analog Worms Attack", "Flat Beat", 211)
        );
    }

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

    #[test]
    fn a_copied_track_names_its_kind() {
        assert_eq!(filetype_word(Path::new("iPod_Control/Music/F01/ABCD.mp3")), "MPEG audio file");
        assert_eq!(filetype_word(Path::new("x/ABCD.M4A")), "AAC audio file");
        assert_eq!(filetype_word(Path::new("x/ABCD.flac")), "");
    }

    #[test]
    fn unticked_tracks_are_written_again() {
        let mut db = ItunesDb { tracks: vec![device_track(1, "Can", "Rare Two", "a", 1)], ..Default::default() };
        db.playlists = vec![writer::master_playlist("ipod", &db)];
        assert!(!plan(&[], &db, G, &Default::default()).reorder);
        db.tracks[0].unchecked = true;
        assert!(plan(&[], &db, G, &Default::default()).reorder);
    }

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
        let info = DeviceInfo { generation: Generation::Third, model_number: None, serial: None, firewire_id: None };
        rename(&device, &info, "kessler").unwrap();
        assert!(device.has_database());
        let written = device.read_database().unwrap();
        assert_eq!(written.master_playlist().unwrap().name, "kessler");
        let itunes = root.join("iPod_Control").join("iTunes");
        assert!(std::fs::read_dir(&itunes).unwrap().flatten().all(|e| !e.file_name().to_string_lossy().contains("backup")));
    }

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
        let info = DeviceInfo { generation: Generation::Shuffle2, model_number: None, serial: None, firewire_id: None };
        remove(&device, &info, &[7]).unwrap();
        let written = std::fs::read(itunes.join("iTunesSD")).unwrap();
        assert_eq!(written, super::super::itunessd::classic(&device.read_database().unwrap()));
        assert_eq!(written[2], 1, "one track left");
        assert!(!itunes.join("iTunesPState").exists());
    }

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
        let info = DeviceInfo { generation: Generation::Third, model_number: None, serial: None, firewire_id: None };
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

    #[test]
    fn the_converter_is_chosen_by_what_it_can_read() {
        assert_eq!(choose("flac", true, false), Some("aac"));
        assert_eq!(choose("flac", true, true), Some("aac"));
        assert_eq!(choose("ogg", true, false), None);
        assert_eq!(choose("ogg", true, true), Some("mp3"));
        assert_eq!(choose("flac", false, true), Some("mp3"));
        assert_eq!(choose("flac", false, false), None);
    }

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
