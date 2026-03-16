use std::net::SocketAddr;

use crate::connection::Connection;
use crate::store::Store;
use tokio::io::Result;
use tokio::net::TcpListener;
use tokio::select;
use tokio::task::{JoinHandle, JoinSet};

pub async fn server_from_listener(listener: TcpListener) -> Result<()> {
    let store = Store::new();
    let mut open_connections = JoinSet::new();

    loop {
        select! {
            connection_result = listener.accept() => {
                match connection_result {
                    Ok((mut socket, _addr)) => {
                        let store = store.clone();
                        open_connections.spawn(async move {
                            let (read_half, write_half) = socket.split();
                            let mut conn = Connection::new(read_half, write_half, store);
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
            }
        }
    }
}

pub async fn run_server(listener_address: &str) -> Result<(SocketAddr, JoinHandle<Result<()>>)> {
    let listener = TcpListener::bind(listener_address).await?;
    let addr: SocketAddr = listener.local_addr()?;
    let handle = tokio::spawn(server_from_listener(listener));
    Ok((addr, handle))
}
