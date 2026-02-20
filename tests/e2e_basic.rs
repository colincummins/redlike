use std::net::SocketAddr;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::{TcpListener, TcpStream};

use redlike::server::server_from_listener;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::task::JoinHandle;
const ADDR: &str = "127.0.0.1:0";
const CONNECTION_WAIT_TIME_MS: u64 = 500;
const CONNECTION_TIMEOUT_SEC: u64 = 5;

struct TestCase<'a> {
    call: &'a str,
    response: &'a str,
    expected: &'a str,
}

struct TestClient {
    reader: BufReader<OwnedReadHalf>,
    writer: BufWriter<OwnedWriteHalf>,
}

impl TestClient {
    async fn write(&mut self, message: &str) -> tokio::io::Result<()> {
        self.writer.write_all(message.as_bytes()).await?;
        self.writer.flush().await?;
        Ok(())
    }

    async fn read_line(&mut self) -> tokio::io::Result<String> {
        let mut buf = String::new();
        self.reader.read_line(&mut buf).await?;
        Ok(buf)
    }

    async fn send_quit(&mut self) -> tokio::io::Result<()> {
        self.write("QUIT\n").await
    }

    async fn new(addr: SocketAddr) -> tokio::io::Result<Self> {
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

async fn setup(
    listener_address: &str,
) -> Result<(TestClient, JoinHandle<Result<(), tokio::io::Error>>), tokio::io::Error> {
    let listener = TcpListener::bind(listener_address).await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    let handle = tokio::spawn(server_from_listener(listener));
    Ok((TestClient::new(addr).await?, handle))
}

#[tokio::test]
async fn e2e_sequential() -> tokio::io::Result<()> {
    let test_case_sequential: Vec<TestCase> = vec![
        TestCase {
            call: "PING\n",
            response: "PONG\n",
            expected: "Should respond to PING with PONG",
        },
        TestCase {
            call: "SET mykey myvalue\n",
            response: "OK\n",
            expected: "Should respond to SET with OK",
        },
        TestCase {
            call: "GET mykey\n",
            response: "myvalue\n",
            expected: "Should retrieve value of mykey: myvalue",
        },
        TestCase {
            call: "GET otherkey\n",
            response: "\n",
            expected: "Empty keys return empty lines",
        },
        TestCase {
            call: "DEL mykey\n",
            response: "1\n",
            expected: "Should return 1 if key is successfully deleted",
        },
        TestCase {
            call: "DEL mykey\n",
            response: "0\n",
            expected: "Should return 0 if DEL called on a key with no value",
        },
        TestCase {
            call: "FOO\n",
            response: "ERR Unknown Command\n",
            expected: "Unknown command gives error",
        },
        TestCase {
            call: "SET mykey myvalue too many\n",
            response: "ERR Wrong number of arguments\n",
            expected: "Wrong number of arguments gives error",
        },
        TestCase {
            call: "GET\n",
            response: "ERR Wrong number of arguments\n",
            expected: "Wrong number of arguments gives error",
        },
    ];

    let (mut client, handle) = setup(ADDR).await?;

    for TestCase {
        call,
        response,
        expected,
    } in test_case_sequential
    {
        client.write(call).await?;
        let received = client.read_line().await?;
        assert!(
            response == received,
            "{} - Expected {}, Received {}",
            expected,
            response,
            received
        );
    }

    client.send_quit().await?;
    handle.abort();
    Ok(())
}

#[tokio::test]
async fn e2e_blank_line_gets_no_response() -> tokio::io::Result<()> {
    let (mut client, handle) = setup(ADDR).await?;

    client.write("\n").await?;
    client.write("PING\n").await?;
    assert!("PONG\n" == client.read_line().await?);
    client.send_quit().await?;
    handle.abort();
    Ok(())
}
