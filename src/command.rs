use crate::error::Error;
use crate::frame::Frame;

#[derive(PartialEq, Eq, Debug)]
pub enum Command {
    PING,
    GET { key: Vec<u8> },
    SET { key: Vec<u8>, value: Vec<u8> },
    DEL { key: Vec<u8> },
    QUIT,
    NOOP,
}

impl TryFrom<Frame> for Command {
    type Error = Error;
    fn try_from(value: Frame) -> Result<Self, Self::Error> {
        let args = match value {
            Frame::Array(Some(inner)) if !inner.is_empty() => inner,
            _ => return Err(Error::InvalidCommandFrame),
        };

        let args: Vec<Vec<u8>> = args
            .into_iter()
            .map(|a| match a {
                Frame::Bulk(Some(i)) => Ok(i),
                _ => Err(Error::InvalidCommandFrame),
            })
            .collect::<Result<_, _>>()?;

        Err(Error::UnknownCommand)
    }
}
