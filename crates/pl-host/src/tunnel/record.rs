//! PLJ1 records and frames (PROTOCOL.md §6.3).
//!
//! After the handshake every message is a frame, `u16 length || ciphertext`, where the ciphertext
//! is the Noise transport encryption of one record, `type (1 byte) || body`.

/// Largest record (type byte and body).
pub const MAX_RECORD: usize = 16_384;
/// Largest piece of a game connection in one `data` record.
pub const MAX_DATA: usize = MAX_RECORD - 1 - 8;
/// Body of every `ctrl-part` piece of a control message longer than one record.
pub const CONTROL_PIECE: usize = MAX_RECORD - 1;
/// Largest control message, all pieces joined.
pub const MAX_CONTROL_MESSAGE: usize = 65_536;
/// Smallest frame length (a type byte and the tag).
pub const MIN_FRAME: usize = 1 + super::noise::TAG_LEN;
/// Largest frame length.
pub const MAX_FRAME: usize = MAX_RECORD + super::noise::TAG_LEN;

const CTRL: u8 = 0x01;
const CTRL_PART: u8 = 0x02;
const DATA: u8 = 0x03;
const ACK: u8 = 0x04;
const PING: u8 = 0x05;
const PONG: u8 = 0x06;

/// One record.
#[derive(Clone, PartialEq, Eq)]
pub enum Record {
    /// UTF-8 JSON, the last piece of a control message.
    Ctrl(Vec<u8>),
    /// UTF-8 JSON, a piece that more pieces follow.
    CtrlPart(Vec<u8>),
    /// Bytes of the game connection starting at `offset` in this direction (game streams only).
    Data {
        offset: u64,
        bytes: Vec<u8>,
    },
    /// Bytes of this direction received in order (game streams only).
    Ack(u64),
    Ping([u8; 8]),
    Pong([u8; 8]),
    /// A type this version does not know; skipped.
    Unknown(u8),
}

impl std::fmt::Debug for Record {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Game bytes and control texts stay out of logs; their sizes are enough.
        match self {
            Self::Ctrl(body) => write!(f, "Ctrl({} bytes)", body.len()),
            Self::CtrlPart(body) => write!(f, "CtrlPart({} bytes)", body.len()),
            Self::Data { offset, bytes } => write!(f, "Data({offset}, {} bytes)", bytes.len()),
            Self::Ack(next) => write!(f, "Ack({next})"),
            Self::Ping(_) => f.write_str("Ping"),
            Self::Pong(_) => f.write_str("Pong"),
            Self::Unknown(kind) => write!(f, "Unknown({kind:#04x})"),
        }
    }
}

/// A record that breaks the rules; the stream closes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RecordError {
    #[error("empty record")]
    Empty,
    #[error("record longer than {MAX_RECORD} bytes")]
    TooLong,
    #[error("malformed {0} record")]
    Malformed(&'static str),
    #[error("control message longer than {MAX_CONTROL_MESSAGE} bytes")]
    ControlTooLong,
}

impl Record {
    /// The record's plaintext: type byte and body.
    pub fn encode(&self) -> Vec<u8> {
        let mut out;
        match self {
            Self::Ctrl(body) | Self::CtrlPart(body) => {
                out = Vec::with_capacity(1 + body.len());
                out.push(if matches!(self, Self::Ctrl(_)) {
                    CTRL
                } else {
                    CTRL_PART
                });
                out.extend_from_slice(body);
            }
            Self::Data { offset, bytes } => {
                out = Vec::with_capacity(9 + bytes.len());
                out.push(DATA);
                out.extend_from_slice(&offset.to_be_bytes());
                out.extend_from_slice(bytes);
            }
            Self::Ack(next) => {
                out = Vec::with_capacity(9);
                out.push(ACK);
                out.extend_from_slice(&next.to_be_bytes());
            }
            Self::Ping(body) | Self::Pong(body) => {
                out = Vec::with_capacity(9);
                out.push(if matches!(self, Self::Ping(_)) {
                    PING
                } else {
                    PONG
                });
                out.extend_from_slice(body);
            }
            Self::Unknown(kind) => out = vec![*kind],
        }
        out
    }

    /// Reads a record's plaintext.
    pub fn decode(plain: &[u8]) -> Result<Self, RecordError> {
        let (&kind, body) = plain.split_first().ok_or(RecordError::Empty)?;
        if plain.len() > MAX_RECORD {
            return Err(RecordError::TooLong);
        }
        Ok(match kind {
            CTRL => Self::Ctrl(body.to_vec()),
            CTRL_PART => Self::CtrlPart(body.to_vec()),
            DATA => {
                if body.len() < 9 {
                    return Err(RecordError::Malformed("data"));
                }
                let (offset, bytes) = body.split_at(8);
                Self::Data {
                    offset: u64::from_be_bytes(offset.try_into().unwrap_or_default()),
                    bytes: bytes.to_vec(),
                }
            }
            ACK => Self::Ack(u64::from_be_bytes(
                body.try_into().map_err(|_| RecordError::Malformed("ack"))?,
            )),
            PING => Self::Ping(
                body.try_into()
                    .map_err(|_| RecordError::Malformed("ping"))?,
            ),
            PONG => Self::Pong(
                body.try_into()
                    .map_err(|_| RecordError::Malformed("pong"))?,
            ),
            other => Self::Unknown(other),
        })
    }
}

/// Splits a control message into records: `ctrl-part` pieces of 16,383 bytes and a final `ctrl`.
pub fn split_control(json: &[u8]) -> Result<Vec<Record>, RecordError> {
    if json.len() > MAX_CONTROL_MESSAGE {
        return Err(RecordError::ControlTooLong);
    }
    let mut records = Vec::with_capacity(json.len() / CONTROL_PIECE + 1);
    let mut rest = json;
    while rest.len() > CONTROL_PIECE {
        let (piece, tail) = rest.split_at(CONTROL_PIECE);
        records.push(Record::CtrlPart(piece.to_vec()));
        rest = tail;
    }
    records.push(Record::Ctrl(rest.to_vec()));
    Ok(records)
}

/// Joins `ctrl-part` pieces and the final `ctrl` into one control message.
#[derive(Debug, Default)]
pub struct ControlJoiner {
    buf: Vec<u8>,
}

impl ControlJoiner {
    /// Takes a `ctrl` or `ctrl-part` body; the whole message once its final piece came.
    pub fn push(&mut self, last: bool, body: &[u8]) -> Result<Option<Vec<u8>>, RecordError> {
        if self.buf.len() + body.len() > MAX_CONTROL_MESSAGE {
            self.buf = Vec::new();
            return Err(RecordError::ControlTooLong);
        }
        self.buf.extend_from_slice(body);
        Ok(last.then(|| std::mem::take(&mut self.buf)))
    }
}

/// Wraps a ciphertext into a frame: `u16 length || ciphertext`.
pub fn frame(ciphertext: &[u8]) -> Vec<u8> {
    debug_assert!((MIN_FRAME..=MAX_FRAME).contains(&ciphertext.len()));
    let mut out = Vec::with_capacity(2 + ciphertext.len());
    out.extend_from_slice(&(ciphertext.len() as u16).to_be_bytes());
    out.extend_from_slice(ciphertext);
    out
}

/// Whether a frame's length field is allowed.
pub fn frame_len_ok(len: usize) -> bool {
    (MIN_FRAME..=MAX_FRAME).contains(&len)
}
