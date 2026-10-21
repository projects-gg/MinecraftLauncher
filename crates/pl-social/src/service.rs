//! The friends feature of one launcher: the feature flag, Discord sign-in, the session token in
//! the keychain, the presence connection, the state the UI shows and what the game is doing.
//! Cheap to clone; the launcher keeps one.
//!
//! Every change reaches the UI through the sink as a full [`SocialView`] (small: at most a few
//! hundred friends) or a [`SocialNotice`]. Async results carry a generation number, so a
//! sign-out or a reconfiguration is never overwritten by a call that was still running.
//!
//! Free-plan rules (docs/social.md §2): the friend list is fetched at start and after a
//! reconnect, never on a timer; presence arrives as events; nothing retries in a loop (the
//! presence task backs off by itself, a manual "try again" is throttled).

use std::collections::HashSet;
use std::future::Future;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tokio::runtime::Handle;
use url::Url;

use crate::auth::{LoginAttempt, SignedIn, auth_code_from_deep_link};
use crate::chat::{
    EDIT_WINDOW_MS, PAGE_SIZE, REPORT_CONTEXT, RESEND_WINDOW_MS, TOO_FAST_WAIT_MAX, Viewer,
    change_view, entry_view, failure_of, merge, message_view, new_nonce, prepare_text,
};
use crate::client::{CrewKey, Session, SocialClient, SocialConfig};
use crate::closed::ClosedInvitesFile;
use crate::codes::{crew_code, normalize_crew_link, normalize_friend_code, normalize_team_code};
use crate::error::{ErrorCode, Result, SocialError};
use crate::game::{self, GameActivity};
use crate::history::{Change, Conversation, History};
use crate::model::{
    ChatMessage, ChatStatus, CrewMuteEdit, EditFeatures, FIRST_EMBLEMS, FriendGroup, FriendGroups,
    JoinMode, JoinOutcome, LimitedKey, MAX_FRIEND_GROUPS, ProfileUpdate, ReportReason,
    RequestOutcome, SessionInfo, SettingsMeta, SettingsPrecondition, TEAM_FEATURE_CALLS,
    TEAM_FEATURE_LINK, TEAM_FEATURE_LOOK, TEAM_FEATURE_MULTI, TEAM_FEATURE_MUTE,
    TEAM_FEATURE_PLANS, TEAM_FEATURE_REQUESTS, TEAM_FEATURE_ROLES, TeamEdit, TeamLook, TeamLookup,
    TeamNotificationsEdit, TeamRole,
};
use crate::presence::{ChatSetup, PresenceControl, PresenceEvent, PresenceOptions, StopReason};
use crate::protocol::{CallAnswer, LocalStatus, PlanAnswer};
use crate::qr::{QrPath, qr_path};
use crate::state::{Connection, SocialState};
use crate::store::TokenStore;
use crate::together::{Periods, Together, TogetherLog};
use crate::view::{
    ActivityView, AuthPhase, CallOutcome, CallPlace, ChatChangeView, ChatPage, ChatView,
    ConversationView, CrewNotificationsView, GroupMember, InviteResult, JoinResult,
    LastMessageView, LookupView, ProfileView, ReportTarget, RestrictionView, SendFailure,
    SendOutcome, SignInFailure, SocialNotice, SocialView, TeamLookupView, me_view,
};

/// After a reconnect the friend list is fetched again only when it is older than this.
const REFETCH_AFTER: Duration = Duration::from_secs(60);
/// "Tekrar dene" is honoured at most this often.
const MANUAL_REFRESH_GAP: Duration = Duration::from_secs(10);
const LOGOUT_TIMEOUT: Duration = Duration::from_secs(5);

/// What the service tells the UI.
#[derive(Debug, Clone)]
pub enum SocialUpdate {
    View(Box<SocialView>),
    Notice(SocialNotice),
    /// What changed in one chat conversation (CHAT1).
    Chat(ChatChangeView),
}

/// Chat events that came before the history could be opened (the profile was not known yet:
/// the service was down at start) wait at most this many.
const CHAT_BACKLOG_MAX: usize = 2_000;

pub type UpdateSink = Arc<dyn Fn(SocialUpdate) + Send + Sync>;

#[derive(Debug, Clone)]
pub struct ServiceConfig {
    /// `ProjectsLauncher/<version>`.
    pub user_agent: String,
    /// This computer's name: the sessions list and the settings backup show it.
    pub device: String,
    pub presence: PresenceOptions,
}

#[derive(Clone)]
pub struct SocialService {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for SocialService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SocialService").finish_non_exhaustive()
    }
}

struct Inner {
    http: reqwest::Client,
    config: ServiceConfig,
    tokens: Arc<dyn TokenStore>,
    sink: UpdateSink,
    runtime: Handle,
    core: Mutex<Core>,
}

#[derive(Default)]
struct Core {
    url: Option<Url>,
    client: Option<SocialClient>,
    session: Option<Session>,
    presence: Option<PresenceControl>,
    /// Bumped by every sign-out and reconfiguration: older async results are dropped.
    generation: u64,
    /// Bumped by every new presence connection: events of a replaced one are dropped.
    epoch: u64,
    seq: u64,
    login: Option<LoginAttempt>,
    auth: AuthPhase,
    state: SocialState,
    activity: LocalStatus,
    game: Option<GameActivity>,
    settings_version: Option<u64>,
    fetched_at: Option<Instant>,
    fetching: bool,
    last_manual: Option<Instant>,
    /// "Verilerimi sil" is running here: the socket closing because the account is gone is this
    /// deletion, not a sign-out from elsewhere.
    deleting: bool,
    /// Share link and its QR, computed once per friend code.
    qr: Option<(String, Option<QrPath>)>,
    /// Teams are on in the signed config (`features.teams`). Survives sign-outs.
    teams: bool,
    /// A friend categories save is on its way; the newest wanted document waits in
    /// `groups_next` (one save at a time, so the service keeps the newest).
    groups_saving: bool,
    groups_next: Option<FriendGroups>,
    /// While saves are on their way: the document the service last confirmed, shown again
    /// when one fails.
    groups_saved: Option<FriendGroups>,
    /// Where answered and closed invites outlive a restart (None: for this run only). Kept
    /// across sign-outs, like the configuration.
    closed_invites: Option<ClosedInvitesFile>,
    /// Chat may show (signed config `features.chat`). Survives sign-outs.
    chat_enabled: bool,
    /// Where chat histories live (one folder per account). Survives sign-outs.
    chat_root: Option<PathBuf>,
    /// The signed-in account's chat history (once its id is known).
    history: Option<History>,
    /// Where the presence task's next sync starts (kept from the history).
    chat_setup: ChatSetup,
    /// The chat state of this connection (None: no chat on it).
    chat_status: Option<ChatStatus>,
    /// This computer's clock minus the hub's (ms).
    chat_skew_ms: i64,
    /// The sync after the last connect is over: waiting messages may go.
    chat_synced: bool,
    /// Chat events that came before the history was open.
    chat_backlog: Vec<PresenceEvent>,
    /// Chat changes for the UI, sent with the next view.
    chat_outbox: Vec<ChatChangeView>,
    /// Waiting messages are being sent (one at a time).
    sending: bool,
    /// Messages that met "too fast" once (they go again by themselves once).
    too_fast: HashSet<String>,
    /// The account was suspended or banned in this run (shown on the sign-in screen).
    restriction: Option<RestrictionView>,
    /// The settings backup waits until then (Unix seconds; storage protection).
    backup_waits_until: Option<i64>,
    /// "Birlikte oynanan süre", kept on this computer (TEAM5). Survives sign-outs.
    together: TogetherLog,
}

impl SocialService {
    /// Starts with the feature off; [`SocialService::configure`] turns it on.
    pub fn new(
        http: reqwest::Client,
        config: ServiceConfig,
        tokens: Arc<dyn TokenStore>,
        runtime: Handle,
        sink: UpdateSink,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                http,
                config,
                tokens,
                sink,
                runtime,
                core: Mutex::new(Core::default()),
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Core> {
        self.inner
            .core
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn spawn(&self, future: impl Future<Output = ()> + Send + 'static) {
        self.inner.runtime.spawn(future);
    }

    /// The current view (for a UI that just started).
    pub fn view(&self) -> SocialView {
        let mut core = self.lock();
        build_view(&mut core)
    }

    fn emit_view(&self) {
        let (changes, view) = {
            let mut core = self.lock();
            core.seq += 1;
            let changes = std::mem::take(&mut core.chat_outbox);
            (changes, build_view(&mut core))
        };
        for change in changes {
            (self.inner.sink)(SocialUpdate::Chat(change));
        }
        (self.inner.sink)(SocialUpdate::View(Box::new(view)));
    }

    fn notify(&self, notice: SocialNotice) {
        (self.inner.sink)(SocialUpdate::Notice(notice));
    }

    /// Keeps answered and closed invites in this file, so the ones the service sends again after
    /// a restart (it lists the last hour's on every connection) stay closed.
    pub fn keep_closed_invites_in(&self, path: impl Into<PathBuf>) {
        self.lock().closed_invites = Some(ClosedInvitesFile::new(path));
    }

    /// Keeps "Birlikte oynanan süre" in this file (only on this computer; without it, for this
    /// run only).
    pub fn keep_together_in(&self, path: impl Into<PathBuf>) {
        self.lock().together = TogetherLog::new(Some(path.into()));
    }

    /// "Birlikte oynanan süre" on or off on this computer. Off deletes what was kept.
    pub fn set_together(&self, on: bool) {
        {
            let mut core = self.lock();
            if let Err(error) = core.together.set_enabled(on) {
                tracing::info!(%error, "the time played together switch holds for this run only");
            }
            observe_together(&mut core);
        }
        self.emit_view();
    }

    /// Deletes the time played together kept on this computer (it counts on from now).
    pub fn clear_together(&self) {
        {
            let mut core = self.lock();
            if let Err(error) = core.together.clear(unix_now()) {
                tracing::info!(%error, "could not delete the time played together");
            }
        }
        self.emit_view();
    }

    /// Teams on or off (signed config `features.teams`); the friends feature must be on too.
    pub fn set_teams_enabled(&self, on: bool) {
        {
            let mut core = self.lock();
            if core.teams == on {
                return;
            }
            core.teams = on;
        }
        self.emit_view();
    }

    /// Turns the feature on with this service, or off (None). A saved session is restored.
    /// Turning it off keeps the saved session for when it comes back.
    pub fn configure(&self, url: Option<Url>) {
        {
            let mut core = self.lock();
            if core.url == url {
                return;
            }
            reset(&mut core);
            core.client = url.as_ref().map(|url| {
                SocialClient::with_http_client(
                    self.inner.http.clone(),
                    SocialConfig::new(url.clone(), self.inner.config.user_agent.clone()),
                )
            });
            core.url = url;
            if let Some(client) = core.client.clone()
                && let Some(token) = self.inner.tokens.load()
            {
                let session = client.session(token);
                core.session = Some(session.clone());
                core.auth = AuthPhase::Restoring;
                let generation = core.generation;
                let service = self.clone();
                self.spawn(async move { service.restore(generation, session).await });
            }
        }
        self.emit_view();
    }

    async fn restore(&self, generation: u64, session: Session) {
        match session.me().await {
            Ok(me) => {
                {
                    let mut core = self.lock();
                    if core.generation != generation {
                        return;
                    }
                    core.settings_version = me.settings.as_ref().map(|s| s.version);
                    core.state.set_me(me);
                    remember_closed_invites(&mut core);
                    open_history(&mut core);
                    core.auth = AuthPhase::SignedIn;
                    self.start_presence(&mut core);
                }
                self.emit_view();
                self.fetch_friends(generation).await;
            }
            // Expired or revoked while the launcher was closed: back to the sign-in screen.
            Err(SocialError::Unauthorized) => self.end_session(generation, true),
            // Suspended or banned while the launcher was closed.
            Err(SocialError::Restricted { until }) => self.end_restricted(generation, until),
            Err(error) => {
                tracing::info!(%error, "friends service unavailable; retrying in the background");
                {
                    let mut core = self.lock();
                    if core.generation != generation {
                        return;
                    }
                    core.auth = AuthPhase::SignedIn;
                    // The presence task retries with backoff; once it connects, the profile
                    // and the friend list are fetched again.
                    self.start_presence(&mut core);
                    core.state.set_connection(Connection::Unavailable);
                }
                self.emit_view();
            }
        }
    }

    /// Moderators suspended or banned the account: every session ended. The token goes, and the
    /// sign-in screen says why (for this run).
    fn end_restricted(&self, generation: u64, until: Option<i64>) {
        {
            let mut core = self.lock();
            if core.generation != generation {
                return;
            }
            reset(&mut core);
            core.restriction = Some(RestrictionView { until });
        }
        self.inner.tokens.clear();
        self.emit_view();
        self.notify(SocialNotice::Restricted { until });
    }

    /// The session is no longer valid: forget it (and say so unless `quietly`).
    fn end_session(&self, generation: u64, quietly: bool) {
        {
            let mut core = self.lock();
            if core.generation != generation {
                return;
            }
            reset(&mut core);
        }
        self.inner.tokens.clear();
        self.emit_view();
        if !quietly {
            self.notify(SocialNotice::SignedOutRemotely);
        }
    }

    fn start_presence(&self, core: &mut Core) {
        let Some(session) = core.session.clone() else {
            return;
        };
        core.epoch += 1;
        let (generation, epoch) = (core.generation, core.epoch);
        let (control, mut events) = {
            // Callers may run outside the runtime (deep links arrive on the UI thread).
            let _runtime = self.inner.runtime.enter();
            if core.chat_enabled {
                session
                    .connect_presence_with_chat(self.inner.config.presence, core.chat_setup.clone())
                    .split()
            } else {
                session.connect_presence(self.inner.config.presence).split()
            }
        };
        control.set_status(core.activity.clone());
        // Replacing the control closes the previous connection cleanly.
        core.presence = Some(control);
        core.chat_status = None;
        core.chat_synced = false;
        core.state.set_connection(Connection::Connecting);
        let service = self.clone();
        self.spawn(async move {
            while let Some(event) = events.next_event().await {
                service.on_presence(generation, epoch, event);
            }
        });
    }

    fn on_presence(&self, generation: u64, epoch: u64, event: PresenceEvent) {
        let mut refetch = false;
        let mut ended = false;
        let mut quietly = false;
        let mut restricted = None;
        let deliver;
        let notices = {
            let mut core = self.lock();
            if core.generation != generation || core.epoch != epoch {
                return;
            }
            if matches!(event, PresenceEvent::GroupsChanged(_)) && core.groups_saving {
                // This device's own save is on its way and wins; its echo arrives after it.
                return;
            }
            let mut notices = core.state.apply(&event, unix_now());
            observe_together(&mut core);
            let teams = core.teams && core.state.teams_supported();
            let calls = teams && core.state.offers(TEAM_FEATURE_CALLS);
            let requests = teams && core.state.offers(TEAM_FEATURE_REQUESTS);
            let plans = teams && core.state.offers(TEAM_FEATURE_PLANS);
            notices.retain(|notice| match notice {
                SocialNotice::TeamInvite { .. }
                | SocialNotice::TeamGone { .. }
                | SocialNotice::CrewmateOnProjects { .. } => core.teams,
                SocialNotice::CrewCall { .. } | SocialNotice::CrewCallJoin { .. } => calls,
                SocialNotice::JoinRequest { .. } | SocialNotice::RequestAccepted { .. } => requests,
                SocialNotice::CrewPlan { .. } => plans,
                _ => true,
            });
            match &event {
                PresenceEvent::Connected { .. } => {
                    refetch = core.state.me().is_none()
                        || !core.state.loaded()
                        || core
                            .fetched_at
                            .is_none_or(|at| at.elapsed() >= REFETCH_AFTER);
                }
                PresenceEvent::Stopped(StopReason::SignedOut | StopReason::AccountDeleted) => {
                    ended = true;
                    quietly = core.deleting;
                }
                PresenceEvent::Stopped(StopReason::Restricted { until }) => {
                    restricted = Some(*until);
                }
                PresenceEvent::Stopped(_) => core.presence = None,
                _ => {}
            }
            let chat = apply_chat(&mut core, &event);
            deliver = chat.deliver;
            notices.extend(chat.notices);
            notices
        };
        if let Some(until) = restricted {
            self.end_restricted(generation, until);
            return;
        }
        if ended {
            self.end_session(generation, quietly);
            return;
        }
        self.emit_view();
        for notice in notices {
            self.notify(notice);
        }
        if deliver {
            self.kick_sender();
        }
        if refetch {
            let service = self.clone();
            self.spawn(async move { service.refetch(generation).await });
        }
    }

    fn current_session(&self, generation: u64) -> Option<Session> {
        let core = self.lock();
        if core.generation == generation {
            core.session.clone()
        } else {
            None
        }
    }

    async fn fetch_friends(&self, generation: u64) {
        let Some(session) = self.current_session(generation) else {
            return;
        };
        match session.friends().await {
            Ok(overview) => {
                {
                    let mut core = self.lock();
                    if core.generation != generation {
                        return;
                    }
                    core.state.set_overview(overview);
                    core.fetched_at = Some(Instant::now());
                    observe_together(&mut core);
                }
                self.emit_view();
            }
            Err(SocialError::Unauthorized) => self.end_session(generation, false),
            Err(SocialError::Restricted { until }) => self.end_restricted(generation, until),
            Err(error) => tracing::info!(%error, "could not load the friend list"),
        }
    }

    /// The profile and the friend list again (after a reconnect, or "Tekrar dene").
    async fn refetch(&self, generation: u64) {
        let session = {
            let mut core = self.lock();
            if core.generation != generation || core.fetching {
                return;
            }
            let Some(session) = core.session.clone() else {
                return;
            };
            core.fetching = true;
            session
        };
        let me = session.me().await;
        let overview = match &me {
            Ok(_) => Some(session.friends().await),
            Err(_) => None,
        };
        let mut ended = false;
        let mut restricted = None;
        {
            let mut core = self.lock();
            if core.generation != generation {
                return;
            }
            core.fetching = false;
            match me {
                Ok(me) => {
                    core.settings_version = me.settings.as_ref().map(|s| s.version);
                    core.state.set_me(me);
                    // The profile may come only now (the service was down at start).
                    remember_closed_invites(&mut core);
                    open_history(&mut core);
                }
                Err(SocialError::Unauthorized) => ended = true,
                Err(SocialError::Restricted { until }) => restricted = Some(until),
                Err(error) => {
                    tracing::info!(%error, "friends service unavailable");
                    if core.state.connection() != Connection::Online {
                        core.state.set_connection(Connection::Unavailable);
                    }
                }
            }
            match overview {
                Some(Ok(overview)) => {
                    core.state.set_overview(overview);
                    core.fetched_at = Some(Instant::now());
                    observe_together(&mut core);
                }
                Some(Err(SocialError::Unauthorized)) => ended = true,
                Some(Err(SocialError::Restricted { until })) => restricted = Some(until),
                Some(Err(error)) => tracing::info!(%error, "could not load the friend list"),
                None => {}
            }
        }
        if let Some(until) = restricted {
            self.end_restricted(generation, until);
        } else if ended {
            self.end_session(generation, false);
        } else {
            self.emit_view();
        }
    }

    /// "Tekrar dene": reconnects a stopped, replaced or failing connection and loads the profile
    /// and the friend list again. Throttled.
    pub async fn refresh(&self) {
        let generation = {
            let mut core = self.lock();
            if core.auth != AuthPhase::SignedIn
                || core
                    .last_manual
                    .is_some_and(|at| at.elapsed() < MANUAL_REFRESH_GAP)
            {
                return;
            }
            core.last_manual = Some(Instant::now());
            if core.presence.is_none()
                || matches!(
                    core.state.connection(),
                    Connection::Replaced | Connection::Unavailable | Connection::Idle
                )
            {
                self.start_presence(&mut core);
            }
            core.generation
        };
        self.emit_view();
        self.refetch(generation).await;
    }

    // --- sign-in --------------------------------------------------------------------------

    /// Starts a Discord sign-in and returns the page to open in the browser. Fails while the
    /// service does not answer, so nobody lands on a broken page.
    pub async fn begin_sign_in(&self) -> Result<Url> {
        let client = {
            let core = self.lock();
            if matches!(core.auth, AuthPhase::SignedIn | AuthPhase::Restoring) {
                return Err(SocialError::InvalidInput("already signed in"));
            }
            core.client
                .clone()
                .ok_or(SocialError::Unavailable { retry_after: None })?
        };
        client.health().await?;
        let url = {
            let mut core = self.lock();
            if core.client.as_ref().map(SocialClient::base_url) != Some(client.base_url()) {
                return Err(SocialError::Unavailable { retry_after: None });
            }
            let attempt = client.begin_login();
            let url = attempt.browser_url().clone();
            core.login = Some(attempt);
            core.auth = AuthPhase::SigningIn;
            url
        };
        self.emit_view();
        Ok(url)
    }

    pub fn cancel_sign_in(&self) {
        {
            let mut core = self.lock();
            core.login = None;
            if core.auth == AuthPhase::SigningIn {
                core.auth = AuthPhase::SignedOut;
            }
        }
        self.emit_view();
    }

    /// Handles `projects://auth/done?c=…`; false for any other link. The link is used only
    /// while this launcher waits for a sign-in it started itself.
    pub fn complete_sign_in(&self, link: &str) -> bool {
        let Some(code) = auth_code_from_deep_link(link) else {
            return false;
        };
        let pending = {
            let mut core = self.lock();
            match (core.login.take(), core.client.clone()) {
                (Some(attempt), Some(client)) => Some((attempt, client, core.generation)),
                _ => None,
            }
        };
        match pending {
            Some((attempt, client, generation)) => {
                let service = self.clone();
                self.spawn(async move { service.claim(generation, client, attempt, code).await });
            }
            None => tracing::debug!("sign-in link while no sign-in is waiting; ignored"),
        }
        true
    }

    async fn claim(
        &self,
        generation: u64,
        client: SocialClient,
        attempt: LoginAttempt,
        code: String,
    ) {
        match client
            .complete_login(&attempt, &code, &self.inner.config.device)
            .await
        {
            Ok(SignedIn { token, me }) => {
                let wanted = {
                    let core = self.lock();
                    core.generation == generation && core.auth == AuthPhase::SigningIn
                };
                if !wanted {
                    // Cancelled or reconfigured meanwhile: keep no session nobody asked for.
                    let _ =
                        tokio::time::timeout(LOGOUT_TIMEOUT, client.session(token).logout()).await;
                    return;
                }
                if let Err(error) = self.inner.tokens.save(&token) {
                    tracing::warn!(%error, "the friends session lasts for this run only");
                }
                let restore = me
                    .settings
                    .clone()
                    .filter(|backup| backup.device != self.inner.config.device);
                {
                    let mut core = self.lock();
                    if core.generation != generation {
                        return;
                    }
                    core.session = Some(client.session(token));
                    core.settings_version = me.settings.as_ref().map(|s| s.version);
                    core.state = SocialState::default();
                    core.state.set_me(me);
                    remember_closed_invites(&mut core);
                    open_history(&mut core);
                    core.restriction = None;
                    core.auth = AuthPhase::SignedIn;
                    self.start_presence(&mut core);
                }
                self.emit_view();
                self.notify(SocialNotice::SignedIn { restore });
                self.fetch_friends(generation).await;
            }
            Err(error) => {
                tracing::info!(%error, "sign-in did not finish");
                let reason = match &error {
                    SocialError::Refused {
                        code: ErrorCode::AuthPendingNotFound,
                        ..
                    } => SignInFailure::Expired,
                    SocialError::Restricted { .. } => SignInFailure::Restricted,
                    SocialError::SignupsClosed { .. } => SignInFailure::SignupsClosed,
                    error if error.is_transient() => SignInFailure::Unavailable,
                    _ => SignInFailure::Failed,
                };
                {
                    let mut core = self.lock();
                    if core.generation == generation && core.auth == AuthPhase::SigningIn {
                        core.auth = AuthPhase::SignedOut;
                    }
                    if let SocialError::Restricted { until } = error {
                        core.restriction = Some(RestrictionView { until });
                    }
                }
                self.emit_view();
                self.notify(SocialNotice::SignInFailed { reason });
            }
        }
    }

    /// Signs out here: the token leaves the keychain at once, the service is told when it
    /// answers (the session expires by itself otherwise).
    pub async fn sign_out(&self) {
        let session = {
            let mut core = self.lock();
            let session = core.session.take();
            reset(&mut core);
            session
        };
        self.inner.tokens.clear();
        self.emit_view();
        if let Some(session) = session {
            let _ = tokio::time::timeout(LOGOUT_TIMEOUT, session.logout()).await;
        }
    }

    // --- signed-in calls ------------------------------------------------------------------

    fn signed_in(&self) -> Result<(Session, u64)> {
        let core = self.lock();
        match (&core.session, core.auth) {
            (Some(session), AuthPhase::SignedIn) => Ok((session.clone(), core.generation)),
            _ => Err(SocialError::Unauthorized),
        }
    }

    async fn call<T, F, Fut>(&self, request: F) -> Result<(T, u64)>
    where
        F: FnOnce(Session) -> Fut,
        Fut: Future<Output = Result<T>>,
    {
        let (session, generation) = self.signed_in()?;
        match request(session).await {
            Ok(value) => Ok((value, generation)),
            Err(SocialError::Unauthorized) => {
                self.end_session(generation, false);
                Err(SocialError::Unauthorized)
            }
            Err(SocialError::Restricted { until }) => {
                self.end_restricted(generation, until);
                Err(SocialError::Restricted { until })
            }
            Err(error) => Err(error),
        }
    }

    fn update(&self, generation: u64, change: impl FnOnce(&mut Core)) {
        {
            let mut core = self.lock();
            if core.generation != generation {
                return;
            }
            change(&mut core);
        }
        self.emit_view();
    }

    /// Who owns a friend code (counts against a daily quota).
    pub async fn lookup(&self, code: &str) -> Result<LookupView> {
        let normalized =
            normalize_friend_code(code).ok_or(SocialError::InvalidInput("not a friend code"))?;
        let request = normalized.clone();
        let (lookup, _) = self
            .call(|session| async move { session.lookup_code(&request).await })
            .await?;
        Ok(LookupView {
            profile: ProfileView::from(&lookup.user),
            relation: lookup.relation,
            code: normalized,
        })
    }

    pub async fn send_request(&self, code: &str) -> Result<SendOutcome> {
        let code = code.to_owned();
        let (outcome, generation) = self
            .call(|session| async move { session.send_request(&code).await })
            .await?;
        let result = match &outcome {
            RequestOutcome::Requested { .. } => SendOutcome::Requested,
            RequestOutcome::Friends { .. } => SendOutcome::Friends,
        };
        self.update(generation, |core| core.state.request_sent(outcome));
        Ok(result)
    }

    pub async fn accept(&self, id: &str) -> Result<()> {
        let from = id.to_owned();
        let (friend, generation) = self
            .call(|session| async move { session.accept_request(&from).await })
            .await?;
        self.update(generation, |core| core.state.request_accepted(friend));
        Ok(())
    }

    pub async fn decline(&self, id: &str) -> Result<()> {
        let from = id.to_owned();
        let (_, generation) = self
            .call(|session| async move { session.decline_request(&from).await })
            .await?;
        self.update(generation, |core| core.state.request_declined(id));
        Ok(())
    }

    pub async fn cancel_request(&self, id: &str) -> Result<()> {
        let to = id.to_owned();
        let (_, generation) = self
            .call(|session| async move { session.cancel_request(&to).await })
            .await?;
        self.update(generation, |core| core.state.request_cancelled(id));
        Ok(())
    }

    pub async fn remove_friend(&self, id: &str) -> Result<()> {
        let friend = id.to_owned();
        let (_, generation) = self
            .call(|session| async move { session.remove_friend(&friend).await })
            .await?;
        self.update(generation, |core| core.state.friend_removed(id));
        Ok(())
    }

    pub async fn block(&self, id: &str) -> Result<()> {
        // Taken before the call: the service also tells the presence socket, and that event
        // may remove them from the lists before the answer arrives.
        let profile = self.lock().state.profile(id);
        let user = id.to_owned();
        let (_, generation) = self
            .call(|session| async move { session.block(&user).await })
            .await?;
        self.update(generation, |core| match profile {
            Some(profile) => core.state.blocked(profile),
            None => core.state.friend_removed(id),
        });
        Ok(())
    }

    pub async fn unblock(&self, id: &str) -> Result<()> {
        let user = id.to_owned();
        let (_, generation) = self
            .call(|session| async move { session.unblock(&user).await })
            .await?;
        self.update(generation, |core| core.state.unblocked(id));
        Ok(())
    }

    /// A new friend code; the old one stops working.
    pub async fn rotate_code(&self) -> Result<String> {
        let (code, generation) = self
            .call(|session| async move { session.rotate_friend_code().await })
            .await?;
        let shown = code.clone();
        self.update(generation, |core| core.state.set_friend_code(shown));
        Ok(code)
    }

    /// Picture, status mode or privacy switches.
    pub async fn update_profile(&self, update: ProfileUpdate) -> Result<()> {
        let (me, generation) = self
            .call(|session| async move { session.update_profile(&update).await })
            .await?;
        self.update(generation, |core| core.state.set_me(me));
        Ok(())
    }

    // --- friend categories ----------------------------------------------------------------

    /// Saves the player's friend categories (the whole document: their order, names and who is
    /// in which). The UI shows the change at once; saves go out one at a time and only the
    /// newest waiting one is sent, so the service always ends with the latest. On failure the
    /// last saved document comes back and the error is returned.
    pub async fn save_groups(
        &self,
        groups: Vec<FriendGroup>,
        members: Vec<GroupMember>,
    ) -> Result<()> {
        let wanted = groups_document(groups, members)?;
        let generation = {
            let mut core = self.lock();
            if core.auth != AuthPhase::SignedIn || core.state.groups().is_none() {
                return Err(SocialError::Unavailable { retry_after: None });
            }
            let version = core.state.groups().map_or(0, |g| g.version);
            if core.groups_saved.is_none() {
                core.groups_saved = core.state.groups().cloned();
            }
            core.state.set_groups(FriendGroups {
                version,
                ..wanted.clone()
            });
            if core.groups_saving {
                core.groups_next = Some(wanted);
                drop(core);
                self.emit_view();
                return Ok(());
            }
            core.groups_saving = true;
            core.generation
        };
        self.emit_view();
        let mut next = Some(wanted);
        let mut outcome = Ok(());
        while let Some(document) = next.take() {
            let session = match self.signed_in() {
                Ok((session, current)) if current == generation => session,
                _ => return Ok(()),
            };
            match session.put_friend_groups(&document).await {
                Ok(version) => {
                    let mut core = self.lock();
                    if core.generation != generation {
                        return Ok(());
                    }
                    let saved = FriendGroups {
                        version,
                        ..document
                    };
                    next = core.groups_next.take();
                    if next.is_none() {
                        // What is shown is what the service has. The next save goes back to
                        // what is shown then on failure (other devices' changes included).
                        core.state.set_groups(saved);
                        core.groups_saving = false;
                        core.groups_saved = None;
                    } else {
                        core.groups_saved = Some(saved);
                    }
                }
                Err(error) => {
                    {
                        let mut core = self.lock();
                        if core.generation == generation {
                            core.groups_saving = false;
                            core.groups_next = None;
                            if let Some(saved) = core.groups_saved.take() {
                                core.state.set_groups(saved);
                            }
                        }
                    }
                    if matches!(error, SocialError::Unauthorized) {
                        self.end_session(generation, false);
                        return Err(error);
                    }
                    outcome = Err(error);
                }
            }
        }
        self.emit_view();
        outcome
    }

    // --- teams ----------------------------------------------------------------------------

    /// One of the player's crews, by its id (else "not in a team": it went away meanwhile).
    fn crew_id(&self, team: &str) -> Result<String> {
        self.lock()
            .state
            .crew(team)
            .map(|entry| entry.team.id.clone())
            .ok_or(SocialError::InvalidInput("not in a team"))
    }

    /// The service offers this part of crews (`teamFeatures`).
    fn offers(&self, feature: &str) -> bool {
        self.lock().state.offers(feature)
    }

    /// Founds a crew: a name, a tag (2-4 letters or digits), an emblem and, where the service
    /// offers looks, a colour, a frame and a motto. Without looks the look stays home and only
    /// the first 20 emblems may be sent (an older service refuses the rest). Where the service
    /// has several crews, one more below the crew limit (`limit_crews` at it).
    pub async fn create_team(
        &self,
        name: &str,
        tag: &str,
        emblem: &str,
        look: Option<TeamLook>,
    ) -> Result<()> {
        let look = look.filter(|_| self.offers(TEAM_FEATURE_LOOK));
        if look.is_none() && !FIRST_EMBLEMS.contains(&emblem) {
            return Err(SocialError::InvalidInput("emblem"));
        }
        let multi = self.offers(TEAM_FEATURE_MULTI);
        let (name, tag, emblem) = (name.to_owned(), tag.to_owned(), emblem.to_owned());
        let (answer, generation) = self
            .call(|session| async move {
                session
                    .create_team(&name, &tag, &emblem, look.as_ref(), multi)
                    .await
            })
            .await?;
        self.update(generation, |core| core.state.team_answer(answer));
        Ok(())
    }

    /// Renames one of the player's crews, changes its tag, emblem, look or permissions (the
    /// leader; a moderator the look where the crew lets them). What the service cannot take (a
    /// look without looks, permissions without roles) is left out; nothing left, nothing is sent.
    pub async fn edit_team(&self, team: &str, edit: TeamEdit) -> Result<()> {
        let team = self.crew_id(team)?;
        let look = self.offers(TEAM_FEATURE_LOOK);
        if !look
            && edit
                .emblem
                .as_deref()
                .is_some_and(|emblem| !FIRST_EMBLEMS.contains(&emblem))
        {
            return Err(SocialError::InvalidInput("emblem"));
        }
        let edit = edit.limited_to(EditFeatures {
            look,
            roles: self.offers(TEAM_FEATURE_ROLES),
            requests: self.offers(TEAM_FEATURE_REQUESTS),
        });
        if edit.is_empty() {
            return Ok(());
        }
        let (answer, generation) = self
            .call(|session| async move { session.update_team(&team, &edit).await })
            .await?;
        self.update(generation, |core| core.state.team_answer(answer));
        Ok(())
    }

    /// Ends one of the player's crews for everyone (founder).
    pub async fn disband_team(&self, team: &str) -> Result<()> {
        let team = self.crew_id(team)?;
        let id = team.clone();
        let (_, generation) = self
            .call(|session| async move { session.disband_team(&team).await })
            .await?;
        self.update(generation, |core| core.state.team_left(&id));
        Ok(())
    }

    /// Leaves one of the player's crews (a founder with members hands the role on first).
    pub async fn leave_team(&self, team: &str) -> Result<()> {
        let team = self.crew_id(team)?;
        let id = team.clone();
        let (_, generation) = self
            .call(|session| async move { session.leave_team(&team).await })
            .await?;
        self.update(generation, |core| core.state.team_left(&id));
        Ok(())
    }

    /// A new code for one of the player's crews (founder); the old one stops working.
    pub async fn rotate_team_code(&self, team: &str) -> Result<String> {
        let team = self.crew_id(team)?;
        let id = team.clone();
        let (code, generation) = self
            .call(|session| async move { session.rotate_team_code(&team).await })
            .await?;
        let shown = code.clone();
        self.update(generation, |core| core.state.team_code(&id, shown));
        Ok(code)
    }

    /// Invites a friend to one of the player's crews.
    pub async fn invite_to_team(&self, team: &str, friend: &str) -> Result<()> {
        let team = self.crew_id(team)?;
        let id = team.clone();
        let to = friend.to_owned();
        let (_, generation) = self
            .call(|session| async move { session.invite_to_team(&team, &to).await })
            .await?;
        self.update(generation, |core| core.state.team_invite_sent(&id, friend));
        Ok(())
    }

    /// Makes a member a moderator, or a moderator a member again (leader; a service with roles).
    pub async fn set_team_moderator(
        &self,
        team: &str,
        member: &str,
        moderator: bool,
    ) -> Result<()> {
        if !self.offers(TEAM_FEATURE_ROLES) {
            return Err(SocialError::InvalidInput("crew roles"));
        }
        let team = self.crew_id(team)?;
        let member = member.to_owned();
        let (answer, generation) =
            self.call(|session| async move {
                session.set_team_moderator(&team, &member, moderator).await
            })
            .await?;
        self.update(generation, |core| core.state.team_answer(answer));
        Ok(())
    }

    /// Removes a member (the leader anyone; a moderator plain members, where the crew lets
    /// them). The code is renewed with it, so the crews are read again.
    pub async fn remove_team_member(&self, team: &str, member: &str) -> Result<()> {
        let team = self.crew_id(team)?;
        let member = member.to_owned();
        let (overview, generation) = self
            .call(|session| async move {
                session.remove_team_member(&team, &member).await?;
                session.teams().await
            })
            .await?;
        self.update(generation, |core| core.state.set_teams(overview));
        Ok(())
    }

    /// Hands the founder role of one of the player's crews to another member.
    pub async fn transfer_team_founder(&self, team: &str, member: &str) -> Result<()> {
        let team = self.crew_id(team)?;
        let member = member.to_owned();
        let (answer, generation) = self
            .call(|session| async move { session.transfer_team_founder(&team, &member).await })
            .await?;
        self.update(generation, |core| core.state.team_answer(answer));
        Ok(())
    }

    /// The player's crews in a new order (every crew once; the first becomes the primary crew),
    /// on a service with several crews. Shown at once; when the service answers that the crews
    /// changed meanwhile, the list is read again; any other failure puts the old order back.
    pub async fn order_crews(&self, order: Vec<String>) -> Result<()> {
        let (before, generation) = {
            let mut core = self.lock();
            if !core.state.offers(TEAM_FEATURE_MULTI) {
                return Err(SocialError::InvalidInput("crew order"));
            }
            let before = core.state.crew_order();
            let mut wanted = order.clone();
            wanted.sort();
            let mut known = before.clone();
            known.sort();
            if wanted != known {
                return Err(SocialError::InvalidInput("crew order"));
            }
            if before == order {
                return Ok(());
            }
            core.state.set_crew_order(&order);
            (before, core.generation)
        };
        self.emit_view();
        let sent = order.clone();
        match self
            .call(|session| async move { session.order_crews(&sent).await })
            .await
        {
            Ok((saved, generation)) => {
                self.update(generation, |core| core.state.set_crew_order(&saved));
                Ok(())
            }
            Err(error) => {
                self.update(generation, |core| core.state.set_crew_order(&before));
                if error.code() == Some(&ErrorCode::CrewsChanged) {
                    let service = self.clone();
                    self.spawn(async move { service.refetch(generation).await });
                }
                Err(error)
            }
        }
    }

    /// What a crew code leads to (counts against the daily code lookup quota). Takes `EKP-…`,
    /// `TKM-…` or the symbols alone. Where the service has several crews, a player at the crew
    /// limit reads `limit`.
    pub async fn lookup_team(&self, code: &str) -> Result<TeamLookupView> {
        let normalized =
            normalize_team_code(code).ok_or(SocialError::InvalidInput("not a team code"))?;
        let lookup = self.lookup_crew(CrewKey::Code(normalized.clone())).await?;
        Ok(lookup_view(lookup, crew_code(&normalized), None))
    }

    /// What a crew's invite link leads to (a service with `link`; the code lookup quota): the
    /// crew, whether it approves newcomers and the player's request to it while that waits.
    pub async fn lookup_crew_link(&self, link: &str) -> Result<TeamLookupView> {
        if !self.offers(TEAM_FEATURE_LINK) {
            return Err(SocialError::InvalidInput("crew links"));
        }
        let normalized =
            normalize_crew_link(link).ok_or(SocialError::InvalidInput("not a crew link"))?;
        let lookup = self.lookup_crew(CrewKey::Link(normalized.clone())).await?;
        Ok(lookup_view(lookup, String::new(), Some(normalized)))
    }

    async fn lookup_crew(&self, key: CrewKey) -> Result<TeamLookup> {
        let multi = self.offers(TEAM_FEATURE_MULTI);
        let (lookup, _) = self
            .call(|session| async move { session.lookup_crew(&key, multi).await })
            .await?;
        Ok(lookup)
    }

    /// Joins the crew of a code (one more, where the service has several crews). A crew that
    /// approves newcomers takes a join request instead (a service with `requests`).
    pub async fn join_team(&self, code: &str) -> Result<JoinResult> {
        self.join_crew(CrewKey::Code(code.to_owned())).await
    }

    /// Joins the crew of an invite link (a service with `link`), or asks to.
    pub async fn join_crew_link(&self, link: &str) -> Result<JoinResult> {
        if !self.offers(TEAM_FEATURE_LINK) {
            return Err(SocialError::InvalidInput("crew links"));
        }
        self.join_crew(CrewKey::Link(link.to_owned())).await
    }

    async fn join_crew(&self, key: CrewKey) -> Result<JoinResult> {
        let multi = self.offers(TEAM_FEATURE_MULTI);
        let request = self.offers(TEAM_FEATURE_REQUESTS);
        let (outcome, generation) = self
            .call(|session| async move { session.join_crew(&key, multi, request).await })
            .await?;
        match outcome {
            JoinOutcome::Joined(answer) => {
                self.update(generation, |core| core.state.team_answer(answer));
                Ok(JoinResult::Joined)
            }
            JoinOutcome::Requested(request) => {
                self.update(generation, |core| core.state.join_requested(request));
                Ok(JoinResult::Requested)
            }
        }
    }

    /// Withdraws the player's waiting join request to a crew; one that is gone already leaves the
    /// list either way.
    pub async fn withdraw_team_request(&self, team: &str) -> Result<()> {
        let id = team.to_owned();
        match self
            .call(|session| async move { session.withdraw_team_request(&id).await })
            .await
        {
            Ok((_, generation)) => {
                self.update(generation, |core| core.state.join_request_withdrawn(team));
                Ok(())
            }
            Err(error) if error.code() == Some(&ErrorCode::TeamRequestNotFound) => {
                let generation = self.lock().generation;
                self.update(generation, |core| core.state.join_request_withdrawn(team));
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    /// Takes a player who asked to join one of the player's crews (`accept`), or declines the
    /// request (silent for them). A request that is gone leaves the list (taking it says so); a
    /// full crew, or a player in as many crews as they may be, keeps it waiting.
    pub async fn answer_team_request(&self, team: &str, user: &str, accept: bool) -> Result<()> {
        let team = self.crew_id(team)?;
        let id = team.clone();
        let who = user.to_owned();
        let result = if accept {
            self.call(
                |session| async move { session.accept_team_request(&team, &who).await.map(Some) },
            )
            .await
        } else {
            self.call(|session| async move {
                session
                    .decline_team_request(&team, &who)
                    .await
                    .map(|()| None)
            })
            .await
        };
        match result {
            Ok((answer, generation)) => {
                self.update(generation, |core| {
                    core.state.join_request_answered(&id, user);
                    if let Some(answer) = answer {
                        core.state.team_answer(answer);
                    }
                });
                Ok(())
            }
            Err(error) if error.code() == Some(&ErrorCode::TeamRequestNotFound) => {
                let generation = self.lock().generation;
                self.update(generation, |core| {
                    core.state.join_request_answered(&id, user)
                });
                if accept { Err(error) } else { Ok(()) }
            }
            Err(error) => Err(error),
        }
    }

    /// A new invite link for one of the player's crews, in place of the old one (`on`), or none
    /// (a service with `link`; `permissions.admit`). Answers the link it has now.
    pub async fn set_team_link(&self, team: &str, on: bool) -> Result<Option<String>> {
        if !self.offers(TEAM_FEATURE_LINK) {
            return Err(SocialError::InvalidInput("crew links"));
        }
        let team = self.crew_id(team)?;
        let id = team.clone();
        if on {
            let (made, generation) = self
                .call(|session| async move { session.create_team_link(&team).await })
                .await?;
            let link = made.link.clone();
            self.update(generation, |core| {
                core.state.team_link(&id, Some(made.link))
            });
            Ok(Some(link))
        } else {
            let (_, generation) = self
                .call(|session| async move { session.delete_team_link(&team).await })
                .await?;
            self.update(generation, |core| core.state.team_link(&id, None));
            Ok(None)
        }
    }

    /// How newcomers with the code or the invite link get in: at once or once approved (the
    /// leader; a service with `requests`).
    pub async fn set_join_mode(&self, team: &str, join: JoinMode) -> Result<()> {
        if !self.offers(TEAM_FEATURE_REQUESTS) {
            return Err(SocialError::InvalidInput("join requests"));
        }
        self.edit_team(
            team,
            TeamEdit {
                join: Some(join),
                ..TeamEdit::default()
            },
        )
        .await
    }

    // --- crew plans ("Planlar", TEAM5) -----------------------------------------------------------

    /// The presence control, while signed in to a crew of a service that offers plans.
    fn plan_presence(&self) -> Result<PresenceControl> {
        let core = self.lock();
        if core.auth != AuthPhase::SignedIn {
            return Err(SocialError::Unauthorized);
        }
        if !(core.teams && core.state.teams_supported() && core.state.offers(TEAM_FEATURE_PLANS)) {
            return Err(SocialError::InvalidInput("crew plans"));
        }
        if core.state.crews().is_empty() {
            return Err(SocialError::InvalidInput("not in a team"));
        }
        core.presence
            .clone()
            .ok_or(SocialError::Unavailable { retry_after: None })
    }

    /// A new plan in one of the player's crews at `at` (Unix seconds, a whole quarter of an
    /// hour; the service checks it is later than its clock and at most 7 days ahead). Answers
    /// the plan's id; the plan is in the view.
    pub async fn make_plan(&self, crew: &str, at: i64) -> Result<String> {
        let control = self.plan_presence()?;
        let crew = self.crew_id(crew)?;
        if at % 900 != 0 {
            return Err(SocialError::InvalidInput("plan time"));
        }
        let generation = self.lock().generation;
        match control.make_plan(&crew, at).await {
            Ok(plan) => {
                let id = plan.id.clone();
                self.update(generation, |core| core.state.upsert_plan(plan));
                Ok(id)
            }
            Err(error) if error.code() == Some(&ErrorCode::TeamNotFound) => {
                // The crew went away in the meantime: the list is read again.
                let service = self.clone();
                self.spawn(async move { service.refetch(generation).await });
                Err(SocialError::InvalidInput("not in a team"))
            }
            Err(error) => Err(error),
        }
    }

    /// "Geliyorum", "Belki" or "Gelemem" to a plan. False when the plan is gone (it leaves the
    /// view); a changed answer the service turns down (20 a day) keeps the old one.
    pub async fn answer_plan(&self, plan: &str, answer: PlanAnswer) -> Result<bool> {
        let control = self.plan_presence()?;
        let generation = self.lock().generation;
        match control.answer_plan(plan, answer).await {
            Ok(()) => {
                self.update(generation, |core| core.state.plan_answered(plan, answer));
                Ok(true)
            }
            Err(error) if error.code() == Some(&ErrorCode::PlanNotFound) => {
                self.update(generation, |core| core.state.plan_gone(plan));
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }

    /// "Planı sil" (its maker, the crew's leader or a moderator); one that is gone already is
    /// simply gone.
    pub async fn cancel_plan(&self, plan: &str) -> Result<()> {
        let control = self.plan_presence()?;
        let generation = self.lock().generation;
        match control.cancel_plan(plan).await {
            Ok(()) => {}
            Err(error) if error.code() == Some(&ErrorCode::PlanNotFound) => {}
            Err(error) => return Err(error),
        }
        self.update(generation, |core| core.state.plan_gone(plan));
        Ok(())
    }

    /// Accepts an invite to a crew (one more, where the service has several crews). An invite
    /// that is gone leaves the list either way.
    pub async fn accept_team_invite(&self, team: &str) -> Result<()> {
        let id = team.to_owned();
        let multi = self.offers(TEAM_FEATURE_MULTI);
        match self
            .call(|session| async move { session.accept_team_invite(&id, multi).await })
            .await
        {
            Ok((answer, generation)) => {
                self.update(generation, |core| core.state.team_answer(answer));
                Ok(())
            }
            Err(error) => {
                if error.code() == Some(&ErrorCode::TeamInviteNotFound) {
                    let generation = self.lock().generation;
                    self.update(generation, |core| core.state.team_invite_answered(team));
                }
                Err(error)
            }
        }
    }

    pub async fn decline_team_invite(&self, team: &str) -> Result<()> {
        let id = team.to_owned();
        let result = self
            .call(|session| async move { session.decline_team_invite(&id).await })
            .await;
        match result {
            Ok((_, generation)) => {
                self.update(generation, |core| core.state.team_invite_answered(team));
                Ok(())
            }
            Err(error) if error.code() == Some(&ErrorCode::TeamInviteNotFound) => {
                let generation = self.lock().generation;
                self.update(generation, |core| core.state.team_invite_answered(team));
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    // --- crew notifications ("Sessize al") -----------------------------------------------------

    /// Changes the player's crew notification settings on a service that keeps them (`mute`):
    /// "Sessize al" until a time or until unmuted, "Sesi aç", crew calls on or off. The answer
    /// reaches the view; the player's other devices hear it through the socket.
    pub async fn set_crew_notifications(&self, edit: TeamNotificationsEdit) -> Result<()> {
        {
            let core = self.lock();
            if !(core.teams && core.state.teams_supported() && core.state.offers(TEAM_FEATURE_MUTE))
            {
                return Err(SocialError::InvalidInput("crew mute"));
            }
        }
        if !edit.valid_at(unix_now()) {
            return Err(SocialError::InvalidInput("mute until"));
        }
        if edit.is_empty() {
            return Ok(());
        }
        let (settings, generation) = self
            .call(|session| async move { session.set_team_notifications(&edit).await })
            .await?;
        self.update(generation, |core| {
            core.state.set_team_notifications(settings)
        });
        Ok(())
    }

    /// "Sessize al" for one of the player's crews (until a time, or until unmuted) or "Sesi aç",
    /// on a service with several crews. The answer reaches the view; the player's other devices
    /// hear it through the socket.
    pub async fn set_crew_mute(&self, team: &str, edit: CrewMuteEdit) -> Result<()> {
        {
            let core = self.lock();
            if !(core.teams
                && core.state.teams_supported()
                && core.state.offers(TEAM_FEATURE_MULTI))
            {
                return Err(SocialError::InvalidInput("crew mute"));
            }
        }
        if !edit.valid_at(unix_now()) {
            return Err(SocialError::InvalidInput("mute until"));
        }
        let team = self.crew_id(team)?;
        let id = team.clone();
        let (mute, generation) = self
            .call(|session| async move { session.set_crew_mute(&team, &edit).await })
            .await?;
        self.update(generation, |core| core.state.set_crew_mute(&id, mute));
        Ok(())
    }

    // --- crew calls ("Ekibi çağır") ---------------------------------------------------------

    /// The presence control, while signed in to a crew of a service that offers calls.
    fn crew_presence(&self) -> Result<PresenceControl> {
        let core = self.lock();
        if core.auth != AuthPhase::SignedIn {
            return Err(SocialError::Unauthorized);
        }
        if !(core.teams && core.state.teams_supported() && core.state.offers(TEAM_FEATURE_CALLS)) {
            return Err(SocialError::InvalidInput("crew calls"));
        }
        if core.state.crews().is_empty() {
            return Err(SocialError::InvalidInput("not in a team"));
        }
        core.presence
            .clone()
            .ok_or(SocialError::Unavailable { retry_after: None })
    }

    /// "Ekibi çağır" for `crews` (a service with several crews; one call however many it names)
    /// or the primary crew (empty, or a service with one crew per player). Only from a multiplayer
    /// server (CALL1): the launcher says why without asking the service when the player is not on
    /// one, and the service checks again. One open call among teammates who see each other, one
    /// every 5 minutes, 10 a day. The call reaches the view through the socket before this
    /// returns.
    pub async fn call_crew(&self, crews: Vec<String>) -> Result<CallOutcome> {
        let control = self.crew_presence()?;
        let (place, named) = {
            let core = self.lock();
            let share = core.state.me().is_some_and(|me| me.privacy.share_server);
            let place = CallPlace::of(&core.activity, share);
            // Only crews of the player's, each once; none named calls the primary one. Crews
            // that all went away meanwhile call nobody else instead.
            let asked = !crews.is_empty();
            let mut named: Vec<String> = Vec::new();
            for id in crews {
                if core.state.crew(&id).is_some() && !named.contains(&id) {
                    named.push(id);
                }
            }
            if asked && named.is_empty() {
                return Err(SocialError::InvalidInput("not in a team"));
            }
            let multi = core.state.offers(TEAM_FEATURE_MULTI);
            (place, (multi && asked).then_some(named))
        };
        match place {
            CallPlace::Ok => {}
            CallPlace::NeedsServer => return Ok(CallOutcome::NeedsServer),
            CallPlace::ServerHidden => return Ok(CallOutcome::ServerHidden),
        }
        match control.call_crew(named).await {
            Ok(()) => Ok(CallOutcome::Called),
            Err(SocialError::RateLimited { retry_after }) => Ok(CallOutcome::Wait {
                seconds: retry_after
                    .map_or(300, |wait| {
                        u32::try_from(wait.as_secs()).unwrap_or(u32::MAX)
                    })
                    .max(1),
            }),
            Err(error) if error.code() == Some(&ErrorCode::CallActive) => Ok(CallOutcome::Active),
            Err(error) if error.code() == Some(&ErrorCode::CallNeedsServer) => {
                Ok(CallOutcome::NeedsServer)
            }
            Err(error) if error.code() == Some(&ErrorCode::CallServerHidden) => {
                Ok(CallOutcome::ServerHidden)
            }
            Err(error) if error.code() == Some(&ErrorCode::TeamNotFound) => {
                // The crew went away in the meantime: the list is read again.
                let generation = self.lock().generation;
                let service = self.clone();
                self.spawn(async move { service.refetch(generation).await });
                Err(SocialError::InvalidInput("not in a team"))
            }
            Err(error) => Err(error),
        }
    }

    /// Answers (or changes the answer to) a teammate's call: "Geliyorum", "Birazdan",
    /// "Gelemem". False when the call is gone; it leaves the view.
    pub async fn answer_call(&self, call: &str, answer: CallAnswer) -> Result<bool> {
        let control = self.crew_presence()?;
        let generation = self.lock().generation;
        match control.answer_call(call, answer).await {
            Ok(()) => {
                self.update(generation, |core| core.state.call_answered(call, answer));
                Ok(true)
            }
            Err(error) if error.code() == Some(&ErrorCode::CallNotFound) => {
                self.update(generation, |core| core.state.call_gone(call));
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }

    /// "Bitir": ends the player's own call (one that is gone already is simply gone).
    pub async fn end_call(&self, call: &str) -> Result<()> {
        let control = self.crew_presence()?;
        let generation = self.lock().generation;
        match control.end_call(call).await {
            Ok(()) => {}
            Err(error) if error.code() == Some(&ErrorCode::CallNotFound) => {}
            Err(error) => return Err(error),
        }
        self.update(generation, |core| core.state.call_gone(call));
        Ok(())
    }

    /// "Birlikte oyna" (no text; one per friend per 5 minutes, 30 a day).
    pub async fn invite(&self, id: &str) -> Result<InviteResult> {
        let control = {
            let core = self.lock();
            if core.auth != AuthPhase::SignedIn {
                return Err(SocialError::Unauthorized);
            }
            core.presence
                .clone()
                .ok_or(SocialError::Unavailable { retry_after: None })?
        };
        Ok(control.invite(id).await?.into())
    }

    /// Answered or closed: the invite leaves the lists and stays closed until it expires, also
    /// over a restart.
    pub fn dismiss_invite(&self, id: &str) {
        let generation = self.lock().generation;
        self.update(generation, |core| core.state.dismiss_invite(id));
        let (file, account, closed) = {
            let core = self.lock();
            let (Some(file), Some(me)) = (&core.closed_invites, core.state.me()) else {
                return;
            };
            (
                file.clone(),
                me.profile.id.clone(),
                core.state.closed_invites().clone(),
            )
        };
        if let Err(error) = file.save(&account, &closed, unix_now()) {
            tracing::info!(%error, "closed invites are kept for this run only");
        }
    }

    pub async fn sessions(&self) -> Result<Vec<SessionInfo>> {
        let (sessions, _) = self
            .call(|session| async move { session.sessions().await })
            .await?;
        Ok(sessions)
    }

    pub async fn revoke_session(&self, id: &str) -> Result<()> {
        let id = id.to_owned();
        self.call(|session| async move { session.revoke_session(&id).await })
            .await
            .map(drop)
    }

    /// Signs out every other device; returns how many.
    pub async fn revoke_other_sessions(&self) -> Result<u32> {
        let (revoked, _) = self
            .call(|session| async move { session.revoke_other_sessions().await })
            .await?;
        Ok(revoked)
    }

    /// "Verilerimi sil": the account and everything about it, for good. Signed out after.
    pub async fn delete_account(&self) -> Result<()> {
        // The service closes the presence socket (4002) before it answers.
        self.lock().deleting = true;
        let result = self
            .call(|session| async move { session.delete_account().await })
            .await;
        let generation = match result {
            Ok((_, generation)) => generation,
            Err(error) => {
                self.lock().deleting = false;
                return Err(error);
            }
        };
        {
            let mut core = self.lock();
            if core.generation == generation {
                if let Some(history) = core.history.as_mut() {
                    history.erase();
                }
                reset(&mut core);
            }
        }
        self.inner.tokens.clear();
        self.emit_view();
        Ok(())
    }

    // --- settings backup ------------------------------------------------------------------

    /// Saves launcher preferences (never secrets: the launcher sends an allow-list). When
    /// another computer saved in between, the newest preferences win.
    pub async fn upload_settings(&self, data: serde_json::Value) -> Result<SettingsMeta> {
        let (session, generation) = self.signed_in()?;
        {
            let core = self.lock();
            if let Some(limited) = core.state.me().and_then(|me| me.limited.as_ref())
                && limited.until > unix_now()
                && limited
                    .items
                    .iter()
                    .any(|item| item.key == LimitedKey::SettingsBackup)
            {
                return Err(SocialError::Limited {
                    item: LimitedKey::SettingsBackup,
                    until: Some(limited.until),
                    retry_after: None,
                });
            }
            if let Some(until) = core.backup_waits_until
                && until > unix_now()
            {
                let left = u64::try_from(until - unix_now()).unwrap_or(0);
                return Err(SocialError::Unavailable {
                    retry_after: Some(Duration::from_secs(left)),
                });
            }
        }
        let version = self.lock().settings_version;
        let device = self.inner.config.device.clone();
        let precondition =
            version.map_or(SettingsPrecondition::Create, SettingsPrecondition::Replace);
        let result = match session.put_settings(&device, &data, precondition).await {
            Err(SocialError::SettingsConflict { current }) => {
                let version = match current {
                    Some(meta) => Some(meta.version),
                    None => session.settings().await?.map(|backup| backup.version),
                };
                let precondition =
                    version.map_or(SettingsPrecondition::Create, SettingsPrecondition::Replace);
                session.put_settings(&device, &data, precondition).await
            }
            other => other,
        };
        match result {
            Ok(meta) => {
                let shown = meta.clone();
                self.update(generation, |core| {
                    core.settings_version = Some(shown.version);
                    core.state.set_settings_meta(Some(shown));
                    core.backup_waits_until = None;
                });
                Ok(meta)
            }
            Err(SocialError::Unauthorized) => {
                self.end_session(generation, false);
                Err(SocialError::Unauthorized)
            }
            Err(SocialError::Restricted { until }) => {
                self.end_restricted(generation, until);
                Err(SocialError::Restricted { until })
            }
            // Storage protection: uploads wait until the next night (`Retry-After`).
            Err(SocialError::Unavailable {
                retry_after: Some(wait),
            }) => {
                let until = unix_now() + i64::try_from(wait.as_secs()).unwrap_or(0);
                self.update(generation, |core| core.backup_waits_until = Some(until));
                Err(SocialError::Unavailable {
                    retry_after: Some(wait),
                })
            }
            Err(error) => Err(error),
        }
    }

    /// The saved preferences, or None when there is no backup.
    pub async fn download_settings(&self) -> Result<Option<serde_json::Value>> {
        let (backup, generation) = self
            .call(|session| async move { session.settings().await })
            .await?;
        if let Some(backup) = &backup {
            let version = backup.version;
            let mut core = self.lock();
            if core.generation == generation {
                core.settings_version = Some(version);
            }
        }
        Ok(backup.map(|backup| backup.data))
    }

    /// Turns the backup off and deletes the stored copy.
    pub async fn delete_settings(&self) -> Result<()> {
        let (_, generation) = self
            .call(|session| async move { session.delete_settings().await })
            .await?;
        self.update(generation, |core| {
            core.settings_version = None;
            core.state.set_settings_meta(None);
        });
        Ok(())
    }

    // --- chat (CHAT1) ------------------------------------------------------------------------

    /// Keeps the chat history under this folder (`<folder>/<account id>/`); without it, for this
    /// run only.
    pub fn keep_chat_in(&self, path: impl Into<PathBuf>) {
        self.lock().chat_root = Some(path.into());
    }

    /// Chat may show (signed config `features.chat`; friends must be on too). The presence
    /// connection asks for chat only while it may: a change reconnects it.
    pub fn set_chat_enabled(&self, on: bool) {
        {
            let mut core = self.lock();
            if core.chat_enabled == on {
                return;
            }
            core.chat_enabled = on;
            if core.auth == AuthPhase::SignedIn && core.presence.is_some() {
                self.start_presence(&mut core);
            }
        }
        self.emit_view();
    }

    /// What the view's conversation `conv` may do now: chat on, not muted, still friends or still
    /// in the crew.
    fn chat_writable(&self, conv: &str) -> Result<()> {
        let core = self.lock();
        if core.auth != AuthPhase::SignedIn {
            return Err(SocialError::Unauthorized);
        }
        let status = match core.chat_status {
            Some(status) if core.chat_enabled => status,
            _ => return Err(SocialError::InvalidInput("chat off")),
        };
        if !status.on {
            return Err(SocialError::InvalidInput("chat off"));
        }
        let skew = core.chat_skew_ms / 1000;
        if status
            .muted_until
            .is_some_and(|until| until + skew > unix_now())
        {
            return Err(SocialError::ChatMuted {
                until: status.muted_until,
            });
        }
        if !conversation_writable(&core.state, conv) {
            return Err(SocialError::InvalidInput("chat not allowed"));
        }
        Ok(())
    }

    /// A conversation's newest messages (`before` None) or those before a message id, oldest first.
    pub fn chat_page(&self, conv: &str, before: Option<i64>) -> ChatPage {
        let mut core = self.lock();
        let viewer = viewer(&core, conv);
        let (entries, more) = match core.history.as_mut() {
            Some(history) => history.page(conv, before, PAGE_SIZE),
            None => (Vec::new(), false),
        };
        ChatPage {
            conv: conv.to_owned(),
            messages: entries.iter().map(|e| entry_view(e, &viewer)).collect(),
            more,
        }
    }

    /// Sends a message: it shows at once (on its way) and goes when the connection lets it, in
    /// the order typed. Returns its key. Refused at once when chat is off, the player is muted,
    /// the conversation is not theirs to write in, or the text is empty or too long.
    pub fn chat_send(&self, conv: &str, text: &str) -> Result<String> {
        self.chat_writable(conv)?;
        let text = prepare_text(text).ok_or(SocialError::InvalidInput("chat text"))?;
        let nonce = new_nonce();
        {
            let mut core = self.lock();
            let viewer = viewer(&core, conv);
            let change = core
                .history
                .as_mut()
                .and_then(|history| history.add_outgoing(conv, &text, &nonce, unix_now_ms()))
                .ok_or(SocialError::InvalidInput("chat not allowed"))?;
            let view = change_view(&change, &viewer);
            core.chat_outbox.push(view);
        }
        self.emit_view();
        self.kick_sender();
        Ok(format!("n:{nonce}"))
    }

    /// "Tekrar dene" on a message that did not go.
    pub fn chat_retry(&self, key: &str) -> Result<()> {
        let nonce = key
            .strip_prefix("n:")
            .ok_or(SocialError::InvalidInput("chat message"))?;
        let conv = {
            let core = self.lock();
            core.history
                .as_ref()
                .and_then(|history| history.outgoing_of(nonce))
                .map(|outgoing| outgoing.conv.clone())
                .ok_or(SocialError::InvalidInput("chat message"))?
        };
        self.chat_writable(&conv)?;
        {
            let mut core = self.lock();
            let viewer = viewer(&core, &conv);
            // A manual retry starts the clock again: it may go by itself after a reconnect too.
            let at = unix_now_ms();
            if let Some(history) = core.history.as_mut() {
                history.restart_outgoing(nonce, at);
                if let Some(change) = history.set_outgoing(nonce, None) {
                    let view = change_view(&change, &viewer);
                    core.chat_outbox.push(view);
                }
            }
            core.too_fast.remove(nonce);
        }
        self.emit_view();
        self.kick_sender();
        Ok(())
    }

    /// Drops a message that did not go.
    pub fn chat_discard(&self, key: &str) {
        let Some(nonce) = key.strip_prefix("n:") else {
            return;
        };
        {
            let mut core = self.lock();
            let Some(change) = core
                .history
                .as_mut()
                .and_then(|history| history.discard_outgoing(nonce))
            else {
                return;
            };
            let viewer = viewer(&core, &change.conv);
            let view = change_view(&change, &viewer);
            core.chat_outbox.push(view);
        }
        self.emit_view();
    }

    /// Edits the player's own message (within 15 minutes). The new version reaches the view.
    pub async fn chat_edit(&self, conv: &str, id: i64, text: &str) -> Result<()> {
        self.chat_writable(conv)?;
        let text = prepare_text(text).ok_or(SocialError::InvalidInput("chat text"))?;
        let (control, message, generation) = {
            let mut core = self.lock();
            let me = core
                .state
                .me()
                .map(|me| me.profile.id.clone())
                .unwrap_or_default();
            let skew = core.chat_skew_ms;
            let message = core
                .history
                .as_mut()
                .and_then(|history| history.message(conv, id))
                .ok_or(SocialError::InvalidInput("chat message"))?;
            if message.from != me {
                return Err(SocialError::InvalidInput("chat message"));
            }
            if message.id + skew + EDIT_WINDOW_MS <= unix_now_ms() {
                return Err(SocialError::Refused {
                    status: 409,
                    code: ErrorCode::ChatTooOld,
                    message: String::new(),
                });
            }
            let control = core
                .presence
                .clone()
                .ok_or(SocialError::Unavailable { retry_after: None })?;
            (control, message, core.generation)
        };
        if message.text == text {
            return Ok(());
        }
        let stored = match control.chat_edit(&message, &text).await {
            Ok(stored) => stored,
            Err(error) => {
                if error.code() == Some(&ErrorCode::MessageNotFound) {
                    // Deleted meanwhile (on another device, or by a moderator).
                    self.chat_forget(generation, conv, id);
                }
                return Err(error);
            }
        };
        self.chat_store(generation, &stored);
        Ok(())
    }

    /// "Herkesten sil": the player's own message (7 days), or any in a crew they lead or
    /// moderate. Gone from the view at once; it comes back if the service refuses.
    pub async fn chat_delete(&self, conv: &str, id: i64) -> Result<()> {
        let (control, message, generation) = {
            let mut core = self.lock();
            if core.auth != AuthPhase::SignedIn {
                return Err(SocialError::Unauthorized);
            }
            let message = core
                .history
                .as_mut()
                .and_then(|history| history.message(conv, id))
                .ok_or(SocialError::InvalidInput("chat message"))?;
            let control = core
                .presence
                .clone()
                .ok_or(SocialError::Unavailable { retry_after: None })?;
            (control, message, core.generation)
        };
        match control.chat_delete(&message).await {
            Ok(()) => {}
            // Gone already (another device, a moderator): it goes here too.
            Err(error) if error.code() == Some(&ErrorCode::MessageNotFound) => {}
            Err(error) => return Err(error),
        }
        self.chat_forget(generation, conv, id);
        Ok(())
    }

    /// Everything in a conversation is read now (it is on screen, scrolled to the newest).
    pub fn chat_read(&self, conv: &str) {
        let changed = self
            .lock()
            .history
            .as_mut()
            .is_some_and(|history| history.mark_read(conv));
        if changed {
            self.emit_view();
        }
    }

    /// "Geçmişi temizle": the conversation's messages leave this device for good (the service's
    /// mailbox and the other devices keep theirs).
    pub fn chat_clear(&self, conv: &str) {
        {
            let mut core = self.lock();
            let Some(change) = core
                .history
                .as_mut()
                .and_then(|history| history.clear(conv))
            else {
                return;
            };
            let viewer = viewer(&core, conv);
            let view = change_view(&change, &viewer);
            core.chat_outbox.push(view);
        }
        self.emit_view();
    }

    /// A message as the service now has it (an answer to an edit): into the history and the view.
    fn chat_store(&self, generation: u64, message: &ChatMessage) {
        {
            let mut core = self.lock();
            if core.generation != generation {
                return;
            }
            let viewer = viewer(&core, &message.conv);
            if let Some(change) = core
                .history
                .as_mut()
                .and_then(|history| history.put(message))
            {
                let view = change_view(&change, &viewer);
                core.chat_outbox.push(view);
            }
        }
        self.emit_view();
    }

    fn chat_forget(&self, generation: u64, conv: &str, id: i64) {
        {
            let mut core = self.lock();
            if core.generation != generation {
                return;
            }
            let viewer = viewer(&core, conv);
            if let Some(change) = core
                .history
                .as_mut()
                .and_then(|history| history.remove(conv, id))
            {
                let view = change_view(&change, &viewer);
                core.chat_outbox.push(view);
            }
        }
        self.emit_view();
    }

    /// Starts sending the messages that wait, unless that is under way already.
    fn kick_sender(&self) {
        {
            let mut core = self.lock();
            if core.sending {
                return;
            }
            core.sending = true;
        }
        let service = self.clone();
        self.spawn(async move { service.send_waiting().await });
    }

    /// Sends the waiting messages one at a time, in the order typed, while the connection is up.
    /// A message that waited longer than the service remembers a resend is not sent by itself; it
    /// waits for "Tekrar dene".
    async fn send_waiting(&self) {
        loop {
            let next = {
                let mut core = self.lock();
                let online = core.state.connection() == Connection::Online
                    && core.chat_status.is_some()
                    && core.chat_synced;
                let now = unix_now_ms();
                let mut stale = Vec::new();
                let pick = core.history.as_ref().and_then(|history| {
                    history
                        .outgoing()
                        .iter()
                        .filter(|o| o.failed.is_none())
                        .find(|o| {
                            if now - o.at > RESEND_WINDOW_MS {
                                stale.push(o.nonce.clone());
                                false
                            } else {
                                true
                            }
                        })
                        .cloned()
                });
                for nonce in stale {
                    let conv = core
                        .history
                        .as_ref()
                        .and_then(|h| h.outgoing_of(&nonce))
                        .map(|o| o.conv.clone())
                        .unwrap_or_default();
                    let viewer = viewer(&core, &conv);
                    if let Some(change) = core
                        .history
                        .as_mut()
                        .and_then(|h| h.set_outgoing(&nonce, Some(SendFailure::Connection)))
                    {
                        let view = change_view(&change, &viewer);
                        core.chat_outbox.push(view);
                    }
                }
                match (core.presence.clone(), pick) {
                    (Some(control), Some(outgoing)) if online => {
                        Some((control, outgoing, core.generation))
                    }
                    _ => {
                        core.sending = false;
                        None
                    }
                }
            };
            let Some((control, outgoing, generation)) = next else {
                self.emit_view();
                return;
            };
            let result = control
                .chat_send(&outgoing.conv, &outgoing.text, &outgoing.nonce)
                .await;
            let mut wait = None;
            {
                let mut core = self.lock();
                if core.generation != generation {
                    core.sending = false;
                    return;
                }
                let viewer = viewer(&core, &outgoing.conv);
                let change = match result {
                    Ok(message) => core.history.as_mut().and_then(|h| h.put(&message)),
                    Err(error) => {
                        let failure = failure_of(&error);
                        if let SocialError::ChatMuted { until } = error
                            && let Some(status) = core.chat_status.as_mut()
                        {
                            status.muted_until = until;
                        }
                        if failure == SendFailure::Off
                            && let Some(status) = core.chat_status.as_mut()
                        {
                            status.on = false;
                        }
                        let retried = core.too_fast.contains(&outgoing.nonce);
                        match failure {
                            // About one a second: it goes again by itself once, after the wait.
                            SendFailure::TooFast if !retried => {
                                core.too_fast.insert(outgoing.nonce.clone());
                                wait = Some(
                                    error
                                        .retry_after()
                                        .unwrap_or(Duration::from_secs(1))
                                        .min(TOO_FAST_WAIT_MAX),
                                );
                                None
                            }
                            // The connection dropped: it goes after the next connect's sync.
                            SendFailure::Connection
                                if core.state.connection() != Connection::Online =>
                            {
                                core.sending = false;
                                drop(core);
                                self.emit_view();
                                return;
                            }
                            failure => core
                                .history
                                .as_mut()
                                .and_then(|h| h.set_outgoing(&outgoing.nonce, Some(failure))),
                        }
                    }
                };
                if let Some(change) = change {
                    let view = change_view(&change, &viewer);
                    core.chat_outbox.push(view);
                }
            }
            self.emit_view();
            if let Some(wait) = wait {
                tokio::time::sleep(wait).await;
            }
        }
    }

    // --- reports and moderators' notes (MOD1) -------------------------------------------------

    /// "Rapor et": a player, a crew or a message (with the 20 messages before it this device
    /// holds, as the moderators' context). `block` also blocks the player (players and messages).
    /// A second report about the same thing while the first is open changes nothing; either way
    /// the player hears "Raporun iletildi.".
    pub async fn report(
        &self,
        target: ReportTarget,
        reason: ReportReason,
        note: Option<String>,
        block: bool,
    ) -> Result<()> {
        let (wire, blocked) = {
            let mut core = self.lock();
            match &target {
                ReportTarget::Player { id } => (
                    serde_json::json!({ "kind": "player", "id": id }),
                    Some(id.clone()),
                ),
                ReportTarget::Crew { id, code } => {
                    let mut wire = serde_json::json!({ "kind": "crew", "id": id });
                    if let Some(code) = code.as_ref().and_then(|c| normalize_team_code(c)) {
                        wire["code"] = code.into();
                    }
                    (wire, None)
                }
                ReportTarget::Message { conv, id } => {
                    let history = core
                        .history
                        .as_mut()
                        .ok_or(SocialError::InvalidInput("chat message"))?;
                    let message = history
                        .message(conv, *id)
                        .ok_or(SocialError::InvalidInput("chat message"))?;
                    let before: Vec<serde_json::Value> = history
                        .before(conv, *id, REPORT_CONTEXT)
                        .iter()
                        .map(ChatMessage::sealed)
                        .collect();
                    let sender = message.from.clone();
                    (
                        serde_json::json!({
                            "kind": "message",
                            "message": message.sealed(),
                            "before": before,
                        }),
                        Some(sender),
                    )
                }
            }
        };
        let block = block && blocked.is_some();
        // Taken before the call: the block also reaches the socket, which may take them off the
        // lists before the answer comes.
        let profile = blocked
            .as_deref()
            .filter(|_| block)
            .and_then(|id| self.lock().state.profile(id));
        let (_, generation) = self
            .call(
                |session| async move { session.report(wire, reason, note.as_deref(), block).await },
            )
            .await?;
        if let (true, Some(id)) = (block, blocked) {
            self.update(generation, |core| match profile {
                Some(profile) => core.state.blocked(profile),
                None => core.state.friend_removed(&id),
            });
        }
        Ok(())
    }

    /// The launcher showed these moderators' notes: they leave the view and the service hears it.
    pub async fn notices_seen(&self, ids: Vec<String>) {
        let generation = self.lock().generation;
        self.update(generation, |core| core.state.notices_seen(&ids));
        if let Err(error) = self
            .call(|session| async move { session.notices_seen(&ids).await })
            .await
        {
            tracing::info!(%error, "the notes' seen mark waits for the next start");
        }
    }

    // --- the game -------------------------------------------------------------------------

    /// The game started. `quick_play` is the server the launch joins by itself (as from
    /// [`crate::server_label`]), when it does.
    pub fn game_started(&self, quick_play: Option<&str>) {
        {
            let mut core = self.lock();
            let game = GameActivity::started(quick_play);
            core.activity = game.status().clone();
            core.game = Some(game);
            if let Some(presence) = &core.presence {
                presence.set_status(core.activity.clone());
            }
            observe_together(&mut core);
        }
        self.emit_view();
    }

    /// One line of the game's log (cheap for lines that mean nothing).
    pub fn game_line(&self, message: &str) {
        if !game::may_signal(message) {
            return;
        }
        {
            let mut core = self.lock();
            let Some(next) = core.game.as_mut().and_then(|game| game.observe(message)) else {
                return;
            };
            core.activity = next.clone();
            if let Some(presence) = &core.presence {
                presence.set_status(next);
            }
            observe_together(&mut core);
        }
        self.emit_view();
    }

    /// The game closed: back to "in the launcher".
    pub fn game_stopped(&self) {
        {
            let mut core = self.lock();
            if core.game.take().is_none() && core.activity == LocalStatus::Launcher {
                return;
            }
            core.activity = LocalStatus::Launcher;
            if let Some(presence) = &core.presence {
                presence.set_status(LocalStatus::Launcher);
            }
            observe_together(&mut core);
        }
        self.emit_view();
    }
}

/// Who is on Projects together with the player now ("Birlikte oynanan süre"): the time since the
/// last change is kept when it changes. Nobody while signed out or without crews on.
fn observe_together(core: &mut Core) {
    let now = unix_now();
    let account = if core.auth == AuthPhase::SignedIn {
        core.state.me().map(|me| me.profile.id.clone())
    } else {
        None
    };
    let with = if core.teams && account.is_some() {
        core.state.together_now(&core.activity)
    } else {
        Together::default()
    };
    core.together
        .observe(account.as_deref(), with, now, || Periods::at(now));
}

/// What a crew code or link led to, for the UI.
fn lookup_view(lookup: TeamLookup, code: String, link: Option<String>) -> TeamLookupView {
    TeamLookupView {
        team_id: lookup.team.id,
        name: lookup.team.name,
        tag: lookup.team.tag,
        emblem: lookup.team.emblem,
        color: lookup.team.color,
        frame: lookup.team.frame,
        motto: lookup.team.motto,
        members: lookup.team.members,
        relation: lookup.relation,
        code,
        link,
        join: lookup.team.join,
        requested_at: lookup.request.map(|request| request.sent_at),
    }
}

/// The invites this account answered or closed in an earlier run stay closed (once the profile
/// says whose they are).
fn remember_closed_invites(core: &mut Core) {
    let (Some(file), Some(me)) = (&core.closed_invites, core.state.me()) else {
        return;
    };
    let now = unix_now();
    let closed = file.load(&me.profile.id, now);
    if !closed.is_empty() {
        core.state.remember_closed_invites(closed, now);
    }
}

/// Back to signed out, keeping the configuration and the saved token.
fn reset(core: &mut Core) {
    // The time played together so far is kept before the account goes.
    let now = unix_now();
    core.together
        .observe(None, Together::default(), now, || Periods::at(now));
    core.generation += 1;
    // Dropping the control closes the presence connection cleanly.
    core.presence = None;
    core.session = None;
    core.login = None;
    core.auth = AuthPhase::SignedOut;
    core.state = SocialState::default();
    core.settings_version = None;
    core.fetched_at = None;
    core.fetching = false;
    core.deleting = false;
    core.qr = None;
    core.groups_saving = false;
    core.groups_next = None;
    core.groups_saved = None;
    core.history = None;
    core.chat_setup.since.store(0, Ordering::Relaxed);
    core.chat_status = None;
    core.chat_skew_ms = 0;
    core.chat_synced = false;
    core.chat_backlog.clear();
    core.chat_outbox.clear();
    core.sending = false;
    core.too_fast.clear();
    core.backup_waits_until = None;
}

/// The categories the UI wants saved, checked as the service will check them.
fn groups_document(groups: Vec<FriendGroup>, members: Vec<GroupMember>) -> Result<FriendGroups> {
    let invalid = SocialError::InvalidInput("friend categories");
    if groups.len() > MAX_FRIEND_GROUPS {
        return Err(invalid);
    }
    let mut document = FriendGroups::default();
    for group in groups {
        let name = group.name.split_whitespace().collect::<Vec<_>>().join(" ");
        let valid_id = (1..=12).contains(&group.id.len())
            && group
                .id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit());
        let length = name.chars().count();
        if !valid_id
            || !(1..=24).contains(&length)
            || document.groups.iter().any(|g| g.id == group.id)
        {
            return Err(invalid);
        }
        document.groups.push(FriendGroup { id: group.id, name });
    }
    for member in members {
        if document.groups.iter().any(|g| g.id == member.group) {
            document.members.insert(member.friend, member.group);
        }
    }
    Ok(document)
}

/// The chat history of the signed-in account, once its id is known; chat events that waited for
/// it are applied now.
fn open_history(core: &mut Core) {
    let Some(me) = core.state.me().map(|me| me.profile.id.clone()) else {
        return;
    };
    if core
        .history
        .as_ref()
        .is_some_and(|history| history.me() == me)
    {
        return;
    }
    let history = History::open(core.chat_root.as_deref(), &me);
    core.chat_setup
        .since
        .store(history.sync_since(), Ordering::Relaxed);
    core.history = Some(history);
    for event in std::mem::take(&mut core.chat_backlog) {
        let outcome = apply_chat(core, &event);
        // Messages that waited are no news: they show as unread.
        drop(outcome.notices);
    }
}

/// What a chat event did: changes for the UI (in the outbox), news worth telling, and whether
/// waiting messages may go now.
#[derive(Default)]
struct ChatOutcome {
    notices: Vec<SocialNotice>,
    deliver: bool,
}

/// How the player sees a conversation's messages (their times, what they may still do).
fn viewer(core: &Core, conv: &str) -> Viewer {
    let me = core
        .state
        .me()
        .map(|me| me.profile.id.clone())
        .unwrap_or_default();
    let moderates = match Conversation::parse(conv) {
        Some(Conversation::Crew(team)) => core.state.crew(&team).is_some_and(|entry| {
            entry
                .team
                .members
                .iter()
                .any(|m| m.profile.id == me && (m.role == TeamRole::Founder || m.moderator))
        }),
        _ => false,
    };
    Viewer {
        me,
        skew_ms: core.chat_skew_ms,
        now_ms: unix_now_ms(),
        moderates,
    }
}

/// The player may write in `conv` now: a crew they are in, or a friend (both ways).
fn conversation_writable(state: &SocialState, conv: &str) -> bool {
    let Some(me) = state.me().map(|me| me.profile.id.as_str()) else {
        return false;
    };
    match Conversation::parse(conv) {
        Some(Conversation::Crew(team)) => state.crew(&team).is_some(),
        Some(direct @ Conversation::Direct(..)) => direct
            .peer(me)
            .is_some_and(|peer| state.friends().iter().any(|f| f.id == peer)),
        None => false,
    }
}

/// The sender's name as the lists show it (a friend, or a crewmate).
fn sender_name(state: &SocialState, id: &str) -> Option<String> {
    if let Some(friend) = state.friends().into_iter().find(|f| f.id == id) {
        return Some(friend.name);
    }
    state
        .crews()
        .iter()
        .flat_map(|entry| entry.team.members.iter())
        .find(|m| m.profile.id == id)
        .map(|m| m.profile.name().to_owned())
}

/// Applies a chat event: the state of this connection, the history, the UI's changes.
fn apply_chat(core: &mut Core, event: &PresenceEvent) -> ChatOutcome {
    let mut outcome = ChatOutcome::default();
    let now_ms = unix_now_ms();
    match event {
        PresenceEvent::Connected { chat, clock, .. } => {
            core.chat_status = *chat;
            core.chat_synced = false;
            if let Some(clock) = clock {
                core.chat_skew_ms = now_ms - clock * 1_000;
            }
            return outcome;
        }
        PresenceEvent::ChatState(status) => {
            core.chat_status = Some(*status);
            return outcome;
        }
        PresenceEvent::ChatSynced { clock } => {
            if let Some(clock) = clock {
                core.chat_skew_ms = now_ms - clock;
            }
            core.chat_synced = true;
            outcome.deliver = true;
            return outcome;
        }
        PresenceEvent::Disconnected { .. } | PresenceEvent::Stopped(_) => {
            core.chat_synced = false;
            return outcome;
        }
        PresenceEvent::ChatMessage(_)
        | PresenceEvent::ChatEdited(_)
        | PresenceEvent::ChatDeleted { .. }
        | PresenceEvent::ChatSync { .. } => {}
        _ => return outcome,
    }
    if core.history.is_none() {
        if core.chat_backlog.len() < CHAT_BACKLOG_MAX {
            core.chat_backlog.push(event.clone());
        }
        return outcome;
    }
    let mut changes: Vec<Change> = Vec::new();
    let mut news: Option<ChatMessage> = None;
    if let Some(history) = core.history.as_mut() {
        let me = history.me().to_owned();
        match event {
            PresenceEvent::ChatMessage(message) => {
                if let Some(change) = history.put(message) {
                    if message.from != me {
                        news = Some(message.clone());
                    }
                    changes.push(change);
                }
            }
            PresenceEvent::ChatEdited(message) => changes.extend(history.put(message)),
            PresenceEvent::ChatDeleted { conv, id, .. } => {
                changes.extend(history.remove(conv, *id));
            }
            PresenceEvent::ChatSync {
                conv,
                messages,
                deleted,
            } => {
                let mut list = Vec::new();
                for message in messages {
                    list.extend(history.put(message));
                }
                for deletion in deleted {
                    list.extend(history.remove(conv, deletion.id));
                }
                changes.extend(merge(list));
            }
            _ => {}
        }
        core.chat_setup
            .since
            .store(history.sync_since(), Ordering::Relaxed);
    }
    for change in changes {
        let viewer = viewer(core, &change.conv);
        core.chat_outbox.push(change_view(&change, &viewer));
    }
    if let Some(message) = news {
        let viewer = viewer(core, &message.conv);
        let crew = match Conversation::parse(&message.conv) {
            Some(Conversation::Crew(team)) => Some(team),
            _ => None,
        };
        outcome.notices.push(SocialNotice::ChatMessage {
            conv: message.conv.clone(),
            message: Box::new(message_view(&message, &viewer)),
            name: sender_name(&core.state, &message.from),
            crew,
        });
    }
    outcome
}

/// The chat part of the view: the conversations this device holds and the player's crews.
fn chat_view(core: &mut Core, now: i64) -> Option<ChatView> {
    if !core.chat_enabled {
        return None;
    }
    let status = core.chat_status?;
    let me = core.state.me()?.profile.id.clone();
    let skew = core.chat_skew_ms;
    let friends = core.state.friends();
    let blocked: HashSet<String> = core
        .state
        .blocked_users()
        .into_iter()
        .map(|profile| profile.id)
        .collect();
    let crews: Vec<(String, String)> = core
        .state
        .crews()
        .iter()
        .map(|entry| (entry.team.id.clone(), entry.team.name.clone()))
        .collect();
    let history = core.history.as_mut()?;
    let mut titles: Vec<(String, String)> = Vec::new();
    let mut failed: HashSet<String> = HashSet::new();
    for outgoing in history.outgoing() {
        if outgoing.failed.is_some() {
            failed.insert(outgoing.conv.clone());
        }
    }
    let waiting: HashSet<String> = history
        .outgoing()
        .iter()
        .map(|outgoing| outgoing.conv.clone())
        .collect();
    let mut list: Vec<ConversationView> = Vec::new();
    for (conv, meta) in history.conversations() {
        let Some(kind) = Conversation::parse(conv) else {
            continue;
        };
        if meta.last.is_none() && !waiting.contains(conv) {
            continue;
        }
        let (peer, crew, known, writable) = match &kind {
            Conversation::Crew(team) => {
                let known = crews
                    .iter()
                    .find(|(id, _)| id == team)
                    .map(|(_, n)| n.clone());
                let writable = known.is_some();
                (None, Some(team.clone()), known, writable)
            }
            Conversation::Direct(..) => {
                let Some(peer) = kind.peer(&me) else {
                    continue;
                };
                if blocked.contains(peer) {
                    continue;
                }
                let known = friends
                    .iter()
                    .find(|f| f.id == peer)
                    .map(|f| f.name.clone());
                let writable = known.is_some();
                (Some(peer.to_owned()), None, known, writable)
            }
        };
        if let Some(name) = &known
            && meta.title.as_deref() != Some(name.as_str())
        {
            titles.push((conv.to_owned(), name.clone()));
        }
        list.push(ConversationView {
            id: conv.to_owned(),
            peer,
            crew,
            title: known.or_else(|| meta.title.clone()),
            last: meta.last.as_ref().map(|last| LastMessageView {
                from: last.f.clone(),
                mine: last.f == me,
                text: last.t.clone(),
                at: last.i + skew,
            }),
            unread: meta.unread,
            writable,
            failed: failed.contains(conv),
        });
    }
    // Every crew of the player's has its chat, messages or not.
    for (team, name) in &crews {
        let conv = format!("c:{team}");
        if !list.iter().any(|c| c.id == conv) {
            list.push(ConversationView {
                id: conv,
                peer: None,
                crew: Some(team.clone()),
                title: Some(name.clone()),
                last: None,
                unread: 0,
                writable: true,
                failed: false,
            });
        }
    }
    for (conv, title) in titles {
        history.set_title(&conv, &title);
    }
    list.sort_by_key(|c| std::cmp::Reverse(c.last.as_ref().map_or(i64::MIN, |last| last.at)));
    Some(ChatView {
        on: status.on,
        mailbox: status.mailbox,
        per_day: status.per_day,
        muted_until: status
            .muted_until
            .map(|until| until + skew / 1_000)
            .filter(|until| *until > now),
        conversations: list,
    })
}

fn unix_now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        })
}

fn build_view(core: &mut Core) -> SocialView {
    let now = unix_now();
    let share = match (core.state.me(), core.client.as_ref()) {
        (Some(me), Some(client)) => client
            .friend_share_link(&me.friend_code)
            .map(|url| url.to_string()),
        _ => None,
    };
    let share = share.map(|link| {
        if core.qr.as_ref().is_none_or(|(cached, _)| *cached != link) {
            let qr = qr_path(&link);
            core.qr = Some((link.clone(), qr));
        }
        let qr = core.qr.as_ref().and_then(|(_, qr)| qr.clone());
        (link, qr)
    });
    let teams = core.teams && core.state.teams_supported();
    let calls = teams && core.state.offers(TEAM_FEATURE_CALLS);
    let share_server = core.state.me().is_some_and(|me| me.privacy.share_server);
    let mut crews = if teams {
        core.state.crew_views(now)
    } else {
        Vec::new()
    };
    // Invite links as this service shares them (`<service>/e/XXXX-XXXX-XXXX`).
    if let Some(client) = core.client.as_ref() {
        for crew in &mut crews {
            crew.link_url = crew
                .link
                .as_deref()
                .and_then(|link| client.crew_link_url(link))
                .map(|url| url.to_string());
        }
    }
    // "Birlikte oynanan süre", from this computer only.
    let together = core.together.enabled();
    if together
        && !crews.is_empty()
        && let Some(me) = core.state.me().map(|me| me.profile.id.clone())
    {
        let periods = Periods::at(now);
        for crew in &mut crews {
            crew.together_week = core.together.crew_week(&me, &crew.id, now, &periods);
            for member in crew.members.iter_mut().filter(|member| !member.me) {
                member.together_month = core.together.person_month(&me, &member.id, now, &periods);
            }
        }
    }
    SocialView {
        seq: core.seq,
        enabled: core.url.is_some(),
        auth: core.auth,
        connection: core.state.connection(),
        me: core.state.me().map(|me| me_view(me, share)),
        loaded: core.state.loaded(),
        friends: core.state.friends(),
        incoming: core.state.incoming(),
        outgoing: core.state.outgoing(),
        blocked: core.state.blocked_users(),
        invites: core.state.invites(now),
        limits: core.state.limits(),
        activity: ActivityView::from(&core.activity),
        groups: core.state.groups_view(),
        teams,
        team_features: if teams {
            core.state.team_features().to_vec()
        } else {
            Vec::new()
        },
        crews,
        crew_limit: core.state.crew_limit(),
        team_invites: if teams {
            core.state.team_invites()
        } else {
            Vec::new()
        },
        team_requests: if teams && core.state.offers(TEAM_FEATURE_REQUESTS) {
            core.state.team_requests(now)
        } else {
            Vec::new()
        },
        plans: if teams && core.state.offers(TEAM_FEATURE_PLANS) {
            core.state.plans(now)
        } else {
            Vec::new()
        },
        together,
        calls: if calls {
            core.state.calls(now)
        } else {
            Vec::new()
        },
        call_place: CallPlace::of(&core.activity, share_server),
        crew_notifications: if teams {
            core.state.team_notifications().map(|settings| {
                let settings = settings.at(now);
                CrewNotificationsView {
                    calls: settings.calls,
                    muted: settings.muted,
                    mute_until: settings.mute_until,
                }
            })
        } else {
            None
        },
        chat_allowed: core.chat_enabled,
        chat: chat_view(core, now),
        notices: core.state.notices(),
        restriction: core.restriction,
        backup_waits_until: core.backup_waits_until.filter(|until| *until > now),
    }
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use super::*;
    use crate::auth::SessionToken;
    use crate::model::MAX_MUTE_SECS;
    use crate::store::MemoryTokenStore;
    use crate::testutil::{MockReply, MockServer};

    const ME: &str = r#"{"id":"123456789012345678","username":"ayse","displayName":"Ayşe","avatar":"discord",
        "avatarHash":null,"discordAvatarHash":null,"friendCode":"PRJ-ABCD-EFGH","status":"online",
        "privacy":{"shareServer":false,"shareLastSeen":true},"createdOn":"2026-10-16",
        "settings":{"version":3,"updatedAt":1760000000,"device":"Laptop","size":64}}"#;
    const FRIENDS: &str = r#"{"friends":[{"id":"223456789012345678","username":"bora","displayName":"Bora",
        "avatar":"preset:fox","avatarHash":null,"since":"2026-10-01","lastSeen":"today"}],
        "incoming":[],"outgoing":[],"blocked":[],"limits":{"friends":200,"outgoingRequests":50}}"#;

    struct Harness {
        service: SocialService,
        tokens: Arc<MemoryTokenStore>,
        updates: Arc<Mutex<Vec<SocialUpdate>>>,
    }

    impl Harness {
        fn new(tokens: Arc<MemoryTokenStore>) -> Self {
            let updates = Arc::new(Mutex::new(Vec::new()));
            let sink: UpdateSink = {
                let updates = updates.clone();
                Arc::new(move |update| updates.lock().unwrap().push(update))
            };
            let service = SocialService::new(
                reqwest::Client::new(),
                ServiceConfig {
                    user_agent: "ProjectsLauncher/test".into(),
                    device: "Ev PC".into(),
                    presence: PresenceOptions::default(),
                },
                tokens.clone(),
                Handle::current(),
                sink,
            );
            Self {
                service,
                tokens,
                updates,
            }
        }

        fn notices(&self) -> Vec<SocialNotice> {
            self.updates
                .lock()
                .unwrap()
                .iter()
                .filter_map(|update| match update {
                    SocialUpdate::Notice(notice) => Some(notice.clone()),
                    SocialUpdate::View(_) | SocialUpdate::Chat(_) => None,
                })
                .collect()
        }

        /// Waits until the current view satisfies `predicate`.
        async fn until(&self, what: &str, predicate: impl Fn(&SocialView) -> bool) -> SocialView {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                let view = self.service.view();
                if predicate(&view) {
                    return view;
                }
                assert!(
                    Instant::now() < deadline,
                    "timed out waiting for {what}: {view:?}"
                );
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }
    }

    fn token() -> SessionToken {
        SessionToken::new(format!("pls_{}", "A".repeat(43)))
    }

    fn url(server: &MockServer) -> Option<Url> {
        Some(Url::parse(&server.base()).unwrap())
    }

    /// The presence socket is refused (an outage), so tests only see the HTTP side.
    fn presence_refused(path: &str) -> Option<MockReply> {
        path.starts_with("/v1/presence").then(|| {
            MockReply::json(503, r#"{"error":{"code":"unavailable","message":"x"}}"#)
                .with_header("Retry-After", "600")
        })
    }

    /// An invite the player closed is listed again by the service on every connection, also
    /// after the launcher restarts: it stays closed until it expires.
    #[tokio::test]
    async fn a_closed_invite_stays_closed_after_a_restart() {
        let server = MockServer::start(|req| {
            presence_refused(&req.path).unwrap_or_else(|| match req.path.as_str() {
                "/v1/me" => MockReply::json(200, ME),
                "/v1/friends" => MockReply::json(200, FRIENDS),
                _ => MockReply::json(404, r#"{"error":{"code":"not_found","message":"x"}}"#),
            })
        })
        .await;
        let dir =
            std::env::temp_dir().join(format!("pl-social-closed-restart-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = dir.join("social").join("closed-invites.json");
        let tokens = Arc::new(MemoryTokenStore::new());
        tokens.save(&token()).unwrap();
        let bora = crate::protocol::Invite {
            id: "inv-1".into(),
            from: "223456789012345678".into(),
            at: unix_now() - 120,
            server: None,
            quiet: false,
        };
        let hello = |invite: &crate::protocol::Invite| PresenceEvent::Connected {
            status: crate::model::StatusMode::Online,
            friends: vec![],
            invites: vec![crate::protocol::Invite {
                quiet: true,
                ..invite.clone()
            }],
            calls: vec![],
            clock: None,
            chat: None,
            plans: None,
        };
        let signed_in = |view: &SocialView| view.loaded && view.auth == AuthPhase::SignedIn;

        // First run: Bora's invite arrives and the player closes it ("Kapat").
        let first = Harness::new(tokens.clone());
        first.service.keep_closed_invites_in(&file);
        first.service.configure(url(&server));
        first.until("signed in", signed_in).await;
        first
            .service
            .lock()
            .state
            .apply(&PresenceEvent::Invite(bora.clone()), unix_now());
        assert_eq!(first.service.view().invites.len(), 1);
        first.service.dismiss_invite("inv-1");
        assert!(first.service.view().invites.is_empty());
        assert!(file.exists());

        // The launcher restarts within the hour; the service lists the invite again.
        let second = Harness::new(tokens.clone());
        second.service.keep_closed_invites_in(&file);
        second.service.configure(url(&server));
        second.until("signed in", signed_in).await;
        second.service.lock().state.apply(&hello(&bora), unix_now());
        assert!(second.service.view().invites.is_empty());

        // Without the file (as before) it came back.
        let forgetful = Harness::new(tokens);
        forgetful.service.configure(url(&server));
        forgetful.until("signed in", signed_in).await;
        forgetful
            .service
            .lock()
            .state
            .apply(&hello(&bora), unix_now());
        assert_eq!(forgetful.service.view().invites.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn stays_hidden_until_configured() {
        let harness = Harness::new(Arc::new(MemoryTokenStore::new()));
        let view = harness.service.view();
        assert!(!view.enabled);
        assert_eq!(view.auth, AuthPhase::SignedOut);
        // The game is followed even while the feature is off (cheap, nothing is sent).
        harness.service.game_started(Some("play.projects.gg"));
        harness
            .service
            .game_line("Connecting to node.example.net, 25565");
        assert_eq!(
            harness.service.view().activity,
            ActivityView::Playing {
                host: Some("play.projects.gg".into())
            }
        );
        harness.service.game_stopped();
        assert_eq!(harness.service.view().activity, ActivityView::Launcher);
    }

    #[tokio::test]
    async fn restores_a_saved_session_and_loads_friends() {
        let server = MockServer::start(|req| {
            presence_refused(&req.path).unwrap_or_else(|| match req.path.as_str() {
                "/v1/me" => MockReply::json(200, ME),
                "/v1/friends" => MockReply::json(200, FRIENDS),
                _ => MockReply::json(404, r#"{"error":{"code":"not_found","message":"x"}}"#),
            })
        })
        .await;
        let tokens = Arc::new(MemoryTokenStore::new());
        tokens.save(&token()).unwrap();
        let harness = Harness::new(tokens);
        harness.service.configure(url(&server));
        let view = harness
            .until("friends", |view| {
                view.loaded && view.auth == AuthPhase::SignedIn
            })
            .await;
        assert!(view.enabled);
        let me = view.me.unwrap();
        assert_eq!(me.name, "Ayşe");
        assert_eq!(me.friend_code, "PRJ-ABCD-EFGH");
        assert!(me.share_link.unwrap().ends_with("/f/PRJ-ABCD-EFGH"));
        assert!(me.qr.is_some());
        assert_eq!(view.friends[0].name, "Bora");
        // Configuring the same service again changes nothing.
        let before = harness.updates.lock().unwrap().len();
        harness.service.configure(url(&server));
        assert_eq!(harness.updates.lock().unwrap().len(), before);
        // Turning the feature off hides it but keeps the saved session.
        harness.service.configure(None);
        assert!(!harness.service.view().enabled);
        assert!(harness.tokens.load().is_some());
    }

    const FRIENDS_WITH_TEAM: &str = r#"{"friends":[{"id":"223456789012345678","username":"bora",
        "displayName":"Bora","avatar":"preset:fox","avatarHash":null,"since":"2026-10-01","lastSeen":"today"}],
        "incoming":[],"outgoing":[],"blocked":[],
        "limits":{"friends":200,"outgoingRequests":50,"teamMembers":30,"groups":20},
        "groups":{"version":0,"groups":[],"members":{}},
        "team":{"id":"0123456789abcdef","name":"Gece Kartalları","tag":"GKR","emblem":"sword",
          "code":"TKM-ABCD-EFGH","createdOn":"2026-10-19","members":[{"id":"123456789012345678",
          "username":"ayse","displayName":"Ayşe","avatar":"discord","avatarHash":null,
          "role":"founder","joinedAt":5}]},
        "teamInvited":[],"teamInvites":[]}"#;

    #[tokio::test]
    async fn teams_wait_for_the_flag_and_categories_save_in_order() {
        let puts = Arc::new(AtomicUsize::new(0));
        let elsewhere = Arc::new(AtomicBool::new(false));
        let failing = Arc::new(AtomicBool::new(false));
        let server = MockServer::start({
            let puts = puts.clone();
            let elsewhere = elsewhere.clone();
            let failing = failing.clone();
            move |req| {
                presence_refused(&req.path).unwrap_or_else(|| match req.path.as_str() {
                    "/v1/me" => MockReply::json(200, ME),
                    // Another computer saved its own categories in between.
                    "/v1/friends" if elsewhere.load(Ordering::SeqCst) => MockReply::json(
                        200,
                        &FRIENDS_WITH_TEAM.replace(
                            r#""groups":{"version":0,"groups":[],"members":{}}"#,
                            r#""groups":{"version":5,"groups":[{"id":"pvp","name":"PvP"}],"members":{}}"#,
                        ),
                    ),
                    "/v1/friends" => MockReply::json(200, FRIENDS_WITH_TEAM),
                    "/v1/friends/groups" if failing.load(Ordering::SeqCst) => MockReply::json(
                        503,
                        r#"{"error":{"code":"unavailable","message":"x"}}"#,
                    ),
                    "/v1/friends/groups" => {
                        let n = puts.fetch_add(1, Ordering::SeqCst) + 1;
                        MockReply::json(200, &format!(r#"{{"version":{n}}}"#))
                    }
                    "/v1/teams/0123456789abcdef/invites" => MockReply::empty(204),
                    _ => MockReply::json(404, r#"{"error":{"code":"not_found","message":"x"}}"#),
                })
            }
        })
        .await;
        let tokens = Arc::new(MemoryTokenStore::new());
        tokens.save(&token()).unwrap();
        let harness = Harness::new(tokens);
        harness.service.configure(url(&server));
        let view = harness.until("friends", |view| view.loaded).await;
        // The service has teams, but the signed config has not turned them on.
        assert!(!view.teams && view.crews.is_empty());
        assert_eq!(view.groups.as_ref().unwrap().version, 0);
        harness.service.set_teams_enabled(true);
        let view = harness.service.view();
        // A service with one crew per player: its crew is the list, and the limit is one.
        assert_eq!((view.crews.len(), view.crew_limit), (1, 1));
        let team = &view.crews[0];
        assert_eq!(team.role, crate::model::TeamRole::Founder);
        assert!(team.members[0].me);

        harness
            .service
            .invite_to_team("0123456789abcdef", "223456789012345678")
            .await
            .unwrap();
        assert_eq!(
            harness.service.view().crews[0].invited,
            vec!["223456789012345678".to_owned()]
        );
        // A crew the player is not in never reaches the service.
        assert!(matches!(
            harness
                .service
                .invite_to_team("fedcba9876543210", "223456789012345678")
                .await,
            Err(SocialError::InvalidInput("not in a team"))
        ));

        let okul = FriendGroup {
            id: "okul".into(),
            name: "  Okul  ".into(),
        };
        let member = GroupMember {
            friend: "223456789012345678".into(),
            group: "okul".into(),
        };
        harness
            .service
            .save_groups(vec![okul.clone()], vec![member.clone()])
            .await
            .unwrap();
        let groups = harness.service.view().groups.unwrap();
        assert_eq!(groups.version, 1);
        assert_eq!(groups.groups[0].name, "Okul");
        assert_eq!(groups.members, vec![member]);
        // A category that does not add up never leaves the launcher.
        let bad = FriendGroup {
            id: "Okul!".into(),
            name: "x".into(),
        };
        assert!(
            harness
                .service
                .save_groups(vec![bad], vec![])
                .await
                .is_err()
        );
        assert_eq!(puts.load(Ordering::SeqCst), 1);

        // A failed save shows what was there before it, including what another computer saved
        // since this one's last save.
        elsewhere.store(true, Ordering::SeqCst);
        harness.service.refresh().await;
        assert_eq!(harness.service.view().groups.unwrap().version, 5);
        failing.store(true, Ordering::SeqCst);
        assert!(
            harness
                .service
                .save_groups(vec![okul], vec![])
                .await
                .is_err()
        );
        let groups = harness.service.view().groups.unwrap();
        assert_eq!(groups.version, 5);
        assert_eq!(groups.groups[0].name, "PvP");
    }

    const TEAM_ANSWER: &str = r#"{"team":{"id":"0123456789abcdef","name":"Gece Kartalları",
        "tag":"GKR","emblem":"sword","code":"TKM-ABCD-EFGH","createdOn":"2026-10-19",
        "members":[{"id":"123456789012345678","username":"ayse","displayName":"Ayşe",
        "avatar":"discord","avatarHash":null,"role":"founder","joinedAt":5}]},"invited":[]}"#;

    #[tokio::test]
    async fn crew_looks_and_calls_wait_for_the_service_to_offer_them() {
        let offers = Arc::new(AtomicBool::new(false));
        let bodies = Arc::new(Mutex::new(Vec::<String>::new()));
        let server = MockServer::start({
            let offers = offers.clone();
            let bodies = bodies.clone();
            move |req| {
                presence_refused(&req.path).unwrap_or_else(|| match req.path.as_str() {
                    "/v1/me" => MockReply::json(200, ME),
                    "/v1/friends" if offers.load(Ordering::SeqCst) => MockReply::json(
                        200,
                        &FRIENDS_WITH_TEAM.replace(
                            r#""teamInvites":[]"#,
                            r#""teamInvites":[],"teamFeatures":["look","calls"]"#,
                        ),
                    ),
                    "/v1/friends" => MockReply::json(200, FRIENDS_WITH_TEAM),
                    "/v1/teams" | "/v1/teams/0123456789abcdef" => {
                        bodies.lock().unwrap().push(req.body_text().to_owned());
                        MockReply::json(200, TEAM_ANSWER)
                    }
                    _ => MockReply::json(404, r#"{"error":{"code":"not_found","message":"x"}}"#),
                })
            }
        })
        .await;
        let tokens = Arc::new(MemoryTokenStore::new());
        tokens.save(&token()).unwrap();
        let harness = Harness::new(tokens);
        harness.service.set_teams_enabled(true);
        harness.service.configure(url(&server));
        let view = harness.until("friends", |view| view.loaded).await;
        // The Worker before crews' looks and calls: today's look, no calls, nothing new sent.
        assert!(view.team_features.is_empty() && view.calls.is_empty());
        let team = view.crews[0].clone();
        assert_eq!((team.color.as_str(), team.frame.as_str()), ("gold", "cut"));
        assert_eq!(team.code, "EKP-ABCD-EFGH");
        let look = TeamLook {
            color: "teal".into(),
            frame: "banner".into(),
            motto: "Birlikte güçlüyüz!".into(),
        };
        // A newer emblem would be refused there: it never leaves the launcher.
        assert!(matches!(
            harness
                .service
                .create_team("Gece Kartalları", "GKR", "controller", Some(look.clone()))
                .await,
            Err(SocialError::InvalidInput("emblem"))
        ));
        harness
            .service
            .create_team("Gece Kartalları", "GKR", "sword", Some(look.clone()))
            .await
            .unwrap();
        // Only a look to change: nothing to send.
        harness
            .service
            .edit_team(
                "0123456789abcdef",
                TeamEdit {
                    color: Some("teal".into()),
                    ..TeamEdit::default()
                },
            )
            .await
            .unwrap();
        harness
            .service
            .edit_team(
                "0123456789abcdef",
                TeamEdit {
                    name: Some("Gece Kuşları".into()),
                    motto: Some("Merhaba".into()),
                    ..TeamEdit::default()
                },
            )
            .await
            .unwrap();
        assert!(matches!(
            harness.service.call_crew(vec![]).await,
            Err(SocialError::InvalidInput("crew calls"))
        ));
        assert_eq!(
            *bodies.lock().unwrap(),
            vec![
                r#"{"emblem":"sword","name":"Gece Kartalları","tag":"GKR"}"#.to_owned(),
                r#"{"name":"Gece Kuşları"}"#.to_owned(),
            ]
        );

        // The service announces them: they go out, and the view says so.
        offers.store(true, Ordering::SeqCst);
        harness.service.refresh().await;
        let view = harness.service.view();
        assert_eq!(
            view.team_features,
            vec!["look".to_owned(), "calls".to_owned()]
        );
        bodies.lock().unwrap().clear();
        harness
            .service
            .create_team("Gece Kartalları", "GKR", "controller", Some(look))
            .await
            .unwrap();
        harness
            .service
            .edit_team(
                "0123456789abcdef",
                TeamEdit {
                    color: Some("teal".into()),
                    motto: Some(String::new()),
                    ..TeamEdit::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(
            *bodies.lock().unwrap(),
            vec![
                r#"{"color":"teal","emblem":"controller","frame":"banner","motto":"Birlikte güçlüyüz!","name":"Gece Kartalları","tag":"GKR"}"#.to_owned(),
                r#"{"color":"teal","motto":""}"#.to_owned(),
            ]
        );
        // Calls are offered now. In the launcher the player is told why without asking the
        // service (CALL1), whatever service it is.
        assert_eq!(
            harness.service.call_crew(vec![]).await.unwrap(),
            CallOutcome::NeedsServer
        );
        // On Projects the call goes out, but the socket is down: the request fails quietly.
        harness.service.game_started(Some("play.projects.gg"));
        harness
            .service
            .game_line("Connecting to play.projects.gg, 25565");
        assert_eq!(harness.service.view().call_place, CallPlace::Ok);
        assert!(matches!(
            harness.service.call_crew(vec![]).await,
            Err(SocialError::Unavailable { .. })
        ));
    }

    #[tokio::test]
    async fn crew_mutes_wait_for_the_service_to_keep_them() {
        let offers = Arc::new(AtomicBool::new(false));
        let bodies = Arc::new(Mutex::new(Vec::<String>::new()));
        let server = MockServer::start({
            let offers = offers.clone();
            let bodies = bodies.clone();
            move |req| {
                presence_refused(&req.path).unwrap_or_else(|| match req.path.as_str() {
                    "/v1/me" => MockReply::json(200, ME),
                    "/v1/friends" if offers.load(Ordering::SeqCst) => MockReply::json(
                        200,
                        &FRIENDS_WITH_TEAM.replace(
                            r#""teamInvites":[]"#,
                            r#""teamInvites":[],"teamFeatures":["look","calls","roles","mute"],
                              "teamNotifications":{"calls":true,"muted":false,"muteUntil":null}"#,
                        ),
                    ),
                    "/v1/friends" => MockReply::json(200, FRIENDS_WITH_TEAM),
                    "/v1/teams/notifications" => {
                        bodies.lock().unwrap().push(req.body_text().to_owned());
                        // What the service keeps now: the change on top of the defaults.
                        let body = req.json();
                        let muted = body["muted"].as_bool().unwrap_or(false);
                        MockReply::json(
                            200,
                            &serde_json::json!({
                                "calls": body["calls"].as_bool().unwrap_or(true),
                                "muted": muted,
                                "muteUntil": if muted { body["muteUntil"].clone() } else {
                                    serde_json::Value::Null
                                },
                            })
                            .to_string(),
                        )
                    }
                    _ => MockReply::json(404, r#"{"error":{"code":"not_found","message":"x"}}"#),
                })
            }
        })
        .await;
        let tokens = Arc::new(MemoryTokenStore::new());
        tokens.save(&token()).unwrap();
        let harness = Harness::new(tokens);
        harness.service.set_teams_enabled(true);
        harness.service.configure(url(&server));
        let view = harness.until("friends", |view| view.loaded).await;
        // A service that keeps no settings: the launcher's own switches decide, nothing is sent.
        assert_eq!(view.crew_notifications, None);
        let mute = |until: Option<i64>| TeamNotificationsEdit {
            muted: Some(true),
            mute_until: until,
            ..TeamNotificationsEdit::default()
        };
        assert!(matches!(
            harness.service.set_crew_notifications(mute(None)).await,
            Err(SocialError::InvalidInput("crew mute"))
        ));

        offers.store(true, Ordering::SeqCst);
        harness.service.refresh().await;
        assert_eq!(
            harness.service.view().crew_notifications,
            Some(CrewNotificationsView {
                calls: true,
                muted: false,
                mute_until: None
            })
        );
        // "1 saat": out with its end, back in the view.
        let until = unix_now() + 3_600;
        harness
            .service
            .set_crew_notifications(mute(Some(until)))
            .await
            .unwrap();
        assert_eq!(
            harness.service.view().crew_notifications,
            Some(CrewNotificationsView {
                calls: true,
                muted: true,
                mute_until: Some(until)
            })
        );
        // What the service would refuse never leaves: an end without a mute, one in the past or
        // too far ahead; nothing to change, nothing sent.
        for refused in [
            TeamNotificationsEdit {
                mute_until: Some(until),
                ..TeamNotificationsEdit::default()
            },
            mute(Some(unix_now() - 1)),
            mute(Some(unix_now() + MAX_MUTE_SECS + 60)),
        ] {
            assert!(matches!(
                harness.service.set_crew_notifications(refused).await,
                Err(SocialError::InvalidInput("mute until"))
            ));
        }
        harness
            .service
            .set_crew_notifications(TeamNotificationsEdit::default())
            .await
            .unwrap();
        // "Ekip çağrıları" off, from this computer: it goes with the account now.
        harness
            .service
            .set_crew_notifications(TeamNotificationsEdit {
                calls: Some(false),
                ..TeamNotificationsEdit::default()
            })
            .await
            .unwrap();
        assert!(
            harness
                .service
                .view()
                .crew_notifications
                .is_some_and(|settings| !settings.calls && !settings.muted)
        );
        assert_eq!(
            *bodies.lock().unwrap(),
            vec![
                format!(r#"{{"muteUntil":{until},"muted":true}}"#),
                r#"{"calls":false}"#.to_owned(),
            ]
        );
        // Without crews (the signed config), none of it shows.
        harness.service.set_teams_enabled(false);
        assert_eq!(harness.service.view().crew_notifications, None);
    }

    const CREW_A: &str = r#"{"id":"0123456789abcdef","name":"Gece Kartalları","tag":"GKR",
        "emblem":"sword","code":"TKM-ABCD-EFGH","createdOn":"2026-10-19","members":[
        {"id":"123456789012345678","username":"ayse","displayName":"Ayşe","avatar":"discord",
        "avatarHash":null,"role":"founder","joinedAt":5}]}"#;
    const CREW_B: &str = r#"{"id":"fedcba9876543210","name":"PvP","tag":"PVP","emblem":"bolt",
        "code":"TKM-WXYZ-1234","createdOn":"2026-10-20","members":[
        {"id":"223456789012345678","username":"bora","displayName":"Bora","avatar":"preset:fox",
        "avatarHash":null,"role":"founder","joinedAt":4},
        {"id":"123456789012345678","username":"ayse","displayName":"Ayşe","avatar":"discord",
        "avatarHash":null,"role":"member","joinedAt":6}]}"#;

    /// A service with several crews: the player leads one and is in Bora's.
    fn friends_with_crews() -> String {
        format!(
            r#"{{"friends":[{{"id":"223456789012345678","username":"bora","displayName":"Bora",
            "avatar":"preset:fox","avatarHash":null,"since":"2026-10-01","lastSeen":"today"}}],
            "incoming":[],"outgoing":[],"blocked":[],
            "limits":{{"friends":200,"outgoingRequests":50,"teamMembers":30,"groups":20}},
            "groups":{{"version":0,"groups":[],"members":{{}}}},
            "team":{CREW_A},"teamInvited":[],
            "crews":[{{"team":{CREW_A},"invited":[],"notifications":{{"muted":false,"muteUntil":null}}}},
              {{"team":{CREW_B},"invited":[],"notifications":{{"muted":false,"muteUntil":null}}}}],
            "crewLimit":5,"teamInvites":[],
            "teamFeatures":["look","calls","roles","mute","multi","callServer"],
            "teamNotifications":{{"calls":true,"muted":false,"muteUntil":null}}}}"#
        )
    }

    #[tokio::test]
    async fn several_crews_found_order_and_mute_one_by_one() {
        let bodies = Arc::new(Mutex::new(Vec::<String>::new()));
        let server = MockServer::start({
            let bodies = bodies.clone();
            move |req| {
                presence_refused(&req.path).unwrap_or_else(|| {
                    match (req.method.as_str(), req.path.as_str()) {
                        ("GET", "/v1/me") => MockReply::json(200, ME),
                        ("GET", "/v1/friends") => MockReply::json(200, &friends_with_crews()),
                        ("POST", "/v1/teams") => {
                            bodies.lock().unwrap().push(req.body_text().to_owned());
                            MockReply::json(
                                201,
                                &format!(
                                    r#"{{"team":{},"invited":[]}}"#,
                                    CREW_A
                                        .replace("0123456789abcdef", "1111222233334444")
                                        .replace("Gece Kartalları", "Yeni")
                                ),
                            )
                        }
                        ("PUT", "/v1/teams/order") => {
                            bodies.lock().unwrap().push(req.body_text().to_owned());
                            if req.json()["order"][0] == "fedcba9876543210" {
                                MockReply::json(200, &req.body_text())
                            } else {
                                MockReply::json(
                                    409,
                                    r#"{"error":{"code":"crews_changed","message":"x"}}"#,
                                )
                            }
                        }
                        ("PATCH", "/v1/teams/fedcba9876543210/notifications") => {
                            bodies.lock().unwrap().push(req.body_text().to_owned());
                            MockReply::json(200, r#"{"muted":true,"muteUntil":null}"#)
                        }
                        _ => {
                            MockReply::json(404, r#"{"error":{"code":"not_found","message":"x"}}"#)
                        }
                    }
                })
            }
        })
        .await;
        let tokens = Arc::new(MemoryTokenStore::new());
        tokens.save(&token()).unwrap();
        let harness = Harness::new(tokens);
        harness.service.set_teams_enabled(true);
        harness.service.configure(url(&server));
        let view = harness.until("friends", |view| view.loaded).await;
        let ids = |view: &SocialView| {
            view.crews
                .iter()
                .map(|crew| crew.id.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&view), vec!["0123456789abcdef", "fedcba9876543210"]);
        assert_eq!(view.crew_limit, 5);
        assert_eq!(view.crews[1].role, crate::model::TeamRole::Member);
        assert_eq!(view.call_place, CallPlace::NeedsServer);

        // One more crew: asked for as one more, and it goes last.
        harness
            .service
            .create_team("Yeni", "YN", "sword", None)
            .await
            .unwrap();
        let three = vec!["0123456789abcdef", "fedcba9876543210", "1111222233334444"];
        assert_eq!(ids(&harness.service.view()), three);

        // Bora's crew first: shown at once and saved.
        let order: Vec<String> = ["fedcba9876543210", "0123456789abcdef", "1111222233334444"]
            .map(str::to_owned)
            .to_vec();
        harness.service.order_crews(order.clone()).await.unwrap();
        assert_eq!(ids(&harness.service.view()), order);
        // The same order again sends nothing; one that is not the player's crews never leaves.
        harness.service.order_crews(order.clone()).await.unwrap();
        assert!(matches!(
            harness
                .service
                .order_crews(vec!["0123456789abcdef".into()])
                .await,
            Err(SocialError::InvalidInput("crew order"))
        ));
        // The service's crews changed meanwhile: the order shown goes back, the list is read again.
        let back: Vec<String> = three.iter().map(|id| (*id).to_owned()).collect();
        assert_eq!(
            harness.service.order_crews(back).await.unwrap_err().code(),
            Some(&ErrorCode::CrewsChanged)
        );
        assert_eq!(ids(&harness.service.view()), order);
        harness
            .until("the crews read again", |view| view.crews.len() == 2)
            .await;

        // "Sessize al" for Bora's crew alone.
        harness
            .service
            .set_crew_mute(
                "fedcba9876543210",
                CrewMuteEdit {
                    muted: true,
                    mute_until: None,
                },
            )
            .await
            .unwrap();
        let view = harness.service.view();
        assert!(view.crews[1].notifications.muted && !view.crews[0].notifications.muted);
        assert!(matches!(
            harness
                .service
                .set_crew_mute(
                    "fedcba9876543210",
                    CrewMuteEdit {
                        muted: false,
                        mute_until: Some(unix_now() + 60),
                    },
                )
                .await,
            Err(SocialError::InvalidInput("mute until"))
        ));

        // CALL1: nothing goes out from the launcher, nor from another server whose address is
        // not shared; the view says which.
        assert_eq!(
            harness
                .service
                .call_crew(vec!["fedcba9876543210".into()])
                .await
                .unwrap(),
            CallOutcome::NeedsServer
        );
        harness.service.game_started(None);
        assert_eq!(harness.service.view().call_place, CallPlace::NeedsServer);
        harness
            .service
            .game_line("Connecting to mc.example.com, 25565");
        assert_eq!(harness.service.view().call_place, CallPlace::ServerHidden);
        assert_eq!(
            harness.service.call_crew(vec![]).await.unwrap(),
            CallOutcome::ServerHidden
        );
        // A crew that went away meanwhile calls nobody else.
        assert!(matches!(
            harness
                .service
                .call_crew(vec!["1111222233334444".into()])
                .await,
            Err(SocialError::InvalidInput("not in a team"))
        ));
        assert_eq!(
            *bodies.lock().unwrap(),
            vec![
                r#"{"emblem":"sword","multi":true,"name":"Yeni","tag":"YN"}"#.to_owned(),
                r#"{"order":["fedcba9876543210","0123456789abcdef","1111222233334444"]}"#
                    .to_owned(),
                r#"{"order":["0123456789abcdef","fedcba9876543210","1111222233334444"]}"#
                    .to_owned(),
                r#"{"muted":true}"#.to_owned(),
            ]
        );
    }

    /// A service with the crews' second wave: the player leads Gece Kartalları, which approves
    /// newcomers, has an invite link and a request from Can; the player's own request to PvP
    /// waits. `features` are the service's `teamFeatures`.
    fn friends_with_requests(features: &str) -> String {
        let crew = CREW_A.replace(
            r#""members":["#,
            r#""join":"approval","link":"ABCD-EFGH-JKMN","members":["#,
        );
        format!(
            r#"{{"friends":[],"incoming":[],"outgoing":[],"blocked":[],
            "limits":{{"friends":200,"outgoingRequests":50,"teamMembers":30,"groups":20}},
            "team":{crew},"teamInvited":[],
            "crews":[{{"team":{crew},"invited":[],"notifications":{{"muted":false,"muteUntil":null}},
              "requests":[{{"from":{{"id":"323456789012345678","username":"can","displayName":"Can",
                "avatar":"discord","avatarHash":null}},"sentAt":{sent}}}]}}],
            "crewLimit":5,"teamInvites":[],
            "teamRequests":[{{"team":{{"id":"fedcba9876543210","name":"PvP","tag":"PVP",
              "emblem":"bolt","members":4,"join":"approval"}},"sentAt":{sent}}}],
            "teamFeatures":{features}}}"#,
            sent = unix_now() - 60,
        )
    }

    #[tokio::test]
    async fn joining_by_approval_requests_links_and_the_join_setting() {
        let bodies = Arc::new(Mutex::new(Vec::<String>::new()));
        let server = MockServer::start({
            let bodies = bodies.clone();
            move |req| {
                presence_refused(&req.path).unwrap_or_else(|| {
                    if req.method != "GET" {
                        bodies
                            .lock()
                            .unwrap()
                            .push(format!("{} {} {}", req.method, req.path, req.body_text()));
                    }
                    match (req.method.as_str(), req.path.as_str()) {
                        ("GET", "/v1/me") => MockReply::json(200, ME),
                        ("GET", "/v1/friends") => MockReply::json(
                            200,
                            &friends_with_requests(r#"["look","roles","multi","requests","link"]"#),
                        ),
                        ("POST", "/v1/teams/lookup") => MockReply::json(
                            200,
                            r#"{"team":{"id":"9999aaaabbbbcccc","name":"Skyblock","tag":"SKY",
                                "emblem":"castle","members":6,"join":"approval"},"relation":"none",
                                "request":null}"#,
                        ),
                        ("POST", "/v1/teams/join") => MockReply::json(
                            202,
                            r#"{"request":{"team":{"id":"9999aaaabbbbcccc","name":"Skyblock",
                                "tag":"SKY","emblem":"castle","members":6,"join":"approval"},
                                "sentAt":1792519000}}"#,
                        ),
                        ("DELETE", "/v1/teams/requests/fedcba9876543210") => MockReply::empty(204),
                        ("POST", path) if path.ends_with("/accept") => MockReply::json(
                            200,
                            &format!(
                                r#"{{"team":{},"invited":[]}}"#,
                                CREW_A.replace(
                                    r#""members":["#,
                                    r#""members":[{"id":"323456789012345678","username":"can",
                                    "displayName":"Can","avatar":"discord","avatarHash":null,
                                    "role":"member","joinedAt":9},"#
                                )
                            ),
                        ),
                        ("POST", "/v1/teams/0123456789abcdef/link") => MockReply::json(
                            200,
                            r#"{"link":"WXYZ-1234-5678","url":"https://social.projects.gg/e/WXYZ-1234-5678"}"#,
                        ),
                        ("PATCH", "/v1/teams/0123456789abcdef") => {
                            MockReply::json(200, &format!(r#"{{"team":{CREW_A},"invited":[]}}"#))
                        }
                        _ => {
                            MockReply::json(404, r#"{"error":{"code":"not_found","message":"x"}}"#)
                        }
                    }
                })
            }
        })
        .await;
        let tokens = Arc::new(MemoryTokenStore::new());
        tokens.save(&token()).unwrap();
        let harness = Harness::new(tokens);
        harness.service.set_teams_enabled(true);
        harness.service.configure(url(&server));
        let view = harness.until("friends", |view| view.loaded).await;
        let crew = &view.crews[0];
        assert_eq!(crew.join, JoinMode::Approval);
        assert_eq!(crew.link.as_deref(), Some("ABCD-EFGH-JKMN"));
        // The page is this service's own.
        assert_eq!(
            crew.link_url.as_deref(),
            Some(format!("{}e/ABCD-EFGH-JKMN", server.base()).as_str())
        );
        assert_eq!(crew.requests[0].name, "Can");
        assert_eq!(view.team_requests[0].name, "PvP");

        // A link that leads to a crew that approves newcomers: asking sends a request.
        let found = harness
            .service
            .lookup_crew_link("abcd efgh jkmn")
            .await
            .unwrap();
        assert_eq!((found.join, found.code.as_str()), (JoinMode::Approval, ""));
        assert_eq!(found.link.as_deref(), Some("ABCD-EFGH-JKMN"));
        assert_eq!(
            harness
                .service
                .join_crew_link("ABCD-EFGH-JKMN")
                .await
                .unwrap(),
            JoinResult::Requested
        );
        let view = harness.service.view();
        assert_eq!(view.team_requests[0].name, "Skyblock");
        // Withdrawn; Can taken in (his row goes, he is in the crew).
        harness
            .service
            .withdraw_team_request("fedcba9876543210")
            .await
            .unwrap();
        assert_eq!(harness.service.view().team_requests.len(), 1);
        harness
            .service
            .answer_team_request("0123456789abcdef", "323456789012345678", true)
            .await
            .unwrap();
        let view = harness.service.view();
        assert!(view.crews[0].requests.is_empty());
        assert_eq!(view.crews[0].members.len(), 2);
        // A new link; the join setting.
        assert_eq!(
            harness
                .service
                .set_team_link("0123456789abcdef", true)
                .await
                .unwrap()
                .as_deref(),
            Some("WXYZ-1234-5678")
        );
        assert_eq!(
            harness.service.view().crews[0].link.as_deref(),
            Some("WXYZ-1234-5678")
        );
        harness
            .service
            .set_join_mode("0123456789abcdef", JoinMode::Open)
            .await
            .unwrap();
        let sent = bodies.lock().unwrap().clone();
        assert!(
            sent[0].starts_with("POST /v1/teams/lookup")
                && sent[0].contains(r#""link":"ABCD-EFGH-JKMN""#)
        );
        assert!(sent[1].contains(r#""request":true"#) && sent[1].contains(r#""multi":true"#));
        assert_eq!(sent[2], "DELETE /v1/teams/requests/fedcba9876543210 ");
        assert_eq!(
            sent.last().unwrap(),
            r#"PATCH /v1/teams/0123456789abcdef {"join":"open"}"#
        );
    }

    #[tokio::test]
    async fn an_older_service_never_hears_of_requests_links_or_plans() {
        let bodies = Arc::new(Mutex::new(Vec::<String>::new()));
        let server = MockServer::start({
            let bodies = bodies.clone();
            move |req| {
                presence_refused(&req.path).unwrap_or_else(|| {
                    if req.method != "GET" {
                        bodies.lock().unwrap().push(req.body_text());
                    }
                    match (req.method.as_str(), req.path.as_str()) {
                        ("GET", "/v1/me") => MockReply::json(200, ME),
                        ("GET", "/v1/friends") => MockReply::json(
                            200,
                            &friends_with_requests(r#"["look","roles","multi"]"#),
                        ),
                        ("POST", "/v1/teams/join") => {
                            MockReply::json(200, &format!(r#"{{"team":{CREW_B},"invited":[]}}"#))
                        }
                        ("PATCH", "/v1/teams/0123456789abcdef") => {
                            MockReply::json(200, &format!(r#"{{"team":{CREW_A},"invited":[]}}"#))
                        }
                        _ => {
                            MockReply::json(404, r#"{"error":{"code":"not_found","message":"x"}}"#)
                        }
                    }
                })
            }
        })
        .await;
        let tokens = Arc::new(MemoryTokenStore::new());
        tokens.save(&token()).unwrap();
        let harness = Harness::new(tokens);
        harness.service.set_teams_enabled(true);
        harness.service.configure(url(&server));
        let view = harness.until("friends", |view| view.loaded).await;
        // What an older service would never send stays out of the view.
        assert!(view.team_requests.is_empty() && view.plans.is_empty());
        assert_eq!(
            harness.service.join_team("EKP-ABCD-EFGH").await.unwrap(),
            JoinResult::Joined
        );
        assert!(matches!(
            harness.service.lookup_crew_link("ABCD-EFGH-JKMN").await,
            Err(SocialError::InvalidInput("crew links"))
        ));
        assert!(matches!(
            harness
                .service
                .set_join_mode("0123456789abcdef", JoinMode::Approval)
                .await,
            Err(SocialError::InvalidInput("join requests"))
        ));
        assert!(matches!(
            harness
                .service
                .make_plan("0123456789abcdef", 1_792_612_800)
                .await,
            Err(SocialError::InvalidInput("crew plans"))
        ));
        // `admit` is left out of a permissions change; nothing else was in it, so nothing goes.
        harness
            .service
            .edit_team(
                "0123456789abcdef",
                TeamEdit {
                    permissions: Some(crate::model::PermissionsEdit {
                        admit: Some(crate::model::PermissionLevel::Leader),
                        ..crate::model::PermissionsEdit::default()
                    }),
                    ..TeamEdit::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(
            *bodies.lock().unwrap(),
            vec![r#"{"code":"ABCD-EFGH","multi":true}"#.to_owned()]
        );
    }

    #[tokio::test]
    async fn the_time_played_together_switch_outlives_a_restart() {
        let server = MockServer::start(|req| {
            presence_refused(&req.path).unwrap_or_else(|| match req.path.as_str() {
                "/v1/me" => MockReply::json(200, ME),
                "/v1/friends" => MockReply::json(200, &friends_with_crews()),
                _ => MockReply::json(404, r#"{"error":{"code":"not_found","message":"x"}}"#),
            })
        })
        .await;
        let dir =
            std::env::temp_dir().join(format!("pl-social-together-svc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = dir.join("social").join("together.json");
        let tokens = Arc::new(MemoryTokenStore::new());
        tokens.save(&token()).unwrap();
        let harness = Harness::new(tokens.clone());
        harness.service.keep_together_in(&file);
        harness.service.set_teams_enabled(true);
        harness.service.configure(url(&server));
        let view = harness.until("friends", |view| view.loaded).await;
        assert!(view.together);
        assert_eq!(view.crews[0].together_week, 0);
        harness.service.set_together(false);
        assert!(!harness.service.view().together);
        // Another run on this computer keeps it off.
        let again = Harness::new(tokens);
        again.service.keep_together_in(&file);
        assert!(!again.service.view().together);
        again.service.set_together(true);
        assert!(again.service.view().together);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn an_expired_session_is_forgotten_quietly() {
        let server = MockServer::start(|req| {
            presence_refused(&req.path).unwrap_or_else(|| {
                MockReply::json(401, r#"{"error":{"code":"unauthorized","message":"x"}}"#)
            })
        })
        .await;
        let tokens = Arc::new(MemoryTokenStore::new());
        tokens.save(&token()).unwrap();
        let harness = Harness::new(tokens);
        harness.service.configure(url(&server));
        harness
            .until("signed out", |view| view.auth == AuthPhase::SignedOut)
            .await;
        assert!(harness.tokens.load().is_none());
        assert!(harness.notices().is_empty(), "no toast at start");
    }

    #[tokio::test]
    async fn an_outage_at_start_keeps_the_session() {
        let server = MockServer::start(|_| {
            MockReply::json(503, r#"{"error":{"code":"unavailable","message":"x"}}"#)
        })
        .await;
        let tokens = Arc::new(MemoryTokenStore::new());
        tokens.save(&token()).unwrap();
        let harness = Harness::new(tokens);
        harness.service.configure(url(&server));
        let view = harness
            .until("unavailable", |view| {
                view.connection == Connection::Unavailable
            })
            .await;
        assert_eq!(view.auth, AuthPhase::SignedIn);
        assert!(view.me.is_none());
        assert!(harness.tokens.load().is_some());
        // Signing out still works and forgets the token.
        harness.service.sign_out().await;
        assert_eq!(harness.service.view().auth, AuthPhase::SignedOut);
        assert!(harness.tokens.load().is_none());
    }

    #[tokio::test]
    async fn signs_in_with_the_link_of_its_own_attempt_only() {
        let claims = Arc::new(AtomicUsize::new(0));
        let server = MockServer::start({
            let claims = claims.clone();
            move |req| {
                presence_refused(&req.path).unwrap_or_else(|| match req.path.as_str() {
                    "/v1/health" => MockReply::json(200, r#"{"ok":true,"api":1}"#),
                    "/v1/auth/claim" => {
                        claims.fetch_add(1, Ordering::SeqCst);
                        assert_eq!(req.json()["device"], "Ev PC");
                        MockReply::json(
                            200,
                            &format!(r#"{{"token":"pls_{}","me":{ME}}}"#, "B".repeat(43)),
                        )
                    }
                    "/v1/friends" => MockReply::json(200, FRIENDS),
                    _ => MockReply::json(404, r#"{"error":{"code":"not_found","message":"x"}}"#),
                })
            }
        })
        .await;
        let harness = Harness::new(Arc::new(MemoryTokenStore::new()));
        harness.service.configure(url(&server));
        let link = format!("projects://auth/done?c={}", "C".repeat(43));
        // No sign-in waiting: the link is recognised but ignored.
        assert!(harness.service.complete_sign_in(&link));
        assert!(
            !harness
                .service
                .complete_sign_in("projects://friend/PRJ-ABCD-EFGH")
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(claims.load(Ordering::SeqCst), 0);

        let browser = harness.service.begin_sign_in().await.unwrap();
        assert!(browser.as_str().contains("/auth/discord/start?challenge="));
        assert_eq!(harness.service.view().auth, AuthPhase::SigningIn);
        assert!(harness.service.complete_sign_in(&link));
        let view = harness
            .until("signed in", |view| {
                view.loaded && view.auth == AuthPhase::SignedIn
            })
            .await;
        assert_eq!(view.me.unwrap().username, "ayse");
        assert_eq!(claims.load(Ordering::SeqCst), 1);
        assert_eq!(
            harness.tokens.load().unwrap().expose(),
            format!("pls_{}", "B".repeat(43))
        );
        // The backup came from another computer: worth offering.
        assert!(harness.notices().iter().any(|notice| matches!(
            notice,
            SocialNotice::SignedIn { restore: Some(meta) } if meta.device == "Laptop"
        )));
        // The same link again does nothing: the attempt is used up.
        assert!(harness.service.complete_sign_in(&link));
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(claims.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn a_used_up_sign_in_says_so() {
        let server = MockServer::start(|req| match req.path.as_str() {
            "/v1/health" => MockReply::json(200, r#"{"ok":true,"api":1}"#),
            _ => MockReply::json(
                404,
                r#"{"error":{"code":"auth_pending_not_found","message":"x"}}"#,
            ),
        })
        .await;
        let harness = Harness::new(Arc::new(MemoryTokenStore::new()));
        harness.service.configure(url(&server));
        harness.service.begin_sign_in().await.unwrap();
        harness
            .service
            .complete_sign_in(&format!("projects://auth/done?c={}", "C".repeat(43)));
        harness
            .until("signed out", |view| view.auth == AuthPhase::SignedOut)
            .await;
        let deadline = Instant::now() + Duration::from_secs(5);
        while !harness.notices().iter().any(|n| {
            matches!(
                n,
                SocialNotice::SignInFailed {
                    reason: SignInFailure::Expired
                }
            )
        }) {
            assert!(Instant::now() < deadline, "no failure notice");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    #[tokio::test]
    async fn no_browser_while_the_service_is_down() {
        let server = MockServer::start(|_| MockReply::json(503, "")).await;
        let harness = Harness::new(Arc::new(MemoryTokenStore::new()));
        harness.service.configure(url(&server));
        assert!(matches!(
            harness.service.begin_sign_in().await,
            Err(SocialError::Unavailable { .. })
        ));
        assert_eq!(harness.service.view().auth, AuthPhase::SignedOut);
        // Not configured: nothing to sign in to.
        harness.service.configure(None);
        assert!(harness.service.begin_sign_in().await.is_err());
    }

    #[tokio::test]
    async fn calls_update_the_view_and_401_signs_out() {
        let server = MockServer::start(|req| {
            presence_refused(&req.path).unwrap_or_else(|| {
                match (req.method.as_str(), req.path.as_str()) {
                    ("GET", "/v1/me") => MockReply::json(200, ME),
                    ("GET", "/v1/friends") => MockReply::json(200, FRIENDS),
                    ("POST", "/v1/friends/requests") => MockReply::json(
                        201,
                        r#"{"status":"requested","user":{"id":"323456789012345678","username":"can",
                        "displayName":null,"avatar":"discord","avatarHash":null},"sentAt":5}"#,
                    ),
                    ("PUT", "/v1/blocks/223456789012345678") => MockReply::json(204, ""),
                    ("POST", "/v1/me/friend-code") => {
                        MockReply::json(200, r#"{"friendCode":"PRJ-ZZZZ-YYYY"}"#)
                    }
                    _ => MockReply::json(401, r#"{"error":{"code":"unauthorized","message":"x"}}"#),
                }
            })
        })
        .await;
        let tokens = Arc::new(MemoryTokenStore::new());
        tokens.save(&token()).unwrap();
        let harness = Harness::new(tokens);
        harness.service.configure(url(&server));
        harness.until("friends", |view| view.loaded).await;

        assert_eq!(
            harness.service.send_request("prj-aaaa-bbbb").await.unwrap(),
            SendOutcome::Requested
        );
        assert_eq!(harness.service.view().outgoing[0].username, "can");

        harness.service.block("223456789012345678").await.unwrap();
        let view = harness.service.view();
        assert!(view.friends.is_empty());
        assert_eq!(view.blocked[0].name, "Bora");

        assert_eq!(
            harness.service.rotate_code().await.unwrap(),
            "PRJ-ZZZZ-YYYY"
        );
        let me = harness.service.view().me.unwrap();
        assert_eq!(me.friend_code, "PRJ-ZZZZ-YYYY");
        assert!(me.share_link.unwrap().ends_with("PRJ-ZZZZ-YYYY"));

        // Malformed codes never reach the service.
        assert!(matches!(
            harness.service.lookup("nope").await,
            Err(SocialError::InvalidInput(_))
        ));
        // The session ended elsewhere: signed out, token gone, the player is told.
        assert!(matches!(
            harness.service.remove_friend("323456789012345678").await,
            Err(SocialError::Unauthorized)
        ));
        assert_eq!(harness.service.view().auth, AuthPhase::SignedOut);
        assert!(harness.tokens.load().is_none());
        assert!(harness.notices().contains(&SocialNotice::SignedOutRemotely));
        // Signed out: calls fail fast without a request.
        assert!(matches!(
            harness.service.accept("323456789012345678").await,
            Err(SocialError::Unauthorized)
        ));
    }

    #[tokio::test]
    async fn deleting_the_account_here_is_not_a_sign_out_from_elsewhere() {
        let slot: Arc<Mutex<Option<SocialService>>> = Arc::new(Mutex::new(None));
        let server = MockServer::start({
            let slot = slot.clone();
            move |req| {
                presence_refused(&req.path).unwrap_or_else(|| {
                    match (req.method.as_str(), req.path.as_str()) {
                        ("GET", "/v1/me") => MockReply::json(200, ME),
                        ("GET", "/v1/friends") => MockReply::json(200, FRIENDS),
                        ("DELETE", "/v1/me") => {
                            // The service closes the account's sockets (4002) before it answers.
                            let service = slot.lock().unwrap().clone().unwrap();
                            let (generation, epoch) = {
                                let core = service.lock();
                                (core.generation, core.epoch)
                            };
                            service.on_presence(
                                generation,
                                epoch,
                                PresenceEvent::Stopped(StopReason::AccountDeleted),
                            );
                            MockReply::json(204, "")
                        }
                        _ => {
                            MockReply::json(404, r#"{"error":{"code":"not_found","message":"x"}}"#)
                        }
                    }
                })
            }
        })
        .await;
        let tokens = Arc::new(MemoryTokenStore::new());
        tokens.save(&token()).unwrap();
        let harness = Harness::new(tokens);
        *slot.lock().unwrap() = Some(harness.service.clone());
        harness.service.configure(url(&server));
        harness.until("friends", |view| view.loaded).await;

        harness.service.delete_account().await.unwrap();
        assert_eq!(harness.service.view().auth, AuthPhase::SignedOut);
        assert!(harness.tokens.load().is_none());
        assert!(
            !harness.notices().contains(&SocialNotice::SignedOutRemotely),
            "no \"signed out elsewhere\" right after deleting here"
        );
    }

    #[tokio::test]
    async fn settings_backup_wins_a_conflict_once() {
        let puts = Arc::new(AtomicUsize::new(0));
        let server = MockServer::start({
            let puts = puts.clone();
            move |req| {
                presence_refused(&req.path).unwrap_or_else(|| match (req.method.as_str(), req.path.as_str()) {
                    ("GET", "/v1/me") => MockReply::json(200, ME),
                    ("GET", "/v1/friends") => MockReply::json(200, FRIENDS),
                    ("PUT", "/v1/settings") => {
                        puts.fetch_add(1, Ordering::SeqCst);
                        if req.header("if-match") == Some("\"7\"") {
                            MockReply::json(
                                200,
                                r#"{"version":8,"updatedAt":1760000100,"device":"Ev PC","size":20}"#,
                            )
                            .with_header("ETag", "\"8\"")
                        } else {
                            // Version 3 is stale: another computer saved version 7.
                            MockReply::json(
                                412,
                                r#"{"error":{"code":"precondition_failed","message":"x",
                                    "current":{"version":7,"updatedAt":1,"device":"Laptop","size":20}}}"#,
                            )
                        }
                    }
                    _ => MockReply::json(404, r#"{"error":{"code":"not_found","message":"x"}}"#),
                })
            }
        })
        .await;
        let tokens = Arc::new(MemoryTokenStore::new());
        tokens.save(&token()).unwrap();
        let harness = Harness::new(tokens);
        harness.service.configure(url(&server));
        harness.until("friends", |view| view.loaded).await;
        let meta = harness
            .service
            .upload_settings(serde_json::json!({"prefs": {"sounds": false}}))
            .await
            .unwrap();
        assert_eq!(meta.version, 8);
        assert_eq!(puts.load(Ordering::SeqCst), 2);
        assert_eq!(
            harness.service.view().me.unwrap().backup.unwrap().device,
            "Ev PC"
        );
    }
}
