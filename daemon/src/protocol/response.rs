use crate::player::Repeat;
use rodio::source::SeekError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub enum Response {
    Ok(Option<String>),
    Err(String),

    Volume(f32),
    Position(Duration),
    Total(Option<Duration>),
    Repeat(Repeat),
    Shuffled(bool),

    TrackSearchResult(HashMap<String, HashMap<String, String>>),
}

#[derive(Debug, thiserror::Error)]
pub enum EventError {
    #[error("Underlying decoder returned an error or does not support seeking")]
    Seek(#[from] SeekError),
}
