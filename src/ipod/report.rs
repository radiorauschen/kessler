use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::path::Path;

use super::device_info::DeviceInfo;
use super::itunesdb::types::{IpodTrack, ItunesDb};
use super::sync::SyncPlan;
use super::IpodDevice;

const INDICES: [(u32, &str); 5] = [
    (0x03, "titel"),
    (0x04, "alben"),
    (0x05, "künstler:innen"),
    (0x07, "genres"),
    (0x12, "komponist:innen"),
];

const SHOWN: usize = 40;

pub fn write(device: &IpodDevice, info: &DeviceInfo, db: &ItunesDb, plan: Option<&SyncPlan>) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "kessler {} — ipod-bericht", crate::VERSION_SHOWN);
    let _ = writeln!(out, "geschrieben: {}", now());
    out.push('\n');

    let _ = writeln!(out, "## gerät");
    let _ = writeln!(out, "laufwerk: {}", device.name);
    let _ = writeln!(out, "modell: {} ({})", info.generation.label(), info.model_number.as_deref().unwrap_or("ohne nummer"));
    let _ = writeln!(
        out,
        "schreiben: {}",
        match info.writable() {
            Ok(()) => "ja".to_string(),
            Err(why) => format!("nein — {why}"),
        }
    );
    if let Some((total, free)) = device.capacity() {
        let _ = writeln!(out, "platz: {} von {} frei", super::format_bytes(free), super::format_bytes(total));
    }
    out.push('\n');

    section_database(&mut out, device, db);
    section_menus(&mut out, db);
    let only_here: BTreeSet<u32> = plan.map(|p| p.device_only_ids.iter().copied().collect()).unwrap_or_default();
    section_names(&mut out, &db.tracks, &only_here);
    section_files(&mut out, device, db);
    if let Some(plan) = plan {
        section_plan(&mut out, plan, db);
    }
    out
}

fn section_database(out: &mut String, device: &IpodDevice, db: &ItunesDb) {
    let _ = writeln!(out, "## datenbank");
    if !device.has_database() {
        let _ = writeln!(out, "noch keine (die erste synchronisierung legt sie an)");
    }
    let size = std::fs::metadata(device.itunesdb_path()).map(|m| m.len()).unwrap_or(0);
    let _ = writeln!(out, "version: 0x{:x}, {}", db.version, super::format_bytes(size));
    let _ = writeln!(out, "titel: {}", db.tracks.len());
    let unchecked = db.tracks.iter().filter(|t| t.unchecked).count();
    let _ = writeln!(out, "nicht angehakt: {unchecked}");
    let by_kessler = db.tracks.iter().filter(|t| t.dbid >> 32 == super::sync::KESSLER_MARK >> 32).count();
    let _ = writeln!(out, "von kessler geschrieben: {by_kessler}");
    match db.master_playlist() {
        Some(master) => {
            let _ = writeln!(out, "name (hauptliste): {}", master.name);
            let _ = writeln!(out, "hauptliste: {} titel", master.track_ids.len());
            let missing: Vec<&str> =
                INDICES.iter().filter(|(f, _)| !master.indexed.contains(f)).map(|(_, n)| *n).collect();
            let listed: Vec<u32> = db.tracks.iter().map(|t| t.id).collect();
            let same = listed.len() == master.track_ids.len() && listed == master.track_ids;
            let _ = writeln!(out, "titelliste wie die hauptliste geordnet: {}", if same { "ja" } else { "nein" });
            let rising = listed.windows(2).all(|w| w[0] < w[1]);
            let _ = writeln!(out, "titelnummern steigen: {}", if rising { "ja" } else { "nein" });
            if missing.is_empty() && master.indexed != super::itunesdb::writer::INDEX_FIELDS {
                let _ = writeln!(out, "indizes: alle fünf, nicht in libgpods reihenfolge");
            } else if missing.is_empty() {
                let _ = writeln!(out, "indizes: alle fünf");
            } else {
                let _ = writeln!(out, "indizes fehlen für: {}", missing.join(", "));
            }
            if master.jumps > 0 {
                let _ = writeln!(out, "sprungtabellen: {} (apple schreibt keine)", master.jumps);
            }
        }
        None => {
            let _ = writeln!(out, "hauptliste: keine");
        }
    }
    let lists: Vec<String> =
        db.user_playlists().map(|p| format!("{} ({})", p.name, p.track_ids.len())).collect();
    let _ = writeln!(out, "playlists: {}", if lists.is_empty() { "keine".to_string() } else { lists.join(", ") });
    let backups = device
        .itunesdb_path()
        .parent()
        .and_then(|dir| std::fs::read_dir(dir).ok())
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.file_name().to_string_lossy().starts_with("iTunesDB.kessler-backup."))
                .count()
        })
        .unwrap_or(0);
    let _ = writeln!(out, "sicherungen: {backups}");
    let plays = super::playcounts::read(device).map(|p| p.len()).unwrap_or(0);
    let _ = writeln!(out, "play counts: {}", if plays == 0 { "keine".to_string() } else { format!("{plays} einträge") });
    out.push('\n');
}

fn section_menus(out: &mut String, db: &ItunesDb) {
    let _ = writeln!(out, "## menüs");
    let Some(master) = db.master_playlist() else {
        out.push('\n');
        return;
    };
    let by_id: BTreeMap<u32, &IpodTrack> = db.tracks.iter().map(|t| (t.id, t)).collect();
    let walked: Vec<&IpodTrack> = master.track_ids.iter().filter_map(|id| by_id.get(id).copied()).collect();
    let indexed = INDICES.iter().all(|(f, _)| master.indexed.contains(f));
    for (word, name) in [("künstler:innen", (|t: &IpodTrack| t.artist.clone()) as fn(&IpodTrack) -> String), ("alben", |t: &IpodTrack| t.album.clone())] {
        let mut runs: BTreeMap<String, usize> = BTreeMap::new();
        let mut last: Option<String> = None;
        for track in &walked {
            let this = name(track);
            if last.as_ref() != Some(&this) {
                *runs.entry(this.clone()).or_default() += 1;
            }
            last = Some(this);
        }
        let split: Vec<String> =
            runs.iter().filter(|(_, n)| **n > 1).map(|(name, n)| format!("{name} ({n}×)")).collect();
        let _ = writeln!(out, "{word}: {} verschiedene", runs.len());
        if split.is_empty() {
            let _ = writeln!(out, "  in der hauptliste alle am stück");
        } else {
            let _ = writeln!(out, "  ohne indizes mehrfach (nicht am stück): {}", split.len());
            if indexed {
                let _ = writeln!(out, "  (die indizes sind da: das menü zeigt jede einmal)");
            }
            list(out, &split);
        }
    }
    out.push('\n');
}

fn section_names(out: &mut String, tracks: &[IpodTrack], only_here: &BTreeSet<u32>) {
    let _ = writeln!(out, "## schreibweisen");
    for (word, name) in [
        ("künstler:innen", (|t: &IpodTrack| t.artist.as_str()) as fn(&IpodTrack) -> &str),
        ("alben", |t: &IpodTrack| t.album.as_str()),
        ("genres", |t: &IpodTrack| t.genre.as_str()),
    ] {
        let mut spellings: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
        let mut matched: BTreeSet<&str> = BTreeSet::new();
        for track in tracks {
            let written = name(track);
            spellings.entry(crate::db::sort_key(written)).or_default().insert(written);
            if !only_here.contains(&track.id) {
                matched.insert(written);
            }
        }
        let said = |n: &str| {
            if only_here.is_empty() || matched.contains(n) {
                format!("„{n}“")
            } else {
                format!("„{n}“ (nur auf dem ipod)")
            }
        };
        let twice: Vec<String> = spellings
            .values()
            .filter(|s| s.len() > 1)
            .map(|s| s.iter().map(|n| said(n)).collect::<Vec<_>>().join(" / "))
            .collect();
        let untidy = tracks.iter().map(name).filter(|n| n.trim() != *n || n.contains("  ")).collect::<BTreeSet<_>>();
        let _ = writeln!(out, "{word}: {} in mehreren schreibweisen, {} mit leerzeichen zu viel", twice.len(), untidy.len());
        list(out, &twice);
        let untidy: Vec<String> = untidy.into_iter().map(said).collect();
        list(out, &untidy);
    }
    let empty = tracks.iter().filter(|t| t.artist.trim().is_empty()).count();
    let _ = writeln!(out, "ohne künstler:in: {empty}");
    out.push('\n');
}

fn section_files(out: &mut String, device: &IpodDevice, db: &ItunesDb) {
    let _ = writeln!(out, "## dateien");
    let mut named = BTreeSet::new();
    let mut missing = 0;
    for track in &db.tracks {
        let Some(path) = &track.path else {
            missing += 1;
            continue;
        };
        let text = path.to_string_lossy().replace(':', "/");
        let on_disk = device.mount_point.join(text.trim_start_matches('/'));
        if !on_disk.is_file() {
            missing += 1;
        }
        named.insert(on_disk.to_string_lossy().to_lowercase());
    }
    let music = device.mount_point.join("iPod_Control").join("Music");
    let mut left_over = 0;
    walk(&music, &mut |file| {
        if !named.contains(&file.to_string_lossy().to_lowercase()) {
            left_over += 1;
        }
    });
    let _ = writeln!(out, "titel ohne datei: {missing}");
    let _ = writeln!(out, "dateien ohne titel: {left_over}");
    out.push('\n');
}

fn section_plan(out: &mut String, plan: &SyncPlan, db: &ItunesDb) {
    let _ = writeln!(out, "## nächster abgleich der ganzen bibliothek");
    let _ = writeln!(out, "schon drauf: {}", plan.already_present);
    let _ = writeln!(out, "zu kopieren: {} ({})", plan.to_copy.len(), super::format_bytes(plan.bytes_to_copy));
    let _ = writeln!(out, "nicht abspielbar: {}", plan.incompatible.len());
    let _ = writeln!(out, "tags nachziehen: {}", plan.retag.len());
    let by_id: BTreeMap<u32, &IpodTrack> = db.tracks.iter().map(|t| (t.id, t)).collect();
    let changes: Vec<String> = plan
        .retag
        .iter()
        .filter_map(|(id, track)| {
            let device = by_id.get(id)?;
            let fields = super::sync::tag_changes(device, track);
            (!fields.is_empty()).then(|| {
                let fields: Vec<String> =
                    fields.iter().map(|(f, was, will)| format!("{f} „{was}“ → „{will}“")).collect();
                format!("{} — {}: {}", track.artist, track.display_title(), fields.join(", "))
            })
        })
        .collect();
    list(out, &changes);
    let _ = writeln!(out, "nur sound check: {}", plan.sound_only);
    let _ = writeln!(out, "nur auf dem ipod: {}", plan.device_only);
    let _ = writeln!(out, "davon schreibweisen angleichen: {}", plan.respell.len());
    let _ = writeln!(out, "dateien fehlen in der bibliothek: {}", plan.missing_files.len());
    let _ = writeln!(out, "menüs neu ordnen: {}", if plan.reorder { "ja" } else { "nein" });
    out.push('\n');
}

fn list(out: &mut String, entries: &[String]) {
    for entry in entries.iter().take(SHOWN) {
        let _ = writeln!(out, "    {entry}");
    }
    if entries.len() > SHOWN {
        let _ = writeln!(out, "    … und {} weitere", entries.len() - SHOWN);
    }
}

fn walk(dir: &Path, each: &mut dyn FnMut(&Path)) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, each);
        } else if !entry.file_name().to_string_lossy().starts_with('.') {
            each(&path);
        }
    }
}

fn now() -> String {
    let secs = super::itunesdb::writer::now_unix();
    let (days, rest) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year}-{month:02}-{day:02} {:02}:{:02} utc", rest / 3600, rest % 3600 / 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipod::itunesdb::writer;

    fn library(artist: &str, album: &str, genre: &str, year: Option<i32>) -> crate::db::models::Track {
        crate::db::models::Track {
            id: 1,
            path: "/music/t1.mp3".into(),
            title: "t1".into(),
            artist: artist.into(),
            album_artist: String::new(),
            album: album.into(),
            genre: genre.into(),
            year,
            track_number: None,
            disc_number: None,
            duration_secs: 1.0,
            bitrate: None,
            sample_rate: None,
            file_size: 1,
            format: "mp3".into(),
            rating: 0,
            play_count: 0,
            last_played: None,
            date_added: Default::default(),
            date_modified: Default::default(),
            file_hash: None,
            has_artwork: false,
        }
    }

    fn track(id: u32, artist: &str, album: &str) -> IpodTrack {
        IpodTrack {
            id,
            artist: artist.into(),
            album: album.into(),
            title: format!("t{id}"),
            path: Some(format!(":iPod_Control:Music:F00:T{id}.mp3").into()),
            ..Default::default()
        }
    }

    #[test]
    fn the_report_names_what_would_stand_twice() {
        let root = crate::library::fixtures::scratch("ipod-report");
        let itunes = root.join("iPod_Control").join("iTunes");
        let music = root.join("iPod_Control").join("Music").join("F00");
        std::fs::create_dir_all(&itunes).unwrap();
        std::fs::create_dir_all(&music).unwrap();
        for id in 1..=3 {
            std::fs::write(music.join(format!("T{id}.mp3")), b"x").unwrap();
        }
        std::fs::write(music.join("STRAY.mp3"), b"x").unwrap();
        let tracks = vec![track(1, "Qluster", "Echtzeit"), track(2, "Popol Vuh", "Hosianna"), track(3, "qluster ", "Echtzeit"), track(4, "Qluster", "Echtzeit")];
        let mut db = ItunesDb { version: 0x13, tracks, ..Default::default() };
        db.playlists.push(crate::ipod::itunesdb::types::IpodPlaylist {
            name: "iPod".into(),
            is_master: true,
            track_ids: vec![1, 2, 4, 3],
            ..Default::default()
        });
        std::fs::write(itunes.join("iTunesDB"), writer::write(&db)).unwrap();
        let device = IpodDevice::at(&root).unwrap();
        let info = DeviceInfo { generation: super::super::device_info::Generation::Third, model_number: None, serial: None, firewire_id: None };
        let read = device.read_database().unwrap();
        let said = write(&device, &info, &read, None);
        assert!(said.contains("Qluster (2×)"), "{said}");
        assert!(said.contains("„Qluster“ / „qluster “") || said.contains("„qluster “ / „Qluster“"), "{said}");
        assert!(said.contains("titel ohne datei: 1"), "{said}");
        assert!(said.contains("dateien ohne titel: 1"), "{said}");
        assert!(said.contains("name (hauptliste): iPod"), "{said}");

        let mut db = read.clone();
        db.playlists = vec![writer::master_playlist("iPod", &db)];
        std::fs::write(itunes.join("iTunesDB"), writer::write(&db)).unwrap();
        let said = write(&device, &info, &device.read_database().unwrap(), None);
        assert!(said.contains("indizes: alle fünf"), "{said}");
        assert!(!said.contains("(2×)"), "{said}");

        let read = device.read_database().unwrap();
        let id = |title: &str| read.tracks.iter().find(|t| t.title == title).unwrap().id;
        let library = library("Qluster", "Echtzeit", "", None);
        let plan = SyncPlan { device_only_ids: vec![id("t3")], retag: vec![(id("t1"), library)], ..Default::default() };
        let said = write(&device, &info, &read, Some(&plan));
        assert!(said.contains("„qluster “ (nur auf dem ipod)"), "{said}");
        assert!(said.contains("„Qluster“ /") || said.contains("/ „Qluster“"), "{said}");
        assert!(!said.contains("„Qluster“ (nur"), "{said}");
    }

    #[test]
    fn a_retag_says_which_field() {
        let device = track(1, "Qluster", "Echtzeit");
        let library = library("Qluster", "Echtzeit ", "Ambient", Some(1980));
        let changes = crate::ipod::sync::tag_changes(&device, &library);
        let fields: Vec<&str> = changes.iter().map(|(f, _, _)| *f).collect();
        assert_eq!(fields, ["genre", "jahr"], "{changes:?}");
        assert_eq!(changes[1], ("jahr", "0".to_string(), "1980".to_string()));
    }
}
