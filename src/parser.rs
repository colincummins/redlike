use std::arch::x86_64::_SIDD_NEGATIVE_POLARITY;

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
    ReadingLine,
    ReadingBulkLength,
    ReadingBulkString(usize),
    ReadingArrayLength,
    ReadingArray(usize, Vec<Frame>),
}

#[derive(Debug, PartialEq)]
enum ParseError {
    UnreadableUtf,
    InvalidBulkLength,
    InvalidArrayLength,
    UnreadableBulkString,
}

#[derive(Debug, PartialEq)]
struct Parser {
    state: State,
    buf: Vec<u8>,
}

impl Parser {
    fn new() -> Self {
        Parser {
            state: State::Start,
            buf: Vec::new(),
        }
    }

    fn parse<'a>(&mut self, input: &[u8]) -> Result<Option<Frame>, ParseError> {
        self.buf.extend_from_slice(input);
        loop {
            match self.state {
                State::Start => match self.buf.drain(..1).next() {
                    Some(b'+') => {
                        self.state = State::ReadingSimpleString;
                        continue;
                    }
                    Some(b'$') => {
                        self.state = State::ReadingBulkLength;
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
                    self.state = State::Start;
                    self.buf.drain(..2);
                    return Ok(Some(Frame::SimpleString(payload.to_owned())));
                }

                State::ReadingBulkLength => {
                    let pos = match self.buf.windows(2).position(|w| w == b"\r\n") {
                        Some(pos) => pos,
                        None => return Ok(None),
                    };
                    let bulk_length: i64 = str::from_utf8(&self.buf[..pos])
                        .map_err(|_| ParseError::InvalidBulkLength)?
                        .parse()
                        .map_err(|_| ParseError::InvalidBulkLength)?;
                    if bulk_length == -1 {
                        self.buf.drain(..pos + 2);
                        return Ok(Some(Frame::Bulk(None)));
                    }
                    if bulk_length < -1 {
                        return Err(ParseError::InvalidBulkLength);
                    }
                    let bulk_length =
                        usize::try_from(bulk_length).map_err(|_| ParseError::InvalidBulkLength)?;

                    self.buf.drain(..pos + 2);
                    self.state = State::ReadingBulkString(bulk_length);
                    continue;
                }

                State::ReadingBulkString(bulk_length) => {
                    if bulk_length + 2 > self.buf.len() {
                        return Ok(None);
                    }
                    if self.buf[bulk_length] != b'\r' || self.buf[bulk_length + 1] != b'\n' {
                        return Err(ParseError::UnreadableBulkString);
                    }

                    let payload = self.buf.drain(..bulk_length).collect();
                    self.buf.drain(..2);
                    return Ok(Some(Frame::Bulk(Some(payload))));
                }

                _ => return Ok(None),
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
            assert_eq!(p.parse(buf), Ok(None));
            let mut p = Parser::new();
            let buf = b"+OK";
            assert_eq!(p.parse(buf), Ok(None));
        }

        #[test]
        fn parse_empty_simple_string() {
            let mut p = Parser::new();
            let buf = b"+\r\n";
            assert_eq!(p.parse(buf), Ok(Some(Frame::SimpleString("".to_string()))))
        }

        #[test]
        fn parse_simple_string() {
            let mut p = Parser::new();
            let buf = b"+OK\r\n";
            assert_eq!(
                p.parse(buf),
                Ok(Some(Frame::SimpleString("OK".to_string())))
            )
        }

        #[test]
        fn parser_only_grabs_one_frame() {
            let mut p = Parser::new();
            let buf = b"+OK\r\n+OK\r\n";
            assert_eq!(
                p.parse(buf),
                Ok(Some(Frame::SimpleString("OK".to_string())))
            )
        }
    }

    mod bulk_string_tests {
        use super::*;
        #[test]
        fn bulk_string_marker_only_returns_none() {
            let mut p = Parser::new();
            let buf = &b"$"[..];
            assert_eq!(p.parse(buf), Ok(None));
        }

        #[test]
        fn incomplete_length_returns_none() {
            let mut p = Parser::new();
            let buf = &b"$5"[..];
            assert_eq!(p.parse(buf), Ok(None));
        }

        #[test]
        fn complete_length_but_no_payload_returns_none() {
            let mut p = Parser::new();
            let buf = b"$5\r\n";
            assert_eq!(p.parse(buf), Ok(None));
        }

        #[test]
        fn complete_length_but_incomplete_payload_returns_none() {
            let mut p = Parser::new();
            let buf = b"$5\r\nh";
            assert_eq!(p.parse(buf), Ok(None));
        }

        #[test]
        fn out_of_bounds_length_returns_error() {
            let mut p = Parser::new();
            let buf = b"$-2\r\n";
            assert_eq!(p.parse(buf), Err(ParseError::InvalidBulkLength));
        }

        #[test]
        fn minus_one_returns_nil_bulk_string() {
            let mut p = Parser::new();
            let buf = b"$-1\r\n";
            assert_eq!(p.parse(buf), Ok(Some(Frame::Bulk(None))))
        }

        #[test]
        fn zero_length_bulk_string() {
            let mut p = Parser::new();
            let buf = b"$0\r\n\r\n";
            assert_eq!(p.parse(buf), Ok(Some(Frame::Bulk(Some(vec![])))));
        }

        #[test]
        fn payload_continues_past_expected_length_gets_error() {
            let mut p = Parser::new();
            let buf = b"$5\r\nhellothere\r\n";
            assert_eq!(p.parse(buf), Err(ParseError::UnreadableBulkString));
        }

        #[test]
        fn payload_is_not_terminated_at_expected_length_gets_error() {
            let mut p = Parser::new();
            let buf = b"$5\r\nhellothere\r\n";
            assert_eq!(p.parse(buf), Err(ParseError::UnreadableBulkString));
        }

        #[test]
        fn proper_payload_parsed_leaving_remaining_buffer() {
            let mut p = Parser::new();
            let buf = b"$5\r\nhello\r\nleftovers";
            assert_eq!(p.parse(buf), Ok(Some(Frame::Bulk(Some(b"hello".to_vec())))));
        }
    }
}
