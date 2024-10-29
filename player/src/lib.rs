mod _impl;
pub mod error;

use std::{collections::HashMap, fmt::Debug};

pub use _impl::Player;
use error::PlayerResult;

pub type TrackInfo = HashMap<String, String>;
pub type TrackSource = Box<dyn rodio::Source<Item = f32> + Send + Sync>;

pub trait Track: Send + Sync {
    fn info(&self) -> PlayerResult<TrackInfo>;

    fn build_source(&self) -> PlayerResult<TrackSource>;

    fn get_unique_id(&self) -> String;
}

pub type TrackObject = Box<dyn Track>;

impl Debug for TrackObject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrackObject")
            .field("info", &self.info())
            .field("unique_id", &self.get_unique_id())
            .finish()
    }
}
