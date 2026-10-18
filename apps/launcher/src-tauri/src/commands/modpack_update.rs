//! Updating an instance made from a Modrinth modpack (`pl_mc::modpack::update` decides): which
//! pack it was made from, whether a newer version exists, what an update would change, and the
//! update itself. Few requests: one version list per check (only when the instance page asks),
//! the new `.mrpack`, and one project lookup for names.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use pl_lang::pick;
use pl_mc::addons::{AddonIndex, AddonSource, index_key};
use pl_mc::instance::Instance;
use pl_mc::modpack::{
    KeptReason, ModpackState, PackContents, PackFileSource, UpdatePlan, disk_state,
    pack_update_candidate, plan_update, read_pack_contents, remove_dropped, update_download_items,
    write_bundled,
};
use pl_mc::modrinth::ModrinthVersion;
use pl_net::{CancellationToken, DownloadOptions, ProgressFn, VerifyMode};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, State};

use crate::commands::modpack::{download_pack_version, modpack_state_path};
use crate::commands::modrinth::{
    JobProgressView, addon_index_path, addons_for, content_error, instance_lock, modrinth_client,
    start_job,
};
use crate::commands::versions::refuse_while_running;
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

/// The Modrinth modpack an instance was made from.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModpackInfoView {
    pub project_id: String,
    pub version_id: String,
    pub version_number: Option<String>,
    pub title: Option<String>,
    pub icon_url: Option<String>,
}

/// A newer version of an instance's modpack.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModpackUpdateView {
    pub version_id: String,
    pub version_number: String,
    pub name: String,
    /// ISO 8601.
    pub published_at: String,
}

/// One file an update adds, changes, removes or leaves alone.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PackChangeView {
    /// Game-relative path.
    pub path: String,
    /// The Modrinth project's title when known, else the file name.
    pub name: String,
    /// First folder of the path (`mods`, `resourcepacks`, `shaderpacks`, `config`, …; empty at
    /// the top).
    pub folder: String,
    pub icon_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct KeptChangeView {
    pub item: PackChangeView,
    pub reason: KeptReason,
}

/// What updating the pack to a version would do.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModpackUpdatePlanView {
    pub from_version: Option<String>,
    pub to_version: String,
    pub added: Vec<PackChangeView>,
    pub updated: Vec<PackChangeView>,
    pub removed: Vec<PackChangeView>,
    /// Pack files the player changed, turned off or deleted (and files of theirs where the pack
    /// puts one): they stay as they are.
    pub kept: Vec<KeptChangeView>,
    pub download_bytes: u64,
    /// The mod loader's version before and after, when it changes.
    pub loader_from: Option<String>,
    pub loader_to: Option<String>,
}

/// What an update did.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModpackUpdateResultView {
    pub version_number: String,
    pub added: u64,
    pub updated: u64,
    pub removed: u64,
    pub kept: u64,
}

fn no_pack() -> CmdError {
    CmdError::new(
        "PAKET-YOK",
        pick(
            "Bu kurulum bir Modrinth mod paketinden oluşturulmadı.",
            "This instance wasn't made from a Modrinth modpack.",
        ),
        "no modpack source in the add-on index",
    )
}

async fn modpack_source(state: &AppState, instance_id: &str) -> CmdResult<Option<AddonSource>> {
    let dir = state
        .instances
        .instance_dir(instance_id)
        .map_err(CmdError::from)?;
    Ok(AddonIndex::load(&addon_index_path(&dir)).await.modpack)
}

/// The Modrinth modpack the instance was made from (`None` for other instances).
#[tauri::command]
#[specta::specta]
pub async fn modpack_info(
    state: State<'_, AppState>,
    instance_id: String,
) -> CmdResult<Option<ModpackInfoView>> {
    Ok(modpack_source(&state, &instance_id)
        .await?
        .map(|s| ModpackInfoView {
            project_id: s.project_id,
            version_id: s.version_id,
            version_number: s.version_number,
            title: s.title,
            icon_url: s.icon_url,
        }))
}

/// The newest version of the instance's modpack for its Minecraft version and loader, when it is
/// newer than the installed one (one request).
#[tauri::command]
#[specta::specta]
pub async fn modpack_update_check(
    state: State<'_, AppState>,
    instance_id: String,
) -> CmdResult<Option<ModpackUpdateView>> {
    let Some(source) = modpack_source(&state, &instance_id).await? else {
        return Ok(None);
    };
    let instance = state
        .instances
        .get(&instance_id)
        .await
        .map_err(CmdError::from)?;
    let client = modrinth_client(&state);
    let versions = client
        .project_versions(
            &source.project_id,
            &[],
            std::slice::from_ref(&instance.mc_version),
        )
        .await
        .map_err(content_error)?;
    let installed = match versions.iter().find(|v| v.id == source.version_id) {
        Some(found) => found.clone(),
        None => client
            .version(&source.version_id)
            .await
            .map_err(content_error)?,
    };
    Ok(pack_update_candidate(
        &versions,
        &installed,
        &instance.mc_version,
        instance.loader.kind,
    )
    .map(|v| ModpackUpdateView {
        version_id: v.id.clone(),
        version_number: v.version_number.clone(),
        name: v.name.clone(),
        published_at: v.date_published.clone(),
    }))
}

/// Everything an update needs, read fresh: the target version, both states and the plan.
struct Prepared {
    instance: Instance,
    instance_dir: PathBuf,
    game_dir: PathBuf,
    version: ModrinthVersion,
    archive: PathBuf,
    old: ModpackState,
    new: PackContents,
    plan: UpdatePlan,
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> pl_mc::Result<T> + Send + 'static,
) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(CmdError::internal)?
        .map_err(content_error)
}

async fn prepare(
    state: &AppState,
    instance_id: &str,
    version_id: &str,
    cancel: &CancellationToken,
    progress: &Channel<JobProgressView>,
) -> CmdResult<Prepared> {
    let source = modpack_source(state, instance_id)
        .await?
        .ok_or_else(no_pack)?;
    let instance = state
        .instances
        .get(instance_id)
        .await
        .map_err(CmdError::from)?;
    let instance_dir = state
        .instances
        .instance_dir(instance_id)
        .map_err(CmdError::from)?;
    let game_dir = state
        .instances
        .game_dir(instance_id)
        .map_err(CmdError::from)?;
    let client = modrinth_client(state);
    let version = client.version(version_id).await.map_err(content_error)?;
    if version.project_id != source.project_id {
        return Err(CmdError::internal("the version belongs to another project"));
    }
    let archive = download_pack_version(state, &version, cancel, progress, "pack").await?;

    // What the installed version put in place: saved at import, else read from its own pack
    // (instances imported before the state existed got every optional file; a state written
    // before it recorded projects is read again, keeping the player's optional choices).
    let saved = ModpackState::load(&modpack_state_path(&instance_dir))
        .filter(|s| s.version_id == source.version_id && s.project_id == source.project_id);
    let old = match saved {
        Some(old) if !old.projects.is_empty() => old,
        saved => {
            let off = saved.map(|s| s.optional_off).unwrap_or_default();
            let installed = client
                .version(&source.version_id)
                .await
                .map_err(content_error)?;
            let old_archive =
                download_pack_version(state, &installed, cancel, progress, "pack").await?;
            let contents = blocking(move || read_pack_contents(&old_archive, &off)).await?;
            ModpackState::from_contents(
                &source.project_id,
                &source.version_id,
                source.version_number.clone(),
                &contents,
            )
        }
    };
    if !old.minecraft.is_empty() && old.minecraft != instance.mc_version {
        return Err(CmdError::new(
            "PAKET-SURUM",
            pick(
                "Mod paketi bu kurulumun şu anki Minecraft sürümü için kurulmadı.",
                "The modpack wasn't installed for this instance's current Minecraft version.",
            ),
            format!("pack files are for {}", old.minecraft),
        ));
    }

    send(progress, JobProgressView::stage("checking"));
    let index = AddonIndex::load(&addon_index_path(&instance_dir)).await;
    let (pack, off, dir) = (archive.clone(), old.optional_off.clone(), game_dir.clone());
    let old_files = old.clone();
    let (new, plan) = blocking(move || {
        let new = read_pack_contents(&pack, &off)?;
        // The add-on index knows the SHA-1 of add-ons that did not change since it saw them.
        let cache = |rel: &str, size: u64, modified: Option<i64>| {
            let (folder, name) = rel.split_once('/')?;
            if name.contains('/') {
                return None;
            }
            index
                .files
                .get(&index_key(folder, name))
                .filter(|record| record.matches(size, modified))
                .map(|record| record.sha1.clone())
        };
        let mut disk = |rel: &str| disk_state(&dir, rel, &cache);
        let plan = plan_update(&old_files, &new, &mut disk);
        Ok((new, plan))
    })
    .await?;
    Ok(Prepared {
        instance,
        instance_dir,
        game_dir,
        version,
        archive,
        old,
        new,
        plan,
    })
}

fn send(channel: &Channel<JobProgressView>, view: JobProgressView) {
    let _ = channel.send(view);
}

fn folder_of(path: &str) -> String {
    path.split_once('/')
        .map(|(folder, _)| folder.to_owned())
        .unwrap_or_default()
}

/// Display names for the plan's files: Modrinth titles and icons for hosted files (one request,
/// best effort), file names for the rest.
async fn change_views(state: &AppState, prepared: &Prepared) -> HashMap<String, PackChangeView> {
    let mut paths: Vec<&String> = prepared
        .plan
        .add
        .iter()
        .chain(&prepared.plan.replace)
        .chain(&prepared.plan.remove)
        .chain(prepared.plan.kept.iter().map(|(path, _)| path))
        .collect();
    paths.sort();
    paths.dedup();
    // Projects from the new index, else from the installed version's state (dropped or held
    // files).
    let project_of = |path: &str| -> Option<String> {
        match prepared.new.files.get(path).map(|f| &f.source) {
            Some(PackFileSource::Download(file)) => file.modrinth_project(),
            _ => prepared.old.projects.get(path).cloned(),
        }
    };
    let ids: Vec<String> = {
        let mut ids: Vec<String> = paths.iter().filter_map(|p| project_of(p)).collect();
        ids.sort();
        ids.dedup();
        ids
    };
    let projects: HashMap<String, (String, Option<String>)> = if ids.is_empty() {
        HashMap::new()
    } else {
        match modrinth_client(state).projects(&ids).await {
            Ok(list) => list
                .into_iter()
                .map(|p| (p.id, (p.title, p.icon_url)))
                .collect(),
            Err(error) => {
                tracing::debug!(%error, "no project names for the update plan");
                HashMap::new()
            }
        }
    };
    paths
        .into_iter()
        .map(|path| {
            let file_name = path.rsplit('/').next().unwrap_or(path).to_owned();
            let known = project_of(path).and_then(|id| projects.get(&id).cloned());
            let view = PackChangeView {
                path: path.clone(),
                name: known
                    .as_ref()
                    .map(|(title, _)| title.clone())
                    .unwrap_or(file_name),
                folder: folder_of(path),
                icon_url: known.and_then(|(_, icon)| icon),
            };
            (path.clone(), view)
        })
        .collect()
}

/// Downloads the target version's `.mrpack` and tells what updating to it would change; nothing
/// in the instance changes. Stoppable with `content_cancel`.
#[tauri::command]
#[specta::specta]
pub async fn modpack_update_plan(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    version_id: String,
    job_id: String,
    on_progress: Channel<JobProgressView>,
) -> CmdResult<ModpackUpdatePlanView> {
    let job = start_job(&app, &job_id);
    let prepared = prepare(&state, &instance_id, &version_id, &job.token, &on_progress).await?;
    let views = change_views(&state, &prepared).await;
    let view_of = |path: &String| views.get(path).cloned();
    let (_, loader_to) = prepared.new.index.loader();
    let loader_from = prepared.instance.loader.version.clone();
    let loader_changes = loader_to.is_some() && loader_to != loader_from;
    let download_bytes = prepared
        .plan
        .add
        .iter()
        .chain(&prepared.plan.replace)
        .filter_map(|path| prepared.new.files.get(path))
        .filter(|file| matches!(file.source, PackFileSource::Download(_)))
        .map(|file| file.size)
        .sum();
    // A mod's new version under another file name is an update for the player.
    let moved_new: BTreeSet<&String> = prepared.plan.moved.iter().map(|(_, new)| new).collect();
    let moved_old: BTreeSet<&String> = prepared.plan.moved.iter().map(|(old, _)| old).collect();
    Ok(ModpackUpdatePlanView {
        from_version: prepared.old.version_number.clone(),
        to_version: prepared.version.version_number.clone(),
        added: prepared
            .plan
            .add
            .iter()
            .filter(|path| !moved_new.contains(path))
            .filter_map(view_of)
            .collect(),
        updated: prepared
            .plan
            .replace
            .iter()
            .chain(prepared.plan.moved.iter().map(|(_, new)| new))
            .filter_map(view_of)
            .collect(),
        removed: prepared
            .plan
            .remove
            .iter()
            .filter(|path| !moved_old.contains(path))
            .filter_map(view_of)
            .collect(),
        kept: prepared
            .plan
            .kept
            .iter()
            .filter_map(|(path, reason)| {
                Some(KeptChangeView {
                    item: view_of(path)?,
                    reason: *reason,
                })
            })
            .collect(),
        download_bytes,
        loader_from: loader_changes.then_some(loader_from).flatten(),
        loader_to: if loader_changes { loader_to } else { None },
    })
}

/// Updates the instance to a newer version of its pack: files the player did not touch follow
/// the pack, everything the player changed or added stays. Refused while the game runs.
#[tauri::command]
#[specta::specta]
pub async fn modpack_update_apply(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    version_id: String,
    job_id: String,
    on_progress: Channel<JobProgressView>,
) -> CmdResult<ModpackUpdateResultView> {
    refuse_while_running(&state, &instance_id)?;
    let job = start_job(&app, &job_id);
    let lock = instance_lock(&app, &instance_id);
    let _guard = lock.lock().await;
    refuse_while_running(&state, &instance_id)?;
    let prepared = prepare(&state, &instance_id, &version_id, &job.token, &on_progress).await?;
    let Prepared {
        mut instance,
        instance_dir,
        game_dir,
        version,
        archive,
        old,
        new,
        plan,
    } = prepared;

    let items = update_download_items(&game_dir, &new, &plan).map_err(content_error)?;
    if !items.is_empty() {
        // Nothing starts that cannot fit on the disk (the launch's margin; "Yer aç").
        pl_mc::install::space::ensure_room(
            &game_dir,
            pl_mc::install::space::download_bytes(&items),
        )
        .map_err(content_error)?;
        let events = on_progress.clone();
        let on_download: ProgressFn = Arc::new(move |p: pl_net::DownloadProgress| {
            send(&events, JobProgressView::download("downloading", &p))
        });
        state
            .mc
            .downloader
            .download_all(
                items,
                &DownloadOptions {
                    concurrency: 8,
                    verify: VerifyMode::Full,
                    cancel: job.token.clone(),
                    progress: Some(on_download),
                    progress_interval: Duration::from_millis(150),
                },
            )
            .await
            .map_err(|e| content_error(e.into()))?;
    }
    if job.token.is_cancelled() {
        return Err(content_error(pl_mc::Error::Cancelled));
    }

    // From here on the update finishes: no more stopping halfway.
    send(&on_progress, JobProgressView::stage("finishing"));
    let (dir, pack, contents, steps) =
        (game_dir.clone(), archive.clone(), new.clone(), plan.clone());
    blocking(move || {
        write_bundled(&pack, &dir, &contents, &steps, &CancellationToken::new())?;
        remove_dropped(&dir, &steps);
        Ok(())
    })
    .await?;

    let (loader, loader_version) = new.index.loader();
    if loader == instance.loader.kind
        && loader_version.is_some()
        && loader_version != instance.loader.version
    {
        instance.loader.version = loader_version;
        state
            .instances
            .update(&instance)
            .await
            .map_err(CmdError::from)?;
    }

    let source = modpack_source(&state, &instance_id)
        .await?
        .ok_or_else(no_pack)?;
    let next = old.after_update(
        &version.id,
        Some(version.version_number.clone()),
        &new,
        &plan,
    );
    let state_path = modpack_state_path(&instance_dir);
    blocking(move || next.save(&state_path)).await?;
    let addons = addons_for(&state, &instance_id)?;
    let downloaded: Vec<pl_mc::modpack::MrFile> = plan
        .add
        .iter()
        .chain(&plan.replace)
        .filter_map(|path| match new.files.get(path).map(|f| &f.source) {
            Some(PackFileSource::Download(file)) => Some(file.clone()),
            _ => None,
        })
        .collect();
    if let Err(error) = addons.record_pack_files(&downloaded).await {
        tracing::warn!(%error, "could not record the updated pack files");
    }
    if let Err(error) = addons
        .set_modpack(Some(AddonSource {
            version_id: version.id.clone(),
            version_number: Some(version.version_number.clone()),
            ..source
        }))
        .await
    {
        tracing::warn!(%error, "could not record the new pack version");
    }
    tracing::info!(
        instance = %instance_id,
        version = %version.version_number,
        added = plan.add.len(),
        updated = plan.replace.len(),
        removed = plan.remove.len(),
        kept = plan.kept.len(),
        "modpack updated"
    );
    Ok(ModpackUpdateResultView {
        version_number: version.version_number,
        added: plan.add.len() as u64,
        updated: plan.replace.len() as u64,
        removed: plan.remove.len() as u64,
        kept: plan.kept.len() as u64,
    })
}

/// A Modrinth project's title and icon.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProjectBriefView {
    pub id: String,
    pub title: String,
    pub icon_url: Option<String>,
}

/// Titles and icons of Modrinth projects (one request for up to 100; unknown ids are left out).
/// The import preview names a pack's optional files with it.
#[tauri::command]
#[specta::specta]
pub async fn modrinth_project_briefs(
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> CmdResult<Vec<ProjectBriefView>> {
    let ids: Vec<String> = ids
        .into_iter()
        .filter(|id| {
            !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric())
        })
        .take(100)
        .collect();
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let projects = modrinth_client(&state)
        .projects(&ids)
        .await
        .map_err(content_error)?;
    Ok(projects
        .into_iter()
        .map(|p| ProjectBriefView {
            id: p.id,
            title: p.title,
            icon_url: p.icon_url,
        })
        .collect())
}
