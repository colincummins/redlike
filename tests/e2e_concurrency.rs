mod common;
use common::test_case::TestCase;
use redlike::server::{run_server, server_from_listener};
const ADDR: &str = "127.0.0.1:0";

#[tokio::test]
async fn concurrent_get_set_del() -> tokio::io::Result<()> {
    let (handle, addr) = run_server(ADDR).await?;

    Ok(())
}
