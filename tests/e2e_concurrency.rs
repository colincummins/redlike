mod common;
use common::test_client::TestClient;
use redlike::config::Config;
use redlike::frame::Frame;
use redlike::server::{ServerError, run_server};
use tokio::io;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

async fn test_all_commands(addr: std::net::SocketAddr) -> tokio::io::Result<()> {
    let mut client = TestClient::new(addr).await?;

    for i in 0..10 {
        client.write(b"*1\r\n$4\r\nPING\r\n").await?;
        tokio::time::sleep(tokio::time::Duration::from_millis(i * 10)).await;
        assert_eq!(
            client.read_frame().await?,
            Frame::SimpleString("PONG".into())
        );
        client
            .write(b"*3\r\n$3\r\nSET\r\n$5\r\nmykey\r\n$7\r\nmyvalue\r\n")
            .await?;
        tokio::time::sleep(tokio::time::Duration::from_millis(i * 10)).await;
        assert_eq!(client.read_frame().await?, Frame::SimpleString("OK".into()));
        client.write(b"*2\r\n$3\r\nGET\r\n$5\r\nmykey\r\n").await?;
        tokio::time::sleep(tokio::time::Duration::from_millis(i * 10)).await;
        let get_response = client.read_frame().await?;
        assert!(
            matches!(
                get_response,
                Frame::Bulk(Some(ref bytes)) if bytes == b"myvalue"
            ) || matches!(get_response, Frame::Bulk(None))
        );
        client.write(b"*2\r\n$3\r\nDEL\r\n$5\r\nmykey\r\n").await?;
        tokio::time::sleep(tokio::time::Duration::from_millis(i * 10)).await;
        assert!(matches!(
            client.read_frame().await?,
            Frame::Integer(0) | Frame::Integer(1)
        ));
    }

    client.send_quit().await?;

    Ok(())
}

#[tokio::test]
async fn get_set_del_same_record() -> tokio::io::Result<()> {
    let shutdown = CancellationToken::new();
    let config = Config {
        address: "127.0.0.1".parse().unwrap(),
        port: 0,
        archive_path: None,
    };
    let (addr, handle) = run_server(&config, shutdown)
        .await
        .map_err(server_error_to_io)?;

    let mut client_handles = JoinSet::new();

    for _i in 0..32 {
        client_handles.spawn(test_all_commands(addr));
    }

    while let Some(res) = client_handles.join_next().await {
        res.expect("Client panicked")?;
    }

    handle.abort();
    Ok(())
}

fn server_error_to_io(err: ServerError) -> io::Error {
    match err {
        ServerError::Io(err) => err,
        ServerError::Archive(err) => io::Error::other(err),
    }
}
