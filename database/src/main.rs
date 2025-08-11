use database::{server, DEFAULT_PORT};

use tokio::net::TcpListener;
use tokio::signal;


#[tokio::main]
pub async fn main() -> database::Result<()> {
    let listener = TcpListener::bind(&format!("127.0.0.1:{}", DEFAULT_PORT)).await?;

    server::run(listener, signal::ctrl_c()).await;

    Ok(())
}
