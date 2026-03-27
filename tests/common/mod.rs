#![allow(dead_code)]

use redlike::server::ServerError;
use tokio::io;

pub mod setup_test_server;
pub mod test_case;
pub mod test_client;

pub fn server_error_to_io(err: ServerError) -> io::Error {
    match err {
        ServerError::Io(err) => err,
        ServerError::Archive(err) => io::Error::other(err),
        ServerError::InvalidAuthFile => io::Error::other(ServerError::InvalidAuthFile),
        ServerError::UnreadableAuthFile(err) => io::Error::other(err),
    }
}
