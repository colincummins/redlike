use redlike::server::run_server;
use tokio::signal::{self};
use tokio_util::sync::CancellationToken;
const ADDR: &str = "127.0.0.1:6379";

#[tokio::main]
#[allow(unused_variables)]
async fn main() -> Result<(), std::io::Error> {
    let shutdown_token = CancellationToken::new();
    let (_address, handle) = run_server(ADDR, shutdown_token.clone()).await?;

    signal::ctrl_c().await?;
    shutdown_token.cancel();
    handle.await.map_err(std::io::Error::other)??;

    Ok(())
}
