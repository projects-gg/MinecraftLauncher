//! Secret rows are sealed here, in the launcher, before they leave it (the contract's "Seal format
//! `pl1`"), and a stored row the player asks to see comes back sealed to a key made for that one
//! request ("Reveal format `rv1`"). Only the cloud AFK machines can open a seal; the service,
//! Studio and staff never see the words.
//!
//! ```text
//! sealed   = "pl1." kid "." base64url(enc || ct)      aad = owner|account|row|host:port
//! revealed = "rv1." base64url(enc || ct)              aad = owner|account|row|nonce
//! padded   = UTF-8(text) || 0x00 * n   (a multiple of 64 bytes, 64..=768)
//! ```
//!
//! HPKE (RFC 9180) base mode, single shot: DHKEM(P-256, HKDF-SHA256), HKDF-SHA256, AES-128-GCM.
//! The AAD ties a seal to the player, the account, the row and the server: copied anywhere else it
//! does not open, which is why a server change needs every secret sealed again.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hpke::{Deserializable, Kem as _, OpModeR, OpModeS, Serializable};
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, Zeroizing};

use crate::model::{CloudServer, VaultKey};

type Kem = hpke::kem::DhP256HkdfSha256;
type Kdf = hpke::kdf::HkdfSha256;
type Aead = hpke::aead::AesGcm128;

/// The suite `GET /vault-key` names; the launcher seals with nothing else.
pub const SEAL_ALG: &str = "HPKE-P256-SHA256-AES128GCM";
/// The row id of a line typed into the console.
pub const CONSOLE_ROW: &str = "console";

const SEAL_PREFIX: &str = "pl1";
const REVEAL_PREFIX: &str = "rv1";
const SEAL_INFO: &[u8] = b"projects-cloud-afk seal v1";
const REVEAL_INFO: &[u8] = b"projects-cloud-afk reveal v1";
const PAD_BLOCK: usize = 64;
const MAX_PADDED: usize = 768;
/// An uncompressed P-256 point.
const POINT_BYTES: usize = 65;
const TAG_BYTES: usize = 16;

/// Why a value could not be sealed or opened (the test vectors' error classes for opening).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SealError {
    /// The vault key is not the contract's suite, or not a P-256 point.
    #[error("unusable vault key")]
    Key,
    /// The text is empty or longer than a seal holds.
    #[error("text does not fit a seal")]
    Length,
    /// Not the format's shape: prefix, kid, canonical base64url, length.
    #[error("malformed value")]
    Format,
    /// Sealed to a key this opener does not have.
    #[error("unknown key id")]
    UnknownKid,
    /// The key exchange or the AEAD refused it (another key, another AAD, tampering).
    #[error("does not open")]
    OpenFailed,
    /// Opened, but the words are not a text.
    #[error("not a text")]
    Plaintext,
}

/// Who and where a seal is for: its AAD.
#[derive(Debug, Clone, Copy)]
pub struct SealTarget<'a> {
    /// The owner's Discord id.
    pub discord_id: &'a str,
    pub account_id: &'a str,
    /// A command row or rule id, or [`CONSOLE_ROW`].
    pub row_id: &'a str,
    /// The account's server exactly as it is (or will be) stored.
    pub server: &'a CloudServer,
}

impl SealTarget<'_> {
    pub fn aad(&self) -> String {
        format!(
            "{}|{}|{}|{}:{}",
            self.discord_id, self.account_id, self.row_id, self.server.host, self.server.port
        )
    }
}

/// The text's UTF-8 bytes, zero-padded to the next multiple of 64 (at least 64, at most 768).
fn pad(text: &str) -> Result<Zeroizing<Vec<u8>>, SealError> {
    let bytes = text.as_bytes();
    let size = bytes.len().div_ceil(PAD_BLOCK).max(1) * PAD_BLOCK;
    if bytes.is_empty() || size > MAX_PADDED || bytes.contains(&0) {
        return Err(SealError::Length);
    }
    let mut padded = Zeroizing::new(vec![0u8; size]);
    padded[..bytes.len()].copy_from_slice(bytes);
    Ok(padded)
}

fn is_kid(kid: &str) -> bool {
    (1..=12).contains(&kid.len())
        && kid
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

fn public_key(encoded: &str) -> Result<<Kem as hpke::Kem>::PublicKey, SealError> {
    let raw = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| SealError::Key)?;
    if raw.len() != POINT_BYTES || raw[0] != 0x04 {
        return Err(SealError::Key);
    }
    <Kem as hpke::Kem>::PublicKey::from_bytes(&raw).map_err(|_| SealError::Key)
}

/// Seals `text` (it must already pass [`crate::check_text`]) to the vault key for `target`, with
/// a fresh ephemeral key.
pub fn seal(key: &VaultKey, text: &str, target: &SealTarget<'_>) -> Result<String, SealError> {
    seal_with(key, text, target, &mut hpke::rand_core::UnwrapErr(OsRandom))
}

/// The operating system's randomness (getrandom, through hpke's own `rand_core`).
struct OsRandom;

impl hpke::rand_core::TryRng for OsRandom {
    type Error = SealError;

    fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
        let mut bytes = [0u8; 4];
        self.try_fill_bytes(&mut bytes)?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
        let mut bytes = [0u8; 8];
        self.try_fill_bytes(&mut bytes)?;
        Ok(u64::from_le_bytes(bytes))
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), Self::Error> {
        rand::fill(dest);
        Ok(())
    }
}

impl hpke::rand_core::TryCryptoRng for OsRandom {}

fn seal_with(
    key: &VaultKey,
    text: &str,
    target: &SealTarget<'_>,
    rng: &mut impl hpke::rand_core::CryptoRng,
) -> Result<String, SealError> {
    if key.alg != SEAL_ALG || !is_kid(&key.kid) {
        return Err(SealError::Key);
    }
    let recipient = public_key(&key.public_key)?;
    let padded = pad(text)?;
    let (enc, ct) = hpke::single_shot_seal_with_rng::<Aead, Kdf, Kem>(
        &OpModeS::Base,
        &recipient,
        SEAL_INFO,
        &padded,
        target.aad().as_bytes(),
        rng,
    )
    .map_err(|_| SealError::Key)?;
    let mut body = enc.to_bytes().to_vec();
    body.extend_from_slice(&ct);
    Ok(format!(
        "{SEAL_PREFIX}.{}.{}",
        key.kid,
        URL_SAFE_NO_PAD.encode(body)
    ))
}

/// The stored seal's fingerprint as the account view gives it (`fp`): the first 16 characters of
/// base64url(SHA-256(the sealed value)). It says nothing about the words.
pub fn fingerprint(sealed: &str) -> String {
    let digest = Sha256::digest(sealed.as_bytes());
    let mut encoded = URL_SAFE_NO_PAD.encode(digest);
    encoded.truncate(16);
    encoded
}

/// `enc || ct` of a canonical base64url payload of `65 + 16 + 64·k` bytes, `1 <= k <= 12`.
fn split_payload(payload: &str) -> Result<(Vec<u8>, Vec<u8>), SealError> {
    let body = URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|_| SealError::Format)?;
    if URL_SAFE_NO_PAD.encode(&body) != payload {
        return Err(SealError::Format);
    }
    let Some(blocks) = body.len().checked_sub(POINT_BYTES + TAG_BYTES) else {
        return Err(SealError::Format);
    };
    if blocks == 0 || blocks % PAD_BLOCK != 0 || blocks > MAX_PADDED {
        return Err(SealError::Format);
    }
    let (enc, ct) = body.split_at(POINT_BYTES);
    Ok((enc.to_vec(), ct.to_vec()))
}

/// Opens `enc || ct` with `secret` and strips the padding.
fn open_body(
    secret: &<Kem as hpke::Kem>::PrivateKey,
    enc: &[u8],
    ct: &[u8],
    info: &[u8],
    aad: &[u8],
) -> Result<Zeroizing<Vec<u8>>, SealError> {
    let encapped =
        <Kem as hpke::Kem>::EncappedKey::from_bytes(enc).map_err(|_| SealError::OpenFailed)?;
    let mut padded = Zeroizing::new(
        hpke::single_shot_open::<Aead, Kdf, Kem>(&OpModeR::Base, secret, &encapped, info, ct, aad)
            .map_err(|_| SealError::OpenFailed)?,
    );
    let end = padded.iter().rposition(|&b| b != 0).map_or(0, |at| at + 1);
    padded.truncate(end);
    Ok(padded)
}

/// A key pair made for one "Göster": its public half goes with the request, the private half
/// opens the answer once and is dropped (it is wiped when dropped).
pub struct RevealKey {
    secret: <Kem as hpke::Kem>::PrivateKey,
    public: String,
}

impl std::fmt::Debug for RevealKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RevealKey")
            .field("public", &self.public)
            .finish_non_exhaustive()
    }
}

/// Who and which request an answer is for: its AAD.
#[derive(Debug, Clone, Copy)]
pub struct RevealTarget<'a> {
    pub discord_id: &'a str,
    pub account_id: &'a str,
    pub row_id: &'a str,
    /// The service's nonce for this request (the answer carries it).
    pub nonce: &'a str,
}

impl RevealTarget<'_> {
    pub fn aad(&self) -> String {
        format!(
            "{}|{}|{}|{}",
            self.discord_id, self.account_id, self.row_id, self.nonce
        )
    }
}

impl RevealKey {
    /// A fresh one-time key pair.
    pub fn generate() -> Self {
        let mut ikm = Zeroizing::new([0u8; 32]);
        rand::fill(&mut ikm[..]);
        Self::from_ikm(&ikm[..])
    }

    /// DeriveKeyPair(ikm): the test vectors' client keys.
    fn from_ikm(ikm: &[u8]) -> Self {
        let (secret, public) = Kem::derive_keypair(ikm);
        Self {
            secret,
            public: URL_SAFE_NO_PAD.encode(public.to_bytes()),
        }
    }

    /// The public key for the request: the uncompressed point in base64url (87 characters).
    pub fn public(&self) -> &str {
        &self.public
    }

    /// Opens the machine's answer and drops the key. The words come back wiped on drop.
    pub fn open(
        self,
        revealed: &str,
        target: &RevealTarget<'_>,
    ) -> Result<Zeroizing<String>, SealError> {
        let payload = revealed
            .strip_prefix(REVEAL_PREFIX)
            .and_then(|rest| rest.strip_prefix('.'))
            .ok_or(SealError::Format)?;
        let (enc, ct) = split_payload(payload)?;
        let mut bytes = open_body(
            &self.secret,
            &enc,
            &ct,
            REVEAL_INFO,
            target.aad().as_bytes(),
        )?;
        if bytes.is_empty() || bytes.contains(&0) {
            return Err(SealError::Plaintext);
        }
        match String::from_utf8(std::mem::take(&mut *bytes)) {
            Ok(text) => Ok(Zeroizing::new(text)),
            Err(error) => {
                error.into_bytes().zeroize();
                Err(SealError::Plaintext)
            }
        }
    }
}

/// Opens a `pl1` value as the machine does (opening steps 1-5): the test vectors' openers' half,
/// so this client is checked against every value an opener must refuse. Never used outside tests:
/// the launcher holds no vault key.
#[cfg(test)]
fn open_sealed(
    keys: &[(String, <Kem as hpke::Kem>::PrivateKey)],
    sealed: &str,
    target: &SealTarget<'_>,
) -> Result<String, SealError> {
    let mut parts = sealed.splitn(3, '.');
    let (Some(prefix), Some(kid), Some(payload)) = (parts.next(), parts.next(), parts.next())
    else {
        return Err(SealError::Format);
    };
    if prefix != SEAL_PREFIX || !is_kid(kid) || payload.contains('.') {
        return Err(SealError::Format);
    }
    let (enc, ct) = split_payload(payload)?;
    let secret = keys
        .iter()
        .find(|(id, _)| id == kid)
        .map(|(_, secret)| secret)
        .ok_or(SealError::UnknownKid)?;
    let bytes = open_body(secret, &enc, &ct, SEAL_INFO, target.aad().as_bytes())?;
    let text = String::from_utf8(bytes.to_vec()).map_err(|_| SealError::Plaintext)?;
    if text.is_empty() || crate::text::check_text(&text).is_err() {
        return Err(SealError::Plaintext);
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;

    /// The copies of the contract's vectors start with `//` lines naming their source.
    fn vectors(raw: &str) -> Value {
        let json: String = raw
            .lines()
            .filter(|line| !line.starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        serde_json::from_str(&json).expect("vectors are JSON")
    }

    fn seal_vectors() -> Value {
        vectors(include_str!("../tests/vectors/seal-v1.json"))
    }

    fn reveal_vectors() -> Value {
        vectors(include_str!("../tests/vectors/reveal-v1.json"))
    }

    fn s(value: &Value, key: &str) -> String {
        value[key]
            .as_str()
            .unwrap_or_else(|| panic!("{key} is a string"))
            .to_owned()
    }

    fn hex(value: &Value, key: &str) -> Vec<u8> {
        ::hex::decode(s(value, key)).unwrap()
    }

    fn server(value: &Value) -> CloudServer {
        CloudServer::new(
            value["server"]["host"].as_str().unwrap(),
            value["server"]["port"].as_u64().unwrap() as u16,
        )
    }

    /// Hands out the vector's ephemeral IKM as the "randomness", as RFC 9180's DeriveKeyPair would
    /// get it.
    struct Replay(Vec<u8>);

    impl hpke::rand_core::TryRng for Replay {
        type Error = std::convert::Infallible;

        fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
            hpke::rand_core::utils::next_word_via_fill(self)
        }

        fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
            hpke::rand_core::utils::next_word_via_fill(self)
        }

        fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), Self::Error> {
            assert!(dest.len() <= self.0.len(), "ran out of replayed randomness");
            let rest = self.0.split_off(dest.len());
            dest.copy_from_slice(&self.0);
            self.0 = rest;
            Ok(())
        }
    }

    impl hpke::rand_core::TryCryptoRng for Replay {}

    fn vault_keys(vectors: &Value) -> Vec<(String, <Kem as hpke::Kem>::PrivateKey)> {
        vectors["keys"]
            .as_array()
            .unwrap()
            .iter()
            .map(|key| {
                let secret = <Kem as hpke::Kem>::PrivateKey::from_bytes(&hex(key, "sk")).unwrap();
                (s(key, "kid"), secret)
            })
            .collect()
    }

    fn vault_key(vectors: &Value, kid: &str) -> VaultKey {
        let key = vectors["keys"]
            .as_array()
            .unwrap()
            .iter()
            .find(|key| key["kid"] == kid)
            .unwrap();
        VaultKey {
            kid: kid.to_owned(),
            alg: SEAL_ALG.to_owned(),
            public_key: s(key, "publicKey"),
        }
    }

    #[test]
    fn the_library_passes_rfc9180_a_3_1() {
        let rfc = &seal_vectors()["rfc9180"];
        let (secret, public) = Kem::derive_keypair(&hex(rfc, "ikmR"));
        assert_eq!(public.to_bytes().to_vec(), hex(rfc, "pkRm"));
        assert_eq!(secret.to_bytes().to_vec(), hex(rfc, "skRm"));
        let first = &rfc["encryptions"][0];
        let (enc, ct) = hpke::single_shot_seal_with_rng::<Aead, Kdf, Kem>(
            &OpModeS::Base,
            &public,
            &hex(rfc, "info"),
            &hex(first, "pt"),
            &hex(first, "aad"),
            &mut hpke::rand_core::UnwrapErr(Replay(hex(rfc, "ikmE"))),
        )
        .unwrap();
        assert_eq!(enc.to_bytes().to_vec(), hex(rfc, "enc"));
        assert_eq!(ct, hex(first, "ct"));
    }

    #[test]
    fn seals_every_vector_byte_for_byte() {
        let vectors = seal_vectors();
        let valid = vectors["vectors"].as_array().unwrap();
        assert_eq!(valid.len(), 10);
        for vector in valid {
            let name = s(vector, "name");
            let server = server(vector);
            let discord_id = s(vector, "discordId");
            let account_id = s(vector, "accountId");
            let row_id = s(vector, "rowId");
            let target = SealTarget {
                discord_id: &discord_id,
                account_id: &account_id,
                row_id: &row_id,
                server: &server,
            };
            assert_eq!(target.aad(), s(vector, "aad"), "{name}");
            let text = s(vector, "text");
            assert_eq!(
                pad(&text).unwrap().to_vec(),
                hex(vector, "padded"),
                "{name}"
            );
            let key = vault_key(&vectors, &s(vector, "kid"));
            let sealed = seal_with(
                &key,
                &text,
                &target,
                &mut hpke::rand_core::UnwrapErr(Replay(hex(vector, "ikmE"))),
            )
            .unwrap();
            assert_eq!(sealed, s(vector, "sealed"), "{name}");
            // And the machines' half opens it again.
            assert_eq!(
                open_sealed(&vault_keys(&vectors), &sealed, &target).unwrap(),
                text,
                "{name}"
            );
        }
    }

    #[test]
    fn refuses_every_invalid_seal_with_its_error_class() {
        let vectors = seal_vectors();
        let keys = vault_keys(&vectors);
        let invalid = vectors["invalid"].as_array().unwrap();
        assert_eq!(invalid.len(), 30);
        for vector in invalid {
            let name = s(vector, "name");
            let server = server(vector);
            let discord_id = s(vector, "discordId");
            let account_id = s(vector, "accountId");
            let row_id = s(vector, "rowId");
            let target = SealTarget {
                discord_id: &discord_id,
                account_id: &account_id,
                row_id: &row_id,
                server: &server,
            };
            assert_eq!(target.aad(), s(vector, "aad"), "{name}");
            let expected = match s(vector, "error").as_str() {
                "format" => SealError::Format,
                "unknown_kid" => SealError::UnknownKid,
                "open_failed" => SealError::OpenFailed,
                "plaintext" => SealError::Plaintext,
                other => panic!("unknown error class {other}"),
            };
            assert_eq!(
                open_sealed(&keys, &s(vector, "sealed"), &target),
                Err(expected),
                "{name}"
            );
        }
    }

    #[test]
    fn fresh_seals_open_and_pad_to_one_length() {
        let vectors = seal_vectors();
        let key = vault_key(&vectors, "k1");
        let server = CloudServer::new("play.projects.gg", 0);
        let target = SealTarget {
            discord_id: "123456789012345678",
            account_id: "a_7KQ2MZ3P9TW4XB1C",
            row_id: "c1",
            server: &server,
        };
        let a = seal(&key, "/login hunter2", &target).unwrap();
        let b = seal(&key, "/login hunter2", &target).unwrap();
        assert_ne!(a, b, "a fresh ephemeral key each time");
        // Every text of up to 64 bytes seals to the same length (201 with a two-letter kid).
        assert_eq!(a.len(), 201);
        assert_eq!(seal(&key, "x", &target).unwrap().len(), 201);
        assert_eq!(
            open_sealed(&vault_keys(&vectors), &a, &target).unwrap(),
            "/login hunter2"
        );
        assert_eq!(fingerprint(&a).len(), 16);
        assert_ne!(fingerprint(&a), fingerprint(&b));
    }

    #[test]
    fn refuses_to_seal_with_another_suite_or_a_bad_key() {
        let vectors = seal_vectors();
        let server = CloudServer::new("play.projects.gg", 0);
        let target = SealTarget {
            discord_id: "1",
            account_id: "a",
            row_id: "c1",
            server: &server,
        };
        let mut key = vault_key(&vectors, "k1");
        key.alg = "HPKE-X25519".into();
        assert_eq!(seal(&key, "x", &target), Err(SealError::Key));
        let mut key = vault_key(&vectors, "k1");
        key.public_key = "AAAA".into();
        assert_eq!(seal(&key, "x", &target), Err(SealError::Key));
        let mut key = vault_key(&vectors, "k1");
        key.kid = "K1".into();
        assert_eq!(seal(&key, "x", &target), Err(SealError::Key));
        let key = vault_key(&vectors, "k1");
        assert_eq!(seal(&key, "", &target), Err(SealError::Length));
        assert_eq!(
            seal(&key, &"a".repeat(769), &target),
            Err(SealError::Length)
        );
    }

    #[test]
    fn the_fingerprint_is_sha256_of_the_value() {
        // base64url(SHA-256("pl1.k1.x")) starts like this.
        let full = URL_SAFE_NO_PAD.encode(Sha256::digest(b"pl1.k1.x"));
        assert_eq!(fingerprint("pl1.k1.x"), full[..16]);
    }

    fn client_key(vectors: &Value, name: &str) -> RevealKey {
        let key = vectors["clientKeys"]
            .as_array()
            .unwrap()
            .iter()
            .find(|key| key["name"] == name)
            .unwrap();
        let derived = RevealKey::from_ikm(&hex(key, "ikm"));
        assert_eq!(derived.public(), s(key, "pub"));
        assert_eq!(derived.secret.to_bytes().to_vec(), hex(key, "sk"));
        derived
    }

    #[test]
    fn opens_every_reveal_vector() {
        let vectors = reveal_vectors();
        let valid = vectors["vectors"].as_array().unwrap();
        assert_eq!(valid.len(), 8);
        for vector in valid {
            let name = s(vector, "name");
            let key = client_key(&vectors, &s(vector, "client"));
            let discord_id = s(vector, "discordId");
            let account_id = s(vector, "accountId");
            let row_id = s(vector, "rowId");
            let nonce = s(vector, "nonce");
            let target = RevealTarget {
                discord_id: &discord_id,
                account_id: &account_id,
                row_id: &row_id,
                nonce: &nonce,
            };
            assert_eq!(target.aad(), s(vector, "aad"), "{name}");
            let text = key.open(&s(vector, "revealed"), &target).unwrap();
            assert_eq!(*text, s(vector, "text"), "{name}");
        }
    }

    #[test]
    fn refuses_every_invalid_reveal_with_its_error_class() {
        let vectors = reveal_vectors();
        let invalid = vectors["invalid"].as_array().unwrap();
        assert_eq!(invalid.len(), 20);
        for vector in invalid {
            let name = s(vector, "name");
            let key = client_key(&vectors, &s(vector, "client"));
            let discord_id = s(vector, "discordId");
            let account_id = s(vector, "accountId");
            let row_id = s(vector, "rowId");
            let nonce = s(vector, "nonce");
            let target = RevealTarget {
                discord_id: &discord_id,
                account_id: &account_id,
                row_id: &row_id,
                nonce: &nonce,
            };
            assert_eq!(target.aad(), s(vector, "aad"), "{name}");
            let expected = match s(vector, "error").as_str() {
                "format" => SealError::Format,
                "open_failed" => SealError::OpenFailed,
                "plaintext" => SealError::Plaintext,
                other => panic!("unknown error class {other}"),
            };
            assert_eq!(
                key.open(&s(vector, "revealed"), &target).map(|_| ()),
                Err(expected),
                "{name}"
            );
        }
    }

    #[test]
    fn one_time_keys_are_fresh_points() {
        let a = RevealKey::generate();
        let b = RevealKey::generate();
        assert_eq!(a.public().len(), 87);
        assert!(a.public().starts_with('B'), "0x04 in base64url");
        assert_ne!(a.public(), b.public());
    }
}
