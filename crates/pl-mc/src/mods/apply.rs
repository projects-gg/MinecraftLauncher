//! Carrying out repairs for the active version set of an instance: the steps of a
//! [`RepairPlan`] ("Onar") and a set reinstall ([`ReinstallPlan`], "Yeniden yükle").
//!
//! The order of a repair:
//!
//! 1. the pack phase: one official pack sync that brings the pack's files in line, puts broken
//!    or replaced pack files back to the version chosen in Studio and switches pack items on or
//!    off (a pack file is never renamed or upgraded here),
//! 2. broken Modrinth files are downloaded again,
//! 3. the player's own files are turned on or off (renamed),
//! 4. the mods are checked again and the network steps are planned anew from that check, so a
//!    download an earlier step made unnecessary is dropped and a dependency a re-enabled mod
//!    brought in is added,
//! 5. the loader version, then Java,
//! 6. updates of the player's own mods and missing dependencies, resolved on Modrinth with a few
//!    batched read-only requests (the project from the file's origin, else by SHA-1, else by
//!    slug; versions filtered by loader, game version and every requirement) and installed in
//!    one go; a dependency the official pack offers is switched on there instead,
//! 7. a last check, whose report says what is left.
//!
//! What depends on the launcher (the pack sync, the instance's loader and Java settings) goes
//! through [`RepairHost`]; everything else (renames, Modrinth, add-on installs, the launcher
//! trash) happens here, so the order and the decisions can be tested without the app.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use pl_net::{CancellationToken, Downloader};
use serde::{Deserialize, Serialize};

use super::check::{CheckEnvironment, FileOrigin, FileOrigins, ModCheckReport, ModFix, check_mods};
use super::repair::{
    KeptJar, ReinstallPlan, RepairAction, RepairContext, RepairPlan, RepairStep, action_for,
    pack_file_name, plan_repair, with_origin,
};
use super::scan::{ModScanner, ModSnapshot, strip_disabled};
use super::version::requirement_matches;
use crate::addons::{AddonIndex, AddonInstallOptions, AddonInstallReport, AddonProgress, Addons};
use crate::error::{Error, Result};
use crate::gamefiles::fsops::blocking;
use crate::gamefiles::{ImportKind, LauncherTrash, TrashEntry, TrashKind};
use crate::launch::crash::CrashDiagnosis;
use crate::loaders::{LoaderKind, LoaderVersion};
use crate::modrinth::{
    ModrinthClient, ModrinthProject, ModrinthProjectType, ModrinthResolveTarget, ModrinthVersion,
    ModrinthVersionType,
};

// --- inputs ------------------------------------------------------------------------------------

/// Where the files of a scanned mods folder came from: files the official pack installed
/// (`pack_paths` from its state, `mods/x.jar`) and files the add-on index knows from Modrinth
/// (only while a file still has the recorded size and modification time). Pack files win.
pub fn file_origins(
    snapshot: &ModSnapshot,
    pack_paths: &[String],
    index: &AddonIndex,
) -> FileOrigins {
    let pack: HashMap<String, &str> = pack_paths
        .iter()
        .filter_map(|path| {
            let name = path.strip_prefix("mods/")?;
            (!name.contains(['/', '\\'])).then(|| (name.to_ascii_lowercase(), path.as_str()))
        })
        .collect();
    let mut origins = FileOrigins::new();
    for jar in &snapshot.jars {
        let base = jar.base_name();
        if let Some(path) = pack.get(&base.to_ascii_lowercase()) {
            origins.insert(
                base.to_owned(),
                FileOrigin::Pack {
                    path: (*path).to_owned(),
                },
            );
            continue;
        }
        if let Some(source) = index.source_for("mods", &jar.file_name, jar.size, jar.modified_ms) {
            origins.insert(
                base.to_owned(),
                FileOrigin::Modrinth {
                    project_id: source.project_id.clone(),
                    version_id: Some(source.version_id.clone()).filter(|v| !v.is_empty()),
                },
            );
        }
    }
    origins
}

/// The Java major version a vanilla version JSON asks for (`javaVersion.majorVersion`; 8 for
/// versions that predate the field). `None` for a JSON that inherits from another one without
/// saying (a loader profile) or that cannot be read.
pub fn java_major_from_version_json(bytes: &[u8]) -> Option<u32> {
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    match value.get("javaVersion") {
        Some(java) => java
            .get("majorVersion")
            .and_then(serde_json::Value::as_u64)
            .and_then(|major| u32::try_from(major).ok()),
        None if value.get("inheritsFrom").is_some() => None,
        None => value.get("id").is_some().then_some(8),
    }
}

/// The loader version as the check compares it: blank or vanilla → `None`; Forge and NeoForge
/// versions without a `<minecraft>-` prefix (`1.20.1-47.4.10` → `47.4.10`).
pub fn loader_version_for_check(
    loader: LoaderKind,
    minecraft: &str,
    version: Option<&str>,
) -> Option<String> {
    if loader == LoaderKind::Vanilla {
        return None;
    }
    let version = version.map(str::trim).filter(|v| !v.is_empty())?;
    let version = match loader {
        LoaderKind::Forge | LoaderKind::NeoForge => version
            .strip_prefix(minecraft)
            .and_then(|rest| rest.strip_prefix('-'))
            .unwrap_or(version),
        _ => version,
    };
    Some(version.to_owned())
}

/// The check's view of an instance.
pub fn check_environment(
    minecraft: &str,
    loader: LoaderKind,
    loader_version: Option<&str>,
    java_major: Option<u32>,
) -> CheckEnvironment {
    CheckEnvironment {
        minecraft: minecraft.trim().to_owned(),
        loader,
        loader_version: loader_version_for_check(loader, minecraft.trim(), loader_version),
        java_major,
    }
}

// --- choosing versions -------------------------------------------------------------------------

/// Whether a version number meets every requirement: certainly, certainly not, or unknown
/// (a Modrinth version number is often not the jar's own version).
fn fits(loader: LoaderKind, reqs: &[String], version_number: &str) -> Option<bool> {
    let mut certain = true;
    for req in reqs {
        match requirement_matches(loader, req, version_number) {
            Some(true) => {}
            Some(false) => return Some(false),
            None => certain = false,
        }
    }
    certain.then_some(true)
}

/// The version to install from a project's versions (the API lists them newest first): one
/// `target` accepts that certainly meets every requirement when there is one, else one that
/// may; the newest release before betas and alphas.
pub fn pick_version<'a>(
    versions: &'a [ModrinthVersion],
    target: &ModrinthResolveTarget,
    loader: LoaderKind,
    reqs: &[String],
) -> Option<&'a ModrinthVersion> {
    let candidates: Vec<(&ModrinthVersion, bool)> = versions
        .iter()
        .filter(|v| target.accepts(v))
        .filter_map(|v| match fits(loader, reqs, &v.version_number) {
            Some(false) => None,
            fit => Some((v, fit == Some(true))),
        })
        .collect();
    let best = |certain: bool| {
        let pool = || candidates.iter().filter(move |(_, c)| *c == certain);
        [
            ModrinthVersionType::Release,
            ModrinthVersionType::Beta,
            ModrinthVersionType::Alpha,
        ]
        .iter()
        .find_map(|kind| pool().find(|(v, _)| v.version_type == *kind))
        .or_else(|| pool().next())
        .map(|(v, _)| *v)
    };
    best(true).or_else(|| best(false))
}

/// The newest stable loader version (from a newest-first list) that certainly meets every
/// requirement.
pub fn pick_loader_version(
    versions: &[LoaderVersion],
    loader: LoaderKind,
    reqs: &[String],
) -> Option<String> {
    versions
        .iter()
        .filter(|v| v.stable)
        .find(|v| {
            reqs.iter()
                .all(|req| requirement_matches(loader, req, &v.version) == Some(true))
        })
        .map(|v| v.version.clone())
}

// --- progress and results ----------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum StepStatus {
    Pending,
    Running,
    Done,
    Failed,
    /// Not run (not needed any more, or the repair was stopped).
    Skipped,
}

/// Why a step ended as it did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum StepNote {
    /// No version fits; the file was turned off instead.
    DisabledInstead,
    /// No version fits this Minecraft version, loader and requirements.
    NoVersion,
    /// Modrinth does not have the mod.
    NotFound,
    /// The official pack provides it: it was switched on there.
    FromPack,
    /// An earlier step made it unnecessary.
    NotNeeded,
    /// Modrinth or the pack could not be reached.
    Offline,
    /// The file is in use by another program (or could not be changed).
    Busy,
    /// The official pack chooses the loader version of this instance.
    PackDecides,
    /// Not possible for this instance (e.g. the game's own Java is older than a mod needs).
    NotPossible,
    /// Another enabled pack mod needs this one, so the pack keeps it.
    KeptByPack,
    /// The repair was stopped before this step.
    Cancelled,
}

/// One step of a running repair, as the UI shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StepState {
    /// Stable for the life of the repair (UI list key).
    pub key: String,
    pub step: RepairStep,
    pub status: StepStatus,
    pub note: Option<StepNote>,
}

/// A snapshot of a running repair.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RepairProgress {
    pub steps: Vec<StepState>,
    /// Download progress of the running step(s), 0–100.
    pub percent: Option<f64>,
    /// The mods are being checked (between the phases and at the end).
    pub checking: bool,
}

pub type RepairProgressFn = Arc<dyn Fn(&RepairProgress) + Send + Sync>;
pub type PercentFn = Arc<dyn Fn(f64) + Send + Sync>;

/// What a repair did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RepairOutcome {
    pub steps: Vec<StepState>,
    /// The check after the repair.
    pub report: ModCheckReport,
    /// Stopped by the player.
    pub cancelled: bool,
}

// --- the host ----------------------------------------------------------------------------------

/// Changes for one official pack sync of the set.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PackRequest {
    /// Run the sync even without explicit changes.
    pub sync: bool,
    /// Pack paths (`mods/x.jar`) whose files go back to the pack's version (broken or replaced).
    pub revert: Vec<String>,
    /// Pack paths whose items are switched on.
    pub enable: Vec<String>,
    /// Pack paths whose items are switched off.
    pub disable: Vec<String>,
    /// Items switched on because a mod needs them (dependencies the pack provides).
    pub enable_items: Vec<String>,
    /// The set's folders were emptied: install the saved selection again.
    pub reinstall: bool,
}

impl PackRequest {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// What a repair needs from the launcher.
pub trait RepairHost: Send + Sync {
    /// The set follows the official pack and the pack's manifest is at hand.
    fn has_pack(&self) -> bool;
    /// Pack paths (`mods/x.jar`) the official pack has installed in the set, as recorded now.
    fn pack_files(&self) -> impl Future<Output = Vec<String>> + Send;
    /// The pack item that installs the Modrinth project `project_id` (or a mod known by one of
    /// `slugs`), when the pack has one.
    fn pack_item_for(&self, project_id: &str, slugs: &[String]) -> Option<String>;
    /// Run the official pack sync with `request`; `progress` gets 0–100.
    fn sync_pack(
        &self,
        request: PackRequest,
        progress: PercentFn,
    ) -> impl Future<Output = Result<()>> + Send;
    /// Installable versions of the instance's loader for its Minecraft version, newest first.
    fn loader_versions(&self) -> impl Future<Output = Result<Vec<LoaderVersion>>> + Send;
    /// Use this loader version from now on; `false` when the instance does not choose its
    /// loader version (the official pack does).
    fn set_loader_version(&self, version: &str) -> impl Future<Output = Result<bool>> + Send;
    /// Run the game with at least Java `major` from now on. Returns the Java major that will
    /// run, or `None` when that is not possible.
    fn use_java(&self, major: u32) -> impl Future<Output = Result<Option<u32>>> + Send;
}

// --- running a repair --------------------------------------------------------------------------

/// Everything a repair runs with.
pub struct RepairJob<'a, H: RepairHost> {
    pub host: &'a H,
    /// The active set's mods folder (`<game dir>/mods`).
    pub mods_dir: PathBuf,
    pub env: CheckEnvironment,
    /// Add-on manager of the game directory (installs into its `mods/`).
    pub addons: Addons,
    pub client: ModrinthClient,
    pub downloader: Downloader,
    pub cancel: CancellationToken,
    pub progress: Option<RepairProgressFn>,
}

/// The stable key of a step (one per file, mod or loader).
pub fn step_key(action: &RepairAction) -> String {
    match action {
        RepairAction::VerifyPackFiles => "pack".to_owned(),
        RepairAction::RedownloadPackFile { path } => format!("pack-file:{path}"),
        RepairAction::RevertPackFile { path, .. } => format!("revert:{path}"),
        RepairAction::RedownloadModrinthFile { file, .. } => format!("redownload:{file}"),
        RepairAction::EnableFile { file, .. } => format!("enable:{file}"),
        RepairAction::DisableFile { file, .. } => format!("disable:{file}"),
        RepairAction::UpdateLoader { loader, .. } => format!("loader:{loader:?}"),
        RepairAction::UseJava { .. } => "java".to_owned(),
        RepairAction::UpdateToCompatible { file, .. } => format!("update:{file}"),
        RepairAction::FetchDependency { mod_id, .. } => format!("fetch:{mod_id}"),
    }
}

/// The step key a fix of the check becomes for this set (a pack file's update becomes a
/// revert, a broken file's disable a re-download), to match fixes against steps already run.
pub fn fix_key(fix: &ModFix, ctx: &RepairContext<'_>) -> String {
    step_key(&action_for(with_origin(fix.clone(), ctx.origins), ctx))
}

fn is_network_step(action: &RepairAction) -> bool {
    matches!(
        action,
        RepairAction::UpdateLoader { .. }
            | RepairAction::UseJava { .. }
            | RepairAction::UpdateToCompatible { .. }
            | RepairAction::FetchDependency { .. }
    )
}

/// Whether a planned network step still has work with the mods folder as it is now: a
/// dependency no enabled jar provides, a file that is still there and turned on.
pub fn still_needed(action: &RepairAction, snapshot: &ModSnapshot) -> bool {
    match action {
        RepairAction::FetchDependency { mod_id, .. } => !snapshot
            .enabled()
            .any(|jar| jar.contents.all_mods().any(|m| m.answers_to(mod_id))),
        RepairAction::UpdateToCompatible { file, .. } => snapshot
            .jars
            .iter()
            .any(|jar| jar.enabled && jar.file_name.eq_ignore_ascii_case(file)),
        _ => true,
    }
}

/// A Modrinth project a network step resolved to.
#[derive(Debug, Clone)]
struct Resolved {
    project_id: String,
    /// The version installed now (updates only).
    current_version: Option<String>,
}

struct Run<'j, 'a, H: RepairHost> {
    job: &'j RepairJob<'a, H>,
    env: CheckEnvironment,
    steps: Vec<StepState>,
    percent: Option<f64>,
    checking: bool,
}

impl<H: RepairHost> Run<'_, '_, H> {
    fn emit(&self) {
        if let Some(progress) = &self.job.progress {
            progress(&RepairProgress {
                steps: self.steps.clone(),
                percent: self.percent,
                checking: self.checking,
            });
        }
    }

    fn set(&mut self, index: usize, status: StepStatus, note: Option<StepNote>) {
        if let Some(step) = self.steps.get_mut(index) {
            step.status = status;
            step.note = note;
        }
        if status != StepStatus::Running {
            self.percent = None;
        }
        self.emit();
    }

    fn set_all(&mut self, indexes: &[usize], status: StepStatus, note: Option<StepNote>) {
        for &index in indexes {
            if let Some(step) = self.steps.get_mut(index) {
                step.status = status;
                step.note = note;
            }
        }
        if status != StepStatus::Running {
            self.percent = None;
        }
        self.emit();
    }

    fn cancelled(&self) -> bool {
        self.job.cancel.is_cancelled()
    }

    /// Every step not run yet is skipped (the repair was stopped).
    fn skip_rest(&mut self) {
        for step in &mut self.steps {
            if matches!(step.status, StepStatus::Pending | StepStatus::Running) {
                step.status = StepStatus::Skipped;
                step.note = Some(StepNote::Cancelled);
            }
        }
        self.percent = None;
        self.emit();
    }

    fn pending(&self, pred: impl Fn(&RepairAction) -> bool) -> Vec<usize> {
        self.steps
            .iter()
            .enumerate()
            .filter(|(_, s)| s.status == StepStatus::Pending && pred(&s.step.action))
            .map(|(i, _)| i)
            .collect()
    }

    fn percent_fn(&self) -> PercentFn {
        // Progress inside a step: the UI gets it through the next snapshot, so the closure
        // only needs the reporter and the current list.
        let progress = self.job.progress.clone();
        let steps = self.steps.clone();
        Arc::new(move |percent: f64| {
            if let Some(progress) = &progress {
                progress(&RepairProgress {
                    steps: steps.clone(),
                    percent: Some(percent.clamp(0.0, 100.0)),
                    checking: false,
                });
            }
        })
    }

    async fn scan(&self) -> Result<ModSnapshot> {
        let dir = self.job.mods_dir.clone();
        blocking(move || Ok(ModScanner::shared().scan_dir(&dir))).await
    }

    async fn origins(&self, snapshot: &ModSnapshot) -> FileOrigins {
        let pack = self.job.host.pack_files().await;
        let index = self.job.addons.load_index().await;
        file_origins(snapshot, &pack, &index)
    }

    async fn check(&mut self) -> Result<(ModSnapshot, FileOrigins, ModCheckReport)> {
        self.checking = true;
        self.emit();
        let snapshot = self.scan().await?;
        let origins = self.origins(&snapshot).await;
        let report = check_mods(&snapshot, &self.env, &origins);
        self.checking = false;
        self.emit();
        Ok((snapshot, origins, report))
    }

    /// Phase 1: one pack sync that verifies, puts pack files back and switches pack items on
    /// or off. Without the pack's manifest (offline) pack files can only be renamed, which the
    /// local phase does (the next sync adopts that as the player's choice).
    async fn pack_phase(&mut self, origins: &FileOrigins) {
        let has_pack = self.job.host.has_pack();
        let mut request = PackRequest::default();
        let mut involved = Vec::new();
        for (index, state) in self.steps.iter().enumerate() {
            let pack = |file: &str| match origins.get(strip_disabled(file)) {
                Some(FileOrigin::Pack { path }) if has_pack => Some(path.clone()),
                _ => None,
            };
            match &state.step.action {
                RepairAction::VerifyPackFiles => request.sync = true,
                RepairAction::RedownloadPackFile { path }
                | RepairAction::RevertPackFile { path, .. } => request.revert.push(path.clone()),
                RepairAction::EnableFile { file, .. } => match pack(file) {
                    Some(path) => request.enable.push(path),
                    None => continue,
                },
                RepairAction::DisableFile { file, .. } => match pack(file) {
                    Some(path) => request.disable.push(path),
                    None => continue,
                },
                _ => continue,
            }
            involved.push(index);
        }
        if involved.is_empty() {
            return;
        }
        if !has_pack {
            self.set_all(&involved, StepStatus::Failed, Some(StepNote::Offline));
            return;
        }
        self.set_all(&involved, StepStatus::Running, None);
        let disabled = request.disable.clone();
        let result = self.job.host.sync_pack(request, self.percent_fn()).await;
        match result {
            Ok(()) => {
                // A pack item another enabled item needs stays installed.
                for index in involved {
                    let kept = match &self.steps[index].step.action {
                        RepairAction::DisableFile { file, .. } => {
                            let path = self.job.mods_dir.join(strip_disabled(file));
                            disabled
                                .iter()
                                .any(|p| pack_file_name(p) == strip_disabled(file))
                                && std::fs::symlink_metadata(&path).is_ok()
                        }
                        _ => false,
                    };
                    if kept {
                        self.set(index, StepStatus::Failed, Some(StepNote::KeptByPack));
                    } else {
                        self.set(index, StepStatus::Done, None);
                    }
                }
            }
            Err(error) if error.is_cancelled() => {}
            Err(error) => {
                tracing::warn!(%error, "pack sync during a mod repair failed");
                let note = if error.is_offline() {
                    StepNote::Offline
                } else {
                    StepNote::Busy
                };
                self.set_all(&involved, StepStatus::Failed, Some(note));
            }
        }
    }

    /// Phase 2: Broken Modrinth files: the version the file came from, downloaded again.
    async fn redownload_phase(&mut self) {
        let indexes = self.pending(|a| matches!(a, RepairAction::RedownloadModrinthFile { .. }));
        if indexes.is_empty() {
            return;
        }
        self.set_all(&indexes, StepStatus::Running, None);
        let mut roots: Vec<ModrinthVersion> = Vec::new();
        let mut by_project: HashMap<String, Vec<usize>> = HashMap::new();
        let target = target_for(&self.env);
        let pinned: Vec<String> = indexes
            .iter()
            .filter_map(|&i| match &self.steps[i].step.action {
                RepairAction::RedownloadModrinthFile { version_id, .. } => version_id.clone(),
                _ => None,
            })
            .collect();
        let known = if pinned.is_empty() {
            Ok(Vec::new())
        } else {
            self.job.client.versions(&pinned).await
        };
        let known = match known {
            Ok(list) => list,
            Err(error) => {
                let note = network_note(&error);
                self.set_all(&indexes, StepStatus::Failed, Some(note));
                return;
            }
        };
        for &index in &indexes {
            let RepairAction::RedownloadModrinthFile {
                file,
                project_id,
                version_id,
            } = self.steps[index].step.action.clone()
            else {
                continue;
            };
            let version = match version_id
                .as_ref()
                .and_then(|id| known.iter().find(|v| &v.id == id))
            {
                Some(version) => Some(version.clone()),
                None => self
                    .job
                    .client
                    .best_version(&project_id, &target)
                    .await
                    .ok()
                    .flatten(),
            };
            let Some(version) = version else {
                self.set(index, StepStatus::Failed, Some(StepNote::NoVersion));
                continue;
            };
            // The broken copy goes first, or the installer would take it for the version.
            let path = self.job.mods_dir.join(&file);
            if std::fs::symlink_metadata(&path).is_ok()
                && let Err(error) = self.job.addons.delete_file(&path).await
            {
                tracing::warn!(%error, path = %path.display(), "could not remove a broken mod");
                self.set(index, StepStatus::Failed, Some(StepNote::Busy));
                continue;
            }
            by_project
                .entry(version.project_id.clone())
                .or_default()
                .push(index);
            roots.push(version);
        }
        if roots.is_empty() {
            return;
        }
        match self.install(roots, false).await {
            Ok(report) => {
                let installed = installed_projects(&report);
                for (project, indexes) in by_project {
                    let status = if installed.contains(&project) {
                        StepStatus::Done
                    } else {
                        StepStatus::Failed
                    };
                    self.set_all(&indexes, status, None);
                }
            }
            Err(error) if error.is_cancelled() => {}
            Err(error) => {
                let note = network_note(&error);
                let all: Vec<usize> = by_project.into_values().flatten().collect();
                self.set_all(&all, StepStatus::Failed, Some(note));
            }
        }
    }

    /// Phase 3: The player's own files turned on or off.
    async fn local_phase(&mut self, origins: &FileOrigins) {
        let indexes = self.pending(|a| {
            matches!(
                a,
                RepairAction::EnableFile { .. } | RepairAction::DisableFile { .. }
            )
        });
        for index in indexes {
            if self.cancelled() {
                return;
            }
            let (file, enable) = match &self.steps[index].step.action {
                RepairAction::EnableFile { file, .. } => (file.clone(), true),
                RepairAction::DisableFile { file, .. } => (file.clone(), false),
                _ => continue,
            };
            if self.job.host.has_pack()
                && matches!(
                    origins.get(strip_disabled(&file)),
                    Some(FileOrigin::Pack { .. })
                )
            {
                // Pack files were handled by the pack phase.
                continue;
            }
            self.set(index, StepStatus::Running, None);
            let path = self.job.mods_dir.join(&file);
            let result = if enable {
                super::set_enabled(&path, true).await.map(|_| ())
            } else {
                disable_file(&path).await
            };
            match result {
                Ok(()) => self.set(index, StepStatus::Done, None),
                Err(error) => {
                    tracing::warn!(%error, path = %path.display(), "could not rename a mod");
                    self.set(index, StepStatus::Failed, Some(StepNote::Busy));
                }
            }
        }
    }

    /// Phase 4: After the local steps: plan the network steps again from a fresh check.
    async fn replan(&mut self, crash: Option<&CrashDiagnosis>) -> Result<()> {
        let (snapshot, origins, report) = self.check().await?;
        let ctx = RepairContext {
            env: &self.env,
            origins: &origins,
            has_pack: self.job.host.has_pack(),
        };
        let fresh = plan_repair(&ctx, Some(&report), crash);
        let wanted: Vec<RepairStep> = fresh
            .steps
            .into_iter()
            .filter(|s| is_network_step(&s.action) && still_needed(&s.action, &snapshot))
            .collect();
        let wanted_keys: HashSet<String> = wanted.iter().map(|s| step_key(&s.action)).collect();
        // Planned network steps nobody needs any more.
        for state in &mut self.steps {
            if state.status == StepStatus::Pending
                && is_network_step(&state.step.action)
                && !wanted_keys.contains(&state.key)
            {
                state.status = StepStatus::Skipped;
                state.note = Some(StepNote::NotNeeded);
            }
        }
        // Updated requirements for the ones that stay, new ones at the end.
        for step in wanted {
            let key = step_key(&step.action);
            match self.steps.iter_mut().find(|s| s.key == key) {
                Some(state) if state.status == StepStatus::Pending => state.step = step,
                Some(_) => {}
                None => self.steps.push(StepState {
                    key,
                    step,
                    status: StepStatus::Pending,
                    note: None,
                }),
            }
        }
        self.emit();
        Ok(())
    }

    /// Phase 5a: A loader version every mod accepts.
    async fn loader_phase(&mut self) {
        for index in self.pending(|a| matches!(a, RepairAction::UpdateLoader { .. })) {
            if self.cancelled() {
                return;
            }
            let RepairAction::UpdateLoader {
                loader,
                version_reqs,
            } = self.steps[index].step.action.clone()
            else {
                continue;
            };
            self.set(index, StepStatus::Running, None);
            let versions = match self.job.host.loader_versions().await {
                Ok(versions) => versions,
                Err(error) => {
                    self.set(index, StepStatus::Failed, Some(network_note(&error)));
                    continue;
                }
            };
            let Some(version) = pick_loader_version(&versions, loader, &version_reqs) else {
                self.set(index, StepStatus::Failed, Some(StepNote::NoVersion));
                continue;
            };
            match self.job.host.set_loader_version(&version).await {
                Ok(true) => {
                    self.env.loader_version = Some(version);
                    self.set(index, StepStatus::Done, None);
                }
                Ok(false) => self.set(index, StepStatus::Failed, Some(StepNote::PackDecides)),
                Err(error) => self.set(index, StepStatus::Failed, Some(network_note(&error))),
            }
        }
    }

    /// Phase 5b: Java.
    async fn java_phase(&mut self) {
        for index in self.pending(|a| matches!(a, RepairAction::UseJava { .. })) {
            if self.cancelled() {
                return;
            }
            let RepairAction::UseJava { major } = self.steps[index].step.action else {
                continue;
            };
            self.set(index, StepStatus::Running, None);
            match self.job.host.use_java(major).await {
                Ok(Some(running)) => {
                    self.env.java_major = Some(running);
                    let status = if running >= major {
                        StepStatus::Done
                    } else {
                        StepStatus::Failed
                    };
                    let note = (status == StepStatus::Failed).then_some(StepNote::NotPossible);
                    self.set(index, status, note);
                }
                Ok(None) => self.set(index, StepStatus::Failed, Some(StepNote::NotPossible)),
                Err(error) => self.set(index, StepStatus::Failed, Some(network_note(&error))),
            }
        }
    }

    /// Phase 6: Own mods updated, missing dependencies downloaded (or switched on in the pack).
    async fn modrinth_phase(&mut self) {
        let indexes = self.pending(|a| {
            matches!(
                a,
                RepairAction::UpdateToCompatible { .. } | RepairAction::FetchDependency { .. }
            )
        });
        if indexes.is_empty() || self.cancelled() {
            return;
        }
        self.set_all(&indexes, StepStatus::Running, None);

        // Projects: the file's origin, else Modrinth by SHA-1 (one request), else by slug.
        let mut resolved: HashMap<usize, Resolved> = HashMap::new();
        let needs_hash = indexes.iter().any(|&i| {
            matches!(
                &self.steps[i].step.action,
                RepairAction::UpdateToCompatible { lookup, .. } if lookup.project_id.is_none()
            )
        });
        let mut by_file: HashMap<String, (String, String)> = HashMap::new();
        let origins = match self.scan().await {
            Ok(snapshot) => self.origins(&snapshot).await,
            Err(_) => FileOrigins::new(),
        };
        if needs_hash {
            match self
                .job
                .addons
                .identify(&self.job.client, ImportKind::Mod)
                .await
            {
                Ok(scan) if !scan.offline => {
                    for entry in scan.entries {
                        if let Some(source) = entry.source {
                            by_file.insert(
                                strip_disabled(&entry.file_name).to_ascii_lowercase(),
                                (source.project_id, source.version_id),
                            );
                        }
                    }
                }
                Ok(_) => {
                    self.set_all(&indexes, StepStatus::Failed, Some(StepNote::Offline));
                    return;
                }
                Err(error) => tracing::debug!(%error, "could not identify mods by hash"),
            }
        }
        let mut slug_wanted: Vec<String> = Vec::new();
        for &index in &indexes {
            match &self.steps[index].step.action {
                RepairAction::UpdateToCompatible { file, lookup, .. } => {
                    let current = match origins.get(strip_disabled(file)) {
                        Some(FileOrigin::Modrinth { version_id, .. }) => version_id.clone(),
                        _ => None,
                    };
                    let hashed = by_file.get(&strip_disabled(file).to_ascii_lowercase());
                    match (&lookup.project_id, hashed) {
                        (Some(project_id), _) => {
                            resolved.insert(
                                index,
                                Resolved {
                                    project_id: project_id.clone(),
                                    current_version: current
                                        .or_else(|| hashed.map(|(_, v)| v.clone())),
                                },
                            );
                        }
                        (None, Some((project_id, version_id))) => {
                            resolved.insert(
                                index,
                                Resolved {
                                    project_id: project_id.clone(),
                                    current_version: Some(version_id.clone()),
                                },
                            );
                        }
                        (None, None) => slug_wanted.extend(lookup.slugs.iter().cloned()),
                    }
                }
                RepairAction::FetchDependency { lookup, .. } => {
                    slug_wanted.extend(lookup.slugs.iter().cloned());
                }
                _ => {}
            }
        }
        let mut projects: HashMap<String, ModrinthProject> = HashMap::new();
        slug_wanted.sort();
        slug_wanted.dedup();
        if !slug_wanted.is_empty() {
            match self.job.client.projects(&slug_wanted).await {
                Ok(list) => {
                    for project in list {
                        if let Some(slug) = &project.slug {
                            projects.insert(slug.to_ascii_lowercase(), project.clone());
                        }
                        projects.insert(project.id.to_ascii_lowercase(), project);
                    }
                }
                Err(error) => {
                    let unresolved: Vec<usize> = indexes
                        .iter()
                        .copied()
                        .filter(|i| !resolved.contains_key(i))
                        .collect();
                    self.set_all(&unresolved, StepStatus::Failed, Some(network_note(&error)));
                }
            }
        }
        let loaders = target_for(&self.env).loaders;
        let mut pack_items: Vec<(usize, String)> = Vec::new();
        for &index in &indexes {
            if resolved.contains_key(&index) || self.steps[index].status != StepStatus::Running {
                continue;
            }
            let lookup = match &self.steps[index].step.action {
                RepairAction::UpdateToCompatible { lookup, .. }
                | RepairAction::FetchDependency { lookup, .. } => lookup.clone(),
                _ => continue,
            };
            let project = lookup.slugs.iter().find_map(|slug| {
                projects
                    .get(&slug.to_ascii_lowercase())
                    .filter(|p| p.project_type == ModrinthProjectType::Mod)
                    .filter(|p| {
                        p.loaders.is_empty()
                            || p.loaders.iter().any(|l| loaders.iter().any(|t| t == l))
                    })
            });
            // A dependency the official pack offers comes from the pack.
            if matches!(
                self.steps[index].step.action,
                RepairAction::FetchDependency { .. }
            ) && let Some(item) = self.job.host.pack_item_for(
                project.map(|p| p.id.as_str()).unwrap_or_default(),
                &lookup.slugs,
            ) {
                pack_items.push((index, item));
                continue;
            }
            match (project, &self.steps[index].step.action) {
                (Some(project), _) => {
                    resolved.insert(
                        index,
                        Resolved {
                            project_id: project.id.clone(),
                            current_version: None,
                        },
                    );
                }
                // Not on Modrinth, so no fitting version: turned off when the step allows it.
                (
                    None,
                    RepairAction::UpdateToCompatible {
                        file,
                        disable_if_unavailable: true,
                        ..
                    },
                ) => {
                    let path = self.job.mods_dir.join(file);
                    match disable_file(&path).await {
                        Ok(()) => {
                            self.set(index, StepStatus::Done, Some(StepNote::DisabledInstead))
                        }
                        Err(_) => self.set(index, StepStatus::Failed, Some(StepNote::Busy)),
                    }
                }
                (None, _) => self.set(index, StepStatus::Failed, Some(StepNote::NotFound)),
            }
        }

        // One version per project, meeting every step's requirements.
        let target = target_for(&self.env);
        let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
        for (&index, r) in &resolved {
            groups.entry(r.project_id.clone()).or_default().push(index);
        }
        let mut roots: Vec<ModrinthVersion> = Vec::new();
        let mut installing: HashMap<String, Vec<usize>> = HashMap::new();
        let mut group_list: Vec<(String, Vec<usize>)> = groups.into_iter().collect();
        group_list.sort();
        for (project_id, mut members) in group_list {
            members.sort_unstable();
            if self.cancelled() {
                break;
            }
            let reqs: Vec<String> = members
                .iter()
                .flat_map(|&i| match &self.steps[i].step.action {
                    RepairAction::UpdateToCompatible { lookup, .. }
                    | RepairAction::FetchDependency { lookup, .. } => lookup.version_reqs.clone(),
                    _ => Vec::new(),
                })
                .collect();
            let versions = match self
                .job
                .client
                .project_versions(&project_id, &target.loaders, &target.game_versions)
                .await
            {
                Ok(versions) => versions,
                Err(error) => {
                    self.set_all(&members, StepStatus::Failed, Some(network_note(&error)));
                    continue;
                }
            };
            let chosen = pick_version(&versions, &target, self.env.loader, &reqs).cloned();
            let mut to_install = Vec::new();
            for &index in &members {
                let current = resolved.get(&index).and_then(|r| r.current_version.clone());
                let fresh = chosen
                    .as_ref()
                    .filter(|v| current.as_deref() != Some(v.id.as_str()));
                match (&self.steps[index].step.action, fresh) {
                    (_, Some(_)) => to_install.push(index),
                    (
                        RepairAction::UpdateToCompatible {
                            file,
                            disable_if_unavailable: true,
                            ..
                        },
                        None,
                    ) => {
                        let path = self.job.mods_dir.join(file);
                        match disable_file(&path).await {
                            Ok(()) => {
                                self.set(index, StepStatus::Done, Some(StepNote::DisabledInstead));
                            }
                            Err(_) => self.set(index, StepStatus::Failed, Some(StepNote::Busy)),
                        }
                    }
                    _ => self.set(index, StepStatus::Failed, Some(StepNote::NoVersion)),
                }
            }
            if let Some(version) = chosen
                && !to_install.is_empty()
            {
                installing.insert(version.project_id.clone(), to_install);
                roots.push(version);
            }
        }

        if !roots.is_empty() && !self.cancelled() {
            match self.install(roots, true).await {
                Ok(report) => {
                    let installed = installed_projects(&report);
                    for (project, members) in installing {
                        if !installed.contains(&project) {
                            self.set_all(&members, StepStatus::Failed, None);
                            continue;
                        }
                        for index in members {
                            // An old file Modrinth did not know is not replaced by the
                            // installer: turn it off so the mod is not there twice.
                            if let RepairAction::UpdateToCompatible { file, .. } =
                                &self.steps[index].step.action
                            {
                                let path = self.job.mods_dir.join(file);
                                let new_file = report
                                    .installed
                                    .iter()
                                    .any(|i| i.project_id == project && i.file_name == *file);
                                if !new_file && std::fs::symlink_metadata(&path).is_ok() {
                                    let _ = disable_file(&path).await;
                                }
                            }
                            self.set(index, StepStatus::Done, None);
                        }
                    }
                }
                Err(error) if error.is_cancelled() => {}
                Err(error) => {
                    let note = network_note(&error);
                    let all: Vec<usize> = installing.into_values().flatten().collect();
                    self.set_all(&all, StepStatus::Failed, Some(note));
                }
            }
        }

        // Dependencies the pack offers: switched on in one more pack sync.
        if !pack_items.is_empty() && !self.cancelled() {
            let members: Vec<usize> = pack_items.iter().map(|(i, _)| *i).collect();
            let request = PackRequest {
                enable_items: pack_items.into_iter().map(|(_, item)| item).collect(),
                ..PackRequest::default()
            };
            match self.job.host.sync_pack(request, self.percent_fn()).await {
                Ok(()) => self.set_all(&members, StepStatus::Done, Some(StepNote::FromPack)),
                Err(error) if error.is_cancelled() => {}
                Err(error) => {
                    self.set_all(&members, StepStatus::Failed, Some(network_note(&error)))
                }
            }
        }
    }

    async fn install(
        &self,
        roots: Vec<ModrinthVersion>,
        with_dependencies: bool,
    ) -> Result<AddonInstallReport> {
        let progress = self.percent_fn();
        let target = target_for(&self.env);
        self.job
            .addons
            .install(
                &self.job.client,
                &self.job.downloader,
                ImportKind::Mod,
                roots,
                &target,
                AddonInstallOptions {
                    with_dependencies,
                    concurrency: 6,
                    cancel: self.job.cancel.clone(),
                    progress: Some(Arc::new(move |p: AddonProgress| {
                        if let AddonProgress::Downloading {
                            done_bytes,
                            total_bytes,
                            done_files,
                            total_files,
                            ..
                        } = p
                        {
                            let fraction = if total_bytes > 0 {
                                done_bytes as f64 / total_bytes as f64
                            } else if total_files > 0 {
                                done_files as f64 / total_files as f64
                            } else {
                                0.0
                            };
                            progress(fraction * 100.0);
                        }
                    })),
                },
            )
            .await
    }
}

fn target_for(env: &CheckEnvironment) -> ModrinthResolveTarget {
    ModrinthResolveTarget::for_loader(env.loader, env.minecraft.clone())
}

fn network_note(error: &Error) -> StepNote {
    if error.is_offline() || matches!(error, Error::Net(_)) {
        StepNote::Offline
    } else if matches!(error, Error::NotFound(_)) {
        StepNote::NotFound
    } else {
        StepNote::Busy
    }
}

fn installed_projects(report: &AddonInstallReport) -> HashSet<String> {
    report
        .installed
        .iter()
        .chain(&report.already_installed)
        .map(|i| i.project_id.clone())
        .collect()
}

/// `x.jar` → `x.jar.disabled`; when that name is taken, `x (2).jar.disabled` and so on.
async fn disable_file(path: &Path) -> Result<()> {
    match super::set_enabled(path, false).await {
        Ok(_) => Ok(()),
        Err(Error::AlreadyExists(_)) => {
            let path = path.to_path_buf();
            blocking(move || {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let stem = name.strip_suffix(".jar").unwrap_or(&name).to_owned();
                let target = (2..1000)
                    .map(|n| path.with_file_name(format!("{stem} ({n}).jar.disabled")))
                    .find(|candidate| std::fs::symlink_metadata(candidate).is_err())
                    .ok_or_else(|| Error::AlreadyExists(path.clone()))?;
                std::fs::rename(&path, &target).map_err(|e| Error::io(&path, e))
            })
            .await
        }
        Err(error) => Err(error),
    }
}

/// Carry out `plan` (made for `job.env` from the check and, for a crash, `crash`). Steps that
/// fail are recorded and the repair goes on; it stops early only when cancelled. The outcome
/// carries every step's result and a fresh check of the mods folder.
pub async fn run_repair<H: RepairHost>(
    job: &RepairJob<'_, H>,
    plan: RepairPlan,
    crash: Option<&CrashDiagnosis>,
) -> Result<RepairOutcome> {
    let mut run = Run {
        job,
        env: job.env.clone(),
        steps: plan
            .steps
            .into_iter()
            .map(|step| StepState {
                key: step_key(&step.action),
                step,
                status: StepStatus::Pending,
                note: None,
            })
            .collect(),
        percent: None,
        checking: false,
    };
    run.emit();
    let snapshot = run.scan().await?;
    let origins = run.origins(&snapshot).await;

    if !run.cancelled() {
        run.pack_phase(&origins).await;
    }
    if !run.cancelled() {
        run.redownload_phase().await;
    }
    if !run.cancelled() {
        run.local_phase(&origins).await;
    }
    if !run.cancelled() {
        run.replan(crash).await?;
    }
    if !run.cancelled() {
        run.loader_phase().await;
    }
    if !run.cancelled() {
        run.java_phase().await;
    }
    if !run.cancelled() {
        run.modrinth_phase().await;
    }
    let cancelled = run.cancelled();
    if cancelled {
        run.skip_rest();
    }
    let (_, _, report) = run.check().await?;
    Ok(RepairOutcome {
        steps: run.steps,
        report,
        cancelled,
    })
}

// --- reinstalling the set ----------------------------------------------------------------------

/// A phase of "Yeniden yükle".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ReinstallPhase {
    /// Jars of unknown origin are copied aside.
    KeepAside,
    /// The set's `mods/` and `config/` go to the launcher trash.
    Trash,
    /// The official pack's selection is installed again.
    Pack,
    /// The player's Modrinth mods are installed again.
    Modrinth,
    /// The new set is checked.
    Check,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ReinstallProgress {
    /// Every phase this reinstall goes through, in order.
    pub phases: Vec<ReinstallPhase>,
    pub phase: ReinstallPhase,
    /// Download progress of the phase, 0–100.
    pub percent: Option<f64>,
    /// It can still be stopped (nothing has moved yet).
    pub cancellable: bool,
}

pub type ReinstallProgressFn = Arc<dyn Fn(&ReinstallProgress) + Send + Sync>;

/// What a reinstall did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ReinstallOutcome {
    /// Jars of unknown origin and where their copies are.
    pub kept: Vec<KeptJar>,
    /// The official pack's files are in place (`false`: the next launch installs them).
    pub pack_synced: bool,
    /// Names of the Modrinth mods installed again.
    pub reinstalled: Vec<String>,
    /// Names of Modrinth mods without a version for this game version (or unreachable):
    /// their old files were put back from the trash.
    pub restored: Vec<String>,
    /// `config/` went to the trash, so mod settings start from their defaults.
    pub settings_reset: bool,
    /// The check of the new set.
    pub report: ModCheckReport,
    /// Stopped before anything moved.
    pub cancelled: bool,
}

/// Everything a reinstall runs with.
pub struct ReinstallJob<'a, H: RepairHost> {
    pub host: &'a H,
    pub plan: ReinstallPlan,
    pub trash: LauncherTrash,
    pub instance_id: String,
    pub env: CheckEnvironment,
    pub addons: Addons,
    pub client: ModrinthClient,
    pub downloader: Downloader,
    pub cancel: CancellationToken,
    pub progress: Option<ReinstallProgressFn>,
}

/// `path` when free, else `name (2).ext`, `name (3).ext`, … next to it.
fn free_file_path(path: &Path) -> PathBuf {
    if std::fs::symlink_metadata(path).is_err() {
        return path.to_path_buf();
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (stem, ext) = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem.to_owned(), format!(".{ext}")),
        _ => (name.clone(), String::new()),
    };
    (2..10_000)
        .map(|n| path.with_file_name(format!("{stem} ({n}){ext}")))
        .find(|candidate| std::fs::symlink_metadata(candidate).is_err())
        .unwrap_or_else(|| path.with_file_name(format!("{stem}-{}{ext}", uuid::Uuid::new_v4())))
}

/// Reinstall one version's mod set as `job.plan` describes: jars of unknown origin are copied
/// aside first, `mods/` and `config/` go to the launcher trash (recoverable), the pack's saved
/// selection and the player's Modrinth mods are installed again (newest version for the game
/// version and loader; turned off when they were off). A Modrinth mod that cannot be installed
/// gets its old file back from the trash, so nothing the player had is lost. It can be stopped
/// only until the files move.
pub async fn run_reinstall<H: RepairHost>(job: &ReinstallJob<'_, H>) -> Result<ReinstallOutcome> {
    let plan = &job.plan;
    let mut phases = Vec::new();
    if !plan.keep_aside.is_empty() {
        phases.push(ReinstallPhase::KeepAside);
    }
    phases.push(ReinstallPhase::Trash);
    if plan.resync_pack {
        phases.push(ReinstallPhase::Pack);
    }
    if !plan.reinstall_from_modrinth.is_empty() {
        phases.push(ReinstallPhase::Modrinth);
    }
    phases.push(ReinstallPhase::Check);
    let report = |phase: ReinstallPhase, percent: Option<f64>, cancellable: bool| {
        if let Some(progress) = &job.progress {
            progress(&ReinstallProgress {
                phases: phases.clone(),
                phase,
                percent,
                cancellable,
            });
        }
    };
    let mut outcome = ReinstallOutcome {
        kept: Vec::new(),
        pack_synced: false,
        reinstalled: Vec::new(),
        restored: Vec::new(),
        settings_reset: false,
        report: ModCheckReport::default(),
        cancelled: false,
    };

    // Unknown jars: a copy outside the set, so they are never lost.
    if !plan.keep_aside.is_empty() {
        report(ReinstallPhase::KeepAside, None, true);
        let kept = plan.keep_aside.clone();
        outcome.kept = blocking(move || {
            let mut done = Vec::with_capacity(kept.len());
            for jar in kept {
                let wanted = jar.to.with_file_name(strip_disabled(&jar.file));
                if let Some(parent) = wanted.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
                }
                let to = free_file_path(&wanted);
                std::fs::copy(&jar.from, &to).map_err(|e| Error::io(&jar.from, e))?;
                done.push(KeptJar { to, ..jar });
            }
            Ok(done)
        })
        .await?;
    }
    if job.cancel.is_cancelled() {
        outcome.cancelled = true;
        return Ok(outcome);
    }

    // The old set goes to the launcher trash; `mods/` must move, `config/` may stay.
    report(ReinstallPhase::Trash, None, false);
    let mut mods_entry: Option<TrashEntry> = None;
    for dir in &plan.move_to_trash {
        let folder = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let name = format!("{} {folder}", plan.minecraft);
        match job
            .trash
            .put(dir, TrashKind::ModSet, &job.instance_id, &name)
            .await
        {
            Ok(entry) if folder == "mods" => mods_entry = Some(entry),
            Ok(_) => outcome.settings_reset = true,
            Err(error) if folder == "mods" => return Err(error),
            Err(error) => tracing::warn!(%error, "config folder stays (could not be moved)"),
        }
    }
    for dir in &plan.move_to_trash {
        tokio::fs::create_dir_all(dir)
            .await
            .map_err(|e| Error::io(dir, e))?;
    }
    let mods_dir = plan
        .move_to_trash
        .iter()
        .find(|d| d.file_name().is_some_and(|n| n == "mods"))
        .cloned()
        .unwrap_or_else(|| job.addons.game_dir().join("mods"));
    tokio::fs::create_dir_all(&mods_dir)
        .await
        .map_err(|e| Error::io(&mods_dir, e))?;
    // From here on the job finishes: a stop request is ignored.
    let cancel = CancellationToken::new();

    if plan.resync_pack {
        report(ReinstallPhase::Pack, None, false);
        let progress_report = job.progress.clone();
        let phases_for_pack = phases.clone();
        let progress: PercentFn = Arc::new(move |percent| {
            if let Some(progress) = &progress_report {
                progress(&ReinstallProgress {
                    phases: phases_for_pack.clone(),
                    phase: ReinstallPhase::Pack,
                    percent: Some(percent),
                    cancellable: false,
                });
            }
        });
        let request = PackRequest {
            sync: true,
            reinstall: true,
            ..PackRequest::default()
        };
        match job.host.sync_pack(request, progress).await {
            Ok(()) => outcome.pack_synced = true,
            Err(error) => tracing::warn!(%error, "pack sync after a reinstall failed"),
        }
    }

    if !plan.reinstall_from_modrinth.is_empty() {
        report(ReinstallPhase::Modrinth, None, false);
        let target = target_for(&job.env);
        let mut roots: Vec<ModrinthVersion> = Vec::new();
        let mut missing: Vec<usize> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        for (i, entry) in plan.reinstall_from_modrinth.iter().enumerate() {
            if !seen.insert(entry.project_id.clone()) {
                continue;
            }
            match job.client.best_version(&entry.project_id, &target).await {
                Ok(Some(version)) => roots.push(version),
                Ok(None) | Err(_) => missing.push(i),
            }
        }
        let progress_report = job.progress.clone();
        let phases_for_mods = phases.clone();
        let installed = if roots.is_empty() {
            Ok(AddonInstallReport::default())
        } else {
            job.addons
                .install(
                    &job.client,
                    &job.downloader,
                    ImportKind::Mod,
                    roots,
                    &target,
                    AddonInstallOptions {
                        with_dependencies: true,
                        concurrency: 6,
                        cancel: cancel.clone(),
                        progress: Some(Arc::new(move |p: AddonProgress| {
                            if let (
                                Some(progress),
                                AddonProgress::Downloading {
                                    done_bytes,
                                    total_bytes,
                                    ..
                                },
                            ) = (&progress_report, p)
                                && total_bytes > 0
                            {
                                progress(&ReinstallProgress {
                                    phases: phases_for_mods.clone(),
                                    phase: ReinstallPhase::Modrinth,
                                    percent: Some(done_bytes as f64 / total_bytes as f64 * 100.0),
                                    cancellable: false,
                                });
                            }
                        })),
                    },
                )
                .await
        };
        let done: HashMap<String, String> = match &installed {
            Ok(report) => report
                .installed
                .iter()
                .chain(&report.already_installed)
                .map(|i| (i.project_id.clone(), i.file_name.clone()))
                .collect(),
            Err(error) => {
                tracing::warn!(%error, "reinstalling Modrinth mods failed");
                HashMap::new()
            }
        };
        for (i, entry) in plan.reinstall_from_modrinth.iter().enumerate() {
            match done.get(&entry.project_id) {
                Some(file) if !missing.contains(&i) => {
                    if !entry.enabled && !file.ends_with(".disabled") {
                        let _ = super::set_enabled(&mods_dir.join(file), false).await;
                    }
                    if !outcome.reinstalled.contains(&entry.name) {
                        outcome.reinstalled.push(entry.name.clone());
                    }
                }
                _ => {
                    // Put the old file back from the trash: the player keeps the mod.
                    let back = mods_entry.as_ref().map(|e| {
                        let name = if entry.enabled {
                            entry.file.clone()
                        } else {
                            format!("{}.disabled", entry.file)
                        };
                        (job.trash.item_path(e), name)
                    });
                    if let Some((trashed, name)) = back {
                        let from = [
                            name.clone(),
                            entry.file.clone(),
                            format!("{}.disabled", entry.file),
                        ]
                        .into_iter()
                        .map(|n| trashed.join(n))
                        .find(|p| p.is_file());
                        if let Some(from) = from {
                            let to = free_file_path(&mods_dir.join(&name));
                            if tokio::fs::copy(&from, &to).await.is_ok() {
                                outcome.restored.push(entry.name.clone());
                            }
                        }
                    }
                }
            }
        }
    }

    report(ReinstallPhase::Check, None, false);
    let dir = mods_dir.clone();
    let snapshot = blocking(move || Ok(ModScanner::shared().scan_dir(&dir))).await?;
    let pack = job.host.pack_files().await;
    let index = job.addons.load_index().await;
    let origins = file_origins(&snapshot, &pack, &index);
    outcome.report = check_mods(&snapshot, &job.env, &origins);
    Ok(outcome)
}

#[cfg(test)]
mod tests;
