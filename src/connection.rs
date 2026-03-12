#![allow(clippy::upper_case_acronyms)]
use crate::command::Command;
use crate::error::Error;
use crate::frame::Frame;
use crate::parser::{ParseResult, Parser};
use crate::store::Store;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader, BufWriter};

pub struct Connection<R, W> {
    reader: BufReader<R>,
    writer: BufWriter<W>,
    store: Store,
}

#[derive(PartialEq, Eq, Debug)]
enum ProcessOutcome {
    Quit,
    Noop,
    Respond(Frame),
}

impl<R, W> Connection<R, W>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    pub fn new(reader: R, writer: W, store: Store) -> Self {
        Connection {
            reader: BufReader::new(reader),
            writer: BufWriter::new(writer),
            store,
        }
    }

    async fn process_command(&mut self, command: Command) -> ProcessOutcome {
        match command {
            Command::NOOP => ProcessOutcome::Noop,
            Command::QUIT => ProcessOutcome::Quit,
            Command::PING => ProcessOutcome::Respond(Frame::SimpleString("PONG".into())),
            Command::SET { key, value } => {
                self.store.set(key, value).await;
                ProcessOutcome::Respond(Frame::SimpleString("OK".into()))
            }
            Command::GET { key } => {
                ProcessOutcome::Respond(Frame::Bulk(self.store.get(&key).await))
            }
            Command::DEL { key } => {
                let deleted = self.store.del(&key).await.map(|_| 1).unwrap_or(0);
                ProcessOutcome::Respond(Frame::Integer(deleted.into()))
            }
            Command::EXPIRE { key: _, value: _ } => {
                todo!();
            }
        }
    }

    async fn send_response(&mut self, response: Frame) -> Result<(), Error> {
        self.writer
            .write_all(response.to_bytes().as_slice())
            .await?;
        self.writer.flush().await?;
        Ok(())
    }

    pub async fn run(&mut self) -> Result<(), Error> {
        let mut p = Parser::new();
        let mut buf = Vec::<u8>::new();
        loop {
            buf.clear();
            self.reader.read_buf(&mut buf).await?;
            if buf.is_empty() {
                return Ok(());
            }

            let (frames, halting_error) = match p.parse(&buf) {
                ParseResult::Complete(f) => (f, None),
                ParseResult::Partial(f, e) => (f, Some(e)),
            };

            for f in frames {
                let outcome: ProcessOutcome = match Command::try_from(f) {
                    Ok(cmd) => self.process_command(cmd).await,
                    Err(Error::UnknownCommand) => {
                        ProcessOutcome::Respond(Frame::SimpleError("Unknown Command".into()))
                    }
                    Err(Error::WrongArity {
                        command: _,
                        given: _,
                        expected: _,
                    }) => ProcessOutcome::Respond(Frame::SimpleError(
                        "Wrong number of arguments".into(),
                    )),
                    Err(Error::WrongArgumentType) => {
                        ProcessOutcome::Respond(Frame::SimpleError("Wrong Argument Type".into()))
                    }
                    Err(Error::Io(_e)) => return Ok(()),
                    Err(Error::InvalidCommandFrame) => return Ok(()),
                };
                match outcome {
                    ProcessOutcome::Noop => continue,
                    ProcessOutcome::Quit => {
                        return Ok(());
                    }
                    ProcessOutcome::Respond(r) => self.send_response(r).await?,
                }
            }
            if halting_error.is_some() {
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt, Sink, sink, split};

    use super::*;

    fn setup_dummy_connection() -> Connection<tokio::io::Empty, Sink> {
        let store: Store = Store::new();
        Connection::new(tokio::io::empty(), sink(), store)
    }

    #[tokio::test]
    async fn responds_to_ping() {
        let mut conn = setup_dummy_connection();
        let response = conn.process_command(Command::PING).await;
        assert_eq!(
            response,
            ProcessOutcome::Respond(Frame::SimpleString("PONG".to_string()))
        )
    }

    #[tokio::test]
    async fn noop_gives_noop_outcome() {
        let mut conn = setup_dummy_connection();
        let response = conn.process_command(Command::NOOP).await;
        assert_eq!(response, ProcessOutcome::Noop)
    }

    #[tokio::test]
    async fn set_sends_ok_response() {
        let mut conn = setup_dummy_connection();
        let response = conn
            .process_command(Command::SET {
                key: "mykey".into(),
                value: "myvalue".into(),
            })
            .await;
        assert_eq!(
            response,
            ProcessOutcome::Respond(Frame::SimpleString("OK".into()))
        )
    }

    #[tokio::test]
    async fn set_then_get() {
        let mut conn = setup_dummy_connection();
        let response = conn
            .process_command(Command::SET {
                key: "mykey".into(),
                value: "myvalue".into(),
            })
            .await;
        assert_eq!(
            response,
            ProcessOutcome::Respond(Frame::SimpleString("OK".into()))
        );
        let response = conn
            .process_command(Command::GET {
                key: "mykey".into(),
            })
            .await;
        assert_eq!(
            response,
            ProcessOutcome::Respond(Frame::Bulk(Some("myvalue".into())))
        )
    }

    #[tokio::test]
    async fn get_nonexistent_key_returns_null_bulk_response() {
        let mut conn = setup_dummy_connection();
        let response = conn
            .process_command(Command::GET {
                key: "mykey".into(),
            })
            .await;
        assert_eq!(response, ProcessOutcome::Respond(Frame::Bulk(None)))
    }

    #[tokio::test]
    async fn delete_existing_key() {
        let mut conn = setup_dummy_connection();
        let _ = conn
            .process_command(Command::SET {
                key: "mykey".into(),
                value: "myvalue".into(),
            })
            .await;
        let response = conn
            .process_command(Command::DEL {
                key: "mykey".into(),
            })
            .await;
        assert_eq!(response, ProcessOutcome::Respond(Frame::Integer(1)))
    }

    #[tokio::test]
    async fn delete_nonexistent_key() {
        let mut conn = setup_dummy_connection();
        let response = conn
            .process_command(Command::DEL {
                key: "mykey".into(),
            })
            .await;
        assert_eq!(response, ProcessOutcome::Respond(Frame::Integer(0)))
    }

    #[tokio::test]
    async fn send_response() {
        let (client, server) = tokio::io::duplex(64);
        let mut client_reader = BufReader::new(client);
        let store = Store::new();
        let mut conn = Connection::new(tokio::io::empty(), server, store);
        conn.send_response(Frame::SimpleString("OK".into()))
            .await
            .unwrap();
        let mut buf = [0; 5];
        client_reader.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"+OK\r\n");
        conn.send_response(Frame::SimpleError("Test".into()))
            .await
            .unwrap();
        let mut buf = [0; 7];
        client_reader.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"-Test\r\n")
    }

    struct TestCase<'a> {
        call: &'a [u8],
        response: &'a [u8],
        expected: &'a str,
    }

    #[tokio::test]
    async fn e2e_run() {
        let test_cases = vec![
            TestCase {
                call: b"*1\r\n$4\r\nPING\r\n",
                response: b"+PONG\r\n",
                expected: "Should respond to PING with PONG",
            },
            TestCase {
                call: b"*3\r\n$3\r\nSET\r\n$5\r\nmykey\r\n$7\r\nmyvalue\r\n",
                response: b"+OK\r\n",
                expected: "Should respond to SET with OK",
            },
            TestCase {
                call: b"*2\r\n$3\r\nGET\r\n$5\r\nmykey\r\n",
                response: b"$7\r\nmyvalue\r\n",
                expected: "Should retrieve value of mykey: myvalue",
            },
            TestCase {
                call: b"*2\r\n$3\r\nGET\r\n$8\r\notherkey\r\n",
                response: b"$-1\r\n",
                expected: "Missing keys return null bulk strings",
            },
            TestCase {
                call: b"*2\r\n$3\r\nDEL\r\n$5\r\nmykey\r\n",
                response: b":1\r\n",
                expected: "Should return 1 if key is successfully deleted",
            },
            TestCase {
                call: b"*2\r\n$3\r\nDEL\r\n$5\r\nmykey\r\n",
                response: b":0\r\n",
                expected: "Should return 0 if DEL called on a key with no value",
            },
            TestCase {
                call: b"*1\r\n$3\r\nFOO\r\n",
                response: b"-Unknown Command\r\n",
                expected: "Unknown command gives error",
            },
            TestCase {
                call: b"*4\r\n$3\r\nSET\r\n$5\r\nmykey\r\n$7\r\nmyvalue\r\n$8\r\ntoo many\r\n",
                response: b"-Wrong number of arguments\r\n",
                expected: "Wrong number of arguments gives error",
            },
            TestCase {
                call: b"*1\r\n$3\r\nGET\r\n",
                response: b"-Wrong number of arguments\r\n",
                expected: "Wrong number of arguments gives error",
            },
        ];

        let (client, server) = tokio::io::duplex(128);
        let (reader, writer) = split(server);
        let store = Store::new();
        let mut conn = Connection::new(reader, writer, store);

        let (reader, writer) = split(client);

        let mut reader = BufReader::new(reader);
        let mut writer = BufWriter::new(writer);

        let handle = tokio::spawn(async move { conn.run().await });

        for TestCase {
            call,
            response,
            expected,
        } in test_cases
        {
            writer.write_all(call).await.unwrap();
            writer.flush().await.unwrap();
            let mut read_buffer = vec![0; response.len()];
            reader.read_exact(&mut read_buffer).await.unwrap();
            assert_eq!(response, read_buffer.as_slice(), "{}", expected);
        }

        let mut read_buffer = [0; 7];
        writer.write_all(b"\n").await.unwrap();
        writer.flush().await.unwrap();
        writer.write_all(b"*1\r\n$4\r\nPING\r\n").await.unwrap();
        writer.flush().await.unwrap();
        reader.read_exact(&mut read_buffer).await.unwrap();
        assert_eq!(
            &read_buffer, b"+PONG\r\n",
            "NOOP should not return anything"
        );

        let mut read_buffer = [0; 1];
        writer.write_all(b"*1\r\n$4\r\nQUIT\r\n").await.unwrap();
        writer.flush().await.unwrap();
        assert_eq!(
            reader.read(&mut read_buffer).await.unwrap(),
            0,
            "QUIT should close connection"
        );

        handle.await.unwrap().unwrap();
    }
}
