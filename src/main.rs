use tokio::io::AsyncReadExt;
use::tokio::net::{TcpListener, TcpStream};
use::tokio::io::AsyncWriteExt;
use std::io; 
const ADDR: &str = "127.0.0.1:6379";
const READ_BUFFER_SIZE: usize = 1024;

async fn handle_connection(mut socket: TcpStream) -> io::Result<()> {

    loop {
        let mut read_buffer: [u8; READ_BUFFER_SIZE] = [0; READ_BUFFER_SIZE];
        socket.read_buf(read_buffer);

    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let listener = TcpListener::bind(ADDR).await?;

    loop {
        match listener.accept().await {
            Ok((mut _socket, addr)) => {
                println!("new client: {:?}", addr);
                handle_connection(_socket).await?;
            },
            Err(e) => println!("client couldn't connect: {:?}", e)
        }
    }

}
