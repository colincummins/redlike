use redlike::config::Config;
use redlike::server::run_server;
use std::net::SocketAddr;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

pub async fn setup_test_server(
    listener_address: &str,
) -> Result<
    (
        SocketAddr,
        JoinHandle<Result<(), tokio::io::Error>>,
        CancellationToken,
    ),
    tokio::io::Error,
> {
    let shutdown_token = CancellationToken::new();
    let socket_addr: SocketAddr = listener_address.parse().map_err(|err| {
        tokio::io::Error::new(
            tokio::io::ErrorKind::InvalidInput,
            format!("invalid test listener address: {err}"),
        )
    })?;
    let config = Config {
        address: socket_addr.ip(),
        port: socket_addr.port(),
        archive_path: None,
    };
    let (addr, handle) = run_server(&config, shutdown_token.clone()).await?;
    Ok((addr, handle, shutdown_token))
}
