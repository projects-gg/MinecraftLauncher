use std::collections::BTreeMap;
use std::future::Future;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use pl_auth::FailureClass;
use pl_content::{PackManifest, PackState, SyncPolicy, plan_sync};
use pl_mc::install::context::BACKGROUND_CONCURRENCY;
use pl_mc::install::space::{self, GameFiles, Target};
use pl_mc::install::{
    InstallOptions, InstallProgress, InstallStage, LaunchBar, LaunchPart, McContext,
    install_version,
};
use pl_mc::instance::Instance;
use pl_mc::launch::{
    GameEvent, GameProcess, LaunchIdentity, QuickPlay, SpawnOptions, UserType, build_launch_command,
};
use pl_mc::loaders::LoaderKind;
use pl_net::{CancellationToken, VerifyMode};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::commands::instances::{InstalledMarker, read_marker, write_marker};
use crate::commands::modcenter::ModsNoticeView;
use crate::commands::{modhealth, modrinth, official, versions};
use crate::error::{CmdError, CmdResult};
use crate::state::{AppState, RunningGame};

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProgressView {
    /// metadata | java | libraries | assets | client | natives | loader | pack
    pub stage: String,
    /// The whole launch, 0–100, never going back (the stage's own figures are the fields below).
    pub percent: f64,
    pub done_files: u64,
    pub total_files: u64,
    pub bytes_per_sec: u64,
    pub eta_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CrashView {
    pub title: String,
    pub hint: String,
    pub actions: Vec<String>,
    pub mods: Vec<String>,
    pub log_file: Option<String>,
    pub crash_report: Option<String>,
    pub kind: pl_mc::launch::crash::CrashKind,
    pub instance_id: String,
    /// Mods the crash points at, strongest evidence first.
    pub suspects: Vec<crate::commands::modhealth::CrashSuspectView>,
    /// "Onar" has something to do.
    pub repairable: bool,
    /// The first strongly suspected mod that can be turned off ("Bu modu kapat").
    pub turn_off: Option<TurnOffView>,
    /// "Yeniden yükle" makes sense (a mod crash in a set with mods).
    pub reinstall: bool,
    /// Memory crashes: the maximum memory to retry with.
    pub memory: Option<MemoryAdviceView>,
    /// The JVM did not start because of the instance's own JVM arguments, and it has some:
    /// "Argümanları sıfırla ve yeniden dene" (`instance_set_jvm_args`).
    pub reset_jvm_args: bool,
}

/// A mod the crash dialog can turn off.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TurnOffView {
    pub file: String,
    pub name: String,
}

/// "Belleği artır/düşür ve yeniden dene".
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MemoryAdviceView {
    pub current_mb: u32,
    pub suggested_mb: u32,
    pub increase: bool,
}

/// Everything that happens between pressing OYNA and the game closing.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum LaunchEventView {
    Progress(ProgressView),
    /// The network is unreachable: continuing with installed files only.
    Offline,
    Launching,
    Started {
        pid: Option<u32>,
    },
    WindowShown,
    #[serde(rename_all = "camelCase")]
    Exited {
        code: Option<i32>,
        runtime_secs: u64,
    },
    Crashed(CrashView),
    Error(CmdError),
    /// `launch_cancel` stopped the launch before the game started (downloads and installs end,
    /// partial files go); the command bar simply returns to idle.
    Cancelled,
    /// The official pack moved to a new version during this launch.
    #[serde(rename_all = "camelCase")]
    PackUpdated {
        name: String,
        version: String,
        changelog: Option<String>,
        changed_files: u64,
    },
    /// Changes the player made in the mods folder were adopted, files could not be read, or the
    /// mods folder was installed again (a non-blocking notice; details in the mod center).
    ModsNotice(ModsNoticeView),
    /// The pack no longer offers this Minecraft version: the installed set was launched as it
    /// is. `recommended` is the version the pack recommends now.
    #[serde(rename_all = "camelCase")]
    PackVersionDropped {
        minecraft: String,
        recommended: Option<String>,
    },
    /// The mods would certainly stop the game: it was not started (the player chooses
    /// "Onar", "Yine de başlat" or "İptal").
    ModsBlocked(crate::commands::modhealth::ModCheckView),
    /// The instance's custom Java does not suit this game (too old for it or its mods, too new
    /// for a LaunchWrapper game, gone, not working): it was not started (the player chooses
    /// "Kur ve oyna", "Yine de dene" or cancels).
    JavaMismatch(crate::commands::java::JavaPromptView),
    /// Some mods will not work; the game starts anyway (shown once per set of warnings).
    ModsWarning(crate::commands::modhealth::ModCheckView),
    /// The session took screenshots (sent right before `Exited`). `recent` are the newest
    /// files (at most three, newest first) for thumbnails; their folder is allowed in the asset
    /// protocol.
    NewScreenshots {
        count: u32,
        recent: Vec<String>,
    },
}

/// Download preferences from Settings → İndirmeler, and how this launch starts.
#[derive(Debug, Clone, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LaunchSettings {
    /// Files downloaded in parallel (clamped to 4..=48).
    pub concurrent_downloads: u32,
    /// Hash every game file before each launch (slower; catches corrupted files).
    pub verify_files: bool,
    /// Start even when the mod check finds problems that stop the game ("Yine de başlat").
    #[serde(default)]
    pub ignore_mod_issues: bool,
    /// "Devam et": join this world or server right away (1.20+; ignored by older versions).
    /// Wins over the instance's auto-join server.
    #[serde(default)]
    pub quick_play: Option<QuickPlay>,
    /// The answer to the Java question of the previous attempt ("Kur ve oyna", "Yine de dene").
    #[serde(default)]
    pub java: Option<crate::commands::java::JavaChoice>,
}

/// One line of the running game's log, emitted as the `game-log` event.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GameLogEvent {
    pub instance_id: String,
    pub line: pl_mc::launch::LogLine,
}

fn stage_key(stage: InstallStage) -> &'static str {
    match stage {
        InstallStage::Loader => "loader",
        InstallStage::Metadata => "metadata",
        InstallStage::Java => "java",
        InstallStage::Libraries => "libraries",
        InstallStage::Assets => "assets",
        InstallStage::Client => "client",
        InstallStage::Natives => "natives",
        InstallStage::Done => "natives",
    }
}

/// A progress report for the command bar: the stage, its files, speed and time left, and the
/// launch's one overall percentage (`LaunchBar`: every part of the preparation fills its share
/// and the bar never moves back).
fn progress_view(p: &InstallProgress, bar: &LaunchBar) -> ProgressView {
    let d = p.download;
    let fraction = if d.total_bytes > 0 {
        d.done_bytes as f64 / d.total_bytes as f64
    } else if d.total_files > 0 {
        d.done_files as f64 / d.total_files as f64
    } else {
        0.0
    };
    let remaining = d.total_bytes.saturating_sub(d.done_bytes);
    // Loader progress counts steps (installer files and processors), not files to show.
    let files = if p.stage == InstallStage::Loader {
        (0, 0)
    } else {
        (d.done_files as u64, d.total_files as u64)
    };
    ProgressView {
        stage: stage_key(p.stage).to_owned(),
        percent: bar.stage_percent(p.stage, fraction),
        done_files: files.0,
        total_files: files.1,
        bytes_per_sec: d.bytes_per_sec,
        eta_secs: (d.bytes_per_sec > 0 && remaining > 0).then(|| remaining / d.bytes_per_sec),
    }
}

/// Run a user hook (pre-launch / post-exit) with MultiMC-compatible environment variables.
async fn run_hook(command: &str, env: &BTreeMap<String, String>, cwd: &Path) -> Result<(), String> {
    #[cfg(windows)]
    let mut cmd = {
        let mut c = tokio::process::Command::new("cmd");
        c.arg("/C").arg(command);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = tokio::process::Command::new("sh");
        c.arg("-c").arg(command);
        c
    };
    cmd.envs(env).current_dir(cwd).kill_on_drop(true);
    let status = tokio::time::timeout(Duration::from_secs(300), cmd.status())
        .await
        .map_err(|_| "komut 5 dakika içinde bitmedi".to_owned())?
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("komut {status} ile bitti"))
    }
}

/// A launch being prepared (`AppState::launch_cancel`): from OYNA until the game started or the
/// preparation ended. Cleanup and data moves wait while one exists; `launch_cancel` cancels it.
pub struct PreparingLaunch {
    id: u64,
    cancel: CancellationToken,
}

static NEXT_LAUNCH: AtomicU64 = AtomicU64::new(1);

/// How long a cancelled preparation step may take to stop by itself (removing its partial files)
/// before it is dropped.
const CANCEL_GRACE: Duration = Duration::from_millis(500);

fn cancelled() -> CmdError {
    CmdError::new("IPTAL", "İşlem iptal edildi.", "launch cancelled")
}

/// Stops the launch here when it was cancelled.
fn checkpoint(cancel: &CancellationToken) -> CmdResult<()> {
    if cancel.is_cancelled() {
        Err(cancelled())
    } else {
        Ok(())
    }
}

/// Runs a preparation step that is safe to stop half-way (metadata, downloads, installs, a hook
/// process). Once the launch is cancelled the step gets [`CANCEL_GRACE`] to stop on its own
/// (the crates check the token) and is dropped after that, so a cancel never waits for a slow
/// request or a step without a token.
async fn step<T>(
    cancel: &CancellationToken,
    future: impl Future<Output = CmdResult<T>>,
) -> CmdResult<T> {
    tokio::pin!(future);
    tokio::select! {
        biased;
        result = &mut future => result,
        () = cancel.cancelled() => {
            let since = Instant::now();
            let stopped = tokio::time::timeout(CANCEL_GRACE, &mut future).await.is_ok();
            tracing::info!(
                stopped,
                after_ms = since.elapsed().as_millis() as u64,
                "cancelled preparation step ended"
            );
            Err(cancelled())
        }
    }
}

#[tauri::command]
#[specta::specta]
pub async fn launch_instance(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    auto_join: bool,
    settings: LaunchSettings,
    on_event: Channel<LaunchEventView>,
) -> CmdResult<()> {
    if state.running.lock().is_some() {
        return Err(CmdError::new(
            "OYUN-ACIK",
            "Oyun zaten açık. Önce mevcut oyunu kapat.",
            "",
        ));
    }
    // A desktop shortcut or the tray can start a game while the data folder is being copied;
    // the game would write into the old folder, which goes after the restart.
    if crate::commands::data::move_running(&app) {
        return Err(CmdError::new(
            "TASI-SURUYOR",
            "Veri klasörü taşınırken oyun başlatılamaz. Taşıma bitince tekrar dene.",
            "the data folder is being moved",
        ));
    }
    let cancel = CancellationToken::new();
    let id = NEXT_LAUNCH.fetch_add(1, Ordering::Relaxed);
    let previous = state.launch_cancel.lock().replace(PreparingLaunch {
        id,
        cancel: cancel.clone(),
    });
    if let Some(previous) = previous {
        previous.cancel.cancel();
    }
    // Background preparation stops (what it finished, and its half-done files, are this launch's
    // head start) and a Java runtime update downloading in the background leaves the network to
    // the launch.
    crate::prepare::yield_to_launch(&app).await;
    crate::commands::java::yield_to_launch(&app).await;
    let started = Instant::now();
    let result = prepare_and_spawn(
        &app,
        &state,
        &instance_id,
        auto_join,
        settings,
        &on_event,
        &cancel,
    )
    .await;
    {
        // Only this launch's own entry: a newer launch may have taken its place.
        let mut current = state.launch_cancel.lock();
        if current.as_ref().is_some_and(|launch| launch.id == id) {
            current.take();
        }
    }
    // A launch that ended without a game lets background preparation go on after a while (a game
    // that started tells it when it closes).
    if state.running.lock().is_none() {
        crate::prepare::launch_ended(&app);
    }
    match result {
        // A cancel is not an error: no error message, the command bar returns to idle.
        Err(_) if cancel.is_cancelled() => {
            tracing::info!(instance = %instance_id, after_ms = started.elapsed().as_millis() as u64, "launch cancelled");
            let _ = on_event.send(LaunchEventView::Cancelled);
            Ok(())
        }
        Err(error) => {
            let error = out_of_room(error, &state, &instance_id);
            let _ = on_event.send(LaunchEventView::Error(error.clone()));
            Err(error)
        }
        Ok(()) => Ok(()),
    }
}

/// Launch failures a full disk can be behind: a write, a mod loader installer's step, a file
/// its installer should have made, Java, the pack's files.
const ROOM_SENSITIVE: [&str; 6] = ["DISK", "OYUN", "SUREC", "BULUNAMADI", "JAVA", "PAKET"];

/// A full disk shows up in many ways (a batch of downloads, a write deep in a mod loader's
/// installer, a step that only kept the text of what failed). A failure that says so, or one
/// that could come from it while the disk is (nearly) full now, tells the player to make room
/// ("Yer aç") instead.
fn out_of_room(error: CmdError, state: &AppState, instance_id: &str) -> CmdError {
    if error.code == CmdError::DISK_FULL || error.code == "IPTAL" {
        return error;
    }
    let said = pl_net::mentions_disk_full(&error.details);
    let full_now = || {
        let game_dir = state.instances.game_dir(instance_id).ok();
        [Some(state.mc.dirs.root().to_path_buf()), game_dir]
            .into_iter()
            .flatten()
            .any(|dir| free_space(&dir).is_some_and(space::nearly_full))
    };
    if said || (ROOM_SENSITIVE.contains(&error.code.as_str()) && full_now()) {
        tracing::warn!(code = %error.code, "launch failed on a full disk");
        CmdError::disk_full(error.details)
    } else {
        error
    }
}

/// Free space where a launch writes (or the end-to-end stand-in of debug builds).
pub(crate) fn free_space(path: &Path) -> Option<u64> {
    space::free_bytes_override().or_else(|| pl_data::usage::free_space(path))
}

/// Stops the launch before anything big is downloaded when a disk cannot hold what it still
/// needs: the game, its mod loader and Java in the data folder, the pack's files in the
/// instance folder (`DISK-DOLU`: how much is needed and how much is free; the command bar
/// offers "Yer aç"). A roomy disk costs one look at the version's file list; the files are only
/// checked when room is tight. A guess that fails (an unreadable file list, no answer from
/// Mojang) never stops a launch: a disk that fills up anyway is reported when it happens.
/// Background preparation (`ctx.background`) leaves [`space::BACKGROUND_RESERVE`] free and
/// guesses nothing: a disk or a download it cannot measure stops it.
async fn ensure_room(
    ctx: &McContext,
    instance: &Instance,
    loader_version: Option<&str>,
    manifest: Option<&pl_mc::version::VersionManifest>,
    pack: Option<&PackManifest>,
    game_dir: &Path,
    cancel: &CancellationToken,
) -> CmdResult<()> {
    let background = ctx.background;
    let check: fn(Target, Target) -> Option<space::Shortfall> = if background {
        space::background_shortfall
    } else {
        space::shortfall
    };
    let shared_free = free_space(ctx.dirs.root());
    let game_free = free_space(game_dir);
    if shared_free.is_none() && game_free.is_none() && !background {
        return Ok(());
    }
    let files = match GameFiles::for_launch(
        ctx,
        &instance.mc_version,
        instance.loader.kind,
        loader_version,
        &instance.java,
        manifest,
    )
    .await
    {
        Ok(files) => files,
        Err(error) if background => return Err(error.into()),
        Err(error) => {
            tracing::warn!(%error, "room check skipped: the version's files are unknown");
            return Ok(());
        }
    };
    let roomy = files.upper_bound(ctx).await.is_some_and(|upper| {
        check(
            Target {
                free: shared_free,
                missing: upper,
            },
            Target {
                free: game_free,
                missing: pack.map_or(0, pack_upper_bound),
            },
        )
        .is_none()
    });
    if roomy {
        return Ok(());
    }
    let shared = match files.missing(ctx, cancel).await {
        Ok(bytes) => bytes,
        Err(_) if cancel.is_cancelled() => return Err(cancelled()),
        Err(error) if background => return Err(error.into()),
        Err(error) => {
            tracing::warn!(%error, "room check skipped: the missing files are unknown");
            return Ok(());
        }
    };
    let game = match pack {
        Some(manifest) => pack_missing(manifest, game_dir).await,
        None => 0,
    };
    let short = check(
        Target {
            free: shared_free,
            missing: shared,
        },
        Target {
            free: game_free,
            missing: game,
        },
    );
    tracing::info!(
        shared,
        game,
        ?shared_free,
        ?game_free,
        short = short.is_some(),
        background,
        "room on the disk checked"
    );
    match short {
        Some(short) => Err(CmdError::disk_short(
            short.needed,
            short.free,
            format!(
                "not enough room: {} bytes needed for {} bytes to download, {} bytes free",
                short.needed, short.missing, short.free
            ),
        )),
        None => Ok(()),
    }
}

/// The pack's files as if none were installed (no file is read).
fn pack_upper_bound(manifest: &PackManifest) -> u64 {
    manifest.files.iter().map(|file| file.file_size).sum()
}

/// The pack files the coming sync downloads, planned as `official::sync_pack_files` plans them
/// (planning only reads). Nothing when the folder cannot be read: the sync reports that itself.
async fn pack_missing(manifest: &PackManifest, game_dir: &Path) -> u64 {
    let previous = PackState::load(game_dir).await.ok().flatten();
    let policy = SyncPolicy {
        repair: false,
        quick_check: true,
    };
    match plan_sync(manifest, game_dir, previous.as_ref(), None, &policy).await {
        Ok(plan) => plan.downloads.iter().map(|download| download.size).sum(),
        Err(error) => {
            tracing::debug!(%error, "pack files not counted for the room check");
            0
        }
    }
}

/// Where the steps shared with background preparation report: the launch's channel, or the
/// background preparation (which keeps the notices for the instance's next launch).
pub(crate) type EventSink = Arc<dyn Fn(LaunchEventView) + Send + Sync>;

/// Who gets the game files ready.
pub(crate) enum Readying<'a> {
    /// The player pressed OYNA: their download settings, the Java question, offline play. The
    /// launch holds the instance lock all along.
    Launch(&'a LaunchSettings),
    /// Background preparation (`crate::prepare`): gentle downloads, a disk reserve, no questions
    /// and nothing without the network. It takes the instance lock only to change the instance
    /// (its version, its install marker, its pack's files); the shared game files download
    /// without it, so the instance page and mod changes never wait for them.
    Background {
        lock: &'a Arc<tokio::sync::Mutex<()>>,
    },
}

/// The game files in place: what the launch command is built from.
pub(crate) struct ReadyGame {
    pub resolved: pl_mc::version::ResolvedVersion,
    pub installed: pl_mc::install::InstalledVersion,
}

/// Background preparation stopped: the network is gone (there is nothing to fetch then).
pub(crate) const OFFLINE: &str = "CEVRIMDISI";
/// Background preparation stopped: the instance's version changed while its files downloaded.
pub(crate) const CHANGED: &str = "KURULUM-DEGISTI";

/// The instance lock, unless the preparation is cancelled first.
async fn take_lock<'a>(
    lock: &'a tokio::sync::Mutex<()>,
    cancel: &CancellationToken,
) -> CmdResult<tokio::sync::MutexGuard<'a, ()>> {
    tokio::select! {
        _ = cancel.cancelled() => Err(cancelled()),
        guard = lock.lock() => Ok(guard),
    }
}

/// Steps 1 and 2 of a launch and the pack's files, shared with background preparation: version
/// metadata, the official pack's version, the mod loader, the game files with Java, the pack sync.
/// No account, no mod check, no process. A launch holds the instance lock; background preparation
/// takes it where it changes the instance. `Ok(None)`: the launch stopped for the Java question
/// (never in the background).
pub(crate) async fn ready_game_files(
    app: &AppHandle,
    state: &AppState,
    instance: &mut Instance,
    (instance_dir, game_dir): (&Path, &Path),
    readying: Readying<'_>,
    sink: &EventSink,
    cancel: &CancellationToken,
) -> CmdResult<Option<ReadyGame>> {
    let (settings, background_lock) = match readying {
        Readying::Launch(settings) => (Some(settings), None),
        Readying::Background { lock } => (None, Some(lock)),
    };
    let background_ctx;
    let ctx = match settings {
        Some(_) => &state.mc,
        None => {
            background_ctx = state.mc.for_background();
            &background_ctx
        }
    };
    let launcher_version = app.package_info().version.to_string();
    // Background: the instance lock while the instance may change (a switch finishing, the pack's
    // version followed).
    let mut background_guard = match background_lock {
        Some(lock) => Some(take_lock(lock, cancel).await?),
        None => None,
    };
    versions::ensure_switch_finished(state, instance).await?;
    checkpoint(cancel)?;

    // 1. Version metadata, falling back to offline mode when the network is gone.
    let mut offline = false;
    let fetched = step(cancel, async {
        Ok(pl_mc::version::manifest::load_manifest(ctx, false).await)
    })
    .await?;
    let manifest = match fetched {
        Ok(m) => Some(m),
        Err(e) if e.is_offline() => {
            if settings.is_none() {
                return Err(CmdError::new(OFFLINE, "", "the network is gone"));
            }
            offline = true;
            sink(LaunchEventView::Offline);
            pl_mc::version::manifest::load_manifest(ctx, true)
                .await
                .ok()
        }
        Err(e) => return Err(e.into()),
    };

    // Official pack: Minecraft and loader versions follow the family member of the instance's
    // version.
    let prepared = step(
        cancel,
        official::prepare_pack(state, instance, game_dir, offline, &launcher_version),
    )
    .await?;
    if prepared.content_changed {
        let _ = app.emit("content-changed", ());
    }
    if let Some(dropped) = prepared.dropped {
        sink(LaunchEventView::PackVersionDropped {
            minecraft: dropped.minecraft,
            recommended: dropped.recommended,
        });
    }
    let pack = prepared.manifest;
    // Background: the shared game files download without the instance lock (what the instance
    // follows now tells a change made meanwhile).
    drop(background_guard.take());
    let followed = background_lock.is_some().then(|| instance.clone());

    let marker = read_marker(instance_dir);
    let first_install = marker.as_ref().is_none_or(|m| {
        m.mc_version != instance.mc_version
            || m.loader != instance.loader.kind
            || m.loader_version != instance.loader.version
    });

    // One bar over the parts this launch runs, in order.
    let modded = instance.loader.kind != LoaderKind::Vanilla;
    let installs_java = settings
        .is_some_and(|s| s.java == Some(crate::commands::java::JavaChoice::Install))
        && matches!(instance.java, pl_mc::java::JavaSetting::Custom { .. });
    let parts: Vec<LaunchPart> = [
        installs_java.then_some(LaunchPart::Java),
        modded.then_some(LaunchPart::Loader),
        Some(LaunchPart::Game),
        pack.is_some().then_some(LaunchPart::Pack),
    ]
    .into_iter()
    .flatten()
    .collect();
    let bar = Arc::new(LaunchBar::new(&parts));

    let events = Arc::clone(sink);
    let progress_bar = Arc::clone(&bar);
    let progress: pl_mc::install::InstallProgressFn = Arc::new(move |p: InstallProgress| {
        events(LaunchEventView::Progress(progress_view(&p, &progress_bar)));
    });

    // A custom Java that does not suit this game is asked about before anything is downloaded;
    // "Kur ve oyna" switches the instance here and the launch goes on. The background leaves
    // the question to the launch (and the instance's Java as it is).
    if let Some(settings) = settings {
        let java_gate = step(
            cancel,
            crate::commands::java::prelaunch(
                app,
                state,
                instance,
                manifest.as_ref(),
                settings.java,
                &progress,
                cancel,
            ),
        )
        .await?;
        if let crate::commands::java::JavaGate::Ask(prompt) = java_gate {
            sink(LaunchEventView::JavaMismatch(prompt));
            return Ok(None);
        }
    }
    let concurrency = ctx.concurrency(settings.map_or(BACKGROUND_CONCURRENCY, |s| {
        s.concurrent_downloads.clamp(4, 48) as usize
    }));
    let verify_files = settings.is_some_and(|s| s.verify_files);
    let install_options = |verify: VerifyMode| InstallOptions {
        java: instance.java.clone(),
        offline,
        verify: if offline {
            VerifyMode::Exists
        } else if verify_files {
            VerifyMode::Full
        } else {
            verify
        },
        game_dir: Some(game_dir.to_path_buf()),
        concurrency,
        cancel: cancel.clone(),
        progress: Some(progress.clone()),
    };

    // The mod loader version to install: the instance's, else the newest stable one.
    let loader_version = match instance.loader.kind {
        LoaderKind::Vanilla => None,
        kind => Some(match &instance.loader.version {
            Some(v) if !v.trim().is_empty() => v.clone(),
            _ => step(cancel, async {
                pl_mc::loaders::list_loader_versions(ctx, kind, &instance.mc_version)
                    .await
                    .map_err(CmdError::from)
            })
            .await?
            .into_iter()
            .find(|v| v.stable)
            .map(|v| v.version)
            .ok_or_else(|| {
                CmdError::new(
                    "YUKLEYICI-YOK",
                    "Bu Minecraft sürümü için seçilen mod yükleyici bulunamadı.",
                    format!("{kind:?} for {}", instance.mc_version),
                )
            })?,
        }),
    };

    // Before anything big is downloaded: room on the disk for what this launch still needs.
    if !offline {
        step(
            cancel,
            ensure_room(
                ctx,
                instance,
                loader_version.as_deref(),
                manifest.as_ref(),
                pack.as_ref(),
                game_dir,
                cancel,
            ),
        )
        .await?;
    }

    // Mod loaders install their profile (and run Forge/NeoForge installer steps) first.
    if modded {
        bar.enter(LaunchPart::Loader);
    }
    let version_id = match &loader_version {
        None => instance.mc_version.clone(),
        Some(loader_version) => {
            step(cancel, async {
                pl_mc::loaders::install_loader(
                    ctx,
                    &instance.mc_version,
                    instance.loader.kind,
                    loader_version,
                    install_options(if first_install {
                        VerifyMode::Full
                    } else {
                        VerifyMode::Size
                    }),
                )
                .await
                .map_err(CmdError::from)
            })
            .await?
        }
    };

    bar.enter(LaunchPart::Game);
    let resolved = step(cancel, async {
        pl_mc::version::resolve::resolve_version(ctx, &version_id, manifest.as_ref())
            .await
            .map_err(CmdError::from)
    })
    .await?;

    // 2. Install / verify files. Full hashing only on first install; size checks otherwise
    //    (loader installs above already verified their files, so a size check suffices then).
    let verify = if first_install && instance.loader.kind == LoaderKind::Vanilla {
        VerifyMode::Full
    } else {
        VerifyMode::Size
    };
    let installed = step(cancel, async {
        install_version(ctx, &resolved, install_options(verify))
            .await
            .map_err(CmdError::from)
    })
    .await?;

    // Background: the instance lock again for its own files; an instance whose version changed
    // (or that went) meanwhile is left alone (the change brings its own preparation).
    if let (Some(lock), Some(followed)) = (background_lock, &followed) {
        background_guard = Some(take_lock(lock, cancel).await?);
        let unchanged = state.instances.get(&instance.id).await.is_ok_and(|now| {
            now.mc_version == followed.mc_version
                && now.loader == followed.loader
                && now.managed_pack == followed.managed_pack
        });
        if !unchanged {
            return Err(CmdError::new(CHANGED, "", "the instance changed meanwhile"));
        }
    }
    let _ = write_marker(
        instance_dir,
        &InstalledMarker {
            version_id: version_id.clone(),
            mc_version: instance.mc_version.clone(),
            loader: instance.loader.kind,
            loader_version: instance.loader.version.clone(),
        },
    );

    // The pack's own files (mods, configs, resource packs) once the game itself is in place.
    // The sync stops on the token by itself (it is not dropped half-way: it moves files).
    checkpoint(cancel)?;
    if let Some(manifest) = &pack {
        bar.enter(LaunchPart::Pack);
        official::sync_pack_files(
            state,
            instance,
            manifest,
            game_dir,
            offline,
            official::PackSync {
                events: sink,
                bar: &bar,
                ctx,
            },
            cancel,
        )
        .await?;
    }
    drop(background_guard);
    Ok(Some(ReadyGame {
        resolved,
        installed,
    }))
}

async fn prepare_and_spawn(
    app: &AppHandle,
    state: &AppState,
    instance_id: &str,
    auto_join: bool,
    settings: LaunchSettings,
    on_event: &Channel<LaunchEventView>,
    cancel: &CancellationToken,
) -> CmdResult<()> {
    let mut instance = state
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
    let launcher_version = app.package_info().version.to_string();

    // Mod center changes, add-on installs and version switches of this instance wait until the
    // game files are prepared (and the other way round).
    let lock = modrinth::instance_lock(app, instance_id);
    let preparing = tokio::select! {
        _ = cancel.cancelled() => return Err(cancelled()),
        guard = lock.lock() => guard,
    };
    let sink: EventSink = {
        let channel = on_event.clone();
        Arc::new(move |event| {
            let _ = channel.send(event);
        })
    };
    let Some(ReadyGame {
        resolved,
        installed,
    }) = ready_game_files(
        app,
        state,
        &mut instance,
        (instance_dir.as_path(), game_dir.as_path()),
        Readying::Launch(&settings),
        &sink,
        cancel,
    )
    .await?
    else {
        return Ok(());
    };
    // What a background preparation of this instance found on its way (a pack update, mod
    // changes it adopted, a version the pack dropped) is told now, as this launch would have.
    for notice in crate::prepare::take_notices(app, instance_id) {
        let _ = on_event.send(notice);
    }
    // Instances made before every instance got the Projects server: an empty list gets it once.
    crate::commands::instances::seed_default_server_once(state, &mut instance).await;
    // First launch without an options.txt (the player's, a modpack's or the pack's): Turkish,
    // GUI scale 2 and the client's data version.
    match pl_mc::instance::seed_options(&game_dir, &instance.mc_version, &installed.client_jar)
        .await
    {
        Ok(pl_mc::instance::OptionsSeed::Written { data_version }) => {
            tracing::info!(?data_version, "first-launch options written");
        }
        Ok(pl_mc::instance::OptionsSeed::Skipped) => {
            tracing::info!("client data version unknown; first-launch options left to the game");
        }
        Ok(pl_mc::instance::OptionsSeed::Exists) => {}
        Err(error) => tracing::warn!(%error, "could not write first-launch options"),
    }
    // The mods are checked with the Java that will run, after the pack sync: problems that
    // would certainly stop the game stop the launch here instead.
    checkpoint(cancel)?;
    let java_major = Some(installed.java.info.major);
    let health = modhealth::prelaunch(
        app,
        state,
        &instance,
        java_major,
        settings.ignore_mod_issues,
    )
    .await;
    checkpoint(cancel)?;
    match health {
        modhealth::Prelaunch::Blocked(check) => {
            let _ = on_event.send(LaunchEventView::ModsBlocked(check));
            return Ok(());
        }
        modhealth::Prelaunch::Warned(check) => {
            let _ = on_event.send(LaunchEventView::ModsWarning(check));
        }
        modhealth::Prelaunch::Clear => {}
    }
    // The instance lock (`preparing`) is held until the game counts as running: a version switch
    // or a mod change asked for meanwhile waits for it and is then refused, instead of moving
    // `mods/` and `config/` under a game about to start with the old version.

    // 3. Account: refresh the Microsoft session; offline play keeps working with the cached one.
    let account = state.auth.active_account().ok_or_else(|| {
        CmdError::new(
            "HESAP-YOK",
            "Oynamak için önce bir hesap ekle (Ayarlar > Hesaplar).",
            "no active account",
        )
    })?;
    let identity = match state.auth.prepare_launch(account.id, cancel).await {
        Ok(identity) => identity,
        Err(_) if cancel.is_cancelled() => return Err(cancelled()),
        Err(e) if e.class() == FailureClass::Offline => state
            .auth
            .prepare_offline_launch(account.id)
            .await
            .map_err(CmdError::from)?,
        Err(e) => return Err(e.into()),
    };
    checkpoint(cancel)?;
    let identity = LaunchIdentity {
        name: identity.name,
        uuid: identity.uuid,
        access_token: identity.access_token,
        user_type: match identity.user_type {
            pl_auth::UserType::Msa => UserType::Msa,
            pl_auth::UserType::Legacy => UserType::Legacy,
        },
        xuid: identity.xuid,
        client_id: identity.client_id,
    };

    // 4. Command. "Otomatik" memory follows the machine and the mods enabled right now (never
    //    below what the pack recommends).
    let memory = instance.launch_memory(
        pl_mc::instance::system_memory_mb(),
        pl_mc::mods::count_enabled_mods(&game_dir.join("mods")),
    );
    tracing::info!(
        max_mb = memory.max_mb,
        auto = instance.memory_auto,
        "game memory"
    );
    let mut options = instance.to_launch_options(installed.java.path.clone(), game_dir.clone());
    options.min_memory_mb = Some(memory.min_mb);
    options.max_memory_mb = Some(memory.max_mb);
    options.launcher_name = "Projects Launcher".to_owned();
    options.launcher_version = launcher_version;
    options.quick_play = match (&settings.quick_play, &instance.auto_join, auto_join) {
        // "Devam et": only versions with quick play can join a world directly.
        (Some(target), _, _) if resolved.supports_quick_play() => Some(target.clone()),
        (_, Some(server), true) => Some(QuickPlay::Multiplayer {
            host: server.host.clone(),
            port: server.port,
        }),
        _ => None,
    };
    let command = build_launch_command(&state.mc.dirs, &resolved, &installed, &identity, &options)
        .map_err(CmdError::from)?;
    tracing::info!(command = %command.display_redacted(), "launching");

    let hook_env: BTreeMap<String, String> = [
        ("INST_NAME", instance.name.clone()),
        ("INST_ID", instance.id.clone()),
        ("INST_DIR", instance_dir.to_string_lossy().into_owned()),
        ("INST_MC_DIR", game_dir.to_string_lossy().into_owned()),
        (
            "INST_JAVA",
            installed.java.path.to_string_lossy().into_owned(),
        ),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v))
    .collect();
    if let Some(pre) = &instance.hooks.pre_launch {
        // Dropping the hook on a cancel ends its process (`kill_on_drop`).
        step(cancel, async {
            run_hook(pre, &hook_env, &game_dir).await.map_err(|detail| {
                CmdError::new(
                    "ON-KOMUT",
                    "Başlatmadan önceki komut başarısız oldu.",
                    detail,
                )
            })
        })
        .await?;
    }

    // 5. Spawn and supervise. The last moment a cancel can still stop the game.
    checkpoint(cancel)?;
    let _ = on_event.send(LaunchEventView::Launching);
    let events = on_event.clone();
    let log_app = app.clone();
    let log_instance = instance.id.clone();
    // Friends see the game: started, the server read from the log, exited.
    let game_presence =
        crate::commands::social::GamePresence::new(&state.social, options.quick_play.as_ref());
    let on_game: pl_mc::launch::GameEventFn = Arc::new(move |event| {
        game_presence.on_event(&event);
        let view = match event {
            GameEvent::Started { pid } => Some(LaunchEventView::Started { pid }),
            GameEvent::WindowLikelyShown => Some(LaunchEventView::WindowShown),
            // Sent after the exit, once the suspects are found in the mods folder.
            GameEvent::Crashed(_) => None,
            GameEvent::Line(line) => {
                // The instance page's log tab follows the game live.
                let _ = log_app.emit(
                    "game-log",
                    GameLogEvent {
                        instance_id: log_instance.clone(),
                        line,
                    },
                );
                None
            }
            GameEvent::Exited { .. } => None,
        };
        if let Some(view) = view {
            let _ = events.send(view);
        }
    });
    // Screenshots already there, so the ones this session takes can be announced after it.
    let screenshots_before = pl_mc::gamefiles::ScreenshotSnapshot::take(&game_dir)
        .await
        .map_err(|error| tracing::debug!(%error, "screenshots not listed before the game"))
        .ok();
    let process = GameProcess::spawn(
        &command,
        SpawnOptions {
            instance_id: instance.id.clone(),
            logs_dir: state.root.join("logs"),
            java_major: Some(installed.java.info.major),
            required_java: resolved.java_version.as_ref().and_then(|j| j.major_version),
            max_memory_mb: Some(memory.max_mb),
        },
        on_game,
    )
    .await
    .map_err(CmdError::from)?;
    let process = Arc::new(process);
    let started = Instant::now();
    *state.running.lock() = Some(RunningGame {
        instance_id: instance.id.clone(),
        process: Arc::clone(&process),
    });
    drop(preparing);
    // The set of this version was played now (cleanup suggestions use it).
    let played = pl_content::SetInfo {
        minecraft: instance.mc_version.clone(),
        loader: instance
            .loader
            .kind
            .modrinth_name()
            .unwrap_or_default()
            .to_owned(),
        loader_version: instance.loader.version.clone().unwrap_or_default(),
        pack_id: instance
            .managed_pack
            .as_ref()
            .map(|managed| managed.pack_id.clone())
            .unwrap_or_default(),
        ..pl_content::SetInfo::default()
    };
    if let Err(error) = pl_content::VersionSets::new(&game_dir)
        .record_played(played, jiff::Timestamp::now())
        .await
    {
        tracing::debug!(%error, "could not record when the set was played");
    }

    let app = app.clone();
    let events = on_event.clone();
    let post_exit = instance.hooks.post_exit.clone();
    let id = instance.id.clone();
    // Memory advice after a crash starts from what the game really had.
    let mut crashed_instance = instance.clone();
    crashed_instance.memory = memory;
    tauri::async_runtime::spawn(async move {
        let exit = process.wait().await;
        // A page suspended while the game ran must bring the window back and tell about a
        // crash: it runs again before anything below reaches it (crate::webview_rest).
        crate::webview_rest::wake(&app, "the game exited");
        let state = app.state::<AppState>();
        state.running.lock().take();
        // Java runtime updates waiting for the game can take their place now.
        crate::commands::java::game_exited(&app);
        // Background preparation looks again once the player is done with the launcher.
        crate::prepare::game_exited(&app);
        if let Some(diagnosis) = exit.crash.clone() {
            let view =
                modhealth::crash_view(&app, &state, &crashed_instance, java_major, diagnosis).await;
            let _ = events.send(LaunchEventView::Crashed(view));
        }
        let played = started.elapsed();
        if let Err(error) = state.instances.record_session(&id, played).await {
            tracing::warn!(%error, "could not record play time");
        }
        if let Some(post) = post_exit
            && let Err(detail) = run_hook(&post, &hook_env, &game_dir).await
        {
            tracing::warn!(%detail, "post-exit hook failed");
        }
        // "N yeni ekran görüntüsü": the gallery opens from the notice.
        if let Some(before) = &screenshots_before
            && let Ok(added) = before.new_since(&game_dir).await
            && !added.is_empty()
        {
            if let Err(error) = app
                .asset_protocol_scope()
                .allow_directory(game_dir.join("screenshots"), false)
            {
                tracing::debug!(%error, "screenshots folder not allowed for thumbnails");
            }
            let _ = events.send(LaunchEventView::NewScreenshots {
                count: u32::try_from(added.len()).unwrap_or(u32::MAX),
                recent: added
                    .iter()
                    .take(3)
                    .map(|shot| shot.path.to_string_lossy().into_owned())
                    .collect(),
            });
        }
        let _ = events.send(LaunchEventView::Exited {
            code: exit.code,
            runtime_secs: exit.runtime_secs,
        });
    });
    Ok(())
}

/// Stops the launch being prepared: downloads and installs end within a second and the launch
/// reports `Cancelled`. Its entry stays until it has really stopped (cleanup and data moves keep
/// waiting until then).
#[tauri::command]
#[specta::specta]
pub fn launch_cancel(state: State<'_, AppState>) {
    if let Some(launch) = state.launch_cancel.lock().as_ref() {
        tracing::info!("launch cancel requested");
        launch.cancel.cancel();
    }
}

#[tauri::command]
#[specta::specta]
pub fn game_kill(state: State<'_, AppState>) {
    if let Some(running) = state.running.lock().as_ref() {
        running.process.kill();
    }
}

/// Instance id of the running game, if any (used after a UI reload).
#[tauri::command]
#[specta::specta]
pub fn game_running(state: State<'_, AppState>) -> Option<String> {
    state.running.lock().as_ref().map(|r| r.instance_id.clone())
}
