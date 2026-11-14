use std::path::{Path, PathBuf};

use super::{metadata, scanner};
use crate::db::models::Track;
use crate::db::Database;

pub fn passed_over(paths: &[PathBuf]) -> Vec<(String, u32)> {
    let mut counted: Vec<(String, u32)> = Vec::new();

    let mut note = |path: &Path| {
        if let Some(ext) = metadata::foreign_audio(path) {
            match counted.iter_mut().find(|(had, _)| *had == ext) {
                Some((_, running)) => *running += 1,
                None => counted.push((ext, 1)),
            }
        }
    };

    for path in paths {
        if path.is_dir() {
            for entry in walkdir::WalkDir::new(path)
                .follow_links(true)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                if entry.path().is_file() {
                    note(entry.path());
                }
            }
        } else if path.is_file() {
            note(path);
        }
    }

    counted.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    counted
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outcome {
    pub added: u32,
    pub already: u32,
    pub failed: Vec<(PathBuf, String)>,
    pub handed: u32,
}

pub fn gather(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for path in paths {
        if path.is_dir() {
            for entry in walkdir::WalkDir::new(path)
                .follow_links(true)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let path = entry.path();
                if path.is_file() && metadata::is_supported_audio(path) {
                    found.push(path.to_path_buf());
                }
            }
        } else if path.is_file() && metadata::is_supported_audio(path) {
            found.push(path.clone());
        }
    }
    found.sort();
    found.dedup();
    found
}

pub fn tidy(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '"' | '*' | '?' | '<' | '>' | '|' => ' ',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let cleaned = cleaned.trim_matches('.').trim().to_string();
    cleaned.chars().take(90).collect::<String>().trim().to_string()
}

pub fn destination(root: &Path, track: &Track, source: &Path) -> PathBuf {
    let artist = or_else(tidy(crate::db::grouping_artist(track)), "unbekannt");
    let album = or_else(tidy(&track.album), "unbekanntes album");

    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unbenannt");
    let title = or_else(tidy(&track.title), stem);

    let numbered = match track.track_number {
        Some(number) if number > 0 => format!("{number:02} {title}"),
        _ => title,
    };
    let kind = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("mp3")
        .to_lowercase();

    root.join(artist).join(album).join(format!("{numbered}.{kind}"))
}

fn or_else(value: String, fallback: &str) -> String {
    if value.is_empty() {
        fallback.to_string()
    } else {
        value
    }
}

pub fn free(wanted: &Path) -> PathBuf {
    if !wanted.exists() {
        return wanted.to_path_buf();
    }
    let parent = wanted.parent().unwrap_or(Path::new("."));
    let stem = wanted
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unbenannt");
    let kind = wanted.extension().and_then(|e| e.to_str()).unwrap_or("mp3");
    for number in 2..1000 {
        let candidate = parent.join(format!("{stem} {number}.{kind}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    wanted.to_path_buf()
}

fn already_there(target: &Path, source: &Path) -> bool {
    match (std::fs::metadata(target), std::fs::metadata(source)) {
        (Ok(there), Ok(offered)) => there.len() == offered.len(),
        _ => false,
    }
}

pub fn refiled(roots: &[PathBuf], before: &Track, after: &Track) -> Option<PathBuf> {
    let path = &before.path;
    let file = path.file_name()?;
    let album_dir = path.parent()?;
    let artist_dir = album_dir.parent()?;
    let base = artist_dir.parent()?;
    if !roots.iter().any(|root| base.starts_with(root)) {
        return None;
    }
    let named = |dir: &Path, wanted: &str| {
        dir.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.to_lowercase() == wanted.to_lowercase())
    };
    let old = destination(base, before, path);
    let old_artist = old.parent()?.parent()?.file_name()?.to_str()?;
    let old_album = old.parent()?.file_name()?.to_str()?;
    if !named(artist_dir, old_artist) || !named(album_dir, old_album) {
        return None;
    }
    let new = destination(base, after, path);
    let ours = old
        .file_name()
        .and_then(|n| n.to_str())
        .zip(file.to_str())
        .is_some_and(|(a, b)| a.to_lowercase() == b.to_lowercase());
    let new = if ours { new } else { new.with_file_name(file) };
    let new = super::as_the_disk_spells_it(&new, &mut super::DiskNames::new());
    if new.to_string_lossy().to_lowercase() == path.to_string_lossy().to_lowercase() {
        return None;
    }
    Some(free(&new))
}

pub fn move_filed(from: &Path, to: &Path) -> std::io::Result<PathBuf> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let to = free(to);
    std::fs::rename(from, &to)?;
    let mut dir = from.parent();
    for _ in 0..2 {
        let Some(here) = dir else { break };
        let left: Vec<_> = match std::fs::read_dir(here) {
            Ok(entries) => entries.filter_map(|e| e.ok()).map(|e| e.file_name()).collect(),
            Err(_) => break,
        };
        if left.iter().any(|name| name != ".DS_Store") {
            break;
        }
        let _ = std::fs::remove_file(here.join(".DS_Store"));
        if std::fs::remove_dir(here).is_err() {
            break;
        }
        dir = here.parent();
    }
    Ok(to)
}

#[derive(Debug, Clone, Copy)]
pub struct Progress {
    pub done: u32,
    pub total: u32,
}

pub fn take(
    sources: &[PathBuf],
    root: &Path,
    db: &Database,
    artwork: Option<&Path>,
    moving: bool,
    to_music: bool,
    progress: Option<&std::sync::mpsc::Sender<Progress>>,
) -> Outcome {
    let mut outcome = Outcome::default();
    let mut filed: Vec<PathBuf> = Vec::new();
    let total = sources.len() as u32;
    let mut seen = super::DiskNames::new();

    for (index, source) in sources.iter().enumerate() {
        if let Some(tx) = progress {
            let _ = tx.send(Progress {
                done: index as u32,
                total,
            });
        }

        let track = match metadata::read_metadata(source) {
            Ok(track) => track,
            Err(e) => {
                outcome.failed.push((source.clone(), e.to_string()));
                continue;
            }
        };

        let wanted = super::as_the_disk_spells_it(&destination(root, &track, source), &mut seen);
        if already_there(&wanted, source) {
            outcome.already += 1;
            continue;
        }
        let target = free(&wanted);

        if let Some(parent) = target.parent() {
            if !parent.is_dir() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    outcome.failed.push((source.clone(), e.to_string()));
                    continue;
                }
                seen.clear();
            }
        }

        if let Err(e) = place(source, &target, moving) {
            outcome.failed.push((source.clone(), e));
            continue;
        }

        if let Err(e) = scanner::ingest_file(&target, db, artwork) {
            outcome.failed.push((target.clone(), e.to_string()));
            continue;
        }
        if to_music {
            filed.push(target.clone());
        }
        outcome.added += 1;
    }

    outcome.handed = hand_to_music(&filed);

    if let Some(tx) = progress {
        let _ = tx.send(Progress { done: total, total });
    }
    outcome
}

fn place(source: &Path, target: &Path, moving: bool) -> std::result::Result<(), String> {
    if moving && std::fs::rename(source, target).is_ok() {
        return Ok(());
    }
    std::fs::copy(source, target).map_err(|e| e.to_string())?;
    if moving {
        let _ = std::fs::remove_file(source);
    }
    Ok(())
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const PER_SCRIPT: usize = 100;

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn music_script(paths: &[PathBuf]) -> String {
    let files: Vec<String> = paths
        .iter()
        .filter(|p| !p.to_string_lossy().contains(['"', '\\']))
        .map(|p| format!("POSIX file \"{}\"", p.display()))
        .collect();
    format!("tell application \"Music\" to add {{{}}}", files.join(", "))
}

#[cfg(target_os = "macos")]
pub fn hand_to_music(paths: &[PathBuf]) -> u32 {
    let mut handed = 0;
    for batch in paths.chunks(PER_SCRIPT) {
        let sent = std::process::Command::new("osascript")
            .arg("-e")
            .arg(music_script(batch))
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        if sent {
            handed += batch.len() as u32;
        }
    }
    handed
}

#[cfg(not(target_os = "macos"))]
pub fn hand_to_music(_paths: &[PathBuf]) -> u32 {
    0
}

#[cfg(target_os = "macos")]
pub fn music_tracks(files: &[(String, PathBuf)], launch: bool) -> Vec<Option<String>> {
    let mut args: Vec<String> = vec![if launch { "1" } else { "0" }.to_string()];
    for (title, path) in files {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
        args.push(if title.trim().is_empty() { stem.to_string() } else { title.clone() });
        args.push(path.to_string_lossy().to_string());
    }
    let out = std::process::Command::new("osascript")
        .arg("-e")
        .arg(MUSIC_FIND)
        .args(&args)
        .output();
    let said = match out {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).to_string(),
        _ => String::new(),
    };
    let mut lines = said.lines().map(str::trim);
    files
        .iter()
        .map(|_| lines.next().filter(|l| !l.is_empty()).map(str::to_string))
        .collect()
}

#[cfg(not(target_os = "macos"))]
pub fn music_tracks(files: &[(String, PathBuf)], _launch: bool) -> Vec<Option<String>> {
    vec![None; files.len()]
}

#[cfg(target_os = "macos")]
pub fn tell_music(tracks: &[(String, Option<PathBuf>)]) -> u32 {
    if tracks.is_empty() {
        return 0;
    }
    let mut args: Vec<String> = Vec::new();
    for (id, path) in tracks {
        args.push(id.clone());
        args.push(path.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_default());
    }
    std::process::Command::new("osascript")
        .arg("-e")
        .arg(MUSIC_MOVE)
        .args(&args)
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8_lossy(&out.stdout).trim().parse().ok())
        .unwrap_or(0)
}

#[cfg(not(target_os = "macos"))]
pub fn tell_music(_tracks: &[(String, Option<PathBuf>)]) -> u32 {
    0
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const MUSIC_FIND: &str = r#"on run argv
    set launch to item 1 of argv
    set found to {}
    if launch is "0" and application "Music" is not running then return ""
    tell application "Music"
        repeat with i from 2 to (count of argv) by 2
            set wanted to item (i + 1) of argv
            set answer to ""
            try
                repeat with t in (search library playlist 1 for (item i of argv) only songs)
                    try
                        if class of t is file track then
                            if POSIX path of (location of t as alias) is wanted then
                                set answer to persistent ID of t
                                exit repeat
                            end if
                        end if
                    end try
                end repeat
            end try
            set end of found to answer
        end repeat
    end tell
    set AppleScript's text item delimiters to linefeed
    return found as text
end run"#;

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const MUSIC_MOVE: &str = r#"on run argv
    set done to 0
    repeat with i from 1 to (count of argv) by 2
        try
            set newPath to item (i + 1) of argv
            tell application "Music"
                set t to (first file track of library playlist 1 whose persistent ID is (item i of argv))
            end tell
            if newPath is not "" then
                set there to (POSIX file newPath) as alias
                tell application "Music" to set location of t to there
            end if
            tell application "Music" to refresh t
            set done to done + 1
        end try
    end repeat
    return done as text
end run"#;

pub fn music_app_available() -> bool {
    cfg!(target_os = "macos")
}

#[cfg(test)]
mod refiling_tests {
    use super::*;

    fn track(path: &Path, artist: &str, album: &str, nr: u32, title: &str) -> Track {
        Track {
            id: 0,
            path: path.to_path_buf(),
            title: title.into(),
            artist: artist.into(),
            album_artist: String::new(),
            album: album.into(),
            genre: String::new(),
            year: None,
            track_number: Some(nr),
            disc_number: None,
            duration_secs: 1.0,
            bitrate: None,
            sample_rate: None,
            file_size: 1,
            format: "mp3".into(),
            rating: 0,
            play_count: 0,
            last_played: None,
            date_added: chrono::NaiveDateTime::default(),
            date_modified: chrono::NaiveDateTime::default(),
            file_hash: None,
            has_artwork: false,
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kessler-refile-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        crate::library::one_name_for(&dir)
    }

    #[test]
    fn a_renamed_artist_takes_the_file_to_its_new_folder() {
        let root = scratch("artist");
        let path = root.join("Paysage d´Hiver").join("Im Wald").join("01 Im Wald.mp3");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"x").unwrap();
        std::fs::write(path.parent().unwrap().join(".DS_Store"), b"").unwrap();
        let before = track(&path, "Paysage d´Hiver", "Im Wald", 1, "Im Wald");
        let after = track(&path, "Paysage d'Hiver", "Im Wald", 1, "Im Wald");

        let target = refiled(std::slice::from_ref(&root), &before, &after).expect("filed, so moved");
        assert_eq!(target, root.join("Paysage d'Hiver").join("Im Wald").join("01 Im Wald.mp3"));
        assert_eq!(move_filed(&path, &target).unwrap(), target);
        assert!(target.is_file());
        assert!(!root.join("Paysage d´Hiver").exists(), "the empty folders stayed");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_name_of_the_files_own_is_kept() {
        let root = scratch("own");
        let path = root.join("Alda").join("Tahoma").join("1-01 Tahoma.mp3");
        let before = track(&path, "Alda", "Tahoma", 1, "Tahoma");
        let after = track(&path, "Alda", "Tahoma (live)", 1, "Tahoma");
        assert_eq!(
            refiled(std::slice::from_ref(&root), &before, &after),
            Some(root.join("Alda").join("Tahoma (live)").join("1-01 Tahoma.mp3"))
        );

        let ours = root.join("Alda").join("Tahoma").join("01 Tahoma.mp3");
        let before = track(&ours, "Alda", "Tahoma", 1, "Tahoma");
        let after = track(&ours, "Alda", "Tahoma", 1, "Tahoma Pt. 1");
        assert_eq!(
            refiled(std::slice::from_ref(&root), &before, &after),
            Some(root.join("Alda").join("Tahoma").join("01 Tahoma Pt. 1.mp3"))
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn two_files_planned_for_one_name_both_survive() {
        let root = scratch("merge");
        let one = root.join("Alda").join("Tahoma CD1").join("01 Intro.mp3");
        let two = root.join("Alda").join("Tahoma CD2").join("01 Intro.mp3");
        for (path, body) in [(&one, b"first"), (&two, b"secnd")] {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, body).unwrap();
        }
        let roots = std::slice::from_ref(&root);
        let plan: Vec<_> = [(&one, "Tahoma CD1"), (&two, "Tahoma CD2")]
            .into_iter()
            .map(|(path, album)| {
                let before = track(path, "Alda", album, 1, "Intro");
                let after = track(path, "Alda", "Tahoma", 1, "Intro");
                (path.clone(), refiled(roots, &before, &after).expect("moves"))
            })
            .collect();
        let mut landed = Vec::new();
        for (from, to) in &plan {
            landed.push(move_filed(from, to).unwrap());
        }
        assert_ne!(landed[0], landed[1]);
        let mut bodies: Vec<_> = landed.iter().map(|p| std::fs::read(p).unwrap()).collect();
        bodies.sort();
        assert_eq!(bodies, vec![b"first".to_vec(), b"secnd".to_vec()]);
        assert!(!root.join("Alda").join("Tahoma CD1").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn what_kessler_did_not_file_stays_put() {
        let root = scratch("put");
        let by_hand = root.join("Lieblinge").join("Sommer").join("01 Tahoma.mp3");
        let before = track(&by_hand, "Alda", "Tahoma", 1, "Tahoma");
        let after = track(&by_hand, "Alda Live", "Tahoma", 1, "Tahoma");
        assert_eq!(refiled(std::slice::from_ref(&root), &before, &after), None);

        let outside = Path::new("/elsewhere/Alda/Tahoma/01 Tahoma.mp3");
        let before = track(outside, "Alda", "Tahoma", 1, "Tahoma");
        let after = track(outside, "Alda Live", "Tahoma", 1, "Tahoma");
        assert_eq!(refiled(std::slice::from_ref(&root), &before, &after), None);

        let filed = root.join("Alda").join("Tahoma").join("01 Tahoma.mp3");
        let before = track(&filed, "Alda", "Tahoma", 1, "Tahoma");
        let mut after = before.clone();
        after.genre = "ambient".into();
        assert_eq!(refiled(std::slice::from_ref(&root), &before, &after), None);
        let _ = std::fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn music_is_told_about_the_lot_at_once() {
        let files = vec![
            PathBuf::from("/m/boards of canada/music/01 olson.mp3"),
            PathBuf::from("/m/boards of canada/music/02 roygbiv.mp3"),
        ];
        let script = music_script(&files);
        let expected = concat!(
            r#"tell application "Music" to add {"#,
            r#"POSIX file "/m/boards of canada/music/01 olson.mp3", "#,
            r#"POSIX file "/m/boards of canada/music/02 roygbiv.mp3"}"#,
        );
        assert_eq!(script, expected);

        let many: Vec<PathBuf> = (0..101).map(|i| PathBuf::from(format!("/m/{i}.mp3"))).collect();
        assert_eq!(many.chunks(PER_SCRIPT).count(), 2);
    }

    #[test]
    fn a_name_that_could_carry_a_command_does_not_go() {
        let sneaky = PathBuf::from("/m/a\" & (do shell script \"rm -rf ~\") & \".mp3");
        let script = music_script(&[sneaky, PathBuf::from("/m/plain.mp3")]);
        assert_eq!(script, "tell application \"Music\" to add {POSIX file \"/m/plain.mp3\"}");
        assert!(!script.contains("shell script"));
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kessler-import-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn track(artist: &str, album: &str, title: &str, number: Option<u32>) -> Track {
        Track {
            id: 0,
            path: PathBuf::from("/in/01.mp3"),
            title: title.into(),
            artist: artist.into(),
            album_artist: String::new(),
            album: album.into(),
            genre: String::new(),
            year: None,
            track_number: number,
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

    #[test]
    fn a_track_is_filed_under_what_it_says_it_is() {
        let root = Path::new("/music");
        let source = Path::new("/downloads/New folder (2)/01.mp3");
        let at = destination(
            root,
            &track("boards of canada", "geogaddi", "music is math", Some(2)),
            source,
        );
        assert_eq!(
            at,
            Path::new("/music/boards of canada/geogaddi/02 music is math.mp3")
        );
    }

    #[test]
    fn a_record_is_filed_under_the_folder_that_is_already_there() {
        let root = scratch("cave");
        std::fs::create_dir_all(root.join("Cave").join("Allways")).unwrap();

        let wanted = destination(
            &root,
            &track("CAVE", "Allways", "san'yago", Some(2)),
            Path::new("/in/02.mp3"),
        );
        assert_eq!(wanted, root.join("CAVE/Allways/02 san'yago.mp3"), "the tag's own spelling");

        let filed = super::super::as_the_disk_spells_it(&wanted, &mut super::super::DiskNames::new());
        assert_eq!(
            filed,
            root.join("Cave/Allways/02 san'yago.mp3"),
            "filed under a second name for a folder that is already there"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_name_never_becomes_a_folder() {
        assert_eq!(tidy("AC/DC"), "AC DC");
        assert_eq!(tidy("06:30"), "06 30");
        assert_eq!(tidy("say \"hello\""), "say hello");
        assert_eq!(tidy("  spaced   out  "), "spaced out");
        assert_eq!(tidy("...and justice"), "and justice");
        assert_eq!(tidy("\t\nname"), "name");
        assert_eq!(tidy("坂本龍一"), "坂本龍一");
        assert_eq!(tidy("Mjölnir (Röyksopp Mix)"), "Mjölnir (Röyksopp Mix)");
    }

    #[test]
    fn a_file_with_no_tags_keeps_its_own_name() {
        let at = destination(
            Path::new("/music"),
            &track("", "", "", None),
            Path::new("/downloads/weird sounds.flac"),
        );
        assert_eq!(
            at,
            Path::new("/music/unbekannt/unbekanntes album/weird sounds.flac")
        );
    }

    #[test]
    fn a_compilation_is_filed_under_its_album_artist() {
        let mut t = track("bob james", "various", "nautilus", Some(3));
        t.album_artist = "verschiedene".into();
        let at = destination(Path::new("/music"), &t, Path::new("/in/x.mp3"));
        assert_eq!(
            at,
            Path::new("/music/verschiedene/various/03 nautilus.mp3")
        );
    }

    #[test]
    fn a_taken_name_gets_a_number() {
        let dir = scratch("free");
        let wanted = dir.join("02 music is math.mp3");
        assert_eq!(free(&wanted), wanted);

        std::fs::write(&wanted, b"first").unwrap();
        let second = free(&wanted);
        assert_eq!(second, dir.join("02 music is math 2.mp3"));

        std::fs::write(&second, b"second").unwrap();
        assert_eq!(free(&wanted), dir.join("02 music is math 3.mp3"));
        assert_eq!(std::fs::read(&wanted).unwrap(), b"first");
    }

    #[test]
    fn the_same_file_twice_is_recognised() {
        let dir = scratch("same");
        let there = dir.join("a.mp3");
        let offered = dir.join("b.mp3");
        std::fs::write(&there, b"same size!!").unwrap();
        std::fs::write(&offered, b"same size!!").unwrap();
        assert!(already_there(&there, &offered));

        std::fs::write(&offered, b"a different length").unwrap();
        assert!(!already_there(&there, &offered));
        assert!(!already_there(&dir.join("nothing.mp3"), &offered));
    }

    #[test]
    fn a_dropped_folder_is_its_music() {
        let dir = scratch("gather");
        let album = dir.join("album");
        std::fs::create_dir_all(&album).unwrap();
        for name in ["02 b.mp3", "01 a.mp3", "cover.jpg", "notes.txt", "03 c.flac"] {
            std::fs::write(album.join(name), b"x").unwrap();
        }
        let loose = dir.join("loose.mp3");
        std::fs::write(&loose, b"x").unwrap();

        let found = gather(std::slice::from_ref(&dir));
        let names: Vec<String> = found
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["01 a.mp3", "02 b.mp3", "03 c.flac", "loose.mp3"]);

        assert_eq!(gather(&[loose.clone(), loose.clone()]).len(), 1);
        assert_eq!(gather(std::slice::from_ref(&loose)).len(), 1);
    }

    #[test]
    fn music_is_filed_and_read_in() {
        use crate::library::fixtures;
        use crate::library::tags::{Edit, Field};

        let dir = scratch("take");
        let inbox = dir.join("inbox");
        let root = dir.join("library");
        std::fs::create_dir_all(&root).unwrap();

        for (name, title, number) in [("01.mp3", "olson", "1"), ("02.mp3", "roygbiv", "2")] {
            let at = inbox.join(name);
            fixtures::place(&at, fixtures::MP3);
            let mut edit = Edit::new();
            edit.set(Field::Title, title);
            edit.set(Field::Artist, "boards of canada");
            edit.set(Field::Album, "music has the right to children");
            edit.set(Field::TrackNumber, number);
            crate::library::tags::write(&at, &edit).unwrap();
        }

        let db = Database::open_memory().unwrap();
        let outcome = take(
            &gather(std::slice::from_ref(&inbox)),
            &root,
            &db,
            None,
            false,
            false,
            None,
        );

        assert_eq!(outcome.added, 2, "{:?}", outcome.failed);
        assert_eq!(outcome.already, 0);
        assert!(outcome.failed.is_empty());

        let album = root
            .join("boards of canada")
            .join("music has the right to children");
        assert!(album.join("01 olson.mp3").exists());
        assert!(album.join("02 roygbiv.mp3").exists());
        assert!(inbox.join("01.mp3").exists());

        let tracks = db
            .get_tracks(
                Some("boards of canada"),
                None,
                crate::db::models::SortField::TrackNumber,
                crate::db::models::SortOrder::Ascending,
                None,
            )
            .unwrap();
        let titles: Vec<&str> = tracks.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, ["olson", "roygbiv"]);

        let again = take(
            &gather(std::slice::from_ref(&inbox)),
            &root,
            &db,
            None,
            false,
            false,
            None,
        );
        assert_eq!(again.added, 0);
        assert_eq!(again.already, 2);
    }

    #[test]
    fn moved_in_and_read_again_every_file_is_there_once() {
        use crate::library::fixtures;
        use crate::library::tags::{Edit, Field};

        let dir = scratch("once");
        let inbox = dir.join("inbox");
        let root = dir.join("library");
        let decomposed = "Bjo\u{308}rk";
        let album = root.join(decomposed).join("Vespertine");
        std::fs::create_dir_all(&album).unwrap();
        fixtures::place(&album.join("01 Hidden Place.mp3"), fixtures::MP3);
        let mut edit = Edit::new();
        edit.set(Field::Title, "Hidden Place");
        edit.set(Field::Artist, decomposed);
        edit.set(Field::Album, "Vespertine");
        edit.set(Field::TrackNumber, "1");
        crate::library::tags::write(&album.join("01 Hidden Place.mp3"), &edit).unwrap();

        for (name, title, number) in [("a.mp3", "Cocoon", "2"), ("b.mp3", "Aurora", "3")] {
            let at = inbox.join(name);
            fixtures::place(&at, fixtures::MP3);
            let mut edit = Edit::new();
            edit.set(Field::Title, title);
            edit.set(Field::Artist, "BJÖRK");
            edit.set(Field::Album, "VESPERTINE");
            edit.set(Field::TrackNumber, number);
            crate::library::tags::write(&at, &edit).unwrap();
        }

        let db = Database::open_memory().unwrap();
        crate::library::scanner::scan_directory(&root, &db, None, None, crate::library::scanner::Arrival::Now).unwrap();
        let outcome = take(&gather(std::slice::from_ref(&inbox)), &root, &db, None, true, false, None);
        assert_eq!(outcome.added, 2, "{:?}", outcome.failed);
        crate::library::scanner::scan_directory(&root, &db, None, None, crate::library::scanner::Arrival::Now).unwrap();

        assert_eq!(db.track_count().unwrap(), 3, "a file came back as a second row");
        let artists: Vec<_> = std::fs::read_dir(&root).unwrap().filter_map(|e| e.ok()).collect();
        assert_eq!(artists.len(), 1, "a second artist folder was made");
        let files: Vec<_> = std::fs::read_dir(&album).unwrap().filter_map(|e| e.ok()).collect();
        assert_eq!(files.len(), 3);
        assert_eq!(gather(std::slice::from_ref(&inbox)).len(), 0, "moving left a file behind");
    }

    #[test]
    fn copying_and_moving_are_what_they_say() {
        let dir = scratch("place");
        let source = dir.join("from.mp3");
        let target = dir.join("to.mp3");
        std::fs::write(&source, b"music").unwrap();

        place(&source, &target, false).unwrap();
        assert!(source.exists(), "a copy leaves the original");
        assert_eq!(std::fs::read(&target).unwrap(), b"music");

        let moved = dir.join("moved.mp3");
        place(&source, &moved, true).unwrap();
        assert!(!source.exists(), "a move takes it away");
        assert_eq!(std::fs::read(&moved).unwrap(), b"music");
    }
}

#[cfg(test)]
mod passed_over_tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kessler-passed-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(dir: &Path, name: &str) {
        std::fs::write(dir.join(name), b"x").unwrap();
    }

    #[test]
    fn a_folder_of_unplayable_music_is_counted_by_format() {
        let dir = scratch("folder");
        let album = dir.join("Some Artist").join("Some Album");
        std::fs::create_dir_all(&album).unwrap();
        for name in [
            "01.opus", "02.opus", "03.opus", "04.wma", "05.m4a", "06.aiff", "cover.jpg", "notes.txt",
        ] {
            touch(&album, name);
        }

        assert_eq!(
            passed_over(std::slice::from_ref(&dir)),
            vec![("opus".to_string(), 3), ("wma".to_string(), 1)]
        );
    }

    #[test]
    fn files_named_one_by_one_are_counted_too() {
        let dir = scratch("files");
        touch(&dir, "a.wma");
        touch(&dir, "b.mp3");

        assert_eq!(
            passed_over(&[dir.join("a.wma"), dir.join("b.mp3")]),
            vec![("wma".to_string(), 1)],
            "the mp3 is playable, so it is not passed over"
        );
    }

    #[test]
    fn nothing_foreign_is_an_empty_answer() {
        let dir = scratch("clean");
        touch(&dir, "a.mp3");
        touch(&dir, "cover.jpg");
        assert!(passed_over(&[dir]).is_empty());
    }
}
