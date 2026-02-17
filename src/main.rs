use redlike::connection::{self, Connection};
use tokio::net::{TcpListener};
use tokio::io::Result;
use redlike::store::Store;

const ADDR: &str = "127.0.0.1:6379";


#[tokio::main]
#[allow(unused_variables)]
async fn main() -> Result<()> {
    let listener = TcpListener::bind(ADDR).await?;
    let store = Store::new();

    loop {
        match listener.accept().await {
            Ok((mut socket, _addr)) => {
                    let store = store.clone();
                let handle = tokio::spawn(
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
