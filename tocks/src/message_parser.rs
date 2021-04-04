use toxcore::Message;

use anyhow::{bail, Result};
use thiserror::Error;

#[derive(Error, Eq, PartialEq, Debug)]
pub enum ParseError {
    #[error("Message empty")]
    EmptyMessage,
}

fn find_split_point(utf8_string: &[u8], desired_split_point: usize) -> usize {
    let mut ret = desired_split_point;

    if ret >= utf8_string.len() {
        return utf8_string.len();
    }

    while utf8_string[ret] & 0b1100_0000 == 0b1000_0000 {
        ret -= 1;
    }

    ret
}

pub fn parse(message: String, max_message_length: usize) -> Result<Vec<Message>> {
    if message.is_empty() {
        bail!(ParseError::EmptyMessage);
    }

    if message.len() <= max_message_length {
        return Ok(vec![Message::Normal(message)]);
    }

    let message_bytes = message.into_bytes();

    let mut cursor = 0;

    let mut ret = Vec::new();
    while cursor < message_bytes.len() {
        let start = cursor;
        cursor = find_split_point(&message_bytes, cursor + max_message_length);

        let s = unsafe { std::str::from_utf8_unchecked(&message_bytes[start..cursor]).to_string() };

        ret.push(Message::Normal(s));
    }

    Ok(ret)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_message_is_err() {
        let res = parse("".into(), 100);

        let err = res.unwrap_err();
        let err = err.downcast::<ParseError>().unwrap();
        assert_eq!(err, ParseError::EmptyMessage);
    }

    #[test]
    fn string_splitting() -> Result<()> {
        let res = parse("123456".into(), 5)?;
        assert_eq!(res[0], Message::Normal("12345".into()));
        assert_eq!(res[1], Message::Normal("6".into()));
        assert_eq!(res.len(), 2);

        let res = parse("12345678901".into(), 5)?;
        assert_eq!(res[0], Message::Normal("12345".into()));
        assert_eq!(res[1], Message::Normal("67890".into()));
        assert_eq!(res[2], Message::Normal("1".into()));
        assert_eq!(res.len(), 3);

        Ok(())
    }

    #[test]
    fn utf8_string_splitting() -> Result<()> {
        // ࣢ is a 3 byte utf8 character
        let res = parse("12345࣢".into(), 5)?;
        assert_eq!(res[0], Message::Normal("12345".into()));
        assert_eq!(res[1], Message::Normal("࣢".into()));
        assert_eq!(res.len(), 2);

        let res = parse("1234࣢".into(), 5)?;
        assert_eq!(res[0], Message::Normal("1234".into()));
        assert_eq!(res[1], Message::Normal("࣢".into()));
        assert_eq!(res.len(), 2);

        let res = parse("123࣢".into(), 5)?;
        assert_eq!(res[0], Message::Normal("123".into()));
        assert_eq!(res[1], Message::Normal("࣢".into()));
        assert_eq!(res.len(), 2);

        let res = parse("12࣢".into(), 5)?;
        assert_eq!(res[0], Message::Normal("12࣢".into()));
        assert_eq!(res.len(), 1);

        let res = parse("123࣢78901".into(), 5)?;
        assert_eq!(res[0], Message::Normal("123".into()));
        assert_eq!(res[1], Message::Normal("࣢78".into()));
        assert_eq!(res[2], Message::Normal("901".into()));
        assert_eq!(res.len(), 3);

        Ok(())
    }
}
