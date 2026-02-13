#![allow(clippy::upper_case_acronyms)]
use std::fmt::format;

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
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


#[derive(PartialEq, Eq, Debug)]
enum Response {
    Simple(String),
    Error(String),
}

#[derive(PartialEq, Eq, Debug)]
enum ProcessOutcome {
    Quit,
    Noop,
    Respond(Response)
}

impl <R,W> Connection<R,W> where
R: AsyncRead + Unpin,
W: AsyncWrite + Unpin, 
{
    pub fn new(reader: R, writer: W, store: Store) -> Self {
        Connection {
            reader: BufReader::new(reader),
            writer,
            store
        }
    }
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
            ("SET", [key, value]) => Ok(Some(Command::SET{key: key.to_string(), value: value.to_string()})),
            ("SET", rest @ [..]) => Err(Error::WrongArity { command: "SET".into(), given: rest.len(), expected: 2 }),
            ("DEL", [key]) => Ok(Some(Command::DEL{key: key.to_string()})),
            ("DEL", rest) => Err(Error::WrongArity { command: "DEL".into(), given: rest.len(), expected: 1 }),
            ("QUIT", []) => Ok(Some(Command::QUIT)),
            ("QUIT", rest) => Err(Error::WrongArity { command: "QUIT".into(), given: rest.len(), expected: 0 }),
            (_, _) => Err(Error::UnknownCommand),
        }

    }

    #[allow(dead_code)]
    async fn process_command(&mut self, command: Command) -> ProcessOutcome {
        match command {
            Command::NOOP => ProcessOutcome::Noop,
            Command::QUIT => ProcessOutcome::Quit,
            Command::PING => ProcessOutcome::Respond(Response::Simple("PONG".into())),
            Command::SET {key, value} => {
                self.store.set(key, value).await;
                ProcessOutcome::Respond(Response::Simple("OK".into()))
            },
            Command::GET {key} => ProcessOutcome::Respond(Response::Simple(self.store.get(&key).await.unwrap_or_default())),
            Command::DEL {key} => {
                let deleted = self.store.del(&key).await
                .map(|_| "1")
                .unwrap_or("0");
                ProcessOutcome::Respond(Response::Simple(deleted.into()))
            }
        }
    }

    #[allow(dead_code)]
    async fn send_response(&mut self, response: Response) -> Result<(), Error> {
        let message_text = match response {
            Response::Error(inner) => format!("ERR {}\n", inner),
            Response::Simple(inner)=> format!("{}\n", inner)
        };
        self.writer.write_all(message_text.as_bytes()).await?;
        self.writer.flush().await?;
        Ok(())
    }

    #[allow(dead_code)] 
    async fn run(&mut self) -> Result<(), Error> {
        loop {
            let outcome = match self.read_command().await {
                Ok(None) => break,
                Ok(Some(Command::NOOP)) => continue,
                Ok(Some(command)) => self.process_command(command).await,
                Err(Error::UnknownCommand) => ProcessOutcome::Respond(Response::Error("Unknown Command".into())),
                Err(Error::WrongArity { command:_, given:_, expected:_ }) => ProcessOutcome::Respond(Response::Error("Wrong number or arguments".into())),
                Err(Error::Io(e)) => break,
            };
            match outcome {
                ProcessOutcome::Noop => continue,
                ProcessOutcome::Quit => break,
                ProcessOutcome::Respond(r) => self.send_response(r).await?
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncWriteExt, DuplexStream, Sink, duplex};

    use super::*;

    fn setup_connection () -> (Connection<tokio::io::DuplexStream, Sink>, DuplexStream) {
        let (client, server) = duplex(64);
        let store:Store = Store::new();
        let connection: Connection<tokio::io::DuplexStream, _> = Connection::new(server, sink(), store);
        (connection, client)
    }

    fn setup_dummy_connection () -> Connection<tokio::io::Empty, Sink> {
        let store:Store = Store::new();
        Connection::new(tokio::io::empty(), sink(), store)
    }

    #[tokio::test]
    async fn eol_returns_none () {
        let mut connection = setup_dummy_connection();
        let cmd = connection.read_command().await.unwrap();
        assert_eq!(cmd, None);
    }

    #[tokio::test]
    async fn blank_line_returns_noop () {
        let (mut connection, mut client) = setup_connection();
        client.write_all(b"\n").await.unwrap();
        let cmd = connection.read_command().await.unwrap();
        assert_eq!(cmd, Some(Command::NOOP));
    }

    #[tokio::test]
    async fn successful_read_ping () {
        let (mut connection, mut client) = setup_connection();
        client.write_all(b"PING\n").await.unwrap();
        let cmd = connection.read_command().await.unwrap();
        assert_eq!(cmd, Some(Command::PING));
    }

    #[tokio::test]
    async fn reject_bad_arity_ping () {
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
    async fn reject_bad_arity_get () {
        let (mut connection, mut client) = setup_connection();
        let _ = client.write_all(b"GET\n").await;
        let result = connection.read_command().await.unwrap_err();
        assert!(matches!(result, Error::WrongArity { command, given: 0, expected: 1 } if command == "GET"));
        let _ = client.write_all(b"GET too many\n").await;
        let result = connection.read_command().await.unwrap_err();
        assert!(matches!(result, Error::WrongArity { command, given: 2, expected: 1 } if command == "GET"));
    }

    #[tokio::test]
    async fn successful_read_set () {
        let (mut connection, mut client) = setup_connection();
        client.write_all(b"set mykey myvalue\n").await.unwrap();
        let cmd = connection.read_command().await.unwrap();
        assert_eq!(cmd, Some(Command::SET { key: "mykey".to_string(), value: "myvalue".to_string()}));
    }

    #[tokio::test]
    async fn reject_bad_arity_set () {
        let (mut connection, mut client) = setup_connection();
        let _ = client.write_all(b"set\n").await;
        let result = connection.read_command().await.unwrap_err();
        assert!(matches!(result, Error::WrongArity { command, given: 0, expected: 2 } if command == "SET"));
        let _ = client.write_all(b"set mykey\n").await;
        let result = connection.read_command().await.unwrap_err();
        assert!(matches!(result, Error::WrongArity { command, given: 1, expected: 2 } if command == "SET"));
        let _ = client.write_all(b"set mykey myvalue extra\n").await;
        let result = connection.read_command().await.unwrap_err();
        assert!(matches!(result, Error::WrongArity { command, given: 3, expected: 2 } if command == "SET"));
    }

    #[tokio::test]
    async fn successful_read_del () {
        let (mut connection, mut client) = setup_connection();
        client.write_all(b"del mykey\n").await.unwrap();
        let cmd = connection.read_command().await.unwrap();
        assert_eq!(cmd, Some(Command::DEL { key: "mykey".to_string()}));
    }

    #[tokio::test]
    async fn reject_bad_arity_del () {
        let (mut connection, mut client) = setup_connection();
        let _ = client.write_all(b"del\n").await;
        let result = connection.read_command().await.unwrap_err();
        assert!(matches!(result, Error::WrongArity { command, given: 0, expected: 1 } if command == "DEL"));
        let _ = client.write_all(b"del too many\n").await;
        let result = connection.read_command().await.unwrap_err();
        assert!(matches!(result, Error::WrongArity { command, given: 2, expected: 1 } if command == "DEL"));
    }

    #[tokio::test]
    async fn successful_read_quit () {
        let (mut connection, mut client) = setup_connection();
        client.write_all(b"quit\n").await.unwrap();
        let cmd = connection.read_command().await.unwrap();
        assert_eq!(cmd, Some(Command::QUIT));
    }

    #[tokio::test]
    async fn reject_bad_arity_quit () {
        let (mut connection, mut client) = setup_connection();
        let _ = client.write_all(b"quit extra words\n").await;
        let result = connection.read_command().await.unwrap_err();
        assert!(matches!(result, Error::WrongArity { command, given: 2, expected: 0 } if command == "QUIT"));
    }

    #[tokio::test]
    async fn reject_unknown_commands () {
        let (mut connection, mut client) = setup_connection();
        let _ = client.write_all(b"FOO\n").await;
        let result = connection.read_command().await.unwrap_err();
        assert!(matches!(result, Error::UnknownCommand));
    }

    #[tokio::test]
    async fn responds_to_ping () {
        let mut conn = setup_dummy_connection();
        let response = conn.process_command(Command::PING).await;
        assert_eq!(response, ProcessOutcome::Respond(Response::Simple("PONG".to_string())))
    }

    #[tokio::test]
    async fn noop_gives_noop_outcome () {
        let mut conn = setup_dummy_connection();
        let response = conn.process_command(Command::NOOP).await;
        assert_eq!(response, ProcessOutcome::Noop)
    }

    #[tokio::test]
    async fn set_sends_ok_response () {
        let mut conn = setup_dummy_connection();
        let response = conn.process_command(Command::SET { key: "mykey".into(), value: "myvalue".into() }).await;
        assert_eq!(response, ProcessOutcome::Respond(Response::Simple("OK".into())))
    }

    #[tokio::test]
    async fn set_then_get () {
        let mut conn = setup_dummy_connection();
        let response = conn.process_command(Command::SET { key: "mykey".into(), value: "myvalue".into() }).await;
        assert_eq!(response, ProcessOutcome::Respond(Response::Simple("OK".into())));
        let response = conn.process_command(Command::GET { key: "mykey".into() }).await;
        assert_eq!(response, ProcessOutcome::Respond(Response::Simple("myvalue".into())))
    }

    #[tokio::test]
    async fn get_nonexistent_key_returns_empty_string_response () {
        let mut conn = setup_dummy_connection();
        let response = conn.process_command(Command::GET { key: "mykey".into() }).await;
        assert_eq!(response, ProcessOutcome::Respond(Response::Simple(String::new())))
    }

    #[tokio::test]
    async fn delete_existing_key () {
        let mut conn = setup_dummy_connection();
        let _ = conn.process_command(Command::SET { key: "mykey".into(), value: "myvalue".into() }).await;
        let response = conn.process_command(Command::DEL { key: "mykey".into() }).await;
        assert_eq!(response, ProcessOutcome::Respond(Response::Simple("1".into())))
    }

    #[tokio::test]
    async fn delete_nonexistent_key () {
        let mut conn = setup_dummy_connection();
        let response = conn.process_command(Command::DEL { key: "mykey".into() }).await;
        assert_eq!(response, ProcessOutcome::Respond(Response::Simple("0".into())))
    }

    #[tokio::test]
    async fn send_response () {
        let (client, server) = tokio::io::duplex(64);
        let mut client_reader = BufReader::new(client);
        let store = Store::new();
        let mut conn = Connection::new(tokio::io::empty(), server, store);
        conn.send_response(Response::Simple("OK".into())).await.unwrap();
        let mut buf = String::new();
        client_reader.read_line(&mut buf).await.unwrap();
        assert_eq!(buf, "OK\n".to_string());
        conn.send_response(Response::Error("Test".into())).await.unwrap();
        let mut buf = String::new();
        client_reader.read_line(&mut buf).await.unwrap();
        assert_eq!(buf, "ERR Test\n".to_string());
    }
}