//! A host's world session: invites, admission, the game connections and the paths they ride on
//! (PROTOCOL.md §3, §5.5, §6.7, §6.8).
//!
//! The session owns the paths the launcher opened for it and accepts every stream a guest opens on
//! them: the opening and the slot's PSK, then the guest's `hello`. Control streams get the name
//! verdict, the manifest and the world's state; game streams are connected to the world on
//! `127.0.0.1` with a PLG1 header first and relayed; a broken game stream waits 15 s for the guest
//! to resume it, possibly on another path. The session moves its guests between paths without the
//! players noticing (TURN renewal, `turn_move`, a lost relay), and tells the launcher what to send
//! through [`HostEvent`]s.

use std::collections::VecDeque;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::sync::{Notify, mpsc, oneshot};
use tokio::task::AbortHandle;
use tokio::time::{Instant, sleep_until, timeout};

use super::admission::{GameAsk, admit_control, admit_game};
use super::binding::{BindingKey, Bindings};
use super::gate::{GateKey, Identity, header};
use crate::manifest::Manifest;
use crate::policy::Policy;
use crate::policy::env::{Creds, CredsError, CredsReason, HostPaths};
use crate::policy::host::{OpenedPath, open_first, open_path};
use crate::report::{Event, MoveCause, Role, RttSinks, bucket_minutes};
use crate::transport::{BoxStream, HostPath, PathKind};
use crate::tunnel::channel::{Channel, In, Keepalive, Out};
use crate::tunnel::control::{
    Accept, ByeReason, Control, ControlError, Hello, MoveWhy, NameVerdict, Refusal, WorldState,
};
use crate::tunnel::handshake;
use crate::tunnel::invite::is_public_relay;
use crate::tunnel::link::{self, LinkConfig, LinkEnd, LinkHandle, LinkState};
use crate::tunnel::noise;
use crate::tunnel::{GuestId, HostKey, Join, Paths, Psk, SessionId, Slot, StreamId};

/// Where the world listens: what `host-published` said.
#[derive(Debug, Clone)]
pub struct GameEndpoint {
    /// The integrated server's port on `127.0.0.1`.
    pub port: u16,
    /// The host game's protocol number.
    pub proto: i32,
    /// The publish id of `host-publish` (1–32 of `[A-Za-z0-9_-]`).
    pub publish_id: String,
    pub gate: GateKey,
}

/// What a session is opened with.
#[derive(Debug)]
pub struct HostConfig {
    pub session: SessionId,
    pub key: HostKey,
    /// The guest limit, 1–7 (after the service's answer).
    pub max_guests: u8,
    /// The host's in-game name: never admitted for a guest.
    pub host_name: String,
    pub mc: String,
    /// The official pack's family id the install follows (`join.pack`).
    pub pack: Option<String>,
    pub manifest: Manifest,
    pub game: GameEndpoint,
    /// The world's folder, where `projects-guests.json` is.
    pub world_dir: PathBuf,
    pub binding: BindingKey,
    pub policy: Policy,
}

/// What the session asks the launcher to send or show.
#[derive(Debug, Clone)]
pub enum HostEvent {
    /// Send `world{open:true, session, guests, max, paths}` (the service fans `world_update` out).
    Announce { guests: u8, max: u8, paths: Paths },
    /// The session is over: send `world{open:false}`.
    Closed,
    /// No path is left and none opens: the bridge's `host-stop`, "Dünyan şu an açılamıyor."
    StopGame,
    /// The bridge's `host-remove{guest}`: the mod disconnects that guest.
    RemoveFromGame { guest: GuestId },
    /// Send `world_revoke{session, to}`.
    Revoke { to: String },
    /// A guest's place in the session changed (the host's lists).
    Guest(GuestUpdate),
    /// Telemetry for the presence socket.
    Report(Event),
}

/// A guest, for the host's lists ("Mert, Notch adıyla").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuestUpdate {
    /// The friend's user id.
    pub friend: String,
    pub guest: GuestId,
    /// The in-game name, once a stream named it.
    pub name: Option<String>,
    pub status: GuestStatus,
}

/// Where a guest is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuestStatus {
    /// Invited; its launcher has not connected.
    Invited,
    /// Its launcher connected (control stream), with this verdict on its name.
    Connected(NameVerdict),
    /// Its game is in the world.
    InWorld,
    /// Its game left the world.
    OutOfWorld,
    /// It left the session.
    Left,
    /// The host removed it.
    Removed,
}

/// The launcher's side of a session: [`HostPaths`] and somewhere to put [`HostEvent`]s.
pub trait HostEnv: HostPaths {
    fn event(&self, event: HostEvent);
}

/// Opening a session failed: "Dünyan şu an açılamıyor." (the session already sent `Closed` and
/// `StopGame`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum OpenError {
    #[error("no path could be opened")]
    NoPath,
    #[error("the session's settings are invalid")]
    Invalid,
}

/// Why an invite could not be made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InviteError {
    #[error("the session is closed")]
    Closed,
    #[error("too many live invites")]
    TooMany,
    #[error("no path is open")]
    NoPath,
}

/// How long a guest has to send its `hello` after the handshake.
const HELLO_WAIT: Duration = Duration::from_secs(5);
/// How long the host waits for a `bye` to go out before closing what is under it.
const BYE_WAIT: Duration = Duration::from_secs(1);
/// TURN relays remembered per guest (its current and previous allocation).
const RELAYS_PER_GUEST: usize = 2;

#[derive(Clone, Copy)]
struct Moving {
    from: PathKind,
    to: PathKind,
    why: MoveWhy,
    since: Instant,
}

struct ControlEntry {
    id: u64,
    tx: mpsc::Sender<Out>,
    path: u64,
    stop: Option<oneshot::Sender<()>>,
}

struct GameEntry {
    stream: StreamId,
    link: LinkHandle,
    path: u64,
    kind: PathKind,
    name: String,
    broken_at: Option<Instant>,
}

struct SlotState {
    slot: Slot,
    friend: String,
    digest: String,
    guest: GuestId,
    psk: Option<Psk>,
    removed: bool,
    starts: VecDeque<Instant>,
    admitting: u32,
    control: Option<ControlEntry>,
    games: Vec<GameEntry>,
    relays: Vec<SocketAddrV4>,
    moving: Option<Moving>,
    name: Option<String>,
    verdict: Option<NameVerdict>,
}

impl SlotState {
    fn live_games(&self) -> impl Iterator<Item = &GameEntry> {
        self.games.iter().filter(|g| !g.link.is_ended())
    }

    fn in_world(&self) -> bool {
        !self.removed && self.live_games().next().is_some()
    }

    fn open_streams(&self) -> usize {
        usize::from(self.control.is_some()) + self.live_games().count() + self.admitting as usize
    }
}

struct PathEntry {
    id: u64,
    kind: PathKind,
    path: Arc<dyn HostPath>,
    creds: Option<Creds>,
    creds_since: Instant,
    retiring: bool,
    unused_since: Option<Instant>,
    usage_at: Instant,
    usage_rx: u64,
    tasks: Vec<AbortHandle>,
}

struct State {
    open: bool,
    slots: Vec<SlotState>,
    paths: Vec<PathEntry>,
    next_id: u64,
    next_guest: u16,
    next_conn: u64,
    failed: VecDeque<Instant>,
    busy_until: Option<Instant>,
    count_dirty: bool,
    last_world: Option<Instant>,
    most_guests: u32,
    moves: u32,
    rx_turn: u64,
    rx_e4mc: u64,
    renewing: bool,
}

enum Job {
    Close(u64),
    Renew(u64),
}

struct Inner {
    session: SessionId,
    key: HostKey,
    max: u8,
    host_name: String,
    mc: String,
    pack: Option<String>,
    manifest: Manifest,
    game: GameEndpoint,
    world_dir: PathBuf,
    binding: BindingKey,
    key_id: String,
    policy: Policy,
    env: Arc<dyn HostEnv>,
    state: Mutex<State>,
    wake: Notify,
    rtt: RttSinks,
    started: Instant,
    opening: tokio::sync::Mutex<()>,
}

/// A host's open world session. Cheap to clone; [`HostSession::close`] ends it, and so does
/// dropping the last clone.
#[derive(Clone)]
pub struct HostSession {
    inner: Arc<Inner>,
    _last: Arc<CloseOnDrop>,
}

/// Closes the session when the last handle goes (the background tasks keep the session itself).
struct CloseOnDrop(std::sync::Weak<Inner>);

impl Drop for CloseOnDrop {
    fn drop(&mut self) {
        if let (Some(inner), Ok(runtime)) =
            (self.0.upgrade(), tokio::runtime::Handle::try_current())
        {
            runtime.spawn(async move { inner.close().await });
        }
    }
}

impl std::fmt::Debug for HostSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostSession")
            .field("session", &self.inner.session)
            .finish_non_exhaustive()
    }
}

fn ms(value: u64) -> Duration {
    Duration::from_millis(value)
}

fn valid_publish_id(id: &str) -> bool {
    (1..=32).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Sends a control message on a stream from outside its task.
fn push(tx: &mpsc::Sender<Out>, message: Control) {
    if let Err(mpsc::error::TrySendError::Full(item)) = tx.try_send(Out::Control(message)) {
        let tx = tx.clone();
        tokio::spawn(async move {
            let _ = tx.send(item).await;
        });
    }
}

/// Ends a control stream with `bye{reason}` and waits (a little) until it went out, so the
/// guest hears the reason before its game stream and the paths go.
async fn say_bye(mut control: ControlEntry, reason: ByeReason) {
    let _ = timeout(BYE_WAIT, async {
        let _ = control.tx.send(Out::Control(Control::Bye(reason))).await;
        let _ = control.tx.send(Out::Finish).await;
    })
    .await;
    if let Some(stop) = control.stop.take() {
        let _ = stop.send(());
    }
    let _ = timeout(BYE_WAIT, control.tx.closed()).await;
}

/// Refuses a stream: `no{reason}`, then the stream ends.
async fn refuse(channel: Channel, reason: Refusal) {
    let _ = channel.send(Control::No(reason)).await;
    channel.finish().await;
}

impl HostSession {
    /// Opens a session after `host-published` (PROTOCOL.md §3.1 step 5): asks `turn_creds{primary}`
    /// for the order, prepares the first path that works and announces it. On failure the session
    /// already asked for `world{open:false}` and the bridge's `host-stop`.
    pub async fn open(config: HostConfig, env: Arc<dyn HostEnv>) -> Result<Self, OpenError> {
        if !(1..=7).contains(&config.max_guests)
            || !valid_publish_id(&config.game.publish_id)
            || config.game.port == 0
        {
            return Err(OpenError::Invalid);
        }
        let now = Instant::now();
        let key_id = config.binding.key_id();
        let inner = Arc::new(Inner {
            session: config.session,
            key: config.key,
            max: config.max_guests,
            host_name: config.host_name,
            mc: config.mc,
            pack: config.pack,
            manifest: config.manifest,
            game: config.game,
            world_dir: config.world_dir,
            binding: config.binding,
            key_id,
            policy: config.policy,
            env: env.clone(),
            state: Mutex::new(State {
                open: true,
                slots: Vec::new(),
                paths: Vec::new(),
                next_id: 1,
                next_guest: 1,
                next_conn: 1,
                failed: VecDeque::new(),
                busy_until: None,
                count_dirty: false,
                last_world: None,
                most_guests: 0,
                moves: 0,
                rx_turn: 0,
                rx_e4mc: 0,
                renewing: false,
            }),
            wake: Notify::new(),
            rtt: RttSinks::default(),
            started: now,
            opening: tokio::sync::Mutex::new(()),
        });
        let report = |event| env.event(HostEvent::Report(event));
        let (order, creds) = match env.turn_creds(CredsReason::Primary).await {
            Ok(answer) => (answer.order, answer.creds),
            Err(CredsError::Gone) => (Vec::new(), None),
            Err(_) => (vec![PathKind::E4mc], None),
        };
        let paths_env: &dyn HostPaths = env.as_ref();
        let Some((kind, opened)) = open_first(paths_env, &order, creds, &report).await else {
            inner.state().open = false;
            env.event(HostEvent::Closed);
            env.event(HostEvent::StopGame);
            return Err(OpenError::NoPath);
        };
        inner.add_path(kind, opened);
        tokio::spawn(maintain(inner.clone()));
        let last = Arc::new(CloseOnDrop(Arc::downgrade(&inner)));
        Ok(Self { inner, _last: last })
    }

    pub fn session_id(&self) -> SessionId {
        self.inner.session
    }

    /// The paths the session announces now.
    pub fn paths(&self) -> Paths {
        current_paths(&self.inner.state())
    }

    /// The friends whose invites are live (`world.invited` after the presence socket reconnected).
    pub fn invited(&self) -> Vec<String> {
        self.inner
            .state()
            .slots
            .iter()
            .filter(|s| !s.removed)
            .map(|s| s.friend.clone())
            .collect()
    }

    /// The invite for `friend` (`world_invite.join`): the same slot and PSK while the friend is not
    /// removed, a new slot after a removal. Opens a path first when none is open.
    pub async fn invite(&self, friend: &str) -> Result<Join, InviteError> {
        if self.paths().is_empty() {
            self.inner.ensure_any_path().await;
        }
        let mut st = self.inner.state();
        if !st.open {
            return Err(InviteError::Closed);
        }
        let paths = current_paths(&st);
        if paths.is_empty() {
            return Err(InviteError::NoPath);
        }
        let existing = st
            .slots
            .iter()
            .position(|s| !s.removed && s.friend == friend);
        let index = match existing {
            Some(i) => i,
            None => {
                let live = st.slots.iter().filter(|s| !s.removed).count();
                let next_guest = GuestId::new(st.next_guest);
                let (true, Some(guest)) = (
                    live < self.inner.policy.host.max_live_invites as usize,
                    next_guest,
                ) else {
                    return Err(InviteError::TooMany);
                };
                st.next_guest += 1;
                st.slots.push(SlotState {
                    slot: Slot::generate(),
                    friend: friend.to_owned(),
                    digest: self.inner.binding.digest(friend),
                    guest,
                    psk: Some(Psk::generate()),
                    removed: false,
                    starts: VecDeque::new(),
                    admitting: 0,
                    control: None,
                    games: Vec::new(),
                    relays: Vec::new(),
                    moving: None,
                    name: None,
                    verdict: None,
                });
                st.slots.len() - 1
            }
        };
        let slot = &st.slots[index];
        Ok(Join {
            session: self.inner.session,
            key: *self.inner.key.public(),
            slot: slot.slot,
            psk: slot.psk.clone().expect("live slots have a PSK"),
            mc: self.inner.mc.clone(),
            proto: self.inner.game.proto,
            pack: self.inner.pack.clone(),
            paths,
        })
    }

    /// A guest's `world_peer{slot, path, relay?}`: opens the path when the session has none of
    /// that kind (TURN: asks `turn_creds{role:"host", reason:"primary"}`), permits the guest's TURN
    /// relay, and returns the paths for `world_peer_ok`. `None` when the slot is not a live invite.
    pub async fn world_peer(
        &self,
        slot: &Slot,
        kind: PathKind,
        relay: Option<SocketAddrV4>,
    ) -> Option<Paths> {
        {
            let st = self.inner.state();
            if !st.open || !st.slots.iter().any(|s| s.slot == *slot && !s.removed) {
                return None;
            }
        }
        self.inner.ensure_path(kind).await;
        if let (PathKind::Turn, Some(relay)) = (kind, relay)
            && is_public_relay(relay)
        {
            self.inner.admit_relay(slot, relay).await;
        }
        Some(self.paths())
    }

    /// Takes a guest out: the host's button (`by_host`, then `world_revoke`), or the service's
    /// `world_revoked{to}` (a block, an unfriending, a restriction).
    pub async fn remove(&self, friend: &str, by_host: bool) {
        let taken = {
            let mut st = self.inner.state();
            let Some(slot) = st
                .slots
                .iter_mut()
                .find(|s| !s.removed && s.friend == friend)
            else {
                return;
            };
            slot.removed = true;
            slot.psk = None;
            slot.moving = None;
            let control = slot.control.take();
            let games: Vec<LinkHandle> = slot.games.drain(..).map(|g| g.link).collect();
            let relays = std::mem::take(&mut slot.relays);
            let guest = slot.guest;
            let name = slot.name.clone();
            let turn_paths: Vec<Arc<dyn HostPath>> = st
                .paths
                .iter()
                .filter(|p| p.kind == PathKind::Turn)
                .map(|p| p.path.clone())
                .collect();
            (control, games, relays, guest, name, turn_paths)
        };
        let (control, games, relays, guest, name, turn_paths) = taken;
        if let Some(control) = control {
            say_bye(control, ByeReason::Removed).await;
        }
        for link in games {
            link.abort().await;
        }
        for path in turn_paths {
            for relay in &relays {
                path.forget_peer(*relay);
            }
        }
        let env = &self.inner.env;
        env.event(HostEvent::RemoveFromGame { guest });
        if by_host {
            env.event(HostEvent::Revoke {
                to: friend.to_owned(),
            });
        }
        env.event(HostEvent::Guest(GuestUpdate {
            friend: friend.to_owned(),
            guest,
            name,
            status: GuestStatus::Removed,
        }));
        self.inner.count_changed();
    }

    /// The service's `turn_move{reason}` for this session: every guest on TURN moves to e4mc and
    /// TURN closes once they did (§6.7, "Host-side moves").
    pub async fn turn_move(&self, reason: &str) {
        let why = MoveWhy::from_turn_move(reason);
        self.inner
            .move_off(PathKind::Turn, PathKind::E4mc, why)
            .await;
    }

    /// The guests, for the host's lists.
    pub fn guests(&self) -> Vec<GuestUpdate> {
        let st = self.inner.state();
        st.slots
            .iter()
            .filter(|s| !s.removed)
            .map(|s| GuestUpdate {
                friend: s.friend.clone(),
                guest: s.guest,
                name: s.name.clone(),
                status: if s.in_world() {
                    GuestStatus::InWorld
                } else if s.control.is_some() {
                    GuestStatus::Connected(s.verdict.unwrap_or(NameVerdict::Ok))
                } else {
                    GuestStatus::Invited
                },
            })
            .collect()
    }

    /// Ends the session (§3.4): `bye{closed}` on every control stream, the game connections and
    /// paths closed, credentials released, the session's report, then [`HostEvent::Closed`].
    pub async fn close(&self) {
        self.inner.close().await;
    }
}

fn soon(at: Instant, next: &mut Option<Instant>) {
    *next = Some(next.map_or(at, |n| n.min(at)));
}

fn current_paths(st: &State) -> Paths {
    let mut paths = Paths::default();
    for kind in [PathKind::Turn, PathKind::E4mc] {
        if let Some(entry) = st
            .paths
            .iter()
            .rev()
            .find(|p| p.kind == kind && !p.retiring)
        {
            paths.set(entry.path.address());
        }
    }
    paths
}

fn guests_in_world(st: &State) -> usize {
    st.slots.iter().filter(|s| s.in_world()).count()
}

fn path_in_use(st: &State, id: u64) -> bool {
    st.slots.iter().filter(|s| !s.removed).any(|s| {
        s.control.as_ref().is_some_and(|c| c.path == id) || s.live_games().any(|g| g.path == id)
    })
}

impl Inner {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().expect("host session state")
    }

    fn report(&self, event: Event) {
        self.env.event(HostEvent::Report(event));
    }

    fn world_state(&self, st: &State) -> WorldState {
        WorldState {
            guests: u8::try_from(guests_in_world(st)).unwrap_or(u8::MAX),
            max: self.max,
            paths: current_paths(st),
        }
    }

    /// Sends `state` on every control stream.
    fn broadcast_state(&self, st: &State) {
        let state = self.world_state(st);
        for slot in st.slots.iter().filter(|s| !s.removed) {
            if let Some(control) = &slot.control {
                push(&control.tx, Control::State(state.clone()));
            }
        }
    }

    /// The paths changed: announce at once.
    fn announce(&self) {
        let (event, open) = {
            let mut st = self.state();
            st.count_dirty = false;
            st.last_world = Some(Instant::now());
            self.broadcast_state(&st);
            let state = self.world_state(&st);
            (
                HostEvent::Announce {
                    guests: state.guests,
                    max: state.max,
                    paths: state.paths,
                },
                st.open,
            )
        };
        if open {
            self.env.event(event);
        }
        self.wake.notify_one();
    }

    /// The guest count changed: `state` at once, `world` at most every 10 s.
    fn count_changed(&self) {
        {
            let mut st = self.state();
            let now = u32::try_from(guests_in_world(&st)).unwrap_or(u32::MAX);
            st.most_guests = st.most_guests.max(now);
            st.count_dirty = true;
            self.broadcast_state(&st);
        }
        self.wake.notify_one();
    }

    fn is_busy(&self) -> bool {
        self.state()
            .busy_until
            .is_some_and(|until| Instant::now() < until)
    }

    fn failed_opening(&self) {
        let mut st = self.state();
        let now = Instant::now();
        st.failed.push_back(now);
        while st
            .failed
            .front()
            .is_some_and(|t| now.duration_since(*t) >= Duration::from_secs(60))
        {
            st.failed.pop_front();
        }
        if st.failed.len() > self.policy.host.failed_openings_per_minute as usize {
            st.busy_until = Some(now + ms(self.policy.host.busy_ms));
            st.failed.clear();
        }
    }

    /// Whether `slot` may open one more stream now; counts it as a new stream either way.
    fn slot_admits_stream(&self, st: &mut State, index: usize) -> bool {
        let now = Instant::now();
        let busy_window = st.busy_until.is_some_and(|until| now < until);
        let host = &self.policy.host;
        let slot = &mut st.slots[index];
        while slot
            .starts
            .front()
            .is_some_and(|t| now.duration_since(*t) >= Duration::from_secs(60))
        {
            slot.starts.pop_front();
        }
        let too_many_new = slot.starts.len() >= host.max_new_streams_per_guest_per_minute as usize;
        let too_many_open = slot.open_streams() > host.max_streams_per_guest as usize;
        slot.starts.push_back(now);
        !(busy_window || too_many_new || too_many_open)
    }

    fn add_path(self: &Arc<Self>, kind: PathKind, opened: OpenedPath) {
        let OpenedPath { path, creds } = opened;
        let now = Instant::now();
        let id = {
            let mut st = self.state();
            let id = st.next_id;
            st.next_id += 1;
            let accept = tokio::spawn(accept_loop(self.clone(), id, kind, path.clone()));
            let watch = tokio::spawn(watch_path(self.clone(), id, path.clone()));
            st.paths.push(PathEntry {
                id,
                kind,
                path,
                creds,
                creds_since: now,
                retiring: false,
                unused_since: Some(now),
                usage_at: now,
                usage_rx: 0,
                tasks: vec![accept.abort_handle(), watch.abort_handle()],
            });
            id
        };
        tracing::info!(path = kind.as_str(), id, "world path ready");
        self.announce();
    }

    /// Opens a path of `kind` unless one is open; one opening at a time.
    async fn ensure_path(self: &Arc<Self>, kind: PathKind) -> bool {
        let _one = self.opening.lock().await;
        {
            let st = self.state();
            if !st.open {
                return false;
            }
            if st.paths.iter().any(|p| p.kind == kind && !p.retiring) {
                return true;
            }
        }
        let report = |event| self.report(event);
        let env: &dyn HostPaths = self.env.as_ref();
        let Some(opened) = open_path(env, kind, None, CredsReason::Primary, &report).await else {
            return false;
        };
        if kind == PathKind::Turn {
            let relays: Vec<SocketAddrV4> = {
                let st = self.state();
                st.slots
                    .iter()
                    .filter(|s| !s.removed)
                    .flat_map(|s| s.relays.clone())
                    .collect()
            };
            for relay in relays {
                let _ = opened.path.admit_peer(relay).await;
            }
        }
        if !self.state().open {
            close_opened(env, opened).await;
            return false;
        }
        self.add_path(kind, opened);
        true
    }

    /// Opens some path when none is open: the service's order first.
    async fn ensure_any_path(self: &Arc<Self>) {
        let order = match self.env.turn_creds(CredsReason::Primary).await {
            Ok(answer) => {
                if let Some(creds) = answer.creds {
                    // `ensure_path` asks again; these are not needed.
                    self.env.turn_release(creds);
                }
                answer.order
            }
            Err(_) => vec![PathKind::E4mc],
        };
        for kind in order {
            if self.ensure_path(kind).await {
                return;
            }
        }
    }

    async fn admit_relay(&self, slot: &Slot, relay: SocketAddrV4) {
        let paths: Vec<Arc<dyn HostPath>> = {
            let mut st = self.state();
            let Some(entry) = st.slots.iter_mut().find(|s| s.slot == *slot && !s.removed) else {
                return;
            };
            if !entry.relays.contains(&relay) {
                entry.relays.push(relay);
                if entry.relays.len() > RELAYS_PER_GUEST {
                    entry.relays.remove(0);
                }
            }
            st.paths
                .iter()
                .filter(|p| p.kind == PathKind::Turn)
                .map(|p| p.path.clone())
                .collect()
        };
        for path in paths {
            if path.admit_peer(relay).await.is_err() {
                tracing::debug!("permitting a guest's relay failed");
            }
        }
    }

    /// Moves every guest on a `from` path to a `to` path: opens it if needed, announces, then
    /// `move{to, why}`; the old paths retire (they close 30 s after their last guest left).
    async fn move_off(self: &Arc<Self>, from: PathKind, to: PathKind, why: MoveWhy) {
        let target_ok = self.ensure_path(to).await;
        let now = Instant::now();
        {
            let mut st = self.state();
            let old: Vec<u64> = st
                .paths
                .iter_mut()
                .filter(|p| p.kind == from && !p.retiring)
                .map(|p| {
                    p.retiring = true;
                    p.id
                })
                .collect();
            if target_ok {
                for slot in st.slots.iter_mut().filter(|s| !s.removed) {
                    let on_old = slot.control.as_ref().is_some_and(|c| old.contains(&c.path))
                        || slot.live_games().any(|g| old.contains(&g.path));
                    if !on_old {
                        continue;
                    }
                    slot.moving = Some(Moving {
                        from,
                        to,
                        why,
                        since: now,
                    });
                    if let Some(control) = &slot.control {
                        push(&control.tx, Control::Move { to, why });
                    }
                }
            }
        }
        self.announce();
    }

    /// A path is gone (its relay closed it, the allocation or credentials were lost).
    async fn path_lost(self: &Arc<Self>, id: u64) {
        let (entry, replace) = {
            let mut st = self.state();
            let Some(index) = st.paths.iter().position(|p| p.id == id) else {
                return;
            };
            let entry = st.paths.remove(index);
            let rx = entry.path.rx_bytes();
            match entry.kind {
                PathKind::Turn => st.rx_turn += rx,
                PathKind::E4mc => st.rx_e4mc += rx,
            }
            let other_open = st.paths.iter().any(|p| !p.retiring);
            let replace = st.open && !entry.retiring && !other_open;
            (entry, replace)
        };
        tracing::info!(path = entry.kind.as_str(), id, "world path lost");
        for task in &entry.tasks {
            task.abort();
        }
        self.final_usage(&entry);
        if let Some(creds) = entry.creds {
            self.env.turn_release(creds);
        }
        self.announce();
        if replace {
            // Guests resume on whatever opens; same kind first, then the other.
            let other = match entry.kind {
                PathKind::Turn => PathKind::E4mc,
                PathKind::E4mc => PathKind::Turn,
            };
            if !self.ensure_path(entry.kind).await && !self.ensure_path(other).await {
                tracing::warn!("no world path left; the session stops");
                self.env.event(HostEvent::StopGame);
                self.close().await;
            }
        }
    }

    fn final_usage(&self, entry: &PathEntry) {
        if entry.kind == PathKind::Turn {
            let rx = entry.path.rx_bytes();
            self.report(Event::Usage {
                session: self.session,
                role: Role::Host,
                turn_rx: rx.saturating_sub(entry.usage_rx),
            });
        }
    }

    /// Closes a path the session no longer needs (retired, idle TURN): deletes it, releases its
    /// credentials, announces.
    async fn close_path(self: &Arc<Self>, id: u64) {
        let entry = {
            let mut st = self.state();
            let Some(index) = st.paths.iter().position(|p| p.id == id) else {
                return;
            };
            let entry = st.paths.remove(index);
            let rx = entry.path.rx_bytes();
            match entry.kind {
                PathKind::Turn => st.rx_turn += rx,
                PathKind::E4mc => st.rx_e4mc += rx,
            }
            entry
        };
        for task in &entry.tasks {
            task.abort();
        }
        self.final_usage(&entry);
        entry.path.close().await;
        if let Some(creds) = entry.creds {
            self.env.turn_release(creds);
        }
        tracing::info!(path = entry.kind.as_str(), id, "world path closed");
        self.announce();
    }

    /// TURN credential renewal (§5.5): a new allocation, the guests moved to it, the old one
    /// retired. Refused: the guests move to e4mc.
    async fn renew(self: &Arc<Self>, id: u64) {
        let in_use = {
            let mut st = self.state();
            st.renewing = true;
            path_in_use(&st, id)
        };
        if !in_use {
            // A host with no guest on TURN releases instead.
            self.close_path(id).await;
            self.state().renewing = false;
            return;
        }
        let report = |event| self.report(event);
        let env: &dyn HostPaths = self.env.as_ref();
        let renewed = open_path(env, PathKind::Turn, None, CredsReason::Renew, &report).await;
        match renewed {
            Some(opened) => {
                let relays: Vec<SocketAddrV4> = {
                    let st = self.state();
                    st.slots
                        .iter()
                        .filter(|s| !s.removed)
                        .flat_map(|s| s.relays.clone())
                        .collect()
                };
                for relay in relays {
                    let _ = opened.path.admit_peer(relay).await;
                }
                {
                    let mut st = self.state();
                    if let Some(old) = st.paths.iter_mut().find(|p| p.id == id) {
                        old.retiring = true;
                    }
                }
                self.add_path(PathKind::Turn, opened);
                let now = Instant::now();
                let mut st = self.state();
                for slot in st.slots.iter_mut().filter(|s| !s.removed) {
                    let on_old = slot.control.as_ref().is_some_and(|c| c.path == id)
                        || slot.live_games().any(|g| g.path == id);
                    if !on_old {
                        continue;
                    }
                    slot.moving = Some(Moving {
                        from: PathKind::Turn,
                        to: PathKind::Turn,
                        why: MoveWhy::Renew,
                        since: now,
                    });
                    if let Some(control) = &slot.control {
                        push(
                            &control.tx,
                            Control::Move {
                                to: PathKind::Turn,
                                why: MoveWhy::Renew,
                            },
                        );
                    }
                }
            }
            None => {
                self.move_off(PathKind::Turn, PathKind::E4mc, MoveWhy::Renew)
                    .await
            }
        }
        self.state().renewing = false;
    }

    async fn close(self: &Arc<Self>) {
        let (controls, links, paths, summary) = {
            let mut st = self.state();
            if !st.open {
                return;
            }
            st.open = false;
            let mut controls = Vec::new();
            let mut links = Vec::new();
            for slot in &mut st.slots {
                if let Some(control) = slot.control.take() {
                    controls.push(control);
                }
                links.extend(slot.games.drain(..).map(|g| g.link));
                slot.psk = None;
            }
            let paths: Vec<PathEntry> = std::mem::take(&mut st.paths);
            let mut rx_turn = st.rx_turn;
            let mut rx_e4mc = st.rx_e4mc;
            for path in &paths {
                match path.kind {
                    PathKind::Turn => rx_turn += path.path.rx_bytes(),
                    PathKind::E4mc => rx_e4mc += path.path.rx_bytes(),
                }
            }
            let (rtt_turn, rtt_e4mc) = self.rtt.medians();
            let summary = Event::Session {
                role: Role::Host,
                min: bucket_minutes(self.started.elapsed()),
                guests: st.most_guests,
                turn_rx: rx_turn,
                e4mc_rx: rx_e4mc,
                moves: st.moves,
                rtt_turn,
                rtt_e4mc,
            };
            (controls, links, paths, summary)
        };
        let byes: Vec<_> = controls
            .into_iter()
            .map(|control| tokio::spawn(say_bye(control, ByeReason::Closed)))
            .collect();
        for bye in byes {
            let _ = bye.await;
        }
        for link in links {
            link.abort().await;
        }
        for entry in paths {
            for task in &entry.tasks {
                task.abort();
            }
            self.final_usage(&entry);
            entry.path.close().await;
            if let Some(creds) = entry.creds {
                self.env.turn_release(creds);
            }
        }
        self.report(summary);
        tracing::info!("world session closed");
        self.env.event(HostEvent::Closed);
        self.wake.notify_one();
    }

    /// The timers: the guest count's `world`, retiring and idle paths, TURN renewal and usage.
    fn tick(&self, now: Instant) -> (Option<Instant>, Vec<Job>) {
        let mut st = self.state();
        if !st.open {
            return (None, Vec::new());
        }
        let mut next: Option<Instant> = None;
        let mut jobs = Vec::new();
        let mut announce = None;
        if st.count_dirty {
            let due = st.last_world.map_or(now, |t| {
                t + ms(self.policy.host.world_count_min_interval_ms)
            });
            if now >= due {
                st.count_dirty = false;
                st.last_world = Some(now);
                let state = self.world_state(&st);
                announce = Some(HostEvent::Announce {
                    guests: state.guests,
                    max: state.max,
                    paths: state.paths,
                });
            } else {
                soon(due, &mut next);
            }
        }
        let ids: Vec<u64> = st.paths.iter().map(|p| p.id).collect();
        let renewing = st.renewing;
        let mut usage = Vec::new();
        for id in ids {
            let in_use = path_in_use(&st, id);
            let Some(entry) = st.paths.iter_mut().find(|p| p.id == id) else {
                continue;
            };
            if in_use {
                entry.unused_since = None;
            } else if entry.unused_since.is_none() {
                entry.unused_since = Some(now);
            }
            if entry.retiring {
                if let Some(since) = entry.unused_since {
                    let due = since + ms(self.policy.host.old_path_linger_ms);
                    if now >= due {
                        jobs.push(Job::Close(id));
                    } else {
                        soon(due, &mut next);
                    }
                }
                continue;
            }
            if entry.kind != PathKind::Turn {
                continue;
            }
            if let Some(since) = entry.unused_since {
                let due = since + ms(self.policy.turn.host_release_idle_ms);
                if now >= due {
                    jobs.push(Job::Close(id));
                    continue;
                }
                soon(due, &mut next);
            }
            if let Some(creds) = entry.creds {
                let lead = Duration::from_secs(self.policy.turn.renew_when_left_s);
                let due = entry.creds_since + creds.ttl.saturating_sub(lead);
                if now >= due && !renewing {
                    jobs.push(Job::Renew(id));
                } else if now < due {
                    soon(due, &mut next);
                }
            }
            let due = entry.usage_at + ms(self.policy.report.usage_every_ms);
            if now >= due {
                let rx = entry.path.rx_bytes();
                usage.push(rx.saturating_sub(entry.usage_rx));
                entry.usage_rx = rx;
                entry.usage_at = now;
                soon(now + ms(self.policy.report.usage_every_ms), &mut next);
            } else {
                soon(due, &mut next);
            }
        }
        drop(st);
        if let Some(event) = announce {
            self.env.event(event);
        }
        for turn_rx in usage {
            self.report(Event::Usage {
                session: self.session,
                role: Role::Host,
                turn_rx,
            });
        }
        (next, jobs)
    }
}

async fn close_opened(env: &dyn HostPaths, opened: OpenedPath) {
    opened.path.close().await;
    if let Some(creds) = opened.creds {
        env.turn_release(creds);
    }
}

async fn maintain(inner: Arc<Inner>) {
    loop {
        let (next, jobs) = inner.tick(Instant::now());
        for job in jobs {
            match job {
                Job::Close(id) => inner.close_path(id).await,
                Job::Renew(id) => {
                    let inner = inner.clone();
                    tokio::spawn(async move { inner.renew(id).await });
                }
            }
        }
        if !inner.state().open {
            return;
        }
        match next {
            Some(at) => {
                tokio::select! {
                    () = sleep_until(at) => {}
                    () = inner.wake.notified() => {}
                }
            }
            None => inner.wake.notified().await,
        }
    }
}

async fn accept_loop(inner: Arc<Inner>, id: u64, kind: PathKind, path: Arc<dyn HostPath>) {
    loop {
        match path.accept().await {
            Ok(stream) => {
                tokio::spawn(serve_stream(inner.clone(), id, kind, stream));
            }
            Err(_) => {
                let inner = inner.clone();
                tokio::spawn(async move { inner.path_lost(id).await });
                return;
            }
        }
    }
}

async fn watch_path(inner: Arc<Inner>, id: u64, path: Arc<dyn HostPath>) {
    let _ = path.closed().await;
    let inner = inner.clone();
    tokio::spawn(async move { inner.path_lost(id).await });
}

/// A connection to the world: always `127.0.0.1:<port>`, never another address (the session can
/// never be a general tunnel).
async fn connect_world(port: u16) -> std::io::Result<BoxStream> {
    #[cfg(test)]
    if let Some(stream) = memory_world::connect(port) {
        return Ok(Box::new(stream));
    }
    let tcp = TcpStream::connect(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)).await?;
    tcp.set_nodelay(true)?;
    Ok(Box::new(tcp))
}

/// One stream a guest opened: the opening, the handshake, the `hello`, then its service.
async fn serve_stream(inner: Arc<Inner>, path_id: u64, kind: PathKind, mut stream: BoxStream) {
    if inner.is_busy() {
        // Closed at once without reading.
        return;
    }
    let limit = ms(inner.policy.host.opening_timeout_ms);
    let Ok(Some((slot, msg1))) = timeout(limit, handshake::read_opening(&mut stream)).await else {
        inner.failed_opening();
        return;
    };
    // The slot comes first; no other key is ever tried.
    let psk = {
        let st = inner.state();
        st.slots
            .iter()
            .find(|s| s.slot == slot && !s.removed)
            .and_then(|s| s.psk.clone())
    };
    let Some(psk) = psk else {
        inner.failed_opening();
        return;
    };
    let Ok((cipher, reply)) = noise::respond(&inner.key, &psk, &slot, &inner.session, &msg1) else {
        inner.failed_opening();
        return;
    };
    drop(psk);
    if stream.write_all(&reply).await.is_err() || stream.flush().await.is_err() {
        return;
    }
    let keepalive = Keepalive::from_policy(&inner.policy);
    let mut channel = Channel::spawn(stream, cipher, keepalive, Some(inner.rtt.for_kind(kind)));
    let hello = loop {
        match timeout(HELLO_WAIT, channel.recv()).await {
            Ok(Some(In::Control(Ok(Control::Hello(hello))))) => break hello,
            Ok(Some(In::Control(Err(
                ControlError::UnsupportedHello | ControlError::Malformed(_),
            )))) => {
                refuse(channel, Refusal::Unsupported).await;
                return;
            }
            Ok(Some(In::Control(Err(ControlError::Unknown(_))))) => {}
            _ => return,
        }
    };
    match hello {
        Hello::Control {
            name,
            premium: _,
            launcher: _,
        } => {
            serve_control(inner, slot, path_id, channel, name).await;
        }
        Hello::Game {
            name,
            premium,
            mc: _,
            proto,
        } => {
            serve_game(inner, slot, path_id, kind, channel, name, premium, proto).await;
        }
        Hello::Resume { stream, recv } => {
            serve_resume(inner, slot, path_id, kind, channel, stream, recv).await;
        }
    }
}

async fn serve_control(
    inner: Arc<Inner>,
    slot: Slot,
    path_id: u64,
    mut channel: Channel,
    name: String,
) {
    let admitted = {
        let mut st = inner.state();
        let open = st.open;
        let Some(index) = st.slots.iter().position(|s| s.slot == slot) else {
            return;
        };
        let allowed = inner.slot_admits_stream(&mut st, index);
        let entry = &st.slots[index];
        match admit_control(entry.removed, open, !allowed) {
            Err(reason) => Err(reason),
            Ok(()) => {
                let lower = name.to_ascii_lowercase();
                let in_world_elsewhere = st
                    .slots
                    .iter()
                    .filter(|s| s.slot != slot)
                    .any(|s| s.live_games().any(|g| g.name.to_ascii_lowercase() == lower));
                let entry = &mut st.slots[index];
                entry.admitting += 1;
                Ok((
                    entry.digest.clone(),
                    entry.guest,
                    entry.friend.clone(),
                    in_world_elsewhere,
                ))
            }
        }
    };
    let (digest, guest, friend, in_world_elsewhere) = match admitted {
        Ok(admitted) => admitted,
        Err(reason) => {
            refuse(channel, reason).await;
            return;
        }
    };
    let bindings = Bindings::read(&inner.world_dir, &inner.key_id).await;
    let mut verdict = bindings.verdict(&name, &digest, &inner.host_name);
    if verdict == NameVerdict::Ok && in_world_elsewhere {
        verdict = NameVerdict::Name;
    }
    let stream_id = StreamId::generate();
    let (stop_tx, mut stop) = oneshot::channel();
    let registered = {
        let mut st = inner.state();
        let open = st.open;
        let id = st.next_id;
        st.next_id += 1;
        let state = inner.world_state(&st);
        let Some(entry) = st.slots.iter_mut().find(|s| s.slot == slot) else {
            return;
        };
        entry.admitting = entry.admitting.saturating_sub(1);
        if entry.removed || !open {
            Err(if open {
                Refusal::Removed
            } else {
                Refusal::Closed
            })
        } else {
            // A second control stream from the same slot replaces the first.
            if let Some(mut old) = entry.control.take()
                && let Some(stop) = old.stop.take()
            {
                let _ = stop.send(());
            }
            entry.control = Some(ControlEntry {
                id,
                tx: channel.tx.clone(),
                path: path_id,
                stop: Some(stop_tx),
            });
            entry.name = Some(name.clone());
            entry.verdict = Some(verdict);
            let _ = channel
                .tx
                .try_send(Out::Control(Control::Ok(Accept::control(
                    guest, stream_id, verdict,
                ))));
            let _ = channel.tx.try_send(Out::Control(Control::Manifest(Box::new(
                inner.manifest.clone(),
            ))));
            let _ = channel.tx.try_send(Out::Control(Control::State(state)));
            Ok(id)
        }
    };
    let control_id = match registered {
        Ok(id) => id,
        Err(reason) => {
            refuse(channel, reason).await;
            return;
        }
    };
    inner.env.event(HostEvent::Guest(GuestUpdate {
        friend: friend.clone(),
        guest,
        name: Some(name),
        status: GuestStatus::Connected(verdict),
    }));
    inner.wake.notify_one();
    let mut left = false;
    loop {
        tokio::select! {
            _ = &mut stop => break,
            item = channel.recv() => match item {
                Some(In::Control(Ok(Control::Bye(ByeReason::Leave)))) => {
                    left = true;
                    break;
                }
                Some(In::Control(_)) => {}
                // Game records on a control stream close it.
                Some(In::Data { .. } | In::Ack(_)) | Some(In::Ended(_)) | None => break,
            },
        }
    }
    let ours = {
        let mut st = inner.state();
        let entry = st.slots.iter_mut().find(|s| s.slot == slot);
        match entry {
            Some(entry) if entry.control.as_ref().is_some_and(|c| c.id == control_id) => {
                entry.control = None;
                true
            }
            _ => false,
        }
    };
    if ours && left {
        inner.env.event(HostEvent::Guest(GuestUpdate {
            friend,
            guest,
            name: None,
            status: GuestStatus::Left,
        }));
    }
    inner.wake.notify_one();
}

#[allow(clippy::too_many_arguments)]
async fn serve_game(
    inner: Arc<Inner>,
    slot: Slot,
    path_id: u64,
    kind: PathKind,
    channel: Channel,
    name: String,
    premium: bool,
    proto: i32,
) {
    let lower = name.to_ascii_lowercase();
    let asked = {
        let mut st = inner.state();
        let open = st.open;
        let Some(index) = st.slots.iter().position(|s| s.slot == slot) else {
            return;
        };
        let allowed = inner.slot_admits_stream(&mut st, index);
        let besides = st
            .slots
            .iter()
            .filter(|s| s.slot != slot && s.in_world())
            .count();
        let elsewhere = st
            .slots
            .iter()
            .filter(|s| s.slot != slot)
            .any(|s| s.live_games().any(|g| g.name.to_ascii_lowercase() == lower));
        let entry = &mut st.slots[index];
        // The same guest's live game under this name is in the world already; a suspended one is
        // replaced by this new connection.
        let live_here = entry.live_games().any(|g| {
            g.name.to_ascii_lowercase() == lower && matches!(g.link.state(), LinkState::Live)
        });
        let superseded: Vec<LinkHandle> = entry
            .live_games()
            .filter(|g| matches!(g.link.state(), LinkState::Broken(_)))
            .map(|g| g.link.clone())
            .collect();
        entry.admitting += 1;
        (
            entry.removed,
            open,
            entry.digest.clone(),
            entry.guest,
            entry.friend.clone(),
            elsewhere || live_here,
            besides,
            !allowed,
            superseded,
        )
    };
    let (removed, open, digest, guest, friend, in_world, besides, busy, superseded) = asked;
    let done_admitting = |inner: &Inner| {
        let mut st = inner.state();
        if let Some(entry) = st.slots.iter_mut().find(|s| s.slot == slot) {
            entry.admitting = entry.admitting.saturating_sub(1);
        }
    };
    let bindings = Bindings::read(&inner.world_dir, &inner.key_id).await;
    let taken = bindings.verdict(&name, &digest, &inner.host_name) == NameVerdict::NameTaken;
    let ask = GameAsk {
        slot_removed: removed,
        session_open: open,
        proto,
        host_proto: inner.game.proto,
        name: &name,
        host_name: &inner.host_name,
        name_in_world: in_world,
        name_taken: taken,
        guests_besides: besides,
        max: usize::from(inner.max),
        busy,
    };
    if let Err(reason) = admit_game(&ask) {
        done_admitting(&inner);
        refuse(channel, reason).await;
        return;
    }
    for link in superseded {
        link.abort().await;
    }
    // Step 7: the world on loopback, the identity header first.
    let conn = {
        let mut st = inner.state();
        let conn = st.next_conn;
        st.next_conn += 1;
        conn
    };
    let identity = Identity {
        v: 1,
        id: inner.game.publish_id.clone(),
        conn,
        guest,
        name: name.clone(),
        premium,
        friend: digest,
    };
    let connected = match timeout(Duration::from_secs(5), connect_world(inner.game.port)).await {
        Ok(Ok(mut world)) => match world.write_all(&header(&inner.game.gate, &identity)).await {
            Ok(()) => Some(world),
            Err(_) => None,
        },
        _ => None,
    };
    let Some(world) = connected else {
        done_admitting(&inner);
        refuse(channel, Refusal::Closed).await;
        return;
    };
    let stream_id = StreamId::generate();
    if !channel
        .send(Control::Ok(Accept::game(guest, stream_id)))
        .await
    {
        done_admitting(&inner);
        return;
    }
    let link = link::spawn(world, Vec::new(), channel, LinkConfig::host(&inner.policy));
    let first = {
        let mut st = inner.state();
        let open = st.open;
        let Some(entry) = st.slots.iter_mut().find(|s| s.slot == slot) else {
            return;
        };
        entry.admitting = entry.admitting.saturating_sub(1);
        if entry.removed || !open {
            None
        } else {
            let first = !entry.in_world();
            entry.games.push(GameEntry {
                stream: stream_id,
                link: link.clone(),
                path: path_id,
                kind,
                name: name.clone(),
                broken_at: None,
            });
            entry.name = Some(name.clone());
            Some(first)
        }
    };
    let Some(first) = first else {
        link.abort().await;
        return;
    };
    tracing::info!(guest = %guest, name = %name, "guest in the world");
    if first {
        inner.env.event(HostEvent::Guest(GuestUpdate {
            friend: friend.clone(),
            guest,
            name: Some(name),
            status: GuestStatus::InWorld,
        }));
    }
    inner.count_changed();
    watch_game(inner, slot, stream_id, link, friend, guest).await;
}

/// Follows a game link: notes when it breaks, forgets it when it ends.
async fn watch_game(
    inner: Arc<Inner>,
    slot: Slot,
    stream: StreamId,
    mut link: LinkHandle,
    friend: String,
    guest: GuestId,
) {
    let mut seen = LinkState::Live;
    loop {
        let state = link.changed_from(seen).await;
        match state {
            LinkState::Broken(at) => {
                let mut st = inner.state();
                if let Some(game) = st
                    .slots
                    .iter_mut()
                    .find(|s| s.slot == slot)
                    .and_then(|s| s.games.iter_mut().find(|g| g.stream == stream))
                {
                    game.broken_at = Some(at);
                }
            }
            LinkState::Live => {}
            LinkState::Ended(end) => {
                let (gone, failed_move) = {
                    let mut st = inner.state();
                    let Some(entry) = st.slots.iter_mut().find(|s| s.slot == slot) else {
                        return;
                    };
                    let game = entry
                        .games
                        .iter()
                        .position(|g| g.stream == stream)
                        .map(|i| entry.games.remove(i));
                    let failed_move = match (&game, end) {
                        (Some(game), LinkEnd::Expired) => {
                            let moving = entry.moving.take();
                            let (from, to, why, since) = match moving {
                                Some(m) => (m.from, m.to, MoveCause::from(m.why), m.since),
                                None => (
                                    game.kind,
                                    game.kind,
                                    MoveCause::Failure,
                                    game.broken_at.unwrap_or_else(Instant::now),
                                ),
                            };
                            Some(Event::moved(from, to, why, false, since.elapsed()))
                        }
                        _ => None,
                    };
                    (!entry.removed && !entry.in_world(), failed_move)
                };
                if let Some(event) = failed_move {
                    inner.report(event);
                }
                tracing::info!(guest = %guest, "guest's game connection ended");
                if gone {
                    inner.env.event(HostEvent::Guest(GuestUpdate {
                        friend,
                        guest,
                        name: None,
                        status: GuestStatus::OutOfWorld,
                    }));
                }
                inner.count_changed();
                return;
            }
        }
        seen = state;
    }
}

#[allow(clippy::too_many_arguments)]
async fn serve_resume(
    inner: Arc<Inner>,
    slot: Slot,
    path_id: u64,
    kind: PathKind,
    channel: Channel,
    stream: StreamId,
    recv: u64,
) {
    let found = {
        let mut st = inner.state();
        let open = st.open;
        let Some(index) = st.slots.iter().position(|s| s.slot == slot) else {
            return;
        };
        let allowed = inner.slot_admits_stream(&mut st, index);
        let entry = &st.slots[index];
        if entry.removed {
            Err(Refusal::Removed)
        } else if !open {
            Err(Refusal::Closed)
        } else if !allowed {
            Err(Refusal::Busy)
        } else {
            match entry.live_games().find(|g| g.stream == stream) {
                Some(game) => Ok(game.link.clone()),
                None => Err(Refusal::Resume),
            }
        }
    };
    let link = match found {
        Ok(link) => link,
        Err(reason) => {
            refuse(channel, reason).await;
            return;
        }
    };
    match link.attach(channel, recv, Some(stream)).await {
        Ok(_) => {
            let event = {
                let mut st = inner.state();
                let Some(entry) = st.slots.iter_mut().find(|s| s.slot == slot) else {
                    return;
                };
                let moving = entry.moving;
                let Some(game) = entry.games.iter_mut().find(|g| g.stream == stream) else {
                    return;
                };
                let from = game.kind;
                let broken_at = game.broken_at.take();
                game.path = path_id;
                game.kind = kind;
                let event = match (moving, broken_at) {
                    // A move the host asked for.
                    (Some(m), _) if m.to == kind => {
                        entry.moving = None;
                        Some(Event::moved(
                            from,
                            kind,
                            m.why.into(),
                            true,
                            m.since.elapsed(),
                        ))
                    }
                    // A broken stream the guest resumed.
                    (_, Some(at)) => Some(Event::moved(
                        from,
                        kind,
                        MoveCause::Failure,
                        true,
                        at.elapsed(),
                    )),
                    // The guest moved on its own (its own renewal or `turn_move`): its report.
                    _ => None,
                };
                if event.is_some() {
                    st.moves += 1;
                }
                event
            };
            if let Some(event) = event {
                inner.report(event);
            }
            inner.wake.notify_one();
        }
        Err(Some(channel)) => refuse(channel, Refusal::Resume).await,
        Err(None) => {}
    }
}

/// Tests stand an in-memory world behind a made-up port.
#[cfg(test)]
pub(crate) mod memory_world {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU16, Ordering};

    use tokio::io::DuplexStream;
    use tokio::sync::mpsc;

    static NEXT: AtomicU16 = AtomicU16::new(40_000);
    static WORLDS: Mutex<Vec<(u16, mpsc::UnboundedSender<DuplexStream>)>> = Mutex::new(Vec::new());

    /// A port whose connections arrive on the receiver.
    pub fn listen() -> (u16, mpsc::UnboundedReceiver<DuplexStream>) {
        let port = NEXT.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::unbounded_channel();
        WORLDS.lock().unwrap().push((port, tx));
        (port, rx)
    }

    pub(super) fn connect(port: u16) -> Option<DuplexStream> {
        let worlds = WORLDS.lock().unwrap();
        let (_, tx) = worlds.iter().find(|(p, _)| *p == port)?;
        let (near, far) = tokio::io::duplex(1 << 20);
        tx.send(far).ok()?;
        Some(near)
    }
}
