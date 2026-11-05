//! Screen sharing (SHARE; the lead's contract `docs/tasks/share.md`): the page's commands over
//! `pl_voice::Voice` (`crate::voice::share` carries the effects out). Every change reaches the
//! page inside `voice-state` (`VoiceState.share`), toasts as `share-notice`.
//!
//! A page that watches (the main window, or the "Yayın" window) attaches its own channel; the
//! launcher brokers its SDP with the room (`share_watch_report`). The "Yayın" window may call only
//! the commands it needs (`pl_voice::guard`).

use pl_voice::share::{
    ShareLayer, ShareRefusal, ShareSource, ShareSources, ShareSurface, WatchCmd, WatchReport,
};
use pl_voice::view::VoiceAnswer;
use tauri::ipc::Channel;
use tauri::{AppHandle, State, WebviewWindow};

use crate::error::CmdResult;
use crate::state::AppState;

fn surface_of(window: &WebviewWindow) -> Option<ShareSurface> {
    ShareSurface::of_window(window.label())
}

/// The picker's list: "Oyunu paylaş" (the game the launcher runs), windows, screens. Thumbnails
/// are made now, once per opening.
#[tauri::command]
#[specta::specta]
pub async fn share_sources() -> CmdResult<ShareSources> {
    Ok(
        tauri::async_runtime::spawn_blocking(crate::voice::share::sources)
            .await
            .unwrap_or_default(),
    )
}

/// "Paylaş": null when it started (the state says "Yayında" once the room answers).
#[tauri::command]
#[specta::specta]
pub fn share_start(
    state: State<'_, AppState>,
    source: ShareSource,
    label: String,
) -> CmdResult<Option<ShareRefusal>> {
    Ok(state.voice.share_start(source, label).err())
}

/// "Durdur" on one of the player's shares.
#[tauri::command]
#[specta::specta]
pub fn share_stop(state: State<'_, AppState>, n: u8) -> CmdResult<()> {
    state.voice.share_stop(n);
    Ok(())
}

/// "İzle": null when asked for.
#[tauri::command]
#[specta::specta]
pub fn share_watch(
    state: State<'_, AppState>,
    person: String,
    n: u8,
) -> CmdResult<Option<ShareRefusal>> {
    Ok(state.voice.share_watch(person, n).err())
}

/// "İzlemeyi bırak".
#[tauri::command]
#[specta::specta]
pub fn share_unwatch(state: State<'_, AppState>, person: String, n: u8) -> CmdResult<()> {
    state.voice.share_unwatch(person, n);
    Ok(())
}

/// The tile of a watched share grew or shrank: the full or the small layer.
#[tauri::command]
#[specta::specta]
pub fn share_watch_layer(
    state: State<'_, AppState>,
    person: String,
    n: u8,
    layer: ShareLayer,
) -> CmdResult<()> {
    state.voice.share_layer(person, n, layer);
    Ok(())
}

/// A page that watches hands over its channel (the main window, or the "Yayın" window).
#[tauri::command]
#[specta::specta]
pub fn share_watch_attach(
    window: WebviewWindow,
    state: State<'_, AppState>,
    on_command: Channel<WatchCmd>,
) -> CmdResult<()> {
    if let Some(surface) = surface_of(&window) {
        crate::voice::share::attach(&state.voice, surface, on_command);
    }
    Ok(())
}

/// The watching page's report (its answer, TURN, connected, lost).
#[tauri::command]
#[specta::specta]
pub fn share_watch_report(
    window: WebviewWindow,
    state: State<'_, AppState>,
    report: WatchReport,
) -> CmdResult<()> {
    if let Some(surface) = surface_of(&window) {
        state.voice.share_watch_report(surface, report);
    }
    Ok(())
}

/// The watching page went out of sight (minimized, in the tray, the game in front) or came back.
#[tauri::command]
#[specta::specta]
pub fn share_watch_visible(
    window: WebviewWindow,
    state: State<'_, AppState>,
    visible: bool,
) -> CmdResult<()> {
    if let Some(surface) = surface_of(&window) {
        state.voice.share_visible(surface, visible);
    }
    Ok(())
}

/// "Evet" on "Hâlâ izliyor musun?" (any touch of the stage too).
#[tauri::command]
#[specta::specta]
pub fn share_watch_still(state: State<'_, AppState>) -> CmdResult<()> {
    state.voice.share_still();
    Ok(())
}

/// "Ayrı pencerede aç" (`open`), or back into the launcher's window.
#[tauri::command]
#[specta::specta]
pub fn share_popout(state: State<'_, AppState>, open: bool) -> CmdResult<()> {
    state.voice.share_popout(open);
    Ok(())
}

/// "Her zaman üstte" on the "Yayın" window: now, and for the next one.
#[tauri::command]
#[specta::specta]
pub fn share_popout_on_top(app: AppHandle, state: State<'_, AppState>, on: bool) -> CmdResult<()> {
    let mut settings = state.voice.settings();
    if settings.share.always_on_top != on {
        settings.share.always_on_top = on;
        state.voice.save_settings(settings);
    }
    crate::voice::share::set_on_top(&app, on);
    Ok(())
}

/// The watching window fills the screen (or not).
#[tauri::command]
#[specta::specta]
pub fn share_fullscreen(app: AppHandle, window: WebviewWindow, on: bool) -> CmdResult<()> {
    crate::voice::share::set_fullscreen(&app, window.label(), on);
    Ok(())
}

/// A sharer's stream sound for the player (percent 0–100, muted).
#[tauri::command]
#[specta::specta]
pub fn share_stream_volume(
    state: State<'_, AppState>,
    person: String,
    volume: u16,
    muted: bool,
) -> CmdResult<()> {
    state.voice.set_stream_volume(person, volume, muted);
    Ok(())
}

/// "Kim paylaşabilir" in a friend room (its leader; the room checks).
#[tauri::command]
#[specta::specta]
pub async fn share_set_sharers(
    state: State<'_, AppState>,
    who: pl_voice::share::ShareBy,
) -> CmdResult<VoiceAnswer> {
    Ok(state.voice.share_set_sharers(who).await)
}

/// "Paylaşımı durdur" on someone else's share (a leader or moderator; the room checks).
#[tauri::command]
#[specta::specta]
pub async fn share_unshare(
    state: State<'_, AppState>,
    person: String,
    n: u8,
) -> CmdResult<VoiceAnswer> {
    Ok(state.voice.share_unshare(person, n).await)
}
