//! The list of changes, as the menu shows it.
//!
//! `CHANGELOG.md` is read at build time rather than from disk, which settles
//! two questions at once: where the file would have to live beside a
//! double-clickable application, and whether what it says matches the program
//! showing it. Compiled in, it cannot drift and cannot go missing.
//!
//! The format is the one the file already has: `## ` begins a version, `- `
//! begins a change, everything else is prose for whoever opens the file and
//! is skipped here. Deliberately forgiving — a changelog that has to parse
//! before the program will start would be a poor trade.

/// One released version and what went into it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub changes: Vec<String>,
}

/// The file itself, as it stood when this was built.
const CHANGELOG: &str = include_str!("../../CHANGELOG.md");

/// Every release in the file, newest first — which is the order it is written
/// in, so no sorting is done: a version number is not always something you can
/// compare, and the person writing the file knows the order they meant.
pub fn releases() -> Vec<Release> {
    read(CHANGELOG)
}

fn read(text: &str) -> Vec<Release> {
    let mut out: Vec<Release> = Vec::new();

    for line in text.lines() {
        let line = line.trim();
        if let Some(version) = line.strip_prefix("## ") {
            out.push(Release {
                version: version.trim().to_string(),
                changes: Vec::new(),
            });
        } else if let Some(change) = line.strip_prefix("- ") {
            if let Some(release) = out.last_mut() {
                release.changes.push(plainly(change));
            }
        } else if !line.is_empty() {
            if let Some(change) = out.last_mut().and_then(|r| r.changes.last_mut()) {
                if !line.starts_with('#') {
                    change.push(' ');
                    change.push_str(&plainly(line));
                }
            }
        }
    }

    out
}

/// The file is markdown, the menu is not: `*so*` is emphasis to a reader of
/// the file and two stray asterisks to a reader of the window.
fn plainly(text: &str) -> String {
    text.trim().replace(['*', '`'], "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_versions_and_their_changes() {
        let text = "# Änderungen\n\nsome prose\n\n## 0.6.0\n\n- one thing\n- another\n\n## 0.5.0\n\n- older\n";
        assert_eq!(
            read(text),
            vec![
                Release {
                    version: "0.6.0".into(),
                    changes: vec!["one thing".into(), "another".into()],
                },
                Release {
                    version: "0.5.0".into(),
                    changes: vec!["older".into()],
                },
            ]
        );
    }

    /// The file is wrapped so it can be read as a file; an entry that runs
    /// over two lines is still one entry.
    #[test]
    fn a_wrapped_entry_is_one_entry() {
        let text = "## 0.1.0\n- a change that was too long\n  for one line\n- a short one\n";
        let releases = read(text);
        assert_eq!(
            releases[0].changes,
            vec![
                "a change that was too long for one line".to_string(),
                "a short one".to_string()
            ]
        );
    }

    #[test]
    fn prose_before_the_first_version_belongs_to_nobody() {
        let text = "# Änderungen\n\n- a stray line\n\n## 0.1.0\n- a real one\n";
        let releases = read(text);
        assert_eq!(releases.len(), 1);
        assert_eq!(releases[0].changes, vec!["a real one".to_string()]);
    }

    #[test]
    fn markdown_emphasis_does_not_reach_the_window() {
        let text = "## 0.1.0\n- lesen *und* schreiben, mit `code` darin\n";
        assert_eq!(
            read(text)[0].changes,
            vec!["lesen und schreiben, mit code darin".to_string()]
        );
    }

    #[test]
    fn nothing_at_all_is_an_empty_list() {
        assert!(read("").is_empty());
        assert!(read("just some words\n").is_empty());
    }

    /// The real file has to parse, and its newest version has to be the one
    /// the program says it is — a changelog whose top entry is not this build
    /// is worse than none.
    #[test]
    fn the_file_that_ships_says_what_this_build_is() {
        let releases = releases();
        assert!(!releases.is_empty(), "CHANGELOG.md has no versions in it");
        assert_eq!(
            releases[0].version,
            env!("CARGO_PKG_VERSION"),
            "the newest entry in CHANGELOG.md is not the version being built"
        );
        assert!(
            !releases[0].changes.is_empty(),
            "the newest version lists no changes"
        );
    }
}
