use tokio::net::{TcpListener};
use tokio::io::Result;
use redlike::store::Store;

const ADDR: &str = "127.0.0.1:6379";


#[tokio::main]
async fn main() -> Result<()> {
    let listener = TcpListener::bind(ADDR).await?;
    let store: Store;

    loop {
        match listener.accept().await {
            Ok((mut _socket, addr)) => {
                println!("new client: {:?}", addr);
            },
            Err(e) => println!("client couldn't connect: {:?}", e)
        }
    }

}
