use tokio::io::{BufReader, BufWriter};
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use crate::store::Store;
use crate::error::Error;

pub struct Connection {
    reader: BufReader<OwnedReadHalf>,
    writer: BufWriter<OwnedWriteHalf>,
    store: Store,
}

enum Command {}


enum Response {}

impl Connection {
    pub fn new(stream: TcpStream, store: Store) -> Self {
        unimplemented!("Connection constructor unimplemented");
    }
    
    async fn read_command(&mut self) -> Result<Command, Error> {
        unimplemented!("Read Command unimplemented");
    }

    async fn process_command(&mut self, command: Command) -> Result<Response, Error> {
        unimplemented!("Process Command unimplemented");
    }

    async fn send_response(&mut self, response: Response) -> Result<(), Error> {
        unimplemented!("Send response unimplemented")
    }
}