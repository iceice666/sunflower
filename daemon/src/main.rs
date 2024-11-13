#[cfg(all(windows, not(feature = "daemon-tcp")))]
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};

#[cfg(feature = "daemon-tcp")]
use tokio::net::{TcpListener, TcpStream};

#[cfg(all(unix, not(feature = "daemon-tcp")))]
use tokio::net::{UnixListener, UnixStream};

use std::io;

use anyhow::anyhow;
use time::UtcOffset;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use sunflower_daemon::Player;
use sunflower_daemon_proto::{PlayerRequest, PlayerResponse};

use tracing::{debug, error, info, level_filters::LevelFilter};
use tracing_subscriber::fmt;

fn init_logger() {
    let timer = time::format_description::parse(
        "[year]-[month padding:zero]-[day padding:zero] [hour]:[minute]:[second]",
    )
    .expect("Cataplum");
    let time_offset = UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC);
    let timer = fmt::time::OffsetTime::new(time_offset, timer);

    tracing_subscriber::fmt()
        .with_max_level(LevelFilter::DEBUG)
        .with_timer(timer)
        .init();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_logger();

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

        match exchange(&sender, &mut receiver, socket).await {
            Ok(v) => v,
            Err(e) => error!("Error occurred during message exchange: {}", e),
        }
    }
}

async fn exchange(
    sender: &UnboundedSender<PlayerRequest>,
    receiver: &mut UnboundedReceiver<PlayerResponse>,

    #[cfg(feature = "daemon-tcp")] socket: TcpStream,
    #[cfg(all(windows, not(feature = "daemon-tcp")))] socket: NamedPipeServer,
    #[cfg(all(unix, not(feature = "daemon-tcp")))] socket: UnixStream,
) -> anyhow::Result<()> {
    let buf = {
        let mut buf = [0u8; 1024];
        loop {
            debug!("Waiting for data from client...");
            socket.readable().await?;
            debug!("Data received, reading...");
            match socket.try_read(&mut buf) {
                Ok(0) => return Err(anyhow!("Connection closed by peer")),
                Ok(n) => break Vec::from(&buf[..n]),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                Err(e) => return Err(e.into()),
            }
        }
    };

    let req = sunflower_daemon_proto::deserialize_request(&buf)?;
    sender.send(req)?;

    let Some(resp) = receiver.recv().await else {
        return Ok(());
    };
    let buf = sunflower_daemon_proto::serialize_response(resp);

    loop {
        socket.writable().await?;
        match socket.try_write(&buf) {
            Ok(_) => break,
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
            Err(e) => return Err(e.into()),
        }
    }

    Ok(())
}
