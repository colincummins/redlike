use redlike::config::get_config;
use redlike::server::run_server;
use tokio_util::sync::CancellationToken;
use tracing::info;

#[tokio::main]
#[allow(unused_variables)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let subscriber = tracing_subscriber::FmtSubscriber::new();
    tracing::subscriber::set_global_default(subscriber)?;
    let config = get_config();
    info!(?config, "configuration loaded");
    let shutdown_token = CancellationToken::new();
    let (_address, handle) = run_server(&config, shutdown_token.clone()).await?;
    info!(%_address, "server listening");
    wait_for_shutdown_signal().await?;
    info!("shutdown signal received");
    shutdown_token.cancel();
    handle.await.map_err(std::io::Error::other)??;
    info!("server shutdown complete");

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
