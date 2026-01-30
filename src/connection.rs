#![allow(clippy::upper_case_acronyms)]
use tokio::io::{AsyncBufReadExt, BufReader, AsyncRead, AsyncWrite};
use tokio::io::sink;
use crate::store::Store;
use crate::error::Error;

pub struct Connection<R, W> {
    reader: BufReader<R>,
    writer: W,
    store: Store,
}

#[derive(PartialEq, Eq, Debug)]
enum Command {
    PING,
    GET{key: String},
    SET{key: String, value: String},
    DEL{key: String},
    QUIT,
    NOOP
}


enum Response {}

impl <R,W> Connection<R,W> where
R: AsyncRead + Unpin,
W: AsyncWrite + Unpin, 
{
    fn new(reader: R, writer: W, store: Store) -> Self {
        Connection {
            reader: BufReader::new(reader),
            writer,
            store
        }
    }
    #[allow(dead_code)]
    async fn read_command(&mut self) -> Result<Option<Command>, Error> {
        let mut line = String::new();
        
        if self.reader.read_line(& mut line).await? == 0 {
            return Ok(None)
        };

        let mut args = line.split_whitespace();
        let Some(c) = args.next() else {
            return Ok(Some(Command::NOOP));
        };
        let args: Vec<&str> = args.collect();
        match (c.to_ascii_uppercase().as_str(), args.as_slice()) {
            ("PING", []) => Ok(Some(Command::PING)),
            ("PING", rest) => Err(Error::WrongArity { command: "PING".into(), given: rest.len(), expected: 0 }),
            ("GET", [key]) => Ok(Some(Command::GET{key: key.to_string()})),
            ("GET", rest) => Err(Error::WrongArity { command: "GET".into(), given: rest.len(), expected: 1 }),
            (_, _) => Err(Error::UnknownCommand),
        }

    }

    #[allow(dead_code)]
    fn process_command(&mut self, command: Command) -> Result<Response, Error> {
        todo!()
    }

    #[allow(dead_code)]
    fn send_response(&mut self, response: Response) -> Result<(), Error> {
        todo!()
    }

    #[allow(dead_code)] 
    fn run(&mut self, response: Response) -> Result<(), Error> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncWriteExt, DuplexStream, Sink, duplex};

    use super::*;

    fn setup_connection () -> (Connection<tokio::io::DuplexStream, Sink>, DuplexStream) {
        let (client, server) = duplex(64);
        let store:Store = Default::default();
        let connection: Connection<tokio::io::DuplexStream, _> = Connection::new(server, sink(), store);
        (connection, client)
    }

    #[tokio::test]
    async fn successful_read_ping () {
        let (mut connection, mut client) = setup_connection();
        client.write_all(b"PING\n").await.unwrap();
        let cmd = connection.read_command().await.unwrap();
        assert_eq!(cmd, Some(Command::PING));
    }

    #[tokio::test]
    async fn fail_read_ping () {
        let (mut connection, mut client) = setup_connection();
        let _ = client.write_all(b"PING extra words\n").await;
        let result = connection.read_command().await.unwrap_err();
        assert!(matches!(result, Error::WrongArity { command, given: 2, expected: 0 } if command == "PING"));
    }
    
    #[tokio::test]
    async fn successful_read_get () {
        let (mut connection, mut client) = setup_connection();
        client.write_all(b"get mykey\n").await.unwrap();
        let cmd = connection.read_command().await.unwrap();
        assert_eq!(cmd, Some(Command::GET { key: "mykey".to_string()}));
    }

    #[tokio::test]
    async fn fail_read_get () {
        let (mut connection, mut client) = setup_connection();
        let _ = client.write_all(b"GET\n").await;
        let result = connection.read_command().await.unwrap_err();
        assert!(matches!(result, Error::WrongArity { command, given: 0, expected: 1 } if command == "GET"));
        let _ = client.write_all(b"GET too many\n").await;
        let result = connection.read_command().await.unwrap_err();
        assert!(matches!(result, Error::WrongArity { command, given: 2, expected: 1 } if command == "GET"));
    }
}