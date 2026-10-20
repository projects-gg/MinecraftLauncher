//! Java (J1): the check before launching with a custom Java, Settings → Java (the launcher's own
//! runtimes and the Javas on the system, and which instances use which), and keeping the
//! launcher's runtimes current in the background. Decisions live in
//! `pl_mc::java::{prelaunch, requirement, managed}`; this module gathers inputs, keeps the
//! session's memory and maps views.
//!
//! Background updates: shortly after start and then every ten minutes the updater looks at the
//! schedule (`meta/java-updates.json`: Mojang is asked at most once a day, an hour after a
//! failure) and at the runtimes the player asked to update. It works only while the launcher is
//! quiet (no game running or being prepared, no data job, no content download), stages the new
//! build next to the old one and swaps it in while no game runs. A launch cancels a download in
//! progress (it resumes later); a game that closes wakes the updater.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use pl_lang::pick;
use pl_mc::install::{InstallProgress, InstallProgressFn, InstallStage};
use pl_mc::instance::Instance;
use pl_mc::java::managed::{self, ManagedRuntime, SwapOutcome, UpdateSchedule};
use pl_mc::java::prelaunch::{
    CustomJava, JavaRemedy, JavaVerdict, ProbeCache, PromptMemory, check_custom_java, judge, remedy,
};
use pl_mc::java::{
    EnsureJavaOptions, JavaInfo, JavaInstallation, JavaNeed, JavaSetting, RuntimeEntry,
    resolve_custom_java,
};
use pl_mc::platform::Arch;
use pl_mc::version::VersionManifest;
use pl_net::{CancellationToken, DownloadProgress, ProgressFn, VerifyMode};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

/// The first background round, after the launcher settled.
const FIRST_ROUND: Duration = Duration::from_secs(120);
/// Rounds after that (local checks only; Mojang is asked at most daily).
const ROUND_EVERY: Duration = Duration::from_secs(600);
/// Javas found on the system are kept this long (finding them may start programs).
const SYSTEM_TTL: Duration = Duration::from_secs(600);
const SYSTEM_TIMEOUT: Duration = Duration::from_secs(15);

/// (component, platform) of a launcher runtime.
type RuntimeKey = (String, String);

// --- session state -----------------------------------------------------------------------------

/// What the launcher keeps about Java while it runs.
#[derive(Default)]
pub struct JavaService {
    /// Probes of custom Java executables (by file size and time).
    probes: Mutex<ProbeCache>,
    /// "Yine de dene" answers of this session.
    prompts: Mutex<PromptMemory>,
    /// Javas found on the system and when.
    system: tokio::sync::Mutex<Option<(Instant, Vec<JavaInstallation>)>>,
    /// The runtime update downloading now.
    job: Mutex<Option<UpdateJob>>,
    /// Runtimes the player asked to update ("Güncelle").
    requested: Mutex<HashSet<RuntimeKey>>,
    /// Runtimes whose last update failed (until the player asks again).
    failed: Mutex<HashSet<RuntimeKey>>,
    /// Wakes the updater (a request, a game that closed).
    wake: tokio::sync::Notify,
}

struct UpdateJob {
    key: RuntimeKey,
    cancel: CancellationToken,
    percent: f64,
}

/// The service, created on first use.
fn service(app: &AppHandle) -> State<'_, JavaService> {
    if app.try_state::<JavaService>().is_none() {
        app.manage(JavaService::default());
    }
    app.state::<JavaService>()
}

/// A runtime update is downloading (data moves and cleanups wait for it).
pub(crate) fn job_running(app: &AppHandle) -> bool {
    service(app).job.lock().is_some()
}

fn managed_id(component: &str, platform: &str) -> String {
    format!("managed:{component}/{platform}")
}

fn path_id(path: &Path) -> String {
    format!("path:{}", path.display())
}

fn now_ms() -> i64 {
    jiff::Timestamp::now().as_millisecond()
}

fn changed(app: &AppHandle) {
    let _ = app.emit("java-changed", ());
}

// --- views -------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum JavaRuntimeKind {
    /// Downloaded and kept current by the launcher.
    Managed,
    /// Installed on the computer.
    System,
    /// A path an instance points at that is not among the above.
    Custom,
}

/// Why a Java does not suit an instance's game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum JavaFit {
    TooOld,
    TooNew,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct JavaUserView {
    pub instance_id: String,
    pub name: String,
    /// Uses it through "Otomatik".
    pub automatic: bool,
    /// The Java does not suit the instance's game (custom paths only).
    pub fit: Option<JavaFit>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum JavaProblem {
    /// The file is gone.
    Missing,
    /// The file does not run as Java.
    Broken,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct JavaRuntimeView {
    /// `managed:<component>/<platform>` or `path:<executable>`.
    pub id: String,
    pub kind: JavaRuntimeKind,
    pub major: Option<u32>,
    /// Full version (`21.0.7`, `1.8.0_51`).
    pub version: Option<String>,
    /// A familiar vendor name (`Eclipse Temurin`, `Microsoft`, `Mojang`).
    pub vendor: Option<String>,
    /// The Java folder (launcher runtimes: their folder in the data folder).
    pub location: String,
    pub executable: String,
    /// A 32-bit Java on a 64-bit computer (the game gets little memory with it).
    pub bits32: bool,
    pub users: Vec<JavaUserView>,
    /// Launcher runtimes: Mojang's component and platform key.
    pub component: Option<String>,
    pub platform: Option<String>,
    /// Launcher runtimes: the newer build Mojang offers.
    pub update: Option<String>,
    /// Launcher runtimes: the update is downloaded and waits for the game to close.
    pub staged: bool,
    /// Launcher runtimes: the player asked for the update; it starts once no game runs.
    pub queued: bool,
    /// Launcher runtimes: the update downloading now (percent).
    pub updating: Option<f64>,
    /// Launcher runtimes: the last update failed.
    pub update_failed: bool,
    /// Custom paths: gone or not working.
    pub problem: Option<JavaProblem>,
    /// Launcher runtimes no instance uses: "Kaldır" frees this much.
    pub removable_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct JavaOverviewView {
    pub runtimes: Vec<JavaRuntimeView>,
    /// Looking for Javas on the computer did not finish in time.
    pub system_incomplete: bool,
    /// When Mojang was last asked for runtime updates (ms), if ever.
    pub checked_at: Option<i64>,
    /// A game runs or is being prepared: updates wait and nothing is removed.
    pub game_active: bool,
}

/// `java-progress`: a launcher runtime update is downloading.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct JavaProgressEvent {
    pub id: String,
    pub percent: f64,
}

// --- who uses which Java -----------------------------------------------------------------------

/// The Java an instance uses.
enum InstanceJava {
    /// "Otomatik": the launcher runtime of its game's component.
    Automatic(Option<String>),
    /// A custom path, resolved to the program.
    Custom(PathBuf),
}

struct InstanceInfo {
    id: String,
    name: String,
    java: InstanceJava,
    need: Option<JavaNeed>,
}

async fn instance_infos(state: &AppState, instances: Vec<Instance>) -> Vec<InstanceInfo> {
    let dirs = state.mc.dirs.clone();
    let os = state.mc.platform.os;
    tokio::task::spawn_blocking(move || {
        instances
            .into_iter()
            .map(|instance| {
                let need =
                    pl_mc::java::game_need(&dirs, &instance.mc_version, instance.loader.kind);
                let java = match &instance.java {
                    JavaSetting::Auto => {
                        InstanceJava::Automatic(need.as_ref().map(|n| n.component.clone()))
                    }
                    JavaSetting::Custom { path } => {
                        InstanceJava::Custom(resolve_custom_java(path, os))
                    }
                };
                InstanceInfo {
                    id: instance.id,
                    name: instance.name,
                    java,
                    need,
                }
            })
            .collect()
    })
    .await
    .unwrap_or_default()
}

/// The installed launcher runtime "Otomatik" uses for a component (native platform first).
fn automatic_runtime<'a>(
    managed: &'a [ManagedRuntime],
    component: &str,
    platforms: &[&str],
) -> Option<&'a ManagedRuntime> {
    platforms.iter().find_map(|platform| {
        managed
            .iter()
            .find(|r| r.component == component && r.platform == *platform)
    })
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Two executables of the same Java installation (`java.exe` and `javaw.exe` count as one).
fn same_installation(a: &Path, b: &Path) -> bool {
    match (
        pl_mc::java::release::java_home(a),
        pl_mc::java::release::java_home(b),
    ) {
        (Some(a), Some(b)) => canonical(&a) == canonical(&b),
        _ => canonical(a) == canonical(b),
    }
}

/// Launcher runtimes instances use now (for the updater and "Kaldır").
fn used_runtimes(
    state: &AppState,
    managed: &[ManagedRuntime],
    infos: &[InstanceInfo],
) -> HashSet<RuntimeKey> {
    let platforms = state.mc.platform.java_runtime_platforms();
    let mut used = HashSet::new();
    for info in infos {
        match &info.java {
            InstanceJava::Automatic(Some(component)) => {
                if let Some(runtime) = automatic_runtime(managed, component, &platforms) {
                    used.insert((runtime.component.clone(), runtime.platform.clone()));
                }
            }
            InstanceJava::Automatic(None) => {}
            InstanceJava::Custom(path) => {
                if let Some(key) = managed::managed_runtime_of(&state.mc.dirs, path) {
                    used.insert(key);
                }
            }
        }
    }
    used
}

fn fit_of(major: Option<u32>, need: Option<&JavaNeed>) -> Option<JavaFit> {
    let (major, need) = (major?, need?);
    let info = JavaInfo {
        version: major.to_string(),
        major,
        arch: Arch::Other,
        vendor: None,
    };
    match judge(&CustomJava::Found(info), need) {
        JavaVerdict::TooOld { .. } => Some(JavaFit::TooOld),
        JavaVerdict::TooNew { .. } => Some(JavaFit::TooNew),
        _ => None,
    }
}

// --- the overview ------------------------------------------------------------------------------

/// Javas on the system, kept for [`SYSTEM_TTL`]; `false` when the search did not finish in time.
async fn system_javas(app: &AppHandle, rescan: bool) -> (Vec<JavaInstallation>, bool) {
    let svc = service(app);
    let mut cached = svc.system.lock().await;
    if !rescan
        && let Some((at, list)) = cached.as_ref()
        && at.elapsed() < SYSTEM_TTL
    {
        return (list.clone(), true);
    }
    match tokio::time::timeout(SYSTEM_TIMEOUT, pl_mc::java::detect_system_javas_quick()).await {
        Ok(list) => {
            *cached = Some((Instant::now(), list.clone()));
            (list, true)
        }
        Err(_) => {
            tracing::warn!("looking for Javas on the system took too long");
            (
                cached
                    .as_ref()
                    .map(|(_, list)| list.clone())
                    .unwrap_or_default(),
                false,
            )
        }
    }
}

fn game_active(state: &AppState) -> bool {
    state.running.lock().is_some() || state.launch_cancel.lock().is_some()
}

fn bits32(arch: Arch) -> bool {
    matches!(arch, Arch::X86 | Arch::Arm) && matches!(Arch::current(), Arch::X86_64 | Arch::Aarch64)
}

/// Settings → Java: the launcher's runtimes, then the Javas on the computer, then custom paths
/// that are neither, each with the instances that use it. `rescan` looks for system Javas again.
#[tauri::command]
#[specta::specta]
pub async fn java_overview(
    app: AppHandle,
    state: State<'_, AppState>,
    rescan: bool,
) -> CmdResult<JavaOverviewView> {
    let instances = state.instances.list().await.map_err(CmdError::from)?;
    let infos = instance_infos(&state, instances).await;
    let dirs = state.mc.dirs.clone();
    let os = state.mc.platform.os;
    let managed = {
        let dirs = dirs.clone();
        tokio::task::spawn_blocking(move || managed::list_managed(&dirs, os))
            .await
            .unwrap_or_default()
    };
    let index = pl_mc::java::runtime_index(&state.mc, true).await.ok();
    let (system, system_complete) = system_javas(&app, rescan).await;
    let platforms = state.mc.platform.java_runtime_platforms();
    let runtimes_dir = canonical(&dirs.runtimes());
    let system: Vec<JavaInstallation> = system
        .into_iter()
        .filter(|java| !canonical(&java.path).starts_with(&runtimes_dir))
        .collect();

    let mut users: HashMap<String, Vec<JavaUserView>> = HashMap::new();
    // Custom paths that are neither launcher runtimes nor found on the system.
    let mut custom: BTreeMap<PathBuf, Vec<JavaUserView>> = BTreeMap::new();
    let svc = service(&app);
    let mut custom_majors: HashMap<PathBuf, CustomJava> = HashMap::new();
    for info in &infos {
        let user = |automatic: bool, fit: Option<JavaFit>| JavaUserView {
            instance_id: info.id.clone(),
            name: info.name.clone(),
            automatic,
            fit,
        };
        match &info.java {
            InstanceJava::Automatic(component) => {
                if let Some(runtime) = component
                    .as_deref()
                    .and_then(|c| automatic_runtime(&managed, c, &platforms))
                {
                    users
                        .entry(managed_id(&runtime.component, &runtime.platform))
                        .or_default()
                        .push(user(true, None));
                }
            }
            InstanceJava::Custom(path) => {
                if let Some((component, platform)) = managed::managed_runtime_of(&dirs, path) {
                    let major = managed
                        .iter()
                        .find(|r| r.component == component && r.platform == platform)
                        .map(|r| r.major);
                    users
                        .entry(managed_id(&component, &platform))
                        .or_default()
                        .push(user(false, fit_of(major, info.need.as_ref())));
                } else if let Some(java) = system.iter().find(|j| same_installation(&j.path, path))
                {
                    users.entry(path_id(&java.path)).or_default().push(user(
                        false,
                        fit_of(Some(java.info.major), info.need.as_ref()),
                    ));
                } else {
                    let checked = match custom_majors.get(path) {
                        Some(known) => known.clone(),
                        None => {
                            let checked = check_custom_java(path, &svc.probes).await;
                            custom_majors.insert(path.clone(), checked.clone());
                            checked
                        }
                    };
                    let major = match &checked {
                        CustomJava::Found(found) => Some(found.major),
                        _ => None,
                    };
                    custom
                        .entry(path.clone())
                        .or_default()
                        .push(user(false, fit_of(major, info.need.as_ref())));
                }
            }
        }
    }

    let job = svc
        .job
        .lock()
        .as_ref()
        .map(|job| (job.key.clone(), job.percent));
    let requested = svc.requested.lock().clone();
    let failed = svc.failed.lock().clone();
    let active = game_active(&state);

    let mut runtimes = Vec::new();
    let mut sorted_managed: Vec<&ManagedRuntime> = managed.iter().collect();
    sorted_managed.sort_by(|a, b| b.major.cmp(&a.major).then(a.platform.cmp(&b.platform)));
    for runtime in sorted_managed {
        let id = managed_id(&runtime.component, &runtime.platform);
        let key = (runtime.component.clone(), runtime.platform.clone());
        let runtime_users = users.remove(&id).unwrap_or_default();
        let update = index
            .as_ref()
            .and_then(|index| managed::update_for(index, runtime))
            .map(|entry| entry.version.name.clone());
        let updating = job
            .as_ref()
            .filter(|(job_key, _)| *job_key == key)
            .map(|(_, percent)| *percent);
        let removable_bytes = if runtime_users.is_empty() && updating.is_none() {
            let root = runtime.root.clone();
            Some(
                tokio::task::spawn_blocking(move || managed::tree_bytes(&root))
                    .await
                    .unwrap_or(0),
            )
        } else {
            None
        };
        runtimes.push(JavaRuntimeView {
            id,
            kind: JavaRuntimeKind::Managed,
            major: Some(runtime.major),
            version: Some(runtime.version.clone()),
            vendor: Some("Mojang".to_owned()),
            location: runtime.root.display().to_string(),
            executable: runtime.executable.display().to_string(),
            bits32: bits32(runtime.arch),
            users: runtime_users,
            component: Some(runtime.component.clone()),
            platform: Some(runtime.platform.clone()),
            staged: runtime.staged.is_some(),
            queued: update.is_some() && requested.contains(&key) && updating.is_none(),
            update,
            updating,
            update_failed: failed.contains(&key),
            problem: None,
            removable_bytes,
        });
    }

    let mut sorted_system = system;
    sorted_system.sort_by(|a, b| b.info.major.cmp(&a.info.major).then(a.path.cmp(&b.path)));
    for java in sorted_system {
        let id = path_id(&java.path);
        let location = pl_mc::java::release::java_home(&java.path)
            .unwrap_or_else(|| java.path.clone())
            .display()
            .to_string();
        let release = {
            let exe = java.path.clone();
            tokio::task::spawn_blocking(move || pl_mc::java::read_release(&exe))
                .await
                .ok()
                .flatten()
        };
        runtimes.push(JavaRuntimeView {
            users: users.remove(&id).unwrap_or_default(),
            id,
            kind: JavaRuntimeKind::System,
            major: Some(java.info.major),
            version: Some(java.info.version.clone()),
            vendor: java
                .info
                .vendor
                .as_deref()
                .or(release.as_ref().and_then(|r| r.vendor.as_deref()))
                .and_then(pl_mc::java::vendor_label),
            location,
            executable: java.path.display().to_string(),
            bits32: bits32(java.info.arch),
            component: None,
            platform: None,
            update: None,
            staged: false,
            queued: false,
            updating: None,
            update_failed: false,
            problem: None,
            removable_bytes: None,
        });
    }

    for (path, path_users) in custom {
        let known = custom_majors
            .get(&path)
            .cloned()
            .unwrap_or(CustomJava::Missing);
        let (major, version, vendor, arch, problem) = match known {
            CustomJava::Found(info) => (
                Some(info.major),
                Some(info.version),
                info.vendor.as_deref().and_then(pl_mc::java::vendor_label),
                info.arch,
                None,
            ),
            CustomJava::Missing => (None, None, None, Arch::Other, Some(JavaProblem::Missing)),
            CustomJava::Broken => (None, None, None, Arch::Other, Some(JavaProblem::Broken)),
        };
        runtimes.push(JavaRuntimeView {
            id: path_id(&path),
            kind: JavaRuntimeKind::Custom,
            major,
            version,
            vendor,
            location: pl_mc::java::release::java_home(&path)
                .unwrap_or_else(|| path.clone())
                .display()
                .to_string(),
            executable: path.display().to_string(),
            bits32: bits32(arch),
            users: path_users,
            component: None,
            platform: None,
            update: None,
            staged: false,
            queued: false,
            updating: None,
            update_failed: false,
            problem,
            removable_bytes: None,
        });
    }

    let checked_at = {
        let dirs = dirs.clone();
        tokio::task::spawn_blocking(move || UpdateSchedule::load(&dirs).last_success)
            .await
            .ok()
            .flatten()
    };
    Ok(JavaOverviewView {
        runtimes,
        system_incomplete: !system_complete,
        checked_at,
        game_active: active,
    })
}

// --- actions -----------------------------------------------------------------------------------

/// "Güncelle": the runtime is updated as soon as no game runs (now, usually).
#[tauri::command]
#[specta::specta]
pub async fn java_runtime_update(
    app: AppHandle,
    component: String,
    platform: String,
) -> CmdResult<()> {
    let svc = service(&app);
    let key = (component, platform);
    svc.failed.lock().remove(&key);
    svc.requested.lock().insert(key);
    svc.wake.notify_one();
    changed(&app);
    Ok(())
}

/// "Kaldır": removes a launcher runtime no instance uses. Returns the bytes freed.
#[tauri::command]
#[specta::specta]
pub async fn java_runtime_remove(
    app: AppHandle,
    state: State<'_, AppState>,
    component: String,
    platform: String,
) -> CmdResult<u64> {
    if game_active(&state) {
        return Err(CmdError::new(
            "JAVA-OYUN",
            pick(
                "Oyun açıkken Java kaldırılamaz. Önce oyunu kapat.",
                "You can't remove Java while the game is open. Close the game first.",
            ),
            "a game runs or is being prepared",
        ));
    }
    if crate::commands::data::job_running(&app) {
        return Err(CmdError::new(
            "JAVA-VERI",
            pick(
                "Veri klasöründe başka bir işlem sürüyor. Bitince tekrar dene.",
                "Something else is working in the data folder. Try again when it's done.",
            ),
            "a data job runs",
        ));
    }
    let key = (component.clone(), platform.clone());
    if service(&app)
        .job
        .lock()
        .as_ref()
        .is_some_and(|job| job.key == key)
    {
        return Err(CmdError::new(
            "JAVA-ISLEM",
            pick(
                "Bu Java güncelleniyor. Bitince tekrar dene.",
                "This Java is updating. Try again when it's done.",
            ),
            "an update of this runtime runs",
        ));
    }
    let dirs = state.mc.dirs.clone();
    let os = state.mc.platform.os;
    let managed = {
        let dirs = dirs.clone();
        tokio::task::spawn_blocking(move || managed::list_managed(&dirs, os))
            .await
            .unwrap_or_default()
    };
    let instances = state.instances.list().await.map_err(CmdError::from)?;
    let infos = instance_infos(&state, instances).await;
    if used_runtimes(&state, &managed, &infos).contains(&key) {
        return Err(CmdError::new(
            "JAVA-KULLANIMDA",
            pick(
                "Bu Java'yı kullanan kurulumlar var.",
                "Some instances use this Java.",
            ),
            format!("{component}/{platform} is in use"),
        ));
    }
    let root = dirs
        .runtime_dir(&component, &platform)
        .map_err(CmdError::from)?;
    let freed = tokio::task::spawn_blocking(move || managed::remove_runtime(&root))
        .await
        .map_err(|error| {
            CmdError::new(
                "JAVA-SILINEMEDI",
                pick("Java kaldırılamadı.", "Couldn't remove Java."),
                error.to_string(),
            )
        })?
        .map_err(|error| {
            CmdError::new(
                "JAVA-SILINEMEDI",
                pick(
                    "Java kaldırılamadı. Onu kullanan bir program açık olabilir.",
                    "Couldn't remove Java. A program that uses it may be open.",
                ),
                error.to_string(),
            )
        })?;
    let svc = service(&app);
    svc.requested.lock().remove(&key);
    svc.failed.lock().remove(&key);
    tracing::info!(%component, %platform, freed, "Java runtime removed");
    changed(&app);
    Ok(freed)
}

/// "Otomatik'e geç": these instances use automatic Java from now on. Returns how many changed.
#[tauri::command]
#[specta::specta]
pub async fn java_use_auto(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_ids: Vec<String>,
) -> CmdResult<u32> {
    let mut count = 0;
    for id in instance_ids {
        let mut instance = state.instances.get(&id).await.map_err(CmdError::from)?;
        if instance.java != JavaSetting::Auto {
            instance.java = JavaSetting::Auto;
            state
                .instances
                .update(&instance)
                .await
                .map_err(CmdError::from)?;
            count += 1;
        }
    }
    if count > 0 {
        tracing::info!(count, "instances switched to automatic Java");
        changed(&app);
    }
    Ok(count)
}

/// Opens the folder of a Java from the overview.
#[tauri::command]
#[specta::specta]
pub async fn java_open_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<()> {
    let folder = if let Some(rest) = id.strip_prefix("managed:") {
        let (component, platform) = rest.split_once('/').unwrap_or((rest, ""));
        state
            .mc
            .dirs
            .runtime_dir(component, platform)
            .map_err(CmdError::from)?
    } else {
        let path = PathBuf::from(id.strip_prefix("path:").unwrap_or(&id));
        // Only Javas the overview listed: system ones and the instances' own paths.
        let known = system_javas(&app, false)
            .await
            .0
            .iter()
            .any(|java| java.path == path)
            || state
                .instances
                .list()
                .await
                .map_err(CmdError::from)?
                .iter()
                .any(|instance| match &instance.java {
                    JavaSetting::Custom { path: custom } => {
                        resolve_custom_java(custom, state.mc.platform.os) == path
                    }
                    JavaSetting::Auto => false,
                });
        if !known {
            return Err(CmdError::new(
                "KLASOR-ACILMADI",
                pick("Klasör açılamadı.", "Couldn't open the folder."),
                "not a listed Java",
            ));
        }
        pl_mc::java::release::java_home(&path)
            .filter(|home| home.is_dir())
            .or_else(|| path.parent().map(Path::to_path_buf))
            .unwrap_or(path)
    };
    app.opener()
        .open_path(folder.to_string_lossy(), None::<&str>)
        .map_err(|error| {
            CmdError::new(
                "KLASOR-ACILMADI",
                pick(
                    "Klasör açılamadı. Dosya gezgininden elle açabilirsin.",
                    "Couldn't open the folder. You can open it yourself in your file manager.",
                ),
                error.to_string(),
            )
        })
}

/// A Java the player picks in an instance's settings ("Özel"): what it is and whether it suits
/// the instance's game and mods.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct JavaCheckView {
    /// The program the launcher would run (a Java folder or its `bin` folder leads to it).
    pub executable: String,
    pub major: Option<u32>,
    /// Full version (`21.0.7`, `1.8.0_51`).
    pub version: Option<String>,
    /// A familiar vendor name (`Eclipse Temurin`, `Microsoft`).
    pub vendor: Option<String>,
    /// A 32-bit Java on a 64-bit computer (the game gets little memory with it).
    pub bits32: bool,
    /// Gone or not working.
    pub problem: Option<JavaProblem>,
    /// Too old or too new for the instance's game or its mods.
    pub fit: Option<JavaFit>,
    /// What the game needs: the lowest Java when this one is too old, the highest when too new.
    pub need: Option<u32>,
    /// The mods raised the lowest Java above the game's own.
    pub for_mods: bool,
}

/// Instance settings → Java → "Özel": checks a picked file or folder (the same probe and cache as
/// the check before a launch), so the settings say at once what the launch would ask about.
#[tauri::command]
#[specta::specta]
pub async fn java_check(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    path: String,
) -> CmdResult<JavaCheckView> {
    let instance = state
        .instances
        .get(&instance_id)
        .await
        .map_err(CmdError::from)?;
    let picked = PathBuf::from(path.trim());
    let os = state.mc.platform.os;
    let resolved = tokio::task::spawn_blocking(move || resolve_custom_java(&picked, os))
        .await
        .map_err(CmdError::internal)?;
    let java = check_custom_java(&resolved, &service(&app).probes).await;
    let need = {
        let dirs = state.mc.dirs.clone();
        let (minecraft, loader) = (instance.mc_version.clone(), instance.loader.kind);
        tokio::task::spawn_blocking(move || pl_mc::java::game_need(&dirs, &minecraft, loader))
            .await
            .ok()
            .flatten()
    };
    let need = match need {
        Some(need) => Some(
            need.with_mods(crate::commands::modhealth::mods_java_minimum(&state, &instance).await),
        ),
        None => None,
    };
    let verdict = need.as_ref().map(|need| judge(&java, need));
    let (fit, needed) = match verdict {
        Some(JavaVerdict::TooOld { need, .. }) => (Some(JavaFit::TooOld), Some(need)),
        Some(JavaVerdict::TooNew { max, .. }) => (Some(JavaFit::TooNew), Some(max)),
        _ => (None, need.as_ref().map(|need| need.min)),
    };
    let for_mods = fit == Some(JavaFit::TooOld) && need.as_ref().is_some_and(|n| n.mods.is_some());
    let executable = resolved.display().to_string();
    let problem = match &java {
        CustomJava::Missing => Some(JavaProblem::Missing),
        CustomJava::Broken => Some(JavaProblem::Broken),
        CustomJava::Found(_) => None,
    };
    Ok(match java {
        CustomJava::Found(info) => JavaCheckView {
            executable,
            major: Some(info.major),
            vendor: info.vendor.as_deref().and_then(pl_mc::java::vendor_label),
            version: Some(info.version),
            bits32: bits32(info.arch),
            problem,
            fit,
            need: needed,
            for_mods,
        },
        CustomJava::Missing | CustomJava::Broken => JavaCheckView {
            executable,
            major: None,
            version: None,
            vendor: None,
            bits32: false,
            problem,
            fit: None,
            need: needed,
            for_mods: false,
        },
    })
}

// --- the check before a launch -----------------------------------------------------------------

/// The answer to the Java question, for the next launch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum JavaChoice {
    /// "Kur ve oyna": switch the instance to a Java that fits (downloaded if needed) and play.
    Install,
    /// "Yine de dene": launch with the custom Java; not asked again this session.
    TryAnyway,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum JavaPromptProblem {
    TooOld,
    TooNew,
    Missing,
    Broken,
}

/// The Java question before a launch.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct JavaPromptView {
    pub instance_id: String,
    pub instance_name: String,
    pub problem: JavaPromptProblem,
    /// The custom Java's version, when it runs.
    pub have: Option<u32>,
    /// The Java the game needs: the minimum (too old, missing, broken) or the maximum (too new).
    pub need: u32,
    /// The mods raised the minimum above the game's own.
    pub for_mods: bool,
    /// "Kur ve oyna" can fix it (a launcher runtime fits).
    pub fixable: bool,
    /// "Kur ve oyna" switches the instance to "Otomatik" (else to a launcher runtime of `need`,
    /// for mods that need more than the game's own runtime).
    pub automatic: bool,
}

/// What the launch does after the Java check.
pub(crate) enum JavaGate {
    Continue,
    /// Ask first: the launch stops with the question.
    Ask(JavaPromptView),
}

/// Downloads the launcher runtime `component` (if needed) and points the instance at it. For
/// games whose mods need a newer Java than the game's own runtime. Returns its Java version.
pub(crate) async fn use_managed_runtime(
    state: &AppState,
    instance: &mut Instance,
    component: &str,
    cancel: &CancellationToken,
    progress: Option<ProgressFn>,
) -> pl_mc::Result<u32> {
    let installed = pl_mc::java::install_runtime(
        &state.mc,
        component,
        &EnsureJavaOptions {
            offline: false,
            verify: VerifyMode::Size,
            cancel: cancel.clone(),
            progress,
        },
    )
    .await?;
    instance.java = JavaSetting::Custom {
        path: installed.path.clone(),
    };
    state.instances.update(instance).await?;
    tracing::info!(instance = %instance.id, %component, major = installed.info.major, "instance uses a launcher runtime");
    Ok(installed.info.major)
}

async fn apply_remedy(
    state: &AppState,
    instance: &mut Instance,
    need: &JavaNeed,
    progress: &InstallProgressFn,
    cancel: &CancellationToken,
) -> CmdResult<()> {
    match remedy(need) {
        Some(JavaRemedy::Automatic) => {
            instance.java = JavaSetting::Auto;
            state
                .instances
                .update(instance)
                .await
                .map_err(CmdError::from)?;
            tracing::info!(instance = %instance.id, "switched to automatic Java");
            Ok(())
        }
        Some(JavaRemedy::Managed { component, .. }) => {
            let progress = Arc::clone(progress);
            let download: ProgressFn = Arc::new(move |download: DownloadProgress| {
                progress(InstallProgress {
                    stage: InstallStage::Java,
                    download,
                });
            });
            use_managed_runtime(state, instance, component, cancel, Some(download))
                .await
                .map(|_| ())
                .map_err(CmdError::from)
        }
        None => Err(CmdError::new(
            "JAVA-UYGUN-YOK",
            pick(
                "Bu oyun için uygun bir Java bulunamadı.",
                "No suitable Java was found for this game.",
            ),
            format!("{need:?}"),
        )),
    }
}

fn prompt_view(instance: &Instance, verdict: JavaVerdict, need: &JavaNeed) -> JavaPromptView {
    let (problem, have, needed) = match verdict {
        JavaVerdict::TooOld { have, need } => (JavaPromptProblem::TooOld, Some(have), need),
        JavaVerdict::TooNew { have, max } => (JavaPromptProblem::TooNew, Some(have), max),
        JavaVerdict::Missing => (JavaPromptProblem::Missing, None, need.min),
        JavaVerdict::Broken | JavaVerdict::Fine => (JavaPromptProblem::Broken, None, need.min),
    };
    let fix = remedy(need);
    JavaPromptView {
        instance_id: instance.id.clone(),
        instance_name: instance.name.clone(),
        problem,
        have,
        need: needed,
        for_mods: problem == JavaPromptProblem::TooOld && need.mods.is_some(),
        fixable: fix.is_some(),
        automatic: fix == Some(JavaRemedy::Automatic),
    }
}

/// The Java check before a launch, for instances with a custom Java: what the game (and its mods)
/// need against what the custom Java is. Asks only about certain problems, once per answer and
/// session; "Kur ve oyna" switches the instance here (downloading a launcher runtime when the
/// game's own runtime is not enough) and the launch goes on.
pub(crate) async fn prelaunch(
    app: &AppHandle,
    state: &AppState,
    instance: &mut Instance,
    manifest: Option<&VersionManifest>,
    choice: Option<JavaChoice>,
    progress: &InstallProgressFn,
    cancel: &CancellationToken,
) -> CmdResult<JavaGate> {
    let JavaSetting::Custom { path } = &instance.java else {
        return Ok(JavaGate::Continue);
    };
    let path = resolve_custom_java(path, state.mc.platform.os);
    let need = match pl_mc::version::load_version_json(&state.mc, &instance.mc_version, manifest)
        .await
    {
        Ok(json) if json.inherits_from.is_none() => Some(pl_mc::java::need_from_version_json(
            &json,
            instance.loader.kind,
            &instance.mc_version,
        )),
        _ => {
            let dirs = state.mc.dirs.clone();
            let (minecraft, loader) = (instance.mc_version.clone(), instance.loader.kind);
            tokio::task::spawn_blocking(move || pl_mc::java::game_need(&dirs, &minecraft, loader))
                .await
                .ok()
                .flatten()
        }
    };
    let Some(need) = need else {
        tracing::info!(instance = %instance.id, "the game's Java need is unknown; not checked");
        return Ok(JavaGate::Continue);
    };
    let need = need.with_mods(crate::commands::modhealth::mods_java_minimum(state, instance).await);
    let svc = service(app);
    let java = check_custom_java(&path, &svc.probes).await;
    let verdict = judge(&java, &need);
    tracing::info!(
        instance = %instance.id,
        verdict = %verdict.key(),
        min = need.min,
        max = ?need.max,
        "pre-launch Java check"
    );
    if verdict.is_fine() {
        return Ok(JavaGate::Continue);
    }
    match choice {
        Some(JavaChoice::TryAnyway) => {
            svc.prompts.lock().remember(&instance.id, &path, verdict);
            Ok(JavaGate::Continue)
        }
        Some(JavaChoice::Install) => {
            apply_remedy(state, instance, &need, progress, cancel).await?;
            changed(app);
            Ok(JavaGate::Continue)
        }
        None if svc.prompts.lock().answered(&instance.id, &path, verdict) => Ok(JavaGate::Continue),
        None => Ok(JavaGate::Ask(prompt_view(instance, verdict, &need))),
    }
}

// --- background updates ------------------------------------------------------------------------

/// Nothing else wants the network or the data folder: no game running or being prepared, no
/// data job, no content download.
fn quiet(app: &AppHandle, state: &AppState) -> bool {
    !game_active(state)
        && !crate::commands::data::job_running(app)
        && !crate::commands::modrinth::jobs_running(app)
        && !crate::commands::library::jobs_running(app)
}

/// A launch is starting: a runtime download in progress stops (it resumes later) and leaves the
/// network to the game.
pub(crate) async fn yield_to_launch(app: &AppHandle) {
    let svc = service(app);
    let cancel = svc.job.lock().as_ref().map(|job| job.cancel.clone());
    let Some(cancel) = cancel else {
        return;
    };
    cancel.cancel();
    let deadline = Instant::now() + Duration::from_secs(2);
    while svc.job.lock().is_some() && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    tracing::info!("runtime update paused for a launch");
}

/// The game closed: staged updates can take their place now.
pub(crate) fn game_exited(app: &AppHandle) {
    service(app).wake.notify_one();
}

/// Puts a staged update in place while no game runs; a launch that starts meanwhile waits for
/// the renames.
async fn swap_runtime(app: &AppHandle, root: PathBuf) -> Option<SwapOutcome> {
    let app = app.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let preparing = state.launch_cancel.lock();
        if preparing.is_some() || state.running.lock().is_some() {
            return Ok(SwapOutcome::InUse);
        }
        managed::swap_in_staged(&root)
    })
    .await
    .ok()?;
    match outcome {
        Ok(outcome) => Some(outcome),
        Err(error) => {
            tracing::warn!(%error, "could not apply a Java runtime update");
            None
        }
    }
}

async fn stage_one(
    app: &AppHandle,
    state: &AppState,
    runtime: &ManagedRuntime,
    entry: &RuntimeEntry,
) -> bool {
    let svc = service(app);
    let key = (runtime.component.clone(), runtime.platform.clone());
    let cancel = CancellationToken::new();
    {
        let mut job = svc.job.lock();
        if job.is_some() {
            return false;
        }
        *job = Some(UpdateJob {
            key: key.clone(),
            cancel: cancel.clone(),
            percent: 0.0,
        });
    }
    changed(app);
    crate::commands::cleanup::status_changed(app);
    let id = managed_id(&runtime.component, &runtime.platform);
    let progress: ProgressFn = {
        let app = app.clone();
        Arc::new(move |p: DownloadProgress| {
            let percent = if p.total_bytes > 0 {
                p.done_bytes as f64 * 100.0 / p.total_bytes as f64
            } else if p.total_files > 0 {
                p.done_files as f64 * 100.0 / p.total_files as f64
            } else {
                0.0
            }
            .clamp(0.0, 100.0);
            if let Some(job) = service(&app).job.lock().as_mut() {
                job.percent = percent;
            }
            let _ = app.emit(
                "java-progress",
                JavaProgressEvent {
                    id: id.clone(),
                    percent,
                },
            );
        })
    };
    tracing::info!(
        component = %runtime.component,
        installed = %runtime.version,
        available = %entry.version.name,
        "updating a Java runtime"
    );
    let result = managed::stage_update(
        &state.mc,
        &runtime.component,
        &runtime.platform,
        entry,
        &cancel,
        Some(progress),
    )
    .await;
    svc.job.lock().take();
    crate::commands::cleanup::status_changed(app);
    let done = match result {
        Ok(_) => {
            svc.requested.lock().remove(&key);
            if quiet(app, state) {
                swap_runtime(app, runtime.root.clone()).await;
            }
            true
        }
        Err(error) if error.is_cancelled() => {
            tracing::info!(component = %runtime.component, "runtime update stopped; it resumes later");
            false
        }
        Err(error) => {
            tracing::warn!(%error, component = %runtime.component, "runtime update failed");
            svc.requested.lock().remove(&key);
            svc.failed.lock().insert(key);
            false
        }
    };
    changed(app);
    done
}

/// One round of the updater.
async fn update_round(app: &AppHandle) {
    let state = app.state::<AppState>();
    if !quiet(app, &state) {
        return;
    }
    let svc = service(app);
    let dirs = state.mc.dirs.clone();
    let os = state.mc.platform.os;
    let list = |dirs: pl_mc::paths::DataDirs| async move {
        tokio::task::spawn_blocking(move || managed::list_managed(&dirs, os))
            .await
            .unwrap_or_default()
    };

    // Updates already downloaded take their place first.
    let mut swapped = false;
    for runtime in list(dirs.clone()).await {
        if runtime.staged.is_some()
            && let Some(SwapOutcome::Swapped { .. }) = swap_runtime(app, runtime.root.clone()).await
        {
            swapped = true;
        }
    }
    if swapped {
        changed(app);
    }

    let requested = svc.requested.lock().clone();
    let mut schedule = {
        let dirs = dirs.clone();
        tokio::task::spawn_blocking(move || UpdateSchedule::load(&dirs))
            .await
            .unwrap_or_default()
    };
    let now = now_ms();
    if requested.is_empty() && !schedule.due(now) {
        return;
    }
    let index = pl_mc::java::refresh_runtime_index(&state.mc).await;
    schedule.record(now, index.is_ok());
    {
        let dirs = dirs.clone();
        if let Ok(Err(error)) = tokio::task::spawn_blocking(move || schedule.save(&dirs)).await {
            tracing::warn!(%error, "could not save the runtime update schedule");
        }
    }
    let index = match index {
        Ok(index) => index,
        Err(error) => {
            tracing::info!(%error, "could not look for Java runtime updates");
            if !requested.is_empty() {
                let mut failed = svc.failed.lock();
                for key in &requested {
                    failed.insert(key.clone());
                }
                svc.requested.lock().clear();
                drop(failed);
                changed(app);
            }
            return;
        }
    };
    let managed = list(dirs.clone()).await;
    let instances = state.instances.list().await.unwrap_or_default();
    let infos = instance_infos(&state, instances).await;
    let used = used_runtimes(&state, &managed, &infos);
    // Requests for runtimes that are current (or gone) are done.
    svc.requested.lock().retain(|key| {
        managed.iter().any(|runtime| {
            runtime.component == key.0
                && runtime.platform == key.1
                && managed::update_for(&index, runtime).is_some()
        })
    });
    let wanted = svc.requested.lock().clone();
    let targets = managed::runtimes_to_stage(&managed, &index, |runtime| {
        let key = (runtime.component.clone(), runtime.platform.clone());
        wanted.contains(&key) || used.contains(&key)
    });
    if targets.is_empty() {
        changed(app);
        return;
    }
    for (runtime, entry) in targets {
        if !quiet(app, &state) {
            break;
        }
        stage_one(app, &state, runtime, &entry).await;
    }
}

/// Starts the background updater of the launcher's Java runtimes.
pub fn start_java_updates(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let svc = service(&app);
        tokio::select! {
            () = tokio::time::sleep(FIRST_ROUND) => {}
            () = svc.wake.notified() => {}
        }
        loop {
            update_round(&app).await;
            tokio::select! {
                () = tokio::time::sleep(ROUND_EVERY) => {}
                () = svc.wake.notified() => {}
            }
        }
    });
}
