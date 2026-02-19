use std::time::Duration;

use tokio::io::{BufReader, BufWriter};
use tokio::net::TcpStream;

use redlike::server::run_server;
const ADDR: &str = "127.0.0.1:6379";

struct TestCase<'a> {
    call: &'a str,
    response: &'a str,
    expected: &'a str,
}

#[tokio::test]
async fn e2e_concurrency() {
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

    tokio::spawn(run_server(ADDR));
    tokio::time::sleep(Duration::from_millis(50)).await;
    let mut stream = TcpStream::connect(ADDR).await.unwrap();
    let (read_half, write_half) = stream.split();
    let reader = BufReader::new(read_half);
    let writer = BufWriter::new(write_half);
}
