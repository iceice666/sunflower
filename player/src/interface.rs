use std::sync::mpsc::{Receiver, Sender};

use crate::{_impl::Player as PlayerImpl, error::PlayerResult, TrackObject};

pub use crate::_impl::{EventRequest, EventResponse, RepeatState};

type SendResult<T = ()> = Result<T, String>;

pub struct Player {
    base: PlayerImpl,
    tx: Sender<EventRequest>,
    rx: Receiver<EventResponse>,
}

impl Player {
    pub fn try_new() -> PlayerResult<Self> {
        let (base, tx, rx) = PlayerImpl::try_new()?;
        Ok(Self { base, tx, rx })
    }

    fn send_request(&self, event: EventRequest) -> SendResult<EventResponse> {
        self.tx
            .send(event)
            .map_err(|e| format!("Failed to send request: {}", e))?;
        let resp = self
            .rx
            .recv()
            .map_err(|e| format!("Failed to receive response: {}", e))?;

        match resp {
            EventResponse::Error(msg) => Err(msg),
            _ => Ok(resp),
        }
    }
}

impl Player {
    pub fn play(&self) -> SendResult {
        self.send_request(EventRequest::Play).map(|_| ())
    }

    pub fn pause(&self) -> SendResult {
        self.send_request(EventRequest::Pause).map(|_| ())
    }

    pub fn stop(&self) -> SendResult {
        self.send_request(EventRequest::Stop).map(|_| ())
    }

    pub fn next(&self) -> SendResult {
        self.send_request(EventRequest::Next).map(|_| ())
    }

    pub fn prev(&self) -> SendResult {
        self.send_request(EventRequest::Prev).map(|_| ())
    }

    pub fn set_volume(&self, volume: f32) -> SendResult {
        self.send_request(EventRequest::SetVolume(volume))
            .map(|_| ())
    }

    pub fn get_volume(&self) -> SendResult<f32> {
        self.send_request(EventRequest::GetVolume)
            .map(|res| match res {
                EventResponse::Volume(volume) => volume,
                _ => unreachable!(),
            })
    }

    pub fn set_repeat(&self, state: RepeatState) -> SendResult {
        self.send_request(EventRequest::SetRepeat(state))
            .map(|_| ())
    }

    pub fn get_repeat(&self) -> SendResult<RepeatState> {
        self.send_request(EventRequest::GetRepeat)
            .map(|res| match res {
                EventResponse::Repeat(repeat) => repeat,
                _ => unreachable!(),
            })
    }

    pub fn toggle_shuffle(&self) -> SendResult<bool> {
        self.send_request(EventRequest::ToggleShuffle)
            .map(|res| match res {
                EventResponse::Shuffled(enabled) => enabled,
                _ => unreachable!(),
            })
    }

    pub fn new_track(&self, track: TrackObject) -> SendResult {
        self.send_request(EventRequest::NewTrack(track)).map(|_| ())
    }

    pub fn clear_playlist(&self) -> SendResult {
        self.send_request(EventRequest::ClearPlaylist).map(|_| ())
    }

    pub fn remove_track(&self, index: usize) -> SendResult {
        self.send_request(EventRequest::RemoveTrack(index))
            .map(|_| ())
    }

    pub fn terminate(&self) -> SendResult {
        self.send_request(EventRequest::Terminate).map(|_| ())
    }
}
