#[derive(Debug, PartialEq)]
pub enum Frame {
    SimpleString(String),
    SimpleError(String),
    Bulk(Option<Vec<u8>>),
    Integer(i64),
    Array(Option<Vec<Frame>>),
}
