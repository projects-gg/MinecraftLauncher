//! Minecraft's VarInt: LEB128 of the two's-complement `i32`, at most 5 bytes.

use tokio::io::{AsyncRead, AsyncReadExt};

/// The longest VarInt.
pub const MAX_LEN: usize = 5;

/// Appends `value`'s VarInt to `out`.
pub fn write(out: &mut Vec<u8>, value: i32) {
    let mut rest = value as u32;
    loop {
        let byte = (rest & 0x7f) as u8;
        rest >>= 7;
        if rest == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

/// `value`'s VarInt.
pub fn encode(value: i32) -> Vec<u8> {
    let mut out = Vec::with_capacity(MAX_LEN);
    write(&mut out, value);
    out
}

/// What reading a VarInt from a buffer found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decoded {
    /// The value and how many bytes it took.
    Value(i32, usize),
    /// The buffer ends inside the VarInt.
    Incomplete,
    /// More than five bytes, or a fifth byte with bits beyond 32.
    TooLong,
}

/// Reads a VarInt from the start of `bytes`.
pub fn decode(bytes: &[u8]) -> Decoded {
    let mut value: u32 = 0;
    for (i, &byte) in bytes.iter().enumerate().take(MAX_LEN) {
        if i == MAX_LEN - 1 && byte & 0xf0 != 0 {
            return Decoded::TooLong;
        }
        value |= u32::from(byte & 0x7f) << (7 * i);
        if byte & 0x80 == 0 {
            return Decoded::Value(value as i32, i + 1);
        }
    }
    if bytes.len() >= MAX_LEN {
        Decoded::TooLong
    } else {
        Decoded::Incomplete
    }
}

/// Reads one VarInt from a stream, byte by byte.
pub async fn read<R: AsyncRead + Unpin + ?Sized>(reader: &mut R) -> std::io::Result<i32> {
    let mut buf = [0u8; MAX_LEN];
    for i in 0..MAX_LEN {
        buf[i] = reader.read_u8().await?;
        match decode(&buf[..=i]) {
            Decoded::Value(value, _) => return Ok(value),
            Decoded::Incomplete => {}
            Decoded::TooLong => break,
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "VarInt too long",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_cases() {
        assert_eq!(decode(&[0x80]), Decoded::Incomplete);
        assert_eq!(decode(&[]), Decoded::Incomplete);
        assert_eq!(decode(&[0xff, 0xff, 0xff, 0xff, 0x1f]), Decoded::TooLong);
        assert_eq!(
            decode(&[0x80, 0x80, 0x80, 0x80, 0x80, 0x00]),
            Decoded::TooLong
        );
        assert_eq!(decode(&[0x2a, 0xff]), Decoded::Value(42, 1));
    }

    #[tokio::test]
    async fn reads_from_a_stream() {
        let mut bytes: &[u8] = &[0xdd, 0xc7, 0x01, 0x07];
        assert_eq!(read(&mut bytes).await.unwrap(), 25565);
        assert_eq!(bytes, &[0x07]);
        let mut long: &[u8] = &[0x80, 0x80, 0x80, 0x80, 0x80];
        assert!(read(&mut long).await.is_err());
    }
}
