#[derive(Debug, PartialEq, Clone, Eq)]
pub enum Frame {
    SimpleString(String),
    SimpleError(String),
    Bulk(Option<Vec<u8>>),
    Integer(i64),
    Array(Option<Vec<Frame>>),
}

impl Frame {
    fn write_to(&self, buf: &mut Vec<u8>) {
        match self {
            Self::SimpleString(inner) => {
                buf.extend_from_slice(b"+");
                buf.extend_from_slice(inner.as_bytes());
                buf.extend_from_slice(b"\r\n");
            }
            Self::SimpleError(inner) => {
                buf.extend_from_slice(b"-");
                buf.extend_from_slice(inner.as_bytes());
                buf.extend_from_slice(b"\r\n");
            }
            Self::Bulk(Some(inner)) => {
                buf.extend_from_slice(b"$");
                buf.extend_from_slice(inner.iter().len().to_string().as_bytes());
                buf.extend_from_slice(b"\r\n");
                buf.extend_from_slice(inner);
                buf.extend_from_slice(b"\r\n");
            }
            Self::Bulk(None) => {
                buf.extend_from_slice(b"$-1\r\n");
            }
            Self::Integer(inner) => {
                buf.extend_from_slice(b":");
                buf.extend_from_slice(inner.to_string().as_bytes());
                buf.extend_from_slice(b"\r\n");
            }
            Self::Array(Some(inner)) => {
                buf.extend_from_slice(b"*");
                buf.extend_from_slice(inner.iter().len().to_string().as_bytes());
                buf.extend_from_slice(b"\r\n");
                for element in inner {
                    element.write_to(buf);
                }
            }
            Self::Array(None) => {
                buf.extend_from_slice(b"*-1\r\n");
            }
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::<u8>::new();
        self.write_to(&mut buf);
        buf
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn to_simple_string() {
        assert_eq!(
            Frame::SimpleString("Hello".to_string()).to_bytes(),
            b"+Hello\r\n".to_vec()
        );
    }
    #[test]
    fn to_simple_error() {
        assert_eq!(
            Frame::SimpleError("Error".to_string()).to_bytes(),
            b"-Error\r\n".to_vec()
        );
    }

    #[test]
    fn bulk_some() {
        assert_eq!(
            Frame::Bulk(Some("test".as_bytes().to_vec())).to_bytes(),
            b"$4\r\ntest\r\n".to_vec()
        )
    }

    #[test]
    fn bulk_nil() {
        assert_eq!(Frame::Bulk(None).to_bytes(), b"$-1\r\n".to_vec())
    }

    #[test]
    fn positive_integer() {
        assert_eq!(Frame::Integer(12345).to_bytes(), b":12345\r\n".to_vec())
    }

    #[test]
    fn negative_integer() {
        assert_eq!(Frame::Integer(-12345).to_bytes(), b":-12345\r\n".to_vec())
    }

    #[test]
    fn nil_array() {
        assert_eq!(Frame::Array(None).to_bytes(), b"*-1\r\n".to_vec())
    }

    #[test]
    fn zero_length_array() {
        assert_eq!(Frame::Array(Some(vec![])).to_bytes(), b"*0\r\n".to_vec())
    }

    #[test]
    fn some_array() {
        assert_eq!(
            Frame::Array(Some(vec![
                Frame::Bulk(Some(b"hello".to_vec())),
                Frame::Integer(12345)
            ]))
            .to_bytes(),
            b"*2\r\n$5\r\nhello\r\n:12345\r\n".to_vec()
        )
    }
}
