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
    ReadingBulkLen,
    ReadingBulkString(usize),
    ReadingArraySize,
    ReadingArray(usize, Vec<Frame>),
}

#[derive(Debug, PartialEq)]
enum ParseError {
    ParseUtfError,
    ParseBulkLengthError,
    ParseBulkStringError,
}

#[derive(Debug, PartialEq)]
struct Parser {
    state: State,
}

impl Parser {
    fn new() -> Self {
        Parser {
            state: State::Start,
        }
    }

    fn parse<'a>(&mut self, buf: &'a [u8]) -> Result<Option<(Frame, &'a [u8])>, ParseError> {
        let mut input = buf;
        loop {
            match self.state {
                State::Start => match input.first() {
                    Some(b'+') => {
                        self.state = State::ReadingSimpleString;
                        continue;
                    }
                    Some(b'$') => {
                        self.state = State::ReadingBulkLen;
                        continue;
                    }
                    Some(_) => return Ok(None),
                    None => return Ok(None),
                },

                State::ReadingSimpleString => {
                    let pos = match input.windows(2).position(|w| w == b"\r\n") {
                        Some(pos) => pos,
                        None => return Ok(None),
                    };
                    let payload = std::str::from_utf8(&input[1..pos])
                        .map_err(|_| ParseError::ParseUtfError)?;
                    self.state = State::Start;
                    return Ok(Some((
                        Frame::SimpleString(payload.to_owned()),
                        &input[pos + 2..],
                    )));
                }

                State::ReadingBulkLen => {
                    let pos = match input.windows(2).position(|w| w == b"\r\n") {
                        Some(pos) => pos,
                        None => return Ok(None),
                    };
                    let bulk_length: i64 = str::from_utf8(&input[1..pos])
                        .map_err(|_| ParseError::ParseBulkLengthError)?
                        .parse()
                        .map_err(|_| ParseError::ParseBulkLengthError)?;
                    if bulk_length == -1 {
                        return Ok(Some((Frame::Bulk(None), &input[pos + 2..])));
                    }
                    if bulk_length < -1 {
                        return Err(ParseError::ParseBulkLengthError);
                    }
                    let bulk_length = usize::try_from(bulk_length)
                        .map_err(|_| ParseError::ParseBulkLengthError)?;

                    input = &input[pos + 2..];
                    self.state = State::ReadingBulkString(bulk_length);
                    continue;
                }

                State::ReadingBulkString(bulk_length) => {
                    if bulk_length + 2 > input.len() {
                        return Ok(None);
                    }
                    if input[bulk_length] != b'\r' || input[bulk_length + 1] != b'\n' {
                        return Err(ParseError::ParseBulkStringError);
                    }

                    return Ok(Some((
                        Frame::Bulk(Some(input[..bulk_length].to_vec())),
                        &input[bulk_length + 2..],
                    )));
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
            assert_eq!(
                p.parse(buf),
                Ok(Some((Frame::SimpleString("".to_string()), &b""[..])))
            )
        }

        #[test]
        fn parse_simple_string() {
            let mut p = Parser::new();
            let buf = b"+OK\r\n";
            assert_eq!(
                p.parse(buf),
                Ok(Some((Frame::SimpleString("OK".to_string()), &b""[..])))
            )
        }

        #[test]
        fn parser_only_grabs_one_frame() {
            let mut p = Parser::new();
            let buf = b"+OK\r\n+OK\r\n";
            assert_eq!(
                p.parse(buf),
                Ok(Some((
                    Frame::SimpleString("OK".to_string()),
                    &b"+OK\r\n"[..]
                )))
            )
        }
    }

    mod bulk_string_tests {
        use super::*;
        #[test]
        fn bulk_string_marker_only_returns_none() {
            let mut p = Parser::new();
            let buf = b"$";
            assert_eq!(p.parse(buf), Ok(None));
        }

        #[test]
        fn incomplete_length_returns_none() {
            let mut p = Parser::new();
            let buf = b"$5";
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
            assert_eq!(p.parse(buf), Err(ParseError::ParseBulkLengthError));
        }

        #[test]
        fn minus_one_returns_nil_bulk_string() {
            let mut p = Parser::new();
            let buf = b"$-1\r\n";
            assert_eq!(p.parse(buf), Ok(Some((Frame::Bulk(None), &b""[..]))));
        }

        #[test]
        fn zero_length_bulk_string() {
            let mut p = Parser::new();
            let buf = b"$0\r\n\r\n";
            assert_eq!(
                p.parse(buf),
                Ok(Some((Frame::Bulk(Some(vec![])), &b""[..])))
            );
        }

        #[test]
        fn payload_continues_past_expected_length_gets_error() {
            let mut p = Parser::new();
            let buf = b"$5\r\nhellothere\r\n";
            assert_eq!(p.parse(buf), Err(ParseError::ParseBulkStringError));
        }

        #[test]
        fn payload_is_not_terminated_at_expected_length_gets_error() {
            let mut p = Parser::new();
            let buf = b"$5\r\nhellothere\r\n";
            assert_eq!(p.parse(buf), Err(ParseError::ParseBulkStringError));
        }

        #[test]
        fn proper_payload_parsed_leaving_remaining_buffer() {
            let mut p = Parser::new();
            let buf = b"$5\r\nhello\r\nleftovers";
            assert_eq!(
                p.parse(buf),
                Ok(Some((
                    Frame::Bulk(Some(b"hello".to_vec())),
                    &b"leftovers"[..]
                )))
            );
        }
    }
}
