use thiserror::Error;

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
