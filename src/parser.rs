use std::marker::PhantomData;

enum Frame {
    SimpleString(String),
    SimpleError(String),
    Bulk(Option<String>),
    Array(Option<Vec<Frame>>),
}

struct Start;
struct ReadingLine;
struct ReadingBulkData;

struct ParserShared {
    bytes_consumed: usize,
}
struct Parser<T> {
    shared: ParserShared,
    state: PhantomData<T>,
}
