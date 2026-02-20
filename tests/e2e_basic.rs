mod common;
use common::setup_test_server_and_test_client::setup_test_server_and_client;
use common::test_case::TestCase;
use redlike::server::server_from_listener;
const ADDR: &str = "127.0.0.1:0";

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

    let (mut client, handle) = setup_test_server_and_client(ADDR).await?;

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
    let (mut client, handle) = setup_test_server_and_client(ADDR).await?;

    client.write("\n").await?;
    client.write("PING\n").await?;
    assert!("PONG\n" == client.read_line().await?);
    client.send_quit().await?;
    handle.abort();
    Ok(())
}
