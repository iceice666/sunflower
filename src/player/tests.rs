use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use std::thread::sleep;
use std::time::Duration;

use sunflower_daemon_proto::*;
use tracing_subscriber::filter::LevelFilter;

use crate::player::_impl::Player;

#[tokio::test]
async fn test_request_and_control() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(LevelFilter::TRACE)
        .init();

    fn callback(sender: Sender<PlayerRequest>, receiver: Receiver<PlayerResponse>) {
        let send = |request: PlayerRequest| {
            sender.send(request).unwrap();
            receiver.recv().unwrap()
        };

        let track_440 = RequestPayload::TrackConfig(TrackConfig {
            provider: "sine_wave".to_string(),
            config: vec![
                ("freq".to_string(), "440.0".to_string()),
                ("duration".to_string(), "30.0".to_string()),
            ]
            .into_iter()
            .collect(),
        });
        let track_660 = RequestPayload::TrackConfig(TrackConfig {
            provider: "sine_wave".to_string(),
            config: vec![
                ("freq".to_string(), "660.0".to_string()),
                ("duration".to_string(), "30.0".to_string()),
            ]
            .into_iter()
            .collect(),
        });
        let track_880 = RequestPayload::TrackConfig(TrackConfig {
            provider: "sine_wave".to_string(),
            config: vec![
                ("freq".to_string(), "880.0".to_string()),
                ("duration".to_string(), "30.0".to_string()),
            ]
            .into_iter()
            .collect(),
        });

        send(PlayerRequest {
            r#type: RequestType::AddTrackFromConfig.into(),
            payload: Some(track_440),
        });

        send(PlayerRequest {
            r#type: RequestType::AddTrackFromConfig.into(),
            payload: Some(track_660),
        });
        sleep(Duration::from_secs(2));

        send(PlayerRequest {
            r#type: RequestType::Pause.into(),
            payload: None,
        });
        sleep(Duration::from_secs(2));

        send(PlayerRequest {
            r#type: RequestType::Play.into(),
            payload: None,
        });
        sleep(Duration::from_secs(2));

        send(PlayerRequest {
            r#type: RequestType::SetVolume.into(),
            payload: Some(RequestPayload::Data("0.3".to_string())),
        });
        sleep(Duration::from_secs(2));

        let resp = send(PlayerRequest {
            r#type: RequestType::GetVolume.into(),
            payload: None,
        });

        let payload = resp.payload.unwrap();
        assert_eq!(payload, ResponsePayload::Data("0.3".to_string()));

        send(PlayerRequest {
            r#type: RequestType::SetRepeat.into(),
            payload: Some(RequestPayload::Data("track".to_string())),
        });

        send(PlayerRequest {
            r#type: RequestType::Next.into(),
            payload: None,
        });
        sleep(Duration::from_secs(5));

        send(PlayerRequest {
            r#type: RequestType::SetRepeat.into(),
            payload: Some(RequestPayload::Data("none".to_string())),
        });

        send(PlayerRequest {
            r#type: RequestType::Next.into(),
            payload: None,
        });
        sleep(Duration::from_secs(5));

        send(PlayerRequest {
            r#type: RequestType::Prev.into(),
            payload: None,
        });
        sleep(Duration::from_secs(5));

        send(PlayerRequest {
            r#type: RequestType::Stop.into(),
            payload: None,
        });

        send(PlayerRequest {
            r#type: RequestType::AddTrackFromConfig.into(),
            payload: Some(track_880),
        });
        sleep(Duration::from_secs(5));
        send(PlayerRequest {
            r#type: RequestType::Terminate.into(),
            payload: None,
        });
    }

    let (player, sender, receiver) = Player::try_new()?;

    let handle = thread::spawn(|| callback(sender, receiver));

    player.main_loop().await;

    handle.join().unwrap();

    Ok(())
}
