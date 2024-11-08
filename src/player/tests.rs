use crate::player::_impl::{PlayerRequest, PlayerResponse, RepeatState};
use crate::player::error::PlayerResult;
use crate::player::play;
use crate::player::track::{Track, TrackInfo, TrackSource};
use rodio::source::SineWave;
use rodio::Source;
use std::thread::sleep;
use std::time::Duration;
use tracing_subscriber::filter::LevelFilter;

struct SineWaveTestTrack {
    freq: f32,
    duration: f32,
}

impl Track for SineWaveTestTrack {
    fn info(&self) -> PlayerResult<TrackInfo> {
        unreachable!()
    }

    fn build_source(&self) -> PlayerResult<TrackSource> {
        Ok(TrackSource::F32(Box::new(
            SineWave::new(self.freq)
                .take_duration(Duration::from_secs_f32(self.duration))
                .amplify(0.20),
        )))
    }

    fn get_unique_id(&self) -> String {
        format!("SineWave {} hz {} secs", self.freq, self.duration)
    }
}

#[tokio::test]
async fn test_request_and_control() -> anyhow::Result<()> {
    unimplemented!("This test is not done yet.");
    play(|sender, receiver| {
        tracing_subscriber::fmt()
            .with_max_level(LevelFilter::TRACE)
            .init();

        let track = Box::new(SineWaveTestTrack {
            freq: 440.0,
            duration: 30.0,
        });
        sender.send(PlayerRequest::AddTrack(track)).unwrap();
        let track = Box::new(SineWaveTestTrack {
            freq: 660.0,
            duration: 30.0,
        });
        sender.send(PlayerRequest::AddTrack(track)).unwrap();
        sleep(Duration::from_secs(2));

        sender.send(PlayerRequest::Pause).unwrap();
        sleep(Duration::from_secs(2));

        sender.send(PlayerRequest::Play).unwrap();
        sleep(Duration::from_secs(2));

        sender.send(PlayerRequest::SetVolume(0.3)).unwrap();
        sleep(Duration::from_secs(2));

        sender.send(PlayerRequest::GetVolume).unwrap();
        let vol = receiver.recv().unwrap();
        assert_eq!(vol, PlayerResponse::Volume(0.3));

        sender
            .send(PlayerRequest::SetRepeat(RepeatState::Track))
            .unwrap();
        sender.send(PlayerRequest::Next).unwrap();
        sleep(Duration::from_secs(5));

        sender
            .send(PlayerRequest::SetRepeat(RepeatState::None))
            .unwrap();

        sender.send(PlayerRequest::Next).unwrap();
        sleep(Duration::from_secs(5));

        sender.send(PlayerRequest::Prev).unwrap();
        sleep(Duration::from_secs(5));

        sender.send(PlayerRequest::Stop).unwrap();

        let track = Box::new(SineWaveTestTrack {
            freq: 880.0,
            duration: 30.0,
        });
        sender.send(PlayerRequest::AddTrack(track)).unwrap();
        sleep(Duration::from_secs(5));
        sender.send(PlayerRequest::Terminate).unwrap();
    }).await?
    .join()
    .unwrap();

    Ok(())
}
