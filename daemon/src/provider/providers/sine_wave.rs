use crate::provider::error::ProviderError;
use crate::provider::providers::JUST_A_EMPTY_HASHMAP;
use crate::provider::sources::sine_wave::SineWaveTrack;
use crate::provider::sources::TrackObject;
use crate::provider::SearchResult;
use crate::provider::{Provider, ProviderResult};
use std::collections::HashMap;

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
