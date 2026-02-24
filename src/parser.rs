use std::string::ParseError;

enum Frame {
    SimpleString(String),
    SimpleError(String),
    Bulk(Option<String>),
    Array(Option<Vec<Frame>>),
}

enum State {
    Start,
    ReadingLine,
    ReadingBulk,
    ReadingSize,
    ReadingArray,
}

struct Parser {
    state: State,
}

impl Parser {
    fn parse(&self, buf: &[u8]) -> Result<Option<(Frame, usize)>, ParseError> {
        match self.state {
            State::Start => {
                if let Some(first_character) = buf.get(0) {
                } else {
                    return Ok(None);
                }
            }
        }
        Ok(None)
    }
}
