use crate::error::Error;
use crate::frame::Frame;

#[derive(PartialEq, Eq, Debug)]
pub enum Command {
    PING,
    GET { key: String },
    SET { key: String, value: String },
    DEL { key: String },
    QUIT,
    NOOP,
}

impl TryFrom<Frame> for Command {
    type Error = Error;
    fn try_from(value: Frame) -> Result<Self, Self::Error> {
        Err(Error::UnknownCommand)
    }
}
