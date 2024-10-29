use sunflower_player::Player;
use sunflower_provider::{sources::SineWaveProvider, Provider};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut player = Player::try_new()?;
    let sinewave_generator = SineWaveProvider;
    let sinewave_track = sinewave_generator.get_track("30")?;
    player.add_track(sinewave_track);

    // This block current thread
    player.mainloop()?;

    Ok(())
}
