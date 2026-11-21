use std::ops::Range;

#[derive(Debug, Clone, PartialEq)]
pub enum Mark {
    At(f64),
    Link(String),
}

const ENDINGS: &[&str] = &[
    "com", "org", "net", "de", "at", "ch", "fm", "io", "co", "uk", "eu", "info", "me", "tv", "app", "dev", "news", "radio",
    "show", "audio", "link", "ly", "gl", "be", "nl", "fr", "it", "es", "jp", "us", "ca", "au", "social", "online", "blog",
];

pub fn marks_in(text: &str) -> Vec<(Range<usize>, Mark)> {
    let mut out = Vec::new();
    for (start, word) in words(text) {
        let trimmed = word.trim_start_matches(['(', '[', '„', '"', '\'', '<']);
        let lead = word.len() - trimmed.len();
        let trimmed = trimmed.trim_end_matches(['.', ',', ';', ':', '!', '?', ')', ']', '“', '"', '\'', '>', '…']);
        if trimmed.is_empty() {
            continue;
        }
        let at = start + lead;
        let range = at..at + trimmed.len();
        if let Some(secs) = time(trimmed) {
            out.push((range, Mark::At(secs)));
        } else if let Some(url) = link(trimmed) {
            out.push((range, Mark::Link(url)));
        }
    }
    out
}

fn words(text: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut at = 0;
    text.split_inclusive(char::is_whitespace).filter_map(move |piece| {
        let start = at;
        at += piece.len();
        let word = piece.trim_end();
        (!word.is_empty()).then_some((start, word))
    })
}

fn time(word: &str) -> Option<f64> {
    let parts: Vec<&str> = word.split(':').collect();
    if !(2..=3).contains(&parts.len()) || parts.iter().any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit())) {
        return None;
    }
    if parts[1..].iter().any(|p| p.len() != 2) || parts[0].len() > 2 {
        return None;
    }
    let n: Vec<u64> = parts.iter().map(|p| p.parse().unwrap_or(0)).collect();
    if n[1..].iter().any(|&v| v >= 60) {
        return None;
    }
    let secs = n.iter().fold(0, |sum, &v| sum * 60 + v);
    Some(secs as f64)
}

fn link(word: &str) -> Option<String> {
    let lower = word.to_lowercase();
    if lower.starts_with("https://") || lower.starts_with("http://") {
        return (word.len() > 8).then(|| word.to_string());
    }
    if let Some((name, host)) = word.split_once('@') {
        return (!name.is_empty() && is_host(host)).then(|| format!("mailto:{word}"));
    }
    let host = word.split(['/', '?', '#']).next().unwrap_or(word);
    is_host(host).then(|| format!("https://{word}"))
}

fn host_parts(host: &str) -> Option<Vec<&str>> {
    let parts: Vec<&str> = host.split('.').collect();
    let fine = parts.len() >= 2 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_alphanumeric() || c == '-'));
    fine.then_some(parts)
}

fn is_host(host: &str) -> bool {
    let Some(parts) = host_parts(host) else { return false };
    let last = parts.last().map(|p| p.to_lowercase()).unwrap_or_default();
    let named = parts[parts.len() - 2].chars().any(char::is_alphabetic);
    named && ENDINGS.contains(&last.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(text: &str) -> Vec<(&str, Mark)> {
        marks_in(text).into_iter().map(|(r, m)| (&text[r], m)).collect()
    }

    #[test]
    fn times_are_found_at_a_line_start_and_in_the_middle() {
        let text = "00:00:00 intro\n00:13:31 ilse aigner: „die junge\nab 1:02:15 und (bei 5:07).";
        assert_eq!(
            found(text),
            vec![("00:00:00", Mark::At(0.0)), ("00:13:31", Mark::At(811.0)), ("1:02:15", Mark::At(3735.0)), ("5:07", Mark::At(307.0))]
        );
    }

    #[test]
    fn what_only_looks_like_a_time_is_left() {
        assert!(found("um 12:75 uhr, 1:2:3, 123:45, 10:30:99").is_empty());
    }

    #[test]
    fn links_are_found_with_and_without_a_scheme() {
        let text = "on npr.org/machinegods. mail hey@machinegods.fm, or https://example.org/a?b=1 (www.spiegel.de)";
        assert_eq!(
            found(text),
            vec![
                ("npr.org/machinegods", Mark::Link("https://npr.org/machinegods".into())),
                ("hey@machinegods.fm", Mark::Link("mailto:hey@machinegods.fm".into())),
                ("https://example.org/a?b=1", Mark::Link("https://example.org/a?b=1".into())),
                ("www.spiegel.de", Mark::Link("https://www.spiegel.de".into())),
            ]
        );
    }

    #[test]
    fn abbreviations_and_versions_are_no_links() {
        assert!(found("z.b. ist 1.5.de oder u.s.a. kein link, auch nicht e.g.").is_empty());
    }
}
