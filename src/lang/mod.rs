use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

mod en;
mod ja;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    De,
    En,
    Ja,
}

impl Lang {
    pub const ALL: [Lang; 3] = [Lang::De, Lang::En, Lang::Ja];

    pub fn key(self) -> &'static str {
        match self {
            Lang::De => "de",
            Lang::En => "en",
            Lang::Ja => "ja",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Lang::De => "deutsch",
            Lang::En => "english",
            Lang::Ja => "日本語",
        }
    }

    pub fn of_key(key: &str) -> Option<Lang> {
        Lang::ALL.into_iter().find(|l| l.key() == key.trim())
    }

    pub fn of_system() -> Lang {
        Self::of_locale(sys_locale::get_locale().as_deref().unwrap_or(""))
    }

    fn of_locale(locale: &str) -> Lang {
        let lower = locale.to_ascii_lowercase();
        let is = |code: &str| lower == code || lower.starts_with(&format!("{code}-")) || lower.starts_with(&format!("{code}_"));
        if is("de") {
            Lang::De
        } else if is("ja") {
            Lang::Ja
        } else {
            Lang::En
        }
    }

    pub fn chosen(setting: &str) -> Lang {
        Self::of_key(setting).unwrap_or_else(Self::of_system)
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set(lang: Lang) {
    CURRENT.store(lang as u8, Ordering::Relaxed);
}

pub fn current() -> Lang {
    match CURRENT.load(Ordering::Relaxed) {
        1 => Lang::En,
        2 => Lang::Ja,
        _ => Lang::De,
    }
}

pub fn said<'a>(name: &str, shown: &'a str) -> &'a str {
    if name.is_empty() { t(shown) } else { shown }
}

pub fn t(de: &str) -> &str {
    in_lang(current(), de)
}

pub fn in_lang(lang: Lang, de: &str) -> &str {
    match lang {
        Lang::De => shown(de),
        Lang::En => english(de).unwrap_or_else(|| shown(de)),
        Lang::Ja => japanese(de).or_else(|| english(de)).unwrap_or_else(|| shown(de)),
    }
}

pub fn app_name() -> &'static str {
    match current() {
        Lang::Ja => "ケスラー",
        _ => "kessler",
    }
}

fn shown(de: &str) -> &str {
    de.split('|').next().unwrap_or(de)
}

fn english(de: &str) -> Option<&'static str> {
    static TABLE: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    TABLE
        .get_or_init(|| en::ALL.iter().flat_map(|part| part.iter().copied()).collect())
        .get(de)
        .copied()
}

fn japanese(de: &str) -> Option<&'static str> {
    static TABLE: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    TABLE
        .get_or_init(|| ja::ALL.iter().flat_map(|part| part.iter().copied()).collect())
        .get(de)
        .copied()
}

pub fn fill(template: &str, args: &[String]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut args = args.iter();
    let mut rest = template;
    while let Some(at) = rest.find("{}") {
        out.push_str(&rest[..at]);
        match args.next() {
            Some(arg) => out.push_str(arg),
            None => out.push_str("{}"),
        }
        rest = &rest[at + 2..];
    }
    out.push_str(rest);
    out
}

macro_rules! tr {
    ($de:literal) => {
        $crate::lang::t($de)
    };
    ($de:literal, $($arg:expr),+ $(,)?) => {
        $crate::lang::fill($crate::lang::t($de), &[$(format!("{}", $arg)),+])
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_missing_name_is_said_in_the_language() {
        assert_eq!(in_lang(Lang::En, "unbekannt"), "unknown");
        assert_eq!(said("nichts", "nichts"), "nichts");
        assert_eq!(said("björk", "björk"), "björk");
    }

    fn sentences_in_source() -> Vec<(String, String)> {
        let mut found = Vec::new();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        for entry in walkdir::WalkDir::new(&root) {
            let entry = entry.unwrap();
            if entry.path().extension().and_then(|e| e.to_str()) != Some("rs")
                || entry.path().starts_with(root.join("lang"))
            {
                continue;
            }
            let source = std::fs::read_to_string(entry.path()).unwrap();
            let name = entry.path().strip_prefix(&root).unwrap().display().to_string();
            let mut rest = source.as_str();
            while let Some(at) = rest.find("tr!(\"") {
                let before = rest[..at].chars().next_back();
                rest = &rest[at + 5..];
                if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                let (literal, after) = read_literal(rest);
                found.push((name.clone(), literal));
                rest = after;
            }
        }
        found
    }

    fn read_literal(from: &str) -> (String, &str) {
        let mut out = String::new();
        let mut chars = from.char_indices();
        while let Some((i, c)) = chars.next() {
            match c {
                '"' => return (out, &from[i + 1..]),
                '\\' => match chars.next() {
                    Some((_, 'n')) => out.push('\n'),
                    Some((_, 't')) => out.push('\t'),
                    Some((_, other)) => out.push(other),
                    None => {}
                },
                other => out.push(other),
            }
        }
        (out, "")
    }

    fn holes(s: &str) -> usize {
        s.matches("{}").count()
    }

    #[test]
    fn every_sentence_has_its_english() {
        let missing: Vec<String> = sentences_in_source()
            .into_iter()
            .filter(|(_, de)| english(de).is_none())
            .map(|(file, de)| format!("{file}: {de:?}"))
            .collect();
        assert!(missing.is_empty(), "no english for:\n{}", missing.join("\n"));
    }

    #[test]
    fn every_sentence_has_its_japanese() {
        let missing: Vec<String> = sentences_in_source()
            .into_iter()
            .filter(|(_, de)| japanese(de).is_none())
            .map(|(file, de)| format!("{file}: {de:?}"))
            .collect();
        assert!(missing.is_empty(), "no japanese for:\n{}", missing.join("\n"));
    }

    #[test]
    fn the_japanese_has_the_same_words_as_the_english() {
        let en: std::collections::BTreeSet<&str> = en::ALL.iter().flat_map(|p| p.iter()).map(|(de, _)| *de).collect();
        let ja: std::collections::BTreeSet<&str> = ja::ALL.iter().flat_map(|p| p.iter()).map(|(de, _)| *de).collect();
        assert_eq!(en, ja);
    }

    #[test]
    fn the_japanese_has_as_many_holes_and_one_answer() {
        let mut seen: HashMap<&str, &str> = HashMap::new();
        for part in ja::ALL {
            for (de, ja) in *part {
                assert_eq!(holes(de), holes(ja), "{de:?} → {ja:?}");
                if let Some(before) = seen.insert(de, ja) {
                    assert_eq!(before, *ja, "{de:?} is given twice");
                }
            }
        }
    }

    #[test]
    fn the_english_has_as_many_holes_as_the_german() {
        for part in en::ALL {
            for (de, en) in *part {
                assert_eq!(holes(de), holes(en), "{de:?} → {en:?}");
            }
        }
    }

    #[test]
    fn no_german_is_given_two_englishes() {
        let mut seen: HashMap<&str, &str> = HashMap::new();
        for part in en::ALL {
            for (de, en) in *part {
                if let Some(before) = seen.insert(de, en) {
                    assert_eq!(before, *en, "{de:?} is given twice");
                }
            }
        }
    }

    #[test]
    fn every_english_is_still_wanted() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut source = String::new();
        for entry in walkdir::WalkDir::new(&root) {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs")
                || path.starts_with(root.join("lang"))
            {
                continue;
            }
            source.push_str(&std::fs::read_to_string(path).unwrap());
        }
        let unused: Vec<&str> = en::ALL
            .iter()
            .flat_map(|part| part.iter())
            .map(|(de, _)| *de)
            .filter(|de| {
                let literal = format!(
                    "\"{}\"",
                    de.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
                );
                !source.contains(&literal)
            })
            .collect();
        assert!(unused.is_empty(), "english nobody asks for:\n{}", unused.join("\n"));
    }

    #[test]
    fn a_sentence_is_filled_in_turn() {
        assert_eq!(fill("noch {}", &["12".into()]), "noch 12");
        assert_eq!(fill("{} von {} frei", &["2 gb".into(), "5 gb".into()]), "2 gb von 5 gb frei");
        assert_eq!(fill("{} und {}", &["a".into()]), "a und {}");
        assert_eq!(fill("ohne", &[]), "ohne");
    }

    #[test]
    fn german_is_itself_and_english_falls_back_to_it() {
        assert_eq!(in_lang(Lang::De, "warteschlange"), "warteschlange");
        assert_eq!(in_lang(Lang::En, "warteschlange"), "queue");
        assert_eq!(in_lang(Lang::En, "gibt es nicht"), "gibt es nicht");
        assert_eq!(in_lang(Lang::De, "titel|name"), "titel");
        assert_eq!(in_lang(Lang::En, "titel|name"), "title");
        assert_eq!(in_lang(Lang::En, "titel"), "tracks");
        assert_eq!(in_lang(Lang::En, "gibt es|nicht"), "gibt es");
    }

    #[test]
    fn a_german_locale_is_german_and_the_rest_english() {
        for de in ["de", "de-DE", "de-AT", "de_CH", "DE-de"] {
            assert_eq!(Lang::of_locale(de), Lang::De, "{de}");
        }
        for en in ["en-US", "en-GB", "fr-FR", "", "dex"] {
            assert_eq!(Lang::of_locale(en), Lang::En, "{en}");
        }
        for ja in ["ja", "ja-JP", "ja_JP"] {
            assert_eq!(Lang::of_locale(ja), Lang::Ja, "{ja}");
        }
        assert_eq!(in_lang(Lang::Ja, "warteschlange"), "キュー");
        assert_eq!(Lang::chosen("de"), Lang::De);
        assert_eq!(Lang::chosen("en"), Lang::En);
    }
}
