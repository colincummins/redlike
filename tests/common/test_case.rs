pub struct TestCase<'a> {
    pub call: &'a str,
    pub response: &'a str,
    pub expected: &'a str,
}
