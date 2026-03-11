use redlike::frame::Frame;

pub struct TestCase<'a> {
    pub call: &'a [u8],
    pub response: Frame,
    pub expected: &'a str,
}
