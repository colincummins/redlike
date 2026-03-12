use crate::error::Error;
use crate::frame::Frame;

#[derive(PartialEq, Eq, Debug)]
pub enum Command {
    PING,
    GET { key: Vec<u8> },
    SET { key: Vec<u8>, value: Vec<u8> },
    DEL { key: Vec<u8> },
    EXPIRE { key: Vec<u8>, value: u64 },
    QUIT,
    NOOP,
}

impl TryFrom<&Frame> for Command {
    type Error = Error;

    fn try_from(value: &Frame) -> Result<Self, Self::Error> {
        let args = match value {
            Frame::Array(Some(inner)) if !inner.is_empty() => inner,
            _ => return Err(Error::InvalidCommandFrame),
        };

        let args: Vec<&[u8]> = args
            .iter()
            .map(|a| match a {
                Frame::Bulk(Some(i)) => Ok(i.as_slice()),
                _ => Err(Error::InvalidCommandFrame),
            })
            .collect::<Result<_, _>>()?;

        match args.as_slice() {
            [cmd] if cmd.eq_ignore_ascii_case(b"ping") => Ok(Command::PING),
            [cmd, ..] if cmd.eq_ignore_ascii_case(b"ping") => Err(Error::WrongArity {
                command: "PING".to_string(),
                given: args.len() - 1,
                expected: 0,
            }),
            [cmd] if cmd.eq_ignore_ascii_case(b"quit") => Ok(Command::QUIT),
            [cmd, ..] if cmd.eq_ignore_ascii_case(b"quit") => Err(Error::WrongArity {
                command: "QUIT".to_string(),
                given: args.len() - 1,
                expected: 0,
            }),
            [cmd, key] if cmd.eq_ignore_ascii_case(b"get") => {
                Ok(Command::GET { key: key.to_vec() })
            }
            [cmd, ..] if cmd.eq_ignore_ascii_case(b"get") => Err(Error::WrongArity {
                command: "GET".to_string(),
                given: args.len() - 1,
                expected: 1,
            }),
            [cmd, key, value] if cmd.eq_ignore_ascii_case(b"set") => Ok(Command::SET {
                key: key.to_vec(),
                value: value.to_vec(),
            }),
            [cmd, ..] if cmd.eq_ignore_ascii_case(b"set") => Err(Error::WrongArity {
                command: "SET".to_string(),
                given: args.len() - 1,
                expected: 2,
            }),
            [cmd, key] if cmd.eq_ignore_ascii_case(b"del") => {
                Ok(Command::DEL { key: key.to_vec() })
            }
            [cmd, ..] if cmd.eq_ignore_ascii_case(b"del") => Err(Error::WrongArity {
                command: "DEL".to_string(),
                given: args.len() - 1,
                expected: 1,
            }),
            [cmd, key, value] if cmd.eq_ignore_ascii_case(b"expire") => {
                match String::from_utf8(value.to_vec())
                    .map_err(|_| Error::WrongArgumentType)
                    .and_then(|v| v.parse::<u64>().map_err(|_| Error::WrongArgumentType))
                {
                    Ok(v) => Ok(Command::EXPIRE {
                        key: key.to_vec(),
                        value: v,
                    }),
                    Err(e) => Err(e),
                }
            }
            [cmd, ..] if cmd.eq_ignore_ascii_case(b"expire") => Err(Error::WrongArity {
                command: "EXPIRE".to_string(),
                given: args.len() - 1,
                expected: 2,
            }),

            [..] => Err(Error::UnknownCommand),
        }
    }
}

impl TryFrom<Frame> for Command {
    type Error = Error;

    fn try_from(value: Frame) -> Result<Self, Self::Error> {
        Command::try_from(&value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bulk(value: &[u8]) -> Frame {
        Frame::Bulk(Some(value.to_vec()))
    }

    #[test]
    fn get_command_parses() {
        let frame = Frame::Array(Some(vec![bulk(b"GET"), bulk(b"mykey")]));

        let command = Command::try_from(frame).unwrap();
        assert_eq!(
            command,
            Command::GET {
                key: b"mykey".to_vec()
            }
        );
    }

    #[test]
    fn ping_command_parses() {
        let frame = Frame::Array(Some(vec![bulk(b"PING")]));

        let command = Command::try_from(frame).unwrap();
        assert_eq!(command, Command::PING);
    }

    #[test]
    fn quit_command_parses() {
        let frame = Frame::Array(Some(vec![bulk(b"QUIT")]));

        let command = Command::try_from(frame).unwrap();
        assert_eq!(command, Command::QUIT);
    }

    #[test]
    fn set_command_parses() {
        let frame = Frame::Array(Some(vec![bulk(b"SET"), bulk(b"mykey"), bulk(b"myvalue")]));

        let command = Command::try_from(frame).unwrap();
        assert_eq!(
            command,
            Command::SET {
                key: b"mykey".to_vec(),
                value: b"myvalue".to_vec(),
            }
        );
    }

    #[test]
    fn del_command_parses() {
        let frame = Frame::Array(Some(vec![bulk(b"DEL"), bulk(b"mykey")]));

        let command = Command::try_from(frame).unwrap();
        assert_eq!(
            command,
            Command::DEL {
                key: b"mykey".to_vec()
            }
        );
    }

    #[test]
    fn expire_command_parses() {
        let frame = Frame::Array(Some(vec![bulk(b"EXPIRE"), bulk(b"mykey"), bulk(b"123")]));

        let command = Command::try_from(frame).unwrap();
        assert_eq!(
            command,
            Command::EXPIRE {
                key: b"mykey".to_vec(),
                value: 123,
            }
        );
    }

    #[test]
    fn command_name_is_case_insensitive() {
        let ping = Frame::Array(Some(vec![bulk(b"pInG")]));
        let quit = Frame::Array(Some(vec![bulk(b"qUiT")]));
        let get = Frame::Array(Some(vec![bulk(b"gEt"), bulk(b"mykey")]));
        let set = Frame::Array(Some(vec![bulk(b"SeT"), bulk(b"mykey"), bulk(b"myvalue")]));
        let del = Frame::Array(Some(vec![bulk(b"dEl"), bulk(b"mykey")]));
        let expire = Frame::Array(Some(vec![bulk(b"eXpIrE"), bulk(b"mykey"), bulk(b"60")]));

        assert_eq!(Command::try_from(ping).unwrap(), Command::PING);
        assert_eq!(Command::try_from(quit).unwrap(), Command::QUIT);
        assert_eq!(
            Command::try_from(get).unwrap(),
            Command::GET {
                key: b"mykey".to_vec()
            }
        );
        assert_eq!(
            Command::try_from(set).unwrap(),
            Command::SET {
                key: b"mykey".to_vec(),
                value: b"myvalue".to_vec(),
            }
        );
        assert_eq!(
            Command::try_from(del).unwrap(),
            Command::DEL {
                key: b"mykey".to_vec()
            }
        );
        assert_eq!(
            Command::try_from(expire).unwrap(),
            Command::EXPIRE {
                key: b"mykey".to_vec(),
                value: 60,
            }
        );
    }

    #[test]
    fn binary_key_and_value_are_preserved() {
        let frame = Frame::Array(Some(vec![
            bulk(b"SET"),
            bulk(b"\0key\xff"),
            bulk(b"va\0lue\xfe"),
        ]));

        let command = Command::try_from(frame).unwrap();
        assert_eq!(
            command,
            Command::SET {
                key: b"\0key\xff".to_vec(),
                value: b"va\0lue\xfe".to_vec(),
            }
        );
    }

    #[test]
    fn non_array_frame_is_invalid() {
        assert!(matches!(
            Command::try_from(Frame::Bulk(Some(b"GET".to_vec()))),
            Err(Error::InvalidCommandFrame)
        ));
    }

    #[test]
    fn nil_array_is_invalid() {
        assert!(matches!(
            Command::try_from(Frame::Array(None)),
            Err(Error::InvalidCommandFrame)
        ));
    }

    #[test]
    fn empty_array_is_invalid() {
        assert!(matches!(
            Command::try_from(Frame::Array(Some(vec![]))),
            Err(Error::InvalidCommandFrame)
        ));
    }

    #[test]
    fn non_bulk_element_is_invalid() {
        let frame = Frame::Array(Some(vec![bulk(b"GET"), Frame::Integer(1)]));

        assert!(matches!(
            Command::try_from(frame),
            Err(Error::InvalidCommandFrame)
        ));
    }

    #[test]
    fn nil_bulk_element_is_invalid() {
        let frame = Frame::Array(Some(vec![bulk(b"GET"), Frame::Bulk(None)]));

        assert!(matches!(
            Command::try_from(frame),
            Err(Error::InvalidCommandFrame)
        ));
    }

    #[test]
    fn unknown_command_returns_unknown_command() {
        let frame = Frame::Array(Some(vec![bulk(b"FOO"), bulk(b"bar")]));

        assert!(matches!(
            Command::try_from(frame),
            Err(Error::UnknownCommand)
        ));
    }

    #[test]
    fn get_with_missing_key_returns_wrong_arity() {
        let frame = Frame::Array(Some(vec![bulk(b"GET")]));

        assert!(matches!(
            Command::try_from(frame),
            Err(Error::WrongArity {
                command,
                given: 0,
                expected: 1,
            }) if command == "GET"
        ));
    }

    #[test]
    fn ping_with_extra_args_returns_wrong_arity() {
        let frame = Frame::Array(Some(vec![bulk(b"PING"), bulk(b"extra")]));

        assert!(matches!(
            Command::try_from(frame),
            Err(Error::WrongArity {
                command,
                given: 1,
                expected: 0,
            }) if command == "PING"
        ));
    }

    #[test]
    fn quit_with_extra_args_returns_wrong_arity() {
        let frame = Frame::Array(Some(vec![bulk(b"QUIT"), bulk(b"extra")]));

        assert!(matches!(
            Command::try_from(frame),
            Err(Error::WrongArity {
                command,
                given: 1,
                expected: 0,
            }) if command == "QUIT"
        ));
    }

    #[test]
    fn get_with_extra_args_returns_wrong_arity() {
        let frame = Frame::Array(Some(vec![bulk(b"GET"), bulk(b"key"), bulk(b"extra")]));

        assert!(matches!(
            Command::try_from(frame),
            Err(Error::WrongArity {
                command,
                given: 2,
                expected: 1,
            }) if command == "GET"
        ));
    }

    #[test]
    fn set_with_missing_value_returns_wrong_arity() {
        let frame = Frame::Array(Some(vec![bulk(b"SET"), bulk(b"key")]));

        assert!(matches!(
            Command::try_from(frame),
            Err(Error::WrongArity {
                command,
                given: 1,
                expected: 2,
            }) if command == "SET"
        ));
    }

    #[test]
    fn set_with_extra_args_returns_wrong_arity() {
        let frame = Frame::Array(Some(vec![
            bulk(b"SET"),
            bulk(b"key"),
            bulk(b"value"),
            bulk(b"extra"),
        ]));

        assert!(matches!(
            Command::try_from(frame),
            Err(Error::WrongArity {
                command,
                given: 3,
                expected: 2,
            }) if command == "SET"
        ));
    }

    #[test]
    fn del_with_missing_key_returns_wrong_arity() {
        let frame = Frame::Array(Some(vec![bulk(b"DEL")]));

        assert!(matches!(
            Command::try_from(frame),
            Err(Error::WrongArity {
                command,
                given: 0,
                expected: 1,
            }) if command == "DEL"
        ));
    }

    #[test]
    fn del_with_extra_args_returns_wrong_arity() {
        let frame = Frame::Array(Some(vec![bulk(b"DEL"), bulk(b"key"), bulk(b"extra")]));

        assert!(matches!(
            Command::try_from(frame),
            Err(Error::WrongArity {
                command,
                given: 2,
                expected: 1,
            }) if command == "DEL"
        ));
    }

    #[test]
    fn expire_with_missing_ttl_returns_wrong_arity() {
        let frame = Frame::Array(Some(vec![bulk(b"EXPIRE"), bulk(b"key")]));

        assert!(matches!(
            Command::try_from(frame),
            Err(Error::WrongArity {
                command,
                given: 1,
                expected: 2,
            }) if command == "EXPIRE"
        ));
    }

    #[test]
    fn expire_with_extra_args_returns_wrong_arity() {
        let frame = Frame::Array(Some(vec![
            bulk(b"EXPIRE"),
            bulk(b"key"),
            bulk(b"60"),
            bulk(b"extra"),
        ]));

        assert!(matches!(
            Command::try_from(frame),
            Err(Error::WrongArity {
                command,
                given: 3,
                expected: 2,
            }) if command == "EXPIRE"
        ));
    }

    #[test]
    fn expire_with_non_numeric_ttl_returns_wrong_argument_type() {
        let frame = Frame::Array(Some(vec![
            bulk(b"EXPIRE"),
            bulk(b"key"),
            bulk(b"not-a-number"),
        ]));

        assert!(matches!(
            Command::try_from(frame),
            Err(Error::WrongArgumentType)
        ));
    }

    #[test]
    fn expire_with_non_utf8_ttl_returns_wrong_argument_type() {
        let frame = Frame::Array(Some(vec![
            bulk(b"EXPIRE"),
            bulk(b"key"),
            Frame::Bulk(Some(vec![0xff, 0xfe])),
        ]));

        assert!(matches!(
            Command::try_from(frame),
            Err(Error::WrongArgumentType)
        ));
    }
}
