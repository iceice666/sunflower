use std::sync::mpsc::{Receiver, Sender};

use crate::{_impl::Player as PlayerImpl, error::PlayerResult, TrackObject};

pub use crate::_impl::{EventRequest, EventResponse, RepeatState};

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

    fn send_request(&self, event: EventRequest) -> Result<EventResponse, String> {
        self.tx
            .send(event)
            .map_err(|e| format!("Failed to send request: {}", e))?;
        self.rx
            .recv()
            .map_err(|e| format!("Failed to receive response: {}", e))
    }
}

impl Player {
    pub fn play(&self) -> Result<EventResponse, String> {
        self.send_request(EventRequest::Play)
    }

    pub fn pause(&self) -> Result<EventResponse, String> {
        self.send_request(EventRequest::Pause)
    }

    pub fn stop(&self) -> Result<EventResponse, String> {
        self.send_request(EventRequest::Stop)
    }

    pub fn next(&self) -> Result<EventResponse, String> {
        self.send_request(EventRequest::Next)
    }

    pub fn prev(&self) -> Result<EventResponse, String> {
        self.send_request(EventRequest::Prev)
    }

    pub fn set_volume(&self, volume: f32) -> Result<EventResponse, String> {
        self.send_request(EventRequest::SetVolume(volume))
    }

    pub fn get_volume(&self) -> Result<EventResponse, String> {
        self.send_request(EventRequest::GetVolume)
    }

    pub fn set_repeat(&self, state: RepeatState) -> Result<EventResponse, String> {
        self.send_request(EventRequest::SetRepeat(state))
    }

    pub fn get_repeat(&self) -> Result<EventResponse, String> {
        self.send_request(EventRequest::GetRepeat)
    }

    pub fn toggle_shuffle(&self) -> Result<EventResponse, String> {
        self.send_request(EventRequest::ToggleShuffle)
    }

    pub fn new_track(&self, track: TrackObject) -> Result<EventResponse, String> {
        self.send_request(EventRequest::NewTrack(track))
    }

    pub fn clear_playlist(&self) -> Result<EventResponse, String> {
        self.send_request(EventRequest::ClearPlaylist)
    }

    pub fn remove_track(&self, index: usize) -> Result<EventResponse, String> {
        self.send_request(EventRequest::RemoveTrack(index))
    }

    pub fn terminate(&self) -> Result<EventResponse, String> {
        self.send_request(EventRequest::Terminate)
    }
}
