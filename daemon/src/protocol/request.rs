use crate::player::Repeat;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub enum PlayerRequest {
    // Player related
    Play,
    Stop,
    Next,
    Prev,
    Pause,

    GetVolume,
    SetVolume(f32),

    GetPos,
    GetTotalDuration,

    JumpTo(Duration),
}

#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub enum PlayerStateRequest {
    // Player state related
    GetRepeat,
    SetRepeat(Repeat),
    GetShuffle,
    ToggleShuffle,
}

#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub enum Request {
    Player(PlayerRequest),
    State(PlayerStateRequest),
}
