use crate::connection::Connection;
use tokio::net::{TcpListener};
use tokio::io::Result;
use crate::store::Store;

pub async fn run_server(listener_address: &str) -> Result<()> {
    let listener = TcpListener::bind(listener_address).await?;
    let store = Store::new();

    loop {
        match listener.accept().await {
            Ok((mut socket, _addr)) => {
                    let store = store.clone();
                tokio::spawn(
                    async move {
                        let (read_half, write_half) = socket.split();
                        let mut conn = Connection::new(read_half, write_half, store);
                        if let Err(e) = conn.run().await {
                            println!("connection failed: {:?}", e)
                        }
                    }
                );
            },
            Err(e) => println!("client couldn't connect: {:?}", e)
        }
    }

}