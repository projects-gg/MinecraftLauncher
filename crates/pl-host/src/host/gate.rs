//! PLG1: the identity header the host launcher writes first on every local connection to the world
//! (PROTOCOL.md §7.1, §7.2). The overlay mod checks it before the game reads a byte.
//!
//! ```text
//! "PLG1" (4) || u16 length || JSON (length bytes, at most 1,024) || HMAC-SHA256(gate key, all bytes before) (32)
//! ```

use std::fmt;

use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use zeroize::Zeroizing;

use crate::tunnel::GuestId;

/// The header's magic.
pub const MAGIC: &[u8; 4] = b"PLG1";
/// Longest JSON part.
pub const MAX_JSON: usize = 1_024;
/// The MAC's length.
pub const MAC_LEN: usize = 32;
const GATE_LABEL: &[u8] = b"projects host gate v1";

type HmacSha256 = Hmac<Sha256>;

/// The key both the launcher and the mod derive from the bridge token: HMAC-SHA256(key = the token's
/// 43 ASCII bytes, message = ASCII `projects host gate v1`). No new secret travels.
#[derive(Clone)]
pub struct GateKey(Zeroizing<[u8; 32]>);

impl GateKey {
    /// Derives the gate key from the bridge token (as the game got it, 43 characters).
    pub fn from_bridge_token(token: &str) -> Self {
        let mut mac =
            HmacSha256::new_from_slice(token.as_bytes()).expect("HMAC takes any key length");
        mac.update(GATE_LABEL);
        let mut key = Zeroizing::new([0u8; 32]);
        key.copy_from_slice(&mac.finalize().into_bytes());
        Self(key)
    }

    #[cfg(test)]
    pub(crate) fn expose(&self) -> &[u8; 32] {
        &self.0
    }

    fn mac(&self) -> HmacSha256 {
        HmacSha256::new_from_slice(&self.0[..]).expect("HMAC takes any key length")
    }
}

impl fmt::Debug for GateKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("GateKey(<redacted>)")
    }
}

/// Who a local connection carries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    /// 1.
    pub v: u32,
    /// The publish id of `host-publish`.
    pub id: String,
    /// Unique within that publish id, from 1.
    pub conn: u64,
    /// The guest's id in the session.
    pub guest: GuestId,
    /// The in-game name from the game `hello`.
    pub name: String,
    /// The game authenticates with Mojang for this connection.
    pub premium: bool,
    /// The friend's binding digest.
    pub friend: String,
}

/// A header that does not pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum GateError {
    #[error("not a PLG1 header")]
    Magic,
    #[error("header length out of range")]
    Length,
    #[error("header MAC does not match")]
    Mac,
    #[error("header JSON is malformed")]
    Json,
}

/// The header for one local connection.
pub fn header(key: &GateKey, identity: &Identity) -> Vec<u8> {
    let json = serde_json::to_vec(identity).expect("identity serializes");
    assert!(
        json.len() <= MAX_JSON,
        "PLG1 identity longer than {MAX_JSON} bytes"
    );
    let mut out = Vec::with_capacity(4 + 2 + json.len() + MAC_LEN);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(json.len() as u16).to_be_bytes());
    out.extend_from_slice(&json);
    let mut mac = key.mac();
    mac.update(&out);
    out.extend_from_slice(&mac.finalize().into_bytes());
    out
}

/// Checks a header the way the mod does (magic, length, MAC in constant time, then the JSON) and
/// returns the identity and the header's length. For tests and diagnostics; the gate itself is in
/// the mod.
pub fn verify(key: &GateKey, bytes: &[u8]) -> Result<(Identity, usize), GateError> {
    if bytes.len() < 6 || &bytes[..4] != MAGIC {
        return Err(GateError::Magic);
    }
    let len = usize::from(u16::from_be_bytes([bytes[4], bytes[5]]));
    if len > MAX_JSON || bytes.len() < 6 + len + MAC_LEN {
        return Err(GateError::Length);
    }
    let (signed, rest) = bytes.split_at(6 + len);
    let mut mac = key.mac();
    mac.update(signed);
    mac.verify_slice(&rest[..MAC_LEN])
        .map_err(|_| GateError::Mac)?;
    let identity: Identity = serde_json::from_slice(&signed[6..]).map_err(|_| GateError::Json)?;
    if identity.v != 1 {
        return Err(GateError::Json);
    }
    Ok((identity, 6 + len + MAC_LEN))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdata;

    #[test]
    fn vectors() {
        let file = testdata::json("gate/plg1.json");
        let key = GateKey::from_bridge_token(file["bridgeToken"].as_str().unwrap());
        assert_eq!(
            hex::encode(key.expose()),
            file["gateKeyHex"].as_str().unwrap()
        );
        for case in file["valid"].as_array().unwrap() {
            let identity: Identity = serde_json::from_value(case["fields"].clone()).unwrap();
            assert_eq!(
                hex::encode(serde_json::to_vec(&identity).unwrap()),
                case["jsonHex"].as_str().unwrap()
            );
            let bytes = header(&key, &identity);
            assert_eq!(hex::encode(&bytes), case["headerHex"].as_str().unwrap());
            assert_eq!(
                hex::encode(&bytes[bytes.len() - MAC_LEN..]),
                case["macHex"].as_str().unwrap()
            );
            assert_eq!(verify(&key, &bytes), Ok((identity, bytes.len())));
        }
        for case in file["invalid"].as_array().unwrap() {
            let bytes = hex::decode(case["headerHex"].as_str().unwrap()).unwrap();
            assert!(verify(&key, &bytes).is_err(), "{}", case["case"]);
        }
    }
}
