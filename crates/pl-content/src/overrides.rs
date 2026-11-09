//! The player's own versions of pack items (PACK-EDIT). Launcher-local: nothing here is part of
//! the signed content.
//!
//! - [`ItemOverride`]: a Modrinth version of a pack item the player picked ("Sürümü Değiştir"),
//!   or the launcher picked because another mod needs a newer one. It lives in the set's pack
//!   state ([`PackState::overrides`]), so it belongs to one Minecraft version; the sync installs
//!   its file instead of the pack's and never puts the pack's back. A selection's `revert`
//!   ("Paketin Sürümüne Dön") drops it. When the pack's own file for the item changes, the
//!   launcher asks Modrinth when the pack's version was published ([`pending_comparisons`]) and
//!   [`follow_pack`] ends the override once the pack's version is at least as new.
//! - [`DiskJar`] and [`own_stand_ins`]: a jar the player put into `mods/` that declares (or
//!   provides) the mod id of a pack item stands in for that item. The pack's copy is not
//!   installed and a copy the pack installed goes, so the loader never sees the mod twice. A pack
//!   item's mod ids are learned from its jar whenever a sync finds it on disk
//!   ([`PackState::item_mods`]); before that only Fabric API is known by its id.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::fsutil::normalize_hex;
use crate::items::{LauncherItems, is_fabric_api};
use crate::model::{EnvSupport, PackEnv, PackFile, PackHashes, PackItem, PackManifest};
use crate::path::check_relative_path;
use crate::sync::PackState;

/// Where an override's file may come from: Modrinth's CDN only.
pub const OVERRIDE_DOWNLOAD_PREFIX: &str = "https://cdn.modrinth.com/";

/// Fabric API's mod id (the id its jar declares).
pub const FABRIC_API_MOD_ID: &str = "fabric-api";

/// Why a set has an override.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "camelCase")]
pub enum OverrideReason {
    /// The player picked the version.
    #[default]
    Chosen,
    /// The launcher picked it: a mod of the player's needs a newer version than the pack's.
    Needed,
    /// A reason a newer launcher wrote.
    #[serde(other)]
    Other,
}

/// A version of a pack item used instead of the pack's file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ItemOverride {
    /// Where the file goes: `mods/<file name>`.
    pub path: String,
    pub sha1: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha512: Option<String>,
    #[serde(default)]
    pub size: u64,
    /// The download (Modrinth's CDN, [`OVERRIDE_DOWNLOAD_PREFIX`]).
    pub url: String,
    /// The Modrinth version.
    pub version_id: String,
    pub version_number: String,
    /// When Modrinth published it (RFC 3339).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published: Option<String>,
    /// SHA-1 of the pack's file for the item when the override was made or last compared with
    /// the pack: while the pack keeps that file, nothing is asked.
    pub pack_sha1: String,
    #[serde(default)]
    pub reason: OverrideReason,
    /// For [`OverrideReason::Needed`]: the mods that needed it, by name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub needed_by: Vec<String>,
}

impl ItemOverride {
    /// The sync can install it: a jar directly under `mods/` with a safe path, its SHA-1 and a
    /// download from Modrinth's CDN. An override that is not is ignored and dropped.
    pub fn usable(&self) -> bool {
        let name = self.path.strip_prefix("mods/").unwrap_or_default();
        check_relative_path(&self.path).is_ok()
            && !name.is_empty()
            && !name.contains('/')
            && name.to_ascii_lowercase().ends_with(".jar")
            && normalize_hex(&self.sha1, 40).is_some()
            && self
                .sha512
                .as_deref()
                .is_none_or(|hash| normalize_hex(hash, 128).is_some())
            && self.url.starts_with(OVERRIDE_DOWNLOAD_PREFIX)
            && !self.url.contains(char::is_whitespace)
    }

    /// The file the sync installs for the item.
    pub(crate) fn pack_file(&self) -> PackFile {
        PackFile {
            path: self.path.clone(),
            hashes: PackHashes {
                sha1: self.sha1.to_ascii_lowercase(),
                sha512: self.sha512.as_ref().map(|hash| hash.to_ascii_lowercase()),
            },
            env: Some(PackEnv {
                client: EnvSupport::Required,
                server: EnvSupport::Required,
            }),
            downloads: vec![self.url.clone()],
            file_size: self.size,
        }
    }
}

/// The overrides a plan follows: the state's, minus those `reverted`, those of items that left
/// the pack or that a launcher setting owns, unusable ones and ones whose path another file of
/// the manifest (or another override) already takes.
pub(crate) fn effective_overrides(
    manifest: &PackManifest,
    state: &PackState,
    reverted: &dyn Fn(&str) -> bool,
) -> BTreeMap<String, ItemOverride> {
    let mut taken: HashSet<String> = HashSet::new();
    let mut out = BTreeMap::new();
    for (id, choice) in &state.overrides {
        let Some(item) = manifest.projects.item(id) else {
            continue;
        };
        if reverted(id) || LauncherItems::owns(id) || !choice.usable() {
            continue;
        }
        let lower = choice.path.to_ascii_lowercase();
        let clashes = manifest.files.iter().any(|file| {
            !file.path.eq_ignore_ascii_case(&item.path) && file.path.to_ascii_lowercase() == lower
        });
        if clashes || !taken.insert(lower) {
            continue;
        }
        out.insert(id.clone(), choice.clone());
    }
    out
}

/// A jar of the mods folder with the mod ids it declares at its top level and the ids it
/// provides (`pl_mc::mods::scan`; jars bundled inside it do not count).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DiskJar {
    /// `mods/<file name>`, without `.disabled`.
    pub path: String,
    pub ids: Vec<String>,
    /// Not renamed to `.disabled`. A disabled jar of the pack's still teaches its item's ids; only
    /// an enabled jar of the player's stands in for an item.
    #[serde(default = "enabled_default")]
    pub enabled: bool,
}

fn enabled_default() -> bool {
    true
}

/// The mod ids a pack item's jar declares: learned from the disk, else Fabric API's own id.
pub(crate) fn item_ids<'a>(
    item: &PackItem,
    learned: &'a BTreeMap<String, Vec<String>>,
) -> Vec<&'a str> {
    match learned.get(&item.id) {
        Some(ids) if !ids.is_empty() => ids.iter().map(String::as_str).collect(),
        _ if is_fabric_api(item) => vec![FABRIC_API_MOD_ID],
        _ => Vec::new(),
    }
}

/// The mod ids each pack item's jar declares, from `previous` and updated from the jars on disk
/// (enabled or kept as `.disabled`) at the paths the last sync installed the items (for an
/// override, its file). Only items of the manifest are kept.
pub(crate) fn learn_item_mods(
    manifest: &PackManifest,
    previous: &PackState,
    disk: &[DiskJar],
) -> BTreeMap<String, Vec<String>> {
    let on_disk: HashMap<String, &DiskJar> = disk
        .iter()
        .map(|jar| (jar.path.to_ascii_lowercase(), jar))
        .collect();
    let mut learned = previous.item_mods.clone();
    for (path, record) in &previous.files {
        let Some(id) = record.item.as_deref() else {
            continue;
        };
        if record.modified {
            continue;
        }
        if let Some(jar) = on_disk.get(&path.to_ascii_lowercase())
            && !jar.ids.is_empty()
        {
            let mut ids = jar.ids.clone();
            ids.sort();
            ids.dedup();
            learned.insert(id.to_owned(), ids);
        }
    }
    learned.retain(|id, _| manifest.projects.item(id).is_some());
    learned
}

/// The pack items a jar of the player's stands in for (item id → `mods/<file name>` of that
/// jar). The player's jars are the enabled ones the pack did not install (no record in
/// `previous`) and that are no override's file. Items a launcher setting owns are left to it.
pub fn own_stand_ins(
    manifest: &PackManifest,
    previous: &PackState,
    learned: &BTreeMap<String, Vec<String>>,
    overrides: &BTreeMap<String, ItemOverride>,
    disk: &[DiskJar],
) -> BTreeMap<String, String> {
    let ours: HashSet<String> = previous
        .files
        .keys()
        .chain(overrides.values().map(|choice| &choice.path))
        .map(|path| path.to_ascii_lowercase())
        .collect();
    let mut provider: HashMap<&str, &str> = HashMap::new();
    for jar in disk
        .iter()
        .filter(|jar| jar.enabled && !ours.contains(&jar.path.to_ascii_lowercase()))
    {
        for id in &jar.ids {
            provider.entry(id.as_str()).or_insert(jar.path.as_str());
        }
    }
    let mut out = BTreeMap::new();
    if provider.is_empty() {
        return out;
    }
    for item in manifest
        .projects
        .items
        .iter()
        .filter(|item| !LauncherItems::owns(&item.id))
    {
        if let Some(path) = item_ids(item, learned)
            .into_iter()
            .find_map(|id| provider.get(id))
        {
            out.insert(item.id.clone(), (*path).to_owned());
        }
    }
    out
}

/// An override to compare with the pack's newer file: the launcher looks up when the pack's
/// Modrinth version was published and calls [`follow_pack`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingComparison {
    pub item: String,
    /// SHA-1 of the pack's file for the item now.
    pub pack_sha1: String,
    /// The pack's Modrinth version for the item, when it names one.
    pub pack_version_id: Option<String>,
}

/// The overrides of `state` whose item's file in `manifest` is no longer the one they were last
/// compared with.
pub fn pending_comparisons(manifest: &PackManifest, state: &PackState) -> Vec<PendingComparison> {
    state
        .overrides
        .iter()
        .filter_map(|(id, choice)| {
            let item = manifest.projects.item(id)?;
            let file = manifest.file(&item.path)?;
            let sha1 = normalize_hex(&file.hashes.sha1, 40)?;
            (!choice.pack_sha1.eq_ignore_ascii_case(&sha1)).then(|| PendingComparison {
                item: id.clone(),
                pack_sha1: sha1,
                pack_version_id: item.modrinth.as_ref().map(|m| m.version_id.clone()),
            })
        })
        .collect()
}

/// What [`follow_pack`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FollowOutcome {
    /// The pack's version is at least as new: the override went, the pack's file comes back.
    Following,
    /// The player's version is newer (or the two cannot be compared): it stays, and the pack's
    /// current file is remembered so the question is not asked again.
    Kept,
    /// No override for the item.
    None,
}

/// The pack's file for `item` is now `pack_sha1`, published at `pack_published` (RFC 3339, from
/// Modrinth). The override ends when that is no earlier than the override's own version.
pub fn follow_pack(
    state: &mut PackState,
    item: &str,
    pack_sha1: &str,
    pack_published: Option<&str>,
) -> FollowOutcome {
    let Some(choice) = state.overrides.get_mut(item) else {
        return FollowOutcome::None;
    };
    let parse = |text: &str| text.parse::<jiff::Timestamp>().ok();
    let newer = match (
        pack_published.and_then(parse),
        choice.published.as_deref().and_then(parse),
    ) {
        (Some(pack), Some(mine)) => pack >= mine,
        _ => false,
    };
    if newer {
        state.overrides.remove(item);
        FollowOutcome::Following
    } else {
        choice.pack_sha1 = pack_sha1.to_ascii_lowercase();
        FollowOutcome::Kept
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ModrinthRef, PackDependencies, ProjectsExtension};
    use crate::sync::InstalledFile;
    use crate::testutil::sha1_hex;

    pub(crate) fn choice(path: &str, content: &[u8]) -> ItemOverride {
        ItemOverride {
            path: path.into(),
            sha1: sha1_hex(content),
            sha512: None,
            size: content.len() as u64,
            url: format!("{OVERRIDE_DOWNLOAD_PREFIX}data/x/versions/y/{}", &path[5..]),
            version_id: "v2".into(),
            version_number: "2.0.0".into(),
            published: Some("2026-11-01T10:00:00Z".into()),
            pack_sha1: sha1_hex(b"fabric"),
            reason: OverrideReason::Chosen,
            needed_by: Vec::new(),
        }
    }

    fn file(path: &str, content: &[u8]) -> PackFile {
        PackFile {
            path: path.into(),
            hashes: PackHashes {
                sha1: sha1_hex(content),
                sha512: None,
            },
            env: None,
            downloads: vec!["https://cdn.example/x".into()],
            file_size: content.len() as u64,
        }
    }

    fn item(id: &str, path: &str) -> PackItem {
        PackItem {
            id: id.into(),
            path: path.into(),
            title: id.into(),
            summary: None,
            category: Default::default(),
            icon_url: None,
            page_url: None,
            modrinth: None,
            default_enabled: true,
            requires: Vec::new(),
            library: false,
        }
    }

    fn manifest(items: Vec<PackItem>, files: Vec<PackFile>) -> PackManifest {
        PackManifest {
            format_version: 1,
            game: "minecraft".into(),
            version_id: "1".into(),
            name: "Projects".into(),
            summary: None,
            files,
            dependencies: PackDependencies {
                minecraft: "26.1.2".into(),
                ..PackDependencies::default()
            },
            projects: ProjectsExtension {
                pack_id: "projects".into(),
                items,
                ..ProjectsExtension::default()
            },
        }
    }

    fn jar(path: &str, ids: &[&str]) -> DiskJar {
        DiskJar {
            path: path.into(),
            ids: ids.iter().map(|id| (*id).to_owned()).collect(),
            enabled: true,
        }
    }

    #[test]
    fn only_modrinth_jars_under_mods_are_usable() {
        assert!(choice("mods/fabric-api-0.141.jar", b"x").usable());
        let mut bad = choice("mods/a.jar", b"x");
        bad.url = "https://example.com/a.jar".into();
        assert!(!bad.usable(), "another host");
        for path in [
            "config/a.jar",
            "mods/sub/a.jar",
            "mods/a.zip",
            "mods/../a.jar",
            "mods/",
        ] {
            let mut bad = choice("mods/a.jar", b"x");
            bad.path = path.into();
            assert!(!bad.usable(), "{path}");
        }
        let mut bad = choice("mods/a.jar", b"x");
        bad.sha1 = "nope".into();
        assert!(!bad.usable(), "a broken hash");
    }

    #[test]
    fn overrides_of_gone_reverted_or_clashing_items_are_ignored() {
        let m = manifest(
            vec![
                item("sodium", "mods/sodium.jar"),
                item("iris", "mods/iris.jar"),
            ],
            vec![file("mods/sodium.jar", b"s"), file("mods/iris.jar", b"i")],
        );
        let mut state = PackState::default();
        state
            .overrides
            .insert("sodium".into(), choice("mods/sodium-2.jar", b"s2"));
        // Not in the pack any more.
        state
            .overrides
            .insert("zoom".into(), choice("mods/zoom-2.jar", b"z"));
        // The same path as Sodium's (letter case aside): the first one (by id) keeps it.
        state
            .overrides
            .insert("iris".into(), choice("mods/SODIUM-2.jar", b"i2"));
        let all = effective_overrides(&m, &state, &|_| false);
        assert_eq!(all.keys().collect::<Vec<_>>(), ["iris"]);
        // Another file of the manifest has the path: refused.
        let mut taken = state.clone();
        taken
            .overrides
            .insert("iris".into(), choice("mods/sodium.jar", b"i2"));
        let none_for_iris = effective_overrides(&m, &taken, &|_| false);
        assert_eq!(none_for_iris.keys().collect::<Vec<_>>(), ["sodium"]);
        let reverted = effective_overrides(&m, &state, &|id| id == "iris");
        assert_eq!(reverted.keys().collect::<Vec<_>>(), ["sodium"]);
        state
            .overrides
            .insert("iris".into(), choice("mods/iris.jar", b"i2"));
        // The item's own path (same file name, another version) is fine.
        let same = effective_overrides(&m, &state, &|_| false);
        assert_eq!(same.len(), 2);
    }

    #[test]
    fn a_jar_of_the_players_stands_in_for_the_pack_item_with_its_id() {
        let mut fabric = item("P7dR8mSH", "mods/fabric-api.jar");
        fabric.library = true;
        fabric.modrinth = Some(ModrinthRef {
            project_id: "P7dR8mSH".into(),
            version_id: "old".into(),
        });
        let m = manifest(
            vec![fabric, item("sodium", "mods/sodium.jar")],
            vec![
                file("mods/fabric-api.jar", b"f"),
                file("mods/sodium.jar", b"s"),
            ],
        );
        let mut state = PackState::default();
        state.files.insert(
            "mods/sodium.jar".into(),
            InstalledFile {
                sha1: sha1_hex(b"s"),
                policy: Default::default(),
                item: Some("sodium".into()),
                disabled: false,
                modified: false,
            },
        );
        let disk = vec![
            jar("mods/sodium.jar", &["sodium"]),
            jar("mods/fabric-api-0.141.1.jar", &["fabric-api"]),
            jar("mods/my-sodium-fork.jar", &["sodiumfork", "sodium"]),
        ];
        let learned = learn_item_mods(&m, &state, &disk);
        assert_eq!(learned["sodium"], ["sodium"]);
        let stand_ins = own_stand_ins(&m, &state, &learned, &BTreeMap::new(), &disk);
        assert_eq!(
            stand_ins,
            BTreeMap::from([
                (
                    "P7dR8mSH".to_owned(),
                    "mods/fabric-api-0.141.1.jar".to_owned()
                ),
                ("sodium".to_owned(), "mods/my-sodium-fork.jar".to_owned()),
            ]),
            "Fabric API by its known id, Sodium by the id its pack jar declared"
        );
        // The pack's own copy and an override's file never count as the player's.
        let overrides = BTreeMap::from([(
            "P7dR8mSH".to_owned(),
            choice("mods/fabric-api-0.141.1.jar", b"f2"),
        )]);
        let stand_ins = own_stand_ins(&m, &state, &learned, &overrides, &disk[..2]);
        assert!(stand_ins.is_empty());
    }

    #[test]
    fn a_disabled_pack_copy_teaches_its_ids_but_only_enabled_jars_stand_in() {
        let mut lib = item("lib", "mods/lib.jar");
        lib.library = true;
        let m = manifest(vec![lib], vec![file("mods/lib.jar", b"l")]);
        let mut state = PackState::default();
        state.files.insert(
            "mods/lib.jar".into(),
            InstalledFile {
                sha1: sha1_hex(b"l"),
                policy: Default::default(),
                item: Some("lib".into()),
                disabled: true,
                modified: false,
            },
        );
        let mut pack_copy = jar("mods/lib.jar", &["lib"]);
        pack_copy.enabled = false;
        let mut theirs_off = jar("mods/lib-2.jar", &["lib"]);
        theirs_off.enabled = false;
        let learned = learn_item_mods(&m, &state, &[pack_copy.clone(), theirs_off]);
        assert_eq!(learned["lib"], ["lib"]);
        let none = own_stand_ins(&m, &state, &learned, &BTreeMap::new(), &[pack_copy.clone()]);
        assert!(none.is_empty(), "a disabled jar of the player's stands in for nothing");
        let theirs = jar("mods/lib-2.jar", &["lib"]);
        let some = own_stand_ins(&m, &state, &learned, &BTreeMap::new(), &[pack_copy, theirs]);
        assert_eq!(some["lib"], "mods/lib-2.jar");
    }

    #[test]
    fn learned_ids_survive_the_pack_jar_and_follow_the_manifest() {
        let m = manifest(
            vec![item("sodium", "mods/sodium.jar")],
            vec![file("mods/sodium.jar", b"s")],
        );
        let mut state = PackState::default();
        state
            .item_mods
            .insert("sodium".into(), vec!["sodium".into()]);
        state.item_mods.insert("gone".into(), vec!["gone".into()]);
        let learned = learn_item_mods(&m, &state, &[]);
        assert_eq!(learned.keys().collect::<Vec<_>>(), ["sodium"]);
    }

    #[test]
    fn the_pack_catches_up_with_a_version_at_least_as_new() {
        let mut fabric = item("fabric", "mods/fabric-new.jar");
        fabric.modrinth = Some(ModrinthRef {
            project_id: "P7dR8mSH".into(),
            version_id: "pack-v".into(),
        });
        let m = manifest(
            vec![fabric],
            vec![file("mods/fabric-new.jar", b"fabric-new")],
        );
        let mut state = PackState::default();
        state
            .overrides
            .insert("fabric".into(), choice("mods/fabric-mine.jar", b"mine"));
        let pending = pending_comparisons(&m, &state);
        assert_eq!(
            pending,
            [PendingComparison {
                item: "fabric".into(),
                pack_sha1: sha1_hex(b"fabric-new"),
                pack_version_id: Some("pack-v".into()),
            }]
        );
        // An older pack version: kept, and not asked again.
        let sha = &pending[0].pack_sha1;
        assert_eq!(
            follow_pack(&mut state, "fabric", sha, Some("2026-10-01T00:00:00Z")),
            FollowOutcome::Kept
        );
        assert!(pending_comparisons(&m, &state).is_empty());
        // Unknown dates keep it too.
        assert_eq!(
            follow_pack(&mut state, "fabric", sha, None),
            FollowOutcome::Kept
        );
        assert_eq!(
            follow_pack(&mut state, "fabric", sha, Some("2026-11-01T10:00:00.000Z")),
            FollowOutcome::Following
        );
        assert!(state.overrides.is_empty());
        assert_eq!(
            follow_pack(&mut state, "fabric", sha, None),
            FollowOutcome::None
        );
    }
}
