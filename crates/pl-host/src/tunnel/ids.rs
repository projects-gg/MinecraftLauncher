//! Identifiers and secrets of a world session (PROTOCOL.md §2).
//!
//! Every random value comes from the operating system's CSPRNG. Secrets (the PSK, the host session's
//! private key) are wiped from memory when dropped and never print in `Debug`. Slots are not secret,
//! but they never reach a log either (§13), so their `Debug` is redacted too.

use std::fmt;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use zeroize::{Zeroize, Zeroizing};

/// `N` bytes from the operating system's CSPRNG.
pub(crate) fn random<const N: usize>() -> [u8; N] {
    let mut out = [0u8; N];
    // Windows' ProcessPrng and the other platforms' sources do not fail once the process runs.
    getrandom::fill(&mut out).expect("the operating system's random generator is unavailable");
    out
}

/// base64url without padding.
pub(crate) fn b64u(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Decodes exactly `N` bytes of canonical base64url without padding (the length must be the one `N`
/// bytes encode to, and unused trailing bits must be zero).
pub(crate) fn from_b64u<const N: usize>(text: &str) -> Option<[u8; N]> {
    if text.len() != (N * 4).div_ceil(3) {
        return None;
    }
    let bytes = URL_SAFE_NO_PAD.decode(text).ok()?;
    bytes.try_into().ok()
}

macro_rules! b64u_serde {
    ($ty:ident, $len:expr) => {
        impl Serialize for $ty {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&b64u(&self.0[..]))
            }
        }

        impl<'de> Deserialize<'de> for $ty {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let text = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
                from_b64u::<$len>(&text)
                    .map(|bytes| Self(bytes.into()))
                    .ok_or_else(|| serde::de::Error::custom(concat!("not a ", stringify!($ty))))
            }
        }
    };
}

/// The world session's id: 16 random bytes, base64url (22 characters). Travels in service messages,
/// the invite, the Noise prologue and the TURN label.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionId([u8; 16]);

impl SessionId {
    pub fn generate() -> Self {
        Self(random())
    }

    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Reads the 22-character form.
    pub fn parse(text: &str) -> Option<Self> {
        from_b64u(text).map(Self)
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&b64u(&self.0))
    }
}

impl fmt::Debug for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SessionId({self})")
    }
}

b64u_serde!(SessionId, 16);

/// One invitee's place in a session: 16 random bytes, base64url (22 characters). Sent in the clear
/// in the PLJ1 opening, so not a secret; it still never reaches a log (PROTOCOL.md §13).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Slot([u8; 16]);

impl Slot {
    pub fn generate() -> Self {
        Self(random())
    }

    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Reads the 22-character form (`world_peer.slot`).
    pub fn parse(text: &str) -> Option<Self> {
        from_b64u(text).map(Self)
    }

    /// The 22-character form, for service messages only.
    pub fn to_b64u(&self) -> String {
        b64u(&self.0)
    }
}

impl fmt::Debug for Slot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Slot(<redacted>)")
    }
}

b64u_serde!(Slot, 16);

/// An invitee's pre-shared key: 32 random bytes, base64url (43 characters). Travels only inside the
/// invite; wiped from memory when dropped.
#[derive(Clone, PartialEq, Eq)]
pub struct Psk(Zeroizing<[u8; 32]>);

impl Psk {
    pub fn generate() -> Self {
        Self(Zeroizing::new(random()))
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    pub(crate) fn expose(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for Psk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

b64u_serde!(Psk, 32);

/// A PLJ1 stream's id: 8 random bytes, base64url (11 characters). Names a game stream in `resume`.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct StreamId([u8; 8]);

impl StreamId {
    pub fn generate() -> Self {
        Self(random())
    }

    pub fn from_bytes(bytes: [u8; 8]) -> Self {
        Self(bytes)
    }

    pub fn parse(text: &str) -> Option<Self> {
        from_b64u(text).map(Self)
    }
}

impl fmt::Display for StreamId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&b64u(&self.0))
    }
}

impl fmt::Debug for StreamId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "StreamId({self})")
    }
}

b64u_serde!(StreamId, 8);

/// A guest's id within a session, `g1` … `g999`, given per slot in order.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GuestId(u16);

impl GuestId {
    /// The largest number a guest id takes.
    pub const MAX: u16 = 999;

    /// `n` in 1..=999.
    pub fn new(n: u16) -> Option<Self> {
        (1..=Self::MAX).contains(&n).then_some(Self(n))
    }

    pub fn number(self) -> u16 {
        self.0
    }

    /// Reads `g1` … `g999` (no leading zeros).
    pub fn parse(text: &str) -> Option<Self> {
        let digits = text.strip_prefix('g')?;
        if digits.is_empty() || digits.len() > 3 || digits.starts_with('0') {
            return None;
        }
        if !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        Self::new(digits.parse().ok()?)
    }
}

impl fmt::Display for GuestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "g{}", self.0)
    }
}

impl fmt::Debug for GuestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self}")
    }
}

impl Serialize for GuestId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for GuestId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
        Self::parse(&text).ok_or_else(|| serde::de::Error::custom("not a guest id"))
    }
}

/// The host session's X25519 key pair (Noise static key). The public half goes into every invite;
/// the private half lives in memory only and is wiped when dropped.
#[derive(Clone)]
pub struct HostKey {
    secret: Zeroizing<[u8; 32]>,
    public: [u8; 32],
}

impl HostKey {
    /// A fresh key pair for one session.
    pub fn generate() -> Self {
        let builder = snow::Builder::new(super::noise::params());
        let mut pair = builder
            .generate_keypair()
            .expect("X25519 key generation with the built-in resolver");
        let mut secret = Zeroizing::new([0u8; 32]);
        secret.copy_from_slice(&pair.private);
        pair.private.zeroize();
        let mut public = [0u8; 32];
        public.copy_from_slice(&pair.public);
        Self { secret, public }
    }

    /// A key pair from its private half (tests and vectors).
    pub fn from_secret(secret: [u8; 32]) -> Self {
        let secret = Zeroizing::new(secret);
        let public = super::noise::public_key(&secret);
        Self { secret, public }
    }

    pub fn public(&self) -> &[u8; 32] {
        &self.public
    }

    pub(crate) fn secret(&self) -> &[u8; 32] {
        &self.secret
    }
}

impl fmt::Debug for HostKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostKey")
            .field("public", &b64u(&self.public))
            .field("secret", &"<redacted>")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64url_lengths_are_exact() {
        assert_eq!(SessionId::generate().to_string().len(), 22);
        assert_eq!(Slot::generate().to_b64u().len(), 22);
        assert_eq!(StreamId::generate().to_string().len(), 11);
        assert_eq!(b64u(Psk::generate().expose()).len(), 43);
        let sid = SessionId::parse("stuiX9ATk0i2-2_h8YikXQ").unwrap();
        assert_eq!(sid.to_string(), "stuiX9ATk0i2-2_h8YikXQ");
        // One character short, one too many, padding, and non-zero trailing bits are refused.
        assert!(SessionId::parse("stuiX9ATk0i2-2_h8YikX").is_none());
        assert!(SessionId::parse("stuiX9ATk0i2-2_h8YikXQA").is_none());
        assert!(SessionId::parse("stuiX9ATk0i2-2_h8YikX==").is_none());
        assert!(SessionId::parse("stuiX9ATk0i2-2_h8YikXR").is_none());
        assert!(SessionId::parse("stuiX9ATk0i2+2/h8YikXQ").is_none());
    }

    #[test]
    fn guest_ids() {
        assert_eq!(GuestId::new(1).unwrap().to_string(), "g1");
        assert_eq!(GuestId::parse("g999"), GuestId::new(999));
        for bad in ["g0", "g1000", "g01", "g", "1", "G1", "g1a", "g-1"] {
            assert!(GuestId::parse(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn secrets_never_print() {
        let psk = Psk::generate();
        assert_eq!(format!("{psk:?}"), "<redacted>");
        assert_eq!(format!("{:?}", Slot::generate()), "Slot(<redacted>)");
        let key = HostKey::generate();
        let shown = format!("{key:?}");
        assert!(shown.contains("<redacted>"));
        assert!(!shown.contains(&b64u(key.secret())));
    }

    #[test]
    fn host_key_public_half_matches_the_vector() {
        let secret: [u8; 32] =
            hex::decode("c1a00503b720ba6e7154c31d593c8ac9588e5b5cffdb0993967faa596221a8dc")
                .unwrap()
                .try_into()
                .unwrap();
        let key = HostKey::from_secret(secret);
        assert_eq!(
            b64u(key.public()),
            "nDMeBTfWvxUS50h4YF02xUXMd_V91FnFw1aW9ackAXc"
        );
    }
}
