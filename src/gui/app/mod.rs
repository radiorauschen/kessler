//! The window's own behaviour, in the order a reader might want it: each
//! file is one more `impl GuiApp`, grouped by what it is about. The state
//! itself — [`crate::gui::GuiApp`] and its types — is in [`crate::gui`].

mod lists;
mod device;
mod navigation;
mod tags;
mod playback;
mod look;
mod music;
mod panels;
mod artists;
mod lyrics;
mod eq;
mod radio;
mod scrobbling;
mod keys;
mod podcast_keys;
mod reel;
pub mod podcasts;

pub(in crate::gui) use look::ZoomDip;
pub(in crate::gui) use reel::{Film, Reel, Written};
