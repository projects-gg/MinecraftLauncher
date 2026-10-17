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
//!   choices rank below an explicit [`PackSelection`] and above the recorded choices.
//!
//! [`resolve_items`] is pure; [`crate::plan_sync`] turns its result into downloads and
//! deletions.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::fsutil::normalize_hex;
use crate::model::{PackItem, PackManifest};
use crate::sync::{InstalledFile, PackState};

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
/// player confirmed the mod center, their selection. See the module documentation for the
/// rules.
pub fn resolve_items(
    manifest: &PackManifest,
    previous: Option<&PackState>,
    selection: Option<&PackSelection>,
) -> ItemResolution {
    resolve_with(manifest, previous, selection, &BTreeMap::new())
}

/// [`resolve_items`] plus choices adopted from the game directory (`adopted`: item id →
/// selected, see [`ManualChange`]). They win over recorded choices and fresh-set defaults, but
/// an explicit selection wins over them.
pub(crate) fn resolve_with(
    manifest: &PackManifest,
    previous: Option<&PackState>,
    selection: Option<&PackSelection>,
    adopted: &BTreeMap<String, bool>,
) -> ItemResolution {
    let items = &manifest.projects.items;
    let empty = PackState::default();
    let prev = previous.unwrap_or(&empty);
    let fresh = prev.items.is_empty() && prev.seen_items.is_empty();

    let mut choices = prev.items.clone();
    let mut selected: HashSet<&str> = HashSet::new();
    for item in items.iter().filter(|item| !item.library) {
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
    let mut enabled = selected.clone();
    let mut pending: Vec<&str> = selected.iter().copied().collect();
    while let Some(id) = pending.pop() {
        let Some(item) = by_id.get(id) else { continue };
        for required in &item.requires {
            if by_id.contains_key(required.as_str()) && enabled.insert(required.as_str()) {
                pending.push(required.as_str());
            }
        }
    }

    let mut seen = prev.seen_items.clone();
    if fresh || selection.is_some() {
        seen.extend(items.iter().map(|item| item.id.clone()));
    }

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
        let background = resolve_with(&m, Some(&state), None, &adopted);
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
        let explicit = resolve_with(&m, Some(&state), Some(&selection), &adopted);
        assert!(explicit.is_enabled("minimap"));
        assert!(
            explicit.is_enabled("iris"),
            "unlisted items still follow the disk"
        );
    }

    #[test]
    fn manifests_without_items_resolve_to_nothing() {
        let plain = manifest(Vec::new());
        let resolution = resolve_items(&plain, None, Some(&PackSelection::default()));
        assert!(resolution.items.is_empty());
        assert!(resolution.choices.is_empty() && resolution.seen.is_empty());
    }
}
