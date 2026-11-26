//! Choosing a cover for one album.
//!
//! The state of the chooser, and the threads it needs. The drawing is in
//! [`super::library`]; what is written and where is in
//! [`crate::library::tags`] and [`crate::library::artwork`].
//!
//! Everything that touches the network happens on another thread. A cover
//! search is two requests and a handful of downloads, and any one of them can
//! sit there for twenty seconds — on the drawing thread that is a window that
//! has stopped answering.

use std::path::PathBuf;
use std::sync::mpsc;

use crate::error::Result;
use crate::library::coverart::{self, Candidate};

/// One suggestion, with its picture if it arrived.
pub struct Found {
    pub candidate: Candidate,
    /// The small version, as it came down the wire.
    pub thumb: Option<Vec<u8>>,
    /// Uploaded once, on the first frame that draws it.
    pub texture: Option<eframe::egui::TextureHandle>,
}

/// One suggestion as the search thread hands it over: what was found, and the
/// small picture if it came down.
type Offer = (Candidate, Option<Vec<u8>>);

/// What the chooser is doing.
pub enum State {
    /// The search is out.
    Looking(mpsc::Receiver<Result<Vec<Offer>>>),
    /// Here is what came back.
    Offering { found: Vec<Found>, focus: usize },
    /// One was chosen and the full-size picture is on its way.
    Fetching(mpsc::Receiver<Result<Vec<u8>>>),
    /// Nothing came back, or something went wrong saying so.
    Trouble(String),
}

/// The chooser, while it is open.
pub struct Picker {
    pub artist: String,
    pub album: String,
    /// The folded keys, so the cache is written where the grid looks for it.
    pub artist_key: String,
    pub album_key: String,
    /// Every file of the album — the cover goes into all of them.
    pub paths: Vec<PathBuf>,
    /// How many suggestions the drawing last fitted across. The keyboard needs
    /// it: in a grid, down is one row further on, and only the layout knows how
    /// far that is.
    pub across: usize,
    pub state: State,
}

impl Picker {
    /// Open the chooser and send the search out.
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

    /// Whether a thread is out. While one is, the window has to keep asking
    /// for frames — egui draws when something happens, and an answer arriving
    /// on a channel is not something happening.
    pub fn is_busy(&self) -> bool {
        matches!(self.state, State::Looking(_) | State::Fetching(_))
    }

    /// Move the highlight, if there is anything to move it over.
    pub fn step(&mut self, delta: i32) {
        if let State::Offering { found, focus } = &mut self.state {
            if found.is_empty() {
                return;
            }
            let last = found.len() as i32 - 1;
            *focus = (*focus as i32 + delta).clamp(0, last) as usize;
        }
    }

    /// Take whichever one the highlight is on.
    pub fn take_focused(&mut self) {
        if let State::Offering { focus, .. } = &self.state {
            self.take(*focus);
        }
    }

    /// Take one suggestion: fetch it at full size.
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

    /// Look in on the threads. `Some` when a picture has arrived and is ready
    /// to be written; the caller does the writing, because only it knows about
    /// the database and the cache.
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

/// A message without the machinery in front of it.
///
/// Errors come up through layers that each name themselves — "metadata error:
/// metadata error: curl: (7) …" — and none of those words is for the person
/// reading the window. What is left is the part that says what happened.
fn plainly(said: &str) -> String {
    match said.rfind("error: ") {
        Some(at) => said[at + "error: ".len()..].to_string(),
        None => said.to_string(),
    }
}

/// The search itself, on its own thread: ask the sources, then fetch the
/// small pictures so there is something to look at.
///
/// A thumbnail that will not come down is not a reason to drop the
/// suggestion — the archive sometimes has the record but not the picture, and
/// saying so is more use than a shorter list.
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

    /// The window is not the place for the names of the layers an error came
    /// up through.
    #[test]
    fn a_message_loses_its_machinery() {
        assert_eq!(
            plainly("metadata error: metadata error: curl: (7) nicht erreichbar"),
            "curl: (7) nicht erreichbar"
        );
        assert_eq!(plainly("nichts gefunden"), "nichts gefunden");
        assert_eq!(plainly(""), "");
    }

    /// The highlight stops at both ends rather than wrapping: a wall of
    /// pictures is looked at, not cycled through, and jumping from the last to
    /// the first loses your place.
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

    /// Nothing to choose from is not an empty list to stare at.
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

    /// A thread that dies without sending must not leave the chooser waiting
    /// for ever.
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

    /// The picture arrives once, and the caller is the one told about it.
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

    /// While a thread is out, the window has to keep asking for frames.
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
