//! The official Projects pack: one instance follows the pack family the content index publishes
//! and is brought up to date right before every launch.
//!
//! 1. `prepare_pack` (before the game files): the instance follows the family member published
//!    for its Minecraft version (an in-place Minecraft update of the same pack is followed like
//!    any update). When the family no longer offers that version the installed set keeps
//!    working and the player is told which version is recommended; versions never change
//!    silently.
//! 2. `sync_pack_files` (after the game files): the pack's files are planned and applied with
//!    pl-content's sync engine, which never touches files the player added and adopts the
//!    changes they made in the mods folder (reported as a non-blocking notice).
//!
//! Nothing here blocks offline play: without the network the instance launches with what is
//! already installed.

use std::path::Path;
use std::sync::Arc;

use pl_content::{
    ContentSnapshot, PackLoader, PackManifest, PackRef, PackState, RootIndex, SyncPolicy,
    SyncProgress, SyncProgressFn, apply_sync, plan_sync,
};
use pl_mc::install::{LaunchBar, McContext};
use pl_mc::instance::model::{ManagedPack, ServerAddress};
use pl_mc::instance::{Instance, InstanceIcon, InstanceLoader, NewInstance};
use pl_mc::loaders::LoaderKind;
use pl_net::{CancellationToken, DownloadOptions, VerifyMode};
use serde::Serialize;
use tauri::State;

use crate::commands::instances::{InstanceView, seed_default_server_once, view};
use crate::commands::launch::{EventSink, LaunchEventView, ProgressView};
use crate::commands::modcenter;
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

/// The community server every Projects instance joins.
pub const PROJECTS_SERVER: &str = "play.projects.gg";
const PROJECTS_NAME: &str = "Projects";
const PROJECTS_ICON: &str = "dragon-egg";

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OfficialPackView {
    pub id: String,
    pub name: String,
    pub version: String,
    pub mc_version: String,
    pub loader: LoaderKind,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OfficialStatus {
    /// The pack the official instance follows (or, before it exists, the one new installs get);
    /// `None` until one is published (or while offline without a cache).
    pub pack: Option<OfficialPackView>,
    /// The instance following that pack family, or the "Projects" instance when none is
    /// published.
    pub instance_id: Option<String>,
}

pub(crate) fn pack_error(error: pl_content::Error) -> CmdError {
    if matches!(error, pl_content::Error::Cancelled) {
        CmdError::new("IPTAL", "İşlem iptal edildi.", "")
    } else if error.is_offline() {
        CmdError::new(
            "PAKET-AG",
            "Projects paketi indirilemedi. İnternet bağlantını kontrol edip tekrar dene.",
            error.to_string(),
        )
    } else {
        CmdError::new(
            "PAKET",
            "Projects paketi hazırlanamadı. Tekrar dene; sürerse ayrıntıları destek ekibine gönder.",
            error.to_string(),
        )
    }
}

pub(crate) fn loader_kind(loader: PackLoader) -> LoaderKind {
    match loader {
        PackLoader::Fabric => LoaderKind::Fabric,
        PackLoader::Quilt => LoaderKind::Quilt,
        PackLoader::Forge => LoaderKind::Forge,
        PackLoader::NeoForge => LoaderKind::NeoForge,
    }
}

pub(crate) fn pack_loader(manifest: &PackManifest) -> InstanceLoader {
    match manifest.dependencies.loader() {
        Some((loader, version)) => InstanceLoader {
            kind: loader_kind(loader),
            version: Some(version.to_owned()),
        },
        None => InstanceLoader::default(),
    }
}

/// The pack new installs use: the default member of `recommendedPack`'s family, else of the
/// first published pack's family.
pub(crate) fn official_pack_ref(snapshot: &ContentSnapshot) -> Option<&PackRef> {
    snapshot
        .index
        .recommended_pack(snapshot.config.recommended_pack.as_deref())
}

/// The family a managed instance follows: its pack's family in the index, else the family its
/// installed set records (the pack left the index), else its pack id. `None` for instances
/// without a pack.
pub(crate) fn instance_family(
    index: &RootIndex,
    instance: &Instance,
    game_dir: Option<&Path>,
) -> Option<String> {
    let managed = instance.managed_pack.as_ref()?;
    if let Some(pack) = index.pack(&managed.pack_id) {
        return Some(pack.family_id().to_owned());
    }
    let recorded = game_dir
        .and_then(|dir| std::fs::read(PackState::path(dir)).ok())
        .and_then(|bytes| serde_json::from_slice::<PackState>(&bytes).ok())
        .map(|state| state.family)
        .filter(|family| !family.is_empty());
    Some(recorded.unwrap_or_else(|| managed.pack_id.clone()))
}

/// Which pack a managed instance uses for its Minecraft version.
pub(crate) enum Followed<'a> {
    /// The family member published for the instance's version (it may be another pack id
    /// than the one recorded, when Studio split the family).
    Member(&'a PackRef),
    /// The pack the instance follows moved to another Minecraft version in place: an
    /// ordinary update.
    InPlace(&'a PackRef),
    /// The family no longer offers the instance's version: the installed set keeps working.
    Dropped { recommended: Option<&'a PackRef> },
}

pub(crate) fn followed_pack<'a>(
    index: &'a RootIndex,
    instance: &Instance,
    family: &str,
) -> Followed<'a> {
    if let Some(member) = index.family_member(family, &instance.mc_version) {
        return Followed::Member(member);
    }
    let pack_id = instance
        .managed_pack
        .as_ref()
        .map(|managed| managed.pack_id.as_str())
        .unwrap_or_default();
    match index.pack(pack_id) {
        Some(pack) => Followed::InPlace(pack),
        None => Followed::Dropped {
            recommended: index.family_default(family),
        },
    }
}

/// Content for decisions that may wait for the network: the cache first, then a refresh.
pub(crate) async fn cached_or_fresh(state: &AppState) -> Option<ContentSnapshot> {
    match state.content.load_cached().await {
        Ok(snapshot) => Some(snapshot),
        Err(_) => state.content.refresh().await.ok(),
    }
}

/// Content right before a launch: the network when reachable, otherwise the cache. The flag
/// is true when the network brought a newer index than the cached one.
async fn latest(state: &AppState, offline: bool) -> Option<(ContentSnapshot, bool)> {
    let cached = state.content.load_cached().await.ok();
    if !offline {
        let known = cached.as_ref().map(|s| s.index.sequence);
        match state.content.refresh().await {
            Ok(snapshot) => {
                let changed = !snapshot.from_cache && Some(snapshot.index.sequence) != known;
                return Some((snapshot, changed));
            }
            Err(error) => tracing::info!(%error, "content refresh before launch failed"),
        }
    }
    cached.map(|snapshot| (snapshot, false))
}

/// The family stopped offering the instance's Minecraft version.
pub(crate) struct VersionDropped {
    pub minecraft: String,
    /// The family's recommended Minecraft version, when known.
    pub recommended: Option<String>,
}

/// What [`prepare_pack`] found.
pub(crate) struct PreparedPack {
    /// Manifest for [`sync_pack_files`]; `None` when the instance launches as it is.
    pub manifest: Option<PackManifest>,
    /// New content arrived while preparing (the home screen should refresh).
    pub content_changed: bool,
    /// The installed set is used as is because its version is no longer offered.
    pub dropped: Option<VersionDropped>,
}

fn instance_follows(
    state: &AppState,
    index: &RootIndex,
    instance: &Instance,
    family: &str,
) -> bool {
    let game_dir = state.instances.game_dir(&instance.id).ok();
    instance_family(index, instance, game_dir.as_deref()).as_deref() == Some(family)
}

#[tauri::command]
#[specta::specta]
pub async fn official_status(state: State<'_, AppState>) -> CmdResult<OfficialStatus> {
    let instances = state.instances.list().await.map_err(CmdError::from)?;
    let snapshot = cached_or_fresh(&state).await;
    let Some((snapshot, recommended)) = snapshot
        .as_ref()
        .and_then(|s| official_pack_ref(s).map(|pack| (s, pack)))
    else {
        return Ok(OfficialStatus {
            pack: None,
            instance_id: instances
                .iter()
                .find(|i| i.name.eq_ignore_ascii_case(PROJECTS_NAME))
                .map(|i| i.id.clone()),
        });
    };
    let family = recommended.family_id();
    let instance = instances
        .iter()
        .find(|i| instance_follows(&state, &snapshot.index, i, family));
    // The member the official instance follows (its version), else the one new installs get.
    let pack = instance
        .and_then(|i| match followed_pack(&snapshot.index, i, family) {
            Followed::Member(pack) | Followed::InPlace(pack) => Some(pack),
            Followed::Dropped { .. } => None,
        })
        .unwrap_or(recommended);
    let manifest = state.content.pack_manifest(pack).await.ok();
    Ok(OfficialStatus {
        pack: Some(OfficialPackView {
            id: pack.id.clone(),
            name: pack.name.clone(),
            version: pack.latest.clone(),
            mc_version: manifest
                .as_ref()
                .map(|m| m.dependencies.minecraft.clone())
                .unwrap_or_default(),
            loader: manifest
                .as_ref()
                .map_or(LoaderKind::Vanilla, |m| pack_loader(m).kind),
            summary: manifest.and_then(|m| m.summary),
        }),
        instance_id: instance.map(|i| i.id.clone()),
    })
}

/// The Projects instance, created on first use. It follows the official pack family when one
/// is published (new installs get the family's default member); until then it is vanilla on the
/// newest release. Files are installed by the first launch.
#[tauri::command]
#[specta::specta]
pub async fn official_instance_ensure(state: State<'_, AppState>) -> CmdResult<InstanceView> {
    // The first-run screen and the pack watcher may ask at the same time: one creation only.
    static ENSURE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _guard = ENSURE.lock().await;
    let instances = state.instances.list().await.map_err(CmdError::from)?;
    let snapshot = cached_or_fresh(&state).await;

    if let Some(snapshot) = snapshot.as_ref()
        && let Some(pack) = official_pack_ref(snapshot)
    {
        let family = pack.family_id();
        if let Some(found) = instances
            .iter()
            .find(|i| instance_follows(&state, &snapshot.index, i, family))
        {
            return Ok(view(&state, found));
        }
        let manifest = state
            .content
            .pack_manifest(pack)
            .await
            .map_err(pack_error)?;
        let unmanaged: Vec<&Instance> = instances
            .iter()
            .filter(|i| i.managed_pack.is_none() && i.name.eq_ignore_ascii_case(PROJECTS_NAME))
            .collect();
        // A vanilla "Projects" (made before any pack was published) simply starts following
        // the pack: it has no mods that could clash with the pack's.
        if let Some(vanilla) = unmanaged
            .iter()
            .find(|i| i.loader.kind == LoaderKind::Vanilla)
        {
            let mut adopted = (*vanilla).clone();
            adopted.managed_pack = Some(ManagedPack {
                pack_id: pack.id.clone(),
                version: String::new(),
            });
            adopted.mc_version = manifest.dependencies.minecraft.clone();
            adopted.loader = pack_loader(&manifest);
            let total = pl_mc::instance::system_memory_mb();
            let recommended = pl_mc::instance::recommended_memory(total, adopted.is_modded());
            if adopted.memory.max_mb < recommended.max_mb {
                adopted.memory = recommended;
            }
            state
                .instances
                .update(&adopted)
                .await
                .map_err(CmdError::from)?;
            return Ok(view(&state, &adopted));
        }
        // A modded one (e.g. imported from v1) keeps its files under a name that tells the
        // two apart.
        for old in unmanaged {
            let mut renamed = old.clone();
            renamed.name = format!("{PROJECTS_NAME} (eski)");
            state
                .instances
                .update(&renamed)
                .await
                .map_err(CmdError::from)?;
        }
        let created = create_projects_instance(
            &state,
            manifest.dependencies.minecraft.clone(),
            pack_loader(&manifest),
            Some(ManagedPack {
                pack_id: pack.id.clone(),
                // Empty until the first sync records the installed version.
                version: String::new(),
            }),
            manifest.projects.recommended_memory_mb,
        )
        .await?;
        return Ok(view(&state, &created));
    }

    if let Some(found) = instances
        .iter()
        .find(|i| i.name.eq_ignore_ascii_case(PROJECTS_NAME))
    {
        return Ok(view(&state, found));
    }
    let manifest = match pl_mc::version::manifest::load_manifest(&state.mc, false).await {
        Ok(manifest) => manifest,
        Err(error) if error.is_offline() => {
            pl_mc::version::manifest::load_manifest(&state.mc, true)
                .await
                .map_err(CmdError::from)?
        }
        Err(error) => return Err(error.into()),
    };
    let created = create_projects_instance(
        &state,
        manifest.latest.release.clone(),
        InstanceLoader::default(),
        None,
        None,
    )
    .await?;
    Ok(view(&state, &created))
}

async fn create_projects_instance(
    state: &AppState,
    mc_version: String,
    loader: InstanceLoader,
    managed_pack: Option<ManagedPack>,
    recommended_memory_mb: Option<u32>,
) -> CmdResult<Instance> {
    let total = pl_mc::instance::system_memory_mb();
    let modded = loader.kind != LoaderKind::Vanilla;
    let mut memory = pl_mc::instance::recommended_memory(total, modded);
    if let Some(mb) = recommended_memory_mb {
        memory.max_mb = memory.max_mb.max(mb);
        memory = pl_mc::instance::clamp_memory(&memory, total);
    }
    let mut created = state
        .instances
        .create(NewInstance {
            name: PROJECTS_NAME.to_owned(),
            mc_version,
            loader,
            icon: Some(InstanceIcon::Builtin {
                key: PROJECTS_ICON.to_owned(),
            }),
            group: None,
            memory: Some(memory),
            managed_pack,
        })
        .await
        .map_err(CmdError::from)?;
    created.auto_join = Some(ServerAddress {
        host: PROJECTS_SERVER.to_owned(),
        port: None,
    });
    // "Otomatik" (never below the pack's recommendation); the values above stay for "Özel".
    created.memory_auto = true;
    state
        .instances
        .update(&created)
        .await
        .map_err(CmdError::from)?;
    seed_default_server_once(state, &mut created).await;
    Ok(created)
}

/// Adds the pack's servers to the top of `servers.dat`, skipping addresses already listed. An
/// unreadable list is left alone.
async fn add_servers(game_dir: &Path, servers: &[(String, String)]) {
    if let Err(error) = pl_mc::instance::add_servers(game_dir, servers).await {
        tracing::warn!(%error, "servers.dat unreadable; not adding the pack's servers");
    }
}

/// Before the game files: a managed instance follows the family member of its Minecraft
/// version (or its pack's in-place update) and that pack's loader. The manifest is `None` when
/// the pack is unknown (not published, its version dropped, or offline without a cache) and the
/// instance launches as it is.
pub(crate) async fn prepare_pack(
    state: &AppState,
    instance: &mut Instance,
    game_dir: &Path,
    offline: bool,
    launcher_version: &str,
) -> CmdResult<PreparedPack> {
    let mut prepared = PreparedPack {
        manifest: None,
        content_changed: false,
        dropped: None,
    };
    if instance.managed_pack.is_none() {
        return Ok(prepared);
    }
    let Some((snapshot, changed)) = latest(state, offline).await else {
        return Ok(prepared);
    };
    prepared.content_changed = changed;
    crate::commands::content::apply_snapshot(state, &snapshot);
    let family = instance_family(&snapshot.index, instance, Some(game_dir)).unwrap_or_default();
    let pack = match followed_pack(&snapshot.index, instance, &family) {
        Followed::Member(pack) | Followed::InPlace(pack) => pack,
        Followed::Dropped { recommended } => {
            tracing::info!(
                minecraft = %instance.mc_version,
                %family,
                "the pack family no longer offers this version; launching the installed set"
            );
            prepared.dropped = Some(VersionDropped {
                minecraft: instance.mc_version.clone(),
                recommended: recommended
                    .and_then(|pack| pack.minecraft())
                    .map(str::to_owned),
            });
            return Ok(prepared);
        }
    };
    let manifest = match state.content.pack_manifest(pack).await {
        Ok(manifest) => manifest,
        Err(error) => {
            tracing::warn!(%error, "pack manifest unavailable; launching as is");
            return Ok(prepared);
        }
    };
    if let (Some(min), Ok(current)) = (
        &manifest.projects.min_launcher_version,
        semver::Version::parse(launcher_version),
    ) && current < *min
    {
        return Err(CmdError::new(
            "PAKET-LAUNCHER",
            "Projects paketinin yeni sürümü için launcher'ın güncellenmesi gerekiyor. Güncelleme hazır olunca üst çubukta göreceksin.",
            format!("pack needs launcher {min}, running {current}"),
        ));
    }
    let mut changed_instance = false;
    if let Some(managed) = instance.managed_pack.as_mut()
        && managed.pack_id != pack.id
    {
        tracing::info!(from = %managed.pack_id, to = %pack.id, "managed instance follows the member of its version");
        managed.pack_id = pack.id.clone();
        changed_instance = true;
    }
    let loader = pack_loader(&manifest);
    if instance.mc_version != manifest.dependencies.minecraft || instance.loader != loader {
        tracing::info!(
            from = %instance.mc_version,
            to = %manifest.dependencies.minecraft,
            "managed instance follows its pack"
        );
        instance.mc_version = manifest.dependencies.minecraft.clone();
        instance.loader = loader;
        changed_instance = true;
    }
    if changed_instance {
        state
            .instances
            .update(instance)
            .await
            .map_err(CmdError::from)?;
    }
    prepared.manifest = Some(manifest);
    Ok(prepared)
}

/// Where [`sync_pack_files`] reports (the launch's events, its progress on the launch's one bar)
/// and how it downloads: for the player's launch, or gently for background preparation
/// (`McContext::for_background`).
pub(crate) struct PackSync<'a> {
    pub events: &'a EventSink,
    pub bar: &'a Arc<LaunchBar>,
    pub ctx: &'a McContext,
}

/// After the game files: makes the pack's files match `manifest` (player-added files stay as
/// they are; changes the player made to pack mods are adopted and reported with `ModsNotice`).
/// Emits `PackUpdated` when an installed pack moved to a new version.
pub(crate) async fn sync_pack_files(
    state: &AppState,
    instance: &mut Instance,
    manifest: &PackManifest,
    game_dir: &Path,
    offline: bool,
    PackSync {
        events: on_event,
        bar,
        ctx,
    }: PackSync<'_>,
    cancel: &CancellationToken,
) -> CmdResult<()> {
    let previous = PackState::load(game_dir).await.map_err(pack_error)?;
    let first_install = previous.is_none();
    let plan = plan_sync(
        manifest,
        game_dir,
        previous.as_ref(),
        None,
        &SyncPolicy {
            repair: false,
            quick_check: true,
        },
    )
    .await
    .map_err(pack_error)?;

    let mut changed = 0;
    let mut failed = 0;
    if !plan.is_noop() {
        if offline && !plan.downloads.is_empty() {
            tracing::info!("pack update pending until the network is back");
        } else {
            let events = Arc::clone(on_event);
            let bar = Arc::clone(bar);
            let progress: SyncProgressFn = Arc::new(move |p: SyncProgress| {
                let fraction = if p.total_bytes > 0 {
                    p.done_bytes as f64 / p.total_bytes as f64
                } else if p.total_files > 0 {
                    p.done_files as f64 / p.total_files as f64
                } else {
                    1.0
                };
                let remaining = p.total_bytes.saturating_sub(p.done_bytes);
                events(LaunchEventView::Progress(ProgressView {
                    stage: "pack".to_owned(),
                    percent: bar.part_percent(fraction),
                    done_files: p.done_files as u64,
                    total_files: p.total_files as u64,
                    bytes_per_sec: p.bytes_per_sec,
                    eta_secs: (p.bytes_per_sec > 0 && remaining > 0)
                        .then(|| remaining / p.bytes_per_sec),
                }));
            });
            let options = DownloadOptions {
                concurrency: ctx.concurrency(8),
                verify: VerifyMode::Full,
                cancel: cancel.clone(),
                ..DownloadOptions::default()
            };
            let report = apply_sync(&plan, &ctx.downloader, &options, Some(progress))
                .await
                .map_err(pack_error)?;
            changed = report.downloaded + report.deleted + report.restored;
            failed = report.issues.len();
        }
    }
    if let Some(notice) = modcenter::notice(&plan, manifest, failed) {
        on_event(LaunchEventView::ModsNotice(notice));
    }

    if first_install {
        let mut servers: Vec<(String, String)> = manifest
            .projects
            .servers
            .iter()
            .map(|s| (s.name.clone(), s.address.clone()))
            .collect();
        if servers.is_empty() {
            servers.push((PROJECTS_NAME.to_owned(), PROJECTS_SERVER.to_owned()));
        }
        add_servers(game_dir, &servers).await;
    }

    let recorded = instance
        .managed_pack
        .as_ref()
        .map(|m| m.version.clone())
        .unwrap_or_default();
    // "Otomatik" memory never goes below what the pack recommends (kept for offline starts).
    let pack_memory = manifest.projects.recommended_memory_mb;
    let memory_changed = instance.pack_memory_mb != pack_memory;
    instance.pack_memory_mb = pack_memory;
    if recorded != manifest.version_id || memory_changed {
        if let Some(managed) = instance.managed_pack.as_mut() {
            managed.version = manifest.version_id.clone();
        }
        state
            .instances
            .update(instance)
            .await
            .map_err(CmdError::from)?;
        if recorded != manifest.version_id && !first_install && !recorded.is_empty() {
            let changelog = manifest.projects.changelog.trim();
            on_event(LaunchEventView::PackUpdated {
                name: manifest.name.clone(),
                version: manifest.version_id.clone(),
                changelog: (!changelog.is_empty()).then(|| changelog.to_owned()),
                changed_files: changed as u64,
            });
        }
    }
    Ok(())
}
