use rodio::decoder::DecoderError;

#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Decode error: {0}")]
    Decoder(#[from] DecoderError),
}

pub type SourceResult<T> = Result<T, SourceError>;
