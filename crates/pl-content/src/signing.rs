//! Ed25519 signing of the root index.
//!
//! `index.json.sig` is a small JSON document `{ "keyId": "...", "signature": "..." }`. The
//! signature is Ed25519 over the exact bytes of `index.json`, base64 encoded (standard alphabet,
//! padded). `keyId` is the lowercase hex of the first 8 bytes of SHA-256(public key) and lets
//! the launcher trust several keys at once (key rotation).
//!
//! Keys travel as plain base64 of their 32 raw bytes (no PEM). Secret keys live in the OS
//! keychain (Studio); they are wrapped in [`Zeroizing`] and never logged.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ed25519_dalek::{Signature, Signer};
use rand::TryRng;
use serde::{Deserialize, Serialize};
pub use zeroize::Zeroizing;

pub use ed25519_dalek::{SigningKey, VerifyingKey};

use crate::error::{Error, Result};
use crate::fsutil::sha256_hex;

/// A public key launchers trust to sign the content index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrustedKey {
    /// [`key_id`] of the key.
    pub id: &'static str,
    /// Base64 of its 32 raw bytes ([`encode_verifying_key`]).
    pub public_key: &'static str,
}

/// The keys released launchers trust to sign the content index, primary first. This is the one
/// list: the launcher verifies with it and Studio tells whether its own key is in it. Rotation:
/// add the new key here, ship a launcher, sign with the new key, later drop the old one. The
/// Android app (separate repository) keeps its own copy of this list.
pub const CONTENT_PUBLIC_KEYS: &[TrustedKey] = &[
    // Primary, generated 2026-10-15.
    TrustedKey {
        id: "5125c0d365b019ce",
        public_key: "muj3CSu3m9jpB7fUP5TbkRSnISGEH13XbTt6agmIK68=",
    },
    // Offline backup, generated 2026-10-16: not used for everyday publishing.
    TrustedKey {
        id: "b63fd046213136eb",
        public_key: "qTmm+6djdFYCmoHKop6DuvVMgUsHk1EzdZZ9wDhXdOM=",
    },
];

/// [`CONTENT_PUBLIC_KEYS`] decoded, in order (a test keeps every entry decodable).
pub fn content_public_keys() -> Vec<VerifyingKey> {
    CONTENT_PUBLIC_KEYS
        .iter()
        .filter_map(|key| decode_verifying_key(key.public_key).ok())
        .collect()
}

/// Whether `key` is one of [`CONTENT_PUBLIC_KEYS`].
pub fn is_content_public_key(key: &VerifyingKey) -> bool {
    let encoded = encode_verifying_key(key);
    CONTENT_PUBLIC_KEYS
        .iter()
        .any(|trusted| trusted.public_key == encoded)
}

/// Contents of `index.json.sig`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SignatureFile {
    /// [`key_id`] of the signing key.
    pub key_id: String,
    /// Base64 Ed25519 signature over the exact bytes of `index.json`.
    pub signature: String,
}

/// Generates a new signing key from the operating system's random generator.
pub fn generate_signing_key() -> Result<SigningKey> {
    let mut secret = Zeroizing::new([0u8; ed25519_dalek::SECRET_KEY_LENGTH]);
    rand::rngs::SysRng
        .try_fill_bytes(secret.as_mut())
        .map_err(|e| Error::InvalidKey(format!("system random generator failed: {e}")))?;
    Ok(SigningKey::from_bytes(&secret))
}

/// Key identifier: lowercase hex of the first 8 bytes of SHA-256(public key bytes).
pub fn key_id(key: &VerifyingKey) -> String {
    let mut digest = sha256_hex(key.as_bytes());
    digest.truncate(16);
    digest
}

/// Signs the exact bytes of `index.json`.
pub fn sign_index(bytes: &[u8], key: &SigningKey) -> SignatureFile {
    let signature: Signature = key.sign(bytes);
    SignatureFile {
        key_id: key_id(&key.verifying_key()),
        signature: STANDARD.encode(signature.to_bytes()),
    }
}

/// Verifies `bytes` against `signature` using the trusted key whose id matches `keyId`.
/// Returns the key that verified the index.
///
/// Fails with [`Error::UntrustedKey`] when no trusted key has that id and with
/// [`Error::BadSignature`] when the signature does not verify (strict Ed25519 verification).
pub fn verify_index(
    bytes: &[u8],
    signature: &SignatureFile,
    trusted: &[VerifyingKey],
) -> Result<VerifyingKey> {
    let wanted = signature.key_id.trim().to_ascii_lowercase();
    let key = trusted
        .iter()
        .find(|key| key_id(key) == wanted)
        .ok_or_else(|| Error::UntrustedKey {
            key_id: signature.key_id.clone(),
        })?;
    let raw = STANDARD
        .decode(signature.signature.trim())
        .map_err(|_| Error::BadSignature)?;
    let signature = Signature::from_slice(&raw).map_err(|_| Error::BadSignature)?;
    key.verify_strict(bytes, &signature)
        .map_err(|_| Error::BadSignature)?;
    Ok(*key)
}

/// Encodes a secret key as base64 of its 32 bytes, for storage in the OS keychain.
pub fn encode_signing_key(key: &SigningKey) -> Zeroizing<String> {
    Zeroizing::new(STANDARD.encode(key.as_bytes()))
}

/// Decodes a secret key produced by [`encode_signing_key`].
pub fn decode_signing_key(text: &str) -> Result<SigningKey> {
    let raw = Zeroizing::new(
        STANDARD
            .decode(text.trim())
            .map_err(|_| Error::InvalidKey("the signing key is not valid base64".into()))?,
    );
    let secret: &[u8; ed25519_dalek::SECRET_KEY_LENGTH] = raw
        .as_slice()
        .try_into()
        .map_err(|_| Error::InvalidKey("the signing key must be 32 bytes".into()))?;
    Ok(SigningKey::from_bytes(secret))
}

/// Encodes a public key as base64 of its 32 bytes (embedded in the launcher).
pub fn encode_verifying_key(key: &VerifyingKey) -> String {
    STANDARD.encode(key.as_bytes())
}

/// Decodes a public key produced by [`encode_verifying_key`].
pub fn decode_verifying_key(text: &str) -> Result<VerifyingKey> {
    let raw = STANDARD
        .decode(text.trim())
        .map_err(|_| Error::InvalidKey("the public key is not valid base64".into()))?;
    let bytes: &[u8; ed25519_dalek::PUBLIC_KEY_LENGTH] = raw
        .as_slice()
        .try_into()
        .map_err(|_| Error::InvalidKey("the public key must be 32 bytes".into()))?;
    VerifyingKey::from_bytes(bytes)
        .map_err(|_| Error::InvalidKey("the public key is not a valid Ed25519 point".into()))
}

/// Decodes a list of embedded public keys (e.g. the launcher's trusted keys).
pub fn decode_verifying_keys<S: AsRef<str>>(texts: &[S]) -> Result<Vec<VerifyingKey>> {
    texts
        .iter()
        .map(|text| decode_verifying_key(text.as_ref()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed_key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    #[test]
    fn sign_and_verify_round_trip() {
        let key = generate_signing_key().unwrap();
        let bytes = br#"{"schema":1,"sequence":7}"#;
        let sig = sign_index(bytes, &key);
        assert_eq!(sig.key_id.len(), 16);
        let used = verify_index(bytes, &sig, &[key.verifying_key()]).unwrap();
        assert_eq!(used, key.verifying_key());
    }

    #[test]
    fn tampered_bytes_are_rejected() {
        let key = fixed_key(1);
        let sig = sign_index(b"original", &key);
        let err = verify_index(b"originaL", &sig, &[key.verifying_key()]).unwrap_err();
        assert!(matches!(err, Error::BadSignature), "{err}");

        let mut broken = sig.clone();
        broken.signature = STANDARD.encode([0u8; 64]);
        assert!(matches!(
            verify_index(b"original", &broken, &[key.verifying_key()]),
            Err(Error::BadSignature)
        ));
        broken.signature = "not base64!".into();
        assert!(matches!(
            verify_index(b"original", &broken, &[key.verifying_key()]),
            Err(Error::BadSignature)
        ));
    }

    #[test]
    fn unknown_key_is_rejected() {
        let signer = fixed_key(1);
        let other = fixed_key(2);
        let sig = sign_index(b"index", &signer);
        let err = verify_index(b"index", &sig, &[other.verifying_key()]).unwrap_err();
        assert!(matches!(err, Error::UntrustedKey { .. }), "{err}");
        assert!(matches!(
            verify_index(b"index", &sig, &[]),
            Err(Error::UntrustedKey { .. })
        ));
    }

    #[test]
    fn key_id_must_match_the_signing_key() {
        // A signature made by `a` but labelled with `b`'s id must not verify under `b`.
        let a = fixed_key(1);
        let b = fixed_key(2);
        let mut sig = sign_index(b"index", &a);
        sig.key_id = key_id(&b.verifying_key());
        assert!(matches!(
            verify_index(b"index", &sig, &[a.verifying_key(), b.verifying_key()]),
            Err(Error::BadSignature)
        ));
    }

    #[test]
    fn rotation_accepts_any_trusted_key() {
        let old = fixed_key(1);
        let new = fixed_key(2);
        let trusted = [old.verifying_key(), new.verifying_key()];
        for key in [&old, &new] {
            let sig = sign_index(b"index", key);
            assert_eq!(
                verify_index(b"index", &sig, &trusted).unwrap(),
                key.verifying_key()
            );
        }
    }

    #[test]
    fn key_encoding_round_trips() {
        let key = generate_signing_key().unwrap();
        let text = encode_signing_key(&key);
        assert_eq!(
            decode_signing_key(&text).unwrap().as_bytes(),
            key.as_bytes()
        );

        let public = encode_verifying_key(&key.verifying_key());
        assert_eq!(decode_verifying_key(&public).unwrap(), key.verifying_key());
        assert_eq!(decode_verifying_keys(&[public]).unwrap().len(), 1);

        assert!(matches!(
            decode_signing_key("%%%"),
            Err(Error::InvalidKey(_))
        ));
        assert!(matches!(
            decode_signing_key(&STANDARD.encode([1u8; 31])),
            Err(Error::InvalidKey(_))
        ));
        assert!(matches!(
            decode_verifying_key(&STANDARD.encode([1u8; 33])),
            Err(Error::InvalidKey(_))
        ));
    }

    #[test]
    fn key_id_is_stable() {
        let key = fixed_key(7).verifying_key();
        assert_eq!(key_id(&key), key_id(&key));
        assert_eq!(key_id(&key), sha256_hex(key.as_bytes())[..16]);
        assert!(key_id(&key).bytes().all(|b| b.is_ascii_hexdigit()));
    }

    #[test]
    fn content_public_keys_decode_to_their_ids() {
        let ids: Vec<&str> = CONTENT_PUBLIC_KEYS.iter().map(|key| key.id).collect();
        assert_eq!(
            ids,
            ["5125c0d365b019ce", "b63fd046213136eb"],
            "primary first"
        );
        let keys = content_public_keys();
        assert_eq!(keys.len(), CONTENT_PUBLIC_KEYS.len(), "every key decodes");
        for (key, trusted) in keys.iter().zip(CONTENT_PUBLIC_KEYS) {
            assert_eq!(key_id(key), trusted.id);
            assert_eq!(encode_verifying_key(key), trusted.public_key);
            assert!(is_content_public_key(key));
        }
        assert!(!is_content_public_key(&fixed_key(5).verifying_key()));
    }

    #[test]
    fn signature_file_json_shape() {
        let sig = sign_index(b"x", &fixed_key(3));
        let json = serde_json::to_value(&sig).unwrap();
        assert!(json.get("keyId").is_some() && json.get("signature").is_some());
        let parsed: SignatureFile =
            serde_json::from_str(r#"{"keyId":"abc","signature":"def","future":1}"#).unwrap();
        assert_eq!(parsed.key_id, "abc");
    }
}
