use tokio::net::{TcpListener, TcpStream};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

type Db = Arc<Mutex<BTreeMap<i64, u64>>>;

#[tokio::main]
async fn main() {
    let listener = TcpListener::bind("0.0.0.0:6379").await.unwrap();

    let db: Db = Arc::new(Mutex::new(BTreeMap::new()));

    loop {
        let (socket, _) = listener.accept().await.unwrap();

        let db = db.clone();

        tokio::spawn(async move {
            process(socket, db).await;
        });
    }
}


async fn process(socket: TcpStream, db: Db) {
    let stream = tokio::io::BufWriter::new(socket);

    let buffer = bytes::BytesMut::with_capacity(1024);

    loop {
    }
}
