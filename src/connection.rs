use std::io::Read;

use tokio::io::{BufReader, BufWriter};
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use crate::store::Store;

pub struct Connection {
    reader: BufReader<OwnedReadHalf>,
    writer: BufWriter<OwnedWriteHalf>,
    store: Store,
}

enum Command {}

enum ConnectionError {
    ReadError,
    ProcessError,
    ResponseError
}

enum Response {}

impl Connection {
    pub fn new(stream: TcpStream, store: Store) -> Self {
        unimplemented!("Connection constructor unimplemented");
    }
    
    async fn read_command(&mut self) -> Result<Command, ConnectionError> {
        unimplemented!("Read Command unimplemented");
    }

    async fn process_command(&mut self, command: Command) -> Result<Response, ConnectionError> {
        unimplemented!("Process Command unimplemented");
    }

    async fn send_response(&mut self, response: Response) -> Result<(), ConnectionError> {
        unimplemented!("Send response unimplemented")
    }
}