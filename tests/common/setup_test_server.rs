use redlike::server::server_from_listener;
use std::net::SocketAddr;
use tokio::net::TcpListener;
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
    let listener = TcpListener::bind(listener_address).await?;
    let addr: SocketAddr = listener.local_addr()?;
    let handle = tokio::spawn(server_from_listener(listener, shutdown_token.clone()));
    Ok((addr, handle, shutdown_token))
}
