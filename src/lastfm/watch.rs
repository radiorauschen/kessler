use crate::db::models::Track;

pub const MIN_LENGTH_SECS: f64 = 30.0;
pub const ENOUGH_SECS: f64 = 240.0;
pub const ENOUGH_FRACTION: f64 = 0.5;

#[derive(Debug, Clone)]
pub enum Event {
    NowPlaying(Box<Track>),
    Scrobble {
        track: Box<Track>,
        started_at: i64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Moment {
    pub track_id: Option<i64>,
    pub position: f64,
    pub now: i64,
}

#[derive(Debug, Clone)]
struct Playing {
    track: Track,
    started_at: i64,
    furthest: f64,
    listened: f64,
    scrobbled: bool,
}

#[derive(Debug, Default)]
pub struct Watch {
    playing: Option<Playing>,
    last_position: f64,
}

impl Watch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn observe(&mut self, moment: Moment, track: Option<&Track>) -> Vec<Event> {
        let mut events = Vec::new();

        let changed = match (&self.playing, moment.track_id) {
            (Some(playing), Some(id)) => playing.track.id != id,
            (Some(_), None) => true,
            (None, Some(_)) => true,
            (None, None) => false,
        };

        if changed {
            if let Some(done) = self.playing.take() {
                if let Some(event) = finished(done) {
                    events.push(event);
                }
            }
            if let (Some(track), Some(_)) = (track, moment.track_id) {
                self.playing = Some(Playing {
                    track: track.clone(),
                    started_at: moment.now,
                    furthest: moment.position,
                    listened: 0.0,
                    scrobbled: false,
                });
                self.last_position = moment.position;
                events.push(Event::NowPlaying(Box::new(track.clone())));
            }
            return events;
        }

        let Some(playing) = self.playing.as_mut() else {
            return events;
        };

        let step = moment.position - self.last_position;
        if step > 0.0 && step < 4.0 {
            playing.listened += step;
        }
        playing.furthest = playing.furthest.max(moment.position);
        self.last_position = moment.position;

        if !playing.scrobbled && earned(&playing.track, playing.listened) {
            playing.scrobbled = true;
            events.push(Event::Scrobble {
                track: Box::new(playing.track.clone()),
                started_at: playing.started_at,
            });
        }

        events
    }

    #[cfg(test)]
    pub fn current(&self) -> Option<&Track> {
        self.playing.as_ref().map(|p| &p.track)
    }

    pub fn standing(&self) -> Option<Standing> {
        let playing = self.playing.as_ref()?;
        if playing.scrobbled {
            return Some(Standing::Counted);
        }
        if !could_ever(&playing.track) {
            return Some(Standing::Never);
        }
        Some(Standing::Listening {
            left: (wanted(&playing.track) - playing.listened).max(0.0),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Standing {
    Listening { left: f64 },
    Counted,
    Never,
}

fn wanted(track: &Track) -> f64 {
    ENOUGH_SECS.min(track.duration_secs * ENOUGH_FRACTION)
}

fn could_ever(track: &Track) -> bool {
    track.duration_secs > MIN_LENGTH_SECS
        && !track.artist.trim().is_empty()
        && !track.title.trim().is_empty()
}

fn finished(done: Playing) -> Option<Event> {
    if done.scrobbled || !earned(&done.track, done.listened) {
        return None;
    }
    Some(Event::Scrobble {
        track: Box::new(done.track),
        started_at: done.started_at,
    })
}

pub fn earned(track: &Track, listened: f64) -> bool {
    could_ever(track) && listened >= wanted(track)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: i64, title: &str, seconds: f64) -> Track {
        Track {
            id,
            path: std::path::PathBuf::from(format!("/m/{id}.mp3")),
            title: title.into(),
            artist: "Autechre".into(),
            album_artist: "Autechre".into(),
            album: "Amber".into(),
            genre: String::new(),
            year: Some(1994),
            track_number: Some(1),
            disc_number: Some(1),
            duration_secs: seconds,
            bitrate: None,
            sample_rate: None,
            file_size: 1,
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

    fn play(watch: &mut Watch, track: &Track, seconds: i64, from: i64) -> Vec<Event> {
        let mut events = Vec::new();
        for step in 0..=seconds {
            events.extend(watch.observe(
                Moment {
                    track_id: Some(track.id),
                    position: step as f64,
                    now: from + step,
                },
                Some(track),
            ));
        }
        events
    }

    #[test]
    fn the_standing_counts_down_and_then_says_it_counted() {
        let mut watch = Watch::new();
        let song = track(1, "foil", 300.0);

        assert_eq!(watch.standing(), None, "nothing is playing yet");

        play(&mut watch, &song, 0, 1000);
        assert_eq!(watch.standing(), Some(Standing::Listening { left: 150.0 }));

        play_on(&mut watch, &song, 1..=100);
        assert_eq!(watch.standing(), Some(Standing::Listening { left: 50.0 }));

        play_on(&mut watch, &song, 101..=160);
        assert_eq!(watch.standing(), Some(Standing::Counted));
    }

    #[test]
    fn a_track_that_cannot_count_says_so_from_the_first_second() {
        let mut watch = Watch::new();

        let short = track(1, "interlude", 12.0);
        play(&mut watch, &short, 3, 1000);
        assert_eq!(watch.standing(), Some(Standing::Never));

        let mut untitled = track(2, "", 300.0);
        untitled.title = String::new();
        let mut watch = Watch::new();
        play(&mut watch, &untitled, 3, 1000);
        assert_eq!(watch.standing(), Some(Standing::Never));
    }

    fn play_on(watch: &mut Watch, track: &Track, seconds: std::ops::RangeInclusive<i64>) {
        for second in seconds {
            watch.observe(
                Moment {
                    track_id: Some(track.id),
                    position: second as f64,
                    now: 1000 + second,
                },
                Some(track),
            );
        }
    }

    #[test]
    fn a_track_is_announced_as_soon_as_it_starts() {
        let mut watch = Watch::new();
        let song = track(1, "foil", 300.0);
        let events = play(&mut watch, &song, 2, 1000);
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], Event::NowPlaying(_)));
    }

    #[test]
    fn a_long_track_scrobbles_after_four_minutes() {
        let mut watch = Watch::new();
        let song = track(1, "montreal", 900.0);

        let early = play(&mut watch, &song, 238, 1000);
        assert_eq!(scrobbles(&early), 0, "not yet at four minutes");

        let mut later = Vec::new();
        for second in 239..=241 {
            later.extend(watch.observe(
                Moment {
                    track_id: Some(1),
                    position: second as f64,
                    now: 1000 + second,
                },
                Some(&song),
            ));
        }
        assert_eq!(scrobbles(&later), 1);
    }

    fn scrobbles(events: &[Event]) -> usize {
        events
            .iter()
            .filter(|e| matches!(e, Event::Scrobble { .. }))
            .count()
    }

    #[test]
    fn half_a_short_track_is_enough() {
        let mut watch = Watch::new();
        let song = track(1, "silverside", 120.0);
        let events = play(&mut watch, &song, 61, 1000);
        let scrobbles: Vec<&Event> = events
            .iter()
            .filter(|e| matches!(e, Event::Scrobble { .. }))
            .collect();
        assert_eq!(scrobbles.len(), 1);
        match scrobbles[0] {
            Event::Scrobble { started_at, track } => {
                assert_eq!(*started_at, 1000);
                assert_eq!(track.title, "silverside");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn a_track_is_only_scrobbled_once() {
        let mut watch = Watch::new();
        let song = track(1, "yulquen", 100.0);
        let events = play(&mut watch, &song, 99, 1000);
        assert_eq!(
            events.iter().filter(|e| matches!(e, Event::Scrobble { .. })).count(),
            1
        );
    }

    #[test]
    fn dragging_to_the_end_earns_nothing() {
        let mut watch = Watch::new();
        let song = track(1, "nine", 300.0);
        let mut events = watch.observe(
            Moment { track_id: Some(1), position: 0.0, now: 1000 },
            Some(&song),
        );
        events.extend(watch.observe(
            Moment { track_id: Some(1), position: 290.0, now: 1001 },
            Some(&song),
        ));
        events.extend(watch.observe(
            Moment { track_id: Some(1), position: 291.0, now: 1002 },
            Some(&song),
        ));
        assert_eq!(
            events.iter().filter(|e| matches!(e, Event::Scrobble { .. })).count(),
            0
        );
    }

    #[test]
    fn a_skipped_track_is_not_scrobbled() {
        let mut watch = Watch::new();
        let first = track(1, "foil", 300.0);
        let second = track(2, "tha", 300.0);
        let mut events = play(&mut watch, &first, 20, 1000);
        events.extend(watch.observe(
            Moment { track_id: Some(2), position: 0.0, now: 1100 },
            Some(&second),
        ));
        assert_eq!(
            events.iter().filter(|e| matches!(e, Event::Scrobble { .. })).count(),
            0
        );
        assert_eq!(
            events.iter().filter(|e| matches!(e, Event::NowPlaying(_))).count(),
            2
        );
    }

    #[test]
    fn a_finished_track_pays_out_when_it_ends() {
        let mut watch = Watch::new();
        let song = track(1, "rsdio", 60.0);
        let mut events = play(&mut watch, &song, 29, 1000);
        assert_eq!(
            events.iter().filter(|e| matches!(e, Event::Scrobble { .. })).count(),
            0
        );
        events.extend(watch.observe(
            Moment { track_id: Some(1), position: 30.0, now: 1030 },
            Some(&song),
        ));
        assert_eq!(
            events.iter().filter(|e| matches!(e, Event::Scrobble { .. })).count(),
            1
        );
    }

    #[test]
    fn a_very_short_track_is_never_scrobbled() {
        let mut watch = Watch::new();
        let song = track(1, "interlude", 20.0);
        let events = play(&mut watch, &song, 20, 1000);
        assert_eq!(
            events.iter().filter(|e| matches!(e, Event::Scrobble { .. })).count(),
            0
        );
    }

    #[test]
    fn a_track_without_a_name_is_not_reported() {
        let mut nameless = track(1, "", 300.0);
        nameless.artist = String::new();
        assert!(!earned(&nameless, 1000.0));
    }

    #[test]
    fn stopping_pays_out_what_was_earned() {
        let mut watch = Watch::new();
        let song = track(1, "clipper", 100.0);
        let mut events = play(&mut watch, &song, 40, 1000);
        assert_eq!(
            events.iter().filter(|e| matches!(e, Event::Scrobble { .. })).count(),
            0
        );
        events.extend(watch.observe(
            Moment { track_id: None, position: 0.0, now: 1100 },
            None,
        ));
        assert_eq!(
            events.iter().filter(|e| matches!(e, Event::Scrobble { .. })).count(),
            0,
            "forty seconds of a hundred is not half"
        );
        assert!(watch.current().is_none());
    }
}
