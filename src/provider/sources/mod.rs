// HINT: $PROVIDER_IMPL$: Remember adding others provider/track implementations here
pub(crate) mod sine_wave;

#[cfg(feature = "provider-local_file")]
pub(crate) mod local_file;

////////////////////////////////////////////////////////////////////////

use crate::player::error::PlayerResult;
use crate::provider::error::{ProviderError, ProviderResult};
use crate::provider::sources::local_file::LocalFileTrack;
use crate::provider::sources::sine_wave::SineWaveTrack;
use std::collections::HashMap;
use std::fmt::Debug;
use sunflower_daemon_proto::TrackConfig;

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

    fn try_from_config(config: HashMap<String, String>) -> ProviderResult<Self>
    where
        Self: Sized;
}

pub type TrackObject = Box<dyn Track>;

impl Debug for TrackObject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TrackObject({})", self.get_unique_id())
    }
}

pub fn try_from_config(config: TrackConfig) -> ProviderResult<TrackObject> {
    let provider = config.provider;
    let config = config.config;

    let obj: TrackObject = match provider.as_str() {
        // HINT: $PROVIDER_IMPL$: Remember adding others provider/track implementations here
        "sine_wave" => Box::new(SineWaveTrack::try_from_config(config)?),

        #[cfg(feature = "provider-local_file")]
        "local_file" => Box::new(LocalFileTrack::try_from_config(config)?),

        _ => return Err(ProviderError::ProviderNotFound(provider)),
    };

    Ok(obj)
}
