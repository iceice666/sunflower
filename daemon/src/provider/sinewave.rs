use crate::provider::error::ProviderError;
use crate::provider::ProviderResult;
use crate::provider::{ProviderTrait, SearchResult};
use crate::source::sinewave::SineWaveTrack;
use crate::source::SourceKinds;
use std::collections::HashMap;

#[derive(PartialEq, Eq, Default, Debug)]
pub struct SineWaveProvider;

impl From<HashMap<String, String>> for SineWaveProvider {
    fn from(_: HashMap<String, String>) -> Self {
        SineWaveProvider
    }
}

impl ProviderTrait for SineWaveProvider {
    fn get_name(&self) -> String {
        "SineWaveProvider".to_string()
    }

    fn search(&mut self, _: &str) -> SearchResult {
        Err(ProviderError::EmptySearchResult)
    }

    fn get_track(&self, input: &str) -> ProviderResult<SourceKinds> {
        let (freq, duration) = input.split_once('+').ok_or(ProviderError::TrackNotFound(
            "SineWaveProvider: input should be in format of 'freq+duration'".into(),
        ))?;
        let freq = freq.parse().map_err(|_| {
            ProviderError::TrackNotFound("SineWaveProvider: freq should be a number".into())
        })?;
        let duration = duration.parse().map_err(|_| {
            ProviderError::TrackNotFound("SineWaveProvider: duration should be a number".into())
        })?;
        Ok(SourceKinds::Sinwave(SineWaveTrack { freq, duration }))
    }
}
