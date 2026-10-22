//! The optional friends feature ("Arkadaşlar"): thin commands over `pl_social::SocialService`.
//! The service emits `social-view` (a full `SocialView` after every change) and `social-notice`
//! (moments worth a toast). The feature flag comes from the signed launcher config; debug builds
//! may point it at a local `wrangler dev` with `PROJECTS_SOCIAL_URL`. Teams follow
//! `features.teams` (debug builds: also `PROJECTS_SOCIAL_TEAMS=1`), chat `features.chat` (debug
//! builds: also `PROJECTS_SOCIAL_CHAT=1`); chat changes go out as `social-chat`.

use std::sync::Arc;

use pl_lang::pick;
use pl_mc::launch::{GameEvent, QuickPlay};
use pl_social::{
    CallAnswer, CallOutcome, ChatPage, CrewMuteEdit, ErrorCode, FriendGroup, GroupMember,
    InviteResult, JoinMode, JoinResult, LookupView, PermissionsEdit, PlanAnswer, ProfileUpdate,
    ReportReason, ReportTarget, SendOutcome, SessionInfo, SettingsMeta, SocialError, SocialService,
    SocialUpdate, SocialView, StatusMode, TeamEdit, TeamLook, TeamLookupView,
    TeamNotificationsEdit,
};
use tauri::{AppHandle, Emitter, State};

use crate::error::CmdResult;
use crate::state::AppState;

/// Builds the service; it stays off until [`apply_flag`] (or the debug override) turns it on.
/// `data_root` keeps the invites the player closed over a restart.
pub fn create(
    app: &AppHandle,
    http: &pl_net::Http,
    version: &str,
    runtime: tokio::runtime::Handle,
    ephemeral_secrets: bool,
    data_root: &std::path::Path,
) -> SocialService {
    let emitter = app.clone();
    let sink: pl_social::UpdateSink = Arc::new(move |update| {
        let _ = match update {
            SocialUpdate::View(view) => emitter.emit("social-view", &*view),
            SocialUpdate::Notice(notice) => emitter.emit("social-notice", &notice),
            SocialUpdate::Chat(change) => emitter.emit("social-chat", &change),
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
    service.keep_closed_invites_in(data_root.join("social").join("closed-invites.json"));
    service.keep_chat_in(data_root.join("social").join("chat"));
    service.keep_together_in(data_root.join("social").join("together.json"));
    service.set_teams_enabled(debug_teams());
    service.set_chat_enabled(debug_chat());
    service.configure(pl_social::service_url(false, debug_override().as_deref()));
    service
}

/// Turns the feature on or off from the signed config's `features.friends`, teams from
/// `features.teams` and chat from `features.chat` (both need friends on too).
pub fn apply_flag(service: &SocialService, friends: bool, teams: bool, chat: bool) {
    service.set_teams_enabled(teams || debug_teams());
    service.set_chat_enabled((friends && chat) || debug_chat());
    service.configure(pl_social::service_url(friends, debug_override().as_deref()));
}

/// `PROJECTS_SOCIAL_CHAT=1` (debug builds only): chat on whatever the config says, for a local
/// `wrangler dev`.
fn debug_chat() -> bool {
    #[cfg(debug_assertions)]
    {
        std::env::var("PROJECTS_SOCIAL_CHAT").is_ok_and(|value| value == "1")
    }
    #[cfg(not(debug_assertions))]
    {
        false
    }
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

/// "Arkadaş ekle" on a crewmate who is not a friend (FIX2): a request without their code.
#[tauri::command]
#[specta::specta]
pub async fn social_request_crewmate(
    state: State<'_, AppState>,
    user: String,
) -> CmdResult<SendOutcome> {
    Ok(state.social.send_request_to_crewmate(&user).await?)
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

/// Renames one of the player's crews (`team`), changes its tag, emblem, look or permissions (the
/// leader; a moderator the look where the crew lets them); unset fields stay, `motto: ""` takes
/// the motto away.
#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
pub async fn social_team_edit(
    state: State<'_, AppState>,
    team: String,
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
        join: None,
    };
    let text = edit.changes_text();
    match state.social.edit_team(&team, edit).await {
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

/// Makes a member of one of the player's crews a moderator, or a moderator a member again
/// (leader).
#[tauri::command]
#[specta::specta]
pub async fn social_team_moderator(
    state: State<'_, AppState>,
    team: String,
    id: String,
    moderator: bool,
) -> CmdResult<()> {
    Ok(state
        .social
        .set_team_moderator(&team, &id, moderator)
        .await?)
}

/// A mute the service turned down as past or too far ahead by its clock: this computer's clock is
/// off (a plain 400 reads as a wrong friend code elsewhere).
fn mute_refused(result: Result<(), SocialError>) -> CmdResult<()> {
    match result {
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

/// The player's crew notification settings for all crews, on a service that keeps them: "Sessize
/// al" (until `muteUntil`, or until unmuted without it), "Sesi aç" (`muted: false`), crew calls on
/// or off.
#[tauri::command]
#[specta::specta]
pub async fn social_team_notifications(
    state: State<'_, AppState>,
    change: TeamNotificationsEdit,
) -> CmdResult<()> {
    mute_refused(state.social.set_crew_notifications(change).await)
}

/// "Sessize al" for one of the player's crews (until `muteUntil`, or until unmuted without it) or
/// "Sesi aç", on a service with several crews.
#[tauri::command]
#[specta::specta]
pub async fn social_team_mute(
    state: State<'_, AppState>,
    team: String,
    change: CrewMuteEdit,
) -> CmdResult<()> {
    mute_refused(state.social.set_crew_mute(&team, change).await)
}

/// The player's crews in a new order (each once; the first becomes the primary crew).
#[tauri::command]
#[specta::specta]
pub async fn social_team_order(state: State<'_, AppState>, order: Vec<String>) -> CmdResult<()> {
    Ok(state.social.order_crews(order).await?)
}

/// "Ekibi çağır" for `crews` (empty: the primary crew). Waiting (called too recently), an open
/// call and not being on a multiplayer server (CALL1) are answers, not errors.
#[tauri::command]
#[specta::specta]
pub async fn social_team_call(
    state: State<'_, AppState>,
    crews: Vec<String>,
) -> CmdResult<CallOutcome> {
    Ok(state.social.call_crew(crews).await?)
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

/// Ends one of the player's crews for everyone (founder).
#[tauri::command]
#[specta::specta]
pub async fn social_team_disband(state: State<'_, AppState>, team: String) -> CmdResult<()> {
    Ok(state.social.disband_team(&team).await?)
}

/// Leaves one of the player's crews.
#[tauri::command]
#[specta::specta]
pub async fn social_team_leave(state: State<'_, AppState>, team: String) -> CmdResult<()> {
    Ok(state.social.leave_team(&team).await?)
}

/// A new code for one of the player's crews (founder); the old one stops working.
#[tauri::command]
#[specta::specta]
pub async fn social_team_rotate_code(
    state: State<'_, AppState>,
    team: String,
) -> CmdResult<String> {
    Ok(state.social.rotate_team_code(&team).await?)
}

/// Invites a friend to one of the player's crews.
#[tauri::command]
#[specta::specta]
pub async fn social_team_invite(
    state: State<'_, AppState>,
    team: String,
    id: String,
) -> CmdResult<()> {
    Ok(state.social.invite_to_team(&team, &id).await?)
}

/// Removes a member from one of the player's crews; the code is renewed with it.
#[tauri::command]
#[specta::specta]
pub async fn social_team_remove(
    state: State<'_, AppState>,
    team: String,
    id: String,
) -> CmdResult<()> {
    Ok(state.social.remove_team_member(&team, &id).await?)
}

/// Hands the founder role of one of the player's crews to another member.
#[tauri::command]
#[specta::specta]
pub async fn social_team_transfer(
    state: State<'_, AppState>,
    team: String,
    id: String,
) -> CmdResult<()> {
    Ok(state.social.transfer_team_founder(&team, &id).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn social_team_lookup(
    state: State<'_, AppState>,
    code: String,
) -> CmdResult<TeamLookupView> {
    Ok(state.social.lookup_team(&code).await?)
}

/// A daily limit (about a day to wait) reads better as the rule itself than as "too often".
fn daily(result: Result<JoinResult, SocialError>) -> CmdResult<JoinResult> {
    match result {
        Err(error @ SocialError::RateLimited { .. })
            if error
                .retry_after()
                .is_some_and(|wait| wait.as_secs() >= 3_600) =>
        {
            Err(crate::error::CmdError::new(
                "SOSYAL-EKIP-GUNLUK",
                pick(
                    "Bugünlük sınıra ulaştın. Yarın yeniden dene.",
                    "You've reached today's limit. Try again tomorrow.",
                ),
                error.to_string(),
            ))
        }
        other => Ok(other?),
    }
}

/// Joins the crew of a code; a crew that approves newcomers takes a join request instead.
#[tauri::command]
#[specta::specta]
pub async fn social_team_join(state: State<'_, AppState>, code: String) -> CmdResult<JoinResult> {
    daily(state.social.join_team(&code).await)
}

/// A link that was renewed or switched off looks like no crew: said as the link's, not a code's.
fn gone_link(error: SocialError) -> crate::error::CmdError {
    if error.code() == Some(&ErrorCode::TeamNotFound) {
        return crate::error::CmdError::new(
            "SOSYAL-EKIP-BAGLANTI-YOK",
            pick(
                "Bu davet bağlantısı artık geçerli değil.",
                "This invite link doesn't work anymore.",
            ),
            error.to_string(),
        );
    }
    error.into()
}

/// What a crew's invite link (`XXXX-XXXX-XXXX`) leads to.
#[tauri::command]
#[specta::specta]
pub async fn social_team_lookup_link(
    state: State<'_, AppState>,
    link: String,
) -> CmdResult<TeamLookupView> {
    state
        .social
        .lookup_crew_link(&link)
        .await
        .map_err(gone_link)
}

/// Joins the crew of an invite link, or asks to (a crew that approves newcomers).
#[tauri::command]
#[specta::specta]
pub async fn social_team_join_link(
    state: State<'_, AppState>,
    link: String,
) -> CmdResult<JoinResult> {
    match state.social.join_crew_link(&link).await {
        Err(error) if error.code() == Some(&ErrorCode::TeamNotFound) => Err(gone_link(error)),
        other => daily(other),
    }
}

/// Withdraws the player's waiting join request to a crew.
#[tauri::command]
#[specta::specta]
pub async fn social_team_request_withdraw(
    state: State<'_, AppState>,
    team: String,
) -> CmdResult<()> {
    Ok(state.social.withdraw_team_request(&team).await?)
}

/// Takes a player who asked into one of the player's crews (`accept`), or declines (silently).
#[tauri::command]
#[specta::specta]
pub async fn social_team_request_answer(
    state: State<'_, AppState>,
    team: String,
    id: String,
    accept: bool,
) -> CmdResult<()> {
    match state.social.answer_team_request(&team, &id, accept).await {
        // The player is in as many crews as they may be: say who cannot come, not "you".
        Err(error) if error.code() == Some(&ErrorCode::LimitCrews) => {
            Err(crate::error::CmdError::new(
                "SOSYAL-EKIP-ISTEK-SINIR",
                pick(
                    "Bu oyuncu daha fazla ekibe katılamıyor. İsteği bekliyor.",
                    "This player can't join any more crews. Their request keeps waiting.",
                ),
                error.to_string(),
            ))
        }
        other => Ok(other?),
    }
}

/// A new invite link for one of the player's crews (`on`, in place of the old one), or none.
/// Answers the link it has now.
#[tauri::command]
#[specta::specta]
pub async fn social_team_link(
    state: State<'_, AppState>,
    team: String,
    on: bool,
) -> CmdResult<Option<String>> {
    match state.social.set_team_link(&team, on).await {
        // The code and the link share 5 renewals a day.
        Err(error @ SocialError::RateLimited { .. }) => Err(crate::error::CmdError::new(
            "SOSYAL-EKIP-BAGLANTI",
            pick(
                "Kodu ve bağlantıyı bugün çok yeniledin. Yarın yeniden dene.",
                "You've renewed the code and the link a lot today. Try again tomorrow.",
            ),
            error.to_string(),
        )),
        other => Ok(other?),
    }
}

/// How newcomers with the code or the invite link get in (the leader).
#[tauri::command]
#[specta::specta]
pub async fn social_team_join_mode(
    state: State<'_, AppState>,
    team: String,
    join: JoinMode,
) -> CmdResult<()> {
    match state.social.set_join_mode(&team, join).await {
        // A PATCH counts against the 60 crew changes a day.
        Err(error @ SocialError::RateLimited { .. }) => Err(crate::error::CmdError::new(
            "SOSYAL-EKIP-DEGISIKLIK",
            pick(
                "Ekibi bugün çok değiştirdin. Yarın yeniden dene.",
                "You've changed the crew a lot today. Try again tomorrow.",
            ),
            error.to_string(),
        )),
        other => Ok(other?),
    }
}

/// A new plan in one of the player's crews at `at` (Unix seconds, a whole quarter of an hour),
/// with its short note (FIX2; empty or none: no note). Answers its id.
#[tauri::command]
#[specta::specta]
pub async fn social_plan_make(
    state: State<'_, AppState>,
    crew: String,
    at: i64,
    note: Option<String>,
) -> CmdResult<String> {
    match state.social.make_plan(&crew, at, note.as_deref()).await {
        // Five new plans a day.
        Err(error @ SocialError::RateLimited { .. }) => Err(crate::error::CmdError::new(
            "SOSYAL-PLAN-SINIR",
            pick(
                "Bugün en çok 5 plan yapabilirsin. Yarın yeniden dene.",
                "You can make up to 5 plans a day. Try again tomorrow.",
            ),
            error.to_string(),
        )),
        other => Ok(other?),
    }
}

/// "Geliyorum", "Belki" or "Gelemem" to a plan. False: the plan is gone.
#[tauri::command]
#[specta::specta]
pub async fn social_plan_answer(
    state: State<'_, AppState>,
    plan: String,
    answer: PlanAnswer,
) -> CmdResult<bool> {
    match state.social.answer_plan(&plan, answer).await {
        // Twenty changed answers a day; the answer stays as it was.
        Err(error @ SocialError::RateLimited { .. }) => Err(crate::error::CmdError::new(
            "SOSYAL-PLAN-YANIT",
            pick(
                "Yanıtını bugün çok değiştirdin. Yarın yeniden dene.",
                "You've changed your answer a lot today. Try again tomorrow.",
            ),
            error.to_string(),
        )),
        other => Ok(other?),
    }
}

/// "Planı sil".
#[tauri::command]
#[specta::specta]
pub async fn social_plan_cancel(state: State<'_, AppState>, plan: String) -> CmdResult<()> {
    Ok(state.social.cancel_plan(&plan).await?)
}

/// A plan's note (FIX2): its maker writes or changes it, `""` clears it (its maker, the crew's
/// leader and moderators).
#[tauri::command]
#[specta::specta]
pub async fn social_plan_note(
    state: State<'_, AppState>,
    plan: String,
    note: String,
) -> CmdResult<()> {
    match state.social.set_plan_note(&plan, &note).await {
        // Twenty changed answers and notes a day; the note stays as it was.
        Err(error @ SocialError::RateLimited { .. }) => Err(crate::error::CmdError::new(
            "SOSYAL-PLAN-NOT-SINIR",
            pick(
                "Planları bugün çok değiştirdin. Yarın yeniden dene.",
                "You've changed plans a lot today. Try again tomorrow.",
            ),
            error.to_string(),
        )),
        // A moderator's chat mute covers notes too.
        Err(error @ SocialError::ChatMuted { .. }) => Err(crate::error::CmdError::new(
            "SOSYAL-PLAN-NOT-SUSTURMA",
            pick("Şu an not yazamazsın.", "You can't write notes right now."),
            error.to_string(),
        )),
        other => Ok(other?),
    }
}

/// "Birlikte oynanan süre" on or off on this computer (off deletes what was kept).
#[tauri::command]
#[specta::specta]
pub fn social_together(state: State<'_, AppState>, on: bool) {
    state.social.set_together(on);
}

/// Deletes the time played together kept on this computer.
#[tauri::command]
#[specta::specta]
pub fn social_together_clear(state: State<'_, AppState>) {
    state.social.clear_together();
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

// --- chat (CHAT1) ---------------------------------------------------------------------------

/// A conversation's newest messages (`before` None) or those before a message id, oldest first.
#[tauri::command]
#[specta::specta]
pub fn social_chat_page(state: State<'_, AppState>, conv: String, before: Option<i64>) -> ChatPage {
    state.social.chat_page(&conv, before)
}

/// Sends a message: it shows at once and goes in the order typed. Returns its key.
#[tauri::command]
#[specta::specta]
pub fn social_chat_send(
    state: State<'_, AppState>,
    conv: String,
    text: String,
) -> CmdResult<String> {
    Ok(state.social.chat_send(&conv, &text)?)
}

/// "Tekrar dene" on a message that did not go.
#[tauri::command]
#[specta::specta]
pub fn social_chat_retry(state: State<'_, AppState>, key: String) -> CmdResult<()> {
    Ok(state.social.chat_retry(&key)?)
}

/// Drops a message that did not go.
#[tauri::command]
#[specta::specta]
pub fn social_chat_discard(state: State<'_, AppState>, key: String) {
    state.social.chat_discard(&key);
}

/// Edits the player's own message (within 15 minutes).
#[tauri::command]
#[specta::specta]
pub async fn social_chat_edit(
    state: State<'_, AppState>,
    conv: String,
    id: i64,
    text: String,
) -> CmdResult<()> {
    match state.social.chat_edit(&conv, id, &text).await {
        Err(error) if error.code() == Some(&ErrorCode::ChatTooOld) => {
            Err(crate::error::CmdError::new(
                "SOSYAL-SOHBET-SURE",
                pick(
                    "Bu mesajı düzenleme süresi doldu.",
                    "It's too late to edit this message.",
                ),
                error.to_string(),
            ))
        }
        other => Ok(other?),
    }
}

/// "Herkesten sil".
#[tauri::command]
#[specta::specta]
pub async fn social_chat_delete(
    state: State<'_, AppState>,
    conv: String,
    id: i64,
) -> CmdResult<()> {
    match state.social.chat_delete(&conv, id).await {
        Err(error) if error.code() == Some(&ErrorCode::ChatTooOld) => {
            Err(crate::error::CmdError::new(
                "SOSYAL-SOHBET-SURE",
                pick(
                    "Bu mesaj artık herkesten silinemez.",
                    "This message can't be deleted for everyone anymore.",
                ),
                error.to_string(),
            ))
        }
        other => Ok(other?),
    }
}

/// Everything in a conversation is read (it is on screen at its newest message).
#[tauri::command]
#[specta::specta]
pub fn social_chat_read(state: State<'_, AppState>, conv: String) {
    state.social.chat_read(&conv);
}

/// "Geçmişi temizle": the conversation's messages leave this computer.
#[tauri::command]
#[specta::specta]
pub fn social_chat_clear(state: State<'_, AppState>, conv: String) {
    state.social.chat_clear(&conv);
}

// --- reports and moderators' notes (MOD1) -----------------------------------------------------

/// "Rapor et": a player, a crew or a message, with a reason and an optional note (200
/// characters); `block` also blocks the player.
#[tauri::command]
#[specta::specta]
pub async fn social_report(
    state: State<'_, AppState>,
    target: ReportTarget,
    reason: ReportReason,
    note: Option<String>,
    block: bool,
) -> CmdResult<()> {
    match state.social.report(target, reason, note, block).await {
        // Ten reports a day: the plain "too often" reads better as the rule itself.
        Err(error @ SocialError::RateLimited { .. }) => Err(crate::error::CmdError::new(
            "SOSYAL-RAPOR-SINIR",
            pick(
                "Bugün çok rapor gönderdin. Yarın yeniden dene.",
                "You've sent a lot of reports today. Try again tomorrow.",
            ),
            error.to_string(),
        )),
        other => Ok(other?),
    }
}

/// The launcher showed these moderators' notes.
#[tauri::command]
#[specta::specta]
pub async fn social_notices_seen(state: State<'_, AppState>, ids: Vec<String>) -> CmdResult<()> {
    state.social.notices_seen(ids).await;
    Ok(())
}
