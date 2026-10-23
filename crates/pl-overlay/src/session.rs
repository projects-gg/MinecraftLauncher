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

use pl_social::{
    CallAnswer as SocialAnswer, ChatChangeView, ChatMessageView, ChatPage, ChatRejection,
    ChatTypingView, ErrorCode, SocialError, SocialNotice, SocialService, SocialView,
};
use serde::Serialize;
use tokio::sync::{mpsc, oneshot};

use crate::OverlayConfig;
use crate::feed::{self, Listener, Mirror, Scene, Sends};
use crate::host::{HostSignal, HostSink, WorldsShown};
use crate::protocol::{
    ByeReason, CallAnswer, FEATURE_CONNECT, FEATURE_HOST, FEATURE_TAGS, FromGame,
    MAX_MESSAGE_CHARS, ReactFailure, SendFailure, ToGame,
};
use crate::server::{BridgeEvent, ConnId, Control};

/// How long "Bağlan" waits for the game to say whether it switches servers.
pub const CONNECT_WAIT: Duration = Duration::from_secs(5);

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
}

impl SocialActions for SocialService {
    fn view(&self) -> SocialView {
        SocialService::view(self)
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

/// Called with `true` once the mod is greeted and `false` when it goes.
pub(crate) type ConnectedFn = Arc<dyn Fn(bool) + Send + Sync>;

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
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
        })
}

/// The scene of a view with what world sharing shows (HOST1): the friends who invited the player
/// to their world, and `host` among the features while the player may open theirs.
fn scene_of(view: &SocialView, config: &OverlayConfig, worlds: &WorldsShown) -> Scene {
    let mut scene = Scene::from_view(view, &config.notify, now_ms());
    if scene.signed_in {
        scene.mark_worlds(&worlds.marks);
        if worlds.may_host {
            scene.features.push(FEATURE_HOST.to_owned());
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
    pub(crate) fn new(
        social: S,
        launcher: String,
        config: OverlayConfig,
        control: mpsc::UnboundedSender<Control>,
        connected: ConnectedFn,
        host: HostSink,
        worlds: WorldsShown,
    ) -> Self {
        // The friends feature as it is now; every change after comes as a message.
        let view = Arc::new(social.view());
        let scene = scene_of(&view, &config, &worlds);
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
        }
    }

    pub(crate) async fn run(
        mut self,
        mut msgs: mpsc::UnboundedReceiver<Msg>,
        mut events: mpsc::UnboundedReceiver<BridgeEvent>,
    ) {
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
                        return;
                    }
                },
            }
        }
    }

    fn finish(&mut self, reason: ByeReason, done: Option<oneshot::Sender<()>>) {
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
        let was = self.live.is_some();
        self.live = live;
        if self.live.is_none()
            && let Some(pending) = self.pending_connect.take()
        {
            // The mod went before it answered.
            let _ = pending
                .reply
                .send(ConnectOutcome::unavailable("no-overlay"));
        }
        if was != self.live.is_some() {
            (self.connected)(self.live.is_some());
        }
        if was && self.live.is_none() {
            (self.host)(HostSignal::Gone);
        }
    }

    /// Takes a new view; whether someone now wears another crew tag than before (or none).
    fn set_view(&mut self, view: Arc<SocialView>) -> bool {
        let scene = scene_of(&view, &self.config, &self.worlds);
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

    fn send(&self, frames: Vec<ToGame>) {
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
            }
            Msg::Chat(change) => self.on_chat(&change),
            Msg::Notice(notice) => self.on_notice(&notice),
            Msg::Typing(typing) => self.on_typing(&typing),
            Msg::Config(config) => {
                // Conversation mutes and the reduced-motion choice show in the panel.
                self.config = config;
                self.set_view(Arc::clone(&self.view));
                self.refresh();
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
        }
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
        let name = self.scene.sender(&typing.conv, &typing.by);
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
                let mut mirror = Mirror::for_mod(tags);
                let frames = mirror.greet(&self.scene, &self.launcher, self.config.reduced_motion);
                // A reconnect forgets a "Bağlan" the earlier connection never answered.
                if let Some(pending) = self.pending_connect.take() {
                    let _ = pending
                        .reply
                        .send(ConnectOutcome::unavailable("no-overlay"));
                }
                let hosts = features.iter().any(|f| f == FEATURE_HOST);
                self.set_live(Some(Live {
                    conn,
                    mirror,
                    panel: false,
                    open: None,
                    connects: features.iter().any(|f| f == FEATURE_CONNECT),
                    listener: Listener::from_hello(&language, &features),
                    typed_at: None,
                    hosts,
                    tags,
                }));
                self.send(frames);
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
            | FromGame::HostStop { .. }) => {
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
            FromGame::Hello { .. } | FromGame::Unknown => {}
        }
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
