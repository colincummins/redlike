use redlike::connection::Connection;
use::tokio::net::{TcpListener, TcpStream};
use::tokio::io::{BufReader, AsyncBufReadExt, Result};
use::redlike::store::Store;
const ADDR: &str = "127.0.0.1:6379";

async fn handle_connection(socket: TcpStream) -> tokio::io::Result<()> {
    let mut lines = BufReader::new(socket).lines();
    while let Some(command) = lines.next_line().await? {
        println!("Command received: {}", command);
    };
    println!("Client closed connection");
    Ok(())
}

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
