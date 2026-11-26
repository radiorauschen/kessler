use thiserror::Error;

/// Central error type for Kessler.
#[derive(Error, Debug)]
pub enum KesslerError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Config error: {0}")]
    Config(String),

    #[error("Audio playback error: {0}")]
    Playback(String),

    /// The file a track stands for is no longer where the library says.
    ///
    /// Told apart from `Playback` because the two are different troubles: a
    /// file that will not decode is a bad file, a file that is not there has
    /// been moved or deleted since the shelf was read. Only the second can be
    /// said in one short line and marked on the row itself, which is what the
    /// window does with it. The path is carried for that marking rather than
    /// for the message — the row says which track, the line says what
    /// happened.
    #[error("{}", crate::lang::t("die datei ist nicht mehr da"))]
    Gone(std::path::PathBuf),

    #[error("Metadata error: {0}")]
    Metadata(String),

    #[error("iPod error: {0}")]
    Ipod(String),

    #[error("UI error: {0}")]
    Ui(String),

    #[error("last.fm: {0}")]
    Lastfm(String),
}

pub type Result<T> = std::result::Result<T, KesslerError>;
