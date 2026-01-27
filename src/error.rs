#[derive(Debug)]
pub enum Error {
    Io(tokio::io::Error),
    Protocol(ProtocolError),
    Command(CommandError),
}
impl From<tokio::io::Error> for Error {
    fn from(value: tokio::io::Error) -> crate::error::Error {
        Error::Io(value)
    }
}

#[derive(Debug)]
pub enum ProtocolError {
    WrongArity,
    UnknownCommand
}

#[derive(Debug)]
pub enum CommandError {
    KeyNotFound,
}
