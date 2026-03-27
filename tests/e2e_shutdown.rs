mod common;

use common::setup_test_server::{setup_test_server, setup_test_server_with_archive};
use common::test_client::TestClient;
use redlike::archive::load;
use redlike::frame::Frame;
use tempfile::tempdir;
use tokio::io::ErrorKind;
use tokio::net::TcpStream;
use tokio::time::{Duration, timeout};

const ADDR: &str = "127.0.0.1:0";
const CLIENT_TIMEOUT: Duration = Duration::from_millis(250);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_millis(250);

async fn assert_connection_closed(client: &mut TestClient) {
    let err = timeout(CLIENT_TIMEOUT, client.read_frame())
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

#[tokio::test]
async fn multiple_idle_clients_are_closed_by_shutdown() -> tokio::io::Result<()> {
    let (addr, handle, shutdown) = setup_test_server(ADDR).await?;
    let mut clients = Vec::with_capacity(3);
    for _ in 0..3 {
        clients.push(TestClient::new(addr).await?);
    }
    shutdown.cancel();
    assert_server_shutdown(handle).await?;
    for client in clients.iter_mut() {
        assert_connection_closed(client).await;
    }
    Ok(())
}

#[tokio::test]
async fn midstream_client_is_closed_by_shutdown() -> tokio::io::Result<()> {
    let (addr, handle, shutdown) = setup_test_server(ADDR).await?;
    let mut client = TestClient::new(addr).await?;
    timeout(CLIENT_TIMEOUT, client.write(b"*2\r\n$3\r\nGET\r\n$5\r\nmy"))
        .await
        .expect("Client write timed out")
        .expect("Client write failed");
    tokio::time::sleep(Duration::from_millis(20)).await;
    shutdown.cancel();
    assert_server_shutdown(handle).await?;
    assert_connection_closed(&mut client).await;
    Ok(())
}

#[tokio::test]
async fn shutdown_persists_store_to_archive() -> tokio::io::Result<()> {
    let temp_dir = tempdir()?;
    let archive_path = temp_dir.path().join("redlike.rdb");
    let (addr, handle, shutdown) =
        setup_test_server_with_archive(ADDR, Some(archive_path.clone())).await?;
    let mut client = TestClient::new(addr).await?;

    client
        .write(b"*3\r\n$3\r\nSET\r\n$7\r\npersist\r\n$5\r\nvalue\r\n")
        .await?;
    assert_eq!(client.read_frame().await?, Frame::SimpleString("OK".into()));

    shutdown.cancel();
    assert_server_shutdown(handle).await?;

    let store = load(archive_path).await.map_err(tokio::io::Error::other)?;
    assert_eq!(
        store.get(&b"persist".to_vec()).await,
        Some(b"value".to_vec())
    );
    Ok(())
}
