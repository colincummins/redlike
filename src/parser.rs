#[derive(Debug, PartialEq)]
enum Frame {
    SimpleString(String),
    SimpleError(String),
    Bulk(Option<String>),
    Array(Option<Vec<Frame>>),
}

#[derive(Debug, PartialEq)]
enum State {
    Start,
    ReadingLine,
    ReadingBulk,
    ReadingSize,
    ReadingArray,
}

#[derive(Debug, PartialEq)]
enum ParseError {
    ParseStrError,
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

    fn parse<'a>(&self, buf: &'a [u8]) -> Result<Option<(Frame, &'a [u8])>, ParseError> {
        match self.state {
            State::Start => {
                if let Some(first) = buf.get(0) {
                    match first {
                        b'+' => {
                            let pos = match buf.windows(2).position(|w| w == b"\r\n") {
                                Some(pos) => pos,
                                None => return Ok(None),
                            };
                            let payload = std::str::from_utf8(&buf[1..pos])
                                .map_err(|_| ParseError::ParseStrError)?;
                            return Ok(Some((
                                Frame::SimpleString(payload.to_owned()),
                                &buf[pos + 2..],
                            )));
                        }
                        _ => return Ok(None),
                    }
                } else {
                    return Ok(None);
                }
            }
            _ => return Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn incomplete_just_a_plus() {
        let p = Parser::new();
        let buf = b"+";
        assert_eq!(p.parse(buf), Ok(None));
    }

    #[test]
    fn incomplete_plus_and_ok() {
        let p = Parser::new();
        let buf = b"+OK";
        assert_eq!(p.parse(buf), Ok(None));
    }

    #[test]
    fn empty_simple_string() {
        let p = Parser::new();
        let buf = b"+\r\n";
        assert_eq!(
            p.parse(buf),
            Ok(Some((Frame::SimpleString("".to_string()), &b""[..])))
        )
    }

    #[test]
    fn parse_simple_string() {
        let p = Parser::new();
        let buf = b"+OK\r\n";
        assert_eq!(
            p.parse(buf),
            Ok(Some((Frame::SimpleString("OK".to_string()), &b""[..])))
        )
    }

    #[test]
    fn only_grabs_one_frame() {
        let p = Parser::new();
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
