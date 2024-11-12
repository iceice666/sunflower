use std::io;

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use sunflower_daemon::Player;
use sunflower_daemon_proto::{DecodeError, PlayerRequest, PlayerResponse};
use thiserror::Error;

#[cfg(all(windows, not(feature = "daemon-tcp")))]
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};

#[cfg(feature = "daemon-tcp")]
use tokio::net::{TcpListener, TcpStream};

#[cfg(all(unix, not(feature = "daemon-tcp")))]
use tokio::net::{UnixListener, UnixStream};
use tracing::{info, level_filters::LevelFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(LevelFilter::DEBUG)
        .init();

    let (player, sender, receiver) = Player::try_new()?;

    // Create a local task set that can handle non-Send futures
    let local = tokio::task::LocalSet::new();

    local
        .run_until(async move {
            let player_handle = tokio::task::spawn_local(player.main_loop());
            let message_handle = tokio::spawn(message_transfer(sender, receiver));

            let (_, handle) = tokio::join!(player_handle, message_handle);
            handle??;

            Ok(())
        })
        .await
}

#[derive(Debug, Error)]
enum LoopCtrl {
    #[error("")]
    Break,
    #[error("")]
    Continue,

    #[error("")]
    IoError(#[from] io::Error),

    #[error("")]
    DecodeError(#[from] DecodeError),

    #[error("")]
    SendError(#[from] tokio::sync::mpsc::error::SendError<PlayerRequest>),
}

async fn exchange(
    sender: &UnboundedSender<PlayerRequest>,
    receiver: &mut UnboundedReceiver<PlayerResponse>,

    #[cfg(feature = "daemon-tcp")] socket: TcpStream,
    #[cfg(all(windows, not(feature = "daemon-tcp")))] socket: NamedPipeServer,
    #[cfg(all(unix, not(feature = "daemon-tcp")))] socket: UnixStream,
) -> Result<(), LoopCtrl> {
    let mut buf = [0; 1024];

    socket.writable().await?;

    match socket.try_read(&mut buf) {
        Ok(0) => return Err(LoopCtrl::Break),
        Ok(n) => {
            let req = sunflower_daemon_proto::deserialize_request(&buf[..n])?;
            sender.send(req)?;

            let Some(resp) = receiver.recv().await else {
                return Err(LoopCtrl::Continue);
            };
            let resp_buf = sunflower_daemon_proto::serialize_response(resp);

            socket.writable().await?;
            socket.try_write(&resp_buf)?;
        }
        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
            return Err(LoopCtrl::Continue);
        }
        Err(e) => {
            return Err(LoopCtrl::IoError(e));
        }
    };

    Ok(())
}

#[allow(dead_code)]
async fn message_transfer(
    sender: UnboundedSender<PlayerRequest>,
    mut receiver: UnboundedReceiver<PlayerResponse>,
) -> anyhow::Result<()> {
    const LISTENING_URL: &str = "localhost:8888";
    const PIPE_NAME: &str = r"\\.\pipe\sunflower-daemon";
    const UNIX_SOCKET_PATH: &str = "/tmp/sunflower-daemon.sock";

    #[cfg(feature = "daemon-tcp")]
    let listener = TcpListener::bind(LISTENING_URL).await?;

    #[cfg(all(windows, not(feature = "daemon-tcp")))]
    let mut server = ServerOptions::new().create(PIPE_NAME)?;

    #[cfg(all(unix, not(feature = "daemon-tcp")))]
    let listener = UnixListener::bind(UNIX_SOCKET_PATH)?;

    #[cfg(feature = "daemon-tcp")]
    info!(
        "Starting accepting connections with TCP at {}...",
        LISTENING_URL
    );

    #[cfg(all(windows, not(feature = "daemon-tcp")))]
    info!(
        "Starting accepting connections with Windows named pipe at {}...",
        PIPE_NAME
    );

    #[cfg(all(unix, not(feature = "daemon-tcp")))]
    info!(
        "Starting accepting connections with unix socket at {}...",
        UNIX_SOCKET_PATH
    );
    loop {
        #[cfg(any(feature = "daemon-tcp", unix))]
        let socket = {
            let (socket, _) = listener.accept().await?;
            socket
        };

        #[cfg(all(windows, not(feature = "daemon-tcp")))]
        let socket = {
            server.connect().await?;
            let connected_client = server;

            server = ServerOptions::new().create(PIPE_NAME)?;
            connected_client
        };

        let Err(e) = exchange(&sender, &mut receiver, socket).await else {
            continue;
        };

        match e {
            LoopCtrl::Continue => continue,
            LoopCtrl::Break => break,
            _ => return Err(e.into()),
        }
    }
    Ok(())
}
