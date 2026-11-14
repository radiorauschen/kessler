use std::io::Write as _;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Remembered {
    pub paths: Vec<PathBuf>,
    pub at: Option<usize>,
}

fn state_line(at: Option<usize>) -> String {
    at.map(|n| n.to_string()).unwrap_or_else(|| "-".into())
}

pub fn save(path: &Path, queue: &Remembered) {
    if queue.paths.is_empty() {
        let _ = std::fs::remove_file(path);
        return;
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let Ok(file) = std::fs::File::create(path) else {
        return;
    };
    let mut out = std::io::BufWriter::new(file);
    let _ = writeln!(out, "{}", state_line(queue.at));
    for p in &queue.paths {
        let text = p.to_string_lossy();
        if text.contains('\n') {
            continue;
        }
        let _ = writeln!(out, "{text}");
    }
    let _ = out.flush();
}

pub fn load(path: &Path) -> Remembered {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Remembered::default();
    };
    read(&text)
}

fn read(text: &str) -> Remembered {
    let mut lines = text.lines();
    let Some(state) = lines.next() else {
        return Remembered::default();
    };
    let mut fields = state.split('\t');
    let at = fields
        .next()
        .and_then(|f| f.trim().parse::<usize>().ok());

    let paths: Vec<PathBuf> = lines
        .filter(|line| !line.trim().is_empty())
        .map(PathBuf::from)
        .collect();

    let at = at.filter(|n| *n < paths.len());
    Remembered { paths, at }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(queue: &Remembered) -> Remembered {
        let dir = std::env::temp_dir().join("kessler-remembered");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!("{}.txt", queue.paths.len()));
        save(&path, queue);
        let back = load(&path);
        let _ = std::fs::remove_file(&path);
        back
    }

    #[test]
    fn it_comes_back_as_it_was_left() {
        let queue = Remembered {
            paths: vec!["/m/a.mp3".into(), "/m/b.mp3".into(), "/m/c.mp3".into()],
            at: Some(1),
        };
        assert_eq!(roundtrip(&queue), queue);
    }

    #[test]
    fn a_queue_with_nothing_playing_says_so() {
        let queue = Remembered {
            paths: vec!["/m/a.mp3".into()],
            at: None,
        };
        assert_eq!(roundtrip(&queue), queue);
    }

    #[test]
    fn an_empty_queue_leaves_nothing() {
        let dir = std::env::temp_dir().join("kessler-remembered");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("empty.txt");
        save(
            &path,
            &Remembered {
                paths: vec!["/m/a.mp3".into()],
                at: Some(0),
                },
        );
        assert!(path.exists());
        save(&path, &Remembered::default());
        assert!(!path.exists());
        assert_eq!(load(&path), Remembered::default());
    }

    #[test]
    fn nonsense_in_the_file_is_simply_no_queue() {
        assert_eq!(read(""), Remembered::default());
        assert_eq!(read("not a number\n"), Remembered::default());
        let back = read("9\thand\n/m/a.mp3\n");
        assert_eq!(back.at, None);
        assert_eq!(back.paths.len(), 1);
    }

    #[test]
    fn blank_lines_are_not_tracks() {
        let back = read("0\tliste\n/m/a.mp3\n\n/m/b.mp3\n");
        assert_eq!(back.paths.len(), 2);
    }
}
