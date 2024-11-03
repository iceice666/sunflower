use std::io;

use crate::_impl::EventRequest;

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

    #[error("Cannot fetch source information: {0}")]
    UnableToFetchSourceInfo(String),

    #[error("Empty track")]
    EmptyTrack,
}

pub type PlayerResult<T = ()> = Result<T, PlayerError>;

#[cfg(feature = "interface")]
use crate::interface::PlayerInterface;

#[cfg(feature = "interface")]
#[derive(Debug, thiserror::Error)]
pub enum PlayerInterfaceError {
    #[error("Failed to send request: {0}")]
    SendRequestError(#[from] std::sync::mpsc::SendError<EventRequest>),

    #[error("Failed to receive response: {0}")]
    RecvResponseError(#[from] std::sync::mpsc::RecvError),

    #[error("Failed to create player: {0}")]
    UnableToRecvPlayer(#[from] oneshot::RecvError),

    #[error("Failed to create player: {0}")]
    UnableToSendPlayer(#[from] oneshot::SendError<PlayerInterface>),

    #[error("Failed to create player: {0}")]
    UnableToStartPlayerThread(#[from] std::io::Error),

    #[error("Player error: {0}")]
    PlayerImplError(#[from] PlayerError),
}
