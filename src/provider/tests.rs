use std::collections::HashMap;
use sunflower_daemon_proto::{
    PlayerRequest, PlayerResponse, RequestPayload, RequestType, TrackData,
};
use tracing::level_filters::LevelFilter;

use crate::provider::Provider;
use crate::{player::_impl::Player, provider::providers::local_file::LocalFileProvider};

use std::sync::mpsc::{Receiver, Sender};

#[tokio::test]
async fn test_search_and_play_with_local_file_provider() -> anyhow::Result<()> {
    const SEARCH_REGEX: &str = r".*\.mp3$";
    tracing_subscriber::fmt()
        .with_max_level(LevelFilter::DEBUG)
        .init();

    async fn callback(sender: Sender<PlayerRequest>, _: Receiver<PlayerResponse>) {
        let list: Vec<String>;
        let mut provider = LocalFileProvider::new("../music");

        {
            let search_result = provider.search(SEARCH_REGEX).await.unwrap();
            let search_result: &HashMap<String, String> = search_result;

            println!("==== Search Result ====");
            for (key, value) in search_result {
                println!("{}: {}", key, value);
            }
            println!();

            list = search_result.keys().map(|s| s.to_string()).collect();
        }

        for file_name in list {
            let request = PlayerRequest {
                r#type: RequestType::AddTrack.into(),
                payload: Some(RequestPayload::Track(TrackData {
                    provider: provider.get_name().await,
                    id: file_name,
                })),
            };

            sender.send(request).unwrap();
        }
    }

    let (player, sender, receiver) = Player::try_new()?;
    let handle = tokio::spawn(async { callback(sender, receiver).await });

    player.main_loop().await;

    handle.await?;

    Ok(())
}
