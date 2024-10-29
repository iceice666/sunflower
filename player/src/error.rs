use std::io;

#[derive(Debug, thiserror::Error)]
pub enum PlayerError {
    #[error("IO Error: {0}")]
    Io(#[from] io::Error),

    #[error("Rodio Stream Error: {0}")]
    RodioStream(#[from] rodio::StreamError),

    #[error("Rodio Play Error: {0}")]
    RodioPlay(#[from] rodio::PlayError),

    #[error("Cannot build source: {0}")]
    UnableToBuildSource(String),

    #[error("Cannot fetch source infomation: {0}")]
    UnableToFetchSourceInfo(String),
}

pub type PlayerResult<T = ()> = Result<T, PlayerError>;
