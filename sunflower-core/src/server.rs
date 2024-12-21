use sunflower_core::Daemon;

use sunflower_core::PlayerServiceServer;
use tonic::transport::Server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::1]:11451".parse().unwrap();

    let daemon = Daemon::new();

    let service:PlayerServiceServer<Daemon> = PlayerServiceServer::new(daemon);

    Server::builder()
        .add_service(service)
        .serve(addr)
        .await
        ?;

    Ok(())
}
