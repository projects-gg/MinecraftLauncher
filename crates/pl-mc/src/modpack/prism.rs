//! Prism Launcher / MultiMC instance zips: `instance.cfg` (INI) + `mmc-pack.json` components
//! + the game directory (`.minecraft/` or `minecraft/`).
//!
//! Pre/post-launch and wrapper commands from `instance.cfg` are deliberately not imported: they
//! would run arbitrary programs from a downloaded file.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::path::Path;

use pl_net::CancellationToken;
use serde::Deserialize;
use zip::ZipArchive;

use super::extract::{ExtractReport, entry_names, extract_prefix, read_entry_string};
use super::{ImportedInstance, ModpackFormat};
use crate::error::{Error, Result};
use crate::gamefiles::fsops::{MAX_ICON_BYTES, read_png_limited};
use crate::loaders::LoaderKind;

const MAX_CONFIG_BYTES: u64 = 4 * 1024 * 1024;

/// Component uids that map onto an instance without loss.
const KNOWN_COMPONENTS: [&str; 10] = [
    "net.minecraft",
    "net.fabricmc.fabric-loader",
    "net.fabricmc.intermediary",
    "org.quiltmc.quilt-loader",
    "org.quiltmc.hashed",
    "net.minecraftforge",
    "net.neoforged",
    "org.lwjgl",
    "org.lwjgl3",
    "com.mumfrey.liteloader",
];

#[derive(Debug, Deserialize)]
struct MmcPack {
    #[serde(default)]
    components: Vec<MmcComponent>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MmcComponent {
    uid: String,
    version: Option<String>,
    cached_version: Option<String>,
    cached_name: Option<String>,
}

impl MmcComponent {
    fn version(&self) -> Option<String> {
        self.version
            .clone()
            .or_else(|| self.cached_version.clone())
            .filter(|v| !v.trim().is_empty())
    }
}

/// Parse Qt `QSettings` INI text (sections are flattened; Prism writes `[General]`).
pub(crate) fn parse_instance_cfg(text: &str) -> HashMap<String, String> {
    let mut values = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            values.insert(key.trim().to_owned(), unquote(value.trim()));
        }
    }
    values
}

fn unquote(value: &str) -> String {
    let inner = if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        &value[1..value.len() - 1]
    } else {
        value
    };
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => {}
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// Find the instance root inside the zip: the folder holding `instance.cfg` (the zip root or
/// one folder deep). Returns the prefix with a trailing `/` (or empty).
pub(crate) fn find_root(names: &[String], marker: &str) -> Option<String> {
    names
        .iter()
        .filter_map(|name| {
            let prefix = name.strip_suffix(marker)?;
            let depth = prefix.matches('/').count();
            let ok = prefix.is_empty() || (depth == 1 && prefix.ends_with('/'));
            ok.then(|| prefix.to_owned())
        })
        .min_by_key(String::len)
}

fn flag(cfg: &HashMap<String, String>, key: &str) -> bool {
    cfg.get(key).is_some_and(|v| v.eq_ignore_ascii_case("true"))
}

fn loader_for(uid: &str) -> Option<LoaderKind> {
    match uid {
        "net.fabricmc.fabric-loader" => Some(LoaderKind::Fabric),
        "org.quiltmc.quilt-loader" => Some(LoaderKind::Quilt),
        "net.minecraftforge" => Some(LoaderKind::Forge),
        "net.neoforged" => Some(LoaderKind::NeoForge),
        _ => None,
    }
}

pub(crate) fn import_prism_archive<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    archive_path: &Path,
    game_dir: &Path,
    cancel: &CancellationToken,
) -> Result<ImportedInstance> {
    let (mut instance, game_prefix) = describe_prism_archive(archive, archive_path)?;
    let mut report = ExtractReport::default();
    extract_prefix(
        archive,
        archive_path,
        &game_prefix,
        game_dir,
        cancel,
        &mut report,
    )?;
    instance.apply_report(report);
    Ok(instance)
}

/// Read what a Prism/MultiMC export describes without extracting anything. Returns the
/// instance and the archive prefix of its game directory.
pub(crate) fn describe_prism_archive<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    archive_path: &Path,
) -> Result<(ImportedInstance, String)> {
    let names = entry_names(archive);
    let root = find_root(&names, "instance.cfg")
        .ok_or_else(|| Error::archive(archive_path, "instance.cfg not found"))?;
    let cfg_text = read_entry_string(archive, &format!("{root}instance.cfg"), MAX_CONFIG_BYTES)
        .ok_or_else(|| Error::archive(archive_path, "instance.cfg is unreadable"))?;
    let cfg = parse_instance_cfg(&cfg_text);
    let pack: Option<MmcPack> =
        read_entry_string(archive, &format!("{root}mmc-pack.json"), MAX_CONFIG_BYTES)
            .map(|text| serde_json::from_str(&text).map_err(|e| Error::json("mmc-pack.json", e)))
            .transpose()?;
    let components = pack.map(|p| p.components).unwrap_or_default();

    let mc_version = components
        .iter()
        .find(|c| c.uid == "net.minecraft")
        .and_then(MmcComponent::version)
        .or_else(|| {
            cfg.get("IntendedVersion")
                .cloned()
                .filter(|v| !v.is_empty())
        })
        .ok_or_else(|| Error::Invalid("the instance has no Minecraft version".into()))?;
    let (loader, loader_version) = components
        .iter()
        .find_map(|c| loader_for(&c.uid).map(|kind| (kind, c.version())))
        .unwrap_or((LoaderKind::Vanilla, None));

    let mut unsupported: Vec<String> = components
        .iter()
        .filter(|c| !KNOWN_COMPONENTS.contains(&c.uid.as_str()))
        .map(|c| c.cached_name.clone().unwrap_or_else(|| c.uid.clone()))
        .collect();
    if components.iter().any(|c| c.uid == "com.mumfrey.liteloader") {
        unsupported.push("LiteLoader".into());
    }
    for (folder, label) in [
        ("jarmods/", "jar mods"),
        ("patches/", "custom version patches"),
    ] {
        let prefix = format!("{root}{folder}");
        if names
            .iter()
            .any(|n| n.starts_with(&prefix) && n.len() > prefix.len())
        {
            unsupported.push(label.into());
        }
    }
    for key in ["PreLaunchCommand", "PostExitCommand", "WrapperCommand"] {
        if flag(&cfg, "OverrideCommands") && cfg.get(key).is_some_and(|v| !v.trim().is_empty()) {
            unsupported.push(format!("{key} (not imported for safety)"));
        }
    }

    let dot_minecraft = format!("{root}.minecraft/");
    let game_prefix = if names.iter().any(|n| n.starts_with(&dot_minecraft)) {
        dot_minecraft
    } else {
        format!("{root}minecraft/")
    };

    let name = cfg
        .get("name")
        .cloned()
        .filter(|n| !n.trim().is_empty())
        .or_else(|| {
            archive_path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "Imported instance".into());
    let mut instance = ImportedInstance::new(ModpackFormat::Prism, name, mc_version);
    instance.loader = loader;
    instance.loader_version = loader_version;
    instance.notes = cfg.get("notes").cloned().filter(|n| !n.trim().is_empty());
    if flag(&cfg, "OverrideJavaArgs") {
        instance.jvm_args = cfg.get("JvmArgs").cloned().filter(|a| !a.trim().is_empty());
    }
    if flag(&cfg, "OverrideMemory") {
        instance.min_memory_mb = cfg.get("MinMemAlloc").and_then(|v| v.parse().ok());
        instance.max_memory_mb = cfg.get("MaxMemAlloc").and_then(|v| v.parse().ok());
    }
    instance.total_played_secs = cfg.get("totalTimePlayed").and_then(|v| v.parse().ok());
    instance.icon_png = cfg
        .get("iconKey")
        .filter(|key| !key.is_empty() && !key.contains('/') && !key.contains('\\'))
        .and_then(|key| {
            let index = archive.index_for_name(&format!("{root}{key}.png"))?;
            let entry = archive.by_index(index).ok()?;
            if entry.size() > MAX_ICON_BYTES {
                return None;
            }
            read_png_limited(entry, MAX_ICON_BYTES)
        });
    instance.unsupported = unsupported;
    Ok((instance, game_prefix))
}

pub(crate) fn import_prism_blocking(
    path: &Path,
    game_dir: &Path,
    cancel: &CancellationToken,
) -> Result<ImportedInstance> {
    let file = File::open(path).map_err(|e| Error::io(path, e))?;
    let mut archive = ZipArchive::new(BufReader::new(file)).map_err(|e| Error::archive(path, e))?;
    import_prism_archive(&mut archive, path, game_dir, cancel)
}

/// Import a Prism Launcher / MultiMC instance export into `game_dir`.
pub async fn import_prism(
    path: &Path,
    game_dir: &Path,
    cancel: CancellationToken,
) -> Result<ImportedInstance> {
    let (path, game_dir) = (path.to_path_buf(), game_dir.to_path_buf());
    crate::gamefiles::fsops::blocking(move || import_prism_blocking(&path, &game_dir, &cancel))
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mods::test_support::{png, zip_bytes};

    const CFG: &str = "[General]\nConfigVersion=1.2\nInstanceType=OneSix\nname=My \\\"Fabric\\\" Pack\niconKey=myicon\nnotes=\"Line one\\nLine two\"\nOverrideMemory=true\nMinMemAlloc=1024\nMaxMemAlloc=6144\nOverrideJavaArgs=true\nJvmArgs=-XX:+UseZGC\nOverrideCommands=true\nPreLaunchCommand=rm -rf ~\ntotalTimePlayed=3600\n";

    const PACK: &str = r#"{
        "components": [
            {"uid": "org.lwjgl3", "version": "3.3.1", "dependencyOnly": true},
            {"uid": "net.minecraft", "version": "1.20.1", "important": true},
            {"uid": "net.fabricmc.intermediary", "version": "1.20.1"},
            {"uid": "net.fabricmc.fabric-loader", "cachedVersion": "0.15.11"},
            {"uid": "custom.patch", "cachedName": "Custom Thing", "version": "1"}
        ],
        "formatVersion": 1
    }"#;

    #[test]
    fn instance_cfg_parsing() {
        let cfg = parse_instance_cfg(CFG);
        assert_eq!(cfg["name"], "My \"Fabric\" Pack");
        assert_eq!(cfg["notes"], "Line one\nLine two");
        assert_eq!(cfg["MaxMemAlloc"], "6144");
    }

    #[test]
    fn root_detection() {
        let names = vec![
            "Pack/instance.cfg".to_owned(),
            "Pack/.minecraft/options.txt".to_owned(),
        ];
        assert_eq!(find_root(&names, "instance.cfg").as_deref(), Some("Pack/"));
        let names = vec!["instance.cfg".to_owned()];
        assert_eq!(find_root(&names, "instance.cfg").as_deref(), Some(""));
        let names = vec!["a/b/instance.cfg".to_owned()];
        assert_eq!(find_root(&names, "instance.cfg"), None);
    }

    #[tokio::test]
    async fn prism_zip_import() {
        let dir = tempfile::tempdir().unwrap();
        let zip = dir.path().join("export.zip");
        let icon = png(50);
        std::fs::write(
            &zip,
            zip_bytes(&[
                ("My Pack/instance.cfg", CFG.as_bytes()),
                ("My Pack/mmc-pack.json", PACK.as_bytes()),
                ("My Pack/myicon.png", &icon),
                ("My Pack/.minecraft/mods/sodium.jar", b"jar"),
                ("My Pack/.minecraft/config/sodium.json", b"{}"),
                ("My Pack/.minecraft/../../escape", b"x"),
                ("My Pack/patches/custom.patch.json", b"{}"),
            ]),
        )
        .unwrap();
        let game = dir.path().join("game");
        let instance = import_prism(&zip, &game, CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(instance.format, ModpackFormat::Prism);
        assert_eq!(instance.name, "My \"Fabric\" Pack");
        assert_eq!(instance.mc_version, "1.20.1");
        assert_eq!(instance.loader, LoaderKind::Fabric);
        assert_eq!(instance.loader_version.as_deref(), Some("0.15.11"));
        assert_eq!(instance.min_memory_mb, Some(1024));
        assert_eq!(instance.max_memory_mb, Some(6144));
        assert_eq!(instance.jvm_args.as_deref(), Some("-XX:+UseZGC"));
        assert_eq!(instance.notes.as_deref(), Some("Line one\nLine two"));
        assert_eq!(instance.total_played_secs, Some(3600));
        assert_eq!(instance.icon_png.as_deref(), Some(icon.as_slice()));
        assert_eq!(instance.extracted_files, 2);
        assert_eq!(instance.skipped_entries.len(), 1);
        assert!(instance.unsupported.contains(&"Custom Thing".to_owned()));
        assert!(
            instance
                .unsupported
                .contains(&"custom version patches".to_owned())
        );
        assert!(
            instance
                .unsupported
                .iter()
                .any(|u| u.starts_with("PreLaunchCommand"))
        );
        assert_eq!(
            std::fs::read(game.join("mods").join("sodium.jar")).unwrap(),
            b"jar"
        );
    }

    #[tokio::test]
    async fn legacy_multimc_without_mmc_pack() {
        let dir = tempfile::tempdir().unwrap();
        let zip = dir.path().join("old.zip");
        std::fs::write(
            &zip,
            zip_bytes(&[
                ("instance.cfg", b"IntendedVersion=1.7.10\nname=Old\n"),
                ("minecraft/options.txt", b"x"),
            ]),
        )
        .unwrap();
        let game = dir.path().join("game");
        let instance = import_prism(&zip, &game, CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(instance.mc_version, "1.7.10");
        assert_eq!(instance.loader, LoaderKind::Vanilla);
        assert!(game.join("options.txt").exists());

        std::fs::write(&zip, zip_bytes(&[("instance.cfg", b"name=NoVersion\n")])).unwrap();
        assert!(
            import_prism(&zip, &game, CancellationToken::new())
                .await
                .is_err()
        );
    }
}
