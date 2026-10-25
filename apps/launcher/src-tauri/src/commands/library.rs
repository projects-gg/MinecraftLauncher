//! The instance page's logs, worlds and screenshots tabs, and library groups: captured game
//! logs (viewer, mclo.gs sharing), world backups and restores, the launcher trash, the
//! screenshot gallery (clipboard, system viewer), instance groups and the room an instance's
//! folder takes.
//!
//! Worlds are never touched while the game of their instance runs; long jobs (backup, restore)
//! stream progress over a channel and can be cancelled with `library_cancel`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Mutex;
use pl_lang::pick;
use pl_mc::gamefiles::{
    self, BackupProgress, BackupProgressFn, LauncherTrash, LogSource, RestoreMode, TrashEntry,
    TrashKind,
};
use pl_mc::instance::usage;
use pl_mc::launch::LogLine;
use pl_net::CancellationToken;
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_opener::OpenerExt;

use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

// ---------------------------------------------------------------------------------------------
// Jobs and shared helpers

/// Cancellation tokens of running backups/restores, and one lock per instance so world
/// operations of an instance run one after another.
#[derive(Default)]
pub struct LibraryJobs {
    tokens: Mutex<HashMap<String, CancellationToken>>,
    locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

/// The job registry, created on first use (no setup code needed).
fn library_jobs(app: &AppHandle) -> tauri::State<'_, LibraryJobs> {
    if app.try_state::<LibraryJobs>().is_none() {
        app.manage(LibraryJobs::default());
    }
    app.state::<LibraryJobs>()
}

/// A registered job; its token is unregistered on drop.
struct JobGuard {
    app: AppHandle,
    id: String,
    token: CancellationToken,
}

impl Drop for JobGuard {
    fn drop(&mut self) {
        library_jobs(&self.app).tokens.lock().remove(&self.id);
        crate::commands::cleanup::status_changed(&self.app);
    }
}

fn start_job(app: &AppHandle, job_id: &str) -> JobGuard {
    let token = CancellationToken::new();
    if let Some(previous) = library_jobs(app)
        .tokens
        .lock()
        .insert(job_id.to_owned(), token.clone())
    {
        previous.cancel();
    }
    crate::commands::cleanup::status_changed(app);
    JobGuard {
        app: app.clone(),
        id: job_id.to_owned(),
        token,
    }
}

/// Whether a backup or restore is running (a data folder move or cleanup waits for it).
pub(crate) fn jobs_running(app: &AppHandle) -> bool {
    !library_jobs(app).tokens.lock().is_empty()
}

pub(crate) fn world_lock(app: &AppHandle, instance_id: &str) -> Arc<tokio::sync::Mutex<()>> {
    library_jobs(app)
        .locks
        .lock()
        .entry(instance_id.to_owned())
        .or_default()
        .clone()
}

/// Cancel a running backup or restore.
#[tauri::command]
#[specta::specta]
pub fn library_cancel(app: AppHandle, job_id: String) {
    if let Some(token) = library_jobs(&app).tokens.lock().get(&job_id) {
        token.cancel();
    }
}

/// Map a crate error to a command error with a specific message (cancellations stay "IPTAL").
fn fail(code: &'static str, message: &'static str) -> impl FnOnce(pl_mc::Error) -> CmdError {
    move |error| {
        if error.is_cancelled() {
            CmdError::from(error)
        } else {
            CmdError::new(code, message, error.to_string())
        }
    }
}

/// A single file or folder name from the UI (no separators, no `..`).
fn plain_name(name: &str) -> CmdResult<&str> {
    pl_mc::paths::safe_name(name).map_err(|e| {
        CmdError::new(
            "AD",
            pick("Bu dosya bulunamadı.", "This file wasn't found."),
            e.to_string(),
        )
    })
}

fn game_dir(state: &AppState, instance_id: &str) -> CmdResult<PathBuf> {
    state
        .instances
        .game_dir(instance_id)
        .map_err(CmdError::from)
}

pub(crate) fn launcher_trash(state: &AppState) -> LauncherTrash {
    LauncherTrash::new(state.root.join("trash"))
}

fn is_running(state: &AppState, instance_id: &str) -> bool {
    state
        .running
        .lock()
        .as_ref()
        .is_some_and(|r| r.instance_id == instance_id)
}

/// Worlds are never changed while their game runs (it keeps them open and would overwrite or
/// corrupt them).
fn ensure_game_closed(state: &AppState, instance_id: &str) -> CmdResult<()> {
    if is_running(state, instance_id) {
        return Err(CmdError::new(
            "OYUN-ACIK",
            pick(
                "Oyun açıkken dünyalarda değişiklik yapılamaz. Önce oyunu kapat.",
                "You can't change worlds while the game is open. Close the game first.",
            ),
            format!("the game of instance {instance_id} is running"),
        ));
    }
    Ok(())
}

fn iso(ms: Option<i64>) -> Option<String> {
    ms.and_then(|ms| jiff::Timestamp::from_millisecond(ms).ok())
        .map(|t| t.to_string())
}

fn png_data_url(bytes: &[u8]) -> String {
    use base64::Engine;
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

fn folder_error(error: impl std::fmt::Display) -> CmdError {
    CmdError::new(
        "KLASOR",
        pick("Klasör açılamadı.", "Couldn't open the folder."),
        error.to_string(),
    )
}

// ---------------------------------------------------------------------------------------------
// Game logs

/// One log of an instance: a session the launcher captured, or one of the game's own files.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GameLogSessionView {
    pub file_name: String,
    pub source: LogSource,
    /// When the session started (ISO 8601); for the game's dated archives, the day's start.
    pub started_at: Option<String>,
    /// Last write to the log (the end of a finished session).
    pub updated_at: Option<String>,
    pub size_bytes: u64,
    /// The running game is writing this log right now (captured sessions only).
    pub live: bool,
}

/// A session log, parsed and with secrets masked.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GameLogView {
    pub lines: Vec<LogLine>,
    /// Only the end of a very large log was read.
    pub truncated: bool,
    pub size_bytes: u64,
}

/// A log published on mclo.gs.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SharedLogView {
    pub id: String,
    pub url: String,
}

/// File name of the log the running game of `instance_id` writes, if it runs.
fn live_log_name(state: &AppState, instance_id: &str) -> Option<String> {
    let process = state
        .running
        .lock()
        .as_ref()
        .filter(|r| r.instance_id == instance_id)
        .map(|r| Arc::clone(&r.process))?;
    let path = process.log_file()?;
    Some(path.file_name()?.to_string_lossy().into_owned())
}

/// Path of one of the instance's logs: a captured `game-<instance>-….log` in the launcher's
/// `logs/`, or a `.log`/`.log.gz` file in the game's own `logs/`.
fn log_path(
    state: &AppState,
    instance_id: &str,
    file_name: &str,
    source: Option<LogSource>,
) -> CmdResult<PathBuf> {
    let name = plain_name(file_name)?;
    let missing = || {
        CmdError::new(
            "KAYIT-YOK",
            pick("Bu kayıt bulunamadı.", "This log wasn't found."),
            format!("{name} is not a log of {instance_id}"),
        )
    };
    match source.unwrap_or_default() {
        LogSource::Launcher => {
            let prefix = format!("game-{}-", gamefiles::game_log_id(instance_id));
            if !name.starts_with(&prefix) || !name.ends_with(".log") {
                return Err(missing());
            }
            Ok(state.root.join("logs").join(name))
        }
        LogSource::Game => {
            if !gamefiles::is_game_log_name(name) {
                return Err(missing());
            }
            Ok(game_dir(state, instance_id)?.join("logs").join(name))
        }
    }
}

/// The instance's logs, newest first: the sessions the launcher captured, then the game's own
/// files (`latest.log`, `debug.log` and their archives). The game's folder is optional: when it
/// cannot be read, the captured sessions still come.
#[tauri::command]
#[specta::specta]
pub async fn game_logs(
    state: State<'_, AppState>,
    instance_id: String,
) -> CmdResult<Vec<GameLogSessionView>> {
    let live = live_log_name(&state, &instance_id);
    let files = gamefiles::list_game_logs(&state.root.join("logs"), &instance_id)
        .await
        .map_err(fail(
            "KAYIT-LISTE",
            pick("Oyun kayıtları okunamadı.", "Couldn't read the game logs."),
        ))?;
    let own = match game_dir(&state, &instance_id) {
        Ok(dir) => gamefiles::list_game_folder_logs(&dir)
            .await
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "could not list the game's own logs");
                Vec::new()
            }),
        Err(_) => Vec::new(),
    };
    let view = |f: gamefiles::GameLogFile, source: LogSource| GameLogSessionView {
        live: source == LogSource::Launcher && live.as_deref() == Some(f.file_name.as_str()),
        source,
        started_at: iso(f.started_ms),
        updated_at: iso(f.modified_ms),
        size_bytes: f.size,
        file_name: f.file_name,
    };
    Ok(files
        .into_iter()
        .map(|f| view(f, LogSource::Launcher))
        .chain(own.into_iter().map(|f| view(f, LogSource::Game)))
        .collect())
}

/// One log for the viewer (access tokens and similar secrets masked; archives inflated).
#[tauri::command]
#[specta::specta]
pub async fn game_log_read(
    state: State<'_, AppState>,
    instance_id: String,
    file_name: String,
    source: Option<LogSource>,
) -> CmdResult<GameLogView> {
    let path = log_path(&state, &instance_id, &file_name, source)?;
    let content = gamefiles::read_game_log(&path, gamefiles::MAX_READ_BYTES)
        .await
        .map_err(fail(
            "KAYIT-OKU",
            pick("Kayıt dosyası okunamadı.", "Couldn't read the log file."),
        ))?;
    Ok(GameLogView {
        lines: content.lines,
        truncated: content.truncated,
        size_bytes: content.size,
    })
}

/// Upload a session's log to mclo.gs (masked and shortened to its limits) and return the link.
/// The UI asks first: the log becomes public.
#[tauri::command]
#[specta::specta]
pub async fn game_log_share(
    state: State<'_, AppState>,
    instance_id: String,
    file_name: String,
    source: Option<LogSource>,
) -> CmdResult<SharedLogView> {
    let path = log_path(&state, &instance_id, &file_name, source)?;
    let content = gamefiles::read_log_for_upload(&path).await.map_err(fail(
        "KAYIT-OKU",
        pick("Kayıt dosyası okunamadı.", "Couldn't read the log file."),
    ))?;
    if content.trim().is_empty() {
        return Err(CmdError::new(
            "KAYIT-BOS",
            pick(
                "Bu kayıt boş, paylaşılacak bir şey yok.",
                "This log is empty; there's nothing to share.",
            ),
            file_name,
        ));
    }
    let shared = gamefiles::upload_to_mclogs(state.http.client(), gamefiles::MCLOGS_API, &content)
        .await
        .map_err(|error| {
            if error.is_offline() {
                CmdError::new(
                    "PAYLAS-AG",
                    pick(
                        "mclo.gs'e ulaşılamadı. İnternet bağlantını kontrol edip tekrar dene.",
                        "Couldn't reach mclo.gs. Check your internet connection and try again.",
                    ),
                    error.to_string(),
                )
            } else {
                CmdError::new(
                    "PAYLAS",
                    pick(
                        "Kayıt paylaşılamadı. Biraz sonra tekrar dene.",
                        "Couldn't share the log. Try again in a bit.",
                    ),
                    error.to_string(),
                )
            }
        })?;
    tracing::info!(id = %shared.id, "game log shared on mclo.gs");
    Ok(SharedLogView {
        id: shared.id,
        url: shared.url,
    })
}

/// Show a log file in the file manager (or its logs folder).
#[tauri::command]
#[specta::specta]
pub async fn game_log_reveal(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    file_name: Option<String>,
    source: Option<LogSource>,
) -> CmdResult<()> {
    let dir = match source.unwrap_or_default() {
        LogSource::Launcher => state.root.join("logs"),
        LogSource::Game => game_dir(&state, &instance_id)?.join("logs"),
    };
    if let Some(name) = file_name {
        let path = log_path(&state, &instance_id, &name, source)?;
        if path.is_file() {
            return app.opener().reveal_item_in_dir(&path).map_err(folder_error);
        }
    }
    std::fs::create_dir_all(&dir).map_err(folder_error)?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(folder_error)
}

// ---------------------------------------------------------------------------------------------
// Worlds and backups

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorldDetailView {
    pub folder: String,
    pub name: String,
    /// survival | creative | adventure | spectator | unknown
    pub game_mode: String,
    pub hardcore: bool,
    pub cheats: bool,
    pub last_played_at: Option<String>,
    pub size_bytes: u64,
    /// `icon.png` as a data URL.
    pub icon_url: Option<String>,
    /// Game version that last saved the world.
    pub version: Option<String>,
    /// False when `level.dat` could not be read (the other fields are then guesses).
    pub readable: bool,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorldBackupView {
    pub file_name: String,
    pub size_bytes: u64,
    pub created_at: Option<String>,
    /// Folder name of the backed-up world.
    pub world_folder: Option<String>,
    /// The world's own name.
    pub world_name: Option<String>,
    /// False when the archive does not hold a world.
    pub valid: bool,
    /// Why the launcher made it by itself ("Arkadaşlarına açılmadan önce"); none for the
    /// player's own backups and the game's.
    pub reason: Option<gamefiles::BackupReason>,
}

/// Something in the launcher trash.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TrashItemView {
    pub id: String,
    pub kind: TrashKind,
    pub name: String,
    pub file_name: String,
    pub deleted_at: String,
    /// When it is deleted for good.
    pub expires_at: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RestoredWorldView {
    /// Folder in `saves/` the world now lives in.
    pub folder: String,
    /// The world it replaced, now in the launcher trash.
    pub replaced: Option<TrashItemView>,
}

fn trash_view(entry: TrashEntry) -> TrashItemView {
    let retention = i64::try_from(gamefiles::TRASH_RETENTION.as_millis()).unwrap_or(i64::MAX);
    TrashItemView {
        deleted_at: iso(Some(entry.deleted_at_ms)).unwrap_or_default(),
        expires_at: iso(Some(entry.deleted_at_ms.saturating_add(retention))).unwrap_or_default(),
        id: entry.id,
        kind: entry.kind,
        name: entry.name,
        file_name: entry.file_name,
        size_bytes: entry.size,
    }
}

fn backup_view(b: gamefiles::WorldBackupInfo) -> WorldBackupView {
    WorldBackupView {
        file_name: b.file_name,
        size_bytes: b.size,
        created_at: iso(b.created_ms),
        world_folder: b.world_folder,
        world_name: b.world_name,
        valid: b.valid,
        reason: b.reason,
    }
}

/// Apply the trash limits in the background (best effort): expired items go, and the oldest
/// while the trash is over its size cap (`commands::cleanup::trash_limits`).
pub(crate) fn purge_trash_later(trash: LauncherTrash) {
    tauri::async_runtime::spawn(async move {
        let limits = crate::commands::cleanup::trash_limits(trash.dir());
        if let Err(error) = trash.purge(limits).await {
            tracing::warn!(%error, "could not purge the launcher trash");
        }
    });
}

/// Worlds of the instance (`saves/`), most recently played first.
#[tauri::command]
#[specta::specta]
pub async fn worlds_list(
    state: State<'_, AppState>,
    instance_id: String,
) -> CmdResult<Vec<WorldDetailView>> {
    let game_dir = game_dir(&state, &instance_id)?;
    let worlds = gamefiles::list_worlds(&game_dir).await.map_err(fail(
        "DUNYA-LISTE",
        pick("Dünyalar okunamadı.", "Couldn't read the worlds."),
    ))?;
    Ok(worlds
        .into_iter()
        .map(|w| WorldDetailView {
            game_mode: match w.game_mode {
                gamefiles::WorldGameMode::Survival => "survival",
                gamefiles::WorldGameMode::Creative => "creative",
                gamefiles::WorldGameMode::Adventure => "adventure",
                gamefiles::WorldGameMode::Spectator => "spectator",
                gamefiles::WorldGameMode::Unknown => "unknown",
            }
            .to_owned(),
            hardcore: w.hardcore,
            cheats: w.allow_commands,
            last_played_at: iso(w.last_played_ms),
            size_bytes: w.size,
            icon_url: w.icon_png.as_deref().map(png_data_url),
            version: w.version_name,
            readable: w.readable,
            name: w.name,
            folder: w.folder_name,
        })
        .collect())
}

/// Backups in the instance's `backups/` folder, newest first.
#[tauri::command]
#[specta::specta]
pub async fn world_backups(
    state: State<'_, AppState>,
    instance_id: String,
) -> CmdResult<Vec<WorldBackupView>> {
    let game_dir = game_dir(&state, &instance_id)?;
    let backups = gamefiles::list_world_backups(&game_dir)
        .await
        .map_err(fail(
            "YEDEK-LISTE",
            pick("Yedekler okunamadı.", "Couldn't read the backups."),
        ))?;
    Ok(backups.into_iter().map(backup_view).collect())
}

/// Zip a world into `backups/<time>_<folder>.zip` (the game's own backup layout).
#[tauri::command]
#[specta::specta]
pub async fn world_backup(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    folder: String,
    job_id: String,
    on_progress: Channel<BackupProgress>,
) -> CmdResult<WorldBackupView> {
    ensure_game_closed(&state, &instance_id)?;
    let folder = plain_name(&folder)?.to_owned();
    let game_dir = game_dir(&state, &instance_id)?;
    let world = game_dir.join("saves").join(&folder);
    // Registered before waiting for the instance lock, so a queued job can be cancelled too.
    let job = start_job(&app, &job_id);
    let lock = world_lock(&app, &instance_id);
    let _guard = lock.lock().await;
    if job.token.is_cancelled() {
        return Err(pl_mc::Error::Cancelled.into());
    }
    ensure_game_closed(&state, &instance_id)?;
    let dest = gamefiles::default_backup_path(&game_dir, &folder);
    let progress: BackupProgressFn = Arc::new(move |p| {
        let _ = on_progress.send(p);
    });
    let summary = gamefiles::backup_world(&world, &dest, job.token.clone(), Some(progress))
        .await
        .map_err(fail(
            "YEDEK",
            pick(
                "Dünya yedeklenemedi. Diskte yer olduğundan emin olup tekrar dene.",
                "Couldn't back up the world. Make sure there's space on the disk and try again.",
            ),
        ))?;
    let file_name = summary
        .path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let listed = gamefiles::list_world_backups(&game_dir)
        .await
        .unwrap_or_default()
        .into_iter()
        .find(|b| b.file_name == file_name);
    Ok(match listed {
        Some(info) => backup_view(info),
        None => WorldBackupView {
            file_name,
            size_bytes: std::fs::metadata(&summary.path).map_or(0, |m| m.len()),
            created_at: Some(jiff::Timestamp::now().to_string()),
            world_folder: Some(folder),
            world_name: None,
            valid: true,
            reason: None,
        },
    })
}

/// Unpack a backup into `saves/`: as a new world, or in place of the world it was made from
/// (`replace`; that world goes to the launcher trash).
#[tauri::command]
#[specta::specta]
pub async fn world_restore(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    file_name: String,
    replace: bool,
    job_id: String,
    on_progress: Channel<BackupProgress>,
) -> CmdResult<RestoredWorldView> {
    ensure_game_closed(&state, &instance_id)?;
    let name = plain_name(&file_name)?;
    let game_dir = game_dir(&state, &instance_id)?;
    let backup = game_dir.join("backups").join(name);
    // Registered before waiting for the instance lock, so a queued job can be cancelled too.
    let job = start_job(&app, &job_id);
    let lock = world_lock(&app, &instance_id);
    let _guard = lock.lock().await;
    if job.token.is_cancelled() {
        return Err(pl_mc::Error::Cancelled.into());
    }
    ensure_game_closed(&state, &instance_id)?;
    let mode = if replace {
        RestoreMode::Replace {
            trash: launcher_trash(&state),
            instance_id: instance_id.clone(),
        }
    } else {
        RestoreMode::AsNew
    };
    let progress: BackupProgressFn = Arc::new(move |p| {
        let _ = on_progress.send(p);
    });
    let restored = gamefiles::restore_world_backup(
        &backup,
        &game_dir.join("saves"),
        mode,
        job.token.clone(),
        Some(progress),
    )
    .await
    .map_err(|error| match error {
        pl_mc::Error::Invalid(_) | pl_mc::Error::Archive { .. } => CmdError::new(
            "YEDEK-GECERSIZ",
            pick(
                "Bu dosya bir dünya yedeği değil ya da bozuk.",
                "This file isn't a world backup, or it's damaged.",
            ),
            error.to_string(),
        ),
        other => fail(
            "GERI-YUKLE",
            pick(
                "Yedek geri yüklenemedi. Diskte yer olduğundan emin olup tekrar dene.",
                "Couldn't restore the backup. Make sure there's space on the disk and try again.",
            ),
        )(other),
    })?;
    Ok(RestoredWorldView {
        folder: restored.folder,
        replaced: restored.replaced.map(trash_view),
    })
}

/// Move a world to the launcher trash (recoverable for 30 days).
#[tauri::command]
#[specta::specta]
pub async fn world_delete(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    folder: String,
) -> CmdResult<TrashItemView> {
    ensure_game_closed(&state, &instance_id)?;
    let folder = plain_name(&folder)?.to_owned();
    let world = game_dir(&state, &instance_id)?.join("saves").join(&folder);
    let lock = world_lock(&app, &instance_id);
    let _guard = lock.lock().await;
    ensure_game_closed(&state, &instance_id)?;
    let name = gamefiles::read_world(&world)
        .await
        .map(|w| w.name)
        .unwrap_or_else(|_| folder.clone());
    let trash = launcher_trash(&state);
    let entry = trash
        .put(&world, TrashKind::World, &instance_id, &name)
        .await
        .map_err(fail(
            "DUNYA-SIL",
            pick(
                "Dünya çöp kutusuna taşınamadı. Başka bir program dünyayı kullanıyor olabilir.",
                "Couldn't move the world to the trash. Another program may be using it.",
            ),
        ))?;
    purge_trash_later(trash);
    Ok(trash_view(entry))
}

/// Move a backup to the launcher trash.
#[tauri::command]
#[specta::specta]
pub async fn world_backup_delete(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    file_name: String,
) -> CmdResult<TrashItemView> {
    let name = plain_name(&file_name)?.to_owned();
    let path = game_dir(&state, &instance_id)?.join("backups").join(&name);
    let lock = world_lock(&app, &instance_id);
    let _guard = lock.lock().await;
    let trash = launcher_trash(&state);
    let entry = trash
        .put(&path, TrashKind::Backup, &instance_id, &name)
        .await
        .map_err(fail(
            "YEDEK-SIL",
            pick(
                "Yedek çöp kutusuna taşınamadı.",
                "Couldn't move the backup to the trash.",
            ),
        ))?;
    purge_trash_later(trash);
    Ok(trash_view(entry))
}

/// The instance's items in the launcher trash, newest first.
#[tauri::command]
#[specta::specta]
pub async fn trash_list(
    state: State<'_, AppState>,
    instance_id: String,
) -> CmdResult<Vec<TrashItemView>> {
    let trash = launcher_trash(&state);
    let items = trash.list(Some(&instance_id)).await.map_err(fail(
        "COP-LISTE",
        pick("Çöp kutusu okunamadı.", "Couldn't read the trash."),
    ))?;
    // Expired items are deleted in the background (big worlds take a while) and hidden now.
    let retention = i64::try_from(gamefiles::TRASH_RETENTION.as_millis()).unwrap_or(i64::MAX);
    let now = jiff::Timestamp::now().as_millisecond();
    purge_trash_later(trash);
    Ok(items
        .into_iter()
        // A deleted instance with this id is the library's (Settings → Veri), not this page's.
        .filter(|item| item.kind != TrashKind::Instance)
        .filter(|item| item.deleted_at_ms.saturating_add(retention) > now)
        .map(trash_view)
        .collect())
}

/// Put an item from the launcher trash back where it was. Returns its file or folder name
/// (numbered when the old name was taken meanwhile).
#[tauri::command]
#[specta::specta]
pub async fn trash_restore(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    id: String,
) -> CmdResult<String> {
    let trash = launcher_trash(&state);
    let not_found = |detail: String| {
        CmdError::new(
            "COP-YOK",
            pick("Bu öğe çöp kutusunda yok.", "This item isn't in the trash."),
            detail,
        )
    };
    let entry = trash
        .get(plain_name(&id)?)
        .await
        .map_err(|e| not_found(e.to_string()))?;
    if entry.instance_id != instance_id || entry.kind == TrashKind::Instance {
        return Err(not_found(format!("{id} belongs to another instance")));
    }
    let lock = world_lock(&app, &instance_id);
    let _guard = lock.lock().await;
    if entry.kind == TrashKind::World {
        ensure_game_closed(&state, &instance_id)?;
    }
    let path = trash.restore(&id).await.map_err(fail(
        "COP-GERI",
        pick(
            "Geri alınamadı. Tekrar dene.",
            "Couldn't bring it back. Try again.",
        ),
    ))?;
    Ok(path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default())
}

/// Delete an item of the launcher trash for good.
#[tauri::command]
#[specta::specta]
pub async fn trash_delete(
    state: State<'_, AppState>,
    instance_id: String,
    id: String,
) -> CmdResult<()> {
    let trash = launcher_trash(&state);
    let entry = trash.get(plain_name(&id)?).await.map_err(|e| {
        CmdError::new(
            "COP-YOK",
            pick("Bu öğe çöp kutusunda yok.", "This item isn't in the trash."),
            e.to_string(),
        )
    })?;
    if entry.instance_id != instance_id || entry.kind == TrashKind::Instance {
        return Err(CmdError::new(
            "COP-YOK",
            pick("Bu öğe çöp kutusunda yok.", "This item isn't in the trash."),
            format!("{id} belongs to another instance"),
        ));
    }
    trash.delete(&id).await.map_err(fail(
        "COP-SIL",
        pick(
            "Kalıcı olarak silinemedi. Tekrar dene.",
            "Couldn't delete it for good. Try again.",
        ),
    ))
}

// ---------------------------------------------------------------------------------------------
// Screenshots

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotFileView {
    pub file_name: String,
    /// Local path; the UI loads it with `convertFileSrc` (the folder is allowed on listing).
    pub path: String,
    pub size_bytes: u64,
    pub taken_at: Option<String>,
    /// Local path of its thumbnail when one was made already (else `screenshot_thumbnail`).
    pub thumb: Option<String>,
}

/// How a screenshot reached the clipboard.
#[derive(Debug, Clone, Copy, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ClipboardCopy {
    /// The picture itself.
    Image,
    /// Only the file path (the clipboard did not take the picture).
    Path,
}

fn screenshot_path(state: &AppState, instance_id: &str, file_name: &str) -> CmdResult<PathBuf> {
    let name = plain_name(file_name)?;
    Ok(game_dir(state, instance_id)?.join("screenshots").join(name))
}

/// The instance's thumbnail folder (`cache/thumbs/<instance>/`).
fn thumbs_dir(state: &AppState, instance_id: &str) -> CmdResult<PathBuf> {
    let id = plain_name(instance_id)?;
    Ok(gamefiles::thumbnails_dir(&state.root.join("cache"), id))
}

/// Screenshots of the instance, newest first, with the thumbnails made so far. Allows the
/// screenshots and thumbnail folders in the asset protocol so the gallery can show them, and
/// prunes thumbnails no picture needs any more in the background.
#[tauri::command]
#[specta::specta]
pub async fn screenshots_list(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
) -> CmdResult<Vec<ScreenshotFileView>> {
    let game_dir = game_dir(&state, &instance_id)?;
    let thumbs = thumbs_dir(&state, &instance_id)?;
    // The scope is a pattern on the folder's real path: make sure it exists.
    let _ = tokio::fs::create_dir_all(&thumbs).await;
    for dir in [game_dir.join("screenshots"), thumbs.clone()] {
        if let Err(error) = app.asset_protocol_scope().allow_directory(&dir, false) {
            tracing::warn!(%error, "could not allow a gallery folder");
        }
    }
    let shots = gamefiles::list_screenshots(&game_dir).await.map_err(fail(
        "GORSEL-LISTE",
        pick(
            "Ekran görüntüleri okunamadı.",
            "Couldn't read the screenshots.",
        ),
    ))?;
    let views = shots
        .iter()
        .map(|s| ScreenshotFileView {
            path: s.path.to_string_lossy().into_owned(),
            size_bytes: s.size,
            taken_at: iso(s.modified_ms),
            thumb: gamefiles::existing_thumbnail(&thumbs, s)
                .map(|p| p.to_string_lossy().into_owned()),
            file_name: s.file_name.clone(),
        })
        .collect();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = gamefiles::prune_thumbnails(&thumbs, &shots).await {
            tracing::debug!(%error, "could not prune thumbnails");
        }
    });
    Ok(views)
}

/// The thumbnail of a screenshot, made now when missing (two at a time). `None` when the
/// picture cannot be read as an image: the gallery shows the original then.
#[tauri::command]
#[specta::specta]
pub async fn screenshot_thumbnail(
    state: State<'_, AppState>,
    instance_id: String,
    file_name: String,
) -> CmdResult<Option<String>> {
    let path = screenshot_path(&state, &instance_id, &file_name)?;
    let thumbs = thumbs_dir(&state, &instance_id)?;
    let meta = tokio::fs::metadata(&path).await.map_err(|e| {
        CmdError::new(
            "GORSEL-OKU",
            pick(
                "Ekran görüntüsü okunamadı.",
                "Couldn't read the screenshot.",
            ),
            format!("{}: {e}", path.display()),
        )
    })?;
    let shot = gamefiles::ScreenshotInfo {
        file_name: plain_name(&file_name)?.to_owned(),
        size: meta.len(),
        modified_ms: meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .and_then(|d| i64::try_from(d.as_millis()).ok()),
        path,
    };
    match gamefiles::ensure_thumbnail(&thumbs, &shot).await {
        Ok(thumb) => Ok(Some(thumb.to_string_lossy().into_owned())),
        Err(error) => {
            tracing::debug!(%error, file = %file_name, "no thumbnail for a screenshot");
            Ok(None)
        }
    }
}

/// Copy a screenshot to the clipboard as a picture; when the clipboard does not take it, copy
/// the file path instead (the UI says which happened).
#[tauri::command]
#[specta::specta]
pub async fn screenshot_copy(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    file_name: String,
) -> CmdResult<ClipboardCopy> {
    let path = screenshot_path(&state, &instance_id, &file_name)?;
    let bytes = tokio::fs::read(&path).await.map_err(|e| {
        CmdError::new(
            "GORSEL-OKU",
            pick(
                "Ekran görüntüsü okunamadı.",
                "Couldn't read the screenshot.",
            ),
            format!("{}: {e}", path.display()),
        )
    })?;
    let decoded =
        tauri::async_runtime::spawn_blocking(move || tauri::image::Image::from_bytes(&bytes))
            .await
            .map_err(CmdError::internal)?;
    let copied = match decoded {
        Ok(image) => app
            .clipboard()
            .write_image(&image)
            .map_err(|e| e.to_string()),
        Err(error) => Err(error.to_string()),
    };
    match copied {
        Ok(()) => Ok(ClipboardCopy::Image),
        Err(reason) => {
            tracing::warn!(%reason, "could not copy the screenshot as a picture, copying its path");
            app.clipboard()
                .write_text(path.to_string_lossy().into_owned())
                .map_err(|e| {
                    CmdError::new(
                        "PANO",
                        pick("Panoya kopyalanamadı.", "Couldn't copy to the clipboard."),
                        e.to_string(),
                    )
                })?;
            Ok(ClipboardCopy::Path)
        }
    }
}

/// Open a screenshot in the system's image viewer.
#[tauri::command]
#[specta::specta]
pub async fn screenshot_open(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    file_name: String,
) -> CmdResult<()> {
    let path = screenshot_path(&state, &instance_id, &file_name)?;
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|e| {
            CmdError::new(
                "GORSEL-AC",
                pick("Görüntü açılamadı.", "Couldn't open the picture."),
                e.to_string(),
            )
        })
}

/// Show a file of the instance (`screenshots/`, `backups/`) in the file manager.
#[tauri::command]
#[specta::specta]
pub async fn instance_reveal_file(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    folder: String,
    file_name: String,
) -> CmdResult<()> {
    if !matches!(folder.as_str(), "screenshots" | "backups" | "saves") {
        return Err(folder_error(format!("folder {folder} is not shown")));
    }
    let path = game_dir(&state, &instance_id)?
        .join(&folder)
        .join(plain_name(&file_name)?);
    app.opener().reveal_item_in_dir(&path).map_err(folder_error)
}

/// Move a screenshot to the launcher trash.
#[tauri::command]
#[specta::specta]
pub async fn screenshot_delete(
    state: State<'_, AppState>,
    instance_id: String,
    file_name: String,
) -> CmdResult<TrashItemView> {
    let path = screenshot_path(&state, &instance_id, &file_name)?;
    let trash = launcher_trash(&state);
    let entry = trash
        .put(&path, TrashKind::Screenshot, &instance_id, &file_name)
        .await
        .map_err(fail(
            "GORSEL-SIL",
            pick(
                "Ekran görüntüsü silinemedi.",
                "Couldn't delete the screenshot.",
            ),
        ))?;
    purge_trash_later(trash);
    Ok(trash_view(entry))
}

// ---------------------------------------------------------------------------------------------
// Groups

/// Put an instance in a library group (`None` or blank: no group).
#[tauri::command]
#[specta::specta]
pub async fn instance_set_group(
    state: State<'_, AppState>,
    instance_id: String,
    group: Option<String>,
) -> CmdResult<()> {
    let mut instance = state
        .instances
        .get(&instance_id)
        .await
        .map_err(CmdError::from)?;
    instance.group = group
        .map(|g| g.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|g| !g.is_empty())
        .map(|g| g.chars().take(40).collect());
    state
        .instances
        .update(&instance)
        .await
        .map_err(CmdError::from)
}

// ---------------------------------------------------------------------------------------------
// The instance folder's size ("Kapladığı yer" on the overview)

/// What `instance_disk_usage` measured.
#[derive(Debug, Clone, Copy, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct InstanceDiskUsageView {
    pub bytes: u64,
    pub files: u64,
}

/// The last measurement of each instance with the folder's stamp at that time, and the walk that
/// runs now (one at a time: a new request stops the one before).
#[derive(Default)]
pub struct DiskUsageJobs {
    cache: Mutex<HashMap<String, (u64, InstanceDiskUsageView)>>,
    running: Mutex<Option<(String, Arc<AtomicBool>)>>,
}

fn disk_usage_jobs(app: &AppHandle) -> tauri::State<'_, DiskUsageJobs> {
    if app.try_state::<DiskUsageJobs>().is_none() {
        app.manage(DiskUsageJobs::default());
    }
    app.state::<DiskUsageJobs>()
}

/// The room the whole instance folder takes (`pl_mc::instance::usage`: every version set,
/// worlds, backups, screenshots and settings; never the game files all instances share). A
/// folder whose top levels did not change since the last measurement answers from it at once;
/// otherwise the folder is walked off the async threads. `request_id` lets
/// `instance_disk_usage_cancel` stop exactly this walk; a newer request stops it too.
#[tauri::command]
#[specta::specta]
pub async fn instance_disk_usage(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    request_id: String,
) -> CmdResult<InstanceDiskUsageView> {
    let dir = state
        .instances
        .instance_dir(&instance_id)
        .map_err(CmdError::from)?;
    let jobs = disk_usage_jobs(&app);
    let look = dir.clone();
    let stamp = tauri::async_runtime::spawn_blocking(move || usage::stamp(&look))
        .await
        .map_err(CmdError::internal)?;
    if let Some((known, view)) = jobs.cache.lock().get(&instance_id).copied()
        && known == stamp
    {
        return Ok(view);
    }
    let cancel = Arc::new(AtomicBool::new(false));
    if let Some((_, previous)) = jobs
        .running
        .lock()
        .replace((request_id.clone(), Arc::clone(&cancel)))
    {
        previous.store(true, Ordering::Relaxed);
    }
    let flag = Arc::clone(&cancel);
    let measured = tauri::async_runtime::spawn_blocking(move || usage::measure(&dir, &flag)).await;
    {
        let mut running = jobs.running.lock();
        if running.as_ref().is_some_and(|(id, _)| *id == request_id) {
            running.take();
        }
    }
    let usage = match measured.map_err(CmdError::internal)? {
        Ok(usage) => usage,
        Err(pl_mc::Error::Cancelled) => {
            return Err(CmdError::new(
                "IPTAL",
                pick("İşlem iptal edildi.", "Cancelled."),
                "the measurement was stopped",
            ));
        }
        Err(error) => return Err(CmdError::from(error)),
    };
    let view = InstanceDiskUsageView {
        bytes: usage.bytes,
        files: usage.files,
    };
    jobs.cache.lock().insert(instance_id, (stamp, view));
    Ok(view)
}

/// Stops the measurement `instance_disk_usage` runs for `request_id` (the overview went away).
#[tauri::command]
#[specta::specta]
pub fn instance_disk_usage_cancel(app: AppHandle, request_id: String) {
    if let Some((id, cancel)) = disk_usage_jobs(&app).running.lock().as_ref()
        && *id == request_id
    {
        cancel.store(true, Ordering::Relaxed);
    }
}
