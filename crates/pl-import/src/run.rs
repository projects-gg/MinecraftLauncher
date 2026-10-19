//! Bringing the picked items over, one after the other: game settings and the server list first
//! (instant), then the profiles (each a new instance with its mods), then worlds (each into the
//! instance it was last played with, else the Projects instance), packs and screenshots.
//!
//! Every item ends with an [`ItemOutcome`]; one that fails does not stop the others. A cancel
//! stops after the file being copied: finished items stay, the one in progress leaves nothing
//! (a half-made instance is removed again), the rest are reported as cancelled.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pl_mc::instance::store::free_name as free_instance_name;
use pl_mc::instance::{
    DEFAULT_SERVER_ADDRESS, DEFAULT_SERVER_NAME, InstanceIcon, InstanceLoader, InstanceStore,
    KeyBinding, NewInstance, official_pack_bindings, official_pack_key_lines,
};
use pl_mc::loaders::LoaderKind;
use pl_mc::server::ServerList;
use tokio::sync::OwnedMutexGuard;
use tokio_util::sync::CancellationToken;

use crate::copy::{Copier, free_name, no_skip, remove_leftovers, tree_size};
use crate::detect::{is_pack_entry, is_screenshot_entry};
use crate::error::{Error, Result};
use crate::journal::{Journal, JournalFrom, path_key};
use crate::measure::{instance_skip, plan_mods, shared_sizes};
use crate::model::{
    CreatedInstance, ImportProfile, ImportProgress, ImportReport, ImportRequest, ImportShared,
    ImportSource, ImportStage, ItemOutcome, ItemRef, OutcomeReason, OutcomeStatus,
};
use crate::options::merge_options;
use crate::servers::merge_server_lists;
use crate::worlds::{WorldDir, in_use, list_worlds};

/// The lock of an instance (the app's: launches, mod changes and installs take it too).
pub type InstanceLocker = Arc<dyn Fn(&str) -> Arc<tokio::sync::Mutex<()>> + Send + Sync>;
pub type ProgressFn = Arc<dyn Fn(ImportProgress) + Send + Sync>;

/// How often progress is reported while bytes flow.
const PROGRESS_EVERY: Duration = Duration::from_millis(120);
const MAX_OPTIONS: u64 = 2 * 1024 * 1024;

/// The Projects instance: where shared data goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfficialTarget {
    pub id: String,
    pub name: String,
    pub minecraft: String,
    /// It follows the official pack: a new `options.txt` gets the pack's key table.
    pub official_pack: bool,
}

pub struct RunContext {
    pub store: InstanceStore,
    /// `None` when it could not be prepared (offline at the first start): shared data then
    /// stays behind, profiles still come over.
    pub official: Option<OfficialTarget>,
    pub locker: InstanceLocker,
    pub cancel: CancellationToken,
    pub progress: ProgressFn,
    /// Bytes left free on the disk: the launcher still downloads the games afterwards.
    pub reserve_bytes: u64,
}

#[derive(Debug, Clone)]
struct Target {
    id: String,
    name: String,
    instance_dir: PathBuf,
    game_dir: PathBuf,
}

/// An instance made in this run from a profile (worlds of its source follow it).
#[derive(Debug, Clone)]
struct Made {
    source_id: String,
    target: Target,
    minecraft: String,
    loader: LoaderKind,
}

enum Task<'a> {
    Shared {
        source: &'a ImportSource,
        kind: ImportShared,
    },
    Profile {
        source: &'a ImportSource,
        profile: &'a ImportProfile,
    },
}

/// Copies what `request` picks from `sources` (a fresh [`crate::scan`]).
pub async fn run_import(
    sources: Vec<ImportSource>,
    request: ImportRequest,
    ctx: RunContext,
) -> ImportReport {
    let tasks = plan(&sources, &request);
    let progress = Arc::new(Progress::new(ctx.progress.clone(), tasks.len() as u32));
    progress.stage(ImportStage::Preparing);
    let sizes = task_sizes(&tasks).await;
    progress.set_total(sizes.iter().sum());
    progress.stage(ImportStage::Copying);

    let mut runner = Runner {
        ctx,
        progress: progress.clone(),
        report: ImportReport::default(),
        guards: Vec::new(),
        locked: HashSet::new(),
        made: Vec::new(),
        routes: Vec::new(),
    };
    for (task, expected) in tasks.iter().zip(sizes) {
        let outcome = if runner.ctx.cancel.is_cancelled() {
            task_outcome(task).skipped(OutcomeReason::Cancelled)
        } else {
            if let Task::Profile { profile, .. } = task {
                progress.current(Some(profile.name.clone()));
            }
            let before = progress.done();
            let outcome = runner.run_task(task, expected).await;
            progress.item_done(expected, progress.done().saturating_sub(before));
            outcome
        };
        // Each item holds the instances it writes into only while it runs: a long modpack copy
        // never keeps the Projects instance from starting.
        runner.guards.clear();
        runner.locked.clear();
        if outcome.reason == Some(OutcomeReason::Cancelled) {
            runner.report.cancelled = true;
        }
        runner.report.outcomes.push(outcome);
    }
    progress.stage(ImportStage::Finishing);
    runner.report.created = runner
        .made
        .iter()
        .map(|m| CreatedInstance {
            id: m.target.id.clone(),
            name: m.target.name.clone(),
        })
        .collect();
    runner.report.cancelled |= runner.ctx.cancel.is_cancelled();
    progress.emit(true);
    runner.report
}

/// The request in running order.
fn plan<'a>(sources: &'a [ImportSource], request: &ImportRequest) -> Vec<Task<'a>> {
    let picked: Vec<(&'a ImportSource, &crate::model::SourceRequest)> = request
        .sources
        .iter()
        .filter_map(|r| sources.iter().find(|s| s.id == r.source_id).map(|s| (s, r)))
        .collect();
    let mut tasks = Vec::new();
    let shared = |tasks: &mut Vec<Task<'a>>, kinds: &[ImportShared]| {
        for &(source, req) in &picked {
            if source.shared.is_none() {
                continue;
            }
            for &kind in kinds {
                if req.shared.contains(&kind) {
                    tasks.push(Task::Shared { source, kind });
                }
            }
        }
    };
    shared(&mut tasks, &[ImportShared::Options, ImportShared::Servers]);
    for &(source, req) in &picked {
        for id in &req.profiles {
            if let Some(profile) = source.profiles.iter().find(|p| &p.id == id) {
                tasks.push(Task::Profile { source, profile });
            }
        }
    }
    shared(
        &mut tasks,
        &[
            ImportShared::Worlds,
            ImportShared::ResourcePacks,
            ImportShared::ShaderPacks,
            ImportShared::Screenshots,
        ],
    );
    tasks
}

fn task_outcome(task: &Task<'_>) -> ItemOutcome {
    match task {
        Task::Shared { source, kind } => {
            ItemOutcome::new(&source.id, ItemRef::Shared { kind: *kind }, "")
        }
        Task::Profile { source, profile } => ItemOutcome::new(
            &source.id,
            ItemRef::Profile {
                id: profile.id.clone(),
            },
            &profile.name,
        ),
    }
}

/// What each task copies, for the progress bar and the free-space checks.
async fn task_sizes(tasks: &[Task<'_>]) -> Vec<u64> {
    let jobs: Vec<SizeJob> = tasks
        .iter()
        .map(|task| match task {
            Task::Shared { source, kind } => SizeJob::Shared(source.root.clone(), *kind),
            Task::Profile { profile, source } => {
                SizeJob::Profile(Box::new((*profile).clone()), source.root.clone())
            }
        })
        .collect();
    tokio::task::spawn_blocking(move || jobs.iter().map(SizeJob::size).collect())
        .await
        .unwrap_or_default()
}

enum SizeJob {
    Shared(PathBuf, ImportShared),
    Profile(Box<ImportProfile>, PathBuf),
}

impl SizeJob {
    fn size(&self) -> u64 {
        match self {
            Self::Shared(root, kind) => {
                let sizes = || shared_sizes(root);
                match kind {
                    ImportShared::Options | ImportShared::Servers => 0,
                    ImportShared::Worlds => sizes().worlds,
                    ImportShared::ResourcePacks => sizes().resource_packs,
                    ImportShared::ShaderPacks => sizes().shader_packs,
                    ImportShared::Screenshots => sizes().screenshots,
                }
            }
            Self::Profile(profile, root) => {
                if !profile.supported() {
                    return 0;
                }
                let mods = plan_mods(profile).bytes;
                if profile.own_folder {
                    mods + tree_size(&profile.game_dir, &instance_skip)
                } else if profile.loader != LoaderKind::Vanilla {
                    mods + tree_size(&root.join("config"), &no_skip)
                } else {
                    0
                }
            }
        }
    }
}

struct Runner {
    ctx: RunContext,
    progress: Arc<Progress>,
    report: ImportReport,
    guards: Vec<OwnedMutexGuard<()>>,
    locked: HashSet<String>,
    /// Instances this run made.
    made: Vec<Made>,
    /// Instances worlds can follow: the ones made now and the ones an earlier run made.
    routes: Vec<Made>,
}

impl Runner {
    async fn run_task(&mut self, task: &Task<'_>, expected: u64) -> ItemOutcome {
        match task {
            Task::Shared { source, kind } => match kind {
                ImportShared::Options => self.options(source).await,
                ImportShared::Servers => self.servers(source).await,
                ImportShared::Worlds => self.worlds(source).await,
                ImportShared::ResourcePacks
                | ImportShared::ShaderPacks
                | ImportShared::Screenshots => self.files(source, *kind, expected).await,
            },
            Task::Profile { source, profile } => self.profile(source, profile, expected).await,
        }
    }

    /// Holds an instance's lock until the item ends. False when the run was cancelled meanwhile.
    async fn lock(&mut self, id: &str) -> bool {
        if self.locked.contains(id) {
            return true;
        }
        let mutex = (self.ctx.locker)(id);
        let guard = tokio::select! {
            _ = self.ctx.cancel.cancelled() => return false,
            guard = mutex.lock_owned() => guard,
        };
        self.guards.push(guard);
        self.locked.insert(id.to_owned());
        true
    }

    fn target(&self, id: &str, name: &str) -> Result<Target> {
        Ok(Target {
            id: id.to_owned(),
            name: name.to_owned(),
            instance_dir: self.ctx.store.instance_dir(id)?,
            game_dir: self.ctx.store.game_dir(id)?,
        })
    }

    /// The Projects instance, locked. `Err` carries the outcome to report.
    async fn official(
        &mut self,
        out: &ItemOutcome,
    ) -> std::result::Result<Target, Box<ItemOutcome>> {
        let Some(official) = self.ctx.official.clone() else {
            return Err(Box::new(out.clone().failed(
                OutcomeReason::NoTarget,
                "the Projects instance is not available",
            )));
        };
        let target = self
            .target(&official.id, &official.name)
            .map_err(|e| Box::new(out.clone().failed(OutcomeReason::NoTarget, e.to_string())))?;
        if !self.lock(&official.id).await {
            return Err(Box::new(out.clone().skipped(OutcomeReason::Cancelled)));
        }
        Ok(target)
    }

    fn copier_parts(&self) -> (CancellationToken, Arc<Progress>) {
        (self.ctx.cancel.clone(), self.progress.clone())
    }

    async fn options(&mut self, source: &ImportSource) -> ItemOutcome {
        let out = ItemOutcome::new(
            &source.id,
            ItemRef::Shared {
                kind: ImportShared::Options,
            },
            "",
        );
        let target = match self.official(&out).await {
            Ok(target) => target,
            Err(outcome) => return *outcome,
        };
        let pack: Vec<KeyBinding> = match &self.ctx.official {
            Some(o) if o.official_pack && !official_pack_key_lines(&o.minecraft).is_empty() => {
                official_pack_bindings().to_vec()
            }
            _ => Vec::new(),
        };
        let from = source.root.join("options.txt");
        let job_target = target.clone();
        let result = blocking(move || {
            let mut journal = Journal::read(&job_target.instance_dir);
            if journal.copied_to(&from, &job_target.game_dir).is_some() {
                return Ok(Some(OutcomeReason::AlreadyThere));
            }
            let dest = job_target.game_dir.join("options.txt");
            if dest.exists() {
                return Ok(Some(OutcomeReason::KeptExisting));
            }
            let text = read_text(&from, MAX_OPTIONS)?;
            let merged = merge_options(&text, &pack);
            write_new(&dest, merged.text.as_bytes())?;
            journal.record("options.txt", &from);
            journal.write(&job_target.instance_dir)?;
            Ok(None)
        })
        .await;
        finish_single(out, result, &target)
    }

    async fn servers(&mut self, source: &ImportSource) -> ItemOutcome {
        let out = ItemOutcome::new(
            &source.id,
            ItemRef::Shared {
                kind: ImportShared::Servers,
            },
            "",
        );
        let target = match self.official(&out).await {
            Ok(target) => target,
            Err(outcome) => return *outcome,
        };
        let from = source.root.join("servers.dat");
        let job_target = target.clone();
        let result = blocking(move || {
            if !from.is_file() {
                return Ok(None);
            }
            merge_servers_into(Some(&from), &job_target, true).map(Some)
        })
        .await;
        let mut out = out;
        match result {
            Ok(None) => out.skipped(OutcomeReason::NotFound),
            Ok(Some((added, wrote))) => {
                if wrote {
                    self.mark_seeded(&target.id).await;
                }
                if added == 0 {
                    return out.skipped(OutcomeReason::AlreadyThere);
                }
                out.copied = added;
                out.add_destination(&target.id, &target.name, added);
                out
            }
            Err(error) => fail(out, &error),
        }
    }

    async fn profile(
        &mut self,
        source: &ImportSource,
        profile: &ImportProfile,
        expected: u64,
    ) -> ItemOutcome {
        let mut out = ItemOutcome::new(
            &source.id,
            ItemRef::Profile {
                id: profile.id.clone(),
            },
            &profile.name,
        );
        let Some(minecraft) = profile.minecraft.clone().filter(|_| profile.supported()) else {
            return out.skipped(OutcomeReason::Unsupported);
        };
        if let Some(existing) = profile.imported_as.as_deref()
            && let Ok(instance) = self.ctx.store.get(existing).await
        {
            // Its worlds still follow it.
            if let Ok(target) = self.target(&instance.id, &instance.name) {
                self.routes.push(Made {
                    source_id: source.id.clone(),
                    target,
                    minecraft: instance.mc_version.clone(),
                    loader: instance.loader.kind,
                });
            }
            out.add_destination(&instance.id, &instance.name, 1);
            return out.skipped(OutcomeReason::AlreadyThere);
        }
        if let Err(error) = self.check_space(expected) {
            return fail(out, &error);
        }
        let taken: Vec<String> = self
            .ctx
            .store
            .list()
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|i| i.name)
            .collect();
        let modded = profile.loader != LoaderKind::Vanilla;
        let created = self
            .ctx
            .store
            .create(NewInstance {
                name: free_instance_name(&profile.name, &taken),
                mc_version: minecraft.clone(),
                loader: InstanceLoader {
                    kind: profile.loader,
                    version: profile.loader_version.clone().filter(|_| modded),
                },
                icon: Some(InstanceIcon::Builtin {
                    key: icon_for(profile.loader).to_owned(),
                }),
                group: None,
                memory: None,
                managed_pack: None,
            })
            .await;
        let created = match created {
            Ok(created) => created,
            Err(error) => return fail(out, &Error::from(error)),
        };
        let target = match self.target(&created.id, &created.name) {
            Ok(target) => target,
            Err(error) => return fail(out, &error),
        };
        if !self.lock(&created.id).await {
            remove_instance(&target).await;
            return out.skipped(OutcomeReason::Cancelled);
        }
        let journal = Journal {
            from: Some(JournalFrom {
                source: path_key(&source.root),
                profile: profile.id.clone(),
            }),
            ..Journal::default()
        };
        let (cancel, progress) = self.copier_parts();
        let job_profile = profile.clone();
        let job_target = target.clone();
        let source_root = source.root.clone();
        let reserve = self.ctx.reserve_bytes;
        let copied = blocking(move || {
            journal.write(&job_target.instance_dir)?;
            let count = |n: u64| progress.add_bytes(n);
            let copier = Copier {
                cancel: &cancel,
                on_bytes: &count,
            };
            copy_profile(&copier, &job_profile, &source_root, &job_target, reserve)
        })
        .await;
        match copied {
            Ok(result) => {
                if result.servers_written {
                    self.mark_seeded(&target.id).await;
                }
                let made = Made {
                    source_id: source.id.clone(),
                    target: target.clone(),
                    minecraft,
                    loader: profile.loader,
                };
                self.routes.push(made.clone());
                self.made.push(made);
                out.add_destination(&target.id, &target.name, 1);
                out.copied = result.mods;
                out.left = result.left_mods + result.left_other;
                if result.left_mods > 0 {
                    out.reason = Some(OutcomeReason::IncompatibleMods);
                } else if result.left_other > 0 {
                    out.reason = Some(result.other_reason.unwrap_or(OutcomeReason::Failed));
                }
                if out.left > 0 {
                    out.status = OutcomeStatus::Partial;
                }
                out
            }
            Err(error) => {
                remove_instance(&target).await;
                fail(out, &error)
            }
        }
    }

    async fn worlds(&mut self, source: &ImportSource) -> ItemOutcome {
        let mut out = ItemOutcome::new(
            &source.id,
            ItemRef::Shared {
                kind: ImportShared::Worlds,
            },
            "",
        );
        let saves = source.root.join("saves");
        let worlds = blocking(move || Ok(list_worlds(&saves)))
            .await
            .unwrap_or_default();
        if worlds.is_empty() {
            return out.skipped(OutcomeReason::NotFound);
        }
        // Each world goes into the instance it was last played with, else the Projects one.
        let mut routed: Vec<(WorldDir, Option<Target>)> = Vec::new();
        let mut official: Option<Option<Target>> = None;
        for world in worlds {
            let home = self.home_of(&source.id, &world);
            let target = match home {
                Some(target) => Some(target),
                None => {
                    if official.is_none() {
                        official = Some(match self.official(&out).await {
                            Ok(target) => Some(target),
                            Err(outcome) if outcome.reason == Some(OutcomeReason::Cancelled) => {
                                return *outcome;
                            }
                            Err(_) => None,
                        });
                    }
                    official.clone().flatten()
                }
            };
            routed.push((world, target));
        }
        let mut already = 0;
        for (world, target) in routed {
            if self.ctx.cancel.is_cancelled() {
                return cancelled(out);
            }
            let Some(target) = target else {
                out.left += 1;
                out.reason.get_or_insert(OutcomeReason::NoTarget);
                continue;
            };
            if !self.lock(&target.id).await {
                return cancelled(out);
            }
            self.progress.current(Some(world.folder.clone()));
            let (cancel, progress) = self.copier_parts();
            let reserve = self.ctx.reserve_bytes;
            let job_target = target.clone();
            let result = blocking(move || {
                let count = |n: u64| progress.add_bytes(n);
                let copier = Copier {
                    cancel: &cancel,
                    on_bytes: &count,
                };
                copy_world(&copier, &world, &job_target, reserve, true)
            })
            .await;
            match result {
                Ok(WorldResult::Copied) => {
                    out.copied += 1;
                    out.add_destination(&target.id, &target.name, 1);
                }
                Ok(WorldResult::Already) => already += 1,
                Ok(WorldResult::Left(reason)) => {
                    out.left += 1;
                    out.reason.get_or_insert(reason);
                }
                Err(Error::Cancelled) => return cancelled(out),
                Err(error) => {
                    out.left += 1;
                    out.reason.get_or_insert(reason_of(&error));
                    out.detail.get_or_insert(error.to_string());
                }
            }
        }
        self.progress.current(None);
        settle(out, already)
    }

    /// The instance made in this run from the same source that plays the world's version with
    /// the world's loader.
    fn home_of(&self, source_id: &str, world: &WorldDir) -> Option<Target> {
        let version = world.version.as_deref()?;
        self.routes
            .iter()
            .filter(|m| m.source_id == source_id && m.minecraft == version)
            .find(|m| match world.loader {
                Some(LoaderKind::Fabric) => {
                    matches!(m.loader, LoaderKind::Fabric | LoaderKind::Quilt)
                }
                Some(loader) => m.loader == loader,
                None => m.loader == LoaderKind::Vanilla,
            })
            .map(|m| m.target.clone())
    }

    async fn files(
        &mut self,
        source: &ImportSource,
        kind: ImportShared,
        expected: u64,
    ) -> ItemOutcome {
        let out = ItemOutcome::new(&source.id, ItemRef::Shared { kind }, "");
        let target = match self.official(&out).await {
            Ok(target) => target,
            Err(outcome) => return *outcome,
        };
        if let Err(error) = self.check_space(expected) {
            return fail(out, &error);
        }
        let folder = match kind {
            ImportShared::ResourcePacks => "resourcepacks",
            ImportShared::ShaderPacks => "shaderpacks",
            _ => "screenshots",
        };
        let (cancel, progress) = self.copier_parts();
        let from = source.root.join(folder);
        let job_target = target.clone();
        let reserve = self.ctx.reserve_bytes;
        let result = blocking(move || {
            let count = |n: u64| progress.add_bytes(n);
            let copier = Copier {
                cancel: &cancel,
                on_bytes: &count,
            };
            copy_entries(&copier, kind, &from, &job_target, folder, reserve)
        })
        .await;
        let mut out = out;
        match result {
            Ok(counts) => {
                out.copied = counts.copied;
                out.left = counts.left;
                out.reason = counts.reason;
                out.detail = counts.detail;
                out.add_destination(&target.id, &target.name, counts.copied);
                if counts.cancelled {
                    return cancelled(out);
                }
                settle(out, counts.already)
            }
            Err(error) => fail(out, &error),
        }
    }

    fn check_space(&self, needed: u64) -> Result<()> {
        if needed == 0 {
            return Ok(());
        }
        let root = self.ctx.store.root();
        let probe = if root.exists() {
            root.to_path_buf()
        } else {
            root.parent().map(Path::to_path_buf).unwrap_or_default()
        };
        match fs4::available_space(&probe) {
            Ok(free) if free < needed.saturating_add(self.ctx.reserve_bytes) => {
                Err(Error::NoSpace { needed, free })
            }
            _ => Ok(()),
        }
    }

    /// The instance's server list now has the Projects server: never added again.
    async fn mark_seeded(&self, id: &str) {
        match self.ctx.store.get(id).await {
            Ok(mut instance) if !instance.default_server_seeded => {
                instance.default_server_seeded = true;
                if let Err(error) = self.ctx.store.update(&instance).await {
                    tracing::warn!(%error, instance = id, "could not record the default server");
                }
            }
            Ok(_) => {}
            Err(error) => tracing::warn!(%error, instance = id, "instance unreadable"),
        }
    }
}

/// The game folder entries copied one by one after the rest, whose failures leave the instance
/// usable (reported as left behind).
const LATER: [&str; 4] = ["saves", "resourcepacks", "shaderpacks", "screenshots"];

#[derive(Debug, Default)]
struct ProfileCopy {
    mods: u32,
    left_mods: u32,
    left_other: u32,
    other_reason: Option<OutcomeReason>,
    /// The server list was written (it has the Projects server now).
    servers_written: bool,
}

fn copy_profile(
    copier: &Copier<'_>,
    profile: &ImportProfile,
    source_root: &Path,
    target: &Target,
    reserve: u64,
) -> Result<ProfileCopy> {
    guard_overlap(&profile.game_dir, &target.game_dir)?;
    let mut result = ProfileCopy::default();
    let mods = plan_mods(profile);
    result.left_mods = mods.left;
    let mods_dir = target.game_dir.join("mods");
    for jar in &mods.take {
        let Some(name) = jar.file_name() else {
            continue;
        };
        let dest = mods_dir.join(name);
        if dest.exists() {
            continue;
        }
        copier.file(jar, &dest)?;
        result.mods += 1;
    }

    if profile.own_folder {
        // An instance never started has no game folder yet: it comes over as its version.
        if profile.game_dir.is_dir() {
            copy_own_folder(copier, profile, target, reserve, &mut result)?;
        }
    } else {
        if profile.loader != LoaderKind::Vanilla {
            let config = source_root.join("config");
            let dest = target.game_dir.join("config");
            if config.is_dir() && !dest.exists() {
                copier.folder(&config, &dest, &no_skip, &[])?;
            }
        }
        let options = source_root.join("options.txt");
        let dest = target.game_dir.join("options.txt");
        if options.is_file() && !dest.exists() {
            copier.file(&options, &dest)?;
        }
    }
    let servers = profile.game_dir.join("servers.dat");
    result.servers_written = match merge_servers_into(Some(&servers), target, false) {
        Ok((_, wrote)) => wrote,
        Err(error) => {
            tracing::warn!(%error, "the profile's server list was not imported");
            result.left_other += 1;
            result.other_reason.get_or_insert(OutcomeReason::Unreadable);
            // The instance still lists the Projects server.
            merge_servers_into(None, target, false).is_ok_and(|(_, wrote)| wrote)
        }
    };
    Ok(result)
}

/// A profile's own game folder: everything that makes the instance work first (a failure there
/// fails the profile), then worlds, packs and screenshots (failures there leave the instance
/// usable and are counted as left behind).
fn copy_own_folder(
    copier: &Copier<'_>,
    profile: &ImportProfile,
    target: &Target,
    reserve: u64,
    result: &mut ProfileCopy,
) -> Result<()> {
    let mut entries: Vec<std::fs::DirEntry> = std::fs::read_dir(&profile.game_dir)
        .map_err(|e| Error::io(&profile.game_dir, e))?
        .flatten()
        .collect();
    entries.sort_by_key(|e| e.file_name());
    let lock_only = |rel: &Path| rel.file_name().is_some_and(|n| n == "session.lock");
    for entry in &entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        if instance_skip(Path::new(&name)) || LATER.contains(&name.to_lowercase().as_str()) {
            continue;
        }
        let dest = target.game_dir.join(&name);
        if dest.exists() {
            continue;
        }
        let kind = entry.file_type().map_err(|e| Error::io(entry.path(), e))?;
        if kind.is_dir() {
            copier.folder(&entry.path(), &dest, &lock_only, &[])?;
        } else if kind.is_file() {
            copier.file(&entry.path(), &dest)?;
        }
    }
    let left = |result: &mut ProfileCopy, reason: OutcomeReason| {
        result.left_other += 1;
        result.other_reason.get_or_insert(reason);
    };
    for later in LATER {
        let from = profile.game_dir.join(later);
        if !from.is_dir() {
            continue;
        }
        if later == "saves" {
            for world in list_worlds(&from) {
                match copy_world(copier, &world, target, reserve, false) {
                    Ok(WorldResult::Copied | WorldResult::Already) => {}
                    Ok(WorldResult::Left(reason)) => left(result, reason),
                    Err(Error::Cancelled) => return Err(Error::Cancelled),
                    Err(error) => {
                        tracing::warn!(%error, world = %world.folder, "instance world not imported");
                        left(result, reason_of(&error));
                    }
                }
            }
            continue;
        }
        let dest = target.game_dir.join(later);
        if dest.exists() {
            continue;
        }
        match copier.folder(&from, &dest, &no_skip, &[]) {
            Ok(_) => {}
            Err(Error::Cancelled) => return Err(Error::Cancelled),
            Err(error) => {
                tracing::warn!(%error, folder = later, "instance folder not imported");
                left(result, reason_of(&error));
            }
        }
    }
    Ok(())
}

/// Merges the player's `servers.dat` at `from` into the target's (the Projects server first).
/// Returns how many of the player's servers were added and whether the file was written. A
/// missing source list (or `None`) still gives the target the Projects server; an unreadable
/// one (theirs or ours) is reported and nothing is written.
fn merge_servers_into(from: Option<&Path>, target: &Target, record: bool) -> Result<(u32, bool)> {
    let theirs = match from.map(std::fs::read) {
        Some(Ok(bytes)) => ServerList::from_nbt(&bytes)?,
        Some(Err(e)) if e.kind() != std::io::ErrorKind::NotFound => {
            return Err(Error::io(from.unwrap_or(Path::new("servers.dat")), e));
        }
        _ => ServerList::default(),
    };
    let dest = target.game_dir.join("servers.dat");
    let ours = match std::fs::read(&dest) {
        Ok(bytes) => ServerList::from_nbt(&bytes)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => ServerList::default(),
        Err(e) => return Err(Error::io(&dest, e)),
    };
    let merged = merge_server_lists(
        &ours,
        &theirs,
        Some((DEFAULT_SERVER_NAME, DEFAULT_SERVER_ADDRESS)),
    );
    if merged.added == 0 && !merged.added_projects {
        return Ok((0, false));
    }
    pl_mc::paths::write_durable_blocking(&dest, &merged.list.to_nbt()?)?;
    if record && let Some(from) = from {
        let mut journal = Journal::read(&target.instance_dir);
        journal.record("servers.dat", from);
        journal.write(&target.instance_dir)?;
    }
    Ok((merged.added as u32, true))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorldResult {
    Copied,
    Already,
    Left(OutcomeReason),
}

/// One world into `<target>/saves` under a free folder name. `journaled` records it (the
/// Projects instance's shared worlds; an instance's own worlds come with it).
fn copy_world(
    copier: &Copier<'_>,
    world: &WorldDir,
    target: &Target,
    reserve: u64,
    journaled: bool,
) -> Result<WorldResult> {
    guard_overlap(&world.path, &target.game_dir)?;
    let saves = target.game_dir.join("saves");
    let mut journal = journaled.then(|| Journal::read(&target.instance_dir));
    if let Some(journal) = journal.as_ref()
        && journal.copied_to(&world.path, &target.game_dir).is_some()
    {
        return Ok(WorldResult::Already);
    }
    // A copy from a run that ended before its record was written.
    if same_level_dat(&world.path, &saves.join(&world.folder)) {
        return Ok(WorldResult::Already);
    }
    if in_use(&world.path) {
        return Ok(WorldResult::Left(OutcomeReason::InUse));
    }
    let size = tree_size(&world.path, &no_skip);
    if let Ok(free) = fs4::available_space(target.game_dir.as_path())
        && free < size.saturating_add(reserve)
    {
        return Ok(WorldResult::Left(OutcomeReason::NoSpace));
    }
    std::fs::create_dir_all(&saves).map_err(|e| Error::io(&saves, e))?;
    remove_leftovers(&saves);
    let name = free_name(&saves, &world.folder, false);
    let lock_only = |rel: &Path| rel.file_name().is_some_and(|n| n == "session.lock");
    match copier.folder(
        &world.path,
        &saves.join(&name),
        &lock_only,
        &["level.dat", "level.dat_old"],
    ) {
        Ok(_) => {}
        Err(error) if error.is_disk_full() => return Ok(WorldResult::Left(OutcomeReason::NoSpace)),
        Err(error) => return Err(error),
    }
    if let Some(journal) = journal.as_mut() {
        journal.record(&format!("saves/{name}"), &world.path);
        journal.write(&target.instance_dir)?;
    }
    Ok(WorldResult::Copied)
}

fn same_level_dat(a: &Path, b: &Path) -> bool {
    match (
        std::fs::read(a.join("level.dat")),
        std::fs::read(b.join("level.dat")),
    ) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

#[derive(Debug, Default)]
struct EntryCounts {
    copied: u32,
    already: u32,
    left: u32,
    reason: Option<OutcomeReason>,
    detail: Option<String>,
    cancelled: bool,
}

/// Packs or screenshots, one entry at a time, into `<target>/<folder>`. An entry already there
/// (recorded, or the same name and size) is not copied again; another one with its name gets a
/// numbered name.
fn copy_entries(
    copier: &Copier<'_>,
    kind: ImportShared,
    from: &Path,
    target: &Target,
    folder: &str,
    reserve: u64,
) -> Result<EntryCounts> {
    guard_overlap(from, &target.game_dir)?;
    let mut counts = EntryCounts::default();
    let Ok(entries) = std::fs::read_dir(from) else {
        return Ok(counts);
    };
    let mut entries: Vec<std::fs::DirEntry> = entries
        .flatten()
        .filter(|e| match kind {
            ImportShared::ResourcePacks => is_pack_entry(e),
            ImportShared::Screenshots => is_screenshot_entry(e),
            // Shader packs come with their settings files (`<pack>.zip.txt`).
            _ => !e.file_name().to_string_lossy().starts_with('.'),
        })
        .collect();
    entries.sort_by_key(|e| e.file_name());
    let dest_dir = target.game_dir.join(folder);
    std::fs::create_dir_all(&dest_dir).map_err(|e| Error::io(&dest_dir, e))?;
    remove_leftovers(&dest_dir);
    let mut journal = Journal::read(&target.instance_dir);
    let mut changed = false;
    for entry in entries {
        if copier.cancel.is_cancelled() {
            counts.cancelled = true;
            break;
        }
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_dir = entry.file_type().is_ok_and(|t| t.is_dir());
        if journal.copied_to(&path, &target.game_dir).is_some()
            || same_file_there(&path, &dest_dir.join(&name))
        {
            counts.already += 1;
            continue;
        }
        let size = tree_size(&path, &no_skip);
        if let Ok(free) = fs4::available_space(&dest_dir)
            && free < size.saturating_add(reserve)
        {
            counts.left += 1;
            counts.reason.get_or_insert(OutcomeReason::NoSpace);
            continue;
        }
        let dest_name = free_name(&dest_dir, &name, !is_dir);
        let dest = dest_dir.join(&dest_name);
        let copied = if is_dir {
            copier.folder(&path, &dest, &no_skip, &[])
        } else {
            copier.file(&path, &dest)
        };
        match copied {
            Ok(_) => {
                counts.copied += 1;
                journal.record(&format!("{folder}/{dest_name}"), &path);
                changed = true;
            }
            Err(Error::Cancelled) => {
                counts.cancelled = true;
                break;
            }
            Err(error) => {
                counts.left += 1;
                counts.reason.get_or_insert(reason_of(&error));
                counts.detail.get_or_insert(error.to_string());
            }
        }
    }
    if changed {
        journal.write(&target.instance_dir)?;
    }
    Ok(counts)
}

/// A file of the same name and size is already there (a copy from an earlier run).
fn same_file_there(source: &Path, dest: &Path) -> bool {
    match (std::fs::metadata(source), std::fs::metadata(dest)) {
        (Ok(a), Ok(b)) if a.is_file() && b.is_file() => a.len() == b.len(),
        (Ok(a), Ok(b)) if a.is_dir() && b.is_dir() => {
            tree_size(source, &no_skip) == tree_size(dest, &no_skip)
        }
        _ => false,
    }
}

/// Never copy a folder into itself or one inside it (a data folder kept inside `.minecraft`).
fn guard_overlap(source: &Path, target: &Path) -> Result<()> {
    let (s, t) = (path_key(source), path_key(target));
    let inside = |a: &str, b: &str| a == b || a.starts_with(&format!("{b}/"));
    if inside(&t, &s) || inside(&s, &t) {
        return Err(Error::Invalid(format!(
            "{} and {} overlap",
            source.display(),
            target.display()
        )));
    }
    Ok(())
}

fn icon_for(loader: LoaderKind) -> &'static str {
    match loader {
        LoaderKind::Vanilla => "grass",
        LoaderKind::Fabric | LoaderKind::Quilt => "compass",
        LoaderKind::Forge | LoaderKind::NeoForge => "pickaxe",
    }
}

/// Removes an instance this run made (it holds nothing of the player's yet).
async fn remove_instance(target: &Target) {
    let dir = target.instance_dir.clone();
    let removed = tokio::task::spawn_blocking(move || std::fs::remove_dir_all(&dir)).await;
    if !matches!(removed, Ok(Ok(()))) {
        tracing::warn!(instance = %target.id, "unfinished import instance not removed");
    }
}

fn read_text(path: &Path, max: u64) -> Result<String> {
    let meta = std::fs::metadata(path).map_err(|e| Error::io(path, e))?;
    if meta.len() > max {
        return Err(Error::Invalid(format!("{} is too large", path.display())));
    }
    let bytes = std::fs::read(path).map_err(|e| Error::io(path, e))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Writes a file that must not exist yet (flushed: the player's settings).
fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    if path.exists() {
        return Err(Error::io(
            path,
            std::io::Error::new(std::io::ErrorKind::AlreadyExists, "already exists"),
        ));
    }
    pl_mc::paths::write_durable_blocking(path, bytes)?;
    Ok(())
}

async fn blocking<T, F>(work: F) -> Result<T>
where
    F: FnOnce() -> Result<T> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| Error::Invalid(format!("import task failed: {e}")))?
}

fn reason_of(error: &Error) -> OutcomeReason {
    match error {
        Error::Cancelled => OutcomeReason::Cancelled,
        error if error.is_disk_full() => OutcomeReason::NoSpace,
        Error::Io { source, .. } if source.kind() == std::io::ErrorKind::NotFound => {
            OutcomeReason::NotFound
        }
        Error::Mc(pl_mc::Error::Invalid(_)) => OutcomeReason::Unreadable,
        _ => OutcomeReason::Failed,
    }
}

fn fail(out: ItemOutcome, error: &Error) -> ItemOutcome {
    match error {
        Error::Cancelled => out.skipped(OutcomeReason::Cancelled),
        error => {
            tracing::warn!(%error, item = ?out.item, "import item failed");
            out.failed(reason_of(error), error.to_string())
        }
    }
}

fn cancelled(mut out: ItemOutcome) -> ItemOutcome {
    out.reason = Some(OutcomeReason::Cancelled);
    out.status = if out.copied > 0 {
        OutcomeStatus::Partial
    } else {
        OutcomeStatus::Skipped
    };
    out
}

/// Done, partial, skipped (everything was there already) or failed from the counts.
fn settle(out: ItemOutcome, already: u32) -> ItemOutcome {
    if out.copied == 0 && out.left == 0 {
        return out.skipped(if already > 0 {
            OutcomeReason::AlreadyThere
        } else {
            OutcomeReason::NotFound
        });
    }
    out.settle(OutcomeReason::Failed)
}

fn finish_single(
    out: ItemOutcome,
    result: Result<Option<OutcomeReason>>,
    target: &Target,
) -> ItemOutcome {
    match result {
        Ok(None) => {
            let mut out = out;
            out.copied = 1;
            out.add_destination(&target.id, &target.name, 1);
            out
        }
        Ok(Some(reason)) => out.skipped(reason),
        Err(error) => fail(out, &error),
    }
}

/// Progress shared with the copying threads; reported at most every [`PROGRESS_EVERY`] while
/// bytes flow, and at once when an item or the stage changes.
struct Progress {
    send: ProgressFn,
    state: Mutex<(ImportProgress, Instant)>,
}

impl Progress {
    fn new(send: ProgressFn, total_items: u32) -> Self {
        Self {
            send,
            state: Mutex::new((
                ImportProgress {
                    stage: ImportStage::Preparing,
                    done_bytes: 0,
                    total_bytes: 0,
                    done_items: 0,
                    total_items,
                    current: None,
                },
                Instant::now(),
            )),
        }
    }

    fn with(&self, change: impl FnOnce(&mut ImportProgress), force: bool) {
        let snapshot = {
            let Ok(mut state) = self.state.lock() else {
                return;
            };
            change(&mut state.0);
            if !force && state.1.elapsed() < PROGRESS_EVERY {
                return;
            }
            state.1 = Instant::now();
            state.0.clone()
        };
        (self.send)(snapshot);
    }

    fn stage(&self, stage: ImportStage) {
        self.with(|p| p.stage = stage, true);
    }

    fn set_total(&self, total: u64) {
        self.with(|p| p.total_bytes = total, true);
    }

    fn add_bytes(&self, n: u64) {
        self.with(|p| p.done_bytes = p.done_bytes.saturating_add(n), false);
    }

    fn current(&self, current: Option<String>) {
        self.with(|p| p.current = current, true);
    }

    fn done(&self) -> u64 {
        self.state.lock().map(|s| s.0.done_bytes).unwrap_or(0)
    }

    /// An item ended: bytes it did not copy (skipped, failed) count as done, so the bar moves on.
    fn item_done(&self, expected: u64, copied: u64) {
        self.with(
            |p| {
                p.done_bytes = p.done_bytes.saturating_add(expected.saturating_sub(copied));
                p.done_items += 1;
                p.current = None;
            },
            true,
        );
    }

    fn emit(&self, force: bool) {
        self.with(|_| {}, force);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlapping_folders_are_refused() {
        assert!(
            guard_overlap(
                Path::new("/a/.minecraft"),
                Path::new("/a/.minecraft/x/minecraft")
            )
            .is_err()
        );
        assert!(guard_overlap(Path::new("/a/data/instances/x"), Path::new("/a/data")).is_err());
        assert!(guard_overlap(Path::new("/a/.minecraft"), Path::new("/a/.minecraft2")).is_ok());
    }
}
