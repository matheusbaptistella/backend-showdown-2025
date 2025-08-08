use database::server;

use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{self, Duration};

#[tokio::test]
async fn key_value_get_set() {
    let addr = start_server().await;

    // Establish a connection to the server
    let mut stream = TcpStream::connect(addr).await.unwrap();

    stream
        .write_all(b"*1\r\n$3\r\nGET\r\n")
        .await
        .unwrap();

    // Check for 0, 0 return since the db has't processed any requests or amounts yet
    let mut response = [0; 12];
    stream.read_exact(&mut response).await.unwrap();
    assert_eq!(b"*2\r\n:0\r\n:0\r\n", &response);

    stream
        .write_all(b"*3\r\n$3\r\nSET\r\n;17231289881111\r\n:1990\r\n")
        .await
        .unwrap();

    // Read OK
    let mut response = [0; 5];
    stream.read_exact(&mut response).await.unwrap();
    assert_eq!(b"+OK\r\n", &response);

    stream
        .write_all(b"*1\r\n$3\r\nGET\r\n")
        .await
        .unwrap();

    // Check for 1, 1990 return
    let mut response = [0; 15];
    stream.read_exact(&mut response).await.unwrap();
    assert_eq!(b"*2\r\n:1\r\n:1990\r\n", &response);

    stream
        .write_all(b"*3\r\n$3\r\nSET\r\n;17231289882222\r\n:2090\r\n")
        .await
        .unwrap();

    // Read OK
    let mut response = [0; 5];
    stream.read_exact(&mut response).await.unwrap();
    assert_eq!(b"+OK\r\n", &response);

    stream
        .write_all(b"*1\r\n$3\r\nGET\r\n")
        .await
        .unwrap();

    // Check for 2, 3980 return
    let mut response = [0; 15];
    stream.read_exact(&mut response).await.unwrap();
    assert_eq!(b"*2\r\n:2\r\n:4080\r\n", &response);

    // Ask for values from a timestamp that is greater than all the ones in the db
    stream
        .write_all(b"*3\r\n$3\r\nGET\r\n$4\r\nFROM\r\n;17231289883333\r\n")
        .await
        .unwrap();

    // Check for 0, 0 return since the db has't processed any requests or amounts yet
    let mut response = [0; 12];
    stream.read_exact(&mut response).await.unwrap();
    assert_eq!(b"*2\r\n:0\r\n:0\r\n", &response);

    // Ask for values to a timestamp that is smaller than all the ones in the db
    stream
        .write_all(b"*3\r\n$3\r\nGET\r\n$2\r\nTO\r\n;17231289880000\r\n")
        .await
        .unwrap();

    // Check for 0, 0 return since the db has't processed any requests or amounts yet
    let mut response = [0; 12];
    stream.read_exact(&mut response).await.unwrap();
    assert_eq!(b"*2\r\n:0\r\n:0\r\n", &response);

    // Ask for values to a timestamp that is in between the ones in the db
    stream
        .write_all(b"*5\r\n$3\r\nGET\r\n$4\r\nFROM\r\n;17231289881122\r\n$2\r\nTO\r\n;17231289882211\r\n")
        .await
        .unwrap();

    // Check for 0, 0 return since the db has't processed any requests or amounts yet
    let mut response = [0; 12];
    stream.read_exact(&mut response).await.unwrap();
    assert_eq!(b"*2\r\n:0\r\n:0\r\n", &response);

    // Only first transaction
    stream
        .write_all(b"*5\r\n$3\r\nGET\r\n$4\r\nFROM\r\n;17231289881110\r\n$2\r\nTO\r\n;17231289882211\r\n")
        .await
        .unwrap();

    // Check for 1, 1990 return
    let mut response = [0; 15];
    stream.read_exact(&mut response).await.unwrap();
    assert_eq!(b"*2\r\n:1\r\n:1990\r\n", &response);

    // Only second transaction
    stream
        .write_all(b"*5\r\n$3\r\nGET\r\n$4\r\nFROM\r\n;17231289881112\r\n$2\r\nTO\r\n;17231289882223\r\n")
        .await
        .unwrap();

    // Check for 1, 2090 return
    let mut response = [0; 15];
    stream.read_exact(&mut response).await.unwrap();
    assert_eq!(b"*2\r\n:1\r\n:2090\r\n", &response);
}

async fn start_server() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { server::run(listener, tokio::signal::ctrl_c()).await });

    addr
}
