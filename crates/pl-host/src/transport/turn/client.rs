//! One connection to a TURN server and the allocation on it (RFC 8656 over RFC 8489): the
//! transport, transactions (retransmitted over UDP), long-term credentials, refreshes, permissions
//! and channels. Datagrams from admitted peers go to the QUIC socket (`socket.rs`); nothing else
//! does.

use std::collections::HashMap;
use std::io;
use std::net::{SocketAddr, SocketAddrV4};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, Ordering};
use std::sync::{
    Arc, Mutex, MutexGuard, OnceLock, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard,
};
use std::task::{Context, Poll};
use std::time::Duration;

use bytes::{Bytes, BytesMut};
use rustls::pki_types::ServerName;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::{Notify, mpsc, oneshot, watch};
use tokio::time::{Instant, sleep_until, timeout_at};
use tokio_rustls::TlsConnector;
use zeroize::Zeroizing;

use super::stun::{self, Builder, Class, attr, method};
use super::{IceServers, TurnConfig};
use crate::transport::net::{Secret, Tasks, client_tls, fail, local_for};
use crate::transport::{PathError, Step};

/// The first retransmission of a request over UDP; it doubles up to `MAX_RTO`.
const INITIAL_RTO: Duration = Duration::from_millis(250);
const MAX_RTO: Duration = Duration::from_secs(2);
/// A failed refresh is tried again this soon (while the allocation lasts).
const REFRESH_RETRY: Duration = Duration::from_secs(5);
/// A failed permission or channel refresh is tried again this soon.
const PEER_RETRY: Duration = Duration::from_secs(10);
/// Deleting an allocation waits this long for the server's answer.
const DELETE_WAIT: Duration = Duration::from_secs(1);
/// A forgotten peer still gets what is sent to it this long (QUIC's goodbye), never anything back.
const FORGET_LINGER: Duration = Duration::from_millis(300);
/// Datagrams waiting for the QUIC socket, and frames waiting for a TCP or TLS stream; more are
/// dropped like on a congested network.
const DATAGRAM_QUEUE: usize = 1024;
const STREAM_QUEUE: usize = 1024;
/// UDP over IPv4 or IPv6 carries at most this much.
const MAX_DATAGRAM: usize = 65_536;

/// How the client reaches the TURN server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Proto {
    Udp,
    Tcp,
    Tls,
}

impl Proto {
    pub(crate) fn is_stream(self) -> bool {
        self != Self::Udp
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Udp => "udp",
            Self::Tcp => "tcp",
            Self::Tls => "tls",
        }
    }
}

/// A TURN server from `ice.urls`.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Server {
    pub(crate) proto: Proto,
    pub(crate) host: String,
    pub(crate) port: u16,
}

/// Why a request or an attempt failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fail {
    /// No answer in time.
    Timeout,
    /// The credentials were refused (401 after the challenge, 441).
    Unauthorized,
    /// The transport is gone (or refused, or the connection could not be made).
    Closed,
    /// Another error response.
    Code(u16),
    /// Something that is not TURN, or a response that does not check out.
    Bad,
}

/// The timers of one allocation.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Timers {
    pub(crate) lifetime: Duration,
    pub(crate) refresh_when_left: Duration,
    pub(crate) permission_refresh: Duration,
    pub(crate) channel_refresh: Duration,
    pub(crate) request_timeout: Duration,
}

impl Timers {
    pub(crate) fn of(config: &TurnConfig) -> Self {
        Self {
            lifetime: config.allocation_lifetime,
            refresh_when_left: config.refresh_when_left,
            permission_refresh: config.permission_refresh,
            channel_refresh: config.channel_refresh,
            request_timeout: config.request_timeout,
        }
    }
}

/// A datagram a peer sent through the allocation.
pub(crate) struct Datagram {
    pub(crate) from: SocketAddrV4,
    pub(crate) data: Bytes,
}

enum Wire {
    Udp(Arc<UdpSocket>),
    Stream {
        control: mpsc::UnboundedSender<Bytes>,
        data: mpsc::Sender<Bytes>,
    },
}

struct Peer {
    channel: u16,
    active: bool,
    /// A forgotten peer can still be sent to until then (QUIC's close frames go out).
    linger_until: Option<Instant>,
    permission_due: Instant,
    channel_due: Instant,
}

#[derive(Default)]
struct Peers {
    by_address: HashMap<SocketAddrV4, Peer>,
    by_channel: HashMap<u16, SocketAddrV4>,
}

impl Peers {
    fn active(&self, address: SocketAddrV4) -> Option<&Peer> {
        self.by_address.get(&address).filter(|peer| peer.active)
    }
}

/// Long-term credentials and what the server's challenge added (RFC 8489 §9.2).
struct Auth {
    username: Secret,
    password: Secret,
    realm: Option<String>,
    nonce: Option<Vec<u8>>,
    key: Option<Zeroizing<[u8; 16]>>,
    algorithms: Option<Vec<u8>>,
}

impl Auth {
    /// Adds the credential attributes once the server has challenged; returns the key to sign with.
    fn sign(&self, builder: &mut Builder) -> Option<Zeroizing<[u8; 16]>> {
        let (realm, nonce, key) = (
            self.realm.as_ref()?,
            self.nonce.as_ref()?,
            self.key.as_ref()?,
        );
        builder
            .attr(attr::USERNAME, self.username.expose().as_bytes())
            .attr(attr::REALM, realm.as_bytes())
            .attr(attr::NONCE, nonce);
        if let Some(algorithms) = &self.algorithms {
            builder
                .attr(attr::PASSWORD_ALGORITHMS, algorithms)
                .attr(attr::PASSWORD_ALGORITHM, &stun::PASSWORD_ALGORITHM_MD5);
        }
        Some(key.clone())
    }

    /// Takes the realm and nonce of a 401 or 438; whether a retry makes sense.
    fn challenge(&mut self, message: &stun::Message<'_>) -> bool {
        if let Some(realm) = message
            .get(attr::REALM)
            .and_then(|realm| std::str::from_utf8(realm).ok())
        {
            self.realm = Some(realm.to_owned());
        }
        let (Some(nonce), Some(realm)) = (message.get(attr::NONCE), self.realm.as_deref()) else {
            return false;
        };
        // RFC 8489 §9.2.4: a server that lists its password algorithms wants the list echoed with
        // the one picked (MD5, the long-term mechanism every TURN server speaks).
        self.algorithms = message
            .get(attr::PASSWORD_ALGORITHMS)
            .filter(|list| stun::nonce_has_cookie(nonce) && stun::offers_md5(list))
            .map(<[u8]>::to_vec);
        self.key = Some(stun::long_term_key(
            self.username.expose(),
            realm,
            self.password.expose(),
        ));
        self.nonce = Some(nonce.to_vec());
        true
    }
}

/// A response to a request (validated).
pub(crate) struct Response(Vec<u8>);

impl Response {
    fn message(&self) -> Option<stun::Message<'_>> {
        stun::parse(&self.0)
    }

    fn u32(&self, kind: u16) -> Option<u32> {
        self.message()?.u32(kind)
    }

    fn xor_address(&self, kind: u16) -> Option<SocketAddr> {
        self.message()?.xor_address(kind)
    }
}

enum Outcome {
    Done,
    Retry,
    Failed(Fail),
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn read<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(PoisonError::into_inner)
}

fn write<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    lock.write().unwrap_or_else(PoisonError::into_inner)
}

fn transaction_id() -> [u8; 12] {
    let mut txid = [0u8; 12];
    getrandom::fill(&mut txid).expect("the operating system's random source works");
    txid
}

/// Everything one TURN connection shares with its background tasks and the QUIC socket.
pub(crate) struct Shared {
    proto: Proto,
    wire: Wire,
    timers: Timers,
    pending: Mutex<HashMap<[u8; 12], oneshot::Sender<Vec<u8>>>>,
    auth: Mutex<Auth>,
    peers: RwLock<Peers>,
    next_channel: AtomicU16,
    datagrams: mpsc::Sender<Datagram>,
    datagrams_out: Mutex<Option<mpsc::Receiver<Datagram>>>,
    rx_bytes: Arc<AtomicU64>,
    relay: OnceLock<SocketAddrV4>,
    dead: watch::Sender<Option<PathError>>,
    wake: Notify,
    probe: AtomicBool,
    closing: AtomicBool,
    tasks: Tasks,
}

impl Shared {
    fn new(
        proto: Proto,
        wire: Wire,
        ice: &IceServers,
        timers: Timers,
        rx_bytes: Arc<AtomicU64>,
    ) -> Arc<Self> {
        let (datagrams, datagrams_out) = mpsc::channel(DATAGRAM_QUEUE);
        Arc::new(Self {
            proto,
            wire,
            timers,
            pending: Mutex::new(HashMap::new()),
            auth: Mutex::new(Auth {
                username: ice.username.clone(),
                password: ice.credential.clone(),
                realm: None,
                nonce: None,
                key: None,
                algorithms: None,
            }),
            peers: RwLock::new(Peers::default()),
            next_channel: AtomicU16::new(0),
            datagrams,
            datagrams_out: Mutex::new(Some(datagrams_out)),
            rx_bytes,
            relay: OnceLock::new(),
            dead: watch::channel(None).0,
            wake: Notify::new(),
            probe: AtomicBool::new(false),
            closing: AtomicBool::new(false),
            tasks: Tasks::default(),
        })
    }

    fn spawn(&self, task: impl Future<Output = ()> + Send + 'static) {
        self.tasks.add(tokio::spawn(task).abort_handle());
    }

    pub(crate) fn proto(&self) -> Proto {
        self.proto
    }

    pub(crate) fn rx_bytes(&self) -> u64 {
        self.rx_bytes.load(Ordering::Relaxed)
    }

    /// The datagram queue for the QUIC socket (once).
    pub(crate) fn take_datagrams(&self) -> Option<mpsc::Receiver<Datagram>> {
        lock(&self.datagrams_out).take()
    }

    /// Ends the allocation's life as far as its users are concerned.
    fn die(&self, error: PathError) {
        let first = self.dead.send_if_modified(|state| {
            if state.is_some() {
                return false;
            }
            *state = Some(error.clone());
            true
        });
        if first && !self.closing.load(Ordering::Acquire) {
            tracing::info!(path = "turn", step = error.step.as_str(), code = %error.code, "TURN allocation lost");
        }
        self.wake.notify_one();
    }

    fn transport_lost(&self) {
        lock(&self.pending).clear();
        self.die(fail(Step::Alloc, "turn_alloc_error"));
    }

    pub(crate) fn dead(&self) -> watch::Receiver<Option<PathError>> {
        self.dead.subscribe()
    }

    /// What arrived from the server: a response, a peer's datagram, or nothing we want.
    fn on_frame(&self, frame: Bytes) {
        let Some(&first) = frame.first() else {
            return;
        };
        if stun::is_channel_data(first) {
            let Some((channel, payload)) = stun::parse_channel_data(&frame) else {
                return;
            };
            let from = {
                let peers = read(&self.peers);
                peers
                    .by_channel
                    .get(&channel)
                    .copied()
                    .filter(|address| peers.active(*address).is_some())
            };
            if let Some(from) = from {
                let _ = self.datagrams.try_send(Datagram {
                    from,
                    data: frame.slice(payload),
                });
            }
            return;
        }
        let Some(message) = stun::parse(&frame) else {
            return;
        };
        match message.class {
            Class::Success | Class::Error => {
                if let Some(waiting) = lock(&self.pending).remove(&message.txid) {
                    let _ = waiting.send(frame.to_vec());
                }
            }
            Class::Indication if message.method == method::DATA => {
                let (Some(SocketAddr::V4(from)), Some(data)) = (
                    message.xor_address(attr::XOR_PEER_ADDRESS),
                    message.get(attr::DATA),
                ) else {
                    return;
                };
                if read(&self.peers).active(from).is_some() {
                    let _ = self.datagrams.try_send(Datagram {
                        from,
                        data: Bytes::copy_from_slice(data),
                    });
                }
            }
            _ => {}
        }
    }

    /// Sends one request and waits for its response; over UDP the request is sent again with a
    /// doubling timeout until `deadline`.
    async fn transact(
        &self,
        request: Vec<u8>,
        txid: [u8; 12],
        deadline: Instant,
    ) -> Result<Vec<u8>, Fail> {
        struct Entry<'a>(&'a Shared, [u8; 12]);
        impl Drop for Entry<'_> {
            fn drop(&mut self) {
                lock(&self.0.pending).remove(&self.1);
            }
        }
        let (answer, mut waiting) = oneshot::channel();
        lock(&self.pending).insert(txid, answer);
        let _entry = Entry(self, txid);
        match &self.wire {
            Wire::Udp(socket) => {
                let mut rto = INITIAL_RTO;
                loop {
                    if let Err(error) = socket.send(&request).await
                        && matches!(
                            error.kind(),
                            io::ErrorKind::ConnectionReset | io::ErrorKind::ConnectionRefused
                        )
                    {
                        return Err(Fail::Closed);
                    }
                    let wait = (Instant::now() + rto).min(deadline);
                    match timeout_at(wait, &mut waiting).await {
                        Ok(Ok(response)) => return Ok(response),
                        Ok(Err(_)) => return Err(Fail::Closed),
                        Err(_) if Instant::now() >= deadline => return Err(Fail::Timeout),
                        Err(_) => rto = (rto * 2).min(MAX_RTO),
                    }
                }
            }
            Wire::Stream { control, .. } => {
                control
                    .send(Bytes::from(request))
                    .map_err(|_| Fail::Closed)?;
                match timeout_at(deadline, waiting).await {
                    Ok(Ok(response)) => Ok(response),
                    Ok(Err(_)) => Err(Fail::Closed),
                    Err(_) => Err(Fail::Timeout),
                }
            }
        }
    }

    /// One request with long-term credentials: the first goes out bare and the server's 401
    /// challenge (realm, nonce) signs the rest; a stale nonce (438) is retried with the new one.
    async fn request(
        &self,
        method: u16,
        add: impl Fn(&mut Builder),
        deadline: Instant,
    ) -> Result<Response, Fail> {
        let mut challenged = false;
        for _ in 0..4 {
            let txid = transaction_id();
            let mut builder = Builder::new(method, Class::Request, txid);
            add(&mut builder);
            let key = lock(&self.auth).sign(&mut builder);
            if let Some(key) = &key {
                builder.integrity(key.as_ref());
            }
            builder.fingerprint();
            let raw = self.transact(builder.finish(), txid, deadline).await?;
            let outcome = match stun::parse(&raw) {
                Some(message) if message.method == method && message.txid == txid => {
                    match message.class {
                        Class::Success => match &key {
                            // A response that names our key must carry its proof.
                            Some(key) if message.check_integrity(key.as_ref()) == Some(false) => {
                                Outcome::Failed(Fail::Bad)
                            }
                            _ => Outcome::Done,
                        },
                        Class::Error => {
                            let code = message.error_code().unwrap_or(0);
                            let retry = match code {
                                401 if !challenged => {
                                    challenged = true;
                                    lock(&self.auth).challenge(&message)
                                }
                                438 => lock(&self.auth).challenge(&message),
                                _ => false,
                            };
                            match (retry, code) {
                                (true, _) => Outcome::Retry,
                                (false, 401 | 441) => Outcome::Failed(Fail::Unauthorized),
                                (false, code) => Outcome::Failed(Fail::Code(code)),
                            }
                        }
                        _ => Outcome::Failed(Fail::Bad),
                    }
                }
                _ => Outcome::Failed(Fail::Bad),
            };
            match outcome {
                Outcome::Done => return Ok(Response(raw)),
                Outcome::Retry => {}
                Outcome::Failed(fail) => return Err(fail),
            }
        }
        Err(Fail::Bad)
    }

    async fn refresh(&self, lifetime: Duration) -> Result<Duration, Fail> {
        let seconds = u32::try_from(lifetime.as_secs()).unwrap_or(u32::MAX);
        let deadline = Instant::now() + self.timers.request_timeout;
        let response = self
            .request(
                method::REFRESH,
                |builder| {
                    builder.u32(attr::LIFETIME, seconds);
                },
                deadline,
            )
            .await?;
        Ok(response
            .u32(attr::LIFETIME)
            .map_or(lifetime, |granted| Duration::from_secs(u64::from(granted))))
    }

    async fn create_permission(&self, peer: SocketAddrV4, deadline: Instant) -> Result<(), Fail> {
        self.request(
            method::CREATE_PERMISSION,
            |builder| {
                builder.xor_address(attr::XOR_PEER_ADDRESS, SocketAddr::V4(peer));
            },
            deadline,
        )
        .await
        .map(drop)
    }

    async fn channel_bind(
        &self,
        channel: u16,
        peer: SocketAddrV4,
        deadline: Instant,
    ) -> Result<(), Fail> {
        self.request(
            method::CHANNEL_BIND,
            |builder| {
                builder
                    .u32(attr::CHANNEL_NUMBER, u32::from(channel) << 16)
                    .xor_address(attr::XOR_PEER_ADDRESS, SocketAddr::V4(peer));
            },
            deadline,
        )
        .await
        .map(drop)
    }

    /// Deletes the allocation (a refresh with lifetime 0), waiting at most `wait` for the answer.
    async fn delete(&self, wait: Duration) {
        if self.relay.get().is_none() || self.dead.borrow().is_some() {
            return;
        }
        let _ = self
            .request(
                method::REFRESH,
                |builder| {
                    builder.u32(attr::LIFETIME, 0);
                },
                Instant::now() + wait,
            )
            .await;
    }

    /// Stops everything: background tasks, waiting requests; `closed()` resolves.
    fn shutdown(&self) {
        self.closing.store(true, Ordering::Release);
        self.die(fail(Step::Alloc, "turn_alloc_error"));
        self.tasks.abort_all();
        lock(&self.pending).clear();
    }

    fn channel_for(&self, peer: SocketAddrV4) -> u16 {
        if let Some(known) = read(&self.peers).by_address.get(&peer) {
            return known.channel;
        }
        0x4000 + self.next_channel.fetch_add(1, Ordering::Relaxed) % 0x1000
    }

    /// The channel to an admitted peer (or one forgotten a moment ago).
    fn channel_of(&self, peer: SocketAddrV4) -> Option<u16> {
        read(&self.peers)
            .by_address
            .get(&peer)
            .filter(|peer| {
                peer.active
                    || peer
                        .linger_until
                        .is_some_and(|until| Instant::now() < until)
            })
            .map(|peer| peer.channel)
    }

    /// Sends one datagram to an admitted peer through its channel. Other destinations, and frames a
    /// full TCP queue cannot take, are dropped like lost datagrams; only a full UDP socket pushes
    /// back.
    pub(crate) fn send_datagram(&self, to: SocketAddrV4, payload: &[u8]) -> io::Result<()> {
        let Some(channel) = self.channel_of(to) else {
            return Ok(());
        };
        let frame = stun::channel_data(channel, payload, self.proto.is_stream());
        match &self.wire {
            Wire::Udp(socket) => match socket.try_send(&frame) {
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => Err(error),
                _ => Ok(()),
            },
            Wire::Stream { data, .. } => {
                let _ = data.try_send(Bytes::from(frame));
                Ok(())
            }
        }
    }

    pub(crate) fn writable_socket(&self) -> Option<Arc<UdpSocket>> {
        match &self.wire {
            Wire::Udp(socket) => Some(socket.clone()),
            Wire::Stream { .. } => None,
        }
    }

    fn next_peer_due(&self) -> Option<Instant> {
        read(&self.peers)
            .by_address
            .values()
            .filter(|peer| peer.active)
            .map(|peer| peer.permission_due.min(peer.channel_due))
            .min()
    }

    async fn refresh_peers(&self, now: Instant) {
        let due: Vec<(SocketAddrV4, u16, bool, bool)> = read(&self.peers)
            .by_address
            .iter()
            .filter(|(_, peer)| peer.active)
            .filter_map(|(address, peer)| {
                let permission = peer.permission_due <= now;
                let channel = peer.channel_due <= now;
                (permission || channel).then_some((*address, peer.channel, permission, channel))
            })
            .collect();
        for (address, channel, permission, bind) in due {
            let deadline = Instant::now() + self.timers.request_timeout;
            if permission {
                let next = match self.create_permission(address, deadline).await {
                    Ok(()) => now + self.timers.permission_refresh,
                    Err(_) => now + PEER_RETRY,
                };
                if let Some(peer) = write(&self.peers).by_address.get_mut(&address) {
                    peer.permission_due = next;
                }
            }
            if bind {
                let next = match self.channel_bind(channel, address, deadline).await {
                    Ok(()) => now + self.timers.channel_refresh,
                    Err(_) => now + PEER_RETRY,
                };
                if let Some(peer) = write(&self.peers).by_address.get_mut(&address) {
                    peer.channel_due = next;
                }
            }
        }
    }
}

/// Keeps the allocation alive: refreshes before its lifetime runs out, and keeps every admitted
/// peer's permission and channel. A refresh the server refuses ends the allocation.
async fn maintain(shared: Arc<Shared>, granted: Duration) {
    let timers = shared.timers;
    let mut expires = Instant::now() + granted;
    let mut refresh_at = Instant::now() + refresh_delay(granted, timers.refresh_when_left);
    loop {
        let next = shared
            .next_peer_due()
            .map_or(refresh_at, |due| due.min(refresh_at));
        tokio::select! {
            () = sleep_until(next) => {}
            () = shared.wake.notified() => {}
        }
        if shared.dead.borrow().is_some() {
            return;
        }
        let now = Instant::now();
        if now >= refresh_at || shared.probe.swap(false, Ordering::AcqRel) {
            match shared.refresh(timers.lifetime).await {
                Ok(lifetime) => {
                    expires = now + lifetime;
                    refresh_at = now + refresh_delay(lifetime, timers.refresh_when_left);
                }
                Err(Fail::Unauthorized) => {
                    return shared.die(fail(Step::Alloc, "turn_alloc_unauthorized"));
                }
                Err(Fail::Code(437) | Fail::Closed) => {
                    return shared.die(fail(Step::Alloc, "turn_alloc_error"));
                }
                Err(_) if now + REFRESH_RETRY >= expires => {
                    return shared.die(fail(Step::Alloc, "turn_alloc_timeout"));
                }
                Err(_) => refresh_at = now + REFRESH_RETRY,
            }
        }
        shared.refresh_peers(now).await;
    }
}

fn refresh_delay(lifetime: Duration, when_left: Duration) -> Duration {
    lifetime
        .saturating_sub(when_left)
        .max(Duration::from_secs(1))
}

async fn read_udp(shared: Arc<Shared>, socket: Arc<UdpSocket>) {
    let mut buf = vec![0u8; MAX_DATAGRAM];
    loop {
        match socket.recv(&mut buf).await {
            Ok(len) => {
                shared.rx_bytes.fetch_add(len as u64, Ordering::Relaxed);
                shared.on_frame(Bytes::copy_from_slice(&buf[..len]));
            }
            // ICMP unreachable from the server's address: before the allocation, a refusal.
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::ConnectionReset | io::ErrorKind::ConnectionRefused
                ) =>
            {
                if shared.relay.get().is_none() {
                    lock(&shared.pending).clear();
                }
            }
            Err(_) => return shared.transport_lost(),
        }
    }
}

async fn read_stream(shared: Arc<Shared>, mut reader: Box<dyn AsyncRead + Send + Unpin>) {
    let mut buf = BytesMut::with_capacity(16 * 1024);
    loop {
        while buf.len() >= 4 {
            let Some(len) = stun::frame_len([buf[0], buf[1], buf[2], buf[3]]) else {
                return shared.transport_lost();
            };
            if buf.len() < len {
                break;
            }
            let frame = buf.split_to(len).freeze();
            shared.on_frame(frame);
        }
        buf.reserve(16 * 1024);
        match reader.read_buf(&mut buf).await {
            Ok(0) | Err(_) => return shared.transport_lost(),
            Ok(_) => {}
        }
    }
}

/// Writes queued frames (requests first), as many as are waiting in one write (one TLS record
/// instead of one per datagram).
async fn write_stream(
    mut writer: Box<dyn AsyncWrite + Send + Unpin>,
    mut control: mpsc::UnboundedReceiver<Bytes>,
    mut data: mpsc::Receiver<Bytes>,
) {
    const BATCH: usize = 64 * 1024;
    let mut batch = BytesMut::with_capacity(BATCH);
    loop {
        let first = tokio::select! {
            biased;
            frame = control.recv() => frame,
            frame = data.recv() => frame,
        };
        let Some(first) = first else {
            break;
        };
        batch.extend_from_slice(&first);
        while batch.len() < BATCH {
            let next = match control.try_recv() {
                Ok(frame) => frame,
                Err(_) => match data.try_recv() {
                    Ok(frame) => frame,
                    Err(_) => break,
                },
            };
            batch.extend_from_slice(&next);
        }
        if writer.write_all(&batch).await.is_err() || writer.flush().await.is_err() {
            return;
        }
        batch.clear();
    }
    let _ = writer.shutdown().await;
}

/// Counts the bytes read from the server below TLS (what TURN bills includes its framing).
struct Counted<T> {
    inner: T,
    rx_bytes: Arc<AtomicU64>,
}

impl<T: AsyncRead + Unpin> AsyncRead for Counted<T> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let before = buf.filled().len();
        let result = Pin::new(&mut this.inner).poll_read(cx, buf);
        if result.is_ready() {
            this.rx_bytes
                .fetch_add((buf.filled().len() - before) as u64, Ordering::Relaxed);
        }
        result
    }
}

impl<T: AsyncWrite + Unpin> AsyncWrite for Counted<T> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().inner).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}

/// Opens the transport to `server` and starts reading it.
async fn connect(
    server: &Server,
    ice: &IceServers,
    config: &TurnConfig,
    deadline: Instant,
) -> Result<Arc<Shared>, Fail> {
    let addresses = timeout_at(deadline, config.resolver.resolve(&server.host, server.port))
        .await
        .map_err(|_| Fail::Timeout)?
        .map_err(|_| Fail::Closed)?;
    let address = addresses[0];
    let timers = Timers::of(config);
    let rx_bytes = Arc::new(AtomicU64::new(0));
    if server.proto == Proto::Udp {
        let socket = UdpSocket::bind(local_for(address))
            .await
            .map_err(|_| Fail::Closed)?;
        socket.connect(address).await.map_err(|_| Fail::Closed)?;
        let socket = Arc::new(socket);
        let shared = Shared::new(Proto::Udp, Wire::Udp(socket.clone()), ice, timers, rx_bytes);
        shared.spawn(read_udp(shared.clone(), socket));
        return Ok(shared);
    }
    let tcp = timeout_at(deadline, TcpStream::connect(address))
        .await
        .map_err(|_| Fail::Timeout)?
        .map_err(|_| Fail::Closed)?;
    let _ = tcp.set_nodelay(true);
    let counted = Counted {
        inner: tcp,
        rx_bytes: rx_bytes.clone(),
    };
    let (reader, writer): (
        Box<dyn AsyncRead + Send + Unpin>,
        Box<dyn AsyncWrite + Send + Unpin>,
    ) = if server.proto == Proto::Tls {
        let tls_config = client_tls(&config.trust, false).map_err(|_| Fail::Bad)?;
        let name = ServerName::try_from(server.host.clone()).map_err(|_| Fail::Bad)?;
        let tls = timeout_at(
            deadline,
            TlsConnector::from(Arc::new(tls_config)).connect(name, counted),
        )
        .await
        .map_err(|_| Fail::Timeout)?
        .map_err(|_| Fail::Closed)?;
        let (reader, writer) = tokio::io::split(tls);
        (Box::new(reader), Box::new(writer))
    } else {
        let (reader, writer) = tokio::io::split(counted);
        (Box::new(reader), Box::new(writer))
    };
    let (control, control_out) = mpsc::unbounded_channel();
    let (data, data_out) = mpsc::channel(STREAM_QUEUE);
    let shared = Shared::new(
        server.proto,
        Wire::Stream { control, data },
        ice,
        timers,
        rx_bytes,
    );
    shared.spawn(read_stream(shared.clone(), reader));
    shared.spawn(write_stream(writer, control_out, data_out));
    Ok(shared)
}

/// An allocation on a TURN server. Dropping it deletes the allocation in the background; `close`
/// deletes it and waits.
pub(crate) struct Allocation {
    shared: Arc<Shared>,
    relay: SocketAddrV4,
}

/// Allocates a relay address on `server` (REQUESTED-TRANSPORT UDP, the configured lifetime) and
/// keeps it alive.
pub(crate) async fn allocate(
    server: Server,
    ice: &IceServers,
    config: &TurnConfig,
    deadline: Instant,
) -> Result<Allocation, Fail> {
    struct Abandon(Option<Arc<Shared>>);
    impl Drop for Abandon {
        fn drop(&mut self) {
            if let Some(shared) = self.0.take() {
                shared.shutdown();
            }
        }
    }
    let shared = connect(&server, ice, config, deadline).await?;
    let mut abandon = Abandon(Some(shared.clone()));
    let lifetime = u32::try_from(config.allocation_lifetime.as_secs()).unwrap_or(u32::MAX);
    let response = shared
        .request(
            method::ALLOCATE,
            |builder| {
                builder
                    .attr(attr::REQUESTED_TRANSPORT, &[17, 0, 0, 0])
                    .u32(attr::LIFETIME, lifetime);
            },
            deadline,
        )
        .await?;
    // Relay addresses are IPv4 (PROTOCOL.md §5.1).
    let Some(SocketAddr::V4(relay)) = response.xor_address(attr::XOR_RELAYED_ADDRESS) else {
        return Err(Fail::Bad);
    };
    let granted = response
        .u32(attr::LIFETIME)
        .map_or(config.allocation_lifetime, |s| {
            Duration::from_secs(u64::from(s))
        });
    let _ = shared.relay.set(relay);
    shared.spawn(maintain(shared.clone(), granted));
    abandon.0 = None;
    Ok(Allocation { shared, relay })
}

impl Allocation {
    pub(crate) fn relay(&self) -> SocketAddrV4 {
        self.relay
    }

    pub(crate) fn shared(&self) -> &Arc<Shared> {
        &self.shared
    }

    /// Permits `peer` and binds a channel to it (refreshed until `forget`).
    pub(crate) async fn admit(&self, peer: SocketAddrV4) -> Result<(), Fail> {
        let shared = &self.shared;
        let deadline = Instant::now() + shared.timers.request_timeout;
        let channel = shared.channel_for(peer);
        shared.create_permission(peer, deadline).await?;
        shared.channel_bind(channel, peer, deadline).await?;
        let now = Instant::now();
        {
            let mut peers = write(&shared.peers);
            peers.by_channel.insert(channel, peer);
            peers.by_address.insert(
                peer,
                Peer {
                    channel,
                    active: true,
                    linger_until: None,
                    permission_due: now + shared.timers.permission_refresh,
                    channel_due: now + shared.timers.channel_refresh,
                },
            );
        }
        shared.wake.notify_one();
        Ok(())
    }

    /// Stops refreshing `peer` and drops what it sends from now on (its permission runs out on the
    /// server); what is sent to it still goes out for a moment, so QUIC's close reaches it.
    pub(crate) fn forget(&self, peer: SocketAddrV4) {
        if let Some(known) = write(&self.shared.peers).by_address.get_mut(&peer) {
            known.active = false;
            known.linger_until = Some(Instant::now() + FORGET_LINGER);
        }
    }

    pub(crate) fn is_admitted(&self, peer: SocketAddrV4) -> bool {
        read(&self.shared.peers).active(peer).is_some()
    }

    pub(crate) fn admitted(&self) -> Vec<SocketAddrV4> {
        read(&self.shared.peers)
            .by_address
            .iter()
            .filter(|(_, peer)| peer.active)
            .map(|(address, _)| *address)
            .collect()
    }

    /// Checks the allocation now (after a QUIC connection over it timed out).
    pub(crate) fn probe(&self) {
        self.shared.probe.store(true, Ordering::Release);
        self.shared.wake.notify_one();
    }

    pub(crate) async fn close(&self) {
        if self.shared.closing.swap(true, Ordering::AcqRel) {
            return;
        }
        self.shared.delete(DELETE_WAIT).await;
        self.shared.shutdown();
    }
}

impl Drop for Allocation {
    fn drop(&mut self) {
        if self.shared.closing.swap(true, Ordering::AcqRel) {
            return;
        }
        let shared = self.shared.clone();
        match tokio::runtime::Handle::try_current() {
            Ok(runtime) => {
                runtime.spawn(async move {
                    shared.delete(DELETE_WAIT).await;
                    shared.shutdown();
                });
            }
            Err(_) => shared.shutdown(),
        }
    }
}
