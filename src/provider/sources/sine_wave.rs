use crate::player::error::PlayerResult;
use rodio::source::SineWave;
use rodio::Source;
use std::collections::HashMap;
use std::time::Duration;
use crate::provider::error::{ProviderError, ProviderResult};
use crate::provider::sources::{Track, TrackInfo, TrackSource};

pub(crate) struct SineWaveTrack {
    pub(crate) freq: f32,
    pub(crate) duration: f32,
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
