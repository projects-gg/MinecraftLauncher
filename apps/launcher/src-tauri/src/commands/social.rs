//! The optional friends feature ("Arkadaşlar"): thin commands over `pl_social::SocialService`.
//! The service emits `social-view` (a full `SocialView` after every change) and `social-notice`
//! (moments worth a toast). The feature flag comes from the signed launcher config; debug builds
//! may point it at a local `wrangler dev` with `PROJECTS_SOCIAL_URL`.

use std::sync::Arc;

use pl_mc::launch::{GameEvent, QuickPlay};
use pl_social::{
    InviteResult, LookupView, ProfileUpdate, SendOutcome, SessionInfo, SettingsMeta, SocialService,
    SocialUpdate, SocialView, StatusMode,
};
use tauri::{AppHandle, Emitter, State};

use crate::error::CmdResult;
use crate::state::AppState;

/// Builds the service; it stays off until [`apply_flag`] (or the debug override) turns it on.
pub fn create(
    app: &AppHandle,
    http: &pl_net::Http,
    version: &str,
    runtime: tokio::runtime::Handle,
    ephemeral_secrets: bool,
) -> SocialService {
    let emitter = app.clone();
    let sink: pl_social::UpdateSink = Arc::new(move |update| {
        let _ = match update {
            SocialUpdate::View(view) => emitter.emit("social-view", &*view),
            SocialUpdate::Notice(notice) => emitter.emit("social-notice", &notice),
        };
    });
    let tokens: Arc<dyn pl_social::TokenStore> =
        if ephemeral_secrets || !pl_social::KeyringTokenStore::is_available() {
            Arc::new(pl_social::MemoryTokenStore::new())
        } else {
            Arc::new(pl_social::KeyringTokenStore::with_service(
                crate::identity::current().keychain_service.clone(),
            ))
        };
    let service = SocialService::new(
        http.client().clone(),
        pl_social::ServiceConfig {
            user_agent: format!("ProjectsLauncher/{version}"),
            device: device_label(),
            presence: pl_social::PresenceOptions::default(),
        },
        tokens,
        runtime,
        sink,
    );
    service.configure(pl_social::service_url(false, debug_override().as_deref()));
    service
}

/// Turns the feature on or off from the signed config's `features.friends`.
pub fn apply_flag(service: &SocialService, friends: bool) {
    service.configure(pl_social::service_url(friends, debug_override().as_deref()));
}

/// `PROJECTS_SOCIAL_URL` (debug builds only): forces the feature on against that service.
fn debug_override() -> Option<String> {
    #[cfg(debug_assertions)]
    {
        std::env::var("PROJECTS_SOCIAL_URL")
            .ok()
            .filter(|url| !url.trim().is_empty())
    }
    #[cfg(not(debug_assertions))]
    {
        None
    }
}

/// The computer's name, shown in the sessions list and on the settings backup.
fn device_label() -> String {
    let name = tauri_plugin_os::hostname();
    let name = name.trim();
    if name.is_empty() {
        "Bilgisayar".to_owned()
    } else {
        name.to_owned()
    }
}

/// Follows one game session for presence: started, the log, exited.
pub struct GamePresence {
    social: SocialService,
    quick_play: Option<String>,
}

impl GamePresence {
    pub fn new(social: &SocialService, quick_play: Option<&QuickPlay>) -> Self {
        let quick_play = match quick_play {
            Some(QuickPlay::Multiplayer { host, port }) => {
                pl_social::server_label(host, port.unwrap_or(25565))
            }
            _ => None,
        };
        Self {
            social: social.clone(),
            quick_play,
        }
    }

    pub fn on_event(&self, event: &GameEvent) {
        match event {
            GameEvent::Started { .. } => self.social.game_started(self.quick_play.as_deref()),
            GameEvent::Line(line) => self.social.game_line(&line.message),
            GameEvent::Exited { .. } => self.social.game_stopped(),
            _ => {}
        }
    }
}

#[tauri::command]
#[specta::specta]
pub fn social_state(state: State<'_, AppState>) -> SocialView {
    state.social.view()
}

/// Opens Discord's sign-in page in the browser; the page comes back through
/// `projects://auth/done`. Returns the page so the UI can open it again.
#[tauri::command]
#[specta::specta]
pub async fn social_sign_in(app: AppHandle, state: State<'_, AppState>) -> CmdResult<String> {
    let url = state.social.begin_sign_in().await?;
    // Debug builds can skip the browser for automated tests (the test plays the browser).
    #[cfg(debug_assertions)]
    let open = std::env::var_os("PROJECTS_SOCIAL_NO_BROWSER").is_none();
    #[cfg(not(debug_assertions))]
    let open = true;
    if open {
        use tauri_plugin_opener::OpenerExt;
        if let Err(error) = app.opener().open_url(url.as_str(), None::<&str>) {
            tracing::warn!(%error, "could not open the browser for the friends sign-in");
        }
    }
    Ok(url.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn social_sign_in_cancel(state: State<'_, AppState>) {
    state.social.cancel_sign_in();
}

#[tauri::command]
#[specta::specta]
pub async fn social_sign_out(state: State<'_, AppState>) -> CmdResult<()> {
    state.social.sign_out().await;
    Ok(())
}

/// "Tekrar dene" after an outage or when another device took over.
#[tauri::command]
#[specta::specta]
pub async fn social_refresh(state: State<'_, AppState>) -> CmdResult<()> {
    state.social.refresh().await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn social_lookup(state: State<'_, AppState>, code: String) -> CmdResult<LookupView> {
    Ok(state.social.lookup(&code).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn social_send_request(
    state: State<'_, AppState>,
    code: String,
) -> CmdResult<SendOutcome> {
    Ok(state.social.send_request(&code).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn social_accept(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    Ok(state.social.accept(&id).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn social_decline(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    Ok(state.social.decline(&id).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn social_cancel_request(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    Ok(state.social.cancel_request(&id).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn social_remove_friend(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    Ok(state.social.remove_friend(&id).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn social_block(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    Ok(state.social.block(&id).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn social_unblock(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    Ok(state.social.unblock(&id).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn social_rotate_code(state: State<'_, AppState>) -> CmdResult<String> {
    Ok(state.social.rotate_code().await?)
}

/// Çevrimiçi / Rahatsız etmeyin / Görünmez.
#[tauri::command]
#[specta::specta]
pub async fn social_set_status(state: State<'_, AppState>, status: StatusMode) -> CmdResult<()> {
    Ok(state
        .social
        .update_profile(ProfileUpdate::default().status(status))
        .await?)
}

#[tauri::command]
#[specta::specta]
pub async fn social_set_privacy(
    state: State<'_, AppState>,
    share_server: Option<bool>,
    share_last_seen: Option<bool>,
) -> CmdResult<()> {
    let update = ProfileUpdate {
        share_server,
        share_last_seen,
        ..ProfileUpdate::default()
    };
    Ok(state.social.update_profile(update).await?)
}

/// `discord`, `preset:<slug>` or `minecraft:<texture hash>`.
#[tauri::command]
#[specta::specta]
pub async fn social_set_avatar(state: State<'_, AppState>, avatar: String) -> CmdResult<()> {
    let avatar = pl_social::Avatar::parse(&avatar).ok_or_else(|| {
        crate::error::CmdError::new(
            "SOSYAL-AVATAR",
            "Bu resim seçilemedi.",
            "invalid avatar choice",
        )
    })?;
    Ok(state
        .social
        .update_profile(ProfileUpdate::default().avatar(&avatar))
        .await?)
}

/// "Birlikte oyna".
#[tauri::command]
#[specta::specta]
pub async fn social_invite(state: State<'_, AppState>, id: String) -> CmdResult<InviteResult> {
    Ok(state.social.invite(&id).await?)
}

#[tauri::command]
#[specta::specta]
pub fn social_dismiss_invite(state: State<'_, AppState>, id: String) {
    state.social.dismiss_invite(&id);
}

#[tauri::command]
#[specta::specta]
pub async fn social_sessions(state: State<'_, AppState>) -> CmdResult<Vec<SessionInfo>> {
    Ok(state.social.sessions().await?)
}

#[tauri::command]
#[specta::specta]
pub async fn social_revoke_session(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    Ok(state.social.revoke_session(&id).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn social_revoke_other_sessions(state: State<'_, AppState>) -> CmdResult<u32> {
    Ok(state.social.revoke_other_sessions().await?)
}

/// "Verilerimi sil".
#[tauri::command]
#[specta::specta]
pub async fn social_delete_account(state: State<'_, AppState>) -> CmdResult<()> {
    Ok(state.social.delete_account().await?)
}

/// Saves launcher preferences: a JSON object the UI builds from an allow-list (never secrets).
/// JSON travels as text: specta cannot describe an arbitrary JSON value.
#[tauri::command]
#[specta::specta]
pub async fn social_backup_upload(
    state: State<'_, AppState>,
    data: String,
) -> CmdResult<SettingsMeta> {
    let data: serde_json::Value = serde_json::from_str(&data).map_err(|error| {
        crate::error::CmdError::new("SOSYAL-YEDEK", "Ayarlar yedeklenemedi.", error.to_string())
    })?;
    Ok(state.social.upload_settings(data).await?)
}

/// The saved preferences as JSON text, or None when there is no backup.
#[tauri::command]
#[specta::specta]
pub async fn social_backup_download(state: State<'_, AppState>) -> CmdResult<Option<String>> {
    Ok(state
        .social
        .download_settings()
        .await?
        .map(|data| data.to_string()))
}

/// Turns the backup off and deletes the stored copy.
#[tauri::command]
#[specta::specta]
pub async fn social_backup_delete(state: State<'_, AppState>) -> CmdResult<()> {
    Ok(state.social.delete_settings().await?)
}
