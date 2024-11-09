use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::Duration;

use crate::player::error::PlayerResult;
use crate::player::track::{Track, TrackInfo, TrackObject, TrackSource};
use rodio::{source::SineWave, Source};

use crate::provider::error::ProviderError;
use crate::provider::SearchResult;
use crate::provider::{Provider, ProviderResult};

static JUST_A_EMPTY_HASHMAP: LazyLock<HashMap<String, String>> = LazyLock::new(HashMap::new);

#[derive(PartialEq, Eq, Default)]
pub struct SineWaveProvider;

impl From<HashMap<String, String>> for SineWaveProvider {
    fn from(_: HashMap<String, String>) -> Self {
        SineWaveProvider
    }
}

#[async_trait::async_trait]
impl Provider for SineWaveProvider {
    async fn get_name(&self) -> String {
        "SineWaveProvider".to_string()
    }

    async fn search(&mut self, _: &str) -> SearchResult {
        Ok(&JUST_A_EMPTY_HASHMAP)
    }

    async fn get_track(&self, input: &str) -> ProviderResult<TrackObject> {
        let (freq, duration) = input.split_once('+').ok_or(ProviderError::TrackNotFound(
            "SineWaveProvider: input should be in format of 'freq+duration'".into(),
        ))?;
        let freq = freq.parse().map_err(|_| {
            ProviderError::TrackNotFound("SineWaveProvider: freq should be a number".into())
        })?;
        let duration = duration.parse().map_err(|_| {
            ProviderError::TrackNotFound("SineWaveProvider: duration should be a number".into())
        })?;
        Ok(Box::new(SineWaveTrack { freq, duration }))
    }
}

pub(crate) struct SineWaveTrack {
    freq: f32,
    duration: f32,
}

impl Track for SineWaveTrack {
    fn info(&self) -> PlayerResult<TrackInfo> {
        Ok(HashMap::new())
    }

    fn build_source(&self) -> PlayerResult<TrackSource> {
        let source = SineWave::new(self.freq)
            .take_duration(Duration::from_secs_f32(self.duration))
            .amplify(0.20);

        Ok(TrackSource::F32(Box::new(source)))
    }

    fn get_unique_id(&self) -> String {
        format!("SineWave {} hz {} secs", self.freq, self.duration)
    }
}
