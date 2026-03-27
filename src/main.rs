use redlike::config::get_config;
use redlike::server::run_server;
use tokio_util::sync::CancellationToken;

#[tokio::main]
#[allow(unused_variables)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = get_config();
    let shutdown_token = CancellationToken::new();
    let (_address, handle) = run_server(&config, shutdown_token.clone()).await?;

    wait_for_shutdown_signal().await?;
    shutdown_token.cancel();
    handle.await.map_err(std::io::Error::other)??;

    Ok(())
}

async fn wait_for_shutdown_signal() -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};

        let mut sigterm = signal(SignalKind::terminate())?;
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = sigterm.recv() => {}
        }
        Ok(())
    }

    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c().await
    }
}
