//! The Modrinth App: one folder per profile under `ModrinthApp/profiles/` (older app:
//! `com.modrinth.theseus/profiles/`). The current app keeps each profile's version and loader in
//! its own database; the older one wrote `profile.json`. Without that file the profile's game log
//! names them (`Loading Minecraft 1.21.1 with Fabric Loader 0.16.5`, or Forge's launch
//! arguments), which every profile played once has.

use std::io::Read;
use std::path::Path;
use std::sync::LazyLock;

use pl_mc::loaders::LoaderKind;
use regex::Regex;
use serde::Deserialize;

use super::{count_mods, mark_imported, modified_ms, read_small, sort_profiles, timestamp_ms};
use crate::journal::ImportedFrom;
use crate::locate::Locations;
use crate::model::{ImportProfile, ImportSource, SourceKind, Unsupported};

const MAX_PROFILE_JSON: u64 = 4 * 1024 * 1024;
/// The loader names its versions in the first lines of the log.
const LOG_HEAD_BYTES: u64 = 256 * 1024;

pub(super) fn detect(locations: &Locations, imported: &[ImportedFrom]) -> Vec<ImportSource> {
    let mut out = Vec::new();
    for (index, root) in locations.modrinth_profiles().into_iter().enumerate() {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        let mut profiles: Vec<ImportProfile> = entries
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
            .filter_map(|e| read_profile(&e.path()))
            .collect();
        if profiles.is_empty() {
            continue;
        }
        mark_imported(&root, &mut profiles, imported);
        sort_profiles(&mut profiles);
        out.push(ImportSource {
            id: if index == 0 {
                "modrinth".to_owned()
            } else {
                "modrinth-old".to_owned()
            },
            kind: SourceKind::Modrinth,
            path: root.display().to_string(),
            shared: None,
            last_used_ms: profiles.iter().filter_map(|p| p.last_used_ms).max(),
            profiles,
            offline_name: None,
            root,
        });
    }
    out
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct LegacyProfile {
    metadata: LegacyMetadata,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct LegacyMetadata {
    name: String,
    game_version: String,
    loader: String,
    loader_version: Option<LegacyLoaderVersion>,
    last_played: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct LegacyLoaderVersion {
    id: String,
}

fn read_profile(dir: &Path) -> Option<ImportProfile> {
    let folder = dir.file_name()?.to_string_lossy().into_owned();
    let legacy = read_small(&dir.join("profile.json"), MAX_PROFILE_JSON)
        .and_then(|text| serde_json::from_str::<LegacyProfile>(&text).ok())
        .map(|p| p.metadata);
    let (minecraft, loader, loader_version, name, last_played) = match legacy {
        Some(meta) if !meta.game_version.trim().is_empty() => {
            let loader = LoaderKind::from_modrinth_name(&meta.loader);
            (
                Some(meta.game_version.trim().to_owned()),
                loader,
                meta.loader_version
                    .map(|v| v.id)
                    .filter(|v| !v.trim().is_empty()),
                Some(meta.name).filter(|n| !n.trim().is_empty()),
                meta.last_played.as_deref().and_then(timestamp_ms),
            )
        }
        _ => {
            let from_log = version_from_log(&dir.join("logs").join("latest.log"));
            match from_log {
                Some((mc, loader, version)) => (Some(mc), Some(loader), version, None, None),
                None => (None, Some(LoaderKind::Vanilla), None, None, None),
            }
        }
    };
    let unsupported = match (loader, minecraft.as_ref()) {
        (None, _) => Some(Unsupported::Loader {
            name: String::new(),
        }),
        (_, None) => Some(Unsupported::UnknownVersion),
        _ => None,
    };
    // Profiles never played and without a version on file have nothing worth bringing but
    // their mods, which cannot be placed without a version: leave them out entirely.
    let mods = count_mods(&dir.join("mods"));
    let saves = crate::worlds::count_worlds(&dir.join("saves"));
    if minecraft.is_none() && mods == 0 && saves == 0 {
        return None;
    }
    Some(ImportProfile {
        id: format!("instance:{folder}"),
        name: name.unwrap_or_else(|| folder.clone()),
        minecraft,
        loader: loader.unwrap_or(LoaderKind::Vanilla),
        loader_version,
        unsupported,
        without_optifine: false,
        own_folder: true,
        mod_files: mods,
        last_used_ms: last_played
            .or_else(|| modified_ms(&dir.join("logs").join("latest.log")))
            .or_else(|| modified_ms(&dir.join("options.txt"))),
        imported_as: None,
        game_dir: dir.to_path_buf(),
        installed_ms: None,
    })
}

static FABRIC_LINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"Loading Minecraft (\S+) with (Fabric|Quilt) Loader (\S+)").expect("valid regex")
});
static FML_ARG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"--fml\.(mcVersion|forgeVersion|neoForgeVersion), ([^,\]\s]+)")
        .expect("valid regex")
});

/// Minecraft version, loader and loader version from the head of a game log.
pub(crate) fn version_from_log(path: &Path) -> Option<(String, LoaderKind, Option<String>)> {
    let file = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(LOG_HEAD_BYTES).read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    parse_log_head(&text)
}

pub(crate) fn parse_log_head(text: &str) -> Option<(String, LoaderKind, Option<String>)> {
    if let Some(caps) = FABRIC_LINE.captures(text) {
        let loader = if &caps[2] == "Quilt" {
            LoaderKind::Quilt
        } else {
            LoaderKind::Fabric
        };
        return Some((caps[1].to_owned(), loader, Some(caps[3].to_owned())));
    }
    let mut minecraft = None;
    let mut loader = None;
    for caps in FML_ARG.captures_iter(text) {
        let value = caps[2].to_owned();
        match &caps[1] {
            "mcVersion" => minecraft = Some(value),
            "neoForgeVersion" => loader = Some((LoaderKind::NeoForge, value)),
            "forgeVersion" if loader.is_none() => loader = Some((LoaderKind::Forge, value)),
            _ => {}
        }
    }
    let (kind, version) = loader?;
    Some((minecraft?, kind, Some(version)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logs_name_the_version_and_loader() {
        assert_eq!(
            parse_log_head(
                "[12:00:01] [main/INFO]: Loading Minecraft 1.21.1 with Fabric Loader 0.16.5\n"
            ),
            Some(("1.21.1".into(), LoaderKind::Fabric, Some("0.16.5".into())))
        );
        let forge = "[main/INFO] [cpw.mods.modlauncher.Launcher/MODLAUNCHER]: ModLauncher running: args [--username, Ejder, --version, 1.20.1, --gameDir, C:\\x, --accessToken, ❄❄❄❄❄❄❄❄, --launchTarget, forgeclient, --fml.forgeVersion, 47.3.0, --fml.mcVersion, 1.20.1, --fml.forgeGroup, net.minecraftforge]";
        assert_eq!(
            parse_log_head(forge),
            Some(("1.20.1".into(), LoaderKind::Forge, Some("47.3.0".into())))
        );
        let neo = "args [--fml.neoForgeVersion, 21.1.77, --fml.fmlVersion, 4.0.31, --fml.mcVersion, 1.21.1]";
        assert_eq!(
            parse_log_head(neo),
            Some((
                "1.21.1".into(),
                LoaderKind::NeoForge,
                Some("21.1.77".into())
            ))
        );
        assert_eq!(parse_log_head("[main/INFO]: Setting user: Ejder"), None);
    }
}
