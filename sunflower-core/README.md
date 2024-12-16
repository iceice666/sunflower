# sunflower-core

The player daemon implementation for sunflower-rs

## Usage

```rust
use sunflower_core::protocol::{Request, Response};
use sunflower_core::Daemon;

#[tokio::main]
async fn main() {
    let daemon = Daemon::new();
    let (tx, mut rx) = daemon.start().await;

    loop {
        let request = recv_request();

        tx.send(request).expect("Handle error as you need");

        let response = rx.recv().await.expect("Handle error as you need");

        send_response(response);
    }
}

fn recv_request() -> Request {
    todo!("impl recv request")
}

fn send_response(response: Response) {
    todo!("impl send response");
}


```