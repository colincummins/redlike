use crate::error::Error;
use crate::frame::Frame;

#[derive(PartialEq, Eq, Debug)]
pub enum Command {
    PING,
    GET { key: Vec<u8> },
    SET { key: Vec<u8>, value: Vec<u8> },
    DEL { key: Vec<u8> },
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
        assert_eq!(command, Command::GET { key: b"mykey".to_vec() });
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
        assert_eq!(command, Command::DEL { key: b"mykey".to_vec() });
    }

    #[test]
    fn command_name_is_case_insensitive() {
        let get = Frame::Array(Some(vec![bulk(b"gEt"), bulk(b"mykey")]));
        let set = Frame::Array(Some(vec![bulk(b"SeT"), bulk(b"mykey"), bulk(b"myvalue")]));
        let del = Frame::Array(Some(vec![bulk(b"dEl"), bulk(b"mykey")]));

        assert_eq!(Command::try_from(get).unwrap(), Command::GET { key: b"mykey".to_vec() });
        assert_eq!(
            Command::try_from(set).unwrap(),
            Command::SET {
                key: b"mykey".to_vec(),
                value: b"myvalue".to_vec(),
            }
        );
        assert_eq!(Command::try_from(del).unwrap(), Command::DEL { key: b"mykey".to_vec() });
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

        assert!(matches!(Command::try_from(frame), Err(Error::InvalidCommandFrame)));
    }

    #[test]
    fn nil_bulk_element_is_invalid() {
        let frame = Frame::Array(Some(vec![bulk(b"GET"), Frame::Bulk(None)]));

        assert!(matches!(Command::try_from(frame), Err(Error::InvalidCommandFrame)));
    }

    #[test]
    fn unknown_command_returns_unknown_command() {
        let frame = Frame::Array(Some(vec![bulk(b"FOO"), bulk(b"bar")]));

        assert!(matches!(Command::try_from(frame), Err(Error::UnknownCommand)));
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
}
