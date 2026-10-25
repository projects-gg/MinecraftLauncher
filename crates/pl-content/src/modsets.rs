//! Ready-made mod sets in a game directory.
//!
//! `<game_dir>/.projects/mod-sets.json` records which sets the player added and, per set, the
//! variant it installed and a [`PackState`] (what it owns, the player's choices). Like the pack
//! state it belongs to one Minecraft version: [`crate::VersionSets`] parks it with `mods/` and
//! `config/` and splits it with [`ModSetStates::switch_version`].
//!
//! A set is installed with the pack sync engine: [`ModSetVariant::manifest`] turns its variant
//! into a small pack, so every mod center rule holds for sets too (choices, new items stay off,
//! the player's changes are adopted, only unchanged files the set installed are removed). Files
//! another part of the instance already brings (the official pack's projects, the player's own
//! Modrinth mods) are left out of the set ([`provided_projects`]).
//!
//! Sets change files only when the player acts (add, apply choices, update, remove): no launch
//! syncs them and nothing is updated on its own. Resource packs a set installs are turned on in
//! `options.txt` once (`resource_packs`; the launcher writes the file), shader packs are
//! reported so the launcher can pick one in Iris once.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use jiff::Timestamp;
use pl_net::{DownloadOptions, Downloader};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::fsutil::{read_optional, write_durable};
use crate::items::PackSelection;
use crate::model::{
    MRPACK_FORMAT_VERSION, ModSet, ModSetItemKind, ModSetPurpose, ModSetVariant, PackDependencies,
    PackManifest, ProjectsExtension,
};
use crate::path::LAUNCHER_DIR;
use crate::sync::{
    DownloadAction, FileCategory, PackState, SyncPlan, SyncPolicy, SyncProgressFn, SyncReport,
    apply_sync_into, plan_sync,
};

/// File name of the set states inside `<game_dir>/.projects/` and inside a parked set folder.
pub const MOD_SETS_FILE: &str = "mod-sets.json";

/// The ready-made sets of one game directory (one Minecraft version).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct ModSetStates {
    /// Set id → what the set did here.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub sets: BTreeMap<String, AddedSet>,
}

/// One set the player added.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AddedSet {
    /// The set's name when it was last applied (shown when the catalog no longer has it).
    pub name: String,
    #[serde(default)]
    pub purpose: ModSetPurpose,
    /// The variant as last installed for this version. `None` while the set waits to be
    /// installed for it: a version switch carried the set over to a version it never ran on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied: Option<ModSetVariant>,
    /// Files and choices, as for a pack: `packId` is the set id, `versionId` the applied
    /// variant's [`ModSetVariant::content_key`] (empty while not installed).
    #[serde(default)]
    pub state: PackState,
    /// Resource packs the set installed (file names): `true` once turned on in `options.txt`,
    /// `false` while that waits for the game's first `options.txt`. Each is turned on once;
    /// the player's later choices in the game are never undone.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub resource_packs: BTreeMap<String, bool>,
    pub added_at: Timestamp,
}

impl AddedSet {
    /// Installed for this version (a variant was applied).
    pub fn installed(&self) -> bool {
        self.applied.is_some()
    }

    /// The catalog has another content for the applied variant: files, items or requirements
    /// changed (wording does not count). Never true before the set is installed here.
    pub fn has_update(&self, current: &ModSetVariant) -> bool {
        self.applied.is_some() && self.state.version_id != current.content_key()
    }

    /// Paths this set owns (recorded in its state), with the item that installed each.
    pub fn owned(&self) -> impl Iterator<Item = (&str, Option<&str>)> {
        self.state
            .files
            .iter()
            .map(|(path, record)| (path.as_str(), record.item.as_deref()))
    }
}

impl ModSetStates {
    /// `<game_dir>/.projects/mod-sets.json`.
    pub fn path(game_dir: &Path) -> PathBuf {
        game_dir.join(LAUNCHER_DIR).join(MOD_SETS_FILE)
    }

    /// The active states; empty when there are none. A corrupt file is reported and treated as
    /// empty (nothing is deleted then: the sets' files read as the player's own).
    pub async fn load(game_dir: &Path) -> Result<Self> {
        Ok(Self::load_from(&Self::path(game_dir))
            .await?
            .unwrap_or_default())
    }

    /// Blocking variant of [`Self::load`] for listings that already run off the async runtime.
    pub fn load_blocking(game_dir: &Path) -> Self {
        let path = Self::path(game_dir);
        match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|error| {
                tracing::warn!(path = %path.display(), %error, "ignoring corrupt mod set states");
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    /// Writes the active states (flushed: they hold the player's choices); removes the file
    /// when no set is left.
    pub async fn save(&self, game_dir: &Path) -> Result<()> {
        self.save_to(&Self::path(game_dir)).await
    }

    pub(crate) async fn load_from(path: &Path) -> Result<Option<Self>> {
        let Some(bytes) = read_optional(path).await? else {
            return Ok(None);
        };
        match serde_json::from_slice(&bytes) {
            Ok(states) => Ok(Some(states)),
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "ignoring corrupt mod set states");
                Ok(None)
            }
        }
    }

    pub(crate) async fn save_to(&self, path: &Path) -> Result<()> {
        if self.sets.is_empty() {
            return match tokio::fs::remove_file(path).await {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(Error::io(path, error)),
            };
        }
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| Error::json(MOD_SETS_FILE, e))?;
        write_durable(path, &bytes).await
    }

    /// The set that owns `path` (a file one of the sets installed), if any.
    pub fn owner(&self, path: &str) -> Option<(&str, &AddedSet)> {
        self.sets
            .iter()
            .find(|(_, set)| set.state.files.contains_key(path))
            .map(|(id, set)| (id.as_str(), set))
    }

    /// The sets installed for this version, by id (a friend's copy of the install adds the same
    /// ones: HOST1's world manifest, `sets`).
    pub fn installed_ids(&self) -> Vec<String> {
        self.sets
            .iter()
            .filter(|(_, set)| set.installed())
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// The Modrinth projects whose files the sets installed here (an item a set left out because
    /// another part of the instance has it is not theirs).
    pub fn owned_projects(&self) -> BTreeSet<String> {
        self.sets
            .values()
            .filter_map(|set| set.applied.as_ref().map(|variant| (set, variant)))
            .flat_map(|(set, variant)| {
                variant
                    .items
                    .iter()
                    .filter(move |item| set.state.files.contains_key(&item.path))
            })
            .filter_map(|item| item.modrinth.as_ref().map(|m| m.project_id.clone()))
            .collect()
    }

    /// Resource packs waiting to be turned on, in the order the sets list them (a set's first
    /// pack ends up on top of the game's list).
    pub fn pending_resource_packs(&self) -> Vec<String> {
        let mut pending = Vec::new();
        for set in self.sets.values() {
            let order: Vec<String> = set
                .applied
                .iter()
                .flat_map(|variant| &variant.items)
                .filter(|item| ModSetVariant::item_kind(item) == ModSetItemKind::ResourcePack)
                .filter_map(|item| file_name(&item.path).map(str::to_owned))
                .collect();
            let mut waiting: Vec<&String> = set
                .resource_packs
                .iter()
                .filter(|(_, done)| !**done)
                .map(|(name, _)| name)
                .collect();
            waiting.sort_by_key(|name| order.iter().position(|o| o == *name).unwrap_or(usize::MAX));
            pending.extend(waiting.into_iter().cloned());
        }
        pending
    }

    /// Records that these resource packs are now on (or that the game will never need it).
    pub fn mark_resource_packs_done(&mut self, names: &[String]) {
        for set in self.sets.values_mut() {
            for (name, done) in &mut set.resource_packs {
                if names.contains(name) {
                    *done = true;
                }
            }
        }
    }

    /// Splits the states when the instance moves from `from` to `to`; `(parked, active)`.
    /// `returning`: the instance was on `to` before, and `target` holds that version's parked
    /// states (if any).
    ///
    /// - The parked states keep each set's records under `mods/` and `config/`, its choices and
    ///   its applied variant.
    /// - A set the target had too comes back with its own records, choices and variant, plus the
    ///   leaving set's records of shared files (resource and shader packs stay in place).
    /// - Any other set is carried over, not installed for `to` yet: its choices and its shared
    ///   records go along; the mod center offers to install it for the new version.
    /// - Sets only the target had come back as they were parked.
    pub fn switch_version(
        self,
        from: &str,
        to: &str,
        returning: bool,
        target: Option<ModSetStates>,
    ) -> (ModSetStates, ModSetStates) {
        let mut target = target.unwrap_or_default();
        let mut parked = ModSetStates::default();
        let mut active = ModSetStates::default();
        for (id, leaving) in self.sets {
            let AddedSet {
                name,
                purpose,
                applied,
                state,
                resource_packs,
                added_at,
            } = leaving;
            let incoming = target.sets.remove(&id);
            let returning_state = match &incoming {
                Some(set) => Some(set.state.clone()),
                // Returning to a version that never had this set: no folder records come
                // along (config/ is not copied then), the choices do.
                None if returning => Some(PackState {
                    family: state.family.clone(),
                    groups: state.groups.clone(),
                    items: state.items.clone(),
                    seen_items: state.seen_items.clone(),
                    ..PackState::default()
                }),
                None => None,
            };
            let split = state.switch_version(from, to, returning_state);
            parked.sets.insert(
                id.clone(),
                AddedSet {
                    name: name.clone(),
                    purpose,
                    applied,
                    state: split.parked,
                    resource_packs: BTreeMap::new(),
                    added_at,
                },
            );
            let entry = match incoming {
                Some(incoming) => AddedSet {
                    state: split.active,
                    resource_packs,
                    ..incoming
                },
                None => AddedSet {
                    name,
                    purpose,
                    applied: None,
                    state: split.active,
                    resource_packs,
                    added_at,
                },
            };
            active.sets.insert(id, entry);
        }
        for (id, incoming) in target.sets {
            let state = PackState {
                minecraft: to.to_owned(),
                ..incoming.state
            };
            active.sets.insert(id, AddedSet { state, ..incoming });
        }
        (parked, active)
    }
}

/// The Modrinth projects (and item ids) the instance already gets elsewhere: every item the
/// official pack offers (on or off: switching it on later must not give two copies) and the
/// player's own Modrinth files. A set leaves these out.
pub fn provided_projects<'a>(
    pack: Option<&PackManifest>,
    own: impl IntoIterator<Item = &'a str>,
) -> BTreeSet<String> {
    let mut provided: BTreeSet<String> = own.into_iter().map(str::to_owned).collect();
    if let Some(pack) = pack {
        for item in &pack.projects.items {
            provided.insert(item.id.clone());
            if let Some(reference) = &item.modrinth {
                provided.insert(reference.project_id.clone());
            }
        }
    }
    provided
}

/// Plans installing `set` (its `variant`) into a game directory, or applying the player's
/// choices or the catalog's newer content to a set already added. `current` is the set's
/// entry in [`ModSetStates`] (`None` when the set is new here). `selection` carries the
/// player's choices (`None`: keep them; a new set starts from its defaults).
pub async fn plan_set(
    game_dir: &Path,
    set: &ModSet,
    variant: &ModSetVariant,
    current: Option<&AddedSet>,
    selection: Option<&PackSelection>,
    provided: &BTreeSet<String>,
) -> Result<SyncPlan> {
    let manifest = variant.manifest(&set.id, &set.name, provided);
    let mut plan = plan_sync(
        &manifest,
        game_dir,
        current.map(|added| &added.state),
        selection,
        &SyncPolicy::default(),
    )
    .await?;
    // Items left out because the instance has their project elsewhere still get the choice a
    // new install would make (or the player's): when that other copy goes, the set brings its
    // own instead of treating the item as new.
    for item in variant
        .items
        .iter()
        .filter(|item| !item.library && manifest.projects.item(&item.id).is_none())
    {
        let choice = selection
            .and_then(|selection| selection.items.get(&item.id).copied())
            .unwrap_or(item.default_enabled);
        let state = &mut plan.next_state;
        let recorded = state.items.contains_key(&item.id);
        if !recorded || selection.is_some_and(|s| s.items.contains_key(&item.id)) {
            plan.state_changed |= state.items.insert(item.id.clone(), choice) != Some(choice);
        }
        plan.state_changed |= state.seen_items.insert(item.id.clone());
    }
    Ok(plan)
}

/// Keeps the files another added set also records: two sets that share a mod (same file) both
/// own it, and it goes only with the last of them. Call before applying `plan` for `set_id`.
pub fn keep_shared_files(plan: &mut SyncPlan, states: &ModSetStates, set_id: &str) {
    let shared = |path: &str| {
        let path = path.strip_suffix(".disabled").unwrap_or(path);
        states
            .sets
            .iter()
            .any(|(id, set)| id != set_id && set.state.files.contains_key(path))
    };
    plan.deletes.retain(|delete| !shared(&delete.path));
}

/// Plans removing a set: every file it installed goes while it is unchanged; files the player
/// changed stay (they become the player's own).
pub async fn plan_set_removal(
    game_dir: &Path,
    set_id: &str,
    current: &AddedSet,
) -> Result<SyncPlan> {
    let minecraft = current
        .applied
        .as_ref()
        .map(|variant| variant.minecraft().to_owned())
        .unwrap_or_else(|| current.state.minecraft.clone());
    let manifest = PackManifest {
        format_version: MRPACK_FORMAT_VERSION,
        game: "minecraft".to_owned(),
        version_id: String::new(),
        name: current.name.clone(),
        summary: None,
        files: Vec::new(),
        dependencies: PackDependencies {
            minecraft,
            ..PackDependencies::default()
        },
        projects: ProjectsExtension {
            pack_id: set_id.to_owned(),
            ..ProjectsExtension::default()
        },
    };
    plan_sync(
        &manifest,
        game_dir,
        Some(&current.state),
        None,
        &SyncPolicy::default(),
    )
    .await
}

/// What [`apply_set`] records for the set.
#[derive(Debug, Clone)]
pub enum SetChange {
    /// The set (new or added before) now has `variant` installed.
    Install {
        name: String,
        purpose: ModSetPurpose,
        variant: ModSetVariant,
    },
    /// The set is gone from the game directory.
    Remove,
}

/// Result of [`apply_set`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SetApplyReport {
    pub sync: SyncReport,
    /// Resource packs (file names) the set no longer has: the launcher takes them out of
    /// `options.txt`.
    pub removed_resource_packs: Vec<String>,
    /// Shader packs (file names) this apply put in place for the first time, in set order.
    pub new_shader_packs: Vec<String>,
}

/// Applies a plan from [`plan_set`] or [`plan_set_removal`] and records the result in
/// `.projects/mod-sets.json` (at the point where a pack sync writes its state, so a crash never
/// leaves a state that misreads the set's own changes as the player's).
pub async fn apply_set(
    plan: &SyncPlan,
    set_id: &str,
    change: SetChange,
    downloader: &Downloader,
    options: &DownloadOptions,
    progress: Option<SyncProgressFn>,
) -> Result<SetApplyReport> {
    let game_dir = plan.game_dir.clone();
    let set_id_owned = set_id.to_owned();
    let before = ModSetStates::load(&game_dir).await?;
    let previous = before.sets.get(set_id).cloned();
    let previous_packs: BTreeSet<String> = previous
        .as_ref()
        .map(|set| set.resource_packs.keys().cloned().collect())
        .unwrap_or_default();
    let installing = matches!(change, SetChange::Install { .. });
    let new_shader_packs: Vec<String> = plan
        .downloads
        .iter()
        .filter(|download| {
            download.category == FileCategory::ShaderPack
                && download.action == DownloadAction::Add
                && previous
                    .as_ref()
                    .is_none_or(|set| !set.state.files.contains_key(&download.path))
        })
        .filter_map(|download| file_name(&download.path).map(str::to_owned))
        .collect();

    let removed = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let removed_out = std::sync::Arc::clone(&removed);
    let sync = apply_sync_into(
        plan,
        downloader,
        options,
        progress,
        move |state| async move {
            let mut states = ModSetStates::load(&game_dir).await?;
            match change {
                SetChange::Remove => {
                    states.sets.remove(&set_id_owned);
                }
                SetChange::Install {
                    name,
                    purpose,
                    variant,
                } => {
                    let now = Timestamp::now();
                    let entry =
                        states
                            .sets
                            .entry(set_id_owned.clone())
                            .or_insert_with(|| AddedSet {
                                name: name.clone(),
                                purpose,
                                applied: None,
                                state: PackState::default(),
                                resource_packs: BTreeMap::new(),
                                added_at: now,
                            });
                    entry.name = name;
                    entry.purpose = purpose;
                    entry.applied = Some(variant);
                    entry.state = state;
                    let installed: BTreeSet<String> = entry
                        .state
                        .files
                        .iter()
                        .filter(|(path, record)| {
                            !record.disabled
                                && ModSetItemKind::of_path(path)
                                    == Some(ModSetItemKind::ResourcePack)
                        })
                        .filter_map(|(path, _)| file_name(path).map(str::to_owned))
                        .collect();
                    entry
                        .resource_packs
                        .retain(|name, _| installed.contains(name));
                    for name in installed {
                        entry.resource_packs.entry(name).or_insert(false);
                    }
                }
            }
            let still: BTreeSet<String> = states
                .sets
                .get(&set_id_owned)
                .map(|set| set.resource_packs.keys().cloned().collect())
                .unwrap_or_default();
            if let Ok(mut removed) = removed_out.lock() {
                removed.extend(previous_packs.difference(&still).cloned());
            }
            states.save(&game_dir).await
        },
    )
    .await?;
    let removed_resource_packs = removed.lock().map(|r| r.clone()).unwrap_or_default();
    Ok(SetApplyReport {
        sync,
        removed_resource_packs,
        new_shader_packs: if installing {
            new_shader_packs
        } else {
            Vec::new()
        },
    })
}

/// The file name of a set path (`resourcepacks/LowFire.zip` → `LowFire.zip`).
fn file_name(path: &str) -> Option<&str> {
    path.rsplit_once('/')
        .map(|(_, name)| name)
        .filter(|name| !name.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::ManualChange;
    use crate::sync::InstalledFile;
    use crate::testutil::{TestServer, sample_mod_sets, sample_variant, test_http};

    fn write(dir: &Path, rel: &str, content: &[u8]) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    async fn apply(plan: &SyncPlan, set: &ModSet, variant: &ModSetVariant) -> SetApplyReport {
        apply_set(
            plan,
            &set.id,
            SetChange::Install {
                name: set.name.clone(),
                purpose: set.purpose,
                variant: variant.clone(),
            },
            &Downloader::new(test_http()),
            &DownloadOptions::default(),
            None,
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn a_set_installs_updates_and_leaves() {
        let server = TestServer::start().await;
        let game = tempfile::tempdir().unwrap();
        let game = game.path();
        let mut variant = sample_variant(
            Some(&server),
            "26.3",
            &[
                ("mods/zoom.jar", b"zoom 1"),
                ("mods/api.jar", b"api"),
                ("resourcepacks/LowFire.zip", b"fire"),
                ("shaderpacks/Soft.zip", b"shader"),
            ],
        );
        variant.items[0].requires = vec!["api".into()];
        variant.items[1].library = true;
        variant.items[3].default_enabled = false;
        let catalog = sample_mod_sets(variant.clone());
        let set = &catalog.sets[0];

        // Adding the set installs its defaults.
        let plan = plan_set(game, set, &variant, None, None, &BTreeSet::new())
            .await
            .unwrap();
        assert_eq!(plan.downloads.len(), 3, "the default-off shader stays out");
        let report = apply(&plan, set, &variant).await;
        assert_eq!(report.sync.downloaded, 3);
        assert!(report.new_shader_packs.is_empty());
        assert_eq!(
            std::fs::read(game.join("mods/zoom.jar")).unwrap(),
            b"zoom 1"
        );
        let states = ModSetStates::load(game).await.unwrap();
        let added = &states.sets["pvp"];
        assert!(added.installed());
        assert_eq!(added.state.version_id, variant.content_key());
        assert_eq!(
            added.resource_packs,
            BTreeMap::from([("LowFire.zip".into(), false)])
        );
        assert_eq!(states.pending_resource_packs(), ["LowFire.zip"]);
        assert_eq!(states.owner("mods/zoom.jar").map(|(id, _)| id), Some("pvp"));
        assert!(states.owner("mods/own.jar").is_none());
        assert!(!added.has_update(&variant));

        // The team ships a new zoom file and a new HUD mod: the update replaces the file and
        // offers the HUD mod without installing it.
        let mut next = sample_variant(
            Some(&server),
            "26.3",
            &[
                ("mods/zoom-2.jar", b"zoom 2"),
                ("mods/api.jar", b"api"),
                ("resourcepacks/LowFire.zip", b"fire"),
                ("shaderpacks/Soft.zip", b"shader"),
                ("mods/hud.jar", b"hud"),
            ],
        );
        next.items[0].id = "zoom".into();
        next.items[0].requires = vec!["api".into()];
        next.items[1].library = true;
        next.items[3].default_enabled = false;
        assert!(added.has_update(&next));
        let plan = plan_set(game, set, &next, Some(added), None, &BTreeSet::new())
            .await
            .unwrap();
        let hud = plan.items.iter().find(|item| item.id == "hud").unwrap();
        assert!(hud.is_new && !hud.effectively_enabled);
        apply(&plan, set, &next).await;
        assert!(!game.join("mods/zoom.jar").exists());
        assert!(game.join("mods/zoom-2.jar").exists());
        assert!(!game.join("mods/hud.jar").exists(), "new items stay off");

        // The player turns the shader on: it is reported once for Iris.
        let states = ModSetStates::load(game).await.unwrap();
        let selection = PackSelection {
            items: BTreeMap::from([("Soft".into(), true)]),
            ..PackSelection::default()
        };
        let plan = plan_set(
            game,
            set,
            &next,
            Some(&states.sets["pvp"]),
            Some(&selection),
            &BTreeSet::new(),
        )
        .await
        .unwrap();
        let report = apply(&plan, set, &next).await;
        assert_eq!(report.new_shader_packs, ["Soft.zip"]);

        // The player replaced the zoom jar: removing the set keeps it.
        write(game, "mods/zoom-2.jar", b"my zoom");
        let states = ModSetStates::load(game).await.unwrap();
        let plan = plan_set_removal(game, "pvp", &states.sets["pvp"])
            .await
            .unwrap();
        let report = apply_set(
            &plan,
            "pvp",
            SetChange::Remove,
            &Downloader::new(test_http()),
            &DownloadOptions::default(),
            None,
        )
        .await
        .unwrap();
        assert_eq!(report.removed_resource_packs, ["LowFire.zip"]);
        assert!(
            game.join("mods/zoom-2.jar").exists(),
            "the player's file stays"
        );
        for gone in [
            "mods/api.jar",
            "resourcepacks/LowFire.zip",
            "shaderpacks/Soft.zip",
        ] {
            assert!(!game.join(gone).exists(), "{gone}");
        }
        assert!(!ModSetStates::path(game).exists(), "no set left");
    }

    #[tokio::test]
    async fn provided_projects_are_not_installed_twice() {
        let server = TestServer::start().await;
        let game = tempfile::tempdir().unwrap();
        let game = game.path();
        let mut variant = sample_variant(
            Some(&server),
            "26.3",
            &[("mods/zoom.jar", b"zoom"), ("mods/api.jar", b"api")],
        );
        variant.items[0].requires = vec!["api".into()];
        variant.items[1].library = true;
        let catalog = sample_mod_sets(variant.clone());
        let mut pack = crate::testutil::sample_manifest(&server, &[]);
        pack.projects
            .items
            .push(crate::testutil::set_item("api", "mods/x.jar"));
        let provided = provided_projects(Some(&pack), ["own-mod"]);
        assert!(provided.contains("api") && provided.contains("own-mod"));
        let plan = plan_set(game, &catalog.sets[0], &variant, None, None, &provided)
            .await
            .unwrap();
        let paths: Vec<&str> = plan.downloads.iter().map(|d| d.path.as_str()).collect();
        assert_eq!(paths, ["mods/zoom.jar"]);
    }

    #[tokio::test]
    async fn two_sets_share_a_file_and_left_out_items_keep_their_choice() {
        let server = TestServer::start().await;
        let game = tempfile::tempdir().unwrap();
        let game = game.path();
        let pvp_variant = sample_variant(
            Some(&server),
            "26.3",
            &[("mods/sodium.jar", b"sodium"), ("mods/zoom.jar", b"zoom")],
        );
        let mut perf_variant =
            sample_variant(Some(&server), "26.3", &[("mods/sodium.jar", b"sodium")]);
        perf_variant.items[0].default_enabled = true;
        let pvp = sample_mod_sets(pvp_variant.clone()).sets.remove(0);
        let mut perf = pvp.clone();
        perf.id = "perf".into();
        perf.variants = vec![perf_variant.clone()];

        for (set, variant) in [(&pvp, &pvp_variant), (&perf, &perf_variant)] {
            let states = ModSetStates::load(game).await.unwrap();
            let plan = plan_set(
                game,
                set,
                variant,
                states.sets.get(&set.id),
                None,
                &BTreeSet::new(),
            )
            .await
            .unwrap();
            apply(&plan, set, variant).await;
        }
        let states = ModSetStates::load(game).await.unwrap();
        assert!(
            states.sets["pvp"]
                .state
                .files
                .contains_key("mods/sodium.jar")
        );
        assert!(
            states.sets["perf"]
                .state
                .files
                .contains_key("mods/sodium.jar")
        );

        // Removing PvP keeps the shared Sodium for the other set.
        let mut plan = plan_set_removal(game, "pvp", &states.sets["pvp"])
            .await
            .unwrap();
        keep_shared_files(&mut plan, &states, "pvp");
        let paths: Vec<&str> = plan.deletes.iter().map(|d| d.path.as_str()).collect();
        assert_eq!(paths, ["mods/zoom.jar"]);

        // A set whose Sodium another part of the instance brings still records the choice, so
        // it installs Sodium itself once that copy is gone.
        let other = tempfile::tempdir().unwrap();
        let provided = BTreeSet::from(["sodium".to_owned()]);
        let plan = plan_set(other.path(), &pvp, &pvp_variant, None, None, &provided)
            .await
            .unwrap();
        assert_eq!(plan.downloads.len(), 1);
        assert_eq!(plan.next_state.items.get("sodium"), Some(&true));
        assert!(plan.next_state.seen_items.contains("sodium"));
        apply(&plan, &pvp, &pvp_variant).await;
        let states = ModSetStates::load(other.path()).await.unwrap();
        let plan = plan_set(
            other.path(),
            &pvp,
            &pvp_variant,
            states.sets.get("pvp"),
            None,
            &BTreeSet::new(),
        )
        .await
        .unwrap();
        let sodium = plan.items.iter().find(|item| item.id == "sodium").unwrap();
        assert!(!sodium.is_new && sodium.effectively_enabled);
        assert_eq!(plan.downloads.len(), 1);
    }

    #[tokio::test]
    async fn the_players_changes_are_adopted() {
        let server = TestServer::start().await;
        let game = tempfile::tempdir().unwrap();
        let game = game.path();
        let variant = sample_variant(
            Some(&server),
            "26.3",
            &[("mods/zoom.jar", b"zoom"), ("mods/hud.jar", b"hud")],
        );
        let catalog = sample_mod_sets(variant.clone());
        let set = &catalog.sets[0];
        let plan = plan_set(game, set, &variant, None, None, &BTreeSet::new())
            .await
            .unwrap();
        apply(&plan, set, &variant).await;
        // The player disables one jar by hand: the set adopts it and never turns it back on.
        std::fs::rename(
            game.join("mods/hud.jar"),
            game.join("mods/hud.jar.disabled"),
        )
        .unwrap();
        let states = ModSetStates::load(game).await.unwrap();
        let plan = plan_set(
            game,
            set,
            &variant,
            Some(&states.sets["pvp"]),
            None,
            &BTreeSet::new(),
        )
        .await
        .unwrap();
        assert!(plan.downloads.is_empty() && plan.restores.is_empty());
        let hud = plan.items.iter().find(|item| item.id == "hud").unwrap();
        assert_eq!(hud.manual, Some(ManualChange::Disabled));
        assert!(!hud.effectively_enabled);
    }

    fn record(item: &str) -> InstalledFile {
        InstalledFile {
            sha1: "a".repeat(40),
            policy: crate::model::FilePolicy::PreserveUserChanges,
            item: Some(item.into()),
            disabled: false,
            modified: false,
        }
    }

    fn added(minecraft: &str, files: &[(&str, &str)], applied: bool) -> AddedSet {
        AddedSet {
            name: "PvP".into(),
            purpose: ModSetPurpose::Pvp,
            applied: applied.then(|| sample_variant(None, minecraft, &[("mods/zoom.jar", b"z")])),
            state: PackState {
                pack_id: "pvp".into(),
                version_id: if applied { "key".into() } else { String::new() },
                minecraft: minecraft.into(),
                files: files
                    .iter()
                    .map(|(path, item)| ((*path).to_owned(), record(item)))
                    .collect(),
                items: BTreeMap::from([("zoom".into(), true)]),
                seen_items: BTreeSet::from(["zoom".into()]),
                ..PackState::default()
            },
            resource_packs: BTreeMap::from([("LowFire.zip".into(), true)]),
            added_at: Timestamp::UNIX_EPOCH,
        }
    }

    #[test]
    fn installed_sets_and_the_projects_their_files_belong_to() {
        let states = ModSetStates {
            sets: BTreeMap::from([
                (
                    "pvp".into(),
                    added("26.3", &[("mods/zoom.jar", "zoom")], true),
                ),
                // Carried over from another version, not installed for this one yet.
                ("perf".into(), added("26.3", &[], false)),
            ]),
        };
        assert_eq!(states.installed_ids(), ["pvp"]);
        assert_eq!(states.owned_projects(), BTreeSet::from(["zoom".to_owned()]));
        // An item the set left out (the instance had it already) is not the set's.
        let left_out = ModSetStates {
            sets: BTreeMap::from([("pvp".into(), added("26.3", &[], true))]),
        };
        assert_eq!(left_out.installed_ids(), ["pvp"]);
        assert!(left_out.owned_projects().is_empty());
        assert!(ModSetStates::default().installed_ids().is_empty());
    }

    #[test]
    fn a_version_switch_parks_and_carries_sets() {
        let leaving = ModSetStates {
            sets: BTreeMap::from([(
                "pvp".into(),
                added(
                    "26.2",
                    &[
                        ("mods/zoom.jar", "zoom"),
                        ("resourcepacks/LowFire.zip", "fire"),
                    ],
                    true,
                ),
            )]),
        };

        // First visit of 26.3: the set comes along, not installed, with its shared records
        // and choices; its mods stay with 26.2.
        let (parked, active) = leaving.clone().switch_version("26.2", "26.3", false, None);
        let parked_pvp = &parked.sets["pvp"];
        assert!(parked_pvp.installed());
        assert_eq!(
            parked_pvp.state.files.keys().collect::<Vec<_>>(),
            ["mods/zoom.jar"]
        );
        assert!(parked_pvp.resource_packs.is_empty());
        let active_pvp = &active.sets["pvp"];
        assert!(!active_pvp.installed());
        assert_eq!(active_pvp.state.minecraft, "26.3");
        assert_eq!(active_pvp.state.version_id, "");
        assert_eq!(
            active_pvp.state.files.keys().collect::<Vec<_>>(),
            ["resourcepacks/LowFire.zip"]
        );
        assert_eq!(
            active_pvp.state.items,
            BTreeMap::from([("zoom".into(), true)])
        );
        assert_eq!(active_pvp.resource_packs.len(), 1);

        // Back to 26.2: its parked set returns with its mods and the current shared records.
        let (_, back) = active.switch_version("26.3", "26.2", true, Some(parked));
        let back_pvp = &back.sets["pvp"];
        assert!(back_pvp.installed());
        assert_eq!(back_pvp.state.version_id, "key");
        assert_eq!(
            back_pvp.state.files.keys().collect::<Vec<_>>(),
            ["mods/zoom.jar", "resourcepacks/LowFire.zip"]
        );

        // A set only the target had comes back as it was; one only the leaving version had is
        // carried over without folder records.
        let only_target = ModSetStates {
            sets: BTreeMap::from([(
                "perf".into(),
                added("26.2", &[("mods/fast.jar", "fast")], true),
            )]),
        };
        let (_, active) = leaving.switch_version("26.3", "26.2", true, Some(only_target));
        assert!(active.sets["perf"].installed());
        assert_eq!(active.sets["perf"].state.minecraft, "26.2");
        assert!(!active.sets["pvp"].installed());
        assert_eq!(
            active.sets["pvp"].state.files.keys().collect::<Vec<_>>(),
            ["resourcepacks/LowFire.zip"]
        );
    }

    #[test]
    fn pending_packs_follow_the_set_order() {
        let mut set = added("26.3", &[], true);
        // The set lists the shield pack first: it goes on top.
        let variant = sample_variant(
            None,
            "26.3",
            &[
                ("resourcepacks/Shield.zip", b"s"),
                ("resourcepacks/LowFire.zip", b"f"),
            ],
        );
        set.applied = Some(variant);
        set.resource_packs =
            BTreeMap::from([("LowFire.zip".into(), false), ("Shield.zip".into(), false)]);
        let mut states = ModSetStates {
            sets: BTreeMap::from([("pvp".into(), set)]),
        };
        assert_eq!(
            states.pending_resource_packs(),
            ["Shield.zip", "LowFire.zip"]
        );
        states.mark_resource_packs_done(&["Shield.zip".into()]);
        assert_eq!(states.pending_resource_packs(), ["LowFire.zip"]);
    }

    #[tokio::test]
    async fn states_round_trip_and_tolerate_corruption() {
        let game = tempfile::tempdir().unwrap();
        let game = game.path();
        assert_eq!(
            ModSetStates::load(game).await.unwrap(),
            ModSetStates::default()
        );
        let states = ModSetStates {
            sets: BTreeMap::from([(
                "pvp".into(),
                added("26.3", &[("mods/zoom.jar", "zoom")], true),
            )]),
        };
        states.save(game).await.unwrap();
        assert_eq!(ModSetStates::load(game).await.unwrap(), states);
        assert_eq!(ModSetStates::load_blocking(game), states);
        write(game, ".projects/mod-sets.json", b"{ broken");
        assert_eq!(
            ModSetStates::load(game).await.unwrap(),
            ModSetStates::default()
        );
        ModSetStates::default().save(game).await.unwrap();
        assert!(!ModSetStates::path(game).exists());
    }
}
