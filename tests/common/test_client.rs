use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};

pub const CONNECTION_WAIT_TIME_MS: u64 = 500;
pub const CONNECTION_TIMEOUT_SEC: u64 = 5;
pub struct TestClient {
    reader: BufReader<OwnedReadHalf>,
    writer: BufWriter<OwnedWriteHalf>,
}

impl TestClient {
    pub async fn write(&mut self, message: &str) -> tokio::io::Result<()> {
        self.writer.write_all(message.as_bytes()).await?;
        self.writer.flush().await?;
        Ok(())
    }

    pub async fn read_line(&mut self) -> tokio::io::Result<String> {
        let mut buf = String::new();
        self.reader.read_line(&mut buf).await?;
        Ok(buf)
    }

    pub async fn send_quit(&mut self) -> tokio::io::Result<()> {
        self.write("QUIT\n").await
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
