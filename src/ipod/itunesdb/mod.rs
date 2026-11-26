//! The iPod `iTunesDB` database: types and a read-only parser.
//!
//! Reading works for every little-endian iTunesDB. Writing works for devices
//! that do not require a signed header — see `crate::ipod::device_info`. A
//! malformed or unsigned-where-signing-is-required iTunesDB makes an iPod show
//! an empty library, so callers must check the device before writing.

pub mod parser;
#[cfg(test)]
pub mod test_writer;
pub mod types;
pub mod writer;

#[allow(unused_imports)]
pub use parser::parse;
#[allow(unused_imports)]
pub use types::{IpodPlaylist, IpodTrack, ItunesDb};

use std::path::Path;

use crate::error::{KesslerError, Result};

/// Read and parse an iTunesDB from disk.
pub fn read_from_file(path: &Path) -> Result<ItunesDb> {
    let data = std::fs::read(path).map_err(|e| {
        KesslerError::Ipod(format!("cannot read {}: {}", path.display(), e))
    })?;
    parse(&data)
}
