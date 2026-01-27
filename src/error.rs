#[derive(Debug)]
pub enum Error {
    Io(tokio::io::Error),
    WrongArity{
        command: String,
        given: usize,
        expected: usize,
    },
    UnknownCommand,
    KeyNotFound,
}
impl From<tokio::io::Error> for Error {
    fn from(value: tokio::io::Error) -> crate::error::Error {
        Error::Io(value)
    }
}