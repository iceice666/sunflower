#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Track not found: {0}")]
    TrackNotFound(String),

    #[error("Empty search result")]
    EmptySearchResult,

    #[error("Provider doesn't exist: {0}")]
    ProviderNotFound(String),

    #[error("Failed to perform SQL query: {0}")]
    Sql(#[from] rusqlite::Error),
}

pub type ProviderResult<T> = Result<T, ProviderError>;
