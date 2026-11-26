//! Asking the operating system for a folder.
//!
//! Deliberately no crate for this. The Rust file-dialog crates are fine on
//! macOS, where they wrap the one panel there is, but on Linux they pull in a
//! D-Bus and portal stack that would end up in the build for the sake of a
//! dialog this program opens perhaps twice in its life. Every platform already
//! ships a command that shows its own native picker, and shelling out to it
//! costs one process and no dependencies.
//!
//! The call blocks until the person answers, so it never runs on the thread
//! drawing the window — see `GuiApp::ask_for_folder`.

use std::path::PathBuf;
use std::process::Command;

/// What comes back from a picker that was cancelled, on each platform, so a
/// cancel reads as "no folder" rather than as an error.
fn parse_reply(status_ok: bool, stdout: &str) -> Option<PathBuf> {
    if !status_ok {
        return None;
    }
    let line = stdout.lines().next()?.trim();
    if line.is_empty() {
        return None;
    }
    let path = PathBuf::from(line);
    path.is_dir().then_some(path)
}

/// Show the platform's own folder picker. `None` means cancelled.
pub fn ask(prompt: &str) -> Option<PathBuf> {
    let output = if cfg!(target_os = "macos") {
        Command::new("osascript")
            .arg("-e")
            .arg(format!(
                "POSIX path of (choose folder with prompt \"{}\")",
                prompt.replace('"', "")
            ))
            .output()
    } else if cfg!(target_os = "windows") {
        Command::new("powershell")
            .args(["-NoProfile", "-Command"])
            .arg(
                "Add-Type -AssemblyName System.Windows.Forms; \
                 $d = New-Object System.Windows.Forms.FolderBrowserDialog; \
                 if ($d.ShowDialog() -eq 'OK') { $d.SelectedPath }",
            )
            .output()
    } else {
        let zenity = Command::new("zenity")
            .args(["--file-selection", "--directory", "--title", prompt])
            .output();
        match zenity {
            Ok(out) if out.status.success() => Ok(out),
            _ => Command::new("kdialog")
                .args(["--getexistingdirectory", "."])
                .output(),
        }
    };

    let output = output.ok()?;
    parse_reply(
        output.status.success(),
        &String::from_utf8_lossy(&output.stdout),
    )
}

/// The same, for one picture file. `None` means cancelled.
///
/// Separate from [`ask`] rather than a flag on it: every platform spells the
/// two panels differently, and a picker that can return either a folder or a
/// file would have to be checked by every caller anyway.
pub fn ask_image(prompt: &str) -> Option<PathBuf> {
    let output = if cfg!(target_os = "macos") {
        Command::new("osascript")
            .arg("-e")
            .arg(format!(
                "POSIX path of (choose file with prompt \"{}\" of type {{\"public.image\"}})",
                prompt.replace('"', "")
            ))
            .output()
    } else if cfg!(target_os = "windows") {
        Command::new("powershell")
            .args(["-NoProfile", "-Command"])
            .arg(format!(
                "Add-Type -AssemblyName System.Windows.Forms; \
                 $d = New-Object System.Windows.Forms.OpenFileDialog; \
                 $d.Filter = '{}|*.jpg;*.jpeg;*.png;*.gif;*.bmp'; \
                 if ($d.ShowDialog() -eq 'OK') {{ $d.FileName }}",
                tr!("Bilder")
            ))
            .output()
    } else {
        let zenity = Command::new("zenity")
            .args(["--file-selection", "--title", prompt])
            .output();
        match zenity {
            Ok(out) if out.status.success() => Ok(out),
            _ => Command::new("kdialog").args(["--getopenfilename", "."]).output(),
        }
    };

    let output = output.ok()?;
    parse_file_reply(
        output.status.success(),
        &String::from_utf8_lossy(&output.stdout),
    )
}

/// Ask for a list of stations, an `.m3u` or a `.pls`. `None` means
/// cancelled. Not filtered by kind: such lists come under many names.
pub fn ask_playlist(prompt: &str) -> Option<PathBuf> {
    let output = if cfg!(target_os = "macos") {
        Command::new("osascript")
            .arg("-e")
            .arg(format!("POSIX path of (choose file with prompt \"{}\")", prompt.replace('"', "")))
            .output()
    } else if cfg!(target_os = "windows") {
        Command::new("powershell")
            .args(["-NoProfile", "-Command"])
            .arg(
                "Add-Type -AssemblyName System.Windows.Forms; \
                 $d = New-Object System.Windows.Forms.OpenFileDialog; \
                 $d.Filter = 'm3u, pls|*.m3u;*.m3u8;*.pls|*|*.*'; \
                 if ($d.ShowDialog() -eq 'OK') { $d.FileName }",
            )
            .output()
    } else {
        let zenity = Command::new("zenity").args(["--file-selection", "--title", prompt]).output();
        match zenity {
            Ok(out) if out.status.success() => Ok(out),
            _ => Command::new("kdialog").args(["--getopenfilename", "."]).output(),
        }
    };
    let output = output.ok()?;
    parse_file_reply(output.status.success(), &String::from_utf8_lossy(&output.stdout))
}

/// Ask for one or more audio files. An empty list means cancelled.
///
/// Several at once, because music arrives by the album — and the panel is the
/// only way in for anyone who would rather not drag things about.
pub fn ask_audio() -> Vec<PathBuf> {
    let output = if cfg!(target_os = "macos") {
        Command::new("osascript")
            .arg("-e")
            .arg(format!(
                "set chosen to choose file with prompt \"{}\" \
                 of type {{\"public.audio\"}} with multiple selections allowed
\
                 set out to \"\"
\
                 repeat with one in chosen
\
                 set out to out & POSIX path of one & linefeed
\
                 end repeat
\
                 return out",
                tr!("Musik hinzufügen")
            ))
            .output()
    } else if cfg!(target_os = "windows") {
        Command::new("powershell")
            .args(["-NoProfile", "-Command"])
            .arg(format!(
                "Add-Type -AssemblyName System.Windows.Forms; \
                 $d = New-Object System.Windows.Forms.OpenFileDialog; \
                 $d.Multiselect = $true; \
                 $d.Filter = '{}|*.mp3;*.flac;*.ogg;*.oga;*.wav'; \
                 if ($d.ShowDialog() -eq 'OK') {{ $d.FileNames }}",
                tr!("Musik")
            ))
            .output()
    } else {
        let zenity = Command::new("zenity")
            .args([
                "--file-selection",
                "--multiple",
                "--separator",
                "\n",
                "--title",
                tr!("Musik hinzufügen"),
            ])
            .output();
        match zenity {
            Ok(out) if out.status.success() => Ok(out),
            _ => Command::new("kdialog")
                .args(["--getopenfilename", ".", "--multiple", "--separate-output"])
                .output(),
        }
    };

    let Ok(output) = output else {
        return Vec::new();
    };
    parse_files_reply(
        output.status.success(),
        &String::from_utf8_lossy(&output.stdout),
    )
}

fn parse_files_reply(status_ok: bool, stdout: &str) -> Vec<PathBuf> {
    if !status_ok {
        return Vec::new();
    }
    stdout
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .collect()
}

fn parse_file_reply(status_ok: bool, stdout: &str) -> Option<PathBuf> {
    if !status_ok {
        return None;
    }
    let line = stdout.lines().next()?.trim();
    if line.is_empty() {
        return None;
    }
    let path = PathBuf::from(line);
    path.is_file().then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Several files come back as several lines, and anything that is not a
    /// file is not one.
    #[test]
    fn a_list_of_files_comes_back_as_a_list() {
        let dir = std::env::temp_dir().join("kessler-files-test");
        std::fs::create_dir_all(&dir).unwrap();
        let one = dir.join("one.mp3");
        let two = dir.join("two.mp3");
        std::fs::write(&one, b"x").unwrap();
        std::fs::write(&two, b"x").unwrap();

        let text = format!("{}\n{}\n", one.display(), two.display());
        assert_eq!(parse_files_reply(true, &text), vec![one.clone(), two]);

        assert!(parse_files_reply(false, &text).is_empty());
        assert!(parse_files_reply(true, "").is_empty());
        let text = format!("{}\n{}\n{}\n", one.display(), dir.display(), "/nope.mp3");
        assert_eq!(parse_files_reply(true, &text), vec![one]);
    }

    /// The file picker's answer is checked the same way, the other way round.
    #[test]
    fn a_chosen_file_is_a_file_and_not_a_folder() {
        let dir = std::env::temp_dir().join("kessler-file-test");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("cover.jpg");
        std::fs::write(&file, b"x").unwrap();

        assert_eq!(parse_file_reply(true, &format!("{}\n", file.display())), Some(file));
        assert_eq!(parse_file_reply(true, &format!("{}\n", dir.display())), None);
        assert_eq!(parse_file_reply(false, "/tmp/x"), None);
        assert_eq!(parse_file_reply(true, "abgebrochen"), None);
    }

    #[test]
    fn a_cancelled_picker_is_not_a_folder() {
        assert_eq!(parse_reply(false, "/tmp\n"), None);
        assert_eq!(parse_reply(true, ""), None);
        assert_eq!(parse_reply(true, "\n"), None);
        assert_eq!(parse_reply(true, "   \n"), None);
    }

    #[test]
    fn a_chosen_folder_comes_back_trimmed() {
        let dir = std::env::temp_dir().join("kessler-folder-test");
        std::fs::create_dir_all(&dir).unwrap();
        let text = format!("{}\n", dir.display());
        assert_eq!(parse_reply(true, &text), Some(dir.clone()));
        assert_eq!(parse_reply(true, &format!("  {}  \n", dir.display())), Some(dir));
    }

    /// A helper that is missing prints its complaint somewhere, and that must
    /// never be taken for a path.
    #[test]
    fn a_message_is_not_a_folder() {
        assert_eq!(parse_reply(true, "command not found: zenity"), None);
        assert_eq!(parse_reply(true, "/this/does/not/exist"), None);
    }

    /// A file is not a folder, however it arrived.
    #[test]
    fn a_file_is_refused() {
        let file = std::env::temp_dir().join("kessler-folder-test-file");
        std::fs::write(&file, b"x").unwrap();
        assert_eq!(parse_reply(true, &format!("{}\n", file.display())), None);
        let _ = std::fs::remove_file(&file);
    }
}
