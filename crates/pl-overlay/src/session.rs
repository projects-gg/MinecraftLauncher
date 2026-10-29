//! One game's overlay session: the bridge's live connection, fed from the friends feature.
//!
//! An actor task owns everything a session knows (the newest view, what the mod was told, the
//! panel's messages on their way) and waits on two channels: what the friends feature says
//! ([`Msg`], forwarded by [`crate::Overlay`]) and what the bridge says ([`BridgeEvent`]). It never
//! waits on the network itself: answering a call, reacting and asking for a link's card run in
//! tasks of their own.

use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use pl_content::ChatEmojis;
use pl_lang::Lang;
use pl_social::{
    CallAnswer as SocialAnswer, ChatChangeView, ChatMessageView, ChatPage, ChatRejection,
    ChatTypingView, ErrorCode, FriendGroup, GroupMember, InviteResult, McIntent, ReportReason,
    ReportTarget, SendOutcome, SocialError, SocialNotice, SocialService, SocialView,
    TEAM_FEATURE_ROLES, TeamRole,
};
use serde::Serialize;
use tokio::sync::{mpsc, oneshot};

use crate::OverlayConfig;
use crate::feed::{self, Listener, Mirror, Scene, Sends};
use crate::host::{HostSignal, HostSink, WorldsShown};
use crate::protocol::{
    ByeReason, CallAnswer, FEATURE_CONNECT, FEATURE_EMOJI, FEATURE_HOST, FEATURE_LANG,
    FEATURE_LINK, FEATURE_PANEL_OFF, FEATURE_PEOPLE, FEATURE_PEOPLE2, FEATURE_PEOPLE3,
    FEATURE_TAGS, FriendAction, FriendRefusal, FromGame, MAX_MESSAGE_CHARS, ReactFailure, Secret,
    SendFailure, ToGame,
};
use crate::server::{BridgeEvent, ConnId, Control};

/// How long "Bağlan" waits for the game to say whether it switches servers.
pub const CONNECT_WAIT: Duration = Duration::from_secs(5);

/// SKIN1-O: a permit with less left than this is not handed out again: the server still has to
/// ask ProBot, wait for a free screen (up to 30 s) and give the player two minutes to answer.
const PERMIT_MARGIN_SECS: i64 = 5 * 60;
/// SKIN1-O: permits one game session opens at most (each lives 30 minutes; the service allows 20 a
/// day). Projects asks once a connection at most, so a session needs one or two.
const MAX_PERMITS: u32 = 6;
/// SKIN1-O: `link-want` is heard at most this often (a server asks once a connection; anything
/// faster is a stuck mod). The service is asked at most [`MAX_PERMITS`] times anyway.
const LINK_WANT_GAP: Duration = Duration::from_millis(250);

/// What asking the running game to join a server led to ([`crate::Overlay::connect_in_game`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum ConnectOutcome {
    /// The game is on its way to the server.
    Done,
    /// The overlay cannot do it now: start the game again as before. `reason`: `no-overlay` (no
    /// game, or no mod connected), `unsupported` (an older mod), `busy` (another one is on its
    /// way), `not-allowed` (not Projects nor an address shared now), `timeout` (no answer in 5 s),
    /// or the mod's own (`busy`, `refused`, …).
    Unavailable { reason: String },
}

impl ConnectOutcome {
    pub(crate) fn unavailable(reason: &str) -> Self {
        Self::Unavailable {
            reason: reason.to_owned(),
        }
    }
}

/// A mark the player changed on a friend from the game's person menu (OVL-UX2): the launcher's
/// page keeps these (its friend list's star and "Çevrimiçi olunca bildir"), so it hears them as
/// the `overlay-people` event and keeps them as its own buttons would.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PeopleChange {
    /// "Favorilere ekle" (`on`) or "Favorilerden çıkar".
    Favorite { friend: String, on: bool },
    /// "Çevrimiçi olunca bildir" on or off.
    Alert { friend: String, on: bool },
}

/// A "Bağlan" the game has not answered yet.
struct PendingConnect {
    id: String,
    asked: Instant,
    reply: oneshot::Sender<ConnectOutcome>,
}

/// What the overlay asks of the friends feature. [`SocialService`] is the real one; tests use a
/// fake.
pub trait SocialActions: Clone + Send + Sync + 'static {
    fn view(&self) -> SocialView;
    /// The game showed Cloud AFK's notice `id` (AFKPLUS).
    fn afk_seen(&self, _id: &str) {}
    fn chat_page(&self, conv: &str) -> ChatPage;
    fn chat_send(&self, conv: &str, text: &str) -> pl_social::Result<String>;
    /// CHAT2: a message answering message `reply` (its service id).
    fn chat_reply(&self, conv: &str, text: &str, reply: i64) -> pl_social::Result<String>;
    /// CHAT2: a Projects sticker, maybe as a reply.
    fn chat_sticker(
        &self,
        conv: &str,
        sticker: &str,
        reply: Option<i64>,
    ) -> pl_social::Result<String>;
    fn chat_discard(&self, key: &str);
    fn chat_read(&self, conv: &str);
    /// CHAT2: the player writes in a conversation ("yazıyor…"; the friends feature paces it).
    fn chat_typing(&self, conv: &str);
    /// CHAT2: gives (`on`) or takes back the player's reaction on message `id`.
    fn chat_react(
        &self,
        conv: &str,
        id: i64,
        emoji: &str,
        on: bool,
    ) -> impl Future<Output = pl_social::Result<()>> + Send + 'static;
    /// CHAT2: asks for the card of a message's first link; the message comes back with it.
    fn chat_unfurl(
        &self,
        conv: &str,
        key: &str,
    ) -> impl Future<Output = pl_social::Result<()>> + Send + 'static;
    fn dismiss_invite(&self, id: &str);
    fn answer_call(
        &self,
        call: &str,
        answer: SocialAnswer,
    ) -> impl Future<Output = pl_social::Result<bool>> + Send + 'static;
    /// SKIN1 §3.7: signed in to friends, and the service offers codeless linking now.
    fn codeless_offered(&self) -> bool;
    /// SKIN1 §3.7: a codeless linking permit for game account `name`; None when there is none
    /// (not signed in, refused, unreachable).
    fn codeless_permit(
        &self,
        name: &str,
    ) -> impl Future<Output = Option<McIntent>> + Send + 'static;
    /// SKIN1 §3.7: the permit stops working (the game that held it closed).
    fn codeless_close(&self) -> impl Future<Output = ()> + Send + 'static;
    /// OVL-UX: "Birlikte oyna" to a friend, from the game's friend menu.
    fn invite(
        &self,
        friend: &str,
    ) -> impl Future<Output = pl_social::Result<InviteResult>> + Send + 'static;
    /// OVL-UX: "Arkadaşlıktan çıkar".
    fn remove_friend(
        &self,
        friend: &str,
    ) -> impl Future<Output = pl_social::Result<()>> + Send + 'static;
    /// OVL-UX: "Engelle".
    fn block(&self, friend: &str) -> impl Future<Output = pl_social::Result<()>> + Send + 'static;
    /// OVL-UX: "Rapor et" about a friend: a reason, a note for moderators, and whether to block
    /// them too.
    fn report_player(
        &self,
        friend: &str,
        reason: ReportReason,
        note: Option<String>,
        block: bool,
    ) -> impl Future<Output = pl_social::Result<()>> + Send + 'static;
    /// OVL-UX2: "Kategoriye taşı": the player's friend categories, saved whole as the launcher's
    /// list saves them.
    fn save_groups(
        &self,
        groups: Vec<FriendGroup>,
        members: Vec<GroupMember>,
    ) -> impl Future<Output = pl_social::Result<()>> + Send + 'static;
    /// OVL-UX2: "Ekibe davet et".
    fn invite_to_team(
        &self,
        team: &str,
        friend: &str,
    ) -> impl Future<Output = pl_social::Result<()>> + Send + 'static;
    /// OVL-UX2: "Arkadaş ekle" on a crewmate (a request without their code).
    fn request_crewmate(
        &self,
        user: &str,
    ) -> impl Future<Output = pl_social::Result<SendOutcome>> + Send + 'static;
    /// OVL-UX2: "İsteği kabul et" on a crewmate who asked first.
    fn accept(&self, user: &str) -> impl Future<Output = pl_social::Result<()>> + Send + 'static;
    /// OVL-UX3: "Moderatör yap" (`moderator`) or "Moderatörlükten al" in a crew the player leads.
    fn set_team_moderator(
        &self,
        team: &str,
        member: &str,
        moderator: bool,
    ) -> impl Future<Output = pl_social::Result<()>> + Send + 'static;
    /// OVL-UX3: "Liderliği devret".
    fn transfer_team_founder(
        &self,
        team: &str,
        member: &str,
    ) -> impl Future<Output = pl_social::Result<()>> + Send + 'static;
    /// OVL-UX3: "Ekipten çıkar".
    fn remove_team_member(
        &self,
        team: &str,
        member: &str,
    ) -> impl Future<Output = pl_social::Result<()>> + Send + 'static;
}

impl SocialActions for SocialService {
    fn view(&self) -> SocialView {
        SocialService::view(self)
    }

    fn afk_seen(&self, id: &str) {
        SocialService::afk_seen(self, id);
    }

    fn chat_page(&self, conv: &str) -> ChatPage {
        SocialService::chat_page(self, conv, None)
    }

    fn chat_send(&self, conv: &str, text: &str) -> pl_social::Result<String> {
        SocialService::chat_send(self, conv, text)
    }

    fn chat_reply(&self, conv: &str, text: &str, reply: i64) -> pl_social::Result<String> {
        SocialService::chat_reply(self, conv, text, reply)
    }

    fn chat_sticker(
        &self,
        conv: &str,
        sticker: &str,
        reply: Option<i64>,
    ) -> pl_social::Result<String> {
        SocialService::chat_sticker(self, conv, sticker, reply)
    }

    fn chat_discard(&self, key: &str) {
        SocialService::chat_discard(self, key);
    }

    fn chat_read(&self, conv: &str) {
        SocialService::chat_read(self, conv);
    }

    fn chat_typing(&self, conv: &str) {
        SocialService::chat_typing(self, conv);
    }

    fn chat_react(
        &self,
        conv: &str,
        id: i64,
        emoji: &str,
        on: bool,
    ) -> impl Future<Output = pl_social::Result<()>> + Send + 'static {
        let service = self.clone();
        let conv = conv.to_owned();
        let emoji = emoji.to_owned();
        async move { service.chat_react(&conv, id, &emoji, on).await }
    }

    fn chat_unfurl(
        &self,
        conv: &str,
        key: &str,
    ) -> impl Future<Output = pl_social::Result<()>> + Send + 'static {
        let service = self.clone();
        let conv = conv.to_owned();
        let key = key.to_owned();
        async move { service.chat_unfurl(&conv, &key).await }
    }

    fn dismiss_invite(&self, id: &str) {
        SocialService::dismiss_invite(self, id);
    }

    fn answer_call(
        &self,
        call: &str,
        answer: SocialAnswer,
    ) -> impl Future<Output = pl_social::Result<bool>> + Send + 'static {
        let service = self.clone();
        let call = call.to_owned();
        async move { service.answer_call(&call, answer).await }
    }

    fn codeless_offered(&self) -> bool {
        self.mc_codeless_offered()
    }

    fn codeless_permit(
        &self,
        name: &str,
    ) -> impl Future<Output = Option<McIntent>> + Send + 'static {
        let service = self.clone();
        let name = name.to_owned();
        async move { service.mc_codeless_permit(&name).await }
    }

    fn codeless_close(&self) -> impl Future<Output = ()> + Send + 'static {
        let service = self.clone();
        async move { service.mc_codeless_close().await }
    }

    fn invite(
        &self,
        friend: &str,
    ) -> impl Future<Output = pl_social::Result<InviteResult>> + Send + 'static {
        let service = self.clone();
        let friend = friend.to_owned();
        async move { SocialService::invite(&service, &friend).await }
    }

    fn remove_friend(
        &self,
        friend: &str,
    ) -> impl Future<Output = pl_social::Result<()>> + Send + 'static {
        let service = self.clone();
        let friend = friend.to_owned();
        async move { SocialService::remove_friend(&service, &friend).await }
    }

    fn block(&self, friend: &str) -> impl Future<Output = pl_social::Result<()>> + Send + 'static {
        let service = self.clone();
        let friend = friend.to_owned();
        async move { SocialService::block(&service, &friend).await }
    }

    fn report_player(
        &self,
        friend: &str,
        reason: ReportReason,
        note: Option<String>,
        block: bool,
    ) -> impl Future<Output = pl_social::Result<()>> + Send + 'static {
        let service = self.clone();
        let target = ReportTarget::Player {
            id: friend.to_owned(),
            plan: None,
            reaction: None,
        };
        async move { service.report(target, reason, note, block).await }
    }

    fn save_groups(
        &self,
        groups: Vec<FriendGroup>,
        members: Vec<GroupMember>,
    ) -> impl Future<Output = pl_social::Result<()>> + Send + 'static {
        let service = self.clone();
        async move { SocialService::save_groups(&service, groups, members).await }
    }

    fn invite_to_team(
        &self,
        team: &str,
        friend: &str,
    ) -> impl Future<Output = pl_social::Result<()>> + Send + 'static {
        let service = self.clone();
        let team = team.to_owned();
        let friend = friend.to_owned();
        async move { SocialService::invite_to_team(&service, &team, &friend).await }
    }

    fn request_crewmate(
        &self,
        user: &str,
    ) -> impl Future<Output = pl_social::Result<SendOutcome>> + Send + 'static {
        let service = self.clone();
        let user = user.to_owned();
        async move { service.send_request_to_crewmate(&user).await }
    }

    fn accept(&self, user: &str) -> impl Future<Output = pl_social::Result<()>> + Send + 'static {
        let service = self.clone();
        let user = user.to_owned();
        async move { SocialService::accept(&service, &user).await }
    }

    fn set_team_moderator(
        &self,
        team: &str,
        member: &str,
        moderator: bool,
    ) -> impl Future<Output = pl_social::Result<()>> + Send + 'static {
        let service = self.clone();
        let team = team.to_owned();
        let member = member.to_owned();
        async move { SocialService::set_team_moderator(&service, &team, &member, moderator).await }
    }

    fn transfer_team_founder(
        &self,
        team: &str,
        member: &str,
    ) -> impl Future<Output = pl_social::Result<()>> + Send + 'static {
        let service = self.clone();
        let team = team.to_owned();
        let member = member.to_owned();
        async move { SocialService::transfer_team_founder(&service, &team, &member).await }
    }

    fn remove_team_member(
        &self,
        team: &str,
        member: &str,
    ) -> impl Future<Output = pl_social::Result<()>> + Send + 'static {
        let service = self.clone();
        let team = team.to_owned();
        let member = member.to_owned();
        async move { SocialService::remove_team_member(&service, &team, &member).await }
    }
}

/// What the friends feature and the launcher tell a session.
pub(crate) enum Msg {
    View(Arc<SocialView>),
    Chat(ChatChangeView),
    Notice(SocialNotice),
    /// Someone writes in a conversation (CHAT2).
    Typing(ChatTypingView),
    Config(OverlayConfig),
    /// "Bağlan": the game joins `target` (`host[:port]`); `reply` hears how it went. With `label`
    /// (HOST1, H12) the target is the launcher's own way to a friend's world on `127.0.0.1`.
    Connect {
        target: String,
        label: Option<String>,
        reply: oneshot::Sender<ConnectOutcome>,
    },
    /// Ends the session; `done` fires once the bridge closed.
    Stop {
        reason: ByeReason,
        done: Option<oneshot::Sender<()>>,
    },
    /// HOST1: frames for a mod that hosts (dropped for one that does not).
    Host(Vec<ToGame>),
    /// HOST1: the friends who invited the player to their world, and whether the player may open
    /// theirs now.
    Worlds(WorldsShown),
    /// OVL-LANG: the overlay's language now (the player's pick, else the launcher's).
    Language(Lang),
    /// CHAT-EMOJI: the server's chat emojis now (Studio's list, else the built-in one).
    Emojis(Arc<ChatEmojis>),
}

/// The panel sends at most this many messages ...
const SEND_BURST: usize = 5;
/// ... within this long (a person types slower; anything faster is a stuck key or a bad mod).
const SEND_WINDOW: Duration = Duration::from_secs(5);
/// The panel gives or takes back at most this many reactions within [`SEND_WINDOW`].
const REACT_BURST: usize = 8;
/// "Yazıyor…" from the panel is passed on at most this often (the friends feature sends it at most
/// every 4 s per conversation; this only keeps a stuck mod from keeping it busy).
const TYPING_GAP: Duration = Duration::from_secs(1);
/// Cards asked for at most when a conversation opens in the panel (its newest messages first).
const CARDS_PER_OPEN: usize = 8;
/// Cards asked for in one session at most (each message once; the friends feature keeps them).
const CARDS_PER_SESSION: usize = 200;
/// A sticker reference as the service takes it (`pl1/gg`).
fn sticker_ref_ok(sticker: &str) -> bool {
    let Some((set, name)) = sticker.split_once('/') else {
        return false;
    };
    (1..=8).contains(&set.len())
        && set
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        && (1..=24).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// The friend menu sends at most this many actions ... (OVL-UX)
const PEOPLE_BURST: usize = 6;
/// ... within this long (a person picks slower; anything faster is a stuck key or a bad mod).
const PEOPLE_WINDOW: Duration = Duration::from_secs(10);
/// "Discord'dan mesaj at" for the same friend again within this long opens nothing more (a double
/// click, as the launcher's own button).
const DISCORD_REPEAT: Duration = Duration::from_secs(2);
/// Longest note a report takes (the launcher's `NOTE_MAX`).
const REPORT_NOTE_CHARS: usize = 200;

/// As many friend categories as the service keeps (the launcher's `MAX_GROUPS`; OVL-UX3).
const MAX_GROUPS: usize = 20;
/// A category's name at most (the launcher's `GROUP_NAME_MAX`, what the service stores).
const GROUP_NAME_MAX: usize = 24;
/// The implicit "Arkadaşlar" category's id, never a new one's (the launcher's `REST`).
const REST_GROUP: &str = "rest";

/// A category's name as the launcher keeps it (its `cleanGroupName`): spaces folded and trimmed,
/// at most [`GROUP_NAME_MAX`] characters.
pub(crate) fn clean_group_name(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(GROUP_NAME_MAX)
        .collect()
}

/// A new category's id as the launcher makes it (its `newGroupId`): 8 lower case letters and
/// digits, not used yet and never [`REST_GROUP`].
pub(crate) fn new_group_id(groups: &[FriendGroup]) -> String {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    loop {
        let mut bytes = [0u8; 8];
        rand::fill(&mut bytes[..]);
        let id: String = bytes
            .iter()
            .map(|byte| char::from(ALPHABET[usize::from(*byte) % ALPHABET.len()]))
            .collect();
        if id != REST_GROUP && !groups.iter().any(|group| group.id == id) {
            return id;
        }
    }
}

/// A crew row from the game (OVL-UX3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CrewRow {
    /// "Moderatör yap" (`true`) or "Moderatörlükten al".
    Moderator(bool),
    /// "Liderliği devret".
    Lead,
    /// "Ekipten çıkar".
    Remove,
}

impl CrewRow {
    fn action(self) -> FriendAction {
        match self {
            Self::Moderator(_) => FriendAction::CrewMod,
            Self::Lead => FriendAction::CrewLead,
            Self::Remove => FriendAction::CrewRemove,
        }
    }
}

/// Called with `true` once the mod is greeted and `false` when it goes.
pub(crate) type ConnectedFn = Arc<dyn Fn(bool) + Send + Sync>;

/// Opens a friend's Discord profile (their user id, their web profile): the app's own way, in the
/// Discord app when it is installed. False when nothing can open it (OVL-UX).
pub(crate) type DiscordFn = Arc<dyn Fn(&str, &str) -> bool + Send + Sync>;

/// A mark changed on a friend from the game (OVL-UX2): the launcher's page keeps it.
pub(crate) type PeopleFn = Arc<dyn Fn(PeopleChange) + Send + Sync>;

/// The player picked the overlay's language in its settings (OVL-LANG): the launcher keeps it.
pub(crate) type LangFn = Arc<dyn Fn(Lang) + Send + Sync>;

/// The player switched the chat's emoji picker in the overlay's settings (CHAT-EMOJI): the
/// launcher keeps it.
pub(crate) type EmojiFn = Arc<dyn Fn(bool) + Send + Sync>;

/// Whom a person menu frame may be about (OVL-UX2): the friend list's people for most rows; crew
/// chat's senders (a crewmate who is not a friend) for "Engelle", "Rapor et" and "Arkadaş ekle".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Who {
    Friend,
    FriendOrCrewmate,
    Crewmate,
}

/// A player's report reason from the friend menu (the launcher's player reasons), or None.
fn player_reason(reason: &str) -> Option<ReportReason> {
    Some(match reason {
        "name" => ReportReason::Name,
        "profile" => ReportReason::Profile,
        "harassment" => ReportReason::Harassment,
        "spam" => ReportReason::Spam,
        "scam" => ReportReason::Scam,
        "other" => ReportReason::Other,
        _ => return None,
    })
}

/// A report's note as it goes: spaces folded, trimmed, at most [`REPORT_NOTE_CHARS`]; None when
/// empty (the launcher's `noteOf`).
fn report_note(note: &str) -> Option<String> {
    let folded = note
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .filter(|c| !c.is_control())
        .take(REPORT_NOTE_CHARS)
        .collect::<String>();
    let note = folded.trim();
    (!note.is_empty()).then(|| note.to_owned())
}

/// Why a friend menu action did not go, as the mod says it.
fn friend_refusal(error: &SocialError) -> FriendRefusal {
    match error {
        SocialError::RateLimited { .. } => FriendRefusal::Wait,
        SocialError::Unavailable { .. } | SocialError::Network(_) | SocialError::Unauthorized => {
            FriendRefusal::Offline
        }
        _ => FriendRefusal::Failed,
    }
}

/// The live connection and what it shows.
struct Live {
    conn: ConnId,
    mirror: Mirror,
    panel: bool,
    /// The conversation the panel shows (its new messages count as read).
    open: Option<String>,
    /// The mod switches servers on `connect`.
    connects: bool,
    /// Its language and whether it knows CHAT2.
    listener: Listener,
    /// When the panel last said the player writes.
    typed_at: Option<Instant>,
    /// The mod hosts worlds (HOST1: `host` in its hello).
    hosts: bool,
    /// The mod draws crew tags (TEAM6-O: `tags` in its hello).
    tags: bool,
    /// The mod carries the codeless linking permit (SKIN1-O: `link` in its hello).
    links: bool,
    /// When it last asked for the permit.
    link_wanted_at: Option<Instant>,
    /// The mod has the friend menu (OVL-UX: `people` in its hello): it hears `friend-result`.
    people: bool,
    /// The mod has the launcher's whole person menu (OVL-UX2: `people2` in its hello): it hears
    /// `lists` and the new `friend-result` actions.
    people2: bool,
    /// The mod has OVL-UX3's crew rows and "Yeni kategori…" (`people3` in its hello): it hears
    /// `roles` in `lists` and sends `crew-mod`, `crew-lead`, `crew-remove` and named `move`s.
    people3: bool,
    /// The mod's words follow the launcher (OVL-LANG: `lang` in its hello): its welcome names the
    /// overlay's language and the player picks it there.
    langs: bool,
    /// The mod has the chat's emoji picker (CHAT-EMOJI: `emoji` in its hello): it hears `emojis`.
    emojis: bool,
    /// The mod runs the emoji picker alone (`panel-off` in its hello): no panel, no toasts. It is
    /// not counted as connected and hears only `welcome` and `emojis`.
    quiet: bool,
}

/// The codeless linking permit this session holds (SKIN1-O). It never goes to a log.
struct Permit {
    name: String,
    nonce: Secret,
    /// Unix seconds.
    expires_at: i64,
    /// The friends account it was opened with: another one makes it worthless.
    me_id: String,
}

impl Permit {
    fn offer(&self) -> ToGame {
        ToGame::LinkOffer {
            nonce: self.nonce.clone(),
            name: self.name.clone(),
            expires_at: self.expires_at,
        }
    }
}

/// What asking the service for a permit came back with.
struct PermitAnswer {
    name: String,
    me_id: String,
    intent: Option<McIntent>,
}

pub(crate) struct Session<S> {
    social: S,
    launcher: String,
    config: OverlayConfig,
    /// The newest view, and what it means for the overlay with the current choices.
    view: Arc<SocialView>,
    scene: Scene,
    control: mpsc::UnboundedSender<Control>,
    live: Option<Live>,
    sends: Sends,
    recent_sends: VecDeque<Instant>,
    recent_reacts: VecDeque<Instant>,
    /// Messages (`conv` and key) whose card this session asked for already.
    carded: HashSet<(String, String)>,
    connected: ConnectedFn,
    pending_connect: Option<PendingConnect>,
    next_connect: u64,
    /// HOST1: world sharing hears the mod here; what it shows of worlds.
    host: HostSink,
    worlds: WorldsShown,
    /// SKIN1-O: the game account this game was started with (None: not started from here).
    game: Option<String>,
    /// SKIN1-O: the friends service offers codeless linking now (read with every view).
    codeless: bool,
    permit: Option<Permit>,
    /// A permit is on its way from the service.
    permit_asking: bool,
    /// Permits this session opened (closed again when it ends).
    permits_opened: u32,
    permit_tx: mpsc::UnboundedSender<PermitAnswer>,
    permit_rx: Option<mpsc::UnboundedReceiver<PermitAnswer>>,
    /// OVL-UX: when the friend menu's last actions came, the Discord opener and its last press.
    recent_people: VecDeque<Instant>,
    discord: DiscordFn,
    last_discord: Option<(String, Instant)>,
    /// OVL-UX2: favourites and online notices changed from the game go to the launcher's page.
    people: PeopleFn,
    /// OVL-LANG: the overlay's language (the player's pick, else the launcher's), and where the
    /// player's pick from the game goes.
    lang: Lang,
    lang_picked: LangFn,
    /// CHAT-EMOJI: the server's chat emojis, and where the player's switch from the game goes.
    emojis: Arc<ChatEmojis>,
    emoji_switched: EmojiFn,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
        })
}

/// The scene of a view with what world sharing shows (HOST1): the friends who invited the player
/// to their world, and `host` among the features while the player may open theirs. With `link`
/// (SKIN1-O, K2) the launcher can vouch for the player in game now.
fn scene_of(view: &SocialView, config: &OverlayConfig, worlds: &WorldsShown, link: bool) -> Scene {
    let mut scene = Scene::from_view(view, &config.notify, now_ms());
    if scene.signed_in {
        scene.mark_worlds(&worlds.marks);
        if worlds.may_host {
            scene.features.push(FEATURE_HOST.to_owned());
        }
        if link {
            scene.features.push(FEATURE_LINK.to_owned());
        }
    }
    scene
}

/// Why `chat_send` (or a reply, or a sticker) refused at once.
fn refusal_of(error: &SocialError, listener: Listener) -> SendFailure {
    match error {
        SocialError::ChatMuted { .. } => SendFailure::Muted,
        SocialError::InvalidInput("chat off") => SendFailure::Off,
        SocialError::InvalidInput("chat text") => SendFailure::TooLong,
        SocialError::InvalidInput("chat extras" | "chat sticker") if listener.chat2 => {
            SendFailure::FeatureOff
        }
        SocialError::InvalidInput("chat extras" | "chat sticker") => SendFailure::Off,
        SocialError::InvalidInput(_) => SendFailure::NotAllowed,
        _ => SendFailure::Offline,
    }
}

/// Why a reaction did not go.
fn react_failure(error: &SocialError) -> ReactFailure {
    match error {
        SocialError::Refused {
            code: ErrorCode::ChatReactionsFull,
            detail,
            ..
        } => {
            if detail.as_deref() == Some("kinds") {
                ReactFailure::Kinds
            } else {
                ReactFailure::Full
            }
        }
        SocialError::Refused {
            code: ErrorCode::MessageNotFound,
            ..
        }
        | SocialError::InvalidInput("chat message") => ReactFailure::Old,
        SocialError::Refused {
            code: ErrorCode::ChatFeatureOff,
            ..
        }
        | SocialError::InvalidInput("chat extras" | "chat off") => ReactFailure::Off,
        SocialError::InvalidInput("reaction")
        | SocialError::ChatRejected {
            reason: ChatRejection::Words,
            ..
        } => ReactFailure::Words,
        SocialError::InvalidInput(_) => ReactFailure::NotAllowed,
        SocialError::ChatMuted { .. } => ReactFailure::Muted,
        SocialError::RateLimited { .. } | SocialError::Limited { .. } => ReactFailure::Rate,
        _ => ReactFailure::Offline,
    }
}

/// Drops the times older than `window`; whether `max` are left.
fn full(recent: &mut VecDeque<Instant>, now: Instant, window: Duration, max: usize) -> bool {
    while recent
        .front()
        .is_some_and(|at| now.duration_since(*at) >= window)
    {
        recent.pop_front();
    }
    recent.len() >= max
}

impl<S: SocialActions> Session<S> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        social: S,
        launcher: String,
        config: OverlayConfig,
        control: mpsc::UnboundedSender<Control>,
        connected: ConnectedFn,
        host: HostSink,
        worlds: WorldsShown,
        game: Option<String>,
        discord: DiscordFn,
        people: PeopleFn,
        (lang, lang_picked): (Lang, LangFn),
        (emojis, emoji_switched): (Arc<ChatEmojis>, EmojiFn),
    ) -> Self {
        // The friends feature as it is now; every change after comes as a message.
        let view = Arc::new(social.view());
        let codeless = social.codeless_offered();
        let scene = scene_of(&view, &config, &worlds, codeless && game.is_some());
        let (permit_tx, permit_rx) = mpsc::unbounded_channel();
        Self {
            social,
            launcher,
            config,
            view,
            scene,
            control,
            live: None,
            sends: Sends::default(),
            recent_sends: VecDeque::new(),
            recent_reacts: VecDeque::new(),
            carded: HashSet::new(),
            connected,
            pending_connect: None,
            next_connect: 0,
            host,
            worlds,
            game,
            codeless,
            permit: None,
            permit_asking: false,
            permits_opened: 0,
            permit_tx,
            permit_rx: Some(permit_rx),
            recent_people: VecDeque::new(),
            discord,
            last_discord: None,
            people,
            lang,
            lang_picked,
            emojis,
            emoji_switched,
        }
    }

    pub(crate) async fn run(
        mut self,
        mut msgs: mpsc::UnboundedReceiver<Msg>,
        mut events: mpsc::UnboundedReceiver<BridgeEvent>,
    ) {
        // The session keeps a sender itself, so this never closes; it only wakes for an answer.
        let Some(mut permits) = self.permit_rx.take() else {
            return;
        };
        loop {
            tokio::select! {
                biased;
                msg = msgs.recv() => match msg {
                    Some(Msg::Stop { reason, done }) => {
                        self.finish(reason, done);
                        return;
                    }
                    Some(msg) => self.on_msg(msg),
                    None => {
                        self.finish(ByeReason::LauncherExit, None);
                        return;
                    }
                },
                event = events.recv() => match event {
                    Some(event) => self.on_event(event),
                    None => {
                        self.set_live(None);
                        self.close_permit();
                        return;
                    }
                },
                Some(answer) = permits.recv() => self.on_permit(answer),
            }
        }
    }

    fn finish(&mut self, reason: ByeReason, done: Option<oneshot::Sender<()>>) {
        self.close_permit();
        self.set_live(None);
        if let Err(error) = self.control.send(Control::Close { reason, done })
            && let Control::Close {
                done: Some(done), ..
            } = error.0
        {
            // The bridge is gone already.
            let _ = done.send(());
        }
    }

    fn set_live(&mut self, live: Option<Live>) {
        // A mod that runs the emoji picker alone shows nothing of the friends feature: the page
        // keeps its own notifications for it (CHAT-EMOJI).
        let was = self.live.as_ref().is_some_and(|live| !live.quiet);
        self.live = live;
        if self.live.is_none()
            && let Some(pending) = self.pending_connect.take()
        {
            // The mod went before it answered.
            let _ = pending
                .reply
                .send(ConnectOutcome::unavailable("no-overlay"));
        }
        let now = self.live.as_ref().is_some_and(|live| !live.quiet);
        if was != now {
            (self.connected)(now);
        }
        if was && self.live.is_none() {
            (self.host)(HostSignal::Gone);
        }
    }

    /// Takes a new view; whether someone now wears another crew tag than before (or none).
    fn set_view(&mut self, view: Arc<SocialView>) -> bool {
        self.codeless = self.social.codeless_offered();
        let link = self.codeless && self.game.is_some();
        let scene = scene_of(&view, &self.config, &self.worlds, link);
        let wears_changed = scene.wears_differ(&self.scene);
        self.scene = scene;
        self.view = view;
        wears_changed
    }

    /// Someone took off their crew tag or wears another (TEAM6-O): the messages the panel shows
    /// carry the old one, so the conversation on screen comes again (only to a mod that draws tags;
    /// the next `open-conv` refreshes any other).
    fn reshow_open_conversation(&self) {
        let Some(live) = &self.live else {
            return;
        };
        // Only crew chat names its senders.
        let Some(conv) = live
            .open
            .as_deref()
            .filter(|conv| live.panel && live.tags && self.scene.is_crew_conv(conv))
        else {
            return;
        };
        let page = self.social.chat_page(conv);
        self.send(vec![feed::history_frame(&page, &self.scene, live.listener)]);
    }

    /// The live mod's language and features (an older one's, while none is connected).
    fn listener(&self) -> Listener {
        self.live
            .as_ref()
            .map_or_else(Listener::default, |live| live.listener)
    }

    fn send(&self, mut frames: Vec<ToGame>) {
        if let Some(live) = &self.live
            && live.quiet
        {
            // The emoji picker alone: its words (the greeting names the language) and its list.
            frames.retain(|frame| matches!(frame, ToGame::Welcome { .. } | ToGame::Emojis { .. }));
        }
        if let Some(live) = &self.live
            && !frames.is_empty()
        {
            let _ = self.control.send(Control::Send {
                conn: live.conn,
                frames,
            });
        }
    }

    fn refresh(&mut self) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let frames = live
            .mirror
            .update(&self.scene, &self.launcher, self.config.reduced_motion);
        self.send(frames);
    }

    fn on_msg(&mut self, msg: Msg) {
        match msg {
            Msg::View(view) => {
                let wears_changed = self.set_view(view);
                self.refresh();
                if wears_changed {
                    // After the cards (in the refresh), so every chip it names is known.
                    self.reshow_open_conversation();
                }
                self.drop_stale_permit();
            }
            Msg::Chat(change) => self.on_chat(&change),
            Msg::Notice(notice) => self.on_notice(&notice),
            Msg::Typing(typing) => self.on_typing(&typing),
            Msg::Config(config) => {
                // Conversation mutes and the reduced-motion choice show in the panel.
                let emoji_changed = self.config.emoji != config.emoji;
                self.config = config;
                self.set_view(Arc::clone(&self.view));
                self.refresh();
                if emoji_changed {
                    self.send_emojis();
                }
            }
            Msg::Connect {
                target,
                label,
                reply,
            } => self.on_connect(&target, label, reply),
            Msg::Stop { .. } => {}
            Msg::Host(frames) => {
                if self.live.as_ref().is_some_and(|live| live.hosts) {
                    self.send(frames);
                }
            }
            Msg::Worlds(worlds) => {
                if self.worlds != worlds {
                    self.worlds = worlds;
                    self.set_view(Arc::clone(&self.view));
                    self.refresh();
                }
            }
            Msg::Language(lang) => self.set_language(lang),
            Msg::Emojis(emojis) => {
                self.emojis = emojis;
                self.send_emojis();
            }
        }
    }

    /// CHAT-EMOJI (E3): the emoji picker's switch and the server's emojis, to a mod that has the
    /// picker.
    fn send_emojis(&self) {
        if self.live.as_ref().is_some_and(|live| live.emojis) {
            self.send(vec![self.emojis_frame()]);
        }
    }

    fn emojis_frame(&self) -> ToGame {
        ToGame::Emojis {
            on: self.config.emoji,
            hosts: self.emojis.hosts.clone(),
            emojis: self.emojis.emojis.clone(),
        }
    }

    /// CHAT-EMOJI (E4): the player switched the emoji picker in the overlay's settings. It holds at
    /// once (the answer is `emojis` with the switch) and the launcher keeps it. Only from a mod that
    /// has the picker.
    fn on_emoji_switch(&mut self, on: bool) {
        if !self.live.as_ref().is_some_and(|live| live.emojis) {
            return;
        }
        if self.config.emoji != on {
            tracing::info!(on, "the chat's emoji picker was switched in the game");
            self.config.emoji = on;
            (self.emoji_switched)(on);
        }
        self.send_emojis();
    }

    /// OVL-LANG: the overlay speaks `lang` from now on. A mod whose words follow the launcher is
    /// greeted again in it (its panel builds itself anew and asks for the open conversation again);
    /// an older mod keeps the language its hello named.
    fn set_language(&mut self, lang: Lang) {
        if self.lang == lang {
            return;
        }
        self.lang = lang;
        let Some(live) = self.live.as_mut().filter(|live| live.langs) else {
            return;
        };
        tracing::info!(lang = lang.code(), "the in-game overlay's language changed");
        live.listener.english = lang == Lang::En;
        live.mirror.set_lang(Some(lang));
        self.refresh();
    }

    /// SKIN1-O: a Projects server asked the game whether its launcher vouches for the player
    /// (`projects:link`), and the mod needs the permit for its account `name`. Answered with the
    /// permit held for that account, a new one from the service, or `link-clear`: not signed in to
    /// friends, codeless linking off, or another account than the one this game was started with.
    fn on_link_want(&mut self, name: &str) {
        let Some(live) = self.live.as_mut().filter(|live| live.links) else {
            return;
        };
        let now = Instant::now();
        if live
            .link_wanted_at
            .is_some_and(|at| now.duration_since(at) < LINK_WANT_GAP)
        {
            // A stuck mod: the answer to the last one is on its way.
            return;
        }
        live.link_wanted_at = Some(now);
        // The account the game was started with, as the launcher spells it.
        let game = self
            .game
            .clone()
            .filter(|game| !name.is_empty() && game.eq_ignore_ascii_case(name));
        let Some(name) = game.filter(|_| self.scene.signed_in && self.codeless) else {
            self.send(vec![ToGame::LinkClear]);
            return;
        };
        let held = self.permit.as_ref().filter(|permit| {
            permit.me_id == self.scene.me_id
                && permit.name == name
                && permit.expires_at - now_ms() / 1000 > PERMIT_MARGIN_SECS
        });
        if let Some(permit) = held {
            self.send(vec![permit.offer()]);
            return;
        }
        if self.permit_asking {
            return;
        }
        if self.permits_opened >= MAX_PERMITS {
            tracing::info!("no more codeless linking permits for this game");
            self.send(vec![ToGame::LinkClear]);
            return;
        }
        self.permit_asking = true;
        self.permits_opened += 1;
        let asking = self.social.codeless_permit(&name);
        let answers = self.permit_tx.clone();
        let me_id = self.scene.me_id.clone();
        tokio::spawn(async move {
            let intent = asking.await;
            let _ = answers.send(PermitAnswer {
                name,
                me_id,
                intent,
            });
        });
    }

    /// The service answered: the mod gets the permit, or hears there is none. A permit that came
    /// back for a friends account signed out meanwhile is dropped.
    fn on_permit(&mut self, answer: PermitAnswer) {
        self.permit_asking = false;
        let current = self.scene.signed_in && self.codeless && answer.me_id == self.scene.me_id;
        let links = self.live.as_ref().is_some_and(|live| live.links);
        match answer.intent {
            Some(intent) if current => {
                let permit = Permit {
                    name: answer.name,
                    nonce: Secret::new(intent.nonce),
                    expires_at: intent.expires_at,
                    me_id: answer.me_id,
                };
                if links {
                    self.send(vec![permit.offer()]);
                }
                self.permit = Some(permit);
            }
            _ => {
                if links {
                    self.send(vec![ToGame::LinkClear]);
                }
            }
        }
    }

    /// A permit of another friends account, or held while codeless linking went off, is worth
    /// nothing: the mod forgets it too.
    fn drop_stale_permit(&mut self) {
        let stale = self.permit.as_ref().is_some_and(|permit| {
            !self.scene.signed_in || !self.codeless || permit.me_id != self.scene.me_id
        });
        if !stale {
            return;
        }
        self.permit = None;
        if self.live.as_ref().is_some_and(|live| live.links) {
            self.send(vec![ToGame::LinkClear]);
        }
    }

    /// The game this session served is gone: a permit it opened stops working (spent or not).
    fn close_permit(&mut self) {
        self.permit = None;
        if self.permits_opened == 0 {
            return;
        }
        self.permits_opened = 0;
        tokio::spawn(self.social.codeless_close());
    }

    /// "Bağlan", which the player chose and confirmed in the launcher: the game joins the server
    /// itself instead of restarting. Only Projects or an address shared now, one at a time, and
    /// only to a mod that can.
    fn on_connect(
        &mut self,
        target: &str,
        label: Option<String>,
        reply: oneshot::Sender<ConnectOutcome>,
    ) {
        // One at a time: an earlier one still counts until it is answered, given up by its caller
        // or older than the wait (checked here: nothing ticks meanwhile).
        if let Some(pending) = &self.pending_connect
            && !pending.reply.is_closed()
            && pending.asked.elapsed() < CONNECT_WAIT
        {
            let _ = reply.send(ConnectOutcome::unavailable("busy"));
            return;
        }
        self.pending_connect = None;
        let refused = match &self.live {
            None => Some("no-overlay"),
            Some(live) if !live.connects => Some("unsupported"),
            // A friend's world needs a mod that shows its label instead of the address (HOST1).
            Some(live) if label.is_some() && !live.hosts => Some("unsupported"),
            Some(_) => None,
        };
        if let Some(reason) = refused {
            let _ = reply.send(ConnectOutcome::unavailable(reason));
            return;
        }
        let target = match &label {
            // The launcher's own way to a friend's world: `127.0.0.1` and nothing else (H12).
            Some(_) => feed::server_address(target).filter(|(host, _)| host == "127.0.0.1"),
            None => self.scene.connect_target(target),
        };
        let Some((host, port)) = target else {
            tracing::info!("the launcher did not send the game to a server nobody shares");
            let _ = reply.send(ConnectOutcome::unavailable("not-allowed"));
            return;
        };
        self.next_connect += 1;
        let id = self.next_connect.to_string();
        if label.is_some() {
            tracing::info!("the game joins a friend's world the player chose");
        } else {
            tracing::info!(%host, port, "the game joins a server the player chose in the launcher");
        }
        self.send(vec![ToGame::Connect {
            id: id.clone(),
            host,
            port,
            label,
        }]);
        self.pending_connect = Some(PendingConnect {
            id,
            asked: Instant::now(),
            reply,
        });
    }

    fn on_chat(&mut self, change: &ChatChangeView) {
        let listener = self.listener();
        let out = feed::chat_frames(change, &self.scene, &mut self.sends, listener);
        for key in &out.discard {
            self.social.chat_discard(key);
        }
        let open_here = self
            .live
            .as_ref()
            .is_some_and(|live| live.panel && live.open.as_deref() == Some(change.conv.as_str()));
        if out.arrived && open_here {
            // The panel shows it: it is read, as it would be on the launcher's chat page.
            self.social.chat_read(&change.conv);
        }
        self.send(out.frames);
        if open_here {
            // A message that came onto the panel's screen with a link gets its card, once.
            self.ask_cards(&change.conv, change.upsert.iter().rev(), usize::MAX);
        }
    }

    /// Asks for the cards of messages the panel shows (CHAT2 §9.3: each device once, only for what
    /// comes on screen): one task asks them in turn; each message comes back with its card.
    fn ask_cards<'a>(
        &mut self,
        conv: &str,
        messages: impl Iterator<Item = &'a ChatMessageView>,
        max: usize,
    ) {
        if !self.listener().chat2 {
            return;
        }
        let now = now_ms();
        let mut asks = Vec::new();
        for message in messages {
            if asks.len() >= max || self.carded.len() >= CARDS_PER_SESSION {
                break;
            }
            if feed::wants_card(message, &self.scene, now)
                && self.carded.insert((conv.to_owned(), message.key.clone()))
            {
                asks.push(self.social.chat_unfurl(conv, &message.key));
            }
        }
        if asks.is_empty() {
            return;
        }
        tokio::spawn(async move {
            for ask in asks {
                if let Err(error) = ask.await {
                    tracing::debug!(%error, "no card for a link in the in-game overlay");
                }
            }
        });
    }

    fn on_notice(&mut self, notice: &SocialNotice) {
        let Some(toast) = feed::toast_of(
            notice,
            &self.scene,
            &self.config.notify,
            now_ms(),
            self.listener(),
        ) else {
            return;
        };
        // The conversation the panel shows says it in place.
        let shown = self.live.as_ref().is_some_and(|live| {
            live.panel && toast.conv.is_some() && live.open.as_deref() == toast.conv.as_deref()
        });
        if !shown {
            self.send(vec![ToGame::Toast(toast)]);
        }
        // An AFK notice the game showed was seen: the phone stays quiet (AFKPLUS).
        if let SocialNotice::AfkNotice { notice } = notice
            && self.live.is_some()
        {
            self.social.afk_seen(&notice.id);
        }
    }

    /// "Yazıyor…" reaches the game only while its panel is open (nothing to show otherwise) and
    /// only a mod that knows it.
    fn on_typing(&mut self, typing: &ChatTypingView) {
        let Some(live) = &self.live else {
            return;
        };
        if !live.listener.chat2
            || !live.panel
            || !self.scene.offers(|extras| extras.typing)
            || typing.by == self.scene.me_id
            || self.scene.is_blocked(&typing.by)
        {
            return;
        }
        let name = self
            .scene
            .sender(&typing.conv, &typing.by, self.listener().english);
        self.send(vec![ToGame::Typing {
            conv: typing.conv.clone(),
            by: typing.by.clone(),
            name,
        }]);
    }

    fn on_event(&mut self, event: BridgeEvent) {
        match event {
            BridgeEvent::Connected {
                conn,
                mod_version,
                minecraft,
                language,
                features,
            } => {
                tracing::info!(
                    conn,
                    mod_version,
                    minecraft,
                    language,
                    ?features,
                    "the in-game overlay connected"
                );
                let tags = features.iter().any(|f| f == FEATURE_TAGS);
                let people2 = features.iter().any(|f| f == FEATURE_PEOPLE2);
                // The crew rows build on the whole person menu.
                let people3 = people2 && features.iter().any(|f| f == FEATURE_PEOPLE3);
                let langs = features.iter().any(|f| f == FEATURE_LANG);
                let mut mirror = Mirror::for_mod(tags, people2).with_people3(people3);
                mirror.set_lang(langs.then_some(self.lang));
                let frames = mirror.greet(&self.scene, &self.launcher, self.config.reduced_motion);
                // A reconnect forgets a "Bağlan" the earlier connection never answered.
                if let Some(pending) = self.pending_connect.take() {
                    let _ = pending
                        .reply
                        .send(ConnectOutcome::unavailable("no-overlay"));
                }
                let hosts = features.iter().any(|f| f == FEATURE_HOST);
                let emojis = features.iter().any(|f| f == FEATURE_EMOJI);
                self.set_live(Some(Live {
                    conn,
                    mirror,
                    panel: false,
                    open: None,
                    connects: features.iter().any(|f| f == FEATURE_CONNECT),
                    listener: Listener::from_hello(&language, &features, self.lang),
                    typed_at: None,
                    hosts,
                    tags,
                    links: features.iter().any(|f| f == FEATURE_LINK),
                    link_wanted_at: None,
                    people: features.iter().any(|f| f == FEATURE_PEOPLE),
                    people2,
                    people3,
                    langs,
                    emojis,
                    quiet: features.iter().any(|f| f == FEATURE_PANEL_OFF),
                }));
                self.send(frames);
                self.send_emojis();
                (self.host)(HostSignal::Connected { hosts });
            }
            BridgeEvent::Frame { conn, frame } => {
                if self.live.as_ref().is_some_and(|live| live.conn == conn) {
                    self.on_frame(frame);
                }
            }
            BridgeEvent::Disconnected { conn } => {
                if self.live.as_ref().is_some_and(|live| live.conn == conn) {
                    tracing::info!(conn, "the in-game overlay went away");
                    self.set_live(None);
                }
            }
        }
    }

    fn on_frame(&mut self, frame: FromGame) {
        match frame {
            FromGame::Panel { open } => {
                if let Some(live) = self.live.as_mut() {
                    live.panel = open;
                    if !open {
                        live.open = None;
                    }
                }
            }
            FromGame::OpenConv { conv } => {
                let page = self.social.chat_page(&conv);
                self.send(vec![feed::history_frame(
                    &page,
                    &self.scene,
                    self.listener(),
                )]);
                self.social.chat_read(&conv);
                if let Some(live) = self.live.as_mut() {
                    live.panel = true;
                    live.open = Some(conv.clone());
                }
                // The newest messages with links get their cards.
                self.ask_cards(&conv, page.messages.iter().rev(), CARDS_PER_OPEN);
            }
            FromGame::Send {
                cid,
                conv,
                text,
                reply,
                sticker,
            } => self.on_send(cid, &conv, &text, reply, sticker.as_deref()),
            FromGame::React { conv, sid, e, on } => self.on_react(conv, sid, e, on),
            FromGame::Typing { conv } => {
                let now = Instant::now();
                if let Some(live) = self.live.as_mut() {
                    if live
                        .typed_at
                        .is_some_and(|at| now.duration_since(at) < TYPING_GAP)
                    {
                        return;
                    }
                    live.typed_at = Some(now);
                }
                self.social.chat_typing(&conv);
            }
            FromGame::Answer { call, answer } => {
                let answer = match answer {
                    CallAnswer::Coming => SocialAnswer::Yes,
                    CallAnswer::Soon => SocialAnswer::Soon,
                    CallAnswer::Cant => SocialAnswer::No,
                };
                // The new counts come back in the next view.
                let answering = self.social.answer_call(&call, answer);
                tokio::spawn(async move {
                    if let Err(error) = answering.await {
                        tracing::info!(%error, "the overlay's call answer did not go");
                    }
                });
            }
            // HOST1: the player's world, to the launcher's world sharing (a mod that hosts only).
            frame @ (FromGame::HostWorld { .. }
            | FromGame::HostRequest { .. }
            | FromGame::HostPublished { .. }
            | FromGame::HostGuest { .. }
            | FromGame::HostInvite { .. }
            | FromGame::HostRemove { .. }
            | FromGame::HostStop { .. }
            | FromGame::HostMeet { .. }) => {
                if self.live.as_ref().is_some_and(|live| live.hosts) {
                    (self.host)(HostSignal::Frame(frame));
                }
            }
            // A friend who invited the player to their world: the launcher joins it (H15).
            FromGame::Join { friend }
                if self
                    .scene
                    .friend(&friend)
                    .is_some_and(|f| f.world.is_some()) =>
            {
                tracing::info!("the player joins a friend's world from the in-game overlay");
                (self.host)(HostSignal::Join { friend });
            }
            FromGame::Join { friend } => {
                if self.scene.friend(&friend).is_some_and(|f| f.joinable) {
                    // The game connects itself after its own question; an invite from that
                    // friend is answered by it.
                    for invite in self.scene.invites_from(&friend).to_vec() {
                        self.social.dismiss_invite(&invite);
                    }
                    tracing::info!("the player joins a friend from the in-game overlay");
                } else {
                    tracing::debug!("the overlay asked to join a friend who cannot be joined");
                }
            }
            FromGame::ConnectResult { id, ok, reason } => {
                let Some(pending) = self.pending_connect.take_if(|pending| pending.id == id) else {
                    return;
                };
                let outcome = if ok {
                    ConnectOutcome::Done
                } else if reason.is_empty() {
                    ConnectOutcome::unavailable("refused")
                } else {
                    ConnectOutcome::Unavailable { reason }
                };
                tracing::info!(?outcome, "the game answered the launcher's server switch");
                let _ = pending.reply.send(outcome);
            }
            FromGame::LinkWant { name } => self.on_link_want(&name),
            // OVL-UX: the friend menu (the player's own presses, after the mod's question where one
            // is asked).
            FromGame::Invite { friend } => self.on_invite(friend),
            FromGame::Discord { friend } => self.on_discord(friend),
            FromGame::Unfriend { friend } => {
                if self.person_allowed(&friend, FriendAction::Unfriend, Who::Friend, true) {
                    let doing = self.social.remove_friend(&friend);
                    self.settle(friend, FriendAction::Unfriend, doing);
                }
            }
            FromGame::Block { friend } => {
                // A friend, or a crewmate from crew chat (OVL-UX2): never anyone else.
                if self.person_allowed(&friend, FriendAction::Block, Who::FriendOrCrewmate, true) {
                    let doing = self.social.block(&friend);
                    self.settle(friend, FriendAction::Block, doing);
                }
            }
            FromGame::Report {
                friend,
                reason,
                note,
                block,
            } => self.on_report(friend, &reason, &note, block),
            // OVL-UX2: the rest of the launcher's person menu.
            FromGame::Favorite { friend, on } => self.on_mark(friend, on, FriendAction::Favorite),
            FromGame::Alert { friend, on } => self.on_mark(friend, on, FriendAction::Alert),
            FromGame::Move {
                friend,
                group,
                name,
            } => {
                if name.is_empty() {
                    self.on_move(friend, &group);
                } else {
                    self.on_move_new(friend, &group, &name);
                }
            }
            FromGame::CrewInvite { friend, crew } => self.on_crew_invite(friend, &crew),
            FromGame::AddFriend { friend } => self.on_add_friend(friend),
            FromGame::CrewMod { crew, friend, on } => {
                self.on_crew_row(&crew, friend, CrewRow::Moderator(on));
            }
            FromGame::CrewLead { crew, friend } => self.on_crew_row(&crew, friend, CrewRow::Lead),
            FromGame::CrewRemove { crew, friend } => {
                self.on_crew_row(&crew, friend, CrewRow::Remove);
            }
            FromGame::Lang { lang } => self.on_lang_pick(&lang),
            FromGame::Emoji { on } => self.on_emoji_switch(on),
            FromGame::Hello { .. } | FromGame::Unknown => {}
        }
    }

    /// OVL-LANG (G4): the player picked the overlay's language in its settings. It speaks it at
    /// once (the answer is the new greeting) and the launcher keeps the pick for every game after.
    /// Only from a mod whose words follow the launcher; a value that is no language is ignored.
    fn on_lang_pick(&mut self, lang: &str) {
        if !self.live.as_ref().is_some_and(|live| live.langs) {
            return;
        }
        let lang = match lang {
            "tr" => Lang::Tr,
            "en" => Lang::En,
            _ => return,
        };
        tracing::info!(
            lang = lang.code(),
            "the player picked the overlay's language in game"
        );
        self.set_language(lang);
        (self.lang_picked)(lang);
    }

    // --- the friend menu (OVL-UX) ------------------------------------------------------------------

    /// How a friend menu action went, to a mod that hears it (`people` in its hello).
    fn friend_result(
        &self,
        friend: &str,
        action: FriendAction,
        queued: bool,
        reason: Option<FriendRefusal>,
    ) {
        if self.live.as_ref().is_some_and(|live| live.people) {
            self.send(vec![ToGame::FriendResult {
                friend: friend.to_owned(),
                action,
                ok: reason.is_none(),
                queued,
                reason,
            }]);
        }
    }

    /// Whether a friend menu frame may go on: the game has not sent too many, they are someone
    /// the row may be about (`who`: on the friend list, or a crewmate), and (`service`) the
    /// friends service is connected. Otherwise the mod hears why at once.
    fn person_allowed(
        &mut self,
        friend: &str,
        action: FriendAction,
        who: Who,
        service: bool,
    ) -> bool {
        let now = Instant::now();
        let is_friend = self.scene.friend(friend).is_some();
        let known = match who {
            Who::Friend => is_friend,
            Who::FriendOrCrewmate => is_friend || self.scene.is_crewmate(friend),
            Who::Crewmate => !is_friend && self.scene.is_crewmate(friend),
        };
        let refused = if full(&mut self.recent_people, now, PEOPLE_WINDOW, PEOPLE_BURST) {
            Some(FriendRefusal::Rate)
        } else if !known {
            Some(FriendRefusal::Failed)
        } else if service && !self.scene.online {
            Some(FriendRefusal::Offline)
        } else {
            None
        };
        if let Some(reason) = refused {
            tracing::debug!(
                ?action,
                ?reason,
                "a friend menu action from the game did not go"
            );
            self.friend_result(friend, action, false, Some(reason));
            return false;
        }
        self.recent_people.push_back(now);
        true
    }

    /// Runs an action against the friends service in a task of its own; the mod hears how it went.
    fn settle(
        &self,
        friend: String,
        action: FriendAction,
        doing: impl Future<Output = pl_social::Result<()>> + Send + 'static,
    ) {
        let reply = self
            .live
            .as_ref()
            .filter(|live| live.people)
            .map(|live| live.conn);
        let control = self.control.clone();
        tracing::info!(?action, "a friend menu action from the in-game overlay");
        tokio::spawn(async move {
            let reason = match doing.await {
                Ok(()) => None,
                Err(error) => {
                    tracing::info!(%error, ?action, "the overlay's friend menu action did not go");
                    Some(friend_refusal(&error))
                }
            };
            if let Some(conn) = reply {
                let _ = control.send(Control::Send {
                    conn,
                    frames: vec![ToGame::FriendResult {
                        friend,
                        action,
                        ok: reason.is_none(),
                        queued: false,
                        reason,
                    }],
                });
            }
        });
    }

    /// "Birlikte oyna" from the game: only while it can go (the player is on Projects or shows
    /// their server); the service keeps its own five minutes per friend.
    fn on_invite(&mut self, friend: String) {
        if !self.person_allowed(&friend, FriendAction::Invite, Who::Friend, true) {
            return;
        }
        if !self.scene.may_invite() {
            self.friend_result(
                &friend,
                FriendAction::Invite,
                false,
                Some(FriendRefusal::Failed),
            );
            return;
        }
        let reply = self
            .live
            .as_ref()
            .filter(|live| live.people)
            .map(|live| live.conn);
        let control = self.control.clone();
        let inviting = self.social.invite(&friend);
        tracing::info!("the player invites a friend from the in-game overlay");
        tokio::spawn(async move {
            let (queued, reason) = match inviting.await {
                Ok(InviteResult::Delivered) => (false, None),
                Ok(InviteResult::Queued) => (true, None),
                Err(error) => {
                    tracing::info!(%error, "the overlay's invite did not go");
                    (false, Some(friend_refusal(&error)))
                }
            };
            if let Some(conn) = reply {
                let _ = control.send(Control::Send {
                    conn,
                    frames: vec![ToGame::FriendResult {
                        friend,
                        action: FriendAction::Invite,
                        ok: reason.is_none(),
                        queued,
                        reason,
                    }],
                });
            }
        });
    }

    /// "Discord'dan mesaj at" from the game: the launcher opens the profile its own way (the
    /// Discord app when installed, else the web profile). A second press for the same friend right
    /// after opens nothing more.
    fn on_discord(&mut self, friend: String) {
        let now = Instant::now();
        if self
            .last_discord
            .as_ref()
            .is_some_and(|(last, at)| *last == friend && now.duration_since(*at) < DISCORD_REPEAT)
        {
            return;
        }
        // A friend, or a crewmate from crew chat (OVL-UX2, Q15).
        if !self.person_allowed(&friend, FriendAction::Discord, Who::FriendOrCrewmate, false) {
            return;
        }
        let Some(url) = self.scene.discord_url(&friend).map(str::to_owned) else {
            self.friend_result(
                &friend,
                FriendAction::Discord,
                false,
                Some(FriendRefusal::Failed),
            );
            return;
        };
        self.last_discord = Some((friend.clone(), now));
        tracing::info!("the player opens a friend's Discord profile from the in-game overlay");
        if !(self.discord)(&friend, &url) {
            self.friend_result(
                &friend,
                FriendAction::Discord,
                false,
                Some(FriendRefusal::Failed),
            );
        }
    }

    /// "Rapor et" from the game: one of a player's reasons, the note as it goes, maybe a block.
    fn on_report(&mut self, friend: String, reason: &str, note: &str, block: bool) {
        let Some(reason) = player_reason(reason) else {
            self.friend_result(
                &friend,
                FriendAction::Report,
                false,
                Some(FriendRefusal::Failed),
            );
            return;
        };
        if self.person_allowed(&friend, FriendAction::Report, Who::FriendOrCrewmate, true) {
            let doing = self
                .social
                .report_player(&friend, reason, report_note(note), block);
            self.settle(friend, FriendAction::Report, doing);
        }
    }

    // --- the rest of the launcher's person menu (OVL-UX2) ----------------------------------------

    /// "Favorilere ekle" / "Çevrimiçi olunca bildir" from the game: the marks the launcher's page
    /// keeps. This session shows the change at once (the friend's frame), the page hears it and
    /// keeps it as its own buttons would.
    fn on_mark(&mut self, friend: String, on: bool, action: FriendAction) {
        if !self.live.as_ref().is_some_and(|live| live.people2)
            || !self.person_allowed(&friend, action, Who::Friend, false)
        {
            return;
        }
        let change = if action == FriendAction::Favorite {
            self.config.notify.set_favorite(&friend, on);
            PeopleChange::Favorite {
                friend: friend.clone(),
                on,
            }
        } else {
            self.config.notify.online_alerts.insert(friend.clone(), on);
            PeopleChange::Alert {
                friend: friend.clone(),
                on,
            }
        };
        tracing::info!(
            ?action,
            on,
            "a friend's mark changed from the in-game overlay"
        );
        (self.people)(change);
        self.set_view(Arc::clone(&self.view));
        self.refresh();
    }

    /// "Kategoriye taşı" from the game: the categories saved whole, as the launcher's list saves
    /// them (its `moveFriend`): an unknown category or "" puts them in "Arkadaşlar".
    fn on_move(&mut self, friend: String, group: &str) {
        if !self.live.as_ref().is_some_and(|live| live.people2)
            || !self.person_allowed(&friend, FriendAction::Move, Who::Friend, true)
        {
            return;
        }
        let Some(doc) = self.view.groups.clone() else {
            self.friend_result(
                &friend,
                FriendAction::Move,
                false,
                Some(FriendRefusal::Failed),
            );
            return;
        };
        let known = doc.groups.iter().any(|g| g.id == group);
        if !group.is_empty() && !known {
            self.friend_result(
                &friend,
                FriendAction::Move,
                false,
                Some(FriendRefusal::Failed),
            );
            return;
        }
        let now = doc
            .members
            .iter()
            .find(|member| member.friend == friend)
            .map(|member| member.group.as_str());
        if now.unwrap_or_default() == group {
            // Already there: nothing to save, and nothing went wrong (the mod says no failure).
            self.friend_result(&friend, FriendAction::Move, true, None);
            return;
        }
        let mut members: Vec<GroupMember> = doc
            .members
            .into_iter()
            .filter(|member| member.friend != friend)
            .collect();
        if known {
            members.push(GroupMember {
                friend: friend.clone(),
                group: group.to_owned(),
            });
        }
        let doing = self.social.save_groups(doc.groups, members);
        self.settle(friend, FriendAction::Move, doing);
    }

    /// "Ekibe davet et" from the game: only to a crew the player may invite to, with room, that
    /// the friend is not in or invited to already.
    fn on_crew_invite(&mut self, friend: String, crew: &str) {
        if !self.live.as_ref().is_some_and(|live| live.people2)
            || !self.person_allowed(&friend, FriendAction::CrewInvite, Who::Friend, true)
        {
            return;
        }
        let open = self
            .scene
            .lists
            .crews
            .iter()
            .any(|item| item.id == crew && !item.full);
        let placed = self
            .scene
            .friend(&friend)
            .is_some_and(|f| f.crews.contains_key(crew));
        if !open || placed {
            self.friend_result(
                &friend,
                FriendAction::CrewInvite,
                false,
                Some(FriendRefusal::Failed),
            );
            return;
        }
        let doing = self.social.invite_to_team(crew, &friend);
        self.settle(friend, FriendAction::CrewInvite, doing);
    }

    /// "Yeni kategori…" from the game (OVL-UX3, U5): a new category of that name at the end of the
    /// list, with the friend in it, saved whole as "Kategoriye taşı" saves; the launcher's own
    /// `addGroup` ([`clean_group_name`], [`new_group_id`], at most [`MAX_GROUPS`]). A named move
    /// to a category that exists is no new one.
    fn on_move_new(&mut self, friend: String, group: &str, name: &str) {
        if !self.live.as_ref().is_some_and(|live| live.people3)
            || !self.person_allowed(&friend, FriendAction::Move, Who::Friend, true)
        {
            return;
        }
        let clean = clean_group_name(name);
        let doc =
            self.view.groups.clone().filter(|doc| {
                group.is_empty() && !clean.is_empty() && doc.groups.len() < MAX_GROUPS
            });
        let Some(doc) = doc else {
            self.friend_result(
                &friend,
                FriendAction::Move,
                false,
                Some(FriendRefusal::Failed),
            );
            return;
        };
        let id = new_group_id(&doc.groups);
        let mut groups = doc.groups;
        groups.push(FriendGroup {
            id: id.clone(),
            name: clean,
        });
        let mut members: Vec<GroupMember> = doc
            .members
            .into_iter()
            .filter(|member| member.friend != friend)
            .collect();
        members.push(GroupMember {
            friend: friend.clone(),
            group: id,
        });
        tracing::info!("a new friend category from the in-game overlay");
        let doing = self.social.save_groups(groups, members);
        self.settle(friend, FriendAction::Move, doing);
    }

    /// A crew row from the game (OVL-UX3, U2–U4), checked again against the view as it is now
    /// (the mod's rows came from `lists.roles`, which may be a little older), by the launcher's own
    /// rules: "Moderatör yap" / "Moderatörlükten al" and "Liderliği devret" only in a crew the
    /// player leads (moderators only while the service offers roles, at most
    /// [`feed::MAX_MODERATORS`]), "Ekipten çıkar" as [`feed::may_remove`] says. Never the leader
    /// nor the player. A moderator row that asks for what already is answers that it went.
    fn on_crew_row(&mut self, crew: &str, friend: String, row: CrewRow) {
        let action = row.action();
        if !self.live.as_ref().is_some_and(|live| live.people3)
            || !self.person_allowed(&friend, action, Who::FriendOrCrewmate, true)
        {
            return;
        }
        let view = Arc::clone(&self.view);
        let roles = view.team_features.iter().any(|f| f == TEAM_FEATURE_ROLES);
        let target = view
            .teams
            .then(|| view.crews.iter().find(|team| team.id == crew))
            .flatten()
            .and_then(|team| {
                team.members
                    .iter()
                    .find(|member| member.id == friend && !member.me)
                    .map(|member| (team, member))
            });
        let verdict = target.map(|(team, member)| {
            let lead = team.role == TeamRole::Founder && member.role != TeamRole::Founder;
            match row {
                CrewRow::Moderator(on) if member.moderator == on => (lead && roles, true),
                CrewRow::Moderator(on) => {
                    let moderators = team.members.iter().filter(|m| m.moderator).count();
                    (
                        lead && roles && (!on || moderators < feed::MAX_MODERATORS),
                        false,
                    )
                }
                CrewRow::Lead => (lead, false),
                CrewRow::Remove => (feed::may_remove(team, member, roles), false),
            }
        });
        match verdict {
            Some((true, true)) => self.friend_result(&friend, action, false, None),
            Some((true, false)) => {
                let doing: std::pin::Pin<Box<dyn Future<Output = pl_social::Result<()>> + Send>> =
                    match row {
                        CrewRow::Moderator(on) => {
                            Box::pin(self.social.set_team_moderator(crew, &friend, on))
                        }
                        CrewRow::Lead => Box::pin(self.social.transfer_team_founder(crew, &friend)),
                        CrewRow::Remove => Box::pin(self.social.remove_team_member(crew, &friend)),
                    };
                self.settle(friend, action, doing);
            }
            _ => self.friend_result(&friend, action, false, Some(FriendRefusal::Failed)),
        }
    }

    /// "Arkadaş ekle" (or "İsteği kabul et") on a crewmate from crew chat: only while the service
    /// lets crewmates ask each other. `queued`: the request waits for them; without it they are
    /// friends now.
    fn on_add_friend(&mut self, friend: String) {
        if !self.live.as_ref().is_some_and(|live| live.people2)
            || !self.person_allowed(&friend, FriendAction::AddFriend, Who::Crewmate, true)
        {
            return;
        }
        let lists = &self.scene.lists;
        if !lists.crewmates || lists.asking.contains(&friend) {
            self.friend_result(
                &friend,
                FriendAction::AddFriend,
                false,
                Some(FriendRefusal::Failed),
            );
            return;
        }
        let reply = self
            .live
            .as_ref()
            .filter(|live| live.people2)
            .map(|live| live.conn);
        let control = self.control.clone();
        let answering = lists.asked.contains(&friend);
        let accepting = answering.then(|| self.social.accept(&friend));
        let asking = (!answering).then(|| self.social.request_crewmate(&friend));
        tracing::info!(
            answering,
            "a crewmate's friend request from the in-game overlay"
        );
        tokio::spawn(async move {
            let outcome = match (accepting, asking) {
                (Some(accepting), _) => accepting.await.map(|()| SendOutcome::Friends),
                (None, Some(asking)) => asking.await,
                (None, None) => return,
            };
            let (queued, reason) = match outcome {
                Ok(SendOutcome::Friends) => (false, None),
                Ok(SendOutcome::Requested) => (true, None),
                Err(error) => {
                    tracing::info!(%error, "the overlay's friend request did not go");
                    (false, Some(friend_refusal(&error)))
                }
            };
            if let Some(conn) = reply {
                let _ = control.send(Control::Send {
                    conn,
                    frames: vec![ToGame::FriendResult {
                        friend,
                        action: FriendAction::AddFriend,
                        ok: reason.is_none(),
                        queued,
                        reason,
                    }],
                });
            }
        });
    }

    fn on_send(
        &mut self,
        cid: String,
        conv: &str,
        text: &str,
        reply: Option<i64>,
        sticker: Option<&str>,
    ) {
        let listener = self.listener();
        let now = Instant::now();
        let refused = if full(&mut self.recent_sends, now, SEND_WINDOW, SEND_BURST) {
            Some(SendFailure::Rate)
        } else if sticker.is_none() && text.chars().count() > MAX_MESSAGE_CHARS {
            Some(SendFailure::TooLong)
        } else if sticker.is_some_and(|sticker| !sticker_ref_ok(sticker)) {
            Some(if listener.chat2 {
                SendFailure::FeatureOff
            } else {
                SendFailure::Off
            })
        } else if !self.scene.online {
            // Told at once instead of waiting for the friends service to come back.
            Some(SendFailure::Offline)
        } else {
            None
        };
        if let Some(reason) = refused {
            self.send(vec![ToGame::SendFailed {
                cid,
                reason,
                rule: None,
            }]);
            return;
        }
        self.recent_sends.push_back(now);
        let sent = match (sticker, reply) {
            (Some(sticker), reply) => self.social.chat_sticker(conv, sticker, reply),
            (None, Some(reply)) => match self.social.chat_reply(conv, text, reply) {
                // The message it answers is not held here any more, or Studio switched replies
                // off meanwhile: it goes as a plain message (the player does not lose it).
                Err(SocialError::InvalidInput("chat message" | "chat extras")) => {
                    self.social.chat_send(conv, text)
                }
                other => other,
            },
            (None, None) => self.social.chat_send(conv, text),
        };
        match sent {
            // Its progress (on its way, sent, refused) arrives as chat changes.
            Ok(key) => self.sends.insert(key, cid),
            Err(error) => self.send(vec![ToGame::SendFailed {
                cid,
                reason: refusal_of(&error, listener),
                rule: None,
            }]),
        }
    }

    /// The panel gives or takes back a reaction: shown at once by the friends feature (the message
    /// comes back with it), settled by the service; a refusal is told to the panel.
    fn on_react(&mut self, conv: String, sid: i64, e: String, on: bool) {
        let Some(conn) = self.live.as_ref().map(|live| live.conn) else {
            return;
        };
        let refused = if full(
            &mut self.recent_reacts,
            Instant::now(),
            SEND_WINDOW,
            REACT_BURST,
        ) {
            Some(ReactFailure::Rate)
        } else if !self.scene.online {
            Some(ReactFailure::Offline)
        } else if !self.scene.offers(|extras| extras.reactions) {
            Some(ReactFailure::Off)
        } else {
            None
        };
        if let Some(reason) = refused {
            self.send(vec![ToGame::ReactFailed {
                conv,
                sid,
                e,
                reason,
            }]);
            return;
        }
        self.recent_reacts.push_back(Instant::now());
        let reacting = self.social.chat_react(&conv, sid, &e, on);
        let control = self.control.clone();
        tokio::spawn(async move {
            if let Err(error) = reacting.await {
                tracing::debug!(%error, "the overlay's reaction did not go");
                let _ = control.send(Control::Send {
                    conn,
                    frames: vec![ToGame::ReactFailed {
                        conv,
                        sid,
                        e,
                        reason: react_failure(&error),
                    }],
                });
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_plain_sticker_reference_goes() {
        assert!(sticker_ref_ok("pl1/gg"));
        assert!(sticker_ref_ok("pl1/come-on"));
        assert!(!sticker_ref_ok("pl1/../gg"));
        assert!(!sticker_ref_ok("PL1/gg"));
        assert!(!sticker_ref_ok("pl1gg"));
        assert!(!sticker_ref_ok("verylongset/gg"));
        assert!(!sticker_ref_ok("pl1/"));
    }

    #[test]
    fn a_report_from_the_game_takes_a_players_reasons_and_a_clean_note() {
        assert_eq!(player_reason("harassment"), Some(ReportReason::Harassment));
        assert_eq!(player_reason("other"), Some(ReportReason::Other));
        // A message's reason is not a player's, and nothing else is either.
        assert_eq!(player_reason("inappropriate"), None);
        assert_eq!(player_reason(""), None);
        assert_eq!(report_note("  "), None);
        assert_eq!(
            report_note("  sürekli \n küfür\tediyor "),
            Some("sürekli küfür ediyor".into())
        );
        let long = "a".repeat(250);
        assert_eq!(report_note(&long).map(|n| n.chars().count()), Some(200));
        assert_eq!(
            friend_refusal(&SocialError::RateLimited {
                retry_after: None,
                item: None
            }),
            FriendRefusal::Wait
        );
        assert_eq!(
            friend_refusal(&SocialError::Unavailable { retry_after: None }),
            FriendRefusal::Offline
        );
        assert_eq!(
            friend_refusal(&SocialError::InvalidInput("x")),
            FriendRefusal::Failed
        );
    }

    #[test]
    fn refusals_name_what_the_panel_can_say() {
        let chat2 = Listener {
            english: false,
            chat2: true,
        };
        assert_eq!(
            refusal_of(&SocialError::InvalidInput("chat sticker"), chat2),
            SendFailure::FeatureOff
        );
        assert_eq!(
            refusal_of(
                &SocialError::InvalidInput("chat sticker"),
                Listener::default()
            ),
            SendFailure::Off
        );
        let full = |detail: &str| SocialError::Refused {
            status: 409,
            code: ErrorCode::ChatReactionsFull,
            message: String::new(),
            detail: Some(detail.to_owned()),
        };
        assert_eq!(react_failure(&full("mine")), ReactFailure::Full);
        assert_eq!(react_failure(&full("kinds")), ReactFailure::Kinds);
        assert_eq!(
            react_failure(&SocialError::InvalidInput("chat message")),
            ReactFailure::Old
        );
        assert_eq!(
            react_failure(&SocialError::ChatRejected {
                reason: ChatRejection::Words,
                rule: None
            }),
            ReactFailure::Words
        );
        assert_eq!(
            react_failure(&SocialError::ChatMuted { until: None }),
            ReactFailure::Muted
        );
        assert_eq!(
            react_failure(&SocialError::Unavailable { retry_after: None }),
            ReactFailure::Offline
        );
    }
}
