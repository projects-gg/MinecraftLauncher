//! "Ekran görüntülerini panoya kopyala" (Settings → Genel, off by default): while a game runs,
//! every screenshot the game finishes writing goes to the clipboard as a picture, so F2 and then
//! Ctrl+V in Discord just works. The screenshots folder is watched only during the session, with
//! the system's change notifications (`pl_mc::gamefiles::ScreenshotWatcher`, no polling), and a
//! picture that was there before the watch began is never copied. Successes are silent; the
//! first failure of a session is told once (`LaunchEventView::ScreenshotNotCopied`: the page
//! shows a quiet notice, a system notification while the window is away).
//!
//! The setting travels with each launch (`LaunchSettings::copy_screenshots`); switching it while
//! a game runs starts or stops the watch at once (`game_copy_screenshots`).

use std::path::{Path, PathBuf};

use parking_lot::Mutex;
use pl_mc::gamefiles::{ScreenshotEvent, ScreenshotWatcher};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::commands::launch::LaunchEventView;

/// The running game's session (one game at a time) and its watch, while the setting is on.
#[derive(Default)]
pub struct ShotCopy(Mutex<Option<Session>>);

struct Session {
    game_dir: PathBuf,
    /// The launch's channel: the page's notice comes through it.
    events: Channel<LaunchEventView>,
    watch: Option<Watch>,
}

/// The folder watch and the task that copies what it reports, in order.
struct Watch {
    _watcher: ScreenshotWatcher,
    task: tauri::async_runtime::JoinHandle<()>,
}

impl Drop for Watch {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// The registry, created on first use (no setup code needed).
fn shot_copy(app: &AppHandle) -> tauri::State<'_, ShotCopy> {
    if app.try_state::<ShotCopy>().is_none() {
        app.manage(ShotCopy::default());
    }
    app.state::<ShotCopy>()
}

/// A game started in `game_dir`; `enabled` is the setting the launch came with.
pub fn session_started(
    app: &AppHandle,
    game_dir: &Path,
    events: Channel<LaunchEventView>,
    enabled: bool,
) {
    let mut session = Session {
        game_dir: game_dir.to_path_buf(),
        events,
        watch: None,
    };
    if enabled {
        session.watch = watch(app, &session);
    }
    *shot_copy(app).0.lock() = Some(session);
}

/// The game closed: the watch ends with it.
pub fn session_ended(app: &AppHandle) {
    shot_copy(app).0.lock().take();
}

/// The player switched the setting: a running game starts or stops being watched now.
pub fn set_enabled(app: &AppHandle, enabled: bool) {
    let state = shot_copy(app);
    let mut current = state.0.lock();
    let Some(session) = current.as_mut() else {
        return;
    };
    if !enabled {
        session.watch = None;
    } else if session.watch.is_none() {
        session.watch = watch(app, session);
    }
}

fn watch(app: &AppHandle, session: &Session) -> Option<Watch> {
    let (watcher, mut shots) = match ScreenshotWatcher::start(&session.game_dir) {
        Ok(started) => started,
        Err(error) => {
            // The game could not save pictures there either; nothing to tell yet.
            tracing::warn!(%error, "screenshots folder not watched");
            return None;
        }
    };
    tracing::info!("copying the session's screenshots to the clipboard");
    let app = app.clone();
    let events = session.events.clone();
    let task = tauri::async_runtime::spawn(async move {
        let mut told = false;
        while let Some(shot) = shots.recv().await {
            let copied = match shot {
                ScreenshotEvent::Ready(path) => copy_picture(&app, &path).await,
                ScreenshotEvent::Unfinished(path) => {
                    Err(format!("{} was never a complete picture", path.display()))
                }
            };
            match copied {
                Ok(()) => tracing::info!("a new screenshot is on the clipboard"),
                Err(reason) => {
                    tracing::warn!(%reason, "a new screenshot was not copied");
                    if !told {
                        told = true;
                        // A page resting while the game runs hears about it now.
                        crate::webview_rest::wake(&app, "a screenshot was not copied");
                        let _ = events.send(LaunchEventView::ScreenshotNotCopied);
                    }
                }
            }
        }
    });
    Some(Watch {
        _watcher: watcher,
        task,
    })
}

/// The picture at `path` onto the clipboard (decoded off the async threads).
async fn copy_picture(app: &AppHandle, path: &Path) -> Result<(), String> {
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let image =
        tauri::async_runtime::spawn_blocking(move || tauri::image::Image::from_bytes(&bytes))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
    app.clipboard()
        .write_image(&image)
        .map_err(|e| e.to_string())
}
