//! "Dünyanı arkadaşlarına aç" (HOST1): the page's commands over `pl_worlds::Worlds`
//! (`crate::host`). Every change reaches the page as `host-changed` (the whole `HostView`), moments
//! worth a toast as `host-notice`. The page starts the game itself (OYNA's flow): `host_open`
//! answers `start` when it should, and a join's `ready` stage names the install and the local port
//! for quick play. Refusals are answers or notices, never errors.

use pl_worlds::{
    HostPlace, HostView, WorldInviteOutcome, WorldJoinAnswer, WorldOpenOutcome, WorldOpenRequest,
};
use tauri::State;

use crate::error::CmdResult;
use crate::state::AppState;

/// The view now (the page's start; every change after comes as `host-changed`).
#[tauri::command]
#[specta::specta]
pub fn host_state(state: State<'_, AppState>) -> CmdResult<HostView> {
    Ok(state.worlds.view())
}

/// Whether an instance's worlds offer "Arkadaşlarınla oyna", and the world open in its running
/// game.
#[tauri::command]
#[specta::specta]
pub async fn host_place(state: State<'_, AppState>, instance_id: String) -> CmdResult<HostPlace> {
    Ok(state.worlds.place(&instance_id).await)
}

/// "Dünyayı aç": the backup a world needs first, then `start` (the page starts the game into the
/// world), `running` (it opens in place) or `cancelled`.
#[tauri::command]
#[specta::specta]
pub async fn host_open(
    state: State<'_, AppState>,
    request: WorldOpenRequest,
) -> CmdResult<WorldOpenOutcome> {
    Ok(state.worlds.open(request).await)
}

/// "Vazgeç" during the backup, or the game did not start after all.
#[tauri::command]
#[specta::specta]
pub fn host_cancel_open(state: State<'_, AppState>) -> CmdResult<()> {
    state.worlds.cancel_open();
    Ok(())
}

/// "Davet et" while the world is open.
#[tauri::command]
#[specta::specta]
pub async fn host_invite(
    state: State<'_, AppState>,
    friends: Vec<String>,
) -> CmdResult<WorldInviteOutcome> {
    Ok(state.worlds.invite(friends).await)
}

/// "Dünyadan çıkar".
#[tauri::command]
#[specta::specta]
pub async fn host_remove(state: State<'_, AppState>, friend: String) -> CmdResult<()> {
    state.worlds.remove(&friend).await;
    Ok(())
}

/// "Paylaşımı kapat".
#[tauri::command]
#[specta::specta]
pub async fn host_stop(state: State<'_, AppState>) -> CmdResult<()> {
    state.worlds.stop().await;
    Ok(())
}

/// "Kapat" on a failed opening.
#[tauri::command]
#[specta::specta]
pub fn host_dismiss_failure(state: State<'_, AppState>) -> CmdResult<()> {
    state.worlds.dismiss_failure();
    Ok(())
}

/// "Projects'te buluşun": these friends get "Birlikte oyna" once the player is on Projects.
#[tauri::command]
#[specta::specta]
pub async fn host_meet_on_projects(
    state: State<'_, AppState>,
    friends: Vec<String>,
) -> CmdResult<()> {
    state.worlds.meet_on_projects(friends).await;
    Ok(())
}

/// "Katıl" on a world invite (also "Tekrar dene", "Yeniden katıl"); `copy_name`: "Kurulumu
/// kopyala", the copy's name in the player's language.
#[tauri::command]
#[specta::specta]
pub fn host_join(
    state: State<'_, AppState>,
    session: String,
    copy_name: Option<String>,
) -> CmdResult<()> {
    state.worlds.join(&session, copy_name);
    Ok(())
}

/// The answer to a join's question.
#[tauri::command]
#[specta::specta]
pub fn host_answer(
    state: State<'_, AppState>,
    session: String,
    answer: WorldJoinAnswer,
) -> CmdResult<()> {
    state.worlds.answer(&session, answer);
    Ok(())
}

/// "Vazgeç" on a join, or its game never started.
#[tauri::command]
#[specta::specta]
pub async fn host_cancel_join(state: State<'_, AppState>, session: String) -> CmdResult<()> {
    state.worlds.cancel_join(&session).await;
    Ok(())
}

/// "Kapat" on a world invite.
#[tauri::command]
#[specta::specta]
pub async fn host_dismiss_invite(state: State<'_, AppState>, session: String) -> CmdResult<()> {
    state.worlds.dismiss_invite(&session).await;
    Ok(())
}

/// "Bağlan" in game: the overlay moves the running game into the world, showing `label`.
#[tauri::command]
#[specta::specta]
pub async fn host_connect_in_game(
    state: State<'_, AppState>,
    session: String,
    label: String,
) -> CmdResult<bool> {
    Ok(state.worlds.connect_in_game(&session, label).await)
}
