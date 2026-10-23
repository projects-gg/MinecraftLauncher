//! What `pl-host`'s sessions need from the launcher (`HostEnv`, `GuestEnv`): the friends service's
//! answers over the presence socket and the paths of HOST1-R2 (`transport::{e4mc, turn}`).
//!
//! TURN credentials stay here ([`CredsTable`]): `pl-host` only ever holds the [`Creds`] handle. A
//! guest's TURN allocation is kept with the credentials it was made with and closed before those
//! credentials go back with `turn_release` (PROTOCOL.md §5.5). The host has one QUIC identity per
//! session: every TURN path of the session (renewals too) shows the same pin.

use std::collections::HashMap;
use std::net::SocketAddrV4;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use pl_host::guest::{GuestEnv, GuestEvent};
use pl_host::host::{HostEnv, HostEvent};
use pl_host::policy::env::{
    Creds, CredsAnswer, CredsError, CredsReason, E4mcOpened, GuestPaths, HostPaths,
    TurnAllocation,
};
use pl_host::transport::e4mc::{self, E4mcConfig};
use pl_host::transport::turn::{self, IceServers, QuicIdentity, Secret, TurnConfig, TurnGuest};
use pl_host::transport::{BoxFuture, GuestPath, HostPath, PathError, PathKind, Step};
use pl_host::tunnel::Paths;
use pl_social::{Hidden, TurnIce, WorldAnswer, WorldError, WorldFrame, WorldRole};
use tokio::sync::oneshot;

/// How long a `turn_creds` may take (PROTOCOL.md §12.2: no answer within 4 s is
/// `turn_creds_error`).
pub const CREDS_WAIT: Duration = Duration::from_secs(4);
/// How long a guest's path signal and the host's answer may take (inside a path attempt's 4 s).
pub const PEER_WAIT: Duration = Duration::from_secs(4);
/// Usernames one `turn_held` names at most (the service's rule).
const HELD_PER_FRAME: usize = 4;

/// The friends service as world sessions use it (the launcher's `SocialService`; fakes in tests).
pub trait Service: Send + Sync + 'static {
    /// A frame the service answers by `rid`: its `ack`, or why not, within `wait`.
    fn request(
        &self,
        frame: WorldFrame,
        wait: Duration,
    ) -> BoxFuture<'_, Result<WorldAnswer, WorldError>>;
    /// A frame nobody answers; dropped while disconnected.
    fn send(&self, frame: WorldFrame);
}

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Credentials a session holds.
struct Held {
    ice: TurnIce,
    session: String,
    role: WorldRole,
    /// The host's friend id, for a guest's credentials.
    host: Option<String>,
    /// A guest's allocations made with them.
    allocations: Vec<TurnGuest>,
}

/// Every TURN credential this launcher holds, by the handle `pl-host` was given.
#[derive(Default)]
pub struct CredsTable {
    next: AtomicU64,
    held: Mutex<HashMap<u64, Held>>,
}

impl CredsTable {
    fn add(&self, ice: TurnIce, session: &str, role: WorldRole, host: Option<&str>) -> Creds {
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let ttl = Duration::from_secs(ice.ttl);
        lock(&self.held).insert(
            id,
            Held {
                ice,
                session: session.to_owned(),
                role,
                host: host.map(str::to_owned),
                allocations: Vec::new(),
            },
        );
        Creds { id, ttl }
    }

    fn ice(&self, id: u64) -> Option<IceServers> {
        lock(&self.held).get(&id).map(|held| {
            IceServers::new(
                held.ice.urls.clone(),
                Secret::new(held.ice.username.expose().clone()),
                Secret::new(held.ice.credential.expose().clone()),
                held.ice.ttl,
            )
        })
    }

    fn keep_allocation(&self, id: u64, allocation: TurnGuest) {
        if let Some(held) = lock(&self.held).get_mut(&id) {
            held.allocations.push(allocation);
        }
    }

    fn take(&self, id: u64) -> Option<Held> {
        lock(&self.held).remove(&id)
    }

    /// How many credentials are held (tests, and the session report's sanity).
    pub fn len(&self) -> usize {
        lock(&self.held).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// After the presence socket came back: one `turn_held` per session and role (at most four
    /// usernames each) for the credentials still in use (PROTOCOL.md §3.3).
    pub fn held_frames(&self) -> Vec<WorldFrame> {
        let held = lock(&self.held);
        let mut groups: Vec<(String, WorldRole, Option<String>, Vec<Hidden<String>>)> = Vec::new();
        let mut ids: Vec<&u64> = held.keys().collect();
        ids.sort();
        for id in ids {
            let entry = &held[id];
            let username = Hidden(entry.ice.username.expose().clone());
            match groups.iter_mut().find(|(session, role, host, names)| {
                *session == entry.session
                    && *role == entry.role
                    && *host == entry.host
                    && names.len() < HELD_PER_FRAME
            }) {
                Some((_, _, _, names)) => names.push(username),
                None => groups.push((
                    entry.session.clone(),
                    entry.role,
                    entry.host.clone(),
                    vec![username],
                )),
            }
        }
        groups
            .into_iter()
            .map(|(session, role, host, usernames)| WorldFrame::TurnHeld {
                session,
                role,
                host,
                usernames,
            })
            .collect()
    }

    /// Gives back the credentials and closes what was made with them: allocations first, then
    /// `turn_release` (the service revokes what this socket holds).
    fn release(&self, id: u64, service: &Arc<dyn Service>) {
        let Some(held) = self.take(id) else {
            return;
        };
        let frame = WorldFrame::TurnRelease {
            username: Hidden(held.ice.username.expose().clone()),
        };
        if held.allocations.is_empty() {
            service.send(frame);
            return;
        }
        let service = Arc::clone(service);
        let allocations = held.allocations;
        let go = async move {
            for allocation in &allocations {
                allocation.close().await;
            }
            service.send(frame);
        };
        match tokio::runtime::Handle::try_current() {
            Ok(runtime) => {
                runtime.spawn(go);
            }
            // Outside a runtime the allocations go with their last handle (dropped here).
            Err(_) => drop(go),
        }
    }
}

fn path_kind(name: &str) -> Option<PathKind> {
    match name {
        "turn" => Some(PathKind::Turn),
        "e4mc" => Some(PathKind::E4mc),
        _ => None,
    }
}

/// `turn_creds` for either side and its answer as `pl-host` reads it; the e4mc region too.
async fn ask_creds(
    service: &Arc<dyn Service>,
    table: &CredsTable,
    session: &str,
    role: WorldRole,
    host: Option<&str>,
    reason: CredsReason,
) -> Result<(CredsAnswer, Option<String>), CredsError> {
    let frame = WorldFrame::TurnCreds {
        session: session.to_owned(),
        host: host.map(str::to_owned),
        role,
        reason: reason.as_str(),
    };
    match service.request(frame, CREDS_WAIT).await {
        Ok(answer) => {
            let order = answer
                .order
                .iter()
                .filter_map(|kind| path_kind(kind))
                .collect();
            let creds = answer
                .ice
                .map(|ice| table.add(ice, session, role, host));
            let region = answer.e4mc.map(|hint| hint.region);
            Ok((CredsAnswer { order, creds }, region))
        }
        Err(WorldError::Refused { code, reason, .. }) => Err(match code.as_str() {
            "turn_denied" => CredsError::Denied(reason.unwrap_or_default()),
            // The invite or the session is not valid any more.
            "not_hosting" | "worlds_off" | "friend_not_found" => CredsError::Gone,
            // `turn_failed` (Cloudflare did not answer), a rate, the service's own trouble.
            _ => CredsError::Failed,
        }),
        Err(WorldError::Off) => Err(CredsError::Gone),
        Err(WorldError::Unavailable) => Err(CredsError::Failed),
    }
}

fn creds_lost() -> PathError {
    PathError {
        step: Step::Creds,
        code: "turn_creds_error".into(),
    }
}

/// The transports' settings: the protocol's numbers, the launcher's version in the e4mc broker
/// request.
#[derive(Debug, Clone)]
pub struct Transports {
    pub e4mc: E4mcConfig,
    pub turn: TurnConfig,
}

impl Transports {
    pub fn new(launcher_version: &str) -> Self {
        Self {
            e4mc: E4mcConfig::new(launcher_version),
            turn: TurnConfig::default(),
        }
    }
}

/// The host launcher's side of one session.
pub struct HostLink {
    service: Arc<dyn Service>,
    table: Arc<CredsTable>,
    session: String,
    transports: Transports,
    /// The session's QUIC identity, made with the first TURN path (PROTOCOL.md §2).
    identity: Mutex<Option<QuicIdentity>>,
    /// Studio's e4mc region from the last `turn_creds` answer that named one.
    region: Mutex<String>,
    events: Box<dyn Fn(HostEvent) + Send + Sync>,
}

impl HostLink {
    pub fn new(
        service: Arc<dyn Service>,
        table: Arc<CredsTable>,
        session: String,
        transports: Transports,
        events: impl Fn(HostEvent) + Send + Sync + 'static,
    ) -> Self {
        Self {
            service,
            table,
            session,
            transports,
            identity: Mutex::new(None),
            region: Mutex::new("auto".to_owned()),
            events: Box::new(events),
        }
    }

    fn identity(&self) -> Result<QuicIdentity, PathError> {
        let mut identity = lock(&self.identity);
        if let Some(identity) = identity.as_ref() {
            return Ok(identity.clone());
        }
        let made = QuicIdentity::generate()?;
        *identity = Some(made.clone());
        Ok(made)
    }
}

impl HostPaths for HostLink {
    fn turn_creds(&self, reason: CredsReason) -> BoxFuture<'_, Result<CredsAnswer, CredsError>> {
        Box::pin(async move {
            let (answer, region) = ask_creds(
                &self.service,
                &self.table,
                &self.session,
                WorldRole::Host,
                None,
                reason,
            )
            .await?;
            if let Some(region) = region.filter(|region| !region.is_empty()) {
                *lock(&self.region) = region;
            }
            Ok(answer)
        })
    }

    fn turn_release(&self, creds: Creds) {
        self.table.release(creds.id, &self.service);
    }

    fn turn_open(&self, creds: Creds) -> BoxFuture<'_, Result<Arc<dyn HostPath>, PathError>> {
        Box::pin(async move {
            let ice = self.table.ice(creds.id).ok_or_else(creds_lost)?;
            let identity = self.identity()?;
            let path = turn::open_host(&ice, &identity, &self.transports.turn).await?;
            Ok(Arc::new(path) as Arc<dyn HostPath>)
        })
    }

    fn e4mc_open(&self) -> BoxFuture<'_, E4mcOpened> {
        Box::pin(async move {
            let region = lock(&self.region).clone();
            let opening = e4mc::open_host(&self.transports.e4mc, &region).await;
            E4mcOpened {
                result: opening
                    .result
                    .map(|host| Arc::new(host) as Arc<dyn HostPath>),
                region: opening.region,
                broker_failure: opening.broker_error.map(|error| error.code),
            }
        })
    }
}

impl HostEnv for HostLink {
    fn event(&self, event: HostEvent) {
        (self.events)(event);
    }
}

/// Guests' path signals waiting for the host's `world_peer_ok`, by session and slot.
#[derive(Default)]
pub struct PeerWaiters {
    waiting: Mutex<HashMap<(String, String), oneshot::Sender<Paths>>>,
}

impl PeerWaiters {
    fn wait(&self, session: &str, slot: &str) -> oneshot::Receiver<Paths> {
        let (tx, rx) = oneshot::channel();
        lock(&self.waiting).insert((session.to_owned(), slot.to_owned()), tx);
        rx
    }

    fn forget(&self, session: &str, slot: &str) {
        lock(&self.waiting).remove(&(session.to_owned(), slot.to_owned()));
    }

    /// The host's answer came (`world_peer_ok`); false when nobody waited for it.
    pub fn deliver(&self, session: &str, slot: &str, paths: Paths) -> bool {
        lock(&self.waiting)
            .remove(&(session.to_owned(), slot.to_owned()))
            .is_some_and(|tx| tx.send(paths).is_ok())
    }
}

/// A guest's TURN allocation for `pl-host`.
struct GuestAllocation(TurnGuest);

impl TurnAllocation for GuestAllocation {
    fn relay(&self) -> SocketAddrV4 {
        self.0.relay()
    }

    fn tcp_only(&self) -> bool {
        self.0.tcp_only()
    }

    fn connect(
        &self,
        host_relay: SocketAddrV4,
        pin: [u8; 32],
    ) -> BoxFuture<'_, Result<Arc<dyn GuestPath>, PathError>> {
        Box::pin(async move {
            self.0
                .connect(host_relay, pin)
                .await
                .map(|path| Arc::new(path) as Arc<dyn GuestPath>)
        })
    }
}

/// The guest launcher's side of one invite.
pub struct GuestLink {
    service: Arc<dyn Service>,
    table: Arc<CredsTable>,
    peers: Arc<PeerWaiters>,
    session: String,
    /// The host's friend id.
    host: String,
    slot: String,
    transports: Transports,
    events: Box<dyn Fn(GuestEvent) + Send + Sync>,
}

impl GuestLink {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        service: Arc<dyn Service>,
        table: Arc<CredsTable>,
        peers: Arc<PeerWaiters>,
        session: String,
        host: String,
        slot: String,
        transports: Transports,
        events: impl Fn(GuestEvent) + Send + Sync + 'static,
    ) -> Self {
        Self {
            service,
            table,
            peers,
            session,
            host,
            slot,
            transports,
            events: Box::new(events),
        }
    }
}

impl GuestPaths for GuestLink {
    fn turn_creds(&self, reason: CredsReason) -> BoxFuture<'_, Result<CredsAnswer, CredsError>> {
        Box::pin(async move {
            ask_creds(
                &self.service,
                &self.table,
                &self.session,
                WorldRole::Guest,
                Some(&self.host),
                reason,
            )
            .await
            .map(|(answer, _)| answer)
        })
    }

    fn turn_release(&self, creds: Creds) {
        self.table.release(creds.id, &self.service);
    }

    fn world_peer(
        &self,
        path: PathKind,
        relay: Option<SocketAddrV4>,
    ) -> BoxFuture<'_, Option<Paths>> {
        Box::pin(async move {
            let answer = self.peers.wait(&self.session, &self.slot);
            let frame = WorldFrame::WorldPeer {
                session: self.session.clone(),
                host: self.host.clone(),
                slot: self.slot.clone(),
                path: path.as_str(),
                relay: relay.map(|relay| Hidden(relay.to_string())),
            };
            match self.service.request(frame, PEER_WAIT).await {
                Ok(ack) if ack.delivered != Some(false) => {}
                _ => {
                    self.peers.forget(&self.session, &self.slot);
                    return None;
                }
            }
            match tokio::time::timeout(PEER_WAIT, answer).await {
                Ok(Ok(paths)) => Some(paths),
                _ => {
                    self.peers.forget(&self.session, &self.slot);
                    None
                }
            }
        })
    }

    fn turn_allocate(
        &self,
        creds: Creds,
    ) -> BoxFuture<'_, Result<Arc<dyn TurnAllocation>, PathError>> {
        Box::pin(async move {
            let ice = self.table.ice(creds.id).ok_or_else(creds_lost)?;
            let allocation = turn::allocate(&ice, &self.transports.turn).await?;
            self.table.keep_allocation(creds.id, allocation.clone());
            Ok(Arc::new(GuestAllocation(allocation)) as Arc<dyn TurnAllocation>)
        })
    }

    fn e4mc_connect(
        &self,
        domain: &str,
        proto: i32,
    ) -> BoxFuture<'_, Result<Arc<dyn GuestPath>, PathError>> {
        let path = e4mc::guest(domain, proto, &self.transports.e4mc);
        Box::pin(async move { path.map(|guest| Arc::new(guest) as Arc<dyn GuestPath>) })
    }
}

impl GuestEnv for GuestLink {
    fn event(&self, event: GuestEvent) {
        (self.events)(event);
    }
}
