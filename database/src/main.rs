use database::{server, DEFAULT_PORT};

use tokio::net::TcpListener;
use tokio::signal;


#[tokio::main]
pub async fn main() -> database::Result<()> {
    let listener = TcpListener::bind(&format!("0.0.0.0:{}", DEFAULT_PORT)).await?;

    server::run(listener, signal::ctrl_c()).await;

    Ok(())
}
