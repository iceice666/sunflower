mod cmd_opt;

use tokio::net::TcpStream;

use clap::Parser;
use cmd_opt::{CmdOptions, SendMethod};
use sunflower_daemon_proto::{
    deserialize_response, serialize_request, PlayerRequest, PlayerResponse,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = CmdOptions::parse();
    let method = opt.method.clone();

    #[cfg(unix)]
    let method = method.unwrap_or(SendMethod::UnixSocket);
    #[cfg(windows)]
    let method = method.unwrap_or(SendMethod::WindowsNamedPipe);

    let request = opt.build_request();

    let response = match method {
        SendMethod::Tcp => tcp_send(request).await,

        #[cfg(unix)]
        SendMethod::UnixSocket => unix_send(request).await,

        #[cfg(windows)]
        SendMethod::WindowsNamedPipe => windows_send(request).await,
    }?;

    println!("{:?}", response);

    Ok(())
}

#[cfg(unix)]
async fn unix_send(request: PlayerRequest) -> anyhow::Result<PlayerResponse> {
    use tokio::net::UnixStream;

    let client = UnixStream::connect("/tmp/sunflower-daemon.sock").await?;
    let data = serialize_request(request);

    client.writable().await?;
    client.try_write(&data)?;

    let mut buf = Vec::new();
    client.readable().await?;
    client.try_read(&mut buf)?;
    let resp = deserialize_response(&buf)?;

    Ok(resp)
}

async fn tcp_send(request: PlayerRequest) -> anyhow::Result<PlayerResponse> {
    let client = TcpStream::connect("localhost:8888").await?;
    let data = serialize_request(request);

    client.writable().await?;
    client.try_write(&data)?;

    let mut buf = Vec::new();
    client.readable().await?;
    client.try_read(&mut buf)?;
    let resp = deserialize_response(&buf)?;

    Ok(resp)
}

#[cfg(windows)]
async fn windows_send(request: PlayerRequest) -> anyhow::Result<PlayerResponse> {
    use std::time::Duration;
    use tokio::net::windows::named_pipe::ClientOptions;
    use tokio::time;

    const ERROR_PIPE_BUSY: u32 = 231u32;

    let client = loop {
        match ClientOptions::new().open(r"\\.\pipe\sunflower-daemon") {
            Ok(client) => break client,
            Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) => (),
            Err(e) => return Err(e.into()),
        }

        time::sleep(Duration::from_millis(50)).await;
    };

    let data = serialize_request(request);

    client.writable().await?;
    client.try_write(&data)?;

    let mut buf = Vec::new();
    client.readable().await?;
    client.try_read(&mut buf)?;
    let resp = deserialize_response(&buf)?;

    Ok(resp)
}
