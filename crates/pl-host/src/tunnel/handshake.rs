//! The PLJ1 opening on a path's stream (PROTOCOL.md §5.4 step 3, §6.2).

use std::borrow::Cow;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::ids::Slot;
use super::invite::Join;
use super::noise::{Cipher, Initiator, MSG_LEN, OPENING_LEN, parse_opening};
use super::varint;
use crate::transport::{BoxStream, GuestPath};

/// Longest reply that is not msg2 a guest reads (a relay's refusal packet).
pub const MAX_OTHER_REPLY: usize = 4_096;

/// Why a guest's opening did not get it a stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OpenFailure {
    /// The host closed without a word, the reply was not msg2, or msg2 did not authenticate: the
    /// address may belong to someone else now (`plj_auth_failed`).
    AuthFailed,
    /// The path named the reply (e4mc: `e4mc_unknown_domain`, `e4mc_rate_limited`).
    Explained(Cow<'static, str>),
}

impl OpenFailure {
    /// The telemetry code.
    pub fn code(&self) -> Cow<'static, str> {
        match self {
            Self::AuthFailed => Cow::Borrowed("plj_auth_failed"),
            Self::Explained(code) => code.clone(),
        }
    }
}

/// The guest's side: writes the opening for `join`, reads the reply, finishes the handshake.
pub(crate) async fn open(
    stream: &mut BoxStream,
    join: &Join,
    path: &dyn GuestPath,
) -> Result<Cipher, OpenFailure> {
    let (initiator, opening) = Initiator::start(&join.key, &join.psk, &join.slot, &join.session)
        .map_err(|_| OpenFailure::AuthFailed)?;
    finish_open(stream, initiator, &opening, path).await
}

pub(crate) async fn finish_open(
    stream: &mut BoxStream,
    initiator: Initiator,
    opening: &[u8; OPENING_LEN],
    path: &dyn GuestPath,
) -> Result<Cipher, OpenFailure> {
    stream
        .write_all(opening)
        .await
        .map_err(|_| OpenFailure::AuthFailed)?;
    stream.flush().await.map_err(|_| OpenFailure::AuthFailed)?;
    let len = varint::read(stream)
        .await
        .map_err(|_| OpenFailure::AuthFailed)?;
    if usize::try_from(len).ok() == Some(MSG_LEN) {
        let mut msg2 = [0u8; MSG_LEN];
        stream
            .read_exact(&mut msg2)
            .await
            .map_err(|_| OpenFailure::AuthFailed)?;
        return initiator.finish(&msg2).map_err(|_| OpenFailure::AuthFailed);
    }
    let Some(len) = usize::try_from(len)
        .ok()
        .filter(|l| (1..=MAX_OTHER_REPLY).contains(l))
    else {
        return Err(OpenFailure::AuthFailed);
    };
    let mut packet = vec![0u8; len];
    stream
        .read_exact(&mut packet)
        .await
        .map_err(|_| OpenFailure::AuthFailed)?;
    Err(path
        .explain_reply(&packet)
        .map_or(OpenFailure::AuthFailed, OpenFailure::Explained))
}

/// The host's side: reads exactly the 69-byte opening. `None` (wrong length byte, wrong magic, a
/// short read) means close the stream without sending anything.
pub(crate) async fn read_opening(stream: &mut BoxStream) -> Option<(Slot, [u8; MSG_LEN])> {
    let mut opening = [0u8; OPENING_LEN];
    stream.read_exact(&mut opening).await.ok()?;
    parse_opening(&opening).ok()
}
