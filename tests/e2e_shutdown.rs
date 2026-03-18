mod common;

use common::setup_test_server::setup_test_server;
use common::test_client::TestClient;
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
