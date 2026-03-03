use crate::frame::Frame;

#[derive(Debug, PartialEq, Clone)]
enum ParseError {
    UnreadableUtf,
    InvalidLength,
    UnreadableBulkString,
}

#[derive(Debug, PartialEq)]
enum State {
    Start,
    ReadingSimpleString,
    ReadingSimpleError,
    ReadingBulkLength,
    ReadingBulkString(usize),
    ReadingInteger,
    ReadingInline,
    ReadingArrayLength,
    Error(ParseError),
}

#[derive(Debug, PartialEq)]
struct Parser {
    state: State,
    buf: Vec<u8>,
    stack: Vec<Vec<Frame>>,
}

impl Parser {
    pub fn new() -> Self {
        Parser {
            state: State::Start,
            buf: Vec::new(),
            stack: Vec::new(),
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
                Ok(Some(f)) => {
                    if !self.stack.is_empty() {
                        self.stack.last_mut().unwrap().push(f);
                        while self.stack.last().unwrap().capacity()
                            - self.stack.last().unwrap().len()
                            == 0
                        {
                            let new_array_frame: Frame =
                                Frame::Array(Some(self.stack.pop().unwrap()));
                            if self.stack.is_empty() {
                                output.push(new_array_frame);
                                break;
                            } else {
                                self.stack.last_mut().unwrap().push(new_array_frame);
                            }
                        }
                    } else {
                        output.push(f);
                    }

                    continue;
                }
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
                    Some(b'-') => {
                        self.buf.drain(..1);
                        self.state = State::ReadingSimpleError;
                        continue;
                    }
                    Some(b':') => {
                        self.buf.drain(..1);
                        self.state = State::ReadingInteger;
                        continue;
                    }
                    Some(_) => {
                        self.state = State::ReadingInline;
                        continue;
                    }
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

                State::ReadingInline => {
                    let mut array_inner = Vec::<Frame>::new();
                    let pos = match self.buf.windows(2).position(|w| w == b"\r\n") {
                        Some(pos) => pos,
                        None => return Ok(None),
                    };
                    for p in String::from_utf8(self.buf.drain(..pos).collect())
                        .map_err(|_| ParseError::UnreadableUtf)?
                        .split(" ")
                    {
                        array_inner.push(Frame::Bulk(Some(p.as_bytes().to_vec())));
                    }
                    self.buf.drain(..2);
                    self.state = State::Start;
                    return Ok(Some(Frame::Array(Some(array_inner))));
                }

                State::ReadingSimpleError => {
                    let pos = match self.buf.windows(2).position(|w| w == b"\r\n") {
                        Some(pos) => pos,
                        None => return Ok(None),
                    };
                    let bytes: Vec<u8> = self.buf.drain(..pos).collect();
                    let payload =
                        String::from_utf8(bytes).map_err(|_| ParseError::UnreadableUtf)?;
                    self.buf.drain(..2);
                    self.state = State::Start;
                    return Ok(Some(Frame::SimpleError(payload)));
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

                State::ReadingInteger => {
                    match self.read_length()? {
                        Some(l) => {
                            self.state = State::Start;
                            return Ok(Some(Frame::Integer(l)));
                        }
                        None => return Ok(None),
                    };
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
                    if length == 0 {
                        self.state = State::Start;
                        return Ok(Some(Frame::Array(Some(Vec::new()))));
                    }
                    if length < -1 {
                        return Err(ParseError::InvalidLength);
                    }
                    let length = usize::try_from(length).map_err(|_| ParseError::InvalidLength)?;
                    self.stack.push(Vec::<Frame>::with_capacity(length));

                    self.state = State::Start;
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
    mod simple_error_tests {
        use super::*;
        #[test]
        fn incomplete_buffer_gets_ok_none() {
            let mut p = Parser::new();
            let buf = b"-";
            assert_eq!(p.parse(buf), Ok(Vec::new()));
            let mut p = Parser::new();
            let buf = b"-OK";
            assert_eq!(p.parse(buf), Ok(Vec::new()));
        }

        #[test]
        fn parse_empty_simple_error() {
            let mut p = Parser::new();
            let buf = b"-\r\n";
            assert_eq!(p.parse(buf), Ok(vec![Frame::SimpleError("".to_string())]))
        }

        #[test]
        fn parse_simple_error() {
            let mut p = Parser::new();
            let buf = b"-This is a simple error\r\n";
            assert_eq!(
                p.parse(buf),
                Ok(vec![Frame::SimpleError(
                    "This is a simple error".to_string()
                )])
            )
        }

        #[test]
        fn handles_simple_error_split_at_any_position() {
            let full_buf = b"-This is a simple error\r\n";
            for (i, _) in full_buf.iter().enumerate() {
                let mut p = Parser::new();
                let mut builder: Vec<Frame> = Vec::new();
                let (left, right) = full_buf.split_at(i);
                let mut result = p.parse(left).unwrap();
                builder.append(&mut result);
                let mut result = p.parse(right).unwrap();
                builder.append(&mut result);
                assert_eq!(
                    builder,
                    vec![Frame::SimpleError("This is a simple error".to_string())]
                )
            }
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
            let full_buf = b"$5\r\nhello\r\n$7\r\nanother\r\n$4\r\nbulk\r\n";
            for (i, _) in full_buf.iter().enumerate() {
                let mut p = Parser::new();
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

    mod array_tests {
        use super::*;
        #[test]
        fn array_marker_only_returns_none() {
            let mut p = Parser::new();
            let buf = &b"*"[..];
            assert_eq!(p.parse(buf), Ok(Vec::new()));
        }

        #[test]
        fn incomplete_length_returns_none() {
            let mut p = Parser::new();
            let buf = &b"*5"[..];
            assert_eq!(p.parse(buf), Ok(Vec::new()));
        }

        #[test]
        fn out_of_bounds_length_returns_error() {
            let mut p = Parser::new();
            let buf = b"*-2\r\n";
            assert_eq!(p.parse(buf), Err(ParseError::InvalidLength));
            assert_eq!(p.state, State::Error(ParseError::InvalidLength))
        }

        #[test]
        fn minus_one_returns_nil_array() {
            let mut p = Parser::new();
            let buf = b"*-1\r\n";
            assert_eq!(p.parse(buf), Ok(vec![Frame::Array(None)]))
        }

        #[test]
        fn zero_length_array() {
            let mut p = Parser::new();
            let buf = b"*0\r\n";
            assert_eq!(p.parse(buf), Ok(vec![Frame::Array(Some(Vec::new()))]));
        }

        #[test]
        fn complete_length_but_no_payload_returns_no_frames() {
            let mut p = Parser::new();
            let buf = b"*5\r\n";
            assert_eq!(p.parse(buf), Ok(Vec::new()));
        }

        #[test]
        fn properly_parses_array() {
            let mut p = Parser::new();
            let buf = b"*3\r\n$5\r\nhello\r\n$3\r\nbye\r\n$4\r\nmore\r\n$8leftover";
            assert_eq!(
                p.parse(buf),
                Ok(vec![Frame::Array(Some(vec![
                    Frame::Bulk(Some(b"hello".to_vec())),
                    Frame::Bulk(Some(b"bye".to_vec())),
                    Frame::Bulk(Some(b"more".to_vec()))
                ]))])
            )
        }

        #[test]
        fn divide_simple_array_at_different_locations() {
            let buf = b"*3\r\n$5\r\nhello\r\n$3\r\nbye\r\n$4\r\nmore\r\n$8leftover";
            for (i, _) in buf.iter().enumerate() {
                let mut result = Vec::<Frame>::new();
                let mut p = Parser::new();
                let (left, right) = buf.split_at(i);
                result.extend(p.parse(left).unwrap());
                result.extend(p.parse(right).unwrap());
                assert_eq!(
                    result,
                    vec![Frame::Array(Some(vec![
                        Frame::Bulk(Some(b"hello".to_vec())),
                        Frame::Bulk(Some(b"bye".to_vec())),
                        Frame::Bulk(Some(b"more".to_vec()))
                    ]))]
                )
            }
        }

        #[test]
        fn properly_parses_nested_array() {
            let mut p = Parser::new();
            let buf = b"*1\r\n*1\r\n*2\r\n$1\r\na\r\n*-1\r\n";
            assert_eq!(
                p.parse(buf),
                Ok(vec![Frame::Array(Some(vec![Frame::Array(Some(vec![
                    Frame::Array(Some(vec![
                        Frame::Bulk(Some(b"a".to_vec())),
                        Frame::Array(None)
                    ]))
                ]))]))])
            )
        }

        #[test]
        fn divide_nested_array_at_different_locations() {
            let buf = b"*1\r\n*1\r\n*2\r\n$1\r\na\r\n*-1\r\n";
            for (i, _) in buf.iter().enumerate() {
                let mut result = Vec::<Frame>::new();
                let mut p = Parser::new();
                let (left, right) = buf.split_at(i);
                result.extend(p.parse(left).unwrap());
                result.extend(p.parse(right).unwrap());
                assert_eq!(
                    result,
                    vec![Frame::Array(Some(vec![Frame::Array(Some(vec![
                        Frame::Array(Some(vec![
                            Frame::Bulk(Some(b"a".to_vec())),
                            Frame::Array(None)
                        ]))
                    ]))]))]
                )
            }
        }
    }
    mod integer_tests {
        use super::*;
        #[test]
        fn integer_marker_only_returns_none() {
            let mut p = Parser::new();
            let buf = &b":"[..];
            assert_eq!(p.parse(buf), Ok(Vec::new()));
        }

        #[test]
        fn unterminated_integer_returns_none() {
            let mut p = Parser::new();
            let buf = &b":12345"[..];
            assert_eq!(p.parse(buf), Ok(Vec::new()));
        }

        #[test]
        fn proper_integer_parsed_leaving_buffer() {
            let mut p = Parser::new();
            let buf = b":12345\r\nleftovers";
            assert_eq!(p.parse(buf), Ok(vec![Frame::Integer(12345)]));
            assert_eq!(p.buf, b"leftovers")
        }

        #[test]
        fn properly_handles_leading_signs() {
            let mut p = Parser::new();
            let buf = b":+12345\r\n:-567890\r\n";
            let result = p.parse(buf);
            assert_eq!(p.state, State::Start);
            assert_eq!(
                result,
                Ok(vec![Frame::Integer(12345), Frame::Integer(-567890)])
            );
        }

        #[test]
        fn handles_split_before_sign() {
            let mut p = Parser::new();
            let left = b":";
            let right = b"-123\r\n";
            assert_eq!(p.parse(left), Ok(vec![]));
            assert_eq!(p.parse(right), Ok(vec![Frame::Integer(-123)]));
        }

        #[test]
        fn handles_multiple_integers_split_at_any_position() {
            let full_buf = b":-123\r\n:456\r\n:7890\r\n:3333";
            for (i, _) in full_buf.iter().enumerate() {
                let mut p = Parser::new();
                let mut builder: Vec<Frame> = Vec::new();
                let (left, right) = full_buf.split_at(i);
                let mut result = p.parse(left).unwrap();
                builder.append(&mut result);
                let mut result = p.parse(right).unwrap();
                builder.append(&mut result);
                assert_eq!(
                    builder,
                    vec![
                        Frame::Integer(-123),
                        Frame::Integer(456),
                        Frame::Integer(7890),
                    ]
                )
            }
        }
    }
    mod inline_tests {
        use super::*;
        #[test]
        fn unterminated_inline_returns_none() {
            let mut p = Parser::new();
            let buf = &b"h"[..];
            assert_eq!(p.parse(buf), Ok(Vec::new()));
        }

        #[test]
        fn divide_inline_array_at_different_locations() {
            let buf = b"hello there\r\nanother line\r\nleftover";
            for (i, _) in buf.iter().enumerate() {
                let mut result = Vec::<Frame>::new();
                let mut p = Parser::new();
                let (left, right) = buf.split_at(i);
                result.extend(p.parse(left).unwrap());
                result.extend(p.parse(right).unwrap());
                assert_eq!(
                    result,
                    vec![
                        Frame::Array(Some(vec![
                            Frame::Bulk(Some(b"hello".to_vec())),
                            Frame::Bulk(Some(b"there".to_vec())),
                        ])),
                        Frame::Array(Some(vec![
                            Frame::Bulk(Some(b"another".to_vec())),
                            Frame::Bulk(Some(b"line".to_vec())),
                        ]))
                    ]
                )
            }
        }
    }
}
