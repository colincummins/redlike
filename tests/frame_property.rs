use proptest::prelude::*;
use redlike::frame::Frame;
use redlike::parser::Parser;

fn arb_frame() -> impl Strategy<Value = Frame> {
    let leaf = prop_oneof![
        prop::string::string_regex("[^\r\n]{0,20}")
            .unwrap()
            .prop_map(Frame::SimpleString),
        prop::string::string_regex("[^\r\n]{0,20}")
            .unwrap()
            .prop_map(Frame::SimpleError),
        prop::collection::vec(any::<u8>(), 0..20).prop_map(|v| Frame::Bulk(Some(v))),
        Just(Frame::Bulk(None)),
        any::<i64>().prop_map(Frame::Integer),
        Just(Frame::Array(None)),
    ];
    leaf.prop_recursive(8, 256, 10, |inner| {
        prop_oneof![prop::collection::vec(inner.clone(), 0..10).prop_map(|a| Frame::Array(Some(a)))]
    })
}

proptest! {
    #[test]
    fn frame_bytes_frame(f in arb_frame()) {
        let mut p = Parser::new();
        prop_assert_eq!(p.parse(f.to_bytes().as_slice()).unwrap(),&[f]);
        prop_assert!(p.is_empty())
    }

}

fn main() {
    frame_bytes_frame();
}
