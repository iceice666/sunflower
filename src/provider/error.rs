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
}

pub type ProviderResult<T = ()> = Result<T, ProviderError>;
