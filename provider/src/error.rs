
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),


    #[error("No such track: {0}")]
    TrackNotFound(String),
}

pub type ProviderResult<T=()> = Result<T, ProviderError>;