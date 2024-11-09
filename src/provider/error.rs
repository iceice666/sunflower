#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),

    #[error("No such track: {0}")]
    TrackNotFound(String),

    #[error("No such provider: {0}")]
    ProviderNotFound(String),

    #[error("Missing field `{0}` to build provider {1}")]
    MissingFieldToBuildProvider(String, String),
    
    #[error("This track source ({0}) does not support build from track config")]
    UnsupportedTrackSource(String),

    #[error{"Expected data, but got None"}]
    EmptyData,

    #[error("Invalid data")]
    InvalidData,
}

pub type ProviderResult<T = ()> = Result<T, ProviderError>;
