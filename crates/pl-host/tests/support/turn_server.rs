//! A small TURN server (RFC 8656) on loopback, written apart from the client so a codec mistake in
//! one does not hide in the other: long-term credentials with nonces, Allocate, Refresh,
//! CreatePermission, ChannelBind, Send and Data indications and ChannelData, over UDP, TCP and TLS.
//! Relay addresses are UDP sockets on 127.0.0.1, so two allocations relay to each other.
//! Faults on demand: a stale nonce, revoked users, dropped UDP.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use hmac::{Hmac, KeyInit, Mac};
use md5::{Digest, Md5};
use sha1::Sha1;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, UdpSocket};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_rustls::TlsAcceptor;

use super::certs::{TestCa, server_tls};

const MAGIC: u32 = 0x2112_A442;
const ALLOCATE: u16 = 0x003;
const REFRESH: u16 = 0x004;
const SEND: u16 = 0x006;
const DATA: u16 = 0x007;
const CREATE_PERMISSION: u16 = 0x008;
const CHANNEL_BIND: u16 = 0x009;
const USERNAME: u16 = 0x0006;
const MESSAGE_INTEGRITY: u16 = 0x0008;
const ERROR_CODE: u16 = 0x0009;
const CHANNEL_NUMBER: u16 = 0x000C;
const LIFETIME: u16 = 0x000D;
const XOR_PEER_ADDRESS: u16 = 0x0012;
const DATA_ATTR: u16 = 0x0013;
const REALM: u16 = 0x0014;
const NONCE: u16 = 0x0015;
const XOR_RELAYED_ADDRESS: u16 = 0x0016;
const REQUESTED_TRANSPORT: u16 = 0x0019;
const XOR_MAPPED_ADDRESS: u16 = 0x0020;
const FINGERPRINT: u16 = 0x8028;
const PERMISSION_LIFETIME: Duration = Duration::from_secs(300);

fn guard<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn pad4(len: usize) -> usize {
    (len + 3) & !3
}

struct Message<'a> {
    method: u16,
    class: u8,
    txid: [u8; 12],
    raw: &'a [u8],
    /// (type, attribute start, value start, value length)
    attrs: Vec<(u16, usize, usize, usize)>,
}

fn parse(raw: &[u8]) -> Option<Message<'_>> {
    if raw.len() < 20 || raw[0] & 0xC0 != 0 || raw[4..8] != MAGIC.to_be_bytes() {
        return None;
    }
    let len = usize::from(u16::from_be_bytes([raw[2], raw[3]]));
    if raw.len() != 20 + len {
        return None;
    }
    let kind = u16::from_be_bytes([raw[0], raw[1]]);
    let method = ((kind & 0x3E00) >> 2) | ((kind & 0x00E0) >> 1) | (kind & 0x000F);
    let class = (((kind & 0x0100) >> 7) | ((kind & 0x0010) >> 4)) as u8;
    let mut txid = [0u8; 12];
    txid.copy_from_slice(&raw[8..20]);
    let mut attrs = Vec::new();
    let mut at = 20;
    while at + 4 <= raw.len() {
        let kind = u16::from_be_bytes([raw[at], raw[at + 1]]);
        let len = usize::from(u16::from_be_bytes([raw[at + 2], raw[at + 3]]));
        if at + 4 + len > raw.len() {
            return None;
        }
        attrs.push((kind, at, at + 4, len));
        at += 4 + pad4(len);
    }
    Some(Message {
        method,
        class,
        txid,
        raw,
        attrs,
    })
}

impl<'a> Message<'a> {
    fn attr(&self, kind: u16) -> Option<&'a [u8]> {
        self.attrs
            .iter()
            .find(|attr| attr.0 == kind)
            .map(|attr| &self.raw[attr.2..attr.2 + attr.3])
    }

    fn all(&self, kind: u16) -> Vec<&'a [u8]> {
        self.attrs
            .iter()
            .filter(|attr| attr.0 == kind)
            .map(|attr| &self.raw[attr.2..attr.2 + attr.3])
            .collect()
    }

    fn integrity_ok(&self, key: &[u8]) -> Option<bool> {
        let attr = self.attrs.iter().find(|attr| attr.0 == MESSAGE_INTEGRITY)?;
        let mut signed = self.raw[..attr.1].to_vec();
        signed[2..4].copy_from_slice(&((attr.1 - 20 + 24) as u16).to_be_bytes());
        let mut mac = <Hmac<Sha1> as KeyInit>::new_from_slice(key).unwrap();
        mac.update(&signed);
        Some(mac.verify_slice(&self.raw[attr.2..attr.2 + 20]).is_ok())
    }
}

fn xor_address(value: &[u8]) -> Option<SocketAddr> {
    if value.len() != 8 || value[1] != 0x01 {
        return None;
    }
    let port = u16::from_be_bytes([value[2], value[3]]) ^ (MAGIC >> 16) as u16;
    let ip = u32::from_be_bytes([value[4], value[5], value[6], value[7]]) ^ MAGIC;
    Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::from(ip)), port))
}

fn encode_address(address: SocketAddr) -> Vec<u8> {
    let IpAddr::V4(ip) = address.ip() else {
        panic!("loopback is IPv4 here")
    };
    let mut value = vec![0, 0x01];
    value.extend_from_slice(&(address.port() ^ (MAGIC >> 16) as u16).to_be_bytes());
    value.extend_from_slice(&(u32::from(ip) ^ MAGIC).to_be_bytes());
    value
}

/// A message with these attributes, signed with `key` when given, with a FINGERPRINT.
fn build(
    method: u16,
    class: u8,
    txid: [u8; 12],
    attrs: &[(u16, Vec<u8>)],
    key: Option<&[u8]>,
) -> Vec<u8> {
    let kind = ((method & 0x0F80) << 2)
        | ((method & 0x0070) << 1)
        | (method & 0x000F)
        | ((u16::from(class) & 0b10) << 7)
        | ((u16::from(class) & 0b01) << 4);
    let mut buf = Vec::new();
    buf.extend_from_slice(&kind.to_be_bytes());
    buf.extend_from_slice(&[0, 0]);
    buf.extend_from_slice(&MAGIC.to_be_bytes());
    buf.extend_from_slice(&txid);
    for (kind, value) in attrs {
        buf.extend_from_slice(&kind.to_be_bytes());
        buf.extend_from_slice(&(value.len() as u16).to_be_bytes());
        buf.extend_from_slice(value);
        buf.resize(pad4(buf.len()), 0);
    }
    if let Some(key) = key {
        let len = (buf.len() - 20 + 24) as u16;
        buf[2..4].copy_from_slice(&len.to_be_bytes());
        let mut mac = <Hmac<Sha1> as KeyInit>::new_from_slice(key).unwrap();
        mac.update(&buf);
        let tag = mac.finalize().into_bytes();
        buf.extend_from_slice(&MESSAGE_INTEGRITY.to_be_bytes());
        buf.extend_from_slice(&20u16.to_be_bytes());
        buf.extend_from_slice(&tag);
    }
    let len = (buf.len() - 20 + 8) as u16;
    buf[2..4].copy_from_slice(&len.to_be_bytes());
    let crc = crc32fast::hash(&buf) ^ 0x5354_554E;
    buf.extend_from_slice(&FINGERPRINT.to_be_bytes());
    buf.extend_from_slice(&4u16.to_be_bytes());
    buf.extend_from_slice(&crc.to_be_bytes());
    buf
}

fn channel_data(channel: u16, data: &[u8], pad: bool) -> Vec<u8> {
    let mut frame = Vec::with_capacity(pad4(4 + data.len()));
    frame.extend_from_slice(&channel.to_be_bytes());
    frame.extend_from_slice(&(data.len() as u16).to_be_bytes());
    frame.extend_from_slice(data);
    if pad {
        frame.resize(pad4(frame.len()), 0);
    }
    frame
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Client {
    Udp(SocketAddr),
    Stream(u64),
}

#[derive(Clone)]
enum Sink {
    Udp(Arc<UdpSocket>, SocketAddr),
    Stream(mpsc::UnboundedSender<Vec<u8>>),
}

impl Sink {
    fn is_stream(&self) -> bool {
        matches!(self, Self::Stream(_))
    }
}

struct Allocation {
    username: String,
    relay: Arc<UdpSocket>,
    relay_address: SocketAddr,
    sink: Sink,
    expires: Mutex<Instant>,
    permissions: Mutex<HashMap<IpAddr, Instant>>,
    channels: Mutex<HashMap<u16, SocketAddr>>,
    task: Mutex<Option<JoinHandle<()>>>,
}

impl Allocation {
    fn permitted(&self, ip: IpAddr) -> bool {
        guard(&self.permissions)
            .get(&ip)
            .is_some_and(|until| *until > Instant::now())
    }

    fn channel_peer(&self, channel: u16) -> Option<SocketAddr> {
        guard(&self.channels).get(&channel).copied()
    }

    fn channel_of(&self, peer: SocketAddr) -> Option<u16> {
        guard(&self.channels)
            .iter()
            .find(|(_, address)| **address == peer)
            .map(|(channel, _)| *channel)
    }

    fn stop(&self) {
        if let Some(task) = guard(&self.task).take() {
            task.abort();
        }
    }
}

struct State {
    realm: String,
    users: Mutex<HashMap<String, String>>,
    nonce: Mutex<Vec<u8>>,
    nonce_counter: AtomicU64,
    stale_once: AtomicBool,
    drop_udp: AtomicBool,
    allocations: Mutex<HashMap<Client, Arc<Allocation>>>,
    to_clients: AtomicU64,
    deleted: AtomicUsize,
    next_stream: AtomicU64,
    requests: Mutex<Vec<(u16, String)>>,
}

impl State {
    fn send(&self, sink: &Sink, bytes: Vec<u8>) {
        self.to_clients
            .fetch_add(bytes.len() as u64, Ordering::Relaxed);
        match sink {
            Sink::Udp(socket, to) => {
                let _ = socket.try_send_to(&bytes, *to);
            }
            Sink::Stream(out) => {
                let _ = out.send(bytes);
            }
        }
    }

    fn new_nonce(&self) -> Vec<u8> {
        let nonce = format!(
            "nonce-{}",
            self.nonce_counter.fetch_add(1, Ordering::Relaxed)
        )
        .into_bytes();
        *guard(&self.nonce) = nonce.clone();
        nonce
    }

    fn remove(&self, client: Client) {
        if let Some(allocation) = guard(&self.allocations).remove(&client) {
            allocation.stop();
            self.deleted.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// The server; dropping it stops everything.
pub struct TurnServer {
    pub udp: SocketAddr,
    pub tcp: SocketAddr,
    pub tls: SocketAddr,
    state: Arc<State>,
    tasks: Vec<JoinHandle<()>>,
}

impl Drop for TurnServer {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
        for (_, allocation) in guard(&self.state.allocations).drain() {
            allocation.stop();
        }
    }
}

impl TurnServer {
    /// A server with these users (username, password); TLS with a certificate of `ca` for 127.0.0.1.
    pub async fn start(ca: &TestCa, users: &[(&str, &str)]) -> Self {
        let state = Arc::new(State {
            realm: "pl-host.test".into(),
            users: Mutex::new(
                users
                    .iter()
                    .map(|(user, pass)| ((*user).to_owned(), (*pass).to_owned()))
                    .collect(),
            ),
            nonce: Mutex::new(b"nonce-start".to_vec()),
            nonce_counter: AtomicU64::new(0),
            stale_once: AtomicBool::new(false),
            drop_udp: AtomicBool::new(false),
            allocations: Mutex::new(HashMap::new()),
            to_clients: AtomicU64::new(0),
            deleted: AtomicUsize::new(0),
            next_stream: AtomicU64::new(0),
            requests: Mutex::new(Vec::new()),
        });
        let udp = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let tcp = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let tls = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let acceptor = TlsAcceptor::from(Arc::new(server_tls(ca, &["127.0.0.1"], &[], false)));
        let addresses = (
            udp.local_addr().unwrap(),
            tcp.local_addr().unwrap(),
            tls.local_addr().unwrap(),
        );
        let tasks = vec![
            tokio::spawn(serve_udp(state.clone(), udp)),
            tokio::spawn(serve_tcp(state.clone(), tcp, None)),
            tokio::spawn(serve_tcp(state.clone(), tls, Some(acceptor))),
        ];
        Self {
            udp: addresses.0,
            tcp: addresses.1,
            tls: addresses.2,
            state,
            tasks,
        }
    }

    pub fn add_user(&self, username: &str, password: &str) {
        guard(&self.state.users).insert(username.to_owned(), password.to_owned());
    }

    /// The next authenticated request gets 438 Stale Nonce.
    pub fn stale_nonce_once(&self) {
        self.state.stale_once.store(true, Ordering::Relaxed);
    }

    /// Ignore everything that arrives over UDP (a network that blocks it).
    pub fn drop_udp(&self, drop: bool) {
        self.state.drop_udp.store(drop, Ordering::Relaxed);
    }

    /// Like Cloudflare revoking credentials: the user's allocations stop and its requests get 401.
    pub fn revoke(&self, username: &str) {
        guard(&self.state.users).remove(username);
        let clients: Vec<Client> = guard(&self.state.allocations)
            .iter()
            .filter(|(_, allocation)| allocation.username == username)
            .map(|(client, _)| *client)
            .collect();
        for client in clients {
            self.state.remove(client);
        }
    }

    pub fn allocations(&self) -> usize {
        guard(&self.state.allocations).len()
    }

    pub fn allocations_of(&self, username: &str) -> usize {
        guard(&self.state.allocations)
            .values()
            .filter(|allocation| allocation.username == username)
            .count()
    }

    pub fn deleted(&self) -> usize {
        self.state.deleted.load(Ordering::Relaxed)
    }

    pub fn bytes_to_clients(&self) -> u64 {
        self.state.to_clients.load(Ordering::Relaxed)
    }

    /// How many requests of `method` this user sent with valid credentials.
    pub fn authenticated(&self, method_name: &str, username: &str) -> usize {
        let method = match method_name {
            "allocate" => ALLOCATE,
            "refresh" => REFRESH,
            "permission" => CREATE_PERMISSION,
            "channel" => CHANNEL_BIND,
            other => panic!("unknown method {other}"),
        };
        guard(&self.state.requests)
            .iter()
            .filter(|(kind, user)| *kind == method && user == username)
            .count()
    }
}

async fn serve_udp(state: Arc<State>, socket: Arc<UdpSocket>) {
    let mut buf = vec![0u8; 65_536];
    loop {
        let Ok((len, from)) = socket.recv_from(&mut buf).await else {
            continue;
        };
        if state.drop_udp.load(Ordering::Relaxed) {
            continue;
        }
        handle(
            &state,
            Client::Udp(from),
            &Sink::Udp(socket.clone(), from),
            from,
            &buf[..len],
        );
    }
}

async fn serve_tcp(state: Arc<State>, listener: TcpListener, tls: Option<TlsAcceptor>) {
    loop {
        let Ok((stream, from)) = listener.accept().await else {
            continue;
        };
        let state = state.clone();
        let tls = tls.clone();
        tokio::spawn(async move {
            match tls {
                Some(acceptor) => {
                    if let Ok(stream) = acceptor.accept(stream).await {
                        serve_stream(state, stream, from).await;
                    }
                }
                None => serve_stream(state, stream, from).await,
            }
        });
    }
}

async fn serve_stream<S: AsyncRead + AsyncWrite + Send + 'static>(
    state: Arc<State>,
    stream: S,
    from: SocketAddr,
) {
    let client = Client::Stream(state.next_stream.fetch_add(1, Ordering::Relaxed));
    let (mut reader, mut writer) = tokio::io::split(stream);
    let (out, mut outgoing) = mpsc::unbounded_channel::<Vec<u8>>();
    let writing = tokio::spawn(async move {
        while let Some(bytes) = outgoing.recv().await {
            if writer.write_all(&bytes).await.is_err() {
                break;
            }
        }
    });
    let sink = Sink::Stream(out);
    let mut buf = Vec::new();
    let mut chunk = vec![0u8; 16 * 1024];
    'read: loop {
        while buf.len() >= 4 {
            let len = usize::from(u16::from_be_bytes([buf[2], buf[3]]));
            let total = if buf[0] & 0xC0 == 0 {
                20 + len
            } else {
                pad4(4 + len)
            };
            if buf.len() < total {
                break;
            }
            let frame: Vec<u8> = buf.drain(..total).collect();
            handle(&state, client, &sink, from, &frame);
        }
        match reader.read(&mut chunk).await {
            Ok(0) | Err(_) => break 'read,
            Ok(read) => buf.extend_from_slice(&chunk[..read]),
        }
    }
    // A TCP allocation dies with its connection.
    state.remove(client);
    writing.abort();
}

fn handle(state: &Arc<State>, client: Client, sink: &Sink, from: SocketAddr, frame: &[u8]) {
    let Some(&first) = frame.first() else {
        return;
    };
    let allocation = guard(&state.allocations).get(&client).cloned();
    if (0x40..=0x7F).contains(&first) {
        let len = usize::from(u16::from_be_bytes([frame[2], frame[3]]));
        let channel = u16::from_be_bytes([frame[0], frame[1]]);
        if let Some(allocation) = allocation
            && let Some(peer) = allocation.channel_peer(channel)
            && allocation.permitted(peer.ip())
            && frame.len() >= 4 + len
        {
            let _ = allocation.relay.try_send_to(&frame[4..4 + len], peer);
        }
        return;
    }
    let Some(message) = parse(frame) else {
        return;
    };
    match (message.method, message.class) {
        (SEND, 1) => {
            if let (Some(allocation), Some(peer), Some(data)) = (
                allocation,
                message.attr(XOR_PEER_ADDRESS).and_then(xor_address),
                message.attr(DATA_ATTR),
            ) && allocation.permitted(peer.ip())
            {
                let _ = allocation.relay.try_send_to(data, peer);
            }
        }
        (method, 0) => {
            let response = request(state, client, sink, from, method, &message);
            state.send(sink, response);
        }
        _ => {}
    }
}

fn error(state: &State, message: &Message<'_>, code: u16, challenge: bool) -> Vec<u8> {
    let mut value = vec![0, 0, (code / 100) as u8, (code % 100) as u8];
    value.extend_from_slice(b"error");
    let mut attrs = vec![(ERROR_CODE, value)];
    if challenge {
        attrs.push((REALM, state.realm.clone().into_bytes()));
        attrs.push((NONCE, guard(&state.nonce).clone()));
    }
    build(message.method, 3, message.txid, &attrs, None)
}

fn request(
    state: &Arc<State>,
    client: Client,
    sink: &Sink,
    from: SocketAddr,
    method: u16,
    message: &Message<'_>,
) -> Vec<u8> {
    let (Some(username), Some(_)) = (message.attr(USERNAME), message.attr(MESSAGE_INTEGRITY))
    else {
        return error(state, message, 401, true);
    };
    let username = String::from_utf8_lossy(username).into_owned();
    let Some(password) = guard(&state.users).get(&username).cloned() else {
        return error(state, message, 401, true);
    };
    if message.attr(NONCE) != Some(guard(&state.nonce).as_slice()) {
        return error(state, message, 438, true);
    }
    if message.attr(REALM) != Some(state.realm.as_bytes()) {
        return error(state, message, 401, true);
    }
    let key = Md5::digest(format!("{username}:{}:{password}", state.realm).as_bytes()).to_vec();
    if message.integrity_ok(&key) != Some(true) {
        return error(state, message, 401, true);
    }
    if state.stale_once.swap(false, Ordering::Relaxed) {
        state.new_nonce();
        return error(state, message, 438, true);
    }
    guard(&state.requests).push((method, username.clone()));
    let existing = guard(&state.allocations).get(&client).cloned();
    if let Some(allocation) = &existing
        && allocation.username != username
    {
        return error(state, message, 441, false);
    }
    let lifetime = message
        .attr(LIFETIME)
        .and_then(|value| value.try_into().ok())
        .map(u32::from_be_bytes)
        .unwrap_or(600)
        .min(3600);
    let success = |attrs: &[(u16, Vec<u8>)]| build(method, 2, message.txid, attrs, Some(&key));
    match method {
        ALLOCATE => {
            if existing.is_some() {
                return error(state, message, 437, false);
            }
            if message
                .attr(REQUESTED_TRANSPORT)
                .map(|value| value.first().copied())
                != Some(Some(17))
            {
                return error(state, message, 442, false);
            }
            let std_socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
            std_socket.set_nonblocking(true).unwrap();
            let relay = Arc::new(UdpSocket::from_std(std_socket).unwrap());
            let relay_address = relay.local_addr().unwrap();
            let allocation = Arc::new(Allocation {
                username,
                relay: relay.clone(),
                relay_address,
                sink: sink.clone(),
                expires: Mutex::new(Instant::now() + Duration::from_secs(u64::from(lifetime))),
                permissions: Mutex::new(HashMap::new()),
                channels: Mutex::new(HashMap::new()),
                task: Mutex::new(None),
            });
            *guard(&allocation.task) =
                Some(tokio::spawn(relay_loop(state.clone(), allocation.clone())));
            guard(&state.allocations).insert(client, allocation);
            let mapped = if matches!(client, Client::Udp(_)) {
                from
            } else {
                SocketAddr::from(([127, 0, 0, 1], 0))
            };
            success(&[
                (XOR_RELAYED_ADDRESS, encode_address(relay_address)),
                (XOR_MAPPED_ADDRESS, encode_address(mapped)),
                (LIFETIME, lifetime.to_be_bytes().to_vec()),
            ])
        }
        REFRESH => {
            let Some(allocation) = existing else {
                return error(state, message, 437, false);
            };
            if lifetime == 0 {
                state.remove(client);
            } else {
                *guard(&allocation.expires) =
                    Instant::now() + Duration::from_secs(u64::from(lifetime));
            }
            success(&[(LIFETIME, lifetime.to_be_bytes().to_vec())])
        }
        CREATE_PERMISSION => {
            let Some(allocation) = existing else {
                return error(state, message, 437, false);
            };
            let peers: Vec<SocketAddr> = message
                .all(XOR_PEER_ADDRESS)
                .into_iter()
                .filter_map(xor_address)
                .collect();
            if peers.is_empty() {
                return error(state, message, 400, false);
            }
            for peer in peers {
                guard(&allocation.permissions)
                    .insert(peer.ip(), Instant::now() + PERMISSION_LIFETIME);
            }
            success(&[])
        }
        CHANNEL_BIND => {
            let Some(allocation) = existing else {
                return error(state, message, 437, false);
            };
            let channel = message
                .attr(CHANNEL_NUMBER)
                .filter(|value| value.len() == 4)
                .map(|value| u16::from_be_bytes([value[0], value[1]]));
            let peer = message.attr(XOR_PEER_ADDRESS).and_then(xor_address);
            let (Some(channel), Some(peer)) = (channel, peer) else {
                return error(state, message, 400, false);
            };
            if !(0x4000..=0x4FFF).contains(&channel) {
                return error(state, message, 400, false);
            }
            let mut channels = guard(&allocation.channels);
            let clash = channels.get(&channel).is_some_and(|bound| *bound != peer)
                || channels
                    .iter()
                    .any(|(number, bound)| *bound == peer && *number != channel);
            if clash {
                return error(state, message, 400, false);
            }
            channels.insert(channel, peer);
            drop(channels);
            guard(&allocation.permissions).insert(peer.ip(), Instant::now() + PERMISSION_LIFETIME);
            success(&[])
        }
        _ => error(state, message, 400, false),
    }
}

/// What a peer sends to the relay address reaches the client: ChannelData on a bound channel, a
/// Data indication otherwise; only from permitted IPs.
async fn relay_loop(state: Arc<State>, allocation: Arc<Allocation>) {
    let mut buf = vec![0u8; 65_536];
    loop {
        let Ok((len, from)) = allocation.relay.recv_from(&mut buf).await else {
            continue;
        };
        if !allocation.permitted(from.ip()) || *guard(&allocation.expires) < Instant::now() {
            continue;
        }
        let data = &buf[..len];
        let frame = match allocation.channel_of(from) {
            Some(channel) => channel_data(channel, data, allocation.sink.is_stream()),
            None => build(
                DATA,
                1,
                [9u8; 12],
                &[
                    (XOR_PEER_ADDRESS, encode_address(from)),
                    (DATA_ATTR, data.to_vec()),
                ],
                None,
            ),
        };
        state.send(&allocation.sink, frame);
    }
}

/// The relay address an allocation of this server got (the test asks the client for its own).
pub fn is_loopback_relay(address: SocketAddr) -> bool {
    address.ip() == IpAddr::V4(Ipv4Addr::LOCALHOST)
}
