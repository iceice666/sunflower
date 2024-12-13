use crate::player::Repeat;
use crate::provider::ProviderFields;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::Duration;
use uuid::Uuid;

#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub enum PlayerRequest {
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
    GetRepeat,
    SetRepeat(Repeat),
    GetShuffle,
    ToggleShuffle,
    GetAllState,
}

#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub enum TrackRequest {
    AddTrack {
        provider_name: String,
        track_id: String,
    },
    RemoveTrack {
        idx: usize,
    },
    ClearQueue,
    GetQueue,
}

#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub enum ProviderRequest {
    Register(ProviderFields),
    Unregister(String),
    SearchTracks {
        providers: HashSet<String>,
        max_results: Option<usize>,
        query: String,
    },
    GetRegistered,
}

#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub enum RequestKind {
    AreYouAlive,
    Terminate,
    Player(PlayerRequest),
    State(PlayerStateRequest),
    Track(TrackRequest),
    Provider(ProviderRequest),
}

pub struct Request {
    pub kind: RequestKind,
    pub id: String,
}

impl Request {
    pub fn id(&self) -> &str {
        self.id.as_str()
    }
}

impl From<RequestKind> for Request {
    fn from(value: RequestKind) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            kind: value,
        }
    }
}
