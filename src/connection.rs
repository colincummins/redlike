#![allow(clippy::upper_case_acronyms)]
use tokio::io::{AsyncBufReadExt};
use crate::store::Store;
use crate::error::Error;

pub struct Connection<R, W> {
    reader: R,
    writer: W,
    store: Store,
}

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
R: AsyncBufReadExt + Unpin,
W: Unpin, 
{
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
            ("PING", []) => return Ok(Some(Command::PING)),
            ("PING", _) => return Err(Error::WrongArity { command: "PING".into(), given: 1, expected: 0 }),
            (_, _) => return Err(Error::UnknownCommand),
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