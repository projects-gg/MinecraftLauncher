//! Cloudflare Realtime TURN (PROTOCOL.md §5.5–5.6). Both sides hold an allocation and speak QUIC
//! between the two relay addresses, so each side sees only the other's relay address.
//!
//! - [`open_host`]: the host's allocation and a QUIC server on it ([`TurnHostPath`]).
//! - [`allocate`]: a guest's allocation ([`TurnGuest`]); [`TurnGuest::connect`] permits the host's
//!   relay address and opens a QUIC connection to it that accepts only the pinned certificate
//!   ([`TurnGuestPath`]).
//! - Credential renewal (§5.5) is a new allocation with the new credentials:
//!   [`TurnHostPath::renew`] admits the old allocation's guests on the new one first; the streams
//!   move (§6.7), then the old path is closed. A new allocation always means a new QUIC connection.
//!
//! The allocation tries the URLs `ice.urls` lists in the policy's order (UDP first; TCP and TLS
//! start after UDP's head start or once every started attempt failed) and keeps the first that
//! works.

mod client;
mod socket;
mod stun;

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use quinn::crypto::rustls::{QuicClientConfig, QuicServerConfig};
use quinn::{Connection, ConnectionError, Endpoint, EndpointConfig, VarInt};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use serde::{Deserialize, Deserializer};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinSet;
use tokio::time::{Instant, sleep_until};
use zeroize::Zeroizing;

use self::client::{Allocation, Fail, Proto, Server};
use self::socket::TurnSocket;
pub use super::net::Secret;
use super::net::{Resolver, Tasks, Trust, fail, provider};
use super::quic::{self, PinVerifier, QuicStream};
use super::{BoxFuture, BoxStream, GuestPath, HostPath, PathAddress, PathError, PathKind, Step};

/// ALPN of QUIC over TURN.
pub const ALPN: &[u8] = b"plj1";
/// The server name a guest asks for; the host's certificate names it too.
pub const SERVER_NAME: &str = "plj.invalid";
/// After UDP's head start, TCP and TLS attempts start this far apart (UDP ones too, among
/// themselves, more closely).
const STREAM_STAGGER: Duration = Duration::from_millis(500);
const UDP_STAGGER: Duration = Duration::from_millis(250);
/// Guest streams waiting for `accept`.
const ACCEPT_QUEUE: usize = 16;
/// Closing waits this long for QUIC's goodbyes to go out.
const CLOSE_WAIT: Duration = Duration::from_millis(250);

/// The `ice` object of a `turn_creds` answer.
#[derive(Clone, Deserialize)]
pub struct IceServers {
    #[serde(deserialize_with = "one_or_more")]
    pub urls: Vec<String>,
    /// Never in logs (PROTOCOL.md §13); `turn_release` and `turn_held` name it.
    pub username: Secret,
    pub credential: Secret,
    /// Seconds the credentials live.
    #[serde(default)]
    pub ttl: u64,
}

impl IceServers {
    pub fn new(urls: Vec<String>, username: Secret, credential: Secret, ttl: u64) -> Self {
        Self {
            urls,
            username,
            credential,
            ttl,
        }
    }
}

impl std::fmt::Debug for IceServers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IceServers")
            .field("urls", &self.urls.len())
            .field("username", &self.username)
            .field("credential", &self.credential)
            .field("ttl", &self.ttl)
            .finish()
    }
}

fn one_or_more<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Urls {
        One(String),
        More(Vec<String>),
    }
    Ok(match Urls::deserialize(deserializer)? {
        Urls::One(url) => vec![url],
        Urls::More(urls) => urls,
    })
}

/// The host's QUIC identity for one session (PROTOCOL.md §2): a P-256 key and a self-signed
/// certificate for `plj.invalid`. Guests accept exactly this certificate, by its pin.
#[derive(Clone)]
pub struct QuicIdentity {
    certificate: CertificateDer<'static>,
    key: Zeroizing<Vec<u8>>,
    pin: [u8; 32],
}

impl QuicIdentity {
    pub fn generate() -> Result<Self, PathError> {
        let failed = |_| fail(Step::Alloc, "turn_alloc_error");
        let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).map_err(failed)?;
        let mut params =
            rcgen::CertificateParams::new(vec![SERVER_NAME.to_owned()]).map_err(failed)?;
        params.distinguished_name = rcgen::DistinguishedName::new();
        params
            .distinguished_name
            .push(rcgen::DnType::CommonName, SERVER_NAME);
        // Nobody checks the dates (the pin is the check); they are only plausible.
        let year = 1970
            + SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |since| since.as_secs() / 31_556_952) as i32;
        params.not_before = rcgen::date_time_ymd(year - 1, 1, 1);
        params.not_after = rcgen::date_time_ymd(year + 2, 1, 1);
        let certificate = params.self_signed(&key).map_err(failed)?.der().clone();
        let pin = quic::pin_of(&certificate);
        Ok(Self {
            certificate,
            key: Zeroizing::new(key.serialize_der()),
            pin,
        })
    }

    /// The SHA-256 of the certificate's DER: `paths.turn.pin`.
    pub fn pin(&self) -> [u8; 32] {
        self.pin
    }

    pub fn certificate_der(&self) -> &[u8] {
        &self.certificate
    }

    fn server_config(&self, config: &TurnConfig) -> Option<quinn::ServerConfig> {
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(self.key.to_vec()));
        let mut tls = rustls::ServerConfig::builder_with_provider(provider())
            .with_protocol_versions(&[&rustls::version::TLS13])
            .ok()?
            .with_no_client_auth()
            .with_single_cert(vec![self.certificate.clone()], key)
            .ok()?;
        tls.alpn_protocols = vec![ALPN.to_vec()];
        tls.send_tls13_tickets = 0;
        let crypto = QuicServerConfig::try_from(tls).ok()?;
        let mut server = quinn::ServerConfig::with_crypto(Arc::new(crypto));
        server.transport_config(Arc::new(quic::transport(
            &config.quic_settings(config.quic_max_bidi_streams),
        )));
        server.migration(false);
        Some(server)
    }
}

impl std::fmt::Debug for QuicIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("QuicIdentity(<redacted>)")
    }
}

/// SHA-256 of a certificate's DER (what `paths.turn.pin` carries).
pub fn pin_of(certificate_der: &[u8]) -> [u8; 32] {
    quic::pin_of(certificate_der)
}

/// Whether a relay address may be used (PROTOCOL.md §5.1): a public IPv4 address and a port. Not
/// private, loopback, link-local, shared (100.64/10), multicast, broadcast, unspecified,
/// documentation, benchmarking or otherwise reserved.
pub fn is_public_relay(relay: SocketAddrV4) -> bool {
    let ip = *relay.ip();
    let [a, b, c, _] = ip.octets();
    relay.port() != 0
        && !(ip.is_private()
            || ip.is_loopback()
            || ip.is_link_local()
            || ip.is_multicast()
            || ip.is_broadcast()
            || ip.is_unspecified()
            || ip.is_documentation()
            || a == 0
            || (a == 100 && (64..128).contains(&b))
            || (a == 192 && b == 0 && c == 0)
            || (a == 198 && (18..20).contains(&b))
            || (a == 192 && b == 88 && c == 99)
            || a >= 240)
}

/// Parses `paths.turn.relay` (PROTOCOL.md §5.1): dotted IPv4 without leading zeros, a port 1–65535,
/// and [`is_public_relay`].
pub fn parse_relay(text: &str) -> Option<SocketAddrV4> {
    let (ip, port) = text.split_once(':')?;
    let canonical = |part: &str| {
        !part.is_empty()
            && part.bytes().all(|c| c.is_ascii_digit())
            && (part == "0" || !part.starts_with('0'))
    };
    if !ip.split('.').all(canonical) || !canonical(port) {
        return None;
    }
    let relay = SocketAddrV4::new(ip.parse::<Ipv4Addr>().ok()?, port.parse().ok()?);
    is_public_relay(relay).then_some(relay)
}

/// Where and how the TURN path connects. The default is the protocol's numbers
/// (`testdata/policy/client-policy.json`, `turn` and `quic`).
#[derive(Debug, Clone)]
pub struct TurnConfig {
    /// URL patterns in the order they are tried (`*` stands for any host or port); only URLs the
    /// credentials list are tried.
    pub url_preference: Vec<String>,
    pub udp_head_start: Duration,
    pub allocation_lifetime: Duration,
    pub refresh_when_left: Duration,
    pub permission_refresh: Duration,
    pub channel_refresh: Duration,
    /// The whole allocation, the challenge included, over every transport tried.
    pub alloc_timeout: Duration,
    /// One request once allocated (permission, channel, refresh).
    pub request_timeout: Duration,
    pub quic_idle_timeout: Duration,
    pub quic_keep_alive: Duration,
    pub quic_max_bidi_streams: u32,
    pub quic_initial_mtu: u16,
    pub quic_connect_timeout: Duration,
    pub trust: Trust,
    pub resolver: Resolver,
    /// Refuse relay addresses that are not public (§5.1). Only tests on loopback turn it off.
    pub public_relays_only: bool,
}

impl Default for TurnConfig {
    fn default() -> Self {
        Self {
            url_preference: [
                "turn:*:3478?transport=udp",
                "turn:*:443?transport=udp",
                "turns:*:443?transport=tcp",
                "turn:*:80?transport=tcp",
                "turns:*:5349?transport=tcp",
                "turn:*:3478?transport=tcp",
            ]
            .map(String::from)
            .to_vec(),
            udp_head_start: Duration::from_millis(1000),
            allocation_lifetime: Duration::from_secs(600),
            refresh_when_left: Duration::from_secs(120),
            permission_refresh: Duration::from_secs(240),
            channel_refresh: Duration::from_secs(480),
            alloc_timeout: Duration::from_millis(4000),
            request_timeout: Duration::from_millis(3000),
            quic_idle_timeout: Duration::from_millis(10_000),
            quic_keep_alive: Duration::from_millis(2000),
            quic_max_bidi_streams: 4,
            quic_initial_mtu: 1200,
            quic_connect_timeout: Duration::from_millis(4000),
            trust: Trust::Platform,
            resolver: Resolver::system(),
            public_relays_only: true,
        }
    }
}

impl TurnConfig {
    fn quic_settings(&self, peer_bidi_streams: u32) -> quic::Settings {
        quic::Settings {
            idle_timeout: self.quic_idle_timeout,
            keep_alive: self.quic_keep_alive,
            peer_bidi_streams,
            initial_mtu: self.quic_initial_mtu,
        }
    }

    /// The servers to try, in preference order.
    fn servers(&self, urls: &[String]) -> Vec<Server> {
        let listed: Vec<Server> = urls.iter().filter_map(|url| parse_url(url)).collect();
        let mut used = vec![false; listed.len()];
        let mut servers = Vec::new();
        for pattern in self
            .url_preference
            .iter()
            .filter_map(|pattern| parse_pattern(pattern))
        {
            for (index, server) in listed.iter().enumerate() {
                if !used[index] && pattern.matches(server) {
                    used[index] = true;
                    servers.push(server.clone());
                }
            }
        }
        servers
    }
}

struct Pattern {
    proto: Proto,
    port: Option<u16>,
}

impl Pattern {
    fn matches(&self, server: &Server) -> bool {
        self.proto == server.proto && self.port.is_none_or(|port| port == server.port)
    }
}

/// `turn:` or `turns:`, host, optional port, optional `?transport=`; `None` for anything else
/// (`turns:` over UDP, STUN, other schemes).
fn split_url(url: &str) -> Option<(Proto, &str, Option<&str>)> {
    let (scheme, rest) = url.split_once(':')?;
    let (address, query) = rest.split_once('?').unwrap_or((rest, ""));
    let secure = match scheme {
        "turn" => false,
        "turns" => true,
        _ => return None,
    };
    let transport = query
        .split('&')
        .find_map(|pair| pair.strip_prefix("transport="))
        .unwrap_or(if secure { "tcp" } else { "udp" });
    let proto = match (secure, transport) {
        (false, "udp") => Proto::Udp,
        (false, "tcp") => Proto::Tcp,
        (true, "tcp") => Proto::Tls,
        _ => return None,
    };
    let (host, port) = if let Some(rest) = address.strip_prefix('[') {
        let (host, after) = rest.split_once(']')?;
        (host, after.strip_prefix(':'))
    } else {
        match address.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (address, None),
        }
    };
    (!host.is_empty()).then_some((proto, host, port))
}

fn parse_url(url: &str) -> Option<Server> {
    let (proto, host, port) = split_url(url)?;
    let port = match port {
        Some(port) => port.parse().ok().filter(|&port| port > 0)?,
        None if proto == Proto::Tls => 5349,
        None => 3478,
    };
    Some(Server {
        proto,
        host: host.to_owned(),
        port,
    })
}

fn parse_pattern(pattern: &str) -> Option<Pattern> {
    let (proto, _host, port) = split_url(pattern)?;
    let port = match port {
        Some("*") | None => None,
        Some(port) => Some(port.parse().ok()?),
    };
    Some(Pattern { proto, port })
}

/// When each attempt may start: UDP at once (staggered a little), TCP and TLS after UDP's head
/// start; an attempt also starts as soon as every started one has failed.
fn start_offsets(servers: &[Server], udp_head_start: Duration) -> Vec<Duration> {
    let any_udp = servers.iter().any(|server| server.proto == Proto::Udp);
    let mut udp = Duration::ZERO;
    let mut stream = if any_udp {
        udp_head_start
    } else {
        Duration::ZERO
    };
    servers
        .iter()
        .map(|server| {
            let slot = if server.proto == Proto::Udp {
                &mut udp
            } else {
                &mut stream
            };
            let at = *slot;
            *slot += if server.proto == Proto::Udp {
                UDP_STAGGER
            } else {
                STREAM_STAGGER
            };
            at
        })
        .collect()
}

/// Races the servers for an allocation; the first that works wins, the rest are dropped (and their
/// allocations deleted).
async fn allocate_any(ice: &IceServers, config: &TurnConfig) -> Result<Allocation, PathError> {
    let servers = config.servers(&ice.urls);
    if servers.is_empty() {
        return Err(fail(Step::Alloc, "turn_alloc_error"));
    }
    let started = Instant::now();
    let deadline = started + config.alloc_timeout;
    let offsets = start_offsets(&servers, config.udp_head_start);
    let mut attempts: JoinSet<Result<Allocation, Fail>> = JoinSet::new();
    let (mut next, mut running) = (0, 0);
    let mut failures = Vec::new();
    while next < servers.len() || running > 0 {
        let start_at = (next < servers.len()).then(|| {
            if running == 0 {
                Instant::now()
            } else {
                started + offsets[next]
            }
        });
        tokio::select! {
            biased;
            Some(joined) = attempts.join_next(), if running > 0 => {
                running -= 1;
                match joined {
                    Ok(Ok(allocation)) => {
                        tracing::debug!(
                            path = "turn",
                            transport = allocation.shared().proto().as_str(),
                            ms = started.elapsed().as_millis() as u64,
                            "TURN allocation ready"
                        );
                        return Ok(allocation);
                    }
                    Ok(Err(failure)) => failures.push(failure),
                    Err(_) => failures.push(Fail::Closed),
                }
            }
            () = sleep_until(start_at.unwrap_or(deadline)), if start_at.is_some() => {
                let server = servers[next].clone();
                next += 1;
                running += 1;
                let (ice, config) = (ice.clone(), config.clone());
                attempts.spawn(async move { client::allocate(server, &ice, &config, deadline).await });
            }
            () = sleep_until(deadline) => break,
        }
    }
    let code = if failures.contains(&Fail::Unauthorized) {
        "turn_alloc_unauthorized"
    } else if failures.iter().all(|failure| *failure == Fail::Timeout) {
        "turn_alloc_timeout"
    } else {
        "turn_alloc_error"
    };
    tracing::debug!(
        path = "turn",
        step = "alloc",
        code,
        "TURN allocation failed"
    );
    Err(fail(Step::Alloc, code))
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Waits until the allocation is gone.
async fn allocation_lost(allocation: &Allocation) -> PathError {
    let mut dead = allocation.shared().dead();
    let lost = dead
        .wait_for(Option::is_some)
        .await
        .ok()
        .and_then(|state| state.clone());
    lost.unwrap_or_else(|| fail(Step::Alloc, "turn_alloc_error"))
}

fn attempt_code(allocation: &Allocation) -> &'static str {
    if allocation.shared().proto().is_stream() {
        "turn_tcp_only"
    } else {
        "ok"
    }
}

/// Opens the host's TURN path: an allocation and a QUIC server on it with `identity`.
pub async fn open_host(
    ice: &IceServers,
    identity: &QuicIdentity,
    config: &TurnConfig,
) -> Result<TurnHostPath, PathError> {
    let allocation = allocate_any(ice, config).await?;
    let failed = || fail(Step::Alloc, "turn_alloc_error");
    let socket = TurnSocket::new(allocation.shared().clone()).ok_or_else(failed)?;
    let server = identity.server_config(config).ok_or_else(failed)?;
    let endpoint = Endpoint::new_with_abstract_socket(
        EndpointConfig::default(),
        Some(server),
        Arc::new(socket),
        Arc::new(quinn::TokioRuntime),
    )
    .map_err(|_| failed())?;
    let allocation = Arc::new(allocation);
    let connections = Arc::new(Mutex::new(HashMap::new()));
    let (streams_tx, streams_rx) = mpsc::channel(ACCEPT_QUEUE);
    let tasks = Tasks::default();
    tasks.add(
        tokio::spawn(accept_guests(
            endpoint.clone(),
            allocation.clone(),
            connections.clone(),
            streams_tx,
            config.quic_connect_timeout,
        ))
        .abort_handle(),
    );
    Ok(TurnHostPath {
        inner: Arc::new(HostInner {
            allocation,
            endpoint,
            identity: identity.clone(),
            config: config.clone(),
            streams: tokio::sync::Mutex::new(streams_rx),
            connections,
            tasks,
        }),
    })
}

type Connections = Arc<Mutex<HashMap<SocketAddrV4, Vec<Connection>>>>;

/// Accepts QUIC connections from admitted guests and hands every stream they open to `accept`.
async fn accept_guests(
    endpoint: Endpoint,
    allocation: Arc<Allocation>,
    connections: Connections,
    streams: mpsc::Sender<BoxStream>,
    limit: Duration,
) {
    while let Some(incoming) = endpoint.accept().await {
        let SocketAddr::V4(remote) = incoming.remote_address() else {
            incoming.refuse();
            continue;
        };
        if !allocation.is_admitted(remote) {
            incoming.refuse();
            continue;
        }
        let (allocation, connections, streams) =
            (allocation.clone(), connections.clone(), streams.clone());
        tokio::spawn(async move {
            let Ok(Ok(connection)) = tokio::time::timeout(limit, incoming).await else {
                return;
            };
            lock(&connections)
                .entry(remote)
                .or_default()
                .push(connection.clone());
            loop {
                match connection.accept_bi().await {
                    Ok((send, recv)) => {
                        if streams
                            .send(Box::new(QuicStream::new(send, recv)))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(error) => {
                        if error == ConnectionError::TimedOut {
                            allocation.probe();
                        }
                        break;
                    }
                }
            }
            let id = connection.stable_id();
            if let Some(list) = lock(&connections).get_mut(&remote) {
                list.retain(|known| known.stable_id() != id);
            }
        });
    }
}

/// The host's TURN path: its allocation and the QUIC server on it.
pub struct TurnHostPath {
    inner: Arc<HostInner>,
}

struct HostInner {
    allocation: Arc<Allocation>,
    endpoint: Endpoint,
    identity: QuicIdentity,
    config: TurnConfig,
    streams: tokio::sync::Mutex<mpsc::Receiver<BoxStream>>,
    connections: Connections,
    tasks: Tasks,
}

impl Drop for HostInner {
    fn drop(&mut self) {
        self.endpoint.close(VarInt::from_u32(0), b"");
    }
}

impl std::fmt::Debug for TurnHostPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TurnHostPath")
    }
}

impl TurnHostPath {
    /// The allocation's relay address (announced in `paths`; never logged).
    pub fn relay(&self) -> SocketAddrV4 {
        self.inner.allocation.relay()
    }

    /// `ok`, or `turn_tcp_only` when the allocation went over TCP or TLS (PROTOCOL.md §12.2).
    pub fn attempt_code(&self) -> &'static str {
        attempt_code(&self.inner.allocation)
    }

    /// Whether the allocation went over TCP or TLS.
    pub fn tcp_only(&self) -> bool {
        self.inner.allocation.shared().proto().is_stream()
    }

    /// A path on a new allocation made with fresh credentials, the same identity (so the pin stays)
    /// and every guest admitted here admitted there first (PROTOCOL.md §5.5, §6.7).
    pub async fn renew(&self, ice: &IceServers) -> Result<TurnHostPath, PathError> {
        let next = open_host(ice, &self.inner.identity, &self.inner.config).await?;
        for peer in self.inner.allocation.admitted() {
            if let Err(error) = next.admit_peer(peer).await {
                next.close().await;
                return Err(error);
            }
        }
        Ok(next)
    }
}

impl HostPath for TurnHostPath {
    fn kind(&self) -> PathKind {
        PathKind::Turn
    }

    fn address(&self) -> PathAddress {
        PathAddress::Turn {
            relay: self.inner.allocation.relay(),
            pin: self.inner.identity.pin(),
        }
    }

    fn admit_peer(&self, relay: SocketAddrV4) -> BoxFuture<'_, Result<(), PathError>> {
        Box::pin(async move {
            if self.inner.config.public_relays_only && !is_public_relay(relay) {
                return Err(fail(Step::Peer, "turn_permission_failed"));
            }
            self.inner
                .allocation
                .admit(relay)
                .await
                .map_err(|_| fail(Step::Peer, "turn_permission_failed"))
        })
    }

    fn forget_peer(&self, relay: SocketAddrV4) {
        self.inner.allocation.forget(relay);
        for connection in lock(&self.inner.connections)
            .remove(&relay)
            .unwrap_or_default()
        {
            connection.close(VarInt::from_u32(0), b"");
        }
    }

    fn accept(&self) -> BoxFuture<'_, Result<BoxStream, PathError>> {
        Box::pin(async move {
            let stream = self.inner.streams.lock().await.recv().await;
            match stream {
                Some(stream) => Ok(stream),
                None => Err(allocation_lost(&self.inner.allocation).await),
            }
        })
    }

    fn closed(&self) -> BoxFuture<'_, PathError> {
        Box::pin(allocation_lost(&self.inner.allocation))
    }

    fn rx_bytes(&self) -> u64 {
        self.inner.allocation.shared().rx_bytes()
    }

    fn close(&self) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            self.inner.endpoint.close(VarInt::from_u32(0), b"");
            let _ = tokio::time::timeout(CLOSE_WAIT, self.inner.endpoint.wait_idle()).await;
            self.inner.tasks.abort_all();
            self.inner.allocation.close().await;
        })
    }
}

/// A guest's allocation: [`TurnGuest::relay`] goes to the host in `world_peer`, then
/// [`TurnGuest::connect`] opens the path. Clones share the allocation; it is deleted when the
/// last clone and every path from it are gone, or at once with [`TurnGuest::close`].
#[derive(Clone)]
pub struct TurnGuest {
    node: Arc<GuestNode>,
}

struct GuestNode {
    allocation: Allocation,
    endpoint: Endpoint,
    config: TurnConfig,
    /// Open paths per host relay address: its permission lasts while any of them does.
    paths: Mutex<HashMap<SocketAddrV4, usize>>,
}

impl GuestNode {
    async fn shutdown(&self) {
        self.endpoint.close(VarInt::from_u32(0), b"");
        let _ = tokio::time::timeout(CLOSE_WAIT, self.endpoint.wait_idle()).await;
        self.allocation.close().await;
    }
}

impl Drop for GuestNode {
    fn drop(&mut self) {
        self.endpoint.close(VarInt::from_u32(0), b"");
    }
}

impl std::fmt::Debug for TurnGuest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TurnGuest")
    }
}

/// Allocates the guest's relay address.
pub async fn allocate(ice: &IceServers, config: &TurnConfig) -> Result<TurnGuest, PathError> {
    let allocation = allocate_any(ice, config).await?;
    let failed = || fail(Step::Alloc, "turn_alloc_error");
    let socket = TurnSocket::new(allocation.shared().clone()).ok_or_else(failed)?;
    let endpoint = Endpoint::new_with_abstract_socket(
        EndpointConfig::default(),
        None,
        Arc::new(socket),
        Arc::new(quinn::TokioRuntime),
    )
    .map_err(|_| failed())?;
    Ok(TurnGuest {
        node: Arc::new(GuestNode {
            allocation,
            endpoint,
            config: config.clone(),
            paths: Mutex::new(HashMap::new()),
        }),
    })
}

impl TurnGuest {
    /// The allocation's relay address (for `world_peer`; never logged).
    pub fn relay(&self) -> SocketAddrV4 {
        self.node.allocation.relay()
    }

    /// `ok`, or `turn_tcp_only` when the allocation went over TCP or TLS (PROTOCOL.md §12.2).
    pub fn attempt_code(&self) -> &'static str {
        attempt_code(&self.node.allocation)
    }

    /// Whether the allocation went over TCP or TLS.
    pub fn tcp_only(&self) -> bool {
        self.node.allocation.shared().proto().is_stream()
    }

    /// Permits the host's relay address and connects to it, accepting only the certificate whose
    /// SHA-256 is `pin` (steps `peer`, `quic`).
    pub async fn connect(
        &self,
        host_relay: SocketAddrV4,
        pin: [u8; 32],
    ) -> Result<TurnGuestPath, PathError> {
        let node = &self.node;
        if node.config.public_relays_only && !is_public_relay(host_relay) {
            return Err(fail(Step::Peer, "turn_permission_failed"));
        }
        node.allocation
            .admit(host_relay)
            .await
            .map_err(|_| fail(Step::Peer, "turn_permission_failed"))?;
        let mismatch = Arc::new(AtomicBool::new(false));
        let client = pinned_client(&node.config, pin, mismatch.clone())
            .ok_or_else(|| fail(Step::Quic, "turn_quic_timeout"))?;
        let path = TurnGuestPath {
            node: node.clone(),
            host: host_relay,
            client,
            mismatch,
            connection: tokio::sync::Mutex::new(None),
            closed: watch::channel(false).0,
        };
        // A failed handshake leaves the permission in place: another path of this allocation may
        // use the same relay address.
        let connection = path.handshake().await?;
        *path.connection.lock().await = Some(connection);
        *lock(&node.paths).entry(host_relay).or_default() += 1;
        Ok(path)
    }

    /// Deletes the allocation now; every path from it is gone.
    pub async fn close(&self) {
        self.node.shutdown().await;
    }
}

fn pinned_client(
    config: &TurnConfig,
    pin: [u8; 32],
    mismatch: Arc<AtomicBool>,
) -> Option<quinn::ClientConfig> {
    let mut tls = rustls::ClientConfig::builder_with_provider(provider())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .ok()?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(PinVerifier::new(pin, mismatch)))
        .with_no_client_auth();
    tls.alpn_protocols = vec![ALPN.to_vec()];
    tls.resumption = rustls::client::Resumption::disabled();
    let crypto = QuicClientConfig::try_from(tls).ok()?;
    let mut client = quinn::ClientConfig::new(Arc::new(crypto));
    // The host opens no streams.
    client.transport_config(Arc::new(quic::transport(&config.quic_settings(0))));
    Some(client)
}

/// A guest's TURN path to one host relay address.
pub struct TurnGuestPath {
    node: Arc<GuestNode>,
    host: SocketAddrV4,
    client: quinn::ClientConfig,
    mismatch: Arc<AtomicBool>,
    connection: tokio::sync::Mutex<Option<Connection>>,
    closed: watch::Sender<bool>,
}

impl std::fmt::Debug for TurnGuestPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TurnGuestPath")
    }
}

impl TurnGuestPath {
    async fn handshake(&self) -> Result<Connection, PathError> {
        let timed_out = || fail(Step::Quic, "turn_quic_timeout");
        self.mismatch.store(false, Ordering::Release);
        let connecting = self
            .node
            .endpoint
            .connect_with(self.client.clone(), SocketAddr::V4(self.host), SERVER_NAME)
            .map_err(|_| timed_out())?;
        match tokio::time::timeout(self.node.config.quic_connect_timeout, connecting).await {
            Ok(Ok(connection)) => Ok(connection),
            Ok(Err(_)) if self.mismatch.load(Ordering::Acquire) => {
                Err(fail(Step::Quic, "turn_quic_pin"))
            }
            _ => {
                // Maybe the allocation is what failed: check it now rather than at its next refresh.
                self.node.allocation.probe();
                Err(timed_out())
            }
        }
    }

    /// The live connection, or a new one when the last one ended.
    async fn connection(&self) -> Result<Connection, PathError> {
        let mut slot = self.connection.lock().await;
        if let Some(connection) = slot
            .as_ref()
            .filter(|connection| connection.close_reason().is_none())
        {
            return Ok(connection.clone());
        }
        let connection = self.handshake().await?;
        *slot = Some(connection.clone());
        Ok(connection)
    }
}

impl GuestPath for TurnGuestPath {
    fn kind(&self) -> PathKind {
        PathKind::Turn
    }

    fn open(&self) -> BoxFuture<'_, Result<BoxStream, PathError>> {
        Box::pin(async move {
            if *self.closed.borrow() {
                return Err(fail(Step::Quic, "turn_quic_timeout"));
            }
            let connection = self.connection().await?;
            let (send, recv) = connection
                .open_bi()
                .await
                .map_err(|_| fail(Step::Quic, "turn_quic_timeout"))?;
            Ok(Box::new(QuicStream::new(send, recv)) as BoxStream)
        })
    }

    fn explain_reply(&self, _packet: &[u8]) -> Option<std::borrow::Cow<'static, str>> {
        None
    }

    fn closed(&self) -> BoxFuture<'_, PathError> {
        Box::pin(async move {
            let mut closed = self.closed.subscribe();
            tokio::select! {
                lost = allocation_lost(&self.node.allocation) => lost,
                _ = closed.wait_for(|closed| *closed) => fail(Step::Alloc, "turn_alloc_error"),
            }
        })
    }

    fn rx_bytes(&self) -> u64 {
        self.node.allocation.shared().rx_bytes()
    }

    fn close(&self) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            if self.closed.send_replace(true) {
                return;
            }
            if let Some(connection) = self.connection.lock().await.take() {
                connection.close(VarInt::from_u32(0), b"");
            }
            let last_to_this_host = {
                let mut paths = lock(&self.node.paths);
                let open = paths.entry(self.host).or_default();
                *open = open.saturating_sub(1);
                *open == 0
            };
            if last_to_this_host {
                self.node.allocation.forget(self.host);
            }
            // The last holder of the allocation deletes it now, so the credentials can be released.
            if Arc::strong_count(&self.node) == 1 {
                self.node.shutdown().await;
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(path: &str) -> serde_json::Value {
        let text =
            std::fs::read_to_string(format!("{}/testdata/{path}", env!("CARGO_MANIFEST_DIR")))
                .unwrap();
        serde_json::from_str(&text).unwrap()
    }

    fn ms(value: &serde_json::Value) -> Duration {
        Duration::from_millis(value.as_u64().unwrap())
    }

    fn secs(value: &serde_json::Value) -> Duration {
        Duration::from_secs(value.as_u64().unwrap())
    }

    #[test]
    fn defaults_equal_the_client_policy() {
        let policy = read("policy/client-policy.json");
        let turn = &policy["turn"];
        let quic = &policy["quic"];
        let config = TurnConfig::default();
        let preference: Vec<String> = turn["urlPreference"]
            .as_array()
            .unwrap()
            .iter()
            .map(|url| url.as_str().unwrap().to_owned())
            .collect();
        assert_eq!(config.url_preference, preference);
        assert_eq!(config.udp_head_start, ms(&turn["udpHeadStartMs"]));
        assert_eq!(
            config.allocation_lifetime,
            secs(&turn["allocationLifetimeS"])
        );
        assert_eq!(config.refresh_when_left, secs(&turn["refreshWhenLeftS"]));
        assert_eq!(config.permission_refresh, secs(&turn["permissionRefreshS"]));
        assert_eq!(config.channel_refresh, secs(&turn["channelRefreshS"]));
        assert_eq!(ALPN, quic["alpn"].as_str().unwrap().as_bytes());
        assert_eq!(SERVER_NAME, quic["serverName"].as_str().unwrap());
        assert_eq!(config.quic_idle_timeout, ms(&quic["idleTimeoutMs"]));
        assert_eq!(config.quic_keep_alive, ms(&quic["keepAliveMs"]));
        assert_eq!(
            u64::from(config.quic_max_bidi_streams),
            quic["maxBidiStreamsPerConnection"].as_u64().unwrap()
        );
        assert_eq!(
            u64::from(config.quic_initial_mtu),
            quic["initialMtu"].as_u64().unwrap()
        );
        assert_eq!(quic["datagrams"].as_bool(), Some(false));

        let e4mc = &policy["e4mc"];
        let config = super::super::e4mc::E4mcConfig::new("9.9.9");
        assert_eq!(config.broker_url, e4mc["brokerUrl"].as_str().unwrap());
        let fallback: Vec<String> = e4mc["fallbackRelays"]
            .as_array()
            .unwrap()
            .iter()
            .map(|relay| relay.as_str().unwrap().to_owned())
            .collect();
        assert_eq!(config.fallback_relays, fallback);
        assert_eq!(
            u64::from(config.relay_port),
            e4mc["relayPort"].as_u64().unwrap()
        );
        assert_eq!(
            u64::from(config.player_port),
            e4mc["playerPort"].as_u64().unwrap()
        );
        assert_eq!(
            super::super::e4mc::ALPN,
            e4mc["alpn"].as_str().unwrap().as_bytes()
        );
        assert_eq!(config.broker_timeout, ms(&e4mc["brokerTimeoutMs"]));
        assert_eq!(
            config.quic_connect_timeout,
            ms(&e4mc["quicConnectTimeoutMs"])
        );
        assert_eq!(config.domain_timeout, ms(&e4mc["domainTimeoutMs"]));
        assert_eq!(config.keep_alive, ms(&e4mc["keepAliveMs"]));
        assert_eq!(config.idle_timeout, ms(&e4mc["idleTimeoutMs"]));
        assert_eq!(
            config.user_agent,
            e4mc["userAgent"]
                .as_str()
                .unwrap()
                .replace("<version>", "9.9.9")
        );
        assert!(config.relay_streams >= 32);
    }

    #[test]
    fn the_pin_is_the_certificates_sha256() {
        let vector = read("turn/pin.json");
        use base64::Engine as _;
        let der = base64::engine::general_purpose::STANDARD
            .decode(vector["certificateDerB64"].as_str().unwrap())
            .unwrap();
        let pin = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(vector["pin"].as_str().unwrap())
            .unwrap();
        assert_eq!(pin_of(&der).as_slice(), pin.as_slice());
    }

    #[test]
    fn a_new_identity_pins_its_own_certificate() {
        let identity = QuicIdentity::generate().unwrap();
        assert_eq!(identity.pin(), pin_of(identity.certificate_der()));
        assert_ne!(identity.pin(), QuicIdentity::generate().unwrap().pin());
        assert!(identity.server_config(&TurnConfig::default()).is_some());
        assert_eq!(format!("{identity:?}"), "QuicIdentity(<redacted>)");
    }

    #[test]
    fn relay_addresses_follow_the_invite_vectors() {
        let vectors = read("invite/join.json");
        for relay in vectors["relayAddresses"]["valid"].as_array().unwrap() {
            assert!(parse_relay(relay.as_str().unwrap()).is_some(), "{relay}");
        }
        for relay in vectors["relayAddresses"]["invalid"].as_array().unwrap() {
            assert!(parse_relay(relay.as_str().unwrap()).is_none(), "{relay}");
        }
        for reserved in [
            "198.18.0.1:1",
            "240.0.0.1:1",
            "192.0.0.8:1",
            "0.1.2.3:1",
            "100.127.255.255:1",
        ] {
            assert!(parse_relay(reserved).is_none(), "{reserved}");
        }
        assert!(parse_relay("100.128.0.1:1").is_some());
    }

    #[test]
    fn urls_are_tried_in_the_policys_order() {
        let urls = [
            "stun:stun.cloudflare.com:3478",
            "turns:turn.cloudflare.com:5349?transport=tcp",
            "turn:turn.cloudflare.com:3478?transport=tcp",
            "turn:turn.cloudflare.com:80?transport=tcp",
            "turns:turn.cloudflare.com:443?transport=tcp",
            "turn:turn.cloudflare.com:3478?transport=udp",
            "turn:turn.cloudflare.com:53?transport=udp",
        ]
        .map(String::from);
        let servers = TurnConfig::default().servers(&urls);
        let order: Vec<(Proto, u16)> = servers
            .iter()
            .map(|server| (server.proto, server.port))
            .collect();
        assert_eq!(
            order,
            vec![
                (Proto::Udp, 3478),
                (Proto::Tls, 443),
                (Proto::Tcp, 80),
                (Proto::Tls, 5349),
                (Proto::Tcp, 3478)
            ]
        );
        assert!(
            servers
                .iter()
                .all(|server| server.host == "turn.cloudflare.com")
        );
        let offsets = start_offsets(&servers, Duration::from_millis(1000));
        assert_eq!(offsets[0], Duration::ZERO);
        assert_eq!(offsets[1], Duration::from_millis(1000));
        assert_eq!(offsets[2], Duration::from_millis(1500));
    }

    #[test]
    fn url_shapes_parse() {
        let server = parse_url("turn:[2606:4700::1]:3478?transport=tcp").unwrap();
        assert_eq!(
            (server.proto, server.host.as_str(), server.port),
            (Proto::Tcp, "2606:4700::1", 3478)
        );
        let server = parse_url("turns:turn.example").unwrap();
        assert_eq!((server.proto, server.port), (Proto::Tls, 5349));
        let server = parse_url("turn:turn.example").unwrap();
        assert_eq!((server.proto, server.port), (Proto::Udp, 3478));
        assert!(parse_url("turns:turn.example:443?transport=udp").is_none());
        assert!(parse_url("turn::3478").is_none());
        assert!(parse_url("turn:turn.example:0").is_none());
        assert!(
            parse_pattern("turn:*:*?transport=udp")
                .unwrap()
                .port
                .is_none()
        );
    }

    #[test]
    fn ice_servers_read_and_stay_out_of_debug() {
        let ice: IceServers = serde_json::from_str(
            r#"{"urls":["turn:turn.example:3478?transport=udp"],"username":"user-1","credential":"pass-1","ttl":7200}"#,
        )
        .unwrap();
        assert_eq!(ice.urls.len(), 1);
        assert_eq!(ice.username.expose(), "user-1");
        let shown = format!("{ice:?}");
        assert!(
            !shown.contains("user-1")
                && !shown.contains("pass-1")
                && !shown.contains("turn.example")
        );
        let single: IceServers =
            serde_json::from_str(r#"{"urls":"turn:turn.example","username":"u","credential":"c"}"#)
                .unwrap();
        assert_eq!(single.urls, vec!["turn:turn.example".to_owned()]);
    }
}
