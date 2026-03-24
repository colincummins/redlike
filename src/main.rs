use redlike::config::get_config;
use redlike::server::run_server;
use tokio::signal::{self};
use tokio_util::sync::CancellationToken;

#[tokio::main]
#[allow(unused_variables)]
async fn main() -> Result<(), std::io::Error> {
    let config = get_config();
    let shutdown_token = CancellationToken::new();
    let (_address, handle) = run_server(&config, shutdown_token.clone()).await?;

    signal::ctrl_c().await?;
    shutdown_token.cancel();
    handle.await.map_err(std::io::Error::other)??;

    Ok(())
}
