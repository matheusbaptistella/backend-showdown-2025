use crate::cmd::{Parse};
use crate::{Connection, Db, Frame};

use bytes::Bytes;

#[derive(Debug)]
pub struct Set {
    instance: u8,
    timestamp: i64,
    amount: u64,
}

impl Set {
    pub fn new(instance: u8, timestamp: i64, amount: u64) -> Set {
        Set {
            instance,
            timestamp,
            amount,
        }
    }

    pub fn instance(&self) -> &u8 {
        &self.instance
    }

    pub fn timestamp(&self) -> &i64 {
        &self.timestamp
    }

    pub fn amount(&self) -> &u64 {
        &self.amount
    }

    /// Parse a `Set` instance from a received frame.
    ///
    /// The `Parse` argument provides a cursor-like API to read fields from the
    /// `Frame`. At this point, the entire frame has already been received from
    /// the socket.
    ///
    /// The `SET` string has already been consumed.
    ///
    /// # Returns
    ///
    /// Returns the OK value on success. If the frame is malformed, `Err` is
    /// returned.
    ///
    /// # Format
    ///
    /// Expects an array frame containing 4 entries.
    ///
    /// ```text
    /// SET instance timestamp amount
    /// ```
    pub fn parse_frames(parse: &mut Parse) -> crate::Result<Set> {
        let instance = parse.next_instance()?;

        let timestamp = parse.next_timestamp()?;

        let amount = parse.next_int()?;

        Ok(Set { instance, timestamp, amount })
    }

    /// Apply the `Set` command to the specified `Db` instance.
    ///
    /// The response is written to `dst`. This is called by the server in order
    /// to execute a received command.
    pub async fn apply(self, db: &Db, dst: &mut Connection) -> crate::Result<()> {
        db.set(self.instance, self.timestamp, self.amount);

        let response = Frame::Simple("OK".to_string());
        dst.write_frame(&response).await?;

        Ok(())
    }

    /// Converts the command into an equivalent `Frame`.
    ///
    /// This is called by the client when encoding a `Set` command to send to
    /// the server.
    pub fn into_frame(self) -> Frame {
        let mut frame = Frame::array();
        frame.push_bulk(Bytes::from("set".as_bytes()));
        frame.push_instance(self.instance);
        frame.push_timestamp(self.timestamp);
        frame.push_int(self.amount);

        frame
    }
}