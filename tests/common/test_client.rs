use redlike::frame::Frame;
use redlike::parser::Parser;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};

use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader, BufWriter};

pub const CONNECTION_WAIT_TIME_MS: u64 = 500;
pub const CONNECTION_TIMEOUT_SEC: u64 = 5;
pub struct TestClient {
    reader: BufReader<OwnedReadHalf>,
    writer: BufWriter<OwnedWriteHalf>,
}

impl TestClient {
    pub async fn write(&mut self, message: &[u8]) -> tokio::io::Result<()> {
        self.writer.write_all(message).await?;
        self.writer.flush().await?;
        Ok(())
    }

    pub async fn read_frame(&mut self) -> tokio::io::Result<Frame> {
        let mut parser = Parser::new();

        loop {
            let mut byte = [0; 1];
            let read = self.reader.read(&mut byte).await?;
            if read == 0 {
                return Err(tokio::io::Error::new(
                    tokio::io::ErrorKind::UnexpectedEof,
                    "connection closed before a full frame was received",
                ));
            }
            match parser.parse(&byte[..read]) {
                Ok(frames) if frames.len() == 1 => return Ok(frames.into_iter().next().unwrap()),
                Ok(frames) if frames.is_empty() => continue,
                Ok(_) => {
                    return Err(tokio::io::Error::new(
                        tokio::io::ErrorKind::InvalidData,
                        "received more than one frame",
                    ));
                }
                Err(err) => {
                    return Err(tokio::io::Error::new(
                        tokio::io::ErrorKind::InvalidData,
                        format!("{err:?}"),
                    ));
                }
            }
        }
    }

    pub async fn send_quit(&mut self) -> tokio::io::Result<()> {
        self.write(b"*1\r\n$4\r\nQUIT\r\n").await
    }

    pub async fn new(addr: SocketAddr) -> tokio::io::Result<Self> {
        let stream = tokio::time::timeout(Duration::from_secs(CONNECTION_TIMEOUT_SEC), async {
            loop {
                match TcpStream::connect(addr).await {
                    Err(_) => {
                        tokio::time::sleep(Duration::from_millis(CONNECTION_WAIT_TIME_MS)).await
                    }
                    Ok(s) => return s,
                }
            }
        })
        .await?;

        let (read_half, write_half) = stream.into_split();
        Ok(TestClient {
            reader: BufReader::new(read_half),
            writer: BufWriter::new(write_half),
        })
    }
}
