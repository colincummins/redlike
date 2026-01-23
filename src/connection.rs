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
        let (reader, writer) = stream.into_split();
        let reader = BufReader::new(reader);
        let writer = BufWriter::new(writer);
        Self {
            reader,
            writer,
            store,
        }
    }
    
    fn read_command(&mut self) -> Result<Command, Error> {
        unimplemented!("Read Command unimplemented");
    }

    fn process_command(&mut self, command: Command) -> Result<Response, Error> {
        unimplemented!("Process Command unimplemented");
    }

    fn send_response(&mut self, response: Response) -> Result<(), Error> {
        unimplemented!("Send response unimplemented")
    }
    
    fn run(&mut self, response: Response) -> Result<(), Error> {
        unimplemented!("run implemented")
    }
}