use std::path::Path;
use std::process::{Command, Stdio};

pub const WORD: &str = if cfg!(target_os = "macos") {
    "im finder zeigen"
} else if cfg!(windows) {
    "im explorer zeigen"
} else {
    "im dateimanager zeigen"
};

pub fn show(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err(tr!("{} ist nicht mehr da", path.display()));
    }
    ask(path)
}

#[cfg(target_os = "macos")]
fn ask(path: &Path) -> Result<(), String> {
    started(Command::new("open").arg("-R").arg(path))
}

#[cfg(windows)]
fn ask(path: &Path) -> Result<(), String> {
    let mut select = std::ffi::OsString::from("/select,");
    select.push(path.as_os_str());
    started(Command::new("explorer").arg(select))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn ask(path: &Path) -> Result<(), String> {
    let path = path.to_path_buf();
    std::thread::spawn(move || {
        let asked = Command::new("dbus-send")
            .args([
                "--session",
                "--print-reply",
                "--reply-timeout=3000",
                "--dest=org.freedesktop.FileManager1",
                "--type=method_call",
                "/org/freedesktop/FileManager1",
                "org.freedesktop.FileManager1.ShowItems",
            ])
            .arg(format!("array:string:{}", file_uri(&path)))
            .arg("string:")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !asked {
            if let Some(folder) = path.parent() {
                let _ = Command::new("xdg-open")
                    .arg(folder)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
        }
    });
    Ok(())
}

#[cfg(any(target_os = "macos", windows))]
fn started(command: &mut Command) -> Result<(), String> {
    command
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|mut child| {
            let _ = std::thread::Builder::new().name("kessler-reveal".into()).spawn(move || {
                let _ = child.wait();
            });
        })
        .map_err(|e| tr!("der dateimanager liess sich nicht öffnen: {}", e))
}

#[cfg_attr(any(target_os = "macos", windows), allow(dead_code))]
fn file_uri(path: &Path) -> String {
    let mut out = String::from("file://");
    for byte in path.to_string_lossy().bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                out.push(byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_carries_every_name_whole() {
        assert_eq!(
            file_uri(Path::new("/musik/Amon Düül II/01 #1.mp3")),
            "file:///musik/Amon%20D%C3%BC%C3%BCl%20II/01%20%231.mp3"
        );
        assert_eq!(file_uri(Path::new("/a/b_c-d.e~f")), "file:///a/b_c-d.e~f");
    }

    #[test]
    fn a_file_that_is_gone_is_said_so() {
        let said = show(Path::new("/nirgends/gibt/es/diese/datei.mp3")).unwrap_err();
        assert!(said.contains("nicht mehr da"), "{said}");
    }
}
