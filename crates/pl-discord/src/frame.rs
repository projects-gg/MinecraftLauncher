//! IPC framing: `u32` LE opcode, `u32` LE payload length, JSON payload.

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::error::{Error, Result};

pub const HEADER_LEN: usize = 8;
/// Largest payload accepted from Discord (READY carries user and config data, a few KiB).
pub const MAX_PAYLOAD: usize = 1 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum Opcode {
    Handshake = 0,
    Frame = 1,
    Close = 2,
    Ping = 3,
    Pong = 4,
}

impl Opcode {
    pub fn from_u32(value: u32) -> Option<Self> {
        Some(match value {
            0 => Self::Handshake,
            1 => Self::Frame,
            2 => Self::Close,
            3 => Self::Ping,
            4 => Self::Pong,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub opcode: Opcode,
    pub payload: Vec<u8>,
}

impl Frame {
    pub fn new(opcode: Opcode, payload: Vec<u8>) -> Self {
        Self { opcode, payload }
    }

    pub fn json(opcode: Opcode, value: &serde_json::Value) -> Result<Self> {
        Ok(Self::new(opcode, serde_json::to_vec(value)?))
    }

    pub fn payload_json(&self) -> Result<serde_json::Value> {
        Ok(serde_json::from_slice(&self.payload)?)
    }

    /// Header followed by the payload, ready to be written in one go.
    pub fn encode(&self) -> Vec<u8> {
        let len = u32::try_from(self.payload.len()).unwrap_or(u32::MAX);
        let mut out = Vec::with_capacity(HEADER_LEN + self.payload.len());
        out.extend_from_slice(&(self.opcode as u32).to_le_bytes());
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&self.payload);
        out
    }

    /// Decodes one frame from the start of `buf`: `Ok(None)` when more bytes are needed,
    /// otherwise the frame and the number of bytes it used.
    pub fn decode(buf: &[u8]) -> Result<Option<(Frame, usize)>> {
        let Some(header) = buf.first_chunk::<HEADER_LEN>() else {
            return Ok(None);
        };
        let (opcode, len) = parse_header(header)?;
        let Some(payload) = buf.get(HEADER_LEN..HEADER_LEN + len) else {
            return Ok(None);
        };
        Ok(Some((
            Frame::new(opcode, payload.to_vec()),
            HEADER_LEN + len,
        )))
    }
}

fn parse_header(header: &[u8; HEADER_LEN]) -> Result<(Opcode, usize)> {
    let [a, b, c, d, e, f, g, h] = *header;
    let raw_opcode = u32::from_le_bytes([a, b, c, d]);
    let len = u32::from_le_bytes([e, f, g, h]) as usize;
    let opcode = Opcode::from_u32(raw_opcode)
        .ok_or_else(|| Error::Frame(format!("unknown opcode {raw_opcode}")))?;
    if len > MAX_PAYLOAD {
        return Err(Error::Frame(format!("payload of {len} bytes is too large")));
    }
    Ok((opcode, len))
}

/// Reads one frame. End of stream becomes [`Error::Disconnected`].
pub async fn read_frame<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Frame> {
    let mut header = [0u8; HEADER_LEN];
    read_exact(reader, &mut header).await?;
    let (opcode, len) = parse_header(&header)?;
    let mut payload = vec![0u8; len];
    read_exact(reader, &mut payload).await?;
    Ok(Frame::new(opcode, payload))
}

async fn read_exact<R: AsyncRead + Unpin>(reader: &mut R, buf: &mut [u8]) -> Result<()> {
    match reader.read_exact(buf).await {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Err(Error::Disconnected),
        Err(e) => Err(e.into()),
    }
}

/// Writes one frame (header and payload in a single write) and flushes.
pub async fn write_frame<W: AsyncWrite + Unpin>(writer: &mut W, frame: &Frame) -> Result<()> {
    writer.write_all(&frame.encode()).await?;
    writer.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn encode_layout() {
        let frame = Frame::new(Opcode::Frame, b"{}".to_vec());
        assert_eq!(frame.encode(), [1, 0, 0, 0, 2, 0, 0, 0, b'{', b'}']);
        let handshake =
            Frame::json(Opcode::Handshake, &json!({"v": 1, "client_id": "42"})).unwrap();
        let bytes = handshake.encode();
        assert_eq!(&bytes[..4], &[0, 0, 0, 0]);
        assert_eq!(
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize,
            bytes.len() - HEADER_LEN
        );
    }

    #[test]
    fn decode_round_trip_and_partial_input() {
        let first = Frame::new(Opcode::Ping, b"{\"a\":1}".to_vec());
        let second = Frame::new(Opcode::Close, b"{}".to_vec());
        let mut bytes = first.encode();
        bytes.extend(second.encode());

        let (decoded, used) = Frame::decode(&bytes).unwrap().unwrap();
        assert_eq!(decoded, first);
        let (decoded, rest) = Frame::decode(&bytes[used..]).unwrap().unwrap();
        assert_eq!(decoded, second);
        assert_eq!(used + rest, bytes.len());

        assert_eq!(Frame::decode(&bytes[..5]).unwrap(), None, "partial header");
        assert_eq!(
            Frame::decode(&bytes[..used - 1]).unwrap(),
            None,
            "partial payload"
        );
        assert_eq!(decoded.payload_json().unwrap(), json!({}));
    }

    #[test]
    fn invalid_headers_are_rejected() {
        let mut unknown = Frame::new(Opcode::Frame, Vec::new()).encode();
        unknown[0] = 9;
        assert!(matches!(Frame::decode(&unknown), Err(Error::Frame(_))));
        let mut huge = Frame::new(Opcode::Frame, Vec::new()).encode();
        huge[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(Frame::decode(&huge), Err(Error::Frame(_))));
    }

    #[tokio::test]
    async fn async_read_write() {
        let (mut client, mut server) = tokio::io::duplex(64);
        let frame = Frame::json(Opcode::Frame, &json!({"cmd": "SET_ACTIVITY"})).unwrap();
        write_frame(&mut client, &frame).await.unwrap();
        assert_eq!(read_frame(&mut server).await.unwrap(), frame);
        drop(client);
        assert!(matches!(
            read_frame(&mut server).await,
            Err(Error::Disconnected)
        ));
    }
}
