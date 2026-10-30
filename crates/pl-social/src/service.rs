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

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tokio::runtime::Handle;
use url::Url;

use crate::auth::{LoginAttempt, SessionToken, SignedIn, auth_code_from_deep_link};
use crate::beta::{BetaAccess, BetaAnswer, BetaRecordFile, decide_join, decide_keep};
use crate::cards::StoredCard;
use crate::chat::{
    CARD_WINDOW_MS, Context, EDIT_WINDOW_MS, PAGE_SIZE, REACT_WINDOW_MS, REPORT_CONTEXT,
    RESEND_WINDOW_MS, ReactionStep, TOO_FAST_WAIT_MAX, Viewer, card_view, change_view, context_of,
    entry_view, failure_of, merge, message_view, new_nonce, prepare_plan_note, prepare_text,
    reaction_step,
};
use crate::client::{CrewKey, Session, SocialClient, SocialConfig};
use crate::closed::ClosedInvitesFile;
use crate::codes::{crew_code, normalize_crew_link, normalize_friend_code, normalize_team_code};
use crate::error::{ErrorCode, Result, SocialError};
use crate::game::{self, GameActivity};
use crate::history::{Change, Conversation, Entry, History, crew_conversation};
use crate::links::{find_links, first_link};
use crate::minecraft::{McSignal, McSink};
use crate::model::{
    ChatBody, ChatExtras, ChatMessage, ChatStatus, CrewMute, CrewMuteEdit, EditFeatures,
    FIRST_EMBLEMS, FriendGroup, FriendGroups, JoinMode, JoinOutcome, LimitedKey, LinkRule,
    MAX_FRIEND_GROUPS, Me, ProfileUpdate, ReportReason, RequestOutcome, SessionInfo, SettingsMeta,
    SettingsPrecondition, StatusMode, TEAM_FEATURE_BADGE_REQUESTS, TEAM_FEATURE_BADGES,
    TEAM_FEATURE_CALLS, TEAM_FEATURE_CREW_FRIEND_REQUESTS, TEAM_FEATURE_LINK, TEAM_FEATURE_LOOK,
    TEAM_FEATURE_MULTI, TEAM_FEATURE_MUTE, TEAM_FEATURE_PLAN_NOTES, TEAM_FEATURE_PLANS,
    TEAM_FEATURE_REQUESTS, TEAM_FEATURE_ROLES, TeamEdit, TeamLook, TeamLookup, TeamNotifications,
    TeamNotificationsEdit, TeamRole,
};
use crate::presence::{
    ChatSetup, PresenceControl, PresenceEvent, PresenceOptions, RelVersion, StopReason,
};
use crate::protocol::{CallAnswer, LocalStatus, PlanAnswer, is_projects_host};
use crate::qr::{QrPath, qr_path};
use crate::state::{Connection, SocialState};
use crate::stickers;
use crate::store::TokenStore;
use crate::together::{Periods, Together, TogetherLog};
use crate::view::{
    ActivityView, AuthPhase, BadgeRefusal, BadgeRequestResult, CallOutcome, CallPlace,
    ChatCardView, ChatChangeView, ChatExtrasView, ChatLinksView, ChatPage, ChatTypingView,
    ChatView, ConversationView, CrewNotificationsView, GroupMember, InviteResult, JoinResult,
    LastMessageView, LookupView, ProfileView, ReportTarget, RestrictionView, SendFailure,
    SendOutcome, SignInFailure, SocialNotice, SocialView, TagOffView, TeamLookupView, me_view,
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
    /// Someone writes in a conversation ("yazıyor…", CHAT2): shown for 6 seconds after the last.
    Typing(ChatTypingView),
    /// The leader hid the crew whose tag the player wore, so it came off (TEAM6): "Lider ekibi
    /// gizledi; etiketin çıkarıldı." Once, never for the player's own change.
    TagOff(TagOffView),
}

/// Chat events that came before the history could be opened (the profile was not known yet:
/// the service was down at start) wait at most this many.
const CHAT_BACKLOG_MAX: usize = 2_000;
/// The player's own "yazıyor…" goes at most this often per conversation (CHAT2).
const TYPING_EVERY: Duration = Duration::from_secs(4);
/// A read mark goes at most this often per conversation (CHAT2); the newest that waited goes
/// when the time is up.
const MARK_EVERY: Duration = Duration::from_secs(2);
/// A busy hub's wait for a card is honoured once, if it is at most this long (CHAT2).
const CARD_RETRY_MAX: Duration = Duration::from_secs(10);

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
    /// One link card is asked at a time (the service's rule per socket, CHAT2).
    cards_turn: tokio::sync::Mutex<()>,
    /// The player's profile changes go to the service one at a time, in the order they were made,
    /// so its answers come back in that order too (SOC-LAG).
    profile_turn: tokio::sync::Mutex<()>,
    /// World sessions (HOST1): who hears them, what the connection took.
    worlds: crate::world::WorldsHub,
    /// SKIN1's listener for `mc` messages and reconnects (`crate::minecraft`).
    mc: Mutex<Option<McSink>>,
}

/// When a conversation's read mark last went, and the newest one waiting for its time (CHAT2).
#[derive(Debug, Default)]
struct MarkPace {
    sent: Option<Instant>,
    waiting: Option<i64>,
}

/// What the hub said about a link, to keep or for this run only.
enum CardAnswer {
    Keep(StoredCard),
    Passing,
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
    /// SOC9: the Minecraft version the game plays (checked), sent with the status while playing.
    game_version: Option<String>,
    settings_version: Option<u64>,
    fetched_at: Option<Instant>,
    /// SCALE-SOC: the relations version of the profile and lists this launcher holds, shared with
    /// the presence task (every connect names it); None when it holds none it trusts.
    rel: RelVersion,
    /// Bumped by every event that changes the lists or the profile here: a read that started
    /// before one may not set the version (its answer may miss the change).
    rel_epoch: u64,
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
    /// When the player's own "yazıyor…" last went, per conversation (CHAT2).
    typing_sent: HashMap<String, Instant>,
    /// Read marks per conversation (CHAT2).
    marks: HashMap<String, MarkPace>,
    /// The player's reactions on their way, per message (`conv|id`; CHAT2): the service's answer
    /// stands once none is left.
    reacting: HashMap<String, u32>,
    /// Links whose cards were asked in this run (CHAT2).
    cards_asked: HashSet<String>,
    /// The account was suspended or banned in this run (shown on the sign-in screen).
    restriction: Option<RestrictionView>,
    /// The settings backup waits until then (Unix seconds; storage protection).
    backup_waits_until: Option<i64>,
    /// "Birlikte oynanan süre", kept on this computer (TEAM5). Survives sign-outs.
    together: TogetherLog,
    /// World sessions may run (signed config `features.worlds`, HOST1). Survives sign-outs.
    worlds_enabled: bool,
    /// The player's own profile changes on their way to the service, oldest first (SOC-LAG): the
    /// view shows them until the service answers.
    profile_pending: Vec<PendingProfile>,
    profile_seq: u64,
}

/// A change of the player's picture, status or privacy switches the service has not answered yet.
#[derive(Debug)]
struct PendingProfile {
    id: u64,
    /// The sign-in it belongs to: a change of an earlier one never shows.
    generation: u64,
    update: ProfileUpdate,
}

impl Core {
    /// SCALE-SOC: the profile or the lists changed here (an event, or the player's own change):
    /// the copy is no longer the one its relations version describes, so the next connect names
    /// none and a read already on its way may not set one.
    fn forget_rel(&mut self) {
        self.rel_epoch += 1;
        self.rel.set(None);
    }

    /// SCALE-SOC: keeps the relations version a read brought, unless the copy changed since the
    /// read started (`epoch`).
    fn keep_rel(&mut self, rel: Option<String>, epoch: u64) {
        self.rel.set(rel.filter(|_| self.rel_epoch == epoch));
    }

    /// The player's profile as shown: the service's, with their own changes on the way applied
    /// (SOC-LAG). The view and everything that follows the player's switches (sending
    /// "yazıyor…", showing "Görüldü", where a call may start) read this one.
    fn shown_me(&self) -> Option<Me> {
        let mut me = self.state.me()?.clone();
        for pending in self
            .profile_pending
            .iter()
            .filter(|pending| pending.generation == self.generation)
        {
            let update = &pending.update;
            if let Some(avatar) = &update.avatar {
                me.profile.avatar.clone_from(avatar);
                me.profile.avatar_hash = if avatar == "discord" {
                    me.discord_avatar_hash.clone()
                } else {
                    None
                };
            }
            if let Some(status) = update.status {
                me.status = status;
            }
            if let Some(share) = update.share_server {
                me.privacy.share_server = share;
            }
            if let Some(share) = update.share_last_seen {
                me.privacy.share_last_seen = share;
            }
            if let Some(share) = update.share_read {
                me.privacy.share_read = Some(share);
            }
            if let Some(share) = update.share_typing {
                me.privacy.share_typing = Some(share);
            }
        }
        Some(me)
    }

    /// A picture change of the player's is on its way (their crew pages show it already).
    fn picture_pending(&self) -> bool {
        self.profile_pending
            .iter()
            .any(|pending| pending.generation == self.generation && pending.update.avatar.is_some())
    }
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
                cards_turn: tokio::sync::Mutex::new(()),
                profile_turn: tokio::sync::Mutex::new(()),
                worlds: crate::world::WorldsHub::default(),
                mc: Mutex::new(None),
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
            let chat = core.chat_enabled.then(|| core.chat_setup.clone());
            session
                .connect_presence_with(
                    self.inner.config.presence,
                    chat,
                    core.worlds_enabled,
                    Some(core.rel.clone()),
                )
                .split()
        };
        control.set_status(core.activity.clone());
        control.set_game_version(core.game_version.clone());
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
        // Cloud AFK's notices (AFKPLUS) change nothing of the friends view: straight to the launcher.
        if let PresenceEvent::AfkNotice(notice) = &event {
            let current = {
                let core = self.lock();
                core.generation == generation && core.epoch == epoch
            };
            if current {
                self.notify(SocialNotice::AfkNotice {
                    notice: notice.clone(),
                });
            }
            return;
        }
        // World sessions (HOST1) go to the launcher's host as they come: no view changes. A drop
        // of this connection tells it too.
        let world = match &event {
            PresenceEvent::World(world) => Some(world.clone()),
            PresenceEvent::Disconnected { .. } | PresenceEvent::Stopped(_) => {
                Some(crate::world::WorldEvent::Down)
            }
            _ => None,
        };
        if let Some(world) = world {
            let current = {
                let core = self.lock();
                core.generation == generation && core.epoch == epoch
            };
            if current {
                self.inner.worlds.deliver(world);
            }
            if matches!(event, PresenceEvent::World(_)) {
                return;
            }
        }
        let mut refetch = false;
        let mut ended = false;
        let mut quietly = false;
        let mut restricted = None;
        let deliver;
        let typing;
        let tag_off;
        let mc;
        let notices = {
            let mut core = self.lock();
            if core.generation != generation || core.epoch != epoch {
                return;
            }
            mc = match &event {
                PresenceEvent::Mc(message) => Some(McSignal::Message(message.clone())),
                PresenceEvent::Connected { .. } => Some(McSignal::Connected),
                _ => None,
            };
            if matches!(event, PresenceEvent::GroupsChanged(_)) && core.groups_saving {
                // This device's own save is on its way and wins; its echo arrives after it.
                return;
            }
            let mut notices = core.state.apply(&event, unix_now());
            if changes_relations(&event) {
                core.forget_rel();
            }
            observe_together(&mut core);
            let teams = core.teams && core.state.teams_supported();
            let calls = teams && core.state.offers(TEAM_FEATURE_CALLS);
            let requests = teams && core.state.offers(TEAM_FEATURE_REQUESTS);
            let plans = teams && core.state.offers(TEAM_FEATURE_PLANS);
            // TEAM6: said only while crew tags show at all.
            let badges = teams && core.state.offers(TEAM_FEATURE_BADGES);
            tag_off = core.state.take_tag_off().filter(|_| badges);
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
                PresenceEvent::Connected { rel, .. } => {
                    // SCALE-SOC: the service names the relations version as it is now; the one
                    // this copy has means nothing the profile or the lists show changed meanwhile.
                    let same = rel.is_some() && *rel == core.rel.get();
                    refetch = core.state.me().is_none()
                        || !core.state.loaded()
                        || (!same
                            && core
                                .fetched_at
                                .is_none_or(|at| at.elapsed() >= REFETCH_AFTER));
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
            typing = chat.typing;
            notices.extend(chat.notices);
            notices
        };
        if let Some(signal) = mc {
            self.tell_mc(signal);
            // SKIN1's news change nothing of the friends view.
            if matches!(event, PresenceEvent::Mc(_)) {
                return;
            }
        }
        if let Some(until) = restricted {
            self.end_restricted(generation, until);
            return;
        }
        if ended {
            self.end_session(generation, quietly);
            return;
        }
        // "Yazıyor…" changes nothing else: no new view for it.
        if let Some(typing) = typing {
            (self.inner.sink)(SocialUpdate::Typing(typing));
            return;
        }
        self.emit_view();
        for notice in notices {
            self.notify(notice);
        }
        if let Some(tag_off) = tag_off {
            (self.inner.sink)(SocialUpdate::TagOff(tag_off));
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
        let epoch = self.lock().rel_epoch;
        match session.friends().await {
            Ok(mut overview) => {
                {
                    let mut core = self.lock();
                    if core.generation != generation {
                        return;
                    }
                    // SCALE-SOC: the profile read with the lists (a service with `rel=1`).
                    if let Some(me) = overview.me.take() {
                        core.settings_version = me.settings.as_ref().map(|s| s.version);
                        core.state.set_me(me);
                    }
                    let rel = overview.rel.take();
                    core.state.set_overview(overview);
                    core.fetched_at = Some(Instant::now());
                    core.keep_rel(rel, epoch);
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
        let (session, epoch) = {
            let mut core = self.lock();
            if core.generation != generation || core.fetching {
                return;
            }
            let Some(session) = core.session.clone() else {
                return;
            };
            core.fetching = true;
            (session, core.rel_epoch)
        };
        // SCALE-SOC: the lists first, with the profile and the relations version (`rel=1`): one
        // request. An older service brings no profile with them: then `GET /v1/me` as before;
        // and when the lists fail, the profile still says whether the session holds.
        let (me, overview) = match session.friends().await {
            Ok(mut list) => match list.me.take() {
                Some(me) => (Ok(me), Some(Ok(list))),
                None => (session.me().await, Some(Ok(list))),
            },
            Err(error) => match session.me().await {
                Ok(me) => (Ok(me), Some(Err(error))),
                Err(me_error) => (Err(me_error), None),
            },
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
                Some(Ok(mut overview)) => {
                    let rel = overview.rel.take();
                    core.state.set_overview(overview);
                    core.fetched_at = Some(Instant::now());
                    core.keep_rel(rel, epoch);
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

    /// The session's token for the launcher's other services that take the friends session as
    /// their sign-in (7/24 Bulut): while signed in, or while a saved session is being restored.
    pub fn session_token(&self) -> Option<SessionToken> {
        let core = self.lock();
        match (&core.session, core.auth) {
            (Some(session), AuthPhase::SignedIn | AuthPhase::Restoring) => {
                Some(session.token().clone())
            }
            _ => None,
        }
    }

    /// The feature is on and nobody is signed in (never, or signed out): what those other services
    /// kept for the player goes.
    pub fn is_signed_out(&self) -> bool {
        let core = self.lock();
        core.url.is_some() && core.auth == AuthPhase::SignedOut
    }

    // --- beta access (BETA-GATE, crate::beta) ----------------------------------------------

    /// What the friends service says about the beta update channel for whoever is signed in here
    /// (or nobody). Never fails: what cannot be asked (the feature off, the service away) is
    /// "cannot tell".
    pub async fn beta_answer(&self) -> BetaAnswer {
        let (client, token) = {
            let core = self.lock();
            (core.client.clone(), {
                match (&core.session, core.auth) {
                    (Some(session), AuthPhase::SignedIn | AuthPhase::Restoring) => {
                        Some(session.token().clone())
                    }
                    _ => None,
                }
            })
        };
        let Some(client) = client else {
            return BetaAnswer::unknown();
        };
        match client.beta(token.as_ref()).await {
            Ok(answer) => answer,
            Err(error) => {
                tracing::info!(%error, "beta access could not be asked");
                BetaAnswer::unknown()
            }
        }
    }

    /// The player picked "Beta": let in, or why not. A yes is kept in `record`.
    pub async fn beta_join(&self, record: &BetaRecordFile) -> BetaAccess {
        let (access, let_in) = decide_join(&self.beta_answer().await, unix_now());
        if let_in.is_some()
            && let Err(error) = record.save(let_in)
        {
            tracing::warn!(%error, "could not keep when beta let this launcher in");
        }
        access
    }

    /// Before an update look on the beta channel: stay, or go back to the stable one (and why).
    pub async fn beta_keep(&self, record: &BetaRecordFile) -> BetaAccess {
        let (access, let_in) = decide_keep(&self.beta_answer().await, record.load(), unix_now());
        if let Err(error) = record.save(let_in) {
            tracing::warn!(%error, "could not keep when beta let this launcher in");
        }
        access
    }

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

    /// The player's own change, applied here once the service took it. SCALE-SOC: the copy no
    /// longer matches its relations version (a later change elsewhere could bring the service
    /// back to the version while this copy shows the change).
    fn update(&self, generation: u64, change: impl FnOnce(&mut Core)) {
        {
            let mut core = self.lock();
            if core.generation != generation {
                return;
            }
            change(&mut core);
            core.forget_rel();
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

    /// "Arkadaş ekle" on a crewmate who is not a friend (FIX2; a service with
    /// `crewFriendRequests`): a request without their code, answered as one by code would be. A
    /// crewmate the service no longer lets the player ask (they left, blocked the player, or were
    /// suspended) is `InvalidInput("not a crewmate")`.
    pub async fn send_request_to_crewmate(&self, user: &str) -> Result<SendOutcome> {
        if !self.offers(TEAM_FEATURE_CREW_FRIEND_REQUESTS) {
            return Err(SocialError::InvalidInput("crewmate requests"));
        }
        let crewmate = self.lock().state.crews().iter().any(|entry| {
            entry
                .team
                .members
                .iter()
                .any(|member| member.profile.id == user)
        });
        if !crewmate {
            return Err(SocialError::InvalidInput("not a crewmate"));
        }
        let to = user.to_owned();
        let (outcome, generation) = match self
            .call(|session| async move { session.send_request_to(&to).await })
            .await
        {
            Ok(answer) => answer,
            Err(error) if error.code() == Some(&ErrorCode::UserNotFound) => {
                return Err(SocialError::InvalidInput("not a crewmate"));
            }
            Err(error) => return Err(error),
        };
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

    /// Picture, status mode or privacy switches (CHAT2: "Görüldü bilgisi", "Yazıyor bilgisi").
    ///
    /// Shown at once (SOC-LAG): the view takes the change before the service answers (it used to
    /// wait for the answer). Changes go one at a time in the order they were made; the service's
    /// answer replaces what was shown, and a refusal takes the change back (the error says why).
    pub async fn update_profile(&self, update: ProfileUpdate) -> Result<()> {
        let id = {
            let mut core = self.lock();
            if core.auth != AuthPhase::SignedIn || core.session.is_none() {
                return Err(SocialError::Unauthorized);
            }
            core.profile_seq += 1;
            let id = core.profile_seq;
            let generation = core.generation;
            core.profile_pending.push(PendingProfile {
                id,
                generation,
                update: update.clone(),
            });
            id
        };
        self.emit_view();
        let turn = self.inner.profile_turn.lock().await;
        // Rapid clicks: a newer change of the same switches, still waiting, carries the player's
        // latest choice, so this one need not go (a switch clicked five times saves at most twice).
        if superseded(&self.lock().profile_pending, id) {
            self.lock()
                .profile_pending
                .retain(|pending| pending.id != id);
            drop(turn);
            self.emit_view();
            return Ok(());
        }
        let result = self
            .call(|session| async move { session.update_profile(&update).await })
            .await;
        {
            let mut core = self.lock();
            core.profile_pending.retain(|pending| pending.id != id);
            if let Ok((me, answered)) = &result
                && *answered == core.generation
            {
                core.state.set_me(me.clone());
                core.forget_rel();
            }
        }
        drop(turn);
        self.emit_view();
        result.map(|_| ())
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
            core.forget_rel();
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
            badges: self.offers(TEAM_FEATURE_BADGES),
            badge_requests: self.offers(TEAM_FEATURE_BADGE_REQUESTS),
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
        self.update(generation, |core| core.state.team_left(&id, true));
        Ok(())
    }

    /// Leaves one of the player's crews (a founder with members hands the role on first).
    pub async fn leave_team(&self, team: &str) -> Result<()> {
        let team = self.crew_id(team)?;
        let id = team.clone();
        let (_, generation) = self
            .call(|session| async move { session.leave_team(&team).await })
            .await?;
        self.update(generation, |core| core.state.team_left(&id, false));
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
            core.forget_rel();
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

    // --- crew tags (TEAM6) ----------------------------------------------------------------------

    /// Puts on the tag of one of the player's crews (`team`), in place of any other, or takes the
    /// tag off (None; a service with `badges`). The service's `me` brings it: the profile and the
    /// player's own entries in the crews change then, other devices and friends hear it live.
    pub async fn set_badge(&self, team: Option<&str>) -> Result<()> {
        if !self.offers(TEAM_FEATURE_BADGES) {
            return Err(SocialError::InvalidInput("crew tags"));
        }
        let team = team.map(|team| self.crew_id(team)).transpose()?;
        self.update_profile(ProfileUpdate::default().badge(team.as_deref()))
            .await
    }

    /// "Görünürlük": hides one of the player's crews, or shows it again (the leader; a service
    /// with `badges`). Hiding takes every member's tag of it off; showing puts none back.
    pub async fn set_team_hidden(&self, team: &str, hidden: bool) -> Result<()> {
        if !self.offers(TEAM_FEATURE_BADGES) {
            return Err(SocialError::InvalidInput("crew tags"));
        }
        self.edit_team(
            team,
            TeamEdit {
                hidden: Some(hidden),
                ..TeamEdit::default()
            },
        )
        .await
    }

    /// "Etiketten istek": whether members' friends may ask to join one of the player's crews from
    /// its tag's card (the leader; a service with `badgeRequests`). Requests already waiting stay
    /// answerable.
    pub async fn set_team_badge_requests(&self, team: &str, on: bool) -> Result<()> {
        if !self.offers(TEAM_FEATURE_BADGE_REQUESTS) {
            return Err(SocialError::InvalidInput("tag requests"));
        }
        self.edit_team(
            team,
            TeamEdit {
                badge_requests: Some(on),
                ..TeamEdit::default()
            },
        )
        .await
    }

    /// "Katılma isteği gönder" on the card of the tag `via` wears (a service with
    /// `badgeRequests`): a join request to crew `team` that waits for its leaders, even in an open
    /// crew; it shows among the player's requests at once. The crew's refusals and the player's
    /// own limits are answers (the card says why in one line), anything else an error.
    pub async fn request_from_badge(&self, team: &str, via: &str) -> Result<BadgeRequestResult> {
        if !self.offers(TEAM_FEATURE_BADGE_REQUESTS) {
            return Err(SocialError::InvalidInput("tag requests"));
        }
        let (crew, wearer) = (team.to_owned(), via.to_owned());
        match self
            .call(|session| async move { session.request_from_badge(&crew, &wearer).await })
            .await
        {
            Ok((request, generation)) => {
                self.update(generation, |core| core.state.join_requested(request));
                Ok(BadgeRequestResult::Sent)
            }
            Err(error) => {
                let now = unix_now();
                match badge_refusal(&error, now) {
                    Some(reason) => Ok(BadgeRequestResult::Refused {
                        reason,
                        until: turkey_day_end(now),
                    }),
                    None => Err(error),
                }
            }
        }
    }

    /// "Selam ver" on the line of someone who came into crew `crew` (TEAM6): the `pl1/hi` sticker
    /// where stickers go, else `text` ("Hoş geldin! 👋" in the player's language), as a message of
    /// the crew's chat like any other. Once per line, within 10 minutes. Returns the message's key.
    pub fn crew_greet(&self, crew: &str, id: &str, text: &str) -> Result<String> {
        if !self.lock().state.may_greet(crew, id, unix_now()) {
            return Err(SocialError::InvalidInput("greeting"));
        }
        let conv = crew_conversation(crew);
        let key = match self.chat_sticker(&conv, GREETING_STICKER, None) {
            Ok(key) => key,
            // No stickers on this connection, or not that set: the words.
            Err(SocialError::InvalidInput("chat extras" | "chat sticker")) => {
                self.chat_send(&conv, text)?
            }
            Err(error) => return Err(error),
        };
        let generation = self.lock().generation;
        self.update(generation, |core| core.state.greeted(crew, id));
        Ok(key)
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
    /// hour; the service checks it is later than its clock and at most 7 days ahead), with its
    /// short note (FIX2; only a service with `planNotes` takes one). Answers the plan's id; the
    /// plan is in the view.
    pub async fn make_plan(&self, crew: &str, at: i64, note: Option<&str>) -> Result<String> {
        let control = self.plan_presence()?;
        let crew = self.crew_id(crew)?;
        if at % 900 != 0 {
            return Err(SocialError::InvalidInput("plan time"));
        }
        let note = match note {
            Some(text) => prepare_plan_note(text)?,
            None => None,
        }
        .filter(|_| self.offers(TEAM_FEATURE_PLAN_NOTES));
        let generation = self.lock().generation;
        match control.make_plan(&crew, at, note.as_deref()).await {
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

    /// A plan's note (FIX2): its maker writes, changes or clears it (`""`); the crew's leader and
    /// moderators may only clear it. The plan as the service has it now replaces the one shown;
    /// one that is gone leaves the view.
    pub async fn set_plan_note(&self, plan: &str, note: &str) -> Result<()> {
        let control = self.plan_presence()?;
        if !self.offers(TEAM_FEATURE_PLAN_NOTES) {
            return Err(SocialError::InvalidInput("plan notes"));
        }
        let note = prepare_plan_note(note)?.unwrap_or_default();
        let (crew, generation) = {
            let core = self.lock();
            let crew = core
                .state
                .plan(plan)
                .map(|known| known.crew.clone())
                .ok_or(SocialError::InvalidInput("plan gone"))?;
            (crew, core.generation)
        };
        match control.set_plan_note(&crew, plan, &note).await {
            Ok(stored) => {
                self.update(generation, |core| core.state.upsert_plan(stored));
                Ok(())
            }
            Err(error) if error.code() == Some(&ErrorCode::PlanNotFound) => {
                self.update(generation, |core| core.state.plan_gone(plan));
                Err(error)
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
    /// "Sessize al" until a time or until unmuted, "Sesi aç", crew calls on or off. Shown at once
    /// (SOC-LAG); the answer settles it, a refusal puts the old settings back. The player's other
    /// devices hear it through the socket.
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
        let (before, shown, generation) = {
            let mut core = self.lock();
            let before = core.state.team_notifications().unwrap_or_default();
            let shown = notifications_after(before, &edit);
            core.state.set_team_notifications(shown);
            core.forget_rel();
            (before, shown, core.generation)
        };
        self.emit_view();
        match self
            .call(|session| async move { session.set_team_notifications(&edit).await })
            .await
        {
            Ok((settings, answered)) => {
                self.update(answered, |core| {
                    core.state.set_team_notifications(settings);
                });
                Ok(())
            }
            Err(error) => {
                // Back as it was, unless something newer came meanwhile (another device).
                self.update(generation, |core| {
                    if core.state.team_notifications() == Some(shown) {
                        core.state.set_team_notifications(before);
                    }
                });
                Err(error)
            }
        }
    }

    /// "Sessize al" for one of the player's crews (until a time, or until unmuted) or "Sesi aç",
    /// on a service with several crews. Shown at once (SOC-LAG); the answer settles it, a refusal
    /// puts the old mute back. The player's other devices hear it through the socket.
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
        let shown = CrewMute {
            muted: edit.muted,
            mute_until: edit.mute_until.filter(|_| edit.muted),
        };
        let (before, generation) = {
            let mut core = self.lock();
            let before = core.state.crew(&id).map(|entry| entry.notifications);
            core.state.set_crew_mute(&id, shown);
            core.forget_rel();
            (before, core.generation)
        };
        self.emit_view();
        match self
            .call(|session| async move { session.set_crew_mute(&team, &edit).await })
            .await
        {
            Ok((mute, answered)) => {
                self.update(answered, |core| core.state.set_crew_mute(&id, mute));
                Ok(())
            }
            Err(error) => {
                // Back as it was, unless something newer came meanwhile (another device).
                self.update(generation, |core| {
                    if let Some(before) = before
                        && core.state.crew(&id).map(|entry| entry.notifications) == Some(shown)
                    {
                        core.state.set_crew_mute(&id, before);
                    }
                });
                Err(error)
            }
        }
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
            let share = core.shown_me().is_some_and(|me| me.privacy.share_server);
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
            Err(SocialError::RateLimited { retry_after, .. }) => Ok(CallOutcome::Wait {
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

    // --- chat (CHAT1, CHAT2) ------------------------------------------------------------------

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

    /// World sessions may run (HOST1: signed config `features.worlds`; friends must be on too).
    /// The presence connection asks for them only while they may: a change reconnects it.
    pub fn set_worlds_enabled(&self, on: bool) {
        let mut core = self.lock();
        if core.worlds_enabled == on {
            return;
        }
        core.worlds_enabled = on;
        if core.auth == AuthPhase::SignedIn && core.presence.is_some() {
            self.start_presence(&mut core);
        }
    }

    /// Hears every world session frame of the presence connection, whether a connection took
    /// them (after each connect) and its drops (HOST1, `crate::world`). Called from the
    /// connection's task: it must not block.
    pub fn on_world(&self, sink: impl Fn(crate::world::WorldEvent) + Send + Sync + 'static) {
        self.inner.worlds.set_sink(Arc::new(sink));
    }

    /// World sessions run now: signed in, connected, and the service took them on this
    /// connection, with Studio's switch and guest limit. None otherwise.
    pub fn worlds(&self) -> Option<crate::world::WorldsStatus> {
        let live = {
            let core = self.lock();
            core.worlds_enabled && core.auth == AuthPhase::SignedIn && core.presence.is_some()
        };
        live.then(|| self.inner.worlds.status()).flatten()
    }

    /// A world frame the service answers (HOST1): its `ack`, or why not, within `wait`.
    pub async fn world_request(
        &self,
        frame: crate::world::WorldFrame,
        wait: Duration,
    ) -> std::result::Result<crate::world::WorldAnswer, crate::world::WorldError> {
        let control = self
            .lock()
            .presence
            .clone()
            .ok_or(crate::world::WorldError::Unavailable)?;
        control.world_request(frame, wait).await
    }

    /// A world frame nobody answers (HOST1); dropped without a connection.
    pub fn world_send(&self, frame: crate::world::WorldFrame) {
        let control = self.lock().presence.clone();
        if let Some(control) = control {
            control.world_send(frame);
        }
    }

    /// What the view's conversation `conv` may do now: chat on, not muted, still friends or still
    /// in the crew.
    fn chat_writable(&self, conv: &str) -> Result<()> {
        let core = self.lock();
        if core.auth != AuthPhase::SignedIn {
            return Err(SocialError::Unauthorized);
        }
        let status = match &core.chat_status {
            Some(status) if core.chat_enabled => status,
            _ => return Err(SocialError::InvalidInput("chat off")),
        };
        if !status.on {
            return Err(SocialError::InvalidInput("chat off"));
        }
        if muted(status, core.chat_skew_ms) {
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
        let Some(history) = core.history.as_mut() else {
            return ChatPage {
                conv: conv.to_owned(),
                messages: Vec::new(),
                more: false,
            };
        };
        let (entries, more) = history.page(conv, before, PAGE_SIZE);
        let messages = entries
            .iter()
            .map(|entry| {
                let context = context_of(history, conv, entry);
                entry_view(entry, &viewer, context)
            })
            .collect();
        ChatPage {
            conv: conv.to_owned(),
            messages,
            more,
        }
    }

    /// Sends a message: it shows at once (on its way) and goes when the connection lets it, in
    /// the order typed. Returns its key. Refused at once when chat is off, the player is muted,
    /// the conversation is not theirs to write in, or the text is empty or too long.
    pub fn chat_send(&self, conv: &str, text: &str) -> Result<String> {
        self.chat_writable(conv)?;
        let text = prepare_text(text).ok_or(SocialError::InvalidInput("chat text"))?;
        self.post(conv, ChatBody::Text(text), None)
    }

    /// Sends a message as a reply to message `reply` of the same conversation (CHAT2), held here.
    /// Like [`SocialService::chat_send`]; refused while replies are not on.
    pub fn chat_reply(&self, conv: &str, text: &str, reply: i64) -> Result<String> {
        self.chat_writable(conv)?;
        if !self.chat_offers(|extras| extras.replies) {
            return Err(SocialError::InvalidInput("chat extras"));
        }
        let text = prepare_text(text).ok_or(SocialError::InvalidInput("chat text"))?;
        self.post(conv, ChatBody::Text(text), Some(reply))
    }

    /// Sends a Projects sticker (`pl1/gg`, CHAT2), maybe as a reply: of a set the service takes
    /// and this launcher has. Like [`SocialService::chat_send`].
    pub fn chat_sticker(&self, conv: &str, sticker: &str, reply: Option<i64>) -> Result<String> {
        self.chat_writable(conv)?;
        let taken = {
            let core = self.lock();
            let set = sticker.split_once('/').map_or("", |(set, _)| set);
            core.chat_status
                .as_ref()
                .is_some_and(|status| status.stickers.iter().any(|s| s == set))
        };
        if !self.chat_offers(|extras| extras.stickers)
            || (reply.is_some() && !self.chat_offers(|extras| extras.replies))
        {
            return Err(SocialError::InvalidInput("chat extras"));
        }
        if !taken || stickers::sticker(sticker).is_none() {
            return Err(SocialError::InvalidInput("chat sticker"));
        }
        self.post(conv, ChatBody::Sticker(sticker.to_owned()), reply)
    }

    /// A message of the player's waits to go (shown at once); `reply` must be a message held here.
    fn post(&self, conv: &str, body: ChatBody, reply: Option<i64>) -> Result<String> {
        let nonce = new_nonce();
        {
            let mut core = self.lock();
            let history = core
                .history
                .as_mut()
                .ok_or(SocialError::InvalidInput("chat not allowed"))?;
            if let Some(id) = reply
                && history.message(conv, id).is_none()
            {
                return Err(SocialError::InvalidInput("chat message"));
            }
            let change = history
                .add_outgoing(conv, &body, reply, &nonce, unix_now_ms())
                .ok_or(SocialError::InvalidInput("chat not allowed"))?;
            push_change(&mut core, &change);
        }
        self.emit_view();
        self.kick_sender();
        Ok(format!("n:{nonce}"))
    }

    /// Whether chat offers this extra now (CHAT2): the socket took it and Studio has it on.
    fn chat_offers(&self, extra: impl Fn(&ChatExtras) -> bool) -> bool {
        offered(&self.lock()).is_some_and(|extras| extra(&extras))
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
            // A manual retry starts the clock again: it may go by itself after a reconnect too.
            let at = unix_now_ms();
            let change = core.history.as_mut().and_then(|history| {
                history.restart_outgoing(nonce, at);
                history.set_outgoing(nonce, None, None)
            });
            if let Some(change) = change {
                push_change(&mut core, &change);
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
            push_change(&mut core, &change);
        }
        self.emit_view();
    }

    /// Edits the player's own message (within 15 minutes; never a sticker). The new version
    /// reaches the view.
    pub async fn chat_edit(&self, conv: &str, id: i64, text: &str) -> Result<()> {
        self.chat_writable(conv)?;
        let text = prepare_text(text).ok_or(SocialError::InvalidInput("chat text"))?;
        let (control, message, generation) = {
            let mut core = self.lock();
            let me = my_id(&core);
            let skew = core.chat_skew_ms;
            let message = core
                .history
                .as_mut()
                .and_then(|history| history.message(conv, id))
                .ok_or(SocialError::InvalidInput("chat message"))?;
            if message.from != me || message.sticker.is_some() {
                return Err(SocialError::InvalidInput("chat message"));
            }
            if message.id + skew + EDIT_WINDOW_MS <= unix_now_ms() {
                return Err(SocialError::Refused {
                    status: 409,
                    code: ErrorCode::ChatTooOld,
                    message: String::new(),
                    detail: None,
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
                    // Deleted meanwhile (on another device, or by a moderator): its event says
                    // who, when it comes.
                    self.chat_tombstone(generation, conv, id, "");
                }
                return Err(error);
            }
        };
        self.chat_store(generation, &stored);
        Ok(())
    }

    /// "Herkesten sil": the player's own message (7 days), or any in a crew they lead or
    /// moderate. It turns into its tombstone once the service took it (the launcher shows that
    /// at once; it comes back if the service refuses).
    pub async fn chat_delete(&self, conv: &str, id: i64) -> Result<()> {
        let (control, message, generation, me) = {
            let mut core = self.lock();
            if core.auth != AuthPhase::SignedIn {
                return Err(SocialError::Unauthorized);
            }
            let me = my_id(&core);
            let message = core
                .history
                .as_mut()
                .and_then(|history| history.message(conv, id))
                .ok_or(SocialError::InvalidInput("chat message"))?;
            let control = core
                .presence
                .clone()
                .ok_or(SocialError::Unavailable { retry_after: None })?;
            (control, message, core.generation, me)
        };
        let by = match control.chat_delete(&message).await {
            Ok(()) => me,
            // Gone already (another device, a moderator): its event says who, when it comes.
            Err(error) if error.code() == Some(&ErrorCode::MessageNotFound) => String::new(),
            Err(error) => return Err(error),
        };
        self.chat_tombstone(generation, conv, id, &by);
        Ok(())
    }

    /// The player saw Cloud AFK's notice `id` (AFKPLUS): the launcher's window was in front when it
    /// showed, or the in-game overlay showed it. The friends service then keeps the phone quiet.
    pub fn afk_seen(&self, id: &str) {
        if id.is_empty() || id.len() > 64 {
            return;
        }
        if let Some(presence) = self.lock().presence.as_ref() {
            presence.afk_seen(id);
        }
    }

    /// Everything in a conversation is read now: it is on screen at its newest message, someone
    /// else's newest message there for a second with the window in front. With read marks on
    /// (CHAT2) the service hears it too: the player's other devices keep their unread counts in
    /// step, and in a direct conversation the friend may see "Görüldü".
    pub fn chat_read(&self, conv: &str) {
        let (changed, mark) = {
            let mut core = self.lock();
            let reads = offered(&core).is_some_and(|extras| extras.reads);
            let online = core.state.connection() == Connection::Online;
            let Some(history) = core.history.as_mut() else {
                return;
            };
            let changed = history.mark_read(conv);
            let mark = (reads && online)
                .then(|| history.read_target(conv))
                .flatten()
                .filter(|id| *id > history.mark(conv));
            (changed, mark)
        };
        if let Some(id) = mark {
            self.send_mark(conv, id);
        }
        if changed {
            self.emit_view();
        }
    }

    /// A read mark goes at most every 2 seconds per conversation; the newest that waited goes
    /// when the time is up.
    fn send_mark(&self, conv: &str, id: i64) {
        let mut core = self.lock();
        let now = Instant::now();
        let generation = core.generation;
        let pace = core.marks.entry(conv.to_owned()).or_default();
        if let Some(sent) = pace.sent
            && now.duration_since(sent) < MARK_EVERY
        {
            let first = pace.waiting.is_none();
            pace.waiting = pace.waiting.max(Some(id));
            if first {
                let wait = MARK_EVERY - now.duration_since(sent);
                let service = self.clone();
                let conv = conv.to_owned();
                self.spawn(async move {
                    tokio::time::sleep(wait).await;
                    service.flush_mark(generation, &conv);
                });
            }
            return;
        }
        pace.sent = Some(now);
        pace.waiting = None;
        mark_now(&mut core, conv, id);
    }

    /// The mark that waited for its time goes, unless one as new went meanwhile.
    fn flush_mark(&self, generation: u64, conv: &str) {
        let mut core = self.lock();
        if core.generation != generation {
            return;
        }
        let Some(pace) = core.marks.get_mut(conv) else {
            return;
        };
        let Some(id) = pace.waiting.take() else {
            return;
        };
        pace.sent = Some(Instant::now());
        mark_now(&mut core, conv, id);
    }

    /// The player writes in a conversation (a key in a box with text and focus, not while
    /// editing, not while the window is away; CHAT2). "Yazıyor…" goes at most every 4 seconds,
    /// only while someone else in the conversation is online, and never while the player is
    /// invisible, muted or keeps "Yazıyor bilgisi" off.
    pub fn chat_typing(&self, conv: &str) {
        let mut core = self.lock();
        let Some(status) = core.chat_status.as_ref() else {
            return;
        };
        if !core.chat_enabled
            || !status.on
            || !status.offered().is_some_and(|extras| extras.typing)
            || muted(status, core.chat_skew_ms)
        {
            return;
        }
        let Some(me) = core.shown_me() else {
            return;
        };
        if me.status == StatusMode::Invisible || !me.privacy.shares_typing() {
            return;
        }
        let me = me.profile.id;
        if !conversation_writable(&core.state, conv) || !someone_online(&core.state, conv, &me) {
            return;
        }
        let now = Instant::now();
        if core
            .typing_sent
            .get(conv)
            .is_some_and(|at| now.duration_since(*at) < TYPING_EVERY)
        {
            return;
        }
        if core.typing_sent.len() > 256 {
            core.typing_sent
                .retain(|_, at| now.duration_since(*at) < TYPING_EVERY);
        }
        core.typing_sent.insert(conv.to_owned(), now);
        if let Some(control) = &core.presence {
            control.chat_typing(conv);
        }
    }

    /// Gives (`on`) or takes back the player's reaction on message `id` (CHAT2): shown at once,
    /// settled by the service's answer (put back when it refuses). On messages younger than a week
    /// only; at most 5 different of the player's on a message and 10 kinds on it; never 🖕.
    pub async fn chat_react(&self, conv: &str, id: i64, emoji: &str, on: bool) -> Result<()> {
        self.chat_writable(conv)?;
        if !self.chat_offers(|extras| extras.reactions) {
            return Err(SocialError::InvalidInput("chat extras"));
        }
        let (control, e, generation) = {
            let mut core = self.lock();
            let me = my_id(&core);
            let skew = core.chat_skew_ms;
            let control = core
                .presence
                .clone()
                .ok_or(SocialError::Unavailable { retry_after: None })?;
            let history = core
                .history
                .as_mut()
                .ok_or(SocialError::InvalidInput("chat message"))?;
            let message = history
                .message(conv, id)
                .ok_or(SocialError::InvalidInput("chat message"))?;
            if message.id + skew + REACT_WINDOW_MS <= unix_now_ms() {
                return Err(SocialError::InvalidInput("chat message"));
            }
            let ReactionStep::Send { e } = reaction_step(emoji, on, &message.reactions, &me)?
            else {
                return Ok(());
            };
            let change = history.react(conv, id, &me, &e, on);
            *core.reacting.entry(reacting_key(conv, id)).or_default() += 1;
            if let Some(change) = change {
                push_change(&mut core, &change);
            }
            (control, e, core.generation)
        };
        self.emit_view();
        let result = control.chat_react(conv, id, &e, on).await;
        {
            let mut core = self.lock();
            if core.generation != generation {
                return result.map(drop);
            }
            let key = reacting_key(conv, id);
            let left = core.reacting.get_mut(&key).map_or(0, |n| {
                *n = n.saturating_sub(1);
                *n
            });
            if left == 0 {
                core.reacting.remove(&key);
            }
            let me = my_id(&core);
            let change = core.history.as_mut().and_then(|history| match &result {
                // The service's word stands once nothing else of the player's is on its way.
                Ok(reactions) if left == 0 => history.set_reactions(conv, id, reactions),
                Ok(_) => None,
                // Not taken: what was shown goes back.
                Err(_) => history.react(conv, id, &me, &e, !on),
            });
            if let Some(change) = change {
                push_change(&mut core, &change);
            }
        }
        self.emit_view();
        result.map(drop)
    }

    /// The card of a message's first link (CHAT2): asked once, when the message comes on screen
    /// (younger than a week, while links are on); the message comes back with it. Nothing comes
    /// when there is nothing to ask.
    pub async fn chat_unfurl(&self, conv: &str, key: &str) -> Result<()> {
        let (control, url, generation) = {
            let mut core = self.lock();
            if !offered(&core).is_some_and(|extras| extras.links) {
                return Ok(());
            }
            let skew = core.chat_skew_ms;
            let Some(history) = core.history.as_mut() else {
                return Ok(());
            };
            let Some(Entry::Sent(message)) = history.entry_by_key(conv, key) else {
                return Ok(());
            };
            if message.sticker.is_some() || message.id + skew + CARD_WINDOW_MS <= unix_now_ms() {
                return Ok(());
            }
            let Some(url) = first_link(&message.text).map(str::to_owned) else {
                return Ok(());
            };
            if history.cards().get(&url).is_some() {
                // Known already: the message as it is (with its card) for the screen that asked.
                let change = Change::of(conv, Entry::Sent(message));
                push_change(&mut core, &change);
                drop(core);
                self.emit_view();
                return Ok(());
            }
            if !core.cards_asked.insert(url.clone()) {
                return Ok(());
            }
            let Some(control) = core.presence.clone() else {
                core.cards_asked.remove(&url);
                return Ok(());
            };
            (control, url, core.generation)
        };
        let answer = self.fetch_card(&control, &url).await;
        {
            let mut core = self.lock();
            if core.generation != generation {
                return Ok(());
            }
            let change = core.history.as_mut().and_then(|history| {
                match answer {
                    CardAnswer::Keep(card) => history.cards().put(&url, card),
                    CardAnswer::Passing => history.cards().put_passing(&url),
                }
                history
                    .entry_by_key(conv, key)
                    .map(|entry| Change::of(conv, entry))
            });
            if let Some(change) = change {
                push_change(&mut core, &change);
            }
        }
        self.emit_view();
        Ok(())
    }

    /// The card of a link written in the composer (CHAT2): the same card the message gets once it
    /// is sent. `url` is one plain `https` link as written.
    pub async fn chat_link_card(&self, url: &str) -> Result<ChatCardView> {
        if find_links(url) != [url] || first_link(url) != Some(url) {
            return Err(SocialError::InvalidInput("chat link"));
        }
        let (control, generation) = {
            let mut core = self.lock();
            if !offered(&core).is_some_and(|extras| extras.links) {
                return Err(SocialError::InvalidInput("chat off"));
            }
            if let Some(stored) = core
                .history
                .as_mut()
                .and_then(|history| history.cards().get(url))
            {
                return Ok(card_view(url, &stored));
            }
            let control = core
                .presence
                .clone()
                .ok_or(SocialError::Unavailable { retry_after: None })?;
            (control, core.generation)
        };
        let answer = self.fetch_card(&control, url).await;
        let mut core = self.lock();
        let stored = match answer {
            CardAnswer::Keep(card) => {
                if core.generation == generation
                    && let Some(history) = core.history.as_mut()
                {
                    history.cards().put(url, card.clone());
                    return Ok(history
                        .cards()
                        .get(url)
                        .map_or_else(|| card_view(url, &card), |kept| card_view(url, &kept)));
                }
                card
            }
            CardAnswer::Passing => StoredCard::Plain,
        };
        Ok(card_view(url, &stored))
    }

    /// Asks the hub for a link's card, one at a time; a busy hub's short wait is honoured once.
    async fn fetch_card(&self, control: &PresenceControl, url: &str) -> CardAnswer {
        let _turn = self.inner.cards_turn.lock().await;
        // Another request may have brought it while this one waited for its turn.
        if let Some(stored) = self
            .lock()
            .history
            .as_mut()
            .and_then(|history| history.cards().get(url))
        {
            return match stored {
                StoredCard::Plain => CardAnswer::Passing,
                stored => CardAnswer::Keep(stored),
            };
        }
        for attempt in 0..2 {
            match control.chat_unfurl(url).await {
                Ok(card) => {
                    return CardAnswer::Keep(StoredCard::Ready {
                        card: Box::new(card),
                    });
                }
                Err(error) if error.code() == Some(&ErrorCode::UnfurlFailed) => {
                    return CardAnswer::Keep(match error.detail() {
                        Some(rule) => StoredCard::Refused {
                            rule: Some(LinkRule::parse(rule)),
                        },
                        None => StoredCard::Plain,
                    });
                }
                Err(SocialError::Unavailable {
                    retry_after: Some(wait),
                }) if attempt == 0 && wait <= CARD_RETRY_MAX => {
                    tokio::time::sleep(wait).await;
                }
                Err(error) => {
                    tracing::debug!(%error, "no card for a link this time");
                    return CardAnswer::Passing;
                }
            }
        }
        CardAnswer::Passing
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
            push_change(&mut core, &change);
        }
        self.emit_view();
    }

    /// A message as the service now has it (an answer to an edit): into the history and the view,
    /// with the replies that quote it.
    fn chat_store(&self, generation: u64, message: &ChatMessage) {
        {
            let mut core = self.lock();
            if core.generation != generation {
                return;
            }
            let change = core.history.as_mut().and_then(|history| {
                let change = history.put(message)?;
                Some(with_replies(history, change, message.id))
            });
            if let Some(change) = change {
                push_change(&mut core, &change);
            }
        }
        self.emit_view();
    }

    /// A message of this device's history was deleted for everyone (`by`: who, empty when not
    /// known yet): it turns into its tombstone, now (the hub's clock), and the replies that quote
    /// it say so.
    fn chat_tombstone(&self, generation: u64, conv: &str, id: i64, by: &str) {
        {
            let mut core = self.lock();
            if core.generation != generation {
                return;
            }
            let at = unix_now_ms() - core.chat_skew_ms;
            let change = core.history.as_mut().and_then(|history| {
                let change = history.delete(conv, id, by, at)?;
                Some(with_replies(history, change, id))
            });
            if let Some(change) = change {
                push_change(&mut core, &change);
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
    /// waits for "Tekrar dene". A reply goes with the message it answers as held here; when that
    /// is gone, or the service no longer takes replies, the message goes without its link.
    async fn send_waiting(&self) {
        loop {
            let next =
                {
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
                        if let Some(change) = core.history.as_mut().and_then(|h| {
                            h.set_outgoing(&nonce, Some(SendFailure::Connection), None)
                        }) {
                            push_change(&mut core, &change);
                        }
                    }
                    // The message a reply answers, as held now (its latest version).
                    let parent = pick.as_ref().and_then(|outgoing| {
                        let id = outgoing.reply?;
                        core.history.as_mut()?.message(&outgoing.conv, id)
                    });
                    match (core.presence.clone(), pick) {
                        (Some(control), Some(outgoing)) if online => {
                            Some((control, outgoing, parent, core.generation))
                        }
                        _ => {
                            core.sending = false;
                            None
                        }
                    }
                };
            let Some((control, outgoing, parent, generation)) = next else {
                self.emit_view();
                return;
            };
            let result = control
                .chat_post(
                    &outgoing.conv,
                    &outgoing.body(),
                    parent.as_ref(),
                    &outgoing.nonce,
                )
                .await;
            let mut wait = None;
            {
                let mut core = self.lock();
                if core.generation != generation {
                    core.sending = false;
                    return;
                }
                let change = match result {
                    Ok(message) => core.history.as_mut().and_then(|h| h.put(&message)),
                    // The message it answers is gone (or was never held), or the service no
                    // longer takes replies: it goes again at once, without the link.
                    Err(error)
                        if outgoing.reply.is_some()
                            && (parent.is_none()
                                || error.code() == Some(&ErrorCode::MessageNotFound)
                                || (error.code() == Some(&ErrorCode::ChatFeatureOff)
                                    && error.detail() == Some("replies"))) =>
                    {
                        core.history
                            .as_mut()
                            .and_then(|h| h.unlink_outgoing(&outgoing.nonce))
                    }
                    Err(error) => {
                        let (failure, rule) = failure_of(&error);
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
                                .and_then(|h| h.set_outgoing(&outgoing.nonce, Some(failure), rule)),
                        }
                    }
                };
                if let Some(change) = change {
                    push_change(&mut core, &change);
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
    /// holds, as the moderators' context; CHAT2: a reply with the message it answers). `block`
    /// also blocks the player (players and messages). A second report about the same thing while
    /// the first is open changes nothing; either way the player hears "Raporun iletildi.".
    pub async fn report(
        &self,
        target: ReportTarget,
        reason: ReportReason,
        note: Option<String>,
        block: bool,
    ) -> Result<()> {
        let (wire, blocked) = {
            let mut core = self.lock();
            let extras = offered(&core).is_some();
            match &target {
                ReportTarget::Player { id, plan, reaction } => {
                    let mut wire = serde_json::json!({ "kind": "player", "id": id });
                    // From a plan card (FIX2): the plan goes along as the moderators' evidence.
                    if let Some(plan) = plan.as_ref()
                        && core.state.offers(TEAM_FEATURE_PLAN_NOTES)
                    {
                        wire["plan"] = serde_json::json!({ "crew": plan.crew, "id": plan.id });
                    }
                    // From a reaction's list (CHAT2): the reaction, which the service checks.
                    if let Some(reaction) = reaction.as_ref().filter(|_| extras) {
                        wire["reaction"] = serde_json::json!({
                            "conv": reaction.conv, "id": reaction.id, "e": reaction.e,
                        });
                    }
                    (wire, Some(id.clone()))
                }
                ReportTarget::Crew { id, code, via } => {
                    let mut wire = serde_json::json!({ "kind": "crew", "id": id });
                    // From a tag's card (TEAM6): whose tag it was (the service checks it and keeps
                    // nothing of it). The service takes one way of seeing the crew.
                    let via = via
                        .as_deref()
                        .filter(|_| core.state.offers(TEAM_FEATURE_BADGES));
                    if let Some(via) = via {
                        wire["via"] = via.into();
                    } else if let Some(code) = code.as_ref().and_then(|c| normalize_team_code(c)) {
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
                    let earlier = history.before(conv, *id, REPORT_CONTEXT);
                    // A reply's quoted message goes along when it is not among those (CHAT2).
                    let quoted = message
                        .reply
                        .as_ref()
                        .filter(|reply| earlier.iter().all(|m| m.id != reply.id))
                        .and_then(|reply| history.message(conv, reply.id))
                        .map(|quoted| quoted.sealed());
                    let before: Vec<serde_json::Value> =
                        earlier.iter().map(ChatMessage::sealed).collect();
                    let sender = message.from.clone();
                    let mut wire = serde_json::json!({
                        "kind": "message",
                        "message": message.sealed(),
                        "before": before,
                    });
                    if let Some(quoted) = quoted {
                        wire["quoted"] = quoted;
                    }
                    (wire, Some(sender))
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

    // --- SKIN1 (`crate::minecraft`) ----------------------------------------------------------

    /// SKIN1 hears the socket's `mc` messages and reconnects from now on.
    pub(crate) fn set_mc_sink(&self, sink: McSink) {
        *self
            .inner
            .mc
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(sink);
    }

    /// Called without the core's lock: the listener asks the service back.
    fn tell_mc(&self, signal: McSignal) {
        let sink = self
            .inner
            .mc
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        if let Some(sink) = sink {
            sink(signal);
        }
    }

    /// The signed-in session and its generation (bumped by every sign-out).
    pub(crate) fn mc_session(&self) -> Option<(Session, u64)> {
        self.signed_in().ok()
    }

    /// A `/v1/mc` request found the session ended: as any other request does.
    pub(crate) fn mc_unauthorized(&self, generation: u64) {
        self.end_session(generation, false);
    }

    /// The features of the last friend list (`GET /v1/friends`); None before it came.
    pub(crate) fn friends_features(&self) -> Option<Vec<String>> {
        self.lock().state.features().map(<[String]>::to_vec)
    }

    /// The game this launcher started is on projects.gg right now (what friends see as
    /// "Projects'te").
    pub(crate) fn on_projects(&self) -> bool {
        matches!(&self.lock().activity, LocalStatus::Playing { host: Some(host) } if is_projects_host(host))
    }

    /// The presence socket is connected (its `mc` messages arrive).
    pub(crate) fn socket_online(&self) -> bool {
        self.lock().state.connection() == Connection::Online
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

    /// SOC9: the Minecraft version the leading game plays (its instance's version), which friends
    /// see next to where it is; None when it is not known. Kept for the next presence connection.
    pub fn set_game_version(&self, version: Option<&str>) {
        let mut core = self.lock();
        core.game_version = version.and_then(crate::presence::game_version);
        let version = core.game_version.clone();
        if let Some(presence) = &core.presence {
            presence.set_game_version(version);
        }
    }

    /// The game closed: back to "in the launcher".
    pub fn game_stopped(&self) {
        {
            let mut core = self.lock();
            core.game_version = None;
            if let Some(presence) = &core.presence {
                presence.set_game_version(None);
            }
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

/// SCALE-SOC: whether `event` changes what the profile or the lists show (so the copy no longer
/// matches its relations version). Presence, calls, plans, chat, invites to play, the game's and
/// Cloud AFK's news do not: they are not in the version.
fn changes_relations(event: &PresenceEvent) -> bool {
    matches!(
        event,
        PresenceEvent::TeamRequests { .. }
            | PresenceEvent::TeamChanged { .. }
            | PresenceEvent::BadgeChanged { .. }
            | PresenceEvent::MarkChanged { .. }
            | PresenceEvent::TeamInvited(_)
            | PresenceEvent::GroupsChanged(_)
            | PresenceEvent::CrewNotifications(_)
            | PresenceEvent::CrewMuted { .. }
            | PresenceEvent::CrewOrder(_)
            | PresenceEvent::FriendAdded { .. }
            | PresenceEvent::FriendRemoved { .. }
            | PresenceEvent::RequestReceived { .. }
            | PresenceEvent::RequestCancelled { .. }
            | PresenceEvent::StatusChanged(_)
            | PresenceEvent::ProfileChanged { .. }
            | PresenceEvent::Notice(_)
    )
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

/// The sticker "Selam ver" sends (TEAM6; "Selam").
const GREETING_STICKER: &str = "pl1/hi";

/// Turkey's offset from UTC: the service's days (its daily limits) are Turkey days.
const TURKEY_OFFSET_SECS: i64 = 3 * 3_600;

/// The end of the Turkey day `now` is in (Unix seconds).
fn turkey_day_end(now: i64) -> i64 {
    (now + TURKEY_OFFSET_SECS).div_euclid(86_400) * 86_400 + 86_400 - TURKEY_OFFSET_SECS
}

/// A refusal of a join request from a tag's card (TEAM6), as the card says it; None for what is
/// not one (an outage, a burst of requests, a lost session).
fn badge_refusal(error: &SocialError, now: i64) -> Option<BadgeRefusal> {
    if let SocialError::RateLimited { retry_after, .. } = error {
        // The player's 10 a day wait until the Turkey day ends; a burst waits a minute.
        let left = (turkey_day_end(now) - now).unsigned_abs();
        let daily = retry_after.is_some_and(|wait| wait.as_secs().saturating_add(120) >= left);
        return daily.then_some(BadgeRefusal::OwnDaily);
    }
    Some(match error.code()? {
        // A crew that is gone, as one that stopped taking them: the card was stale.
        ErrorCode::BadgeRequestClosed | ErrorCode::TeamNotFound => BadgeRefusal::Closed,
        ErrorCode::LimitBadgeRequests => BadgeRefusal::Today,
        ErrorCode::LimitTeamRequests => BadgeRefusal::WaitingFull,
        ErrorCode::LimitTeamMembers => BadgeRefusal::Full,
        ErrorCode::BlockedMember => BadgeRefusal::Blocked,
        ErrorCode::LimitOutgoingTeamRequests => BadgeRefusal::OwnWaiting,
        ErrorCode::LimitCrews => BadgeRefusal::OwnCrews,
        ErrorCode::AlreadyMember => BadgeRefusal::Member,
        _ => return None,
    })
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
    core.forget_rel();
    core.fetching = false;
    core.deleting = false;
    core.qr = None;
    core.groups_saving = false;
    core.groups_next = None;
    core.groups_saved = None;
    core.history = None;
    core.chat_setup.since.store(0, Ordering::Relaxed);
    core.chat_setup.catch_up.store(-1, Ordering::Relaxed);
    core.chat_status = None;
    core.chat_skew_ms = 0;
    core.chat_synced = false;
    core.chat_backlog.clear();
    core.chat_outbox.clear();
    core.sending = false;
    core.too_fast.clear();
    core.typing_sent.clear();
    core.marks.clear();
    core.reacting.clear();
    core.cards_asked.clear();
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
    core.chat_setup
        .catch_up
        .store(history.catch_up_since(), Ordering::Relaxed);
    core.history = Some(history);
    for event in std::mem::take(&mut core.chat_backlog) {
        let outcome = apply_chat(core, &event);
        // Messages that waited are no news: they show as unread.
        drop(outcome.notices);
    }
}

/// What a chat event did: changes for the UI (in the outbox), news worth telling, whether
/// waiting messages may go now, and someone writing ("yazıyor…", CHAT2).
#[derive(Default)]
struct ChatOutcome {
    notices: Vec<SocialNotice>,
    deliver: bool,
    typing: Option<ChatTypingView>,
}

/// How the player sees a conversation's messages (their times, what they may still do).
fn viewer(core: &Core, conv: &str) -> Viewer {
    let me = my_id(core);
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

/// The signed-in player's id (empty before the profile is known).
fn my_id(core: &Core) -> String {
    core.state
        .me()
        .map(|me| me.profile.id.clone())
        .unwrap_or_default()
}

/// What chat offers beyond CHAT1 on this connection (CHAT2), while it may show.
fn offered(core: &Core) -> Option<ChatExtras> {
    core.chat_status
        .as_ref()
        .filter(|status| core.chat_enabled && status.on)?
        .offered()
}

/// A moderator muted the player in chat until a time still to come (the hub's clock).
fn muted(status: &ChatStatus, skew_ms: i64) -> bool {
    status
        .muted_until
        .is_some_and(|until| until + skew_ms / 1000 > unix_now())
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

/// Someone else in the conversation is online now: the friend, or a crewmate (CHAT2: only then
/// does "yazıyor…" go).
fn someone_online(state: &SocialState, conv: &str, me: &str) -> bool {
    match Conversation::parse(conv) {
        Some(direct @ Conversation::Direct(..)) => {
            direct.peer(me).is_some_and(|peer| state.is_online(peer))
        }
        Some(Conversation::Crew(team)) => state.crew(&team).is_some_and(|entry| {
            entry
                .team
                .members
                .iter()
                .any(|member| member.profile.id != me && state.is_online(&member.profile.id))
        }),
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

/// A message's key among the player's reactions on their way.
fn reacting_key(conv: &str, id: i64) -> String {
    format!("{conv}|{id}")
}

/// A change for the UI, sent with the next view: each entry with what its view needs from the
/// history (the message a reply answers, a card).
fn push_change(core: &mut Core, change: &Change) {
    let viewer = viewer(core, &change.conv);
    let view = match core.history.as_mut() {
        Some(history) => change_view(change, &viewer, |entry| {
            context_of(history, &change.conv, entry)
        }),
        None => change_view(change, &viewer, |_| Context::default()),
    };
    core.chat_outbox.push(view);
}

/// The change with the entries that quote message `id` (their quote changed with it; CHAT2).
fn with_replies(history: &mut History, mut change: Change, id: i64) -> Change {
    let conv = change.conv.clone();
    for entry in history.replies_to(&conv, id) {
        let key = entry.key();
        if !change.upsert.iter().any(|e| e.key() == key) {
            change.upsert.push(entry);
        }
    }
    change
}

/// The player's read mark goes to the service now (CHAT2).
fn mark_now(core: &mut Core, conv: &str, id: i64) {
    if let Some(control) = &core.presence {
        control.chat_read(conv, id);
    }
    if let Some(history) = core.history.as_mut() {
        history.set_mark(conv, id);
    }
}

/// Applies a chat event: the state of this connection, the history, the UI's changes.
fn apply_chat(core: &mut Core, event: &PresenceEvent) -> ChatOutcome {
    let mut outcome = ChatOutcome::default();
    let now_ms = unix_now_ms();
    match event {
        PresenceEvent::Connected { chat, clock, .. } => {
            core.chat_status = chat.clone();
            core.chat_synced = false;
            if let Some(clock) = clock {
                core.chat_skew_ms = now_ms - clock * 1_000;
            }
            return outcome;
        }
        PresenceEvent::ChatState(status) => {
            core.chat_status = Some(status.clone());
            return outcome;
        }
        PresenceEvent::ChatSynced { clock, caught_up } => {
            if let Some(clock) = clock {
                core.chat_skew_ms = now_ms - clock;
            }
            core.chat_synced = true;
            // The week-long sync with chat extras is done (CHAT2): later ones start at the newest.
            if *caught_up && let Some(history) = core.history.as_mut() {
                history.set_caught_up();
                core.chat_setup.catch_up.store(-1, Ordering::Relaxed);
            }
            outcome.deliver = true;
            return outcome;
        }
        // Someone writes (CHAT2): only while the player shares and sees it; never their own.
        PresenceEvent::ChatTyping { conv, by } => {
            let me = core.shown_me();
            let shown = offered(core).is_some_and(|extras| extras.typing)
                && me.is_some_and(|me| me.privacy.shares_typing() && me.profile.id != *by)
                && Conversation::parse(conv).is_some();
            if shown {
                outcome.typing = Some(ChatTypingView {
                    conv: conv.clone(),
                    by: by.clone(),
                });
            }
            return outcome;
        }
        PresenceEvent::Disconnected { .. } | PresenceEvent::Stopped(_) => {
            core.chat_synced = false;
            return outcome;
        }
        PresenceEvent::ChatMessage(_)
        | PresenceEvent::ChatEdited(_)
        | PresenceEvent::ChatDeleted { .. }
        | PresenceEvent::ChatSync { .. }
        | PresenceEvent::ChatReacted { .. }
        | PresenceEvent::ChatRead { .. } => {}
        _ => return outcome,
    }
    if core.history.is_none() {
        if core.chat_backlog.len() < CHAT_BACKLOG_MAX {
            core.chat_backlog.push(event.clone());
        }
        return outcome;
    }
    // Marks of the friend's show only while the player shares (and so sees) them.
    let sees_marks = core.shown_me().is_some_and(|me| me.privacy.shares_read());
    let mut changes: Vec<Change> = Vec::new();
    let mut news: Option<ChatMessage> = None;
    if let Some(history) = core.history.as_mut() {
        let me = history.me().to_owned();
        // A read mark: the player's own from another device, or the friend's ("Görüldü").
        let mark = |history: &mut History, conv: &str, by: &str, id: i64| {
            if by == me {
                history.read_up_to(conv, id);
            } else if sees_marks {
                history.set_peer_read(conv, id);
            }
        };
        match event {
            PresenceEvent::ChatMessage(message) => {
                if let Some(change) = history.put(message) {
                    if message.from != me {
                        news = Some(message.clone());
                    }
                    changes.push(change);
                }
            }
            // An edit, a deletion: the replies that quote it show it as it is now.
            PresenceEvent::ChatEdited(message) => {
                if let Some(change) = history.put(message) {
                    changes.push(with_replies(history, change, message.id));
                }
            }
            // A message this device holds turns into its tombstone (FIX2).
            PresenceEvent::ChatDeleted { conv, id, by, at } => {
                if let Some(change) = history.delete(conv, *id, by, *at) {
                    changes.push(with_replies(history, change, *id));
                }
            }
            PresenceEvent::ChatSync {
                conv,
                messages,
                deleted,
                reacted,
                reads,
            } => {
                let mut list = Vec::new();
                for message in messages {
                    list.extend(history.put(message));
                }
                for deletion in deleted {
                    if let Some(change) =
                        history.delete(conv, deletion.id, &deletion.by, deletion.at)
                    {
                        list.push(with_replies(history, change, deletion.id));
                    }
                }
                for message in reacted {
                    list.extend(history.set_reactions(conv, message.id, &message.reactions));
                }
                for read in reads {
                    mark(history, conv, &read.by, read.id);
                }
                changes.extend(merge(list));
            }
            PresenceEvent::ChatReacted {
                conv,
                id,
                by,
                e,
                on,
            } => {
                // The player's own reactions on their way settle with the service's answer.
                let waiting = *by == me && core.reacting.contains_key(&reacting_key(conv, *id));
                if !waiting {
                    changes.extend(history.react(conv, *id, by, e, *on));
                }
            }
            PresenceEvent::ChatRead { conv, by, id } => mark(history, conv, by, *id),
            _ => {}
        }
        core.chat_setup
            .since
            .store(history.sync_since(), Ordering::Relaxed);
    }
    for change in &changes {
        push_change(core, change);
    }
    if let Some(message) = news {
        let viewer = viewer(core, &message.conv);
        let crew = match Conversation::parse(&message.conv) {
            Some(Conversation::Crew(team)) => Some(team),
            _ => None,
        };
        let context = core
            .history
            .as_mut()
            .map(|history| context_of(history, &message.conv, &Entry::Sent(message.clone())))
            .unwrap_or_default();
        outcome.notices.push(SocialNotice::ChatMessage {
            conv: message.conv.clone(),
            message: Box::new(message_view(&message, &viewer, context)),
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
    let status = core.chat_status.clone()?;
    let me = core.state.me()?.profile.id.clone();
    let sees_marks = core.shown_me().is_some_and(|me| me.privacy.shares_read());
    let extras = status.offered();
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
        let peer_read = (peer.is_some()
            && sees_marks
            && extras.is_some_and(|extras| extras.reads)
            && meta.peer_read > 0)
            .then_some(meta.peer_read);
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
                deleted_by: last.d.clone(),
                sticker: last.s.clone(),
            }),
            unread: meta.unread,
            writable,
            failed: failed.contains(conv),
            peer_read,
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
                peer_read: None,
            });
        }
    }
    for (conv, title) in titles {
        history.set_title(&conv, &title);
    }
    list.sort_by_key(|c| std::cmp::Reverse(c.last.as_ref().map_or(i64::MIN, |last| last.at)));
    let sticker_sets = status
        .stickers
        .iter()
        .filter(|set| stickers::sticker_sets().contains(set))
        .cloned()
        .collect();
    Some(ChatView {
        on: status.on,
        mailbox: status.mailbox,
        per_day: status.per_day,
        muted_until: status
            .muted_until
            .map(|until| until + skew / 1_000)
            .filter(|until| *until > now),
        conversations: list,
        links: status
            .links
            .as_ref()
            .filter(|_| extras.is_some_and(|extras| extras.links))
            .map(ChatLinksView::from),
        extras: extras.map(ChatExtrasView::from),
        sticker_sets,
    })
}

fn unix_now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        })
}

/// The switches a profile change sets, one bit each.
fn profile_fields(update: &ProfileUpdate) -> u8 {
    u8::from(update.avatar.is_some())
        | (u8::from(update.status.is_some()) << 1)
        | (u8::from(update.share_server.is_some()) << 2)
        | (u8::from(update.share_last_seen.is_some()) << 3)
        | (u8::from(update.share_read.is_some()) << 4)
        | (u8::from(update.share_typing.is_some()) << 5)
        | (u8::from(update.badge.is_some()) << 6)
}

/// Every switch the waiting change `id` sets is set again by newer changes of the same sign-in
/// that wait behind it (SOC-LAG): the newest choice is the one to send.
fn superseded(pending: &[PendingProfile], id: u64) -> bool {
    let Some(at) = pending.iter().position(|pending| pending.id == id) else {
        return false;
    };
    let mine = &pending[at];
    let later = pending[at + 1..]
        .iter()
        .filter(|pending| pending.generation == mine.generation)
        .fold(0, |fields, pending| {
            fields | profile_fields(&pending.update)
        });
    let own = profile_fields(&mine.update);
    own != 0 && own & !later == 0
}

/// What the player's crew notification settings become once the service takes `edit` (shown
/// before it answers, SOC-LAG): a mute without an end lasts until unmuted, unmuting ends any.
fn notifications_after(
    mut settings: TeamNotifications,
    edit: &TeamNotificationsEdit,
) -> TeamNotifications {
    if let Some(calls) = edit.calls {
        settings.calls = calls;
    }
    if let Some(muted) = edit.muted {
        settings.muted = muted;
        settings.mute_until = edit.mute_until.filter(|_| muted);
    }
    settings
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
    // The player's own changes on their way show already (SOC-LAG).
    let shown_me = core.shown_me();
    let share_server = shown_me.as_ref().is_some_and(|me| me.privacy.share_server);
    // Crew tags (TEAM6) belong to crews: none shows while crews are off here.
    let mut friends = core.state.friends();
    if !teams {
        for friend in &mut friends {
            friend.badge = None;
        }
    }
    let my_badge = if teams { core.state.my_badge() } else { None };
    let mut crews = if teams {
        core.state.crew_views(now)
    } else {
        Vec::new()
    };
    // A new picture on its way shows on the player's own crew pages too.
    if core.picture_pending()
        && let Some(me) = shown_me.as_ref()
    {
        for member in crews
            .iter_mut()
            .flat_map(|crew| crew.members.iter_mut())
            .filter(|member| member.me)
        {
            member.avatar.clone_from(&me.profile.avatar);
            member.avatar_url = me.profile.discord_avatar_url(128);
        }
    }
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
        me: shown_me.as_ref().map(|me| me_view(me, share, my_badge)),
        loaded: core.state.loaded(),
        friends,
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
        crew_joins: if teams {
            core.state.crew_joins()
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
                    SocialUpdate::View(_)
                    | SocialUpdate::Chat(_)
                    | SocialUpdate::Typing(_)
                    | SocialUpdate::TagOff(_) => None,
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

    /// BETA-GATE: the question carries the session while someone is signed in and nobody's
    /// otherwise; the feature off is "cannot tell"; a yes is kept, a "not on the team" forgets it.
    #[tokio::test]
    async fn asks_about_the_beta_channel_and_keeps_the_yes() {
        let member = Arc::new(AtomicBool::new(true));
        let server = MockServer::start({
            let member = member.clone();
            move |req| {
                presence_refused(&req.path).unwrap_or_else(|| match req.path.as_str() {
                    "/v1/me" => MockReply::json(200, ME),
                    "/v1/friends" => MockReply::json(200, FRIENDS),
                    "/v1/beta" => match req.header("authorization") {
                        None => MockReply::json(
                            200,
                            r#"{"gate":true,"access":"signin","apply":"https://projects.gg/basvuru"}"#,
                        ),
                        Some(_) if member.load(Ordering::SeqCst) => MockReply::json(
                            200,
                            r#"{"gate":true,"access":"member","apply":"https://projects.gg/basvuru"}"#,
                        ),
                        Some(_) => MockReply::json(
                            200,
                            r#"{"gate":true,"access":"none","apply":"https://projects.gg/basvuru"}"#,
                        ),
                    },
                    _ => MockReply::json(404, r#"{"error":{"code":"not_found","message":"x"}}"#),
                })
            }
        })
        .await;
        let dir = tempfile::tempdir().unwrap();
        let record = crate::BetaRecordFile::new(dir.path().join(crate::BETA_RECORD_FILE));

        // The feature off: nobody to ask.
        let off = Harness::new(Arc::new(MemoryTokenStore::new()));
        assert_eq!(off.service.beta_answer().await, BetaAnswer::unknown());

        // Nobody signed in: the question goes without a session; the join waits for a sign-in.
        let out = Harness::new(Arc::new(MemoryTokenStore::new()));
        out.service.configure(url(&server));
        assert_eq!(out.service.beta_answer().await, BetaAnswer::sign_in());
        let refused = out.service.beta_join(&record).await;
        assert_eq!(refused.verdict, crate::BetaVerdict::SignIn);
        assert_eq!(record.load(), None);

        // Signed in on the team: let in, and the time kept.
        let tokens = Arc::new(MemoryTokenStore::new());
        tokens.save(&token()).unwrap();
        let harness = Harness::new(tokens);
        harness.service.configure(url(&server));
        harness
            .until("signed in", |view| view.auth == AuthPhase::SignedIn)
            .await;
        assert!(harness.service.beta_join(&record).await.allowed());
        let kept = record.load().expect("the yes is kept");
        assert!(harness.service.beta_keep(&record).await.allowed());
        // Off the team: back to stable, and the time goes.
        member.store(false, Ordering::SeqCst);
        let left = harness.service.beta_keep(&record).await;
        assert_eq!(left.verdict, crate::BetaVerdict::NotMember);
        assert_eq!(left.apply, "https://projects.gg/basvuru");
        assert_eq!(record.load(), None);
        assert!(kept > 0);
        let asked: Vec<_> = server
            .requests()
            .into_iter()
            .filter(|r| r.path == "/v1/beta")
            .map(|r| r.header("authorization").is_some())
            .collect();
        assert_eq!(asked, vec![false, false, true, true, true]);
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
            rel: None,
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

    /// SCALE-SOC: a reconnect whose hello names the relations version this copy has reads nothing
    /// again; another version reads the lists once, the profile with them (one request), and keeps
    /// the new version; a copy an event changed reads again whatever hello says; an older
    /// service (no version) reads as before.
    #[tokio::test]
    async fn a_reconnect_with_the_same_relations_version_reads_nothing_again() {
        let version = Arc::new(Mutex::new(Some("0123456789abcdef".to_owned())));
        let server = MockServer::start({
            let version = version.clone();
            move |req| {
                presence_refused(&req.path).unwrap_or_else(|| match req.path.as_str() {
                    "/v1/me" => MockReply::json(200, ME),
                    "/v1/friends" => {
                        let mut list: serde_json::Value = serde_json::from_str(FRIENDS).unwrap();
                        if let Some(rel) = version.lock().unwrap().clone() {
                            list["rel"] = rel.into();
                            list["me"] = serde_json::from_str(ME).unwrap();
                        }
                        MockReply::json(200, &list.to_string())
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
        harness
            .until("loaded", |view| {
                view.loaded && view.auth == AuthPhase::SignedIn
            })
            .await;
        let held = || harness.service.lock().rel.get();
        assert_eq!(held().as_deref(), Some("0123456789abcdef"));
        assert!(
            server
                .requests()
                .iter()
                .any(|r| r.target == "/v1/friends?rel=1")
        );
        let reads = |path: &str| server.requests().iter().filter(|r| r.path == path).count();
        let connected = |rel: Option<&str>| PresenceEvent::Connected {
            status: crate::model::StatusMode::Online,
            friends: vec![],
            invites: vec![],
            calls: vec![],
            clock: None,
            rel: rel.map(str::to_owned),
            chat: None,
            plans: None,
        };
        // A reconnect long after the last read (the minute's rule is not what holds it back).
        let reconnect = |rel: Option<&str>| {
            let (generation, epoch) = {
                let mut core = harness.service.lock();
                core.fetched_at = Some(Instant::now() - Duration::from_secs(600));
                (core.generation, core.epoch)
            };
            harness
                .service
                .on_presence(generation, epoch, connected(rel));
        };
        let settle = || async {
            let deadline = Instant::now() + Duration::from_secs(5);
            while harness.service.lock().fetching && Instant::now() < deadline {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        };
        let (lists, profiles) = (reads("/v1/friends"), reads("/v1/me"));

        // The same version: nothing is read.
        reconnect(Some("0123456789abcdef"));
        settle().await;
        assert_eq!((reads("/v1/friends"), reads("/v1/me")), (lists, profiles));

        // Another version: the lists once, the profile with them; the new version is kept.
        *version.lock().unwrap() = Some("fedcba9876543210".into());
        reconnect(Some("fedcba9876543210"));
        settle().await;
        assert_eq!(
            (reads("/v1/friends"), reads("/v1/me")),
            (lists + 1, profiles)
        );
        assert_eq!(held().as_deref(), Some("fedcba9876543210"));

        // An event changed the copy: the version is forgotten, the next reconnect reads again.
        let (generation, epoch) = {
            let core = harness.service.lock();
            (core.generation, core.epoch)
        };
        harness.service.on_presence(
            generation,
            epoch,
            PresenceEvent::FriendRemoved {
                id: "223456789012345678".into(),
            },
        );
        assert_eq!(held(), None);
        reconnect(Some("fedcba9876543210"));
        settle().await;
        assert_eq!(reads("/v1/friends"), lists + 2);
        assert_eq!(held().as_deref(), Some("fedcba9876543210"));

        // An older service brings no version and no profile with the lists: both, as before.
        *version.lock().unwrap() = None;
        reconnect(None);
        settle().await;
        assert_eq!(
            (reads("/v1/friends"), reads("/v1/me")),
            (lists + 3, profiles + 1)
        );
        assert_eq!(held(), None);
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
                            // Sent a minute ago: a fixed date ran out of a join request's 7 days
                            // (27 Oct 2026) and the view rightly dropped it.
                            &format!(
                                r#"{{"request":{{"team":{{"id":"9999aaaabbbbcccc","name":"Skyblock",
                                "tag":"SKY","emblem":"castle","members":6,"join":"approval"}},
                                "sentAt":{}}}}}"#,
                                unix_now() - 60
                            ),
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
                .make_plan("0123456789abcdef", 1_792_612_800, None)
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

    /// TEAM6's card of crew `crew`.
    fn badge_json(crew: &str, name: &str, tag: &str) -> String {
        format!(
            r#"{{"crew":"{crew}","tag":"{tag}","color":"navy","frame":"shield","emblem":"bolt",
            "name":"{name}","motto":"","createdOn":"2026-09-20","members":3,"requests":true}}"#
        )
    }

    /// The friend list of a service with crew tags (`features`): Bora wears PvP's tag, the player
    /// none; the player leads Gece Kartalları and is in PvP.
    fn friends_with_tags(features: &str) -> String {
        let pvp = badge_json("fedcba9876543210", "PvP", "PVP");
        let crew_b = CREW_B.replace(
            r#""role":"founder","joinedAt":4}"#,
            &format!(r#""role":"founder","joinedAt":4,"badge":{pvp}}}"#),
        );
        format!(
            r#"{{"friends":[{{"id":"223456789012345678","username":"bora","displayName":"Bora",
            "avatar":"preset:fox","avatarHash":null,"since":"2026-10-01","lastSeen":"today",
            "badge":{pvp}}}],"incoming":[],"outgoing":[],"blocked":[],
            "limits":{{"friends":200,"outgoingRequests":50,"teamMembers":30,"groups":20}},
            "team":{CREW_A},"teamInvited":[],
            "crews":[{{"team":{CREW_A},"invited":[]}},{{"team":{crew_b},"invited":[]}}],
            "crewLimit":5,"teamInvites":[],"teamRequests":[],"teamFeatures":{features}}}"#
        )
    }

    #[test]
    fn the_turkey_day_ends_at_midnight_in_turkey() {
        // 2026-10-22 20:59:59 UTC is 23:59:59 in Turkey; a second later the next day starts.
        let before = 1_792_702_799;
        assert_eq!(turkey_day_end(before), before + 1);
        assert_eq!(turkey_day_end(before + 1), before + 1 + 86_400);
        // A daily limit waits until then; a burst a minute.
        let daily = SocialError::RateLimited {
            retry_after: Some(Duration::from_secs(3_000)),
            item: None,
        };
        let now = before - 2_999;
        assert_eq!(badge_refusal(&daily, now), Some(BadgeRefusal::OwnDaily));
        let burst = SocialError::RateLimited {
            retry_after: Some(Duration::from_secs(60)),
            item: None,
        };
        assert_eq!(badge_refusal(&burst, now), None);
        assert_eq!(badge_refusal(&SocialError::Network("x".into()), now), None);
    }

    #[tokio::test]
    async fn crew_tags_are_worn_hidden_asked_from_and_reported_as_the_service_offers_them() {
        let bodies = Arc::new(Mutex::new(Vec::<String>::new()));
        let wearing = ME.replace(
            r#""size":64}"#,
            &format!(
                r#""size":64}},"badge":{}"#,
                badge_json("0123456789abcdef", "Gece Kartalları", "GKR")
            ),
        );
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
                            &friends_with_tags(
                                r#"["look","roles","multi","requests","link","badges","badgeRequests"]"#,
                            ),
                        ),
                        ("PATCH", "/v1/me") => MockReply::json(200, &wearing),
                        ("PATCH", "/v1/teams/0123456789abcdef") => MockReply::json(
                            200,
                            &format!(
                                r#"{{"team":{},"invited":[]}}"#,
                                CREW_A.replace(r#""members":["#, r#""hidden":true,"members":["#)
                            ),
                        ),
                        ("POST", "/v1/teams/9999aaaabbbbcccc/requests") => MockReply::json(
                            202,
                            // Sent a minute ago (a fixed date runs out of the 7 days).
                            &format!(
                                r#"{{"request":{{"team":{{"id":"9999aaaabbbbcccc","name":"Skyblock",
                                "tag":"SKY","emblem":"castle","members":6,"join":"open"}},
                                "sentAt":{}}}}}"#,
                                unix_now() - 60
                            ),
                        ),
                        ("POST", "/v1/teams/aaaabbbbccccdddd/requests") => MockReply::json(
                            422,
                            r#"{"error":{"code":"limit_badge_requests","message":"x"}}"#,
                        ),
                        ("POST", "/v1/teams/bbbbccccddddeeee/requests") => MockReply::json(
                            409,
                            r#"{"error":{"code":"badge_request_closed","message":"x"}}"#,
                        ),
                        ("POST", "/v1/teams/ccccddddeeeeffff/requests") => MockReply::json(
                            429,
                            r#"{"error":{"code":"rate_limited","message":"Too many requests"}}"#,
                        )
                        .with_header("Retry-After", "60"),
                        ("POST", "/v1/reports") => {
                            MockReply::json(201, r#"{"id":"r1","status":"open"}"#)
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
        assert_eq!(
            view.friends[0].badge.as_ref().map(|b| b.tag.as_str()),
            Some("PVP")
        );
        assert!(view.me.as_ref().unwrap().badge.is_none());
        assert_eq!(
            view.crews[1].members[0]
                .badge
                .as_ref()
                .map(|b| b.crew.as_str()),
            Some("fedcba9876543210")
        );

        // Worn: the answer's `me` brings the card, also onto the player's own crew entries.
        harness
            .service
            .set_badge(Some("0123456789abcdef"))
            .await
            .unwrap();
        let view = harness.service.view();
        assert_eq!(
            view.me
                .as_ref()
                .unwrap()
                .badge
                .as_ref()
                .map(|b| b.tag.as_str()),
            Some("GKR")
        );
        assert_eq!(view.crews[0].wearers, 1);
        // A crew that is not the player's goes nowhere.
        assert!(matches!(
            harness.service.set_badge(Some("1111222233334444")).await,
            Err(SocialError::InvalidInput("not in a team"))
        ));
        harness.service.set_badge(None).await.unwrap();

        // The leader's two settings.
        harness
            .service
            .set_team_hidden("0123456789abcdef", true)
            .await
            .unwrap();
        assert!(harness.service.view().crews[0].hidden);
        harness
            .service
            .set_team_badge_requests("0123456789abcdef", false)
            .await
            .unwrap();

        // From Bora's card: sent; the crew's day full; a stale card; a burst is an error.
        let bora = "223456789012345678";
        assert_eq!(
            harness
                .service
                .request_from_badge("9999aaaabbbbcccc", bora)
                .await
                .unwrap(),
            BadgeRequestResult::Sent
        );
        assert_eq!(harness.service.view().team_requests[0].name, "Skyblock");
        let now = unix_now();
        match harness
            .service
            .request_from_badge("aaaabbbbccccdddd", bora)
            .await
            .unwrap()
        {
            BadgeRequestResult::Refused { reason, until } => {
                assert_eq!(reason, BadgeRefusal::Today);
                assert!(until > now && until <= now + 86_400);
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(matches!(
            harness
                .service
                .request_from_badge("bbbbccccddddeeee", bora)
                .await
                .unwrap(),
            BadgeRequestResult::Refused {
                reason: BadgeRefusal::Closed,
                ..
            }
        ));
        let burst = harness
            .service
            .request_from_badge("ccccddddeeeeffff", bora)
            .await;
        // (In the last minutes of a Turkey day a minute's wait reaches the next one.)
        if turkey_day_end(unix_now()) - unix_now() > 300 {
            assert!(matches!(burst, Err(SocialError::RateLimited { .. })));
        }

        // "Ekibi rapor et" from the card: the wearer goes, the code does not.
        harness
            .service
            .report(
                ReportTarget::Crew {
                    id: "fedcba9876543210".into(),
                    code: Some("EKP-WXYZ-1234".into()),
                    via: Some(bora.into()),
                },
                ReportReason::Name,
                None,
                false,
            )
            .await
            .unwrap();

        // Crews off in the signed config: no tag shows at all.
        harness.service.set_teams_enabled(false);
        let view = harness.service.view();
        assert!(view.friends.iter().all(|f| f.badge.is_none()));
        assert!(view.crew_joins.is_empty());

        let sent = bodies.lock().unwrap().clone();
        assert_eq!(
            sent,
            vec![
                r#"PATCH /v1/me {"badge":"0123456789abcdef"}"#.to_owned(),
                r#"PATCH /v1/me {"badge":null}"#.to_owned(),
                r#"PATCH /v1/teams/0123456789abcdef {"hidden":true}"#.to_owned(),
                r#"PATCH /v1/teams/0123456789abcdef {"badgeRequests":false}"#.to_owned(),
                format!(r#"POST /v1/teams/9999aaaabbbbcccc/requests {{"via":"{bora}"}}"#),
                format!(r#"POST /v1/teams/aaaabbbbccccdddd/requests {{"via":"{bora}"}}"#),
                format!(r#"POST /v1/teams/bbbbccccddddeeee/requests {{"via":"{bora}"}}"#),
                format!(r#"POST /v1/teams/ccccddddeeeeffff/requests {{"via":"{bora}"}}"#),
                format!(
                    r#"POST /v1/reports {{"reason":"name","target":{{"id":"fedcba9876543210","kind":"crew","via":"{bora}"}}}}"#
                ),
            ]
        );
    }

    #[tokio::test]
    async fn an_older_service_never_hears_of_crew_tags() {
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
                        // A card a service sends without listing `badges` shows nowhere either.
                        ("GET", "/v1/friends") => MockReply::json(
                            200,
                            &friends_with_tags(r#"["look","roles","multi","requests"]"#),
                        ),
                        ("POST", "/v1/reports") => {
                            MockReply::json(201, r#"{"id":"r1","status":"open"}"#)
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
        assert!(view.friends[0].badge.is_none());
        assert!(view.crews[1].members.iter().all(|m| m.badge.is_none()));
        assert!(matches!(
            harness.service.set_badge(Some("0123456789abcdef")).await,
            Err(SocialError::InvalidInput("crew tags"))
        ));
        assert!(matches!(
            harness
                .service
                .set_team_hidden("0123456789abcdef", true)
                .await,
            Err(SocialError::InvalidInput("crew tags"))
        ));
        assert!(matches!(
            harness
                .service
                .set_team_badge_requests("0123456789abcdef", true)
                .await,
            Err(SocialError::InvalidInput("tag requests"))
        ));
        assert!(matches!(
            harness
                .service
                .request_from_badge("fedcba9876543210", "223456789012345678")
                .await,
            Err(SocialError::InvalidInput("tag requests"))
        ));
        // A visibility change is left out of an edit; the rest goes.
        harness
            .service
            .edit_team(
                "0123456789abcdef",
                TeamEdit {
                    emblem: Some("crown".into()),
                    hidden: Some(true),
                    badge_requests: Some(false),
                    ..TeamEdit::default()
                },
            )
            .await
            .unwrap();
        // A report from a card goes the old way: by its code.
        harness
            .service
            .report(
                ReportTarget::Crew {
                    id: "fedcba9876543210".into(),
                    code: Some("EKP-WXYZ-1234".into()),
                    via: Some("223456789012345678".into()),
                },
                ReportReason::Name,
                None,
                false,
            )
            .await
            .unwrap();
        assert_eq!(
            *bodies.lock().unwrap(),
            vec![
                r#"{"emblem":"crown"}"#.to_owned(),
                r#"{"reason":"name","target":{"code":"WXYZ-1234","id":"fedcba9876543210","kind":"crew"}}"#
                    .to_owned(),
            ]
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

    /// What the player saw while a profile change was on its way (SOC-LAG).
    #[derive(Debug, Clone)]
    struct Seen {
        body: String,
        share_server: bool,
        share_last_seen: bool,
        avatar: String,
        crew_avatar: String,
    }

    /// SOC-LAG: a switch shows at once (before the service answers); changes go one at a time in
    /// the order they were made; a change a newer one makes again never goes; a refusal takes the
    /// change back, also on the player's own crew pages.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn profile_changes_show_at_once_go_in_order_and_a_refusal_goes_back() {
        let slot: Arc<Mutex<Option<SocialService>>> = Arc::new(Mutex::new(None));
        let seen = Arc::new(Mutex::new(Vec::<Seen>::new()));
        let in_flight = Arc::new(AtomicUsize::new(0));
        let most = Arc::new(AtomicUsize::new(0));
        let stored: Arc<Mutex<serde_json::Value>> =
            Arc::new(Mutex::new(serde_json::from_str(ME).unwrap()));
        let server = MockServer::start({
            let (slot, seen, in_flight, most, stored) = (
                slot.clone(),
                seen.clone(),
                in_flight.clone(),
                most.clone(),
                stored.clone(),
            );
            move |req| {
                presence_refused(&req.path).unwrap_or_else(|| {
                    match (req.method.as_str(), req.path.as_str()) {
                        ("GET", "/v1/me") => {
                            MockReply::json(200, &stored.lock().unwrap().to_string())
                        }
                        ("GET", "/v1/friends") => MockReply::json(200, &friends_with_crews()),
                        ("PATCH", "/v1/me") => {
                            let now = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                            most.fetch_max(now, Ordering::SeqCst);
                            let service = slot.lock().unwrap().clone().unwrap();
                            // The four clicks are made in one go, but the first request can
                            // arrive between two of them (the client writes it on a worker
                            // thread): look once the last click (last seen off) shows.
                            let until = std::time::Instant::now() + Duration::from_secs(5);
                            let mut view = service.view();
                            while view
                                .me
                                .as_ref()
                                .is_some_and(|me| me.privacy.share_last_seen)
                                && std::time::Instant::now() < until
                            {
                                std::thread::sleep(Duration::from_millis(2));
                                view = service.view();
                            }
                            let me = view.me.unwrap();
                            seen.lock().unwrap().push(Seen {
                                body: req.body_text(),
                                share_server: me.privacy.share_server,
                                share_last_seen: me.privacy.share_last_seen,
                                avatar: me.avatar.clone(),
                                crew_avatar: view
                                    .crews
                                    .iter()
                                    .flat_map(|crew| crew.members.iter())
                                    .find(|member| member.me)
                                    .map(|member| member.avatar.clone())
                                    .unwrap_or_default(),
                            });
                            std::thread::sleep(Duration::from_millis(60));
                            in_flight.fetch_sub(1, Ordering::SeqCst);
                            let body = req.json();
                            if body["avatar"] == "preset:owl" {
                                return MockReply::json(
                                    429,
                                    r#"{"error":{"code":"rate_limited","message":"x"}}"#,
                                )
                                .with_header("Retry-After", "60");
                            }
                            let mut me = stored.lock().unwrap();
                            for key in ["shareServer", "shareLastSeen"] {
                                if let Some(value) = body.get(key) {
                                    me["privacy"][key] = value.clone();
                                }
                            }
                            if let Some(avatar) = body.get("avatar") {
                                me["avatar"] = avatar.clone();
                            }
                            MockReply::json(200, &me.to_string())
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
        harness.service.set_teams_enabled(true);
        harness.service.configure(url(&server));
        harness
            .until("friends and crews", |view| {
                view.loaded && !view.crews.is_empty()
            })
            .await;

        // Rapid clicks on one switch, then another switch.
        let service = &harness.service;
        let (a, b, c, d) = tokio::join!(
            service.update_profile(ProfileUpdate::default().share_server(true)),
            service.update_profile(ProfileUpdate::default().share_server(false)),
            service.update_profile(ProfileUpdate::default().share_server(true)),
            service.update_profile(ProfileUpdate::default().share_last_seen(false)),
        );
        assert!(a.is_ok() && b.is_ok() && c.is_ok() && d.is_ok());
        assert_eq!(most.load(Ordering::SeqCst), 1, "one at a time");
        let first = seen.lock().unwrap().clone();
        assert_eq!(
            first.iter().map(|s| s.body.as_str()).collect::<Vec<_>>(),
            vec![
                r#"{"shareServer":true}"#,
                r#"{"shareServer":true}"#,
                r#"{"shareLastSeen":false}"#
            ],
            "the middle click never went"
        );
        // While the first was on its way the player saw every click already (the switch clicked
        // three times shows its last value).
        assert!(first[0].share_server && !first[0].share_last_seen);
        let me = harness.service.view().me.unwrap();
        assert!(me.privacy.share_server && !me.privacy.share_last_seen);

        // Refused: the picture shows while it is on its way, then the old one comes back.
        seen.lock().unwrap().clear();
        let refused = harness
            .service
            .update_profile(ProfileUpdate {
                avatar: Some("preset:owl".into()),
                ..ProfileUpdate::default()
            })
            .await;
        assert!(matches!(refused, Err(SocialError::RateLimited { .. })));
        let during = seen.lock().unwrap()[0].clone();
        assert_eq!(
            (during.avatar.as_str(), during.crew_avatar.as_str()),
            ("preset:owl", "preset:owl")
        );
        let view = harness.service.view();
        assert_eq!(view.me.as_ref().unwrap().avatar, "discord");
        let own = |view: &SocialView| {
            view.crews
                .iter()
                .flat_map(|crew| crew.members.iter())
                .filter(|member| member.me)
                .map(|member| member.avatar.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(own(&view), vec!["discord", "discord"]);
        // Taken: the answer keeps it, on the crew pages too.
        harness
            .service
            .update_profile(ProfileUpdate {
                avatar: Some("preset:fox".into()),
                ..ProfileUpdate::default()
            })
            .await
            .unwrap();
        let view = harness.service.view();
        assert_eq!(view.me.as_ref().unwrap().avatar, "preset:fox");
        assert_eq!(own(&view), vec!["preset:fox", "preset:fox"]);
    }

    /// SOC-LAG: "Sessize al" shows at once; a refusal puts the old settings back.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn crew_mutes_show_at_once_and_a_refusal_puts_them_back() {
        let slot: Arc<Mutex<Option<SocialService>>> = Arc::new(Mutex::new(None));
        let seen = Arc::new(Mutex::new(Vec::<bool>::new()));
        let server = MockServer::start({
            let (slot, seen) = (slot.clone(), seen.clone());
            move |req| {
                presence_refused(&req.path).unwrap_or_else(|| {
                    match (req.method.as_str(), req.path.as_str()) {
                        ("GET", "/v1/me") => MockReply::json(200, ME),
                        ("GET", "/v1/friends") => MockReply::json(200, &friends_with_crews()),
                        ("PATCH", "/v1/teams/notifications") => {
                            let view = slot.lock().unwrap().clone().unwrap().view();
                            seen.lock()
                                .unwrap()
                                .push(view.crew_notifications.is_some_and(|n| n.muted));
                            MockReply::json(
                                503,
                                r#"{"error":{"code":"unavailable","message":"x"}}"#,
                            )
                        }
                        ("PATCH", "/v1/teams/fedcba9876543210/notifications") => {
                            let view = slot.lock().unwrap().clone().unwrap().view();
                            seen.lock().unwrap().push(
                                view.crews
                                    .iter()
                                    .find(|crew| crew.id == "fedcba9876543210")
                                    .is_some_and(|crew| crew.notifications.muted),
                            );
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
        *slot.lock().unwrap() = Some(harness.service.clone());
        harness.service.set_teams_enabled(true);
        harness.service.configure(url(&server));
        harness
            .until("friends and crews", |view| {
                view.loaded && view.crew_notifications.is_some()
            })
            .await;
        let refused = harness
            .service
            .set_crew_notifications(TeamNotificationsEdit {
                muted: Some(true),
                ..TeamNotificationsEdit::default()
            })
            .await;
        assert!(refused.is_err());
        assert!(
            harness
                .service
                .view()
                .crew_notifications
                .is_some_and(|n| !n.muted),
            "back as it was"
        );
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
        assert_eq!(
            *seen.lock().unwrap(),
            vec![true, true],
            "shown while on their way"
        );
        assert!(
            harness
                .service
                .view()
                .crews
                .iter()
                .any(|crew| crew.id == "fedcba9876543210" && crew.notifications.muted)
        );
    }
}
