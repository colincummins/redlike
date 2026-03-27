use crate::common::server_error_to_io;
use redlike::config::Config;
use redlike::server::run_server;
use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::io::{self, ErrorKind};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

pub async fn setup_test_server(
    listener_address: &str,
) -> Result<(SocketAddr, JoinHandle<io::Result<()>>, CancellationToken), io::Error> {
    setup_test_server_with_archive(listener_address, None).await
}

pub async fn setup_test_server_with_archive(
    listener_address: &str,
    archive_path: Option<PathBuf>,
) -> Result<(SocketAddr, JoinHandle<io::Result<()>>, CancellationToken), io::Error> {
    let shutdown_token = CancellationToken::new();
    let socket_addr: SocketAddr = listener_address.parse().map_err(|err| {
        io::Error::new(
            ErrorKind::InvalidInput,
            format!("invalid test listener address: {err}"),
        )
    })?;
    let config = Config {
        address: socket_addr.ip(),
        port: socket_addr.port(),
        archive_path,
        auth_password_file: None,
    };
    let (addr, handle) = run_server(&config, shutdown_token.clone())
        .await
        .map_err(server_error_to_io)?;
    let handle = tokio::spawn(async move {
        handle
            .await
            .map_err(io::Error::other)?
            .map_err(io::Error::other)
    });
    Ok((addr, handle, shutdown_token))
}
