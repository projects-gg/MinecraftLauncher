//! "Sesli sohbet" (VOICE): the page's commands over `pl_voice::Voice` (`crate::voice`). Every
//! change reaches the page as `voice-state` (the whole `VoiceState`), toasts as `voice-notice`,
//! who speaks as `voice-speaking` (only while the window is on screen) and the microphone's level
//! for the settings page as `voice-level`. Refusals are answers (`VoiceAnswer`), never errors.
//!
//! The hidden voice window's page has two commands of its own (`voice_page_attach`,
//! `voice_page_report`); it may call nothing else, and no other window may call those
//! (`pl_voice::guard`, checked before any command runs).

use pl_voice::discord::VoiceDiscordOffer;
use pl_voice::engine::{EngineCommand, PageReport};
use pl_voice::settings::{VoiceBackup, VoiceSettings};
use pl_voice::view::{
    VoiceAnswer, VoiceCrewOp, VoiceKeyAsk, VoiceKeyCheck, VoiceModAction, VoiceState,
};
use tauri::State;
use tauri::ipc::Channel;

use crate::error::CmdResult;
use crate::state::AppState;

/// The state now (the page's start; every change after comes as `voice-state`).
#[tauri::command]
#[specta::specta]
pub fn voice_state(state: State<'_, AppState>) -> CmdResult<VoiceState> {
    Ok(state.voice.state())
}

/// "Ara" on a friend: the player waits in the new room while it rings ("Aranıyor…").
#[tauri::command]
#[specta::specta]
pub async fn voice_call(state: State<'_, AppState>, friend: String) -> CmdResult<VoiceAnswer> {
    Ok(state.voice.call(friend).await)
}

/// "Katıl" on a ring, or on a missed call that is still joinable.
#[tauri::command]
#[specta::specta]
pub async fn voice_accept(state: State<'_, AppState>, room: String) -> CmdResult<VoiceAnswer> {
    Ok(state.voice.accept(room).await)
}

/// "Reddet" on a ring.
#[tauri::command]
#[specta::specta]
pub fn voice_decline(state: State<'_, AppState>, room: String) -> CmdResult<()> {
    state.voice.decline(&room);
    Ok(())
}

/// Takes a missed call off the list.
#[tauri::command]
#[specta::specta]
pub fn voice_dismiss_missed(state: State<'_, AppState>, room: String) -> CmdResult<()> {
    state.voice.dismiss_missed(&room);
    Ok(())
}

/// "Katıl" on a crew room: `room` one of the crew's live rooms, none for its "Genel".
#[tauri::command]
#[specta::specta]
pub async fn voice_join_crew(
    state: State<'_, AppState>,
    crew: String,
    room: Option<String>,
) -> CmdResult<VoiceAnswer> {
    Ok(state.voice.join_crew(crew, room, false).await)
}

/// "Yeni oda" in a crew: the player joins it at once.
#[tauri::command]
#[specta::specta]
pub async fn voice_new_crew_room(
    state: State<'_, AppState>,
    crew: String,
) -> CmdResult<VoiceAnswer> {
    Ok(state.voice.join_crew(crew, None, true).await)
}

/// VOICE-ROOMS: a change of a crew's rooms from the crew page ("Odayı düzenle", the order,
/// "Odayı sil", a crewmate dragged to another room); the crew's new list comes as `voice-state`.
#[tauri::command]
#[specta::specta]
pub async fn voice_crew_change(
    state: State<'_, AppState>,
    op: VoiceCrewOp,
) -> CmdResult<VoiceAnswer> {
    Ok(state.voice.crew_change(op).await)
}

/// "Ayrıl"; also "İptal" on the "Aranıyor…" pill (the call ends with it).
#[tauri::command]
#[specta::specta]
pub fn voice_leave(state: State<'_, AppState>) -> CmdResult<()> {
    state.voice.leave();
    Ok(())
}

/// "Tekrar dene" after "Bağlanamadı.".
#[tauri::command]
#[specta::specta]
pub async fn voice_retry(state: State<'_, AppState>) -> CmdResult<VoiceAnswer> {
    Ok(state.voice.retry().await)
}

/// "Mikrofonu kapat" / "Mikrofonu aç" (opening it also ends "Sağırlaştır").
#[tauri::command]
#[specta::specta]
pub fn voice_set_mute(state: State<'_, AppState>, on: bool) -> CmdResult<()> {
    state.voice.set_mute(on);
    Ok(())
}

/// "Sağırlaştır" / "Sağırlaştırmayı kaldır".
#[tauri::command]
#[specta::specta]
pub fn voice_set_deaf(state: State<'_, AppState>, on: bool) -> CmdResult<()> {
    state.voice.set_deaf(on);
    Ok(())
}

/// A person's volume for the player (percent, 0–200), remembered on this computer.
#[tauri::command]
#[specta::specta]
pub fn voice_set_volume(state: State<'_, AppState>, person: String, percent: u16) -> CmdResult<()> {
    state.voice.set_volume(&person, percent);
    Ok(())
}

/// "Sustur (yalnız sen)", remembered on this computer.
#[tauri::command]
#[specta::specta]
pub fn voice_set_muted_for_me(
    state: State<'_, AppState>,
    person: String,
    on: bool,
) -> CmdResult<()> {
    state.voice.set_muted_for_me(&person, on);
    Ok(())
}

/// "Davet et": a friend (friend rooms) or a crewmate (crew rooms).
#[tauri::command]
#[specta::specta]
pub async fn voice_invite(state: State<'_, AppState>, friend: String) -> CmdResult<VoiceAnswer> {
    Ok(state.voice.invite(friend).await)
}

/// "İptal" on the player's ringing invite.
#[tauri::command]
#[specta::specta]
pub async fn voice_cancel_invite(
    state: State<'_, AppState>,
    friend: String,
) -> CmdResult<VoiceAnswer> {
    Ok(state.voice.cancel_invite(friend).await)
}

/// A leader's or moderator's action ("Herkes için sustur", "Odadan çıkar" and its "Geri al",
/// "Taşı", "Moderatör yap", the room's name, "Odayı kapat").
#[tauri::command]
#[specta::specta]
pub async fn voice_moderate(
    state: State<'_, AppState>,
    action: VoiceModAction,
) -> CmdResult<VoiceAnswer> {
    Ok(state.voice.moderate(action).await)
}

/// Settings → Sesli sohbet.
#[tauri::command]
#[specta::specta]
pub fn voice_settings(state: State<'_, AppState>) -> CmdResult<VoiceSettings> {
    Ok(state.voice.settings())
}

/// Saves Settings → Sesli sohbet; answers them as kept (a key used twice stays with the first
/// shortcut, values held to their ranges).
#[tauri::command]
#[specta::specta]
pub fn voice_save_settings(
    state: State<'_, AppState>,
    settings: VoiceSettings,
) -> CmdResult<VoiceSettings> {
    let kept = state.voice.save_settings(settings);
    state.social.set_voice_switch(kept.enabled);
    Ok(kept)
}

/// What the key picker says about a key (Simple Voice Chat's, the overlay's, the game's, Left
/// Alt, another shortcut's).
#[tauri::command]
#[specta::specta]
pub fn voice_check_key(state: State<'_, AppState>, ask: VoiceKeyAsk) -> CmdResult<VoiceKeyCheck> {
    Ok(state.voice.check_key(&ask))
}

/// Discord's push-to-talk, mute and deafen keys on this computer, offered once as the player first
/// opens voice chat (VOICE-KEYS); null when answered before or when Discord has nothing to copy.
/// Discord's local files are read only now, never at the launcher's start; nothing leaves the
/// computer.
#[tauri::command]
#[specta::specta]
pub async fn voice_discord_offer(
    state: State<'_, AppState>,
) -> CmdResult<Option<VoiceDiscordOffer>> {
    let voice = state.voice.clone();
    let offer = tauri::async_runtime::spawn_blocking(move || {
        voice.discord_offer(crate::voice::input::discord_keys)
    })
    .await
    .unwrap_or(None);
    Ok(offer)
}

/// The answer to Discord's keys: "Kopyala" (`copy`) fills the shortcuts, "Hayır" leaves them;
/// never asked again. Answers the settings as kept.
#[tauri::command]
#[specta::specta]
pub fn voice_discord_answer(state: State<'_, AppState>, copy: bool) -> CmdResult<VoiceSettings> {
    Ok(state.voice.discord_answer(copy))
}

/// The settings page opened (`open`) or closed: the voice engine runs meanwhile for the device
/// lists and the microphone's level bar.
#[tauri::command]
#[specta::specta]
pub fn voice_setup(state: State<'_, AppState>, open: bool) -> CmdResult<()> {
    state.voice.set_setup(open);
    Ok(())
}

/// "Mikrofonu dene": the player hears themselves; nothing goes anywhere.
#[tauri::command]
#[specta::specta]
pub fn voice_self_test(state: State<'_, AppState>, on: bool) -> CmdResult<()> {
    state.voice.set_test(on);
    Ok(())
}

/// The page shows the live meters (`on`: its window on screen with a room's pill or panel) or not
/// (VOICE-CALL): `voice-levels` comes meanwhile, and the voice page measures only then.
#[tauri::command]
#[specta::specta]
pub fn voice_levels(state: State<'_, AppState>, on: bool) -> CmdResult<()> {
    state.voice.set_meters(on);
    Ok(())
}

/// "Ayarları aç" under "Mikrofona izin verilmedi.": Windows' microphone privacy page.
#[tauri::command]
#[specta::specta]
pub fn voice_open_mic_settings() -> CmdResult<()> {
    crate::voice::open_mic_settings();
    Ok(())
}

/// What the settings backup carries of voice chat (the page adds it as `voice`).
#[tauri::command]
#[specta::specta]
pub fn voice_backup(state: State<'_, AppState>) -> CmdResult<VoiceBackup> {
    Ok(state.voice.backup())
}

/// A downloaded backup's voice part (this computer's devices stay).
#[tauri::command]
#[specta::specta]
pub fn voice_restore(state: State<'_, AppState>, backup: VoiceBackup) -> CmdResult<()> {
    state.voice.restore(backup);
    state
        .social
        .set_voice_switch(state.voice.settings().enabled);
    Ok(())
}

/// The voice page (the hidden `voice` window only): hands over its channel for the launcher's
/// commands; the launcher configures it and, in a room, starts the call.
#[tauri::command]
#[specta::specta]
pub fn voice_page_attach(
    state: State<'_, AppState>,
    on_command: Channel<EngineCommand>,
) -> CmdResult<()> {
    crate::voice::attach(&state.voice, on_command);
    Ok(())
}

/// The voice page (the hidden `voice` window only): what happened there.
#[tauri::command]
#[specta::specta]
pub fn voice_page_report(state: State<'_, AppState>, report: PageReport) -> CmdResult<()> {
    state.voice.page_report(report);
    Ok(())
}
