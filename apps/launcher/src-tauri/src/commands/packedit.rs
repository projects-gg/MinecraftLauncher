//! PACK-EDIT: the player's own versions of the official pack's mods (`pl_content::overrides`).
//!
//! - `features.packEdit` ([`apply_flag`], [`on`]): "Sürümü Değiştir" in the mod center
//!   ([`pack_item_versions`], [`pack_item_use_version`]) and the upgrades the launcher makes by
//!   itself when a mod of the player's needs a pack library newer ([`upgrades_wanted`],
//!   [`choose_upgrades`]; before a launch in `modhealth::autofix`, in a repair through the host).
//!   With the flag off an override already made still works and can be undone ("Paketin
//!   Sürümüne Dön" is a mod center revert).
//! - [`disk_mods`]: the enabled jars of a set with their mod ids, for every pack sync. A jar of
//!   the player's that declares a pack item's mod stands in for the item (not a feature: the
//!   loader would refuse the mod twice).
//! - [`follow_pack`]: before a launch's sync, an override whose item's file the pack changed ends
//!   when the pack's version is at least as new (one Modrinth request, only then).

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use pl_content::{
    DiskJar, FABRIC_API_PROJECT, FollowOutcome, ItemOverride, OverrideReason, PackManifest,
    PackSelection, PackState, is_fabric_api, pending_comparisons,
};
use pl_lang::pick;
use pl_mc::loaders::LoaderKind;
use pl_mc::modrinth::{ModrinthVersion, ModrinthVersionType, newest_meeting};
use pl_mc::mods::check::{FileOrigin, FileOrigins, IssueKind, ModCheckReport, ModFix, ModRef};
use pl_mc::mods::scan::{DescriptorKind, ModScanner, strip_disabled};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, State};

use crate::commands::modcenter::{
    ModCenterApplyView, PackSource, apply_selection, not_available, pack_source,
};
use crate::commands::modrinth::{
    JobProgressView, content_error, instance_lock, modrinth_client, start_job,
};
use crate::commands::official::pack_error;
use crate::commands::versions::{ensure_switch_finished, refuse_while_running};
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

static FLAG: AtomicBool = AtomicBool::new(false);

/// The signed config's `features.packEdit`, from every applied content snapshot.
pub(crate) fn apply_flag(on: bool) {
    FLAG.store(on, Ordering::Relaxed);
}

/// Whether `features.packEdit` is on.
pub(crate) fn on() -> bool {
    FLAG.load(Ordering::Relaxed)
}

/// How many versions "Sürümü Değiştir" lists (newest first).
const LISTED_VERSIONS: usize = 40;

// --- the mods folder ---------------------------------------------------------------------------

/// The jars of `<game dir>/mods` (enabled and `.disabled` ones) with the Fabric or Quilt mod
/// ids each declares at its top level and provides, through the shared scanner (a warm scan
/// costs a listing and a `stat` per jar). The official pack is a Fabric pack: a jar made for
/// another loader never stands in for its mods.
pub(crate) async fn disk_mods(game_dir: &Path) -> Vec<DiskJar> {
    let dir = game_dir.join("mods");
    tokio::task::spawn_blocking(move || {
        ModScanner::shared()
            .scan_dir(&dir)
            .jars
            .iter()
            .filter_map(|jar| {
                let ids: Vec<String> = jar
                    .contents
                    .mods
                    .iter()
                    .filter(|m| matches!(m.descriptor, DescriptorKind::Fabric | DescriptorKind::Quilt))
                    .flat_map(|m| std::iter::once(m.id.clone()).chain(m.provides.iter().cloned()))
                    .collect();
                (!ids.is_empty()).then(|| DiskJar {
                    path: format!("mods/{}", strip_disabled(&jar.file_name)),
                    ids,
                    enabled: jar.enabled,
                })
            })
            .collect::<Vec<DiskJar>>()
    })
    .await
    .unwrap_or_else(|error| {
        tracing::warn!(%error, "mods folder not read for the pack sync");
        Vec::new()
    })
}

// --- following the pack ------------------------------------------------------------------------

/// Overrides whose item got another file in `manifest` follow the pack again once its version
/// is at least as new as theirs (Modrinth's publish dates). Offline, or when Modrinth does not
/// answer, they stay and are compared on the next launch.
pub(crate) async fn follow_pack(state: &AppState, manifest: &PackManifest, game_dir: &Path) {
    let Ok(Some(mut pack_state)) = PackState::load(game_dir).await else {
        return;
    };
    let pending = pending_comparisons(manifest, &pack_state);
    if pending.is_empty() {
        return;
    }
    let ids: Vec<String> = pending
        .iter()
        .filter_map(|p| p.pack_version_id.clone())
        .collect();
    let published: HashMap<String, String> = if ids.is_empty() {
        HashMap::new()
    } else {
        match modrinth_client(state).versions(&ids).await {
            Ok(list) => list
                .into_iter()
                .map(|version| (version.id, version.date_published))
                .collect(),
            Err(error) => {
                tracing::info!(%error, "pack versions not compared with the player's ones yet");
                return;
            }
        }
    };
    for pending in &pending {
        let date = pending
            .pack_version_id
            .as_ref()
            .and_then(|id| published.get(id))
            .map(String::as_str);
        let outcome =
            pl_content::follow_pack(&mut pack_state, &pending.item, &pending.pack_sha1, date);
        if outcome == FollowOutcome::Following {
            tracing::info!(item = %pending.item, "the pack caught up with the player's version");
        }
    }
    if let Err(error) = pack_state.save(game_dir).await {
        tracing::warn!(%error, "pack state not saved after comparing versions");
    }
}

// --- upgrades a mod needs ----------------------------------------------------------------------

/// A pack file a mod of the player's needs in a newer version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Upgrade {
    /// The pack path (`mods/fabric-api.jar`).
    pub path: String,
    /// Every requirement the new version must meet, in the loader's syntax.
    pub version_reqs: Vec<String>,
    /// The mods that need it, by name.
    pub needed_by: Vec<String>,
}

/// The pack files the check says a mod needs newer (a dependency in a version too old for it):
/// one entry per file, every requirement and every mod that asks.
pub(crate) fn upgrades_wanted(
    report: &ModCheckReport,
    origins: &FileOrigins,
    name: impl Fn(&ModRef) -> String,
) -> Vec<Upgrade> {
    let mut out: Vec<Upgrade> = Vec::new();
    for issue in report
        .issues
        .iter()
        .filter(|issue| issue.kind == IssueKind::DependencyVersion)
    {
        let Some(ModFix::UpdateToCompatible {
            file,
            version_req: Some(req),
            ..
        }) = issue.fixes.first()
        else {
            continue;
        };
        let Some(FileOrigin::Pack { path }) = origins.get(strip_disabled(file)) else {
            continue;
        };
        let entry = match out.iter_mut().position(|u| u.path == *path) {
            Some(index) => &mut out[index],
            None => {
                out.push(Upgrade {
                    path: path.clone(),
                    version_reqs: Vec::new(),
                    needed_by: Vec::new(),
                });
                out.last_mut().expect("just pushed")
            }
        };
        if !entry.version_reqs.contains(req) {
            entry.version_reqs.push(req.clone());
        }
        for m in &issue.mods {
            let named = name(m);
            if !entry.needed_by.contains(&named) {
                entry.needed_by.push(named);
            }
        }
    }
    out
}

/// A pack mod the launcher moved to a newer version for a mod of the player's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UpgradedView {
    /// The pack mod's title.
    pub name: String,
    /// Its new version number.
    pub version: String,
    /// The mods that needed it, by name.
    pub needed_by: Vec<String>,
}

/// The Modrinth project a pack item installs.
fn project_of(manifest: &PackManifest, item_id: &str) -> Option<String> {
    let item = manifest.projects.item(item_id)?;
    match &item.modrinth {
        Some(modrinth) => Some(modrinth.project_id.clone()),
        None if is_fabric_api(item) => Some(FABRIC_API_PROJECT.to_owned()),
        None => None,
    }
}

/// The pack's own SHA-1 for an item's file.
fn pack_sha1(manifest: &PackManifest, item_id: &str) -> String {
    manifest
        .projects
        .item(item_id)
        .and_then(|item| manifest.file(&item.path))
        .map(|file| file.hashes.sha1.to_ascii_lowercase())
        .unwrap_or_default()
}

/// The override for `version` of an item, when its primary file can be installed as one.
fn override_from(
    version: &ModrinthVersion,
    pack_sha1: String,
    reason: OverrideReason,
    needed_by: Vec<String>,
) -> Option<ItemOverride> {
    let file = version.primary_file()?;
    let choice = ItemOverride {
        path: format!("mods/{}", file.filename),
        sha1: file.hashes.sha1.clone()?.to_ascii_lowercase(),
        sha512: file.hashes.sha512.as_ref().map(|h| h.to_ascii_lowercase()),
        size: file.size,
        url: file.url.clone(),
        version_id: version.id.clone(),
        version_number: version.version_number.clone(),
        published: Some(version.date_published.clone()),
        pack_sha1,
        reason,
        needed_by,
    };
    choice.usable().then_some(choice)
}

/// Writes overrides into `pack_state` for every upgrade a fitting Modrinth version exists for.
/// Returns what moved and the pack paths left as they were. Nothing is saved or downloaded.
pub(crate) async fn choose_upgrades(
    state: &AppState,
    manifest: &PackManifest,
    (minecraft, loader): (&str, LoaderKind),
    pack_state: &mut PackState,
    wants: &[Upgrade],
) -> (Vec<UpgradedView>, Vec<String>) {
    let client = modrinth_client(state);
    let mut upgraded = Vec::new();
    let mut left = Vec::new();
    let loaders: Vec<String> = loader
        .modrinth_name()
        .map(str::to_owned)
        .into_iter()
        .collect();
    for want in wants {
        let record = pack_state.files.get(&want.path);
        let installed_sha1 = record.map(|record| record.sha1.clone());
        let item = record.and_then(|record| record.item.clone());
        let found = match item
            .as_deref()
            .and_then(|id| Some((id, project_of(manifest, id)?)))
        {
            Some((id, project)) => match client
                .project_versions(&project, &loaders, &[minecraft.to_owned()])
                .await
            {
                Ok(versions) => newest_meeting(&versions, loader, &want.version_reqs)
                    .and_then(|version| {
                        override_from(
                            version,
                            pack_sha1(manifest, id),
                            OverrideReason::Needed,
                            want.needed_by.clone(),
                        )
                    })
                    // The version installed already: nothing to move (the check reads the
                    // jar's own version, which the Modrinth number did not tell).
                    .filter(|choice| {
                        installed_sha1
                            .as_deref()
                            .is_none_or(|sha1| !sha1.eq_ignore_ascii_case(&choice.sha1))
                    })
                    .map(|choice| (id.to_owned(), choice)),
                Err(error) => {
                    tracing::info!(%error, project, "no versions for a pack upgrade");
                    None
                }
            },
            None => None,
        };
        match found {
            Some((id, choice)) => {
                let name = manifest
                    .projects
                    .item(&id)
                    .map_or_else(|| id.clone(), |item| item.title.clone());
                tracing::info!(item = %id, version = %choice.version_number, "pack mod upgraded for a mod that needs it");
                upgraded.push(UpgradedView {
                    name,
                    version: choice.version_number.clone(),
                    needed_by: want.needed_by.clone(),
                });
                pack_state.overrides.insert(id, choice);
            }
            None => left.push(want.path.clone()),
        }
    }
    (upgraded, left)
}

// --- "Sürümü Değiştir" -------------------------------------------------------------------------

/// One Modrinth version of a pack mod, for "Sürümü Değiştir".
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PackItemVersionView {
    /// The Modrinth version id.
    pub id: String,
    /// Its version number (`0.141.1+26.1.2`).
    pub number: String,
    /// When it was published (RFC 3339).
    pub published: String,
    pub size_bytes: u64,
    /// A release (not a beta or an alpha).
    pub stable: bool,
    /// The pack's own version.
    pub pack: bool,
    /// The version installed for the player now.
    pub current: bool,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PackItemVersionsView {
    /// Newest first.
    pub versions: Vec<PackItemVersionView>,
}

fn no_versions() -> CmdError {
    CmdError::new(
        "SURUM-YOK",
        pick(
            "Bu modun başka sürümü bulunamadı.",
            "No other version of this mod was found.",
        ),
        "the pack item has no Modrinth project",
    )
}

fn wrong_version() -> CmdError {
    CmdError::new(
        "SURUM-UYMUYOR",
        pick(
            "Bu sürüm bu Minecraft sürümüne uymuyor.",
            "This version doesn't fit this Minecraft version.",
        ),
        "the version is for another project, game version or loader",
    )
}

/// The Modrinth versions of a pack mod for the instance's Minecraft version and loader.
#[tauri::command]
#[specta::specta]
pub async fn pack_item_versions(
    state: State<'_, AppState>,
    instance_id: String,
    item_id: String,
) -> CmdResult<PackItemVersionsView> {
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
    let PackSource::Ready { manifest, .. } = pack_source(&state, &instance, &game_dir).await else {
        return Err(not_available());
    };
    let project = project_of(&manifest, &item_id).ok_or_else(no_versions)?;
    let pack_version = manifest
        .projects
        .item(&item_id)
        .and_then(|item| item.modrinth.as_ref())
        .map(|m| m.version_id.clone());
    let chosen = PackState::load(&game_dir)
        .await
        .map_err(pack_error)?
        .and_then(|pack_state| pack_state.overrides.get(&item_id).cloned());
    let current = chosen
        .map(|choice| choice.version_id)
        .or_else(|| pack_version.clone());
    let loaders: Vec<String> = instance
        .loader
        .kind
        .modrinth_name()
        .map(str::to_owned)
        .into_iter()
        .collect();
    let versions = modrinth_client(&state)
        .project_versions(
            &project,
            &loaders,
            std::slice::from_ref(&instance.mc_version),
        )
        .await
        .map_err(content_error)?;
    Ok(PackItemVersionsView {
        versions: versions
            .into_iter()
            .filter(|v| v.primary_file().is_some_and(|f| f.hashes.sha1.is_some()))
            .take(LISTED_VERSIONS)
            .map(|v| PackItemVersionView {
                size_bytes: v.primary_file().map_or(0, |f| f.size),
                stable: v.version_type == ModrinthVersionType::Release,
                pack: pack_version.as_deref() == Some(v.id.as_str()),
                current: current.as_deref() == Some(v.id.as_str()),
                id: v.id,
                number: v.version_number,
                published: v.date_published,
            })
            .collect(),
    })
}

/// Installs `version_id` of a pack mod for the player instead of the pack's (`None`, or the
/// pack's own version: back to the pack's), then syncs like the mod center's "Uygula". Refused
/// while the instance's game runs.
#[tauri::command]
#[specta::specta]
pub async fn pack_item_use_version(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    item_id: String,
    version_id: Option<String>,
    job_id: String,
    on_progress: Channel<JobProgressView>,
) -> CmdResult<ModCenterApplyView> {
    refuse_while_running(&state, &instance_id)?;
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
    let game_dir = state
        .instances
        .game_dir(&instance_id)
        .map_err(CmdError::from)?;
    let PackSource::Ready { manifest, .. } = pack_source(&state, &instance, &game_dir).await else {
        return Err(not_available());
    };
    let item = manifest.projects.item(&item_id).ok_or_else(no_versions)?;
    let pack_version = item.modrinth.as_ref().map(|m| m.version_id.clone());
    let mut selection = PackSelection::default();
    match version_id.filter(|id| Some(id) != pack_version.as_ref()) {
        None => {
            selection.revert.insert(item_id.clone());
        }
        Some(version_id) => {
            let project = project_of(&manifest, &item_id).ok_or_else(no_versions)?;
            let version = modrinth_client(&state)
                .version(&version_id)
                .await
                .map_err(content_error)?;
            let loader = instance.loader.kind.modrinth_name().unwrap_or_default();
            if version.project_id != project
                || !version.game_versions.contains(&instance.mc_version)
                || !version.loaders.iter().any(|l| l == loader)
            {
                return Err(wrong_version());
            }
            let choice = override_from(
                &version,
                pack_sha1(&manifest, &item_id),
                OverrideReason::Chosen,
                Vec::new(),
            )
            .ok_or_else(wrong_version)?;
            let mut pack_state = PackState::load(&game_dir)
                .await
                .map_err(pack_error)?
                .unwrap_or_default();
            pack_state.overrides.insert(item_id.clone(), choice);
            pack_state.save(&game_dir).await.map_err(pack_error)?;
            tracing::info!(instance = %instance_id, item = %item_id, version = %version.version_number, "the player picked a version of a pack mod");
        }
    }
    apply_selection(
        &state,
        &instance_id,
        selection,
        Some(on_progress),
        &job.token,
    )
    .await
}
