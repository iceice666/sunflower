use bytes::BytesMut;
use std::sync::Arc;
use sunflower_daemon::*;
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{error, info};

const BUFFER_SIZE: usize = 8 * 1024; // 8KB buffer

fn init_logger() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_file(true)
        .with_line_number(true)
        .try_init();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_logger();
    info!("Starting daemon server...");

    let daemon = Daemon::new();
    let (tx, rx) = daemon.start().await;

    let task_pool = TaskPool::new(tx);
    let pool_clone = Arc::clone(&task_pool);

    pool_clone.run(rx);

    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    info!("Listening on 127.0.0.1:8080");

    loop {
        let (socket, addr) = listener.accept().await?;
        info!("New connection from {}", addr);

        let pool = Arc::clone(&task_pool);

        tokio::spawn(async move {
            if let Err(e) = handle_connection(socket, pool).await {
                error!("Connection error: {}", e);
            }
        });
    }
}

async fn handle_connection(mut socket: TcpStream, pool: Arc<TaskPool>) -> io::Result<()> {
    let mut buf = BytesMut::with_capacity(BUFFER_SIZE);

    // Read data
    match socket.read_buf(&mut buf).await {
        Ok(0) => return Ok(()), // Connection closed
        Ok(_) => {}
        Err(e) => {
            error!("Failed to read from socket: {}", e);
            return Err(e);
        }
    }

    // Process request
    let response_kind = process_request(&buf, pool).await.unwrap_or_else(|e| {
        let msg = e.to_string();
        error!("{}", e);
        ResponseKind::Err(msg)
    });

    // Send response
    let response_data = match serde_json::to_vec(&response_kind) {
        Ok(data) => data,
        Err(e) => {
            error!("Failed to serialize response: {}", e);
            return Err(io::Error::new(io::ErrorKind::Other, e));
        }
    };

    socket.write_all(&response_data).await?;
    socket.flush().await?;

    Ok(())
}

async fn process_request(buf: &[u8], pool: Arc<TaskPool>) -> Result<ResponseKind, ProcessError> {
    let request_kind =
        serde_json::from_slice::<RequestKind>(buf).map_err(ProcessError::Deserialize)?;

    let rx = pool
        .new_task(request_kind)
        .map_err(ProcessError::TaskCreation)?;

    rx.await.map_err(ProcessError::Processing)
}

#[derive(Debug, thiserror::Error)]
enum ProcessError {
    #[error("Failed to deserialize request: {0}")]
    Deserialize(#[from] serde_json::Error),

    #[error("Failed to create task: {0}")]
    TaskCreation(#[from] tokio::sync::mpsc::error::SendError<Request>),

    #[error("Failed to receive result: {0}")]
    Processing(#[from] tokio::sync::oneshot::error::RecvError),
}
