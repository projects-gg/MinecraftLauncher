//! The world manifest (PROTOCOL.md §10.1): which both-side mods the host's install has, so the
//! guest's launcher can pick or prepare a matching install ([`matching`]).
//!
//! The host sends it on every control stream after `ok`, never through the service. Mods go as
//! Modrinth project and version ids; jars Modrinth does not know go only as a count.

pub mod matching;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};

/// `manifest.v` this launcher speaks.
pub const MANIFEST_VERSION: u32 = 1;
/// Most entries in `mods`; the rest are counted in `unknown`.
pub const MAX_MODS: usize = 1_000;
/// The first line of the mod list's canonical form.
pub const HASH_HEADER: &str = "plj-mods-v1\n";

/// The install's mod loader.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Loader {
    /// `fabric` in phase 1.
    pub kind: String,
    pub version: String,
}

/// The official pack the install follows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackRef {
    /// The pack family (`projects`).
    pub family: String,
    /// The pack id for this Minecraft version (`projects-26.1.2`).
    pub id: String,
    /// The pack version the install is on.
    pub version: String,
}

/// One both-side mod.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ModRef {
    /// Modrinth project id.
    pub project: String,
    /// Modrinth version id.
    pub version: String,
    /// `both` in version 1.
    #[serde(default = "both")]
    pub env: String,
}

fn both() -> String {
    "both".to_owned()
}

impl ModRef {
    pub fn new(project: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            project: project.into(),
            version: version.into(),
            env: both(),
        }
    }

    /// `<project>:<version>`, the line the hash covers.
    fn line(&self) -> String {
        format!("{}:{}\n", self.project, self.version)
    }
}

/// Whether a jar is listed: its `fabric.mod.json` `environment` is `*` or absent.
pub fn is_both_side(environment: Option<&str>) -> bool {
    matches!(environment, None | Some("*"))
}

/// A world manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub mc: String,
    pub loader: Loader,
    /// `None` for an install that follows no pack (phase 2).
    pub pack: Option<PackRef>,
    /// The ready-made sets added to the install.
    pub sets: Vec<String>,
    /// Every enabled both-side jar Modrinth knows, sorted.
    pub mods: Vec<ModRef>,
    /// Both-side jars Modrinth does not know (and entries beyond [`MAX_MODS`]).
    pub unknown: u32,
    /// [`mods_hash`] of `mods`.
    pub hash: String,
}

impl Manifest {
    /// Builds a manifest: sorts and de-duplicates the mods, keeps at most [`MAX_MODS`] (counting
    /// the rest as unknown) and computes the hash.
    pub fn new(
        mc: impl Into<String>,
        loader: Loader,
        pack: Option<PackRef>,
        sets: Vec<String>,
        mods: impl IntoIterator<Item = ModRef>,
        unknown: u32,
    ) -> Self {
        let mut mods: Vec<ModRef> = mods.into_iter().collect();
        mods.sort();
        mods.dedup_by(|a, b| a.project == b.project && a.version == b.version);
        let mut unknown = unknown;
        if mods.len() > MAX_MODS {
            let cut = mods.len() - MAX_MODS;
            unknown = unknown.saturating_add(u32::try_from(cut).unwrap_or(u32::MAX));
            mods.truncate(MAX_MODS);
        }
        let hash = mods_hash(&mods);
        Self {
            mc: mc.into(),
            loader,
            pack,
            sets,
            mods,
            unknown,
            hash,
        }
    }

    /// Whether `hash` matches `mods`.
    pub fn hash_ok(&self) -> bool {
        self.hash == mods_hash(&self.mods)
    }
}

/// Lowercase hex SHA-256 of UTF-8 `plj-mods-v1\n` followed by `<project>:<version>\n` for every mod,
/// the lines sorted bytewise.
pub fn mods_hash(mods: &[ModRef]) -> String {
    hex_lower(&Sha256::digest(canonical(mods)))
}

/// The bytes [`mods_hash`] hashes.
pub fn canonical(mods: &[ModRef]) -> Vec<u8> {
    let mut lines: Vec<String> = mods.iter().map(ModRef::line).collect();
    lines.sort();
    let mut out = HASH_HEADER.as_bytes().to_vec();
    for line in lines {
        out.extend_from_slice(line.as_bytes());
    }
    out
}

fn hex_lower(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, b| {
            let _ = write!(out, "{b:02x}");
            out
        })
}

#[derive(Serialize)]
struct ManifestOut<'a> {
    t: &'static str,
    v: u32,
    mc: &'a str,
    loader: &'a Loader,
    pack: Option<&'a PackRef>,
    sets: &'a [String],
    mods: &'a [ModRef],
    unknown: u32,
    hash: &'a str,
}

#[derive(Deserialize)]
struct ManifestIn {
    #[serde(default)]
    t: Option<String>,
    v: u32,
    mc: String,
    loader: Loader,
    #[serde(default)]
    pack: Option<PackRef>,
    #[serde(default)]
    sets: Vec<String>,
    #[serde(default)]
    mods: Vec<ModRef>,
    #[serde(default)]
    unknown: u32,
    hash: String,
}

impl Serialize for Manifest {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ManifestOut {
            t: "manifest",
            v: MANIFEST_VERSION,
            mc: &self.mc,
            loader: &self.loader,
            pack: self.pack.as_ref(),
            sets: &self.sets,
            mods: &self.mods,
            unknown: self.unknown,
            hash: &self.hash,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Manifest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = ManifestIn::deserialize(deserializer)?;
        if wire.t.as_deref().is_some_and(|t| t != "manifest") {
            return Err(D::Error::custom("not a manifest"));
        }
        if wire.v != MANIFEST_VERSION {
            return Err(D::Error::custom("unknown manifest version"));
        }
        if wire.mods.len() > MAX_MODS {
            return Err(D::Error::custom("too many mods"));
        }
        Ok(Self {
            mc: wire.mc,
            loader: wire.loader,
            pack: wire.pack,
            sets: wire.sets,
            mods: wire.mods,
            unknown: wire.unknown,
            hash: wire.hash,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdata;

    #[test]
    fn examples_match_their_hashes_and_bytes() {
        let file = testdata::json("manifest/examples.json");
        for example in file["examples"].as_array().unwrap() {
            let case = example["case"].as_str().unwrap();
            let wire = &example["manifest"];
            let parsed: Manifest = serde_json::from_value(wire.clone()).unwrap();
            assert!(parsed.hash_ok(), "{case}");
            // Built from its parts the manifest serializes to the same JSON.
            let built = Manifest::new(
                parsed.mc.clone(),
                parsed.loader.clone(),
                parsed.pack.clone(),
                parsed.sets.clone(),
                parsed.mods.iter().rev().cloned(),
                parsed.unknown,
            );
            assert_eq!(built, parsed, "{case}");
            assert_eq!(serde_json::to_value(&built).unwrap(), *wire, "{case}");
            if let Some(hex) = example["canonicalHex"].as_str() {
                assert_eq!(hex::encode(canonical(&built.mods)), hex, "{case}");
            }
        }
    }

    #[test]
    fn long_lists_are_cut_and_counted() {
        let mods = (0..1_005).map(|i| ModRef::new(format!("p{i:07}"), "v"));
        let loader = Loader {
            kind: "fabric".into(),
            version: "0.19.5".into(),
        };
        let manifest = Manifest::new("26.1.2", loader, None, vec![], mods, 2);
        assert_eq!(manifest.mods.len(), MAX_MODS);
        assert_eq!(manifest.unknown, 7);
        let json = serde_json::to_value(&manifest).unwrap();
        assert!(json["pack"].is_null());
    }

    #[test]
    fn environments() {
        assert!(is_both_side(None));
        assert!(is_both_side(Some("*")));
        assert!(!is_both_side(Some("client")));
        assert!(!is_both_side(Some("server")));
    }
}
