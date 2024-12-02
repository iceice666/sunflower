#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Track not found: {0}")]
    TrackNotFound(String),

    #[error("Empty search result")]
    EmptySearchResult,
}

pub type ProviderResult<T> = Result<T, ProviderError>;
