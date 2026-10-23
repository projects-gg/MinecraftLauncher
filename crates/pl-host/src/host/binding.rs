//! The name binding file (PROTOCOL.md §9): a guest's in-game name belongs, in one world, to the
//! friend who first logged in with it.
//!
//! `<world folder>/projects-guests.json` is written by the mod and read by the host launcher for the
//! control stream's name verdict. Friends appear in it only as digests made with this computer's
//! binding key (`<launcher data dir>/host/guest-binding.key`), never as Discord ids.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use hmac::{Hmac, KeyInit, Mac};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::tunnel::control::NameVerdict;
use crate::tunnel::ids::{b64u, random};

/// The binding file's name in the world folder.
pub const FILE_NAME: &str = "projects-guests.json";
/// The binding key's path under the launcher's data folder.
pub const KEY_PATH: &str = "host/guest-binding.key";
/// Most names in the file.
pub const MAX_NAMES: usize = 512;
const DIGEST_LABEL: &[u8] = b"projects guest binding v1";

type HmacSha256 = Hmac<Sha256>;

/// This computer's binding key: 32 random bytes, made on first hosting, never sent anywhere.
#[derive(Clone)]
pub struct BindingKey(Zeroizing<[u8; 32]>);

impl BindingKey {
    /// A key from its bytes (tests and vectors).
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    /// Reads `<data dir>/host/guest-binding.key`, making it on first use. A file of another length
    /// is replaced (the old bindings then count as empty, as after a data reset).
    pub fn load_or_create(data_dir: &Path) -> io::Result<Self> {
        let path = data_dir.join(KEY_PATH);
        match std::fs::read(&path) {
            Ok(bytes) if bytes.len() == 32 => {
                let bytes = Zeroizing::new(bytes);
                let mut key = [0u8; 32];
                key.copy_from_slice(&bytes);
                return Ok(Self::from_bytes(key));
            }
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        let key = Self::from_bytes(random());
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("key.tmp");
        std::fs::write(&tmp, &key.0[..])?;
        std::fs::rename(&tmp, &path)?;
        Ok(key)
    }

    /// The key id the file names: base64url(SHA-256(key)[0..6]), 8 characters.
    pub fn key_id(&self) -> String {
        b64u(&Sha256::digest(&self.0[..])[..6])
    }

    /// A friend's digest: base64url(HMAC-SHA256(key, `projects guest binding v1` || 0x00 || UTF-8
    /// friend user id)[0..16]), 22 characters.
    pub fn digest(&self, friend: &str) -> String {
        let mut mac = HmacSha256::new_from_slice(&self.0[..]).expect("HMAC takes any key length");
        mac.update(DIGEST_LABEL);
        mac.update(&[0]);
        mac.update(friend.as_bytes());
        b64u(&mac.finalize().into_bytes()[..16])
    }
}

impl fmt::Debug for BindingKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("BindingKey(<redacted>)")
    }
}

/// Whether `name` is a name a guest may log in with: 3–16 of `[A-Za-z0-9_]`.
pub fn is_valid_name(name: &str) -> bool {
    (3..=16).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// One binding.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Binding {
    /// The in-game name.
    pub name: String,
    /// The friend's digest ([`BindingKey::digest`]).
    pub friend: String,
    /// The last login with this name (Unix seconds).
    pub last: i64,
}

#[derive(Deserialize)]
struct FileWire {
    v: u32,
    key: String,
    names: Vec<Binding>,
}

/// The bindings that count for this computer's key.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Bindings {
    names: Vec<Binding>,
}

impl Bindings {
    /// Parses the file. A malformed file, another `v`, or a file written under another key id
    /// counts as empty (the mod renames a malformed one and starts over).
    pub fn parse(bytes: &[u8], key_id: &str) -> Self {
        let Ok(wire) = serde_json::from_slice::<FileWire>(bytes) else {
            return Self::default();
        };
        if wire.v != 1 || wire.key != key_id || wire.names.len() > MAX_NAMES {
            return Self::default();
        }
        let mut seen = std::collections::HashSet::new();
        for binding in &wire.names {
            let friend_ok = binding.friend.len() == 22
                && binding
                    .friend
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
            if !is_valid_name(&binding.name)
                || !friend_ok
                || binding.last < 0
                || !seen.insert(binding.name.to_ascii_lowercase())
            {
                return Self::default();
            }
        }
        Self { names: wire.names }
    }

    /// Reads `<world>/projects-guests.json`; a missing or unreadable file counts as empty.
    pub async fn read(world: &Path, key_id: &str) -> Self {
        match tokio::fs::read(world.join(FILE_NAME)).await {
            Ok(bytes) => Self::parse(&bytes, key_id),
            Err(_) => Self::default(),
        }
    }

    /// The digest `name` is bound to, if any (ASCII case-insensitive).
    pub fn bound_to(&self, name: &str) -> Option<&str> {
        self.names
            .iter()
            .find(|b| b.name.eq_ignore_ascii_case(name))
            .map(|b| b.friend.as_str())
    }

    /// The verdict on `name` for the friend with `digest` (PROTOCOL.md §6.8 steps 3 and 4): invalid
    /// names and the host's own name are [`NameVerdict::Name`], a name bound to another friend
    /// [`NameVerdict::NameTaken`]. An unbound name is fine (the mod binds it at login).
    pub fn verdict(&self, name: &str, digest: &str, host_name: &str) -> NameVerdict {
        if !is_valid_name(name) || name.eq_ignore_ascii_case(host_name) {
            return NameVerdict::Name;
        }
        match self.bound_to(name) {
            Some(friend) if friend != digest => NameVerdict::NameTaken,
            _ => NameVerdict::Ok,
        }
    }
}

/// Where a world's binding file is.
pub fn file_path(world: &Path) -> PathBuf {
    world.join(FILE_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdata;

    fn vector_key() -> BindingKey {
        let file = testdata::json("binding/digest.json");
        let bytes: [u8; 32] = hex::decode(file["bindingKeyHex"].as_str().unwrap())
            .unwrap()
            .try_into()
            .unwrap();
        BindingKey::from_bytes(bytes)
    }

    #[test]
    fn digests() {
        let file = testdata::json("binding/digest.json");
        let key = vector_key();
        assert_eq!(key.key_id(), file["keyId"].as_str().unwrap());
        for case in file["digests"].as_array().unwrap() {
            assert_eq!(
                key.digest(case["friend"].as_str().unwrap()),
                case["digest"].as_str().unwrap()
            );
        }
    }

    #[test]
    fn admission_by_name() {
        let key = vector_key();
        let example = testdata::bytes("binding/projects-guests.example.json");
        let rules = testdata::json("binding/rules.json");
        let host = rules["hostName"].as_str().unwrap();
        for case in rules["cases"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let digest = key.digest(case["friend"].as_str().unwrap());
            let file = match case["fileKey"].as_str() {
                Some(other) => {
                    let text = String::from_utf8(example.clone()).unwrap();
                    text.replace(&key.key_id(), other).into_bytes()
                }
                None => example.clone(),
            };
            let bindings = Bindings::parse(&file, &key.key_id());
            let expect = match case["expect"].as_str().unwrap() {
                "ok" => NameVerdict::Ok,
                "taken" => NameVerdict::NameTaken,
                "host" | "invalid" => NameVerdict::Name,
                other => panic!("unknown expectation {other}"),
            };
            assert_eq!(bindings.verdict(name, &digest, host), expect, "{name}");
        }
    }

    #[test]
    fn malformed_files_count_as_empty() {
        let key = vector_key();
        let id = key.key_id();
        for bad in [
            "not json".to_owned(),
            format!(r#"{{"v":2,"key":"{id}","names":[]}}"#),
            format!(
                r#"{{"v":1,"key":"{id}","names":[{{"name":"No","friend":"1UbirWHKbvkTXKMZNmDDug","last":1}}]}}"#
            ),
            format!(
                r#"{{"v":1,"key":"{id}","names":[{{"name":"Notch","friend":"1UbirWHKbvkTXKMZNmDDug","last":1}},{{"name":"NOTCH","friend":"n7ppm8zILloa1X-qpWnqVA","last":2}}]}}"#
            ),
        ] {
            assert_eq!(
                Bindings::parse(bad.as_bytes(), &id),
                Bindings::default(),
                "{bad}"
            );
        }
    }

    #[test]
    fn the_key_file_is_made_once() {
        let dir = tempfile::tempdir().unwrap();
        let first = BindingKey::load_or_create(dir.path()).unwrap();
        let again = BindingKey::load_or_create(dir.path()).unwrap();
        assert_eq!(first.key_id(), again.key_id());
        assert_eq!(std::fs::read(dir.path().join(KEY_PATH)).unwrap().len(), 32);
        assert_eq!(format!("{first:?}"), "BindingKey(<redacted>)");
    }
}
