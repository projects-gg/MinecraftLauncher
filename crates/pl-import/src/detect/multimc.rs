//! Prism Launcher, PolyMC and MultiMC: `<data>/instances/<folder>/` with `instance.cfg` (Qt INI:
//! name, last launch) and `mmc-pack.json` (components: the game and its loader), the game folder
//! being `.minecraft/` or `minecraft/` inside it. The launcher's own settings file may move the
//! instances folder (`InstanceDir`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use pl_mc::loaders::LoaderKind;
use serde::Deserialize;

use super::{count_mods, mark_imported, modified_ms, read_small, sort_profiles};
use crate::journal::{ImportedFrom, same_path};
use crate::locate::Locations;
use crate::model::{ImportProfile, ImportSource, SourceKind, Unsupported};

const MAX_CONFIG: u64 = 4 * 1024 * 1024;

pub(super) fn detect(locations: &Locations, imported: &[ImportedFrom]) -> Vec<ImportSource> {
    let mut out: Vec<ImportSource> = Vec::new();
    let mut seen: Vec<PathBuf> = Vec::new();
    for (kind, root) in locations.multimc_family() {
        let Some(instances) = instances_dir(kind, &root) else {
            continue;
        };
        if seen.iter().any(|s| same_path(s, &instances)) {
            continue;
        }
        seen.push(instances.clone());
        let Ok(entries) = std::fs::read_dir(&instances) else {
            continue;
        };
        let mut profiles: Vec<ImportProfile> = entries
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .filter(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                !name.starts_with('.') && !name.starts_with('_')
            })
            .filter_map(|e| read_instance(&e.path()))
            .collect();
        if profiles.is_empty() {
            continue;
        }
        mark_imported(&instances, &mut profiles, imported);
        sort_profiles(&mut profiles);
        let base = match kind {
            SourceKind::Prism => "prism",
            SourceKind::PolyMc => "polymc",
            _ => "multimc",
        };
        let id = match out.iter().filter(|s| s.id.starts_with(base)).count() {
            0 => base.to_owned(),
            n => format!("{base}-{}", n + 1),
        };
        out.push(ImportSource {
            id,
            kind,
            path: instances.display().to_string(),
            shared: None,
            last_used_ms: profiles.iter().filter_map(|p| p.last_used_ms).max(),
            profiles,
            offline_name: None,
            root: instances,
        });
    }
    out
}

/// The instances folder of a Prism-family data folder, when it has one.
fn instances_dir(kind: SourceKind, root: &Path) -> Option<PathBuf> {
    if !root.is_dir() {
        return None;
    }
    let config = match kind {
        SourceKind::Prism => "prismlauncher.cfg",
        SourceKind::PolyMc => "polymc.cfg",
        _ => "multimc.cfg",
    };
    let custom = read_small(&root.join(config), MAX_CONFIG)
        .map(|text| parse_ini(&text))
        .and_then(|cfg| cfg.get("InstanceDir").cloned())
        .filter(|dir| !dir.trim().is_empty());
    let dir = match custom {
        Some(dir) if Path::new(&dir).is_absolute() => PathBuf::from(dir),
        Some(dir) => root.join(dir),
        None => root.join("instances"),
    };
    dir.is_dir().then_some(dir)
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct MmcPack {
    components: Vec<MmcComponent>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct MmcComponent {
    uid: String,
    version: Option<String>,
    cached_version: Option<String>,
}

impl MmcComponent {
    fn version(&self) -> Option<String> {
        self.version
            .clone()
            .or_else(|| self.cached_version.clone())
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
    }
}

fn read_instance(dir: &Path) -> Option<ImportProfile> {
    let cfg = parse_ini(&read_small(&dir.join("instance.cfg"), MAX_CONFIG)?);
    let pack: MmcPack = read_small(&dir.join("mmc-pack.json"), MAX_CONFIG)
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    let folder = dir.file_name()?.to_string_lossy().into_owned();
    let mut minecraft = None;
    let mut loader = LoaderKind::Vanilla;
    let mut loader_version = None;
    let mut unsupported = None;
    for component in &pack.components {
        let kind = match component.uid.as_str() {
            "net.minecraft" => {
                minecraft = component.version();
                continue;
            }
            "net.fabricmc.fabric-loader" => LoaderKind::Fabric,
            "org.quiltmc.quilt-loader" => LoaderKind::Quilt,
            "net.minecraftforge" => LoaderKind::Forge,
            "net.neoforged" => LoaderKind::NeoForge,
            "com.mumfrey.liteloader" => {
                unsupported = Some(Unsupported::Loader {
                    name: "LiteLoader".to_owned(),
                });
                continue;
            }
            uid if uid.starts_with("net.legacyfabric") => {
                unsupported = Some(Unsupported::Loader {
                    name: "Legacy Fabric".to_owned(),
                });
                continue;
            }
            _ => continue,
        };
        loader = kind;
        loader_version = component.version();
    }
    // Older MultiMC instances named the version in instance.cfg.
    if minecraft.is_none() {
        minecraft = cfg
            .get("IntendedVersion")
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty());
    }
    if minecraft.is_none() && unsupported.is_none() {
        unsupported = Some(Unsupported::UnknownVersion);
    }
    let game_dir = [".minecraft", "minecraft"]
        .into_iter()
        .map(|name| dir.join(name))
        .find(|p| p.is_dir())
        .unwrap_or_else(|| dir.join(".minecraft"));
    let name = cfg
        .get("name")
        .map(|n| n.trim().to_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| folder.clone());
    let last_used_ms = cfg
        .get("lastLaunchTime")
        .and_then(|v| v.trim().parse::<i64>().ok())
        .filter(|ms| *ms > 0)
        .or_else(|| modified_ms(&game_dir.join("options.txt")));
    Some(ImportProfile {
        id: format!("instance:{folder}"),
        name,
        minecraft,
        loader,
        loader_version,
        unsupported,
        without_optifine: false,
        own_folder: true,
        mod_files: count_mods(&game_dir.join("mods")),
        last_used_ms,
        imported_as: None,
        game_dir,
        installed_ms: None,
    })
}

/// Qt `QSettings` INI text, sections flattened (Prism writes `[General]`).
pub(crate) fn parse_ini(text: &str) -> HashMap<String, String> {
    let mut values = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty()
            || line.starts_with(';')
            || line.starts_with('#')
            || line.starts_with('[')
        {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            values.insert(key.trim().to_owned(), unquote(value.trim()));
        }
    }
    values
}

fn unquote(value: &str) -> String {
    let inner = value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or(value);
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
            Some('r') | None => {}
            Some(other) => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qt_ini() {
        let cfg = parse_ini(
            "[General]\nInstanceType=OneSix\nname=\"Ejder \\\"Paketi\\\"\"\nlastLaunchTime=1714564800000\n",
        );
        assert_eq!(cfg["name"], "Ejder \"Paketi\"");
        assert_eq!(cfg["lastLaunchTime"], "1714564800000");
    }
}
