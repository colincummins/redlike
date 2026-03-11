mod common;
use common::setup_test_server_and_test_client::setup_test_server_and_test_client;
use common::test_case::TestCase;
use redlike::frame::Frame;
const ADDR: &str = "127.0.0.1:0";

#[tokio::test]
async fn e2e_sequential() -> tokio::io::Result<()> {
    let test_case_sequential: Vec<TestCase> = vec![
        TestCase {
            call: b"*1\r\n$4\r\nPING\r\n",
            response: Frame::SimpleString("PONG".into()),
            expected: "Should respond to PING with PONG",
        },
        TestCase {
            call: b"*3\r\n$3\r\nSET\r\n$5\r\nmykey\r\n$7\r\nmyvalue\r\n",
            response: Frame::SimpleString("OK".into()),
            expected: "Should respond to SET with OK",
        },
        TestCase {
            call: b"*2\r\n$3\r\nGET\r\n$5\r\nmykey\r\n",
            response: Frame::Bulk(Some(b"myvalue".to_vec())),
            expected: "Should retrieve value of mykey: myvalue",
        },
        TestCase {
            call: b"*2\r\n$3\r\nGET\r\n$8\r\notherkey\r\n",
            response: Frame::Bulk(None),
            expected: "Missing keys return null bulk strings",
        },
        TestCase {
            call: b"*2\r\n$3\r\nDEL\r\n$5\r\nmykey\r\n",
            response: Frame::Integer(1),
            expected: "Should return 1 if key is successfully deleted",
        },
        TestCase {
            call: b"*2\r\n$3\r\nDEL\r\n$5\r\nmykey\r\n",
            response: Frame::Integer(0),
            expected: "Should return 0 if DEL called on a key with no value",
        },
        TestCase {
            call: b"*1\r\n$3\r\nFOO\r\n",
            response: Frame::SimpleError("Unknown Command".into()),
            expected: "Unknown command gives error",
        },
        TestCase {
            call: b"*4\r\n$3\r\nSET\r\n$5\r\nmykey\r\n$7\r\nmyvalue\r\n$8\r\ntoo many\r\n",
            response: Frame::SimpleError("Wrong number of arguments".into()),
            expected: "Wrong number of arguments gives error",
        },
        TestCase {
            call: b"*1\r\n$3\r\nGET\r\n",
            response: Frame::SimpleError("Wrong number of arguments".into()),
            expected: "Wrong number of arguments gives error",
        },
    ];

    let (mut client, handle) = setup_test_server_and_test_client(ADDR).await?;

    for TestCase {
        call,
        response,
        expected,
    } in test_case_sequential
    {
        client.write(call).await?;
        let received = client.read_frame().await?;
        assert!(
            response == received,
            "{} - Expected {:?}, Received {:?}",
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
async fn e2e_inline_terminal_requests() -> tokio::io::Result<()> {
    let test_case_sequential: Vec<TestCase> = vec![
        TestCase {
            call: b"PING\n",
            response: Frame::SimpleString("PONG".into()),
            expected: "Inline PING should respond with PONG",
        },
        TestCase {
            call: b"SET mykey myvalue\n",
            response: Frame::SimpleString("OK".into()),
            expected: "Inline SET should respond with OK",
        },
        TestCase {
            call: b"GET mykey\n",
            response: Frame::Bulk(Some(b"myvalue".to_vec())),
            expected: "Inline GET should return the stored value",
        },
        TestCase {
            call: b"GET otherkey\n",
            response: Frame::Bulk(None),
            expected: "Inline GET on a missing key should return null bulk",
        },
        TestCase {
            call: b"DEL mykey\n",
            response: Frame::Integer(1),
            expected: "Inline DEL should return 1 when it deletes a key",
        },
        TestCase {
            call: b"DEL mykey\n",
            response: Frame::Integer(0),
            expected: "Inline DEL should return 0 for a missing key",
        },
        TestCase {
            call: b"FOO\n",
            response: Frame::SimpleError("Unknown Command".into()),
            expected: "Inline unknown commands should return an error",
        },
        TestCase {
            call: b"SET mykey myvalue too many\n",
            response: Frame::SimpleError("Wrong number of arguments".into()),
            expected: "Inline wrong-arity SET should return an error",
        },
        TestCase {
            call: b"GET\n",
            response: Frame::SimpleError("Wrong number of arguments".into()),
            expected: "Inline wrong-arity GET should return an error",
        },
    ];

    let (mut client, handle) = setup_test_server_and_test_client(ADDR).await?;

    for TestCase {
        call,
        response,
        expected,
    } in test_case_sequential
    {
        client.write(call).await?;
        let received = client.read_frame().await?;
        assert!(
            response == received,
            "{} - Expected {:?}, Received {:?}",
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
    let (mut client, handle) = setup_test_server_and_test_client(ADDR).await?;

    client.write(b"\n").await?;
    client.write(b"*1\r\n$4\r\nPING\r\n").await?;
    assert_eq!(Frame::SimpleString("PONG".into()), client.read_frame().await?);
    client.send_quit().await?;
    handle.abort();
    Ok(())
}
