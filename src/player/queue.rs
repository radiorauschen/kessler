use rand::seq::SliceRandom;
use rand::thread_rng;

use crate::db::models::{RepeatMode, Track};

#[derive(Debug, Clone)]
pub struct PlayQueue {
    tracks: Vec<Track>,
    current_index: Option<usize>,
    history: Vec<usize>,
    shuffle: bool,
    shuffle_order: Vec<usize>,
    shuffle_pos: usize,
    pub repeat: RepeatMode,
    revision: u64,
}

impl PlayQueue {
    pub fn new() -> Self {
        Self {
            tracks: Vec::new(),
            current_index: None,
            history: Vec::new(),
            shuffle: false,
            shuffle_order: Vec::new(),
            shuffle_pos: 0,
            repeat: RepeatMode::Off,
            revision: 0,
        }
    }

    pub fn retarget(&mut self, id: i64, path: &std::path::Path) {
        let mut changed = false;
        for track in self.tracks.iter_mut().filter(|t| t.id == id) {
            track.path = path.to_path_buf();
            changed = true;
        }
        if changed {
            self.revision += 1;
        }
    }

    pub fn set_tracks(&mut self, tracks: Vec<Track>, start_index: usize) {
        self.revision += 1;
        self.tracks = tracks;
        self.history.clear();
        let idx = start_index.min(self.tracks.len().saturating_sub(1));
        self.current_index = if self.tracks.is_empty() {
            None
        } else {
            Some(idx)
        };
        if self.shuffle {
            self.regenerate_shuffle();
        }
    }

    pub fn insert_next(&mut self, tracks: Vec<Track>) {
        if tracks.is_empty() {
            return;
        }
        let at = match self.current_index {
            Some(ci) => (ci + 1).min(self.tracks.len()),
            None => 0,
        };
        self.tracks.splice(at..at, tracks);
        self.revision += 1;
        if self.current_index.is_none() {
            self.current_index = Some(0);
        }
        if self.shuffle {
            self.regenerate_shuffle();
        }
    }

    pub fn insert_at(&mut self, at: usize, tracks: Vec<Track>) {
        if tracks.is_empty() {
            return;
        }
        let at = at.min(self.tracks.len());
        let count = tracks.len();
        self.tracks.splice(at..at, tracks);
        self.revision += 1;
        match self.current_index {
            Some(ci) if ci >= at => self.current_index = Some(ci + count),
            None => self.current_index = Some(0),
            _ => {}
        }
        if self.shuffle {
            self.regenerate_shuffle();
        }
    }

    pub fn append(&mut self, tracks: Vec<Track>) {
        if tracks.is_empty() {
            return;
        }
        self.tracks.extend(tracks);
        self.revision += 1;
        if self.current_index.is_none() {
            self.current_index = Some(0);
        }
        if self.shuffle {
            self.regenerate_shuffle();
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn jump_to(&mut self, index: usize) {
        self.revision += 1;
        if index >= self.tracks.len() {
            return;
        }
        if let Some(ci) = self.current_index {
            self.history.push(ci);
        }
        self.current_index = Some(index);
        if self.shuffle {
            if let Some(pos) = self.shuffle_order.iter().position(|&i| i == index) {
                self.shuffle_order.swap(self.shuffle_pos, pos);
            }
        }
    }

    pub fn put_back(&mut self, before: PlayQueue) {
        let revision = self.revision.max(before.revision) + 1;
        *self = before;
        self.revision = revision;
    }

    pub fn move_row(&mut self, from: usize, to: usize) {
        if from >= self.tracks.len() || from == to {
            return;
        }
        let to = to.min(self.tracks.len() - 1);
        self.revision += 1;
        let playing = self.current_index;
        let track = self.tracks.remove(from);
        self.tracks.insert(to, track);
        if let Some(ci) = playing {
            self.current_index = Some(if ci == from {
                to
            } else {
                let mut at = ci;
                if from < at {
                    at -= 1;
                }
                if to <= at {
                    at += 1;
                }
                at
            });
        }
        if self.shuffle {
            self.regenerate_shuffle();
        }
    }

    pub fn remove(&mut self, index: usize) {
        if index >= self.tracks.len() {
            return;
        }
        self.tracks.remove(index);
        self.revision += 1;
        if let Some(ref mut ci) = self.current_index {
            if index < *ci {
                *ci -= 1;
            } else if index == *ci && *ci >= self.tracks.len() {
                *ci = self.tracks.len().saturating_sub(1);
            }
        }
        if self.tracks.is_empty() {
            self.current_index = None;
        }
        if self.shuffle {
            self.regenerate_shuffle();
        }
    }

    pub fn current(&self) -> Option<&Track> {
        self.current_index.and_then(|i| self.tracks.get(i))
    }

    pub fn peek_next(&self) -> Option<&Track> {
        if self.tracks.is_empty() {
            return None;
        }
        if self.repeat == RepeatMode::One {
            return self.current();
        }
        if self.shuffle {
            let at = self.shuffle_pos + 1;
            if at >= self.shuffle_order.len() {
                return None;
            }
            return self.tracks.get(self.shuffle_order[at]);
        }
        let at = self.current_index.map(|i| i + 1).unwrap_or(0);
        match (at >= self.tracks.len(), self.repeat) {
            (true, RepeatMode::All) => self.tracks.first(),
            (true, _) => None,
            (false, _) => self.tracks.get(at),
        }
    }

    pub fn next(&mut self) -> Option<&Track> {
        if self.tracks.is_empty() {
            return None;
        }
        self.revision += 1;

        if let Some(ci) = self.current_index {
            self.history.push(ci);
        }

        if self.repeat == RepeatMode::One {
            return self.current();
        }

        if self.shuffle {
            self.shuffle_pos += 1;
            if self.shuffle_pos >= self.shuffle_order.len() {
                if self.repeat == RepeatMode::All {
                    self.regenerate_shuffle();
                } else {
                    self.current_index = None;
                    return None;
                }
            }
            self.current_index = Some(self.shuffle_order[self.shuffle_pos]);
        } else {
            let next = self.current_index.map(|i| i + 1).unwrap_or(0);
            if next >= self.tracks.len() {
                if self.repeat == RepeatMode::All {
                    self.current_index = Some(0);
                } else {
                    self.current_index = None;
                    return None;
                }
            } else {
                self.current_index = Some(next);
            }
        }

        self.current()
    }

    pub fn previous(&mut self) -> Option<&Track> {
        self.revision += 1;
        let back = if self.shuffle { self.history.pop() } else { None };
        if let Some(prev_idx) = back {
            self.current_index = Some(prev_idx);
            if self.shuffle_pos > 0 {
                self.shuffle_pos -= 1;
            }
        } else if !self.tracks.is_empty() {
            if let Some(ci) = self.current_index {
                if ci > 0 {
                    self.current_index = Some(ci - 1);
                } else if self.repeat == RepeatMode::All {
                    self.current_index = Some(self.tracks.len() - 1);
                }
            }
        }
        self.current()
    }

    pub fn toggle_shuffle(&mut self) {
        self.shuffle = !self.shuffle;
        if self.shuffle {
            self.regenerate_shuffle();
        }
    }

    pub fn is_shuffle(&self) -> bool {
        self.shuffle
    }

    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    pub fn current_index(&self) -> Option<usize> {
        self.current_index
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    pub fn keep_only_current(&mut self) {
        let Some(current) = self.current().cloned() else {
            self.clear();
            return;
        };
        self.clear();
        self.tracks.push(current);
        self.current_index = Some(0);
        if self.shuffle {
            self.regenerate_shuffle();
        }
    }

    pub fn clear(&mut self) {
        self.revision += 1;
        self.tracks.clear();
        self.current_index = None;
        self.history.clear();
        self.shuffle_order.clear();
        self.shuffle_pos = 0;
    }

    fn regenerate_shuffle(&mut self) {
        let mut rng = thread_rng();
        self.shuffle_order = (0..self.tracks.len()).collect();
        self.shuffle_order.shuffle(&mut rng);

        if let Some(ci) = self.current_index {
            if let Some(pos) = self.shuffle_order.iter().position(|&i| i == ci) {
                self.shuffle_order.swap(0, pos);
            }
        }
        self.shuffle_pos = 0;
    }
}

impl Default for PlayQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(title: &str) -> Track {
        Track {
            id: title.len() as i64,
            path: std::path::PathBuf::from(format!("/m/{title}.mp3")),
            title: title.into(),
            artist: "autechre".into(),
            album_artist: "autechre".into(),
            album: "amber".into(),
            genre: String::new(),
            year: None,
            track_number: None,
            disc_number: None,
            duration_secs: 200.0,
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

    fn queue(titles: &[&str], at: usize) -> PlayQueue {
        let mut q = PlayQueue::new();
        q.set_tracks(titles.iter().map(|t| track(t)).collect(), at);
        q
    }

    fn titles(q: &PlayQueue) -> Vec<String> {
        q.tracks().iter().map(|t| t.title.clone()).collect()
    }

    #[test]
    fn what_is_asked_for_next_goes_next() {
        let mut q = queue(&["a", "b", "c"], 0);
        q.insert_next(vec![track("x"), track("y")]);
        assert_eq!(titles(&q), ["a", "x", "y", "b", "c"]);
        assert_eq!(q.current().unwrap().title, "a");
    }

    #[test]
    fn next_into_an_empty_queue_is_the_queue() {
        let mut q = PlayQueue::new();
        q.insert_next(vec![track("a"), track("b")]);
        assert_eq!(titles(&q), ["a", "b"]);
        assert_eq!(q.current().unwrap().title, "a");
    }

    #[test]
    fn a_drop_lands_where_it_was_let_go_of() {
        for playing in 0..4 {
            for at in 0..=4 {
                let mut q = queue(&["a", "b", "c", "d"], playing);
                let was = q.current().unwrap().title.clone();
                q.insert_at(at, vec![track("x")]);
                assert_eq!(
                    q.tracks()[at].title,
                    "x",
                    "playing {playing}, dropped at {at}: it landed elsewhere"
                );
                assert_eq!(
                    q.current().unwrap().title,
                    was,
                    "playing {playing}, dropped at {at}: the player jumped"
                );
            }
        }
    }

    #[test]
    fn a_drop_into_an_empty_queue_becomes_the_queue() {
        let mut q = PlayQueue::new();
        q.insert_at(0, vec![track("a"), track("b")]);
        assert_eq!(titles(&q), ["a", "b"]);
        assert_eq!(q.current().unwrap().title, "a");
    }

    #[test]
    fn the_end_is_the_end() {
        let mut q = queue(&["a", "b"], 1);
        q.append(vec![track("c")]);
        assert_eq!(titles(&q), ["a", "b", "c"]);
        assert_eq!(q.current().unwrap().title, "b");
    }

    #[test]
    fn moving_a_row_never_changes_what_is_playing() {
        for playing in 0..5 {
            for from in 0..5 {
                for to in 0..5 {
                    let mut q = queue(&["a", "b", "c", "d", "e"], playing);
                    let was = q.current().unwrap().title.clone();
                    q.move_row(from, to);
                    assert_eq!(
                        q.current().unwrap().title,
                        was,
                        "playing {playing}, moved {from} to {to}: the player jumped"
                    );
                    assert_eq!(q.len(), 5, "a row went missing");
                }
            }
        }
    }

    #[test]
    fn a_moved_row_lands_where_it_was_put() {
        let mut q = queue(&["a", "b", "c", "d"], 0);
        q.move_row(3, 1);
        assert_eq!(titles(&q), ["a", "d", "b", "c"]);

        let mut q = queue(&["a", "b", "c", "d"], 0);
        q.move_row(0, 3);
        assert_eq!(titles(&q), ["b", "c", "d", "a"]);
    }

    #[test]
    fn removing_a_row_never_changes_what_is_playing() {
        for playing in 0..4 {
            for gone in 0..4 {
                let mut q = queue(&["a", "b", "c", "d"], playing);
                let was = q.current().unwrap().title.clone();
                q.remove(gone);
                if gone == playing {
                    continue;
                }
                assert_eq!(
                    q.current().unwrap().title,
                    was,
                    "playing {playing}, removed {gone}: the player jumped"
                );
            }
        }
    }

    #[test]
    fn what_is_looked_ahead_to_is_what_is_played() {
        let three = || vec![track("eins"), track("zwei"), track("drei")];

        let mut q = PlayQueue::new();
        q.set_tracks(three(), 0);
        for _ in 0..2 {
            let looked = q.peek_next().map(|t| t.title.clone());
            let went = q.next().map(|t| t.title.clone());
            assert_eq!(looked, went);
        }
        assert_eq!(q.peek_next().map(|t| t.title.clone()), None);
        assert_eq!(q.next().map(|t| t.title.clone()), None);

        let mut q = PlayQueue::new();
        q.set_tracks(three(), 2);
        q.repeat = RepeatMode::All;
        let looked = q.peek_next().map(|t| t.title.clone());
        let went = q.next().map(|t| t.title.clone());
        assert_eq!(looked, went);
        assert_eq!(went.as_deref(), Some("eins"));

        let mut q = PlayQueue::new();
        q.set_tracks(three(), 1);
        q.repeat = RepeatMode::One;
        let looked = q.peek_next().map(|t| t.title.clone());
        let went = q.next().map(|t| t.title.clone());
        assert_eq!(looked, went);
        assert_eq!(went.as_deref(), Some("zwei"));

        let mut q = PlayQueue::new();
        q.set_tracks(three(), 0);
        q.toggle_shuffle();
        for _ in 0..3 {
            let looked = q.peek_next().map(|t| t.title.clone());
            let went = q.next().map(|t| t.title.clone());
            assert_eq!(looked, went, "shuffled");
        }

        let mut q = PlayQueue::new();
        q.set_tracks(three(), 0);
        let before = q.current_index();
        q.peek_next();
        q.peek_next();
        assert_eq!(q.current_index(), before);
    }

    #[test]
    fn back_goes_one_place_up_the_queue_not_to_where_it_jumped_from() {
        let mut queue = PlayQueue::new();
        let tracks: Vec<Track> = (1..=6).map(|n| track(&"x".repeat(n))).collect();
        queue.set_tracks(tracks, 0);
        queue.jump_to(4);
        assert_eq!(queue.previous().map(|t| t.id), Some(4));
        assert_eq!(queue.previous().map(|t| t.id), Some(3));
    }
}
