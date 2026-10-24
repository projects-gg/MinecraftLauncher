//! The world manifest of an install (PROTOCOL.md §10.1), built the same way on both sides: the
//! host sends it, the guest compares its own installs' with it (`pl_host::manifest::matching`).
//!
//! Every enabled jar in `mods/` whose `fabric.mod.json` names no environment or `*` counts, pack
//! jars included. A jar's Modrinth project and version come from the pack's own record of it
//! (`known`, by SHA-1), else from Modrinth's version-file lookup by its SHA-1 (the same version as
//! a SHA-512 lookup finds); jars Modrinth does not know are only counted. No file name, path or
//! hash ever leaves the computer in the manifest.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::Path;

use pl_host::manifest::{Loader, Manifest, ModRef, PackRef, is_both_side};

/// At most this many of the host's ready-made sets go into a copy (Studio offers a handful).
pub const MAX_COPY_SETS: usize = 8;

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

/// The host's ready-made sets a copy of its install adds (PROTOCOL.md §10.2 step 4), in the
/// manifest's order: each once, at most [`MAX_COPY_SETS`], and only ids a set can have (1–64
/// characters from `[A-Za-z0-9._-]`; the catalog decides which exist).
pub fn copy_sets(manifest: &Manifest) -> Vec<String> {
    let mut sets: Vec<String> = Vec::new();
    for id in &manifest.sets {
        let well_formed = (1..=64).contains(&id.len())
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
        if well_formed && !sets.contains(id) {
            sets.push(id.clone());
            if sets.len() == MAX_COPY_SETS {
                break;
            }
        }
    }
    sets
}

/// The manifest's mods an install still downloads from Modrinth (their version ids): not a
/// project `provided` brings already (the pack's, whose sync installs them with the game's start,
/// and the ready-made sets' installed there: a second copy of one mod stops the game), and not a
/// version already in its `mods/` (`have`).
pub fn mods_to_add(
    manifest: &Manifest,
    provided: &HashSet<String>,
    have: &HashSet<String>,
) -> Vec<String> {
    manifest
        .mods
        .iter()
        .filter(|m| !provided.contains(&m.project) && !have.contains(&m.version))
        .map(|m| m.version.clone())
        .collect()
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

    fn with_sets_and_mods(sets: &[&str], mods: &[(&str, &str)]) -> Manifest {
        Manifest::new(
            "26.1.2",
            Loader {
                kind: "fabric".into(),
                version: "0.19.5".into(),
            },
            None,
            sets.iter().map(|s| (*s).to_owned()).collect(),
            mods.iter().map(|&(p, v)| ModRef::new(p, v)),
            0,
        )
    }

    /// The host's sets reach a copy once each, well formed, a handful at most.
    #[test]
    fn a_copy_takes_the_hosts_sets() {
        let manifest = with_sets_and_mods(
            &["pvp", "performance", "pvp", "", "../x", "a b", "pvp.v2_x-1"],
            &[],
        );
        assert_eq!(copy_sets(&manifest), ["pvp", "performance", "pvp.v2_x-1"]);
        let long = "s".repeat(65);
        let many: Vec<String> = (0..20).map(|i| format!("set{i}")).collect();
        let mut ids: Vec<&str> = vec![long.as_str()];
        ids.extend(many.iter().map(String::as_str));
        let capped = copy_sets(&with_sets_and_mods(&ids, &[]));
        assert_eq!(capped.len(), MAX_COPY_SETS);
        assert_eq!(capped[0], "set0", "an id longer than 64 is not one");
        assert!(copy_sets(&with_sets_and_mods(&[], &[])).is_empty());
    }

    /// What the pack or a set brings is never downloaded a second time; a version already there
    /// neither.
    #[test]
    fn a_copy_downloads_only_what_nothing_else_brings() {
        let manifest = with_sets_and_mods(
            &["pvp"],
            &[
                ("fabric-api", "api-2"),
                ("zoom", "zoom-1"),
                ("farmers", "fd-3"),
                ("voice", "vc-2"),
            ],
        );
        let provided: HashSet<String> = ["fabric-api".to_owned(), "zoom".to_owned()].into();
        let have: HashSet<String> = ["vc-2".to_owned()].into();
        assert_eq!(mods_to_add(&manifest, &provided, &have), ["fd-3"]);
        // The manifest keeps its mods sorted by project.
        assert_eq!(
            mods_to_add(&manifest, &HashSet::new(), &HashSet::new()),
            ["api-2", "fd-3", "vc-2", "zoom-1"]
        );
    }
}
