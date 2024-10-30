use std::time::Duration;
use std::{borrow::Borrow, collections::HashMap};

use rodio::{source::SineWave, Source};
use sunflower_player::error::PlayerResult;
use sunflower_player::{Track, TrackInfo, TrackObject, TrackSource};

use crate::{Provider, ProviderResult};

pub struct SineWaveProvider;

impl Provider for SineWaveProvider {
    fn get_name(&self) -> String {
        "SineWaveProvider".to_string()
    }

    fn search(
        &mut self,
        _: impl AsRef<str>,
    ) -> ProviderResult<impl Borrow<HashMap<String, String>> + '_> {
        Ok(HashMap::new())
    }

    fn get_track(&self, duration: impl AsRef<str>) -> ProviderResult<TrackObject> {
        let duration = duration.as_ref().parse().unwrap();
        Ok(Box::new(SineWaveTrack { duration }))
    }
}

pub(crate) struct SineWaveTrack {
    duration: f32,
}

impl Track for SineWaveTrack {
    fn info(&self) -> PlayerResult<TrackInfo> {
        Ok(HashMap::new())
    }

    fn build_source(&self) -> PlayerResult<TrackSource> {
        let source = SineWave::new(440.0)
            .take_duration(Duration::from_secs_f32(self.duration))
            .amplify(0.20);


        Ok(TrackSource::F32(Box::new(source)))
    }

    fn get_unique_id(&self) -> String {
        format!("SineWave with {} secs", self.duration)
    }
}
