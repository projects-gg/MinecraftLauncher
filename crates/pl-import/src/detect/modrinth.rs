//! The Modrinth App. The current app keeps every profile in its database
//! ([`super::modrinth_db`]): its name, Minecraft version, loader and when it was last played. The
//! profiles are folders under `profiles/` in the app's folder (`ModrinthApp`), which the player
//! can move anywhere in the app's settings; the database says where.
//!
//! The app before 2024 (`com.modrinth.theseus`) wrote `profile.json` into each profile folder and
//! kept its own folder setting. A folder that neither describes names its version in its game log
//! (`Loading Minecraft 1.21.1 with Fabric Loader 0.16.5`, or Forge's launch arguments), which
//! every profile played once has.

use std::collections::HashSet;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use pl_mc::loaders::LoaderKind;
use regex::Regex;
use serde::Deserialize;

use super::modrinth_db::{self, DbProfile};
use super::{
    count_mods, folder_key, mark_imported, modified_ms, read_small, sort_profiles, timestamp_ms,
};
use crate::journal::{ImportedFrom, same_path};
use crate::locate::Locations;
use crate::model::{ImportProfile, ImportSource, SourceKind, Unsupported};

const MAX_PROFILE_JSON: u64 = 4 * 1024 * 1024;
const MAX_SETTINGS_JSON: u64 = 4 * 1024 * 1024;
/// The loader names its versions in the first lines of the log.
const LOG_HEAD_BYTES: u64 = 256 * 1024;

pub(super) fn detect(locations: &Locations, imported: &[ImportedFrom]) -> Vec<ImportSource> {
    let mut out = Vec::new();
    // Profile folders the current app's database lists: the older app's copies of them (the
    // current app took them over) are not offered a second time.
    let mut known = HashSet::new();
    let mut current_root = None;
    if let Some(app) = locations.modrinth_app() {
        let db = modrinth_db::read(&app).unwrap_or_default();
        let default_root = app.join("profiles");
        let root = db
            .profiles_root
            .clone()
            .unwrap_or_else(|| default_root.clone());
        let mut profiles = Vec::new();
        for row in &db.profiles {
            known.insert(folder_key(&row.path));
            // A move the app did not finish leaves a profile in the default place.
            let Some(dir) = [&root, &default_root]
                .into_iter()
                .map(|parent| parent.join(&row.path))
                .find(|dir| dir.is_dir())
            else {
                continue;
            };
            profiles.extend(profile_from_db(&dir, row));
        }
        profiles.extend(folder_profiles(&root, &known));
        push_source(&mut out, "modrinth", root.clone(), profiles, imported);
        current_root = Some(root);
    }
    if let Some(old_app) = locations.modrinth_old_app() {
        let root = old_profiles_root(&old_app);
        if !current_root
            .as_ref()
            .is_some_and(|current| same_path(current, &root))
        {
            let profiles = folder_profiles(&root, &known);
            push_source(&mut out, "modrinth-old", root, profiles, imported);
        }
    }
    out
}

fn push_source(
    out: &mut Vec<ImportSource>,
    id: &str,
    root: PathBuf,
    mut profiles: Vec<ImportProfile>,
    imported: &[ImportedFrom],
) {
    if profiles.is_empty() {
        return;
    }
    mark_imported(&root, &mut profiles, imported);
    sort_profiles(&mut profiles);
    out.push(ImportSource {
        id: id.to_owned(),
        kind: SourceKind::Modrinth,
        path: root.display().to_string(),
        shared: None,
        last_used_ms: profiles.iter().filter_map(|p| p.last_used_ms).max(),
        profiles,
        offline_name: None,
        root,
    });
}

/// The profile folders under `root` that `known` does not hold, read from their own clues.
fn folder_profiles(root: &Path, known: &HashSet<String>) -> Vec<ImportProfile> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            !name.starts_with('.') && !known.contains(&folder_key(&name))
        })
        .filter_map(|e| read_profile(&e.path()))
        .collect()
}

/// The older app's profiles: `profiles/` in its folder, or in the folder its settings name.
fn old_profiles_root(app: &Path) -> PathBuf {
    #[derive(Debug, Default, Deserialize)]
    #[serde(default)]
    struct OldSettings {
        loaded_config_dir: Option<String>,
    }
    read_small(&app.join("settings.json"), MAX_SETTINGS_JSON)
        .and_then(|text| serde_json::from_str::<OldSettings>(&text).ok())
        .and_then(|settings| settings.loaded_config_dir)
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .unwrap_or_else(|| app.to_path_buf())
        .join("profiles")
}

/// The loader's own version. Modrinth keeps Forge's (and NeoForge's for 1.20.1) with the
/// Minecraft version in front: `1.20.1-47.3.0` → `47.3.0`.
fn loader_version(raw: &str, minecraft: &str) -> Option<String> {
    let raw = raw.trim();
    let version = raw
        .strip_prefix(minecraft)
        .and_then(|rest| rest.strip_prefix('-'))
        .unwrap_or(raw);
    (!version.is_empty()).then(|| version.to_owned())
}

/// When the profile was last used, from its own files.
fn last_activity(dir: &Path) -> Option<i64> {
    modified_ms(&dir.join("logs").join("latest.log"))
        .or_else(|| modified_ms(&dir.join("options.txt")))
}

/// A profile the database describes.
fn profile_from_db(dir: &Path, row: &DbProfile) -> Option<ImportProfile> {
    let Some(minecraft) = row.game_version.clone() else {
        // Its version is not on file: the folder's own clues, under the player's name for it.
        return read_profile(dir).map(|profile| ImportProfile {
            name: row.name.clone().unwrap_or(profile.name.clone()),
            ..profile
        });
    };
    let loader_name = row.loader.as_deref().unwrap_or_default();
    let loader = LoaderKind::from_modrinth_name(loader_name);
    let loader_version = loader
        .filter(|kind| *kind != LoaderKind::Vanilla)
        .and(row.loader_version.as_deref())
        .and_then(|raw| loader_version(raw, &minecraft));
    Some(ImportProfile {
        id: format!("instance:{}", row.path),
        name: row.name.clone().unwrap_or_else(|| row.path.clone()),
        minecraft: Some(minecraft),
        loader: loader.unwrap_or(LoaderKind::Vanilla),
        loader_version,
        unsupported: loader.is_none().then(|| Unsupported::Loader {
            name: loader_name.to_owned(),
        }),
        without_optifine: false,
        own_folder: true,
        mod_files: count_mods(&dir.join("mods")),
        last_used_ms: row.last_played_ms.or_else(|| last_activity(dir)),
        imported_as: None,
        game_dir: dir.to_path_buf(),
        installed_ms: row.created_ms,
    })
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
    date_created: Option<String>,
    last_played: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct LegacyLoaderVersion {
    id: String,
}

/// A profile folder from its own clues: the older app's `profile.json`, else its game log.
fn read_profile(dir: &Path) -> Option<ImportProfile> {
    let folder = dir.file_name()?.to_string_lossy().into_owned();
    let legacy = read_small(&dir.join("profile.json"), MAX_PROFILE_JSON)
        .and_then(|text| serde_json::from_str::<LegacyProfile>(&text).ok())
        .map(|p| p.metadata);
    let (minecraft, loader, loader_version, name, last_played, created) = match legacy {
        Some(meta) if !meta.game_version.trim().is_empty() => {
            let minecraft = meta.game_version.trim().to_owned();
            let loader = LoaderKind::from_modrinth_name(&meta.loader);
            let version = meta
                .loader_version
                .and_then(|v| loader_version(&v.id, &minecraft));
            (
                Some(minecraft),
                loader,
                version,
                Some(meta.name).filter(|n| !n.trim().is_empty()),
                meta.last_played.as_deref().and_then(timestamp_ms),
                meta.date_created.as_deref().and_then(timestamp_ms),
            )
        }
        _ => {
            let from_log = version_from_log(&dir.join("logs").join("latest.log"));
            match from_log {
                Some((mc, loader, version)) => (Some(mc), Some(loader), version, None, None, None),
                None => (None, Some(LoaderKind::Vanilla), None, None, None, None),
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
        last_used_ms: last_played.or_else(|| last_activity(dir)),
        imported_as: None,
        game_dir: dir.to_path_buf(),
        installed_ms: created,
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
    use crate::detect::modrinth_db::fixtures;
    use crate::locate::Os;

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

    #[test]
    fn loader_versions_lose_the_minecraft_prefix() {
        assert_eq!(
            loader_version("1.20.1-47.3.0", "1.20.1").as_deref(),
            Some("47.3.0")
        );
        assert_eq!(
            loader_version("0.16.5", "1.21.1").as_deref(),
            Some("0.16.5")
        );
        assert_eq!(
            loader_version("21.1.77", "1.21.1").as_deref(),
            Some("21.1.77")
        );
        assert_eq!(loader_version(" ", "1.21.1"), None);
    }

    fn write(path: &Path, bytes: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    const PROFILES: &[fixtures::Profile<'static>] = &[
        (
            "Fabulously Optimized",
            "Fabulously Optimized",
            "1.21.1",
            "fabric",
            Some("0.16.5"),
            Some(1_790_000_000),
        ),
        (
            "Create",
            "Create: Above and Beyond",
            "1.20.1",
            "forge",
            Some("1.20.1-47.3.0"),
            None,
        ),
        ("Gone", "Deleted meanwhile", "1.21.1", "vanilla", None, None),
    ];

    #[test]
    fn the_database_names_profiles_wherever_the_player_moved_them() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let app = home.join("AppData/Roaming/ModrinthApp");
        std::fs::create_dir_all(&app).unwrap();
        let moved = home.join("Oyunlar/Modrinth");
        let profiles = moved.join("profiles");
        write(
            &profiles.join("Fabulously Optimized/options.txt"),
            "lang:tr_tr\n",
        );
        write(&profiles.join("Create/mods/create.jar"), "not a jar");
        // A folder the database does not list still comes with its own clues (last played in
        // 2025, by its log).
        let log = profiles.join("Eski/logs/latest.log");
        write(
            &log,
            "[main/INFO]: Loading Minecraft 1.20.4 with Quilt Loader 0.26.0\n",
        );
        std::fs::File::options()
            .write(true)
            .open(&log)
            .unwrap()
            .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_750_000_000))
            .unwrap();
        let _live = fixtures::current(&app.join("app.db"), Some(moved.to_str().unwrap()), PROFILES);

        let sources = detect(&Locations::under(home, Os::Windows), &[]);
        assert_eq!(sources.len(), 1);
        let source = &sources[0];
        assert_eq!(source.id, "modrinth");
        assert!(same_path(&source.root, &profiles));
        type Row<'a> = (
            &'a str,
            &'a str,
            Option<&'a str>,
            LoaderKind,
            Option<&'a str>,
        );
        let rows: Vec<Row<'_>> = source
            .profiles
            .iter()
            .map(|p| {
                (
                    p.id.as_str(),
                    p.name.as_str(),
                    p.minecraft.as_deref(),
                    p.loader,
                    p.loader_version.as_deref(),
                )
            })
            .collect();
        // Most recently played first; the ones never played by when they were made.
        assert_eq!(
            rows,
            [
                (
                    "instance:Fabulously Optimized",
                    "Fabulously Optimized",
                    Some("1.21.1"),
                    LoaderKind::Fabric,
                    Some("0.16.5")
                ),
                (
                    "instance:Eski",
                    "Eski",
                    Some("1.20.4"),
                    LoaderKind::Quilt,
                    Some("0.26.0")
                ),
                (
                    "instance:Create",
                    "Create: Above and Beyond",
                    Some("1.20.1"),
                    LoaderKind::Forge,
                    Some("47.3.0")
                ),
            ]
        );
        assert_eq!(source.profiles[0].last_used_ms, Some(1_790_000_000_000));
        assert_eq!(source.profiles[2].mod_files, 1);
        assert!(source.profiles.iter().all(|p| p.own_folder));
    }

    #[test]
    fn the_older_app_brings_only_what_the_current_one_did_not_take_over() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let app = home.join("AppData/Roaming/ModrinthApp");
        std::fs::create_dir_all(&app).unwrap();
        write(&app.join("profiles/Create/options.txt"), "lang:tr_tr\n");
        fixtures::legacy(&app.join("app.db"), None, &PROFILES[1..2]);
        // The older app kept its profiles somewhere else, its settings say.
        let old_app = home.join("AppData/Roaming/com.modrinth.theseus");
        let old_place = home.join("Modrinth eski");
        write(
            &old_app.join("settings.json"),
            &serde_json::json!({"theme": "dark", "loaded_config_dir": old_place}).to_string(),
        );
        // Taken over by the current app (listed in its database): not offered again.
        write(
            &old_place.join("profiles/Create/profile.json"),
            r#"{"metadata":{"name":"Create","game_version":"1.20.1","loader":"forge","loader_version":{"id":"1.20.1-47.1.0"}}}"#,
        );
        // Only the older app knows this one.
        write(
            &old_place.join("profiles/Survival/profile.json"),
            r#"{"metadata":{"name":"Hayatta Kalma","game_version":"1.19.2","loader":"forge","loader_version":{"id":"1.19.2-43.3.0"},"date_created":"2023-01-01T00:00:00Z"}}"#,
        );

        let sources = detect(&Locations::under(home, Os::Windows), &[]);
        let ids: Vec<&str> = sources.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["modrinth", "modrinth-old"]);
        assert_eq!(sources[0].profiles.len(), 1);
        assert_eq!(
            sources[0].profiles[0].loader_version.as_deref(),
            Some("47.3.0")
        );
        let old = &sources[1].profiles;
        assert_eq!(old.len(), 1);
        assert_eq!(old[0].name, "Hayatta Kalma");
        assert_eq!(old[0].loader_version.as_deref(), Some("43.3.0"));
        assert_eq!(old[0].installed_ms, Some(1_672_531_200_000));
    }

    #[test]
    fn without_a_readable_database_the_folders_still_speak() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let profiles = home.join("AppData/Roaming/ModrinthApp/profiles");
        write(
            &profiles.join("Vanilla Plus/logs/latest.log"),
            "[main/INFO]: Loading Minecraft 1.21.1 with Fabric Loader 0.16.5\n",
        );
        // Not a database at all.
        write(
            &home.join("AppData/Roaming/ModrinthApp/app.db"),
            &"x".repeat(4096),
        );
        let sources = detect(&Locations::under(home, Os::Windows), &[]);
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].profiles[0].minecraft.as_deref(), Some("1.21.1"));
    }
}
