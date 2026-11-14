use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

pub static AUTO: AtomicBool = AtomicBool::new(true);

static LET_GO: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

const BY_ID: &str = "/dev/disk/by-id";

fn is_ipod(name: &str) -> bool {
    name.starts_with("usb-Apple_iPod")
}

pub fn unmounted() -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(BY_ID) else {
        return Vec::new();
    };
    let names: Vec<String> = entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| is_ipod(n))
        .collect();
    let mounted = mounted_devices();
    let let_go: Vec<PathBuf> = match LET_GO.lock() {
        Ok(mut let_go) => {
            let_go.retain(|device| device.exists());
            let_go.clone()
        }
        Err(_) => Vec::new(),
    };
    let mut found: Vec<PathBuf> = candidates(&names)
        .into_iter()
        .filter_map(|name| std::fs::canonicalize(Path::new(BY_ID).join(name)).ok())
        .filter(|device| !mounted.iter().any(|m| m == device))
        .filter(|device| !let_go.contains(device))
        .filter(|device| readable(device))
        .collect();
    found.sort();
    found.dedup();
    found
}

fn candidates(names: &[String]) -> Vec<&str> {
    names
        .iter()
        .filter(|name| {
            name.contains("-part")
                || !names.iter().any(|other| other.starts_with(name.as_str()) && other.contains("-part"))
        })
        .map(String::as_str)
        .collect()
}

fn readable(device: &Path) -> bool {
    let Ok(out) = Command::new("lsblk").args(["-no", "FSTYPE"]).arg(device).output() else {
        return false;
    };
    let kind = String::from_utf8_lossy(&out.stdout).trim().to_ascii_lowercase();
    matches!(kind.as_str(), "vfat" | "fat" | "fat32" | "hfsplus" | "hfs")
}

fn mounted_devices() -> Vec<PathBuf> {
    let Ok(mounts) = std::fs::read_to_string("/proc/mounts") else {
        return Vec::new();
    };
    mounts
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .filter(|device| device.starts_with("/dev/"))
        .map(|device| std::fs::canonicalize(device).unwrap_or_else(|_| PathBuf::from(device)))
        .collect()
}

pub fn device_of(mount_point: &Path) -> Option<PathBuf> {
    let mounts = std::fs::read_to_string("/proc/mounts").ok()?;
    device_in(&mounts, mount_point)
}

fn device_in(mounts: &str, mount_point: &Path) -> Option<PathBuf> {
    mounts.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let device = fields.next()?;
        let at = fields.next()?.replace("\\040", " ").replace("\\011", "\t").replace("\\134", "\\");
        (Path::new(&at) == mount_point).then(|| PathBuf::from(device))
    })
}

pub fn available() -> bool {
    Command::new("udisksctl")
        .arg("help")
        .output()
        .is_ok_and(|out| out.status.success())
}

pub fn mount(device: &Path) -> Result<PathBuf, String> {
    let out = run(&["mount", "--no-user-interaction", "-b"], device)?;
    said_mounted_at(&out).ok_or(out)
}

fn said_mounted_at(said: &str) -> Option<PathBuf> {
    let at = said.find(" at ")? + " at ".len();
    let path = said[at..].trim().trim_end_matches('.');
    (!path.is_empty()).then(|| PathBuf::from(path))
}

pub fn eject(device: &Path) -> Result<(), String> {
    run(&["unmount", "--no-user-interaction", "-b"], device)?;
    if let Ok(mut let_go) = LET_GO.lock() {
        let device = std::fs::canonicalize(device).unwrap_or_else(|_| device.to_path_buf());
        let_go.push(device);
    }
    let _ = run(&["power-off", "--no-user-interaction", "-b"], device);
    Ok(())
}

fn run(args: &[&str], device: &Path) -> Result<String, String> {
    let out = Command::new("udisksctl")
        .args(args)
        .arg(device)
        .output()
        .map_err(|e| format!("udisksctl: {e}"))?;
    let said = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if out.status.success() {
        return Ok(said);
    }
    let error = String::from_utf8_lossy(&out.stderr);
    Err(error
        .lines()
        .rfind(|l| !l.trim().is_empty())
        .unwrap_or("udisksctl")
        .trim()
        .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_partitions_or_a_bare_disk_are_mounted() {
        let names: Vec<String> = [
            "usb-Apple_iPod_000A27001C3B2F18-0:0",
            "usb-Apple_iPod_000A27001C3B2F18-0:0-part1",
            "usb-Apple_iPod_000A27001C3B2F18-0:0-part2",
            "usb-Apple_iPod_shuffle_8K0331-0:0",
        ]
        .map(String::from)
        .to_vec();
        assert_eq!(
            candidates(&names),
            vec![
                "usb-Apple_iPod_000A27001C3B2F18-0:0-part1",
                "usb-Apple_iPod_000A27001C3B2F18-0:0-part2",
                "usb-Apple_iPod_shuffle_8K0331-0:0",
            ]
        );
        assert!(is_ipod("usb-Apple_iPod_x"));
        assert!(!is_ipod("usb-SanDisk_Cruzer-0:0"));
    }

    #[test]
    fn a_mount_is_read_back() {
        let mounts = "/dev/sda1 / ext4 rw 0 0\n/dev/sdb2 /run/media/user/MY\\040IPOD vfat rw 0 0\n";
        assert_eq!(
            device_in(mounts, Path::new("/run/media/user/MY IPOD")),
            Some(PathBuf::from("/dev/sdb2"))
        );
        assert_eq!(device_in(mounts, Path::new("/nowhere")), None);
        assert_eq!(
            said_mounted_at("Mounted /dev/sdb2 at /run/media/user/IPOD"),
            Some(PathBuf::from("/run/media/user/IPOD"))
        );
        assert_eq!(said_mounted_at("nonsense"), None);
    }
}
