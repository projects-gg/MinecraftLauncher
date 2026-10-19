//! CurseForge: `curseforge/minecraft/Instances/<name>/minecraftinstance.json` with the game
//! folder being the instance folder itself.

use std::path::Path;

use pl_mc::loaders::LoaderKind;
use serde::Deserialize;

use super::{count_mods, mark_imported, modified_ms, read_small, sort_profiles, timestamp_ms};
use crate::journal::ImportedFrom;
use crate::locate::Locations;
use crate::model::{ImportProfile, ImportSource, SourceKind, Unsupported};
use crate::version_json::parse_loader_id;

/// The instance file lists every installed add-on with its file data; big packs reach a few
/// megabytes.
const MAX_INSTANCE_JSON: u64 = 64 * 1024 * 1024;

pub(super) fn detect(locations: &Locations, imported: &[ImportedFrom]) -> Option<ImportSource> {
    let root = locations.curseforge_instances()?;
    let entries = std::fs::read_dir(&root).ok()?;
    let mut profiles = Vec::new();
    for entry in entries.flatten() {
        if !entry.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        if let Some(profile) = read_instance(&entry.path()) {
            profiles.push(profile);
        }
    }
    if profiles.is_empty() {
        return None;
    }
    mark_imported(&root, &mut profiles, imported);
    sort_profiles(&mut profiles);
    Some(ImportSource {
        id: "curseforge".to_owned(),
        kind: SourceKind::CurseForge,
        path: root.display().to_string(),
        shared: None,
        last_used_ms: profiles.iter().filter_map(|p| p.last_used_ms).max(),
        profiles,
        offline_name: None,
        root,
    })
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct CfInstance {
    name: String,
    game_version: String,
    base_mod_loader: Option<CfLoader>,
    last_played: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct CfLoader {
    name: String,
    minecraft_version: String,
}

fn read_instance(dir: &Path) -> Option<ImportProfile> {
    let file = dir.join("minecraftinstance.json");
    let text = read_small(&file, MAX_INSTANCE_JSON)?;
    let raw: CfInstance = serde_json::from_str(&text).ok()?;
    let folder = dir.file_name()?.to_string_lossy().into_owned();
    let minecraft = [
        raw.game_version.trim(),
        raw.base_mod_loader
            .as_ref()
            .map_or("", |l| l.minecraft_version.trim()),
    ]
    .into_iter()
    .find(|v| !v.is_empty())
    .map(str::to_owned);
    let (loader, loader_version, unsupported) = match raw.base_mod_loader.as_ref() {
        None => (LoaderKind::Vanilla, None, None),
        Some(base) if base.name.trim().is_empty() => (LoaderKind::Vanilla, None, None),
        Some(base) => match parse_loader_id(&base.name, minecraft.as_deref().unwrap_or("")) {
            Some((kind, version)) => (kind, Some(version), None),
            None => (
                LoaderKind::Vanilla,
                None,
                Some(Unsupported::Loader {
                    name: base.name.split('-').next().unwrap_or_default().to_owned(),
                }),
            ),
        },
    };
    let unsupported = unsupported.or(minecraft.is_none().then_some(Unsupported::UnknownVersion));
    let name = match raw.name.trim() {
        "" => folder.clone(),
        own => own.to_owned(),
    };
    Some(ImportProfile {
        id: format!("instance:{folder}"),
        name,
        minecraft,
        loader,
        loader_version,
        unsupported,
        without_optifine: false,
        own_folder: true,
        mod_files: count_mods(&dir.join("mods")),
        last_used_ms: raw
            .last_played
            .as_deref()
            .and_then(timestamp_ms)
            .or_else(|| modified_ms(&dir.join("options.txt"))),
        imported_as: None,
        game_dir: dir.to_path_buf(),
        installed_ms: None,
    })
}
