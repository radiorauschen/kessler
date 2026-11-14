use std::collections::{HashMap, HashSet};
use std::path::Path;

use super::artistinfo::{self, plain};

#[derive(Debug, Clone, PartialEq)]
pub struct Gap {
    pub name: String,
    pub by: Vec<(String, Option<f32>)>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Gaps {
    pub gaps: Vec<Gap>,
    pub artists: usize,
    pub known: usize,
    pub unmeasured: usize,
}

pub type Named = (String, Vec<(String, Option<f32>)>);

pub const AT_LEAST: usize = 2;

pub const KEPT: usize = 80;

pub fn find(dir: &Path, artists: &[String]) -> Gaps {
    let mut lists = Vec::new();
    let mut unmeasured = 0;
    for name in artists {
        let key = artistinfo::key(&crate::db::sort_key(name));
        let Some(info) = artistinfo::load(dir, key) else { continue };
        let named: Vec<(String, Option<f32>)> = if !info.found.kin.is_empty() {
            info.found.kin.into_iter().map(|k| (k.name, Some(k.weight))).collect()
        } else if !info.found.similar.is_empty() {
            unmeasured += 1;
            info.found.similar.into_iter().map(|n| (n, None)).collect()
        } else {
            continue;
        };
        lists.push((name.clone(), named));
    }
    Gaps {
        artists: artists.len(),
        known: lists.len(),
        unmeasured,
        gaps: tally(&lists, artists),
    }
}

pub fn tally(lists: &[Named], artists: &[String]) -> Vec<Gap> {
    let here: HashSet<String> = artists.iter().map(|a| plain(a)).collect();
    let mut found: HashMap<String, Gap> = HashMap::new();
    for (who, named) in lists {
        let mut once = HashSet::new();
        for (name, weight) in named {
            let key = plain(name);
            if key.is_empty() || here.contains(&key) || !once.insert(key.clone()) {
                continue;
            }
            found
                .entry(key)
                .or_insert_with(|| Gap { name: name.trim().to_string(), by: Vec::new() })
                .by
                .push((who.clone(), *weight));
        }
    }
    let likeness = |g: &Gap| g.by.iter().map(|(_, w)| w.unwrap_or(0.0)).sum::<f32>();
    let mut gaps: Vec<Gap> = found.into_values().filter(|g| g.by.len() >= AT_LEAST).collect();
    for gap in &mut gaps {
        gap.by.sort_by(|a, b| b.1.unwrap_or(0.0).total_cmp(&a.1.unwrap_or(0.0)).then_with(|| a.0.cmp(&b.0)));
    }
    gaps.sort_by(|a, b| {
        b.by.len()
            .cmp(&a.by.len())
            .then_with(|| likeness(b).total_cmp(&likeness(a)))
            .then_with(|| plain(&a.name).cmp(&plain(&b.name)))
    });
    gaps.truncate(KEPT);
    gaps
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(who: &str, named: &[(&str, f32)]) -> Named {
        (who.into(), named.iter().map(|(n, w)| (n.to_string(), Some(*w))).collect())
    }

    #[test]
    fn the_most_named_who_are_missing_come_first() {
        let artists: Vec<String> = ["My Bloody Valentine", "Ride", "Lush", "Low"].map(String::from).to_vec();
        let lists = vec![
            list("My Bloody Valentine", &[("Slowdive", 0.92), ("Ride", 0.85), ("Pale Saints", 0.58)]),
            list("Ride", &[("Slowdive", 0.90), ("Swervedriver", 0.84), ("Pale Saints", 0.60)]),
            list("Lush", &[("slowdive", 0.82), ("Pale Saints", 0.78)]),
            list("Low", &[("Codeine", 0.74), ("Swervedriver", 0.1)]),
        ];
        let gaps = tally(&lists, &artists);
        let names: Vec<&str> = gaps.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, vec!["Slowdive", "Pale Saints", "Swervedriver"]);
        assert_eq!(
            gaps[0].by,
            vec![
                ("My Bloody Valentine".to_string(), Some(0.92)),
                ("Ride".to_string(), Some(0.90)),
                ("Lush".to_string(), Some(0.82)),
            ]
        );
    }

    #[test]
    fn names_are_compared_plainly_and_counted_once() {
        let artists: Vec<String> = ["The Cure", "A", "B"].map(String::from).to_vec();
        let lists = vec![
            list("A", &[("Cure", 0.9), ("X", 0.5), ("x", 0.4)]),
            list("B", &[("cure", 0.9)]),
        ];
        assert!(tally(&lists, &artists).is_empty());
    }

    #[test]
    fn names_without_a_measure_still_count() {
        let artists: Vec<String> = ["A", "B"].map(String::from).to_vec();
        let lists = vec![
            ("A".to_string(), vec![("X".to_string(), None)]),
            list("B", &[("X", 0.3)]),
        ];
        let gaps = tally(&lists, &artists);
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].by, vec![("B".to_string(), Some(0.3)), ("A".to_string(), None)]);
    }

    #[test]
    fn they_are_read_from_what_is_kept_on_disk() {
        let dir = std::env::temp_dir().join(format!("kessler-gaps-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let keep = |name: &str, kin: &[&str], similar: &[&str]| {
            let mut info = artistinfo::ArtistInfo { name: name.into(), ..Default::default() };
            info.found.kin = kin
                .iter()
                .map(|n| artistinfo::Kin { name: n.to_string(), weight: 0.5 })
                .collect();
            info.found.similar = similar.iter().map(|n| n.to_string()).collect();
            let key = artistinfo::key(&crate::db::sort_key(name));
            artistinfo::save(&dir, key, &info).unwrap();
        };
        keep("A", &["X", "Y"], &[]);
        keep("B", &[], &["X"]);
        let artists: Vec<String> = ["A", "B", "C"].map(String::from).to_vec();
        let gaps = find(&dir, &artists);
        assert_eq!((gaps.artists, gaps.known, gaps.unmeasured), (3, 2, 1));
        assert_eq!(gaps.gaps.len(), 1);
        assert_eq!(gaps.gaps[0].name, "X");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
