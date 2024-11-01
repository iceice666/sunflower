use std::{
    sync::mpsc::{Receiver, Sender},
    thread::{self, JoinHandle},
};

pub use crate::error::PlayerError as PlayerImplError;
use crate::{_impl::Player as PlayerImpl, track::TrackObject};
use tracing::debug;

pub use crate::_impl::{EventRequest, EventResponse, RepeatState};

#[derive(Debug, thiserror::Error)]
pub enum PlayerError {
    #[error("Failed to send request: {0}")]
    SendRequestError(#[from] std::sync::mpsc::SendError<EventRequest>),

    #[error("Failed to receive response: {0}")]
    RecvResponseError(#[from] std::sync::mpsc::RecvError),

    #[error("Failed to create player: {0}")]
    UnableToRecvPlayer(#[from] oneshot::RecvError),

    #[error("Failed to create player: {0}")]
    UnableToSendPlayer(#[from] oneshot::SendError<Player>),

    #[error("Failed to create player: {0}")]
    UnableToStartPlayerThread(#[from] std::io::Error),

    #[error("Player error: {0}")]
    PlayerImplError(#[from] PlayerImplError),
}

pub struct Player {
    tx: Sender<EventRequest>,
    rx: Receiver<EventResponse>,
}

type Result<T = ()> = std::result::Result<T, PlayerError>;

impl Player {
    pub fn try_new() -> Result<(Self, JoinHandle<Result>)> {
        // Since the rodio sink doesn't allow sending, so we start a new thread
        // then send the tx and rx to the main thread

        let (oneshot_tx, oneshot_rx) = oneshot::channel();

        let handle = thread::Builder::new()
            .name("PlayerThread".into())
            .spawn(|| -> Result {
                let (base, tx, rx) = PlayerImpl::try_new()?;

                let this = Self { tx, rx };
                oneshot_tx.send(this)?;

                base.mainloop();

                Ok(())
            })?;

        let this = oneshot_rx.recv()?;

        Ok((this, handle))
    }
}

impl Player {
    fn send_request(&self, event: EventRequest) -> Result<EventResponse> {
        debug!("Sending request: {:?}", event);

        self.tx.send(event)?;

        Ok(self.rx.recv()?)
    }

    pub fn play(&self) -> Result {
        self.send_request(EventRequest::Play).map(|_| ())
    }

    pub fn pause(&self) -> Result {
        self.send_request(EventRequest::Pause).map(|_| ())
    }

    pub fn stop(&self) -> Result {
        self.send_request(EventRequest::Stop).map(|_| ())
    }

    pub fn next(&self) -> Result {
        self.send_request(EventRequest::Next).map(|_| ())
    }

    pub fn prev(&self) -> Result {
        self.send_request(EventRequest::Prev).map(|_| ())
    }

    pub fn set_volume(&self, volume: f32) -> Result {
        self.send_request(EventRequest::SetVolume(volume))
            .map(|_| ())
    }

    pub fn get_volume(&self) -> Result<f32> {
        self.send_request(EventRequest::GetVolume)
            .map(|res| match res {
                EventResponse::Volume(volume) => volume,
                _ => unreachable!(),
            })
    }

    pub fn set_repeat(&self, state: RepeatState) -> Result {
        self.send_request(EventRequest::SetRepeat(state))
            .map(|_| ())
    }

    pub fn get_repeat(&self) -> Result<RepeatState> {
        self.send_request(EventRequest::GetRepeat)
            .map(|res| match res {
                EventResponse::Repeat(repeat) => repeat,
                _ => unreachable!(),
            })
    }

    pub fn toggle_shuffle(&self) -> Result<bool> {
        self.send_request(EventRequest::ToggleShuffle)
            .map(|res| match res {
                EventResponse::Shuffled(enabled) => enabled,
                _ => unreachable!(),
            })
    }

    pub fn new_track(&self, track: TrackObject) -> Result {
        self.send_request(EventRequest::NewTrack(track)).map(|_| ())
    }

    pub fn clear_playlist(&self) -> Result {
        self.send_request(EventRequest::ClearPlaylist).map(|_| ())
    }

    pub fn remove_track(&self, index: usize) -> Result {
        self.send_request(EventRequest::RemoveTrack(index))
            .map(|_| ())
    }

    pub fn terminate(&self) -> Result {
        self.send_request(EventRequest::Terminate).map(|_| ())
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        // Attempt to terminate gracefully
        let _ = self.terminate();
    }
}
