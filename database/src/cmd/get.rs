use crate::cmd::{Parse, ParseError};
use crate::{Connection, Db, Frame};

use bytes::Bytes;

#[derive(Debug)]
pub struct Get {
    from: Option<i64>,
    to: Option<i64>,
}

impl Get {
    pub fn new(from: Option<i64>, to: Option<i64>) -> Get {
        Get {
            from,
            to,
        }
    }

    pub fn from(&self) -> Option<i64> {
        self.from
    }

    pub fn to(&self) -> Option<i64> {
        self.to
    }

    /// Parse a `Get` instance from a received frame.
    ///
    /// The `Parse` argument provides a cursor-like API to read fields from the
    /// `Frame`. At this point, the entire frame has already been received from
    /// the socket.
    ///
    /// The `GET` string has already been consumed.
    ///
    /// # Returns
    ///
    /// Returns two u64 integers COUNT and TOTAL. If the frame is malformed, `Err` is
    /// returned.
    ///
    /// # Format
    ///
    /// Expects an array frame containing at least one entry.
    ///
    /// ```text
    /// GET FROM timestamp TO timestamp
    /// ```
    pub fn parse_frames(parse: &mut Parse) -> crate::Result<Get> {
        use ParseError::EndOfStream;

        // The `GET` string has already been consumed.
        let mut from = None;
        let mut to = None;

        match parse.next_string() {
            Ok(s) if s.to_uppercase() == "FROM" => {
                let ts = parse.next_timestamp()?;
                from = Some(ts);

                match parse.next_string() {
                    Ok(s) if s.to_uppercase() == "TO" => {
                        let ts = parse.next_timestamp()?;
                        to = Some(ts);
                    }
                    Ok(_) => return Err("current `SET` only supports the range oprions".into()),
                    Err(EndOfStream) => {},
                    Err(e) => return Err(e.into()),
                }
            }
            Ok(s) if s.to_uppercase() == "TO" => {
                let ts = parse.next_timestamp()?;
                to = Some(ts);
            }
            Ok(_) => return Err("current `SET` only supports the range oprions".into()),
            Err(EndOfStream) => {},
            Err(e) => return Err(e.into()),
        }

        Ok(Get { from, to })
    }

    /// Apply the `Get` command to the specified `Db` instance.
    ///
    /// The response is written to `dst`. This is called by the server in order
    /// to execute a received command.
    pub async fn apply(self, db: &Db, dst: &mut Connection) -> crate::Result<()> {
        // Get the value from the shared database state
        let (count, total) = db.get(self.from, self.to);
        let mut response = Frame::array();
        response.push_int(count);     
        response.push_int(total);    

        dst.write_frame(&response).await?;

        Ok(())
    }

    /// Converts the command into an equivalent `Frame`.
    ///
    /// This is called by the client when encoding a `Get` command to send to
    /// the server.
    pub fn into_frame(self) -> Frame {
        let mut frame = Frame::array();
        frame.push_bulk(Bytes::from("get".as_bytes()));
        if let Some(from_ts) = self.from {
            frame.push_bulk(Bytes::from("FROM".as_bytes()));
            frame.push_timestamp(from_ts);
        }
        if let Some(to_ts) = self.to {
            frame.push_bulk(Bytes::from("TO".as_bytes()));
            frame.push_timestamp(to_ts);
        }

        frame
    }
}