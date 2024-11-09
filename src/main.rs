use std::sync::mpsc::{Receiver, Sender};
use sunflower_daemon::Player;
use sunflower_daemon_proto::{PlayerRequest, PlayerResponse};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let (player, sender, receiver) = Player::try_new()?;

    // Create a local task set that can handle non-Send futures
    let local = tokio::task::LocalSet::new();

    local
        .run_until(async move {
            let player_handle = tokio::task::spawn_local(player.main_loop());
            let message_handle = tokio::spawn(message_transfer(sender, receiver));

            let (_, handle) = tokio::join!(player_handle, message_handle);
            handle?;

            Ok(())
        })
        .await
}

#[cfg(feature = "daemon-tcp")]
async fn message_transfer(sender: Sender<PlayerRequest>, receiver: Receiver<PlayerResponse>) {
    use tokio::net::TcpListener;

    todo!()
}

#[cfg(windows)]
async fn message_transfer(sender: Sender<PlayerRequest>, receiver: Receiver<PlayerResponse>) {
    use tokio::net::windows::named_pipe::NamedPipeClient;

    todo!()
}

#[cfg(unix)]
async fn message_transfer(sender: Sender<PlayerRequest>, receiver: Receiver<PlayerResponse>) {
    use tokio::net::UnixListener;


    todo!()
}
