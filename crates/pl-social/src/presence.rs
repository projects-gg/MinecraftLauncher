//! Presence WebSocket client.
//!
//! - One background task per session; it reconnects by itself with exponential backoff and
//!   jitter (a healthy connection resets the backoff) and never retries in a tight loop.
//! - Status is sent only when it changes ([`PresenceHandle::set_status`]), and again after a
//!   reconnect when it is not "launcher" (the service starts every connection as "launcher").
//! - Keep-alive uses WebSocket protocol pings, which the service answers without waking up and
//!   which are not billed; a connection that stops answering is replaced.
//! - Close codes 4001 (session ended), 4002 (account deleted) and 4000 (replaced by a newer
//!   connection of the same account) stop the task: [`PresenceEvent::Stopped`].
//! - Chat (CHAT1, [`ChatSetup`]): asked for in the capabilities; after every connect the task
//!   syncs what changed since the device's newest message (continuing with the service's cursor
//!   at once), then everything arrives live. Sends, edits and deletes are answered by `rid`.
//! - Chat extras (CHAT2): replies, stickers, reactions, "yazıyor…" and read marks are asked for
//!   with chat, one capability each; a frame of one the service did not take never goes out (an
//!   older service would refuse it). The first sync after the device gained them reaches back a
//!   week once ([`ChatSetup::catch_up`]). Link cards are asked on the same socket.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tokio::time::{Instant, sleep_until, timeout};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::http::header::{AUTHORIZATION, RETRY_AFTER, USER_AGENT};
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::{CloseFrame, WebSocketConfig};
use tokio_tungstenite::tungstenite::{self, Message};
use tokio_tungstenite::{
    Connector, MaybeTlsStream, WebSocketStream, connect_async_tls_with_config,
};
use url::Url;

use crate::afk::AfkNotice;
use crate::auth::SessionToken;
use crate::backoff::{Backoff, BackoffPolicy, jitter};
use crate::error::{ErrorCode, Result, SocialError, parse_retry_after};
use crate::minecraft::McMessage;
use crate::model::{
    ChatBody, ChatCaps, ChatDeletion, ChatMessage, ChatQuota, ChatReacted, ChatRejection,
    ChatStatus, CrewBadge, CrewMute, FriendGroups, JoinRequest, LimitedKey, LinkCard, LinkRule,
    ModNotice, Privacy, ProfileMark, PublicProfile, Reaction, ReadMark, StatusMode, Team, TeamExit,
    TeamInvite, TeamNotifications,
};
use crate::protocol::{
    AFK_CAP, BADGES_CAP, CHAT_CAP, CallAnswer, CallEndReason, ClientMessage, CrewCall, CrewPlan,
    Invite, LocalStatus, MARKS_CAP, PLANS_CAP, PlanAnswer, Presence, ServerMessage, chat_caps,
};
use crate::tls;
use crate::world::{WORLDS_CAP, WorldAnswer, WorldError, WorldEvent, WorldFrame};

/// What a world request (HOST1) gets back.
type WorldResult = std::result::Result<WorldAnswer, WorldError>;

/// Chat on the presence socket (CHAT1). `since` is where the next sync starts (ms on the hub's
/// clock: the device's newest message minus a minute); the owner keeps it up to date.
/// `catch_up` (CHAT2): where the first sync with chat extras starts instead (the newest message
/// minus 7 days: stickers, reply links and reactions the device could not get before come then),
/// or a negative number once the device has caught up.
#[derive(Debug, Clone)]
pub struct ChatSetup {
    pub since: Arc<AtomicI64>,
    pub catch_up: Arc<AtomicI64>,
}

impl Default for ChatSetup {
    fn default() -> Self {
        Self::from_since(0)
    }
}

impl ChatSetup {
    /// Syncs from `since`, nothing to catch up.
    pub fn from_since(since: i64) -> Self {
        Self {
            since: Arc::new(AtomicI64::new(since)),
            catch_up: Arc::new(AtomicI64::new(-1)),
        }
    }
}

/// A link's card can take this long: the hub reads the page itself (3 s, up to 3 redirects).
const UNFURL_TIMEOUT: Duration = Duration::from_secs(15);

/// A refused sync waits at least this long before it is asked again (one a minute per socket).
const SYNC_RETRY_MIN: Duration = Duration::from_secs(5);
const SYNC_RETRY_MAX: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Copy)]
pub struct PresenceOptions {
    /// Protocol ping interval. Cloudflare closes idle sockets after about 100 s.
    pub keepalive: Duration,
    /// How long a ping may stay unanswered before the connection counts as dead.
    pub pong_timeout: Duration,
    pub backoff: BackoffPolicy,
    /// A connection that stayed up this long resets the backoff.
    pub stable_after: Duration,
    pub connect_timeout: Duration,
    /// How long [`PresenceHandle::invite`] (and a crew call request) waits for the service's
    /// answer.
    pub invite_timeout: Duration,
}

impl Default for PresenceOptions {
    fn default() -> Self {
        Self {
            keepalive: Duration::from_secs(45),
            pong_timeout: Duration::from_secs(20),
            backoff: BackoffPolicy::default(),
            stable_after: Duration::from_secs(60),
            connect_timeout: Duration::from_secs(15),
            invite_timeout: Duration::from_secs(10),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PresenceEvent {
    /// After every (re)connect: your status mode, the friends and teammates who are online right
    /// now (replace the whole online list), invites that waited for you (show them as a quiet
    /// list) and every open crew call you see (replace the list; show them without a
    /// notification). `clock` is the service's time (Unix seconds) for the calls' deadlines.
    Connected {
        status: StatusMode,
        friends: Vec<Presence>,
        invites: Vec<Invite>,
        calls: Vec<CrewCall>,
        clock: Option<i64>,
        /// The chat state, when chat was asked for and the service took it (None: no chat on
        /// this connection).
        chat: Option<ChatStatus>,
        /// Every plan of the player's crews they see, when the service took `plans` (keep
        /// exactly these); None: no plans on this connection.
        plans: Option<Vec<CrewPlan>>,
    },
    /// The waiting join requests of one of the player's crews they may answer for (replace
    /// them); `added` is the player who just asked.
    TeamRequests {
        crew: String,
        requests: Vec<JoinRequest>,
        added: Option<String>,
    },
    /// A crew plan, new or changed (answers): replaces the one with the same id. `quiet`: show it,
    /// never announce it.
    Plan {
        plan: CrewPlan,
        quiet: bool,
    },
    /// Everything the player sees of one crew's plans now (also none): replace that crew's.
    Plans {
        crew: String,
        plans: Vec<CrewPlan>,
    },
    /// A plan was cancelled (`by`: its maker, the crew's leader or a moderator).
    PlanRemoved {
        plan: String,
        crew: String,
        by: String,
    },
    /// The chat state changed: the service's switch, the mailbox, the daily limit or a mute.
    ChatState(ChatStatus),
    /// A new message in one of the player's conversations, their own too (from another device,
    /// or as the service stored this device's message).
    ChatMessage(ChatMessage),
    /// A message was edited: its new text and seal.
    ChatEdited(ChatMessage),
    /// A message was deleted for everyone (`by`: a user id, `staff`, or empty; `at`: the hub's
    /// clock in ms, 0 when not said).
    ChatDeleted {
        conv: String,
        id: i64,
        by: String,
        at: i64,
    },
    /// One conversation of a sync: messages sent or edited, and deletions, since the device's
    /// newest (each message carries its `conv`); with CHAT2's capabilities the messages whose
    /// reactions alone changed, and a direct conversation's read marks.
    ChatSync {
        conv: String,
        messages: Vec<ChatMessage>,
        deleted: Vec<ChatDeletion>,
        reacted: Vec<ChatReacted>,
        reads: Vec<ReadMark>,
    },
    /// The sync after a connect is over (`clock`: the hub's time in ms, when it said so).
    /// `caught_up`: it was the device's first with chat extras, reaching back a week (CHAT2).
    ChatSynced {
        clock: Option<i64>,
        caught_up: bool,
    },
    /// A reaction was given (`on`) or taken back, the player's own from another device too
    /// (CHAT2).
    ChatReacted {
        conv: String,
        id: i64,
        by: String,
        e: String,
        on: bool,
    },
    /// Someone writes in a conversation ("yazıyor…", CHAT2): shown for 6 seconds after the last.
    ChatTyping {
        conv: String,
        by: String,
    },
    /// A read mark moved (CHAT2): the player's own from another device, or in a direct
    /// conversation the friend's.
    ChatRead {
        conv: String,
        by: String,
        id: i64,
    },
    /// A one-time note from moderators.
    Notice(ModNotice),
    /// SKIN1: a job of the player's closed (`job`), or the link or the Projects skin changed
    /// elsewhere; `link`, `skin` and `blocked` are what the service knows now.
    Mc(Box<McMessage>),
    /// Cloud AFK's notice about one of the player's cloud accounts (AFKPLUS): say it, and send
    /// [`PresenceControl::afk_seen`] once the player saw it.
    AfkNotice(AfkNotice),
    /// A crew call, new or changed: replaces the one with the same id. `quiet`: show it but do
    /// not announce it (your own call, do-not-disturb, or a call that is only new to you).
    Call {
        call: CrewCall,
        quiet: bool,
        clock: Option<i64>,
    },
    /// Someone answered a call (you too, from another device).
    CallAnswered {
        call: String,
        by: String,
        answer: CallAnswer,
    },
    /// A call ended before its time (calls that run out end without a message).
    CallEnded {
        call: String,
        reason: CallEndReason,
    },
    /// A friend's or a teammate's presence changed.
    Presence(Presence),
    /// One of your crews changed (members, roles, name, code, its members' tags), or you are no
    /// longer in it (None, with the reason; `by` is who made the change). `crew` is its id (None
    /// from a service with one crew per player: the message is about that crew). `joined`: who
    /// just came in, by code, link, invite or an accepted request (TEAM6: "Hoş geldin"; empty from
    /// an older service and when the crew was founded).
    TeamChanged {
        team: Option<Team>,
        crew: Option<String>,
        reason: Option<TeamExit>,
        by: Option<String>,
        joined: Vec<String>,
    },
    /// The crew tag a friend or crewmate wears, or the player's own from another device, changed
    /// (TEAM6; only on a connection the service took `badges` on): their card everywhere, None
    /// when they wear none now.
    BadgeChanged {
        id: String,
        badge: Option<CrewBadge>,
    },
    /// The mark of a friend, a crewmate or the player changed (SOC-VERIFY; only on a connection
    /// the service took `marks` on): their mark everywhere, None when they have none now.
    MarkChanged {
        id: String,
        mark: Option<ProfileMark>,
    },
    /// A friend invited you to their team.
    TeamInvited(TeamInvite),
    /// Your friend categories changed on another device.
    GroupsChanged(FriendGroups),
    /// Your crew notification settings for all crews changed (on any of your devices).
    CrewNotifications(TeamNotifications),
    /// Your mute of one crew changed (on any of your devices).
    CrewMuted {
        crew: String,
        notifications: CrewMute,
    },
    /// You put your crews in another order on another device (the first is the primary crew).
    CrewOrder(Vec<String>),
    /// A request was accepted (by either side): a new friend, with their current presence.
    FriendAdded {
        friend: PublicProfile,
        presence: Presence,
    },
    /// Unfriended, blocked or deleted: drop them from the list (no reason is given).
    FriendRemoved {
        id: String,
    },
    RequestReceived {
        from: PublicProfile,
    },
    RequestCancelled {
        id: String,
    },
    /// "Birlikte oyna", live.
    Invite(Invite),
    /// Invites that waited while you were on do-not-disturb.
    QueuedInvites(Vec<Invite>),
    /// Your status mode changed on another device.
    StatusChanged(StatusMode),
    /// Your privacy switches or picture changed, on one of your devices (SOC-LAG; only on a
    /// connection the service took `profile` on): what they are now.
    ProfileChanged {
        privacy: Privacy,
        avatar: Option<String>,
        avatar_hash: Option<String>,
    },
    /// The connection dropped; the task reconnects after `retry_in`.
    Disconnected {
        retry_in: Duration,
    },
    /// The task ended and will not reconnect.
    Stopped(StopReason),
    /// World sessions (HOST1): after every connect whether the service took them, then their
    /// frames (only on a connection that took `worlds2`).
    World(WorldEvent),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// The session is no longer valid (signed out, revoked, expired): sign in again.
    SignedOut,
    /// The account was deleted.
    AccountDeleted,
    /// A newer connection of the same account replaced this one (too many devices).
    Replaced,
    /// [`PresenceHandle::shutdown`] or the handle was dropped.
    Shutdown,
    /// Moderators suspended (until `until`, Unix seconds) or banned (None) the account: every
    /// session ended.
    Restricted { until: Option<i64> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InviteOutcome {
    /// The friend saw it right away.
    Delivered,
    /// The friend is offline or on do-not-disturb; it waits for up to an hour.
    Queued,
}

/// What a crew call request asks of the service ("Ekibi çağır", an answer, "Bitir").
#[derive(Debug, Clone, PartialEq, Eq)]
enum CrewRequest {
    Call { crews: Option<Vec<String>> },
    Answer { call: String, answer: CallAnswer },
    End { call: String },
}

/// What a crew plan request asks of the service (TEAM5; the note FIX2).
#[derive(Debug, Clone, PartialEq, Eq)]
enum PlanRequest {
    Make {
        crew: String,
        at: i64,
        note: Option<String>,
    },
    Answer {
        plan: String,
        answer: PlanAnswer,
    },
    Cancel {
        plan: String,
    },
    Note {
        crew: String,
        plan: String,
        note: String,
    },
}

/// What a chat request asks of the service.
#[derive(Debug, Clone, PartialEq)]
enum ChatRequest {
    Send {
        conv: String,
        body: ChatBody,
        /// The message answered, as held (CHAT2).
        reply: Option<serde_json::Value>,
        nonce: String,
    },
    Edit {
        msg: serde_json::Value,
        text: String,
    },
    Delete {
        msg: serde_json::Value,
    },
    /// CHAT2.
    React {
        conv: String,
        id: i64,
        e: String,
        on: bool,
    },
    /// CHAT2.
    Unfurl {
        url: String,
    },
}

impl ChatRequest {
    /// The capability the request needs on this connection (CHAT2), when it needs one.
    fn needs(&self, caps: ChatCaps) -> Option<&'static str> {
        match self {
            Self::Send {
                body: ChatBody::Sticker(_),
                ..
            } if !caps.stickers => Some("stickers"),
            Self::Send { reply: Some(_), .. } if !caps.replies => Some("replies"),
            Self::React { .. } if !caps.reactions => Some("reactions"),
            _ => None,
        }
    }
}

/// What the service answered a chat request with: the message as stored (a send, an edit), a
/// message's reactions (a reaction), a link's card.
#[derive(Debug, Default)]
struct ChatAnswer {
    msg: Option<ChatMessage>,
    reactions: Option<Vec<Reaction>>,
    card: Option<LinkCard>,
}

/// A chat frame that is never answered (CHAT2).
#[derive(Debug, Clone, PartialEq, Eq)]
enum Signal {
    Typing {
        conv: String,
    },
    Read {
        conv: String,
        id: i64,
    },
    /// The player saw an AFK notice (AFKPLUS).
    AfkSeen {
        id: String,
    },
}

enum Command {
    Invite {
        to: String,
        reply: oneshot::Sender<Result<InviteOutcome>>,
    },
    Crew {
        request: CrewRequest,
        reply: oneshot::Sender<Result<()>>,
    },
    Chat {
        request: ChatRequest,
        reply: oneshot::Sender<Result<ChatAnswer>>,
    },
    Signal(Signal),
    Plan {
        request: PlanRequest,
        reply: oneshot::Sender<Result<Option<CrewPlan>>>,
    },
    /// A world frame (HOST1); `reply` for one the service answers by `rid`.
    World {
        frame: WorldFrame,
        reply: Option<oneshot::Sender<WorldResult>>,
    },
    Shutdown,
}

impl Command {
    /// The service cannot be reached for a while: the request fails at once.
    fn refuse(self, retry_after: Option<Duration>) {
        let error = || SocialError::Unavailable { retry_after };
        match self {
            Self::Invite { reply, .. } => drop(reply.send(Err(error()))),
            Self::Crew { reply, .. } => drop(reply.send(Err(error()))),
            Self::Chat { reply, .. } => drop(reply.send(Err(error()))),
            Self::Plan { reply, .. } => drop(reply.send(Err(error()))),
            Self::World {
                reply: Some(reply), ..
            } => drop(reply.send(Err(WorldError::Unavailable))),
            Self::Signal(_) | Self::World { .. } | Self::Shutdown => {}
        }
    }
}

/// What this launcher tells the service it is doing: the status and (SOC9) the Minecraft version
/// the game plays, which goes along only while playing.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Outgoing {
    status: LocalStatus,
    version: Option<String>,
}

impl Outgoing {
    /// As the service gets it: no version outside a game.
    fn on_wire(&self) -> Self {
        let playing = !matches!(self.status, LocalStatus::Launcher);
        Self {
            status: self.status.clone(),
            version: self.version.clone().filter(|_| playing),
        }
    }
}

/// A version the service shows (`1.21.4`, `26.2`, `25w14a`): 1–24 of `0-9 A-Z a-z . _ + -`;
/// anything else is not sent (the service would leave it out anyway).
pub fn game_version(version: &str) -> Option<String> {
    let fits = !version.is_empty()
        && version.len() <= 24
        && version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'+' | b'-'));
    fits.then(|| version.to_owned())
}

/// Controls the background presence task: status, invites and crew calls. Cheap to clone; once
/// every control is gone the task closes the connection cleanly and stops.
#[derive(Clone)]
pub struct PresenceControl {
    status: Arc<watch::Sender<Outgoing>>,
    commands: mpsc::Sender<Command>,
    invite_timeout: Duration,
}

impl std::fmt::Debug for PresenceControl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PresenceControl").finish_non_exhaustive()
    }
}

impl PresenceControl {
    /// Sets what this launcher is doing. Only a real change is sent; returns whether it changed.
    pub fn set_status(&self, status: LocalStatus) -> bool {
        self.status.send_if_modified(|current| {
            if current.status == status {
                false
            } else {
                current.status = status;
                true
            }
        })
    }

    /// SOC9: the Minecraft version the game plays (a vanilla id, checked by [`game_version`]),
    /// sent with the status while playing; None when no game runs or it is not known.
    pub fn set_game_version(&self, version: Option<String>) -> bool {
        let version = version.as_deref().and_then(game_version);
        self.status.send_if_modified(|current| {
            if current.version == version {
                false
            } else {
                current.version = version;
                true
            }
        })
    }

    pub fn status(&self) -> LocalStatus {
        self.status.borrow().status.clone()
    }

    /// Sends a "Birlikte oyna" invite to a friend (limits: one per friend per 5 minutes, 30 a
    /// day). Fails with `Unavailable` while disconnected.
    pub async fn invite(&self, friend_id: &str) -> Result<InviteOutcome> {
        let (reply, answer) = oneshot::channel();
        self.commands
            .send(Command::Invite {
                to: friend_id.to_owned(),
                reply,
            })
            .await
            .map_err(|_| SocialError::Unavailable { retry_after: None })?;
        match timeout(self.invite_timeout, answer).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(SocialError::Network("presence connection closed".into())),
            Err(_) => Err(SocialError::Network("no answer from the service".into())),
        }
    }

    /// "Ekibi çağır": `crews` names the crews it calls (a service with several crews), None the
    /// primary one. Limits: only from a multiplayer server, one open call per player and per crew
    /// among teammates who see each other, one every 5 minutes and 10 a day per player, however
    /// many crews it names. The call itself arrives as [`PresenceEvent::Call`] before this
    /// returns. Fails with `RateLimited` (with the wait), `Refused` (`call_needs_server`,
    /// `call_server_hidden`, `call_active`, `team_not_found`, `team_not_permitted`), or
    /// `Unavailable` while disconnected.
    pub async fn call_crew(&self, crews: Option<Vec<String>>) -> Result<()> {
        self.crew(CrewRequest::Call { crews }).await
    }

    /// Answers (or changes the answer to) a teammate's call. `Refused` `call_not_found` when the
    /// call is gone.
    pub async fn answer_call(&self, call: &str, answer: CallAnswer) -> Result<()> {
        self.crew(CrewRequest::Answer {
            call: call.to_owned(),
            answer,
        })
        .await
    }

    /// Ends the player's own call ("Bitir").
    pub async fn end_call(&self, call: &str) -> Result<()> {
        self.crew(CrewRequest::End {
            call: call.to_owned(),
        })
        .await
    }

    async fn crew(&self, request: CrewRequest) -> Result<()> {
        let (reply, answer) = oneshot::channel();
        self.commands
            .send(Command::Crew { request, reply })
            .await
            .map_err(|_| SocialError::Unavailable { retry_after: None })?;
        match timeout(self.invite_timeout, answer).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(SocialError::Network("presence connection closed".into())),
            Err(_) => Err(SocialError::Network("no answer from the service".into())),
        }
    }

    /// Sends a chat message (CHAT1). `nonce` (1-16 of `A-Z a-z 0-9 _ -`) is this device's key for
    /// it: sending the same again after a lost answer gives the message already taken. Returns
    /// the message as the service stored it; it also arrives as [`PresenceEvent::ChatMessage`].
    /// Fails with `ChatRejected`, `ChatMuted`, `RateLimited`, `Refused` (`chat_off`,
    /// `friend_not_found`, `team_not_found`, `bad_request`) or `Unavailable`/`Network` while the
    /// connection is down or the answer was lost.
    pub async fn chat_send(&self, conv: &str, text: &str, nonce: &str) -> Result<ChatMessage> {
        self.chat_post(conv, &ChatBody::Text(text.to_owned()), None, nonce)
            .await
    }

    /// Sends a text or a sticker (CHAT2), maybe as a reply to `reply` (a message of the same
    /// conversation, as held: its seals). Like [`PresenceControl::chat_send`]; without the
    /// connection's `stickers` or `replies` it fails at once with `Refused` `chat_feature_off`
    /// (the refusal's detail names the extra).
    pub async fn chat_post(
        &self,
        conv: &str,
        body: &ChatBody,
        reply: Option<&ChatMessage>,
        nonce: &str,
    ) -> Result<ChatMessage> {
        self.chat(ChatRequest::Send {
            conv: conv.to_owned(),
            body: body.clone(),
            reply: reply.map(|message| serde_json::json!({ "msg": message.sealed() })),
            nonce: nonce.to_owned(),
        })
        .await?
        .msg
        .ok_or_else(|| SocialError::Protocol("a chat answer without its message".into()))
    }

    /// Edits the player's own message (within 15 minutes), as held: its new version comes back.
    pub async fn chat_edit(&self, message: &ChatMessage, text: &str) -> Result<ChatMessage> {
        self.chat(ChatRequest::Edit {
            msg: message.sealed(),
            text: text.to_owned(),
        })
        .await?
        .msg
        .ok_or_else(|| SocialError::Protocol("a chat answer without its message".into()))
    }

    /// "Herkesten sil": the player's own message (7 days), or any of a crew they lead or
    /// moderate. The other devices and people get [`PresenceEvent::ChatDeleted`]; this one only
    /// the answer.
    pub async fn chat_delete(&self, message: &ChatMessage) -> Result<()> {
        self.chat(ChatRequest::Delete {
            msg: message.sealed(),
        })
        .await
        .map(drop)
    }

    /// Gives (`on`) or takes back the player's reaction `e` on message `id` (CHAT2); answers the
    /// message's reactions as the player sees them. Everyone else in the conversation, the
    /// player's other devices too, get [`PresenceEvent::ChatReacted`]. Fails with `Refused`
    /// (`chat_reactions_full` with `mine` or `kinds`, `message_not_found`, `chat_feature_off`),
    /// `ChatRejected` (`words`: 🖕), `ChatMuted` or `RateLimited` (the day's reactions: item).
    pub async fn chat_react(
        &self,
        conv: &str,
        id: i64,
        e: &str,
        on: bool,
    ) -> Result<Vec<Reaction>> {
        Ok(self
            .chat(ChatRequest::React {
                conv: conv.to_owned(),
                id,
                e: e.to_owned(),
                on,
            })
            .await?
            .reactions
            .unwrap_or_default())
    }

    /// The card of a link (CHAT2), made by the hub (this computer never fetches the site). Fails
    /// with `Refused` `unfurl_failed` (nothing to show; with the link rule as detail when the link
    /// leads somewhere the rules refuse) or `chat_feature_off`, `Unavailable` (the hub is busy:
    /// ask again after the wait), `RateLimited` (the day's cards).
    pub async fn chat_unfurl(&self, url: &str) -> Result<LinkCard> {
        self.request(
            ChatRequest::Unfurl {
                url: url.to_owned(),
            },
            self.invite_timeout.max(UNFURL_TIMEOUT),
        )
        .await?
        .card
        .ok_or_else(|| SocialError::Protocol("a card answer without its card".into()))
    }

    /// A world frame (HOST1, `crate::world`) the service answers by `rid`: its `ack`, or its
    /// `error` as [`WorldError::Refused`]. Fails at once with [`WorldError::Off`] on a connection
    /// that did not take `worlds2`, with [`WorldError::Unavailable`] while disconnected or when no
    /// answer came within `wait`.
    pub async fn world_request(&self, frame: WorldFrame, wait: Duration) -> WorldResult {
        let (reply, answer) = oneshot::channel();
        self.commands
            .send(Command::World {
                frame,
                reply: Some(reply),
            })
            .await
            .map_err(|_| WorldError::Unavailable)?;
        match timeout(wait, answer).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) | Err(_) => Err(WorldError::Unavailable),
        }
    }

    /// A world frame nobody answers (HOST1: counts, path answers, credentials let go, telemetry):
    /// dropped while disconnected or on a connection that did not take `worlds2`.
    pub fn world_send(&self, frame: WorldFrame) {
        let command = Command::World { frame, reply: None };
        if let Err(mpsc::error::TrySendError::Full(command)) = self.commands.try_send(command)
            && let Ok(runtime) = tokio::runtime::Handle::try_current()
        {
            // A burst (a reconnect's claims and counts): it waits its turn instead of going.
            let commands = self.commands.clone();
            runtime.spawn(async move {
                let _ = commands.send(command).await;
            });
        }
    }

    /// "Yazıyor…" in a conversation (CHAT2): never answered, dropped while the connection does
    /// not take it. The caller keeps to one every 4 seconds.
    pub fn chat_typing(&self, conv: &str) {
        let _ = self.commands.try_send(Command::Signal(Signal::Typing {
            conv: conv.to_owned(),
        }));
    }

    /// The player saw Cloud AFK's notice `id` (AFKPLUS): its window in front, or the in-game
    /// overlay showed it. Never answered; dropped while the connection does not take `afk`.
    pub fn afk_seen(&self, id: &str) {
        let _ = self
            .commands
            .try_send(Command::Signal(Signal::AfkSeen { id: id.to_owned() }));
    }

    /// The player has read a conversation up to message `id` (CHAT2): never answered, dropped
    /// while the connection does not take it. The caller sends only forward.
    pub fn chat_read(&self, conv: &str, id: i64) {
        let _ = self.commands.try_send(Command::Signal(Signal::Read {
            conv: conv.to_owned(),
            id,
        }));
    }

    async fn chat(&self, request: ChatRequest) -> Result<ChatAnswer> {
        self.request(request, self.invite_timeout).await
    }

    async fn request(&self, request: ChatRequest, wait: Duration) -> Result<ChatAnswer> {
        let (reply, answer) = oneshot::channel();
        self.commands
            .send(Command::Chat { request, reply })
            .await
            .map_err(|_| SocialError::Unavailable { retry_after: None })?;
        match timeout(wait, answer).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(SocialError::Network("presence connection closed".into())),
            Err(_) => Err(SocialError::Network("no answer from the service".into())),
        }
    }

    /// A new crew plan (TEAM5): `at` in Unix seconds on the service's clock, a whole quarter of an
    /// hour, later than now and at most 7 days ahead. Returns the plan as the service stored it
    /// (the maker is coming); the crewmates who see the player get it too. Limits: 3 upcoming
    /// plans per crew, one per crew and time, 5 a day per player, the right to call the crew.
    /// Fails with `Refused` (`plan_time`, `limit_team_plans`, `plan_exists`, `team_not_found`,
    /// `team_not_permitted`), `RateLimited`, `Limited` (`crew_plans`) or `Unavailable`. A note
    /// (only to a service with `planNotes`) may be turned away: `PlanNoteRejected`, `ChatMuted`.
    pub async fn make_plan(&self, crew: &str, at: i64, note: Option<&str>) -> Result<CrewPlan> {
        self.plan(PlanRequest::Make {
            crew: crew.to_owned(),
            at,
            note: note.map(str::to_owned),
        })
        .await?
        .ok_or_else(|| SocialError::Protocol("a plan answer without its plan".into()))
    }

    /// A plan's new note, or `""` to clear it (FIX2): its maker sets, changes or clears it; the
    /// crew's leader and moderators may only clear it. Returns the plan as the service has it now.
    /// Fails with `PlanNoteRejected`, `ChatMuted`, `Limited` (`crew_plans`), `RateLimited` or
    /// `Refused` (`plan_not_found`, `team_not_permitted`, `team_not_found`).
    pub async fn set_plan_note(&self, crew: &str, plan: &str, note: &str) -> Result<CrewPlan> {
        self.plan(PlanRequest::Note {
            crew: crew.to_owned(),
            plan: plan.to_owned(),
            note: note.to_owned(),
        })
        .await?
        .ok_or_else(|| SocialError::Protocol("a plan answer without its plan".into()))
    }

    /// "Geliyorum", "Belki" or "Gelemem" to a plan; a new answer replaces the old one. Changing
    /// an answer counts (20 a day): `RateLimited` keeps the old one. `Refused` `plan_not_found`
    /// when the plan is gone or over.
    pub async fn answer_plan(&self, plan: &str, answer: PlanAnswer) -> Result<()> {
        self.plan(PlanRequest::Answer {
            plan: plan.to_owned(),
            answer,
        })
        .await
        .map(drop)
    }

    /// "Planı sil": the plan's maker, or the crew's leader or a moderator.
    pub async fn cancel_plan(&self, plan: &str) -> Result<()> {
        self.plan(PlanRequest::Cancel {
            plan: plan.to_owned(),
        })
        .await
        .map(drop)
    }

    async fn plan(&self, request: PlanRequest) -> Result<Option<CrewPlan>> {
        let (reply, answer) = oneshot::channel();
        self.commands
            .send(Command::Plan { request, reply })
            .await
            .map_err(|_| SocialError::Unavailable { retry_after: None })?;
        match timeout(self.invite_timeout, answer).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(SocialError::Network("presence connection closed".into())),
            Err(_) => Err(SocialError::Network("no answer from the service".into())),
        }
    }

    /// Asks the task to close the connection cleanly and stop.
    pub async fn shutdown(&self) {
        let _ = self.commands.send(Command::Shutdown).await;
    }
}

/// The events of the background presence task. Dropping it stops the task.
pub struct PresenceEvents {
    events: mpsc::UnboundedReceiver<PresenceEvent>,
    task: Option<JoinHandle<()>>,
}

impl PresenceEvents {
    /// The next event; None after the task has stopped and every event was read.
    pub async fn next_event(&mut self) -> Option<PresenceEvent> {
        self.events.recv().await
    }

    pub fn try_next_event(&mut self) -> Option<PresenceEvent> {
        self.events.try_recv().ok()
    }

    /// Waits until the task has ended.
    async fn finished(mut self) {
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }
}

impl Drop for PresenceEvents {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

/// Controls the background presence task and reads its events. Dropping it stops the task;
/// [`PresenceHandle::split`] hands the two halves to different owners.
pub struct PresenceHandle {
    control: PresenceControl,
    events: PresenceEvents,
}

impl PresenceHandle {
    pub(crate) fn spawn(
        url: Url,
        token: SessionToken,
        user_agent: String,
        options: PresenceOptions,
        chat: Option<ChatSetup>,
    ) -> Self {
        let (status, status_rx) = watch::channel(Outgoing::default());
        let (commands, commands_rx) = mpsc::channel(32);
        let (events_tx, events) = mpsc::unbounded_channel();
        let endpoint = Endpoint {
            url,
            token,
            user_agent,
            connect_timeout: options.connect_timeout,
            chat,
        };
        let task = tokio::spawn(run(endpoint, status_rx, commands_rx, events_tx, options));
        Self {
            control: PresenceControl {
                status: Arc::new(status),
                commands,
                invite_timeout: options.invite_timeout,
            },
            events: PresenceEvents {
                events,
                task: Some(task),
            },
        }
    }

    /// Sets what this launcher is doing. Only a real change is sent; returns whether it changed.
    pub fn set_status(&self, status: LocalStatus) -> bool {
        self.control.set_status(status)
    }

    /// SOC9: the Minecraft version the game plays, sent while playing (see
    /// [`PresenceControl::set_game_version`]).
    pub fn set_game_version(&self, version: Option<String>) -> bool {
        self.control.set_game_version(version)
    }

    pub fn status(&self) -> LocalStatus {
        self.control.status()
    }

    /// Sends a "Birlikte oyna" invite to a friend (limits: one per friend per 5 minutes, 30 a
    /// day). Fails with `Unavailable` while disconnected.
    pub async fn invite(&self, friend_id: &str) -> Result<InviteOutcome> {
        self.control.invite(friend_id).await
    }

    /// "Ekibi çağır"; see [`PresenceControl::call_crew`].
    pub async fn call_crew(&self, crews: Option<Vec<String>>) -> Result<()> {
        self.control.call_crew(crews).await
    }

    /// See [`PresenceControl::answer_call`].
    pub async fn answer_call(&self, call: &str, answer: CallAnswer) -> Result<()> {
        self.control.answer_call(call, answer).await
    }

    /// See [`PresenceControl::end_call`].
    pub async fn end_call(&self, call: &str) -> Result<()> {
        self.control.end_call(call).await
    }

    /// The next event; None after the task has stopped and every event was read.
    pub async fn next_event(&mut self) -> Option<PresenceEvent> {
        self.events.next_event().await
    }

    pub fn try_next_event(&mut self) -> Option<PresenceEvent> {
        self.events.try_next_event()
    }

    /// A control for status and invites that can live elsewhere (the events stay here).
    pub fn control(&self) -> PresenceControl {
        self.control.clone()
    }

    /// The control half (status, invites) and the event half, for separate owners.
    pub fn split(self) -> (PresenceControl, PresenceEvents) {
        (self.control, self.events)
    }

    /// Closes the connection cleanly and waits for the task.
    pub async fn shutdown(self) {
        self.control.shutdown().await;
        self.events.finished().await;
    }
}

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// How a connection attempt or a connection ended.
#[derive(Debug, PartialEq)]
enum Outcome {
    Retry { at_least: Duration },
    Stop(StopReason),
}

const RETRY_NOW: Outcome = Outcome::Retry {
    at_least: Duration::ZERO,
};

struct Endpoint {
    url: Url,
    token: SessionToken,
    user_agent: String,
    connect_timeout: Duration,
    chat: Option<ChatSetup>,
}

impl Endpoint {
    async fn connect(&self) -> std::result::Result<Socket, Outcome> {
        let mut request = self
            .url
            .as_str()
            .into_client_request()
            .map_err(|_| Outcome::Stop(StopReason::Shutdown))?;
        let headers = request.headers_mut();
        let bearer = HeaderValue::from_str(&format!("Bearer {}", self.token.expose()))
            .map_err(|_| Outcome::Stop(StopReason::SignedOut))?;
        headers.insert(AUTHORIZATION, bearer);
        if let Ok(agent) = HeaderValue::from_str(&self.user_agent) {
            headers.insert(USER_AGENT, agent);
        }
        let connector = if self.url.scheme() == "wss" {
            match tls::client_config() {
                Ok(config) => Connector::Rustls(config),
                Err(error) => {
                    tracing::warn!(%error, "presence TLS setup failed");
                    return Err(Outcome::Retry {
                        at_least: Duration::from_secs(300),
                    });
                }
            }
        } else {
            Connector::Plain
        };
        let config = WebSocketConfig::default()
            .max_message_size(Some(1 << 20))
            .max_frame_size(Some(1 << 20));
        let attempt = connect_async_tls_with_config(request, Some(config), true, Some(connector));
        match timeout(self.connect_timeout, attempt).await {
            Ok(Ok((socket, _))) => Ok(socket),
            Ok(Err(tungstenite::Error::Http(response))) => {
                let retry_after = parse_retry_after(
                    response
                        .headers()
                        .get(RETRY_AFTER)
                        .and_then(|value| value.to_str().ok()),
                );
                Err(handshake_outcome(response.status().as_u16(), retry_after))
            }
            Ok(Err(error)) => {
                tracing::debug!(%error, "presence connect failed");
                Err(RETRY_NOW)
            }
            Err(_) => Err(RETRY_NOW),
        }
    }
}

fn handshake_outcome(status: u16, retry_after: Option<Duration>) -> Outcome {
    match status {
        401 => Outcome::Stop(StopReason::SignedOut),
        // Rate limited or a daily limit reached: wait as long as asked (at most an hour).
        429 | 503 => Outcome::Retry {
            at_least: retry_after
                .unwrap_or(Duration::from_secs(60))
                .min(Duration::from_secs(3_600)),
        },
        500..=599 => RETRY_NOW,
        // Something is misconfigured; do not hammer the service.
        _ => Outcome::Retry {
            at_least: Duration::from_secs(300),
        },
    }
}

/// How a close ends the task. `restricted`: the `bye` before it said the account is restricted
/// (until when; None for good).
fn close_outcome(frame: Option<&CloseFrame>, restricted: Option<Option<i64>>) -> Outcome {
    match frame.map(|frame| u16::from(frame.code)) {
        Some(4001) => match restricted {
            Some(until) => Outcome::Stop(StopReason::Restricted { until }),
            None => Outcome::Stop(StopReason::SignedOut),
        },
        Some(4002) => Outcome::Stop(StopReason::AccountDeleted),
        Some(4000) => Outcome::Stop(StopReason::Replaced),
        Some(4008) => Outcome::Retry {
            at_least: Duration::from_secs(60),
        },
        Some(4003) => Outcome::Retry {
            at_least: Duration::from_secs(30),
        },
        _ => RETRY_NOW,
    }
}

async fn run(
    endpoint: Endpoint,
    mut status: watch::Receiver<Outgoing>,
    mut commands: mpsc::Receiver<Command>,
    events: mpsc::UnboundedSender<PresenceEvent>,
    options: PresenceOptions,
) {
    let mut backoff = Backoff::new(options.backoff);
    loop {
        let started = Instant::now();
        let outcome = match endpoint.connect().await {
            Ok(socket) => {
                session(
                    socket,
                    &mut status,
                    &mut commands,
                    &events,
                    &options,
                    endpoint.chat.as_ref(),
                )
                .await
            }
            Err(outcome) => outcome,
        };
        let wait = match outcome {
            Outcome::Stop(reason) => {
                let _ = events.send(PresenceEvent::Stopped(reason));
                return;
            }
            Outcome::Retry { at_least } => {
                if started.elapsed() >= options.stable_after {
                    backoff.reset();
                }
                backoff.next_delay().max(at_least)
            }
        };
        if events
            .send(PresenceEvent::Disconnected { retry_in: wait })
            .is_err()
        {
            return;
        }
        let deadline = Instant::now() + wait;
        loop {
            tokio::select! {
                () = sleep_until(deadline) => break,
                command = commands.recv() => match command {
                    None | Some(Command::Shutdown) => {
                        let _ = events.send(PresenceEvent::Stopped(StopReason::Shutdown));
                        return;
                    }
                    Some(command) => {
                        command.refuse(Some(deadline.saturating_duration_since(Instant::now())));
                    }
                },
            }
        }
    }
}

type Sink = SplitSink<Socket, Message>;

/// Who waits for the service's `ack` (or `error`) of a request, by its `rid`.
enum Reply {
    Invite(oneshot::Sender<Result<InviteOutcome>>),
    Crew(oneshot::Sender<Result<()>>),
    Chat(oneshot::Sender<Result<ChatAnswer>>),
    Plan(oneshot::Sender<Result<Option<CrewPlan>>>),
    /// HOST1.
    World(oneshot::Sender<WorldResult>),
}

impl Reply {
    fn fail(self, error: SocialError) {
        match self {
            Self::Invite(reply) => drop(reply.send(Err(error))),
            Self::Crew(reply) => drop(reply.send(Err(error))),
            Self::Chat(reply) => drop(reply.send(Err(error))),
            Self::Plan(reply) => drop(reply.send(Err(error))),
            Self::World(reply) => drop(reply.send(Err(WorldError::Unavailable))),
        }
    }
}

type Pending = HashMap<u32, Reply>;

/// One connection, from the handshake to its end.
async fn session(
    socket: Socket,
    status: &mut watch::Receiver<Outgoing>,
    commands: &mut mpsc::Receiver<Command>,
    events: &mpsc::UnboundedSender<PresenceEvent>,
    options: &PresenceOptions,
    chat: Option<&ChatSetup>,
) -> Outcome {
    let (mut sink, mut stream): (Sink, SplitStream<Socket>) = socket.split();
    let mut pending: Pending = HashMap::new();
    let mut state = SessionState {
        greeted: false,
        sent: None,
        next_rid: 0,
        keepalive: options.keepalive,
        chat: chat.cloned(),
        chat_on: false,
        chat_caps: ChatCaps::default(),
        plans_on: false,
        badges_on: false,
        marks_on: false,
        worlds_on: false,
        afk_on: false,
        sync: None,
        sync_again: None,
        restricted: None,
    };
    let mut last_inbound = Instant::now();
    let mut next_ping = Instant::now() + spread(state.keepalive);
    let mut ping_sent: Option<Instant> = None;

    let outcome = loop {
        let pong_deadline = ping_sent.map(|at| at + options.pong_timeout);
        let sync_again = state.sync_again;
        tokio::select! {
            frame = stream.next() => match frame {
                Some(Ok(Message::Text(text))) => {
                    last_inbound = Instant::now();
                    ping_sent = None;
                    if let Err(outcome) =
                        on_text(text.as_str(), &mut state, &mut sink, status, events, &mut pending).await
                    {
                        break outcome;
                    }
                }
                Some(Ok(Message::Close(frame))) => {
                    break close_outcome(frame.as_ref(), state.restricted);
                }
                Some(Ok(_)) => {
                    last_inbound = Instant::now();
                    ping_sent = None;
                }
                Some(Err(error)) => {
                    tracing::debug!(%error, "presence connection error");
                    break RETRY_NOW;
                }
                None => break RETRY_NOW,
            },
            changed = status.changed(), if state.greeted => {
                if changed.is_err() {
                    break Outcome::Stop(StopReason::Shutdown);
                }
                if send_status(&mut state, &mut sink, status).await.is_err() {
                    break RETRY_NOW;
                }
            }
            command = commands.recv() => match command {
                None | Some(Command::Shutdown) => {
                    let _ = sink
                        .send(Message::Close(Some(CloseFrame {
                            code: CloseCode::Normal,
                            reason: "bye".into(),
                        })))
                        .await;
                    break Outcome::Stop(StopReason::Shutdown);
                }
                Some(command) => {
                    if !state.greeted {
                        command.refuse(None);
                        continue;
                    }
                    let rid = state.rid();
                    let (message, reply) = match command {
                        Command::Invite { to, reply } => {
                            (ClientMessage::Invite { to, rid }, Reply::Invite(reply))
                        }
                        Command::Crew { request, reply } => {
                            (crew_message(request, rid), Reply::Crew(reply))
                        }
                        Command::Chat { request, reply } => {
                            if !state.chat_on {
                                // No chat on this connection (an older service, or not asked for).
                                let _ = reply.send(Err(chat_off()));
                                continue;
                            }
                            // An extra this connection did not take: an older service would
                            // refuse its frame (CHAT2).
                            if let Some(extra) = request.needs(state.chat_caps) {
                                let _ = reply.send(Err(feature_off(extra)));
                                continue;
                            }
                            (chat_message(request, rid), Reply::Chat(reply))
                        }
                        Command::Signal(signal) => {
                            let frame = match signal {
                                Signal::Typing { conv } if state.chat_on && state.chat_caps.typing => {
                                    ClientMessage::ChatTyping { conv }
                                }
                                Signal::Read { conv, id } if state.chat_on && state.chat_caps.reads => {
                                    ClientMessage::ChatRead { conv, id }
                                }
                                Signal::AfkSeen { id } if state.afk_on => ClientMessage::AfkSeen { id },
                                // Not on this connection: nothing to tell (CHAT2, AFKPLUS).
                                Signal::Typing { .. } | Signal::Read { .. } | Signal::AfkSeen { .. } => {
                                    continue;
                                }
                            };
                            if send(&mut sink, &frame).await.is_err() {
                                break RETRY_NOW;
                            }
                            continue;
                        }
                        Command::Plan { request, reply } => {
                            if !state.plans_on {
                                // No plans on this connection: what an older service would say.
                                let _ = reply.send(Err(plans_off()));
                                continue;
                            }
                            (plan_message(request, rid), Reply::Plan(reply))
                        }
                        Command::World { frame, reply } => {
                            if !state.worlds_on {
                                // An older service would refuse the frame (HOST1).
                                if let Some(reply) = reply {
                                    let _ = reply.send(Err(WorldError::Off));
                                }
                                continue;
                            }
                            let text = frame.text(reply.is_some().then_some(rid));
                            if sink.send(Message::text(text)).await.is_err() {
                                if let Some(reply) = reply {
                                    let _ = reply.send(Err(WorldError::Unavailable));
                                }
                                break RETRY_NOW;
                            }
                            if let Some(reply) = reply {
                                pending.insert(rid, Reply::World(reply));
                            }
                            continue;
                        }
                        Command::Shutdown => continue,
                    };
                    if send(&mut sink, &message).await.is_err() {
                        reply.fail(SocialError::Network("presence connection lost".into()));
                        break RETRY_NOW;
                    }
                    pending.insert(rid, reply);
                }
            },
            () = sleep_until(sync_again.unwrap_or(next_ping)), if sync_again.is_some() => {
                state.sync_again = None;
                if start_sync(&mut state, &mut sink).await.is_err() {
                    break RETRY_NOW;
                }
            }
            () = sleep_until(next_ping) => {
                if sink.send(Message::Ping(Default::default())).await.is_err() {
                    break RETRY_NOW;
                }
                ping_sent.get_or_insert_with(Instant::now);
                next_ping = Instant::now() + spread(state.keepalive);
            }
            () = sleep_until(pong_deadline.unwrap_or(next_ping)), if pong_deadline.is_some() => {
                if ping_sent.is_some_and(|at| last_inbound < at) {
                    tracing::debug!("presence connection stopped answering");
                    break RETRY_NOW;
                }
            }
        }
    };
    for (_, reply) in pending.drain() {
        reply.fail(SocialError::Network("presence connection lost".into()));
    }
    outcome
}

fn crew_message(request: CrewRequest, rid: u32) -> ClientMessage {
    match request {
        CrewRequest::Call { crews } => ClientMessage::Call { crews, rid },
        CrewRequest::Answer { call, answer } => ClientMessage::CallAnswer { call, answer, rid },
        CrewRequest::End { call } => ClientMessage::CallEnd { call, rid },
    }
}

fn chat_message(request: ChatRequest, rid: u32) -> ClientMessage {
    match request {
        ChatRequest::Send {
            conv,
            body,
            reply,
            nonce,
        } => {
            let (text, sticker) = match body {
                ChatBody::Text(text) => (Some(text), None),
                ChatBody::Sticker(sticker) => (None, Some(sticker)),
            };
            ClientMessage::Chat {
                conv,
                text,
                sticker,
                reply,
                nonce,
                rid,
            }
        }
        ChatRequest::Edit { msg, text } => ClientMessage::ChatEdit { msg, text, rid },
        ChatRequest::Delete { msg } => ClientMessage::ChatDelete { msg, rid },
        ChatRequest::React { conv, id, e, on } => ClientMessage::ChatReact {
            conv,
            id,
            e,
            on,
            rid,
        },
        ChatRequest::Unfurl { url } => ClientMessage::ChatUnfurl { url, rid },
    }
}

/// Chat is not on this connection: what an older service would say.
fn chat_off() -> SocialError {
    SocialError::Refused {
        status: 403,
        code: ErrorCode::ChatOff,
        message: String::new(),
        detail: None,
    }
}

/// A chat extra this connection does not have (CHAT2): as when Studio switched it off.
fn feature_off(extra: &str) -> SocialError {
    SocialError::Refused {
        status: 403,
        code: ErrorCode::ChatFeatureOff,
        message: String::new(),
        detail: Some(extra.to_owned()),
    }
}

fn plan_message(request: PlanRequest, rid: u32) -> ClientMessage {
    match request {
        PlanRequest::Make { crew, at, note } => ClientMessage::Plan {
            crew,
            at,
            note,
            rid,
        },
        PlanRequest::Answer { plan, answer } => ClientMessage::PlanAnswer { plan, answer, rid },
        PlanRequest::Cancel { plan } => ClientMessage::PlanCancel { plan, rid },
        PlanRequest::Note { crew, plan, note } => ClientMessage::PlanNote {
            crew,
            id: plan,
            note,
            rid,
        },
    }
}

/// Plans are not on this connection (an older service): what it would answer a plan frame.
fn plans_off() -> SocialError {
    SocialError::Refused {
        status: 400,
        code: ErrorCode::BadRequest,
        message: String::new(),
        detail: None,
    }
}

/// Pings every `keepalive` ± 10 %, so clients do not ping in step.
fn spread(keepalive: Duration) -> Duration {
    jitter(keepalive.mul_f64(0.9), keepalive.mul_f64(1.1))
}

struct SessionState {
    greeted: bool,
    /// The status the service has for this connection (None before `hello`).
    sent: Option<Outgoing>,
    next_rid: u32,
    keepalive: Duration,
    /// Chat was asked for: where its sync starts.
    chat: Option<ChatSetup>,
    /// The service took chat on this connection.
    chat_on: bool,
    /// The chat extras the service took on this connection (CHAT2).
    chat_caps: ChatCaps,
    /// The service took crew plans on this connection (TEAM5).
    plans_on: bool,
    /// The service took crew tag changes on this connection (TEAM6).
    badges_on: bool,
    /// The service took profile mark changes on this connection (SOC-VERIFY).
    marks_on: bool,
    /// The service took world sessions on this connection (HOST1).
    worlds_on: bool,
    /// The service took Cloud AFK's notices on this connection (AFKPLUS).
    afk_on: bool,
    /// The sync under way: its `rid`, where it started (ms) and whether it is the week-long one
    /// that catches up with chat extras (CHAT2).
    sync: Option<(u32, i64, bool)>,
    /// A refused sync is asked again then.
    sync_again: Option<Instant>,
    /// The `bye` before the close said the account is restricted (until when; None for good).
    restricted: Option<Option<i64>>,
}

impl SessionState {
    fn rid(&mut self) -> u32 {
        self.next_rid = self.next_rid.wrapping_add(1) % 1_000_000;
        self.next_rid
    }
}

async fn send(
    sink: &mut Sink,
    message: &ClientMessage,
) -> std::result::Result<(), tungstenite::Error> {
    let text = serde_json::to_string(message).unwrap_or_default();
    sink.send(Message::text(text)).await
}

async fn send_status(
    state: &mut SessionState,
    sink: &mut Sink,
    status: &mut watch::Receiver<Outgoing>,
) -> std::result::Result<(), tungstenite::Error> {
    let desired = status.borrow_and_update().on_wire();
    if state.sent.as_ref() == Some(&desired) {
        return Ok(());
    }
    send(
        sink,
        &ClientMessage::status(&desired.status, desired.version.as_deref()),
    )
    .await?;
    state.sent = Some(desired);
    Ok(())
}

/// Asks what changed since the device's newest message (after every connect, never on a timer).
/// The first sync with chat extras reaches back a week instead (CHAT2).
async fn start_sync(
    state: &mut SessionState,
    sink: &mut Sink,
) -> std::result::Result<(), tungstenite::Error> {
    let (since, catching_up) = match &state.chat {
        Some(setup) if state.chat_on => {
            let since = setup.since.load(Ordering::Relaxed).max(0);
            let catch_up = setup.catch_up.load(Ordering::Relaxed);
            if state.chat_caps.any() && catch_up >= 0 {
                (since.min(catch_up), true)
            } else {
                (since, false)
            }
        }
        _ => return Ok(()),
    };
    let rid = state.rid();
    state.sync = Some((rid, since, catching_up));
    send(
        sink,
        &ClientMessage::ChatSync {
            since,
            after: None,
            rid,
        },
    )
    .await
}

async fn on_text(
    text: &str,
    state: &mut SessionState,
    sink: &mut Sink,
    status: &mut watch::Receiver<Outgoing>,
    events: &mpsc::UnboundedSender<PresenceEvent>,
    pending: &mut Pending,
) -> std::result::Result<(), Outcome> {
    let message: ServerMessage = match serde_json::from_str(text) {
        Ok(message) => message,
        Err(error) => {
            tracing::debug!(%error, "ignoring an unreadable presence message");
            return Ok(());
        }
    };
    let gone = |_| Outcome::Stop(StopReason::Shutdown);
    let event = match message {
        ServerMessage::Hello {
            status: mode,
            keepalive_seconds,
            mut friends,
            team,
            invites,
            calls,
            now,
            caps,
            chat,
            plans,
            worlds,
        } => {
            tracing::debug!(?caps, "presence connected");
            state.greeted = true;
            // Every connection starts as "launcher" on the service (no version: SOC9).
            state.sent = Some(Outgoing::default());
            if let Some(seconds) = keepalive_seconds.filter(|s| (15..=300).contains(s)) {
                state.keepalive = state.keepalive.min(Duration::from_secs(seconds));
            }
            // Chat flows only when it was asked for and the service took it; so do its extras.
            let chat_on =
                state.chat.is_some() && caps.iter().any(|cap| cap == CHAT_CAP) && chat.is_some();
            state.chat_on = chat_on;
            state.chat_caps = if chat_on {
                chat_caps(&caps)
            } else {
                ChatCaps::default()
            };
            let chat = chat.map(|mut chat| {
                chat.caps = state.chat_caps;
                chat
            });
            // Plans flow only when the service took them (an older one ignores the capability);
            // so do crew tag changes (TEAM6).
            state.plans_on = caps.iter().any(|cap| cap == PLANS_CAP);
            state.badges_on = caps.iter().any(|cap| cap == BADGES_CAP);
            state.marks_on = caps.iter().any(|cap| cap == MARKS_CAP);
            state.afk_on = caps.iter().any(|cap| cap == AFK_CAP);
            state.sync = None;
            state.sync_again = None;
            // Everyone online the player may see: friends, then teammates who are not friends.
            friends.extend(team);
            let _ = events.send(PresenceEvent::Connected {
                status: mode,
                friends,
                invites,
                calls,
                clock: now,
                chat: chat.filter(|_| chat_on),
                plans: if state.plans_on {
                    Some(plans.unwrap_or_default())
                } else {
                    None
                },
            });
            // World sessions flow only when the service took them (HOST1); a connection that did
            // not says nothing of them (the drop before it said so).
            state.worlds_on = caps.iter().any(|cap| cap == WORLDS_CAP);
            if state.worlds_on {
                let _ = events.send(PresenceEvent::World(WorldEvent::Hello {
                    taken: true,
                    status: worlds.unwrap_or_default(),
                }));
            }
            send_status(state, sink, status)
                .await
                .map_err(|_| RETRY_NOW)?;
            return start_sync(state, sink).await.map_err(|_| RETRY_NOW);
        }
        ServerMessage::Call { call, quiet, now } => PresenceEvent::Call {
            call,
            quiet,
            clock: now,
        },
        ServerMessage::CallAnswer { call, by, answer } => {
            let Some(answer) = CallAnswer::parse(&answer) else {
                return Ok(());
            };
            PresenceEvent::CallAnswered { call, by, answer }
        }
        ServerMessage::CallEnd { call, reason } => PresenceEvent::CallEnded { call, reason },
        ServerMessage::Presence { presence } => PresenceEvent::Presence(presence),
        ServerMessage::FriendAdded { friend, presence } => {
            PresenceEvent::FriendAdded { friend, presence }
        }
        ServerMessage::FriendRemoved { id } => PresenceEvent::FriendRemoved { id },
        ServerMessage::Request { from } => PresenceEvent::RequestReceived { from },
        ServerMessage::RequestCancelled { id } => PresenceEvent::RequestCancelled { id },
        ServerMessage::Invite { invite } => PresenceEvent::Invite(invite),
        ServerMessage::Invites { invites } => PresenceEvent::QueuedInvites(invites),
        ServerMessage::Status { status } => PresenceEvent::StatusChanged(status),
        ServerMessage::Profile {
            privacy,
            avatar,
            avatar_hash,
        } => PresenceEvent::ProfileChanged {
            privacy,
            avatar,
            avatar_hash,
        },
        ServerMessage::Team {
            team,
            crew,
            reason,
            by,
            joined,
        } => PresenceEvent::TeamChanged {
            team,
            crew,
            reason,
            by,
            joined,
        },
        ServerMessage::Badge { id, badge } if state.badges_on => {
            PresenceEvent::BadgeChanged { id, badge }
        }
        ServerMessage::Mark { id, mark } if state.marks_on => {
            PresenceEvent::MarkChanged { id, mark }
        }
        ServerMessage::TeamInvite { invite } => PresenceEvent::TeamInvited(invite),
        ServerMessage::TeamRequests {
            crew,
            requests,
            added,
        } => PresenceEvent::TeamRequests {
            crew,
            requests,
            added,
        },
        ServerMessage::Plan { plan, quiet } if state.plans_on => {
            PresenceEvent::Plan { plan, quiet }
        }
        ServerMessage::Plans { crew, plans } if state.plans_on => {
            PresenceEvent::Plans { crew, plans }
        }
        ServerMessage::PlanRemoved { plan, crew, by } if state.plans_on => {
            PresenceEvent::PlanRemoved { plan, crew, by }
        }
        ServerMessage::Groups { groups } => PresenceEvent::GroupsChanged(groups),
        ServerMessage::TeamNotifications { notifications } => {
            PresenceEvent::CrewNotifications(notifications)
        }
        ServerMessage::CrewNotifications {
            crew,
            notifications,
        } => PresenceEvent::CrewMuted {
            crew,
            notifications,
        },
        ServerMessage::CrewOrder { order } => PresenceEvent::CrewOrder(order),
        ServerMessage::Chat { msg } if state.chat_on => PresenceEvent::ChatMessage(msg),
        ServerMessage::ChatEdited { msg } if state.chat_on => PresenceEvent::ChatEdited(msg),
        ServerMessage::ChatDeleted { conv, id, by, at } if state.chat_on => {
            PresenceEvent::ChatDeleted { conv, id, by, at }
        }
        ServerMessage::ChatSync {
            conv,
            mut messages,
            deleted,
            reacted,
            reads,
        } if state.chat_on => {
            for message in &mut messages {
                message.conv.clone_from(&conv);
            }
            PresenceEvent::ChatSync {
                conv,
                messages,
                deleted,
                reacted,
                reads,
            }
        }
        ServerMessage::ChatSynced { rid, now, more } => match state.sync {
            Some((sync, since, catching_up)) if rid == Some(sync) => {
                if let Some(after) = more {
                    // The rest at once, with the cursor just given.
                    let rid = state.rid();
                    state.sync = Some((rid, since, catching_up));
                    return send(
                        sink,
                        &ClientMessage::ChatSync {
                            since,
                            after: Some(after),
                            rid,
                        },
                    )
                    .await
                    .map_err(|_| RETRY_NOW);
                }
                state.sync = None;
                PresenceEvent::ChatSynced {
                    clock: now,
                    caught_up: catching_up,
                }
            }
            _ => return Ok(()),
        },
        ServerMessage::ChatState { mut chat } if state.chat_on => {
            chat.caps = state.chat_caps;
            PresenceEvent::ChatState(chat)
        }
        ServerMessage::ChatReacted {
            conv,
            id,
            by,
            e,
            on,
        } if state.chat_on && state.chat_caps.reactions => PresenceEvent::ChatReacted {
            conv,
            id,
            by,
            e,
            on,
        },
        ServerMessage::ChatTyping { conv, by } if state.chat_on && state.chat_caps.typing => {
            PresenceEvent::ChatTyping { conv, by }
        }
        ServerMessage::ChatRead { conv, by, id } if state.chat_on && state.chat_caps.reads => {
            PresenceEvent::ChatRead { conv, by, id }
        }
        ServerMessage::Notice { notice } => PresenceEvent::Notice(notice),
        ServerMessage::WorldInvite { invite, quiet } if state.worlds_on => {
            PresenceEvent::World(WorldEvent::Invite { invite, quiet })
        }
        ServerMessage::WorldUpdate {
            from,
            session,
            guests,
            max,
            paths,
        } if state.worlds_on => PresenceEvent::World(WorldEvent::Update {
            from,
            session,
            guests,
            max,
            paths,
        }),
        ServerMessage::WorldClosed { from, session } if state.worlds_on => {
            PresenceEvent::World(WorldEvent::Closed { from, session })
        }
        ServerMessage::WorldRevoked { from, session, to } if state.worlds_on => {
            PresenceEvent::World(match (to, from) {
                (Some(to), _) => WorldEvent::RevokedGuest { session, to },
                (None, from) => WorldEvent::Revoked {
                    from: from.unwrap_or_default(),
                    session,
                },
            })
        }
        ServerMessage::WorldPeer {
            session,
            from,
            slot,
            path,
            relay,
        } if state.worlds_on => PresenceEvent::World(WorldEvent::Peer {
            session,
            from,
            slot,
            path,
            relay,
        }),
        ServerMessage::WorldPeerOk {
            session,
            from,
            slot,
            paths,
        } if state.worlds_on => PresenceEvent::World(WorldEvent::PeerOk {
            session,
            from,
            slot,
            paths,
        }),
        ServerMessage::TurnMove {
            session,
            reason,
            grace,
        } if state.worlds_on => PresenceEvent::World(WorldEvent::TurnMove {
            session,
            reason,
            grace,
        }),
        ServerMessage::WorldState { worlds } if state.worlds_on => {
            PresenceEvent::World(WorldEvent::Status(worlds))
        }
        ServerMessage::Mc(message) => PresenceEvent::Mc(message),
        // A service sends these only to a socket that took `afk`.
        ServerMessage::AfkNotice { notice } => PresenceEvent::AfkNotice(notice),
        ServerMessage::Ack {
            rid,
            delivered,
            msg,
            plan,
            reactions,
            card,
            world,
            order,
            ice,
            e4mc,
        } => {
            match rid.and_then(|rid| pending.remove(&rid)) {
                Some(Reply::World(reply)) => {
                    let _ = reply.send(Ok(WorldAnswer {
                        delivered,
                        world,
                        order: order.unwrap_or_default(),
                        ice,
                        e4mc,
                    }));
                }
                Some(Reply::Invite(reply)) => {
                    let outcome = if delivered == Some(true) {
                        InviteOutcome::Delivered
                    } else {
                        InviteOutcome::Queued
                    };
                    let _ = reply.send(Ok(outcome));
                }
                Some(Reply::Crew(reply)) => {
                    let _ = reply.send(Ok(()));
                }
                Some(Reply::Chat(reply)) => {
                    let _ = reply.send(Ok(ChatAnswer {
                        msg: msg.clone(),
                        reactions,
                        card: card.map(|card| *card),
                    }));
                }
                Some(Reply::Plan(reply)) => {
                    let _ = reply.send(Ok(plan));
                }
                None => {}
            }
            // The stored message reaches the owner as an event too: an answer that came after its
            // wait ended still settles the message it is about.
            return match msg {
                Some(msg) if msg.edited_at.is_some() => {
                    events.send(PresenceEvent::ChatEdited(msg)).map_err(gone)
                }
                Some(msg) => events.send(PresenceEvent::ChatMessage(msg)).map_err(gone),
                None => Ok(()),
            };
        }
        ServerMessage::Error {
            rid,
            code,
            retry_after,
            reason,
            rule,
            until,
            item,
        } => {
            if let Some(rid) = rid {
                if state.sync.is_some_and(|(sync, _, _)| sync == rid) {
                    // A sync too soon after the last one (one a minute per socket): asked again
                    // after the wait. Whatever comes meanwhile comes live.
                    state.sync = None;
                    if code == "rate_limited" {
                        let wait = retry_after
                            .map_or(SYNC_RETRY_MIN, Duration::from_secs)
                            .clamp(SYNC_RETRY_MIN, SYNC_RETRY_MAX);
                        state.sync_again = Some(Instant::now() + wait);
                    }
                    return events
                        .send(PresenceEvent::ChatSynced {
                            clock: None,
                            caught_up: false,
                        })
                        .map_err(gone);
                }
                if let Some(reply) = pending.remove(&rid) {
                    match reply {
                        // A world request hears the service's words as they are (HOST1).
                        Reply::World(reply) => {
                            let _ = reply.send(Err(WorldError::Refused {
                                code,
                                reason,
                                item,
                                retry_after,
                            }));
                        }
                        reply => reply.fail(socket_error(&SocketRefusal {
                            code: &code,
                            retry_after,
                            reason: reason.as_deref(),
                            rule: rule.as_deref(),
                            until,
                            item: item.as_deref(),
                        })),
                    }
                    return Ok(());
                }
            }
            tracing::debug!(%code, "presence error");
            return Ok(());
        }
        ServerMessage::Bye { reason, until } => {
            tracing::debug!(%reason, "presence connection closing");
            if reason == "restricted" {
                state.restricted = Some(until);
            }
            return Ok(());
        }
        ServerMessage::Unknown
        | ServerMessage::Chat { .. }
        | ServerMessage::ChatEdited { .. }
        | ServerMessage::ChatDeleted { .. }
        | ServerMessage::ChatSync { .. }
        | ServerMessage::ChatState { .. }
        | ServerMessage::ChatReacted { .. }
        | ServerMessage::ChatTyping { .. }
        | ServerMessage::ChatRead { .. }
        | ServerMessage::Plan { .. }
        | ServerMessage::Plans { .. }
        | ServerMessage::PlanRemoved { .. }
        | ServerMessage::Badge { .. }
        | ServerMessage::Mark { .. }
        | ServerMessage::WorldInvite { .. }
        | ServerMessage::WorldUpdate { .. }
        | ServerMessage::WorldClosed { .. }
        | ServerMessage::WorldRevoked { .. }
        | ServerMessage::WorldPeer { .. }
        | ServerMessage::WorldPeerOk { .. }
        | ServerMessage::TurnMove { .. }
        | ServerMessage::WorldState { .. } => return Ok(()),
    };
    events.send(event).map_err(gone)
}

/// What an `error` frame says, as it said it.
struct SocketRefusal<'a> {
    code: &'a str,
    retry_after: Option<u64>,
    reason: Option<&'a str>,
    rule: Option<&'a str>,
    until: Option<i64>,
    item: Option<&'a str>,
}

/// The error of a socket request (an invite, a crew call, chat or plan request).
fn socket_error(refusal: &SocketRefusal<'_>) -> SocialError {
    let SocketRefusal {
        code,
        retry_after,
        reason,
        rule,
        until,
        item,
    } = *refusal;
    let status = match code {
        "rate_limited" => {
            return SocialError::RateLimited {
                retry_after: retry_after.map(Duration::from_secs),
                item: item.map(ChatQuota::parse),
            };
        }
        "chat_rejected" => {
            return SocialError::ChatRejected {
                reason: ChatRejection::parse(reason.unwrap_or_default()),
                rule: rule.map(LinkRule::parse),
            };
        }
        // The hub fetches too much right now: ask again after the wait (CHAT2).
        "unfurl_busy" => {
            return SocialError::Unavailable {
                retry_after: retry_after.map(Duration::from_secs),
            };
        }
        // A plan's note (FIX2): chat's reasons, or none for its form (a line break, too long).
        "invalid_plan_note" => {
            return SocialError::PlanNoteRejected {
                reason: reason.map(ChatRejection::parse),
            };
        }
        "chat_muted" => return SocialError::ChatMuted { until },
        // A new Discord account may not do this (a plan); the limits end at `until`.
        "account_limited" => {
            return SocialError::Limited {
                item: LimitedKey::parse(item.unwrap_or_default()),
                until,
                retry_after: retry_after.map(Duration::from_secs),
            };
        }
        // An unexpected failure of the service: the frame may be sent again.
        "internal" => return SocialError::Unavailable { retry_after: None },
        "friend_not_found" | "team_not_found" | "call_not_found" | "message_not_found"
        | "plan_not_found" => 404,
        "team_not_permitted" | "chat_off" | "chat_not_permitted" | "chat_feature_off" => 403,
        "call_active"
        | "call_needs_server"
        | "call_server_hidden"
        | "chat_too_old"
        | "plan_exists"
        | "chat_reactions_full" => 409,
        "plan_time" | "limit_team_plans" | "unfurl_failed" => 422,
        _ => 400,
    };
    // What the refusal is about, where the service said (CHAT2).
    let detail = match code {
        "chat_feature_off" => item,
        "chat_reactions_full" => reason,
        "unfurl_failed" => rule,
        _ => None,
    };
    SocialError::Refused {
        status,
        code: ErrorCode::parse(code),
        message: String::new(),
        detail: detail.map(str::to_owned),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use tokio::net::TcpListener;
    use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};

    use super::*;
    use crate::SocialClient;
    use crate::client::SocialConfig;

    fn fast() -> PresenceOptions {
        PresenceOptions {
            keepalive: Duration::from_secs(30),
            pong_timeout: Duration::from_secs(5),
            backoff: BackoffPolicy {
                initial: Duration::from_millis(20),
                max: Duration::from_millis(50),
                spread: Duration::ZERO,
            },
            stable_after: Duration::from_secs(60),
            connect_timeout: Duration::from_secs(5),
            invite_timeout: Duration::from_secs(5),
        }
    }

    fn token() -> SessionToken {
        SessionToken::new(format!("pls_{}", "Z".repeat(43)))
    }

    type Server = WebSocketStream<TcpStream>;

    /// Accepts one connection; `reject` answers the handshake with that status instead.
    async fn accept(
        listener: &TcpListener,
        reject: Option<(u16, Option<&'static str>)>,
        seen: Arc<Mutex<Vec<(String, String)>>>,
    ) -> Option<Server> {
        let (stream, _) = listener.accept().await.unwrap();
        // tungstenite's handshake callback fixes the error type (`ErrorResponse`); it cannot be
        // boxed here.
        #[allow(clippy::result_large_err)]
        let callback = move |request: &Request, response: Response| {
            let mut log = seen.lock().unwrap();
            log.push((":uri".to_owned(), request.uri().to_string()));
            for (name, value) in request.headers() {
                log.push((name.to_string(), value.to_str().unwrap_or("").to_owned()));
            }
            match reject {
                None => Ok(response),
                Some((status, retry_after)) => {
                    let mut error = ErrorResponse::new(None);
                    *error.status_mut() = tungstenite::http::StatusCode::from_u16(status).unwrap();
                    if let Some(value) = retry_after {
                        error
                            .headers_mut()
                            .insert(RETRY_AFTER, HeaderValue::from_static(value));
                    }
                    Err(error)
                }
            }
        };
        tokio_tungstenite::accept_hdr_async(stream, callback)
            .await
            .ok()
    }

    async fn send_json(server: &mut Server, value: serde_json::Value) {
        server.send(Message::text(value.to_string())).await.unwrap();
    }

    async fn next_json(server: &mut Server) -> serde_json::Value {
        loop {
            match server.next().await.unwrap().unwrap() {
                Message::Text(text) => return serde_json::from_str(text.as_str()).unwrap(),
                Message::Ping(_) | Message::Pong(_) => continue,
                other => panic!("unexpected frame {other:?}"),
            }
        }
    }

    fn hello() -> serde_json::Value {
        serde_json::json!({
            "t": "hello", "v": 1, "status": "online", "keepaliveSeconds": 45,
            "friends": [{"id": "111111111111111111", "state": "launcher", "status": "online",
                         "game": null, "avatar": "discord", "avatarHash": null}],
            "invites": []
        })
    }

    async fn start() -> (TcpListener, SocialClient) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/", listener.local_addr().unwrap());
        let client = SocialClient::new(SocialConfig::new(
            Url::parse(&base).unwrap(),
            "ProjectsLauncher/test",
        ))
        .unwrap();
        (listener, client)
    }

    #[tokio::test]
    async fn connects_streams_events_and_sends_status_only_on_change() {
        let (listener, client) = start().await;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut handle = client.session(token()).connect_presence(fast());
        let mut server = accept(&listener, None, seen.clone()).await.unwrap();
        {
            let headers = seen.lock().unwrap();
            assert!(headers.iter().any(
                |(k, v)| k == "authorization" && v == &format!("Bearer pls_{}", "Z".repeat(43))
            ));
            assert!(
                headers
                    .iter()
                    .any(|(k, v)| k == "user-agent" && v == "ProjectsLauncher/test")
            );
            // What this launcher understands rides on the address (older services ignore it).
            assert!(headers.iter().any(|(k, v)| k == ":uri"
                && v == "/v1/presence?caps=callServer,crews,plans,badges,mc,afk,marks,profile"));
        }
        send_json(&mut server, hello()).await;
        match handle.next_event().await.unwrap() {
            PresenceEvent::Connected {
                status, friends, ..
            } => {
                assert_eq!(status, StatusMode::Online);
                assert_eq!(friends.len(), 1);
            }
            other => panic!("unexpected {other:?}"),
        }

        assert!(handle.set_status(LocalStatus::Playing {
            host: Some("projects.gg".into())
        }));
        assert_eq!(
            next_json(&mut server).await,
            serde_json::json!({"t": "status", "state": "playing", "host": "projects.gg"})
        );
        // The same status again is not sent.
        assert!(!handle.set_status(LocalStatus::Playing {
            host: Some("projects.gg".into())
        }));
        assert!(handle.set_status(LocalStatus::Singleplayer));
        assert_eq!(
            next_json(&mut server).await,
            serde_json::json!({"t": "status", "state": "playing", "singleplayer": true})
        );

        send_json(
            &mut server,
            serde_json::json!({"t": "presence", "presence": {"id": "111111111111111111",
                "state": "offline", "lastSeen": "today"}}),
        )
        .await;
        assert!(matches!(
            handle.next_event().await.unwrap(),
            PresenceEvent::Presence(p) if p.state == crate::protocol::PresenceState::Offline
        ));

        // The control half works from another task while this one keeps the events.
        let invite = tokio::spawn({
            let control = handle.control();
            async move { control.invite("111111111111111111").await }
        });
        let sent = next_json(&mut server).await;
        assert_eq!(sent["t"], "invite");
        assert_eq!(sent["to"], "111111111111111111");
        send_json(
            &mut server,
            serde_json::json!({"t": "ack", "rid": sent["rid"], "delivered": false}),
        )
        .await;
        assert_eq!(invite.await.unwrap().unwrap(), InviteOutcome::Queued);

        handle.shutdown().await;
        assert!(matches!(
            server.next().await,
            Some(Ok(Message::Close(Some(frame)))) if frame.code == CloseCode::Normal
        ));
    }

    #[tokio::test]
    async fn reconnects_with_backoff_and_restores_the_status() {
        let (listener, client) = start().await;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut handle = client.session(token()).connect_presence(fast());
        let mut server = accept(&listener, None, seen.clone()).await.unwrap();
        send_json(&mut server, hello()).await;
        handle.next_event().await.unwrap();
        handle.set_status(LocalStatus::Playing { host: None });
        assert_eq!(next_json(&mut server).await["state"], "playing");

        // Cloudflare restarts the server: the socket just goes away.
        drop(server);
        match handle.next_event().await.unwrap() {
            PresenceEvent::Disconnected { retry_in } => {
                assert!(retry_in <= Duration::from_millis(50));
            }
            other => panic!("unexpected {other:?}"),
        }
        let mut server = accept(&listener, None, seen).await.unwrap();
        send_json(&mut server, hello()).await;
        assert!(matches!(
            handle.next_event().await.unwrap(),
            PresenceEvent::Connected { .. }
        ));
        // The new connection starts as "launcher" on the service; the client restores "playing".
        assert_eq!(
            next_json(&mut server).await,
            serde_json::json!({"t": "status", "state": "playing"})
        );
    }

    #[tokio::test]
    async fn stops_when_the_session_ends() {
        let (listener, client) = start().await;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut handle = client.session(token()).connect_presence(fast());
        let mut server = accept(&listener, None, seen.clone()).await.unwrap();
        send_json(&mut server, hello()).await;
        handle.next_event().await.unwrap();
        send_json(
            &mut server,
            serde_json::json!({"t": "bye", "reason": "session_revoked"}),
        )
        .await;
        server
            .close(Some(CloseFrame {
                code: CloseCode::from(4001),
                reason: "session_revoked".into(),
            }))
            .await
            .unwrap();
        assert_eq!(
            handle.next_event().await.unwrap(),
            PresenceEvent::Stopped(StopReason::SignedOut)
        );
        assert!(handle.next_event().await.is_none());

        // A token the service no longer accepts is rejected at the handshake: stop, no retry.
        let mut handle = client.session(token()).connect_presence(fast());
        assert!(accept(&listener, Some((401, None)), seen).await.is_none());
        assert_eq!(
            handle.next_event().await.unwrap(),
            PresenceEvent::Stopped(StopReason::SignedOut)
        );
    }

    #[tokio::test]
    async fn honours_retry_after_when_rate_limited() {
        let (listener, client) = start().await;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut handle = client.session(token()).connect_presence(fast());
        assert!(
            accept(&listener, Some((429, Some("120"))), seen)
                .await
                .is_none()
        );
        match handle.next_event().await.unwrap() {
            PresenceEvent::Disconnected { retry_in } => {
                assert!(retry_in >= Duration::from_secs(120));
            }
            other => panic!("unexpected {other:?}"),
        }
        // While waiting, invites fail fast instead of queueing up.
        let error = handle.invite("111111111111111111").await.unwrap_err();
        assert!(matches!(error, SocialError::Unavailable { .. }));
        handle.shutdown().await;
    }

    #[tokio::test]
    async fn replaces_a_connection_that_stops_answering_pings() {
        let (listener, client) = start().await;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let options = PresenceOptions {
            keepalive: Duration::from_millis(100),
            pong_timeout: Duration::from_millis(100),
            ..fast()
        };
        let mut handle = client.session(token()).connect_presence(options);
        let mut server = accept(&listener, None, seen).await.unwrap();
        send_json(&mut server, hello()).await;
        handle.next_event().await.unwrap();
        // The server stops reading (so it never answers pings) but keeps the TCP connection.
        let event = timeout(Duration::from_secs(5), handle.next_event())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(event, PresenceEvent::Disconnected { .. }));
        drop(server);
    }

    #[test]
    fn maps_close_codes_and_handshake_statuses() {
        let frame = |code: u16| CloseFrame {
            code: CloseCode::from(code),
            reason: "".into(),
        };
        assert_eq!(
            close_outcome(Some(&frame(4002)), None),
            Outcome::Stop(StopReason::AccountDeleted)
        );
        assert_eq!(
            close_outcome(Some(&frame(4000)), None),
            Outcome::Stop(StopReason::Replaced)
        );
        assert_eq!(
            close_outcome(Some(&frame(4008)), None),
            Outcome::Retry {
                at_least: Duration::from_secs(60)
            }
        );
        assert_eq!(close_outcome(Some(&frame(1006)), None), RETRY_NOW);
        assert_eq!(close_outcome(None, None), RETRY_NOW);
        assert_eq!(
            handshake_outcome(503, Some(Duration::from_secs(7_200))),
            Outcome::Retry {
                at_least: Duration::from_secs(3_600)
            }
        );
        assert_eq!(handshake_outcome(502, None), RETRY_NOW);
        assert_eq!(
            handshake_outcome(426, None),
            Outcome::Retry {
                at_least: Duration::from_secs(300)
            }
        );
        let refusal = |code| SocketRefusal {
            code,
            retry_after: None,
            reason: None,
            rule: None,
            until: None,
            item: None,
        };
        assert!(matches!(
            socket_error(&SocketRefusal {
                retry_after: Some(30),
                ..refusal("rate_limited")
            }),
            SocialError::RateLimited { retry_after: Some(d), item: None } if d == Duration::from_secs(30)
        ));
        assert_eq!(
            socket_error(&refusal("friend_not_found")).code(),
            Some(&ErrorCode::FriendNotFound)
        );
        assert_eq!(
            socket_error(&refusal("call_active")).code(),
            Some(&ErrorCode::CallActive)
        );
        assert!(matches!(
            socket_error(&refusal("call_not_found")),
            SocialError::Refused {
                status: 404,
                code: ErrorCode::CallNotFound,
                ..
            }
        ));
        // Plans (TEAM5): a new Discord account is told what and until when.
        assert!(matches!(
            socket_error(&SocketRefusal {
                until: Some(1_792_600_000),
                item: Some("crew_plans"),
                ..refusal("account_limited")
            }),
            SocialError::Limited {
                item: LimitedKey::CrewPlans,
                until: Some(1_792_600_000),
                ..
            }
        ));
        assert_eq!(
            socket_error(&refusal("plan_exists")).code(),
            Some(&ErrorCode::PlanExists)
        );
        assert!(matches!(
            socket_error(&refusal("plan_not_found")),
            SocialError::Refused {
                status: 404,
                code: ErrorCode::PlanNotFound,
                ..
            }
        ));
        // Chat extras (CHAT2): each refusal keeps what it is about.
        assert!(matches!(
            socket_error(&SocketRefusal {
                reason: Some("link"),
                rule: Some("shortener"),
                ..refusal("chat_rejected")
            }),
            SocialError::ChatRejected {
                reason: ChatRejection::Link,
                rule: Some(LinkRule::Shortener)
            }
        ));
        assert!(matches!(
            socket_error(&SocketRefusal {
                retry_after: Some(40_000),
                item: Some("stickers"),
                ..refusal("rate_limited")
            }),
            SocialError::RateLimited {
                item: Some(ChatQuota::Stickers),
                ..
            }
        ));
        let full = socket_error(&SocketRefusal {
            reason: Some("mine"),
            ..refusal("chat_reactions_full")
        });
        assert_eq!(
            (full.code(), full.detail()),
            (Some(&ErrorCode::ChatReactionsFull), Some("mine"))
        );
        let off = socket_error(&SocketRefusal {
            item: Some("stickers"),
            ..refusal("chat_feature_off")
        });
        assert_eq!(off.detail(), Some("stickers"));
        let failed = socket_error(&SocketRefusal {
            rule: Some("grabber"),
            ..refusal("unfurl_failed")
        });
        assert_eq!(
            (failed.code(), failed.detail()),
            (Some(&ErrorCode::UnfurlFailed), Some("grabber"))
        );
        assert!(matches!(
            socket_error(&SocketRefusal {
                retry_after: Some(2),
                ..refusal("unfurl_busy")
            }),
            SocialError::Unavailable { retry_after: Some(d) } if d == Duration::from_secs(2)
        ));
        assert!(matches!(
            socket_error(&SocketRefusal {
                item: Some("chat_links"),
                ..refusal("account_limited")
            }),
            SocialError::Limited {
                item: LimitedKey::ChatLinks,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn crew_calls_go_out_with_their_rid_and_come_back_as_events() {
        let (listener, client) = start().await;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut handle = client.session(token()).connect_presence(fast());
        let mut server = accept(&listener, None, seen).await.unwrap();
        let mut greeting = hello();
        greeting["calls"] = serde_json::json!([{"id": "c0", "from": "111111111111111111",
            "kind": "ready", "at": 1_792_521_000, "until": 1_792_522_200, "answers": {}}]);
        greeting["now"] = serde_json::json!(1_792_521_300);
        send_json(&mut server, greeting).await;
        match handle.next_event().await.unwrap() {
            PresenceEvent::Connected { calls, clock, .. } => {
                assert_eq!(calls.len(), 1);
                assert_eq!(clock, Some(1_792_521_300));
            }
            other => panic!("unexpected {other:?}"),
        }

        // "Ekibi çağır" to two crews: the own call arrives first, then the ack.
        let call = tokio::spawn({
            let control = handle.control();
            async move {
                control
                    .call_crew(Some(vec![
                        "0a1b2c3d4e5f6a7b".into(),
                        "9f8e7d6c5b4a3f2e".into(),
                    ]))
                    .await
            }
        });
        let sent = next_json(&mut server).await;
        assert_eq!(sent["t"], "call");
        assert_eq!(
            sent["crews"],
            serde_json::json!(["0a1b2c3d4e5f6a7b", "9f8e7d6c5b4a3f2e"])
        );
        send_json(
            &mut server,
            serde_json::json!({"t": "call", "call": {"id": "c1", "from": "100",
                "kind": "join", "at": 10, "until": 1210, "answers": {}}, "quiet": true, "now": 12}),
        )
        .await;
        send_json(
            &mut server,
            serde_json::json!({"t": "ack", "rid": sent["rid"]}),
        )
        .await;
        call.await.unwrap().unwrap();
        assert!(matches!(
            handle.next_event().await.unwrap(),
            PresenceEvent::Call { call, quiet: true, clock: Some(12) } if call.id == "c1"
        ));

        // An answer to a call that is gone, and one more call too soon.
        let answer = tokio::spawn({
            let control = handle.control();
            async move { control.answer_call("c0", CallAnswer::Soon).await }
        });
        let sent = next_json(&mut server).await;
        assert_eq!(sent["t"], "call_answer");
        assert_eq!(sent["call"], "c0");
        assert_eq!(sent["answer"], "soon");
        send_json(
            &mut server,
            serde_json::json!({"t": "error", "rid": sent["rid"], "code": "call_not_found"}),
        )
        .await;
        assert_eq!(
            answer.await.unwrap().unwrap_err().code(),
            Some(&ErrorCode::CallNotFound)
        );
        let again = tokio::spawn({
            let control = handle.control();
            async move { control.call_crew(None).await }
        });
        let sent = next_json(&mut server).await;
        // Without crews: the primary one (and the only form an older service knows).
        assert!(sent.get("crews").is_none());
        send_json(
            &mut server,
            serde_json::json!({"t": "error", "rid": sent["rid"], "code": "rate_limited",
                "retryAfter": 240}),
        )
        .await;
        assert_eq!(
            again.await.unwrap().unwrap_err().retry_after(),
            Some(Duration::from_secs(240))
        );
        // Not on a multiplayer server (CALL1): a refusal with its own code.
        let away = tokio::spawn({
            let control = handle.control();
            async move { control.call_crew(None).await }
        });
        let sent = next_json(&mut server).await;
        send_json(
            &mut server,
            serde_json::json!({"t": "error", "rid": sent["rid"], "code": "call_needs_server"}),
        )
        .await;
        assert_eq!(
            away.await.unwrap().unwrap_err().code(),
            Some(&ErrorCode::CallNeedsServer)
        );

        // Answers and ends arrive as events; an unknown answer is skipped.
        send_json(
            &mut server,
            serde_json::json!({"t": "call_answer", "call": "c1", "by": "7", "answer": "maybe"}),
        )
        .await;
        send_json(
            &mut server,
            serde_json::json!({"t": "call_answer", "call": "c1", "by": "7", "answer": "yes"}),
        )
        .await;
        assert_eq!(
            handle.next_event().await.unwrap(),
            PresenceEvent::CallAnswered {
                call: "c1".into(),
                by: "7".into(),
                answer: CallAnswer::Yes
            }
        );
        let end = tokio::spawn({
            let control = handle.control();
            async move { control.end_call("c1").await }
        });
        let sent = next_json(&mut server).await;
        assert_eq!(sent["t"], "call_end");
        assert_eq!(sent["call"], "c1");
        send_json(
            &mut server,
            serde_json::json!({"t": "call_end", "call": "c1", "reason": "ended"}),
        )
        .await;
        send_json(
            &mut server,
            serde_json::json!({"t": "ack", "rid": sent["rid"]}),
        )
        .await;
        end.await.unwrap().unwrap();
        assert_eq!(
            handle.next_event().await.unwrap(),
            PresenceEvent::CallEnded {
                call: "c1".into(),
                reason: CallEndReason::Ended
            }
        );

        // "Sessize al" on another device of the player's.
        send_json(
            &mut server,
            serde_json::json!({"t": "team_notifications", "notifications": {"calls": true,
                "muted": true, "muteUntil": null}}),
        )
        .await;
        assert_eq!(
            handle.next_event().await.unwrap(),
            PresenceEvent::CrewNotifications(TeamNotifications {
                calls: true,
                muted: true,
                mute_until: None
            })
        );
        // One crew muted, and the crews in a new order, on another device.
        send_json(
            &mut server,
            serde_json::json!({"t": "crew_notifications", "crew": "9f8e7d6c5b4a3f2e",
                "notifications": {"muted": true, "muteUntil": 1_792_600_000}}),
        )
        .await;
        assert_eq!(
            handle.next_event().await.unwrap(),
            PresenceEvent::CrewMuted {
                crew: "9f8e7d6c5b4a3f2e".into(),
                notifications: CrewMute {
                    muted: true,
                    mute_until: Some(1_792_600_000)
                }
            }
        );
        send_json(
            &mut server,
            serde_json::json!({"t": "crew_order", "order": ["9f8e7d6c5b4a3f2e", "0a1b2c3d4e5f6a7b"]}),
        )
        .await;
        assert_eq!(
            handle.next_event().await.unwrap(),
            PresenceEvent::CrewOrder(vec!["9f8e7d6c5b4a3f2e".into(), "0a1b2c3d4e5f6a7b".into()])
        );

        // While the connection is down (a flood closes it for a minute), a call fails at once.
        server
            .close(Some(CloseFrame {
                code: CloseCode::from(4008),
                reason: "rate_limited".into(),
            }))
            .await
            .unwrap();
        assert!(matches!(
            handle.next_event().await.unwrap(),
            PresenceEvent::Disconnected { retry_in } if retry_in >= Duration::from_secs(60)
        ));
        let error = handle.call_crew(None).await.unwrap_err();
        assert!(matches!(error, SocialError::Unavailable { .. }));
        handle.shutdown().await;
    }

    #[tokio::test]
    async fn plans_go_out_with_their_rid_and_join_requests_come_live() {
        let (listener, client) = start().await;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut handle = client.session(token()).connect_presence(fast());
        let mut server = accept(&listener, None, seen.clone()).await.unwrap();
        // Plans are asked for on every connection (older services ignore it).
        assert!(seen.lock().unwrap().iter().any(|(k, v)| k == ":uri"
            && v == "/v1/presence?caps=callServer,crews,plans,badges,mc,afk,marks,profile"));
        let mut greeting = hello();
        greeting["caps"] = serde_json::json!(["callServer", "crews", "plans"]);
        greeting["now"] = serde_json::json!(1_792_521_300);
        greeting["plans"] = serde_json::json!([{"id": "p0", "crew": "0a1b2c3d4e5f6a7b",
            "by": "111111111111111111", "at": 1_792_609_200, "until": 1_792_616_400,
            "createdAt": 1_792_521_000, "answers": {"111111111111111111": "yes"}}]);
        send_json(&mut server, greeting).await;
        match handle.next_event().await.unwrap() {
            PresenceEvent::Connected { plans, .. } => {
                assert_eq!(plans.map(|p| p.len()), Some(1));
            }
            other => panic!("unexpected {other:?}"),
        }

        // A new plan: the frame goes with its rid and the stored plan comes back in the ack.
        let make = tokio::spawn({
            let control = handle.control();
            async move {
                control
                    .make_plan("0a1b2c3d4e5f6a7b", 1_792_612_800, None)
                    .await
            }
        });
        let sent = next_json(&mut server).await;
        assert_eq!(sent["t"], "plan");
        assert_eq!(sent["crew"], "0a1b2c3d4e5f6a7b");
        assert_eq!(sent["at"], 1_792_612_800_i64);
        send_json(
            &mut server,
            serde_json::json!({"t": "ack", "rid": sent["rid"], "plan": {"id": "p1",
                "crew": "0a1b2c3d4e5f6a7b", "by": "100", "at": 1_792_612_800,
                "until": 1_792_620_000, "createdAt": 1_792_521_310, "answers": {"100": "yes"}}}),
        )
        .await;
        let made = make.await.unwrap().unwrap();
        assert_eq!(made.id, "p1");
        assert_eq!(made.answers["100"], PlanAnswer::Yes);

        // The same time again; a changed answer over the day's limit; a cancel.
        let twice = tokio::spawn({
            let control = handle.control();
            async move {
                control
                    .make_plan("0a1b2c3d4e5f6a7b", 1_792_612_800, None)
                    .await
            }
        });
        let sent = next_json(&mut server).await;
        send_json(
            &mut server,
            serde_json::json!({"t": "error", "rid": sent["rid"], "code": "plan_exists", "plan": "p1"}),
        )
        .await;
        assert_eq!(
            twice.await.unwrap().unwrap_err().code(),
            Some(&ErrorCode::PlanExists)
        );
        let answer = tokio::spawn({
            let control = handle.control();
            async move { control.answer_plan("p0", PlanAnswer::No).await }
        });
        let sent = next_json(&mut server).await;
        assert_eq!(sent["t"], "plan_answer");
        assert_eq!(sent["answer"], "no");
        send_json(
            &mut server,
            serde_json::json!({"t": "error", "rid": sent["rid"], "code": "rate_limited",
                "retryAfter": 3600}),
        )
        .await;
        assert_eq!(
            answer.await.unwrap().unwrap_err().retry_after(),
            Some(Duration::from_secs(3_600))
        );
        let cancel = tokio::spawn({
            let control = handle.control();
            async move { control.cancel_plan("p1").await }
        });
        let sent = next_json(&mut server).await;
        assert_eq!(sent["t"], "plan_cancel");
        send_json(
            &mut server,
            serde_json::json!({"t": "ack", "rid": sent["rid"]}),
        )
        .await;
        cancel.await.unwrap().unwrap();

        // What others do arrives as events.
        send_json(
            &mut server,
            serde_json::json!({"t": "plan_removed", "plan": "p0", "crew": "0a1b2c3d4e5f6a7b",
                "by": "111111111111111111"}),
        )
        .await;
        assert!(matches!(
            handle.next_event().await.unwrap(),
            PresenceEvent::PlanRemoved { plan, .. } if plan == "p0"
        ));
        send_json(
            &mut server,
            serde_json::json!({"t": "plans", "crew": "0a1b2c3d4e5f6a7b", "plans": []}),
        )
        .await;
        assert!(matches!(
            handle.next_event().await.unwrap(),
            PresenceEvent::Plans { plans, .. } if plans.is_empty()
        ));
        send_json(
            &mut server,
            serde_json::json!({"t": "team_requests", "crew": "0a1b2c3d4e5f6a7b",
                "added": "222222222222222222", "requests": [{"from": {"id": "222222222222222222",
                "username": "bora", "displayName": null, "avatar": "discord", "avatarHash": null},
                "sentAt": 1_792_521_320}]}),
        )
        .await;
        match handle.next_event().await.unwrap() {
            PresenceEvent::TeamRequests {
                requests, added, ..
            } => {
                assert_eq!(requests.len(), 1);
                assert_eq!(added.as_deref(), Some("222222222222222222"));
            }
            other => panic!("unexpected {other:?}"),
        }
        handle.shutdown().await;
    }

    #[tokio::test]
    async fn without_plans_on_the_connection_none_go_out() {
        let (listener, client) = start().await;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut handle = client.session(token()).connect_presence(fast());
        let mut server = accept(&listener, None, seen).await.unwrap();
        // An older service: no `plans` in `caps`.
        send_json(&mut server, hello()).await;
        assert!(matches!(
            handle.next_event().await.unwrap(),
            PresenceEvent::Connected { plans: None, .. }
        ));
        let refused = handle
            .control()
            .make_plan("0a1b2c3d4e5f6a7b", 1_792_612_800, None)
            .await
            .unwrap_err();
        assert_eq!(refused.code(), Some(&ErrorCode::BadRequest));
        // A plan message would be ignored too (it never reaches the owner).
        send_json(
            &mut server,
            serde_json::json!({"t": "plan", "plan": {"id": "p1", "crew": "c", "by": "1", "at": 900,
                "until": 8100, "answers": {}}, "quiet": false}),
        )
        .await;
        send_json(
            &mut server,
            serde_json::json!({"t": "status", "status": "dnd"}),
        )
        .await;
        assert_eq!(
            handle.next_event().await.unwrap(),
            PresenceEvent::StatusChanged(StatusMode::Dnd)
        );
        handle.shutdown().await;
    }

    #[tokio::test]
    async fn afk_notices_come_live_and_seen_goes_out_only_where_the_service_took_afk() {
        let (listener, client) = start().await;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut handle = client.session(token()).connect_presence(fast());
        let mut server = accept(&listener, None, seen).await.unwrap();
        let mut greeting = hello();
        greeting["caps"] = serde_json::json!(["callServer", "crews", "afk"]);
        send_json(&mut server, greeting).await;
        assert!(matches!(
            handle.next_event().await.unwrap(),
            PresenceEvent::Connected { .. }
        ));
        send_json(
            &mut server,
            serde_json::json!({"t": "afk_notice", "notice": {"id": "01JBXK4V7Q2M9R3T5W8Y0ZA1BC",
                "event": "approval", "kind": "afk_deal", "priority": "high", "at": 1792521000,
                "tag": "approval:01JBXK3D2E",
                "data": {"acct": "a_1", "name": "Ahmet_AFK", "price": "37500"}}}),
        )
        .await;
        match handle.next_event().await.unwrap() {
            PresenceEvent::AfkNotice(notice) => {
                assert_eq!(notice.event, "approval");
                assert_eq!(notice.get("price"), Some("37500"));
            }
            other => panic!("unexpected {other:?}"),
        }
        handle.control().afk_seen("01JBXK4V7Q2M9R3T5W8Y0ZA1BC");
        assert_eq!(
            next_json(&mut server).await,
            serde_json::json!({"t": "afk_seen", "id": "01JBXK4V7Q2M9R3T5W8Y0ZA1BC"})
        );
        handle.shutdown().await;

        // A service that did not take `afk` hears nothing of it.
        let (listener, client) = start().await;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut handle = client.session(token()).connect_presence(fast());
        let mut server = accept(&listener, None, seen).await.unwrap();
        send_json(&mut server, hello()).await;
        assert!(matches!(
            handle.next_event().await.unwrap(),
            PresenceEvent::Connected { .. }
        ));
        handle.control().afk_seen("01JBXK4V7Q2M9R3T5W8Y0ZA1BC");
        assert!(handle.set_status(LocalStatus::Singleplayer));
        assert_eq!(next_json(&mut server).await["t"], "status");
        handle.shutdown().await;
    }

    #[tokio::test]
    async fn crew_tags_come_live_only_where_the_service_took_them() {
        let (listener, client) = start().await;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut handle = client.session(token()).connect_presence(fast());
        let mut server = accept(&listener, None, seen).await.unwrap();
        let mut greeting = hello();
        greeting["caps"] = serde_json::json!(["callServer", "crews", "plans", "badges"]);
        send_json(&mut server, greeting).await;
        assert!(matches!(
            handle.next_event().await.unwrap(),
            PresenceEvent::Connected { .. }
        ));
        send_json(
            &mut server,
            serde_json::json!({"t": "badge", "id": "222222222222222222", "badge": {
                "crew": "0a1b2c3d4e5f6a7b", "tag": "GK", "color": "navy", "frame": "shield",
                "emblem": "bolt", "name": "Gece Kartalları", "motto": "", "createdOn": "2026-09-20",
                "members": 12, "requests": true}}),
        )
        .await;
        match handle.next_event().await.unwrap() {
            PresenceEvent::BadgeChanged {
                id,
                badge: Some(badge),
            } => {
                assert_eq!(id, "222222222222222222");
                assert_eq!((badge.tag.as_str(), badge.members), ("GK", 12));
            }
            other => panic!("unexpected {other:?}"),
        }
        send_json(
            &mut server,
            serde_json::json!({"t": "badge", "id": "222222222222222222", "badge": null}),
        )
        .await;
        assert_eq!(
            handle.next_event().await.unwrap(),
            PresenceEvent::BadgeChanged {
                id: "222222222222222222".into(),
                badge: None
            }
        );
        // Who just came in rides on the crew's message ("Hoş geldin").
        send_json(
            &mut server,
            serde_json::json!({"t": "team", "crew": "0a1b2c3d4e5f6a7b", "by": "100",
                "joined": ["222222222222222222"], "team": {"id": "0a1b2c3d4e5f6a7b",
                "name": "Gece", "tag": "GK", "emblem": "bolt", "code": "TKM-ABCD-EFGH",
                "createdOn": "2026-09-20", "members": []}}),
        )
        .await;
        assert!(matches!(
            handle.next_event().await.unwrap(),
            PresenceEvent::TeamChanged { joined, .. } if joined == ["222222222222222222"]
        ));
        handle.shutdown().await;

        // A service that did not take `badges`: its tag messages never reach the launcher.
        let (listener, client) = start().await;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut handle = client.session(token()).connect_presence(fast());
        let mut server = accept(&listener, None, seen).await.unwrap();
        send_json(&mut server, hello()).await;
        assert!(matches!(
            handle.next_event().await.unwrap(),
            PresenceEvent::Connected { .. }
        ));
        send_json(
            &mut server,
            serde_json::json!({"t": "badge", "id": "222222222222222222", "badge": null}),
        )
        .await;
        send_json(
            &mut server,
            serde_json::json!({"t": "status", "status": "dnd"}),
        )
        .await;
        assert_eq!(
            handle.next_event().await.unwrap(),
            PresenceEvent::StatusChanged(StatusMode::Dnd)
        );
        handle.shutdown().await;
    }

    #[tokio::test]
    async fn marks_come_live_only_where_the_service_took_them() {
        let (listener, client) = start().await;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut handle = client.session(token()).connect_presence(fast());
        let mut server = accept(&listener, None, seen).await.unwrap();
        let mut greeting = hello();
        greeting["caps"] = serde_json::json!(["callServer", "crews", "marks"]);
        send_json(&mut server, greeting).await;
        assert!(matches!(
            handle.next_event().await.unwrap(),
            PresenceEvent::Connected { .. }
        ));
        send_json(
            &mut server,
            serde_json::json!({"t": "mark", "id": "222222222222222222", "mark": "verified"}),
        )
        .await;
        assert_eq!(
            handle.next_event().await.unwrap(),
            PresenceEvent::MarkChanged {
                id: "222222222222222222".into(),
                mark: Some(ProfileMark::Verified)
            }
        );
        send_json(
            &mut server,
            serde_json::json!({"t": "mark", "id": "222222222222222222", "mark": null}),
        )
        .await;
        assert_eq!(
            handle.next_event().await.unwrap(),
            PresenceEvent::MarkChanged {
                id: "222222222222222222".into(),
                mark: None
            }
        );
        handle.shutdown().await;

        // A service that did not take `marks`: its mark messages never reach the launcher.
        let (listener, client) = start().await;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut handle = client.session(token()).connect_presence(fast());
        let mut server = accept(&listener, None, seen).await.unwrap();
        send_json(&mut server, hello()).await;
        assert!(matches!(
            handle.next_event().await.unwrap(),
            PresenceEvent::Connected { .. }
        ));
        send_json(
            &mut server,
            serde_json::json!({"t": "mark", "id": "222222222222222222", "mark": "admin"}),
        )
        .await;
        send_json(
            &mut server,
            serde_json::json!({"t": "status", "status": "dnd"}),
        )
        .await;
        assert_eq!(
            handle.next_event().await.unwrap(),
            PresenceEvent::StatusChanged(StatusMode::Dnd)
        );
        handle.shutdown().await;
    }

    #[tokio::test]
    async fn chat_syncs_after_connecting_and_answers_by_rid() {
        let (listener, client) = start().await;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let setup = ChatSetup::default();
        setup.since.store(1_792_520_000_000, Ordering::Relaxed);
        let mut handle = client
            .session(token())
            .connect_presence_with_chat(fast(), setup.clone());
        let mut server = accept(&listener, None, seen.clone()).await.unwrap();
        assert!(
            seen.lock()
                .unwrap()
                .iter()
                .any(|(k, v)| k == ":uri"
                    && v == "/v1/presence?caps=callServer,crews,plans,badges,mc,afk,marks,profile,chat,replies,stickers,reactions,typing,reads"),
            "chat and its extras are asked for"
        );
        // A service from before CHAT2 takes chat alone.
        let mut greeting = hello();
        greeting["caps"] = serde_json::json!(["callServer", "crews", "chat"]);
        greeting["chat"] = serde_json::json!({"on": true, "mailbox": true, "mailboxBack": null,
            "perDay": 500, "mutedUntil": null});
        send_json(&mut server, greeting).await;
        match handle.next_event().await.unwrap() {
            PresenceEvent::Connected { chat, .. } => assert_eq!(chat.map(|c| c.per_day), Some(500)),
            other => panic!("unexpected {other:?}"),
        }
        // The sync starts at once, from where the device is.
        let sync = next_json(&mut server).await;
        assert_eq!(sync["t"], "chat_sync");
        assert_eq!(sync["since"], 1_792_520_000_000_i64);
        assert!(sync.get("after").is_none());
        send_json(
            &mut server,
            serde_json::json!({"t": "chat_sync", "conv": "c:0123456789abcdef",
                "messages": [{"id": 1_792_520_100_000_i64, "from": "7", "text": "selam", "mac": "m"}],
                "deleted": [{"id": 5, "by": "7", "at": 6}]}),
        )
        .await;
        send_json(
            &mut server,
            serde_json::json!({"t": "chat_synced", "rid": sync["rid"], "now": 9, "more": "k1"}),
        )
        .await;
        // The rest comes with the cursor, from the same place.
        let more = next_json(&mut server).await;
        assert_eq!(more["t"], "chat_sync");
        assert_eq!(more["since"], 1_792_520_000_000_i64);
        assert_eq!(more["after"], "k1");
        send_json(
            &mut server,
            serde_json::json!({"t": "chat_synced", "rid": more["rid"], "now": 1_792_521_000_000_i64}),
        )
        .await;
        match handle.next_event().await.unwrap() {
            PresenceEvent::ChatSync {
                conv,
                messages,
                deleted,
                ..
            } => {
                assert_eq!(
                    messages[0].conv, conv,
                    "each message knows its conversation"
                );
                assert_eq!(deleted[0].id, 5);
            }
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(
            handle.next_event().await.unwrap(),
            PresenceEvent::ChatSynced {
                clock: Some(1_792_521_000_000),
                caught_up: false
            }
        );

        // A send is answered by its rid with the message as stored, which also arrives as an
        // event.
        let send = tokio::spawn({
            let control = handle.control();
            async move {
                control
                    .chat_send("c:0123456789abcdef", "Geliyorum", "q8Zr3kT0aWbc")
                    .await
            }
        });
        let sent = next_json(&mut server).await;
        assert_eq!(sent["t"], "chat");
        assert_eq!(sent["nonce"], "q8Zr3kT0aWbc");
        let stored = serde_json::json!({"conv": "c:0123456789abcdef", "id": 1_792_521_000_500_i64,
            "from": "100", "text": "Geliyorum", "mac": "x", "nonce": "q8Zr3kT0aWbc"});
        send_json(
            &mut server,
            serde_json::json!({"t": "ack", "rid": sent["rid"], "msg": stored}),
        )
        .await;
        let message = send.await.unwrap().unwrap();
        assert_eq!(message.id, 1_792_521_000_500);
        assert!(matches!(
            handle.next_event().await.unwrap(),
            PresenceEvent::ChatMessage(m) if m.nonce.as_deref() == Some("q8Zr3kT0aWbc")
        ));

        // A refused edit says why.
        let edit = tokio::spawn({
            let control = handle.control();
            async move { control.chat_edit(&message, "discord.gg/x").await }
        });
        let edited = next_json(&mut server).await;
        assert_eq!(edited["t"], "chat_edit");
        assert!(edited["msg"].get("nonce").is_none());
        send_json(
            &mut server,
            serde_json::json!({"t": "error", "rid": edited["rid"], "code": "chat_rejected",
                "reason": "link"}),
        )
        .await;
        assert!(matches!(
            edit.await.unwrap().unwrap_err(),
            SocialError::ChatRejected {
                reason: ChatRejection::Link,
                rule: None
            }
        ));
        // Without the extras on this connection, nothing of them goes out: a sticker or a
        // reaction fails at once, typing and read marks stay home.
        let control = handle.control();
        let sticker = control
            .chat_post(
                "c:0123456789abcdef",
                &ChatBody::Sticker("pl1/gg".into()),
                None,
                "n2",
            )
            .await
            .unwrap_err();
        assert_eq!(
            (sticker.code(), sticker.detail()),
            (Some(&ErrorCode::ChatFeatureOff), Some("stickers"))
        );
        let reaction = control
            .chat_react("c:0123456789abcdef", 1_792_521_000_500, "👍", true)
            .await
            .unwrap_err();
        assert_eq!(reaction.detail(), Some("reactions"));
        control.chat_typing("c:0123456789abcdef");
        control.chat_read("c:0123456789abcdef", 1_792_521_000_500);

        // Live messages, edits and deletes.
        send_json(
            &mut server,
            serde_json::json!({"t": "chat_deleted", "conv": "c:0123456789abcdef",
                "id": 1_792_521_000_500_i64, "by": "staff", "at": 1}),
        )
        .await;
        assert_eq!(
            handle.next_event().await.unwrap(),
            PresenceEvent::ChatDeleted {
                conv: "c:0123456789abcdef".into(),
                id: 1_792_521_000_500,
                by: "staff".into(),
                at: 1
            }
        );
        send_json(
            &mut server,
            serde_json::json!({"t": "chat_state", "chat": {"on": true, "mailbox": false,
                "mailboxBack": 1_792_540_800, "perDay": 500, "mutedUntil": 1_792_600_000}}),
        )
        .await;
        assert!(matches!(
            handle.next_event().await.unwrap(),
            PresenceEvent::ChatState(state) if !state.mailbox && state.muted_until.is_some()
        ));

        // A suspension: the goodbye says so before the close.
        send_json(
            &mut server,
            serde_json::json!({"t": "bye", "reason": "restricted", "until": 1_792_600_000}),
        )
        .await;
        server
            .close(Some(CloseFrame {
                code: CloseCode::from(4001),
                reason: "restricted".into(),
            }))
            .await
            .unwrap();
        assert_eq!(
            handle.next_event().await.unwrap(),
            PresenceEvent::Stopped(StopReason::Restricted {
                until: Some(1_792_600_000)
            })
        );
    }

    #[tokio::test]
    async fn without_chat_nothing_of_it_flows() {
        let (listener, client) = start().await;
        let seen = Arc::new(Mutex::new(Vec::new()));
        // Asked for, but an older service does not take it.
        let mut handle = client
            .session(token())
            .connect_presence_with_chat(fast(), ChatSetup::default());
        let mut server = accept(&listener, None, seen).await.unwrap();
        send_json(&mut server, hello()).await;
        assert!(matches!(
            handle.next_event().await.unwrap(),
            PresenceEvent::Connected { chat: None, .. }
        ));
        // No sync, a chat frame is ignored, and a send fails at once.
        send_json(
            &mut server,
            serde_json::json!({"t": "chat", "msg": {"conv": "c:0123456789abcdef", "id": 1,
                "from": "7", "text": "x", "mac": "m"}}),
        )
        .await;
        let error = handle
            .control()
            .chat_send("c:0123456789abcdef", "x", "n")
            .await
            .unwrap_err();
        assert_eq!(error.code(), Some(&ErrorCode::ChatOff));
        // The next frame the server reads is the status, not a sync.
        handle.set_status(LocalStatus::Singleplayer);
        assert_eq!(next_json(&mut server).await["t"], "status");
        handle.shutdown().await;
    }

    #[tokio::test]
    async fn chat_extras_go_out_only_as_the_service_takes_them() {
        let (listener, client) = start().await;
        let seen = Arc::new(Mutex::new(Vec::new()));
        // The device gains the extras: its first sync reaches back a week.
        let setup = ChatSetup::from_since(1_792_520_000_000);
        setup
            .catch_up
            .store(1_792_520_000_000 - 7 * 86_400_000, Ordering::Relaxed);
        let mut handle = client
            .session(token())
            .connect_presence_with_chat(fast(), setup.clone());
        let mut server = accept(&listener, None, seen).await.unwrap();
        let mut greeting = hello();
        greeting["caps"] = serde_json::json!([
            "callServer",
            "crews",
            "chat",
            "replies",
            "stickers",
            "reactions",
            "typing",
            "reads"
        ]);
        greeting["chat"] = serde_json::json!({"on": true, "mailbox": true, "mailboxBack": null,
            "perDay": 500, "mutedUntil": null, "stickers": ["pl1"],
            "extras": {"replies": true, "stickers": true, "reactions": true, "typing": true,
                "reads": true, "links": true},
            "links": {"rules": 2, "trusted": ["projects.gg"], "blocked": [], "invites": true}});
        send_json(&mut server, greeting).await;
        match handle.next_event().await.unwrap() {
            PresenceEvent::Connected {
                chat: Some(chat), ..
            } => {
                assert!(chat.caps.stickers && chat.caps.reads);
                assert!(chat.offered().unwrap().reactions);
            }
            other => panic!("unexpected {other:?}"),
        }
        let sync = next_json(&mut server).await;
        assert_eq!(sync["since"], 1_792_520_000_000_i64 - 7 * 86_400_000);
        send_json(
            &mut server,
            serde_json::json!({"t": "chat_sync", "conv": "d:1:2",
                "messages": [{"id": 1_792_520_100_000_i64, "from": "2", "sticker": "pl1/hi", "mac": "m"}],
                "deleted": [], "reacted": [{"id": 9, "reactions": [{"e": "👍", "by": ["2"]}]}],
                "reads": [{"by": "2", "id": 1_792_520_100_000_i64}]}),
        )
        .await;
        send_json(
            &mut server,
            serde_json::json!({"t": "chat_synced", "rid": sync["rid"], "now": 9}),
        )
        .await;
        match handle.next_event().await.unwrap() {
            PresenceEvent::ChatSync {
                messages,
                reacted,
                reads,
                ..
            } => {
                assert_eq!(messages[0].sticker.as_deref(), Some("pl1/hi"));
                assert_eq!(reacted[0].id, 9);
                assert_eq!(reads[0].by, "2");
            }
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(
            handle.next_event().await.unwrap(),
            PresenceEvent::ChatSynced {
                clock: Some(9),
                caught_up: true
            }
        );

        let control = handle.control();
        // A sticker answering a message goes with that message as held.
        let answered = ChatMessage {
            conv: "d:1:2".into(),
            id: 1_792_520_100_000,
            from: "2".into(),
            mac: "m".into(),
            sticker: Some("pl1/hi".into()),
            ..ChatMessage::default()
        };
        let post = tokio::spawn({
            let control = control.clone();
            let answered = answered.clone();
            async move {
                control
                    .chat_post(
                        "d:1:2",
                        &ChatBody::Sticker("pl1/gg".into()),
                        Some(&answered),
                        "n1",
                    )
                    .await
            }
        });
        let frame = next_json(&mut server).await;
        assert_eq!(frame["sticker"], "pl1/gg");
        assert!(frame.get("text").is_none());
        assert_eq!(frame["reply"]["msg"]["sticker"], "pl1/hi");
        send_json(
            &mut server,
            serde_json::json!({"t": "ack", "rid": frame["rid"], "msg": {"conv": "d:1:2",
                "id": 1_792_520_200_000_i64, "from": "1", "sticker": "pl1/gg", "mac": "s",
                "nonce": "n1", "reply": {"id": 1_792_520_100_000_i64, "from": "2"}, "rmac": "r"}}),
        )
        .await;
        let stored = post.await.unwrap().unwrap();
        assert_eq!(stored.reply.as_ref().map(|r| r.id), Some(1_792_520_100_000));
        assert!(matches!(
            handle.next_event().await.unwrap(),
            PresenceEvent::ChatMessage(m) if m.sticker.as_deref() == Some("pl1/gg")
        ));

        // A reaction is answered with the message's reactions.
        let react = tokio::spawn({
            let control = control.clone();
            async move {
                control
                    .chat_react("d:1:2", 1_792_520_100_000, "😂", true)
                    .await
            }
        });
        let frame = next_json(&mut server).await;
        assert_eq!(
            (
                frame["t"].as_str(),
                frame["e"].as_str(),
                frame["on"].as_bool()
            ),
            (Some("chat_react"), Some("😂"), Some(true))
        );
        send_json(
            &mut server,
            serde_json::json!({"t": "ack", "rid": frame["rid"],
                "reactions": [{"e": "😂", "by": ["1"]}]}),
        )
        .await;
        assert_eq!(react.await.unwrap().unwrap()[0].by, ["1"]);

        // Typing and read marks go out without a rid; nothing answers them.
        control.chat_typing("d:1:2");
        assert_eq!(
            next_json(&mut server).await,
            serde_json::json!({"t": "chat_typing", "conv": "d:1:2"})
        );
        control.chat_read("d:1:2", 1_792_520_100_000);
        assert_eq!(
            next_json(&mut server).await,
            serde_json::json!({"t": "chat_read", "conv": "d:1:2", "id": 1_792_520_100_000_i64})
        );

        // A card, then a link the rules refuse.
        let card = tokio::spawn({
            let control = control.clone();
            async move { control.chat_unfurl("https://youtu.be/dQw4w9WgXcQ").await }
        });
        let frame = next_json(&mut server).await;
        assert_eq!(frame["t"], "chat_unfurl");
        send_json(
            &mut server,
            serde_json::json!({"t": "ack", "rid": frame["rid"], "card": {"url": "https://youtu.be/dQw4w9WgXcQ",
                "site": "YouTube", "icon": "youtube", "kind": "video", "title": "Bir video"}}),
        )
        .await;
        assert_eq!(card.await.unwrap().unwrap().site, "YouTube");
        let refused = tokio::spawn({
            let control = control.clone();
            async move { control.chat_unfurl("https://grabify.link/x").await }
        });
        let frame = next_json(&mut server).await;
        send_json(
            &mut server,
            serde_json::json!({"t": "error", "rid": frame["rid"], "code": "unfurl_failed", "rule": "grabber"}),
        )
        .await;
        let error = refused.await.unwrap().unwrap_err();
        assert_eq!(error.detail(), Some("grabber"));

        // What others do arrives as events.
        send_json(
            &mut server,
            serde_json::json!({"t": "chat_reacted", "conv": "d:1:2", "id": 1_792_520_100_000_i64,
                "by": "2", "e": "😂", "on": true}),
        )
        .await;
        assert!(matches!(
            handle.next_event().await.unwrap(),
            PresenceEvent::ChatReacted { on: true, ref by, .. } if by == "2"
        ));
        send_json(
            &mut server,
            serde_json::json!({"t": "chat_typing", "conv": "d:1:2", "by": "2"}),
        )
        .await;
        assert!(matches!(
            handle.next_event().await.unwrap(),
            PresenceEvent::ChatTyping { ref by, .. } if by == "2"
        ));
        send_json(
            &mut server,
            serde_json::json!({"t": "chat_read", "conv": "d:1:2", "by": "2", "id": 1_792_520_200_000_i64}),
        )
        .await;
        assert!(matches!(
            handle.next_event().await.unwrap(),
            PresenceEvent::ChatRead {
                id: 1_792_520_200_000,
                ..
            }
        ));
        // A new state keeps what the socket took.
        send_json(
            &mut server,
            serde_json::json!({"t": "chat_state", "chat": {"on": true, "mailbox": true,
                "perDay": 500, "stickers": ["pl1"], "extras": {"replies": true, "stickers": false,
                "reactions": true, "typing": true, "reads": true, "links": false}}}),
        )
        .await;
        match handle.next_event().await.unwrap() {
            PresenceEvent::ChatState(state) => {
                let offered = state.offered().unwrap();
                assert!(offered.replies && !offered.stickers && !offered.links);
                assert!(state.caps.stickers, "the socket still takes them");
            }
            other => panic!("unexpected {other:?}"),
        }
        handle.shutdown().await;
    }

    #[tokio::test]
    async fn world_frames_go_out_by_rid_and_come_back_as_world_events() {
        use crate::world::{Hidden, WorldRole, WorldsStatus};
        let (listener, client) = start().await;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut handle = client
            .session(token())
            .connect_presence_with(fast(), None, true);
        let mut server = accept(&listener, None, seen.clone()).await.unwrap();
        assert!(seen.lock().unwrap().iter().any(|(name, value)| {
            name == ":uri" && value.contains("caps=") && value.ends_with(",worlds2")
        }));
        let mut hello = hello();
        hello["caps"] = serde_json::json!(["worlds2"]);
        hello["worlds"] = serde_json::json!({"on": true, "maxGuests": 5});
        send_json(&mut server, hello).await;
        assert!(matches!(
            handle.next_event().await,
            Some(PresenceEvent::Connected { .. })
        ));
        assert_eq!(
            handle.next_event().await,
            Some(PresenceEvent::World(WorldEvent::Hello {
                taken: true,
                status: WorldsStatus {
                    on: true,
                    max_guests: 5
                }
            }))
        );

        // A request goes out with its rid; the ack carries the order and the credentials.
        let control = handle.control();
        let asking = tokio::spawn(async move {
            let frame = WorldFrame::TurnCreds {
                session: "s".into(),
                host: Some("h".into()),
                role: WorldRole::Guest,
                reason: "primary",
            };
            control.world_request(frame, Duration::from_secs(5)).await
        });
        let sent = next_json(&mut server).await;
        assert_eq!(sent["t"], "turn_creds");
        assert_eq!(sent["reason"], "primary");
        send_json(
            &mut server,
            serde_json::json!({"t": "ack", "rid": sent["rid"], "order": ["turn", "e4mc"],
                "ice": {"urls": ["turn:turn.example:3478?transport=udp"], "username": "u",
                        "credential": "c", "ttl": 7200}}),
        )
        .await;
        let answer = asking.await.unwrap().unwrap();
        assert_eq!(answer.order, ["turn", "e4mc"]);
        let ice = answer.ice.unwrap();
        assert_eq!((ice.username.expose().as_str(), ice.ttl), ("u", 7200));

        // An error comes back as the service said it.
        let control = handle.control();
        let asking = tokio::spawn(async move {
            let frame = WorldFrame::TurnCreds {
                session: "s".into(),
                host: Some("h".into()),
                role: WorldRole::Guest,
                reason: "fallback",
            };
            control.world_request(frame, Duration::from_secs(5)).await
        });
        let sent = next_json(&mut server).await;
        send_json(
            &mut server,
            serde_json::json!({"t": "error", "rid": sent["rid"], "code": "turn_denied",
                "reason": "budget"}),
        )
        .await;
        assert_eq!(
            asking.await.unwrap(),
            Err(WorldError::Refused {
                code: "turn_denied".into(),
                reason: Some("budget".into()),
                item: None,
                retry_after: None,
            })
        );

        // Frames nobody answers go without a rid.
        handle.control().world_send(WorldFrame::TurnRelease {
            username: Hidden("u".into()),
        });
        assert_eq!(
            next_json(&mut server).await,
            serde_json::json!({"t": "turn_release", "username": "u"})
        );

        // The service's frames become world events.
        send_json(
            &mut server,
            serde_json::json!({"t": "world_invite", "quiet": true, "invite": {
                "id": "0123456789abcdef", "from": "f1", "at": 5, "session": "s",
                "join": {"v": 1}}}),
        )
        .await;
        match handle.next_event().await {
            Some(PresenceEvent::World(WorldEvent::Invite { invite, quiet })) => {
                assert!(quiet);
                assert_eq!((invite.from.as_str(), invite.at), ("f1", 5));
                assert_eq!(invite.join.expose()["v"], 1);
            }
            other => panic!("unexpected {other:?}"),
        }
        send_json(
            &mut server,
            serde_json::json!({"t": "world_revoked", "session": "s", "to": "f2"}),
        )
        .await;
        assert_eq!(
            handle.next_event().await,
            Some(PresenceEvent::World(WorldEvent::RevokedGuest {
                session: "s".into(),
                to: "f2".into()
            }))
        );
        send_json(
            &mut server,
            serde_json::json!({"t": "world_revoked", "session": "s", "from": "f1"}),
        )
        .await;
        assert_eq!(
            handle.next_event().await,
            Some(PresenceEvent::World(WorldEvent::Revoked {
                from: "f1".into(),
                session: "s".into()
            }))
        );
        send_json(
            &mut server,
            serde_json::json!({"t": "turn_move", "session": "s", "reason": "budget", "grace": 20}),
        )
        .await;
        assert_eq!(
            handle.next_event().await,
            Some(PresenceEvent::World(WorldEvent::TurnMove {
                session: "s".into(),
                reason: "budget".into(),
                grace: 20
            }))
        );
        send_json(
            &mut server,
            serde_json::json!({"t": "world_state", "worlds": {"on": false, "maxGuests": 3}}),
        )
        .await;
        assert_eq!(
            handle.next_event().await,
            Some(PresenceEvent::World(WorldEvent::Status(WorldsStatus {
                on: false,
                max_guests: 3
            })))
        );
        handle.shutdown().await;
    }

    #[tokio::test]
    async fn without_worlds_on_the_connection_none_of_them_flows() {
        use crate::world::WorldRole;
        let (listener, client) = start().await;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut handle = client.session(token()).connect_presence(fast());
        let mut server = accept(&listener, None, seen.clone()).await.unwrap();
        assert!(
            !seen
                .lock()
                .unwrap()
                .iter()
                .any(|(_, value)| value.contains("worlds"))
        );
        send_json(&mut server, hello()).await;
        assert!(matches!(
            handle.next_event().await,
            Some(PresenceEvent::Connected { .. })
        ));
        let frame = WorldFrame::TurnCreds {
            session: "s".into(),
            host: None,
            role: WorldRole::Host,
            reason: "primary",
        };
        assert_eq!(
            handle
                .control()
                .world_request(frame, Duration::from_secs(5))
                .await,
            Err(WorldError::Off)
        );
        // An invite on a connection that did not take worlds is dropped; the next event is the
        // presence after it.
        send_json(
            &mut server,
            serde_json::json!({"t": "world_invite", "invite": {"from": "f1", "session": "s",
                "join": {}}}),
        )
        .await;
        send_json(
            &mut server,
            serde_json::json!({"t": "presence", "presence": {"id": "111111111111111111",
                "state": "offline", "status": "online", "game": null}}),
        )
        .await;
        assert!(matches!(
            handle.next_event().await,
            Some(PresenceEvent::Presence(_))
        ));
        handle.shutdown().await;
    }
}
