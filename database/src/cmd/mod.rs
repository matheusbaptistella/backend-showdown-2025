mod get;
pub use get::Get;

mod set;
pub use set::Set;

mod unknown;
pub use unknown::Unknown;

pub enum Command {
    Get(Get),
    Set(Set),
    Unknown(Unknown),
}

use crate::{Connection, Db, Frame, Parse, ParseError};

impl Command {
    pub fn from_frame(frame: Frame) -> crate::Result<Command> {
        let mut parse = Parse::new(frame)?;

        let command_name = parse.next_string()?.to_lowercase();

        let command = match &command_name[..] {
            "get" => Command::Get(Get::parse_frames(&mut parse)?),
            "set" => Command::Set(Set::parse_frames(&mut parse)?),
            _ => {
                return Ok(Command::Unknown(Unknown::new(command_name)));
            }
        };

        parse.finish()?;

        Ok(command)
    }

    pub async fn apply(
        self,
        db: &Db,
        dst: &mut Connection,
    ) -> crate::Result<()> {
        use Command::*;

        match self {
            Get(cmd) => cmd.apply(db, dst).await,
            Set(cmd) => cmd.apply(db, dst).await,
            Unknown(cmd) => cmd.apply(dst).await,
        }
    }

    pub fn get_name(&self) -> &str {
        match self {
            Command::Get(_) => "get",
            Command::Set(_) => "set",
            Command::Unknown(cmd) => cmd.get_name(),
        }
    }
}