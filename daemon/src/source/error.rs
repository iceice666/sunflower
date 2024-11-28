use rodio::decoder::DecoderError;
use std::io;

#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("IO error: {0}")]
    IO(#[from] io::Error),

    #[error("Decode error: {0}")]
    Decoder(#[from] DecoderError),
}

pub type SourceResult<T> = Result<T, SourceError>;
