use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use crate::config::Config;
use crate::connection::Connection;
use crate::store::Store;
use tokio::io::Result;
use tokio::net::TcpListener;
use tokio::select;
use tokio::task::{JoinHandle, JoinSet};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

pub async fn server_from_listener(
    listener: TcpListener,
    shutdown_token: CancellationToken,
    archive_file_path: Option<PathBuf>,
) -> Result<()> {
    let store = Store::new();
    let mut open_connections = JoinSet::new();

    loop {
        select! {
            connection_result = listener.accept() => {
                match connection_result {
                    Ok((mut socket, _addr)) => {
                        let store = store.clone();
                        let connection_shutdown = shutdown_token.clone();
                        open_connections.spawn(async move {
                            let (read_half, write_half) = socket.split();
                            let mut conn = Connection::new(
                                read_half,
                                write_half,
                                store,
                                connection_shutdown,
                            );
                            if let Err(e) = conn.run().await {
                                println!("connection failed: {:?}", e)
                            }
                        });
                    }
                    Err(e) => println!("client couldn't connect: {:?}", e),
                };
            },
            join_result = open_connections.join_next(), if !open_connections.is_empty() => {
                if let Some(Err(err)) = join_result {
                    println!("connection task failed: {:?}", err);
                }
            },
            _ = shutdown_token.cancelled() => {break;}
        }
    }

    // Stop accepting new clients while waiting for open connections to drain
    drop(listener);

    let shutdown_result = timeout(Duration::from_secs(3), async {
        while let Some(join_result) = open_connections.join_next().await {
            if let Err(err) = join_result {
                println!("connection task failed: {:?}", err);
            }
        }
    })
    .await;

    if shutdown_result.is_err() {
        open_connections.abort_all();

        while let Some(join_result) = open_connections.join_next().await {
            if let Err(err) = join_result {
                println!("connection task failed: {:?}", err);
            }
        }
    }

    Ok(())
}

pub async fn run_server(
    config: &Config,
    shutdown_token: CancellationToken,
) -> Result<(SocketAddr, JoinHandle<Result<()>>)> {
    let addr = format!("{}:{}", config.address, config.port);
    let listener = TcpListener::bind(addr).await?;
    let addr: SocketAddr = listener.local_addr()?;
    let handle = tokio::spawn(server_from_listener(
        listener,
        shutdown_token.clone(),
        config.archive_path.clone(),
    ));
    Ok((addr, handle))
}
