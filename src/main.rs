use::tokio::net::{TcpListener, TcpStream};
use::tokio::io::AsyncWriteExt;
use std::io; 
const ADDR: &str = "127.0.0.1:6379";

#[tokio::main]
async fn main() -> io::Result<()> {
    let listener = TcpListener::bind(ADDR).await?;

    loop {
        match listener.accept().await {
            Ok((mut _socket, addr)) => {
                println!("new client: {:?}", addr);
                _socket.write_all(b"Connected").await?},
            Err(e) => println!("client couldn't connect: {:?}", e)
        }
    }

}
