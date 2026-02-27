use std::{fmt::Error, ops::RemAssign};

#[derive(Debug, PartialEq, Clone)]
enum ParseError {
    UnreadableUtf,
    InvalidLength,
    UnreadableBulkString,
}

#[derive(Debug, PartialEq)]
enum Frame {
    SimpleString(String),
    SimpleError(String),
    Bulk(Option<Vec<u8>>),
    Array(Option<Vec<Frame>>),
}

#[derive(Debug, PartialEq)]
enum State {
    Start,
    ReadingSimpleString,
    ReadingBulkLength,
    ReadingBulkString(usize),
    ReadingArrayLength,
    ReadingArray(usize, Vec<Frame>),
    Error(ParseError),
}

#[derive(Debug, PartialEq)]
struct Parser {
    state: State,
    buf: Vec<u8>,
}

impl Parser {
    pub fn new() -> Self {
        Parser {
            state: State::Start,
            buf: Vec::new(),
        }
    }

    fn set_error(&mut self, err: ParseError) -> ParseError {
        self.state = State::Error(err.clone());
        err
    }

    pub fn parse(&mut self, input: &[u8]) -> Result<Vec<Frame>, ParseError> {
        self.buf.extend_from_slice(input);
        let mut output = Vec::<Frame>::new();
        loop {
            match self.try_parse_one_frame() {
                Ok(Some(f)) => output.push(f),
                Ok(None) => return Ok(output),
                Err(e) => return Err(self.set_error(e)),
            }
        }
    }

    fn read_length(&mut self) -> Result<Option<i64>, ParseError> {
        let pos = match self.buf.windows(2).position(|w| w == b"\r\n") {
            Some(pos) => pos,
            None => return Ok(None),
        };
        let slice = &self.buf[..pos];
        let value = std::str::from_utf8(slice)
            .map_err(|_| ParseError::InvalidLength)?
            .parse::<i64>()
            .map_err(|_| ParseError::InvalidLength)?;
        self.buf.drain(..pos + 2);
        Ok(Some(value))
    }

    fn try_parse_one_frame(&mut self) -> Result<Option<Frame>, ParseError> {
        if let State::Error(ref e) = self.state {
            return Err(e.clone());
        }

        loop {
            match &mut self.state {
                State::Start => match self.buf.first() {
                    Some(b'+') => {
                        self.buf.drain(..1);
                        self.state = State::ReadingSimpleString;
                        continue;
                    }
                    Some(b'$') => {
                        self.buf.drain(..1);
                        self.state = State::ReadingBulkLength;
                        continue;
                    }
                    Some(b'*') => {
                        self.buf.drain(..1);
                        self.state = State::ReadingArrayLength;
                        continue;
                    }
                    Some(_) => return Ok(None),
                    None => return Ok(None),
                },

                State::ReadingSimpleString => {
                    let pos = match self.buf.windows(2).position(|w| w == b"\r\n") {
                        Some(pos) => pos,
                        None => return Ok(None),
                    };
                    let bytes: Vec<u8> = self.buf.drain(..pos).collect();
                    let payload =
                        String::from_utf8(bytes).map_err(|_| ParseError::UnreadableUtf)?;
                    self.buf.drain(..2);
                    self.state = State::Start;
                    return Ok(Some(Frame::SimpleString(payload)));
                }

                State::ReadingBulkLength => {
                    let length = match self.read_length()? {
                        Some(l) => l,
                        None => return Ok(None),
                    };
                    if length == -1 {
                        self.state = State::Start;
                        return Ok(Some(Frame::Bulk(None)));
                    }
                    if length < -1 {
                        return Err(ParseError::InvalidLength);
                    }
                    let length = usize::try_from(length).map_err(|_| ParseError::InvalidLength)?;
                    self.state = State::ReadingBulkString(length);
                    continue;
                }

                State::ReadingBulkString(length) => {
                    let len = *length;
                    if len + 2 > self.buf.len() {
                        return Ok(None);
                    }
                    if self.buf[len] != b'\r' || self.buf[len + 1] != b'\n' {
                        return Err(ParseError::UnreadableBulkString);
                    }
                    let payload = self.buf.drain(..len).collect();
                    self.buf.drain(..2);
                    self.state = State::Start;
                    return Ok(Some(Frame::Bulk(Some(payload))));
                }

                State::ReadingArrayLength => {
                    let length = match self.read_length()? {
                        Some(l) => l,
                        None => return Ok(None),
                    };
                    if length == -1 {
                        self.state = State::Start;
                        return Ok(Some(Frame::Array(None)));
                    }
                    if length < -1 {
                        return Err(ParseError::InvalidLength);
                    }
                    let length = usize::try_from(length).map_err(|_| ParseError::InvalidLength)?;
                    self.state = State::ReadingBulkString(length);

                    continue;
                }

                State::ReadingArray(remaining, array_builder) => {
                    let mut remaining = *remaining;
                    let mut payload = std::mem::take(array_builder);
                    if remaining == 0 {
                        self.state = State::Start;
                        return Ok(Some(Frame::Array(Some(payload))));
                    } else {
                        let frame = self.try_parse_one_frame()?;
                        match frame {
                            None => return Ok(None),
                            Some(f) => {
                                payload.push(f);
                                remaining -= 1;
                                self.state = State::ReadingArray(remaining, payload);
                            }
                        }
                    }
                }

                State::Error(e) => {
                    return Err(e.clone());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    mod simple_string_tests {
        use super::*;
        #[test]
        fn incomplete_buffer_gets_ok_none() {
            let mut p = Parser::new();
            let buf = b"+";
            assert_eq!(p.parse(buf), Ok(Vec::new()));
            let mut p = Parser::new();
            let buf = b"+OK";
            assert_eq!(p.parse(buf), Ok(Vec::new()));
        }

        #[test]
        fn parse_empty_simple_string() {
            let mut p = Parser::new();
            let buf = b"+\r\n";
            assert_eq!(p.parse(buf), Ok(vec![Frame::SimpleString("".to_string())]))
        }

        #[test]
        fn parse_simple_string() {
            let mut p = Parser::new();
            let buf = b"+OK\r\n";
            assert_eq!(
                p.parse(buf),
                Ok(vec![Frame::SimpleString("OK".to_string())])
            )
        }
    }

    mod bulk_string_tests {
        use super::*;
        #[test]
        fn bulk_string_marker_only_returns_none() {
            let mut p = Parser::new();
            let buf = &b"$"[..];
            assert_eq!(p.parse(buf), Ok(Vec::new()));
        }

        #[test]
        fn incomplete_length_returns_none() {
            let mut p = Parser::new();
            let buf = &b"$5"[..];
            assert_eq!(p.parse(buf), Ok(Vec::new()));
        }

        #[test]
        fn complete_length_but_no_payload_returns_none() {
            let mut p = Parser::new();
            let buf = b"$5\r\n";
            assert_eq!(p.parse(buf), Ok(Vec::new()));
        }

        #[test]
        fn complete_length_but_incomplete_payload_returns_none() {
            let mut p = Parser::new();
            let buf = b"$5\r\nh";
            assert_eq!(p.parse(buf), Ok(Vec::new()));
        }

        #[test]
        fn out_of_bounds_length_returns_error() {
            let mut p = Parser::new();
            let buf = b"$-2\r\n";
            assert_eq!(p.parse(buf), Err(ParseError::InvalidLength));
            assert_eq!(p.state, State::Error(ParseError::InvalidLength))
        }

        #[test]
        fn minus_one_returns_nil_bulk_string() {
            let mut p = Parser::new();
            let buf = b"$-1\r\n";
            assert_eq!(p.parse(buf), Ok(vec![Frame::Bulk(None)]))
        }

        #[test]
        fn zero_length_bulk_string() {
            let mut p = Parser::new();
            let buf = b"$0\r\n\r\n";
            assert_eq!(p.parse(buf), Ok(vec![Frame::Bulk(Some(vec![]))]));
        }

        #[test]
        fn payload_continues_past_expected_length_gets_error() {
            let mut p = Parser::new();
            let buf = b"$5\r\nhellothere\r\n";
            assert_eq!(p.parse(buf), Err(ParseError::UnreadableBulkString));
            assert_eq!(p.state, State::Error(ParseError::UnreadableBulkString));
        }

        #[test]
        fn payload_is_not_terminated_at_expected_length_gets_error() {
            let mut p = Parser::new();
            let buf = b"$5\r\nhellothere\r\n";
            assert_eq!(p.parse(buf), Err(ParseError::UnreadableBulkString));
            assert_eq!(p.state, State::Error(ParseError::UnreadableBulkString));
        }

        #[test]
        fn proper_payload_parsed_leaving_remaining_buffer() {
            let mut p = Parser::new();
            let buf = b"$5\r\nhello\r\nleftovers";
            assert_eq!(p.parse(buf), Ok(vec![Frame::Bulk(Some(b"hello".to_vec()))]));
            assert_eq!(p.buf, b"leftovers")
        }

        #[test]
        fn handles_bulk_string_split_at_any_position() {
            let mut p = Parser::new();
            let full_buf = b"$5\r\nhello\r\n$7\r\nanother\r\n$4\r\nbulk\r\n";
            for (i, _) in full_buf.iter().enumerate() {
                let mut builder: Vec<Frame> = Vec::new();
                let (left, right) = full_buf.split_at(i);
                let mut result = p.parse(left).unwrap();
                builder.append(&mut result);
                let mut result = p.parse(right).unwrap();
                builder.append(&mut result);
                assert_eq!(
                    builder,
                    vec![
                        Frame::Bulk(Some(b"hello".to_vec())),
                        Frame::Bulk(Some(b"another".to_vec())),
                        Frame::Bulk(Some(b"bulk".to_vec())),
                    ]
                )
            }
        }
    }
}
