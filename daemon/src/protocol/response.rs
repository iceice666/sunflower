use crate::player::Repeat;
use rodio::source::SeekError;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub enum ResponseKind {
    ImAlive,

    Ok(Option<String>),
    Err(String),

    Volume(f32),
    Position(Duration),
    Total(Option<Duration>),
    Repeat(Repeat),
    Shuffled(bool),
    CurrentState {
        volume: f32,
        position: Duration,
        total: Option<Duration>,
        repeat: Repeat,
        shuffled: bool,
    },
    CurrentQueue(Vec<String>),

    TrackSearchResult(HashMap<String, HashMap<String, String>>),
    Registers(HashSet<String>),
}

pub struct Response {
    pub(crate) kind: ResponseKind,
    pub(crate) id: String,
}

impl Response {
    pub fn new(kind: ResponseKind, id: String) -> Self {
        Self { kind, id }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EventError {
    #[error("Underlying decoder returned an error or does not support seeking")]
    Seek(#[from] SeekError),
}
