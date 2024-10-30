use std::io;

#[derive(Debug, thiserror::Error)]
pub enum PlayerError {
    #[error("IO Error: {0}")]
    Io(#[from] io::Error),

    #[error("Rodio Stream Error: {0}")]
    RodioStream(#[from] rodio::StreamError),

    #[error("Rodio Play Error: {0}")]
    RodioPlay(#[from] rodio::PlayError),

    #[error("Rodio Decoder Error: {0}")]
    RodioDecoder(#[from] rodio::decoder::DecoderError),

    #[error("Cannot build source: {0}")]
    UnableToBuildSource(String),

    #[error("Cannot fetch source infomation: {0}")]
    UnableToFetchSourceInfo(String),

    #[error("Empty track")]
    EmptyTrack,
}

pub type PlayerResult<T = ()> = Result<T, PlayerError>;
