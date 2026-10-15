use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use pl_auth::FailureClass;
use pl_mc::install::{InstallOptions, InstallProgress, InstallStage, install_version};
use pl_mc::launch::{
    GameEvent, GameProcess, LaunchIdentity, QuickPlay, SpawnOptions, UserType, build_launch_command,
};
use pl_mc::loaders::LoaderKind;
use pl_net::{CancellationToken, VerifyMode};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use crate::commands::instances::{InstalledMarker, read_marker, write_marker};
use crate::error::{CmdError, CmdResult};
use crate::state::{AppState, RunningGame};

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProgressView {
    /// metadata | java | libraries | assets | client | natives | loader | pack
    pub stage: String,
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
}

/// Everything that happens between pressing OYNA and the game closing.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum LaunchEventView {
    Progress(ProgressView),
    /// The network is unreachable: continuing with installed files only.
    Offline,
    Launching,
    Started { pid: Option<u32> },
    WindowShown,
    #[serde(rename_all = "camelCase")]
    Exited { code: Option<i32>, runtime_secs: u64 },
    Crashed(CrashView),
    Error(CmdError),
}

/// Weight of each install stage in the overall bar (start, end).
fn stage_span(stage: InstallStage) -> (f64, f64) {
    match stage {
        InstallStage::Metadata => (0.0, 4.0),
        InstallStage::Java => (4.0, 22.0),
        InstallStage::Libraries => (22.0, 42.0),
        InstallStage::Assets => (42.0, 86.0),
        InstallStage::Client => (86.0, 93.0),
        InstallStage::Natives => (93.0, 99.0),
        InstallStage::Done => (100.0, 100.0),
    }
}

fn stage_key(stage: InstallStage) -> &'static str {
    match stage {
        InstallStage::Metadata => "metadata",
        InstallStage::Java => "java",
        InstallStage::Libraries => "libraries",
        InstallStage::Assets => "assets",
        InstallStage::Client => "client",
        InstallStage::Natives => "natives",
        InstallStage::Done => "natives",
    }
}

fn progress_view(p: &InstallProgress) -> ProgressView {
    let d = p.download;
    let fraction = if d.total_bytes > 0 {
        d.done_bytes as f64 / d.total_bytes as f64
    } else if d.total_files > 0 {
        d.done_files as f64 / d.total_files as f64
    } else {
        0.0
    };
    let (start, end) = stage_span(p.stage);
    let remaining = d.total_bytes.saturating_sub(d.done_bytes);
    ProgressView {
        stage: stage_key(p.stage).to_owned(),
        percent: (start + (end - start) * fraction.clamp(0.0, 1.0)).min(100.0),
        done_files: d.done_files as u64,
        total_files: d.total_files as u64,
        bytes_per_sec: d.bytes_per_sec,
        eta_secs: (d.bytes_per_sec > 0 && remaining > 0).then(|| remaining / d.bytes_per_sec),
    }
}

fn crash_view(d: &pl_mc::launch::CrashDiagnosis) -> CrashView {
    CrashView {
        title: d.title_tr.clone(),
        hint: d.hint_tr.clone(),
        actions: d
            .suggested_actions
            .iter()
            .map(|a| {
                serde_json::to_value(a)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned).or_else(|| Some(v.to_string())))
                    .unwrap_or_default()
            })
            .collect(),
        mods: d.mods.clone(),
        log_file: d.log_file.as_ref().map(|p| p.to_string_lossy().into_owned()),
        crash_report: d.crash_report.as_ref().map(|p| p.to_string_lossy().into_owned()),
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

#[tauri::command]
#[specta::specta]
pub async fn launch_instance(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    auto_join: bool,
    on_event: Channel<LaunchEventView>,
) -> CmdResult<()> {
    if state.running.lock().is_some() {
        return Err(CmdError::new(
            "OYUN-ACIK",
            "Oyun zaten açık. Önce mevcut oyunu kapat.",
            "",
        ));
    }
    let cancel = CancellationToken::new();
    if let Some(previous) = state.launch_cancel.lock().replace(cancel.clone()) {
        previous.cancel();
    }
    let result = prepare_and_spawn(&app, &state, &instance_id, auto_join, &on_event, &cancel).await;
    state.launch_cancel.lock().take();
    if let Err(error) = &result {
        let _ = on_event.send(LaunchEventView::Error(error.clone()));
    }
    result
}

async fn prepare_and_spawn(
    app: &AppHandle,
    state: &AppState,
    instance_id: &str,
    auto_join: bool,
    on_event: &Channel<LaunchEventView>,
    cancel: &CancellationToken,
) -> CmdResult<()> {
    let instance = state.instances.get(instance_id).await.map_err(CmdError::from)?;
    let instance_dir = state.instances.instance_dir(instance_id).map_err(CmdError::from)?;
    let game_dir = state.instances.game_dir(instance_id).map_err(CmdError::from)?;

    let marker = read_marker(&instance_dir);
    let first_install = marker.as_ref().is_none_or(|m| {
        m.mc_version != instance.mc_version
            || m.loader != instance.loader.kind
            || m.loader_version != instance.loader.version
    });

    // 1. Version metadata, falling back to offline mode when the network is gone.
    let mut offline = false;
    let manifest = match pl_mc::version::manifest::load_manifest(&state.mc, false).await {
        Ok(m) => Some(m),
        Err(e) if e.is_offline() => {
            offline = true;
            let _ = on_event.send(LaunchEventView::Offline);
            pl_mc::version::manifest::load_manifest(&state.mc, true).await.ok()
        }
        Err(e) => return Err(e.into()),
    };

    let version_id = match instance.loader.kind {
        LoaderKind::Vanilla => instance.mc_version.clone(),
        _ => {
            return Err(CmdError::new(
                "YUKLEYICI",
                "Mod yükleyici kurulumu bu sürümde henüz hazır değil.",
                format!("loader {:?} not wired yet", instance.loader.kind),
            ));
        }
    };

    let resolved = pl_mc::version::resolve::resolve_version(&state.mc, &version_id, manifest.as_ref())
        .await
        .map_err(CmdError::from)?;

    // 2. Install / verify files. Full hashing only on first install; size checks otherwise.
    let events = on_event.clone();
    let progress: pl_mc::install::InstallProgressFn = Arc::new(move |p: InstallProgress| {
        let _ = events.send(LaunchEventView::Progress(progress_view(&p)));
    });
    let installed = install_version(
        &state.mc,
        &resolved,
        InstallOptions {
            java: instance.java.clone(),
            offline,
            verify: if offline {
                VerifyMode::Exists
            } else if first_install {
                VerifyMode::Full
            } else {
                VerifyMode::Size
            },
            game_dir: Some(game_dir.clone()),
            concurrency: 16,
            cancel: cancel.clone(),
            progress: Some(progress),
        },
    )
    .await
    .map_err(CmdError::from)?;

    let _ = write_marker(
        &instance_dir,
        &InstalledMarker {
            version_id: version_id.clone(),
            mc_version: instance.mc_version.clone(),
            loader: instance.loader.kind,
            loader_version: instance.loader.version.clone(),
        },
    );

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
        Err(e) if e.class() == FailureClass::Offline => state
            .auth
            .prepare_offline_launch(account.id)
            .await
            .map_err(CmdError::from)?,
        Err(e) => return Err(e.into()),
    };
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

    // 4. Command.
    let version = app.package_info().version.to_string();
    let mut options = instance.to_launch_options(installed.java.path.clone(), game_dir.clone());
    options.launcher_name = "Projects Launcher".to_owned();
    options.launcher_version = version;
    options.quick_play = match (&instance.auto_join, auto_join) {
        (Some(server), true) => Some(QuickPlay::Multiplayer {
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
        ("INST_JAVA", installed.java.path.to_string_lossy().into_owned()),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v))
    .collect();
    if let Some(pre) = &instance.hooks.pre_launch {
        run_hook(pre, &hook_env, &game_dir).await.map_err(|detail| {
            CmdError::new("ON-KOMUT", "Başlatmadan önceki komut başarısız oldu.", detail)
        })?;
    }

    // 5. Spawn and supervise.
    let _ = on_event.send(LaunchEventView::Launching);
    let events = on_event.clone();
    let on_game: pl_mc::launch::GameEventFn = Arc::new(move |event| {
        let view = match event {
            GameEvent::Started { pid } => Some(LaunchEventView::Started { pid }),
            GameEvent::WindowLikelyShown => Some(LaunchEventView::WindowShown),
            GameEvent::Crashed(d) => Some(LaunchEventView::Crashed(crash_view(&d))),
            GameEvent::Exited { .. } | GameEvent::Line(_) => None,
        };
        if let Some(view) = view {
            let _ = events.send(view);
        }
    });
    let process = GameProcess::spawn(
        &command,
        SpawnOptions {
            instance_id: instance.id.clone(),
            logs_dir: state.root.join("logs"),
            java_major: Some(installed.java.info.major),
            required_java: resolved.java_version.as_ref().and_then(|j| j.major_version),
            max_memory_mb: Some(instance.memory.max_mb),
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

    let app = app.clone();
    let events = on_event.clone();
    let post_exit = instance.hooks.post_exit.clone();
    let id = instance.id.clone();
    tauri::async_runtime::spawn(async move {
        let exit = process.wait().await;
        let state = app.state::<AppState>();
        state.running.lock().take();
        let played = started.elapsed();
        if let Err(error) = state.instances.record_session(&id, played).await {
            tracing::warn!(%error, "could not record play time");
        }
        if let Some(post) = post_exit
            && let Err(detail) = run_hook(&post, &hook_env, &game_dir).await
        {
            tracing::warn!(%detail, "post-exit hook failed");
        }
        let _ = events.send(LaunchEventView::Exited {
            code: exit.code,
            runtime_secs: exit.runtime_secs,
        });
    });
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn launch_cancel(state: State<'_, AppState>) {
    if let Some(cancel) = state.launch_cancel.lock().take() {
        cancel.cancel();
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
