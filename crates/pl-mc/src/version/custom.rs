//! Versions other installers put in the data folder (CUSTOM-VER): OptiFine's installer, the Forge,
//! NeoForge, Fabric and Quilt installers, a modpack's own version. They sit where the official
//! launcher keeps them (`versions/<id>/<id>.json`, usually `inheritsFrom` a Mojang version) and
//! launch through the same [`resolve_version`](crate::version::resolve_version) as the launcher's
//! own loader profiles; an instance made from one keeps its id ([`Instance::custom_version`]).
//!
//! [`scan`] lists them: every version folder whose JSON reads, minus Mojang's versions, the
//! launcher's own loader profiles (their mark, [`crate::loaders::launcher_wrote`]) and the ids the
//! caller knows the launcher runs through its own loader path. A version whose chain does not reach
//! a Mojang version is left out: the instance's game version is what mod compatibility, the
//! per-version mod sets and Java follow.
//!
//! Files such an installer put in place that no server offers (OptiFine's jars: libraries named by
//! their coordinate alone, without `url` or `downloads`) are its [`installer_files`]. A launch
//! looks for them before it downloads anything: one that is missing (and that Mojang's library
//! server does not have either) means "install the version again", not a failed download.
//!
//! [`Instance::custom_version`]: crate::instance::Instance::custom_version

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::SystemTime;

use crate::error::Result;
use crate::loaders::{LoaderKind, launcher_wrote, mark_profile_blocking, profile_of};
use crate::paths::DataDirs;
use crate::platform::Platform;
use crate::version::manifest::VersionManifest;
use crate::version::model::{Library, VersionJson};

/// Deepest `inheritsFrom` chain followed (as [`crate::version::resolve_version`]).
const MAX_CHAIN: usize = 16;

/// What a custom version is, for its line in the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum CustomKind {
    OptiFine,
    Fabric,
    Quilt,
    Forge,
    NeoForge,
    /// Nothing the launcher knows (a modpack's own version, a client of its own).
    Other,
}

/// A version another installer wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomVersion {
    pub id: String,
    /// The Mojang version its chain reaches: the game version of an instance made from it.
    pub minecraft: String,
    /// The mod loader its libraries carry (`Vanilla` for none, OptiFine's too).
    pub loader: LoaderKind,
    pub loader_version: Option<String>,
    pub kind: CustomKind,
    /// When its JSON was last written (the list shows the newest first).
    pub modified: Option<SystemTime>,
}

/// The mod loader (and its version) the libraries of a version carry, and what kind of version
/// they make: Fabric, Quilt, Forge, NeoForge by their loader's library, OptiFine by its own.
pub fn detect(libraries: &[Library]) -> (LoaderKind, Option<String>, CustomKind) {
    let mut optifine = false;
    for library in libraries {
        let Ok(coord) = library.coord() else {
            continue;
        };
        let version = coord.version.trim();
        let found = match (coord.group.as_str(), coord.artifact.as_str()) {
            ("net.fabricmc", "fabric-loader") => Some((LoaderKind::Fabric, version.to_owned())),
            ("org.quiltmc", "quilt-loader") => Some((LoaderKind::Quilt, version.to_owned())),
            ("net.neoforged", "neoforge") => Some((LoaderKind::NeoForge, version.to_owned())),
            // NeoForge on 1.20.1 kept Forge's name: `1.20.1-47.1.106`.
            ("net.neoforged", "forge") => Some((LoaderKind::NeoForge, after_minecraft(version))),
            ("net.minecraftforge", "forge" | "fmlloader") => {
                Some((LoaderKind::Forge, after_minecraft(version)))
            }
            ("optifine", "OptiFine") => {
                optifine = true;
                None
            }
            _ => None,
        };
        if let Some((loader, version)) = found {
            let kind = match loader {
                LoaderKind::Fabric => CustomKind::Fabric,
                LoaderKind::Quilt => CustomKind::Quilt,
                LoaderKind::Forge => CustomKind::Forge,
                LoaderKind::NeoForge => CustomKind::NeoForge,
                LoaderKind::Vanilla => CustomKind::Other,
            };
            let version = Some(version).filter(|v| !v.is_empty());
            return (loader, version, kind);
        }
    }
    let kind = if optifine {
        CustomKind::OptiFine
    } else {
        CustomKind::Other
    };
    (LoaderKind::Vanilla, None, kind)
}

/// `1.20.1-47.2.0` → `47.2.0` (Forge's Maven versions lead with the Minecraft version; the
/// oldest end with it too: `1.7.10-10.13.4.1614-1.7.10`).
fn after_minecraft(version: &str) -> String {
    let mut parts = version.split('-');
    match (parts.next(), parts.next()) {
        (Some(first), Some(second)) if first.starts_with(|c: char| c.is_ascii_digit()) => {
            second.to_owned()
        }
        _ => version.to_owned(),
    }
}

/// `versions/<id>/<id>.json`, parsed (its id from the folder when the file has none).
fn read_json(dirs: &DataDirs, id: &str) -> Option<VersionJson> {
    let path = dirs.version_json(id).ok()?;
    let bytes = std::fs::read(&path).ok()?;
    let mut json = VersionJson::from_slice(&bytes, &path.display().to_string()).ok()?;
    if json.id.trim().is_empty() {
        json.id = id.to_owned();
    }
    Some(json)
}

/// The levels of `id`'s chain that are not Mojang's (leaf first), and the Mojang version the
/// chain reaches; `None` when it reaches none (a missing or unreadable level, a loop, a chain
/// without `inheritsFrom` that is not Mojang's).
fn own_levels(
    dirs: &DataDirs,
    id: &str,
    manifest: &VersionManifest,
) -> Option<(Vec<VersionJson>, String)> {
    let mut levels: Vec<VersionJson> = Vec::new();
    let mut next = id.trim().to_owned();
    loop {
        if manifest.find(&next).is_some() {
            return (!levels.is_empty()).then_some((levels, next));
        }
        if levels.len() >= MAX_CHAIN || levels.iter().any(|level| level.id == next) {
            return None;
        }
        let json = read_json(dirs, &next)?;
        let parent = json
            .inherits_from
            .as_deref()
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(str::to_owned);
        levels.push(json);
        next = parent?;
    }
}

/// `id` as a custom version, read from the disk: `None` when it is Mojang's, the launcher's own
/// loader profile, or its chain does not reach a Mojang version. Blocking (reads a few JSONs).
pub fn describe(dirs: &DataDirs, id: &str, manifest: &VersionManifest) -> Option<CustomVersion> {
    let id = id.trim();
    if id.is_empty() || id.starts_with('.') || launcher_wrote(dirs, id) {
        return None;
    }
    let (levels, minecraft) = own_levels(dirs, id, manifest)?;
    let libraries: Vec<Library> = levels
        .iter()
        .flat_map(|level| level.libraries.iter().cloned())
        .collect();
    let (loader, loader_version, kind) = detect(&libraries);
    let modified = dirs
        .version_json(id)
        .ok()
        .and_then(|path| std::fs::metadata(path).ok())
        .and_then(|meta| meta.modified().ok());
    Some(CustomVersion {
        id: id.to_owned(),
        minecraft,
        loader,
        loader_version,
        kind,
        modified,
    })
}

/// Every custom version in the data folder, newest first; `launcher_ids` are the versions the
/// launcher runs through its own loader path (instances' and kept sets' loader profiles), which
/// are left out like the ones it marked. Blocking (lists `versions/` and reads the JSONs).
pub fn scan(
    dirs: &DataDirs,
    manifest: &VersionManifest,
    launcher_ids: &HashSet<String>,
) -> Vec<CustomVersion> {
    let Ok(entries) = std::fs::read_dir(dirs.versions()) else {
        return Vec::new();
    };
    let mut found: Vec<CustomVersion> = entries
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
        .filter(|id| !launcher_ids.contains(id))
        .filter_map(|id| describe(dirs, &id, manifest))
        .collect();
    found.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.id.cmp(&b.id)));
    found
}

/// Written in `meta/` once [`adopt_unmarked_profiles`] ran in a data folder.
pub const ADOPTED_FILE: &str = "custom-versions-adopted";

/// The launcher marks the Fabric and Quilt profiles it writes; the ones it wrote before it did so
/// carry no mark and would read as other installers' versions. The first time the list is read
/// in a data folder, every unmarked profile named exactly as the launcher names Fabric and Quilt
/// profiles is marked as the launcher's; such profiles that come later without a mark are other
/// installers'. Forge's and NeoForge's were always marked. Blocking; returns how many it marked.
pub fn adopt_unmarked_profiles(dirs: &DataDirs) -> Result<usize> {
    let record = dirs.meta().join(ADOPTED_FILE);
    if record.exists() {
        return Ok(0);
    }
    let mut marked = 0;
    for entry in std::fs::read_dir(dirs.versions())
        .into_iter()
        .flatten()
        .filter_map(std::result::Result::ok)
    {
        let Some(id) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if id.starts_with('.') || !entry.path().is_dir() || launcher_wrote(dirs, &id) {
            continue;
        }
        let Some(json) = read_json(dirs, &id) else {
            continue;
        };
        let Some(parent) = json
            .inherits_from
            .as_deref()
            .map(str::trim)
            .filter(|p| !p.is_empty())
        else {
            continue;
        };
        if let Some((kind @ (LoaderKind::Fabric | LoaderKind::Quilt), loader)) =
            profile_of(&id, parent)
        {
            mark_profile_blocking(dirs, &id, kind, parent, &loader)?;
            marked += 1;
        }
    }
    crate::paths::write_atomic_blocking(&record, b"1\n")?;
    Ok(marked)
}

/// A file an installer put in the data folder for a version, which no server is known to offer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallerFile {
    /// Its library coordinate (`optifine:OptiFine:1.21.11_HD_U_J9`).
    pub name: String,
    pub path: PathBuf,
    /// Where a launcher would look for it otherwise (Mojang's library server, Maven Central).
    pub urls: Vec<String>,
}

/// The libraries of `id`'s own (not Mojang's) levels named by their coordinate alone (no `url`,
/// no `downloads`) that apply on `platform`: an installer put them there. Mojang's library server
/// has some (old `launchwrapper`); OptiFine's are nowhere else. Blocking.
pub fn installer_files(
    dirs: &DataDirs,
    id: &str,
    manifest: &VersionManifest,
    platform: &Platform,
) -> Result<Vec<InstallerFile>> {
    let Some((levels, _)) = own_levels(dirs, id, manifest) else {
        return Ok(Vec::new());
    };
    let mut seen = HashSet::new();
    let mut files = Vec::new();
    for library in levels.iter().flat_map(|level| level.libraries.iter()) {
        if library.downloads.is_some()
            || library.url.as_deref().is_some_and(|u| !u.trim().is_empty())
            || library.natives.is_some()
            || !library.applies_to(platform)
        {
            continue;
        }
        let Some(artifact) = library.artifact() else {
            continue;
        };
        let path = dirs.library_file(&artifact.path)?;
        if seen.insert(path.clone()) {
            files.push(InstallerFile {
                name: library.name.clone(),
                path,
                urls: artifact.urls,
            });
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{Arch, Os};

    const OPTIFINE: &str = include_str!("../../testdata/custom-versions/optifine-1.21.11.json");
    const FORGE: &str = include_str!("../../testdata/custom-versions/forge-1.21.11-61.2.5.json");
    const FABRIC: &str =
        include_str!("../../testdata/custom-versions/fabric-loader-0.20.1-1.21.11.json");

    fn manifest() -> VersionManifest {
        serde_json::from_str(
            r#"{"latest": {"release": "1.21.11", "snapshot": "1.21.11"}, "versions": [
                {"id": "1.21.11", "type": "release", "url": "https://x/1.21.11.json", "time": "2025-12-09T12:23:30+00:00", "releaseTime": "2025-12-09T12:23:30+00:00", "sha1": "a", "complianceLevel": 1},
                {"id": "1.20.1", "type": "release", "url": "https://x/1.20.1.json", "time": "2023-06-12T13:25:51+00:00", "releaseTime": "2023-06-12T13:25:51+00:00", "sha1": "b", "complianceLevel": 1}
            ]}"#,
        )
        .unwrap()
    }

    fn put(dirs: &DataDirs, id: &str, json: &str) {
        let path = dirs.version_json(id).unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, json).unwrap();
    }

    fn libraries(json: &str) -> Vec<Library> {
        serde_json::from_str::<VersionJson>(json).unwrap().libraries
    }

    #[test]
    fn real_installers_are_recognized() {
        assert_eq!(
            detect(&libraries(OPTIFINE)),
            (LoaderKind::Vanilla, None, CustomKind::OptiFine)
        );
        assert_eq!(
            detect(&libraries(FORGE)),
            (LoaderKind::Forge, Some("61.2.5".into()), CustomKind::Forge)
        );
        assert_eq!(
            detect(&libraries(FABRIC)),
            (
                LoaderKind::Fabric,
                Some("0.20.1".into()),
                CustomKind::Fabric
            )
        );
        let neo = |name: &str| {
            detect(&[Library {
                name: name.into(),
                ..Library::default()
            }])
        };
        assert_eq!(
            neo("net.neoforged:neoforge:21.11.38-beta:universal"),
            (
                LoaderKind::NeoForge,
                Some("21.11.38-beta".into()),
                CustomKind::NeoForge
            )
        );
        assert_eq!(
            neo("net.neoforged:forge:1.20.1-47.1.106"),
            (
                LoaderKind::NeoForge,
                Some("47.1.106".into()),
                CustomKind::NeoForge
            )
        );
        assert_eq!(
            neo("org.quiltmc:quilt-loader:0.30.1"),
            (LoaderKind::Quilt, Some("0.30.1".into()), CustomKind::Quilt)
        );
        assert_eq!(
            neo("net.minecraftforge:forge:1.7.10-10.13.4.1614-1.7.10"),
            (
                LoaderKind::Forge,
                Some("10.13.4.1614".into()),
                CustomKind::Forge
            )
        );
        assert_eq!(neo("com.example:client:1.0").2, CustomKind::Other);
    }

    #[test]
    fn scan_lists_what_other_installers_wrote() {
        let dir = tempfile::tempdir().unwrap();
        let dirs = DataDirs::new(dir.path());
        put(
            &dirs,
            "1.21.11",
            r#"{"id": "1.21.11", "mainClass": "net.minecraft.client.main.Main"}"#,
        );
        put(&dirs, "1.21.11-OptiFine_HD_U_J9", OPTIFINE);
        put(&dirs, "1.21.11-forge-61.2.5", FORGE);
        put(&dirs, "fabric-loader-0.20.1-1.21.11", FABRIC);
        // The launcher's own profiles: marked, or run by an instance through its loader path.
        put(
            &dirs,
            "1.20.1-forge-47.4.10",
            r#"{"id": "1.20.1-forge-47.4.10", "inheritsFrom": "1.20.1", "mainClass": "M"}"#,
        );
        std::fs::write(
            dirs.version_dir("1.20.1-forge-47.4.10")
                .unwrap()
                .join(crate::loaders::PROFILE_MARKER),
            "{}",
        )
        .unwrap();
        put(
            &dirs,
            "fabric-loader-0.16.5-1.20.1",
            r#"{"id": "fabric-loader-0.16.5-1.20.1", "inheritsFrom": "1.20.1", "mainClass": "M"}"#,
        );
        // Left out: no Mojang version at the end of the chain, a missing parent, a loop.
        put(&dirs, "MyClient", r#"{"id": "MyClient", "mainClass": "M"}"#);
        put(
            &dirs,
            "orphan",
            r#"{"id": "orphan", "inheritsFrom": "gone", "mainClass": "M"}"#,
        );
        put(
            &dirs,
            "loop-a",
            r#"{"id": "loop-a", "inheritsFrom": "loop-b", "mainClass": "M"}"#,
        );
        put(
            &dirs,
            "loop-b",
            r#"{"id": "loop-b", "inheritsFrom": "loop-a", "mainClass": "M"}"#,
        );
        std::fs::create_dir_all(dirs.versions().join("empty")).unwrap();

        let used = HashSet::from(["fabric-loader-0.16.5-1.20.1".to_owned()]);
        let mut ids: Vec<String> = scan(&dirs, &manifest(), &used)
            .into_iter()
            .map(|v| v.id)
            .collect();
        ids.sort();
        assert_eq!(
            ids,
            [
                "1.21.11-OptiFine_HD_U_J9",
                "1.21.11-forge-61.2.5",
                "fabric-loader-0.20.1-1.21.11"
            ]
        );

        let optifine = describe(&dirs, "1.21.11-OptiFine_HD_U_J9", &manifest()).unwrap();
        assert_eq!(optifine.minecraft, "1.21.11");
        assert_eq!(optifine.loader, LoaderKind::Vanilla);
        assert_eq!(optifine.kind, CustomKind::OptiFine);
        // The Fabric installer's JSON has no `id`: the folder names it.
        let fabric = describe(&dirs, "fabric-loader-0.20.1-1.21.11", &manifest()).unwrap();
        assert_eq!(fabric.loader_version.as_deref(), Some("0.20.1"));
        assert!(describe(&dirs, "1.21.11", &manifest()).is_none());
    }

    #[test]
    fn the_launchers_older_fabric_profiles_are_adopted_once() {
        let dir = tempfile::tempdir().unwrap();
        let dirs = DataDirs::new(dir.path());
        // Written by the launcher before it marked its profiles (no instance runs it now), and
        // an unmarked Forge profile, which only another installer writes.
        put(
            &dirs,
            "fabric-loader-0.16.5-1.21.11",
            r#"{"id": "fabric-loader-0.16.5-1.21.11", "inheritsFrom": "1.21.11", "mainClass": "M", "libraries": []}"#,
        );
        put(&dirs, "1.21.11-forge-61.2.5", FORGE);
        put(&dirs, "1.21.11-OptiFine_HD_U_J9", OPTIFINE);
        assert_eq!(adopt_unmarked_profiles(&dirs).unwrap(), 1);
        assert!(launcher_wrote(&dirs, "fabric-loader-0.16.5-1.21.11"));
        // Once per data folder: a Fabric installer's profile that comes later is listed.
        put(&dirs, "fabric-loader-0.20.1-1.21.11", FABRIC);
        assert_eq!(adopt_unmarked_profiles(&dirs).unwrap(), 0);
        let mut ids: Vec<String> = scan(&dirs, &manifest(), &HashSet::new())
            .into_iter()
            .map(|v| v.id)
            .collect();
        ids.sort();
        assert_eq!(
            ids,
            [
                "1.21.11-OptiFine_HD_U_J9",
                "1.21.11-forge-61.2.5",
                "fabric-loader-0.20.1-1.21.11"
            ]
        );
    }

    #[test]
    fn a_custom_version_on_a_custom_version_reaches_mojang() {
        let dir = tempfile::tempdir().unwrap();
        let dirs = DataDirs::new(dir.path());
        put(&dirs, "1.21.11-forge-61.2.5", FORGE);
        put(
            &dirs,
            "pack-2.0",
            r#"{"id": "pack-2.0", "inheritsFrom": "1.21.11-forge-61.2.5", "libraries": [{"name": "com.example:pack:2.0"}]}"#,
        );
        let pack = describe(&dirs, "pack-2.0", &manifest()).unwrap();
        assert_eq!(pack.minecraft, "1.21.11");
        assert_eq!(pack.loader, LoaderKind::Forge);
        assert_eq!(pack.kind, CustomKind::Forge);
    }

    #[test]
    fn installer_files_are_the_bare_coordinates_of_its_own_levels() {
        let dir = tempfile::tempdir().unwrap();
        let dirs = DataDirs::new(dir.path());
        put(&dirs, "1.21.11-OptiFine_HD_U_J9", OPTIFINE);
        put(&dirs, "1.21.11-forge-61.2.5", FORGE);
        let windows = Platform::new(Os::Windows, Arch::X86_64, "10.0");
        let files =
            installer_files(&dirs, "1.21.11-OptiFine_HD_U_J9", &manifest(), &windows).unwrap();
        let names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "optifine:OptiFine:1.21.11_HD_U_J9",
                "optifine:launchwrapper-of:2.3"
            ]
        );
        assert!(
            files[0]
                .path
                .ends_with("optifine/OptiFine/1.21.11_HD_U_J9/OptiFine-1.21.11_HD_U_J9.jar")
        );
        assert_eq!(
            files[1].urls[0],
            "https://libraries.minecraft.net/optifine/launchwrapper-of/2.3/launchwrapper-of-2.3.jar"
        );
        // Forge's libraries all name where they come from (or are its own processors' output).
        assert!(
            installer_files(&dirs, "1.21.11-forge-61.2.5", &manifest(), &windows)
                .unwrap()
                .is_empty()
        );
    }
}
