mod common;

use common::setup_test_server::setup_test_server;
use common::test_client::TestClient;
use tokio::io::ErrorKind;
use tokio::net::TcpStream;
use tokio::time::{Duration, timeout};

const ADDR: &str = "127.0.0.1:0";
const READ_TIMEOUT: Duration = Duration::from_millis(250);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_millis(250);

async fn assert_connection_closed(client: &mut TestClient) {
    let err = timeout(READ_TIMEOUT, client.read_frame())
        .await
        .expect("timed out waiting for connection to close")
        .expect_err("expected connection to be closed");
    assert!(matches!(
        err.kind(),
        ErrorKind::UnexpectedEof | ErrorKind::ConnectionReset
    ));
}

async fn assert_server_shutdown(
    handle: tokio::task::JoinHandle<tokio::io::Result<()>>,
) -> tokio::io::Result<()> {
    let join_result = timeout(SHUTDOWN_TIMEOUT, handle)
        .await
        .expect("shutdown did not complete in time");
    join_result.expect("server panicked")?;
    Ok(())
}

#[tokio::test]
async fn server_handle_finishes_after_shutdown() -> tokio::io::Result<()> {
    let (_addr, handle, shutdown) = setup_test_server(ADDR).await?;
    shutdown.cancel();
    assert_server_shutdown(handle).await?;
    Ok(())
}

#[tokio::test]
async fn shutdown_server_stops_accepting_new_connections() -> tokio::io::Result<()> {
    let (addr, handle, shutdown) = setup_test_server(ADDR).await?;
    shutdown.cancel();
    assert_server_shutdown(handle).await?;
    assert!(TcpStream::connect(addr).await.is_err());
    Ok(())
}

#[tokio::test]
async fn idle_client_is_closed_by_shutdown() -> tokio::io::Result<()> {
    let (addr, handle, shutdown) = setup_test_server(ADDR).await?;
    let mut client = TestClient::new(addr).await?;
    shutdown.cancel();
    assert_server_shutdown(handle).await?;
    assert_connection_closed(&mut client).await;
    Ok(())
}
/*

#[tokio::test]
async fn e2e_malformed_first_frame_closes_without_response() -> tokio::io::Result<()> {
    let (addr, handle, _shutdown) = setup_test_server(ADDR).await?;
    let mut client = TestClient::new(addr).await?;

    client.write(b"*-2\r\n").await?;
    assert_connection_closed(&mut client).await;

    handle.abort();
    Ok(())
}

#[tokio::test]
async fn e2e_invalid_command_frame_closes_connection() -> tokio::io::Result<()> {
    let (addr, handle, _shutdown) = setup_test_server(ADDR).await?;
    let mut client = TestClient::new(addr).await?;

    client.write(b"*1\r\n:1\r\n").await?;
    assert_connection_closed(&mut client).await;

    handle.abort();
    Ok(())
}

#[tokio::test]
async fn e2e_unknown_command_does_not_close_connection() -> tokio::io::Result<()> {
    let (addr, handle, _shutdown) = setup_test_server(ADDR).await?;
    let mut client = TestClient::new(addr).await?;

    client.write(b"*1\r\n$3\r\nFOO\r\n").await?;
    assert_eq!(
        client.read_frame().await?,
        Frame::SimpleError("Unknown Command".into())
    );

    client.write(b"*1\r\n$4\r\nPING\r\n").await?;
    assert_eq!(
        client.read_frame().await?,
        Frame::SimpleString("PONG".into())
    );

    client.send_quit().await?;
    handle.abort();
    Ok(())
}

#[tokio::test]
async fn e2e_wrong_arity_does_not_close_connection() -> tokio::io::Result<()> {
    let (addr, handle, _shutdown) = setup_test_server(ADDR).await?;
    let mut client = TestClient::new(addr).await?;

    client.write(b"*1\r\n$3\r\nGET\r\n").await?;
    assert_eq!(
        client.read_frame().await?,
        Frame::SimpleError("Wrong number of arguments".into())
    );

    client.write(b"*1\r\n$4\r\nPING\r\n").await?;
    assert_eq!(
        client.read_frame().await?,
        Frame::SimpleString("PONG".into())
    );

    client.send_quit().await?;
    handle.abort();
    Ok(())
}

#[tokio::test]
async fn e2e_multiple_frames_in_one_write() -> tokio::io::Result<()> {
    let (addr, handle, _shutdown) = setup_test_server(ADDR).await?;
    let mut client = TestClient::new(addr).await?;

    client
        .write(b"*1\r\n$4\r\nPING\r\n*1\r\n$4\r\nPING\r\n")
        .await?;

    assert_eq!(
        client.read_frame().await?,
        Frame::SimpleString("PONG".into())
    );
    assert_eq!(
        client.read_frame().await?,
        Frame::SimpleString("PONG".into())
    );

    client.send_quit().await?;
    handle.abort();
    Ok(())
}

#[tokio::test]
async fn e2e_inline_and_resp_can_be_mixed() -> tokio::io::Result<()> {
    let (addr, handle, _shutdown) = setup_test_server(ADDR).await?;
    let mut client = TestClient::new(addr).await?;

    client.write(b"PING\n").await?;
    assert_eq!(
        client.read_frame().await?,
        Frame::SimpleString("PONG".into())
    );

    client
        .write(b"*3\r\n$3\r\nSET\r\n$5\r\nmixed\r\n$5\r\nvalue\r\n")
        .await?;
    assert_eq!(client.read_frame().await?, Frame::SimpleString("OK".into()));

    client.write(b"GET mixed\n").await?;
    assert_eq!(
        client.read_frame().await?,
        Frame::Bulk(Some(b"value".to_vec()))
    );

    client.send_quit().await?;
    handle.abort();
    Ok(())
}

#[tokio::test]
async fn e2e_split_frame_writes_are_reassembled() -> tokio::io::Result<()> {
    let (addr, handle, _shutdown) = setup_test_server(ADDR).await?;
    let mut client = TestClient::new(addr).await?;

    client.write(b"*3\r\n$3\r\nSET\r\n").await?;
    client.write(b"$5\r\nsplit\r\n").await?;
    client.write(b"$5\r\nvalue\r\n").await?;
    assert_eq!(client.read_frame().await?, Frame::SimpleString("OK".into()));

    client.write(b"*2\r\n$3\r\nGET\r\n").await?;
    client.write(b"$5\r\nsplit\r\n").await?;
    assert_eq!(
        client.read_frame().await?,
        Frame::Bulk(Some(b"value".to_vec()))
    );

    client.send_quit().await?;
    handle.abort();
    Ok(())
}
*/
