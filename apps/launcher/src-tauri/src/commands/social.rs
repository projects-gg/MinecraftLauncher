//! The optional friends feature ("Arkadaşlar"): thin commands over `pl_social::SocialService`.
//! The service emits `social-view` (a full `SocialView` after every change) and `social-notice`
//! (moments worth a toast). The feature flag comes from the signed launcher config; debug builds
//! may point it at a local `wrangler dev` with `PROJECTS_SOCIAL_URL`. Teams follow
//! `features.teams` (debug builds: also `PROJECTS_SOCIAL_TEAMS=1`).

use std::sync::Arc;

use pl_lang::pick;
use pl_mc::launch::{GameEvent, QuickPlay};
use pl_social::{
    CallAnswer, CallOutcome, ErrorCode, FriendGroup, GroupMember, InviteResult, LookupView,
    PermissionsEdit, ProfileUpdate, SendOutcome, SessionInfo, SettingsMeta, SocialError,
    SocialService, SocialUpdate, SocialView, StatusMode, TeamEdit, TeamLook, TeamLookupView,
    TeamNotificationsEdit,
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
    service.set_teams_enabled(debug_teams());
    service.configure(pl_social::service_url(false, debug_override().as_deref()));
    service
}

/// Turns the feature on or off from the signed config's `features.friends`, and teams from
/// `features.teams` (they need friends on too).
pub fn apply_flag(service: &SocialService, friends: bool, teams: bool) {
    service.set_teams_enabled(teams || debug_teams());
    service.configure(pl_social::service_url(friends, debug_override().as_deref()));
}

/// `PROJECTS_SOCIAL_TEAMS=1` (debug builds only): teams on whatever the config says, for a
/// local `wrangler dev`.
fn debug_teams() -> bool {
    #[cfg(debug_assertions)]
    {
        std::env::var("PROJECTS_SOCIAL_TEAMS").is_ok_and(|value| value == "1")
    }
    #[cfg(not(debug_assertions))]
    {
        false
    }
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

/// Follows one game session for presence: in game once its window is on screen (while it opens
/// the player is still in the launcher), the log, exited.
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
            GameEvent::WindowShown => self.social.game_started(self.quick_play.as_deref()),
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
            pick("Bu resim seçilemedi.", "This picture couldn't be picked."),
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

/// "Discord'dan mesaj at": the person's profile in the Discord app when it is installed (Message
/// is one click away there). Only `discord://-/users/<id>` with a numeric Discord user id is ever
/// opened; the generic opener stays `https://`-only. False when there is no app (or it would not
/// open): the UI opens the web profile instead.
#[tauri::command]
#[specta::specta]
pub async fn social_open_in_discord(id: String) -> bool {
    tauri::async_runtime::spawn_blocking(move || {
        pl_desktop::discord::open_profile_in_app(&id, |link| {
            tauri_plugin_opener::open_url(link, None::<&str>)
                .inspect_err(|error| tracing::warn!(%error, "could not open the Discord app"))
                .is_ok()
        })
    })
    .await
    .unwrap_or(false)
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
        crate::error::CmdError::new(
            "SOSYAL-YEDEK",
            pick("Ayarlar yedeklenemedi.", "Couldn't back up your settings."),
            error.to_string(),
        )
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

/// Saves the player's friend categories: their order, names and who is in which (friends left
/// out are in the implicit "Arkadaşlar"). Shown at once; saved in the background.
#[tauri::command]
#[specta::specta]
pub async fn social_groups_save(
    state: State<'_, AppState>,
    groups: Vec<FriendGroup>,
    members: Vec<GroupMember>,
) -> CmdResult<()> {
    Ok(state.social.save_groups(groups, members).await?)
}

/// Founds a crew. `emblem` is one of the service's emblem names; `color` and `frame` (and the
/// motto) are its look, sent only to a service that offers looks.
#[tauri::command]
#[specta::specta]
pub async fn social_team_create(
    state: State<'_, AppState>,
    name: String,
    tag: String,
    emblem: String,
    color: Option<String>,
    frame: Option<String>,
    motto: Option<String>,
) -> CmdResult<()> {
    let look = match (color, frame) {
        (Some(color), Some(frame)) => Some(TeamLook {
            color,
            frame,
            motto: motto.unwrap_or_default(),
        }),
        _ => None,
    };
    Ok(state.social.create_team(&name, &tag, &emblem, look).await?)
}

/// Renames the crew, changes its tag, emblem, look or permissions (the leader; a moderator the
/// look where the crew lets them); unset fields stay, `motto: ""` takes the motto away.
#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
pub async fn social_team_edit(
    state: State<'_, AppState>,
    name: Option<String>,
    tag: Option<String>,
    emblem: Option<String>,
    color: Option<String>,
    frame: Option<String>,
    motto: Option<String>,
    permissions: Option<PermissionsEdit>,
) -> CmdResult<()> {
    let edit = TeamEdit {
        name,
        tag,
        emblem,
        color,
        frame,
        motto,
        permissions,
    };
    let text = edit.changes_text();
    match state.social.edit_team(edit).await {
        // A crew's name, tag and motto change at most 10 times a day: the service says so with a
        // plain "too often", which reads better as the rule itself.
        Err(error @ SocialError::RateLimited { .. }) if text => Err(crate::error::CmdError::new(
            "SOSYAL-EKIP-METIN",
            pick(
                "Ekibin adı, etiketi ve sloganı günde en çok 10 kez değişebilir. Yarın yeniden dene.",
                "A crew's name, tag and motto can change 10 times a day. Try again tomorrow.",
            ),
            error.to_string(),
        )),
        other => Ok(other?),
    }
}

/// Makes a member a moderator, or a moderator a member again (leader).
#[tauri::command]
#[specta::specta]
pub async fn social_team_moderator(
    state: State<'_, AppState>,
    id: String,
    moderator: bool,
) -> CmdResult<()> {
    Ok(state.social.set_team_moderator(&id, moderator).await?)
}

/// The player's crew notification settings, on a service that keeps them: "Sessize al" (until
/// `muteUntil`, or until unmuted without it), "Sesi aç" (`muted: false`), crew calls on or off.
#[tauri::command]
#[specta::specta]
pub async fn social_team_notifications(
    state: State<'_, AppState>,
    change: TeamNotificationsEdit,
) -> CmdResult<()> {
    match state.social.set_crew_notifications(change).await {
        // The service turns down a mute whose end its clock reads as past or too far ahead: this
        // computer's clock is off (a plain 400 reads as a wrong friend code elsewhere).
        Err(error) if error.code() == Some(&ErrorCode::BadRequest) => {
            Err(crate::error::CmdError::new(
                "SOSYAL-EKIP-SESSIZ",
                pick(
                    "Sessize alınamadı. Bilgisayarının saatini kontrol et.",
                    "Couldn't mute. Check your computer's clock.",
                ),
                error.to_string(),
            ))
        }
        other => Ok(other?),
    }
}

/// "Ekibi çağır". A wait (called too recently) and an open call are answers, not errors.
#[tauri::command]
#[specta::specta]
pub async fn social_team_call(state: State<'_, AppState>) -> CmdResult<CallOutcome> {
    Ok(state.social.call_crew().await?)
}

/// "Geliyorum", "Birazdan" or "Gelemem" to a teammate's call. False: the call is gone.
#[tauri::command]
#[specta::specta]
pub async fn social_team_call_answer(
    state: State<'_, AppState>,
    call: String,
    answer: CallAnswer,
) -> CmdResult<bool> {
    Ok(state.social.answer_call(&call, answer).await?)
}

/// "Bitir": ends the player's own call.
#[tauri::command]
#[specta::specta]
pub async fn social_team_call_end(state: State<'_, AppState>, call: String) -> CmdResult<()> {
    Ok(state.social.end_call(&call).await?)
}

/// Ends the team for everyone (founder).
#[tauri::command]
#[specta::specta]
pub async fn social_team_disband(state: State<'_, AppState>) -> CmdResult<()> {
    Ok(state.social.disband_team().await?)
}

#[tauri::command]
#[specta::specta]
pub async fn social_team_leave(state: State<'_, AppState>) -> CmdResult<()> {
    Ok(state.social.leave_team().await?)
}

/// A new team code (founder); the old one stops working.
#[tauri::command]
#[specta::specta]
pub async fn social_team_rotate_code(state: State<'_, AppState>) -> CmdResult<String> {
    Ok(state.social.rotate_team_code().await?)
}

/// Invites a friend to the player's team.
#[tauri::command]
#[specta::specta]
pub async fn social_team_invite(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    Ok(state.social.invite_to_team(&id).await?)
}

/// Removes a member (founder); the code is renewed with it.
#[tauri::command]
#[specta::specta]
pub async fn social_team_remove(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    Ok(state.social.remove_team_member(&id).await?)
}

/// Hands the founder role to another member.
#[tauri::command]
#[specta::specta]
pub async fn social_team_transfer(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    Ok(state.social.transfer_team_founder(&id).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn social_team_lookup(
    state: State<'_, AppState>,
    code: String,
) -> CmdResult<TeamLookupView> {
    Ok(state.social.lookup_team(&code).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn social_team_join(state: State<'_, AppState>, code: String) -> CmdResult<()> {
    Ok(state.social.join_team(&code).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn social_team_accept(state: State<'_, AppState>, team: String) -> CmdResult<()> {
    Ok(state.social.accept_team_invite(&team).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn social_team_decline(state: State<'_, AppState>, team: String) -> CmdResult<()> {
    Ok(state.social.decline_team_invite(&team).await?)
}
