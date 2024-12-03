use crate::source::error::SourceResult;
use crate::source::{RawAudioSource, SourceInfo, SourceTrait};
use rodio::source::SineWave;
use rodio::Source;
use std::collections::HashMap;
use std::time::Duration;

#[derive(Debug, PartialEq)]
pub struct SineWaveTrack {
    pub freq: f32,
    pub duration: f32,
}

impl SourceTrait for SineWaveTrack {
    fn info(&self) -> SourceResult<SourceInfo> {
        let mut result = HashMap::new();
        result.insert("freq".to_string(), self.freq.to_string());
        result.insert("duration".to_string(), self.duration.to_string());
        Ok(result)
    }

    fn build_source(&self) -> SourceResult<RawAudioSource> {
        let source = SineWave::new(self.freq)
            .take_duration(Duration::from_secs_f32(self.duration))
            .amplify(0.20);
        Ok(RawAudioSource::F32(Box::new(source)))
    }

    fn get_unique_id(&self) -> String {
        format!("sinewave_{}hz_{}sec", self.freq, self.duration)
    }
}
