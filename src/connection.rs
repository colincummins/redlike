#![allow(clippy::upper_case_acronyms)]
use crate::command::Command;
use crate::config::SharedAuthPassword;
use crate::error::Error;
use crate::frame::Frame;
use crate::parser::{ParseResult, Parser};
use crate::store::Store;
use constant_time_eq::constant_time_eq;
use secrecy::ExposeSecret;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader, BufWriter};
use tokio::select;
use tokio_util::sync::CancellationToken;

pub struct Connection<R, W> {
    reader: BufReader<R>,
    writer: BufWriter<W>,
    store: Store,
    shutdown_token: CancellationToken,
    auth_password: SharedAuthPassword,
    authenticated: bool,
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
    pub fn new(
        reader: R,
        writer: W,
        store: Store,
        shutdown_token: CancellationToken,
        auth_password: SharedAuthPassword,
    ) -> Self {
        Connection {
            reader: BufReader::new(reader),
            writer: BufWriter::new(writer),
            store,
            shutdown_token,
            auth_password,
            authenticated: false,
        }
    }

    fn is_authorized(&self) -> bool {
        self.auth_password.as_ref().is_none() || self.authenticated
    }

    fn authenticate_connection(&mut self, password: Vec<u8>) -> ProcessOutcome {
        match self.auth_password.as_ref() {
            None => ProcessOutcome::Respond(Frame::SimpleError(
                "AUTH called without any password configured".into(),
            )),
            Some(secret) => match constant_time_eq(secret.expose_secret(), password.as_slice()) {
                true => {
                    self.authenticated = true;
                    ProcessOutcome::Respond(Frame::SimpleString("OK".into()))
                }
                false => {
                    ProcessOutcome::Respond(Frame::SimpleError("AUTH invalid password".into()))
                }
            },
        }
    }

    fn can_execute(&self, command: &Command) -> bool {
        matches!(
            command,
            Command::AUTH { .. } | Command::PING | Command::QUIT | Command::NOOP
        ) || self.is_authorized()
    }

    async fn process_command(&mut self, command: Command) -> ProcessOutcome {
        if !self.can_execute(&command) {
            return ProcessOutcome::Respond(Frame::SimpleError(
                "NOAUTH Authentication required".into(),
            ));
        }
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
            Command::EXPIRE { key, value } => {
                ProcessOutcome::Respond(Frame::Integer(self.store.expire(key, value).await as i64))
            }
            Command::TTL { key } => {
                ProcessOutcome::Respond(Frame::Integer(self.store.ttl(key).await))
            }
            //TODO: Implement handling
            Command::AUTH { key } => self.authenticate_connection(key),
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
            select! {
                read_result = self.reader.read_buf(&mut buf) => {read_result?;},
                _ = self.shutdown_token.cancelled() => {break;}
            }
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
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::io::{AsyncReadExt, AsyncWriteExt, Sink, sink, split};

    use secrecy::SecretBox;

    use super::*;

    fn dummy_shutdown_token() -> CancellationToken {
        CancellationToken::new()
    }

    fn dummy_auth_password() -> SharedAuthPassword {
        std::sync::Arc::new(None)
    }

    fn setup_dummy_connection() -> Connection<tokio::io::Empty, Sink> {
        let store: Store = Store::new();
        Connection::new(
            tokio::io::empty(),
            sink(),
            store,
            dummy_shutdown_token(),
            dummy_auth_password(),
        )
    }

    fn setup_pwd_protected_connection(password: String) -> Connection<tokio::io::Empty, Sink> {
        let mut connection = setup_dummy_connection();
        connection.auth_password =
            Arc::new(Some(SecretBox::new(Box::new(password.as_bytes().to_vec()))));
        connection
    }

    mod process_command {
        use super::*;

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
        async fn expire_existing_key_returns_one() {
            let mut conn = setup_dummy_connection();
            let _ = conn
                .process_command(Command::SET {
                    key: "mykey".into(),
                    value: "myvalue".into(),
                })
                .await;
            let response = conn
                .process_command(Command::EXPIRE {
                    key: "mykey".into(),
                    value: 60,
                })
                .await;
            assert_eq!(response, ProcessOutcome::Respond(Frame::Integer(1)))
        }

        #[tokio::test]
        async fn expire_missing_key_returns_zero() {
            let mut conn = setup_dummy_connection();
            let response = conn
                .process_command(Command::EXPIRE {
                    key: "mykey".into(),
                    value: 60,
                })
                .await;
            assert_eq!(response, ProcessOutcome::Respond(Frame::Integer(0)))
        }

        #[tokio::test]
        async fn expire_expired_key_returns_zero() {
            let mut conn = setup_dummy_connection();
            let _ = conn
                .process_command(Command::SET {
                    key: "mykey".into(),
                    value: "myvalue".into(),
                })
                .await;
            let _ = conn
                .process_command(Command::EXPIRE {
                    key: "mykey".into(),
                    value: 0,
                })
                .await;
            tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
            let response = conn
                .process_command(Command::EXPIRE {
                    key: "mykey".into(),
                    value: 60,
                })
                .await;
            assert_eq!(response, ProcessOutcome::Respond(Frame::Integer(0)))
        }

        #[tokio::test]
        async fn ttl_missing_key_returns_neg2() {
            let mut conn = setup_dummy_connection();
            let response = conn
                .process_command(Command::TTL {
                    key: "mykey".into(),
                })
                .await;
            assert_eq!(response, ProcessOutcome::Respond(Frame::Integer(-2)))
        }

        #[tokio::test]
        async fn ttl_key_without_expiration_returns_neg1() {
            let mut conn = setup_dummy_connection();
            let _ = conn
                .process_command(Command::SET {
                    key: "mykey".into(),
                    value: "myvalue".into(),
                })
                .await;
            let response = conn
                .process_command(Command::TTL {
                    key: "mykey".into(),
                })
                .await;
            assert_eq!(response, ProcessOutcome::Respond(Frame::Integer(-1)))
        }

        #[tokio::test]
        async fn ttl_existing_key_with_expiration_returns_positive_value() {
            let mut conn = setup_dummy_connection();
            let _ = conn
                .process_command(Command::SET {
                    key: "mykey".into(),
                    value: "myvalue".into(),
                })
                .await;
            let _ = conn
                .process_command(Command::EXPIRE {
                    key: "mykey".into(),
                    value: 60,
                })
                .await;
            let response = conn
                .process_command(Command::TTL {
                    key: "mykey".into(),
                })
                .await;
            assert!(matches!(
                response,
                ProcessOutcome::Respond(Frame::Integer(ttl)) if ttl > 0
            ));
        }

        #[tokio::test]
        async fn ttl_expired_key_returns_neg2() {
            let mut conn = setup_dummy_connection();
            let _ = conn
                .process_command(Command::SET {
                    key: "mykey".into(),
                    value: "myvalue".into(),
                })
                .await;
            let _ = conn
                .process_command(Command::EXPIRE {
                    key: "mykey".into(),
                    value: 0,
                })
                .await;
            tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
            let response = conn
                .process_command(Command::TTL {
                    key: "mykey".into(),
                })
                .await;
            assert_eq!(response, ProcessOutcome::Respond(Frame::Integer(-2)))
        }
    }

    mod auth {

        use super::*;

        #[tokio::test]
        async fn no_password_configured_is_authorized() {
            let connection = setup_dummy_connection();
            assert!(connection.is_authorized());
        }

        #[tokio::test]
        async fn password_configured_without_authentication_is_not_authorized() {
            let connection = setup_pwd_protected_connection("my_password".into());
            assert!(!connection.is_authorized());
        }

        #[tokio::test]
        async fn password_configured_with_authentication_is_authorized() {
            let mut connection = setup_pwd_protected_connection("my_password".into());
            connection.authenticated = true;
            assert!(connection.is_authorized());
        }

        #[tokio::test]
        async fn auth_without_configured_password_returns_error() {
            let mut connection = setup_dummy_connection();
            assert_eq!(
                connection.authenticate_connection("my_password".into()),
                ProcessOutcome::Respond(Frame::SimpleError(
                    "AUTH called without any password configured".into()
                ))
            )
        }

        #[tokio::test]
        async fn invalid_auth_returns_error() {
            let mut connection = setup_pwd_protected_connection("my_password".into());
            assert_eq!(
                connection.authenticate_connection("wrong_password".into()),
                ProcessOutcome::Respond(Frame::SimpleError("AUTH invalid password".into()))
            )
        }

        #[tokio::test]
        async fn valid_auth_sets_authenticated_on_unauthenticated_connection() {
            let mut connection = setup_pwd_protected_connection("my_password".into());
            assert_eq!(
                connection.authenticate_connection("my_password".into()),
                ProcessOutcome::Respond(Frame::SimpleString("OK".into()))
            );
            assert!(connection.authenticated);
        }

        #[tokio::test]
        async fn valid_auth_keeps_authenticated_connection_authenticated() {
            let mut connection = setup_pwd_protected_connection("my_password".into());
            connection.authenticated = true;
            assert_eq!(
                connection.authenticate_connection("my_password".into()),
                ProcessOutcome::Respond(Frame::SimpleString("OK".into()))
            );
            assert!(connection.authenticated);
        }

        #[tokio::test]
        async fn invalid_auth_keeps_authenticated_connection_authenticated() {
            let mut connection = setup_pwd_protected_connection("my_password".into());
            connection.authenticated = true;
            assert_eq!(
                connection.authenticate_connection("wrong_password".into()),
                ProcessOutcome::Respond(Frame::SimpleError("AUTH invalid password".into()))
            );
            assert!(connection.authenticated);
        }

        #[tokio::test]
        async fn invalid_auth_keeps_unauthenticated_connection_unauthenticated() {
            let mut connection = setup_pwd_protected_connection("my_password".into());
            assert_eq!(
                connection.authenticate_connection("wrong_password".into()),
                ProcessOutcome::Respond(Frame::SimpleError("AUTH invalid password".into()))
            );
            assert!(!connection.authenticated);
        }
    }

    mod authorization {
        use super::*;

        fn assert_noauth(response: ProcessOutcome) {
            assert_eq!(
                response,
                ProcessOutcome::Respond(Frame::SimpleError(
                    "NOAUTH Authentication required".into()
                ))
            );
        }

        #[tokio::test]
        async fn protected_command_without_password_configured_executes() {
            let mut connection = setup_dummy_connection();

            let response = connection
                .process_command(Command::SET {
                    key: "mykey".into(),
                    value: "myvalue".into(),
                })
                .await;

            assert_eq!(
                response,
                ProcessOutcome::Respond(Frame::SimpleString("OK".into()))
            );
        }

        #[tokio::test]
        async fn protected_command_on_unauthenticated_password_protected_connection_returns_noauth()
        {
            let mut connection = setup_pwd_protected_connection("my_password".into());

            let response = connection
                .process_command(Command::GET {
                    key: "mykey".into(),
                })
                .await;

            assert_noauth(response);
        }

        #[tokio::test]
        async fn protected_command_on_authenticated_password_protected_connection_executes() {
            let mut connection = setup_pwd_protected_connection("my_password".into());
            connection.authenticated = true;

            let response = connection
                .process_command(Command::SET {
                    key: "mykey".into(),
                    value: "myvalue".into(),
                })
                .await;

            assert_eq!(
                response,
                ProcessOutcome::Respond(Frame::SimpleString("OK".into()))
            );
        }

        #[tokio::test]
        async fn ping_is_allowed_without_authentication() {
            let mut connection = setup_pwd_protected_connection("my_password".into());

            let response = connection.process_command(Command::PING).await;

            assert_eq!(
                response,
                ProcessOutcome::Respond(Frame::SimpleString("PONG".into()))
            );
        }

        #[tokio::test]
        async fn noop_is_allowed_without_authentication() {
            let mut connection = setup_pwd_protected_connection("my_password".into());

            let response = connection.process_command(Command::NOOP).await;

            assert_eq!(response, ProcessOutcome::Noop);
        }

        #[tokio::test]
        async fn quit_is_allowed_without_authentication() {
            let mut connection = setup_pwd_protected_connection("my_password".into());

            let response = connection.process_command(Command::QUIT).await;

            assert_eq!(response, ProcessOutcome::Quit);
        }

        #[tokio::test]
        async fn valid_auth_allows_following_protected_command() {
            let mut connection = setup_pwd_protected_connection("my_password".into());

            let auth_response = connection
                .process_command(Command::AUTH {
                    key: "my_password".into(),
                })
                .await;
            assert_eq!(
                auth_response,
                ProcessOutcome::Respond(Frame::SimpleString("OK".into()))
            );

            let command_response = connection
                .process_command(Command::SET {
                    key: "mykey".into(),
                    value: "myvalue".into(),
                })
                .await;

            assert_eq!(
                command_response,
                ProcessOutcome::Respond(Frame::SimpleString("OK".into()))
            );
        }

        #[tokio::test]
        async fn invalid_auth_does_not_allow_following_protected_command() {
            let mut connection = setup_pwd_protected_connection("my_password".into());

            let auth_response = connection
                .process_command(Command::AUTH {
                    key: "wrong_password".into(),
                })
                .await;
            assert_eq!(
                auth_response,
                ProcessOutcome::Respond(Frame::SimpleError("AUTH invalid password".into()))
            );

            let command_response = connection
                .process_command(Command::GET {
                    key: "mykey".into(),
                })
                .await;

            assert_noauth(command_response);
        }
    }

    mod io {
        use super::*;

        struct TestCase<'a> {
            call: &'a [u8],
            response: &'a [u8],
            expected: &'a str,
        }

        async fn setup_io_connection(
            auth_password: SharedAuthPassword,
        ) -> (BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>, BufWriter<tokio::io::WriteHalf<tokio::io::DuplexStream>>, tokio::task::JoinHandle<Result<(), Error>>)
        {
            let (client, server) = tokio::io::duplex(128);
            let (reader, writer) = split(server);
            let store = Store::new();
            let mut conn = Connection::new(
                reader,
                writer,
                store,
                dummy_shutdown_token(),
                auth_password,
            );

            let (reader, writer) = split(client);
            let reader = BufReader::new(reader);
            let writer = BufWriter::new(writer);
            let handle = tokio::spawn(async move { conn.run().await });

            (reader, writer, handle)
        }

        #[tokio::test]
        async fn send_response() {
            let (client, server) = tokio::io::duplex(64);
            let mut client_reader = BufReader::new(client);
            let store = Store::new();
            let mut conn = Connection::new(
                tokio::io::empty(),
                server,
                store,
                dummy_shutdown_token(),
                dummy_auth_password(),
            );
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
                    call: b"*3\r\n$3\r\nSET\r\n$5\r\nexkey\r\n$5\r\nvalue\r\n",
                    response: b"+OK\r\n",
                    expected: "Should set exkey before expiring it",
                },
                TestCase {
                    call: b"*3\r\n$6\r\nEXPIRE\r\n$5\r\nexkey\r\n$2\r\n60\r\n",
                    response: b":1\r\n",
                    expected: "Should return 1 if EXPIRE called on an existing key",
                },
                TestCase {
                    call: b"*3\r\n$6\r\nEXPIRE\r\n$11\r\nmissing-key\r\n$2\r\n60\r\n",
                    response: b":0\r\n",
                    expected: "Should return 0 if EXPIRE called on a missing key",
                },
                TestCase {
                    call: b"*3\r\n$3\r\nSET\r\n$10\r\nexpiredkey\r\n$5\r\nvalue\r\n",
                    response: b"+OK\r\n",
                    expected: "Should set expiredkey before expiring it immediately",
                },
                TestCase {
                    call: b"*3\r\n$6\r\nEXPIRE\r\n$10\r\nexpiredkey\r\n$1\r\n0\r\n",
                    response: b":1\r\n",
                    expected: "Should return 1 when setting an immediate expiration",
                },
                TestCase {
                    call: b"*3\r\n$6\r\nEXPIRE\r\n$10\r\nexpiredkey\r\n$2\r\n60\r\n",
                    response: b":0\r\n",
                    expected: "Should return 0 if EXPIRE called on an expired key",
                },
                TestCase {
                    call: b"*2\r\n$3\r\nTTL\r\n$11\r\nmissing-key\r\n",
                    response: b":-2\r\n",
                    expected: "Missing keys should return -2 for TTL",
                },
                TestCase {
                    call: b"*3\r\n$3\r\nSET\r\n$8\r\nttl-live\r\n$5\r\nvalue\r\n",
                    response: b"+OK\r\n",
                    expected: "Should set ttl-live before reading TTL without expiration",
                },
                TestCase {
                    call: b"*2\r\n$3\r\nTTL\r\n$8\r\nttl-live\r\n",
                    response: b":-1\r\n",
                    expected: "Keys without expiration should return -1 for TTL",
                },
                TestCase {
                    call: b"*3\r\n$3\r\nSET\r\n$12\r\nttl-expiring\r\n$5\r\nvalue\r\n",
                    response: b"+OK\r\n",
                    expected: "Should set ttl-expiring before reading positive TTL",
                },
                TestCase {
                    call: b"*3\r\n$6\r\nEXPIRE\r\n$12\r\nttl-expiring\r\n$2\r\n60\r\n",
                    response: b":1\r\n",
                    expected: "Should add expiration before reading positive TTL",
                },
                TestCase {
                    call: b"*3\r\n$3\r\nSET\r\n$11\r\nttl-expired\r\n$5\r\nvalue\r\n",
                    response: b"+OK\r\n",
                    expected: "Should set ttl-expired before expiring it immediately",
                },
                TestCase {
                    call: b"*3\r\n$6\r\nEXPIRE\r\n$11\r\nttl-expired\r\n$1\r\n0\r\n",
                    response: b":1\r\n",
                    expected: "Should expire ttl-expired immediately",
                },
                TestCase {
                    call: b"*2\r\n$3\r\nTTL\r\n$11\r\nttl-expired\r\n",
                    response: b":-2\r\n",
                    expected: "Expired keys should return -2 for TTL",
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
            let mut conn = Connection::new(
                reader,
                writer,
                store,
                dummy_shutdown_token(),
                dummy_auth_password(),
            );

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

        #[tokio::test]
        async fn e2e_run_blocks_protected_commands_until_authenticated() {
            let (mut reader, mut writer, handle) = setup_io_connection(Arc::new(Some(
                SecretBox::new(Box::new(b"my_password".to_vec())),
            )))
            .await;

            writer
                .write_all(b"*2\r\n$3\r\nGET\r\n$5\r\nmykey\r\n")
                .await
                .unwrap();
            writer.flush().await.unwrap();

            let mut read_buffer = [0; 33];
            reader.read_exact(&mut read_buffer).await.unwrap();
            assert_eq!(&read_buffer, b"-NOAUTH Authentication required\r\n");

            writer.write_all(b"*1\r\n$4\r\nQUIT\r\n").await.unwrap();
            writer.flush().await.unwrap();
            handle.await.unwrap().unwrap();
        }

        #[tokio::test]
        async fn e2e_run_rejects_invalid_auth_and_keeps_protected_commands_blocked() {
            let (mut reader, mut writer, handle) = setup_io_connection(Arc::new(Some(
                SecretBox::new(Box::new(b"my_password".to_vec())),
            )))
            .await;

            writer
                .write_all(b"*2\r\n$4\r\nAUTH\r\n$14\r\nwrong_password\r\n")
                .await
                .unwrap();
            writer.flush().await.unwrap();

            let mut auth_buffer = [0; 24];
            reader.read_exact(&mut auth_buffer).await.unwrap();
            assert_eq!(&auth_buffer, b"-AUTH invalid password\r\n");

            writer
                .write_all(b"*3\r\n$3\r\nSET\r\n$5\r\nmykey\r\n$7\r\nmyvalue\r\n")
                .await
                .unwrap();
            writer.flush().await.unwrap();

            let mut noauth_buffer = [0; 33];
            reader.read_exact(&mut noauth_buffer).await.unwrap();
            assert_eq!(&noauth_buffer, b"-NOAUTH Authentication required\r\n");

            writer.write_all(b"*1\r\n$4\r\nQUIT\r\n").await.unwrap();
            writer.flush().await.unwrap();
            handle.await.unwrap().unwrap();
        }

        #[tokio::test]
        async fn e2e_run_accepts_valid_auth_and_then_allows_protected_commands() {
            let (mut reader, mut writer, handle) = setup_io_connection(Arc::new(Some(
                SecretBox::new(Box::new(b"my_password".to_vec())),
            )))
            .await;

            writer
                .write_all(b"*2\r\n$4\r\nAUTH\r\n$11\r\nmy_password\r\n")
                .await
                .unwrap();
            writer.flush().await.unwrap();

            let mut auth_buffer = [0; 5];
            reader.read_exact(&mut auth_buffer).await.unwrap();
            assert_eq!(&auth_buffer, b"+OK\r\n");

            writer
                .write_all(b"*3\r\n$3\r\nSET\r\n$5\r\nmykey\r\n$7\r\nmyvalue\r\n")
                .await
                .unwrap();
            writer.flush().await.unwrap();

            let mut set_buffer = [0; 5];
            reader.read_exact(&mut set_buffer).await.unwrap();
            assert_eq!(&set_buffer, b"+OK\r\n");

            writer
                .write_all(b"*2\r\n$3\r\nGET\r\n$5\r\nmykey\r\n")
                .await
                .unwrap();
            writer.flush().await.unwrap();

            let mut get_buffer = [0; 13];
            reader.read_exact(&mut get_buffer).await.unwrap();
            assert_eq!(&get_buffer, b"$7\r\nmyvalue\r\n");

            writer.write_all(b"*1\r\n$4\r\nQUIT\r\n").await.unwrap();
            writer.flush().await.unwrap();
            handle.await.unwrap().unwrap();
        }
    }
}
