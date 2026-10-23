//! STUN messages (RFC 8489) and TURN's ChannelData (RFC 8656): what a TURN client sends and reads.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use hmac::{Hmac, KeyInit, Mac};
use md5::{Digest, Md5};
use sha1::Sha1;
use zeroize::Zeroizing;

pub(crate) const MAGIC_COOKIE: u32 = 0x2112_A442;
pub(crate) const HEADER_LEN: usize = 20;
const FINGERPRINT_XOR: u32 = 0x5354_554E;
/// RFC 8489 §9.2: a nonce that starts with this lists the server's security features.
const NONCE_COOKIE: &[u8] = b"obMatJos2";

pub(crate) mod method {
    pub(crate) const ALLOCATE: u16 = 0x003;
    pub(crate) const REFRESH: u16 = 0x004;
    /// Send indications are never sent (every peer gets a channel first).
    #[cfg(test)]
    pub(crate) const SEND: u16 = 0x006;
    pub(crate) const DATA: u16 = 0x007;
    pub(crate) const CREATE_PERMISSION: u16 = 0x008;
    pub(crate) const CHANNEL_BIND: u16 = 0x009;
}

pub(crate) mod attr {
    pub(crate) const USERNAME: u16 = 0x0006;
    pub(crate) const MESSAGE_INTEGRITY: u16 = 0x0008;
    pub(crate) const ERROR_CODE: u16 = 0x0009;
    pub(crate) const CHANNEL_NUMBER: u16 = 0x000C;
    pub(crate) const LIFETIME: u16 = 0x000D;
    pub(crate) const XOR_PEER_ADDRESS: u16 = 0x0012;
    pub(crate) const DATA: u16 = 0x0013;
    pub(crate) const REALM: u16 = 0x0014;
    pub(crate) const NONCE: u16 = 0x0015;
    pub(crate) const XOR_RELAYED_ADDRESS: u16 = 0x0016;
    pub(crate) const REQUESTED_TRANSPORT: u16 = 0x0019;
    pub(crate) const PASSWORD_ALGORITHM: u16 = 0x001D;
    /// Our own address as the server sees it: never read, never sent anywhere (PROTOCOL.md §5.1).
    #[cfg(test)]
    pub(crate) const XOR_MAPPED_ADDRESS: u16 = 0x0020;
    pub(crate) const PASSWORD_ALGORITHMS: u16 = 0x8002;
    pub(crate) const FINGERPRINT: u16 = 0x8028;
}

/// The message class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Class {
    Request,
    Indication,
    Success,
    Error,
}

pub(crate) fn message_type(method: u16, class: Class) -> u16 {
    let class: u16 = match class {
        Class::Request => 0b00,
        Class::Indication => 0b01,
        Class::Success => 0b10,
        Class::Error => 0b11,
    };
    ((method & 0x0F80) << 2)
        | ((method & 0x0070) << 1)
        | (method & 0x000F)
        | ((class & 0b10) << 7)
        | ((class & 0b01) << 4)
}

fn split_type(message_type: u16) -> (u16, Class) {
    let method =
        ((message_type & 0x3E00) >> 2) | ((message_type & 0x00E0) >> 1) | (message_type & 0x000F);
    let class = match ((message_type & 0x0100) >> 7) | ((message_type & 0x0010) >> 4) {
        0b00 => Class::Request,
        0b01 => Class::Indication,
        0b10 => Class::Success,
        _ => Class::Error,
    };
    (method, class)
}

fn pad4(len: usize) -> usize {
    (len + 3) & !3
}

/// Builds one message, attribute by attribute.
pub(crate) struct Builder {
    buf: Vec<u8>,
    txid: [u8; 12],
}

impl Builder {
    pub(crate) fn new(method: u16, class: Class, txid: [u8; 12]) -> Self {
        let mut buf = Vec::with_capacity(160);
        buf.extend_from_slice(&message_type(method, class).to_be_bytes());
        buf.extend_from_slice(&[0, 0]);
        buf.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());
        buf.extend_from_slice(&txid);
        Self { buf, txid }
    }

    fn set_body_len(&mut self, len: usize) {
        self.buf[2..4].copy_from_slice(&(len as u16).to_be_bytes());
    }

    pub(crate) fn attr(&mut self, kind: u16, value: &[u8]) -> &mut Self {
        self.buf.extend_from_slice(&kind.to_be_bytes());
        self.buf
            .extend_from_slice(&(value.len() as u16).to_be_bytes());
        self.buf.extend_from_slice(value);
        self.buf.resize(pad4(self.buf.len()), 0);
        let body = self.buf.len() - HEADER_LEN;
        self.set_body_len(body);
        self
    }

    pub(crate) fn u32(&mut self, kind: u16, value: u32) -> &mut Self {
        self.attr(kind, &value.to_be_bytes())
    }

    pub(crate) fn xor_address(&mut self, kind: u16, address: SocketAddr) -> &mut Self {
        let value = encode_xor_address(address, &self.txid);
        self.attr(kind, &value)
    }

    /// MESSAGE-INTEGRITY: HMAC-SHA1 over the message so far, its length counting this attribute.
    pub(crate) fn integrity(&mut self, key: &[u8]) -> &mut Self {
        let body = self.buf.len() - HEADER_LEN + 24;
        self.set_body_len(body);
        let mac = hmac_sha1(key, &self.buf);
        self.buf
            .extend_from_slice(&attr::MESSAGE_INTEGRITY.to_be_bytes());
        self.buf.extend_from_slice(&20u16.to_be_bytes());
        self.buf.extend_from_slice(&mac);
        self
    }

    /// FINGERPRINT: CRC-32 of the message so far XOR 0x5354554E, its length counting this attribute.
    pub(crate) fn fingerprint(&mut self) -> &mut Self {
        let body = self.buf.len() - HEADER_LEN + 8;
        self.set_body_len(body);
        let crc = crc32fast::hash(&self.buf) ^ FINGERPRINT_XOR;
        self.buf.extend_from_slice(&attr::FINGERPRINT.to_be_bytes());
        self.buf.extend_from_slice(&4u16.to_be_bytes());
        self.buf.extend_from_slice(&crc.to_be_bytes());
        self
    }

    pub(crate) fn finish(self) -> Vec<u8> {
        self.buf
    }
}

fn hmac_sha1(key: &[u8], data: &[u8]) -> [u8; 20] {
    let mut mac = <Hmac<Sha1> as KeyInit>::new_from_slice(key).expect("HMAC takes any key length");
    mac.update(data);
    let mut out = [0u8; 20];
    out.copy_from_slice(&mac.finalize().into_bytes());
    out
}

fn encode_xor_address(address: SocketAddr, txid: &[u8; 12]) -> Vec<u8> {
    let port = address.port() ^ (MAGIC_COOKIE >> 16) as u16;
    let mut value = Vec::with_capacity(20);
    match address.ip() {
        IpAddr::V4(ip) => {
            value.extend_from_slice(&[0, 0x01]);
            value.extend_from_slice(&port.to_be_bytes());
            value.extend_from_slice(&(u32::from(ip) ^ MAGIC_COOKIE).to_be_bytes());
        }
        IpAddr::V6(ip) => {
            value.extend_from_slice(&[0, 0x02]);
            value.extend_from_slice(&port.to_be_bytes());
            let key = xor_key_v6(txid);
            value.extend(ip.octets().iter().zip(key).map(|(a, b)| a ^ b));
        }
    }
    value
}

fn xor_key_v6(txid: &[u8; 12]) -> [u8; 16] {
    let mut key = [0u8; 16];
    key[..4].copy_from_slice(&MAGIC_COOKIE.to_be_bytes());
    key[4..].copy_from_slice(txid);
    key
}

fn decode_xor_address(value: &[u8], txid: &[u8; 12]) -> Option<SocketAddr> {
    let port = u16::from_be_bytes([*value.get(2)?, *value.get(3)?]) ^ (MAGIC_COOKIE >> 16) as u16;
    match (value.get(1)?, value.len()) {
        (0x01, 8) => {
            let raw = u32::from_be_bytes([value[4], value[5], value[6], value[7]]) ^ MAGIC_COOKIE;
            Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::from(raw)), port))
        }
        (0x02, 20) => {
            let key = xor_key_v6(txid);
            let mut octets = [0u8; 16];
            for (index, octet) in octets.iter_mut().enumerate() {
                *octet = value[4 + index] ^ key[index];
            }
            Some(SocketAddr::new(IpAddr::V6(Ipv6Addr::from(octets)), port))
        }
        _ => None,
    }
}

#[derive(Debug, Clone, Copy)]
struct Attr {
    kind: u16,
    /// Where the attribute's header starts.
    at: usize,
    /// Where its value starts.
    start: usize,
    len: usize,
}

/// A parsed message; borrows the bytes it came from.
pub(crate) struct Message<'a> {
    pub(crate) method: u16,
    pub(crate) class: Class,
    pub(crate) txid: [u8; 12],
    raw: &'a [u8],
    attrs: Vec<Attr>,
}

/// Parses one whole STUN message (exactly `raw`).
pub(crate) fn parse(raw: &[u8]) -> Option<Message<'_>> {
    if raw.len() < HEADER_LEN || raw[0] & 0xC0 != 0 {
        return None;
    }
    let body = usize::from(u16::from_be_bytes([raw[2], raw[3]]));
    if body % 4 != 0 || raw.len() != HEADER_LEN + body || raw[4..8] != MAGIC_COOKIE.to_be_bytes() {
        return None;
    }
    let (method, class) = split_type(u16::from_be_bytes([raw[0], raw[1]]));
    let mut txid = [0u8; 12];
    txid.copy_from_slice(&raw[8..20]);
    let mut attrs = Vec::new();
    let mut at = HEADER_LEN;
    while at < raw.len() {
        let header = raw.get(at..at + 4)?;
        let kind = u16::from_be_bytes([header[0], header[1]]);
        let len = usize::from(u16::from_be_bytes([header[2], header[3]]));
        let start = at + 4;
        if start + len > raw.len() {
            return None;
        }
        attrs.push(Attr {
            kind,
            at,
            start,
            len,
        });
        at = start + pad4(len);
    }
    (at == raw.len()).then_some(Message {
        method,
        class,
        txid,
        raw,
        attrs,
    })
}

impl<'a> Message<'a> {
    /// An attribute's value; attributes after MESSAGE-INTEGRITY are ignored (RFC 8489 §14.5).
    pub(crate) fn get(&self, kind: u16) -> Option<&'a [u8]> {
        self.attrs
            .iter()
            .take_while(|attr| attr.kind != attr::MESSAGE_INTEGRITY)
            .find(|attr| attr.kind == kind)
            .map(|attr| &self.raw[attr.start..attr.start + attr.len])
    }

    pub(crate) fn u32(&self, kind: u16) -> Option<u32> {
        let value = self.get(kind)?;
        Some(u32::from_be_bytes(value.try_into().ok()?))
    }

    pub(crate) fn xor_address(&self, kind: u16) -> Option<SocketAddr> {
        decode_xor_address(self.get(kind)?, &self.txid)
    }

    /// ERROR-CODE as a number (class × 100 + number).
    pub(crate) fn error_code(&self) -> Option<u16> {
        let value = self.get(attr::ERROR_CODE)?;
        if value.len() < 4 {
            return None;
        }
        Some(u16::from(value[2] & 0x07) * 100 + u16::from(value[3]))
    }

    /// Whether MESSAGE-INTEGRITY checks out with `key`; `None` when the message has none.
    pub(crate) fn check_integrity(&self, key: &[u8]) -> Option<bool> {
        let attr = self
            .attrs
            .iter()
            .find(|attr| attr.kind == attr::MESSAGE_INTEGRITY)?;
        if attr.len != 20 {
            return Some(false);
        }
        let mut signed = self.raw[..attr.at].to_vec();
        let body = attr.at - HEADER_LEN + 24;
        signed[2..4].copy_from_slice(&(body as u16).to_be_bytes());
        let mut mac =
            <Hmac<Sha1> as KeyInit>::new_from_slice(key).expect("HMAC takes any key length");
        mac.update(&signed);
        Some(
            mac.verify_slice(&self.raw[attr.start..attr.start + 20])
                .is_ok(),
        )
    }

    /// Whether FINGERPRINT checks out; `None` when the message has none. (The client does not need
    /// it: ChannelData and STUN differ in their first bits.)
    #[cfg(test)]
    pub(crate) fn check_fingerprint(&self) -> Option<bool> {
        let attr = self
            .attrs
            .iter()
            .find(|attr| attr.kind == attr::FINGERPRINT)?;
        if attr.len != 4 || attr.start + 4 != self.raw.len() {
            return Some(false);
        }
        let expected = crc32fast::hash(&self.raw[..attr.at]) ^ FINGERPRINT_XOR;
        Some(self.raw[attr.start..attr.start + 4] == expected.to_be_bytes())
    }
}

/// The long-term credential key (RFC 8489 §9.2.2, MD5): MD5(username ":" realm ":" password).
pub(crate) fn long_term_key(username: &str, realm: &str, password: &str) -> Zeroizing<[u8; 16]> {
    let input = Zeroizing::new(format!("{username}:{realm}:{password}"));
    let mut key = Zeroizing::new([0u8; 16]);
    key.copy_from_slice(&Md5::digest(input.as_bytes()));
    key
}

/// Whether a challenge's nonce carries RFC 8489's security feature cookie.
pub(crate) fn nonce_has_cookie(nonce: &[u8]) -> bool {
    nonce.starts_with(NONCE_COOKIE)
}

/// Whether PASSWORD-ALGORITHMS lists MD5 (algorithm 1).
pub(crate) fn offers_md5(list: &[u8]) -> bool {
    let mut rest = list;
    while rest.len() >= 4 {
        let algorithm = u16::from_be_bytes([rest[0], rest[1]]);
        let params = usize::from(u16::from_be_bytes([rest[2], rest[3]]));
        if algorithm == 0x0001 {
            return true;
        }
        let next = 4 + pad4(params);
        if next > rest.len() {
            return false;
        }
        rest = &rest[next..];
    }
    false
}

/// PASSWORD-ALGORITHM for MD5 without parameters.
pub(crate) const PASSWORD_ALGORITHM_MD5: [u8; 4] = [0x00, 0x01, 0x00, 0x00];

/// Whether a frame starting with `first` is ChannelData (channel numbers 0x4000–0x7FFF, RFC 5766 and
/// 8656), as opposed to STUN (first two bits zero).
pub(crate) fn is_channel_data(first: u8) -> bool {
    (0x40..=0x7F).contains(&first)
}

/// A ChannelData message; `pad` to four bytes over TCP and TLS (RFC 8656 §12.5).
pub(crate) fn channel_data(channel: u16, payload: &[u8], pad: bool) -> Vec<u8> {
    let len = 4 + payload.len();
    let mut frame = Vec::with_capacity(pad4(len));
    frame.extend_from_slice(&channel.to_be_bytes());
    frame.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    frame.extend_from_slice(payload);
    if pad {
        frame.resize(pad4(len), 0);
    }
    frame
}

/// A ChannelData message's channel and where its payload is in `frame`.
pub(crate) fn parse_channel_data(frame: &[u8]) -> Option<(u16, std::ops::Range<usize>)> {
    if frame.len() < 4 || !is_channel_data(frame[0]) {
        return None;
    }
    let channel = u16::from_be_bytes([frame[0], frame[1]]);
    let len = usize::from(u16::from_be_bytes([frame[2], frame[3]]));
    (4 + len <= frame.len()).then_some((channel, 4..4 + len))
}

/// How long the frame starting with these four bytes is on a TCP or TLS stream (ChannelData
/// padding included), or `None` for something that is neither STUN nor ChannelData.
pub(crate) fn frame_len(head: [u8; 4]) -> Option<usize> {
    let len = usize::from(u16::from_be_bytes([head[2], head[3]]));
    if head[0] & 0xC0 == 0 {
        Some(HEADER_LEN + len)
    } else if is_channel_data(head[0]) {
        Some(pad4(4 + len))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // RFC 5769 §2.1: a request with a short-term credential (the key is the password).
    const SAMPLE_REQUEST: &str = "000100582112a442b7e7a701bc34d686fa87dfae802200105354554e207465737420636c69656e74\
00240004 6e0001ff80290008932ff9b151263b36000600096576746a3a6836765920202000080014\
9aeaa70cbfd8cb56781ef2b5b2d3f249c1b571a280280004e57a3bcf";
    // RFC 5769 §2.2: an IPv4 response.
    const SAMPLE_RESPONSE: &str = "0101003c2112a442b7e7a701bc34d686fa87dfae8022000b7465737420766563746f7220\
00200008 0001a147e112a643000800142b91f599fd9e90c38c7489f92af9ba53f06be7d780280004c07d4c96";
    // RFC 5769 §2.3: an IPv6 response.
    const SAMPLE_RESPONSE_V6: &str = "010100482112a442b7e7a701bc34d686fa87dfae8022000b7465737420766563746f7220\
002000140002a1470113a9faa5d3f179bc25f4b5bed2b9d900080014a382954e4be67bf11784c97c8292c275bfe3ed41\
80280004c8fb0b4c";
    const PASSWORD: &str = "VOkJxbRl1RmTxUk/WvJxBt";

    fn bytes(hex_text: &str) -> Vec<u8> {
        hex::decode(hex_text.replace(' ', "")).unwrap()
    }

    #[test]
    fn message_types_split_back() {
        for method in [
            method::ALLOCATE,
            method::REFRESH,
            method::SEND,
            method::DATA,
            method::CREATE_PERMISSION,
            method::CHANNEL_BIND,
        ] {
            for class in [
                Class::Request,
                Class::Indication,
                Class::Success,
                Class::Error,
            ] {
                assert_eq!(split_type(message_type(method, class)), (method, class));
            }
        }
        assert_eq!(message_type(method::ALLOCATE, Class::Success), 0x0103);
        assert_eq!(message_type(method::ALLOCATE, Class::Error), 0x0113);
        assert_eq!(message_type(method::SEND, Class::Indication), 0x0016);
        assert_eq!(message_type(method::DATA, Class::Indication), 0x0017);
        assert_eq!(message_type(method::CHANNEL_BIND, Class::Request), 0x0009);
    }

    #[test]
    fn rfc5769_request_checks_out() {
        let raw = bytes(SAMPLE_REQUEST);
        let message = parse(&raw).unwrap();
        assert_eq!((message.method, message.class), (0x001, Class::Request));
        assert_eq!(message.get(attr::USERNAME), Some(&b"evtj:h6vY"[..]));
        assert_eq!(message.check_integrity(PASSWORD.as_bytes()), Some(true));
        assert_eq!(message.check_integrity(b"wrong"), Some(false));
        assert_eq!(message.check_fingerprint(), Some(true));
    }

    #[test]
    fn rfc5769_responses_carry_their_mapped_addresses() {
        let raw = bytes(SAMPLE_RESPONSE);
        let message = parse(&raw).unwrap();
        assert_eq!(message.class, Class::Success);
        assert_eq!(
            message.xor_address(attr::XOR_MAPPED_ADDRESS),
            Some("192.0.2.1:32853".parse().unwrap())
        );
        assert_eq!(message.check_integrity(PASSWORD.as_bytes()), Some(true));
        assert_eq!(message.check_fingerprint(), Some(true));

        let raw = bytes(SAMPLE_RESPONSE_V6);
        let message = parse(&raw).unwrap();
        assert_eq!(
            message.xor_address(attr::XOR_MAPPED_ADDRESS),
            Some(
                "[2001:db8:1234:5678:11:2233:4455:6677]:32853"
                    .parse()
                    .unwrap()
            )
        );
        assert_eq!(message.check_integrity(PASSWORD.as_bytes()), Some(true));
        assert_eq!(message.check_fingerprint(), Some(true));
    }

    #[test]
    fn rfc5769_long_term_request_is_built_byte_for_byte() {
        // RFC 5769 §2.4. The password is the SASLprep'd "TheMatrIX".
        let expected = bytes(
            "000100602112a44278ad3433c6ad72c029da412e00060012e3839ee38388e383aae38383e382afe382b90000\
             0015001c662f2f3439396b39353464364f4c33346f4c394653547679363473410014000b6578616d706c652e6f726700\
             00080014f67024656dd64a3e02b8e0712e85c9a28ca89666",
        );
        let username = "\u{30DE}\u{30C8}\u{30EA}\u{30C3}\u{30AF}\u{30B9}";
        let key = long_term_key(username, "example.org", "TheMatrIX");
        let mut txid = [0u8; 12];
        txid.copy_from_slice(&expected[8..20]);
        const BINDING: u16 = 0x001;
        let mut builder = Builder::new(BINDING, Class::Request, txid);
        builder
            .attr(attr::USERNAME, username.as_bytes())
            .attr(attr::NONCE, b"f//499k954d6OL34oL9FSTvy64sA")
            .attr(attr::REALM, b"example.org")
            .integrity(key.as_ref());
        assert_eq!(hex::encode(builder.finish()), hex::encode(&expected));
        let message = parse(&expected).unwrap();
        assert_eq!(message.check_integrity(key.as_ref()), Some(true));
    }

    #[test]
    fn built_messages_verify() {
        let txid = [7u8; 12];
        let peer: SocketAddr = "203.0.113.9:4242".parse().unwrap();
        let mut builder = Builder::new(method::CHANNEL_BIND, Class::Request, txid);
        builder
            .u32(attr::CHANNEL_NUMBER, 0x4000_0000)
            .xor_address(attr::XOR_PEER_ADDRESS, peer)
            .integrity(b"key")
            .fingerprint();
        let raw = builder.finish();
        let message = parse(&raw).unwrap();
        assert_eq!(message.method, method::CHANNEL_BIND);
        assert_eq!(message.txid, txid);
        assert_eq!(message.u32(attr::CHANNEL_NUMBER), Some(0x4000_0000));
        assert_eq!(message.xor_address(attr::XOR_PEER_ADDRESS), Some(peer));
        assert_eq!(message.check_integrity(b"key"), Some(true));
        assert_eq!(message.check_fingerprint(), Some(true));
        assert_eq!(frame_len(raw[..4].try_into().unwrap()), Some(raw.len()));
        assert!(parse(&raw[..raw.len() - 4]).is_none());
    }

    #[test]
    fn error_codes_read_as_numbers() {
        let mut builder = Builder::new(method::REFRESH, Class::Error, [1u8; 12]);
        builder.attr(
            attr::ERROR_CODE,
            &[0, 0, 4, 38, b'S', b't', b'a', b'l', b'e'],
        );
        let raw = builder.finish();
        assert_eq!(parse(&raw).unwrap().error_code(), Some(438));
    }

    #[test]
    fn channel_data_is_padded_only_on_streams() {
        let udp = channel_data(0x4001, b"abcde", false);
        assert_eq!(udp, [0x40, 0x01, 0x00, 0x05, b'a', b'b', b'c', b'd', b'e']);
        let tcp = channel_data(0x4001, b"abcde", true);
        assert_eq!(tcp.len(), 12);
        assert_eq!(frame_len(tcp[..4].try_into().unwrap()), Some(12));
        assert_eq!(parse_channel_data(&tcp), Some((0x4001, 4..9)));
        assert_eq!(parse_channel_data(&udp), Some((0x4001, 4..9)));
        assert_eq!(parse_channel_data(&udp[..6]), None);
        assert_eq!(frame_len([0x80, 0, 0, 0]), None);
    }

    #[test]
    fn password_algorithms_are_read() {
        assert!(offers_md5(&[
            0x00, 0x02, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00
        ]));
        assert!(!offers_md5(&[0x00, 0x02, 0x00, 0x00]));
        assert!(nonce_has_cookie(b"obMatJos2AAAAabc"));
        assert!(!nonce_has_cookie(b"plain-nonce"));
    }
}
