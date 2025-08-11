use database::{
    clients::{BufferedClient, Client},
    server,
};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

#[tokio::test]
async fn pool_key_value_get_set() {
    let (addr, _) = start_server().await;
    let client = Client::connect(addr).await.unwrap();
    let mut client = BufferedClient::buffer(client);

    // Initial get: should be (0, 0)
    let res = client.get(None, None).await.unwrap();
    assert_eq!(res, Some((0, 0).into()));

    // Set first value
    client.set(17231289881111, 1990).await.unwrap();

    // Get: should be (1, 1990)
    let res = client.get(None, None).await.unwrap();
    assert_eq!(res, Some((1, 1990).into()));

    // Set second value
    client.set(17231289882222, 2090).await.unwrap();

    // Get: should be (2, 4080)
    let res = client.get(None, None).await.unwrap();
    assert_eq!(res, Some((2, 4080).into()));

    // Get with from > all
    let res = client.get(Some(17231289883333), None).await.unwrap();
    assert_eq!(res, Some((0, 0).into()));

    // Get with to < all
    let res = client.get(None, Some(17231289880000)).await.unwrap();
    assert_eq!(res, Some((0, 0).into()));

    // Get with from/to between
    let res = client.get(Some(17231289881122), Some(17231289882211)).await.unwrap();
    assert_eq!(res, Some((0, 0).into()));

    // Only first transaction
    let res = client.get(Some(17231289881110), Some(17231289882211)).await.unwrap();
    assert_eq!(res, Some((1, 1990).into()));

    // Only second transaction
    let res = client.get(Some(17231289881112), Some(17231289882223)).await.unwrap();
    assert_eq!(res, Some((1, 2090).into()));
}

async fn start_server() -> (SocketAddr, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let handle = tokio::spawn(async move { server::run(listener, tokio::signal::ctrl_c()).await });

    (addr, handle)
}