use std::{
    borrow::Borrow,
    collections::HashMap,
    sync::mpsc::Sender,
    thread::{self, sleep, JoinHandle},
    time::Duration,
};

use crate::{
    sources::{LocalFileProvider, SineWaveProvider},
    Provider,
};
use sunflower_player::{EventRequest, Player, RepeatState};
use tracing::level_filters::LevelFilter;

fn play(
    callback: impl Send + 'static + FnOnce(Sender<EventRequest>),
) -> anyhow::Result<JoinHandle<()>> {
    tracing_subscriber::fmt()
        .with_max_level(LevelFilter::TRACE)
        .init();

    let (mut player, sender, _) = Player::try_new()?;

    let handle = thread::spawn(|| callback(sender));

    // This block current thread
    player.mainloop();

    Ok(handle)
}

#[test]
fn test_append_track_after_previous_track_ends() -> anyhow::Result<()> {
    let handle = play(|sender| {
        let provider = SineWaveProvider;
        let tracks = vec![
            provider.get_track("440+3").unwrap(),
            provider.get_track("880+3").unwrap(),
        ];

        for track in tracks {
            let request = EventRequest::NewTrack(track);
            sender.send(request).unwrap();
        }

        thread::sleep(Duration::from_secs(10));

        let track3 = provider.get_track("440+3").unwrap();
        let request = EventRequest::NewTrack(track3);
        sender.send(request).unwrap();
    })?;

    handle.join().unwrap();
    Ok(())
}

#[test]
fn test_search_and_play_with_local_file_provider() -> anyhow::Result<()> {
    const SEARCH_REGEX: &str = r".*\.mp3$";

    play(|sender| {
        let list: Vec<String>;
        let mut provider = LocalFileProvider::new("../music");

        {
            let search_result = provider.search(SEARCH_REGEX).unwrap();
            let search_result: &HashMap<String, String> = search_result.borrow();

            println!("==== Search Result ====");
            for (key, value) in search_result {
                println!("{}: {}", key, value);
            }
            println!();

            list = search_result.keys().map(|s| s.to_string()).collect();
        }

        for file_name in list {
            let track = provider.get_track(file_name).unwrap();
            sender.send(EventRequest::NewTrack(track)).unwrap();
        }
    })?
    .join()
    .unwrap();

    Ok(())
}

#[test]
fn test_request() -> anyhow::Result<()> {
    play(|sender| {
        let provider = SineWaveProvider;
        let track = provider.get_track("440+30").unwrap();
        sender.send(EventRequest::NewTrack(track)).unwrap();
        sleep(Duration::from_secs(2));
        sender.send(EventRequest::Pause).unwrap();
        sleep(Duration::from_secs(2));
        sender.send(EventRequest::Play).unwrap();
        sleep(Duration::from_secs(2));
        sender.send(EventRequest::SetVolume(0.1)).unwrap();
        sleep(Duration::from_secs(2));
        sender.send(EventRequest::SetVolume(0.5)).unwrap();
        sleep(Duration::from_secs(2));
        sender
            .send(EventRequest::SetRepeat(RepeatState::Track))
            .unwrap();
        sender.send(EventRequest::Next).unwrap();
        sleep(Duration::from_secs(5));
        sender.send(EventRequest::Prev).unwrap();
        sleep(Duration::from_secs(5));
        sender.send(EventRequest::Stop).unwrap();
    })?
    .join()
    .unwrap();

    Ok(())
}
