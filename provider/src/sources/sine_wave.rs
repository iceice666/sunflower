use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::Duration;

use rodio::{source::SineWave, Source};
use sunflower_player::error::PlayerResult;
use sunflower_player::track::{Track, TrackInfo, TrackObject, TrackSource};

use crate::error::ProviderError;
use crate::SearchResult;
use crate::{Provider, ProviderResult};

static JUST_A_EMPTY_HASHMAP: LazyLock<HashMap<String, String>> = LazyLock::new(HashMap::new);

#[derive(PartialEq, Eq)]
pub struct SineWaveProvider;

impl Provider for SineWaveProvider {
    fn get_name(&self) -> String {
        "SineWaveProvider".to_string()
    }

    fn search(&mut self, _: impl AsRef<str>) -> SearchResult {
        Ok(&JUST_A_EMPTY_HASHMAP)
    }

    fn get_track(&self, input: impl AsRef<str>) -> ProviderResult<TrackObject> {
        let (freq, duration) =
            input
                .as_ref()
                .split_once('+')
                .ok_or(ProviderError::TrackNotFound(
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
