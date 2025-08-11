use crate::cmd::{Get, Set};
use crate::{Connection, Frame};

use bytes::Bytes;
use std::io::{Error, ErrorKind};
use std::time::Duration;
use tokio::net::{TcpStream, ToSocketAddrs};

pub struct Client {
    connection: Connection,
}

pub struct Message {
    pub conent: Bytes,
}

impl Client {
    pub async fn connect<T: ToSocketAddrs>(addr: T) -> crate::Result<Client> {
        let socket = TcpStream::connect(addr).await?;

        // Initialize the connection state. This allocates read/write buffers to
        // perform redis protocol frame parsing.
        let connection = Connection::new(socket);

        Ok(Client { connection })
    }

    pub async fn get(&mut self, from: Option<i64>, to: Option<i64>) -> crate::Result<Option<(u64, u64)>> {
        let frame = Get::new(from, to).into_frame();

        self.connection.write_frame(&frame).await?;

        match self.read_response().await? {
            Frame::Array(ref frames) if frames.len() == 2 => {
                if let (Frame::Integer(count), Frame::Integer(total)) = (&frames[0], &frames[1]) {
                    Ok(Some((*count, *total)))
                } else {
                    Err("Invalid frame types for count and total".into())
                }
            }
            Frame::Null => Ok(None),
            frame => Err(frame.to_error()),
        }
    }

    pub async fn set(&mut self, timestamp: i64, amount: u64) -> crate::Result<()> {
        let frame = Set::new(timestamp, amount).into_frame();

        self.connection.write_frame(&frame).await?;

        match self.read_response().await? {
            Frame::Simple(response) if response == "OK" => Ok(()),
            frame => Err(frame.to_error()),
        }
    }

    async fn read_response(&mut self) -> crate::Result<Frame> {
        let response = self.connection.read_frame().await?;

        match response {
            Some(Frame::Error(msg)) => Err(msg.into()),
            Some(frame) => Ok(frame),
            None => {
                // Receiving `None` here indicates the server has closed the
                // connection without sending a frame. This is unexpected and is
                // represented as a "connection reset by peer" error.
                let err = Error::new(ErrorKind::ConnectionReset, "connection reset by server");

                Err(err.into())
            }
        }
    }
}
