//! The PLJ1 handshake: `Noise_NKpsk0_25519_ChaChaPoly_BLAKE2s` (PROTOCOL.md §6.1, §6.2).
//!
//! The guest is the initiator and proves it holds its invite's PSK; the host is the responder and
//! proves it holds the session's static key. Both handshake payloads are empty, so the guest's
//! opening and the host's reply have fixed lengths:
//!
//! ```text
//! guest → host:  0x44 || "PLJ1" || slot (16) || msg1 (48)       VarInt(68), then 68 bytes
//! host → guest:  0x30 || msg2 (48)                               VarInt(48), then 48 bytes
//! ```

use snow::params::{DHChoice, NoiseParams};
use snow::resolvers::{CryptoResolver, DefaultResolver};
use snow::{Builder, HandshakeState, StatelessTransportState};
use zeroize::Zeroizing;

use super::ids::{HostKey, Psk, SessionId, Slot};

/// The Noise protocol name.
pub const PROTOCOL_NAME: &str = "Noise_NKpsk0_25519_ChaChaPoly_BLAKE2s";
/// The magic after the opening's length byte, also the prologue's start.
pub const MAGIC: &[u8; 4] = b"PLJ1";
/// Length of each handshake message (an ephemeral key and an empty payload's tag).
pub const MSG_LEN: usize = 48;
/// Bytes after the opening's length byte: magic, slot, msg1.
pub const OPENING_BODY_LEN: usize = 4 + 16 + MSG_LEN;
/// The whole opening: `0x44 || "PLJ1" || slot || msg1`.
pub const OPENING_LEN: usize = 1 + OPENING_BODY_LEN;
/// The whole reply: `0x30 || msg2`.
pub const REPLY_LEN: usize = 1 + MSG_LEN;
/// Noise's transport tag.
pub const TAG_LEN: usize = 16;

pub(crate) fn params() -> NoiseParams {
    PROTOCOL_NAME
        .parse()
        .expect("PLJ1's Noise protocol name parses")
}

/// The X25519 public key of a private key.
pub(crate) fn public_key(secret: &[u8; 32]) -> [u8; 32] {
    let mut dh = DefaultResolver
        .resolve_dh(&DHChoice::Curve25519)
        .expect("the built-in resolver has X25519");
    dh.set(secret);
    let mut public = [0u8; 32];
    public.copy_from_slice(dh.pubkey());
    public
}

/// `"PLJ1" || slot || sid`, 36 bytes.
pub fn prologue(slot: &Slot, session: &SessionId) -> [u8; 36] {
    let mut out = [0u8; 36];
    out[..4].copy_from_slice(MAGIC);
    out[4..20].copy_from_slice(slot.as_bytes());
    out[20..].copy_from_slice(session.as_bytes());
    out
}

/// Why a handshake did not complete. Deliberately coarse: the host answers none of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum HandshakeError {
    /// The opening is not `0x44 || "PLJ1" || …`.
    #[error("not a PLJ1 opening")]
    BadOpening,
    /// The reply is not `0x30 || msg2`.
    #[error("not a PLJ1 reply")]
    BadReply,
    /// A message did not decrypt (wrong PSK, key, slot or session), or carried a payload.
    #[error("handshake failed")]
    Failed,
}

/// The guest's half-done handshake: the opening was made, the reply is awaited.
pub struct Initiator {
    state: HandshakeState,
}

impl Initiator {
    /// Makes the opening for an invite.
    pub fn start(
        host_public: &[u8; 32],
        psk: &Psk,
        slot: &Slot,
        session: &SessionId,
    ) -> Result<(Self, [u8; OPENING_LEN]), HandshakeError> {
        Self::start_with(host_public, psk, slot, session, None)
    }

    pub(crate) fn start_with(
        host_public: &[u8; 32],
        psk: &Psk,
        slot: &Slot,
        session: &SessionId,
        fixed_ephemeral: Option<&[u8; 32]>,
    ) -> Result<(Self, [u8; OPENING_LEN]), HandshakeError> {
        let prologue = prologue(slot, session);
        let mut builder = Builder::new(params())
            .remote_public_key(host_public)
            .and_then(|b| b.psk(0, psk.expose()))
            .and_then(|b| b.prologue(&prologue))
            .map_err(|_| HandshakeError::Failed)?;
        if let Some(key) = fixed_ephemeral {
            builder = builder.fixed_ephemeral_key_for_testing_only(key);
        }
        let mut state = builder
            .build_initiator()
            .map_err(|_| HandshakeError::Failed)?;
        let mut opening = [0u8; OPENING_LEN];
        opening[0] = OPENING_BODY_LEN as u8;
        opening[1..5].copy_from_slice(MAGIC);
        opening[5..21].copy_from_slice(slot.as_bytes());
        let written = state
            .write_message(&[], &mut opening[21..])
            .map_err(|_| HandshakeError::Failed)?;
        debug_assert_eq!(written, MSG_LEN);
        Ok((Self { state }, opening))
    }

    /// Reads the host's msg2 (the reply without its length byte) and enters transport mode.
    pub fn finish(mut self, msg2: &[u8; MSG_LEN]) -> Result<Cipher, HandshakeError> {
        let mut payload = [0u8; MSG_LEN];
        let read = self
            .state
            .read_message(msg2, &mut payload)
            .map_err(|_| HandshakeError::Failed)?;
        if read != 0 {
            return Err(HandshakeError::Failed);
        }
        Cipher::from_handshake(self.state)
    }
}

/// The host's side: checks a guest's msg1 against the slot's PSK and makes the reply.
pub fn respond(
    host: &HostKey,
    psk: &Psk,
    slot: &Slot,
    session: &SessionId,
    msg1: &[u8; MSG_LEN],
) -> Result<(Cipher, [u8; REPLY_LEN]), HandshakeError> {
    respond_with(host, psk, slot, session, msg1, None)
}

pub(crate) fn respond_with(
    host: &HostKey,
    psk: &Psk,
    slot: &Slot,
    session: &SessionId,
    msg1: &[u8; MSG_LEN],
    fixed_ephemeral: Option<&[u8; 32]>,
) -> Result<(Cipher, [u8; REPLY_LEN]), HandshakeError> {
    let prologue = prologue(slot, session);
    let secret = Zeroizing::new(*host.secret());
    let mut builder = Builder::new(params())
        .local_private_key(&secret[..])
        .and_then(|b| b.psk(0, psk.expose()))
        .and_then(|b| b.prologue(&prologue))
        .map_err(|_| HandshakeError::Failed)?;
    if let Some(key) = fixed_ephemeral {
        builder = builder.fixed_ephemeral_key_for_testing_only(key);
    }
    let mut state = builder
        .build_responder()
        .map_err(|_| HandshakeError::Failed)?;
    let mut payload = [0u8; MSG_LEN];
    let read = state
        .read_message(msg1, &mut payload)
        .map_err(|_| HandshakeError::Failed)?;
    if read != 0 {
        // Version 1 payloads are empty; anything else is a protocol error.
        return Err(HandshakeError::Failed);
    }
    let mut reply = [0u8; REPLY_LEN];
    reply[0] = MSG_LEN as u8;
    let written = state
        .write_message(&[], &mut reply[1..])
        .map_err(|_| HandshakeError::Failed)?;
    debug_assert_eq!(written, MSG_LEN);
    Ok((Cipher::from_handshake(state)?, reply))
}

/// Splits a 69-byte opening into its slot and msg1, checking the length byte and the magic.
pub fn parse_opening(opening: &[u8; OPENING_LEN]) -> Result<(Slot, [u8; MSG_LEN]), HandshakeError> {
    if usize::from(opening[0]) != OPENING_BODY_LEN || &opening[1..5] != MAGIC {
        return Err(HandshakeError::BadOpening);
    }
    let mut slot = [0u8; 16];
    slot.copy_from_slice(&opening[5..21]);
    let mut msg1 = [0u8; MSG_LEN];
    msg1.copy_from_slice(&opening[21..]);
    Ok((Slot::from_bytes(slot), msg1))
}

/// Both directions' transport ciphers after the handshake. Nonces count from 0 in each direction;
/// the caller keeps them (the reading and the writing half of a stream run in different tasks).
pub struct Cipher {
    state: StatelessTransportState,
    handshake_hash: [u8; 32],
}

impl Cipher {
    fn from_handshake(state: HandshakeState) -> Result<Self, HandshakeError> {
        let mut handshake_hash = [0u8; 32];
        handshake_hash.copy_from_slice(state.get_handshake_hash());
        let state = state
            .into_stateless_transport_mode()
            .map_err(|_| HandshakeError::Failed)?;
        Ok(Self {
            state,
            handshake_hash,
        })
    }

    /// The handshake hash (the same on both sides).
    pub fn handshake_hash(&self) -> &[u8; 32] {
        &self.handshake_hash
    }

    /// Encrypts one record for this side's direction with `nonce`.
    pub fn seal(&self, nonce: u64, plaintext: &[u8]) -> Option<Vec<u8>> {
        let mut out = vec![0u8; plaintext.len() + TAG_LEN];
        let written = self.state.write_message(nonce, plaintext, &mut out).ok()?;
        out.truncate(written);
        Some(out)
    }

    /// Decrypts one record of the other side's direction with `nonce`; `None` if it does not
    /// authenticate.
    pub fn open(&self, nonce: u64, ciphertext: &[u8]) -> Option<Vec<u8>> {
        let mut out = vec![0u8; ciphertext.len()];
        let read = self.state.read_message(nonce, ciphertext, &mut out).ok()?;
        out.truncate(read);
        Some(out)
    }
}

impl std::fmt::Debug for Cipher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Cipher(<redacted>)")
    }
}
