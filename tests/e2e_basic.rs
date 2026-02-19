use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::TcpStream;

use redlike::server::run_server;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
const ADDR: &str = "127.0.0.1:6379";

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
    async fn write(&mut self, message: &str) {
        self.writer.write_all(message.as_bytes()).await.unwrap();
        self.writer.flush().await.unwrap()
    }

    async fn read_line(&mut self) -> String {
        let mut buf = String::new();
        self.reader.read_line(&mut buf).await.unwrap();
        buf
    }

    async fn send_quit(&mut self) {
        self.write("QUIT\n").await;
    }

    async fn new(addr: &str) -> Self {
        let stream = TcpStream::connect(addr).await.unwrap();
        let (read_half, write_half) = stream.into_split();
        TestClient {
            reader: BufReader::new(read_half),
            writer: BufWriter::new(write_half),
        }
    }
}

async fn setup() -> TestClient {
    tokio::spawn(run_server(ADDR));
    tokio::time::sleep(Duration::from_millis(50)).await;
    TestClient::new(ADDR).await
}

#[tokio::test]
async fn e2e_sequential() {
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

    let mut client = setup().await;

    for TestCase {
        call,
        response,
        expected,
    } in test_case_sequential
    {
        client.write(call).await;
        let received = client.read_line().await;
        assert!(
            response == received,
            "{} - Expected {}, Received {}",
            expected,
            response,
            received
        );
    }

    client.send_quit().await
}

#[tokio::test]
async fn e2e_blank_line_gets_no_response() {
    let mut client = setup().await;

    client.write("\n").await;
    client.write("PING\n").await;
    assert!("PONG\n" == client.read_line().await);
    client.send_quit().await
}
