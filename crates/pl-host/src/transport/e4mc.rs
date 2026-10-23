//! The e4mc relay (QUIClime, `vgskye/e4mc-quiclime`, branch `kity`): PROTOCOL.md §5.3–5.4.
//!
//! - **Host**: QUIC to a relay (ALPN `quiclime`) and one control stream that asks for a domain. The
//!   relay then opens one stream per player connection for that domain, starting with its
//!   re-encoding of the player's handshake packet.
//! - **Guest**: a TCP connection to `<domain>:25565` carrying the handshake packet and the PLJ1
//!   opening in one write.
//!
//! What the relay does, read from its source (notes in `docs/tasks/host1-r2.md`): it reads the
//! player's handshake (at most 512 bytes) and checks a per-IP limit (30 connections a minute, every
//! connection counts) before the domain. A refused login gets one more packet read (at most 128
//! bytes) and a login disconnect. An accepted one becomes a stream to the host; the two directions
//! are copied until either ends, which ends both. Its control stream frames what the client sends
//! with a VarInt length and what it answers with one length byte. The session lasts as long as the
//! control stream: finishing our side of it, or no longer reading it, ends the domain.

use std::borrow::Cow;
use std::collections::HashMap;
use std::io;
use std::net::{Shutdown, SocketAddrV4};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::task::{Context, Poll, ready};
use std::time::{Duration, Instant};

use quinn::crypto::rustls::QuicClientConfig;
use quinn::{Connection, Endpoint, EndpointConfig, RecvStream, SendStream, VarInt};
use serde::Deserialize;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, watch};

use super::mc;
use super::net::{Resolver, Tasks, Trust, fail};
use super::quic::{self, PlainUdp, QuicStream};
use super::{BoxFuture, BoxStream, GuestPath, HostPath, PathAddress, PathError, PathKind, Step};

/// The relay's ALPN.
pub const ALPN: &[u8] = b"quiclime";
/// The largest handshake packet the relay reads (and so re-encodes).
pub const MAX_HANDSHAKE: usize = 512;
/// The port a guest's handshake packet names (PROTOCOL.md §5.4), whatever port it connected to.
const HANDSHAKE_PORT: u16 = 25565;
const DOMAIN_SUFFIX: &str = ".e4mc.link";
/// Player streams waiting for `accept`.
const ACCEPT_QUEUE: usize = 16;
/// The broker's answer is a few dozen bytes.
const MAX_BROKER_ANSWER: usize = 4096;

/// Where and how the e4mc path connects. [`E4mcConfig::new`] holds the protocol's numbers
/// (`testdata/policy/client-policy.json`, `e4mc`).
#[derive(Debug, Clone)]
pub struct E4mcConfig {
    /// Asked for the best relay when the region is `auto`.
    pub broker_url: String,
    /// Relays tried in turn when the broker fails.
    pub fallback_relays: Vec<String>,
    /// The relays' QUIC port when the broker does not name one.
    pub relay_port: u16,
    /// The port guests connect to.
    pub player_port: u16,
    pub user_agent: String,
    pub broker_timeout: Duration,
    pub quic_connect_timeout: Duration,
    pub domain_timeout: Duration,
    pub keep_alive: Duration,
    pub idle_timeout: Duration,
    /// How long the relay may take with its handshake packet on a new player stream.
    pub handshake_timeout: Duration,
    /// Streams the relay may have open to the host at once (one per player connection).
    pub relay_streams: u32,
    /// A guest's name resolution and TCP connect together.
    pub tcp_connect_timeout: Duration,
    pub trust: Trust,
    pub resolver: Resolver,
}

impl E4mcConfig {
    /// The protocol's numbers, with `launcher_version` in the broker request's User-Agent.
    pub fn new(launcher_version: &str) -> Self {
        Self {
            broker_url: "https://broker.e4mc.link/getBestRelay".into(),
            fallback_relays: vec![
                "eu.e4mc.link".into(),
                "de.e4mc.link".into(),
                "uk.e4mc.link".into(),
            ],
            relay_port: 25575,
            player_port: 25565,
            user_agent: format!("ProjectsLauncher/{launcher_version} (+https://projects.gg)"),
            broker_timeout: Duration::from_millis(1500),
            quic_connect_timeout: Duration::from_millis(2500),
            domain_timeout: Duration::from_millis(2000),
            keep_alive: Duration::from_millis(5000),
            idle_timeout: Duration::from_millis(15000),
            handshake_timeout: Duration::from_millis(5000),
            relay_streams: 64,
            tcp_connect_timeout: Duration::from_millis(3500),
            trust: Trust::Platform,
            resolver: Resolver::system(),
        }
    }
}

impl Default for E4mcConfig {
    fn default() -> Self {
        Self::new(env!("CARGO_PKG_VERSION"))
    }
}

/// Whether `domain` is one a guest may connect to (PROTOCOL.md §5.1):
/// `^[a-z0-9-]{1,63}\.[a-z0-9-]{1,63}\.e4mc\.link$`, at most 253 characters.
pub fn valid_domain(domain: &str) -> bool {
    if domain.len() > 253 {
        return false;
    }
    let Some(name) = domain.strip_suffix(DOMAIN_SUFFIX) else {
        return false;
    };
    let mut labels = name.split('.');
    match (labels.next(), labels.next(), labels.next()) {
        (Some(first), Some(second), None) => valid_label(first) && valid_label(second),
        _ => false,
    }
}

fn valid_label(label: &str) -> bool {
    (1..=63).contains(&label.len())
        && label
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}

/// A relay host: `<labels>.e4mc.link`, lowercase.
fn valid_relay_host(host: &str) -> bool {
    host.len() <= 253
        && host
            .strip_suffix(DOMAIN_SUFFIX)
            .is_some_and(|name| name.split('.').all(valid_label))
}

/// A relay id for telemetry (`region`): a short lowercase word.
fn valid_region(id: &str) -> bool {
    (1..=16).contains(&id.len())
        && id
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
}

/// What a guest's opening got back when it was not msg2 (`packet`: the bytes after the VarInt
/// length): a relay's login disconnect, by its text.
pub fn explain_refusal(packet: &[u8]) -> Option<Cow<'static, str>> {
    let text = mc::disconnect_text(packet)?;
    Some(Cow::Borrowed(if text.contains("too fast") {
        "e4mc_rate_limited"
    } else {
        "e4mc_unknown_domain"
    }))
}

#[derive(Debug, Clone)]
struct Relay {
    id: String,
    host: String,
    port: u16,
}

impl Relay {
    fn named(host: &str, port: u16) -> Option<Self> {
        let host = host.to_ascii_lowercase();
        let id = host.split('.').next()?.to_owned();
        (valid_relay_host(&host) && valid_region(&id)).then_some(Self { id, host, port })
    }
}

/// The result of opening the host's e4mc path.
#[derive(Debug)]
pub struct HostOpening {
    /// The broker's failure, when it was asked and failed: its own telemetry event, before the
    /// attempt's (PROTOCOL.md §12.1).
    pub broker_error: Option<PathError>,
    /// The relay id the attempt ended on (telemetry `region`).
    pub region: Option<String>,
    pub result: Result<E4mcHost, PathError>,
}

/// Opens the host's e4mc path: the relay for `region` (`auto`: the broker's choice, else the
/// fallback relays in turn; a relay id: that relay), QUIC, a domain.
pub async fn open_host(config: &E4mcConfig, region: &str) -> HostOpening {
    let mut broker_error = None;
    let relays = match (region != "auto" && valid_region(region))
        .then(|| Relay::named(&format!("{region}{DOMAIN_SUFFIX}"), config.relay_port))
        .flatten()
    {
        Some(relay) => vec![relay],
        None => match ask_broker(config).await {
            Ok(relay) => vec![relay],
            Err(error) => {
                tracing::info!(path = "e4mc", step = "broker", code = %error.code, "e4mc broker failed");
                broker_error = Some(error);
                config
                    .fallback_relays
                    .iter()
                    .filter_map(|host| Relay::named(host, config.relay_port))
                    .collect()
            }
        },
    };
    let mut last = None;
    let mut region = None;
    for relay in relays {
        region = Some(relay.id.clone());
        let started = Instant::now();
        match connect_relay(config, &relay).await {
            Ok(host) => {
                tracing::info!(path = "e4mc", region = %relay.id, ms = started.elapsed().as_millis() as u64, "e4mc relay ready");
                return HostOpening {
                    broker_error,
                    region,
                    result: Ok(host),
                };
            }
            Err(error) => {
                tracing::info!(path = "e4mc", region = %relay.id, step = error.step.as_str(), code = %error.code, "e4mc relay failed");
                last = Some(error);
            }
        }
    }
    HostOpening {
        broker_error,
        region,
        result: Err(last.unwrap_or_else(|| fail(Step::Quic, "e4mc_quic_refused"))),
    }
}

#[derive(Deserialize)]
struct BrokerAnswer {
    id: String,
    host: String,
    port: u32,
}

async fn ask_broker(config: &E4mcConfig) -> Result<Relay, PathError> {
    let unreachable = || fail(Step::Broker, "e4mc_broker_timeout");
    let bad = || fail(Step::Broker, "e4mc_broker_bad");
    let client = reqwest::Client::builder()
        .user_agent(config.user_agent.clone())
        .timeout(config.broker_timeout)
        .build()
        .map_err(|_| bad())?;
    // Any failure to get an answer (timeout, refused, TLS) reads as the broker not answering.
    let mut response = client
        .get(&config.broker_url)
        .send()
        .await
        .map_err(|_| unreachable())?;
    let status = response.status();
    if !status.is_success() {
        return Err(PathError {
            step: Step::Broker,
            code: Cow::Owned(format!("e4mc_broker_http_{}", status.as_u16())),
        });
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| unreachable())? {
        body.extend_from_slice(&chunk);
        if body.len() > MAX_BROKER_ANSWER {
            return Err(bad());
        }
    }
    let answer: BrokerAnswer = serde_json::from_slice(&body).map_err(|_| bad())?;
    let port = u16::try_from(answer.port)
        .ok()
        .filter(|&port| port > 0)
        .ok_or_else(bad)?;
    let host = answer.host.to_ascii_lowercase();
    if !valid_relay_host(&host) || !valid_region(&answer.id) {
        return Err(bad());
    }
    Ok(Relay {
        id: answer.id,
        host,
        port,
    })
}

fn relay_client_config(config: &E4mcConfig) -> Option<quinn::ClientConfig> {
    let mut tls = super::net::client_tls(&config.trust, true).ok()?;
    tls.alpn_protocols = vec![ALPN.to_vec()];
    let crypto = QuicClientConfig::try_from(tls).ok()?;
    let mut client = quinn::ClientConfig::new(Arc::new(crypto));
    client.transport_config(Arc::new(quic::transport(&quic::Settings {
        idle_timeout: config.idle_timeout,
        keep_alive: config.keep_alive,
        peer_bidi_streams: config.relay_streams,
        initial_mtu: 1200,
    })));
    Some(client)
}

async fn connect_relay(config: &E4mcConfig, relay: &Relay) -> Result<E4mcHost, PathError> {
    let refused = || fail(Step::Quic, "e4mc_quic_refused");
    let timed_out = || fail(Step::Quic, "e4mc_quic_timeout");
    let deadline = tokio::time::Instant::now() + config.quic_connect_timeout;
    let addresses =
        tokio::time::timeout_at(deadline, config.resolver.resolve(&relay.host, relay.port))
            .await
            .map_err(|_| timed_out())?
            .map_err(|_| refused())?;
    let address = addresses[0];
    let socket = PlainUdp::bind(address).map_err(|_| refused())?;
    let endpoint = Endpoint::new_with_abstract_socket(
        EndpointConfig::default(),
        None,
        Arc::new(socket),
        Arc::new(quinn::TokioRuntime),
    )
    .map_err(|_| refused())?;
    let client = relay_client_config(config).ok_or_else(refused)?;
    let connecting = endpoint
        .connect_with(client, address, &relay.host)
        .map_err(|_| refused())?;
    let connection = match tokio::time::timeout_at(deadline, connecting).await {
        Err(_) | Ok(Err(quinn::ConnectionError::TimedOut)) => return Err(timed_out()),
        Ok(Err(error)) if quic::alpn_refused(&error) => {
            return Err(fail(Step::Quic, "e4mc_protocol_changed"));
        }
        Ok(Err(_)) => return Err(refused()),
        Ok(Ok(connection)) => connection,
    };
    let (domain, control_send, control_recv) =
        match request_domain(&connection, config.domain_timeout).await {
            Ok(answer) => answer,
            Err(error) => {
                connection.close(VarInt::from_u32(0), b"");
                return Err(error);
            }
        };
    let tasks = Tasks::default();
    tasks.add(
        tokio::spawn(keep_control(connection.clone(), control_send, control_recv)).abort_handle(),
    );
    let (streams_tx, streams_rx) = mpsc::channel(ACCEPT_QUEUE);
    tasks.add(
        tokio::spawn(accept_players(
            connection.clone(),
            domain.clone(),
            streams_tx,
            config.handshake_timeout,
        ))
        .abort_handle(),
    );
    Ok(E4mcHost {
        inner: Arc::new(HostInner {
            endpoint,
            connection,
            domain,
            region: relay.id.clone(),
            streams: tokio::sync::Mutex::new(streams_rx),
            tasks,
        }),
    })
}

#[derive(Deserialize)]
struct ControlMessage {
    kind: String,
    #[serde(default)]
    domain: Option<String>,
}

/// Opens the control stream and asks for a domain (PROTOCOL.md §5.3 step 3).
async fn request_domain(
    connection: &Connection,
    limit: Duration,
) -> Result<(String, SendStream, RecvStream), PathError> {
    let closed = || fail(Step::Domain, "e4mc_closed");
    let changed = || fail(Step::Domain, "e4mc_protocol_changed");
    let exchange = async {
        let (mut send, mut recv) = connection.open_bi().await.map_err(|_| closed())?;
        let body = br#"{"kind":"request_domain_assignment"}"#;
        let mut request = Vec::with_capacity(body.len() + 1);
        mc::put_varint(&mut request, body.len() as i32);
        request.extend_from_slice(body);
        send.write_all(&request).await.map_err(|_| closed())?;
        loop {
            let mut len = [0u8; 1];
            recv.read_exact(&mut len).await.map_err(|_| closed())?;
            let mut json = vec![0u8; usize::from(len[0])];
            recv.read_exact(&mut json).await.map_err(|_| closed())?;
            let message: ControlMessage = serde_json::from_slice(&json).map_err(|_| changed())?;
            match message.kind.as_str() {
                "domain_assignment_complete" => {
                    let domain = message.domain.map(|domain| domain.to_ascii_lowercase());
                    return match domain.filter(|domain| valid_domain(domain)) {
                        Some(domain) => Ok((domain, send, recv)),
                        None => Err(changed()),
                    };
                }
                "has_capabilities" | "unknown_message" => {}
                _ => return Err(changed()),
            }
        }
    };
    tokio::time::timeout(limit, exchange)
        .await
        .unwrap_or_else(|_| Err(fail(Step::Domain, "e4mc_domain_timeout")))
}

/// Holds the control stream for the life of the connection: our side stays open (finishing it ends
/// the relay's session) and everything the relay sends is read and dropped unread (an operator's
/// broadcast text is never shown or logged).
async fn keep_control(connection: Connection, send: SendStream, mut recv: RecvStream) {
    let mut buf = [0u8; 1024];
    while let Ok(Some(bytes)) = recv.read(&mut buf).await {
        tracing::debug!(path = "e4mc", bytes, "e4mc control message dropped unread");
    }
    connection.closed().await;
    drop(send);
}

/// Each stream the relay opens starts with its handshake packet for our domain; the ones that do
/// not are closed, the others wait for `accept` positioned at the PLJ1 opening.
async fn accept_players(
    connection: Connection,
    domain: String,
    streams: mpsc::Sender<BoxStream>,
    limit: Duration,
) {
    while let Ok((mut send, mut recv)) = connection.accept_bi().await {
        let streams = streams.clone();
        let domain = domain.clone();
        tokio::spawn(async move {
            match tokio::time::timeout(limit, read_player_handshake(&mut recv, &domain)).await {
                Ok(Ok(())) => {
                    let _ = streams.send(Box::new(QuicStream::new(send, recv))).await;
                }
                _ => {
                    tracing::debug!(path = "e4mc", "e4mc stream without our handshake closed");
                    let _ = send.reset(VarInt::from_u32(0));
                    let _ = recv.stop(VarInt::from_u32(0));
                }
            }
        });
    }
}

async fn read_player_handshake(recv: &mut RecvStream, domain: &str) -> io::Result<()> {
    let invalid = || io::Error::from(io::ErrorKind::InvalidData);
    let len = mc::read_varint(recv).await?;
    let len = usize::try_from(len)
        .ok()
        .filter(|len| (1..=MAX_HANDSHAKE).contains(len))
        .ok_or_else(invalid)?;
    let mut body = vec![0u8; len];
    recv.read_exact(&mut body).await.map_err(|_| invalid())?;
    let handshake = mc::parse_handshake(&body).ok_or_else(invalid)?;
    if handshake.next_state != mc::LOGIN
        || !mc::strip_forge(handshake.address).eq_ignore_ascii_case(domain)
    {
        return Err(invalid());
    }
    Ok(())
}

/// The host's e4mc path: a QUIC connection to a relay and the domain it gave.
pub struct E4mcHost {
    inner: Arc<HostInner>,
}

struct HostInner {
    endpoint: Endpoint,
    connection: Connection,
    domain: String,
    region: String,
    streams: tokio::sync::Mutex<mpsc::Receiver<BoxStream>>,
    tasks: Tasks,
}

impl E4mcHost {
    /// The domain guests connect to (also in [`HostPath::address`]). Logs show it only as
    /// `*.e4mc.link`.
    pub fn domain(&self) -> &str {
        &self.inner.domain
    }

    /// The relay id (telemetry `region`).
    pub fn region(&self) -> &str {
        &self.inner.region
    }
}

impl std::fmt::Debug for E4mcHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("E4mcHost")
            .field("region", &self.inner.region)
            .finish_non_exhaustive()
    }
}

impl HostPath for E4mcHost {
    fn kind(&self) -> PathKind {
        PathKind::E4mc
    }

    fn address(&self) -> PathAddress {
        PathAddress::E4mc {
            domain: self.inner.domain.clone(),
        }
    }

    fn admit_peer(&self, _relay: SocketAddrV4) -> BoxFuture<'_, Result<(), PathError>> {
        Box::pin(async { Ok(()) })
    }

    fn forget_peer(&self, _relay: SocketAddrV4) {}

    fn accept(&self) -> BoxFuture<'_, Result<BoxStream, PathError>> {
        Box::pin(async move {
            let stream = self.inner.streams.lock().await.recv().await;
            stream.ok_or_else(|| fail(Step::Quic, "e4mc_closed"))
        })
    }

    fn closed(&self) -> BoxFuture<'_, PathError> {
        Box::pin(async move {
            self.inner.connection.closed().await;
            fail(Step::Quic, "e4mc_closed")
        })
    }

    fn rx_bytes(&self) -> u64 {
        self.inner.connection.stats().udp_rx.bytes
    }

    fn close(&self) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            self.inner.connection.close(VarInt::from_u32(0), b"");
            self.inner.tasks.abort_all();
            let _ =
                tokio::time::timeout(Duration::from_millis(500), self.inner.endpoint.wait_idle())
                    .await;
        })
    }
}

impl Drop for HostInner {
    fn drop(&mut self) {
        self.connection.close(VarInt::from_u32(0), b"");
    }
}

/// The guest's e4mc path to one host's domain.
pub struct E4mcGuest {
    inner: Arc<GuestInner>,
}

struct GuestInner {
    domain: String,
    port: u16,
    handshake: Vec<u8>,
    resolver: Resolver,
    connect_timeout: Duration,
    rx_bytes: Arc<AtomicU64>,
    live: Arc<Mutex<HashMap<u64, Weak<TcpStream>>>>,
    next_id: AtomicU64,
    closed: watch::Sender<bool>,
}

/// The guest's e4mc path to `domain` (checked by PROTOCOL.md §5.1; nothing else is ever connected
/// to), whose handshake packets carry the host's protocol number `protocol`.
pub fn guest(domain: &str, protocol: i32, config: &E4mcConfig) -> Result<E4mcGuest, PathError> {
    if !valid_domain(domain) {
        return Err(fail(Step::Tcp, "e4mc_unknown_domain"));
    }
    Ok(E4mcGuest {
        inner: Arc::new(GuestInner {
            domain: domain.to_owned(),
            port: config.player_port,
            handshake: mc::handshake(protocol, domain, HANDSHAKE_PORT, mc::LOGIN),
            resolver: config.resolver.clone(),
            connect_timeout: config.tcp_connect_timeout,
            rx_bytes: Arc::new(AtomicU64::new(0)),
            live: Arc::new(Mutex::new(HashMap::new())),
            next_id: AtomicU64::new(0),
            closed: watch::channel(false).0,
        }),
    })
}

impl std::fmt::Debug for E4mcGuest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("E4mcGuest")
    }
}

impl GuestInner {
    async fn connect(&self) -> Result<TcpStream, PathError> {
        let deadline = tokio::time::Instant::now() + self.connect_timeout;
        let dns_failed = || fail(Step::Tcp, "e4mc_dns_failed");
        let addresses =
            tokio::time::timeout_at(deadline, self.resolver.resolve(&self.domain, self.port))
                .await
                .map_err(|_| dns_failed())?
                .map_err(|_| dns_failed())?;
        let mut last = fail(Step::Tcp, "e4mc_tcp_refused");
        for address in addresses {
            match tokio::time::timeout_at(deadline, TcpStream::connect(address)).await {
                Ok(Ok(stream)) => return Ok(stream),
                Ok(Err(error)) if error.kind() == io::ErrorKind::TimedOut => {
                    last = fail(Step::Tcp, "e4mc_tcp_timeout")
                }
                Ok(Err(_)) => last = fail(Step::Tcp, "e4mc_tcp_refused"),
                Err(_) => return Err(fail(Step::Tcp, "e4mc_tcp_timeout")),
            }
        }
        Err(last)
    }
}

impl GuestPath for E4mcGuest {
    fn kind(&self) -> PathKind {
        PathKind::E4mc
    }

    fn open(&self) -> BoxFuture<'_, Result<BoxStream, PathError>> {
        Box::pin(async move {
            let inner = &self.inner;
            if *inner.closed.borrow() {
                return Err(fail(Step::Tcp, "e4mc_closed"));
            }
            let stream = inner.connect().await?;
            let _ = stream.set_nodelay(true);
            let stream = Arc::new(stream);
            let id = inner.next_id.fetch_add(1, Ordering::Relaxed);
            lock(&inner.live).insert(id, Arc::downgrade(&stream));
            if *inner.closed.borrow() {
                lock(&inner.live).remove(&id);
                return Err(fail(Step::Tcp, "e4mc_closed"));
            }
            Ok(Box::new(E4mcStream {
                tcp: stream,
                first: Some(inner.handshake.clone()),
                out: Vec::new(),
                sent: 0,
                rx_bytes: inner.rx_bytes.clone(),
                _live: Live {
                    id,
                    registry: inner.live.clone(),
                },
            }) as BoxStream)
        })
    }

    fn explain_reply(&self, packet: &[u8]) -> Option<Cow<'static, str>> {
        explain_refusal(packet)
    }

    fn closed(&self) -> BoxFuture<'_, PathError> {
        Box::pin(async move {
            let mut closed = self.inner.closed.subscribe();
            let _ = closed.wait_for(|closed| *closed).await;
            fail(Step::Tcp, "e4mc_closed")
        })
    }

    fn rx_bytes(&self) -> u64 {
        self.inner.rx_bytes.load(Ordering::Relaxed)
    }

    fn close(&self) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            self.inner.closed.send_replace(true);
            let live: Vec<_> = lock(&self.inner.live)
                .values()
                .filter_map(Weak::upgrade)
                .collect();
            for stream in live {
                let _ = socket2::SockRef::from(stream.as_ref()).shutdown(Shutdown::Both);
            }
        })
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Removes a stream from its path's list when it goes away.
struct Live {
    id: u64,
    registry: Arc<Mutex<HashMap<u64, Weak<TcpStream>>>>,
}

impl Drop for Live {
    fn drop(&mut self) {
        lock(&self.registry).remove(&self.id);
    }
}

/// A guest's TCP connection through the relay. The handshake packet waits for the first write (the
/// PLJ1 opening) and goes out with it as one TCP write; like a `BufWriter`, that first write is
/// accepted once buffered and finishes on the next write, flush, shutdown or read. A flush before
/// any write sends the handshake alone.
struct E4mcStream {
    tcp: Arc<TcpStream>,
    first: Option<Vec<u8>>,
    out: Vec<u8>,
    sent: usize,
    rx_bytes: Arc<AtomicU64>,
    _live: Live,
}

impl E4mcStream {
    fn poll_out(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        while self.sent < self.out.len() {
            ready!(self.tcp.poll_write_ready(cx))?;
            match self.tcp.try_write(&self.out[self.sent..]) {
                Ok(0) => return Poll::Ready(Err(io::ErrorKind::WriteZero.into())),
                Ok(written) => self.sent += written,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) => return Poll::Ready(Err(error)),
            }
        }
        self.out = Vec::new();
        self.sent = 0;
        Poll::Ready(Ok(()))
    }

    fn poll_first(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if let Some(first) = self.first.take() {
            self.out = first;
        }
        self.poll_out(cx)
    }
}

impl AsyncRead for E4mcStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if !this.out.is_empty() {
            // A first write still on its way: keep it moving while the caller waits for the reply.
            let _ = this.poll_out(cx);
        }
        loop {
            ready!(this.tcp.poll_read_ready(cx))?;
            match this.tcp.try_read(buf.initialize_unfilled()) {
                Ok(read) => {
                    buf.advance(read);
                    this.rx_bytes.fetch_add(read as u64, Ordering::Relaxed);
                    return Poll::Ready(Ok(()));
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) => return Poll::Ready(Err(error)),
            }
        }
    }
}

impl AsyncWrite for E4mcStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        data: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        if data.is_empty() {
            return Poll::Ready(Ok(0));
        }
        if let Some(mut joined) = this.first.take() {
            joined.extend_from_slice(data);
            this.out = joined;
            this.sent = 0;
            if let Poll::Ready(Err(error)) = this.poll_out(cx) {
                return Poll::Ready(Err(error));
            }
            return Poll::Ready(Ok(data.len()));
        }
        ready!(this.poll_out(cx))?;
        loop {
            ready!(this.tcp.poll_write_ready(cx))?;
            match this.tcp.try_write(data) {
                Ok(written) => return Poll::Ready(Ok(written)),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) => return Poll::Ready(Err(error)),
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.get_mut().poll_first(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        ready!(this.poll_first(cx))?;
        Poll::Ready(socket2::SockRef::from(this.tcp.as_ref()).shutdown(Shutdown::Write))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vectors() -> serde_json::Value {
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/testdata/e4mc/wire.json"
        ))
        .unwrap();
        serde_json::from_str(&text).unwrap()
    }

    fn hex(value: &serde_json::Value) -> Vec<u8> {
        hex::decode(value.as_str().unwrap()).unwrap()
    }

    #[test]
    fn domains_follow_the_vectors() {
        let vectors = vectors();
        for domain in vectors["domains"]["valid"].as_array().unwrap() {
            assert!(valid_domain(domain.as_str().unwrap()), "{domain}");
        }
        for domain in vectors["domains"]["invalid"].as_array().unwrap() {
            assert!(!valid_domain(domain.as_str().unwrap()), "{domain}");
        }
        assert!(!valid_domain(&format!("{}.eu.e4mc.link", "a".repeat(64))));
    }

    #[test]
    fn the_guest_handshake_matches_the_vector() {
        let vectors = vectors();
        let first = &vectors["guestFirstWrite"];
        let protocol = first["protocol"].as_i64().unwrap() as i32;
        let port = first["port"].as_u64().unwrap() as u16;
        let domain = vectors["control"]["domain"].as_str().unwrap();
        assert_eq!(
            mc::handshake(protocol, domain, port, mc::LOGIN),
            hex(&first["handshakeHex"])
        );
        let mut both = hex(&first["handshakeHex"]);
        both.extend(hex(&first["openingHex"]));
        assert_eq!(both, hex(&first["bothHex"]));
        assert_eq!(both, hex(&vectors["hostStreamStart"]["hex"]));
    }

    #[test]
    fn the_host_reads_the_relays_handshake_and_stops_at_the_opening() {
        let vectors = vectors();
        let start = hex(&vectors["hostStreamStart"]["hex"]);
        let domain = vectors["control"]["domain"].as_str().unwrap();
        let mut rest = start.as_slice();
        let len = mc::take_varint(&mut rest).unwrap() as usize;
        let handshake = mc::parse_handshake(&rest[..len]).unwrap();
        assert_eq!(handshake.address, domain);
        assert_eq!(handshake.next_state, mc::LOGIN);
        assert_eq!(
            &rest[len..],
            hex(&vectors["guestFirstWrite"]["openingHex"]).as_slice()
        );
    }

    #[test]
    fn control_messages_match_the_vectors() {
        let vectors = vectors();
        let body = br#"{"kind":"request_domain_assignment"}"#;
        let mut request = Vec::new();
        mc::put_varint(&mut request, body.len() as i32);
        request.extend_from_slice(body);
        assert_eq!(
            request,
            hex(&vectors["control"]["serverboundRequestDomainHex"])
        );
        let answer = hex(&vectors["control"]["clientboundDomainAssignedHex"]);
        assert_eq!(usize::from(answer[0]), answer.len() - 1);
        let message: ControlMessage = serde_json::from_slice(&answer[1..]).unwrap();
        assert_eq!(message.kind, "domain_assignment_complete");
        assert_eq!(
            message.domain.as_deref(),
            vectors["control"]["domain"].as_str()
        );
    }

    #[test]
    fn refusals_follow_the_vectors() {
        let vectors = vectors();
        for refusal in vectors["relayRefusals"].as_array().unwrap() {
            let bytes = hex(&refusal["hex"]);
            let mut rest = bytes.as_slice();
            let len = mc::take_varint(&mut rest).unwrap();
            assert_ne!(len, 48);
            assert_eq!(len as usize, rest.len());
            assert_eq!(explain_refusal(rest).as_deref(), refusal["code"].as_str());
        }
        // msg2-sized garbage or another packet is not a refusal: the tunnel reads plj_auth_failed.
        assert_eq!(explain_refusal(&[0x01, 0x02, 0x03]), None);
        assert_eq!(explain_refusal(&[]), None);
    }

    #[test]
    fn relay_names_are_checked() {
        assert!(Relay::named("eu.e4mc.link", 25575).is_some());
        assert!(Relay::named("EU.e4mc.link", 25575).is_some());
        assert!(Relay::named("eu.e4mc.link.evil.example", 25575).is_none());
        assert!(Relay::named("e4mc.link", 25575).is_none());
        assert!(Relay::named("eu_1.e4mc.link", 25575).is_none());
        assert!(valid_region("eu") && valid_region("na"));
        assert!(!valid_region("") && !valid_region("e u") && !valid_region("EU"));
    }

    #[test]
    fn the_user_agent_follows_the_policy() {
        let config = E4mcConfig::new("2.1.0");
        assert_eq!(
            config.user_agent,
            "ProjectsLauncher/2.1.0 (+https://projects.gg)"
        );
    }
}
