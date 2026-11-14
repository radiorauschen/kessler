#![allow(clippy::too_many_arguments, clippy::collapsible_if, clippy::collapsible_else_if)]

pub const DEV: bool = false;

pub const VERSION_SHOWN: &str =
    if DEV { concat!(env!("CARGO_PKG_VERSION"), " · dev") } else { env!("CARGO_PKG_VERSION") };

#[macro_use]
mod lang;
mod config;
mod db;
mod error;
#[cfg(feature = "gui")]
#[deny(dead_code)]
mod gui;
#[cfg(feature = "ipod")]
mod ipod;
mod json;
mod lastfm;
mod library;
mod media;
mod misaki;
mod player;
mod podcast;
mod radio;

use std::path::PathBuf;
use std::sync::mpsc;

use clap::{Parser, Subcommand};

use crate::config::Config;
use crate::db::Database;
use crate::error::Result;
use crate::library::{scanner::ScanProgress, LibraryManager};

#[derive(Parser, Debug)]
#[command(
    name = "kessler",
    version,
    about = "A small music player and library, with iPod Classic support"
)]
struct Cli {
    #[arg(short, long, value_name = "FILE", global = true)]
    config: Option<PathBuf>,

    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    Scan {
        #[arg(value_name = "DIR")]
        dirs: Vec<PathBuf>,

        #[arg(long)]
        prune: bool,
    },
    Stats,
    Paths,
    #[cfg(feature = "gui")]
    Gui,
    Lastfm {
        #[command(subcommand)]
        action: LastfmAction,
    },
    #[cfg(feature = "ipod")]
    Ipod {
        #[command(subcommand)]
        action: IpodAction,
    },
    Play,
    Pause,
    Toggle,
    Next,
    Prev,
    Stop,
    Now,
}

#[derive(Subcommand, Debug)]
enum LastfmAction {
    Setup {
        #[arg(long, value_name = "KEY")]
        key: String,
        #[arg(long, value_name = "SECRET")]
        secret: String,
    },
    Login,
    Status,
    Logout,
}

#[cfg(feature = "ipod")]
#[derive(Subcommand, Debug)]
enum IpodAction {
    List,
    Info {
        #[arg(value_name = "MOUNT")]
        mount: Option<PathBuf>,
    },
    Tracks {
        #[arg(value_name = "MOUNT")]
        mount: Option<PathBuf>,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    Playlists {
        #[arg(value_name = "MOUNT")]
        mount: Option<PathBuf>,
    },
    Sync {
        #[arg(value_name = "MOUNT")]
        mount: Option<PathBuf>,
        #[arg(long)]
        apply: bool,
        #[arg(long, value_name = "NAME")]
        artist: Option<String>,
        #[arg(long, value_name = "NAME")]
        album: Option<String>,
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
        #[arg(long)]
        transcode: bool,
    },
    Backup {
        #[arg(value_name = "MOUNT")]
        mount: Option<PathBuf>,
    },
    Report {
        #[arg(value_name = "MOUNT")]
        mount: Option<PathBuf>,
    },
}

fn main() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    unsafe {
        libc::mallopt(libc::M_ARENA_MAX, 2);
    }
    if let Err(e) = run() {
        eprintln!("kessler: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    init_logging(cli.verbose);

    let config = match &cli.config {
        Some(path) => Config::load_from(path)?,
        None => Config::load()?,
    };

    match cli.command {
        Some(Command::Scan { dirs, prune }) => cmd_scan(&config, dirs, prune),
        Some(Command::Stats) => cmd_stats(&config),
        Some(Command::Paths) => cmd_paths(&config),
        #[cfg(feature = "gui")]
        Some(Command::Gui) => gui::run(config),
        Some(Command::Lastfm { action }) => cmd_lastfm(config, action),
        #[cfg(feature = "ipod")]
        Some(Command::Ipod { action }) => cmd_ipod(&config, action),
        Some(Command::Play) => cmd_remote("play"),
        Some(Command::Pause) => cmd_remote("pause"),
        Some(Command::Toggle) => cmd_remote("toggle"),
        Some(Command::Next) => cmd_remote("next"),
        Some(Command::Prev) => cmd_remote("prev"),
        Some(Command::Stop) => cmd_remote("stop"),
        Some(Command::Now) => cmd_remote("now"),
        #[cfg(feature = "gui")]
        None => gui::run(config),
        #[cfg(not(feature = "gui"))]
        None => {
            use clap::CommandFactory;
            Cli::command().print_help()?;
            Ok(())
        }
    }
}

fn cmd_remote(command: &str) -> Result<()> {
    #[cfg(all(target_os = "linux", feature = "gui"))]
    let done = if command == "now" {
        crate::media::remote::now().map(|line| {
            if let Some(line) = line {
                println!("{line}");
            }
        })
    } else {
        crate::media::remote::send(command)
    };
    #[cfg(not(all(target_os = "linux", feature = "gui")))]
    let done: std::result::Result<(), String> = {
        let _ = command;
        Err("das geht nur unter linux — auf dem mac die medientasten".into())
    };
    if let Err(why) = done {
        eprintln!("kessler: {why}");
        std::process::exit(1);
    }
    Ok(())
}

pub fn launched_from_a_bundle() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let mut parts = exe.iter().rev().skip(1);
    parts.next().is_some_and(|p| p == "MacOS")
        && parts.next().is_some_and(|p| p == "Contents")
        && parts
            .next()
            .and_then(|p| p.to_str())
            .is_some_and(|p| p.ends_with(".app"))
}

fn init_logging(verbosity: u8) {
    use tracing_subscriber::EnvFilter;

    let default_level = match verbosity {
        0 => "warn",
        1 => "info",
        _ => "debug",
    };

    let filter = EnvFilter::try_from_env("CRATEDIG_LOG")
        .unwrap_or_else(|_| EnvFilter::new(format!("kessler={default_level}")));

    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(false)
        .try_init();
}

fn cmd_scan(config: &Config, dirs: Vec<PathBuf>, prune: bool) -> Result<()> {
    let dirs = if dirs.is_empty() {
        config.music_dirs.clone()
    } else {
        dirs
    };

    if dirs.is_empty() {
        eprintln!(
            "No directories to scan. Pass them as arguments or set `music_dirs` in {}.",
            Config::config_path().display()
        );
        return Ok(());
    }

    let db = Database::open(&config.db_path())?;
    let manager = LibraryManager::new(&db).with_artwork(config.artwork_dir());

    let (tx, rx) = mpsc::channel::<ScanProgress>();
    let printer = std::thread::spawn(move || {
        let mut skipped: Vec<(String, String)> = Vec::new();
        let mut foreign: Vec<(String, u32)> = Vec::new();

        for progress in rx {
            match progress {
                ScanProgress::Counted { total } => {
                    println!("{total} files to read")
                }
                ScanProgress::StartingDir(dir) => println!("Scanning {dir}"),
                ScanProgress::FileScanned { count, path } => {
                    use std::io::Write;
                    let name = std::path::Path::new(&path)
                        .file_name()
                        .map(|n| n.to_string_lossy().chars().take(48).collect::<String>())
                        .unwrap_or_default();
                    print!("\r  {count:>6} files  {name:<48}");
                    let _ = std::io::stdout().flush();
                }
                ScanProgress::FileError { path, error } => skipped.push((path, error)),
                ScanProgress::Updated(_) => {}
                ScanProgress::Foreign { counted } => {
                    crate::library::scanner::merge_foreign(&mut foreign, counted)
                }
                ScanProgress::Complete {
                    total,
                    added,
                    updated,
                } => {
                    println!("\r  {total} files — {added} added, {updated} updated{:<20}", "");
                    report_foreign(&foreign);
                    report_skipped(&skipped);
                }
            }
        }
    });

    let (total, added, updated) = manager.scan_dirs(&dirs, Some(&tx))?;
    drop(tx);
    let _ = printer.join();

    if prune {
        let removed = manager.clean_missing()?;
        println!("Removed {removed} tracks with missing files.");
    }

    println!("Done: {total} files seen, {added} added, {updated} updated.");
    Ok(())
}

fn cmd_stats(config: &Config) -> Result<()> {
    let db = Database::open(&config.db_path())?;
    let tracks = db.track_count()?;
    let artists = db.get_artists(None)?.len();
    let albums = db.get_albums(None, None)?.len();
    let playlists = db.get_playlists()?.len();

    println!("Library: {}", config.db_path().display());
    println!("  Tracks:    {tracks}");
    println!("  Artists:   {artists}");
    println!("  Albums:    {albums}");
    println!("  Playlists: {playlists}");
    Ok(())
}

#[cfg(feature = "ipod")]
fn cmd_ipod(config: &Config, action: IpodAction) -> Result<()> {
    use crate::ipod::device_info::DeviceInfo;
    use crate::ipod::{format_bytes, sync, IpodDevice};

    fn pick(mount: Option<PathBuf>) -> Result<IpodDevice> {
        if let Some(path) = mount {
            return IpodDevice::at(path);
        }
        let mut found = IpodDevice::detect();
        match found.len() {
            0 => Err(crate::error::KesslerError::Ipod(
                "no iPod found. Connect one and make sure it is mounted, or pass \
                 the mount point explicitly."
                    .into(),
            )),
            1 => Ok(found.remove(0)),
            _ => {
                let names: Vec<String> = found
                    .iter()
                    .map(|d| d.mount_point.display().to_string())
                    .collect();
                Err(crate::error::KesslerError::Ipod(format!(
                    "several iPods found ({}). Pass the one you mean.",
                    names.join(", ")
                )))
            }
        }
    }

    match action {
        IpodAction::List => {
            let found = IpodDevice::detect();
            if found.is_empty() {
                println!("No iPod found.");
            }
            for d in found {
                match d.capacity() {
                    Some((total, avail)) => println!(
                        "{}  ({}, {} free)",
                        d.mount_point.display(),
                        format_bytes(total),
                        format_bytes(avail)
                    ),
                    None => println!("{}", d.mount_point.display()),
                }
            }
        }

        IpodAction::Info { mount } => {
            let device = pick(mount)?;
            let db = device.read_database()?;

            let info = DeviceInfo::read(&device.mount_point);

            println!("iPod:      {}", device.name);
            println!("Mount:     {}", device.mount_point.display());
            println!("Model:     {}", info.generation.label());
            if let Some(m) = &info.model_number {
                println!("Model no.: {m}");
            }
            match info.writable() {
                Ok(()) => println!("Writing:   supported"),
                Err(why) => println!("Writing:   NOT supported — {why}"),
            }
            if let Some((total, avail)) = device.capacity() {
                let used = total.saturating_sub(avail);
                println!(
                    "Capacity:  {} total, {} used, {} free",
                    format_bytes(total),
                    format_bytes(used),
                    format_bytes(avail)
                );
            }
            println!("DB format: version 0x{:x}", db.version);
            println!("Tracks:    {}", db.tracks.len());
            println!("Playlists: {}", db.user_playlists().count());
            println!("Audio:     {}", format_bytes(db.total_bytes()));
        }

        IpodAction::Tracks { mount, limit } => {
            let device = pick(mount)?;
            let db = device.read_database()?;

            for track in db.tracks.iter().take(limit) {
                println!(
                    "{:>6}  {:<28}  {:<28}  {:>7}  {}",
                    track.id,
                    truncate(&track.artist, 28),
                    truncate(&track.title, 28),
                    track.duration_display(),
                    "*".repeat(track.stars() as usize),
                );
            }
            if db.tracks.len() > limit {
                println!("... and {} more", db.tracks.len() - limit);
            }
        }

        IpodAction::Sync {
            mount,
            apply,
            artist,
            album,
            limit,
            transcode,
        } => {
            let device = pick(mount)?;
            let info = DeviceInfo::read(&device.mount_point);
            let device_db = device.read_database()?;

            let library = Database::open(&config.db_path())?;
            let tracks = library.get_tracks(
                artist.as_deref(),
                album.as_deref(),
                crate::db::models::SortField::Artist,
                crate::db::models::SortOrder::Ascending,
                None,
            )?;

            if tracks.is_empty() {
                if artist.is_some() || album.is_some() {
                    println!("Nothing in the library matches that.");
                } else {
                    println!("The library is empty — run `kessler scan` first.");
                }
                return Ok(());
            }

            let whole = if artist.is_some() || album.is_some() {
                library.get_tracks(
                    None,
                    None,
                    crate::db::models::SortField::Artist,
                    crate::db::models::SortOrder::Ascending,
                    None,
                )?
            } else {
                tracks.clone()
            };
            let plan = sync::plan_in(&tracks, &whole, &device_db, info.generation, &sync::soundcheck_table(&library));
            let plan = match limit {
                Some(most) => plan.first(most),
                None => plan,
            };

            println!("iPod:            {} ({})", device.name, info.generation.label());
            if artist.is_some() || album.is_some() {
                println!("Matched:         {} tracks in the library", tracks.len());
            }
            println!("Already on iPod: {}", plan.already_present);
            println!("Only on iPod:    {} (kept)", plan.device_only);
            println!(
                "To copy:         {} tracks, {}",
                plan.to_copy.len(),
                format_bytes(plan.bytes_to_copy)
            );
            if !plan.missing_files.is_empty() {
                println!("Missing files:   {} (skipped)", plan.missing_files.len());
                for path in plan.missing_files.iter().take(MISSING_SHOWN) {
                    println!("                 {}", path.display());
                }
                if plan.missing_files.len() > MISSING_SHOWN {
                    println!(
                        "                 … and {} more",
                        plan.missing_files.len() - MISSING_SHOWN
                    );
                }
            }

            if !plan.incompatible.is_empty() {
                let formats = plan.incompatible_formats().join(", ");
                if transcode {
                    println!(
                        "To convert:      {} tracks ({formats} → {})",
                        plan.incompatible.len(),
                        sync::conversion_word(&plan.incompatible_formats())
                    );
                    if !sync::transcoder_available() {
                        println!();
                        println!(
                            "Nothing here can convert them (afconvert is macOS's own; \
                             elsewhere ffmpeg: `brew install ffmpeg`). Drop --transcode \
                             to skip these tracks."
                        );
                        return Ok(());
                    }
                } else {
                    println!(
                        "Cannot play:     {} tracks ({formats}) — this iPod plays {}",
                        plan.incompatible.len(),
                        info.generation.playable_formats().join(", ")
                    );
                    println!("                 Add --transcode to convert them to MP3.");
                }
            }

            if let Err(why) = info.writable() {
                println!();
                println!("Cannot write to this iPod: {why}");
                return Ok(());
            }

            let will_convert = transcode && !plan.incompatible.is_empty();
            if plan.is_empty() && !will_convert {
                println!();
                println!("Nothing to copy.");
                return Ok(());
            }

            if !apply {
                println!();
                let albums = albums_of(&plan.to_copy);
                if !albums.is_empty() {
                    println!("Albums to copy ({}), smallest first:", albums.len());
                    for (name, count) in albums.iter().take(ALBUMS_SHOWN) {
                        println!("  {:<54} {:>3}", truncate(name, 54), count);
                    }
                    if albums.len() > ALBUMS_SHOWN {
                        println!("  … and {} more", albums.len() - ALBUMS_SHOWN);
                    }
                    println!();
                }
                println!("This was a dry run. Re-run with --apply to copy.");
                return Ok(());
            }

            println!();
            sync::apply(&device, &info, plan, transcode, |p| match p {
                sync::SyncProgress::BackedUp(path) => {
                    println!("Backed up the old database to {}", path.display());
                }
                sync::SyncProgress::Copying {
                    index,
                    total,
                    title,
                } => {
                    use std::io::Write;
                    print!("\r  {index}/{total}  {:<48}", truncate(&title, 48));
                    let _ = std::io::stdout().flush();
                }
                sync::SyncProgress::Transcoding {
                    index,
                    total,
                    title,
                } => {
                    use std::io::Write;
                    print!(
                        "\r  {index}/{total}  converting {:<38}",
                        truncate(&title, 38)
                    );
                    let _ = std::io::stdout().flush();
                }
                sync::SyncProgress::CopyFailed { path, error } => {
                    eprintln!("\r  failed: {} — {error}", path.display());
                }
                sync::SyncProgress::WritingDatabase => {
                    println!("\rWriting the database...{:<40}", "");
                }
                sync::SyncProgress::Done { copied, failed, .. } => {
                    println!("Copied {copied} tracks, {failed} failed.");
                    println!("Eject the iPod before unplugging it.");
                }
            })?;
        }

        IpodAction::Report { mount } => {
            let device = pick(mount)?;
            let info = DeviceInfo::read(&device.mount_point);
            let db = device.read_database()?;
            let plan = Database::open(&config.db_path()).ok().and_then(|library| {
                let tracks = library
                    .get_tracks(
                        None,
                        None,
                        crate::db::models::SortField::Artist,
                        crate::db::models::SortOrder::Ascending,
                        None,
                    )
                    .ok()?;
                Some(sync::plan(&tracks, &db, info.generation, &sync::soundcheck_table(&library)))
            });
            print!("{}", crate::ipod::report::write(&device, &info, &db, plan.as_ref()));
        }

        IpodAction::Backup { mount } => {
            let device = pick(mount)?;
            let path = sync::backup_database(&device)?;
            println!("Backed up to {}", path.display());
        }

        IpodAction::Playlists { mount } => {
            let device = pick(mount)?;
            let db = device.read_database()?;

            for pl in &db.playlists {
                let kind = if pl.is_master { " (library)" } else { "" };
                println!("{:<40} {:>5} tracks{}", pl.name, pl.track_ids.len(), kind);
            }
        }
    }

    Ok(())
}

fn report_foreign(foreign: &[(String, u32)]) {
    if foreign.is_empty() {
        return;
    }
    let all: u32 = foreign.iter().map(|(_, n)| n).sum();
    let formats: Vec<String> = foreign
        .iter()
        .map(|(ext, n)| format!("{n} × {ext}"))
        .collect();

    println!();
    println!("{all} files were passed over — kessler cannot play these formats yet:");
    println!("  {}", formats.join(", "));
}

fn report_skipped(skipped: &[(String, String)]) {
    if skipped.is_empty() {
        return;
    }

    const SHOWN: usize = 10;
    println!();
    println!("{} files could not be read:", skipped.len());
    for (path, error) in skipped.iter().take(SHOWN) {
        let reason = error.rsplit(": ").next().unwrap_or(error);
        println!("  {path}");
        println!("    {reason}");
    }
    if skipped.len() > SHOWN {
        println!("  … and {} more", skipped.len() - SHOWN);
    }
}

#[cfg(feature = "ipod")]
const ALBUMS_SHOWN: usize = 12;

#[cfg(feature = "ipod")]
const MISSING_SHOWN: usize = 8;

#[cfg(feature = "ipod")]
fn albums_of(tracks: &[db::models::Track]) -> Vec<(String, usize)> {
    let mut albums: Vec<(String, usize)> = Vec::new();
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for track in tracks {
        let artist = if track.album_artist.is_empty() {
            &track.artist
        } else {
            &track.album_artist
        };
        let name = format!("{} — {}", artist, track.album);
        match seen.get(&name) {
            Some(&at) => albums[at].1 += 1,
            None => {
                seen.insert(name.clone(), albums.len());
                albums.push((name, 1));
            }
        }
    }

    albums.sort_by_key(|(_, count)| *count);
    albums
}

#[cfg(feature = "ipod")]
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
    }
}

fn cmd_paths(config: &Config) -> Result<()> {
    println!("Config:   {}", Config::config_path().display());
    println!("Database: {}", config.db_path().display());
    println!("Data dir: {}", Config::data_dir().display());
    if config.music_dirs.is_empty() {
        println!("Music:    (none configured)");
    } else {
        for dir in &config.music_dirs {
            println!("Music:    {}", dir.display());
        }
    }
    Ok(())
}

fn cmd_lastfm(mut config: Config, action: LastfmAction) -> Result<()> {
    match action {
        LastfmAction::Setup { key, secret } => {
            config.lastfm.api_key = Some(key.trim().to_string());
            config.lastfm.api_secret = Some(secret.trim().to_string());
            config.lastfm.session_key = None;
            config.lastfm.username = None;
            config.save()?;
            println!("Key und secret sind gespeichert in {}", Config::config_path().display());
            println!("Weiter mit: kessler lastfm login");
            Ok(())
        }
        LastfmAction::Login => {
            let (Some(key), Some(secret)) = (
                config.lastfm.api_key.clone(),
                config.lastfm.api_secret.clone(),
            ) else {
                println!("Erst: kessler lastfm setup --key … --secret …");
                return Ok(());
            };

            let token = lastfm::request_token(&key, &secret)?;
            let url = lastfm::approval_url(&key, &token);
            println!("Im Browser bestätigen:\n\n  {url}\n");
            if !lastfm::open_in_browser(&url) {
                println!("(Der Browser liess sich nicht öffnen — die Adresse von Hand aufrufen.)");
            }
            print!("Danach hier Enter drücken. ");
            use std::io::Write;
            let _ = std::io::stdout().flush();
            let mut line = String::new();
            let _ = std::io::stdin().read_line(&mut line);

            let (session, user) = lastfm::request_session(&key, &secret, &token)?;
            config.lastfm.session_key = Some(session);
            config.lastfm.username = Some(user.clone());
            config.save()?;
            println!("Verbunden als {user}.");
            Ok(())
        }
        LastfmAction::Status => {
            match (&config.lastfm.api_key, &config.lastfm.session_key) {
                (None, _) => println!("Kein schlüssel — kessler lastfm setup …"),
                (Some(_), None) => println!("Schlüssel da, aber nicht angemeldet — kessler lastfm login"),
                (Some(_), Some(_)) => {
                    let who = config.lastfm.username.as_deref().unwrap_or("unbekannt");
                    println!("Verbunden als {who}");
                    println!(
                        "\"gerade läuft\": {}",
                        if config.lastfm.now_playing { "an" } else { "aus" }
                    );
                }
            }
            let waiting = lastfm::queue::Queue::at(&config.scrobble_queue_path()).len();
            println!("Wartend: {waiting}");
            println!("Warteschlange: {}", config.scrobble_queue_path().display());
            Ok(())
        }
        LastfmAction::Logout => {
            config.lastfm.session_key = None;
            config.lastfm.username = None;
            config.save()?;
            println!("Sitzung vergessen. Key und secret bleiben.");
            Ok(())
        }
    }
}

#[cfg(test)]
mod bundle_tests {
    fn inside_bundle(path: &str) -> bool {
        let exe = std::path::Path::new(path);
        let mut parts = exe.iter().rev().skip(1);
        parts.next().is_some_and(|p| p == "MacOS")
            && parts.next().is_some_and(|p| p == "Contents")
            && parts
                .next()
                .and_then(|p| p.to_str())
                .is_some_and(|p| p.ends_with(".app"))
    }

    #[test]
    fn recognises_a_bundle_and_nothing_else() {
        assert!(inside_bundle("/Applications/Kessler.app/Contents/MacOS/kessler"));
        assert!(inside_bundle("/Users/someone/Desktop/Kessler.app/Contents/MacOS/kessler"));

        assert!(!inside_bundle("/Users/someone/code/kessler/target/release/kessler"));
        assert!(!inside_bundle("/usr/local/bin/kessler"));
        assert!(!inside_bundle("/Users/someone/MacOS/kessler"));
        assert!(!inside_bundle("/Users/someone/Kessler.app/MacOS/kessler"));
        assert!(!inside_bundle("/Users/someone/Kessler/Contents/MacOS/kessler"));
    }
}

#[cfg(all(test, feature = "ipod"))]
mod album_tests {
    use super::albums_of;
    use crate::db::models::Track;

    fn track(album_artist: &str, artist: &str, album: &str) -> Track {
        Track {
            id: 0,
            path: std::path::PathBuf::from("/x"),
            title: String::new(),
            artist: artist.into(),
            album_artist: album_artist.into(),
            album: album.into(),
            genre: String::new(),
            year: None,
            track_number: None,
            disc_number: None,
            duration_secs: 0.0,
            bitrate: None,
            sample_rate: None,
            file_size: 0,
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

    #[test]
    fn albums_are_counted_smallest_first() {
        let tracks = vec![
            track("autechre", "autechre", "amber"),
            track("autechre", "autechre", "amber"),
            track("autechre", "autechre", "amber"),
            track("aphex twin", "aphex twin", "drukqs"),
            track("autechre", "autechre", "tri repetae"),
            track("autechre", "autechre", "tri repetae"),
        ];

        let albums = albums_of(&tracks);

        assert_eq!(
            albums,
            vec![
                ("aphex twin — drukqs".to_string(), 1),
                ("autechre — tri repetae".to_string(), 2),
                ("autechre — amber".to_string(), 3),
            ]
        );
    }

    #[test]
    fn a_compilation_groups_under_its_album_artist() {
        let tracks: Vec<Track> = ["a", "b", "c", "d", "e"]
            .iter()
            .map(|name| track("various artists", name, "nuggets"))
            .collect();

        assert_eq!(
            albums_of(&tracks),
            vec![("various artists — nuggets".to_string(), 5)]
        );
    }

    #[test]
    fn a_missing_album_artist_falls_back_to_the_artist() {
        assert_eq!(
            albums_of(&[track("", "alda", "tahoma")]),
            vec![("alda — tahoma".to_string(), 1)]
        );
    }
}
