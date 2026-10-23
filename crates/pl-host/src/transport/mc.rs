//! The few Minecraft wire pieces the e4mc path needs: VarInts, the handshake packet and the login
//! disconnect a relay refuses with.

use std::io;

use tokio::io::{AsyncRead, AsyncReadExt};

/// Next state "login" in a handshake packet.
pub(crate) const LOGIN: i32 = 2;

/// Minecraft's VarInt: LEB128 of the two's-complement `i32`, at most 5 bytes.
pub(crate) fn put_varint(out: &mut Vec<u8>, value: i32) {
    let mut rest = value as u32;
    loop {
        if rest & !0x7F == 0 {
            out.push(rest as u8);
            return;
        }
        out.push((rest & 0x7F) as u8 | 0x80);
        rest >>= 7;
    }
}

/// Reads a VarInt from the front of `buf`.
pub(crate) fn take_varint(buf: &mut &[u8]) -> Option<i32> {
    let mut value = 0u32;
    for shift in 0..5 {
        let (&byte, rest) = buf.split_first()?;
        *buf = rest;
        value |= u32::from(byte & 0x7F) << (7 * shift);
        if byte & 0x80 == 0 {
            return Some(value as i32);
        }
    }
    None
}

pub(crate) async fn read_varint<R: AsyncRead + Unpin>(reader: &mut R) -> io::Result<i32> {
    let mut value = 0u32;
    for shift in 0..5 {
        let byte = reader.read_u8().await?;
        value |= u32::from(byte & 0x7F) << (7 * shift);
        if byte & 0x80 == 0 {
            return Ok(value as i32);
        }
    }
    Err(io::ErrorKind::InvalidData.into())
}

fn take_string<'a>(buf: &mut &'a [u8]) -> Option<&'a str> {
    let len = usize::try_from(take_varint(buf)?).ok()?;
    if len > buf.len() {
        return None;
    }
    let (text, rest) = buf.split_at(len);
    *buf = rest;
    std::str::from_utf8(text).ok()
}

/// A handshake packet with its length prefix: id 0, protocol, address, port, next state.
pub(crate) fn handshake(protocol: i32, address: &str, port: u16, next_state: i32) -> Vec<u8> {
    let mut body = Vec::with_capacity(address.len() + 12);
    put_varint(&mut body, 0);
    put_varint(&mut body, protocol);
    put_varint(&mut body, address.len() as i32);
    body.extend_from_slice(address.as_bytes());
    body.extend_from_slice(&port.to_be_bytes());
    put_varint(&mut body, next_state);
    let mut packet = Vec::with_capacity(body.len() + 2);
    put_varint(&mut packet, body.len() as i32);
    packet.extend_from_slice(&body);
    packet
}

/// A parsed handshake packet. The host checks only the address and the next state; the protocol
/// number is the tunnel's business (`hello.proto`).
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Handshake<'a> {
    #[allow(dead_code)]
    pub(crate) protocol: i32,
    pub(crate) address: &'a str,
    #[allow(dead_code)]
    pub(crate) port: u16,
    pub(crate) next_state: i32,
}

/// Parses a handshake packet's body (the bytes after its length), which must be used up exactly.
pub(crate) fn parse_handshake(mut body: &[u8]) -> Option<Handshake<'_>> {
    if take_varint(&mut body)? != 0 {
        return None;
    }
    let protocol = take_varint(&mut body)?;
    let address = take_string(&mut body)?;
    let (port, rest) = body.split_first_chunk::<2>()?;
    body = rest;
    let next_state = take_varint(&mut body)?;
    body.is_empty().then_some(Handshake {
        protocol,
        address,
        port: u16::from_be_bytes(*port),
        next_state,
    })
}

/// The address without the suffix Forge clients add (QUIClime cuts the same three).
pub(crate) fn strip_forge(address: &str) -> &str {
    ["\0FML3\0", "\0FML2\0", "\0FML\0"]
        .iter()
        .find_map(|suffix| address.strip_suffix(suffix))
        .unwrap_or(address)
}

/// The text of a packet `id 0x00 || String` and nothing else (a login disconnect).
pub(crate) fn disconnect_text(mut packet: &[u8]) -> Option<&str> {
    if take_varint(&mut packet)? != 0 {
        return None;
    }
    let text = take_string(&mut packet)?;
    packet.is_empty().then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn varint(value: i32) -> Vec<u8> {
        let mut out = Vec::new();
        put_varint(&mut out, value);
        out
    }

    #[test]
    fn varints_round_trip() {
        for (value, bytes) in [
            (0, vec![0x00]),
            (1, vec![0x01]),
            (127, vec![0x7F]),
            (128, vec![0x80, 0x01]),
            (9999, vec![0x8F, 0x4E]),
            (25565, vec![0xDD, 0xC7, 0x01]),
            (i32::MAX, vec![0xFF, 0xFF, 0xFF, 0xFF, 0x07]),
            (-1, vec![0xFF, 0xFF, 0xFF, 0xFF, 0x0F]),
        ] {
            assert_eq!(varint(value), bytes, "{value}");
            let mut slice = bytes.as_slice();
            assert_eq!(take_varint(&mut slice), Some(value));
            assert!(slice.is_empty());
        }
        let mut too_long = [0x80u8, 0x80, 0x80, 0x80, 0x80, 0x01].as_slice();
        assert_eq!(take_varint(&mut too_long), None);
    }

    #[test]
    fn handshake_round_trips() {
        let packet = handshake(9999, "able-acid.eu.e4mc.link", 25565, LOGIN);
        let mut rest = packet.as_slice();
        let len = take_varint(&mut rest).unwrap();
        assert_eq!(len as usize, rest.len());
        assert_eq!(
            parse_handshake(rest),
            Some(Handshake {
                protocol: 9999,
                address: "able-acid.eu.e4mc.link",
                port: 25565,
                next_state: LOGIN
            })
        );
        let mut trailing = rest.to_vec();
        trailing.push(0);
        assert_eq!(parse_handshake(&trailing), None);
    }

    #[test]
    fn forge_suffixes_are_cut() {
        assert_eq!(strip_forge("a.b.e4mc.link\0FML\0"), "a.b.e4mc.link");
        assert_eq!(strip_forge("a.b.e4mc.link\0FML2\0"), "a.b.e4mc.link");
        assert_eq!(strip_forge("a.b.e4mc.link\0FML3\0"), "a.b.e4mc.link");
        assert_eq!(strip_forge("a.b.e4mc.link"), "a.b.e4mc.link");
    }

    #[test]
    fn disconnect_needs_an_exact_string() {
        let mut packet = vec![0x00];
        packet.extend(varint(5));
        packet.extend_from_slice(b"hello");
        assert_eq!(disconnect_text(&packet), Some("hello"));
        packet.push(0);
        assert_eq!(disconnect_text(&packet), None);
        assert_eq!(disconnect_text(&[0x01, 0x00]), None);
    }
}
