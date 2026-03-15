use crate::parser::ParseError;

#[derive(Debug)]
pub enum Error {
    Io(tokio::io::Error),
    WrongArity {
        command: String,
        given: usize,
        expected: usize,
    },
    UnknownCommand,
    InvalidCommandFrame,
    WrongArgumentType,
}
impl From<tokio::io::Error> for Error {
    fn from(value: tokio::io::Error) -> crate::error::Error {
        Error::Io(value)
    }
}

impl From<ParseError> for Error {
    fn from(_: ParseError) -> crate::error::Error {
        Error::InvalidCommandFrame
    }
}
