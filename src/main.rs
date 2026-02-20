use redlike::server::run_server;
const ADDR: &str = "127.0.0.1:6379";

#[tokio::main]
#[allow(unused_variables)]
async fn main() -> Result<(), std::io::Error> {
    let (_address, handle) = run_server(ADDR).await?;
    handle.await?
}
