#[derive(PartialEq, Eq, Debug)]
pub enum Command {
    PING,
    GET { key: String },
    SET { key: String, value: String },
    DEL { key: String },
    QUIT,
    NOOP,
}
