use std::collections::HashMap;

use crate::{sources::LocalFileProvider, Provider};
use sunflower_player::{play, EventRequest};
use tracing::level_filters::LevelFilter;

#[test]
fn test_search_and_play_with_local_file_provider() -> anyhow::Result<()> {
    const SEARCH_REGEX: &str = r".*\.mp3$";
    tracing_subscriber::fmt()
        .with_max_level(LevelFilter::DEBUG)
        .init();

    play(|sender, _| {
        let list: Vec<String>;
        let mut provider = LocalFileProvider::new("../music");

        {
            let search_result = provider.search(SEARCH_REGEX).unwrap();
            let search_result: &HashMap<String, String> = search_result;

            println!("==== Search Result ====");
            for (key, value) in search_result {
                println!("{}: {}", key, value);
            }
            println!();

            list = search_result.keys().map(|s| s.to_string()).collect();
        }

        for file_name in list {
            let track = provider.get_track(&file_name).unwrap();
            sender.send(EventRequest::AddTrack(track)).unwrap();
        }
    })?
    .join()
    .unwrap();

    Ok(())
}
