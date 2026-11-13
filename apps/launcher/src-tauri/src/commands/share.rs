//! Screen sharing (SHARE; the lead's contract `docs/tasks/share.md`): the page's commands over
//! `pl_voice::Voice` (`crate::voice::share` carries the effects out). Every change reaches the
//! page inside `voice-state` (`VoiceState.share`), toasts as `share-notice`.
//!
//! A page that watches (the main window, or the "Yayın" window) attaches its own channel; the
//! launcher brokers its SDP with the room (`share_watch_report`). The "Yayın" window may call only
//! the commands it needs (`pl_voice::guard`).

use pl_voice::share::{
    PopoutAudio, ShareChange, ShareLayer, ShareRefusal, ShareSource, ShareSources, ShareSurface,
    ShareViewPrefs, WatchCmd, WatchReport,
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

/// A sharer's stream sound for the player (percent 0 to `volumeMax`, SQOL: up to 200; muted).
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

/// SQOL "Ekranı Değiştir": share `source` in share `n` instead (the watchers stay): null when it
/// went to the publisher; the result comes in the state and as `share-notice` (`switched`,
/// `switchFailed`).
#[tauri::command]
#[specta::specta]
pub fn share_switch(
    state: State<'_, AppState>,
    n: u8,
    source: ShareSource,
    label: String,
) -> CmdResult<Option<ShareRefusal>> {
    Ok(state.voice.share_switch(n, source, label).err())
}

/// SQOL: a live change of share `n` ("Kalite", "Kare Hızı", "Görüntü", "İmleci Göster", "Sesi de
/// Paylaş", "Önizleme Resmini Paylaş"): null when asked; the values the room applied come in the
/// state.
#[tauri::command]
#[specta::specta]
pub fn share_update(
    state: State<'_, AppState>,
    n: u8,
    change: ShareChange,
) -> CmdResult<Option<ShareRefusal>> {
    Ok(state.voice.share_update(n, change).err())
}

/// SQOL "Duraklat" (`on`) / "Sürdür": watchers see a blank picture meanwhile.
#[tauri::command]
#[specta::specta]
pub fn share_hold(state: State<'_, AppState>, n: u8, on: bool) -> CmdResult<Option<ShareRefusal>> {
    Ok(state.voice.share_hold(n, on).err())
}

/// SQOL: the player's own small live preview of share `n` while its card is on screen
/// (`share-preview`, at most twice a second, only for new frames).
#[tauri::command]
#[specta::specta]
pub fn share_preview(state: State<'_, AppState>, n: u8, on: bool) -> CmdResult<()> {
    state.voice.share_preview(n, on);
    Ok(())
}

/// SQOL: what the "Yayın" window plays streams with (it cannot read voice chat's settings);
/// changes come as `share-audio`.
#[tauri::command]
#[specta::specta]
pub fn share_popout_audio(state: State<'_, AppState>) -> CmdResult<PopoutAudio> {
    Ok(crate::voice::share::popout_audio(&state.voice))
}

/// SQOL: the watching choices ("Yayın Kalitesi", "Biri Konuşunca Yayın Sesini Kıs" and its
/// amount), from either window.
#[tauri::command]
#[specta::specta]
pub fn share_view_prefs(state: State<'_, AppState>, change: ShareViewPrefs) -> CmdResult<()> {
    state.voice.share_view_prefs(change);
    Ok(())
}

/// SHARE-LIVE: the hover card's still picture of share `n` of `user` in `room` (the player's own
/// from this computer): `image` (a `data:` URL), `locked` (the player may not join that room:
/// no picture) or `none`. Kept 20 s; a newer picture in the player's room (`th`) is asked at once.
/// Someone outside the player's room has one share as far as the list knows: `n` 1.
#[tauri::command]
#[specta::specta]
pub async fn share_thumb(
    state: State<'_, AppState>,
    room: String,
    user: String,
    n: u8,
) -> CmdResult<crate::voice::thumb::ShareThumb> {
    Ok(crate::voice::thumb::for_page(&state.social, &state.voice, &room, &user, n).await)
}
