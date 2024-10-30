use std::{borrow::Borrow, collections::HashMap};

use crate::{
    sources::{LocalFileProvider, SineWaveProvider},
    Provider,
};
use sunflower_player::{Player, TrackObject};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn play(tracks: Vec<TrackObject>) -> Result {
    let mut player = Player::try_new()?;
    for track in tracks {
        player.add_track(track);
    }

    // This block current thread
    player.mainloop()?;

    Ok(())
}

#[test]
fn sinewave() -> Result {
    let provider = SineWaveProvider;
    let track = provider.get_track("10")?;

    play(vec![track])
}

#[test]
fn local() -> Result {
    const SEARCH_REGEX: &str = "";

    let list: Vec<String>;
    let mut provider = LocalFileProvider::new("../music");

    {
        let search_result = provider.search(SEARCH_REGEX)?;
        let search_result: &HashMap<String, String> = search_result.borrow();

        println!("==== Search Result ====");
        for (key, value) in search_result {
            println!("{}: {}", key, value);
        }
        println!();

        list = search_result.keys().map(|s| s.to_string()).collect();
    }

    let mut playlist = Vec::new();
    for file_name in list {
        let track = provider.get_track(file_name)?;
        playlist.push(track);
    }

    play(playlist)
}
