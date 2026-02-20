use super::super::server_from_listener;
use super::test_client::TestClient;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

pub async fn setup_test_server_and_client(
    listener_address: &str,
) -> Result<(TestClient, JoinHandle<Result<(), tokio::io::Error>>), tokio::io::Error> {
    let listener = TcpListener::bind(listener_address).await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    let handle = tokio::spawn(server_from_listener(listener));
    Ok((TestClient::new(addr).await?, handle))
}
