use std::path::PathBuf;
use std::sync::mpsc;

use crate::error::Result;
use crate::library::coverart::{self, Candidate};

pub struct Found {
    pub candidate: Candidate,
    pub thumb: Option<Vec<u8>>,
    pub texture: Option<eframe::egui::TextureHandle>,
}

type Offer = (Candidate, Option<Vec<u8>>);

pub enum State {
    Looking(mpsc::Receiver<Result<Vec<Offer>>>),
    Offering { found: Vec<Found>, focus: usize },
    Fetching(mpsc::Receiver<Result<Vec<u8>>>),
    Trouble(String),
}

pub struct Picker {
    pub artist: String,
    pub album: String,
    pub artist_key: String,
    pub album_key: String,
    pub paths: Vec<PathBuf>,
    pub across: usize,
    pub state: State,
}

impl Picker {
    pub fn start(
        artist: String,
        album: String,
        artist_key: String,
        album_key: String,
        paths: Vec<PathBuf>,
        lastfm_key: Option<String>,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        let (for_artist, for_album) = (artist.clone(), album.clone());
        std::thread::spawn(move || {
            let _ = tx.send(look(&for_artist, &for_album, lastfm_key.as_deref()));
        });
        Self {
            artist,
            album,
            artist_key,
            album_key,
            paths,
            across: 3,
            state: State::Looking(rx),
        }
    }

    pub fn is_busy(&self) -> bool {
        matches!(self.state, State::Looking(_) | State::Fetching(_))
    }

    pub fn step(&mut self, delta: i32) {
        if let State::Offering { found, focus } = &mut self.state {
            if found.is_empty() {
                return;
            }
            let last = found.len() as i32 - 1;
            *focus = (*focus as i32 + delta).clamp(0, last) as usize;
        }
    }

    pub fn take_focused(&mut self) {
        if let State::Offering { focus, .. } = &self.state {
            self.take(*focus);
        }
    }

    pub fn take(&mut self, index: usize) {
        let url = match &self.state {
            State::Offering { found, .. } => match found.get(index) {
                Some(found) => (found.candidate.full.clone(), found.candidate.thumb.clone()),
                None => return,
            },
            _ => return,
        };
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let (full, thumb) = url;
            let _ = tx.send(coverart::fetch(&full).or_else(|e| {
                if thumb != full {
                    coverart::fetch(&thumb)
                } else {
                    Err(e)
                }
            }));
        });
        self.state = State::Fetching(rx);
    }

    pub fn poll(&mut self) -> Option<Vec<u8>> {
        match &mut self.state {
            State::Looking(rx) => {
                match rx.try_recv() {
                    Ok(Ok(list)) if list.is_empty() => {
                        self.state = State::Trouble(tr!("nichts gefunden").into());
                    }
                    Ok(Ok(list)) => {
                        let found = list
                            .into_iter()
                            .map(|(candidate, thumb)| Found {
                                candidate,
                                thumb,
                                texture: None,
                            })
                            .collect();
                        self.state = State::Offering { found, focus: 0 };
                    }
                    Ok(Err(e)) => self.state = State::Trouble(plainly(&e.to_string())),
                    Err(mpsc::TryRecvError::Empty) => {}
                    Err(mpsc::TryRecvError::Disconnected) => {
                        self.state = State::Trouble(tr!("die suche ist abgebrochen").into())
                    }
                }
                None
            }
            State::Fetching(rx) => match rx.try_recv() {
                Ok(Ok(bytes)) => Some(bytes),
                Ok(Err(e)) => {
                    self.state = State::Trouble(plainly(&e.to_string()));
                    None
                }
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.state = State::Trouble(tr!("das bild ist nicht angekommen").into());
                    None
                }
            },
            _ => None,
        }
    }
}

fn plainly(said: &str) -> String {
    match said.rfind("error: ") {
        Some(at) => said[at + "error: ".len()..].to_string(),
        None => said.to_string(),
    }
}

fn look(artist: &str, album: &str, lastfm_key: Option<&str>) -> Result<Vec<Offer>> {
    let candidates = coverart::search(artist, album, lastfm_key)?;
    Ok(candidates
        .into_iter()
        .map(|candidate| {
            let thumb = coverart::fetch(&candidate.thumb).ok();
            (candidate, thumb)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offering(count: usize) -> Picker {
        let found = (0..count)
            .map(|i| Found {
                candidate: Candidate {
                    source: "itunes",
                    artist: "boards of canada".into(),
                    album: format!("album {i}"),
                    year: None,
                    thumb: String::new(),
                    full: String::new(),
                },
                thumb: None,
                texture: None,
            })
            .collect();
        Picker {
            artist: "boards of canada".into(),
            album: "geogaddi".into(),
            artist_key: "boards of canada".into(),
            album_key: "geogaddi".into(),
            paths: Vec::new(),
            across: 3,
            state: State::Offering { found, focus: 0 },
        }
    }

    #[test]
    fn a_message_loses_its_machinery() {
        assert_eq!(
            plainly("metadata error: metadata error: curl: (7) nicht erreichbar"),
            "curl: (7) nicht erreichbar"
        );
        assert_eq!(plainly("nichts gefunden"), "nichts gefunden");
        assert_eq!(plainly(""), "");
    }

    #[test]
    fn the_highlight_stays_inside_the_list() {
        let mut picker = offering(3);
        picker.step(-1);
        assert!(matches!(picker.state, State::Offering { focus: 0, .. }));
        picker.step(1);
        picker.step(1);
        picker.step(1);
        picker.step(1);
        assert!(matches!(picker.state, State::Offering { focus: 2, .. }));
    }

    #[test]
    fn an_empty_answer_says_so() {
        let (tx, rx) = mpsc::channel();
        let mut picker = Picker {
            state: State::Looking(rx),
            ..offering(0)
        };
        tx.send(Ok(Vec::new())).unwrap();
        assert!(picker.poll().is_none());
        match &picker.state {
            State::Trouble(said) => assert_eq!(said, "nichts gefunden"),
            _ => panic!("still looking"),
        }
    }

    #[test]
    fn a_search_that_never_answers_ends() {
        let (tx, rx) = mpsc::channel::<Result<Vec<Offer>>>();
        let mut picker = Picker {
            state: State::Looking(rx),
            ..offering(0)
        };
        drop(tx);
        picker.poll();
        assert!(matches!(picker.state, State::Trouble(_)));
    }

    #[test]
    fn a_fetched_picture_is_handed_over() {
        let (tx, rx) = mpsc::channel();
        let mut picker = Picker {
            state: State::Fetching(rx),
            ..offering(0)
        };
        assert!(picker.poll().is_none());
        tx.send(Ok(vec![1, 2, 3])).unwrap();
        assert_eq!(picker.poll(), Some(vec![1, 2, 3]));
    }

    #[test]
    fn busy_is_only_while_a_thread_is_out() {
        let (_tx, rx) = mpsc::channel::<Result<Vec<u8>>>();
        let picker = Picker {
            state: State::Fetching(rx),
            ..offering(0)
        };
        assert!(picker.is_busy());
        assert!(!offering(2).is_busy());
        let quiet = Picker {
            state: State::Trouble("nichts gefunden".into()),
            ..offering(0)
        };
        assert!(!quiet.is_busy());
    }
}
