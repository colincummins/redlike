mod common;
use common::test_client::TestClient;
use redlike::server::{run_server, server_from_listener};
use tokio::task::JoinSet;
const ADDR: &str = "127.0.0.1:0";

async fn test_all_commands(addr: std::net::SocketAddr) -> tokio::io::Result<()> {
    let mut client = TestClient::new(addr).await?;

    for i in 0..10 {
        client.write("PING\n").await?;
        tokio::time::sleep(tokio::time::Duration::from_millis(i * 10)).await;
        assert_eq!(client.read_line().await?, "PONG\n".to_string());
        client.write("SET mykey myvalue\n").await?;
        tokio::time::sleep(tokio::time::Duration::from_millis(i * 10)).await;
        assert!(matches!(client.read_line().await?.as_ref(), "OK\n"));
        client.write("GET mykey\n").await?;
        tokio::time::sleep(tokio::time::Duration::from_millis(i * 10)).await;
        assert!(matches!(
            client.read_line().await?.as_ref(),
            "myvalue\n" | "\n"
        ));
        client.write("DEL mykey\n").await?;
        tokio::time::sleep(tokio::time::Duration::from_millis(i * 10)).await;
        assert!(matches!(client.read_line().await?.as_ref(), "0\n" | "1\n"));
    }

    client.send_quit().await?;

    Ok(())
}

#[tokio::test]
async fn get_set_del_same_record() -> tokio::io::Result<()> {
    let (addr, handle) = run_server(ADDR).await?;

    let mut client_handles = JoinSet::new();

    for _i in 0..32 {
        client_handles.spawn(test_all_commands(addr));
    }

    while let Some(res) = client_handles.join_next().await {
        res??;
    }

    handle.abort();
    Ok(())
}
