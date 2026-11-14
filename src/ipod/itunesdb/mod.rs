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

pub fn read_from_file(path: &Path) -> Result<ItunesDb> {
    let data = std::fs::read(path).map_err(|e| {
        KesslerError::Ipod(format!("cannot read {}: {}", path.display(), e))
    })?;
    parse(&data)
}
