use redlike::server::run_server;
use tokio::signal::{self};
use tokio_util::sync::CancellationToken;
const ADDR: &str = "127.0.0.1:6379";

#[tokio::main]
#[allow(unused_variables)]
async fn main() -> Result<(), std::io::Error> {
    let shutdown = CancellationToken::new();
    let (_address, handle) = run_server(ADDR).await?;

    match signal::ctrl_c().await {
        Ok(()) => {
            shutdown.cancel();
            Ok(())
        }
        Err(e) => Err(e),
    }
}
