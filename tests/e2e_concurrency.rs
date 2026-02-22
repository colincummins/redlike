mod common;
use std::{net::ToSocketAddrs, os::unix::net::SocketAddr};

use common::test_case::TestCase;
use common::test_client::TestClient;
use redlike::server::{run_server, server_from_listener};
const ADDR: &str = "127.0.0.1:0";

async fn test_all_commands(addr: std::net::SocketAddr) -> tokio::io::Result<()> {
    let mut client = TestClient::new(addr).await?;

    for _ in 0..100 {
        client.write("PING\n").await?;
        assert_eq!(client.read_line().await?, "PONG\n".to_string());
    }

    client.write("QUIT\n").await?;

    Ok(())
}

#[tokio::test]
async fn get_set_del_same_record() -> tokio::io::Result<()> {
    let (addr, handle) = run_server(ADDR).await?;

    test_all_commands(addr).await?;

    handle.abort();
    Ok(())
}
