mod cmd_opt;

use anyhow::anyhow;
use clap::Parser;
use cmd_opt::{CmdOptions, SendMethod};
use std::fmt::Write;
use std::io;
use sunflower_daemon_proto::{
    deserialize_response, serialize_request, PlayerRequest, PlayerResponse, ProviderList,
    RepeatState, ResponsePayload, ResponseType, SearchResults,
};
use tokio::net::TcpStream;

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

    match ResponseType::try_from(response.r#type)? {
        ResponseType::Ok => {
            if let Some(data) = response.payload {
                match data {
                    ResponsePayload::Data(msg) => println!("{}", msg),
                    _ => unreachable!(),
                }
            } else {
                println!("OK");
            }
        }
        ResponseType::Error => {
            let ResponsePayload::Error(error) = response.payload.unwrap() else {
                return Err(anyhow!("Error: Invalid response payload"));
            };
            eprintln!("Error: {}", error);
        }
        ResponseType::ImAlive => println!("Server is alive"),
        ResponseType::HiImYajyuSenpai => {
            let ResponsePayload::Data(msg) = response.payload.unwrap() else {
                return Err(anyhow!("Error: Invalid response payload"));
            };
            println!("{}", msg);
        }
        ResponseType::Providers => {
            let ResponsePayload::ProviderList(ProviderList { providers }) =
                response.payload.unwrap()
            else {
                return Err(anyhow!("Error: Invalid response payload"));
            };
            println!("{}", providers.join("\n"));
        }
        ResponseType::TrackData => println!("Track data: {:?}", response.payload.unwrap()),
        ResponseType::SearchResult => {
            let ResponsePayload::SearchResults(SearchResults { results }) =
                response.payload.unwrap()
            else {
                return Err(anyhow!("Error: Invalid response payload"));
            };

            results.iter().for_each(|(provider, map)| {
                println!("{}:", provider);
                map.values.iter().for_each(|(id, name)| {
                    println!("    {}: {}", id, name);
                });
            });
        }
        ResponseType::PlayerStatus => {
            let ResponsePayload::PlayerStatus(state) = response.payload.unwrap() else {
                return Err(anyhow!("Error: Invalid response payload"));
            };

            // Find maximum length
            let (queue, max_length) = state.queue.iter().fold(
                (Vec::with_capacity(state.queue.len()), 0),
                |(mut vec, max_len), id| {
                    vec.push(id.clone());
                    (vec, max_len.max(id.len()))
                },
            );
            let index = state.current as usize;

            // Calculate dimensions once
            let max_width = max_length.max(30);
            let padding = (max_width - 26) / 3;

            // Preallocate the final string with estimated capacity
            let estimated_capacity = max_width * (queue.len() + 2) + 20;
            let mut output = String::with_capacity(estimated_capacity);

            // Create header
            let padding = " ".repeat(padding);
            writeln!(
                output,
                "{}Repeat: {}{} Shuffle: {}{}",
                padding,
                RepeatState::try_from(state.repeat)?,
                padding,
                state.shuffled,
                padding,
            )?;
            writeln!(output, "{}", "=".repeat(max_width))?;

            // Format queue items
            for (i, track) in queue.iter().enumerate() {
                let prefix = if i == index {
                    ">>".to_string()
                } else {
                    (i + 1).to_string() + "."
                };
                writeln!(output, " {} {}", prefix, track)?;
            }

            if queue.is_empty() {
                writeln!(output, "        Such empty.   ")?;
            } else if index >= queue.len() {
                writeln!(output, ">> ( END )")?;
            }

            println!("{}", output);
        }
    }

    Ok(())
}

macro_rules! send_and_recv {
    ($client: ident, $data: ident) => {{
        loop {
            $client.writable().await?;
            match $client.try_write(&$data) {
                Ok(_) => break,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                Err(e) => return Err(e.into()),
            }
        }

        let mut buf = [0u8; 1024];
        loop {
            $client.readable().await?;
            match $client.try_read(&mut buf) {
                Ok(0) => return Err(anyhow!("Connection closed by peer")),
                Ok(n) => break Vec::from(&buf[..n]),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                Err(e) => return Err(e.into()),
            }
        }
    }};
}

#[cfg(unix)]
async fn unix_send(request: PlayerRequest) -> anyhow::Result<PlayerResponse> {
    use tokio::net::UnixStream;

    let client = UnixStream::connect("/tmp/sunflower-daemon.sock").await?;
    let data = serialize_request(request);

    let buf = send_and_recv!(client, data);

    let resp = deserialize_response(&buf)?;

    Ok(resp)
}

async fn tcp_send(request: PlayerRequest) -> anyhow::Result<PlayerResponse> {
    let client = TcpStream::connect("localhost:8888").await?;
    let data = serialize_request(request);

    let buf = send_and_recv!(client, data);

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

    let buf = send_and_recv!(client, data);

    let resp = deserialize_response(&buf)?;

    Ok(resp)
}
