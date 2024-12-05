use crate::player::Repeat;
use rodio::source::SeekError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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

    TrackSearchResult(HashMap<String, HashMap<String, String>>),
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
