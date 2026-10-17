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

use std::future::Future;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tokio::runtime::Handle;
use url::Url;

use crate::auth::{LoginAttempt, SignedIn, auth_code_from_deep_link};
use crate::client::{Session, SocialClient, SocialConfig};
use crate::codes::normalize_friend_code;
use crate::error::{ErrorCode, Result, SocialError};
use crate::game::{self, GameActivity};
use crate::model::{
    ProfileUpdate, RequestOutcome, SessionInfo, SettingsMeta, SettingsPrecondition,
};
use crate::presence::{PresenceControl, PresenceEvent, PresenceOptions, StopReason};
use crate::protocol::LocalStatus;
use crate::qr::{QrPath, qr_path};
use crate::state::{Connection, SocialState};
use crate::store::TokenStore;
use crate::view::{
    ActivityView, AuthPhase, InviteResult, LookupView, ProfileView, SendOutcome, SignInFailure,
    SocialNotice, SocialView, me_view,
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
}

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
        let view = {
            let mut core = self.lock();
            core.seq += 1;
            build_view(&mut core)
        };
        (self.inner.sink)(SocialUpdate::View(Box::new(view)));
    }

    fn notify(&self, notice: SocialNotice) {
        (self.inner.sink)(SocialUpdate::Notice(notice));
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
                    core.auth = AuthPhase::SignedIn;
                    self.start_presence(&mut core);
                }
                self.emit_view();
                self.fetch_friends(generation).await;
            }
            // Expired or revoked while the launcher was closed: back to the sign-in screen.
            Err(SocialError::Unauthorized) => self.end_session(generation, true),
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
            session.connect_presence(self.inner.config.presence).split()
        };
        control.set_status(core.activity.clone());
        // Replacing the control closes the previous connection cleanly.
        core.presence = Some(control);
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
        let notices = {
            let mut core = self.lock();
            if core.generation != generation || core.epoch != epoch {
                return;
            }
            let notices = core.state.apply(&event, unix_now());
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
                PresenceEvent::Stopped(_) => core.presence = None,
                _ => {}
            }
            notices
        };
        if ended {
            self.end_session(generation, quietly);
            return;
        }
        self.emit_view();
        for notice in notices {
            self.notify(notice);
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
                }
                self.emit_view();
            }
            Err(SocialError::Unauthorized) => self.end_session(generation, false),
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
                }
                Err(SocialError::Unauthorized) => ended = true,
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
                }
                Some(Err(SocialError::Unauthorized)) => ended = true,
                Some(Err(error)) => tracing::info!(%error, "could not load the friend list"),
                None => {}
            }
        }
        if ended {
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
                    error if error.is_transient() => SignInFailure::Unavailable,
                    _ => SignInFailure::Failed,
                };
                {
                    let mut core = self.lock();
                    if core.generation == generation && core.auth == AuthPhase::SigningIn {
                        core.auth = AuthPhase::SignedOut;
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

    pub fn dismiss_invite(&self, id: &str) {
        let generation = self.lock().generation;
        self.update(generation, |core| core.state.dismiss_invite(id));
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
                });
                Ok(meta)
            }
            Err(SocialError::Unauthorized) => {
                self.end_session(generation, false);
                Err(SocialError::Unauthorized)
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
        }
        self.emit_view();
    }
}

/// Back to signed out, keeping the configuration and the saved token.
fn reset(core: &mut Core) {
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
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::auth::SessionToken;
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
                    SocialUpdate::View(_) => None,
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
        (path == "/v1/presence").then(|| {
            MockReply::json(503, r#"{"error":{"code":"unavailable","message":"x"}}"#)
                .with_header("Retry-After", "600")
        })
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
