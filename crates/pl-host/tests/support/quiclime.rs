//! A QUIClime relay and its broker on loopback, following the relay's source
//! (`vgskye/e4mc-quiclime`, branch `kity`, 6e97f68): a control stream framed with a VarInt length
//! one way and one length byte the other, one stream per routed player that starts with the
//! re-encoded handshake, polite and impolite login refusals with QUIClime's texts after one more
//! packet (at most 128 bytes), a per-IP limit checked before the domain, and the broker's JSON.

use std::collections::HashMap;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use quinn::crypto::rustls::QuicServerConfig;
use quinn::{Connection, Endpoint, RecvStream, SendStream, ServerConfig, TransportConfig, VarInt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use super::certs::{TestCa, server_tls};

/// QUIClime's `disconnect_response.json` and `disconnect_response_rate.json`.
pub const UNKNOWN: &str = "{\n    \"text\": \"Unknown server. Check address and try again.\"\n}";
pub const TOO_FAST: &str = "{\n    \"text\": \"You are trying to connect too fast. Please wait a minute before retrying.\"\n}";
/// Domains handed out in turn (the first is the vectors').
const WORDS: [&str; 3] = ["able-acid", "t-shirt-yo-yo", "drop-down-zebra"];

fn guard<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What the relay does with `request_domain_assignment`.
#[derive(Clone)]
pub enum DomainReply {
    Assign,
    /// `has_capabilities` first (as for a capability probe), then the domain.
    CapabilitiesFirst,
    /// Never answers.
    Never,
    /// A control message the launcher does not know.
    Unknown(&'static str),
}

#[derive(Clone)]
pub struct RelayOptions {
    /// The relay's base domain.
    pub base: &'static str,
    pub alpn: &'static [u8],
    pub domain: DomainReply,
    /// Connections a minute per IP before "too fast" (QUIClime: 30).
    pub limit_per_minute: usize,
    /// An operator broadcast sent after the domain.
    pub broadcast: Option<String>,
    /// Re-encode the handshake with this next state instead (a relay that changed).
    pub next_state_to_host: Option<i32>,
}

impl Default for RelayOptions {
    fn default() -> Self {
        Self {
            base: "eu.e4mc.link",
            alpn: b"quiclime",
            domain: DomainReply::Assign,
            limit_per_minute: 30,
            broadcast: None,
            next_state_to_host: None,
        }
    }
}

type Route = oneshot::Sender<(SendStream, RecvStream)>;

struct RelayState {
    options: RelayOptions,
    routes: Mutex<HashMap<String, mpsc::UnboundedSender<Route>>>,
    assigned: AtomicUsize,
    hosts: Mutex<Vec<Connection>>,
    seen: Mutex<HashMap<IpAddr, Vec<Instant>>>,
    guest_bytes: Mutex<Vec<Vec<u8>>>,
}

pub struct FakeRelay {
    pub quic: SocketAddr,
    pub minecraft: SocketAddr,
    state: Arc<RelayState>,
    endpoint: Endpoint,
    tasks: Vec<JoinHandle<()>>,
}

impl Drop for FakeRelay {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
        self.endpoint.close(VarInt::from_u32(0), b"");
    }
}

impl FakeRelay {
    pub async fn start(ca: &TestCa, options: RelayOptions) -> Self {
        let tls = server_tls(
            ca,
            &["eu.e4mc.link", "de.e4mc.link", "uk.e4mc.link"],
            &[options.alpn],
            true,
        );
        let mut server =
            ServerConfig::with_crypto(Arc::new(QuicServerConfig::try_from(tls).unwrap()));
        let mut transport = TransportConfig::default();
        transport
            .max_concurrent_bidi_streams(VarInt::from_u32(1))
            .max_concurrent_uni_streams(VarInt::from_u32(0))
            .keep_alive_interval(Some(Duration::from_secs(5)));
        server.transport_config(Arc::new(transport));
        let endpoint = Endpoint::server(server, SocketAddr::from(([127, 0, 0, 1], 0))).unwrap();
        let minecraft = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let state = Arc::new(RelayState {
            options,
            routes: Mutex::new(HashMap::new()),
            assigned: AtomicUsize::new(0),
            hosts: Mutex::new(Vec::new()),
            seen: Mutex::new(HashMap::new()),
            guest_bytes: Mutex::new(Vec::new()),
        });
        let (quic, minecraft_address) = (
            endpoint.local_addr().unwrap(),
            minecraft.local_addr().unwrap(),
        );
        let accepting = {
            let (endpoint, state) = (endpoint.clone(), state.clone());
            tokio::spawn(async move {
                while let Some(incoming) = endpoint.accept().await {
                    tokio::spawn(serve_host(state.clone(), incoming));
                }
            })
        };
        let players = {
            let state = state.clone();
            tokio::spawn(async move {
                while let Ok((stream, _)) = minecraft.accept().await {
                    tokio::spawn(serve_player(state.clone(), stream));
                }
            })
        };
        Self {
            quic,
            minecraft: minecraft_address,
            state,
            endpoint,
            tasks: vec![accepting, players],
        }
    }

    /// The bytes guest connection `index` sent, as far as the relay read them (at most 4 KiB).
    pub fn guest_bytes(&self, index: usize) -> Vec<u8> {
        guard(&self.state.guest_bytes)
            .get(index)
            .cloned()
            .unwrap_or_default()
    }

    pub fn domains(&self) -> Vec<String> {
        guard(&self.state.routes).keys().cloned().collect()
    }

    /// Closes every host connection, like the relay going away.
    pub fn close_hosts(&self) {
        for connection in guard(&self.state.hosts).iter() {
            connection.close(VarInt::from_u32(0), b"e4mc closing");
        }
    }
}

async fn write_control(send: &mut SendStream, json: &[u8]) -> bool {
    send.write_all(&[json.len() as u8]).await.is_ok() && send.write_all(json).await.is_ok()
}

async fn read_varint(recv: &mut (impl AsyncReadExt + Unpin)) -> io::Result<i32> {
    let mut value = 0u32;
    for shift in 0..5 {
        let byte = recv.read_u8().await?;
        value |= u32::from(byte & 0x7F) << (7 * shift);
        if byte & 0x80 == 0 {
            return Ok(value as i32);
        }
    }
    Err(io::ErrorKind::InvalidData.into())
}

fn put_varint(out: &mut Vec<u8>, value: i32) {
    let mut rest = value as u32;
    loop {
        if rest & !0x7F == 0 {
            out.push(rest as u8);
            return;
        }
        out.push((rest & 0x7F) as u8 | 0x80);
        rest >>= 7;
    }
}

fn take_varint(buf: &mut &[u8]) -> Option<i32> {
    let mut value = 0u32;
    for shift in 0..5 {
        let (&byte, rest) = buf.split_first()?;
        *buf = rest;
        value |= u32::from(byte & 0x7F) << (7 * shift);
        if byte & 0x80 == 0 {
            return Some(value as i32);
        }
    }
    None
}

async fn serve_host(state: Arc<RelayState>, incoming: quinn::Incoming) {
    let Ok(connection) = incoming.await else {
        return;
    };
    guard(&state.hosts).push(connection.clone());
    let Ok((mut send, mut recv)) = connection.accept_bi().await else {
        return;
    };
    loop {
        let Ok(len) = read_varint(&mut recv).await else {
            return;
        };
        if !(0..=8192).contains(&len) {
            connection.close(VarInt::from_u32(0), b"");
            return;
        }
        let mut buf = vec![0u8; len as usize];
        if recv.read_exact(&mut buf).await.is_err() {
            return;
        }
        let kind = serde_json::from_slice::<serde_json::Value>(&buf)
            .ok()
            .and_then(|value| value["kind"].as_str().map(str::to_owned));
        if kind.as_deref() != Some("request_domain_assignment") {
            write_control(&mut send, br#"{"kind":"unknown_message"}"#).await;
            continue;
        }
        match &state.options.domain {
            DomainReply::Assign => break,
            DomainReply::CapabilitiesFirst => {
                write_control(
                    &mut send,
                    br#"{"kind":"has_capabilities","caps":["dialtone_sidecar"]}"#,
                )
                .await;
                break;
            }
            DomainReply::Never => {
                connection.closed().await;
                return;
            }
            DomainReply::Unknown(kind) => {
                write_control(&mut send, format!(r#"{{"kind":"{kind}"}}"#).as_bytes()).await;
            }
        }
    }
    let word = WORDS[state.assigned.fetch_add(1, Ordering::Relaxed) % WORDS.len()];
    let domain = format!("{word}.{}", state.options.base);
    let (routes, mut requests) = mpsc::unbounded_channel();
    guard(&state.routes).insert(domain.clone(), routes);
    let assigned = format!(r#"{{"kind":"domain_assignment_complete","domain":"{domain}"}}"#);
    write_control(&mut send, assigned.as_bytes()).await;
    if let Some(text) = &state.options.broadcast {
        let broadcast = serde_json::json!({ "kind": "request_message_broadcast", "message": text });
        write_control(&mut send, &serde_json::to_vec(&broadcast).unwrap()).await;
    }
    let mut control = [0u8; 256];
    loop {
        tokio::select! {
            _ = connection.closed() => break,
            route = requests.recv() => {
                let Some(route) = route else { break };
                match connection.open_bi().await {
                    Ok(pair) => {
                        let _ = route.send(pair);
                    }
                    Err(_) => break,
                }
            }
            read = recv.read(&mut control) => {
                if !matches!(read, Ok(Some(_))) {
                    // QUIClime's session ends with the client's side of the control stream.
                    connection.close(VarInt::from_u32(0), b"");
                    break;
                }
            }
        }
    }
    guard(&state.routes).remove(&domain);
}

/// Reads one packet (at most `max` bytes after its length): the raw bytes and the body.
async fn read_packet(tcp: &mut TcpStream, max: usize) -> io::Result<(Vec<u8>, Vec<u8>)> {
    let mut raw = Vec::new();
    let mut len = 0u32;
    for shift in 0..5 {
        let byte = tcp.read_u8().await?;
        raw.push(byte);
        len |= u32::from(byte & 0x7F) << (7 * shift);
        if byte & 0x80 == 0 {
            break;
        }
    }
    let len = len as usize;
    if len > max {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let mut body = vec![0u8; len];
    tcp.read_exact(&mut body).await?;
    raw.extend_from_slice(&body);
    Ok((raw, body))
}

struct Handshake {
    protocol: i32,
    address: String,
    port: u16,
    next_state: i32,
}

fn parse_handshake(mut body: &[u8]) -> Option<Handshake> {
    if take_varint(&mut body)? != 0 {
        return None;
    }
    let protocol = take_varint(&mut body)?;
    let len = take_varint(&mut body)? as usize;
    let address = String::from_utf8(body.get(..len)?.to_vec()).ok()?;
    body = &body[len..];
    let port = u16::from_be_bytes([*body.first()?, *body.get(1)?]);
    body = &body[2..];
    let next_state = take_varint(&mut body)?;
    Some(Handshake {
        protocol,
        address,
        port,
        next_state,
    })
}

fn encode_handshake(handshake: &Handshake, next_state: i32) -> Vec<u8> {
    let mut body = Vec::new();
    put_varint(&mut body, 0);
    put_varint(&mut body, handshake.protocol);
    put_varint(&mut body, handshake.address.len() as i32);
    body.extend_from_slice(handshake.address.as_bytes());
    body.extend_from_slice(&handshake.port.to_be_bytes());
    put_varint(&mut body, next_state);
    let mut packet = Vec::new();
    put_varint(&mut packet, body.len() as i32);
    packet.extend_from_slice(&body);
    packet
}

fn record(state: &RelayState, index: usize, bytes: &[u8]) {
    let mut all = guard(&state.guest_bytes);
    if let Some(seen) = all.get_mut(index)
        && seen.len() < 4096
    {
        seen.extend_from_slice(bytes);
    }
}

fn limited(state: &RelayState, ip: IpAddr) -> bool {
    let mut seen = guard(&state.seen);
    let recent = seen.entry(ip).or_default();
    let now = Instant::now();
    recent.retain(|at| now.duration_since(*at) < Duration::from_secs(60));
    let over = recent.len() >= state.options.limit_per_minute;
    recent.push(now);
    over
}

/// A login refused the way QUIClime does it: one more packet read, then a disconnect.
async fn refuse(state: &RelayState, index: usize, mut tcp: TcpStream, text: &str) {
    let Ok((raw, _)) = read_packet(&mut tcp, 128).await else {
        return;
    };
    record(state, index, &raw);
    let mut body = vec![0x00];
    put_varint(&mut body, text.len() as i32);
    body.extend_from_slice(text.as_bytes());
    let mut packet = Vec::new();
    put_varint(&mut packet, body.len() as i32);
    packet.extend_from_slice(&body);
    let _ = tcp.write_all(&packet).await;
    let _ = tcp.shutdown().await;
}

async fn serve_player(state: Arc<RelayState>, mut tcp: TcpStream) {
    let ip = tcp.peer_addr().map(|address| address.ip()).unwrap();
    let index = {
        let mut all = guard(&state.guest_bytes);
        all.push(Vec::new());
        all.len() - 1
    };
    let Ok((raw, body)) = read_packet(&mut tcp, 512).await else {
        return;
    };
    record(&state, index, &raw);
    let Some(handshake) = parse_handshake(&body) else {
        return;
    };
    if limited(&state, ip) {
        return refuse(&state, index, tcp, TOO_FAST).await;
    }
    let address = handshake
        .address
        .trim_end_matches("\0FML3\0")
        .trim_end_matches("\0FML2\0")
        .trim_end_matches("\0FML\0");
    let route = guard(&state.routes)
        .get(&address.to_ascii_lowercase())
        .cloned();
    let Some(route) = route else {
        return refuse(&state, index, tcp, UNKNOWN).await;
    };
    let (reply, routed) = oneshot::channel();
    if route.send(reply).is_err() {
        return refuse(&state, index, tcp, UNKNOWN).await;
    }
    let Ok((mut send_host, mut recv_host)) = routed.await else {
        return refuse(&state, index, tcp, UNKNOWN).await;
    };
    let next_state = state
        .options
        .next_state_to_host
        .unwrap_or(handshake.next_state);
    if send_host
        .write_all(&encode_handshake(&handshake, next_state))
        .await
        .is_err()
    {
        return;
    }
    let (mut from_guest, mut to_guest) = tcp.split();
    tokio::select! {
        () = async {
            let mut buf = [0u8; 8192];
            loop {
                match from_guest.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(read) => {
                        record(&state, index, &buf[..read]);
                        if send_host.write_all(&buf[..read]).await.is_err() {
                            break;
                        }
                    }
                }
            }
        } => {}
        _ = tokio::io::copy(&mut recv_host, &mut to_guest) => {}
    }
    let _ = tcp.shutdown().await;
    let _ = send_host.finish();
    let _ = recv_host.stop(VarInt::from_u32(0));
}

/// What the broker answers.
#[derive(Clone)]
pub enum BrokerReply {
    Json(String),
    Status(u16),
    Hang,
}

pub struct Broker {
    pub address: SocketAddr,
    user_agents: Arc<Mutex<Vec<String>>>,
    task: JoinHandle<()>,
}

impl Drop for Broker {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Broker {
    pub async fn start(reply: BrokerReply) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let user_agents = Arc::new(Mutex::new(Vec::new()));
        let agents = user_agents.clone();
        let task = tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let (reply, agents) = (reply.clone(), agents.clone());
                tokio::spawn(async move {
                    let mut request = Vec::new();
                    let mut chunk = [0u8; 1024];
                    while !request.windows(4).any(|window| window == b"\r\n\r\n")
                        && request.len() < 8192
                    {
                        match stream.read(&mut chunk).await {
                            Ok(0) | Err(_) => return,
                            Ok(read) => request.extend_from_slice(&chunk[..read]),
                        }
                    }
                    let text = String::from_utf8_lossy(&request).into_owned();
                    if let Some(agent) = text.lines().find_map(|line| {
                        line.split_once(':')
                            .filter(|(name, _)| name.eq_ignore_ascii_case("user-agent"))
                            .map(|(_, value)| value.trim().to_owned())
                    }) {
                        guard(&agents).push(agent);
                    }
                    let response = match reply {
                        BrokerReply::Json(body) => format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        ),
                        BrokerReply::Status(code) => {
                            format!(
                                "HTTP/1.1 {code} Nope\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            )
                        }
                        BrokerReply::Hang => {
                            tokio::time::sleep(Duration::from_secs(30)).await;
                            return;
                        }
                    };
                    let _ = stream.write_all(response.as_bytes()).await;
                    let _ = stream.shutdown().await;
                });
            }
        });
        Self {
            address,
            user_agents,
            task,
        }
    }

    pub fn url(&self) -> String {
        format!("http://{}/getBestRelay", self.address)
    }

    pub fn user_agents(&self) -> Vec<String> {
        guard(&self.user_agents).clone()
    }
}
