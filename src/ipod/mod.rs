//! iPod support: finding a mounted device, identifying it, and reading or
//! writing its database.
//!
//! Writing is gated on [`device_info::DeviceInfo::writable`]. The iPod
//! classic and the nano of the third and fourth generation want their
//! database signed (`hash58`), with the device's own FireWire GUID; where that
//! is not to be found, or a later model wants a later scheme, they are
//! refused rather than broken — an unsigned database makes them show an empty
//! library.

pub mod device_info;
pub mod hash58;
pub mod itunesdb;
pub mod itunessd;
pub mod playcounts;
pub mod report;
pub mod sync;
#[cfg(target_os = "linux")]
pub mod udisks;

use std::path::{Path, PathBuf};

use crate::error::{KesslerError, Result};
use itunesdb::ItunesDb;

/// A mounted iPod.
#[derive(Debug, Clone)]
pub struct IpodDevice {
    /// The volume root, e.g. `/Volumes/IPOD`.
    pub mount_point: PathBuf,
    /// Volume name, as shown in the file manager.
    pub name: String,
}

impl IpodDevice {
    /// Treat `mount_point` as an iPod, failing if it does not look like one.
    pub fn at(mount_point: impl AsRef<Path>) -> Result<Self> {
        let mount_point = mount_point.as_ref().to_path_buf();

        if !mount_point.is_dir() {
            return Err(KesslerError::Ipod(format!(
                "{} is not a directory",
                mount_point.display()
            )));
        }

        if !mount_point.join("iPod_Control").is_dir() {
            return Err(KesslerError::Ipod(format!(
                "{} does not look like an iPod — no iPod_Control",
                mount_point.display()
            )));
        }

        let name = mount_point
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| mount_point.display().to_string());

        Ok(Self { mount_point, name })
    }

    /// Scan the usual mount locations for anything that looks like an iPod.
    pub fn detect() -> Vec<Self> {
        let mut found = Vec::new();

        for root in mount_roots() {
            let entries = match std::fs::read_dir(&root) {
                Ok(e) => e,
                Err(_) => continue,
            };
            for entry in entries.flatten() {
                if let Ok(device) = Self::at(entry.path()) {
                    found.push(device);
                }
            }
        }

        found.sort_by(|a, b| a.mount_point.cmp(&b.mount_point));
        found.dedup_by(|a, b| a.mount_point == b.mount_point);
        found
    }

    pub fn itunesdb_path(&self) -> PathBuf {
        Self::itunesdb_path_in(&self.mount_point)
    }

    fn itunesdb_path_in(mount_point: &Path) -> PathBuf {
        mount_point
            .join("iPod_Control")
            .join("iTunes")
            .join("iTunesDB")
    }

    /// Resolve a track's device-relative path against this mount point.
    pub fn resolve(&self, relative: &Path) -> PathBuf {
        self.mount_point.join(relative)
    }

    /// Whether the device has a database yet.
    pub fn has_database(&self) -> bool {
        self.itunesdb_path().is_file()
    }

    /// Parse the device's iTunesDB — or, on a device without one, an empty
    /// library under a fresh id, which the first sync writes out.
    pub fn read_database(&self) -> Result<ItunesDb> {
        if !self.has_database() {
            let now = itunesdb::writer::now_unix() as u64;
            return Ok(ItunesDb { db_id: now << 16 | 0x6b73, ..Default::default() });
        }
        itunesdb::read_from_file(&self.itunesdb_path())
    }

    /// `(total, available)` bytes on the volume.
    pub fn capacity(&self) -> Option<(u64, u64)> {
        let total = fs4::total_space(&self.mount_point).ok()?;
        let available = fs4::available_space(&self.mount_point).ok()?;
        Some((total, available))
    }

    /// Unmount the volume, so the device can be unplugged.
    ///
    /// Handed to the system's own tool rather than done here: unmounting is the
    /// file manager's business, it needs to flush buffers and tell every other
    /// program that the volume is going, and an iPod pulled out of a volume
    /// this program unmounted by itself is an iPod with a broken database.
    ///
    /// What comes back on failure is the tool's own last line — "Volume in use"
    /// is the useful message, and paraphrasing it would only lose it.
    pub fn eject(&self) -> Result<()> {
        let mount = self.mount_point.as_os_str();
        let (program, args): (&str, Vec<&std::ffi::OsStr>) = if cfg!(target_os = "macos") {
            ("diskutil", vec!["eject".as_ref(), mount])
        } else if cfg!(target_os = "windows") {
            return Err(KesslerError::Ipod(
                tr!("auswerfen geht hier nur über den explorer").into(),
            ));
        } else {
            #[cfg(target_os = "linux")]
            if udisks::available() {
                if let Some(device) = udisks::device_of(&self.mount_point) {
                    return udisks::eject(&device).map_err(KesslerError::Ipod);
                }
            }
            ("umount", vec![mount])
        };

        let output = std::process::Command::new(program)
            .args(&args)
            .output()
            .map_err(|e| KesslerError::Ipod(format!("{program}: {e}")))?;

        if output.status.success() {
            return Ok(());
        }
        let said = String::from_utf8_lossy(&output.stderr);
        let last = said
            .lines()
            .rfind(|l| !l.trim().is_empty())
            .unwrap_or(tr!("konnte nicht ausgeworfen werden"))
            .trim()
            .to_string();
        Err(KesslerError::Ipod(last))
    }
}

/// Directories that hold mounted volumes on each platform.
/// What is mounted, as names only — the entries of the places volumes are
/// mounted under, without looking inside any of them.
///
/// For the watcher, which asks every two seconds: looking *inside* a volume
/// for `iPod_Control` wakes a sleeping external disk, and asking that every
/// two seconds kept every disk on the desk spinning for as long as the window
/// was open. The names of what is mounted are the mount directory's own
/// business and cost nothing; only when they change is anything looked into.
pub fn mounted() -> Vec<PathBuf> {
    let mut names: Vec<PathBuf> = Vec::new();
    for root in mount_roots() {
        if cfg!(target_os = "windows") {
            if root.exists() {
                names.push(root);
            }
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(&root) {
            names.extend(entries.flatten().map(|e| e.path()));
        }
    }
    names.sort();
    names
}

fn mount_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();

    if cfg!(target_os = "macos") {
        roots.push(PathBuf::from("/Volumes"));
    } else if cfg!(target_os = "windows") {
        for letter in b'D'..=b'Z' {
            roots.push(PathBuf::from(format!("{}:\\", letter as char)));
        }
    } else {
        roots.push(PathBuf::from("/media"));
        roots.push(PathBuf::from("/mnt"));
        roots.push(PathBuf::from("/run/media"));
        if let Ok(user) = std::env::var("USER") {
            roots.push(PathBuf::from("/media").join(&user));
            roots.push(PathBuf::from("/run/media").join(&user));
        }
    }

    roots
}

/// Human-readable byte size, e.g. `1.4 GB`.
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_a_directory_that_is_not_an_ipod() {
        let dir = std::env::temp_dir();
        let err = IpodDevice::at(&dir).unwrap_err();
        assert!(err.to_string().contains("does not look like an iPod"));
    }

    #[test]
    fn rejects_a_missing_path() {
        assert!(IpodDevice::at("/definitely/not/here/at/all").is_err());
    }

    #[test]
    fn formats_byte_sizes() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1_572_864), "1.5 MB");
        assert_eq!(format_bytes(80 * 1024 * 1024 * 1024), "80.0 GB");
    }
}
