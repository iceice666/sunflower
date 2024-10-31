use std::sync::mpsc::{Receiver, Sender};
use crate::error::PlayerResult;
use crate::_impl::Player as RawPlayer;

pub use crate::_impl::{EventRequest, EventResponse, RepeatState};

pub struct Player {
    base: RawPlayer,
    tx: Sender<EventRequest>,
    rx: Receiver<EventResponse>,
}

impl Player {
    pub fn try_new() -> PlayerResult<Self> {
        let (base,tx,rx) = RawPlayer::try_new()?;
        Ok(Self {base,tx, rx})
    }
}