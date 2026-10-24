//! The Minecraft versions of an instance and their per-version sets (`pl_content::sets`): the
//! version selector, a preview of a switch, the switch itself (crash-safe, refused while the
//! game runs), finishing a switch an earlier run left unfinished, and moving the player's own
//! mods to another version through Modrinth.
//!
//! Every instance keeps one set per Minecraft version: `mods/` and `config/` (and, for the
//! official pack, the choices of the mod center). The add-on index (Modrinth titles and icons
//! of the files in `mods/`) is a cache; the records of a parked set wait in
//! `<instance>/.launcher/sets/<version>.addons.json` until the set comes back.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use pl_content::sets::{folder_name, read_set_dir};
use pl_content::{
    PackManifest, PackRef, PackState, SetInfo, SwitchJournal, SwitchRequest, VersionSets,
    apply_sync, estimate_downloads, plan_sync, resolve_items,
};
use pl_lang::pick;
use pl_mc::addons::{AddonIndex, AddonInstallOptions, AddonProgress, Addons};
use pl_mc::gamefiles::{ImportKind, LauncherTrash, TRASH_RETENTION, TrashEntry, TrashKind};
use pl_mc::instance::{Instance, InstanceLoader};
use pl_mc::loaders::LoaderKind;
use pl_mc::modrinth::{
    MigrationFit, ModrinthHashAlgorithm, ModrinthResolveTarget, compatible_loaders, migration_moves,
};
use pl_mc::mods::ModInfo;
use pl_mc::version::manifest::VersionType;
use pl_net::CancellationToken;
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use crate::commands::modcenter::{quick, sync_options, sync_progress};
use crate::commands::modrinth::{
    JobProgressView, addon_index_path, content_error, icon_dir, instance_lock, modrinth_client,
    percent, start_job,
};
use crate::commands::official::{cached_or_fresh, instance_family, pack_error, pack_loader};
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

/// What the instance becomes when a switch completes (kept in the switch journal so a switch
/// interrupted by a crash ends the same way).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct SwitchContext {
    loader: Option<LoaderKind>,
    loader_version: Option<String>,
    pack_id: Option<String>,
    pack_version: Option<String>,
}

pub(crate) fn refuse_while_running(state: &AppState, instance_id: &str) -> CmdResult<()> {
    if state
        .running
        .lock()
        .as_ref()
        .is_some_and(|running| running.instance_id == instance_id)
    {
        return Err(CmdError::new(
            "OYUN-ACIK-MOD",
            pick(
                "Oyun açıkken modlar ve sürüm değiştirilemez. Önce oyunu kapat.",
                "You can't change mods or the version while the game is open. Close the game first.",
            ),
            "the game of this instance is running",
        ));
    }
    Ok(())
}

fn switch_error(error: pl_content::Error) -> CmdError {
    CmdError::new(
        "SURUM-DEGISIMI",
        pick(
            "Sürüm değiştirilemedi. Mod klasörünü açık tutan bir program (dosya gezgini ya da antivirüs gibi) olabilir; kapatıp tekrar dene.",
            "Couldn't switch the version. A program may have the mods folder open (like your file manager or antivirus); close it and try again.",
        ),
        error.to_string(),
    )
}

fn loader_name(kind: LoaderKind) -> String {
    kind.modrinth_name().unwrap_or_default().to_owned()
}

/// `<instance>/.launcher/sets/<version>.addons.json`.
pub(crate) fn parked_addons_path(instance_dir: &Path, minecraft: &str) -> PathBuf {
    instance_dir
        .join(".launcher")
        .join("sets")
        .join(format!("{}.addons.json", folder_name(minecraft)))
}

/// Moves the add-on index records of `mods/` aside with the set that was parked. Best effort:
/// the index is only a cache.
async fn park_addons(instance_dir: &Path, minecraft: &str) {
    let parked = parked_addons_path(instance_dir, minecraft);
    if tokio::fs::try_exists(&parked).await.unwrap_or(false) {
        return;
    }
    let index_path = addon_index_path(instance_dir);
    let mut index = AddonIndex::load(&index_path).await;
    let set = AddonIndex {
        files: index
            .files
            .iter()
            .filter(|(key, _)| key.starts_with("mods/"))
            .map(|(key, record)| (key.clone(), record.clone()))
            .collect(),
        ..AddonIndex::default()
    };
    match set.save(&parked).await {
        Ok(()) => {
            index.files.retain(|key, _| !key.starts_with("mods/"));
            if let Err(error) = index.save(&index_path).await {
                tracing::debug!(%error, "could not trim the add-on index");
            }
        }
        Err(error) => tracing::debug!(%error, "could not park the add-on index"),
    }
}

/// Brings a returning set's add-on index records back.
async fn unpark_addons(instance_dir: &Path, minecraft: &str) {
    let parked = parked_addons_path(instance_dir, minecraft);
    if !tokio::fs::try_exists(&parked).await.unwrap_or(false) {
        return;
    }
    let set = AddonIndex::load(&parked).await;
    let index_path = addon_index_path(instance_dir);
    let mut index = AddonIndex::load(&index_path).await;
    for (key, record) in set.files {
        index.files.entry(key).or_insert(record);
    }
    match index.save(&index_path).await {
        Ok(()) => {
            let _ = tokio::fs::remove_file(&parked).await;
        }
        Err(error) => tracing::debug!(%error, "could not restore the add-on index"),
    }
}

/// The instance takes the version the journal names, the add-on index follows the sets and
/// the journal goes.
async fn finalize(
    state: &AppState,
    instance: &mut Instance,
    sets: &VersionSets,
    journal: &SwitchJournal,
) -> CmdResult<()> {
    let context: SwitchContext =
        serde_json::from_value(journal.context.clone()).unwrap_or_default();
    instance.mc_version.clone_from(&journal.to);
    if let Some(kind) = context.loader {
        instance.loader = InstanceLoader {
            kind,
            version: context.loader_version.clone(),
        };
    }
    if let (Some(managed), Some(pack_id)) = (instance.managed_pack.as_mut(), &context.pack_id) {
        managed.pack_id.clone_from(pack_id);
        managed.version = context.pack_version.clone().unwrap_or_default();
    }
    state
        .instances
        .update(instance)
        .await
        .map_err(CmdError::from)?;
    if let Ok(instance_dir) = state.instances.instance_dir(&instance.id) {
        park_addons(&instance_dir, &journal.from).await;
        unpark_addons(&instance_dir, &journal.to).await;
    }
    sets.finish().await.map_err(switch_error)
}

/// Held while a switch runs or an unfinished one is completed, so a switch in progress is never
/// "resumed" by another command at the same time.
pub(crate) static SWITCHES: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn journal_path(game_dir: &Path) -> PathBuf {
    game_dir
        .join(pl_content::path::LAUNCHER_DIR)
        .join(pl_content::sets::SWITCH_JOURNAL)
}

/// Finishes a version switch an earlier run left unfinished (crash, power cut): the folders are
/// completed and the instance takes the new version. Cheap when nothing is pending; waits for a
/// switch that is running right now and then returns the instance as it ended.
pub(crate) async fn ensure_switch_finished(
    state: &AppState,
    instance: &mut Instance,
) -> CmdResult<()> {
    let game_dir = state
        .instances
        .game_dir(&instance.id)
        .map_err(CmdError::from)?;
    if !tokio::fs::try_exists(journal_path(&game_dir))
        .await
        .unwrap_or(false)
    {
        return Ok(());
    }
    let _switching = SWITCHES.lock().await;
    *instance = state
        .instances
        .get(&instance.id)
        .await
        .map_err(CmdError::from)?;
    let sets = VersionSets::new(&game_dir);
    let Some(journal) = sets.resume().await.map_err(switch_error)? else {
        return Ok(());
    };
    tracing::info!(instance = %instance.id, from = %journal.from, to = %journal.to, "finished an interrupted version switch");
    finalize(state, instance, &sets, &journal).await
}

/// At start-up: finishes every switch an earlier run left unfinished.
pub(crate) fn recover_unfinished_switches(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let Ok(instances) = state.instances.list().await else {
            return;
        };
        for mut instance in instances {
            let Ok(game_dir) = state.instances.game_dir(&instance.id) else {
                continue;
            };
            if !tokio::fs::try_exists(journal_path(&game_dir))
                .await
                .unwrap_or(false)
            {
                continue;
            }
            let lock = instance_lock(&app, &instance.id);
            let _guard = lock.lock().await;
            if let Err(error) = ensure_switch_finished(&state, &mut instance).await {
                tracing::warn!(instance = %instance.id, ?error, "could not finish a version switch");
            }
        }
    });
}

/// The version an instance moves to: its loader and, for the official pack, the family member.
struct SwitchTarget {
    loader: InstanceLoader,
    pack: Option<(PackRef, PackManifest)>,
}

async fn switch_target(
    state: &AppState,
    instance: &Instance,
    game_dir: &Path,
    minecraft: &str,
) -> CmdResult<SwitchTarget> {
    if instance.managed_pack.is_some() {
        let snapshot = cached_or_fresh(state).await.ok_or_else(|| {
            CmdError::new(
                "ICERIK-YOK",
                pick(
                    "Sürümler yüklenemedi. İnternet bağlantını kontrol edip tekrar dene.",
                    "Couldn't load the versions. Check your internet connection and try again.",
                ),
                "no content snapshot",
            )
        })?;
        let family = instance_family(&snapshot.index, instance, Some(game_dir)).unwrap_or_default();
        let member = snapshot
            .index
            .family_member(&family, minecraft)
            .cloned()
            .ok_or_else(|| {
                CmdError::new(
                    "SURUM-SUNULMUYOR",
                    pick(
                        "Projects paketi bu Minecraft sürümü için sunulmuyor.",
                        "The Projects pack isn't offered for this Minecraft version.",
                    ),
                    format!("family {family} has no member for {minecraft}"),
                )
            })?;
        let manifest = state
            .content
            .pack_manifest(&member)
            .await
            .map_err(pack_error)?;
        return Ok(SwitchTarget {
            loader: pack_loader(&manifest),
            pack: Some((member, manifest)),
        });
    }
    let kind = instance.loader.kind;
    if kind == LoaderKind::Vanilla {
        return Ok(SwitchTarget {
            loader: InstanceLoader::default(),
            pack: None,
        });
    }
    let version = match pl_mc::loaders::list_loader_versions(&state.mc, kind, minecraft).await {
        Ok(list) if list.is_empty() => {
            return Err(CmdError::new(
                "YUKLEYICI-SURUM",
                pick(
                    "Bu kurulumun mod yükleyicisi bu Minecraft sürümünü desteklemiyor.",
                    "This instance's mod loader doesn't support this Minecraft version.",
                ),
                format!("{kind:?} has no build for {minecraft}"),
            ));
        }
        Ok(list) => list
            .iter()
            .find(|v| v.recommended)
            .or_else(|| list.iter().find(|v| v.stable))
            .or_else(|| list.first())
            .map(|v| v.version.clone()),
        // Chosen at the first launch once the network is back.
        Err(error) if error.is_offline() => None,
        Err(error) => return Err(error.into()),
    };
    Ok(SwitchTarget {
        loader: InstanceLoader { kind, version },
        pack: None,
    })
}

/// The mods (jars) in `mods_dir` (a set of `game_dir`) that `state` does not record: the player's
/// own. The in-game overlay's jar the launcher keeps there is not one (OVL-B).
async fn own_mods(
    game_dir: &Path,
    mods_dir: &Path,
    state: Option<&PackState>,
    with_hashes: bool,
) -> Vec<ModInfo> {
    let overlay = crate::overlay::owned_files(game_dir);
    let owned: HashSet<String> = state
        .map(|state| {
            state
                .files
                .keys()
                .filter_map(|path| path.strip_prefix("mods/"))
                .map(str::to_ascii_lowercase)
                .collect()
        })
        .unwrap_or_default();
    pl_mc::mods::list_mods(mods_dir, with_hashes)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|m| {
            let base = m
                .file_name
                .strip_suffix(".disabled")
                .unwrap_or(&m.file_name);
            !owned.contains(&base.to_ascii_lowercase())
                && !pl_overlay::install::is_owned(&overlay, &m.file_name)
        })
        .collect()
}

/// The jars in `mods_dir`, the in-game overlay's jars the launcher keeps (`overlay`) left out.
async fn count_mods(mods_dir: &Path, overlay: &HashSet<String>) -> u32 {
    let count = pl_mc::mods::list_mods(mods_dir, false)
        .await
        .map(|mods| {
            mods.iter()
                .filter(|m| !pl_overlay::install::is_owned(overlay, &m.file_name))
                .count()
        })
        .unwrap_or(0);
    u32::try_from(count).unwrap_or(u32::MAX)
}

fn version_installed(state: &AppState, minecraft: &str) -> bool {
    state
        .mc
        .dirs
        .version_json(minecraft)
        .is_ok_and(|path| path.is_file())
}

// ---------------------------------------------------------------------------------------------
// Version selector

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VersionOptionView {
    pub minecraft: String,
    pub current: bool,
    /// The launcher's recommended Minecraft version (signed config, else the built-in default);
    /// for the official instance, the family's default member when the pack is not published for
    /// that version.
    pub recommended: bool,
    /// The instance can move to it (the pack publishes it, or the loader supports it).
    pub available: bool,
    /// A set is kept for it: switching back brings its mods and settings back exactly.
    pub has_set: bool,
    /// Mods were moved into it ahead of the first visit.
    pub prepared: bool,
    /// A snapshot (development) version.
    pub snapshot: bool,
    pub last_played_at: Option<String>,
    /// Size of its mods and settings, in bytes.
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VersionOptionsView {
    pub current: String,
    pub managed: bool,
    pub loader: LoaderKind,
    /// Newest first.
    pub options: Vec<VersionOptionView>,
    /// The list could only be partly loaded (no network and nothing cached).
    pub partial: bool,
    pub game_running: bool,
}

/// The Minecraft versions an instance can move to: the official pack's versions, or for other
/// instances the releases its loader supports (and snapshots when `include_snapshots`; the
/// recommended version even when it is a snapshot), plus the current version and every version
/// it keeps a set for. Newest first.
#[tauri::command]
#[specta::specta]
pub async fn instance_versions(
    state: State<'_, AppState>,
    instance_id: String,
    include_snapshots: bool,
) -> CmdResult<VersionOptionsView> {
    let mut instance = state
        .instances
        .get(&instance_id)
        .await
        .map_err(CmdError::from)?;
    ensure_switch_finished(&state, &mut instance).await?;
    let game_dir = state
        .instances
        .game_dir(&instance_id)
        .map_err(CmdError::from)?;
    let sets = VersionSets::new(&game_dir);
    let parked: HashMap<String, SetInfo> = sets
        .parked()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|info| (info.minecraft.clone(), info))
        .collect();
    let managed = instance.managed_pack.is_some();

    // Mojang's list: which versions are snapshots, and the release order (newest first).
    let mojang = match pl_mc::version::manifest::load_manifest(&state.mc, false).await {
        Ok(manifest) => Some(manifest),
        Err(_) => pl_mc::version::manifest::load_manifest(&state.mc, true)
            .await
            .ok(),
    };
    let known: HashMap<String, (usize, VersionType)> = mojang
        .as_ref()
        .map(|manifest| {
            manifest
                .versions
                .iter()
                .enumerate()
                .map(|(position, v)| (v.id.clone(), (position, v.kind)))
                .collect()
        })
        .unwrap_or_default();
    let is_snapshot = |minecraft: &str| {
        known
            .get(minecraft)
            .is_some_and(|(_, kind)| matches!(kind, VersionType::Snapshot | VersionType::Other))
    };

    let recommended_minecraft = crate::commands::content::recommended_minecraft(&state).await;
    let mut partial = false;
    let mut available: Vec<String> = Vec::new();
    let mut recommended = None;
    if managed {
        match cached_or_fresh(&state).await {
            Some(snapshot) => {
                let family = instance_family(&snapshot.index, &instance, Some(&game_dir))
                    .unwrap_or_default();
                for member in snapshot.index.family_members(&family) {
                    if let Some(minecraft) = member.minecraft()
                        && (include_snapshots
                            || !is_snapshot(minecraft)
                            || minecraft == recommended_minecraft)
                        && !available.iter().any(|v| v == minecraft)
                    {
                        available.push(minecraft.to_owned());
                    }
                }
                // The launcher's recommended version when the pack is published for it, else the
                // member new installs use.
                recommended = if available.contains(&recommended_minecraft) {
                    Some(recommended_minecraft.clone())
                } else {
                    snapshot
                        .index
                        .family_default(&family)
                        .and_then(|pack| pack.minecraft())
                        .map(str::to_owned)
                };
            }
            None => partial = true,
        }
    } else {
        recommended = Some(recommended_minecraft.clone());
        match &mojang {
            Some(manifest) => {
                let candidates: Vec<String> = manifest
                    .versions
                    .iter()
                    .filter(|v| {
                        v.kind == VersionType::Release
                            || (include_snapshots && v.kind == VersionType::Snapshot)
                            || v.id == recommended_minecraft
                    })
                    .map(|v| v.id.clone())
                    .collect();
                match pl_mc::loaders::supported_minecraft_versions(
                    &state.mc,
                    instance.loader.kind,
                    &candidates,
                )
                .await
                {
                    Ok(list) => available = list,
                    Err(error) => {
                        tracing::info!(%error, "loader game versions unavailable");
                        partial = true;
                    }
                }
            }
            None => partial = true,
        }
    }

    let active = sets.active_info().await.ok().flatten();
    let active_size = sets.active_size().await;
    let mut versions: Vec<String> = available.clone();
    for minecraft in std::iter::once(&instance.mc_version).chain(parked.keys()) {
        if !versions.contains(minecraft) {
            versions.push(minecraft.clone());
        }
    }
    let mut options: Vec<VersionOptionView> = versions
        .into_iter()
        .map(|minecraft| {
            let current = minecraft == instance.mc_version;
            let set = parked.get(&minecraft);
            VersionOptionView {
                current,
                recommended: recommended.as_deref() == Some(minecraft.as_str()),
                // Versions the pack stopped offering stay listed with their set, but only the
                // published ones can be switched to; other instances can always go back.
                available: !current
                    && (available.contains(&minecraft) || (!managed && set.is_some())),
                has_set: current || set.is_some_and(|info| !info.seeded),
                prepared: set.is_some_and(|info| info.seeded),
                snapshot: is_snapshot(&minecraft),
                // The active set's own record; the instance's last session only for a set made
                // before per-version records existed (after a switch it belongs to the old set).
                last_played_at: if current {
                    match active.as_ref().filter(|info| info.minecraft == minecraft) {
                        Some(info) => info.last_played_at,
                        None => instance.last_played_at,
                    }
                    .map(|t| t.to_string())
                } else {
                    set.and_then(|info| info.last_played_at)
                        .map(|t| t.to_string())
                },
                size_bytes: if current {
                    active_size
                } else {
                    set.map_or(0, |info| info.size_bytes)
                },
                minecraft,
            }
        })
        .collect();
    // Mojang's order where it knows the version (it places snapshots correctly), the version
    // numbers otherwise; one total order either way.
    options.sort_by(|a, b| pl_content::compare_minecraft_versions(&b.minecraft, &a.minecraft));
    let rank: HashMap<String, usize> = options
        .iter()
        .enumerate()
        .map(|(rank, option)| (option.minecraft.clone(), rank))
        .collect();
    options.sort_by_key(|option| {
        (
            known
                .get(&option.minecraft)
                .map_or(usize::MAX, |(position, _)| *position),
            rank.get(&option.minecraft).copied().unwrap_or(usize::MAX),
        )
    });
    Ok(VersionOptionsView {
        current: instance.mc_version.clone(),
        managed,
        loader: instance.loader.kind,
        options,
        partial,
        game_running: state
            .running
            .lock()
            .as_ref()
            .is_some_and(|running| running.instance_id == instance_id),
    })
}

// ---------------------------------------------------------------------------------------------
// Switching

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SwitchPreviewView {
    pub from: String,
    pub to: String,
    /// The instance never was on `to`: its mods start fresh and the settings are copied.
    pub first_visit: bool,
    /// Mods put aside with the current set.
    pub parked_mods: u32,
    /// Size of the current set's mods and settings.
    pub parked_bytes: u64,
    /// The current set has a config folder (copied on a first visit).
    pub has_config: bool,
    /// Mods that come back with `to`'s set (or that were moved into it in advance).
    pub restored_mods: u32,
    pub restored_last_played_at: Option<String>,
    /// Official pack files the switch downloads.
    pub download_files: u32,
    pub download_bytes: u64,
    /// Recommended mods that stay on because they were on before (first visit).
    pub carried_choices: u32,
    /// Recommended mods `to` offers that the player has not seen yet.
    pub new_items: u32,
    /// The player's own mods in the current set (they stay with it).
    pub own_mods: u32,
    /// Minecraft `to` is not downloaded yet: it comes with the first launch.
    pub game_download: bool,
    pub loader: LoaderKind,
    pub loader_version: Option<String>,
    /// `to` has no set, but cleanup moved its old one to the launcher trash: "Eski seti geri
    /// getir" (`instance_restore_trashed_set`, then the switch) brings it back instead of a
    /// first visit.
    pub trashed_set: Option<SwitchTrashedSetView>,
}

/// A version's set in the launcher trash, as the switch would bring it back.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SwitchTrashedSetView {
    pub trash_id: String,
    /// Mods in it.
    pub mods: u32,
    /// Its mods and settings, in bytes.
    pub size_bytes: u64,
    pub last_played_at: Option<String>,
    /// Official pack files bringing it back would download.
    pub download_files: u32,
    pub download_bytes: u64,
}

/// The newest set of `minecraft` that cleanup moved to the launcher trash, unless it is about to
/// expire, with its folder inside the trash.
async fn trashed_set(
    state: &AppState,
    instance_id: &str,
    minecraft: &str,
) -> Option<(TrashEntry, PathBuf)> {
    let trash = LauncherTrash::new(state.root.join("trash"));
    let entry = trash
        .latest(instance_id, TrashKind::VersionSet, minecraft)
        .await
        .ok()
        .flatten()?;
    let retention = i64::try_from(TRASH_RETENTION.as_millis()).unwrap_or(i64::MAX);
    let now = jiff::Timestamp::now().as_millisecond();
    let item = trash.item_path(&entry);
    (entry.deleted_at_ms.saturating_add(retention) > now && item.is_dir()).then_some((entry, item))
}

/// What switching to `minecraft` would put aside, bring back and download.
#[tauri::command]
#[specta::specta]
pub async fn instance_switch_preview(
    state: State<'_, AppState>,
    instance_id: String,
    minecraft: String,
) -> CmdResult<SwitchPreviewView> {
    let mut instance = state
        .instances
        .get(&instance_id)
        .await
        .map_err(CmdError::from)?;
    ensure_switch_finished(&state, &mut instance).await?;
    let game_dir = state
        .instances
        .game_dir(&instance_id)
        .map_err(CmdError::from)?;
    let to = minecraft.trim().to_owned();
    let target = switch_target(&state, &instance, &game_dir, &to).await?;
    let sets = VersionSets::new(&game_dir);
    let target_set = sets.parked_info(&to).await.map_err(switch_error)?;
    let prepared = target_set.as_ref().is_some_and(|info| info.seeded);
    let returning = target_set.is_some() && !prepared;
    let active_state = PackState::load(&game_dir).await.map_err(pack_error)?;
    let overlay_jars = crate::overlay::owned_files(&game_dir);

    let mut preview = SwitchPreviewView {
        from: instance.mc_version.clone(),
        to: to.clone(),
        first_visit: !returning,
        parked_mods: count_mods(&game_dir.join("mods"), &overlay_jars).await,
        parked_bytes: sets.active_size().await,
        has_config: game_dir.join("config").is_dir(),
        restored_mods: if target_set.is_some() {
            count_mods(&sets.set_dir(&to).join("mods"), &overlay_jars).await
        } else {
            0
        },
        restored_last_played_at: target_set
            .as_ref()
            .filter(|_| returning)
            .and_then(|info| info.last_played_at)
            .map(|t| t.to_string()),
        download_files: 0,
        download_bytes: 0,
        carried_choices: 0,
        new_items: 0,
        own_mods: u32::try_from(
            own_mods(
                &game_dir,
                &game_dir.join("mods"),
                active_state.as_ref(),
                false,
            )
            .await
            .len(),
        )
        .unwrap_or(u32::MAX),
        game_download: !version_installed(&state, &to),
        loader: target.loader.kind,
        loader_version: target.loader.version.clone(),
        trashed_set: None,
    };
    if target_set.is_none()
        && let Some((entry, item)) = trashed_set(&state, &instance_id, &to).await
    {
        let (info, kept_state) = read_set_dir(&item).await.unwrap_or_default();
        let estimate = target
            .pack
            .as_ref()
            .map(|(_, manifest)| estimate_downloads(manifest, kept_state.as_ref()));
        preview.trashed_set = Some(SwitchTrashedSetView {
            trash_id: entry.id,
            mods: count_mods(&item.join("mods"), &overlay_jars).await,
            size_bytes: entry.size,
            last_played_at: info
                .and_then(|info| info.last_played_at)
                .map(|t| t.to_string()),
            download_files: estimate
                .as_ref()
                .map_or(0, |e| u32::try_from(e.files).unwrap_or(u32::MAX)),
            download_bytes: estimate.as_ref().map_or(0, |e| e.bytes),
        });
    }
    if let Some((_, manifest)) = &target.pack {
        let target_state = if returning {
            sets.parked_state(&to).await.map_err(switch_error)?
        } else {
            active_state.map(|active| {
                active
                    .switch_version(&instance.mc_version, &to, None)
                    .active
            })
        };
        let estimate = estimate_downloads(manifest, target_state.as_ref());
        preview.download_files = u32::try_from(estimate.files).unwrap_or(u32::MAX);
        preview.download_bytes = estimate.bytes;
        let resolution = resolve_items(manifest, target_state.as_ref(), None);
        let count = |f: &dyn Fn(&pl_content::ItemStatus) -> bool| {
            u32::try_from(resolution.items.iter().filter(|s| f(s)).count()).unwrap_or(u32::MAX)
        };
        if !returning {
            preview.carried_choices = count(&|s| s.selected && !s.library && !s.is_new);
        }
        preview.new_items = count(&|s| s.is_new);
    }
    Ok(preview)
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SwitchResultView {
    pub from: String,
    pub to: String,
    pub first_visit: bool,
    /// Recommended mods that stayed on because the player had them on before (first visit).
    pub carried: Vec<String>,
    /// Recommended mods the new version offers that the player has not seen yet.
    pub new_items: Vec<String>,
    /// The player's own mods in the set that was put aside (candidates for "Taşı").
    pub own_mods_left: u32,
    /// The official pack's files are in place (false: they come with the first launch).
    pub synced: bool,
}

/// Moves an instance to another Minecraft version: the current set is put aside, the target's
/// set comes back (or starts, carrying the choices over and copying `config/`), and for the
/// official pack the new set's files are installed right away when the network allows.
#[tauri::command]
#[specta::specta]
pub async fn instance_switch_version(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    minecraft: String,
    job_id: String,
    on_progress: Channel<JobProgressView>,
) -> CmdResult<SwitchResultView> {
    refuse_while_running(&state, &instance_id)?;
    let job = start_job(&app, &job_id);
    let _ = on_progress.send(JobProgressView::stage("preparing"));
    let lock = instance_lock(&app, &instance_id);
    let _guard = tokio::select! {
        _ = job.token.cancelled() => return Err(content_error(pl_mc::Error::Cancelled)),
        guard = lock.lock() => guard,
    };
    refuse_while_running(&state, &instance_id)?;
    let mut instance = state
        .instances
        .get(&instance_id)
        .await
        .map_err(CmdError::from)?;
    ensure_switch_finished(&state, &mut instance).await?;
    let to = minecraft.trim().to_owned();
    if to.is_empty() || to == instance.mc_version {
        return Err(CmdError::new(
            "SURUM-AYNI",
            pick(
                "Kurulum zaten bu sürümde.",
                "The instance is already on this version.",
            ),
            format!("already on {to}"),
        ));
    }
    let game_dir = state
        .instances
        .game_dir(&instance_id)
        .map_err(CmdError::from)?;
    let target = switch_target(&state, &instance, &game_dir, &to).await?;
    let sets = VersionSets::new(&game_dir);
    let returning_state = sets.parked_state(&to).await.ok().flatten();
    let context = SwitchContext {
        loader: Some(target.loader.kind),
        loader_version: target.loader.version.clone(),
        pack_id: target.pack.as_ref().map(|(pack, _)| pack.id.clone()),
        pack_version: returning_state
            .map(|state| state.version_id)
            .filter(|version| !version.is_empty()),
    };
    let pack_id = |instance: &Instance| {
        instance
            .managed_pack
            .as_ref()
            .map(|managed| managed.pack_id.clone())
            .unwrap_or_default()
    };
    // Sets made before per-version metadata existed take the instance's last session.
    let recorded = sets
        .active_info()
        .await
        .ok()
        .flatten()
        .is_some_and(|info| info.minecraft == instance.mc_version);
    let request = SwitchRequest {
        from: SetInfo {
            minecraft: instance.mc_version.clone(),
            loader: loader_name(instance.loader.kind),
            loader_version: instance.loader.version.clone().unwrap_or_default(),
            pack_id: pack_id(&instance),
            last_played_at: if recorded {
                None
            } else {
                instance.last_played_at
            },
            ..SetInfo::default()
        },
        to: SetInfo {
            minecraft: to.clone(),
            loader: loader_name(target.loader.kind),
            loader_version: target.loader.version.clone().unwrap_or_default(),
            pack_id: context.pack_id.clone().unwrap_or_default(),
            ..SetInfo::default()
        },
        context: serde_json::to_value(&context).unwrap_or_default(),
    };
    let _ = on_progress.send(JobProgressView::stage("switching"));
    let from = instance.mc_version.clone();
    let journal = {
        let _switching = SWITCHES.lock().await;
        let journal = sets.switch(request).await.map_err(switch_error)?;
        finalize(&state, &mut instance, &sets, &journal).await?;
        journal
    };
    tracing::info!(instance = %instance_id, %from, %to, first_visit = journal.first_visit, "switched Minecraft version");

    let own_mods_left = {
        let parked_state = sets.parked_state(&from).await.ok().flatten();
        let left = own_mods(
            sets.game_dir(),
            &sets.set_dir(&from).join("mods"),
            parked_state.as_ref(),
            false,
        )
        .await;
        u32::try_from(left.len()).unwrap_or(u32::MAX)
    };
    let mut result = SwitchResultView {
        from,
        to,
        first_visit: journal.first_visit,
        carried: Vec::new(),
        new_items: Vec::new(),
        own_mods_left,
        synced: target.pack.is_none(),
    };
    if let Some((_, manifest)) = &target.pack {
        let resolution = resolve_items(manifest, journal.active_state.as_ref(), None);
        let title = |id: &str| manifest.projects.item(id).map(|item| item.title.clone());
        if journal.first_visit {
            result.carried = resolution
                .items
                .iter()
                .filter(|s| s.selected && !s.library && !s.is_new)
                .filter_map(|s| title(&s.id))
                .collect();
        }
        result.new_items = resolution
            .items
            .iter()
            .filter(|s| s.is_new)
            .filter_map(|s| title(&s.id))
            .collect();
        match sync_set(
            &state,
            &mut instance,
            &game_dir,
            manifest,
            &on_progress,
            &job.token,
        )
        .await
        {
            Ok(()) => result.synced = true,
            Err(error) => {
                tracing::warn!(?error, "the new set's pack files wait for the first launch")
            }
        }
    }
    Ok(result)
}

/// "Eski seti geri getir": puts the version's set that cleanup moved to the launcher trash back
/// as its parked set, so the switch that follows brings it back exactly instead of starting
/// fresh. Runs under the instance lock and the switch lock (never during a switch, and only
/// after an unfinished one completed); refused while the game runs or when the version has a
/// set again (it then stays in the trash). Remembered like cleanup's "Geri al".
#[tauri::command]
#[specta::specta]
pub async fn instance_restore_trashed_set(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    trash_id: String,
) -> CmdResult<()> {
    refuse_while_running(&state, &instance_id)?;
    let gone = |detail: String| {
        CmdError::new(
            "SET-COP-YOK",
            pick(
                "Eski set artık çöp kutusunda değil.",
                "The old set isn't in the trash anymore.",
            ),
            detail,
        )
    };
    let trash = LauncherTrash::new(state.root.join("trash"));
    let entry = trash
        .get(&trash_id)
        .await
        .map_err(|e| gone(e.to_string()))?;
    if entry.kind != TrashKind::VersionSet || entry.instance_id != instance_id {
        return Err(gone(format!(
            "{trash_id} is not a version set of {instance_id}"
        )));
    }
    // Not while the data folder moves or a cleanup runs.
    let _data_job =
        crate::commands::data::begin_job(&app, crate::commands::data::DataJob::Cleaning)?;
    let lock = instance_lock(&app, &instance_id);
    let _guard = lock.lock().await;
    refuse_while_running(&state, &instance_id)?;
    let mut instance = state
        .instances
        .get(&instance_id)
        .await
        .map_err(CmdError::from)?;
    ensure_switch_finished(&state, &mut instance).await?;
    let game_dir = state
        .instances
        .game_dir(&instance_id)
        .map_err(CmdError::from)?;
    let _switching = SWITCHES.lock().await;
    let sets = VersionSets::new(&game_dir);
    let home = sets.set_dir(&entry.name);
    let taken =
        instance.mc_version == entry.name || tokio::fs::try_exists(&home).await.unwrap_or(true);
    if taken {
        return Err(CmdError::new(
            "SET-DOLU",
            pick(
                "Bu sürümün artık yeni bir seti var; eski set çöp kutusunda kaldı.",
                "This version has a new set now; the old one stays in the trash.",
            ),
            format!("{} has a set", entry.name),
        ));
    }
    let restored = trash.restore(&trash_id).await.map_err(|e| {
        CmdError::new(
            "SET-GERI",
            pick(
                "Eski set geri getirilemedi. Tekrar dene.",
                "Couldn't bring the old set back. Try again.",
            ),
            e.to_string(),
        )
    })?;
    if restored != home {
        // Only when the trash kept another place for it (a folder moved by hand).
        tracing::warn!(from = %restored.display(), to = %home.display(), "moving the restored set where the switch expects it");
        tokio::fs::rename(&restored, &home).await.map_err(|e| {
            CmdError::new(
                "SET-GERI",
                pick(
                    "Eski set geri getirilemedi. Tekrar dene.",
                    "Couldn't bring the old set back. Try again.",
                ),
                e.to_string(),
            )
        })?;
    }
    tracing::info!(instance = %instance_id, minecraft = %entry.name, "put a version set back from the trash before switching to it");
    crate::commands::cleanup::remember_kept_set(&state, &instance_id, &entry.name).await;
    Ok(())
}

/// Installs the active set's pack files (after a switch).
async fn sync_set(
    state: &AppState,
    instance: &mut Instance,
    game_dir: &Path,
    manifest: &PackManifest,
    on_progress: &Channel<JobProgressView>,
    cancel: &CancellationToken,
) -> CmdResult<()> {
    let previous = PackState::load(game_dir).await.map_err(pack_error)?;
    let plan = plan_sync(manifest, game_dir, previous.as_ref(), None, &quick())
        .await
        .map_err(pack_error)?;
    if !plan.is_noop() {
        apply_sync(
            &plan,
            &state.mc.downloader,
            &sync_options(cancel),
            Some(sync_progress(on_progress.clone())),
        )
        .await
        .map_err(pack_error)?;
    }
    if let Some(managed) = instance.managed_pack.as_mut()
        && managed.version != manifest.version_id
    {
        managed.version.clone_from(&manifest.version_id);
        state
            .instances
            .update(instance)
            .await
            .map_err(CmdError::from)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Moving own mods to another version

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum MigrationStatus {
    /// Modrinth has a version for the target: it can be moved.
    Compatible,
    /// The target set already has this mod.
    AlreadyThere,
    /// Modrinth knows the mod but has no version for the target.
    NoVersion,
    /// Modrinth does not know the file (added by hand from elsewhere).
    Unknown,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MigrationModView {
    pub file_name: String,
    pub title: String,
    pub project_id: Option<String>,
    pub current_version: Option<String>,
    /// The version installed in the target set when moving.
    pub target_version_id: Option<String>,
    pub target_version_number: Option<String>,
    pub size_bytes: u64,
    pub status: MigrationStatus,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MigrationPlanView {
    pub from: String,
    pub to: String,
    pub mods: Vec<MigrationModView>,
    /// Modrinth could not be reached: nothing can be checked now.
    pub offline: bool,
}

/// Where a version's set is: the game directory for the active version, else its parked
/// folder. Returns the folder that holds `mods/`, its pack state and whether it is active.
async fn set_location(
    sets: &VersionSets,
    instance: &Instance,
    minecraft: &str,
) -> CmdResult<(PathBuf, Option<PackState>, bool)> {
    if minecraft == instance.mc_version {
        let state = PackState::load(sets.game_dir()).await.map_err(pack_error)?;
        return Ok((sets.game_dir().to_path_buf(), state, true));
    }
    let state = sets.parked_state(minecraft).await.map_err(switch_error)?;
    Ok((sets.set_dir(minecraft), state, false))
}

/// The loader a version of the instance uses: the official pack's member for it (when the
/// content is at hand), otherwise the instance's own loader.
async fn loader_for(
    state: &AppState,
    instance: &Instance,
    game_dir: &Path,
    minecraft: &str,
) -> LoaderKind {
    if minecraft == instance.mc_version || instance.managed_pack.is_none() {
        return instance.loader.kind;
    }
    match switch_target(state, instance, game_dir, minecraft).await {
        Ok(target) => target.loader.kind,
        Err(_) => instance.loader.kind,
    }
}

/// Which of the player's own mods in `from`'s set (the active one by default) have a version
/// for `to` on Modrinth: by project when known, else identified by the file's hash.
#[tauri::command]
#[specta::specta]
pub async fn mods_migration_plan(
    state: State<'_, AppState>,
    instance_id: String,
    from_minecraft: Option<String>,
    to_minecraft: String,
) -> CmdResult<MigrationPlanView> {
    let mut instance = state
        .instances
        .get(&instance_id)
        .await
        .map_err(CmdError::from)?;
    ensure_switch_finished(&state, &mut instance).await?;
    let game_dir = state
        .instances
        .game_dir(&instance_id)
        .map_err(CmdError::from)?;
    let sets = VersionSets::new(&game_dir);
    let from = from_minecraft
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| instance.mc_version.clone());
    let to = to_minecraft.trim().to_owned();
    let mut plan = MigrationPlanView {
        from: from.clone(),
        to: to.clone(),
        mods: Vec::new(),
        offline: false,
    };
    if from == to {
        return Ok(plan);
    }
    let (source_dir, source_state, _) = set_location(&sets, &instance, &from).await?;
    let own = own_mods(
        &game_dir,
        &source_dir.join("mods"),
        source_state.as_ref(),
        true,
    )
    .await;
    if own.is_empty() {
        return Ok(plan);
    }
    let (target_dir, _, _) = set_location(&sets, &instance, &to).await?;
    let target_hashes: Vec<String> = pl_mc::mods::list_mods(&target_dir.join("mods"), true)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|m| m.sha1)
        .collect();
    let loader = loader_for(&state, &instance, &game_dir, &to).await;

    let own_hashes: Vec<String> = own.iter().filter_map(|m| m.sha1.clone()).collect();
    let mut all_hashes = own_hashes.clone();
    all_hashes.extend(target_hashes.iter().cloned());
    let client = modrinth_client(&state);
    let known = match client
        .version_files(&all_hashes, ModrinthHashAlgorithm::Sha1)
        .await
    {
        Ok(known) => known,
        Err(error) => {
            tracing::info!(%error, "Modrinth unreachable for the migration plan");
            plan.offline = true;
            HashMap::new()
        }
    };
    let latest = if plan.offline {
        HashMap::new()
    } else {
        let identified: Vec<String> = own_hashes
            .iter()
            .filter(|hash| known.contains_key(*hash))
            .cloned()
            .collect();
        if identified.is_empty() {
            HashMap::new()
        } else {
            match client
                .update_check(
                    &identified,
                    ModrinthHashAlgorithm::Sha1,
                    &compatible_loaders(loader, &to),
                    std::slice::from_ref(&to),
                )
                .await
            {
                Ok(latest) => latest,
                Err(error) => {
                    tracing::info!(%error, "Modrinth update check failed for the migration plan");
                    plan.offline = true;
                    HashMap::new()
                }
            }
        }
    };
    let in_target: HashSet<&str> = target_hashes
        .iter()
        .filter_map(|hash| known.get(hash))
        .map(|version| version.project_id.as_str())
        .collect();

    plan.mods = own
        .iter()
        .map(|m| {
            let sha1 = m.sha1.as_deref().unwrap_or_default();
            let current = known.get(sha1);
            let newest = latest.get(sha1);
            let status = match current {
                _ if plan.offline => MigrationStatus::Unknown,
                None => MigrationStatus::Unknown,
                Some(version) if in_target.contains(version.project_id.as_str()) => {
                    MigrationStatus::AlreadyThere
                }
                Some(version) => match newest {
                    Some(newest) if newest.project_id == version.project_id => {
                        MigrationStatus::Compatible
                    }
                    _ => MigrationStatus::NoVersion,
                },
            };
            let compatible = status == MigrationStatus::Compatible;
            MigrationModView {
                file_name: m.file_name.clone(),
                title: m.display_name().to_owned(),
                project_id: current.map(|version| version.project_id.clone()),
                current_version: current
                    .map(|version| version.version_number.clone())
                    .or_else(|| m.metadata.as_ref().and_then(|meta| meta.version.clone())),
                target_version_id: newest.filter(|_| compatible).map(|v| v.id.clone()),
                target_version_number: newest
                    .filter(|_| compatible)
                    .map(|v| v.version_number.clone()),
                size_bytes: m.size,
                status,
            }
        })
        .collect();
    Ok(plan)
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MigrationMoveView {
    pub title: String,
    /// The Modrinth version that moves the mod ([`mods_migrate`]).
    pub version_id: String,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MigrationCheckView {
    pub from: String,
    pub to: String,
    /// The player's own mods in `from`'s set.
    pub own: u32,
    /// The own mods with a version for `to` (one entry per mod).
    pub compatible: Vec<MigrationMoveView>,
    /// Own mods the target set already has.
    pub already_there: u32,
    /// Modrinth could not be reached: the full search ([`mods_migration_plan`]) stays on demand.
    pub offline: bool,
}

/// The page after a first visit: which of the player's own mods in `from`'s set have a version
/// for `to`, with one Modrinth request (`version_files/update`) for all files at once.
#[tauri::command]
#[specta::specta]
pub async fn mods_migration_check(
    state: State<'_, AppState>,
    instance_id: String,
    from_minecraft: String,
    to_minecraft: String,
) -> CmdResult<MigrationCheckView> {
    let mut instance = state
        .instances
        .get(&instance_id)
        .await
        .map_err(CmdError::from)?;
    ensure_switch_finished(&state, &mut instance).await?;
    let game_dir = state
        .instances
        .game_dir(&instance_id)
        .map_err(CmdError::from)?;
    let sets = VersionSets::new(&game_dir);
    let from = from_minecraft.trim().to_owned();
    let to = to_minecraft.trim().to_owned();
    let mut check = MigrationCheckView {
        from: from.clone(),
        to: to.clone(),
        own: 0,
        compatible: Vec::new(),
        already_there: 0,
        offline: false,
    };
    if from.is_empty() || to.is_empty() || from == to {
        return Ok(check);
    }
    let (source_dir, source_state, _) = set_location(&sets, &instance, &from).await?;
    let own = own_mods(
        &game_dir,
        &source_dir.join("mods"),
        source_state.as_ref(),
        true,
    )
    .await;
    check.own = u32::try_from(own.len()).unwrap_or(u32::MAX);
    if own.is_empty() {
        return Ok(check);
    }
    let (target_dir, _, _) = set_location(&sets, &instance, &to).await?;
    let target_files: Vec<String> = pl_mc::mods::list_mods(&target_dir.join("mods"), true)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|m| m.sha1)
        .collect();
    let loader = loader_for(&state, &instance, &game_dir, &to).await;
    let own_hashes: Vec<String> = own
        .iter()
        .map(|m| m.sha1.clone().unwrap_or_default())
        .collect();
    let fits = match modrinth_client(&state)
        .migration_check(
            &own_hashes,
            &target_files,
            &ModrinthResolveTarget::for_loader(loader, to.clone()),
        )
        .await
    {
        Ok(fits) => fits,
        Err(error) => {
            tracing::info!(%error, "Modrinth unreachable for the migration check");
            check.offline = true;
            return Ok(check);
        }
    };
    check.compatible = migration_moves(&fits)
        .into_iter()
        .filter_map(|(index, version)| {
            Some(MigrationMoveView {
                title: own.get(index)?.display_name().to_owned(),
                version_id: version.id.clone(),
            })
        })
        .collect();
    let there = fits
        .iter()
        .filter(|fit| matches!(fit, MigrationFit::AlreadyThere))
        .count();
    check.already_there = u32::try_from(there).unwrap_or(u32::MAX);
    Ok(check)
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MigrationResultView {
    pub installed: u32,
    pub already_installed: u32,
    /// Required dependencies that could not be found for the target.
    pub unresolved: u32,
}

/// Installs the given Modrinth versions (from [`mods_migration_plan`]) into `to`'s set: the
/// active `mods/`, a parked set, or a version the instance has not visited yet (prepared so
/// the first switch brings them).
#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
pub async fn mods_migrate(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    to_minecraft: String,
    version_ids: Vec<String>,
    job_id: String,
    on_progress: Channel<JobProgressView>,
) -> CmdResult<MigrationResultView> {
    let job = start_job(&app, &job_id);
    let _ = on_progress.send(JobProgressView::stage("resolving"));
    let lock = instance_lock(&app, &instance_id);
    let _guard = tokio::select! {
        _ = job.token.cancelled() => return Err(content_error(pl_mc::Error::Cancelled)),
        guard = lock.lock() => guard,
    };
    let mut instance = state
        .instances
        .get(&instance_id)
        .await
        .map_err(CmdError::from)?;
    ensure_switch_finished(&state, &mut instance).await?;
    let to = to_minecraft.trim().to_owned();
    let game_dir = state
        .instances
        .game_dir(&instance_id)
        .map_err(CmdError::from)?;
    let instance_dir = state
        .instances
        .instance_dir(&instance_id)
        .map_err(CmdError::from)?;
    let sets = VersionSets::new(&game_dir);
    let loader = loader_for(&state, &instance, &game_dir, &to).await;
    let active = to == instance.mc_version;
    if active {
        refuse_while_running(&state, &instance_id)?;
    }
    let (target_dir, index_path) = if active {
        (game_dir.clone(), addon_index_path(&instance_dir))
    } else {
        let dir = sets
            .prepare(SetInfo {
                minecraft: to.clone(),
                loader: loader_name(loader),
                ..SetInfo::default()
            })
            .await
            .map_err(switch_error)?;
        (dir, parked_addons_path(&instance_dir, &to))
    };
    let ids: Vec<String> = {
        let mut ids = version_ids;
        ids.sort();
        ids.dedup();
        ids
    };
    if ids.is_empty() {
        return Ok(MigrationResultView {
            installed: 0,
            already_installed: 0,
            unresolved: 0,
        });
    }
    let client = modrinth_client(&state);
    let versions = client.versions(&ids).await.map_err(content_error)?;
    let addons = Addons::new(target_dir, index_path).with_icon_dir(icon_dir(&state));
    let events = on_progress.clone();
    let report = addons
        .install(
            &client,
            &state.mc.downloader,
            ImportKind::Mod,
            versions,
            &ModrinthResolveTarget::for_loader(loader, to.clone()),
            AddonInstallOptions {
                with_dependencies: true,
                concurrency: 6,
                cancel: job.token.clone(),
                progress: Some(std::sync::Arc::new(move |p: AddonProgress| {
                    let view = match p {
                        AddonProgress::Resolving => JobProgressView::stage("resolving"),
                        AddonProgress::Downloading {
                            done_files,
                            total_files,
                            done_bytes,
                            total_bytes,
                            bytes_per_sec,
                        } => JobProgressView {
                            stage: "downloading".to_owned(),
                            percent: percent(done_bytes, total_bytes, done_files, total_files),
                            done_files,
                            total_files,
                            bytes_per_sec,
                        },
                        AddonProgress::Finishing => JobProgressView::stage("finishing"),
                    };
                    let _ = events.send(view);
                })),
            },
        )
        .await
        .map_err(content_error)?;
    tracing::info!(instance = %instance_id, %to, installed = report.installed.len(), "own mods moved to another version");
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    Ok(MigrationResultView {
        installed: count(report.installed.len()),
        already_installed: count(report.already_installed.len()),
        unresolved: count(report.unresolved.len()),
    })
}
