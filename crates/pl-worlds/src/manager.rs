//! The launcher's world sharing ([`Worlds`]): the player's own session (opening, publishing to the
//! game, invites, guests, closing) and the friends' invites the player may join (matching an
//! install, the local endpoint, the game's way in), between `pl-host`'s sessions, the friends
//! service ([`Service`]), the running game's overlay ([`Bridge`]) and the launcher's own parts
//! ([`Launcher`]).
//!
//! Everything is event driven: nothing polls and no timer runs while no session or join is live
//! (the telemetry task waits on a notification while it has nothing to send). The page hears a
//! whole [`HostView`] on every change and [`HostNotice`]s for moments worth a toast.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pl_host::guest::local::AcceptFilter;
use pl_host::guest::{GuestConfig, GuestEnd, GuestEvent, GuestSession, RefusalText};
use pl_host::host::binding::BindingKey;
use pl_host::host::gate::GateKey;
use pl_host::host::{
    GameEndpoint, GuestStatus, GuestUpdate, HostConfig, HostEvent, HostSession, InviteError,
};
use pl_host::manifest::matching::{Decision, InstallInfo, decide};
use pl_host::manifest::{Manifest, ModRef};
use pl_host::policy::Policy;
use pl_host::policy::guest::JoinFailure;
use pl_host::report::Event;
use pl_host::report::batch::Reporter;
use pl_host::transport::BoxFuture;
use pl_host::transport::turn::Secret;
use pl_host::tunnel::control::{NameVerdict, Refusal};
use pl_host::tunnel::{GuestId, HostKey, Join, Paths, SessionId, StreamId};
use pl_overlay::protocol::{FromGame, HostGuestView, ToGame, ToastKind, ToastView, WorldMark};
use pl_overlay::{HostSignal, WorldsShown};
use pl_social::{
    ActivityView, AuthPhase, Hidden, PresenceState, SocialView, WorldError, WorldEvent, WorldFrame,
    WorldsStatus,
};
use tokio::sync::{Notify, oneshot};

use crate::link::{CredsTable, GuestLink, HostLink, PeerWaiters, Service, Transports, lock};
use crate::view::{
    HostNotice, HostPlace, HostSessionView, HostView, WorldGameMode, WorldGuest, WorldGuestState,
    WorldInviteOutcome, WorldInviteRefusal, WorldInviteView, WorldJoinAnswer, WorldJoinAsk,
    WorldJoinFailure, WorldJoinStage, WorldNameVerdict, WorldOpenFailure, WorldOpenOutcome,
    WorldOpenRequest, WorldOpeningStep,
};

/// How long the service may take with a world request.
const REQUEST_WAIT: Duration = Duration::from_secs(5);
/// How long the mod may take with `host-published` (H6).
const PUBLISH_WAIT: Duration = Duration::from_secs(10);
/// "Projects'te buluşun" waits this long for the player to reach Projects before its invites go.
const MEET_WAIT: Duration = Duration::from_secs(180);
/// Progress reaches the page at most this often (L2: at most 4 a second).
const PROGRESS_EVERY: Duration = Duration::from_millis(250);
/// How long a join's local endpoint waits for the game, and its control stream for a game stream:
/// the page's launch installs or syncs the pack first, which can take minutes on a first join
/// (PROTOCOL.md §8 says 60 s for a game that is ready). Loopback only, one connection.
const JOIN_WAIT_MS: u64 = 10 * 60 * 1000;

/// A percent from the launcher's long work (0–100).
pub type Progress = Arc<dyn Fn(u8) + Send + Sync>;

/// The install a world is in, as hosting sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostInstall {
    /// Its Minecraft version.
    pub mc: String,
    /// The official pack's family it follows.
    pub family: Option<String>,
    /// The game folder (worlds are in `saves/`).
    pub game_dir: PathBuf,
    /// It may host now: it follows the official pack on 26.1.2, 26.2 or 26.3, the pack puts the
    /// overlay mod in it and no e4mc or e4all jar is there.
    pub hostable: bool,
}

/// A game account of the player.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub id: String,
    /// The in-game name.
    pub name: String,
    /// A Microsoft account.
    pub premium: bool,
}

/// What world sharing needs from the launcher's own parts.
pub trait Launcher: Send + Sync + 'static {
    /// The install `instance_id` as hosting sees it.
    fn host_install(&self, instance_id: &str) -> BoxFuture<'_, Option<HostInstall>>;
    /// The world manifest of the install (its enabled both-side mods), for hosting.
    fn manifest(&self, instance_id: &str) -> BoxFuture<'_, Option<Manifest>>;
    /// A world's shown name.
    fn world_name(&self, instance_id: &str, folder: &str) -> BoxFuture<'_, String>;
    /// The backup a world needs before it opens from the launcher (its last one is older than a
    /// day), with its progress; false when it failed.
    fn backup_if_old(
        &self,
        instance_id: &str,
        folder: &str,
        progress: Progress,
    ) -> BoxFuture<'_, bool>;
    /// The active game account.
    fn account(&self) -> Option<Account>;
    /// Makes this account the active one.
    fn use_account(&self, account_id: &str) -> BoxFuture<'_, Option<Account>>;
    /// The player's installs of Minecraft `mc` as install matching sees them (installs of
    /// other versions never match).
    fn installs(&self, mc: &str) -> BoxFuture<'_, Vec<InstallInfo>>;
    /// The pack version the signed content names for `mc` in this pack family.
    fn latest_pack(&self, family: &str, mc: &str) -> BoxFuture<'_, Option<String>>;
    /// Refreshes the signed content once.
    fn refresh_content(&self) -> BoxFuture<'_, ()>;
    /// "Kur ve katıl": an install following the official pack at `mc` (its files come with the
    /// game's start); its id.
    fn install_official(&self, mc: &str) -> BoxFuture<'_, Option<String>>;
    /// The download size of these mods.
    fn mods_size(&self, mods: &[ModRef]) -> BoxFuture<'_, u64>;
    /// "Kopyala ve katıl": a new install named `name` for the friend `host`'s world: the official
    /// pack at the manifest's version and the manifest's mods; its id.
    fn make_copy(
        &self,
        name: &str,
        host: &str,
        manifest: &Manifest,
        progress: Progress,
    ) -> BoxFuture<'_, Option<String>>;
    /// An earlier copy for this host set to the manifest's mods again.
    fn update_copy(
        &self,
        instance_id: &str,
        manifest: &Manifest,
        progress: Progress,
    ) -> BoxFuture<'_, bool>;
    /// Which connections to the local endpoint belong to the game the launcher starts.
    fn accept_filter(&self) -> Option<AcceptFilter>;
}

/// The running game's overlay as world sharing uses it (`pl_overlay::Overlay`).
pub trait Bridge: Send + Sync + 'static {
    /// Frames for a mod whose hello lists `host` (dropped for any other).
    fn send(&self, frames: Vec<ToGame>);
    /// What every game session shows of world sharing.
    fn show(&self, shown: WorldsShown);
    /// The running game moves into a friend's world (`127.0.0.1:port`), showing `label`.
    fn connect(&self, port: u16, label: String) -> BoxFuture<'_, bool>;
}

/// "Birlikte oyna" for "Projects'te buluşun" (the friends feature's invite).
pub trait PlayTogether: Send + Sync + 'static {
    fn invite(&self, friend: &str) -> BoxFuture<'_, ()>;
}

/// What the page hears.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiEvent {
    Changed(HostView),
    Notice(HostNotice),
}

pub type UiSink = Arc<dyn Fn(UiEvent) + Send + Sync>;

/// How world sharing is set up.
#[derive(Debug, Clone)]
pub struct WorldsConfig {
    /// The launcher's version (`hello.launcher`, telemetry `lv`, the e4mc broker's User-Agent).
    pub launcher_version: String,
    /// The launcher's data folder (`host/guest-binding.key`).
    pub data_dir: PathBuf,
    pub policy: Policy,
    pub transports: Transports,
}

impl WorldsConfig {
    pub fn new(launcher_version: &str, data_dir: PathBuf) -> Self {
        Self {
            launcher_version: launcher_version.to_owned(),
            data_dir,
            policy: Policy::default(),
            transports: Transports::new(launcher_version),
        }
    }
}

/// A friend as the friends feature shows them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Person {
    pub(crate) name: String,
    pub(crate) avatar: Option<String>,
    pub(crate) avatar_url: Option<String>,
    pub(crate) online: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OpenWorld {
    folder: String,
    lan: bool,
}

/// The game that runs now, as far as world sharing cares.
#[derive(Default)]
struct Game {
    instance_id: Option<String>,
    /// The bridge token of the game's overlay (the gate key comes from it).
    token: Option<Secret>,
    /// Its install may host (set once the launcher looked).
    hostable: bool,
    /// The mod is connected; `hosts`: its hello lists `host`.
    connected: bool,
    hosts: bool,
    world: Option<OpenWorld>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Opening(WorldOpeningStep, Option<u8>),
    Open,
    Failed(WorldOpenFailure),
}

struct Guest {
    friend: String,
    state: WorldGuestState,
    game_name: Option<String>,
    guest: Option<GuestId>,
}

struct Hosting {
    generation: u64,
    instance_id: String,
    folder: String,
    world_name: String,
    mode: WorldGameMode,
    commands: bool,
    max: u8,
    guests: Vec<Guest>,
    phase: Phase,
    /// Opened from the launcher: it waits for its world to load.
    from_launcher: bool,
    cancel: Option<oneshot::Sender<()>>,
    session: Option<SessionId>,
    publish: Option<PublishWait>,
    live: Option<HostSession>,
    /// Why the next `host-remove` for a friend goes (`removed`, `blocked`, `restricted`).
    removing: HashMap<String, &'static str>,
    /// The last `host-state` the mod got.
    mirrored: Option<serde_json::Value>,
}

/// Where the mod opened the world (`host-published`): its port and protocol, or why not.
type Published = Result<(u16, i32), String>;

/// An opening waiting for the mod's `host-published` with this publish id.
struct PublishWait {
    id: String,
    answer: oneshot::Sender<Published>,
}

struct Invite {
    session: String,
    from: String,
    at: i64,
    quiet: bool,
    join: Join,
    guests: u8,
    max: u8,
    stage: Option<WorldJoinStage>,
    generation: u64,
    ask: Option<oneshot::Sender<WorldJoinAnswer>>,
    live: Option<GuestSession>,
    task: Option<tokio::task::AbortHandle>,
    /// Asked from the game's overlay: the game moves in by itself once ready.
    in_game: bool,
    progress_at: Option<Instant>,
}

struct Meet {
    friends: Vec<String>,
    until: Instant,
}

#[derive(Default)]
struct State {
    seq: u64,
    /// Signed config `features.worlds` with friends on.
    allowed: bool,
    /// The last connection took `worlds` (with Studio's switch); kept while it reconnects.
    taken: Option<WorldsStatus>,
    friends: HashMap<String, Person>,
    on_projects: bool,
    game: Game,
    hosting: Option<Hosting>,
    /// Newest first.
    invites: Vec<Invite>,
    meet: Option<Meet>,
    next_generation: u64,
    shown: Option<WorldsShown>,
}

impl State {
    fn next(&mut self) -> u64 {
        self.next_generation += 1;
        self.next_generation
    }

    fn switch_on(&self) -> bool {
        self.allowed && self.taken.is_some_and(|status| status.on)
    }

    /// The page shows world sharing: the switch is on, or something of it still runs.
    fn enabled(&self) -> bool {
        self.allowed
            && self.taken.is_some()
            && (self.switch_on()
                || self.hosting.as_ref().is_some_and(|h| h.live.is_some())
                || self.invites.iter().any(|i| i.live.is_some()))
    }

    fn max_guests(&self) -> u8 {
        self.taken.map_or(7, |status| status.max_guests.clamp(1, 7))
    }

    fn person(&self, id: &str) -> Person {
        self.friends.get(id).cloned().unwrap_or_default()
    }

    fn invite(&mut self, session: &str) -> Option<&mut Invite> {
        self.invites.iter_mut().find(|i| i.session == session)
    }
}

struct Telemetry {
    reporter: Mutex<Reporter>,
    wake: Notify,
    running: AtomicBool,
}

struct Inner {
    service: Arc<dyn Service>,
    bridge: Arc<dyn Bridge>,
    launcher: Arc<dyn Launcher>,
    together: Arc<dyn PlayTogether>,
    ui: UiSink,
    config: WorldsConfig,
    creds: Arc<CredsTable>,
    peers: Arc<PeerWaiters>,
    telemetry: Telemetry,
    state: Mutex<State>,
}

/// World sharing. Cheap to clone; the launcher keeps one.
#[derive(Clone)]
pub struct Worlds {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for Worlds {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Worlds").finish_non_exhaustive()
    }
}

fn mode_wire(mode: WorldGameMode) -> String {
    mode.as_str().to_owned()
}

fn is_projects(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    host == "projects.gg" || host.ends_with(".projects.gg")
}

/// The copy's name and the in-game label for a friend's world, in the player's language.
pub fn world_label(friend: &str) -> String {
    pl_lang::pick(
        format!("Arkadaş dünyası: {friend}"),
        format!("{friend}'s world"),
    )
}

/// The game's disconnect texts for the host's refusals, in the player's language (§6.9).
pub fn refusal_text() -> RefusalText {
    Arc::new(|refusal| {
        let (tr, en) = match refusal {
            Refusal::Full => ("Dünya dolu.", "The world is full."),
            Refusal::Closed => ("Dünya kapandı.", "The world closed."),
            Refusal::Removed => (
                "Bu dünyadan çıkarıldın.",
                "You were removed from this world.",
            ),
            Refusal::Name => ("Bu adla giremezsin.", "You can't join with this name."),
            Refusal::NameTaken => (
                "Bu ad bu dünyada başka birine ait.",
                "Someone else uses this name in this world.",
            ),
            _ => ("Bağlanılamadı.", "Couldn't connect."),
        };
        pl_lang::pick(tr, en).to_owned()
    })
}

fn failure_of(refusal: Refusal) -> WorldJoinFailure {
    match refusal {
        Refusal::Full => WorldJoinFailure::Full,
        Refusal::Closed => WorldJoinFailure::Closed,
        Refusal::Removed => WorldJoinFailure::Removed,
        _ => WorldJoinFailure::Unreachable,
    }
}

impl Worlds {
    pub fn new(
        service: Arc<dyn Service>,
        bridge: Arc<dyn Bridge>,
        launcher: Arc<dyn Launcher>,
        together: Arc<dyn PlayTogether>,
        ui: UiSink,
        config: WorldsConfig,
    ) -> Self {
        let reporter = Reporter::new(config.launcher_version.clone());
        Self {
            inner: Arc::new(Inner {
                service,
                bridge,
                launcher,
                together,
                ui,
                config,
                creds: Arc::new(CredsTable::default()),
                peers: Arc::new(PeerWaiters::default()),
                telemetry: Telemetry {
                    reporter: Mutex::new(reporter),
                    wake: Notify::new(),
                    running: AtomicBool::new(false),
                },
                state: Mutex::new(State::default()),
            }),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        lock(&self.inner.state)
    }

    // ── What the page shows ──────────────────────────────────────────────────────────────────

    /// The view now (`host_state`).
    pub fn view(&self) -> HostView {
        let st = self.state();
        build_view(&st)
    }

    /// The page, the game's overlay rows and the mod's mirror of the session, after a change.
    fn changed(&self) {
        let (view, shown, mirror) = {
            let mut st = self.state();
            st.seq += 1;
            let view = build_view(&st);
            let shown = WorldsShown {
                marks: if st.enabled() {
                    st.invites
                        .iter()
                        .map(|i| {
                            (
                                i.from.clone(),
                                WorldMark {
                                    guests: i.guests,
                                    max: i.max,
                                },
                            )
                        })
                        .collect()
                } else {
                    HashMap::new()
                },
                may_host: st.switch_on() && st.game.hostable,
            };
            let shown = (st.shown.as_ref() != Some(&shown)).then(|| {
                st.shown = Some(shown.clone());
                shown
            });
            let mirror = mirror_frame(&st).and_then(|frame| {
                let value = serde_json::to_value(&frame).ok()?;
                let hosting = st.hosting.as_mut()?;
                (hosting.mirrored.as_ref() != Some(&value)).then(|| {
                    hosting.mirrored = Some(value);
                    frame
                })
            });
            (view, shown, mirror)
        };
        if let Some(shown) = shown {
            self.inner.bridge.show(shown);
        }
        if let Some(frame) = mirror {
            self.inner.bridge.send(vec![frame]);
        }
        (self.inner.ui)(UiEvent::Changed(view));
    }

    fn notice(&self, notice: HostNotice) {
        (self.inner.ui)(UiEvent::Notice(notice));
    }

    // ── Inputs from the launcher ─────────────────────────────────────────────────────────────

    /// The signed config's `features.worlds` (with friends on). The presence connection asks for
    /// `worlds` only while it is on (the launcher tells the friends feature too).
    pub fn set_allowed(&self, allowed: bool) {
        {
            let mut st = self.state();
            if st.allowed == allowed {
                return;
            }
            st.allowed = allowed;
        }
        self.changed();
    }

    /// The friends feature's view: names and pictures, who is online, where the player is.
    pub fn on_view(&self, view: &SocialView) {
        let friends: HashMap<String, Person> = view
            .friends
            .iter()
            .map(|f| {
                (
                    f.id.clone(),
                    Person {
                        name: f.name.clone(),
                        avatar: (!f.avatar.is_empty()).then(|| f.avatar.clone()),
                        avatar_url: f.avatar_url.clone(),
                        online: f.state != PresenceState::Offline,
                    },
                )
            })
            .collect();
        let on_projects = matches!(&view.activity, ActivityView::Playing { host: Some(host) } if is_projects(host));
        let signed_in = view.enabled && view.auth == AuthPhase::SignedIn;
        if !signed_in {
            // Signed out (or the feature went off): nothing of world sessions runs until the next
            // connection says so.
            let was = self.state().taken.take().is_some();
            if was {
                self.changed();
            }
        }
        self.on_friends(friends, on_projects);
    }

    /// [`Worlds::on_view`]'s part: the friends by id, and whether the player is on Projects.
    pub(crate) fn on_friends(&self, friends: HashMap<String, Person>, on_projects: bool) {
        let online: std::collections::HashSet<String> = friends
            .iter()
            .filter(|(_, person)| person.online)
            .map(|(id, _)| id.clone())
            .collect();
        let (gone, meet) = {
            let mut st = self.state();
            let names_changed = st.friends != friends;
            st.friends = friends;
            st.on_projects = on_projects;
            // An invite goes with its host's going offline or out of the friend list, unless the
            // player is in (or on the way to) that world.
            let mut gone = Vec::new();
            let mut kept = Vec::new();
            for invite in std::mem::take(&mut st.invites) {
                let host_here = online.contains(&invite.from);
                if host_here || invite.live.is_some() || invite.task.is_some() {
                    kept.push(invite);
                } else {
                    gone.push(invite);
                }
            }
            st.invites = kept;
            let meet = match (&st.meet, on_projects) {
                (Some(meet), true) if Instant::now() < meet.until => st.meet.take(),
                _ => None,
            };
            if !names_changed && gone.is_empty() {
                drop(st);
                if let Some(meet) = meet {
                    self.send_meet(meet.friends);
                }
                return;
            }
            (gone, meet)
        };
        for invite in gone {
            if let Some(task) = invite.task {
                task.abort();
            }
        }
        if let Some(meet) = meet {
            self.send_meet(meet.friends);
        }
        self.changed();
    }

    /// A game with this install started; `token` is its overlay bridge's token (none without the
    /// overlay).
    pub fn game_started(&self, instance_id: &str, token: Option<String>) {
        {
            let mut st = self.state();
            st.game = Game {
                instance_id: Some(instance_id.to_owned()),
                token: token.map(Secret::new),
                ..Game::default()
            };
        }
        let worlds = self.clone();
        let instance_id = instance_id.to_owned();
        tokio::spawn(async move {
            let hostable = worlds
                .inner
                .launcher
                .host_install(&instance_id)
                .await
                .is_some_and(|install| install.hostable);
            {
                let mut st = worlds.state();
                if st.game.instance_id.as_deref() == Some(instance_id.as_str()) {
                    st.game.hostable = hostable;
                }
            }
            worlds.changed();
        });
    }

    /// The game ended: the player's world closes with it; a "Katıl" that waited for this game
    /// stays ready (its session goes by itself after two minutes).
    pub fn game_exited(&self) {
        {
            let mut st = self.state();
            st.game = Game::default();
        }
        self.close_hosting(None);
        self.changed();
    }

    /// The overlay's mod said something (`pl_overlay::Overlay::on_host`).
    pub fn on_bridge(&self, signal: HostSignal) {
        match signal {
            HostSignal::Connected { hosts } => {
                {
                    let mut st = self.state();
                    st.game.connected = true;
                    st.game.hosts = hosts;
                    if let Some(hosting) = st.hosting.as_mut() {
                        // A new connection gets the whole mirror again.
                        hosting.mirrored = None;
                    }
                }
                self.changed();
            }
            HostSignal::Gone => {
                {
                    let mut st = self.state();
                    st.game.connected = false;
                    st.game.hosts = false;
                    st.game.world = None;
                }
                // Without the mod the world can't be looked after: sharing ends.
                self.close_hosting(None);
                self.changed();
            }
            HostSignal::Frame(frame) => self.on_mod_frame(frame),
            HostSignal::Join { friend } => {
                let session = self
                    .state()
                    .invites
                    .iter()
                    .find(|i| i.from == friend)
                    .map(|i| i.session.clone());
                if let Some(session) = session {
                    self.start_join(&session, None, true);
                }
            }
        }
    }

    /// World session frames of the presence socket (`pl_social::SocialService::on_world`).
    pub fn on_service(&self, event: WorldEvent) {
        match event {
            WorldEvent::Hello { taken, status } => {
                let (again, held) = {
                    let mut st = self.state();
                    st.taken = taken.then_some(status);
                    let again = st.hosting.as_ref().and_then(|h| {
                        let live = h.live.as_ref()?;
                        Some(WorldFrame::World {
                            open: true,
                            session: live.session_id().to_string(),
                            guests: Some(
                                live.guests()
                                    .iter()
                                    .filter(|g| g.status == GuestStatus::InWorld)
                                    .count() as u8,
                            ),
                            max: Some(h.max),
                            paths: serde_json::to_value(live.paths()).ok(),
                            invited: Some(live.invited()),
                        })
                    });
                    (again, taken)
                };
                if let Some(frame) = again {
                    self.inner.service.send(frame);
                }
                if held {
                    // The sweeper waits two reads for these (README "TURN budget").
                    for frame in self.inner.creds.held_frames() {
                        self.inner.service.send(frame);
                    }
                }
                self.set_online(taken);
                self.changed();
            }
            WorldEvent::Status(status) => {
                {
                    let mut st = self.state();
                    if st.taken.is_some() {
                        st.taken = Some(status);
                    }
                }
                self.changed();
            }
            WorldEvent::Down => {
                self.set_online(false);
            }
            WorldEvent::Invite { invite, quiet } => self.on_invite(invite, quiet),
            WorldEvent::Update {
                session,
                guests,
                max,
                paths,
                ..
            } => {
                let paths: Option<Paths> =
                    paths.and_then(|paths| serde_json::from_value(paths.into_inner()).ok());
                {
                    let mut st = self.state();
                    let Some(invite) = st.invite(&session) else {
                        return;
                    };
                    invite.guests = guests;
                    invite.max = max.clamp(1, 7);
                    if let Some(live) = &invite.live {
                        live.world_update(guests, max, paths);
                    }
                }
                self.changed();
            }
            WorldEvent::Closed { session, .. } => self.invite_gone(&session, GuestEnd::HostClosed),
            WorldEvent::Revoked { session, .. } => self.invite_gone(&session, GuestEnd::Removed),
            WorldEvent::RevokedGuest { session, to } => {
                let live = {
                    let mut st = self.state();
                    let Some(hosting) = st.hosting.as_mut() else {
                        return;
                    };
                    if hosting.session.map(|s| s.to_string()).as_deref() != Some(session.as_str()) {
                        return;
                    }
                    hosting.removing.insert(to.clone(), "removed");
                    hosting.live.clone()
                };
                if let Some(live) = live {
                    tokio::spawn(async move { live.remove(&to, false).await });
                }
            }
            WorldEvent::Peer {
                session,
                from,
                slot,
                path,
                relay,
            } => {
                let live = {
                    let st = self.state();
                    st.hosting.as_ref().and_then(|h| {
                        let live = h.live.as_ref()?;
                        (live.session_id().to_string() == session).then(|| live.clone())
                    })
                };
                let (Some(live), Some(slot_id)) = (live, pl_host::tunnel::Slot::parse(&slot))
                else {
                    return;
                };
                let kind = match path.as_str() {
                    "turn" => pl_host::transport::PathKind::Turn,
                    "e4mc" => pl_host::transport::PathKind::E4mc,
                    _ => return,
                };
                let relay = relay.and_then(|r| pl_host::tunnel::invite::parse_relay(r.expose()));
                let service = Arc::clone(&self.inner.service);
                tokio::spawn(async move {
                    if let Some(paths) = live.world_peer(&slot_id, kind, relay).await
                        && let Ok(paths) = serde_json::to_value(&paths)
                    {
                        service.send(WorldFrame::WorldPeerOk {
                            session,
                            to: from,
                            slot,
                            paths: Hidden(paths),
                        });
                    }
                });
            }
            WorldEvent::PeerOk {
                session,
                slot,
                paths,
                ..
            } => {
                if let Ok(paths) = serde_json::from_value::<Paths>(paths.into_inner()) {
                    self.inner.peers.deliver(&session, &slot, paths);
                }
            }
            WorldEvent::TurnMove {
                session, reason, ..
            } => {
                let (host, guest) = {
                    let st = self.state();
                    let host = st
                        .hosting
                        .as_ref()
                        .and_then(|h| h.live.clone())
                        .filter(|live| live.session_id().to_string() == session);
                    let guest = st
                        .invites
                        .iter()
                        .find(|i| i.session == session)
                        .and_then(|i| i.live.clone());
                    (host, guest)
                };
                if let Some(live) = guest {
                    live.turn_move(&reason);
                }
                if let Some(live) = host {
                    tokio::spawn(async move { live.turn_move(&reason).await });
                }
            }
        }
    }

    // ── Telemetry ────────────────────────────────────────────────────────────────────────────

    fn set_online(&self, online: bool) {
        let frames = lock(&self.inner.telemetry.reporter).set_online(online, Instant::now());
        self.send_reports(frames);
    }

    pub(crate) fn report(&self, mc: &str, event: Event) {
        let frames = lock(&self.inner.telemetry.reporter).push(mc, event, Instant::now());
        self.send_reports(frames);
        self.inner.telemetry.wake.notify_one();
        if !self.inner.telemetry.running.swap(true, Ordering::SeqCst)
            && let Ok(runtime) = tokio::runtime::Handle::try_current()
        {
            let worlds = self.clone();
            runtime.spawn(async move { worlds.telemetry_task().await });
        }
    }

    fn send_reports(&self, frames: Vec<String>) {
        for frame in frames {
            self.inner.service.send(WorldFrame::Report(frame));
        }
    }

    /// Sends what waits once it is due; with nothing waiting it sleeps on a notification only.
    async fn telemetry_task(self) {
        let mut last = None;
        loop {
            let next = lock(&self.inner.telemetry.reporter).next_deadline();
            // A deadline that did not move after its poll would spin: it waits a second then.
            let at = match next {
                Some(at) if last == Some(at) && at <= Instant::now() => at + Duration::from_secs(1),
                Some(at) => at,
                None => {
                    self.inner.telemetry.wake.notified().await;
                    continue;
                }
            };
            tokio::select! {
                () = tokio::time::sleep_until(tokio::time::Instant::from_std(at)) => {
                    let frames = lock(&self.inner.telemetry.reporter).poll(Instant::now());
                    self.send_reports(frames);
                    last = next;
                }
                () = self.inner.telemetry.wake.notified() => {}
            }
        }
    }

    // ── The player's own world ───────────────────────────────────────────────────────────────

    /// Whether the player's world is open now (the tray's "Çık" asks first; the launcher stays in
    /// the tray).
    pub fn hosting(&self) -> bool {
        self.state()
            .hosting
            .as_ref()
            .is_some_and(|h| h.live.is_some() || matches!(h.phase, Phase::Opening(..)))
    }

    /// Whether a world session needs this launcher running: the player's world is open (or
    /// opening), or the player is joined to a friend's world (its connection goes through here).
    /// "Oyun açılınca launcher: Kapansın" then keeps the launcher in the tray.
    pub fn keeps_launcher(&self) -> bool {
        self.hosting() || self.state().invites.iter().any(|i| i.live.is_some())
    }

    /// Whether an instance's worlds offer "Arkadaşlarınla oyna".
    pub async fn place(&self, instance_id: &str) -> HostPlace {
        if !self.state().switch_on() {
            return HostPlace::default();
        }
        let shown = self
            .inner
            .launcher
            .host_install(instance_id)
            .await
            .is_some_and(|install| install.hostable);
        let st = self.state();
        let game = &st.game;
        let playing = (shown
            && game.instance_id.as_deref() == Some(instance_id)
            && game.connected
            && game.hosts)
            .then(|| {
                game.world
                    .as_ref()
                    .filter(|w| !w.lan)
                    .map(|w| w.folder.clone())
            })
            .flatten();
        HostPlace { shown, playing }
    }

    /// "Dünyayı aç" from the launcher (see [`WorldOpenOutcome`]). Refusals come as `openFailed`.
    pub async fn open(&self, request: WorldOpenRequest) -> WorldOpenOutcome {
        let switch_on = self.state().switch_on();
        let hostable = self
            .inner
            .launcher
            .host_install(&request.instance_id)
            .await
            .is_some_and(|install| install.hostable);
        if !(hostable && switch_on) {
            self.notice(HostNotice::OpenFailed {
                reason: WorldOpenFailure::Unavailable,
            });
            return WorldOpenOutcome::Cancelled;
        }
        let world_name = self
            .inner
            .launcher
            .world_name(&request.instance_id, &request.folder)
            .await;
        let (cancelled, generation) = {
            let mut st = self.state();
            if st.hosting.as_ref().is_some_and(|h| h.live.is_some()) {
                drop(st);
                // One world at a time (the page never asks for a second).
                return WorldOpenOutcome::Cancelled;
            }
            let generation = st.next();
            let max = st.max_guests();
            let (cancel, cancelled) = oneshot::channel();
            let guests = request
                .invite
                .iter()
                .take(usize::from(max))
                .map(|friend| Guest {
                    friend: friend.clone(),
                    state: WorldGuestState::Pending,
                    game_name: None,
                    guest: None,
                })
                .collect();
            st.hosting = Some(Hosting {
                generation,
                instance_id: request.instance_id.clone(),
                folder: request.folder.clone(),
                world_name,
                mode: request.mode,
                commands: request.commands,
                max,
                guests,
                phase: Phase::Opening(WorldOpeningStep::Backup, None),
                from_launcher: true,
                cancel: Some(cancel),
                session: None,
                publish: None,
                live: None,
                removing: HashMap::new(),
                mirrored: None,
            });
            (cancelled, generation)
        };
        self.changed();

        // A backup first when the last one is older than a day ("Arkadaşlarına açılmadan önce").
        let progress = self.opening_progress(generation);
        let backup =
            self.inner
                .launcher
                .backup_if_old(&request.instance_id, &request.folder, progress);
        tokio::select! {
            ok = backup => {
                if !ok {
                    tracing::warn!("the world's backup before sharing failed; it opens anyway");
                }
            }
            _ = cancelled => {
                self.drop_opening(generation);
                return WorldOpenOutcome::Cancelled;
            }
        }
        let running = {
            let mut st = self.state();
            let in_place = st.game.instance_id.as_deref() == Some(request.instance_id.as_str())
                && st.game.connected
                && st.game.hosts
                && st
                    .game
                    .world
                    .as_ref()
                    .is_some_and(|w| w.folder == request.folder && !w.lan);
            let Some(hosting) = st.hosting.as_mut().filter(|h| h.generation == generation) else {
                return WorldOpenOutcome::Cancelled;
            };
            hosting.cancel = None;
            hosting.phase = Phase::Opening(
                if in_place {
                    WorldOpeningStep::Publishing
                } else {
                    WorldOpeningStep::Starting
                },
                None,
            );
            in_place
        };
        self.changed();
        if running {
            self.spawn_publish(generation);
            WorldOpenOutcome::Running
        } else {
            WorldOpenOutcome::Start
        }
    }

    fn opening_progress(&self, generation: u64) -> Progress {
        let worlds = self.clone();
        let last = Arc::new(Mutex::new(None::<Instant>));
        Arc::new(move |percent: u8| {
            let percent = percent.min(100);
            {
                let mut last = lock(&last);
                if percent < 100 && last.is_some_and(|at| at.elapsed() < PROGRESS_EVERY) {
                    return;
                }
                *last = Some(Instant::now());
            }
            {
                let mut st = worlds.state();
                let Some(hosting) = st.hosting.as_mut().filter(|h| h.generation == generation)
                else {
                    return;
                };
                if let Phase::Opening(WorldOpeningStep::Backup, current) = &mut hosting.phase {
                    if *current == Some(percent) {
                        return;
                    }
                    *current = Some(percent);
                } else {
                    return;
                }
            }
            worlds.changed();
        })
    }

    fn drop_opening(&self, generation: u64) {
        let dropped = {
            let mut st = self.state();
            if st
                .hosting
                .as_ref()
                .is_some_and(|h| h.generation == generation && h.live.is_none())
            {
                st.hosting = None;
                true
            } else {
                false
            }
        };
        if dropped {
            self.changed();
        }
    }

    /// "Vazgeç" during the backup, or the game did not start (or left before its world loaded).
    pub fn cancel_open(&self) {
        let (cancel, live) = {
            let mut st = self.state();
            match st.hosting.as_ref().map(|h| h.phase) {
                Some(Phase::Opening(..)) => {
                    let mut hosting = st.hosting.take().expect("an opening");
                    (hosting.cancel.take(), hosting.live.take())
                }
                _ => return,
            }
        };
        if let Some(cancel) = cancel {
            let _ = cancel.send(());
        }
        if let Some(live) = live {
            tokio::spawn(async move { live.close().await });
        }
        self.changed();
    }

    /// "Kapat" on a failed opening.
    pub fn dismiss_failure(&self) {
        let dismissed = {
            let mut st = self.state();
            if matches!(st.hosting.as_ref().map(|h| h.phase), Some(Phase::Failed(_))) {
                st.hosting = None;
                true
            } else {
                false
            }
        };
        if dismissed {
            self.changed();
        }
    }

    /// "Davet et" while the world is open (the page's window). A refusal is an answer.
    pub async fn invite(&self, friends: Vec<String>) -> WorldInviteOutcome {
        let Some(generation) = self.add_guests(&friends) else {
            return WorldInviteOutcome::Session;
        };
        self.changed();
        match self.send_invites(generation, friends).await {
            None => WorldInviteOutcome::Sent,
            Some(refusal) => refusal.into(),
        }
    }

    /// Adds friends not in the list yet (as pending), up to the guest limit counted with those
    /// already invited. The open session's generation, or None without one.
    fn add_guests(&self, friends: &[String]) -> Option<u64> {
        let mut st = self.state();
        let hosting = st.hosting.as_mut()?;
        hosting.live.as_ref()?;
        for friend in friends {
            if hosting.guests.len() >= usize::from(hosting.max) {
                break;
            }
            if !hosting.guests.iter().any(|g| &g.friend == friend) {
                hosting.guests.push(Guest {
                    friend: friend.clone(),
                    state: WorldGuestState::Pending,
                    game_name: None,
                    guest: None,
                });
            }
        }
        Some(hosting.generation)
    }

    /// Sends the invites of these friends once the session is open; the first refusal stops
    /// them (the friends it did not reach leave the list).
    async fn send_invites(
        &self,
        generation: u64,
        friends: Vec<String>,
    ) -> Option<WorldInviteRefusal> {
        let mut refusal = None;
        for friend in friends {
            let (live, session) = {
                let st = self.state();
                let Some(hosting) = st.hosting.as_ref().filter(|h| h.generation == generation)
                else {
                    return refusal;
                };
                if !hosting
                    .guests
                    .iter()
                    .any(|g| g.friend == friend && g.state == WorldGuestState::Pending)
                {
                    continue;
                }
                let (Some(live), Some(session)) = (hosting.live.clone(), hosting.session) else {
                    return refusal;
                };
                (live, session)
            };
            let outcome = if refusal.is_some() {
                Err(None)
            } else {
                match live.invite(&friend).await {
                    Ok(join) => {
                        let frame = WorldFrame::WorldInvite {
                            session: session.to_string(),
                            to: friend.clone(),
                            join: Hidden(join.to_value()),
                        };
                        match self.inner.service.request(frame, REQUEST_WAIT).await {
                            Ok(_) => Ok(()),
                            Err(error) => Err(invite_refusal(&error)),
                        }
                    }
                    Err(InviteError::TooMany) => Err(Some(WorldInviteRefusal::Session)),
                    Err(_) => Err(None),
                }
            };
            {
                let mut st = self.state();
                let Some(hosting) = st.hosting.as_mut().filter(|h| h.generation == generation)
                else {
                    return refusal;
                };
                match outcome {
                    Ok(()) => {
                        if let Some(guest) = hosting.guests.iter_mut().find(|g| g.friend == friend)
                            && guest.state == WorldGuestState::Pending
                        {
                            guest.state = WorldGuestState::Invited;
                        }
                    }
                    Err(why) => {
                        hosting.guests.retain(|g| g.friend != friend);
                        if refusal.is_none() {
                            refusal = why;
                        }
                    }
                }
            }
            self.changed();
        }
        refusal
    }

    /// "Dünyadan çıkar".
    pub async fn remove(&self, friend: &str) {
        let live = {
            let mut st = self.state();
            let Some(hosting) = st.hosting.as_mut() else {
                return;
            };
            hosting.removing.insert(friend.to_owned(), "removed");
            hosting.live.clone()
        };
        if let Some(live) = live {
            live.remove(friend, true).await;
        }
    }

    /// "Paylaşımı kapat".
    pub async fn stop(&self) {
        self.close_hosting(Some("player"));
        self.changed();
    }

    /// The launcher quits: the mod hears it, every session ends, every join is left.
    pub async fn shutdown(&self) {
        let (live, joins) = {
            let mut st = self.state();
            let live = st.hosting.take().and_then(|h| h.live);
            let joins: Vec<GuestSession> = st
                .invites
                .iter_mut()
                .filter_map(|i| i.live.take())
                .collect();
            (live, joins)
        };
        if let Some(live) = live {
            self.inner.bridge.send(vec![ToGame::HostStop {
                reason: Some("launcher-exit".into()),
            }]);
            live.close().await;
        }
        for join in joins {
            join.leave().await;
        }
    }

    /// Ends the player's session; `tell_game`: the mod hears `host-stop{reason}` (the launcher
    /// closed it), otherwise the game itself ended it. A failed opening stays for its "Kapat".
    fn close_hosting(&self, tell_game: Option<&str>) {
        let (live, cancel) = {
            let mut st = self.state();
            let failed = matches!(
                st.hosting.as_ref().map(|h| h.phase),
                Some(Phase::Failed(_)) | None
            );
            if failed {
                return;
            }
            let mut hosting = st.hosting.take().expect("a session");
            (hosting.live.take(), hosting.cancel.take())
        };
        if let Some(cancel) = cancel {
            let _ = cancel.send(());
        }
        if let Some(reason) = tell_game {
            self.inner.bridge.send(vec![ToGame::HostStop {
                reason: Some(reason.to_owned()),
            }]);
        }
        if let Some(live) = live {
            tokio::spawn(async move { live.close().await });
        }
    }

    /// "Projects'te buluşun": the friends get a "Birlikte oyna" invite once the player is on
    /// Projects (the page takes them there with "Bağlan"), within three minutes.
    pub async fn meet_on_projects(&self, friends: Vec<String>) {
        let now_on_projects = {
            let mut st = self.state();
            if matches!(st.hosting.as_ref().map(|h| h.phase), Some(Phase::Failed(_))) {
                st.hosting = None;
            }
            if st.on_projects {
                true
            } else {
                st.meet = Some(Meet {
                    friends: friends.clone(),
                    until: Instant::now() + MEET_WAIT,
                });
                false
            }
        };
        if now_on_projects {
            self.send_meet(friends);
        }
        self.changed();
    }

    fn send_meet(&self, friends: Vec<String>) {
        let together = Arc::clone(&self.inner.together);
        tokio::spawn(async move {
            for friend in friends {
                together.invite(&friend).await;
            }
        });
    }

    fn spawn_publish(&self, generation: u64) {
        let worlds = self.clone();
        tokio::spawn(async move { worlds.publish(generation).await });
    }

    /// The world opens (PROTOCOL.md §3.1 steps 2–6): the service's `world{open:true}`, the mod's
    /// `host-publish`, the session and its first path, then the invites.
    async fn publish(&self, generation: u64) {
        let prepared = {
            let st = self.state();
            let Some(hosting) = st.hosting.as_ref().filter(|h| h.generation == generation) else {
                return;
            };
            let world_ok = st.game.connected
                && st.game.hosts
                && st.game.instance_id.as_deref() == Some(hosting.instance_id.as_str())
                && st
                    .game
                    .world
                    .as_ref()
                    .is_some_and(|w| w.folder == hosting.folder && !w.lan);
            (world_ok && st.switch_on())
                .then(|| st.game.token.clone())
                .flatten()
                .map(|token| {
                    (
                        token,
                        hosting.instance_id.clone(),
                        hosting.folder.clone(),
                        hosting.mode,
                        hosting.commands,
                        hosting.max.min(st.max_guests()),
                    )
                })
        };
        let Some((token, instance_id, folder, mode, commands, max)) = prepared else {
            self.fail_opening(generation, WorldOpenFailure::Unavailable, None);
            return;
        };
        let Some(install) = self
            .inner
            .launcher
            .host_install(&instance_id)
            .await
            .filter(|i| i.hostable)
        else {
            self.fail_opening(generation, WorldOpenFailure::Unavailable, None);
            return;
        };
        let session = SessionId::generate();
        let key = HostKey::generate();
        // 1. The service holds the session (and lowers the limit to Studio's).
        let opened = self
            .inner
            .service
            .request(
                WorldFrame::World {
                    open: true,
                    session: session.to_string(),
                    guests: Some(0),
                    max: Some(max),
                    paths: None,
                    invited: None,
                },
                REQUEST_WAIT,
            )
            .await;
        let max = match opened {
            Ok(answer) => answer
                .world
                .and_then(|world| world.max)
                .unwrap_or(max)
                .clamp(1, 7),
            Err(error) => {
                let reason = if error.limits("world_opens") {
                    WorldOpenFailure::Limit
                } else {
                    WorldOpenFailure::Unavailable
                };
                self.fail_opening(generation, reason, None);
                return;
            }
        };
        let close_world = |worlds: &Worlds| {
            worlds.inner.service.send(WorldFrame::World {
                open: false,
                session: session.to_string(),
                guests: None,
                max: None,
                paths: None,
                invited: None,
            });
        };
        // 2. The mod opens the world on loopback.
        let binding = {
            let dir = self.inner.config.data_dir.clone();
            tokio::task::spawn_blocking(move || BindingKey::load_or_create(&dir))
                .await
                .ok()
                .and_then(Result::ok)
        };
        let Some(binding) = binding else {
            close_world(self);
            self.fail_opening(generation, WorldOpenFailure::Unavailable, None);
            return;
        };
        let publish_id = StreamId::generate().to_string();
        let (published_tx, published) = oneshot::channel();
        {
            let mut st = self.state();
            let Some(hosting) = st.hosting.as_mut().filter(|h| h.generation == generation) else {
                drop(st);
                close_world(self);
                return;
            };
            hosting.session = Some(session);
            hosting.max = max;
            hosting.phase = Phase::Opening(WorldOpeningStep::Publishing, None);
            hosting.publish = Some(PublishWait {
                id: publish_id.clone(),
                answer: published_tx,
            });
        }
        self.changed();
        self.inner.bridge.send(vec![ToGame::HostPublish {
            id: publish_id.clone(),
            mode: mode_wire(mode),
            commands,
            max,
            binding: binding.key_id(),
        }]);
        let (port, proto) = match tokio::time::timeout(PUBLISH_WAIT, published).await {
            Ok(Ok(Ok(endpoint))) => endpoint,
            _ => {
                close_world(self);
                self.fail_opening(generation, WorldOpenFailure::Unavailable, None);
                return;
            }
        };
        // 3. The session, its first path, then the invites.
        let Some(manifest) = self.inner.launcher.manifest(&instance_id).await else {
            close_world(self);
            self.fail_opening(generation, WorldOpenFailure::Unavailable, Some("error"));
            return;
        };
        let host_name = self
            .inner
            .launcher
            .account()
            .map(|account| account.name)
            .unwrap_or_default();
        let worlds = self.clone();
        let mc = install.mc.clone();
        let link = Arc::new(HostLink::new(
            Arc::clone(&self.inner.service),
            Arc::clone(&self.inner.creds),
            session.to_string(),
            self.inner.config.transports.clone(),
            move |event| worlds.on_host_event(generation, &mc, event),
        ));
        let config = HostConfig {
            session,
            key,
            max_guests: max,
            host_name,
            mc: install.mc.clone(),
            pack: install.family.clone(),
            manifest,
            game: GameEndpoint {
                port,
                proto,
                publish_id,
                gate: GateKey::from_bridge_token(token.expose()),
            },
            world_dir: install.game_dir.join("saves").join(&folder),
            binding,
            policy: self.inner.config.policy.clone(),
        };
        let live = match HostSession::open(config, link).await {
            Ok(live) => live,
            // The session already sent `world{open:false}` and the mod's `host-stop`.
            Err(_) => {
                self.fail_opening(generation, WorldOpenFailure::Unavailable, None);
                return;
            }
        };
        let opened = {
            let mut st = self.state();
            st.hosting
                .as_mut()
                .filter(|h| h.generation == generation)
                .map(|hosting| {
                    hosting.live = Some(live.clone());
                    hosting.phase = Phase::Open;
                    let pending: Vec<String> = hosting
                        .guests
                        .iter()
                        .filter(|g| g.state == WorldGuestState::Pending)
                        .map(|g| g.friend.clone())
                        .collect();
                    (pending, hosting.world_name.clone())
                })
        };
        let Some((pending, world_name)) = opened else {
            // Left meanwhile.
            live.close().await;
            return;
        };
        tracing::info!("the player's world is open to friends");
        self.changed();
        self.notice(HostNotice::Opened { world_name });
        if let Some(why) = self.send_invites(generation, pending).await {
            self.notice(HostNotice::InviteRefused { why });
        }
    }

    /// The opening did not work: the page keeps it as failed, the mod hears why (H8), and with
    /// `stop` the mod closes what it opened.
    fn fail_opening(&self, generation: u64, reason: WorldOpenFailure, stop: Option<&str>) {
        let max = {
            let mut st = self.state();
            let Some(hosting) = st.hosting.as_mut().filter(|h| h.generation == generation) else {
                return;
            };
            hosting.phase = Phase::Failed(reason);
            hosting.publish = None;
            hosting.live = None;
            for guest in &mut hosting.guests {
                guest.state = WorldGuestState::Pending;
            }
            hosting.max
        };
        let mut frames = Vec::new();
        if let Some(reason) = stop {
            frames.push(ToGame::HostStop {
                reason: Some(reason.to_owned()),
            });
        }
        frames.push(ToGame::HostState {
            open: false,
            max,
            guests: Vec::new(),
            error: Some(reason.as_str().to_owned()),
        });
        self.inner.bridge.send(frames);
        tracing::info!(reason = reason.as_str(), "the player's world did not open");
        self.changed();
        self.notice(HostNotice::OpenFailed { reason });
    }

    /// What the player's session asks for (`pl_host::host::HostEvent`).
    fn on_host_event(&self, generation: u64, mc: &str, event: HostEvent) {
        let session = {
            let st = self.state();
            st.hosting
                .as_ref()
                .filter(|h| h.generation == generation)
                .and_then(|h| h.session)
                .map(|s| s.to_string())
        };
        match event {
            HostEvent::Report(event) => self.report(mc, event),
            HostEvent::Announce { guests, max, paths } => {
                if let Some(session) = session {
                    self.inner.service.send(WorldFrame::World {
                        open: true,
                        session,
                        guests: Some(guests),
                        max: Some(max),
                        paths: serde_json::to_value(&paths).ok(),
                        invited: None,
                    });
                }
            }
            HostEvent::Closed => {
                if let Some(session) = session {
                    self.inner.service.send(WorldFrame::World {
                        open: false,
                        session,
                        guests: None,
                        max: None,
                        paths: None,
                        invited: None,
                    });
                }
            }
            HostEvent::StopGame => {
                self.inner.bridge.send(vec![ToGame::HostStop {
                    reason: Some("error".into()),
                }]);
                let was_open = {
                    let st = self.state();
                    st.hosting
                        .as_ref()
                        .is_some_and(|h| h.generation == generation && h.phase == Phase::Open)
                };
                if was_open {
                    // Every way to the world is gone: it shows as "Açılamadı".
                    self.fail_opening(generation, WorldOpenFailure::Unavailable, None);
                }
            }
            HostEvent::RemoveFromGame { guest } => {
                let reason = {
                    let mut st = self.state();
                    st.hosting
                        .as_mut()
                        .filter(|h| h.generation == generation)
                        .and_then(|h| {
                            let friend = h
                                .guests
                                .iter()
                                .find(|g| g.guest == Some(guest))
                                .map(|g| g.friend.clone())?;
                            h.removing.remove(&friend)
                        })
                        .unwrap_or("removed")
                };
                self.inner.bridge.send(vec![ToGame::HostRemove {
                    guest: guest.to_string(),
                    reason: reason.to_owned(),
                }]);
            }
            HostEvent::Revoke { to } => {
                if let Some(session) = session {
                    self.inner
                        .service
                        .send(WorldFrame::WorldRevoke { session, to });
                }
            }
            HostEvent::Guest(update) => {
                self.guest_update(generation, update);
                self.changed();
            }
        }
    }

    fn guest_update(&self, generation: u64, update: GuestUpdate) {
        let mut st = self.state();
        let Some(hosting) = st.hosting.as_mut().filter(|h| h.generation == generation) else {
            return;
        };
        if update.status == GuestStatus::Removed {
            hosting.guests.retain(|g| g.friend != update.friend);
            return;
        }
        let guest = match hosting
            .guests
            .iter_mut()
            .position(|g| g.friend == update.friend)
        {
            Some(index) => &mut hosting.guests[index],
            None => {
                hosting.guests.push(Guest {
                    friend: update.friend.clone(),
                    state: WorldGuestState::Invited,
                    game_name: None,
                    guest: None,
                });
                hosting.guests.last_mut().expect("just pushed")
            }
        };
        guest.guest = Some(update.guest);
        if let Some(name) = update.name {
            guest.game_name = Some(name);
        }
        guest.state = match update.status {
            GuestStatus::Invited | GuestStatus::OutOfWorld | GuestStatus::Left => {
                WorldGuestState::Invited
            }
            GuestStatus::Connected(_) => WorldGuestState::Joining,
            // In once the mod says the login finished (`host-guest{in}`).
            GuestStatus::InWorld if guest.state == WorldGuestState::In => WorldGuestState::In,
            GuestStatus::InWorld => WorldGuestState::Joining,
            GuestStatus::Removed => unreachable!("handled above"),
        };
    }

    /// The mod's `host-*` frames.
    fn on_mod_frame(&self, frame: FromGame) {
        match frame {
            FromGame::HostWorld {
                open, folder, lan, ..
            } => {
                enum Next {
                    Publish(u64),
                    /// Vanilla "LAN'a Aç" is on in that world: it can't be shared.
                    Lan(u64),
                    /// Another world than the one being opened.
                    Other,
                    Nothing,
                }
                let next = {
                    let mut st = self.state();
                    st.game.world = open.then(|| OpenWorld {
                        folder: folder.clone(),
                        lan,
                    });
                    let instance = st.game.instance_id.clone();
                    match st.hosting.as_mut() {
                        Some(hosting)
                            if open
                                && hosting.phase
                                    == Phase::Opening(WorldOpeningStep::Starting, None)
                                && hosting.from_launcher =>
                        {
                            let ours = hosting.folder == folder
                                && instance.as_deref() == Some(hosting.instance_id.as_str());
                            match (ours, lan) {
                                (true, false) => {
                                    hosting.phase =
                                        Phase::Opening(WorldOpeningStep::Publishing, None);
                                    Next::Publish(hosting.generation)
                                }
                                (true, true) => Next::Lan(hosting.generation),
                                (false, _) => Next::Other,
                            }
                        }
                        _ => Next::Nothing,
                    }
                };
                if !open {
                    // Leaving the world ends sharing.
                    self.close_hosting(None);
                }
                match next {
                    Next::Publish(generation) => self.spawn_publish(generation),
                    Next::Lan(generation) => {
                        self.fail_opening(generation, WorldOpenFailure::Unavailable, None);
                    }
                    Next::Other => self.cancel_open(),
                    Next::Nothing => {}
                }
                self.changed();
            }
            FromGame::HostRequest {
                mode,
                commands,
                invite,
            } => self.on_game_request(mode, commands, invite),
            FromGame::HostPublished {
                id,
                ok,
                port,
                proto,
                reason,
            } => {
                let waiter = {
                    let mut st = self.state();
                    st.hosting.as_mut().and_then(|h| {
                        let ours = h.publish.as_ref().is_some_and(|wait| wait.id == id);
                        if ours { h.publish.take() } else { None }
                    })
                };
                match waiter {
                    Some(wait) => {
                        let _ = wait.answer.send(if ok && port != 0 {
                            Ok((port, proto))
                        } else {
                            Err(reason)
                        });
                    }
                    // Too late: whatever it opened closes again.
                    None if ok => self.inner.bridge.send(vec![ToGame::HostStop {
                        reason: Some("error".into()),
                    }]),
                    None => {}
                }
            }
            FromGame::HostGuest {
                guest, name, state, ..
            } => {
                let Some(id) = GuestId::parse(&guest) else {
                    return;
                };
                {
                    let mut st = self.state();
                    let Some(hosting) = st.hosting.as_mut() else {
                        return;
                    };
                    let Some(entry) = hosting.guests.iter_mut().find(|g| g.guest == Some(id))
                    else {
                        return;
                    };
                    match state.as_str() {
                        "in" => {
                            entry.state = WorldGuestState::In;
                            if !name.is_empty() {
                                entry.game_name = Some(name);
                            }
                        }
                        "out" => entry.state = WorldGuestState::Invited,
                        _ => return,
                    }
                }
                self.changed();
            }
            FromGame::HostInvite { invite } => {
                if let Some(generation) = self.add_guests(&invite) {
                    self.changed();
                    let worlds = self.clone();
                    tokio::spawn(async move {
                        if let Some(why) = worlds.send_invites(generation, invite).await {
                            worlds.notice(HostNotice::InviteRefused { why });
                        }
                    });
                }
            }
            FromGame::HostRemove { guest } => {
                let friend = {
                    let st = self.state();
                    let id = GuestId::parse(&guest);
                    st.hosting.as_ref().and_then(|h| {
                        h.guests
                            .iter()
                            .find(|g| id.is_some() && g.guest == id)
                            .map(|g| g.friend.clone())
                    })
                };
                if let Some(friend) = friend {
                    let worlds = self.clone();
                    tokio::spawn(async move { worlds.remove(&friend).await });
                }
            }
            FromGame::HostStop { .. } => {
                self.close_hosting(None);
                self.changed();
            }
            _ => {}
        }
    }

    /// "Davet et" in the game: the world opens (or more invites go, when it is open already).
    fn on_game_request(&self, mode: String, commands: bool, invite: Vec<String>) {
        enum Next {
            Open(u64),
            More(u64),
            Refuse(WorldOpenFailure),
        }
        let next = {
            let mut st = self.state();
            let world = st.game.world.clone();
            let instance = st.game.instance_id.clone();
            let ready = st.switch_on() && st.game.hostable && st.game.hosts;
            match (&st.hosting, world, instance) {
                (Some(h), _, _) if h.live.is_some() => Next::More(h.generation),
                (_, Some(world), Some(instance)) if ready && !world.lan => {
                    let generation = st.next();
                    let max = st.max_guests();
                    let guests = invite
                        .iter()
                        .take(usize::from(max))
                        .map(|friend| Guest {
                            friend: friend.clone(),
                            state: WorldGuestState::Pending,
                            game_name: None,
                            guest: None,
                        })
                        .collect();
                    st.hosting = Some(Hosting {
                        generation,
                        instance_id: instance,
                        folder: world.folder.clone(),
                        world_name: world.folder.clone(),
                        mode: WorldGameMode::from_wire(&mode),
                        commands,
                        max,
                        guests,
                        phase: Phase::Opening(WorldOpeningStep::Publishing, None),
                        from_launcher: false,
                        cancel: None,
                        session: None,
                        publish: None,
                        live: None,
                        removing: HashMap::new(),
                        mirrored: None,
                    });
                    Next::Open(generation)
                }
                _ => Next::Refuse(WorldOpenFailure::Unavailable),
            }
        };
        match next {
            Next::Open(generation) => {
                self.changed();
                let worlds = self.clone();
                tokio::spawn(async move {
                    let (instance, folder) = {
                        let st = worlds.state();
                        match st.hosting.as_ref() {
                            Some(h) => (h.instance_id.clone(), h.folder.clone()),
                            None => return,
                        }
                    };
                    let name = worlds.inner.launcher.world_name(&instance, &folder).await;
                    if let Some(h) = worlds
                        .state()
                        .hosting
                        .as_mut()
                        .filter(|h| h.generation == generation)
                    {
                        h.world_name = name;
                    }
                    worlds.publish(generation).await;
                });
            }
            Next::More(generation) => {
                if self.add_guests(&invite).is_some() {
                    self.changed();
                    let worlds = self.clone();
                    tokio::spawn(async move {
                        if let Some(why) = worlds.send_invites(generation, invite).await {
                            worlds.notice(HostNotice::InviteRefused { why });
                        }
                    });
                }
            }
            Next::Refuse(reason) => {
                self.inner.bridge.send(vec![ToGame::HostState {
                    open: false,
                    max: self.state().max_guests(),
                    guests: Vec::new(),
                    error: Some(reason.as_str().to_owned()),
                }]);
            }
        }
    }

    // ── Friends' worlds ──────────────────────────────────────────────────────────────────────

    fn on_invite(&self, invite: pl_social::WorldInvite, quiet: bool) {
        let Ok(join) = Join::parse(invite.join.expose()) else {
            tracing::info!("a world invite this launcher can't read was dropped");
            return;
        };
        let (view, toast) = {
            let mut st = self.state();
            if !st.allowed {
                return;
            }
            let max = st.max_guests();
            if let Some(known) = st.invite(&invite.session) {
                known.quiet = quiet;
                known.join = join;
            } else {
                let entry = Invite {
                    session: invite.session.clone(),
                    from: invite.from.clone(),
                    at: invite.at,
                    quiet,
                    join,
                    guests: 0,
                    max,
                    stage: None,
                    generation: 0,
                    ask: None,
                    live: None,
                    task: None,
                    in_game: false,
                    progress_at: None,
                };
                // One invite per friend: a newer world replaces an older one.
                st.invites
                    .retain(|i| i.from != invite.from || i.live.is_some());
                st.invites.insert(0, entry);
            }
            let view = st
                .invites
                .iter()
                .find(|i| i.session == invite.session)
                .map(|i| invite_view(&st, i));
            let toast = (!quiet && st.game.connected && st.game.hosts).then(|| {
                ToGame::Toast(ToastView {
                    kind: ToastKind::World,
                    key: format!("world:{}", invite.session),
                    title: st.person(&invite.from).name,
                    body: String::new(),
                    conv: None,
                    crew: None,
                    sticker: None,
                    badge: None,
                })
            });
            (view, toast)
        };
        self.changed();
        if let (Some(view), false) = (view, quiet) {
            self.notice(HostNotice::Invite { invite: view });
        }
        if let Some(toast) = toast {
            self.inner.bridge.send(vec![toast]);
        }
    }

    /// The host closed the world or took the invite back: it leaves the list, and a join that was
    /// under way says so.
    fn invite_gone(&self, session: &str, end: GuestEnd) {
        let removed = {
            let mut st = self.state();
            let Some(index) = st.invites.iter().position(|i| i.session == session) else {
                return;
            };
            let invite = st.invites.remove(index);
            let name = st.person(&invite.from).name;
            (invite, name)
        };
        let (invite, name) = removed;
        if let Some(task) = &invite.task {
            task.abort();
        }
        let joining = invite.stage.as_ref().is_some_and(|stage| {
            !matches!(stage, WorldJoinStage::Failed { .. } | WorldJoinStage::Lost)
        });
        if let Some(live) = invite.live {
            live.host_gone(end);
        }
        self.changed();
        if joining {
            self.notice(HostNotice::JoinFailed {
                session: session.to_owned(),
                name,
                failure: match end {
                    GuestEnd::Removed => WorldJoinFailure::Removed,
                    _ => WorldJoinFailure::Closed,
                },
            });
        }
    }

    /// "Katıl", "Tekrar dene", "Yeniden katıl"; `copy`: a copy of the host's install even where
    /// another would do ("Kurulumu kopyala"), named so.
    pub fn join(&self, session: &str, copy: Option<String>) {
        self.start_join(session, copy, false);
    }

    fn start_join(&self, session: &str, copy: Option<String>, in_game: bool) {
        enum Next {
            Again(GuestSession, u64),
            Fresh(u64),
        }
        let next = {
            let mut st = self.state();
            if !st.enabled() {
                return;
            }
            let generation = st.next();
            let Some(invite) = st.invite(session) else {
                return;
            };
            let busy = matches!(
                invite.stage,
                Some(
                    WorldJoinStage::Preparing { .. }
                        | WorldJoinStage::Ask { .. }
                        | WorldJoinStage::Connecting
                        | WorldJoinStage::In
                )
            );
            if busy {
                return;
            }
            invite.in_game = in_game;
            invite.generation = generation;
            invite.stage = Some(WorldJoinStage::Preparing { percent: None });
            match invite.live.clone() {
                Some(live) if copy.is_none() => Next::Again(live, generation),
                _ => Next::Fresh(generation),
            }
        };
        self.changed();
        let worlds = self.clone();
        let session = session.to_owned();
        let (generation, task) = match next {
            Next::Again(live, generation) => {
                let session = session.clone();
                (
                    generation,
                    tokio::spawn(async move {
                        worlds.rejoin(&session, generation, live).await;
                    }),
                )
            }
            Next::Fresh(generation) => {
                let session = session.clone();
                (
                    generation,
                    tokio::spawn(async move {
                        worlds.join_task(&session, generation, copy).await;
                    }),
                )
            }
        };
        let mut st = self.state();
        if let Some(invite) = st
            .invites
            .iter_mut()
            .find(|i| i.session == session && i.generation == generation)
            .filter(|i| {
                // Still under way (a fast end already cleared it).
                matches!(
                    i.stage,
                    Some(WorldJoinStage::Preparing { .. } | WorldJoinStage::Ask { .. })
                )
            })
        {
            // Recorded so a closed invite can stop it; the task ends by itself otherwise.
            invite.task = Some(task.abort_handle());
        }
    }

    fn current(&self, session: &str, generation: u64) -> bool {
        self.state()
            .invites
            .iter()
            .any(|i| i.session == session && i.generation == generation)
    }

    fn set_stage(&self, session: &str, generation: u64, stage: Option<WorldJoinStage>) {
        {
            let mut st = self.state();
            let Some(invite) = st
                .invites
                .iter_mut()
                .find(|i| i.session == session && i.generation == generation)
            else {
                return;
            };
            invite.stage = stage;
        }
        self.changed();
    }

    fn fail_join(&self, session: &str, generation: u64, failure: WorldJoinFailure) {
        let name = {
            let mut st = self.state();
            let Some(invite) = st
                .invites
                .iter_mut()
                .find(|i| i.session == session && i.generation == generation)
            else {
                return;
            };
            invite.stage = Some(WorldJoinStage::Failed { failure });
            invite.task = None;
            let from = invite.from.clone();
            st.person(&from).name
        };
        self.changed();
        self.notice(HostNotice::JoinFailed {
            session: session.to_owned(),
            name,
            failure,
        });
    }

    /// A window's question; None when it was left ("Vazgeç", another join, the invite gone).
    async fn ask(
        &self,
        session: &str,
        generation: u64,
        ask: WorldJoinAsk,
    ) -> Option<WorldJoinAnswer> {
        let (tx, rx) = oneshot::channel();
        {
            let mut st = self.state();
            let invite = st
                .invites
                .iter_mut()
                .find(|i| i.session == session && i.generation == generation)?;
            invite.ask = Some(tx);
            invite.stage = Some(WorldJoinStage::Ask { ask });
        }
        self.changed();
        rx.await.ok()
    }

    fn join_progress(&self, session: &str, generation: u64) -> Progress {
        let worlds = self.clone();
        let session = session.to_owned();
        Arc::new(move |percent: u8| {
            let percent = percent.min(100);
            {
                let mut st = worlds.state();
                let Some(invite) = st
                    .invites
                    .iter_mut()
                    .find(|i| i.session == session && i.generation == generation)
                else {
                    return;
                };
                if percent < 100
                    && invite
                        .progress_at
                        .is_some_and(|at| at.elapsed() < PROGRESS_EVERY)
                {
                    return;
                }
                let stage = Some(WorldJoinStage::Preparing {
                    percent: Some(percent),
                });
                if invite.stage == stage {
                    return;
                }
                invite.progress_at = Some(Instant::now());
                invite.stage = stage;
            }
            worlds.changed();
        })
    }

    /// The whole "Katıl" (PROTOCOL.md §3.2): the way and the control stream, the name, the
    /// install, the local endpoint. The game itself is started by the page (`ready`).
    async fn join_task(&self, session: &str, generation: u64, copy: Option<String>) {
        let invite = {
            let st = self.state();
            st.invites
                .iter()
                .find(|i| i.session == session && i.generation == generation)
                .map(|i| (i.join.clone(), i.from.clone(), i.join.mc.clone()))
        };
        let Some((join, from, mc)) = invite else {
            return;
        };
        let Some(mut account) = self.inner.launcher.account() else {
            self.fail_join(session, generation, WorldJoinFailure::Unreachable);
            return;
        };
        let worlds = self.clone();
        let events_session = session.to_owned();
        let link = Arc::new(GuestLink::new(
            Arc::clone(&self.inner.service),
            Arc::clone(&self.inner.creds),
            Arc::clone(&self.inner.peers),
            session.to_owned(),
            from.clone(),
            join.slot.to_b64u(),
            self.inner.config.transports.clone(),
            move |event| worlds.on_guest_event(&events_session, generation, &mc, event),
        ));
        let mut policy = self.inner.config.policy.clone();
        policy.guest.local_accept_window_ms = policy.guest.local_accept_window_ms.max(JOIN_WAIT_MS);
        policy.guest.control_linger_ms = policy.guest.control_linger_ms.max(JOIN_WAIT_MS);
        let config = GuestConfig {
            join,
            name: account.name.clone(),
            premium: account.premium,
            launcher: self.inner.config.launcher_version.clone(),
            policy,
        };
        let live = match GuestSession::join(config, link).await {
            Ok(live) => live,
            Err(JoinFailure::NoPath) => {
                self.fail_join(session, generation, WorldJoinFailure::Unreachable);
                return;
            }
            Err(JoinFailure::Refused(refusal)) => {
                self.fail_join(session, generation, failure_of(refusal));
                return;
            }
        };
        let kept = {
            let mut st = self.state();
            match st
                .invites
                .iter_mut()
                .find(|i| i.session == session && i.generation == generation)
            {
                Some(invite) => {
                    invite.live = Some(live.clone());
                    true
                }
                None => false,
            }
        };
        if !kept {
            live.leave().await;
            return;
        }
        // The name: another account when this one can't come in.
        loop {
            let verdict = match live.name_verdict() {
                NameVerdict::Ok => break,
                NameVerdict::Name => WorldNameVerdict::Own,
                NameVerdict::NameTaken => WorldNameVerdict::Taken,
            };
            let asked = WorldJoinAsk::Name {
                verdict,
                name: account.name.clone(),
            };
            let Some(WorldJoinAnswer::Account { account_id }) =
                self.ask(session, generation, asked).await
            else {
                self.leave_join(session, generation).await;
                return;
            };
            self.set_stage(
                session,
                generation,
                Some(WorldJoinStage::Preparing { percent: None }),
            );
            let Some(next) = self.inner.launcher.use_account(&account_id).await else {
                self.fail_join(session, generation, WorldJoinFailure::Unreachable);
                return;
            };
            account = next;
            if live
                .rename(account.name.clone(), account.premium)
                .await
                .is_none()
            {
                self.fail_join(session, generation, WorldJoinFailure::Unreachable);
                return;
            }
        }
        // The install.
        let manifest = live.manifest();
        let Some((instance_id, missing)) = self
            .pick_install(session, generation, &manifest, &from, copy)
            .await
        else {
            return;
        };
        self.ready(
            session,
            generation,
            &live,
            &instance_id,
            missing,
            &manifest.mc,
        )
        .await;
    }

    /// "Katıl" again while the session still lives (the game left the world, or "Vazgeç" on the
    /// in-game question): a fresh local endpoint for the same install.
    async fn rejoin(&self, session: &str, generation: u64, live: GuestSession) {
        let manifest = live.manifest();
        let from = {
            let st = self.state();
            st.invites
                .iter()
                .find(|i| i.session == session)
                .map(|i| i.from.clone())
        };
        let Some(from) = from else {
            return;
        };
        let Some((instance_id, missing)) = self
            .pick_install(session, generation, &manifest, &from, None)
            .await
        else {
            return;
        };
        self.ready(
            session,
            generation,
            &live,
            &instance_id,
            missing,
            &manifest.mc,
        )
        .await;
    }

    /// Which install joins (§10.2), asking where the player decides.
    async fn pick_install(
        &self,
        session: &str,
        generation: u64,
        manifest: &Manifest,
        host: &str,
        copy: Option<String>,
    ) -> Option<(String, u32)> {
        let launcher = &self.inner.launcher;
        let progress = self.join_progress(session, generation);
        let mut refreshed = false;
        loop {
            if !self.current(session, generation) {
                return None;
            }
            let installs = launcher.installs(&manifest.mc).await;
            let latest = match &manifest.pack {
                Some(pack) => launcher.latest_pack(&pack.family, &manifest.mc).await,
                None => None,
            };
            let decision = decide(manifest, &installs, latest.as_deref(), host, refreshed);
            // "Kurulumu kopyala": a copy even where an install would do.
            let forced = copy.is_some()
                && matches!(decision, Decision::Use { .. } | Decision::UpdatePack { .. });
            let decision = if forced {
                let earlier = installs
                    .iter()
                    .find(|i| i.copy_of.as_deref() == Some(host) && i.mc == manifest.mc);
                match earlier {
                    Some(earlier) => Decision::UpdateCopy {
                        install: earlier.id.clone(),
                    },
                    None => Decision::Copy {
                        add: manifest.mods.clone(),
                        sets: manifest.sets.clone(),
                    },
                }
            } else {
                decision
            };
            match decision {
                Decision::Use {
                    install,
                    warn_unknown,
                } => return Some((install, warn_unknown)),
                // The game's start syncs the pack to the host's version.
                Decision::UpdatePack { install } => return Some((install, manifest.unknown)),
                Decision::Install { mc } => {
                    let answer = self
                        .ask(
                            session,
                            generation,
                            WorldJoinAsk::Install { mc: mc.clone() },
                        )
                        .await;
                    if answer != Some(WorldJoinAnswer::Install) {
                        self.leave_join(session, generation).await;
                        return None;
                    }
                    self.set_stage(
                        session,
                        generation,
                        Some(WorldJoinStage::Preparing { percent: None }),
                    );
                    // Its files come with the game's start.
                    match launcher.install_official(&mc).await {
                        Some(install) => return Some((install, manifest.unknown)),
                        None => {
                            self.fail_join(session, generation, WorldJoinFailure::Unreachable);
                            return None;
                        }
                    }
                }
                Decision::HostPackOld => {
                    self.fail_join(session, generation, WorldJoinFailure::HostPackOld);
                    return None;
                }
                Decision::RefreshContent => {
                    launcher.refresh_content().await;
                    refreshed = true;
                }
                Decision::Unavailable => {
                    self.fail_join(session, generation, WorldJoinFailure::Unreachable);
                    return None;
                }
                Decision::UpdateCopy { install } => {
                    if launcher
                        .update_copy(&install, manifest, Arc::clone(&progress))
                        .await
                    {
                        return Some((install, manifest.unknown));
                    }
                    self.fail_join(session, generation, WorldJoinFailure::Unreachable);
                    return None;
                }
                Decision::Copy { add, .. } => {
                    let name = match &copy {
                        Some(name) => name.clone(),
                        None => {
                            let bytes = launcher.mods_size(&add).await;
                            let asked = WorldJoinAsk::Copy {
                                mods: u32::try_from(add.len()).unwrap_or(u32::MAX),
                                bytes,
                                unknown: manifest.unknown,
                            };
                            match self.ask(session, generation, asked).await {
                                Some(WorldJoinAnswer::Copy { name }) => name,
                                _ => {
                                    self.leave_join(session, generation).await;
                                    return None;
                                }
                            }
                        }
                    };
                    self.set_stage(
                        session,
                        generation,
                        Some(WorldJoinStage::Preparing { percent: None }),
                    );
                    match launcher
                        .make_copy(&name, host, manifest, Arc::clone(&progress))
                        .await
                    {
                        Some(install) => return Some((install, manifest.unknown)),
                        None => {
                            self.fail_join(session, generation, WorldJoinFailure::Unreachable);
                            return None;
                        }
                    }
                }
            }
        }
    }

    /// The local endpoint opens and the page starts the game into it; an in-game "Katıl" moves
    /// the running game there itself.
    async fn ready(
        &self,
        session: &str,
        generation: u64,
        live: &GuestSession,
        instance_id: &str,
        missing: u32,
        mc: &str,
    ) {
        let filter = self.accept_filter(session, generation);
        let port = match live
            .open_local(mc.to_owned(), Some(refusal_text()), Some(filter))
            .await
        {
            Ok(port) => port,
            Err(error) => {
                tracing::warn!(%error, "the local way to a friend's world did not open");
                self.fail_join(session, generation, WorldJoinFailure::Unreachable);
                return;
            }
        };
        let (in_game, label) = {
            let mut st = self.state();
            let Some(invite) = st
                .invites
                .iter_mut()
                .find(|i| i.session == session && i.generation == generation)
            else {
                return;
            };
            invite.stage = Some(WorldJoinStage::Ready {
                instance_id: instance_id.to_owned(),
                port,
                missing,
            });
            invite.task = None;
            let in_game = invite.in_game;
            let from = invite.from.clone();
            let same_game = st.game.instance_id.as_deref() == Some(instance_id);
            (in_game && same_game, world_label(&st.person(&from).name))
        };
        self.changed();
        if in_game {
            // The player pressed "Katıl" in the game itself: it moves in, nothing restarts.
            self.inner.bridge.connect(port, label).await;
        }
    }

    /// Which connections reach the world: the launcher's filter, and the row says "Bağlanıyor"
    /// once the game came.
    fn accept_filter(&self, session: &str, generation: u64) -> AcceptFilter {
        let worlds = self.clone();
        let session = session.to_owned();
        let filter = self.inner.launcher.accept_filter();
        Arc::new(move |remote| {
            let ok = filter.as_ref().is_none_or(|filter| filter(remote));
            if ok {
                let connecting = {
                    let mut st = worlds.state();
                    match st
                        .invites
                        .iter_mut()
                        .find(|i| i.session == session && i.generation == generation)
                    {
                        Some(invite)
                            if matches!(invite.stage, Some(WorldJoinStage::Ready { .. })) =>
                        {
                            invite.stage = Some(WorldJoinStage::Connecting);
                            true
                        }
                        _ => false,
                    }
                };
                if connecting {
                    worlds.changed();
                }
            }
            ok
        })
    }

    /// What a join's session tells (`pl_host::guest::GuestEvent`).
    fn on_guest_event(&self, session: &str, generation: u64, mc: &str, event: GuestEvent) {
        match event {
            GuestEvent::Report(event) => self.report(mc, event),
            GuestEvent::State { guests, max } => {
                {
                    let mut st = self.state();
                    let Some(invite) = st.invite(session) else {
                        return;
                    };
                    invite.guests = guests;
                    invite.max = max.clamp(1, 7);
                }
                self.changed();
            }
            GuestEvent::GameConnected => {
                self.set_stage(session, generation, Some(WorldJoinStage::In));
            }
            GuestEvent::GameRefused(refusal) => {
                self.fail_join(session, generation, failure_of(refusal));
            }
            GuestEvent::GameEnded => {
                let ended = {
                    let mut st = self.state();
                    match st.invite(session) {
                        Some(invite)
                            if matches!(
                                invite.stage,
                                Some(WorldJoinStage::In | WorldJoinStage::Connecting)
                            ) =>
                        {
                            invite.stage = None;
                            true
                        }
                        _ => false,
                    }
                };
                if ended {
                    self.changed();
                }
            }
            GuestEvent::Ended(end) => {
                let notice = {
                    let mut st = self.state();
                    let Some(index) = st.invites.iter().position(|i| i.session == session) else {
                        return;
                    };
                    let invite = &mut st.invites[index];
                    invite.live = None;
                    invite.ask = None;
                    match end {
                        GuestEnd::Lost => {
                            invite.stage = Some(WorldJoinStage::Lost);
                            None
                        }
                        GuestEnd::Left => {
                            if !matches!(invite.stage, Some(WorldJoinStage::Failed { .. })) {
                                invite.stage = None;
                            }
                            None
                        }
                        GuestEnd::HostClosed | GuestEnd::Removed => {
                            let invite = st.invites.remove(index);
                            let name = st.person(&invite.from).name;
                            let failure = if end == GuestEnd::Removed {
                                WorldJoinFailure::Removed
                            } else {
                                WorldJoinFailure::Closed
                            };
                            Some(HostNotice::JoinFailed {
                                session: session.to_owned(),
                                name,
                                failure,
                            })
                        }
                    }
                };
                self.changed();
                if let Some(notice) = notice {
                    self.notice(notice);
                }
            }
        }
    }

    /// The answer to a join's question.
    pub fn answer(&self, session: &str, answer: WorldJoinAnswer) {
        let tx = self.state().invite(session).and_then(|i| i.ask.take());
        if let Some(tx) = tx {
            let _ = tx.send(answer);
        }
    }

    /// "Vazgeç" (a question, a preparation) or the game that never started.
    pub async fn cancel_join(&self, session: &str) {
        let generation = self.state().invite(session).map(|i| i.generation);
        if let Some(generation) = generation {
            self.leave_join(session, generation).await;
        }
    }

    async fn leave_join(&self, session: &str, generation: u64) {
        let live = {
            let mut st = self.state();
            let next = st.next();
            let Some(invite) = st
                .invites
                .iter_mut()
                .find(|i| i.session == session && i.generation == generation)
            else {
                return;
            };
            // A newer generation: the task that asked stops at its next step.
            invite.generation = next;
            invite.stage = None;
            invite.ask = None;
            if let Some(task) = invite.task.take() {
                task.abort();
            }
            invite.live.take()
        };
        self.changed();
        if let Some(live) = live {
            live.leave().await;
        }
    }

    /// "Kapat" on a world invite: it leaves every list here.
    pub async fn dismiss_invite(&self, session: &str) {
        self.cancel_join(session).await;
        let removed = {
            let mut st = self.state();
            let before = st.invites.len();
            st.invites.retain(|i| i.session != session);
            st.invites.len() != before
        };
        if removed {
            self.changed();
        }
    }

    /// "Bağlan" in game: the overlay moves the running game into the world. False: it could not.
    pub async fn connect_in_game(&self, session: &str, label: String) -> bool {
        let port = {
            let st = self.state();
            st.invites
                .iter()
                .find(|i| i.session == session)
                .and_then(|i| match &i.stage {
                    Some(WorldJoinStage::Ready { port, .. }) => Some(*port),
                    _ => None,
                })
        };
        match port {
            Some(port) => self.inner.bridge.connect(port, label).await,
            None => false,
        }
    }

    /// The credentials table (tests).
    #[cfg(test)]
    pub(crate) fn creds(&self) -> &CredsTable {
        &self.inner.creds
    }
}

fn invite_refusal(error: &WorldError) -> Option<WorldInviteRefusal> {
    match error {
        WorldError::Refused { code, .. } if code == "account_limited" => {
            Some(WorldInviteRefusal::Limited)
        }
        error if error.limits("world_invites") => Some(WorldInviteRefusal::Daily),
        WorldError::Refused { code, .. } if code == "limit_world_invites" => {
            Some(WorldInviteRefusal::Session)
        }
        _ => None,
    }
}

fn invite_view(st: &State, invite: &Invite) -> WorldInviteView {
    let person = st.person(&invite.from);
    WorldInviteView {
        session: invite.session.clone(),
        from: invite.from.clone(),
        name: person.name,
        avatar: person.avatar,
        avatar_url: person.avatar_url,
        mc: invite.join.mc.clone(),
        guests: invite.guests,
        max: invite.max,
        at: invite.at,
        quiet: invite.quiet,
        join: invite.stage.clone(),
    }
}

fn build_view(st: &State) -> HostView {
    let enabled = st.enabled();
    let session = st.hosting.as_ref().filter(|_| enabled).map(|h| {
        let guests: Vec<WorldGuest> = h
            .guests
            .iter()
            .map(|g| {
                let person = st.person(&g.friend);
                WorldGuest {
                    friend: g.friend.clone(),
                    name: person.name,
                    avatar: person.avatar,
                    avatar_url: person.avatar_url,
                    game_name: g.game_name.clone(),
                    state: g.state,
                }
            })
            .collect();
        let (instance_id, folder, world_name, max) = (
            h.instance_id.clone(),
            h.folder.clone(),
            h.world_name.clone(),
            h.max,
        );
        match h.phase {
            Phase::Opening(step, percent) => HostSessionView::Opening {
                instance_id,
                folder,
                world_name,
                guests,
                max,
                step,
                percent,
            },
            Phase::Open => HostSessionView::Open {
                instance_id,
                folder,
                world_name,
                guests,
                max,
                mode: h.mode,
                commands: h.commands,
            },
            Phase::Failed(reason) => HostSessionView::Failed {
                instance_id,
                folder,
                world_name,
                guests,
                max,
                reason,
            },
        }
    });
    HostView {
        seq: st.seq,
        enabled,
        max_guests: st.max_guests(),
        session,
        invites: if enabled {
            st.invites.iter().map(|i| invite_view(st, i)).collect()
        } else {
            Vec::new()
        },
    }
}

/// The mod's mirror of the open session (H8).
fn mirror_frame(st: &State) -> Option<ToGame> {
    let hosting = st.hosting.as_ref()?;
    if hosting.phase != Phase::Open {
        return None;
    }
    Some(ToGame::HostState {
        open: true,
        max: hosting.max,
        guests: hosting
            .guests
            .iter()
            .map(|g| HostGuestView {
                guest: g.guest.map(|id| id.to_string()).unwrap_or_default(),
                friend: g.friend.clone(),
                label: st.person(&g.friend).name,
                name: g.game_name.clone(),
                state: match g.state {
                    WorldGuestState::Pending | WorldGuestState::Invited => "invited",
                    WorldGuestState::Joining => "joining",
                    WorldGuestState::In => "in",
                },
            })
            .collect(),
        error: None,
    })
}
