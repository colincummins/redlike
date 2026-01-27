use tokio::io::{AsyncBufReadExt, BufReader, BufWriter};
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use crate::store::Store;
use crate::error::{Error, ProtocolError};

pub struct Connection {
    reader: BufReader<OwnedReadHalf>,
    writer: BufWriter<OwnedWriteHalf>,
    store: Store,
}

enum Command {
    PING,
    GET{key: String},
    SET{key: String, value: String},
    DEL{key: String},
    QUIT,
    EMPTY
}


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
    
    async fn read_command(&mut self) -> Result<Option<Command>, Error> {
        let mut line = String::new();
        
        if self.reader.read_line(& mut line).await? == 0 {
            return Ok(None)
        };

        let mut parts = line.split_whitespace();
        let Some(c) = parts.next() else {
            return Ok(Some(Command::EMPTY));
        }

        return Ok(None)
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