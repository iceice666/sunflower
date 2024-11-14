#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),

    #[error("No such track: {0}")]
    TrackNotFound(String),

    #[error("No such provider: {0}")]
    ProviderNotFound(String),

    #[error("Missing field `{0}`")]
    MissingField(String),

    #[error("This track source ({0}) does not support build from track config")]
    UnsupportedTrackSource(String),

    #[error{"Expected data, but got None"}]
    EmptyData,

    #[error("Invalid data: {0}")]
    InvalidData(String),

    #[error("UTF-8 error: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),

    #[error("Command execution failed: {0}")]
    Command(String),
}

pub type ProviderResult<T = ()> = Result<T, ProviderError>;
