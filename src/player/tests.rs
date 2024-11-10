use tokio::{task::JoinError, time::sleep};
use std::time::Duration;

use sunflower_daemon_proto::*;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tracing_subscriber::filter::LevelFilter;

use crate::player::_impl::Player;

#[tokio::test]
async fn test_request_and_control() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(LevelFilter::TRACE)
        .init();

    async fn callback(sender: UnboundedSender<PlayerRequest>,mut receiver: UnboundedReceiver<PlayerResponse>) {
        
        macro_rules! send {
            ($request:expr) => {{
                sender.send($request).unwrap();
                receiver.recv().await.unwrap()
            }};
            
        }

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

        send!(PlayerRequest {
            r#type: RequestType::SetRepeat.into(),
            payload: Some(RequestPayload::Data("track".to_string())),
        });
        
        send!(PlayerRequest {
            r#type: RequestType::AddTrackFromConfig.into(),
            payload: Some(track_440),
        });

        send!(PlayerRequest {
            r#type: RequestType::AddTrackFromConfig.into(),
            payload: Some(track_660),
        });
        sleep(Duration::from_secs(2)).await;

        send!(PlayerRequest {
            r#type: RequestType::Pause.into(),
            payload: None,
        });
        sleep(Duration::from_secs(2)).await;

        send!(PlayerRequest {
            r#type: RequestType::Play.into(),
            payload: None,
        });
        sleep(Duration::from_secs(2)).await;

        send!(PlayerRequest {
            r#type: RequestType::SetVolume.into(),
            payload: Some(RequestPayload::Data("0.3".to_string())),
        });
        sleep(Duration::from_secs(2)).await;

        let resp = send!(PlayerRequest {
            r#type: RequestType::GetVolume.into(),
            payload: None,
        });

        let payload = resp.payload.unwrap();
        assert_eq!(payload, ResponsePayload::Data("0.3".to_string()));
        

        send!(PlayerRequest {
            r#type: RequestType::Next.into(),
            payload: None,
        });
        sleep(Duration::from_secs(5)).await;

        send!(PlayerRequest {
            r#type: RequestType::SetRepeat.into(),
            payload: Some(RequestPayload::Data("none".to_string())),
        });

        send!(PlayerRequest {
            r#type: RequestType::Next.into(),
            payload: None,
        });
        sleep(Duration::from_secs(5)).await;

        send!(PlayerRequest {
            r#type: RequestType::Prev.into(),
            payload: None,
        });
        sleep(Duration::from_secs(5)).await;

        send!(PlayerRequest {
            r#type: RequestType::Stop.into(),
            payload: None,
        });

        send!(PlayerRequest {
            r#type: RequestType::AddTrackFromConfig.into(),
            payload: Some(track_880),
        });
        sleep(Duration::from_secs(5)).await;
        send!(PlayerRequest {
            r#type: RequestType::Terminate.into(),
            payload: None,
        });
    }

    let (player, sender, receiver) = Player::try_new()?;

    let local = tokio::task::LocalSet::new();

    
    local
        .run_until(async move {
            let player_handle = tokio::task::spawn_local(player.main_loop());
            let message_handle = tokio::spawn(callback(sender, receiver));

            let (_, handle) = tokio::join!(player_handle, message_handle);
            handle?;

            Ok::<(),JoinError>(())
        })
        .await?;

    Ok(())
}
