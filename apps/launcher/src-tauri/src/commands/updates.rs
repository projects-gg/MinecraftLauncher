//! Launcher self-updates on top of tauri-plugin-updater.
//!
//! 1. Our channel file (`v1/releases/<channel>.json`, Tauri's static format plus `rollout`,
//!    `mandatory`, `minVersion`) is fetched from the content mirrors in order.
//! 2. `ReleaseChannel::decide` applies staged rollout (stable per-install bucket) and the
//!    minimum version.
//! 3. The Tauri updater downloads the signed bundle from the mirror that answered and installs it.
//! 4. Installing never leaves the player stuck (docs/tasks/q2.md, proposal 9): a running game
//!    blocks it (the UI offers the update for after the game); open AFK accounts are stopped
//!    first and come back by themselves after the restart. A one-shot note (`pl_data::restart`)
//!    carries them over, together with the window, the page the player was on and the new
//!    version's notes. A failed install keeps the update and brings the accounts back at once.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Mutex;
use pl_content::{ReleaseChannel, UpdateDecision, rollout_bucket};
use pl_data::restart::RestartNote;
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::error::{CmdError, CmdResult};
use crate::state::{AppState, CONTENT_MIRRORS};
use pl_lang::pick;

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    /// "upToDate" | "available" | "mandatory" | "heldBack" | "unavailable"
    pub state: String,
    pub current: String,
    pub version: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgress {
    pub downloaded: u64,
    pub total: Option<u64>,
    pub done: bool,
}

/// Where `update_apply` is (streamed over its channel; it only returns when installing failed).
#[derive(Debug, Clone, Copy, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum UpdateStep {
    /// Open AFK accounts are being stopped; they connect again after the restart.
    StoppingAfk,
    /// The installer starts: the launcher closes and opens again.
    Installing,
}

/// What the update restart left for the UI (`update_restart_info`).
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RestartInfo {
    /// The version the update installed.
    pub version: String,
    /// Its release notes, set when this start runs that version ("Yenilikler", shown once).
    pub notes: Option<String>,
    /// The page the player was on, set right after the restart.
    pub route: Option<String>,
}

/// This start follows an update installed from an open window: the window opens even when the
/// launcher had been started minimized at login (the installer passes the old arguments on).
static SHOW_AFTER_UPDATE: AtomicBool = AtomicBool::new(false);

/// The UI's part of the restart note, taken once.
static AFTER_RESTART: Mutex<Option<RestartInfo>> = Mutex::new(None);

/// Stable random id of this installation, used only for the rollout bucket.
fn install_id(state: &AppState) -> String {
    let path = state.root.join("install-id");
    if let Ok(id) = std::fs::read_to_string(&path)
        && !id.trim().is_empty()
    {
        return id.trim().to_owned();
    }
    let id = uuid::Uuid::new_v4().to_string();
    let _ = std::fs::write(&path, &id);
    id
}

fn channel_urls(channel: &str) -> Vec<String> {
    let channel = if channel == "beta" { "beta" } else { "stable" };
    CONTENT_MIRRORS
        .iter()
        .map(|base| format!("{base}releases/{channel}.json"))
        .collect()
}

async fn fetch_channel(state: &AppState, channel: &str) -> Option<(ReleaseChannel, String)> {
    let urls = channel_urls(channel);
    match state
        .http
        .get_json_from_mirrors::<ReleaseChannel>(&urls)
        .await
    {
        Ok((release, index)) => Some((release, urls[index].clone())),
        Err(error) => {
            tracing::info!(%error, "update channel unavailable");
            None
        }
    }
}

#[tauri::command]
#[specta::specta]
pub async fn update_check(
    app: AppHandle,
    state: State<'_, AppState>,
    channel: String,
) -> CmdResult<UpdateStatus> {
    let current_text = app.package_info().version.to_string();
    let current = semver::Version::parse(&current_text).map_err(CmdError::internal)?;
    let Some((release, _)) = fetch_channel(&state, &channel).await else {
        return Ok(UpdateStatus {
            state: "unavailable".into(),
            current: current_text,
            version: None,
            notes: None,
        });
    };
    let decision = release.decide(&current, rollout_bucket(&install_id(&state)));
    let state_text = match decision {
        UpdateDecision::UpToDate => "upToDate",
        UpdateDecision::HeldBack => "heldBack",
        UpdateDecision::Available => "available",
        UpdateDecision::Mandatory => "mandatory",
        UpdateDecision::Invalid => "unavailable",
    };
    Ok(UpdateStatus {
        state: state_text.into(),
        current: current_text,
        version: Some(release.version.clone()),
        notes: release.notes.clone(),
    })
}

/// Download the update of `channel` in the background (signature verified by the updater) and
/// keep it ready; `update_apply` installs it. Progress streams over `on_progress`.
#[tauri::command]
#[specta::specta]
pub async fn update_download(
    app: AppHandle,
    state: State<'_, AppState>,
    channel: String,
    on_progress: Channel<UpdateProgress>,
) -> CmdResult<bool> {
    let Some((_, url)) = fetch_channel(&state, &channel).await else {
        return Err(CmdError::new(
            "GUNCELLEME-YOK",
            pick(
                "Güncelleme sunucusuna ulaşılamadı. Daha sonra tekrar dene.",
                "Couldn't reach the update server. Try again later.",
            ),
            "no channel mirror answered",
        ));
    };
    let endpoint = url.parse().map_err(CmdError::internal)?;
    let updater = app
        .updater_builder()
        .endpoints(vec![endpoint])
        .map_err(CmdError::internal)?
        .build()
        .map_err(CmdError::internal)?;
    let Some(update) = updater.check().await.map_err(|e| {
        CmdError::new(
            "GUNCELLEME",
            pick("Güncelleme denetlenemedi.", "Couldn't check for updates."),
            e.to_string(),
        )
    })?
    else {
        return Ok(false);
    };
    let downloaded = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let progress = on_progress.clone();
    let counter = Arc::clone(&downloaded);
    let bytes = update
        .download(
            move |chunk, total| {
                let done = counter.fetch_add(chunk as u64, std::sync::atomic::Ordering::Relaxed)
                    + chunk as u64;
                let _ = progress.send(UpdateProgress {
                    downloaded: done,
                    total,
                    done: false,
                });
            },
            move || {
                let _ = on_progress.send(UpdateProgress {
                    downloaded: downloaded.load(std::sync::atomic::Ordering::Relaxed),
                    total: None,
                    done: true,
                });
            },
        )
        .await
        .map_err(|e| {
            CmdError::new(
                "GUNCELLEME-INDIR",
                pick(
                    "Güncelleme indirilemedi.",
                    "The update couldn't be downloaded.",
                ),
                e.to_string(),
            )
        })?;
    *state.pending_update.lock() = Some((update, bytes));
    Ok(true)
}

/// The downloaded update is gone (it never was, or a failed install lost it): download again.
pub const NOT_READY: &str = "GUNCELLEME-HAZIR-DEGIL";

fn not_ready(details: &str) -> CmdError {
    CmdError::new(
        NOT_READY,
        pick("Hazır bir güncelleme yok.", "No update is ready."),
        details,
    )
}

fn note_path(state: &AppState) -> PathBuf {
    state.root.join(pl_data::restart::FILE_NAME)
}

/// A game runs or is being prepared (downloads, Java, mods): installing would end it.
fn game_active(state: &AppState) -> bool {
    state.running.lock().is_some() || state.launch_cancel.lock().is_some()
}

fn window_on_screen(app: &AppHandle) -> bool {
    app.get_webview_window("main").is_some_and(|window| {
        window.is_visible().unwrap_or(false) && !window.is_minimized().unwrap_or(false)
    })
}

/// Saves the window's size and place now: on Windows the installer ends the launcher with
/// `std::process::exit`, so the window-state plugin's own save at exit never runs.
fn save_window_state(app: &AppHandle) {
    use tauri_plugin_window_state::{AppHandleExt, StateFlags};
    // The parts the plugin restores (lib.rs).
    let flags = StateFlags::SIZE | StateFlags::POSITION | StateFlags::MAXIMIZED;
    if let Err(error) = app.save_window_state(flags) {
        tracing::warn!(%error, "window state not saved before the update");
    }
}

async fn write_note(path: &std::path::Path, note: &RestartNote) {
    let (path, note) = (path.to_path_buf(), note.clone());
    match tauri::async_runtime::spawn_blocking(move || pl_data::restart::save(&path, &note)).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => tracing::warn!(%error, "update restart note not written"),
        Err(error) => tracing::warn!(%error, "update restart note not written"),
    }
}

/// Stops the open AFK accounts and lists them in the note (written before anything stops, so a
/// crash meanwhile still brings them back). A connect that slipped in meanwhile (a click, a
/// countdown that ended) is stopped and listed too.
async fn stop_afk(state: &AppState, path: &std::path::Path, note: &mut RestartNote) {
    for _ in 0..2 {
        for id in state.afk.resume_candidates() {
            if !note.afk_accounts.contains(&id) {
                note.afk_accounts.push(id);
            }
        }
        write_note(path, note).await;
        state.afk.stop_all().await;
        if !state.afk.any_active() {
            break;
        }
    }
    tracing::info!(
        accounts = note.afk_accounts.len(),
        "AFK accounts stopped for the update"
    );
}

/// The install did not happen: the accounts the update stopped connect again now, in the same
/// staggered queue as "connect all".
fn resume_afk_now(app: &AppHandle, accounts: Vec<String>) {
    if accounts.is_empty() {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        if let Err(error) = state.afk.connect_staggered(&accounts).await {
            tracing::warn!(%error, "AFK accounts not reconnected after a failed update");
        }
    });
}

/// Why installing failed, with the update when it is still there to try again.
struct InstallFailed {
    kept: Option<(Update, Vec<u8>)>,
    details: String,
}

/// Runs the installer on the main thread, as the plugin expects there: on Windows it starts the
/// NSIS installer through the shell (which restarts the launcher when done) and ends this process;
/// on macOS and Linux it replaces the app and the launcher restarts itself. It returns only when
/// nothing was installed.
async fn install(app: &AppHandle, update: Update, bytes: Vec<u8>) -> InstallFailed {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let handle = app.clone();
    let dispatched = app.run_on_main_thread(move || match update.install(&bytes) {
        Ok(()) => handle.restart(),
        Err(error) => {
            let _ = sender.send(InstallFailed {
                details: error.to_string(),
                kept: Some((update, bytes)),
            });
        }
    });
    if let Err(error) = dispatched {
        return InstallFailed {
            kept: None,
            details: error.to_string(),
        };
    }
    receiver.await.unwrap_or_else(|_| InstallFailed {
        kept: None,
        details: "the install was dropped before it ran".into(),
    })
}

/// Install the downloaded update and restart into it. Never under a running game; open AFK accounts
/// are stopped first (`on_step` says so) and connect again after the restart, as does the window
/// and `route`, the page the player is on. When installing fails the update is kept ("Tekrar dene"
/// works) and the AFK accounts connect again at once.
#[tauri::command]
#[specta::specta]
pub async fn update_apply(
    app: AppHandle,
    state: State<'_, AppState>,
    route: Option<String>,
    on_step: Channel<UpdateStep>,
) -> CmdResult<()> {
    let Some((update, bytes)) = state.pending_update.lock().take() else {
        return Err(not_ready("no pending update"));
    };
    if game_active(&state) {
        *state.pending_update.lock() = Some((update, bytes));
        return Err(CmdError::new(
            "GUNCELLEME-OYUN",
            pick(
                "Oyun açıkken güncelleme kurulamaz. Oyundan çıkınca tekrar dene.",
                "The update can't be installed while the game is open. Try again after you quit the game.",
            ),
            "a game runs or is being prepared",
        ));
    }
    let path = note_path(&state);
    let mut note = RestartNote {
        saved_at: pl_data::restart::now_ms(),
        version: update.version.clone(),
        notes: update.body.clone().filter(|notes| !notes.trim().is_empty()),
        afk_accounts: Vec::new(),
        window_visible: window_on_screen(&app),
        route: route.as_deref().and_then(pl_data::restart::clean_route),
    };
    if state.afk.any_active() {
        let _ = on_step.send(UpdateStep::StoppingAfk);
        stop_afk(&state, &path, &mut note).await;
    } else {
        write_note(&path, &note).await;
    }
    let _ = on_step.send(UpdateStep::Installing);
    save_window_state(&app);

    let failed = install(&app, update, bytes).await;
    tracing::warn!(error = %failed.details, "the update could not be installed");
    pl_data::restart::discard(&path);
    resume_afk_now(&app, note.afk_accounts);
    match failed.kept {
        Some(pending) => {
            *state.pending_update.lock() = Some(pending);
            Err(CmdError::new(
                "GUNCELLEME-KURULUM",
                pick(
                    "Güncelleme kurulamadı. Tekrar dene.",
                    "The update couldn't be installed. Try again.",
                ),
                failed.details,
            ))
        }
        None => Err(not_ready(&failed.details)),
    }
}

/// Takes the note an update left for this start (`pl_data::restart`): returns the AFK accounts to
/// connect again, keeps the UI's part for `update_restart_info` and decides whether the window
/// opens. A note older than ten minutes (an update that never finished) only keeps its notes.
pub fn take_restart_note(app: &AppHandle) -> Vec<String> {
    let state = app.state::<AppState>();
    let Some(note) = pl_data::restart::take(&note_path(&state)) else {
        return Vec::new();
    };
    let fresh = note.is_fresh_at(pl_data::restart::now_ms());
    let updated = note.version == app.package_info().version.to_string();
    tracing::info!(
        version = %note.version,
        updated,
        fresh,
        afk = note.afk_accounts.len(),
        "started after an update"
    );
    if fresh && note.window_visible {
        SHOW_AFTER_UPDATE.store(true, Ordering::Relaxed);
    }
    let info = RestartInfo {
        version: note.version,
        notes: note.notes.filter(|_| updated),
        route: note.route.filter(|_| fresh),
    };
    if info.notes.is_some() || info.route.is_some() {
        *AFTER_RESTART.lock() = Some(info);
    }
    if fresh { note.afk_accounts } else { Vec::new() }
}

/// The launcher waits in the tray: started at login (`--minimized`), unless this start follows an
/// update the player installed from the open window.
pub fn start_hidden() -> bool {
    crate::shell::start_minimized() && !SHOW_AFTER_UPDATE.load(Ordering::Relaxed)
}

/// What the update restart left for the UI: the new version's notes and the page to return to.
/// Taken once.
#[tauri::command]
#[specta::specta]
pub fn update_restart_info() -> Option<RestartInfo> {
    AFTER_RESTART.lock().take()
}

#[tauri::command]
#[specta::specta]
pub fn app_restart(app: AppHandle) {
    app.restart();
}
