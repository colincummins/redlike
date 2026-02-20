use std::net::SocketAddr;

use crate::connection::Connection;
use crate::store::Store;
use tokio::io::Result;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

pub async fn server_from_listener(listener: TcpListener) -> Result<()> {
    let store = Store::new();

    loop {
        match listener.accept().await {
            Ok((mut socket, _addr)) => {
                let store = store.clone();
                tokio::spawn(async move {
                    let (read_half, write_half) = socket.split();
                    let mut conn = Connection::new(read_half, write_half, store);
                    if let Err(e) = conn.run().await {
                        println!("connection failed: {:?}", e)
                    }
                });
            }
            Err(e) => println!("client couldn't connect: {:?}", e),
        }
    }
}

pub async fn run_server(listener_address: &str) -> Result<(SocketAddr, JoinHandle<Result<()>>)> {
    let listener = TcpListener::bind(listener_address).await?;
    let addr: SocketAddr = listener.local_addr()?;
    let handle = tokio::spawn(server_from_listener(listener));
    Ok((addr, handle))
}
