//! The launcher's one handle on the overlay ([`Overlay`]): the player's choices, and the sessions
//! of the games that run now.
//!
//! The friends feature's sink hands every update to [`Overlay::on_social`]; with no game session
//! it is dropped at once, so the overlay costs nothing between games. A launch calls
//! [`Overlay::start`] for a fresh bridge and passes the [`Endpoint`] to the game; the game's exit
//! ([`Overlay::stop_for`]), the launcher quitting or the switch turning off end it.
//!
//! Several games may run at once (PLAY-AGAIN), each with its own bridge and session: what the
//! friends feature says, the player's choices, the language and the chat emojis reach every one,
//! and a choice made in one game (the language, the emoji picker, a friend's marks) reaches the
//! others at once. World sharing (HOST1) follows one game's session alone: the first game's, until
//! the launcher names another ([`Overlay::set_host`]); the others see no world sharing.

use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use pl_content::ChatEmojis;
use pl_lang::Lang;
use pl_social::SocialUpdate;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};

use crate::host::{HostSignal, HostSink, WorldsShown};
use crate::policy::NotifyPrefs;
use crate::protocol::{ByeReason, ToGame};
use crate::server::{Bridge, Endpoint};
use crate::session::{
    CONNECT_WAIT, ConnectOutcome, ConnectedFn, DiscordFn, EmojiFn, LangFn, Msg, PeopleChange,
    PeopleFn, Session, SocialActions,
};

/// How long [`Overlay::stop`] waits for the goodbye to leave.
const STOP_WAIT: Duration = Duration::from_secs(1);

/// OVL-LANG: the file in the launcher's data folder that keeps the language the player picked in
/// the overlay's settings (`tr`, `en`; `auto` or none: the launcher's own language). One choice for
/// every install and Minecraft version, kept between games and launcher starts.
pub const LANGUAGE_FILE: &str = "overlay-language";

/// The overlay's language pick kept in `dir` ([`LANGUAGE_FILE`]); `None`: the player never picked
/// (or the file cannot be read), so the overlay speaks the launcher's language.
pub fn read_language_pick(dir: &Path) -> Option<Lang> {
    pl_lang::read_pick(&dir.join(LANGUAGE_FILE))
}

/// Keeps the overlay's language pick in `dir` (written only when it changed, through a temporary
/// file, so a crash never leaves half of it).
pub fn write_language_pick(dir: &Path, pick: Option<Lang>) -> io::Result<()> {
    pl_lang::write_pick(&dir.join(LANGUAGE_FILE), pick)
}

/// The player's choices that reach the overlay (Settings → Genel "Oyun içi katman", the friends
/// notification choices, Hareketi azalt). The page pushes them at its start and on every change;
/// the launcher keeps them in its data folder for the starts before the page asks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct OverlayConfig {
    /// "Oyun içi katman" (on by default): the official pack's overlay item is installed and each
    /// game start gets a bridge.
    pub enabled: bool,
    pub notify: NotifyPrefs,
    /// Performance mode or the system's reduced motion: toasts appear in place.
    pub reduced_motion: bool,
    /// "Sohbette emoji seçici" (CHAT-EMOJI, on by default): the chat screen's emoji button. The
    /// mod's jar goes into an instance while this or [`Self::enabled`] is on; with this one alone
    /// the mod runs the picker only (no panel, key, toasts or pause menu button).
    pub emoji: bool,
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            notify: NotifyPrefs::default(),
            reduced_motion: false,
            emoji: true,
        }
    }
}

impl OverlayConfig {
    /// The mod is wanted in a game: for its panel, its emoji picker or both.
    pub fn wants_mod(&self) -> bool {
        self.enabled || self.emoji
    }
}

/// Whether the mod in a running game is connected (`overlay-status`): while one is, its toasts
/// stand in for the launcher's system notifications of the same events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OverlayStatus {
    pub connected: bool,
}

type StatusFn = Arc<dyn Fn(OverlayStatus) + Send + Sync>;

/// Opens a friend's Discord profile for the friend menu in the game (OVL-UX): their Discord user id
/// and their web profile (`https://`). The launcher's own way: the Discord app when installed.
type DiscordSink = Arc<dyn Fn(String, String) + Send + Sync>;

/// Hears the marks the player changed on a friend from the game's person menu (OVL-UX2):
/// favourites and "Çevrimiçi olunca bildir", which the launcher's page keeps.
type PeopleSink = Arc<dyn Fn(PeopleChange) + Send + Sync>;

/// Hears the language the player picked in the overlay's settings (OVL-LANG): the launcher keeps
/// it ([`write_language_pick`]).
type LangSink = Arc<dyn Fn(Lang) + Send + Sync>;

/// Hears the emoji picker switched in the overlay's settings (CHAT-EMOJI): the launcher's page
/// keeps the switch (Settings → Genel shows the same one).
type EmojiSink = Arc<dyn Fn(bool) + Send + Sync>;

struct Running {
    id: u64,
    /// The bridge's port: names the session for [`Overlay::stop_for`].
    port: u16,
    msgs: mpsc::UnboundedSender<Msg>,
    /// Its mod is connected now.
    connected: bool,
    /// What its mod's hello told world sharing ([`HostSignal::Connected`]): told again when its
    /// game becomes the one world sharing follows ([`Overlay::set_host`]).
    hello: Option<HostSignal>,
}

struct Inner {
    config: OverlayConfig,
    /// The sessions of the games that run now, oldest first.
    sessions: Vec<Running>,
    /// The session world sharing follows (HOST1): the first game's, until the launcher names
    /// another. Kept after its session ended, so that its going still reaches world sharing.
    host: Option<u64>,
    /// What world sharing shows in the session it follows (HOST1).
    worlds: WorldsShown,
    /// OVL-LANG: the language the player picked in the overlay's settings (none: the launcher's).
    lang_pick: Option<Lang>,
    /// The launcher's own language (Settings → Dil, else the automatic one).
    launcher_lang: Lang,
    /// CHAT-EMOJI: the server's chat emojis for the picker (Studio's list, else the built-in one).
    emojis: Arc<ChatEmojis>,
}

impl Inner {
    /// The overlay's language: the player's pick in it, else the launcher's.
    fn language(&self) -> Lang {
        self.lang_pick.unwrap_or(self.launcher_lang)
    }

    fn session(&self, id: u64) -> Option<&Running> {
        self.sessions.iter().find(|running| running.id == id)
    }

    /// Hands `msg()` to every session but `except`.
    fn tell_others(&self, except: Option<u64>, msg: impl Fn() -> Msg) {
        for running in self.sessions.iter().filter(|r| Some(r.id) != except) {
            let _ = running.msgs.send(msg());
        }
    }

    /// What world sharing shows in session `id`: everything in the one it follows, nothing in the
    /// others (they never offer to open a world or join a friend's).
    fn worlds_for(&self, id: u64) -> WorldsShown {
        if self.host == Some(id) {
            self.worlds.clone()
        } else {
            WorldsShown::default()
        }
    }
}

struct Shared {
    inner: Mutex<Inner>,
    next: AtomicU64,
    connected: AtomicBool,
    status: Mutex<Option<StatusFn>>,
    /// World sharing (HOST1) hears the running session's mod here.
    host: Mutex<Option<HostSink>>,
    /// "Discord'dan mesaj at" from the game's friend menu (OVL-UX).
    discord: Mutex<Option<DiscordSink>>,
    /// Favourites and online notices changed from the game (OVL-UX2).
    people: Mutex<Option<PeopleSink>>,
    /// The overlay's language picked in the game (OVL-LANG).
    lang: Mutex<Option<LangSink>>,
    /// The emoji picker switched in the game (CHAT-EMOJI).
    emoji: Mutex<Option<EmojiSink>>,
}

/// Cheap to clone; the launcher keeps one.
#[derive(Clone)]
pub struct Overlay {
    shared: Arc<Shared>,
}

impl std::fmt::Debug for Overlay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Overlay")
            .field("connected", &self.connected())
            .finish_non_exhaustive()
    }
}

impl Overlay {
    pub fn new(config: OverlayConfig) -> Self {
        Self {
            shared: Arc::new(Shared {
                inner: Mutex::new(Inner {
                    config,
                    sessions: Vec::new(),
                    host: None,
                    worlds: WorldsShown::default(),
                    lang_pick: None,
                    launcher_lang: Lang::default(),
                    emojis: Arc::new(ChatEmojis::builtin()),
                }),
                next: AtomicU64::new(1),
                connected: AtomicBool::new(false),
                status: Mutex::new(None),
                host: Mutex::new(None),
                discord: Mutex::new(None),
                people: Mutex::new(None),
                lang: Mutex::new(None),
                emoji: Mutex::new(None),
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.shared
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn config(&self) -> OverlayConfig {
        self.lock().config.clone()
    }

    /// "Oyun içi katman" is on.
    pub fn enabled(&self) -> bool {
        self.lock().config.enabled
    }

    /// "Sohbette emoji seçici" is on (CHAT-EMOJI).
    pub fn emoji_enabled(&self) -> bool {
        self.lock().config.emoji
    }

    /// The mod is wanted in a game: for its panel, its emoji picker or both
    /// ([`OverlayConfig::wants_mod`]).
    pub fn wants_mod(&self) -> bool {
        self.lock().config.wants_mod()
    }

    /// CHAT-EMOJI: the server's chat emojis from now on (the content refresh brought Studio's
    /// list, or the built-in one while it has none). Running sessions hear a different list.
    pub fn set_emojis(&self, emojis: ChatEmojis) {
        let mut inner = self.lock();
        if *inner.emojis == emojis {
            return;
        }
        inner.emojis = Arc::new(emojis);
        let emojis = Arc::clone(&inner.emojis);
        inner.tell_others(None, || Msg::Emojis(Arc::clone(&emojis)));
    }

    /// The emoji picker switched in the overlay's settings (CHAT-EMOJI): `sink` hands it to the
    /// launcher's page, which keeps it (and pushes its choices back).
    pub fn on_emoji(&self, sink: impl Fn(bool) + Send + Sync + 'static) {
        *self
            .shared
            .emoji
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Arc::new(sink));
    }

    /// A session's mod switched the emoji picker: the choices here follow at once (the session's
    /// own already did, the other games' sessions hear it), then the page hears it. Only a running
    /// session counts.
    fn emoji_switched(&self, session: u64, on: bool) {
        {
            let mut inner = self.lock();
            if inner.session(session).is_none() || inner.config.emoji == on {
                return;
            }
            inner.config.emoji = on;
            let config = inner.config.clone();
            inner.tell_others(Some(session), || Msg::Config(config.clone()));
        }
        let sink = self
            .shared
            .emoji
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        if let Some(sink) = sink {
            sink(on);
        }
    }

    /// OVL-LANG: the overlay's language: the player's pick in its settings, else the launcher's.
    /// The game starts in it ([`crate::LANG_ARG`]) and the launcher words everything it writes
    /// for the mod in it.
    pub fn language(&self) -> Lang {
        self.lock().language()
    }

    /// The language the player picked in the overlay's settings (`None`: they never did).
    pub fn language_pick(&self) -> Option<Lang> {
        self.lock().lang_pick
    }

    /// The pick as the launcher kept it ([`read_language_pick`]), at its start. Nobody hears it
    /// back.
    pub fn set_language_pick(&self, pick: Option<Lang>) {
        let mut inner = self.lock();
        let before = inner.language();
        inner.lang_pick = pick;
        Self::tell_language(&inner, before);
    }

    /// The launcher's own language, at its start and whenever the player changes it there: the
    /// overlay follows it while the player has picked none in the game.
    pub fn set_launcher_language(&self, lang: Lang) {
        let mut inner = self.lock();
        let before = inner.language();
        inner.launcher_lang = lang;
        Self::tell_language(&inner, before);
    }

    /// The running sessions hear the overlay's language when it is no longer `before`.
    fn tell_language(inner: &Inner, before: Lang) {
        let now = inner.language();
        if now != before {
            inner.tell_others(None, || Msg::Language(now));
        }
    }

    /// The language the player picks in the overlay's settings (OVL-LANG): `sink` keeps it
    /// ([`write_language_pick`]). The overlay speaks it at once.
    pub fn on_language(&self, sink: impl Fn(Lang) + Send + Sync + 'static) {
        *self
            .shared
            .lang
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Arc::new(sink));
    }

    /// A session's mod picked the overlay's language: it holds from now on (the other games'
    /// sessions follow it, the sessions that start later too), then the launcher keeps it. Only a
    /// running session counts.
    fn language_picked(&self, session: u64, lang: Lang) {
        {
            let mut inner = self.lock();
            if inner.session(session).is_none() {
                return;
            }
            let before = inner.language();
            inner.lang_pick = Some(lang);
            if inner.language() != before {
                inner.tell_others(Some(session), || Msg::Language(lang));
            }
        }
        let sink = self
            .shared
            .lang
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        if let Some(sink) = sink {
            sink(lang);
        }
    }

    /// The page's choices. The sessions running now follow them; turning the overlay off ends
    /// them (`bye{disabled}`), as does turning off the emoji picker of games that have only that.
    /// Returns whether anything changed.
    pub fn configure(&self, config: OverlayConfig) -> bool {
        let ended = {
            let mut inner = self.lock();
            if inner.config == config {
                return false;
            }
            // The panel of a running game cannot go away while it runs: switching it off ends the
            // session, as before. A game started with the emoji picker alone keeps its session
            // while the picker stays on.
            let panel_off = inner.config.enabled && !config.enabled;
            inner.config = config.clone();
            if panel_off || !config.wants_mod() {
                std::mem::take(&mut inner.sessions)
            } else {
                inner.tell_others(None, || Msg::Config(config.clone()));
                Vec::new()
            }
        };
        if !ended.is_empty() {
            tracing::info!("the in-game overlay was turned off during the game");
        }
        for running in ended {
            let _ = running.msgs.send(Msg::Stop {
                reason: ByeReason::Disabled,
                done: None,
            });
            self.report(running.id, false);
        }
        true
    }

    /// Hears whether a mod is connected (the launcher tells its page).
    pub fn on_status(&self, sink: impl Fn(OverlayStatus) + Send + Sync + 'static) {
        *self
            .shared
            .status
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Arc::new(sink));
    }

    /// The mod in a running game is connected now.
    pub fn connected(&self) -> bool {
        self.shared.connected.load(Ordering::SeqCst)
    }

    /// A game session has a bridge now.
    pub fn active(&self) -> bool {
        !self.lock().sessions.is_empty()
    }

    /// Everything the friends feature tells the launcher's page goes to every game's session too.
    /// Without a game session it is dropped at once.
    pub fn on_social(&self, update: SocialUpdate) {
        let inner = self.lock();
        if inner.sessions.is_empty() {
            return;
        }
        match update {
            SocialUpdate::View(view) => {
                let view: Arc<_> = Arc::from(view);
                inner.tell_others(None, || Msg::View(Arc::clone(&view)));
            }
            SocialUpdate::Notice(notice) => inner.tell_others(None, || Msg::Notice(notice.clone())),
            SocialUpdate::Chat(change) => inner.tell_others(None, || Msg::Chat(change.clone())),
            // "Yazıyor…" (CHAT2): the sessions pass it on while the panel is open.
            SocialUpdate::Typing(typing) => inner.tell_others(None, || Msg::Typing(typing.clone())),
            // The player's own crew tag taken off by its crew's leader (TEAM6): the next view
            // greets the mod again without it (the panel's header drops the chip, OVL-UX); the
            // launcher's toast says why.
            SocialUpdate::TagOff(_) => {}
        }
    }

    /// A fresh bridge for a game about to start; the sessions of games already running stay
    /// (PLAY-AGAIN). The endpoint's two JVM arguments go on the game's command line.
    pub async fn start<S: SocialActions>(
        &self,
        social: S,
        launcher: impl Into<String>,
    ) -> io::Result<Endpoint> {
        self.open(social, launcher.into(), None).await
    }

    /// [`Overlay::start`] for a game that plays as the account `game` (its game name): the
    /// session vouches for that account only when a Projects server asks the game (SKIN1-O,
    /// codeless linking).
    pub async fn start_with_game<S: SocialActions>(
        &self,
        social: S,
        launcher: impl Into<String>,
        game: impl Into<String>,
    ) -> io::Result<Endpoint> {
        self.open(social, launcher.into(), Some(game.into())).await
    }

    async fn open<S: SocialActions>(
        &self,
        social: S,
        launcher: String,
        game: Option<String>,
    ) -> io::Result<Endpoint> {
        let (bridge, endpoint) = Bridge::bind().await?;
        let id = self.shared.next.fetch_add(1, Ordering::Relaxed);
        let (control, control_rx) = mpsc::unbounded_channel();
        let (events_tx, events) = mpsc::unbounded_channel();
        let (msgs, msgs_rx) = mpsc::unbounded_channel();
        let (config, worlds, lang, emojis) = {
            let mut inner = self.lock();
            // The first game's session is the one world sharing follows, until the launcher names
            // another (`set_host`); a game started beside others sees no world sharing.
            if inner.sessions.is_empty() {
                inner.host = Some(id);
            }
            inner.sessions.push(Running {
                id,
                port: endpoint.port(),
                msgs: msgs.clone(),
                connected: false,
                hello: None,
            });
            (
                inner.config.clone(),
                inner.worlds_for(id),
                inner.language(),
                Arc::clone(&inner.emojis),
            )
        };
        // The game starts in the overlay's language (G2), with the panel and the emoji picker as
        // they are now (CHAT-EMOJI).
        let endpoint = endpoint
            .speaking(lang)
            .switched(config.enabled, config.emoji);
        let hub = self.clone();
        let connected: ConnectedFn = Arc::new(move |on| hub.report(id, on));
        let hub = self.clone();
        let host: HostSink = Arc::new(move |signal| hub.host_signal(id, signal));
        let hub = self.clone();
        let discord: DiscordFn = Arc::new(move |friend, url| hub.open_discord(friend, url));
        let hub = self.clone();
        let people: PeopleFn = Arc::new(move |change| hub.people_change(id, change));
        let hub = self.clone();
        let picked: LangFn = Arc::new(move |lang| hub.language_picked(id, lang));
        let hub = self.clone();
        let emoji: EmojiFn = Arc::new(move |on| hub.emoji_switched(id, on));
        let session = Session::new(
            social,
            launcher,
            config,
            control,
            connected,
            host,
            worlds,
            game,
            discord,
            people,
            (lang, picked),
            (emojis, emoji),
        );
        tokio::spawn(bridge.serve(control_rx, events_tx));
        tokio::spawn(session.run(msgs_rx, events));
        tracing::info!(port = endpoint.port(), "in-game overlay bridge ready");
        Ok(endpoint)
    }

    /// "Bağlan" (a crew call, an invite, a plan), which the player chose and confirmed in the
    /// launcher: the running game joins `target` (`host[:port]`) through its overlay instead of
    /// starting again. Never call it on the launcher's own initiative (a reminder, a timer).
    ///
    /// Only Projects or an address a friend or crew shares right now, one at a time, and only with
    /// a mod that can (`connect` in its hello). [`ConnectOutcome::Unavailable`] when any of that
    /// fails or the game does not answer within [`CONNECT_WAIT`]: the caller then starts the game
    /// again as before.
    pub async fn connect_in_game(&self, target: &str) -> ConnectOutcome {
        self.connect(None, target.to_owned(), None).await
    }

    /// [`Overlay::connect_in_game`] for the game whose bridge is on `game` (PLAY-AGAIN: the one the
    /// player meant among several); `None`: the newest game.
    pub async fn connect_in_game_at(&self, game: Option<u16>, target: &str) -> ConnectOutcome {
        self.connect(game, target.to_owned(), None).await
    }

    /// HOST1 (H12): "Katıl" on a friend's world while the game runs: the game moves to the
    /// launcher's own way to that world (`127.0.0.1:port`) and the mod shows `label` ("Arkadaş
    /// dünyası: Ayşe") instead of an address. The same rules as [`Overlay::connect_in_game`], and
    /// only with a mod whose hello lists `host`.
    pub async fn connect_world_in_game(&self, port: u16, label: String) -> ConnectOutcome {
        self.connect(None, format!("127.0.0.1:{port}"), Some(label))
            .await
    }

    /// [`Overlay::connect_world_in_game`] for the game whose bridge is on `game`; `None`: the
    /// newest game.
    pub async fn connect_world_in_game_at(
        &self,
        game: Option<u16>,
        port: u16,
        label: String,
    ) -> ConnectOutcome {
        self.connect(game, format!("127.0.0.1:{port}"), Some(label))
            .await
    }

    async fn connect(
        &self,
        game: Option<u16>,
        target: String,
        label: Option<String>,
    ) -> ConnectOutcome {
        let msgs = {
            let inner = self.lock();
            let running = match game {
                Some(port) => inner.sessions.iter().find(|running| running.port == port),
                None => inner.sessions.last(),
            };
            match running {
                Some(running) => running.msgs.clone(),
                None => return ConnectOutcome::unavailable("no-overlay"),
            }
        };
        let (reply, answer) = oneshot::channel();
        let asked = Msg::Connect {
            target,
            label,
            reply,
        };
        if msgs.send(asked).is_err() {
            return ConnectOutcome::unavailable("no-overlay");
        }
        match tokio::time::timeout(CONNECT_WAIT, answer).await {
            Ok(Ok(outcome)) => outcome,
            Ok(Err(_)) => ConnectOutcome::unavailable("no-overlay"),
            Err(_) => ConnectOutcome::unavailable("timeout"),
        }
    }

    /// Ends every running session: each mod hears `bye{reason}` (when it is still there) and the
    /// bridges close. Waits for that, at most [`STOP_WAIT`].
    pub async fn stop(&self, reason: ByeReason) {
        self.end(|_| true, reason).await;
    }

    /// [`Overlay::stop`] for the session of the bridge on `port` only (its game ended, its launch
    /// stopped): the other games' sessions are left alone.
    pub async fn stop_for(&self, port: u16, reason: ByeReason) {
        self.end(|running| running.port == port, reason).await;
    }

    async fn end(&self, which: impl Fn(&Running) -> bool, reason: ByeReason) {
        let ended: Vec<Running> = {
            let mut inner = self.lock();
            let (ended, kept) = std::mem::take(&mut inner.sessions)
                .into_iter()
                .partition(|running| which(running));
            inner.sessions = kept;
            ended
        };
        let mut closing = Vec::new();
        for running in ended {
            self.report(running.id, false);
            let (done, closed) = oneshot::channel();
            if running
                .msgs
                .send(Msg::Stop {
                    reason,
                    done: Some(done),
                })
                .is_ok()
            {
                closing.push(closed);
            }
        }
        if closing.is_empty() {
            return;
        }
        let _ = tokio::time::timeout(STOP_WAIT, async {
            for closed in closing {
                let _ = closed.await;
            }
        })
        .await;
    }

    /// HOST1 with several games (PLAY-AGAIN): world sharing follows the game whose bridge is on
    /// `port` from now on (`None`: no game; the launcher names the game that took over when the
    /// one it followed ended). The session that leaves the role sees no world sharing any more;
    /// the one that takes it gets what world sharing shows, and its mod's last hello is told to
    /// world sharing again.
    pub fn set_host(&self, port: Option<u16>) {
        let (left, took, hello, worlds) = {
            let mut inner = self.lock();
            let next = port.and_then(|port| {
                inner
                    .sessions
                    .iter()
                    .find(|running| running.port == port)
                    .map(|running| running.id)
            });
            if next == inner.host {
                return;
            }
            let left = inner
                .host
                .and_then(|id| inner.session(id))
                .map(|running| running.msgs.clone());
            inner.host = next;
            let took = next.and_then(|id| inner.session(id));
            (
                left,
                took.map(|running| running.msgs.clone()),
                took.and_then(|running| running.hello.clone()),
                inner.worlds.clone(),
            )
        };
        if let Some(left) = left {
            let _ = left.send(Msg::Worlds(WorldsShown::default()));
        }
        if let Some(took) = took {
            let _ = took.send(Msg::Worlds(worlds));
        }
        if let Some(hello) = hello {
            self.tell_host(hello);
        }
    }

    /// The port of the session world sharing follows, while it runs.
    pub fn host_port(&self) -> Option<u16> {
        let inner = self.lock();
        inner
            .host
            .and_then(|id| inner.session(id))
            .map(|running| running.port)
    }

    /// World sharing (HOST1) hears the running game's mod: its hello (whether it hosts), its
    /// `host-*` frames, an in-game "Katıl" on a world invite, and its going.
    pub fn on_host(&self, sink: impl Fn(HostSignal) + Send + Sync + 'static) {
        *self
            .shared
            .host
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Arc::new(sink));
    }

    /// "Discord'dan mesaj at" from the game's friend menu (OVL-UX): `sink` opens a friend's
    /// profile (their Discord user id, their web profile) the launcher's own way.
    pub fn on_discord(&self, sink: impl Fn(String, String) + Send + Sync + 'static) {
        *self
            .shared
            .discord
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Arc::new(sink));
    }

    /// Favourites and "Çevrimiçi olunca bildir" changed from the game's person menu (OVL-UX2):
    /// `sink` hands them to the launcher's page, which keeps them (and pushes its choices back).
    pub fn on_people(&self, sink: impl Fn(PeopleChange) + Send + Sync + 'static) {
        *self
            .shared
            .people
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Arc::new(sink));
    }

    /// A session's mod changed a friend's mark: the choices here follow at once (the other games'
    /// sessions hear them, the sessions that start later too, and the page pushing the same choices
    /// back changes nothing), then the page hears it. Only a running session counts.
    fn people_change(&self, session: u64, change: PeopleChange) {
        {
            let mut inner = self.lock();
            if inner.session(session).is_none() {
                return;
            }
            let notify = &mut inner.config.notify;
            match &change {
                PeopleChange::Favorite { friend, on } => notify.set_favorite(friend, *on),
                PeopleChange::Alert { friend, on } => {
                    notify.online_alerts.insert(friend.clone(), *on);
                }
            }
            let config = inner.config.clone();
            inner.tell_others(Some(session), || Msg::Config(config.clone()));
        }
        let sink = self
            .shared
            .people
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        if let Some(sink) = sink {
            sink(change);
        }
    }

    /// Hands a friend's Discord profile to the launcher's opener; false without one.
    fn open_discord(&self, friend: &str, url: &str) -> bool {
        let sink = self
            .shared
            .discord
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        sink.is_some_and(|sink| {
            sink(friend.to_owned(), url.to_owned());
            true
        })
    }

    /// HOST1: frames for the mod of the game world sharing follows (H5, H8, H10, H11, H14),
    /// dropped unless its hello lists `host`. Only right after the player's own press, or for a
    /// security event (H16).
    pub fn host_send(&self, frames: Vec<ToGame>) {
        let inner = self.lock();
        if let Some(running) = inner.host.and_then(|id| inner.session(id)) {
            let _ = running.msgs.send(Msg::Host(frames));
        }
    }

    /// HOST1: what the game world sharing follows shows of it (H2, H13): the friends who invited
    /// the player to their open world, and whether the player may open theirs now. Kept for the
    /// sessions that start later.
    pub fn set_worlds(&self, worlds: WorldsShown) {
        let msgs = {
            let mut inner = self.lock();
            if inner.worlds == worlds {
                return;
            }
            inner.worlds = worlds.clone();
            inner
                .host
                .and_then(|id| inner.session(id))
                .map(|running| running.msgs.clone())
        };
        if let Some(msgs) = msgs {
            let _ = msgs.send(Msg::Worlds(worlds));
        }
    }

    /// A session's mod said something to world sharing: only the session it follows counts (also
    /// its going after the hub took it away). Every session's hello is kept, for the day its game
    /// becomes the one world sharing follows.
    fn host_signal(&self, session: u64, signal: HostSignal) {
        let counts = {
            let mut inner = self.lock();
            if let Some(running) = inner.sessions.iter_mut().find(|r| r.id == session) {
                match &signal {
                    HostSignal::Connected { .. } => running.hello = Some(signal.clone()),
                    HostSignal::Gone => running.hello = None,
                    _ => {}
                }
            }
            inner.host == Some(session)
        };
        if counts {
            self.tell_host(signal);
        }
    }

    /// Hands `signal` to world sharing.
    fn tell_host(&self, signal: HostSignal) {
        let sink = self
            .shared
            .host
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        if let Some(sink) = sink {
            sink(signal);
        }
    }

    /// A session says whether its mod is connected (the hub itself, `false`, for a session it just
    /// took away). The page hears whether any game's mod is connected; a session already gone
    /// cannot connect.
    fn report(&self, session: u64, connected: bool) {
        let connected = {
            let mut inner = self.lock();
            match inner
                .sessions
                .iter_mut()
                .find(|running| running.id == session)
            {
                Some(running) => running.connected = connected,
                None if connected => return,
                None => {}
            }
            inner.sessions.iter().any(|running| running.connected)
        };
        if self.shared.connected.swap(connected, Ordering::SeqCst) == connected {
            return;
        }
        let sink = self
            .shared
            .status
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        if let Some(sink) = sink {
            sink(OverlayStatus { connected });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_config_reads_the_pages_json_and_defaults_to_on() {
        let config: OverlayConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(config, OverlayConfig::default());
        assert!(config.enabled && !config.notify.crew_chat && config.notify.crew_calls);
        let config: OverlayConfig = serde_json::from_str(
            r#"{"enabled":false,"reducedMotion":true,"notify":{"crewChat":true},"later":1}"#,
        )
        .unwrap();
        assert!(!config.enabled && config.reduced_motion && config.notify.crew_chat);
        // CHAT-EMOJI: a file from before the emoji picker has it on; the mod is wanted for it.
        assert!(config.emoji && config.wants_mod());
        let config: OverlayConfig =
            serde_json::from_str(r#"{"enabled":false,"emoji":false}"#).unwrap();
        assert!(!config.wants_mod());
    }

    #[test]
    fn the_overlays_language_is_the_players_pick_else_the_launchers() {
        let overlay = Overlay::new(OverlayConfig::default());
        assert_eq!(overlay.language(), Lang::Tr);
        overlay.set_launcher_language(Lang::En);
        assert_eq!(overlay.language(), Lang::En, "it follows the launcher");
        assert_eq!(overlay.language_pick(), None);
        overlay.set_language_pick(Some(Lang::Tr));
        assert_eq!(overlay.language(), Lang::Tr);
        overlay.set_launcher_language(Lang::Tr);
        overlay.set_launcher_language(Lang::En);
        assert_eq!(
            overlay.language(),
            Lang::Tr,
            "a pick holds, whatever the launcher speaks"
        );
        overlay.set_language_pick(None);
        assert_eq!(overlay.language(), Lang::En);
    }

    #[test]
    fn the_pick_is_kept_in_the_data_folder() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_language_pick(dir.path()), None, "never picked");
        write_language_pick(dir.path(), Some(Lang::En)).unwrap();
        assert_eq!(read_language_pick(dir.path()), Some(Lang::En));
        assert_eq!(
            std::fs::read_to_string(dir.path().join(LANGUAGE_FILE)).unwrap(),
            "en"
        );
        write_language_pick(dir.path(), Some(Lang::Tr)).unwrap();
        assert_eq!(read_language_pick(dir.path()), Some(Lang::Tr));
        write_language_pick(dir.path(), None).unwrap();
        assert_eq!(read_language_pick(dir.path()), None);
        std::fs::write(dir.path().join(LANGUAGE_FILE), "\u{0}garbage").unwrap();
        assert_eq!(
            read_language_pick(dir.path()),
            None,
            "unreadable: the launcher's"
        );
        assert!(!dir.path().join("overlay-language.tmp").exists());
    }

    #[test]
    fn configure_says_whether_anything_changed() {
        let overlay = Overlay::new(OverlayConfig::default());
        assert!(!overlay.configure(OverlayConfig::default()));
        let off = OverlayConfig {
            enabled: false,
            ..OverlayConfig::default()
        };
        assert!(overlay.configure(off.clone()));
        assert!(!overlay.enabled());
        assert_eq!(overlay.config(), off);
        // Without a game nothing runs, and updates are dropped.
        assert!(!overlay.active() && !overlay.connected());
        overlay.on_social(SocialUpdate::Notice(
            pl_social::SocialNotice::SignedOutRemotely,
        ));
    }
}
