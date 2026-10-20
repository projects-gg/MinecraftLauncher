//! CurseForge: `<Minecraft folder>/Instances/<name>/minecraftinstance.json`, the game folder being
//! the instance folder itself. The Minecraft folder is `curseforge/minecraft` in the home (Windows)
//! or Documents folder (macOS, Linux) unless the player moved it in the app's settings; the app's
//! own list of its instances ([`Locations::curseforge_instance_lists`]) names every instance's
//! folder, wherever it is. All places become one source: the player knows one CurseForge.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use pl_mc::loaders::LoaderKind;
use serde::Deserialize;

use super::{
    count_mods, folder_key, mark_imported, modified_ms, read_small, sort_profiles, timestamp_ms,
};
use crate::journal::{ImportedFrom, path_key, same_path};
use crate::locate::Locations;
use crate::model::{ImportProfile, ImportSource, SourceKind, Unsupported};
use crate::version_json::parse_loader_id;

/// The instance file lists every installed add-on with its file data; big packs reach a few
/// megabytes.
const MAX_INSTANCE_JSON: u64 = 64 * 1024 * 1024;
/// The app's list holds every instance's file data: a few of those.
const MAX_INSTANCE_LIST: u64 = 256 * 1024 * 1024;

pub(super) fn detect(locations: &Locations, imported: &[ImportedFrom]) -> Option<ImportSource> {
    // Where the app keeps its instances now first: an instance folder left in an older place
    // under the same name is that one's copy.
    let mut places = listed_places(locations);
    if let Some(default) = locations.curseforge_instances()
        && !places.iter().any(|place| same_path(place, &default))
    {
        places.push(default);
    }
    let mut main = None;
    let mut profiles = Vec::new();
    let mut seen = HashSet::new();
    for place in places {
        let Ok(entries) = std::fs::read_dir(&place) else {
            continue;
        };
        let mut found = false;
        for entry in entries.flatten() {
            if !entry.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            let Some(profile) = read_instance(&entry.path()) else {
                continue;
            };
            found = true;
            if seen.insert(folder_key(&profile.id)) {
                profiles.push(profile);
            }
        }
        if found && main.is_none() {
            main = Some(place);
        }
    }
    let root = main?;
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
#[serde(default)]
struct ListedInstance {
    #[serde(rename = "installPath")]
    install_path: Option<String>,
}

/// The folders the app's instance lists put instances in, the one holding most first.
fn listed_places(locations: &Locations) -> Vec<PathBuf> {
    let mut places: Vec<(PathBuf, usize)> = Vec::new();
    for file in locations.curseforge_instance_lists() {
        let Some(text) = read_small(&file, MAX_INSTANCE_LIST) else {
            continue;
        };
        let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
        let listed: Vec<ListedInstance> = match serde_json::from_str(text) {
            Ok(listed) => listed,
            Err(error) => {
                tracing::debug!(%error, file = %file.display(), "could not read CurseForge's instance list");
                continue;
            }
        };
        for path in listed.into_iter().filter_map(|i| i.install_path) {
            let path = PathBuf::from(path.trim().trim_end_matches(['/', '\\']));
            let Some(parent) = path.parent().filter(|p| p.is_absolute()) else {
                continue;
            };
            match places
                .iter_mut()
                .find(|(place, _)| same_path(place, parent))
            {
                Some((_, count)) => *count += 1,
                None => places.push((parent.to_path_buf(), 1)),
            }
        }
    }
    places.retain(|(place, _)| place.is_dir());
    places.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| path_key(&a.0).cmp(&path_key(&b.0)))
    });
    places.into_iter().map(|(place, _)| place).collect()
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct CfInstance {
    name: String,
    game_version: String,
    base_mod_loader: Option<CfLoader>,
    last_played: Option<String>,
    install_date: Option<String>,
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
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let raw: CfInstance = serde_json::from_str(text).ok()?;
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
        installed_ms: raw.install_date.as_deref().and_then(timestamp_ms),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locate::Os;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn instance(dir: &Path, name: &str, loader: &str, last_played: &str) {
        write(
            &dir.join("minecraftinstance.json"),
            &serde_json::json!({
                "name": name,
                "gameVersion": "1.20.1",
                "baseModLoader": {"name": loader, "minecraftVersion": "1.20.1"},
                "lastPlayed": last_played,
                "installPath": format!("{}\\", dir.display()),
            })
            .to_string(),
        );
    }

    #[test]
    fn a_moved_minecraft_folder_is_found_through_the_apps_list() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        // The player moved CurseForge's Minecraft folder to another drive.
        let moved = home.join("D/CurseForge/Instances");
        instance(
            &moved.join("ATM9"),
            "All the Mods 9",
            "forge-47.2.0",
            "2026-09-01T10:00:00Z",
        );
        instance(
            &moved.join("Vault Hunters"),
            "Vault Hunters",
            "forge-47.1.3",
            "2026-08-01T10:00:00Z",
        );
        // A copy left in the default place under the same folder name, and one only there.
        let default = home.join("curseforge/minecraft/Instances");
        instance(
            &default.join("ATM9"),
            "All the Mods 9 (old)",
            "forge-47.1.0",
            "2025-01-01T10:00:00Z",
        );
        instance(
            &default.join("Better MC"),
            "Better MC",
            "fabric-0.15.0",
            "2026-07-01T10:00:00Z",
        );
        // The standalone app's list (with a byte-order mark, as some writers add one), the old
        // Overwolf app's list naming the same folders.
        let entry = |path: &Path| serde_json::json!({"name": "x", "installPath": format!("{}\\", path.display()), "installedAddons": [{"addonID": 1}]});
        let list = serde_json::json!([
            entry(&moved.join("ATM9")),
            entry(&moved.join("Vault Hunters"))
        ]);
        write(
            &home.join("AppData/Roaming/CurseForge/agent/GameInstances/MinecraftGameInstance.json"),
            &format!("\u{feff}{list}"),
        );
        write(
            &home.join("AppData/Local/Overwolf/Curse/GameInstances/MinecraftGameInstance.json"),
            &list.to_string(),
        );

        let source = detect(&Locations::under(home, Os::Windows), &[]).expect("found");
        assert!(
            same_path(&source.root, &moved),
            "the app's place is the source's"
        );
        let rows: Vec<(&str, &str)> = source
            .profiles
            .iter()
            .map(|p| (p.name.as_str(), p.loader_version.as_deref().unwrap_or("")))
            .collect();
        assert_eq!(
            rows,
            [
                ("All the Mods 9", "47.2.0"),
                ("Vault Hunters", "47.1.3"),
                ("Better MC", "0.15.0"),
            ]
        );
        assert!(source.profiles[0].game_dir.starts_with(&moved));
        assert!(source.profiles[2].game_dir.starts_with(&default));
    }

    #[test]
    fn without_the_apps_list_the_default_place_is_read() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let default = home.join("curseforge/minecraft/Instances");
        instance(
            &default.join("ATM9"),
            "All the Mods 9",
            "forge-47.2.0",
            "2026-09-01T10:00:00Z",
        );
        // A list that is not one is ignored.
        write(
            &home.join("AppData/Roaming/CurseForge/agent/GameInstances/MinecraftGameInstance.json"),
            "{ not json",
        );
        let source = detect(&Locations::under(home, Os::Windows), &[]).expect("found");
        assert!(same_path(&source.root, &default));
        assert_eq!(source.profiles.len(), 1);
        // Linux has the app too, in Documents.
        let linux = tempfile::tempdir().unwrap();
        instance(
            &linux
                .path()
                .join("Documents/curseforge/minecraft/Instances/Pack"),
            "Pack",
            "neoforge-1.20.1-47.1.84",
            "2026-09-01T10:00:00Z",
        );
        let source = detect(&Locations::under(linux.path(), Os::Linux), &[]).expect("found");
        assert_eq!(source.profiles[0].loader, LoaderKind::NeoForge);
        assert_eq!(
            source.profiles[0].loader_version.as_deref(),
            Some("47.1.84")
        );
        assert!(
            detect(
                &Locations::under(tempfile::tempdir().unwrap().path(), Os::Windows),
                &[]
            )
            .is_none()
        );
    }
}
