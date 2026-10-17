//! Settings → Veri: the data folder, its caches and logs, support bundles, and moving the folder
//! to another disk. The work itself lives in `pl_data`; this module adds the app's state, the
//! progress channel and the restart.
//!
//! A moved folder is found through a pointer file outside the data folder
//! (`<OS config>/gg.projects.launcher/location.json`; debug builds use their own identifier's
//! folder, see `crate::identity`), read by `state::default_root`. The old folder is deleted by
//! [`finish_pending_move`] once the launcher has run from the new one.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use parking_lot::Mutex;
use pl_data::bundle::{self, BundleEntry, BundleFolder, InstanceReport, SystemReport};
use pl_data::relocate::{self, MoveStage};
use pl_data::{CancellationToken, TargetProblem};
use pl_mc::instance::Instance;
use pl_mc::java::JavaSetting;
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

const LOCATION_FILE: &str = "location.json";
/// The old folder of a move is deleted only after the launcher has run this long from the new
/// one.
const FINISH_DELAY: Duration = Duration::from_secs(30);
/// What a support bundle includes.
const BUNDLE_LAUNCHER_LOG_AGE: Duration = Duration::from_secs(3 * 24 * 60 * 60);
const BUNDLE_GAME_LOGS: usize = 5;
const BUNDLE_CRASH_AGE: Duration = Duration::from_secs(14 * 24 * 60 * 60);
const BUNDLE_CRASH_REPORTS: usize = 10;
const SYSTEM_JAVA_TIMEOUT: Duration = Duration::from_secs(15);

/// Where the pointer to a moved data folder lives: outside the data folder, in the OS config
/// folder. Debug builds accept `PROJECTS_LAUNCHER_LOCATION_FILE` (tests on throwaway folders).
pub fn location_file() -> Option<PathBuf> {
    #[cfg(debug_assertions)]
    if let Some(file) =
        std::env::var_os("PROJECTS_LAUNCHER_LOCATION_FILE").filter(|file| !file.is_empty())
    {
        return Some(PathBuf::from(file));
    }
    location_file_of(&crate::identity::current().identifier)
}

/// The pointer file of the launcher with this app identifier (Tauri's app config folder name).
pub fn location_file_of(identifier: &str) -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join(identifier).join(LOCATION_FILE))
}

/// The data folder chosen with "Taşı", when there is one and it is reachable.
pub fn relocated_root() -> Option<PathBuf> {
    relocate::pointed_root(&location_file()?)
}

/// `PROJECTS_LAUNCHER_DATA_DIR` pins the data folder (portable installs, development).
fn pinned_by_environment() -> bool {
    std::env::var_os("PROJECTS_LAUNCHER_DATA_DIR").is_some_and(|dir| !dir.is_empty())
}

fn display(path: &Path) -> String {
    path.display().to_string()
}

// ---------------------------------------------------------------------------------------------
// Jobs: one cleanup or move at a time (a cleanup must not delete what a move is copying, and
// finishing an earlier move must not rewrite the pointer while a new move switches it).

pub(crate) enum DataJob {
    Cleaning,
    Moving(CancellationToken),
    /// Deleting the folders an earlier move left behind.
    Finishing,
}

/// The data job in progress, created on first use (no setup code needed).
#[derive(Default)]
pub struct DataJobs {
    current: Mutex<Option<DataJob>>,
}

fn data_jobs(app: &AppHandle) -> State<'_, DataJobs> {
    if app.try_state::<DataJobs>().is_none() {
        app.manage(DataJobs::default());
    }
    app.state::<DataJobs>()
}

/// Whether a data job (move, cleanup, finishing a move) is running.
pub(crate) fn job_running(app: &AppHandle) -> bool {
    data_jobs(app).current.lock().is_some()
}

/// Whether the data folder is being copied to a new place (or was copied and waits for the
/// restart). A game started meanwhile would write into the old folder, which is deleted after
/// the restart: its changes could be lost.
pub(crate) fn move_running(app: &AppHandle) -> bool {
    matches!(
        data_jobs(app).current.lock().as_ref(),
        Some(DataJob::Moving(_))
    )
}

/// A registered job; unregistered on drop.
pub(crate) struct JobGuard {
    app: AppHandle,
}

impl Drop for JobGuard {
    fn drop(&mut self) {
        data_jobs(&self.app).current.lock().take();
    }
}

pub(crate) fn begin_job(app: &AppHandle, job: DataJob) -> CmdResult<JobGuard> {
    let jobs = data_jobs(app);
    let mut current = jobs.current.lock();
    if let Some(running) = current.as_ref() {
        let message = match running {
            DataJob::Moving(_) => "Veri klasörü taşınıyor. Bitmesini bekle.",
            DataJob::Cleaning => "Temizlik sürüyor. Bitmesini bekle.",
            DataJob::Finishing => "Önceki taşıma tamamlanıyor. Biraz sonra tekrar dene.",
        };
        return Err(CmdError::new(
            "VERI-MESGUL",
            message,
            "another data job is running",
        ));
    }
    *current = Some(job);
    Ok(JobGuard { app: app.clone() })
}

// ---------------------------------------------------------------------------------------------
// Overview, folder, cache

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DataOverview {
    /// The data folder.
    pub root: String,
    pub cache_bytes: u64,
    pub logs_bytes: u64,
    /// Free space on the data folder's disk.
    pub free_bytes: Option<u64>,
    /// "Taşı" is possible (the folder is not pinned by the launch environment).
    pub movable: bool,
    /// A moved data folder that was not found at start (an unplugged disk): the default folder
    /// is used until it is back.
    pub unreachable: Option<String>,
}

/// Where the data lives and how much the caches and logs take (sizes are computed off the UI
/// thread).
#[tauri::command]
#[specta::specta]
pub async fn data_overview(state: State<'_, AppState>) -> CmdResult<DataOverview> {
    let root = state.root.clone();
    let usage = tauri::async_runtime::spawn_blocking(move || pl_data::usage::usage(&root))
        .await
        .map_err(CmdError::internal)?;
    let pinned = pinned_by_environment();
    let unreachable = location_file()
        .filter(|_| !pinned)
        .and_then(|file| relocate::read_pointer(&file))
        .filter(|pointer| !pointer.root.is_dir())
        .map(|pointer| display(&pointer.root));
    Ok(DataOverview {
        root: display(&state.root),
        cache_bytes: usage.cache_bytes,
        logs_bytes: usage.logs_bytes,
        free_bytes: usage.free_bytes,
        movable: !pinned,
        unreachable,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn data_open_folder(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    app.opener()
        .open_path(state.root.to_string_lossy(), None::<&str>)
        .map_err(|error| {
            CmdError::new(
                "KLASOR-ACILMADI",
                "Klasör açılamadı. Dosya gezgininden elle açabilirsin.",
                error.to_string(),
            )
        })
}

// ---------------------------------------------------------------------------------------------
// Support bundle

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SupportBundleView {
    pub path: String,
    pub file_name: String,
    /// Where it was saved: "downloads" | "desktop" | "home".
    pub folder: String,
}

/// A zip for the support team in the user's Downloads (else Desktop): launcher logs of the last
/// three days, the last five game logs, recent crash reports, `sistem.txt` and `kurulumlar.txt`.
/// Everything is masked; accounts, AFK data and keychain contents are never included.
#[tauri::command]
#[specta::specta]
pub async fn data_support_bundle(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<SupportBundleView> {
    let Some((dir, folder)) = bundle::bundle_folder() else {
        return Err(CmdError::new(
            "DESTEK-KLASOR",
            "Destek paketini kaydedecek bir klasör bulunamadı.",
            "no Downloads, Desktop or home folder",
        ));
    };
    let instances = match state.instances.list().await {
        Ok(list) => list,
        Err(error) => {
            tracing::warn!(%error, "support bundle: instances could not be listed");
            Vec::new()
        }
    };
    let game_dirs: Vec<(Instance, PathBuf)> = instances
        .into_iter()
        .filter_map(|instance| {
            let game_dir = state.instances.game_dir(&instance.id).ok()?;
            Some((instance, game_dir))
        })
        .collect();
    let now = jiff::Zoned::now();
    let report = SystemReport {
        created_at: now.strftime("%Y-%m-%d %H:%M (%:z)").to_string(),
        launcher_version: app.package_info().version.to_string(),
        debug_build: cfg!(debug_assertions),
        machine: Default::default(),
        data_dir: state.root.clone(),
        data_dir_free: None,
        secure_storage: !state.ephemeral_secrets,
        java: java_lines(&state).await,
    };
    let root = state.root.clone();
    let file_name = bundle::bundle_file_name(&now);

    let path = tauri::async_runtime::spawn_blocking(move || {
        let entries = bundle_entries(&root, report, &game_dirs);
        let path = bundle::unique_path(&dir, &file_name);
        bundle::write_bundle(&path, &entries).map(|count| (path, count))
    })
    .await
    .map_err(CmdError::internal)?
    .map(|(path, count)| {
        tracing::info!(path = %path.display(), entries = count, "support bundle created");
        path
    })
    .map_err(|error| {
        CmdError::new(
            "DESTEK",
            "Destek paketi oluşturulamadı. Diskte yer olduğundan emin olup tekrar dene.",
            error.to_string(),
        )
    })?;
    Ok(SupportBundleView {
        file_name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        path: display(&path),
        folder: match folder {
            BundleFolder::Downloads => "downloads",
            BundleFolder::Desktop => "desktop",
            BundleFolder::Home => "home",
        }
        .to_owned(),
    })
}

/// Every file of a support bundle (blocking: reads folders and system facts).
fn bundle_entries(
    root: &Path,
    mut report: SystemReport,
    game_dirs: &[(Instance, PathBuf)],
) -> Vec<BundleEntry> {
    let now = SystemTime::now();
    let logs = root.join(pl_data::usage::LOGS_DIR);
    report.machine = bundle::MachineInfo::collect();
    report.data_dir_free = pl_data::usage::free_space(root);

    let instances: Vec<InstanceReport> = game_dirs
        .iter()
        .map(|(instance, game_dir)| instance_report(instance, game_dir))
        .collect();
    let mut entries = vec![
        BundleEntry::text("sistem.txt", report.render()),
        BundleEntry::text("kurulumlar.txt", bundle::render_instances(&instances)),
    ];
    let file_name = |path: &Path| {
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    for path in bundle::recent_launcher_logs(&logs, now, BUNDLE_LAUNCHER_LOG_AGE) {
        entries.push(BundleEntry::file(
            format!("launcher-kayitlari/{}", file_name(&path)),
            path,
        ));
    }
    for path in bundle::newest_game_logs(&logs, BUNDLE_GAME_LOGS) {
        entries.push(BundleEntry::file(
            format!("oyun-kayitlari/{}", file_name(&path)),
            path,
        ));
    }
    let labelled: Vec<(String, PathBuf)> = game_dirs
        .iter()
        .map(|(instance, game_dir)| (instance.name.clone(), game_dir.clone()))
        .collect();
    for (label, path) in
        bundle::recent_crash_reports(&labelled, now, BUNDLE_CRASH_AGE, BUNDLE_CRASH_REPORTS)
    {
        entries.push(BundleEntry::file(
            format!(
                "cokme-raporlari/{}/{}",
                bundle::zip_component(&label),
                file_name(&path)
            ),
            path,
        ));
    }
    entries
}

fn instance_report(instance: &Instance, game_dir: &Path) -> InstanceReport {
    let loader = match &instance.loader.version {
        Some(version) => format!("{:?} {version}", instance.loader.kind),
        None => format!("{:?}", instance.loader.kind),
    };
    InstanceReport {
        name: instance.name.clone(),
        id: instance.id.clone(),
        minecraft: instance.mc_version.clone(),
        loader,
        pack: instance
            .managed_pack
            .as_ref()
            .map(|pack| format!("{} {}", pack.pack_id, pack.version)),
        java: match &instance.java {
            JavaSetting::Auto => "Otomatik".to_owned(),
            JavaSetting::Custom { path } => format!("Özel: {}", path.display()),
        },
        memory: format!("{}-{} MB", instance.memory.min_mb, instance.memory.max_mb),
        jvm_args: instance.jvm_args.clone(),
        mods: bundle::mod_files(game_dir),
    }
}

/// Javas the launcher can use: Mojang runtimes in the data folder, then system installs.
async fn java_lines(state: &AppState) -> Vec<String> {
    let mut lines = Vec::new();
    let runtimes = state.mc.dirs.runtimes();
    let components: Vec<String> = std::fs::read_dir(&runtimes)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_ok_and(|t| t.is_dir()))
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    for component in components {
        if let Some(java) = pl_mc::java::installed_runtime(&state.mc, &component).await {
            lines.push(format!(
                "Mojang {component}: Java {} ({:?}), {}",
                java.info.version,
                java.info.arch,
                java.path.display()
            ));
        }
    }
    match tokio::time::timeout(SYSTEM_JAVA_TIMEOUT, pl_mc::java::detect_system_javas()).await {
        Ok(found) => {
            for java in found {
                let vendor = java
                    .info
                    .vendor
                    .map(|vendor| format!(" {vendor}"))
                    .unwrap_or_default();
                lines.push(format!(
                    "Sistem: Java {}{vendor} ({:?}), {}",
                    java.info.version,
                    java.info.arch,
                    java.path.display()
                ));
            }
        }
        Err(_) => lines.push("Sistemdeki Java'lar zamanında taranamadı.".to_owned()),
    }
    lines
}

// ---------------------------------------------------------------------------------------------
// Moving the data folder

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MovePlanView {
    pub from: String,
    /// The folder the data goes into (a `ProjectsLauncher` folder inside the picked one).
    pub target: String,
    pub total_bytes: u64,
    pub file_count: u64,
    /// Free space on the target disk.
    pub free_bytes: Option<u64>,
    pub enough_space: bool,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MoveProgressView {
    /// "scanning" | "copying" | "verifying"
    pub stage: String,
    pub done_bytes: u64,
    pub total_bytes: u64,
    pub done_files: u64,
    pub total_files: u64,
}

/// Moves need a quiet launcher: running games and AFK accounts keep files open, installs and
/// backups running in the background write files the copy would miss, and a pinned folder
/// (`PROJECTS_LAUNCHER_DATA_DIR`) would ignore the new place anyway.
fn ensure_can_move(app: &AppHandle, state: &AppState) -> CmdResult<()> {
    if pinned_by_environment() {
        return Err(CmdError::new(
            "TASI-SABIT",
            "Bu kurulumda veri klasörünün yeri sabit, buradan taşınamaz.",
            "PROJECTS_LAUNCHER_DATA_DIR is set",
        ));
    }
    if state.running.lock().is_some() {
        return Err(CmdError::new(
            "TASI-OYUN",
            "Oyun açıkken veri klasörü taşınamaz. Önce oyunu kapat.",
            "a game is running",
        ));
    }
    if state.launch_cancel.lock().is_some() {
        return Err(CmdError::new(
            "TASI-HAZIRLIK",
            "Oyun hazırlanırken veri klasörü taşınamaz. Bitmesini bekle.",
            "a launch is being prepared",
        ));
    }
    if state.afk.any_active() {
        return Err(CmdError::new(
            "TASI-AFK",
            "AFK hesapları çalışırken veri klasörü taşınamaz. Önce hepsini durdur.",
            "AFK sessions are active",
        ));
    }
    if crate::commands::modrinth::jobs_running(app)
        || crate::commands::library::jobs_running(app)
        || crate::commands::java::job_running(app)
    {
        return Err(CmdError::new(
            "TASI-ISLEM",
            "Arka planda bir kurulum ya da yedekleme sürüyor. Bitince tekrar dene.",
            "a content or library job is running",
        ));
    }
    Ok(())
}

fn move_error(error: pl_data::Error) -> CmdError {
    use pl_data::Error as E;
    use std::io::ErrorKind;
    let details = error.to_string();
    let (code, message) = match &error {
        E::Cancelled => (
            "IPTAL",
            "Taşıma iptal edildi. Verilerin eski yerinde duruyor.",
        ),
        E::Target { problem, .. } => match problem {
            TargetProblem::NotAbsolute | TargetProblem::NotAFolder => (
                "TASI-HEDEF",
                "Seçilen yer bir klasör değil. Başka bir klasör seç.",
            ),
            TargetProblem::Same => ("TASI-AYNI", "Verilerin zaten bu klasörde."),
            TargetProblem::InsideCurrent => (
                "TASI-ICERIDE",
                "Yeni yer, şu anki veri klasörünün içinde olamaz. Başka bir klasör seç.",
            ),
            TargetProblem::ContainsCurrent => (
                "TASI-KAPSIYOR",
                "Yeni yer, şu anki veri klasörünü içeremez. Başka bir klasör seç.",
            ),
            TargetProblem::NotEmpty => (
                "TASI-DOLU",
                "Seçtiğin yerde içi dolu bir ProjectsLauncher klasörü var. Başka bir klasör seç.",
            ),
            TargetProblem::NotWritable => (
                "TASI-YAZILAMIYOR",
                "Seçtiğin klasöre yazılamıyor. Başka bir klasör seç.",
            ),
        },
        E::NotEnoughSpace { .. } => (
            "TASI-YER",
            "Seçtiğin diskte yeterli boş alan yok. Yer aç ya da başka bir disk seç.",
        ),
        E::Mismatch { .. } => (
            "TASI-DOGRULAMA",
            "Kopyalanan dosyalar doğrulanamadı. Verilerin eski yerinde duruyor, tekrar dene.",
        ),
        E::Link { folder: true, .. } => (
            "TASI-BAGLANTI",
            "Veri klasöründeki bir klasör bağlantısı yeni yerde oluşturulamadı. Verilerin eski yerinde duruyor.",
        ),
        E::Link { folder: false, .. } => (
            "TASI-BAGLANTI",
            "Veri klasöründeki bir dosya bağlantısı yeni yerde oluşturulamadı. Verilerin eski yerinde duruyor.",
        ),
        E::Io { source, .. } if source.kind() == ErrorKind::StorageFull => (
            "TASI-YER",
            "Kopyalarken disk doldu. Verilerin eski yerinde duruyor.",
        ),
        E::Io { source, .. } if source.kind() == ErrorKind::PermissionDenied => (
            "TASI-ERISIM",
            "Bazı dosyalar kopyalanamadı, başka bir program kullanıyor olabilir. Diğer programları kapatıp tekrar dene. Verilerin eski yerinde duruyor.",
        ),
        _ => (
            "TASI",
            "Veri klasörü taşınamadı. Verilerin eski yerinde duruyor.",
        ),
    };
    CmdError::new(code, message, details)
}

/// Checks the folder the user picked and estimates the move (size, free space). Nothing is
/// written yet.
#[tauri::command]
#[specta::specta]
pub async fn data_move_plan(
    app: AppHandle,
    state: State<'_, AppState>,
    picked: String,
) -> CmdResult<MovePlanView> {
    ensure_can_move(&app, &state)?;
    let current = state.root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let target = relocate::resolve_target(&current, Path::new(&picked)).map_err(move_error)?;
        let manifest = relocate::scan(&current, &CancellationToken::new()).map_err(move_error)?;
        let free_bytes = pl_data::usage::free_space(&target);
        Ok(MovePlanView {
            from: display(&current),
            target: display(&target),
            total_bytes: manifest.bytes,
            file_count: manifest.files,
            free_bytes,
            enough_space: relocate::ensure_space(manifest.bytes, free_bytes).is_ok(),
        })
    })
    .await
    .map_err(CmdError::internal)?
}

/// Copies the data folder into the picked place with progress, verifies the copy, points the
/// launcher at it and restarts. Until the restart nothing in the old folder changes; on any
/// failure (or `data_move_cancel`) the partial copy is removed.
#[tauri::command]
#[specta::specta]
pub async fn data_move_start(
    app: AppHandle,
    state: State<'_, AppState>,
    picked: String,
    on_progress: Channel<MoveProgressView>,
) -> CmdResult<()> {
    ensure_can_move(&app, &state)?;
    let pointer_file = location_file().ok_or_else(|| {
        CmdError::new(
            "TASI-AYAR",
            "Yeni yer kaydedilemiyor, bu bilgisayarda ayar klasörü bulunamadı.",
            "no OS config folder",
        )
    })?;
    let cancel = CancellationToken::new();
    let job = begin_job(&app, DataJob::Moving(cancel.clone()))?;
    let current = state.root.clone();
    tracing::info!(from = %current.display(), %picked, "moving the data folder");

    let progress: relocate::ProgressFn = Arc::new(move |p| {
        let _ = on_progress.send(MoveProgressView {
            stage: match p.stage {
                MoveStage::Scanning => "scanning",
                MoveStage::Copying => "copying",
                MoveStage::Verifying => "verifying",
            }
            .to_owned(),
            done_bytes: p.copy.done_bytes,
            total_bytes: p.copy.total_bytes,
            done_files: p.copy.done_files,
            total_files: p.copy.total_files,
        });
    });
    let source = current.clone();
    let copy_cancel = cancel.clone();
    let report = tauri::async_runtime::spawn_blocking(move || {
        let target = relocate::resolve_target(&source, Path::new(&picked))?;
        relocate::copy_data_root(
            &source,
            &target,
            &copy_cancel,
            progress,
            &relocate::is_live_launcher_log,
        )
    })
    .await
    .map_err(CmdError::internal)?
    .map_err(|error| {
        tracing::warn!(%error, "data folder move failed");
        move_error(error)
    })?;

    // Something started meanwhile (the tray menu still works, an install was queued), or the
    // user cancelled at the last moment: the copy would miss what they write, so it goes.
    let late = ensure_can_move(&app, &state).err().or_else(|| {
        cancel
            .is_cancelled()
            .then(|| move_error(pl_data::Error::Cancelled))
    });
    if let Some(error) = late {
        discard_copy(report).await;
        return Err(error);
    }
    let (file, from, to) = (pointer_file, current.clone(), report.target.clone());
    let switched =
        tauri::async_runtime::spawn_blocking(move || relocate::switch_root(&file, &from, &to))
            .await
            .map_err(CmdError::internal)?;
    if let Err(error) = switched {
        discard_copy(report).await;
        return Err(CmdError::new(
            "TASI-GECIS",
            "Yeni yer kaydedilemedi. Verilerin eski yerinde duruyor.",
            error.to_string(),
        ));
    }
    tracing::info!(
        to = %report.target.display(),
        files = report.files,
        bytes = report.bytes,
        "data folder copied, restarting from the new place"
    );
    // No other data job until the restart; the UI shows the result for a moment first.
    std::mem::forget(job);
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(1500)).await;
        handle.request_restart();
    });
    Ok(())
}

async fn discard_copy(report: relocate::CopyReport) {
    let _ = tauri::async_runtime::spawn_blocking(move || {
        relocate::discard_copy(&report.target, report.created_target)
    })
    .await;
}

/// Stops a running move; its partial copy is removed and the data stays where it was.
#[tauri::command]
#[specta::specta]
pub fn data_move_cancel(app: AppHandle) {
    if let Some(DataJob::Moving(cancel)) = data_jobs(&app).current.lock().as_ref() {
        cancel.cancel();
    }
}

/// After a move: once the launcher has run a while from the new folder, the old ones are deleted
/// in the background and forgotten (see `pl_data::relocate::finish_move` for the safety rules).
/// Runs as a data job, so a new move cannot switch the pointer meanwhile; when another data job
/// runs, it waits for the next start.
pub fn finish_pending_move(app: &AppHandle) {
    let Some(file) = location_file().filter(|file| file.is_file()) else {
        return;
    };
    let root = app.state::<AppState>().root.clone();
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FINISH_DELAY).await;
        let Ok(_job) = begin_job(&handle, DataJob::Finishing) else {
            tracing::info!("another data job runs; the old data folder waits for the next start");
            return;
        };
        let default = crate::state::platform_default_root();
        let outcome = tauri::async_runtime::spawn_blocking(move || {
            relocate::finish_move(&file, &root, default.as_deref())
        })
        .await;
        match outcome {
            Ok(Ok(relocate::FinishOutcome::Finished {
                deleted,
                left,
                retry,
            })) => {
                for old in deleted {
                    tracing::info!(old = %old.display(), "deleted a data folder left behind by a move");
                }
                for kept in left {
                    tracing::warn!(old = %kept.folder.display(), reason = kept.reason, retry, "kept a data folder left behind by a move");
                }
            }
            Ok(Ok(relocate::FinishOutcome::NotStartedThere)) => {
                tracing::info!("not started from the moved data folder; finishing the move later");
            }
            Ok(Ok(relocate::FinishOutcome::Nothing)) => {}
            Ok(Err(error)) => tracing::warn!(%error, "could not finish the data folder move"),
            Err(error) => tracing::warn!(%error, "data folder move cleanup did not run"),
        }
    });
}
