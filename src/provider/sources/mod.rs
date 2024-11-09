use std::collections::HashMap;
use std::fmt::Debug;
use crate::player::error::PlayerResult;
use crate::provider::error::ProviderResult;

pub(crate) mod local_file;
pub(crate) mod sine_wave;

pub type TrackInfo = HashMap<String, String>;
type TrackSourceType<T> = Box<dyn rodio::Source<Item = T> + Send + Sync>;

pub enum TrackSource {
    F32(TrackSourceType<f32>),
    I16(TrackSourceType<i16>),
    U16(TrackSourceType<u16>),
}

pub trait Track: Send + Sync {
    fn info(&self) -> PlayerResult<TrackInfo>;

    fn build_source(&self) -> PlayerResult<TrackSource>;

    fn get_unique_id(&self) -> String;

    fn try_from_config(config : HashMap<String, String>) -> ProviderResult<Self>
    where
        Self: Sized;
}

pub type TrackObject = Box<dyn Track>;

impl Debug for TrackObject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TrackObject({})", self.get_unique_id())
    }
}