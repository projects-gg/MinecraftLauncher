//! Mod health of an instance's active version set: the pre-launch check, the issues the mod
//! center shows, crash attribution, and running repairs ("Onar") and set reinstalls ("Yeniden
//! yükle"). The decisions live in `pl_mc::mods::{check, repair, apply}`; this module builds
//! their inputs (the check environment, file origins), implements the launcher side of a
//! repair (`RepairHost`: the official pack sync, the instance's loader and Java) and maps the
//! results to views. Visible text is built by the UI from kinds and names; ids, file names and
//! requirements only go to "copy details".
//!
//! Official pack instances also report the pack's mod keys that still clash in an older
//! `options.txt` (KEYS2, `pl_mc::instance::keys`): one warning with "Düzelt" (`mods_fix_keys`,
//! undone by `mods_undo_keys`), and once per session a question before the launch, since the
//! game reads the file at start and writes its own keys back.

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use parking_lot::Mutex;
use pl_content::sets::folder_name;
use pl_content::{PackManifest, PackSelection, PackState, SyncProgress, apply_sync, plan_sync};
use pl_lang::pick;
use pl_mc::addons::{AddonIndex, Addons};
use pl_mc::gamefiles::LauncherTrash;
use pl_mc::instance::{Instance, KeyClash, MemorySettings};
use pl_mc::java::JavaSetting;
use pl_mc::launch::crash::{
    CrashDiagnosis, CrashKind, CrashSuspect, SuggestedAction, SuspectReason, attribute_crash,
};
use pl_mc::loaders::{LoaderKind, LoaderVersion};
use pl_mc::mods::apply::{
    PackRequest, PercentFn, ReinstallJob, ReinstallPhase, ReinstallProgress, RepairHost, RepairJob,
    RepairOutcome, RepairProgress, StepNote, StepState, StepStatus, check_environment,
    file_origins, fix_key, java_major_from_version_json, run_reinstall, run_repair,
};
use pl_mc::mods::check::{
    CheckEnvironment, DisableReason, FileOrigins, IssueKind, ModCheckReport, ModFix, ModIssue,
    Severity, check_mods,
};
use pl_mc::mods::repair::{
    ReinstallTarget, RepairAction, RepairContext, RepairPlan, dependency_project,
    display_name_for_file, display_name_for_id, loader_display_name, plan_reinstall, plan_repair,
};
use pl_mc::mods::scan::{ModScanner, ModSnapshot, strip_disabled};
use pl_net::CancellationToken;
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use crate::commands::launch::{CrashView, MemoryAdviceView, TurnOffView};
use crate::commands::modcenter::{PackSource, QUICK, pack_source, sync_options};
use crate::commands::modrinth::{
    addon_index_path, content_error, icon_dir, instance_lock, modrinth_client, start_job,
};
use crate::commands::versions::{ensure_switch_finished, refuse_while_running};
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

// --- session memory ----------------------------------------------------------------------------

/// What the launcher remembers about mod health while it runs.
#[derive(Default)]
pub struct ModHealth {
    /// The last crash of each instance, attributed to its mods ("Onar" in the crash dialog).
    crashes: Mutex<HashMap<String, CrashDiagnosis>>,
    /// Warnings already announced at a launch, per instance (so they do not nag every launch).
    warned: Mutex<HashMap<String, u64>>,
    /// Steps a repair already ran, per instance: their fixes are not offered first again.
    attempted: Mutex<HashMap<String, HashSet<String>>>,
    /// Java major of custom Java executables.
    java: Mutex<HashMap<PathBuf, u32>>,
    /// The key clashes asked about before a launch, per instance (the issue's key): asked once
    /// per session, whatever the answer.
    keys_asked: Mutex<HashMap<String, String>>,
}

/// The registry, created on first use.
fn health(app: &AppHandle) -> State<'_, ModHealth> {
    if app.try_state::<ModHealth>().is_none() {
        app.manage(ModHealth::default());
    }
    app.state::<ModHealth>()
}

// --- inputs ------------------------------------------------------------------------------------

/// The Java major a version needs, from its installed version JSON.
async fn required_java(state: &AppState, minecraft: &str) -> Option<u32> {
    let path = state.mc.dirs.version_json(minecraft).ok()?;
    let bytes = tokio::fs::read(path).await.ok()?;
    java_major_from_version_json(&bytes)
}

/// The Java major the game will run with, when known without starting it: automatic Java uses
/// the runtime the version asks for; a custom Java is asked once per session.
pub(crate) async fn java_major(
    app: &AppHandle,
    state: &AppState,
    instance: &Instance,
) -> Option<u32> {
    match &instance.java {
        JavaSetting::Auto => required_java(state, &instance.mc_version).await,
        JavaSetting::Custom { path } => {
            if let Some(major) = health(app).java.lock().get(path).copied() {
                return Some(major);
            }
            let major = pl_mc::java::probe_java(path).await.ok()?.major;
            health(app).java.lock().insert(path.clone(), major);
            Some(major)
        }
    }
}

pub(crate) fn environment(instance: &Instance, java: Option<u32>) -> CheckEnvironment {
    check_environment(
        &instance.mc_version,
        instance.loader.kind,
        instance.loader.version.as_deref(),
        java,
    )
}

/// The names players know the set's mod files by (file name without `.disabled`, lowercase →
/// title): the official pack's item titles and the Modrinth project titles the add-on index
/// recorded. Mod health texts use them rather than file names or the jars' own names
/// (`sodium-fabric-0.6.13+mc1.21.4` → "Sodium", "EntityCulling" → "Entity Culling").
#[derive(Debug, Clone, Default)]
struct Titles(HashMap<String, String>);

impl Titles {
    fn key(file: &str) -> String {
        strip_disabled(file).to_lowercase()
    }

    /// The catalog title of a file of the mods folder, if it has one.
    fn of(&self, file: &str) -> Option<&str> {
        self.0.get(&Self::key(file)).map(String::as_str)
    }

    /// The name of a mod an issue names: the catalog title of its file, unless the mod is bundled
    /// inside another jar (that title would be the other mod's).
    fn name(&self, m: &pl_mc::mods::check::ModRef) -> String {
        match self.of(&m.file) {
            Some(title) if !m.bundled => title.to_owned(),
            _ => m.name.clone(),
        }
    }
}

/// Where the set's files came from, and their catalog titles.
async fn sources_for(
    state: &AppState,
    instance: &Instance,
    instance_dir: &Path,
    game_dir: &Path,
    snapshot: &ModSnapshot,
) -> (FileOrigins, Titles) {
    let pack_state = PackState::load(game_dir).await.ok().flatten();
    let pack: Vec<String> = pack_state
        .as_ref()
        .map(|state| state.files.keys().cloned().collect())
        .unwrap_or_default();
    let index = AddonIndex::load(&addon_index_path(instance_dir)).await;
    let origins = file_origins(snapshot, &pack, &index);
    let mut titles = Titles::default();
    for jar in &snapshot.jars {
        if let Some(title) = index
            .source_for("mods", &jar.file_name, jar.size, jar.modified_ms)
            .and_then(|source| source.title.as_deref())
            .filter(|title| !title.trim().is_empty())
        {
            titles
                .0
                .insert(Titles::key(&jar.file_name), title.to_owned());
        }
    }
    // Pack files win, as they do for origins. The manifest comes from the content cache.
    if let Some(pack_state) = &pack_state
        && let PackSource::Ready { manifest, .. } = pack_source(state, instance, game_dir).await
    {
        for (path, file) in &pack_state.files {
            let Some(name) = path.strip_prefix("mods/") else {
                continue;
            };
            if let Some(item) = file
                .item
                .as_deref()
                .and_then(|id| manifest.projects.item(id))
            {
                titles.0.insert(Titles::key(name), item.title.clone());
            }
        }
    }
    (origins, titles)
}

async fn scan(mods_dir: PathBuf) -> ModSnapshot {
    tokio::task::spawn_blocking(move || ModScanner::shared().scan_dir(&mods_dir))
        .await
        .unwrap_or_default()
}

/// A check of the active set.
struct Checked {
    env: CheckEnvironment,
    snapshot: ModSnapshot,
    origins: FileOrigins,
    titles: Titles,
    report: ModCheckReport,
    /// Official pack instances: the pack's mod keys that still clash in `options.txt`.
    keys: Vec<KeyClash>,
}

async fn check_instance(
    state: &AppState,
    instance: &Instance,
    java: Option<u32>,
) -> CmdResult<Checked> {
    let game_dir = state
        .instances
        .game_dir(&instance.id)
        .map_err(CmdError::from)?;
    let instance_dir = state
        .instances
        .instance_dir(&instance.id)
        .map_err(CmdError::from)?;
    let env = environment(instance, java);
    let snapshot = scan(game_dir.join("mods")).await;
    let (origins, titles) = sources_for(state, instance, &instance_dir, &game_dir, &snapshot).await;
    let report = check_mods(&snapshot, &env, &origins);
    let keys = if instance.managed_pack.is_some() {
        pl_mc::instance::key_clashes_in(&game_dir).await
    } else {
        Vec::new()
    };
    Ok(Checked {
        env,
        snapshot,
        origins,
        titles,
        report,
        keys,
    })
}

/// The lowest Java the active set's mods accept (the highest `java` minimum among them), for the
/// Java check before a launch (J1). The check runs as if Java 8 were used, so every higher
/// minimum shows; `None` when no mod asks for more.
pub(crate) async fn mods_java_minimum(state: &AppState, instance: &Instance) -> Option<u32> {
    if instance.loader.kind == LoaderKind::Vanilla {
        return None;
    }
    let checked = check_instance(state, instance, Some(8)).await.ok()?;
    checked
        .report
        .issues
        .iter()
        .flat_map(|issue| issue.fixes.iter())
        .filter_map(|fix| match fix {
            ModFix::UseJava { major } => Some(*major),
            _ => None,
        })
        .max()
}

// --- views -------------------------------------------------------------------------------------

/// What the button of an issue says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum FixLabel {
    /// "Düzelt".
    Fix,
    /// "Bu modu kapat".
    Disable,
    /// "<ad> modunu kapat" (the issue names several mods).
    DisableNamed,
    /// "Eski kopyayı kapat".
    DisableOld,
    /// "Aç".
    Enable,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FixOfferView {
    pub label: FixLabel,
    /// The mod the button acts on, for `disableNamed`.
    pub name: Option<String>,
    /// Files it changes (older copies of a duplicate can be several).
    pub count: u32,
}

/// One problem of the check, for the UI.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModHealthIssueView {
    /// Stable key (kind, files, other side).
    pub key: String,
    pub severity: Severity,
    pub kind: IssueKind,
    /// Display names of the mods it is about.
    pub mods: Vec<String>,
    /// The other side: the dependency, the incompatible mod, the mod that bundles a copy.
    pub target: Option<String>,
    /// For a mod made for another loader: that loader's name.
    pub made_for: Option<String>,
    /// For a mod that needs a newer Java: the version it needs.
    pub java: Option<u32>,
    /// A copy is bundled inside another mod.
    pub bundled: bool,
    /// The files involved ("copy details" only).
    pub files: Vec<String>,
    pub offer: Option<FixOfferView>,
    /// For a key clash: the bindings "Düzelt" moves.
    pub keys: Vec<KeyChangeView>,
}

/// A binding a key fix moves: one clash of the official pack's mods in `options.txt`.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct KeyChangeView {
    /// The key mapping (`key.wi_zoom.zoom`); the UI names its action.
    pub name: String,
    /// The mod it belongs to ("WI Zoom").
    pub mod_name: String,
    /// What it does, in English (for a mapping the UI has no words for).
    pub action: String,
    /// The mapping it shares the key with (`key.voice_chat`).
    pub partner: String,
    /// The shared key (`key.keyboard.v`).
    pub key: String,
    /// The key it gets (`key.keyboard.c`); `None` leaves it without one.
    pub to: Option<String>,
}

fn key_change(clash: &KeyClash) -> KeyChangeView {
    KeyChangeView {
        name: clash.binding.name.clone(),
        mod_name: clash.binding.mod_name.clone(),
        action: clash.binding.action.clone(),
        partner: clash.partner.to_owned(),
        key: clash.key().to_owned(),
        to: (clash.to() != pl_mc::instance::keys::UNBOUND).then(|| clash.to().to_owned()),
    }
}

/// The one warning for the key clashes of an official pack instance (its button: "Düzelt").
fn key_clash_view(clashes: &[KeyClash]) -> ModHealthIssueView {
    let mut mods: Vec<String> = Vec::new();
    for clash in clashes {
        if !mods.contains(&clash.binding.mod_name) {
            mods.push(clash.binding.mod_name.clone());
        }
    }
    let names: Vec<&str> = clashes.iter().map(|c| c.binding.name.as_str()).collect();
    ModHealthIssueView {
        key: format!("{:?}|{}|", IssueKind::KeyClash, names.join(",")),
        severity: Severity::Warning,
        kind: IssueKind::KeyClash,
        mods,
        target: None,
        made_for: None,
        java: None,
        bundled: false,
        files: vec!["options.txt".to_owned()],
        offer: Some(FixOfferView {
            label: FixLabel::Fix,
            name: None,
            count: u32::try_from(clashes.len()).unwrap_or(u32::MAX),
        }),
        keys: clashes.iter().map(key_change).collect(),
    }
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModCheckView {
    pub instance_id: String,
    pub minecraft: String,
    pub loader: LoaderKind,
    /// Blocking and warning issues, worst first (information is left out).
    pub issues: Vec<ModHealthIssueView>,
    pub blocking: u32,
    pub checked_files: u32,
    /// "Onar" has something to do.
    pub repairable: bool,
    /// The set has mods ("Yeniden yükle" makes sense).
    pub has_mods: bool,
}

/// Joins the keys of the check issues one shown issue stands for.
const ISSUE_KEY_JOIN: char = '\u{1f}';

fn issue_key(issue: &ModIssue) -> String {
    let files: Vec<&str> = issue.mods.iter().map(|m| m.file.as_str()).collect();
    let target = issue.target.as_ref().map(|t| t.id.as_str()).unwrap_or("");
    format!("{:?}|{}|{target}", issue.kind, files.join(","))
}

fn loader_named(name: &str) -> Option<LoaderKind> {
    [
        LoaderKind::Fabric,
        LoaderKind::Quilt,
        LoaderKind::Forge,
        LoaderKind::NeoForge,
    ]
    .into_iter()
    .find(|kind| kind.modrinth_name() == Some(name))
}

/// The fixes a button runs: the first one not tried yet (every older copy of a duplicate).
fn offered_fixes(
    issue: &ModIssue,
    ctx: &RepairContext<'_>,
    tried: &HashSet<String>,
) -> Vec<ModFix> {
    let fresh: Vec<&ModFix> = issue
        .fixes
        .iter()
        .filter(|fix| !tried.contains(&fix_key(fix, ctx)))
        .collect();
    match fresh.first() {
        Some(ModFix::DisableFile { .. }) if issue.kind == IssueKind::Duplicate => fresh
            .iter()
            .filter(|fix| matches!(fix, ModFix::DisableFile { .. }))
            .map(|fix| (*fix).clone())
            .collect(),
        Some(fix) => vec![(*fix).clone()],
        None => Vec::new(),
    }
}

fn offer_view(issue: &ModIssue, fixes: &[ModFix], titles: &Titles) -> Option<FixOfferView> {
    let first = fixes.first()?;
    let count = u32::try_from(fixes.len()).unwrap_or(u32::MAX);
    let (label, name) = match first {
        ModFix::DisableFile {
            reason: DisableReason::Duplicate,
            ..
        } if issue.kind == IssueKind::Duplicate => (FixLabel::DisableOld, None),
        ModFix::DisableFile { file, .. } => {
            let files: HashSet<&str> = issue.mods.iter().map(|m| m.file.as_str()).collect();
            if files.len() <= 1 {
                (FixLabel::Disable, None)
            } else {
                let name = issue
                    .mods
                    .iter()
                    .find(|m| &m.file == file)
                    .map(|m| titles.name(m))
                    .or_else(|| {
                        issue
                            .target
                            .as_ref()
                            .filter(|t| t.file.as_deref() == Some(file))
                            .and_then(|t| titles.of(file).map(str::to_owned).or(t.name.clone()))
                    })
                    .unwrap_or_else(|| display_name_for_file(file));
                (FixLabel::DisableNamed, Some(name))
            }
        }
        ModFix::EnableFile { .. } => (FixLabel::Enable, None),
        _ => (FixLabel::Fix, None),
    };
    Some(FixOfferView { label, name, count })
}

fn issue_view(
    issue: &ModIssue,
    ctx: &RepairContext<'_>,
    tried: &HashSet<String>,
    titles: &Titles,
) -> ModHealthIssueView {
    let target = issue.target.as_ref();
    // The other side by its catalog title when it is a file of the set (a dependency in another
    // version, the incompatible mod, the jar that bundles a copy).
    let target_name = target.map(|t| {
        t.file
            .as_deref()
            .and_then(|file| titles.of(file))
            .map(str::to_owned)
            .or_else(|| t.name.clone())
            .unwrap_or_else(|| display_name_for_id(&t.id))
    });
    let made_for = (issue.kind == IssueKind::WrongLoader)
        .then(|| target.and_then(|t| loader_named(&t.id)))
        .flatten()
        .map(|loader| loader_display_name(loader).to_owned());
    let java = issue.fixes.iter().find_map(|fix| match fix {
        ModFix::UseJava { major } => Some(*major),
        _ => None,
    });
    let mut files: Vec<String> = issue.mods.iter().map(|m| m.file.clone()).collect();
    if let Some(file) = target.and_then(|t| t.file.clone())
        && !files.contains(&file)
    {
        files.push(file);
    }
    let mut mods: Vec<String> = Vec::new();
    for m in &issue.mods {
        let name = titles.name(m);
        if !mods.contains(&name) {
            mods.push(name);
        }
    }
    let fixes = offered_fixes(issue, ctx, tried);
    ModHealthIssueView {
        key: issue_key(issue),
        severity: issue.severity,
        kind: issue.kind,
        mods,
        target: match issue.kind {
            IssueKind::MissingDependency
            | IssueKind::DependencyVersion
            | IssueKind::Breaks
            | IssueKind::Conflicts
            | IssueKind::Duplicate => target_name,
            _ => None,
        }
        .filter(|name| !name.is_empty()),
        made_for,
        java,
        // The separate copy of a mod another jar bundles (the check names that jar's file).
        bundled: issue.kind == IssueKind::Duplicate
            && (issue.mods.iter().any(|m| m.bundled) || target.is_some_and(|t| t.file.is_some())),
        files,
        offer: offer_view(issue, &fixes, titles),
        keys: Vec::new(),
    }
}

/// Steps worth a button: anything beyond the plain pack check, not all tried already.
fn repairable(plan: &RepairPlan, tried: &HashSet<String>) -> bool {
    plan.steps.iter().any(|s| {
        !matches!(s.action, RepairAction::VerifyPackFiles)
            && !tried.contains(&pl_mc::mods::apply::step_key(&s.action))
    })
}

fn check_view(
    instance: &Instance,
    checked: &Checked,
    has_pack: bool,
    tried: &HashSet<String>,
) -> ModCheckView {
    let ctx = RepairContext {
        env: &checked.env,
        origins: &checked.origins,
        has_pack,
    };
    // One issue per cause: a dependency missing for several mods, asked for with different
    // requirements or by one of its parts' ids (Fabric API), is one line naming every mod. Its
    // key holds every check issue it stands for, so its button repairs all of them.
    let mut issues: Vec<ModHealthIssueView> = Vec::new();
    let mut projects: HashMap<(Severity, String), usize> = HashMap::new();
    for issue in checked
        .report
        .issues
        .iter()
        .filter(|i| i.severity != Severity::Info)
    {
        let view = issue_view(issue, &ctx, tried, &checked.titles);
        let project = issue
            .target
            .as_ref()
            .filter(|_| issue.kind == IssueKind::MissingDependency)
            .map(|target| (issue.severity, dependency_project(&target.id)));
        match project.as_ref().and_then(|key| projects.get(key)) {
            Some(&at) => {
                let merged = &mut issues[at];
                merged.key = format!("{}{ISSUE_KEY_JOIN}{}", merged.key, view.key);
                for name in view.mods {
                    if !merged.mods.contains(&name) {
                        merged.mods.push(name);
                    }
                }
                for file in view.files {
                    if !merged.files.contains(&file) {
                        merged.files.push(file);
                    }
                }
            }
            None => {
                if let Some(key) = project {
                    projects.insert(key, issues.len());
                }
                issues.push(view);
            }
        }
    }
    // The pack's mod keys that still clash in options.txt: a warning of its own, after the mods'.
    if !checked.keys.is_empty() {
        issues.push(key_clash_view(&checked.keys));
    }
    let plan = plan_repair(&ctx, Some(&checked.report), None);
    ModCheckView {
        instance_id: instance.id.clone(),
        minecraft: instance.mc_version.clone(),
        loader: instance.loader.kind,
        blocking: u32::try_from(
            issues
                .iter()
                .filter(|i| i.severity == Severity::Blocking)
                .count(),
        )
        .unwrap_or(u32::MAX),
        issues,
        checked_files: checked.report.checked_files,
        repairable: repairable(&plan, tried),
        has_mods: !checked.snapshot.jars.is_empty(),
    }
}

fn tried_for(app: &AppHandle, instance_id: &str) -> HashSet<String> {
    health(app)
        .attempted
        .lock()
        .get(instance_id)
        .cloned()
        .unwrap_or_default()
}

// --- the launcher side of a repair -------------------------------------------------------------

/// `RepairHost` for one instance.
struct LauncherHost<'a> {
    state: &'a AppState,
    instance_id: String,
    game_dir: PathBuf,
    minecraft: String,
    loader: LoaderKind,
    /// The official pack decides the loader version.
    managed: bool,
    manifest: Option<PackManifest>,
    cancel: CancellationToken,
}

impl<'a> LauncherHost<'a> {
    async fn new(
        state: &'a AppState,
        instance: &Instance,
        game_dir: &Path,
        cancel: CancellationToken,
    ) -> Self {
        let manifest = match pack_source(state, instance, game_dir).await {
            PackSource::Ready { manifest, .. } => Some(*manifest),
            _ => None,
        };
        Self {
            state,
            instance_id: instance.id.clone(),
            game_dir: game_dir.to_path_buf(),
            minecraft: instance.mc_version.clone(),
            loader: instance.loader.kind,
            managed: instance.managed_pack.is_some(),
            manifest,
            cancel,
        }
    }
}

fn pack_failure(error: pl_content::Error) -> pl_mc::Error {
    if error.is_cancelled() {
        pl_mc::Error::Cancelled
    } else if error.is_offline() {
        pl_mc::Error::Offline
    } else {
        pl_mc::Error::Invalid(error.to_string())
    }
}

impl RepairHost for LauncherHost<'_> {
    fn has_pack(&self) -> bool {
        self.manifest.is_some()
    }

    async fn pack_files(&self) -> Vec<String> {
        PackState::load(&self.game_dir)
            .await
            .ok()
            .flatten()
            .map(|state| state.files.into_keys().collect())
            .unwrap_or_default()
    }

    fn pack_item_for(&self, project_id: &str, slugs: &[String]) -> Option<String> {
        let manifest = self.manifest.as_ref()?;
        manifest
            .projects
            .items
            .iter()
            .find(|item| {
                item.modrinth
                    .as_ref()
                    .is_some_and(|m| !project_id.is_empty() && m.project_id == project_id)
                    || slugs.iter().any(|slug| slug.eq_ignore_ascii_case(&item.id))
            })
            .map(|item| item.id.clone())
    }

    async fn sync_pack(&self, request: PackRequest, progress: PercentFn) -> pl_mc::Result<()> {
        let manifest = self.manifest.as_ref().ok_or(pl_mc::Error::Offline)?;
        let previous = PackState::load(&self.game_dir)
            .await
            .map_err(pack_failure)?;
        let item_of = |path: &str| {
            previous
                .as_ref()
                .and_then(|state| state.files.get(path))
                .and_then(|file| file.item.clone())
        };
        let mut selection = PackSelection::default();
        if request.reinstall
            && let Some(state) = &previous
        {
            // Explicit choices: a sync after the folders were emptied installs the saved
            // selection instead of taking the missing files for the player's removals.
            selection.items = state.items.clone();
        }
        for path in &request.revert {
            match item_of(path) {
                Some(item) => {
                    selection.revert.insert(item.clone());
                    selection.items.insert(item, true);
                }
                None => {
                    // A pack file outside the catalog: the sync adds it again once it is gone.
                    let file = self.game_dir.join(path);
                    if let Err(error) = tokio::fs::remove_file(&file).await
                        && error.kind() != std::io::ErrorKind::NotFound
                    {
                        return Err(pl_mc::Error::io(&file, error));
                    }
                }
            }
        }
        for (paths, on) in [(&request.enable, true), (&request.disable, false)] {
            for path in paths {
                if let Some(item) = item_of(path) {
                    selection.items.insert(item, on);
                }
            }
        }
        for item in &request.enable_items {
            selection.items.insert(item.clone(), true);
        }
        let plan = plan_sync(
            manifest,
            &self.game_dir,
            previous.as_ref(),
            Some(&selection),
            &QUICK,
        )
        .await
        .map_err(pack_failure)?;
        let report: pl_content::SyncProgressFn = Arc::new(move |p: SyncProgress| {
            let fraction = if p.total_bytes > 0 {
                p.done_bytes as f64 / p.total_bytes as f64
            } else if p.total_files > 0 {
                p.done_files as f64 / p.total_files as f64
            } else {
                1.0
            };
            progress(fraction * 100.0);
        });
        apply_sync(
            &plan,
            &self.state.mc.downloader,
            &sync_options(&self.cancel),
            Some(report),
        )
        .await
        .map_err(pack_failure)?;
        tracing::info!(instance = %self.instance_id, summary = %plan.summary().describe_tr(), "pack synced for a mod repair");
        Ok(())
    }

    async fn loader_versions(&self) -> pl_mc::Result<Vec<LoaderVersion>> {
        pl_mc::loaders::list_loader_versions(&self.state.mc, self.loader, &self.minecraft).await
    }

    async fn set_loader_version(&self, version: &str) -> pl_mc::Result<bool> {
        if self.managed {
            return Ok(false);
        }
        let mut instance = self.state.instances.get(&self.instance_id).await?;
        instance.loader.version = Some(version.to_owned());
        self.state.instances.update(&instance).await?;
        tracing::info!(instance = %self.instance_id, %version, "loader version changed by a mod repair");
        Ok(true)
    }

    async fn use_java(&self, major: u32) -> pl_mc::Result<Option<u32>> {
        let mut instance = self.state.instances.get(&self.instance_id).await?;
        let required = required_java(self.state, &self.minecraft).await;
        if let JavaSetting::Custom { path } = &instance.java {
            let path = pl_mc::java::resolve_custom_java(path, self.state.mc.platform.os);
            let current = pl_mc::java::probe_java(&path)
                .await
                .ok()
                .map(|info| info.major);
            if current.is_some_and(|current| current >= major) {
                return Ok(current);
            }
        }
        // The game version's own runtime is enough (or unknown): automatic Java downloads it.
        if required.is_none_or(|required| required >= major) {
            if instance.java != JavaSetting::Auto {
                instance.java = JavaSetting::Auto;
                self.state.instances.update(&instance).await?;
                tracing::info!(instance = %self.instance_id, "switched to automatic Java for a mod");
            }
            return Ok(required);
        }
        // The mods need a newer Java than the game's runtime: a launcher runtime of that version
        // (kept current like the automatic ones).
        let Some((component, _)) = pl_mc::java::component_for_major(major) else {
            return Ok(required);
        };
        match crate::commands::java::use_managed_runtime(
            self.state,
            &mut instance,
            component,
            &self.cancel,
            None,
        )
        .await
        {
            Ok(installed) => Ok(Some(installed)),
            Err(error) if error.is_cancelled() => Err(error),
            Err(error) => {
                tracing::warn!(%error, %component, "could not install a newer Java for the mods");
                Ok(required)
            }
        }
    }
}

// --- the check ---------------------------------------------------------------------------------

/// The mod check of an instance's active set (the mod center's issues). Cheap: jars are read
/// once and cached by size and time.
#[tauri::command]
#[specta::specta]
pub async fn mods_check(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
) -> CmdResult<ModCheckView> {
    let mut instance = state
        .instances
        .get(&instance_id)
        .await
        .map_err(CmdError::from)?;
    ensure_switch_finished(&state, &mut instance).await?;
    let java = java_major(&app, &state, &instance).await;
    let checked = check_instance(&state, &instance, java).await?;
    let has_pack = instance.managed_pack.is_some();
    Ok(check_view(
        &instance,
        &checked,
        has_pack,
        &tried_for(&app, &instance_id),
    ))
}

/// The result of the check right before a launch.
pub(crate) enum Prelaunch {
    Clear,
    /// The game would certainly fail: the launch stops.
    Blocked(ModCheckView),
    /// Something will not work; shown once without stopping the launch.
    Warned(ModCheckView),
    /// The pack's mod keys clash in `options.txt`: the launch waits for the player's answer
    /// ("Düzelt" or "Yine de başlat"), asked once per session. A fix has to come before the game
    /// starts: the game reads the file then and writes its own keys back.
    Keys(ModCheckView),
}

/// Checks the mods right before a launch, with the Java that will run. Never fails the launch:
/// a problem with the check itself only skips it. `ignore_blocking` ("Yine de başlat" after
/// blocking problems, whose dialog listed the key clash too) also skips the key question.
pub(crate) async fn prelaunch(
    app: &AppHandle,
    state: &AppState,
    instance: &Instance,
    java: Option<u32>,
    ignore_blocking: bool,
) -> Prelaunch {
    if instance.loader.kind == LoaderKind::Vanilla {
        return Prelaunch::Clear;
    }
    let started = Instant::now();
    let Ok(checked) = check_instance(state, instance, java).await else {
        return Prelaunch::Clear;
    };
    tracing::info!(
        files = checked.report.checked_files,
        issues = checked.report.issues.len(),
        ms = started.elapsed().as_millis() as u64,
        "pre-launch mod check"
    );
    let tried = tried_for(app, &instance.id);
    let view = check_view(instance, &checked, instance.managed_pack.is_some(), &tried);
    if view.blocking > 0 && !ignore_blocking {
        return Prelaunch::Blocked(view);
    }
    if let Some(keys) = view.issues.iter().find(|i| i.kind == IssueKind::KeyClash) {
        let asked = health(app)
            .keys_asked
            .lock()
            .insert(instance.id.clone(), keys.key.clone());
        if asked.as_deref() != Some(keys.key.as_str()) && !ignore_blocking {
            return Prelaunch::Keys(view);
        }
    }
    // The key clash has its own question; the warning notice is for the mods.
    let warnings: Vec<&str> = view
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Warning && i.kind != IssueKind::KeyClash)
        .map(|i| i.key.as_str())
        .collect();
    if warnings.is_empty() {
        health(app).warned.lock().remove(&instance.id);
        return Prelaunch::Clear;
    }
    let mut hasher = DefaultHasher::new();
    warnings.hash(&mut hasher);
    let fingerprint = hasher.finish();
    let seen = health(app)
        .warned
        .lock()
        .insert(instance.id.clone(), fingerprint);
    if seen == Some(fingerprint) {
        Prelaunch::Clear
    } else {
        Prelaunch::Warned(view)
    }
}

// --- key clashes (KEYS2) -----------------------------------------------------------------------

/// What "Düzelt" did to the key clash.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct KeyFixView {
    /// The backup "Geri al" puts the keys back from (`mods_undo_keys`); `None` when nothing
    /// clashed any more.
    pub backup: Option<String>,
    /// The bindings it moved.
    pub changes: Vec<KeyChangeView>,
}

fn keys_running_error() -> CmdError {
    CmdError::new(
        "OYUN-ACIK-TUS",
        pick(
            "Oyun açıkken tuşlar değiştirilemez. Önce oyunu kapat.",
            "You can't change keys while the game is open. Close the game first.",
        ),
        "the game of this instance is running",
    )
}

/// Waits for the instance (a launch being prepared, a repair) and refuses while its game runs:
/// the game writes its own copy of every key back to `options.txt`.
async fn options_free(
    app: &AppHandle,
    state: &AppState,
    instance_id: &str,
) -> CmdResult<tokio::sync::OwnedMutexGuard<()>> {
    refuse_while_running(state, instance_id).map_err(|_| keys_running_error())?;
    let guard = instance_lock(app, instance_id).lock_owned().await;
    refuse_while_running(state, instance_id).map_err(|_| keys_running_error())?;
    Ok(guard)
}

/// "Düzelt" for the key clash of an official pack instance: backs up `options.txt` and moves only
/// the clashing bindings to the pack's table (`pl_mc::instance::fix_key_clashes_in`).
#[tauri::command]
#[specta::specta]
pub async fn mods_fix_keys(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
) -> CmdResult<KeyFixView> {
    let _guard = options_free(&app, &state, &instance_id).await?;
    let instance = state
        .instances
        .get(&instance_id)
        .await
        .map_err(CmdError::from)?;
    let nothing = KeyFixView {
        backup: None,
        changes: Vec::new(),
    };
    if instance.managed_pack.is_none() {
        return Ok(nothing);
    }
    let game_dir = state
        .instances
        .game_dir(&instance_id)
        .map_err(CmdError::from)?;
    Ok(
        match pl_mc::instance::fix_key_clashes_in(&game_dir)
            .await
            .map_err(CmdError::from)?
        {
            Some(fix) => KeyFixView {
                changes: fix.clashes.iter().map(key_change).collect(),
                backup: Some(fix.backup),
            },
            None => nothing,
        },
    )
}

/// "Geri al" after `mods_fix_keys`: the keys go back from that backup. `false` when the backup is
/// gone (nothing changed).
#[tauri::command]
#[specta::specta]
pub async fn mods_undo_keys(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    backup: String,
) -> CmdResult<bool> {
    let _guard = options_free(&app, &state, &instance_id).await?;
    let game_dir = state
        .instances
        .game_dir(&instance_id)
        .map_err(CmdError::from)?;
    pl_mc::instance::undo_key_fix_in(&game_dir, &backup)
        .await
        .map_err(CmdError::from)
}

// --- crashes -----------------------------------------------------------------------------------

/// A mod the crash points at, for the UI.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CrashSuspectView {
    pub name: String,
    pub reason: SuspectReason,
    /// The other mod involved, by name.
    pub related: Option<String>,
    /// Strong evidence (weak: only seen in the stack trace).
    pub strong: bool,
}

/// Reasons strong enough to offer "Bu modu kapat" for the mod.
fn turn_off_reason(reason: SuspectReason) -> bool {
    matches!(
        reason,
        SuspectReason::MixinFailed
            | SuspectReason::LoadingFailed
            | SuspectReason::NamedInReport
            | SuspectReason::Incompatible
            | SuspectReason::DeclaresIncompatible
            | SuspectReason::IncompatibleMinecraft
            | SuspectReason::CorruptFile
    )
}

fn mod_crash(kind: CrashKind) -> bool {
    matches!(
        kind,
        CrashKind::IncompatibleMods
            | CrashKind::MissingDependency
            | CrashKind::DuplicateMods
            | CrashKind::ModError
    )
}

/// A suspect by the catalog title of its jar (what "Bu modu kapat" acts on), else its own name.
fn suspect_name(s: &CrashSuspect, titles: &Titles) -> String {
    s.file
        .as_deref()
        .and_then(|file| titles.of(file))
        .map(str::to_owned)
        .or_else(|| s.name.clone())
        .or_else(|| s.mod_id.as_deref().map(display_name_for_id))
        .or_else(|| s.file.as_deref().map(display_name_for_file))
        .unwrap_or_default()
}

fn related_name(id: &str, snapshot: &ModSnapshot, titles: &Titles) -> String {
    snapshot
        .declaring(id)
        .next()
        .map(|(jar, m)| match titles.of(&jar.file_name) {
            Some(title) if m.nested_in.is_empty() => title.to_owned(),
            _ => m.display_name().to_owned(),
        })
        .unwrap_or_else(|| display_name_for_id(id))
}

fn memory_advice(diagnosis: &CrashDiagnosis, instance: &Instance) -> Option<MemoryAdviceView> {
    let current = instance.memory.max_mb;
    let limits = pl_mc::instance::memory_limits(pl_mc::instance::system_memory_mb());
    diagnosis
        .suggested_actions
        .iter()
        .find_map(|action| match action {
            SuggestedAction::IncreaseMaxMemory { suggested_mb } => {
                let wanted = suggested_mb.unwrap_or(current + current / 2);
                let wanted = wanted.min(limits.max_ceiling_mb);
                (wanted > current).then_some(MemoryAdviceView {
                    current_mb: current,
                    suggested_mb: wanted,
                    increase: true,
                })
            }
            SuggestedAction::LowerMaxMemory { suggested_mb } => {
                let wanted = suggested_mb.unwrap_or(current / 2).max(1024);
                (wanted < current).then_some(MemoryAdviceView {
                    current_mb: current,
                    suggested_mb: wanted,
                    increase: false,
                })
            }
            _ => None,
        })
}

/// The crash dialog's view: the diagnosis with the suspects found in the mods folder the game
/// used, and which one-click actions make sense. The attributed diagnosis is kept for "Onar".
pub(crate) async fn crash_view(
    app: &AppHandle,
    state: &AppState,
    instance: &Instance,
    java: Option<u32>,
    mut diagnosis: CrashDiagnosis,
) -> CrashView {
    let mut turn_off = None;
    let mut repair = false;
    let mut reinstall = false;
    let mut suspects = Vec::new();
    let modded = instance.loader.kind != LoaderKind::Vanilla;
    // Graphics drivers, memory, JVM options and security software: no mod actions, and no
    // suspects to find (the analyzer names none), so the mods folder is not read and a JVM
    // that did not start is reported at once.
    let not_mods = matches!(
        diagnosis.kind,
        CrashKind::GraphicsDriver
            | CrashKind::OutOfMemory
            | CrashKind::MemoryReserve
            | CrashKind::InvalidJvmArgument
            | CrashKind::AccessDenied
            | CrashKind::NoOutput
    );
    if modded && not_mods {
        health(app)
            .crashes
            .lock()
            .insert(instance.id.clone(), diagnosis.clone());
    } else if modded && let Ok(checked) = check_instance(state, instance, java).await {
        attribute_crash(&mut diagnosis, &checked.snapshot, &checked.env);
        suspects = diagnosis
            .suspects
            .iter()
            .map(|s| CrashSuspectView {
                name: suspect_name(s, &checked.titles),
                reason: s.reason,
                related: s
                    .related
                    .as_deref()
                    .map(|id| related_name(id, &checked.snapshot, &checked.titles)),
                strong: s.reason != SuspectReason::InStackTrace,
            })
            .filter(|s| !s.name.is_empty())
            .collect();
        let ctx = RepairContext {
            env: &checked.env,
            origins: &checked.origins,
            has_pack: instance.managed_pack.is_some(),
        };
        let plan = plan_repair(&ctx, Some(&checked.report), Some(&diagnosis));
        repair = repairable(&plan, &HashSet::new());
        turn_off = diagnosis
            .suspects
            .iter()
            .filter(|s| turn_off_reason(s.reason))
            .find_map(|s| {
                let file = s.file.as_deref()?;
                let jar = checked.snapshot.by_file(file).filter(|j| j.enabled)?;
                Some(TurnOffView {
                    file: jar.file_name.clone(),
                    name: suspect_name(s, &checked.titles),
                })
            });
        let has_jars = !checked.snapshot.jars.is_empty();
        reinstall = has_jars
            && (mod_crash(diagnosis.kind)
                || (matches!(
                    diagnosis.kind,
                    CrashKind::Unknown | CrashKind::CorruptedFiles
                ) && !diagnosis.suspects.is_empty()));
        health(app)
            .crashes
            .lock()
            .insert(instance.id.clone(), diagnosis.clone());
    }
    let memory = memory_advice(&diagnosis, instance);
    // "Argümanları sıfırla": only when the instance has arguments of the player's to reset.
    let reset_jvm_args = !instance.jvm_args.trim().is_empty()
        && diagnosis
            .suggested_actions
            .contains(&SuggestedAction::ResetJvmArguments);
    CrashView {
        title: diagnosis.title_tr.clone(),
        hint: diagnosis.hint_tr.clone(),
        actions: diagnosis
            .suggested_actions
            .iter()
            .map(|a| {
                serde_json::to_value(a)
                    .ok()
                    .and_then(|v| {
                        v.as_str()
                            .map(str::to_owned)
                            .or_else(|| Some(v.to_string()))
                    })
                    .unwrap_or_default()
            })
            .collect(),
        mods: diagnosis.mods.clone(),
        log_file: diagnosis
            .log_file
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned()),
        crash_report: diagnosis
            .crash_report
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned()),
        kind: diagnosis.kind,
        instance_id: instance.id.clone(),
        suspects,
        repairable: repair,
        turn_off,
        reinstall,
        memory,
        reset_jvm_args,
    }
}

// --- repairs -----------------------------------------------------------------------------------

/// What "Onar" repairs.
#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RepairScope {
    /// Every issue of the check.
    All,
    /// The check's issues and the last crash's findings.
    Crash,
    /// One issue (its offered fix).
    Issue { key: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum RepairStepKind {
    VerifyPack,
    RedownloadPack,
    RevertPack,
    Redownload,
    Enable,
    Disable,
    DisableOld,
    Loader,
    Java,
    Update,
    Fetch,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RepairStepView {
    pub key: String,
    pub kind: RepairStepKind,
    /// The mod (display name), the loader or "Java 21"; empty for the pack check.
    pub name: String,
    pub status: StepStatus,
    pub note: Option<StepNote>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RepairProgressView {
    pub steps: Vec<RepairStepView>,
    pub percent: Option<f64>,
    pub checking: bool,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RepairResultView {
    pub steps: Vec<RepairStepView>,
    /// The check after the repair.
    pub check: ModCheckView,
    pub cancelled: bool,
}

/// The file a repair step acts on, if any (a pack path's file name for pack steps).
fn step_file(action: &RepairAction) -> Option<&str> {
    match action {
        RepairAction::RedownloadPackFile { path } | RepairAction::RevertPackFile { path, .. } => {
            Some(path.rsplit(['/', '\\']).next().unwrap_or(path))
        }
        RepairAction::RedownloadModrinthFile { file, .. }
        | RepairAction::EnableFile { file, .. }
        | RepairAction::DisableFile { file, .. }
        | RepairAction::UpdateToCompatible { file, .. } => Some(file),
        _ => None,
    }
}

fn step_view(state: &StepState, titles: &Titles) -> RepairStepView {
    let kind = match &state.step.action {
        RepairAction::VerifyPackFiles => RepairStepKind::VerifyPack,
        RepairAction::RedownloadPackFile { .. } => RepairStepKind::RedownloadPack,
        RepairAction::RevertPackFile { .. } => RepairStepKind::RevertPack,
        RepairAction::RedownloadModrinthFile { .. } => RepairStepKind::Redownload,
        RepairAction::EnableFile { .. } => RepairStepKind::Enable,
        RepairAction::DisableFile {
            reason: DisableReason::Duplicate,
            ..
        } => RepairStepKind::DisableOld,
        RepairAction::DisableFile { .. } => RepairStepKind::Disable,
        RepairAction::UpdateLoader { .. } => RepairStepKind::Loader,
        RepairAction::UseJava { .. } => RepairStepKind::Java,
        RepairAction::UpdateToCompatible { .. } => RepairStepKind::Update,
        RepairAction::FetchDependency { .. } => RepairStepKind::Fetch,
    };
    // A mod by its catalog title ("Sodium yeniden indirildi"), never by its file name.
    let name = step_file(&state.step.action)
        .and_then(|file| titles.of(file))
        .map_or_else(|| state.step.name.clone(), str::to_owned);
    RepairStepView {
        key: state.key.clone(),
        kind,
        name,
        status: state.status,
        note: state.note,
    }
}

fn progress_view(p: &RepairProgress, titles: &Titles) -> RepairProgressView {
    RepairProgressView {
        steps: p.steps.iter().map(|s| step_view(s, titles)).collect(),
        percent: p.percent,
        checking: p.checking,
    }
}

fn game_running_error() -> CmdError {
    CmdError::new(
        "OYUN-ACIK-MOD",
        pick(
            "Oyun açıkken modlar onarılamaz. Önce oyunu kapat.",
            "You can't repair mods while the game is open. Close the game first.",
        ),
        "the game of this instance is running",
    )
}

/// "Onar": carries out the repair plan for `scope` with progress (cancel with `content_cancel`).
/// Refused while the instance's game runs.
#[tauri::command]
#[specta::specta]
pub async fn mods_repair(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    scope: RepairScope,
    job_id: String,
    on_progress: Channel<RepairProgressView>,
) -> CmdResult<RepairResultView> {
    refuse_while_running(&state, &instance_id).map_err(|_| game_running_error())?;
    let job = start_job(&app, &job_id);
    let lock = instance_lock(&app, &instance_id);
    let _guard = tokio::select! {
        _ = job.token.cancelled() => return Err(content_error(pl_mc::Error::Cancelled)),
        guard = lock.lock() => guard,
    };
    // A launch that held the lock may have started the game meanwhile.
    refuse_while_running(&state, &instance_id).map_err(|_| game_running_error())?;
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
    let instance_dir = state
        .instances
        .instance_dir(&instance_id)
        .map_err(CmdError::from)?;
    let java = java_major(&app, &state, &instance).await;
    let checked = check_instance(&state, &instance, java).await?;
    let host = LauncherHost::new(&state, &instance, &game_dir, job.token.clone()).await;
    let ctx = RepairContext {
        env: &checked.env,
        origins: &checked.origins,
        has_pack: host.has_pack(),
    };
    let tried = tried_for(&app, &instance_id);
    let crash = match &scope {
        RepairScope::Crash => health(&app).crashes.lock().get(&instance_id).cloned(),
        _ => None,
    };
    let plan = match &scope {
        RepairScope::All => plan_repair(&ctx, Some(&checked.report), None),
        RepairScope::Crash => plan_repair(&ctx, Some(&checked.report), crash.as_ref()),
        RepairScope::Issue { key } => {
            // A shown issue may stand for several check issues (one cause, see `check_view`).
            let keys: HashSet<&str> = key.split(ISSUE_KEY_JOIN).collect();
            let chosen: Vec<ModIssue> = checked
                .report
                .issues
                .iter()
                .filter(|issue| keys.contains(issue_key(issue).as_str()))
                .map(|issue| ModIssue {
                    severity: Severity::Blocking,
                    fixes: offered_fixes(issue, &ctx, &tried),
                    ..issue.clone()
                })
                .collect();
            let only = (!chosen.is_empty()).then_some(ModCheckReport {
                issues: chosen,
                checked_files: checked.report.checked_files,
            });
            let mut plan = plan_repair(&ctx, only.as_ref(), None);
            // One issue: no general pack check unless its fix needs the pack.
            if plan.steps.len() > 1 {
                plan.steps
                    .retain(|s| !matches!(s.action, RepairAction::VerifyPackFiles));
            }
            plan
        }
    };
    let progress_channel = on_progress.clone();
    let titles = Arc::new(checked.titles.clone());
    let progress_titles = Arc::clone(&titles);
    let repair = RepairJob {
        host: &host,
        mods_dir: game_dir.join("mods"),
        env: checked.env.clone(),
        addons: Addons::new(&game_dir, addon_index_path(&instance_dir))
            .with_icon_dir(icon_dir(&state)),
        client: modrinth_client(&state),
        downloader: state.mc.downloader.clone(),
        cancel: job.token.clone(),
        progress: Some(Arc::new(move |p: &RepairProgress| {
            let _ = progress_channel.send(progress_view(p, &progress_titles));
        })),
    };
    let outcome: RepairOutcome = run_repair(&repair, plan, crash.as_ref())
        .await
        .map_err(content_error)?;
    tracing::info!(
        instance = %instance_id,
        steps = outcome.steps.len(),
        left = outcome.report.issues.len(),
        cancelled = outcome.cancelled,
        "mod repair finished"
    );
    // The view of what is left, with the instance as the repair left it (loader, Java).
    let instance = state
        .instances
        .get(&instance_id)
        .await
        .map_err(CmdError::from)?;
    let java = java_major(&app, &state, &instance).await;
    let after = check_instance(&state, &instance, java).await?;
    // Steps that failed, or ran without answering their issue, are not offered first again; a
    // step that worked may be offered again when its issue comes back.
    {
        let ctx = RepairContext {
            env: &after.env,
            origins: &after.origins,
            has_pack: host.has_pack(),
        };
        let still: HashSet<String> = after
            .report
            .issues
            .iter()
            .flat_map(|issue| issue.fixes.iter().map(|fix| fix_key(fix, &ctx)))
            .collect();
        let registry = health(&app);
        let mut attempted = registry.attempted.lock();
        let entry = attempted.entry(instance_id.clone()).or_default();
        for step in &outcome.steps {
            match step.status {
                StepStatus::Failed => {
                    entry.insert(step.key.clone());
                }
                StepStatus::Done if still.contains(&step.key) => {
                    entry.insert(step.key.clone());
                }
                StepStatus::Done => {
                    entry.remove(&step.key);
                }
                _ => {}
            }
        }
    }
    Ok(RepairResultView {
        steps: outcome
            .steps
            .iter()
            .map(|s| step_view(s, &titles))
            .collect(),
        check: check_view(
            &instance,
            &after,
            host.has_pack(),
            &tried_for(&app, &instance_id),
        ),
        cancelled: outcome.cancelled,
    })
}

// --- reinstall ---------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ReinstallPreviewView {
    pub minecraft: String,
    /// `config/` has files: mod settings go back to their defaults.
    pub resets_settings: bool,
    /// Names of jars of unknown origin, kept aside.
    pub kept: Vec<String>,
    pub pack_mods: u32,
    pub modrinth_mods: u32,
    /// The official pack is needed but cannot be loaded now.
    pub needs_network: bool,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ReinstallProgressView {
    pub phases: Vec<ReinstallPhase>,
    pub phase: ReinstallPhase,
    pub percent: Option<f64>,
    pub cancellable: bool,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ReinstallResultView {
    /// Names of the jars kept aside.
    pub kept: Vec<String>,
    /// Their folder, relative to the game folder (for "Klasörü aç").
    pub kept_folder: Option<String>,
    pub pack_synced: bool,
    pub reinstalled: u32,
    /// Mods without a version for this game version: their old files came back.
    pub restored: Vec<String>,
    pub settings_reset: bool,
    pub check: ModCheckView,
    pub cancelled: bool,
}

struct ReinstallSetup {
    target: ReinstallTarget,
    snapshot: ModSnapshot,
    origins: FileOrigins,
    titles: Titles,
    managed: bool,
}

impl ReinstallSetup {
    /// The plan, with the player's Modrinth mods named by their project titles (the names a
    /// reinstall reports back, e.g. the mods whose old files came back).
    fn plan(&self) -> pl_mc::mods::repair::ReinstallPlan {
        let mut plan = plan_reinstall(&self.target, &self.snapshot, &self.origins);
        for item in &mut plan.reinstall_from_modrinth {
            if let Some(title) = self.titles.of(&item.file) {
                item.name = title.to_owned();
            }
        }
        plan
    }
}

async fn reinstall_setup(state: &AppState, instance: &Instance) -> CmdResult<ReinstallSetup> {
    let game_dir = state
        .instances
        .game_dir(&instance.id)
        .map_err(CmdError::from)?;
    let instance_dir = state
        .instances
        .instance_dir(&instance.id)
        .map_err(CmdError::from)?;
    let snapshot = scan(game_dir.join("mods")).await;
    let (origins, titles) = sources_for(state, instance, &instance_dir, &game_dir, &snapshot).await;
    let target = ReinstallTarget {
        minecraft: instance.mc_version.clone(),
        loader: instance.loader.kind,
        mods_dir: game_dir.join("mods"),
        config_dir: game_dir.join("config"),
        keep_dir: game_dir
            .join(pl_content::path::LAUNCHER_DIR)
            .join("kept-mods")
            .join(folder_name(&instance.mc_version)),
        has_pack: instance.managed_pack.is_some(),
    };
    Ok(ReinstallSetup {
        target,
        snapshot,
        origins,
        titles,
        managed: instance.managed_pack.is_some(),
    })
}

/// What "Yeniden yükle" would do (shown before asking).
#[tauri::command]
#[specta::specta]
pub async fn mods_reinstall_preview(
    state: State<'_, AppState>,
    instance_id: String,
) -> CmdResult<ReinstallPreviewView> {
    let mut instance = state
        .instances
        .get(&instance_id)
        .await
        .map_err(CmdError::from)?;
    ensure_switch_finished(&state, &mut instance).await?;
    let setup = reinstall_setup(&state, &instance).await?;
    let plan = setup.plan();
    let game_dir = state
        .instances
        .game_dir(&instance_id)
        .map_err(CmdError::from)?;
    let needs_network = setup.managed
        && !matches!(
            pack_source(&state, &instance, &game_dir).await,
            PackSource::Ready { .. }
        );
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    Ok(ReinstallPreviewView {
        minecraft: instance.mc_version.clone(),
        resets_settings: plan.resets_settings,
        kept: plan.keep_aside.iter().map(|k| k.name.clone()).collect(),
        pack_mods: count(plan.pack_files.len()),
        modrinth_mods: count(plan.reinstall_from_modrinth.len()),
        needs_network,
    })
}

/// "Yeniden yükle": the version's set starts again (old folders to the launcher trash, the
/// pack's selection and the player's Modrinth mods installed again, unknown jars kept aside).
#[tauri::command]
#[specta::specta]
pub async fn mods_reinstall(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    job_id: String,
    on_progress: Channel<ReinstallProgressView>,
) -> CmdResult<ReinstallResultView> {
    refuse_while_running(&state, &instance_id).map_err(|_| game_running_error())?;
    let job = start_job(&app, &job_id);
    let lock = instance_lock(&app, &instance_id);
    let _guard = tokio::select! {
        _ = job.token.cancelled() => return Err(content_error(pl_mc::Error::Cancelled)),
        guard = lock.lock() => guard,
    };
    // A launch that held the lock may have started the game meanwhile.
    refuse_while_running(&state, &instance_id).map_err(|_| game_running_error())?;
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
    let instance_dir = state
        .instances
        .instance_dir(&instance_id)
        .map_err(CmdError::from)?;
    let host = LauncherHost::new(&state, &instance, &game_dir, job.token.clone()).await;
    let setup = reinstall_setup(&state, &instance).await?;
    if setup.managed && !host.has_pack() {
        return Err(CmdError::new(
            "PAKET-YOK",
            pick(
                "Modları yeniden yüklemek için internet bağlantısı gerekiyor. Bağlantını kontrol edip tekrar dene.",
                "Reinstalling mods needs an internet connection. Check your connection and try again.",
            ),
            "the pack manifest is unavailable",
        ));
    }
    let plan = setup.plan();
    let kept_folder = setup
        .target
        .keep_dir
        .strip_prefix(&game_dir)
        .ok()
        .map(|p| p.to_string_lossy().replace('\\', "/"));
    let java = java_major(&app, &state, &instance).await;
    let channel = on_progress.clone();
    let reinstall = ReinstallJob {
        host: &host,
        plan,
        trash: LauncherTrash::new(state.root.join("trash")),
        instance_id: instance_id.clone(),
        env: environment(&instance, java),
        addons: Addons::new(&game_dir, addon_index_path(&instance_dir))
            .with_icon_dir(icon_dir(&state)),
        client: modrinth_client(&state),
        downloader: state.mc.downloader.clone(),
        cancel: job.token.clone(),
        progress: Some(Arc::new(move |p: &ReinstallProgress| {
            let _ = channel.send(ReinstallProgressView {
                phases: p.phases.clone(),
                phase: p.phase,
                percent: p.percent,
                cancellable: p.cancellable,
            });
        })),
    };
    let outcome = run_reinstall(&reinstall).await.map_err(|error| {
        CmdError::new(
            "YENIDEN-YUKLE",
            pick("Modlar taşınamadı. Mod klasörünü açık tutan bir program (dosya gezgini ya da antivirüs gibi) olabilir; kapatıp tekrar dene.", "Couldn't move the mods. A program may have the mods folder open (like your file manager or antivirus); close it and try again."),
            error.to_string(),
        )
    })?;
    health(&app).attempted.lock().remove(&instance_id);
    tracing::info!(
        instance = %instance_id,
        kept = outcome.kept.len(),
        reinstalled = outcome.reinstalled.len(),
        restored = outcome.restored.len(),
        pack = outcome.pack_synced,
        "mod set reinstalled"
    );
    let checked = check_instance(&state, &instance, java).await?;
    Ok(ReinstallResultView {
        kept: outcome.kept.iter().map(|k| k.name.clone()).collect(),
        kept_folder: (!outcome.kept.is_empty()).then_some(kept_folder).flatten(),
        pack_synced: outcome.pack_synced,
        reinstalled: u32::try_from(outcome.reinstalled.len()).unwrap_or(u32::MAX),
        restored: outcome.restored,
        settings_reset: outcome.settings_reset,
        check: check_view(&instance, &checked, host.has_pack(), &HashSet::new()),
        cancelled: outcome.cancelled,
    })
}

// --- memory ------------------------------------------------------------------------------------

/// Sets the instance's maximum memory (kept within what the computer allows; the minimum
/// follows when it would be higher). Returns the maximum that was saved.
#[tauri::command]
#[specta::specta]
pub async fn instance_set_max_memory(
    state: State<'_, AppState>,
    instance_id: String,
    max_mb: u32,
) -> CmdResult<u32> {
    let mut instance = state
        .instances
        .get(&instance_id)
        .await
        .map_err(CmdError::from)?;
    let total = pl_mc::instance::system_memory_mb();
    let memory = pl_mc::instance::clamp_memory(
        &MemorySettings {
            min_mb: instance.memory.min_mb.min(max_mb),
            max_mb,
        },
        total,
    );
    instance.memory = memory;
    // The player chose a value: "Otomatik" ends for this instance.
    instance.memory_auto = false;
    state
        .instances
        .update(&instance)
        .await
        .map_err(CmdError::from)?;
    tracing::info!(instance = %instance_id, max_mb = memory.max_mb, "maximum memory changed from the crash dialog");
    Ok(memory.max_mb)
}

/// Sets the instance's own JVM arguments ("Argümanları sıfırla" in the crash dialog sends an
/// empty text, its "Geri al" the previous one). Returns the arguments it replaced.
#[tauri::command]
#[specta::specta]
pub async fn instance_set_jvm_args(
    state: State<'_, AppState>,
    instance_id: String,
    jvm_args: String,
) -> CmdResult<String> {
    let mut instance = state
        .instances
        .get(&instance_id)
        .await
        .map_err(CmdError::from)?;
    let previous = std::mem::replace(&mut instance.jvm_args, jvm_args.trim().to_owned());
    state
        .instances
        .update(&instance)
        .await
        .map_err(CmdError::from)?;
    tracing::info!(instance = %instance_id, reset = instance.jvm_args.is_empty(), "JVM arguments changed from the crash dialog");
    Ok(previous)
}
