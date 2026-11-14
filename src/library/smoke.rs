use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::db::models::Track;
use crate::db::Database;
use crate::library::metadata::{read_extras, Extras};
use crate::library::scanner::{self, Arrival};
use crate::library::tags::{self, Edit, Field};

#[derive(Debug, Clone, PartialEq)]
struct Said {
    title: String,
    artist: String,
    album_artist: String,
    album: String,
    genre: String,
    year: Option<i32>,
    track: Option<u32>,
    disc: Option<u32>,
    cover: bool,
    extras: Extras,
}

fn said(t: &Track) -> Said {
    Said {
        title: t.title.clone(),
        artist: t.artist.clone(),
        album_artist: t.album_artist.clone(),
        album: t.album.clone(),
        genre: t.genre.clone(),
        year: t.year,
        track: t.track_number,
        disc: t.disc_number,
        cover: t.has_artwork,
        extras: Extras::default(),
    }
}

fn said_whole(t: &Track) -> Said {
    Said { extras: read_extras(&t.path), ..said(t) }
}

struct Library {
    root: PathBuf,
    artwork: PathBuf,
    db: Database,
    db_path: PathBuf,
    expected: HashMap<i64, Said>,
    length: HashMap<i64, f64>,
    count: u32,
}

fn all_tracks(db: &Database) -> Vec<Track> {
    db.get_tracks(None, None, crate::db::models::SortField::Title, crate::db::models::SortOrder::Ascending, None)
        .unwrap()
}

fn by_title(db: &Database, title: &str) -> Track {
    all_tracks(db)
        .into_iter()
        .find(|t| t.title == title)
        .unwrap_or_else(|| panic!("no track „{title}\""))
}

fn hear(path: &Path) -> (f64, String) {
    let probe = Command::new("ffprobe")
        .args(["-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0"])
        .arg(path)
        .output()
        .expect("ffprobe");
    let length = String::from_utf8_lossy(&probe.stdout).trim().parse::<f64>().unwrap_or(0.0);
    let decode = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-f", "null", "-"])
        .output()
        .expect("ffmpeg");
    (length, String::from_utf8_lossy(&decode.stderr).trim().to_string())
}

fn audio_files(root: &Path) -> Vec<PathBuf> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .map(|e| e.into_path())
        .filter(|p| {
            p.is_file()
                && crate::library::metadata::is_supported_audio(p)
        })
        .collect()
}

impl Library {
    fn open(source: &Path) -> Self {
        let work = std::env::temp_dir().join("kessler-tag-smoke");
        let _ = std::fs::remove_dir_all(&work);
        let root = work.join("library");
        std::fs::create_dir_all(&root).unwrap();
        for file in walkdir::WalkDir::new(source).into_iter().filter_map(|e| e.ok()) {
            let to = root.join(file.path().strip_prefix(source).unwrap());
            if file.file_type().is_dir() {
                std::fs::create_dir_all(&to).unwrap();
            } else {
                std::fs::copy(file.path(), &to).unwrap();
            }
        }
        let root = crate::library::one_name_for(&root);
        let artwork = work.join("artwork");
        std::fs::create_dir_all(&artwork).unwrap();
        let db_path = work.join("kessler.db");
        let db = Database::open(&db_path).unwrap();
        let swept = scanner::scan_directory(&root, &db, Some(&artwork), None, Arrival::FromFile).unwrap();
        println!("scan: {} dateien, {} neu", swept.files, swept.added);
        let tracks = all_tracks(&db);
        assert_eq!(tracks.len(), audio_files(&root).len(), "every file a track");
        let mut length = HashMap::new();
        let mut expected = HashMap::new();
        for t in &tracks {
            let (heard, errors) = hear(&t.path);
            assert!(errors.is_empty(), "{} does not decode before anything: {errors}", t.path.display());
            length.insert(t.id, heard);
            expected.insert(t.id, said_whole(t));
            println!("  {:<28} {:<20} {:<24} {:?}", t.title, t.artist, t.album, t.path.file_name().unwrap());
        }
        let count = tracks.len() as u32;
        Self { root, artwork, db, db_path, expected, length, count }
    }

    fn save(&mut self, ids: &[i64], edit: &Edit, changes: impl Fn(&mut Said)) {
        let roots = vec![self.root.clone()];
        let mut plan = Vec::new();
        for id in ids {
            let track = self.db.get_track(*id).unwrap().expect("track");
            tags::write(&track.path, edit).unwrap_or_else(|e| panic!("{}: {e}", track.path.display()));
            let target = crate::library::metadata::read_metadata(&track.path)
                .ok()
                .and_then(|after| crate::library::import::refiled(&roots, &track, &after));
            plan.push((*id, track, target));
        }
        for (id, track, target) in plan {
            let mut path = track.path.clone();
            if let Some(target) = target {
                let landed = crate::library::import::move_filed(&path, &target).unwrap();
                path = crate::library::one_name_for(&landed);
                self.db.move_track_path(id, &path).unwrap();
                println!("    verschoben: {}", path.strip_prefix(&self.root).unwrap().display());
            }
            scanner::ingest_file(&path, &self.db, Some(&self.artwork)).unwrap();
            changes(self.expected.get_mut(&id).unwrap());
        }
        self.check(&edit.summary());
    }

    fn cover(&mut self, id: i64) {
        let track = self.db.get_track(id).unwrap().unwrap();
        let png = {
            let img = image::RgbImage::from_pixel(64, 64, image::Rgb([180, 40, 20]));
            let mut out = std::io::Cursor::new(Vec::new());
            img.write_to(&mut out, image::ImageFormat::Png).unwrap();
            out.into_inner()
        };
        let jpeg = tags::picture_for_tag(&png).unwrap();
        tags::write_picture(&track.path, &jpeg).unwrap_or_else(|e| panic!("{}: {e}", track.path.display()));
        scanner::ingest_file(&track.path, &self.db, Some(&self.artwork)).unwrap();
        self.expected.get_mut(&id).unwrap().cover = true;
        self.check(&format!("cover auf {}", track.path.file_name().unwrap().to_string_lossy()));
    }

    fn check(&self, step: &str) {
        let sql = rusqlite::Connection::open(&self.db_path).unwrap();
        let integrity: String = sql.query_row("PRAGMA integrity_check", [], |r| r.get(0)).unwrap();
        assert_eq!(integrity, "ok", "{step}: database integrity");
        let rows: u32 = sql.query_row("SELECT COUNT(*) FROM tracks", [], |r| r.get(0)).unwrap();
        let paths: u32 = sql.query_row("SELECT COUNT(DISTINCT path) FROM tracks", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, self.count, "{step}: as many tracks as before");
        assert_eq!(paths, rows, "{step}: no file twice");

        let tracks = all_tracks(&self.db);
        let files = audio_files(&self.root);
        assert_eq!(files.len() as u32, self.count, "{step}: as many files as before");
        for t in &tracks {
            assert!(t.path.is_file(), "{step}: row without a file: {}", t.path.display());
            assert!(files.iter().any(|f| f == &t.path), "{step}: {} outside the library", t.path.display());
            let file = crate::library::metadata::read_metadata(&t.path).unwrap();
            assert_eq!(said(&file), said(t), "{step}: database and file disagree on {}", t.path.display());
            assert_eq!(&said_whole(t), &self.expected[&t.id], "{step}: {}", t.path.display());
            let (heard, errors) = hear(&t.path);
            assert!(errors.is_empty(), "{step}: {} no longer decodes: {errors}", t.path.display());
            let was = self.length[&t.id];
            assert!((heard - was).abs() < 0.15, "{step}: {} ran {was:.2} s, now {heard:.2} s", t.path.display());
        }
        let left: Vec<_> = walkdir::WalkDir::new(&self.root)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with(".kessler-"))
            .map(|e| e.into_path())
            .collect();
        assert!(left.is_empty(), "{step}: copies left behind: {left:?}");
        println!("ok  {step}");
    }

    fn ids(&self, titles: &[&str]) -> Vec<i64> {
        titles.iter().map(|t| by_title(&self.db, t).id).collect()
    }
}

fn edit(changes: &[(Field, &str)]) -> Edit {
    let mut edit = Edit::new();
    for (field, value) in changes {
        edit.set(*field, *value);
    }
    edit
}

#[test]
#[ignore = "needs the library bundle/smoke-tags.sh builds, and ffmpeg"]
fn tag_smoke() {
    let Some(source) = std::env::var_os("KESSLER_SMOKE") else {
        panic!("KESSLER_SMOKE: run bundle/smoke-tags.sh");
    };
    let mut lib = Library::open(Path::new(&source));
    lib.check("nach dem ersten scan");

    let ids = lib.ids(&["Trainwreck Afterbirth"]);
    lib.save(&ids, &edit(&[(Field::Title, "Trainwreck Afterbirth (live)")]), |s| {
        s.title = "Trainwreck Afterbirth (live)".into()
    });

    let harvest = lib.ids(&["Trainwreck Afterbirth (live)", "Cherry", "Making Space", "Hatchet Man"]);
    lib.save(&harvest, &edit(&[(Field::Genre, "Drone"), (Field::Year, "2007")]), |s| {
        s.genre = "Drone".into();
        s.year = Some(2007);
    });

    lib.save(&harvest, &edit(&[(Field::Album, "The Harvest (Remaster)")]), |s| {
        s.album = "The Harvest (Remaster)".into()
    });
    assert!(
        !audio_files(&lib.root).iter().any(|f| f.parent().unwrap().ends_with("Ajilvsga/The Harvest")),
        "nothing left in the old album's folder"
    );

    let earth = lib.ids(&["Untitled"]);
    lib.save(&earth, &edit(&[(Field::Artist, "Ajilvsga & Friends")]), |s| {
        s.artist = "Ajilvsga & Friends".into()
    });

    let tahoma = lib.ids(&["Tahoma 2"]);
    lib.save(&tahoma, &edit(&[(Field::DiscNumber, "3"), (Field::TrackNumber, "5")]), |s| {
        s.disc = Some(3);
        s.track = Some(5);
    });

    let dance = lib.ids(&["Tibetan Dance"]);
    lib.save(&dance, &edit(&[(Field::Title, "チベタン・ダンス"), (Field::Genre, "")]), |s| {
        s.title = "チベタン・ダンス".into();
        s.genre = String::new();
    });

    let tanz = lib.ids(&["Tanzmusik"]);
    lib.save(&tanz, &edit(&[(Field::AlbumArtist, "Kraftwerk"), (Field::Genre, "Krautrock – früh")]), |s| {
        s.album_artist = "Kraftwerk".into();
        s.genre = "Krautrock – früh".into();
    });

    let paysage = lib.ids(&["Im Wald", "Schnee"]);
    lib.save(&paysage, &edit(&[(Field::AlbumArtist, "")]), |s| s.album_artist = String::new());

    let radigue = lib.ids(&["Kyema", "Kailasha", "Koume"]);
    lib.save(&radigue, &edit(&[(Field::Genre, "Drone"), (Field::Album, "Trilogie de la mort")]), |s| {
        s.genre = "Drone".into();
        s.album = "Trilogie de la mort".into();
    });

    let every = lib.ids(&["Cherry", "Hatchet Man", "Untitled", "Tahoma 1", "Tanzmusik", "チベタン・ダンス", "Im Wald", "Schnee", "Kyema", "Kailasha", "Koume"]);
    let info = edit(&[
        (Field::Composer, "Erik Satie"),
        (Field::Grouping, "Gymnopédies"),
        (Field::Bpm, "72"),
        (Field::Comment, "für Kim – ケスラー"),
    ]);
    lib.save(&every, &info, |s| {
        s.extras = Extras {
            composer: "Erik Satie".into(),
            grouping: "Gymnopédies".into(),
            bpm: Some(72),
            comment: "für Kim – ケスラー".into(),
        }
    });
    lib.save(&lib.ids(&["Tanzmusik"]), &edit(&[(Field::Comment, ""), (Field::Bpm, "")]), |s| {
        s.extras.comment = String::new();
        s.extras.bpm = None;
    });

    for title in ["Cherry", "Hatchet Man", "Tahoma 1", "Tanzmusik", "チベタン・ダンス", "Im Wald", "Schnee", "Kyema", "Kailasha", "Koume"] {
        let id = by_title(&lib.db, title).id;
        lib.cover(id);
    }

    let cherry = lib.ids(&["Cherry"]);
    lib.save(&cherry, &edit(&[(Field::Genre, "Drone")]), |_| {});

    let swept = scanner::scan_directory(&lib.root, &lib.db, Some(&lib.artwork), None, Arrival::Now).unwrap();
    println!("zweiter scan: {} dateien, {} neu, {} aktualisiert", swept.files, swept.added, swept.updated);
    assert_eq!(swept.added, 0, "a second scan adds nothing");
    lib.check("nach dem zweiten scan");

    println!("\nam ende:");
    for t in all_tracks(&lib.db) {
        println!("  {:<28} {:<20} {:<24} {}", t.title, t.artist, t.album, t.path.strip_prefix(&lib.root).unwrap().display());
    }
}

fn id3_frames(path: &Path) -> Vec<String> {
    use lofty::file::AudioFile as _;
    let Ok(mut file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let Ok(mpeg) = lofty::mpeg::MpegFile::read_from(&mut file, lofty::config::ParseOptions::new()) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = mpeg
        .id3v2()
        .map(|tag| tag.into_iter().map(|f| f.id_str().to_string()).collect())
        .unwrap_or_default();
    ids.sort();
    ids
}

#[test]
#[ignore = "needs the files bundle/smoke-tags.sh builds, and ffmpeg"]
fn tag_smoke_weird() {
    let Some(source) = std::env::var_os("KESSLER_SMOKE_WEIRD") else {
        panic!("KESSLER_SMOKE_WEIRD: run bundle/smoke-tags.sh");
    };
    let work = std::env::temp_dir().join("kessler-tag-smoke-weird");
    let _ = std::fs::remove_dir_all(&work);
    let root = work.join("library");
    std::fs::create_dir_all(&root).unwrap();
    for file in std::fs::read_dir(&source).unwrap().filter_map(|e| e.ok()) {
        std::fs::copy(file.path(), root.join(file.file_name())).unwrap();
    }
    let root = crate::library::one_name_for(&root);
    let artwork = work.join("artwork");
    std::fs::create_dir_all(&artwork).unwrap();
    let db_path = work.join("kessler.db");
    let db = Database::open(&db_path).unwrap();

    let whole = |step: &str| {
        let sql = rusqlite::Connection::open(&db_path).unwrap();
        let integrity: String = sql.query_row("PRAGMA integrity_check", [], |r| r.get(0)).unwrap();
        assert_eq!(integrity, "ok", "{step}: database integrity");
        let rows: u32 = sql.query_row("SELECT COUNT(*) FROM tracks", [], |r| r.get(0)).unwrap();
        let paths: u32 = sql.query_row("SELECT COUNT(DISTINCT path) FROM tracks", [], |r| r.get(0)).unwrap();
        assert_eq!(paths, rows, "{step}: no file twice");
        rows
    };

    let swept = scanner::scan_directory(&root, &db, Some(&artwork), None, Arrival::FromFile).unwrap();
    let count = whole("scan");
    println!("scan: {} dateien, {} in der bibliothek", swept.files, count);
    let mut files: Vec<PathBuf> = std::fs::read_dir(&root).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).collect();
    files.sort();
    for f in &files {
        let known = db.get_track_by_path(&f.to_string_lossy()).unwrap();
        let name = f.file_name().unwrap().to_string_lossy();
        match known {
            Some(t) => println!("  drin    {name:<34} „{}\" · {} · {}", t.title.chars().take(40).collect::<String>(), t.artist, t.album),
            None => println!("  draußen {name}"),
        }
    }

    let unknown = root.join("11 unknown frames.mp3");
    let extras = read_extras(&unknown);
    println!("11 liest sich als {extras:?}");
    assert_eq!(extras.comment, "ein kommentar", "the comment, not iTunes' iTunNORM");
    assert_eq!((extras.composer.as_str(), extras.grouping.as_str(), extras.bpm), ("Erik Satie", "Gymnopédies", Some(72)));
    let unknown_before = id3_frames(&unknown);
    println!("unbekannte rahmen vorher: {unknown_before:?}");

    let png = {
        let img = image::RgbImage::from_pixel(32, 32, image::Rgb([20, 120, 60]));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    };
    let jpeg = tags::picture_for_tag(&png).unwrap();
    let tracks = all_tracks(&db);
    for t in &tracks {
        let name = t.path.file_name().unwrap().to_string_lossy().to_string();
        for (what, attempt) in [("genre", 0), ("cover", 1)] {
            let bytes_before = std::fs::read(&t.path).unwrap();
            let (length_before, errors_before) = hear(&t.path);
            let before = crate::library::metadata::read_metadata(&t.path).unwrap();
            let result = if attempt == 0 {
                tags::write(&t.path, &edit(&[(Field::Genre, "Smoke")]))
            } else {
                tags::write_picture(&t.path, &jpeg)
            };
            match result {
                Ok(()) => {
                    scanner::ingest_file(&t.path, &db, Some(&artwork)).unwrap();
                    let after = crate::library::metadata::read_metadata(&t.path).unwrap();
                    let (length, errors) = hear(&t.path);
                    assert!(
                        errors.is_empty() || !errors_before.is_empty(),
                        "{name}: decoded cleanly before the {what}, not after: {errors}"
                    );
                    assert!((length - length_before).abs() < 0.15, "{name}: {length_before:.2} s, after the {what} {length:.2} s");
                    assert_eq!((&after.title, &after.artist, &after.album), (&before.title, &before.artist, &before.album), "{name}: names after the {what}");
                    if attempt == 0 {
                        assert_eq!(after.genre, "Smoke", "{name}: the genre written");
                    } else {
                        assert!(after.has_artwork, "{name}: the cover written");
                    }
                    let row = db.get_track(t.id).unwrap().unwrap();
                    assert_eq!(said(&row), said(&after), "{name}: database and file after the {what}");
                    println!("  ok      {what:<6} {name}");
                }
                Err(e) => {
                    assert_eq!(std::fs::read(&t.path).unwrap(), bytes_before, "{name}: refused the {what} but changed the file");
                    println!("  nein    {what:<6} {name}: {e}");
                }
            }
            whole(&format!("{what} on {name}"));
        }
    }
    let unknown_after = id3_frames(&unknown);
    let bytes = std::fs::read(&unknown).unwrap();
    assert!(bytes.windows(8).any(|w| w == b"iTunNORM"), "iTunes' own comment kept");
    assert!(bytes.windows(7).any(|w| w == b"USLT\0\0\0") && bytes.windows(3).any(|w| w == b"eng"), "the lyrics keep their language");
    assert_eq!(read_extras(&unknown), extras, "an edit of the genre keeps the rest of „info\"");
    println!("unbekannte rahmen nachher: {unknown_after:?}");
    let mut lost = unknown_after.clone();
    let lost: Vec<_> = unknown_before
        .iter()
        .filter(|f| match lost.iter().position(|g| g == *f) {
            Some(at) => {
                lost.remove(at);
                false
            }
            None => true,
        })
        .collect();
    println!("verloren: {lost:?}");

    let left: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with(".kessler-"))
        .collect();
    assert!(left.is_empty(), "copies left behind");
    let again = scanner::scan_directory(&root, &db, Some(&artwork), None, Arrival::Now).unwrap();
    assert_eq!(whole("second scan"), count, "the second scan keeps the count");
    println!("zweiter scan: {} neu, {} aktualisiert", again.added, again.updated);
    assert!(lost.is_empty(), "frames lost in an edit: {lost:?}");
}
