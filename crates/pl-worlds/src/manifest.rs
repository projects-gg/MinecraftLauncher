//! The world manifest of an install (PROTOCOL.md §10.1), built the same way on both sides: the
//! host sends it, the guest compares its own installs' with it (`pl_host::manifest::matching`).
//!
//! Every enabled jar in `mods/` whose `fabric.mod.json` names no environment or `*` counts, pack
//! jars included. A jar's Modrinth project and version come from the pack's own record of it
//! (`known`, by SHA-1), else from Modrinth's version-file lookup by its SHA-1 (the same version as
//! a SHA-512 lookup finds); jars Modrinth does not know are only counted. No file name, path or
//! hash ever leaves the computer in the manifest.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use pl_host::manifest::{Loader, Manifest, ModRef, PackRef, is_both_side};

/// An enabled jar of an install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Jar {
    /// Lowercase hex SHA-1.
    pub sha1: String,
    /// Its `fabric.mod.json` names no environment or `*` (a jar without one is not a Fabric mod and
    /// is left out too).
    pub both_side: bool,
}

/// The `environment` of a jar's `fabric.mod.json`: `Some(None)` when the file has none, `None`
/// when there is no readable `fabric.mod.json`.
pub fn fabric_environment(path: &Path) -> Option<Option<String>> {
    let file = std::fs::File::open(path).ok()?;
    let mut archive = zip::ZipArchive::new(std::io::BufReader::new(file)).ok()?;
    let mut entry = archive.by_name("fabric.mod.json").ok()?;
    if entry.size() > 1024 * 1024 {
        return None;
    }
    let mut text = String::new();
    entry.read_to_string(&mut text).ok()?;
    // Fabric tolerates control characters in strings; serde_json does not. Newlines and tabs
    // inside strings are the usual culprits.
    let cleaned: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let value: serde_json::Value = serde_json::from_str(&cleaned).ok()?;
    Some(
        value
            .get("environment")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
    )
}

fn sha1_hex(path: &Path) -> Option<String> {
    use sha1::Digest;
    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = sha1::Sha1::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = file.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(hex::encode(hasher.finalize()))
}

/// The enabled jars of a `mods/` folder (blocking: hashes every jar). A missing folder has none.
pub fn scan(mods_dir: &Path) -> Vec<Jar> {
    let Ok(entries) = std::fs::read_dir(mods_dir) else {
        return Vec::new();
    };
    let mut jars = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if !name.ends_with(".jar") || name.starts_with('.') {
            continue;
        }
        if !entry.file_type().is_ok_and(|t| t.is_file()) {
            continue;
        }
        let path = entry.path();
        let Some(environment) = fabric_environment(&path) else {
            continue;
        };
        let Some(sha1) = sha1_hex(&path) else {
            continue;
        };
        jars.push(Jar {
            sha1,
            both_side: is_both_side(environment.as_deref()),
        });
    }
    jars.sort_by(|a, b| a.sha1.cmp(&b.sha1));
    jars
}

/// The install the jars belong to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallSide {
    pub mc: String,
    /// `fabric` and its version.
    pub loader: Loader,
    pub pack: Option<PackRef>,
    pub sets: Vec<String>,
}

/// The SHA-1s the manifest still needs Modrinth for (both-side jars `known` does not name).
pub fn unknown_hashes(jars: &[Jar], known: &HashMap<String, ModRef>) -> Vec<String> {
    jars.iter()
        .filter(|jar| jar.both_side && !known.contains_key(&jar.sha1))
        .map(|jar| jar.sha1.clone())
        .collect()
}

/// The manifest of an install: `known` names the jars the pack recorded, `looked_up` those
/// Modrinth knew (both by SHA-1); the rest are counted as unknown.
pub fn build(
    install: InstallSide,
    jars: &[Jar],
    known: &HashMap<String, ModRef>,
    looked_up: &HashMap<String, ModRef>,
) -> Manifest {
    let mut mods = Vec::new();
    let mut unknown = 0u32;
    for jar in jars.iter().filter(|jar| jar.both_side) {
        match known.get(&jar.sha1).or_else(|| looked_up.get(&jar.sha1)) {
            Some(found) => mods.push(ModRef::new(&found.project, &found.version)),
            None => unknown = unknown.saturating_add(1),
        }
    }
    Manifest::new(
        install.mc,
        install.loader,
        install.pack,
        install.sets,
        mods,
        unknown,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn jar(dir: &Path, name: &str, fabric: Option<&str>) -> std::path::PathBuf {
        let path = dir.join(name);
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        if let Some(json) = fabric {
            zip.start_file("fabric.mod.json", options).unwrap();
            zip.write_all(json.as_bytes()).unwrap();
        }
        zip.start_file(format!("{name}.class"), options).unwrap();
        zip.write_all(name.as_bytes()).unwrap();
        zip.finish().unwrap();
        path
    }

    #[test]
    fn both_side_jars_count_and_ids_come_from_the_pack_then_modrinth() {
        let dir = tempfile::tempdir().unwrap();
        let mods = dir.path().join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        let sodium = jar(
            &mods,
            "sodium.jar",
            Some(r#"{"id":"sodium","environment":"client"}"#),
        );
        let fabric_api = jar(&mods, "fabric-api.jar", Some(r#"{"id":"fabric-api"}"#));
        let voice = jar(
            &mods,
            "voicechat.jar",
            Some("{\"id\":\"voicechat\",\"environment\":\"*\",\"description\":\"a\tb\"}"),
        );
        let homemade = jar(&mods, "homemade.jar", Some(r#"{"id":"homemade"}"#));
        jar(&mods, "not-fabric.jar", None);
        jar(&mods, "off.jar.disabled", Some(r#"{"id":"off"}"#));
        std::fs::write(mods.join("notes.txt"), "x").unwrap();

        assert_eq!(fabric_environment(&sodium), Some(Some("client".into())));
        assert_eq!(fabric_environment(&fabric_api), Some(None));
        let jars = scan(&mods);
        assert_eq!(
            jars.len(),
            4,
            "the jars with a fabric.mod.json, enabled ones only"
        );
        assert_eq!(jars.iter().filter(|j| j.both_side).count(), 3);

        let sha = |path: &Path| sha1_hex(path).unwrap();
        let known: HashMap<String, ModRef> =
            [(sha(&fabric_api), ModRef::new("P7dR8mSH", "api-1"))].into();
        let mut missing = unknown_hashes(&jars, &known);
        missing.sort();
        let mut expected = vec![sha(&voice), sha(&homemade)];
        expected.sort();
        assert_eq!(missing, expected);
        let looked_up: HashMap<String, ModRef> =
            [(sha(&voice), ModRef::new("9eGKb6K1", "vc-2"))].into();
        let install = InstallSide {
            mc: "26.1.2".into(),
            loader: Loader {
                kind: "fabric".into(),
                version: "0.19.5".into(),
            },
            pack: None,
            sets: Vec::new(),
        };
        let manifest = build(install, &jars, &known, &looked_up);
        assert_eq!(
            manifest.mods,
            vec![
                ModRef::new("9eGKb6K1", "vc-2"),
                ModRef::new("P7dR8mSH", "api-1")
            ]
        );
        assert_eq!(
            manifest.unknown, 1,
            "the homemade jar Modrinth does not know"
        );
        assert!(manifest.hash_ok());
        assert!(scan(&dir.path().join("none")).is_empty());
    }
}
