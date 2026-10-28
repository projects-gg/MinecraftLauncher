//! The mod center: which items of a pack a player gets.
//!
//! Every [`PackItem`] is optional. The player's choices live in the [`PackState`] (`items`:
//! item id → selected) together with the ids already shown to the player (`seenItems`):
//! - a fresh set (no recorded choices yet: a new install, or a set made before the pack had
//!   items) starts from each item's `defaultEnabled`; an item whose file the pack already
//!   installed counts as selected, so older sets keep their mods;
//! - once choices exist, items added to the pack are new: switched off, not recorded, and
//!   flagged until the player confirms the mod center (any explicit [`PackSelection`]);
//! - choices are keyed by item id, so a new file name for the same mod keeps the choice;
//! - `requires` is closed over: an enabled item switches on what it needs, even items the
//!   player switched off; libraries are on exactly when an enabled item needs them;
//! - changes the player made in the mods folder are adopted ([`ManualChange`]): a sync that
//!   finds a pack jar deleted or renamed to `.disabled` records the item as switched off (and a
//!   disabled copy renamed back as switched on) instead of undoing the change. Those adopted
//!   choices rank below an explicit [`PackSelection`] and above the recorded choices;
//! - an item a launcher setting owns ([`LauncherItems`]: the in-game overlay, [`OVERLAY_ITEM`];
//!   "Hızlı doku paketi yükleme", [`FAST_PACK_ITEM`] and [`FAST_PACK_CACHE_ITEM`]) follows that
//!   setting alone: never recorded, adopted or new, and the mod center does not list it. A
//!   launcher that ships the overlay's mod itself never installs that item, and keeps the pack's
//!   Fabric API installed while the overlay is on ([`LauncherItems::needs`]). A fast-pack item
//!   also stays out while the game directory has a mod of its own for the same work
//!   ([`fast_pack_taken`]).
//!
//! [`resolve_items`] is pure; [`crate::plan_sync`] turns its result into downloads and
//! deletions.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::fsutil::normalize_hex;
use crate::model::{PackItem, PackManifest};
use crate::sync::{InstalledFile, PackState};

/// The in-game overlay's item in the official pack (OVL2): Settings → Genel "Oyun içi katman"
/// owns its choice ([`LauncherItems::overlay`]).
pub const OVERLAY_ITEM: &str = "projects-overlay";

/// Fabric API's Modrinth project. Studio gives Modrinth items their project id as item id; a pack
/// item is Fabric API when either says so ([`is_fabric_api`]).
pub const FABRIC_API_PROJECT: &str = "P7dR8mSH";

/// Whether a pack item installs Fabric API.
pub fn is_fabric_api(item: &PackItem) -> bool {
    item.id == FABRIC_API_PROJECT
        || item
            .modrinth
            .as_ref()
            .is_some_and(|modrinth| modrinth.project_id == FABRIC_API_PROJECT)
}

/// quick-pack's Modrinth project (FASTPACK): Settings → Genel "Hızlı doku paketi yükleme" owns
/// its choice ([`LauncherItems::fast_packs`]). Studio pins the file built for each family member's
/// Minecraft version; a member without the item has nothing to install.
pub const FAST_PACK_ITEM: &str = "pSISfJ4O";

/// ResourcePackCached's Modrinth project: the same setting owns it. Reserved until the FASTPACK
/// measurement passes: Studio lists it only then, so until a manifest has it nothing installs it.
pub const FAST_PACK_CACHE_ITEM: &str = "d4phKsx2";

/// Mod ids that do [`FAST_PACK_ITEM`]'s work: quick-pack itself (the player's own copy),
/// FastPackLoad and PackForge.
pub const FAST_PACK_RIVALS: &[&str] = &["quick-pack", "fastpackload", "packforge"];

/// Mod ids that do [`FAST_PACK_CACHE_ITEM`]'s work: ResourcePackCached itself and Server
/// Resourcepack Checker.
pub const FAST_PACK_CACHE_RIVALS: &[&str] = &["rpc", "resourcepackchecker"];

/// The fast-pack items a game directory already covers with a mod of its own
/// ([`fast_pack_taken`]): such an item stays out, and a copy the pack installed goes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct FastPackTaken {
    /// A jar declares one of [`FAST_PACK_RIVALS`].
    pub loader: bool,
    /// A jar declares one of [`FAST_PACK_CACHE_RIVALS`].
    pub cache: bool,
}

/// Which fast-pack items the game directory's mods already cover. `mods` lists its enabled jars
/// as (path relative to the game directory, the mod ids the jar declares, nested ones too); the
/// jar the pack state records for a fast-pack item is the pack's own copy and does not count.
pub fn fast_pack_taken(mods: &[(String, Vec<String>)], state: Option<&PackState>) -> FastPackTaken {
    let packs_own = |path: &str| {
        state.is_some_and(|state| {
            state.files.iter().any(|(recorded, file)| {
                recorded.eq_ignore_ascii_case(path)
                    && matches!(
                        file.item.as_deref(),
                        Some(FAST_PACK_ITEM | FAST_PACK_CACHE_ITEM)
                    )
            })
        })
    };
    let mut taken = FastPackTaken::default();
    for (path, ids) in mods {
        if packs_own(path) {
            continue;
        }
        for id in ids {
            taken.loader |= FAST_PACK_RIVALS.contains(&id.as_str());
            taken.cache |= FAST_PACK_CACHE_RIVALS.contains(&id.as_str());
        }
    }
    taken
}

/// Pack items a launcher setting owns instead of the mod center: one switch, one place. Such an
/// item is installed exactly while its setting is on (an enabled item that requires it does not
/// switch it on), its choice is never recorded in the pack state, a change to its file in the mods
/// folder is never adopted, it is never new, and the mod center does not list it
/// ([`LauncherItems::owns`]). No signed content field marks it: the item id does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct LauncherItems {
    /// "Oyun içi katman" (on by default).
    pub overlay: bool,
    /// The launcher ships the overlay's mod and puts it in every Fabric instance itself (OVL-B):
    /// the pack's overlay item is then never installed (a copy an older launcher installed goes
    /// like any item switched off), and while [`Self::overlay`] is on the pack's Fabric API stays
    /// installed for the mod ([`Self::needs`]), so the pack, not a second copy, provides it. Off
    /// by default: older launchers and Studio's previews install the item.
    pub overlay_bundled: bool,
    /// "Hızlı doku paketi yükleme" (FASTPACK) while the signed config's `features.fastPacks` is on
    /// too (the launcher passes the two together): the pack's [`FAST_PACK_ITEM`] and, once Studio
    /// lists it, [`FAST_PACK_CACHE_ITEM`] are installed. Off by default, so callers without the
    /// setting (Studio's previews) install neither.
    pub fast_packs: bool,
    /// The fast-pack items the game directory already covers ([`fast_pack_taken`]): they stay
    /// out whatever the setting says.
    pub fast_packs_taken: FastPackTaken,
}

impl Default for LauncherItems {
    fn default() -> Self {
        Self {
            overlay: true,
            overlay_bundled: false,
            fast_packs: false,
            fast_packs_taken: FastPackTaken::default(),
        }
    }
}

impl LauncherItems {
    /// Whether a launcher setting owns the item `id`.
    pub fn owns(id: &str) -> bool {
        matches!(id, OVERLAY_ITEM | FAST_PACK_ITEM | FAST_PACK_CACHE_ITEM)
    }

    /// The setting's choice for an item it owns.
    pub fn choice(&self, id: &str) -> Option<bool> {
        match id {
            OVERLAY_ITEM => Some(self.overlay && !self.overlay_bundled),
            FAST_PACK_ITEM => Some(self.fast_packs && !self.fast_packs_taken.loader),
            FAST_PACK_CACHE_ITEM => Some(self.fast_packs && !self.fast_packs_taken.cache),
            _ => None,
        }
    }

    /// Whether the launcher itself needs `item` installed: Fabric API while the mod the launcher
    /// ships for the overlay is on. Such an item is installed like a required one (whatever the
    /// player chose) and its status says why ([`ItemStatus::launcher_needs`]).
    pub fn needs(&self, item: &PackItem) -> bool {
        self.overlay && self.overlay_bundled && is_fabric_api(item)
    }
}

/// Choices from the mod center, for [`resolve_items`] and [`crate::plan_sync`]. Passing one
/// means the player confirmed the mod center: afterwards every item of the manifest counts as
/// seen (no longer new) and the adopted manual changes are acknowledged.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PackSelection {
    /// Item id → selected. Items not listed keep their recorded choice; libraries and ids the
    /// manifest does not have are ignored.
    pub items: BTreeMap<String, bool>,
    /// Legacy optional group id → enabled. Groups not listed keep their recorded choice.
    pub groups: BTreeMap<String, bool>,
    /// Items to bring back to the pack's file ("Önerilene döndür"): a file the player replaced
    /// is overwritten with the pack's. Listing an item does not change its choice.
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub revert: BTreeSet<String>,
}

/// A change the player made to a pack item's file outside the launcher. A sync adopts it
/// instead of fighting it; the mod center shows it until the player confirms the mod center.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ManualChange {
    /// The file was deleted: the item counts as switched off and is not downloaded again
    /// (unless an enabled item requires it).
    Removed,
    /// The file was renamed to `<path>.disabled`: switched off; the disabled copy is kept.
    Disabled,
    /// A disabled copy was renamed back: switched on.
    Enabled,
    /// The file was replaced by another one: it is never overwritten (the mod center offers
    /// to bring the pack's file back).
    Replaced,
    /// A change a newer launcher recorded.
    #[serde(other)]
    Other,
}

/// A manual change a sync adopted (new since the last sync), for a non-blocking notice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AdoptedChange {
    pub item: String,
    pub change: ManualChange,
}

/// One item as the mod center shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ItemStatus {
    pub id: String,
    /// The `files[].path` the item installs.
    pub path: String,
    /// Dependency only: not a choice, on when an enabled item requires it.
    pub library: bool,
    /// The player's choice (false for libraries and for new items nobody chose yet).
    pub selected: bool,
    /// Installed by the sync: selected, or required by another enabled item.
    pub effectively_enabled: bool,
    /// Added after the player's set was created and not confirmed in the mod center yet.
    pub is_new: bool,
    /// Enabled items that require this one, in manifest order (why it is on although not
    /// selected).
    pub required_by: Vec<String>,
    /// The launcher's own in-game overlay needs it (Fabric API, [`LauncherItems::needs`]): it is
    /// installed for that too, whatever the player chose.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub launcher_needs: bool,
    /// The pack state records a file of this item (an earlier sync installed it).
    pub installed: bool,
    /// Installed, but the manifest now has another file for it (new hash or file name).
    pub update_available: bool,
    /// Size of the item's file in the manifest, in bytes.
    pub size: u64,
    /// The manual change a sync adopted for this item and the player has not confirmed yet
    /// (set by [`crate::plan_sync`]; `None` from [`resolve_items`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manual: Option<ManualChange>,
    /// The file on disk is not the pack's: the player replaced it, so it is kept as it is (set
    /// by [`crate::plan_sync`]).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub modified: bool,
    /// Switched off and kept as `<path>.disabled` on disk (set by [`crate::plan_sync`]).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub disabled_copy: bool,
}

/// Result of [`resolve_items`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ItemResolution {
    /// One entry per manifest item, in manifest order.
    pub items: Vec<ItemStatus>,
    /// Item id → selected, as the next pack state records it.
    pub choices: BTreeMap<String, bool>,
    /// Item ids the next pack state counts as seen.
    pub seen: BTreeSet<String>,
}

impl ItemResolution {
    pub fn status(&self, id: &str) -> Option<&ItemStatus> {
        self.items.iter().find(|status| status.id == id)
    }

    /// True when the item will be installed.
    pub fn is_enabled(&self, id: &str) -> bool {
        self.status(id)
            .is_some_and(|status| status.effectively_enabled)
    }

    /// Items to highlight as new recommendations.
    pub fn new_items(&self) -> impl Iterator<Item = &ItemStatus> {
        self.items.iter().filter(|status| status.is_new)
    }
}

/// Computes the player's item set from the manifest, the previous pack state and, when the
/// player confirmed the mod center, their selection, with the launcher's own items at their
/// defaults ([`resolve_items_with`] takes their settings). See the module documentation for the
/// rules.
pub fn resolve_items(
    manifest: &PackManifest,
    previous: Option<&PackState>,
    selection: Option<&PackSelection>,
) -> ItemResolution {
    resolve_items_with(manifest, previous, selection, &LauncherItems::default())
}

/// [`resolve_items`] with the launcher settings that own their items.
pub fn resolve_items_with(
    manifest: &PackManifest,
    previous: Option<&PackState>,
    selection: Option<&PackSelection>,
    launcher: &LauncherItems,
) -> ItemResolution {
    resolve_with(manifest, previous, selection, &BTreeMap::new(), launcher)
}

/// [`resolve_items_with`] plus choices adopted from the game directory (`adopted`: item id →
/// selected, see [`ManualChange`]). They win over recorded choices and fresh-set defaults, but
/// an explicit selection wins over them; a launcher setting wins over everything for its items.
pub(crate) fn resolve_with(
    manifest: &PackManifest,
    previous: Option<&PackState>,
    selection: Option<&PackSelection>,
    adopted: &BTreeMap<String, bool>,
    launcher: &LauncherItems,
) -> ItemResolution {
    let items = &manifest.projects.items;
    let empty = PackState::default();
    let prev = previous.unwrap_or(&empty);
    let fresh = prev.items.is_empty() && prev.seen_items.is_empty();

    // A launcher setting's items are never recorded as choices.
    let mut choices = prev.items.clone();
    choices.retain(|id, _| !LauncherItems::owns(id));
    let mut selected: HashSet<&str> = HashSet::new();
    let mut set_on: HashSet<&str> = HashSet::new();
    for item in items.iter().filter(|item| !item.library) {
        if let Some(on) = launcher.choice(&item.id) {
            if on {
                set_on.insert(item.id.as_str());
            }
            continue;
        }
        let choice = selection
            .and_then(|s| s.items.get(&item.id))
            .or_else(|| adopted.get(&item.id))
            .or_else(|| prev.items.get(&item.id))
            .copied()
            .or_else(|| {
                fresh.then(|| installed_record(prev, item).is_some() || item.default_enabled)
            });
        if let Some(choice) = choice {
            choices.insert(item.id.clone(), choice);
            if choice {
                selected.insert(item.id.as_str());
            }
        }
    }

    let by_id: HashMap<&str, &PackItem> =
        items.iter().map(|item| (item.id.as_str(), item)).collect();
    let mut enabled: HashSet<&str> = selected.union(&set_on).copied().collect();
    // What the launcher's own overlay mod needs (Fabric API) is installed like a requirement.
    enabled.extend(
        items
            .iter()
            .filter(|item| launcher.needs(item))
            .map(|item| item.id.as_str()),
    );
    let mut pending: Vec<&str> = enabled.iter().copied().collect();
    while let Some(id) = pending.pop() {
        let Some(item) = by_id.get(id) else { continue };
        for required in &item.requires {
            // A launcher setting that is off keeps its item off, whatever needs it.
            if launcher.choice(required) == Some(false) {
                continue;
            }
            if by_id.contains_key(required.as_str()) && enabled.insert(required.as_str()) {
                pending.push(required.as_str());
            }
        }
    }

    let mut seen = prev.seen_items.clone();
    if fresh || selection.is_some() {
        seen.extend(items.iter().map(|item| item.id.clone()));
    }
    // Never new: the setting decides, the mod center does not show them.
    seen.extend(
        items
            .iter()
            .filter(|item| LauncherItems::owns(&item.id))
            .map(|item| item.id.clone()),
    );

    let statuses = items
        .iter()
        .map(|item| {
            let file = manifest.file(&item.path);
            let expected = file.and_then(|file| normalize_hex(&file.hashes.sha1, 40));
            let record = installed_record(prev, item);
            ItemStatus {
                id: item.id.clone(),
                path: item.path.clone(),
                library: item.library,
                selected: selected.contains(item.id.as_str()),
                effectively_enabled: enabled.contains(item.id.as_str()),
                is_new: !item.library
                    && !fresh
                    && !LauncherItems::owns(&item.id)
                    && !prev.seen_items.contains(&item.id)
                    && !prev.items.contains_key(&item.id),
                required_by: items
                    .iter()
                    .filter(|other| {
                        other.id != item.id
                            && enabled.contains(other.id.as_str())
                            && other.requires.contains(&item.id)
                    })
                    .map(|other| other.id.clone())
                    .collect(),
                launcher_needs: launcher.needs(item),
                installed: record.is_some(),
                update_available: record.is_some_and(|(path, installed)| {
                    *path != item.path
                        || expected.as_deref() != Some(installed.sha1.to_ascii_lowercase().as_str())
                }),
                size: file.map_or(0, |file| file.file_size),
                manual: None,
                modified: false,
                disabled_copy: false,
            }
        })
        .collect();

    ItemResolution {
        items: statuses,
        choices,
        seen,
    }
}

/// The record of the file the pack installed for `item`: tagged with its id (any path, so a
/// renamed jar is found), or for states written before items existed, untagged at its path.
pub(crate) fn installed_record<'a>(
    state: &'a PackState,
    item: &PackItem,
) -> Option<(&'a String, &'a InstalledFile)> {
    state
        .files
        .get_key_value(&item.path)
        .filter(|(_, record)| record.item.as_deref().is_none_or(|id| id == item.id))
        .or_else(|| {
            state
                .files
                .iter()
                .find(|(_, record)| record.item.as_deref() == Some(item.id.as_str()))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        FilePolicy, ItemCategory, PackDependencies, PackFile, PackHashes, ProjectsExtension,
    };

    fn sha1(content: &str) -> String {
        pl_net::hash_bytes(content.as_bytes(), pl_net::HashAlgo::Sha1)
    }

    fn file(path: &str) -> PackFile {
        PackFile {
            path: path.into(),
            hashes: PackHashes {
                sha1: sha1(path),
                sha512: None,
            },
            env: None,
            downloads: vec!["https://cdn.example/x".into()],
            file_size: path.len() as u64,
        }
    }

    fn item(id: &str, default_enabled: bool, requires: &[&str]) -> PackItem {
        PackItem {
            id: id.into(),
            path: format!("mods/{id}.jar"),
            title: id.to_uppercase(),
            summary: None,
            category: ItemCategory::Performance,
            icon_url: None,
            page_url: None,
            modrinth: None,
            default_enabled,
            requires: requires.iter().map(|r| (*r).to_owned()).collect(),
            library: false,
        }
    }

    fn library(id: &str) -> PackItem {
        PackItem {
            library: true,
            category: ItemCategory::Library,
            ..item(id, false, &[])
        }
    }

    fn manifest(items: Vec<PackItem>) -> PackManifest {
        let mut files: Vec<PackFile> = items.iter().map(|item| file(&item.path)).collect();
        files.push(file("options.txt"));
        let manifest = PackManifest {
            format_version: 1,
            game: "minecraft".into(),
            version_id: "1.0.0".into(),
            name: "Projects".into(),
            summary: None,
            files,
            dependencies: PackDependencies {
                minecraft: "1.21.1".into(),
                ..PackDependencies::default()
            },
            projects: ProjectsExtension {
                pack_id: "projects".into(),
                items,
                ..ProjectsExtension::default()
            },
        };
        manifest.validate().unwrap();
        manifest
    }

    /// sodium (on) → fabric-api (library); iris (off) → sodium; shaders (off) → iris.
    fn catalog() -> PackManifest {
        manifest(vec![
            item("sodium", true, &["fabric-api"]),
            item("iris", false, &["sodium"]),
            item("shaders", false, &["iris"]),
            item("minimap", true, &[]),
            library("fabric-api"),
        ])
    }

    fn enabled(resolution: &ItemResolution) -> Vec<&str> {
        resolution
            .items
            .iter()
            .filter(|status| status.effectively_enabled)
            .map(|status| status.id.as_str())
            .collect()
    }

    fn state_with(resolution: &ItemResolution) -> PackState {
        PackState {
            items: resolution.choices.clone(),
            seen_items: resolution.seen.clone(),
            ..PackState::default()
        }
    }

    #[test]
    fn first_install_uses_defaults() {
        let m = catalog();
        let first = resolve_items(&m, None, None);
        assert_eq!(enabled(&first), ["sodium", "minimap", "fabric-api"]);
        assert_eq!(
            first.choices,
            BTreeMap::from([
                ("iris".to_owned(), false),
                ("minimap".to_owned(), true),
                ("shaders".to_owned(), false),
                ("sodium".to_owned(), true),
            ]),
            "defaults are recorded, libraries are not choices"
        );
        assert_eq!(first.seen.len(), 5, "everything counts as seen");
        assert_eq!(first.new_items().count(), 0);
        let fabric = first.status("fabric-api").unwrap();
        assert!(fabric.library && !fabric.selected && fabric.effectively_enabled);
        assert_eq!(fabric.required_by, ["sodium"]);
        assert!(!fabric.installed && !fabric.update_available);
        assert_eq!(fabric.size, "mods/fabric-api.jar".len() as u64);

        // Recorded choices survive a changed default.
        let mut changed = catalog();
        changed.projects.items[3].default_enabled = false;
        let again = resolve_items(&changed, Some(&state_with(&first)), None);
        assert!(again.is_enabled("minimap"));
        assert_eq!(again.choices, first.choices);
    }

    #[test]
    fn new_items_stay_off_for_existing_players() {
        let before = resolve_items(&catalog(), None, None);
        let state = state_with(&before);
        let mut updated = catalog();
        updated.projects.items.push(item("zoom", true, &[]));
        updated.files.push(file("mods/zoom.jar"));
        updated.validate().unwrap();

        let after = resolve_items(&updated, Some(&state), None);
        let zoom = after.status("zoom").unwrap();
        assert!(zoom.is_new && !zoom.selected && !zoom.effectively_enabled);
        assert!(!after.choices.contains_key("zoom"), "no choice is invented");
        assert!(
            !after.seen.contains("zoom"),
            "a background sync shows nothing"
        );
        assert_eq!(after.new_items().count(), 1);
        assert!(after.is_enabled("sodium") && after.is_enabled("minimap"));

        // Confirming the mod center (even without changes) marks it seen; it stays off.
        let confirmed = resolve_items(&updated, Some(&state), Some(&PackSelection::default()));
        assert!(confirmed.seen.contains("zoom"));
        let later = resolve_items(&updated, Some(&state_with(&confirmed)), None);
        let zoom = later.status("zoom").unwrap();
        assert!(!zoom.is_new && !zoom.effectively_enabled);

        // Picking it records the choice.
        let selection = PackSelection {
            items: BTreeMap::from([("zoom".to_owned(), true)]),
            ..PackSelection::default()
        };
        let picked = resolve_items(&updated, Some(&state), Some(&selection));
        assert!(picked.is_enabled("zoom"));
        assert_eq!(picked.choices.get("zoom"), Some(&true));
    }

    #[test]
    fn requires_closure_and_libraries() {
        let m = catalog();
        let state = state_with(&resolve_items(&m, None, None));
        let pick = |pairs: &[(&str, bool)]| PackSelection {
            items: pairs
                .iter()
                .map(|(id, on)| ((*id).to_owned(), *on))
                .collect(),
            ..PackSelection::default()
        };

        // Shaders pull in Iris, which pulls in Sodium and the library.
        let shaders = resolve_items(&m, Some(&state), Some(&pick(&[("shaders", true)])));
        assert_eq!(
            enabled(&shaders),
            ["sodium", "iris", "shaders", "minimap", "fabric-api"]
        );
        let iris = shaders.status("iris").unwrap();
        assert!(!iris.selected && iris.effectively_enabled);
        assert_eq!(iris.required_by, ["shaders"]);

        // A deselected item that something enabled needs stays on (and says why).
        let forced = resolve_items(
            &m,
            Some(&state),
            Some(&pick(&[("shaders", true), ("sodium", false)])),
        );
        let sodium = forced.status("sodium").unwrap();
        assert!(!sodium.selected && sodium.effectively_enabled);
        assert_eq!(sodium.required_by, ["iris"]);

        // Nothing enabled needs the library any more: it goes too.
        let off = resolve_items(&m, Some(&state), Some(&pick(&[("sodium", false)])));
        assert_eq!(enabled(&off), ["minimap"]);
        assert!(off.status("fabric-api").unwrap().required_by.is_empty());

        // Libraries and unknown ids are not choices.
        let ignored = resolve_items(
            &m,
            Some(&state),
            Some(&pick(&[("fabric-api", false), ("unknown", true)])),
        );
        assert!(ignored.is_enabled("fabric-api"));
        assert!(!ignored.choices.contains_key("fabric-api"));
        assert!(!ignored.choices.contains_key("unknown"));
    }

    #[test]
    fn sets_from_before_items_keep_their_files() {
        // The pack installed sodium and iris before it had items; nothing was chosen yet.
        let m = catalog();
        let mut legacy = PackState::default();
        for path in ["mods/sodium.jar", "mods/iris.jar"] {
            legacy.files.insert(
                path.to_owned(),
                InstalledFile {
                    sha1: sha1(path),
                    policy: FilePolicy::Overwrite,
                    item: None,
                    disabled: false,
                    modified: false,
                },
            );
        }
        let resolution = resolve_items(&m, Some(&legacy), None);
        assert_eq!(
            enabled(&resolution),
            ["sodium", "iris", "minimap", "fabric-api"],
            "installed files stay, defaults fill the rest"
        );
        assert_eq!(resolution.new_items().count(), 0);
        let iris = resolution.status("iris").unwrap();
        assert!(iris.selected && iris.installed && !iris.update_available);
    }

    #[test]
    fn renamed_files_keep_the_choice() {
        let m = catalog();
        let mut state = state_with(&resolve_items(&m, None, None));
        // Version 1 installed sodium under another file name.
        state.files.insert(
            "mods/sodium-0.5.jar".into(),
            InstalledFile {
                sha1: sha1("old"),
                policy: FilePolicy::Overwrite,
                item: Some("sodium".into()),
                disabled: false,
                modified: false,
            },
        );
        let resolution = resolve_items(&m, Some(&state), None);
        let sodium = resolution.status("sodium").unwrap();
        assert!(sodium.selected && sodium.installed && sodium.update_available);

        // The same file, same hash: installed and current.
        state.files.clear();
        state.files.insert(
            "mods/sodium.jar".into(),
            InstalledFile {
                sha1: sha1("mods/sodium.jar").to_uppercase(),
                policy: FilePolicy::Overwrite,
                item: Some("sodium".into()),
                disabled: false,
                modified: false,
            },
        );
        let current = resolve_items(&m, Some(&state), None);
        let sodium = current.status("sodium").unwrap();
        assert!(sodium.installed && !sodium.update_available);
    }

    #[test]
    fn adopted_choices_rank_between_the_selection_and_the_records() {
        let m = catalog();
        let state = state_with(&resolve_items(&m, None, None));
        let adopted = BTreeMap::from([("minimap".to_owned(), false), ("iris".to_owned(), true)]);
        let background = resolve_with(&m, Some(&state), None, &adopted, &LauncherItems::default());
        assert!(!background.is_enabled("minimap"));
        assert!(background.is_enabled("iris") && background.is_enabled("sodium"));
        assert_eq!(background.choices.get("minimap"), Some(&false));
        assert_eq!(
            background.seen, state.seen_items,
            "adopting shows nothing new"
        );
        assert_eq!(background.new_items().count(), 0);

        // An explicit selection wins over what the disk shows.
        let selection = PackSelection {
            items: BTreeMap::from([("minimap".to_owned(), true)]),
            ..PackSelection::default()
        };
        let explicit = resolve_with(
            &m,
            Some(&state),
            Some(&selection),
            &adopted,
            &LauncherItems::default(),
        );
        assert!(explicit.is_enabled("minimap"));
        assert!(
            explicit.is_enabled("iris"),
            "unlisted items still follow the disk"
        );
    }

    /// The catalog with the in-game overlay, which needs the library.
    fn with_overlay() -> PackManifest {
        let mut m = catalog();
        m.projects
            .items
            .push(item(OVERLAY_ITEM, true, &["fabric-api"]));
        m.files.push(file(&format!("mods/{OVERLAY_ITEM}.jar")));
        m.validate().unwrap();
        m
    }

    /// The setting off, as a launcher without the overlay's own jars has it.
    const OFF: LauncherItems = LauncherItems {
        overlay: false,
        overlay_bundled: false,
        fast_packs: false,
        fast_packs_taken: FastPackTaken {
            loader: false,
            cache: false,
        },
    };

    /// "Hızlı doku paketi yükleme" on with the content flag, nothing of the player's in the way.
    const FAST: LauncherItems = LauncherItems {
        fast_packs: true,
        ..OFF
    };

    /// The catalog with both fast-pack items, as Studio lists Modrinth mods (their projects as
    /// ids), default-off for clients without the setting.
    fn with_fast_packs() -> PackManifest {
        let mut m = catalog();
        for id in [FAST_PACK_ITEM, FAST_PACK_CACHE_ITEM] {
            m.projects.items.push(item(id, false, &[]));
            m.files.push(file(&format!("mods/{id}.jar")));
        }
        m.validate().unwrap();
        m
    }

    #[test]
    fn fast_packs_follow_their_setting_alone() {
        let m = with_fast_packs();
        // Without the setting (or with the content flag off) neither is installed.
        let none = resolve_items(&m, None, None);
        assert!(!none.is_enabled(FAST_PACK_ITEM) && !none.is_enabled(FAST_PACK_CACHE_ITEM));
        assert!(!none.choices.contains_key(FAST_PACK_ITEM));

        // On: both installed in a set made before they came, never new, never a choice.
        let state = state_with(&resolve_items(&catalog(), None, None));
        let on = resolve_items_with(&m, Some(&state), None, &FAST);
        for id in [FAST_PACK_ITEM, FAST_PACK_CACHE_ITEM] {
            let status = on.status(id).unwrap();
            assert!(
                status.effectively_enabled && !status.selected && !status.is_new,
                "{id}"
            );
            assert!(!on.choices.contains_key(id) && on.seen.contains(id), "{id}");
        }
        assert_eq!(on.new_items().count(), 0);

        // The mod center's selection and what the mods folder shows do not move them.
        let named = only(FAST_PACK_ITEM, false);
        assert!(resolve_items_with(&m, Some(&state), Some(&named), &FAST).is_enabled(FAST_PACK_ITEM));
        let adopted = BTreeMap::from([(FAST_PACK_ITEM.to_owned(), true)]);
        assert!(!resolve_with(&m, Some(&state), None, &adopted, &OFF).is_enabled(FAST_PACK_ITEM));

        // A choice recorded for one is forgotten.
        let mut old = state.clone();
        old.items.insert(FAST_PACK_CACHE_ITEM.to_owned(), true);
        let cleaned = resolve_items_with(&m, Some(&old), None, &OFF);
        assert!(!cleaned.is_enabled(FAST_PACK_CACHE_ITEM));
        assert!(!cleaned.choices.contains_key(FAST_PACK_CACHE_ITEM));
    }

    #[test]
    fn a_mod_of_the_players_keeps_its_fast_pack_item_out() {
        let m = with_fast_packs();
        let loader = LauncherItems {
            fast_packs_taken: FastPackTaken {
                loader: true,
                cache: false,
            },
            ..FAST
        };
        let taken = resolve_items_with(&m, None, None, &loader);
        assert!(!taken.is_enabled(FAST_PACK_ITEM) && taken.is_enabled(FAST_PACK_CACHE_ITEM));
        let cache = LauncherItems {
            fast_packs_taken: FastPackTaken {
                loader: false,
                cache: true,
            },
            ..FAST
        };
        let taken = resolve_items_with(&m, None, None, &cache);
        assert!(taken.is_enabled(FAST_PACK_ITEM) && !taken.is_enabled(FAST_PACK_CACHE_ITEM));
    }

    #[test]
    fn rivals_are_read_from_the_mods_folder_without_the_packs_own_copies() {
        let jar = |path: &str, ids: &[&str]| {
            (
                path.to_owned(),
                ids.iter().map(|id| (*id).to_owned()).collect::<Vec<_>>(),
            )
        };
        let mut state = PackState::default();
        for (path, id) in [
            ("mods/quick-pack-1.5.1.jar", FAST_PACK_ITEM),
            ("mods/rpc-1.3.0.jar", FAST_PACK_CACHE_ITEM),
        ] {
            state.files.insert(
                path.to_owned(),
                InstalledFile {
                    sha1: "0".repeat(40),
                    policy: Default::default(),
                    item: Some(id.to_owned()),
                    disabled: false,
                    modified: false,
                },
            );
        }
        let ours = [
            jar("mods/quick-pack-1.5.1.jar", &["quick-pack"]),
            jar("mods/RPC-1.3.0.jar", &["rpc"]),
            jar("mods/sodium.jar", &["sodium"]),
        ];
        assert_eq!(fast_pack_taken(&ours, Some(&state)), FastPackTaken::default());

        // The player's own copy (another file), FastPackLoad, PackForge, a nested declaration.
        for rival in ["quick-pack", "fastpackload", "packforge"] {
            let mods = [jar("mods/theirs.jar", &["bundle", rival])];
            let taken = fast_pack_taken(&mods, Some(&state));
            assert!(taken.loader && !taken.cache, "{rival}");
        }
        for rival in ["rpc", "resourcepackchecker"] {
            let taken = fast_pack_taken(&[jar("mods/theirs.jar", &[rival])], None);
            assert!(taken.cache && !taken.loader, "{rival}");
        }
        // Without a pack state every copy is the player's.
        assert!(fast_pack_taken(&ours, None).loader);
    }

    #[test]
    fn the_overlay_follows_its_setting_alone() {
        let on = LauncherItems::default();
        let off = OFF;
        // A set made before the overlay came: it is installed at once, never new, never recorded.
        let before = resolve_items(&catalog(), None, None);
        let state = state_with(&before);
        let m = with_overlay();
        let now = resolve_items_with(&m, Some(&state), None, &on);
        let overlay = now.status(OVERLAY_ITEM).unwrap();
        assert!(overlay.effectively_enabled && !overlay.selected && !overlay.is_new);
        assert!(!now.choices.contains_key(OVERLAY_ITEM));
        assert!(now.seen.contains(OVERLAY_ITEM));
        assert_eq!(now.new_items().count(), 0);

        // Off: not installed, and the library goes too when nothing else needs it.
        let quiet = resolve_items_with(&m, Some(&state), None, &off);
        assert!(!quiet.is_enabled(OVERLAY_ITEM));
        assert!(quiet.is_enabled("fabric-api"), "sodium still needs it");
        let lean = resolve_items_with(
            &m,
            Some(&state),
            Some(&PackSelection {
                items: BTreeMap::from([("sodium".to_owned(), false)]),
                ..PackSelection::default()
            }),
            &on,
        );
        assert!(
            lean.is_enabled("fabric-api"),
            "the overlay alone keeps its library"
        );
        assert_eq!(
            lean.status("fabric-api").unwrap().required_by,
            [OVERLAY_ITEM]
        );

        // The mod center's selection and what the mods folder shows do not move it.
        let named = PackSelection {
            items: BTreeMap::from([(OVERLAY_ITEM.to_owned(), false)]),
            ..PackSelection::default()
        };
        assert!(resolve_items_with(&m, Some(&state), Some(&named), &on).is_enabled(OVERLAY_ITEM));
        let adopted = BTreeMap::from([(OVERLAY_ITEM.to_owned(), true)]);
        assert!(!resolve_with(&m, Some(&state), None, &adopted, &off).is_enabled(OVERLAY_ITEM));

        // A choice an older launcher recorded is forgotten.
        let mut old = state.clone();
        old.items.insert(OVERLAY_ITEM.to_owned(), false);
        let cleaned = resolve_items_with(&m, Some(&old), None, &on);
        assert!(cleaned.is_enabled(OVERLAY_ITEM));
        assert!(!cleaned.choices.contains_key(OVERLAY_ITEM));

        // A fresh set: the same.
        let fresh = resolve_items_with(&m, None, None, &off);
        assert!(!fresh.is_enabled(OVERLAY_ITEM));
        assert!(!fresh.choices.contains_key(OVERLAY_ITEM));
    }

    #[test]
    fn an_item_that_needs_the_overlay_does_not_switch_it_on() {
        let mut m = with_overlay();
        m.projects
            .items
            .push(item("overlay-skin", true, &[OVERLAY_ITEM]));
        m.files.push(file("mods/overlay-skin.jar"));
        m.validate().unwrap();
        let off = resolve_items_with(&m, None, None, &OFF);
        assert!(off.is_enabled("overlay-skin"));
        assert!(!off.is_enabled(OVERLAY_ITEM));
    }

    /// Fabric API the way Studio lists a Modrinth library: its project as id and reference.
    fn fabric_api(id: &str) -> PackItem {
        PackItem {
            modrinth: Some(crate::model::ModrinthRef {
                project_id: FABRIC_API_PROJECT.into(),
                version_id: "api-1".into(),
            }),
            ..library(id)
        }
    }

    fn only(id: &str, on: bool) -> PackSelection {
        PackSelection {
            items: BTreeMap::from([(id.to_owned(), on)]),
            ..PackSelection::default()
        }
    }

    #[test]
    fn a_launcher_with_its_own_overlay_jar_never_installs_the_item() {
        let bundled = LauncherItems {
            overlay: true,
            overlay_bundled: true,
            ..OFF
        };
        let off = LauncherItems {
            overlay: false,
            overlay_bundled: true,
            ..OFF
        };
        // The live pack's shape: the overlay and a mod both need Fabric API.
        let m = manifest(vec![
            item("sodium", true, &[FABRIC_API_PROJECT]),
            item(OVERLAY_ITEM, true, &[FABRIC_API_PROJECT]),
            fabric_api(FABRIC_API_PROJECT),
        ]);
        let first = resolve_items_with(&m, None, None, &bundled);
        assert!(
            !first.is_enabled(OVERLAY_ITEM),
            "the launcher puts its own in place"
        );
        assert!(!first.choices.contains_key(OVERLAY_ITEM));
        let fabric = first.status(FABRIC_API_PROJECT).unwrap();
        assert!(fabric.effectively_enabled && fabric.launcher_needs);
        assert_eq!(fabric.required_by, ["sodium"]);
        let state = state_with(&first);

        // Nothing in the pack needs Fabric API any more: the overlay alone keeps it.
        let lean = resolve_items_with(&m, Some(&state), Some(&only("sodium", false)), &bundled);
        assert_eq!(enabled(&lean), [FABRIC_API_PROJECT]);
        assert!(
            lean.status(FABRIC_API_PROJECT)
                .unwrap()
                .required_by
                .is_empty()
        );

        // Switched off: the library goes with the last mod that needed it.
        let quiet = resolve_items_with(&m, Some(&state), Some(&only("sodium", false)), &off);
        assert!(enabled(&quiet).is_empty());
        assert!(!quiet.status(FABRIC_API_PROJECT).unwrap().launcher_needs);

        // A Fabric API item that is a choice (found by its Modrinth reference) comes back too.
        let plain = manifest(vec![PackItem {
            modrinth: fabric_api("api").modrinth,
            ..item("api", false, &[])
        }]);
        let back = resolve_items_with(&plain, None, Some(&only("api", false)), &bundled);
        let api = back.status("api").unwrap();
        assert!(api.effectively_enabled && !api.selected && api.launcher_needs);

        // An older launcher (and Studio's preview) installs the item as before.
        assert!(resolve_items(&m, None, None).is_enabled(OVERLAY_ITEM));
        assert!(
            !resolve_items(&m, None, None)
                .status(FABRIC_API_PROJECT)
                .unwrap()
                .launcher_needs
        );
    }

    #[test]
    fn launcher_items_read_and_default_on() {
        assert!(LauncherItems::default().overlay);
        assert!(!LauncherItems::default().overlay_bundled);
        assert!(LauncherItems::owns(OVERLAY_ITEM) && !LauncherItems::owns("sodium"));
        assert!(LauncherItems::owns(FAST_PACK_ITEM) && LauncherItems::owns(FAST_PACK_CACHE_ITEM));
        assert_eq!(OFF.choice(OVERLAY_ITEM), Some(false));
        let bundled = LauncherItems {
            overlay: true,
            overlay_bundled: true,
            ..OFF
        };
        assert_eq!(bundled.choice(OVERLAY_ITEM), Some(false));
        assert_eq!(LauncherItems::default().choice("sodium"), None);
        // The fast packs wait for the launcher to say both the setting and the flag are on.
        assert!(!LauncherItems::default().fast_packs);
        assert_eq!(LauncherItems::default().choice(FAST_PACK_ITEM), Some(false));
        assert_eq!(FAST.choice(FAST_PACK_CACHE_ITEM), Some(true));
        let read: LauncherItems = serde_json::from_str("{}").unwrap();
        assert_eq!(read, LauncherItems::default());
        assert!(is_fabric_api(&library(FABRIC_API_PROJECT)));
        assert!(is_fabric_api(&fabric_api("fabric-api")));
        assert!(!is_fabric_api(&library("fabric-api")));
    }

    #[test]
    fn manifests_without_items_resolve_to_nothing() {
        let plain = manifest(Vec::new());
        let resolution = resolve_items(&plain, None, Some(&PackSelection::default()));
        assert!(resolution.items.is_empty());
        assert!(resolution.choices.is_empty() && resolution.seen.is_empty());
    }
}
