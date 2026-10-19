//! Minecraft Java Edition Server List Ping (1.7+), with the pre-1.7 legacy ping as a fallback.

use std::fmt;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine;
use hickory_resolver::TokioResolver;
use hickory_resolver::config::{CLOUDFLARE, ResolverConfig};
use hickory_resolver::net::runtime::TokioRuntimeProvider;
use hickory_resolver::proto::rr::RData;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::OnceCell;

use super::chat::{ChatText, parse_chat, parse_legacy, strip_formatting};
use crate::error::{Error, Result};

pub const DEFAULT_PORT: u16 = 25565;

/// Largest status packet accepted. The protocol caps the JSON at 32767 characters, but modded
/// servers routinely send more (mod lists, big favicons).
const MAX_PACKET: usize = 4 * 1024 * 1024;

/// Why a ping failed, so the UI can pick a fitting message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum PingFailure {
    InvalidAddress,
    UnknownHost,
    Refused,
    Timeout,
    /// Connected, but the answer was not a valid status response.
    Protocol,
    Io,
}

#[derive(Debug, Clone)]
pub struct PingOptions {
    /// Limit for connecting and for each read/write step.
    pub timeout: Duration,
    /// Protocol number sent in the handshake. Servers answer with their own version regardless;
    /// some proxies tailor the MOTD to it.
    pub protocol_version: i32,
    /// Try the pre-1.7 ping when the modern one gets an unusable answer.
    pub legacy_fallback: bool,
    /// Look up `_minecraft._tcp.<host>` SRV records (only for the default port, like the game).
    pub resolve_srv: bool,
}

impl Default for PingOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(5),
            protocol_version: 767,
            legacy_fallback: true,
            resolve_srv: true,
        }
    }
}

/// Server status as shown in the server list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ServerPing {
    /// The address that was pinged, as given.
    pub address: String,
    /// The socket address that answered (after SRV and DNS resolution).
    pub connected_to: String,
    pub version: Option<PingVersion>,
    pub players: Option<PingPlayers>,
    pub motd: ChatText,
    /// Decoded server icon (64×64 PNG).
    pub favicon_png: Option<Vec<u8>>,
    pub latency_ms: u64,
    /// Answered through the pre-1.7 legacy ping.
    pub legacy: bool,
    pub enforces_secure_chat: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PingVersion {
    /// Version label; may contain `§` codes (servers use it for status text).
    pub name: String,
    pub name_plain: String,
    pub protocol: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PingPlayers {
    pub online: i64,
    pub max: i64,
    /// Some servers put custom hover text here instead of real players.
    pub sample: Vec<PingPlayer>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PingPlayer {
    pub name: String,
    pub id: String,
}

/// A parsed `host[:port]` server address. IPv6 literals use `[addr]:port`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerAddress {
    pub host: String,
    pub port: u16,
}

impl ServerAddress {
    pub fn parse(input: &str) -> Result<Self> {
        let input = input.trim();
        let invalid = || ping_error(input, PingFailure::InvalidAddress, "invalid server address");
        if input.is_empty() || input.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(invalid());
        }
        let (host, port) = if let Some(rest) = input.strip_prefix('[') {
            let (host, after) = rest.split_once(']').ok_or_else(invalid)?;
            let port = match after {
                "" => None,
                p => Some(p.strip_prefix(':').ok_or_else(invalid)?),
            };
            (host, port)
        } else if input.matches(':').count() > 1 {
            (input, None) // bare IPv6 literal
        } else {
            match input.split_once(':') {
                Some((host, port)) => (host, Some(port)),
                None => (input, None),
            }
        };
        if host.is_empty() {
            return Err(invalid());
        }
        let port = match port {
            Some(p) => p
                .parse::<u16>()
                .ok()
                .filter(|p| *p != 0)
                .ok_or_else(invalid)?,
            None => DEFAULT_PORT,
        };
        Ok(Self {
            host: host.trim_end_matches('.').to_owned(),
            port,
        })
    }

    pub fn ip(&self) -> Option<IpAddr> {
        self.host.parse().ok()
    }
}

impl fmt::Display for ServerAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.host.contains(':') {
            write!(f, "[{}]:{}", self.host, self.port)
        } else {
            write!(f, "{}:{}", self.host, self.port)
        }
    }
}

/// Reusable pinger: owns the DNS resolver used for SRV lookups. Cheap to clone.
#[derive(Clone, Default)]
pub struct ServerPinger {
    resolver: Arc<OnceCell<Option<TokioResolver>>>,
}

impl fmt::Debug for ServerPinger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ServerPinger").finish_non_exhaustive()
    }
}

/// Ping once with a throwaway [`ServerPinger`].
pub async fn ping_server(address: &str, options: &PingOptions) -> Result<ServerPing> {
    ServerPinger::new().ping(address, options).await
}

impl ServerPinger {
    pub fn new() -> Self {
        Self::default()
    }

    async fn resolver(&self) -> Option<&TokioResolver> {
        self.resolver
            .get_or_init(|| async { build_resolver() })
            .await
            .as_ref()
    }

    /// Resolve an address to socket addresses: SRV first (default port only), then A/AAAA.
    pub async fn resolve(
        &self,
        address: &ServerAddress,
        options: &PingOptions,
    ) -> Result<Vec<SocketAddr>> {
        if let Some(ip) = address.ip() {
            return Ok(vec![SocketAddr::new(ip, address.port)]);
        }
        let mut target = (address.host.clone(), address.port);
        if options.resolve_srv
            && address.port == DEFAULT_PORT
            && let Some(found) = self.lookup_srv(&address.host, options.timeout).await
        {
            target = found;
        }
        let label = address.to_string();
        let lookup = tokio::net::lookup_host((target.0.as_str(), target.1));
        match tokio::time::timeout(options.timeout, lookup).await {
            Err(_) => Err(ping_error(
                &label,
                PingFailure::Timeout,
                "DNS lookup timed out",
            )),
            Ok(Err(e)) => Err(ping_error(&label, PingFailure::UnknownHost, e)),
            Ok(Ok(addrs)) => {
                let mut addrs: Vec<SocketAddr> = addrs.collect();
                // Prefer IPv4 like the game (Java's default), keep IPv6 as a fallback.
                addrs.sort_by_key(|a| a.is_ipv6());
                addrs.dedup();
                if addrs.is_empty() {
                    Err(ping_error(&label, PingFailure::UnknownHost, "no addresses"))
                } else {
                    Ok(addrs)
                }
            }
        }
    }

    async fn lookup_srv(&self, host: &str, timeout: Duration) -> Option<(String, u16)> {
        let resolver = self.resolver().await?;
        let name = format!("_minecraft._tcp.{host}.");
        let lookup = tokio::time::timeout(timeout, resolver.srv_lookup(name)).await;
        let lookup = match lookup {
            Ok(Ok(lookup)) => lookup,
            Ok(Err(e)) => {
                tracing::debug!(%host, error = %e, "no SRV record");
                return None;
            }
            Err(_) => {
                tracing::debug!(%host, "SRV lookup timed out");
                return None;
            }
        };
        lookup
            .answers()
            .iter()
            .filter_map(|record| match &record.data {
                RData::SRV(srv) => Some(srv),
                _ => None,
            })
            // Lowest priority wins; among equals prefer the heaviest weight.
            .min_by_key(|srv| (srv.priority, std::cmp::Reverse(srv.weight)))
            .map(|srv| {
                let target = srv.target.to_ascii();
                (target.trim_end_matches('.').to_owned(), srv.port)
            })
            .filter(|(target, port)| !target.is_empty() && *port != 0)
    }

    /// Ping a server (`host`, `host:port`, `[v6]:port`).
    pub async fn ping(&self, address: &str, options: &PingOptions) -> Result<ServerPing> {
        let parsed = ServerAddress::parse(address)?;
        let candidates = self.resolve(&parsed, options).await?;
        let label = parsed.to_string();

        let (mut stream, connected) = connect_any(&candidates, &label, options.timeout).await?;
        let modern = ping_modern(&mut stream, &parsed, options).await;
        drop(stream);
        let error = match modern {
            Ok((status, latency_ms)) => {
                return Ok(build_status(address, connected, status, latency_ms));
            }
            Err(error) => error,
        };
        let retry_legacy = options.legacy_fallback
            && matches!(
                &error,
                Error::Ping {
                    kind: PingFailure::Protocol | PingFailure::Io,
                    ..
                }
            );
        if !retry_legacy {
            return Err(error);
        }
        tracing::debug!(address = %label, %error, "modern ping failed, trying legacy ping");
        let (mut stream, connected) = connect_any(&[connected], &label, options.timeout).await?;
        match ping_legacy(&mut stream, &parsed, options.timeout).await {
            Ok((legacy, latency_ms)) => Ok(ServerPing {
                address: address.to_owned(),
                connected_to: connected.to_string(),
                version: legacy.version.map(|name| PingVersion {
                    name_plain: strip_formatting(&name),
                    name,
                    protocol: legacy.protocol.unwrap_or(-1),
                }),
                players: Some(PingPlayers {
                    online: legacy.online,
                    max: legacy.max,
                    sample: Vec::new(),
                }),
                motd: parse_legacy(&legacy.motd),
                favicon_png: None,
                latency_ms,
                legacy: true,
                enforces_secure_chat: None,
            }),
            // The modern error is usually the more telling one.
            Err(_) => Err(error),
        }
    }
}

fn build_resolver() -> Option<TokioResolver> {
    let configure = |mut builder: hickory_resolver::ResolverBuilder<TokioRuntimeProvider>| {
        let options = builder.options_mut();
        options.timeout = Duration::from_secs(2);
        options.attempts = 2;
        // Windows: hickory binds each UDP query to 0.0.0.0 on a port it picks itself, which
        // Windows Defender Firewall takes for a server: at the first server ping (seconds after
        // the first start) players got a "Windows Security" prompt to let Projects Launcher on
        // the network (E2E2, 19 Oct 2026; hickory documents it on this option). The OS's own
        // ephemeral port raises none.
        options.os_port_selection = cfg!(windows);
        builder.build()
    };
    let system = TokioResolver::builder_tokio()
        .map_err(|e| e.to_string())
        .and_then(|builder| configure(builder).map_err(|e| e.to_string()));
    match system {
        Ok(resolver) => Some(resolver),
        Err(error) => {
            tracing::debug!(%error, "system DNS config unavailable, using public resolvers");
            let config = ResolverConfig::udp_and_tcp(&CLOUDFLARE);
            let builder = hickory_resolver::Resolver::builder_with_config(
                config,
                TokioRuntimeProvider::default(),
            );
            match configure(builder) {
                Ok(resolver) => Some(resolver),
                Err(error) => {
                    tracing::warn!(%error, "no DNS resolver for SRV lookups");
                    None
                }
            }
        }
    }
}

fn ping_error(address: &str, kind: PingFailure, message: impl fmt::Display) -> Error {
    Error::Ping {
        address: address.to_owned(),
        kind,
        message: message.to_string(),
    }
}

fn io_failure(address: &str, error: std::io::Error) -> Error {
    use std::io::ErrorKind;
    let kind = match error.kind() {
        ErrorKind::ConnectionRefused => PingFailure::Refused,
        ErrorKind::TimedOut => PingFailure::Timeout,
        ErrorKind::UnexpectedEof | ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted => {
            PingFailure::Protocol
        }
        _ => PingFailure::Io,
    };
    ping_error(address, kind, error)
}

async fn connect_any(
    candidates: &[SocketAddr],
    label: &str,
    timeout: Duration,
) -> Result<(TcpStream, SocketAddr)> {
    let mut last = None;
    for addr in candidates {
        match tokio::time::timeout(timeout, TcpStream::connect(addr)).await {
            Ok(Ok(stream)) => {
                let _ = stream.set_nodelay(true);
                return Ok((stream, *addr));
            }
            Ok(Err(e)) => last = Some(io_failure(label, e)),
            Err(_) => last = Some(ping_error(label, PingFailure::Timeout, "connect timed out")),
        }
    }
    Err(last.unwrap_or_else(|| ping_error(label, PingFailure::UnknownHost, "no addresses")))
}

async fn with_timeout<T>(
    label: &str,
    timeout: Duration,
    step: &str,
    fut: impl Future<Output = std::io::Result<T>>,
) -> Result<T> {
    match tokio::time::timeout(timeout, fut).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(e)) => Err(io_failure(label, e)),
        Err(_) => Err(ping_error(
            label,
            PingFailure::Timeout,
            format!("{step} timed out"),
        )),
    }
}

// --- VarInt framing -------------------------------------------------------------------------

pub(crate) fn write_varint(out: &mut Vec<u8>, value: i32) {
    let mut value = value as u32;
    loop {
        if value & !0x7f == 0 {
            out.push(value as u8);
            return;
        }
        out.push((value & 0x7f) as u8 | 0x80);
        value >>= 7;
    }
}

/// Decode a VarInt from the front of `bytes`, returning the value and its encoded length.
pub(crate) fn read_varint_slice(bytes: &[u8]) -> Option<(i32, usize)> {
    let mut value = 0u32;
    for (index, byte) in bytes.iter().enumerate().take(5) {
        value |= u32::from(byte & 0x7f) << (7 * index);
        if byte & 0x80 == 0 {
            return Some((value as i32, index + 1));
        }
    }
    None
}

async fn read_varint<R: AsyncRead + Unpin>(reader: &mut R) -> std::io::Result<i32> {
    let mut value = 0u32;
    for index in 0..5 {
        let byte = reader.read_u8().await?;
        value |= u32::from(byte & 0x7f) << (7 * index);
        if byte & 0x80 == 0 {
            return Ok(value as i32);
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "VarInt is too long",
    ))
}

fn write_string(out: &mut Vec<u8>, value: &str) {
    write_varint(out, value.len() as i32);
    out.extend_from_slice(value.as_bytes());
}

/// Frame a packet: VarInt length, then VarInt id and payload.
fn frame(packet_id: i32, payload: &[u8]) -> Vec<u8> {
    let mut body = Vec::with_capacity(payload.len() + 5);
    write_varint(&mut body, packet_id);
    body.extend_from_slice(payload);
    let mut out = Vec::with_capacity(body.len() + 5);
    write_varint(&mut out, body.len() as i32);
    out.extend_from_slice(&body);
    out
}

async fn read_packet<R: AsyncRead + Unpin>(reader: &mut R) -> std::io::Result<(i32, Vec<u8>)> {
    let invalid = |msg: &str| std::io::Error::new(std::io::ErrorKind::InvalidData, msg.to_owned());
    let length = read_varint(reader).await?;
    let length = usize::try_from(length).map_err(|_| invalid("negative packet length"))?;
    if length == 0 || length > MAX_PACKET {
        return Err(invalid("packet length out of range"));
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).await?;
    let (id, used) = read_varint_slice(&body).ok_or_else(|| invalid("bad packet id"))?;
    body.drain(..used);
    Ok((id, body))
}

fn handshake(address: &ServerAddress, protocol: i32) -> Vec<u8> {
    let mut payload = Vec::new();
    write_varint(&mut payload, protocol);
    write_string(&mut payload, &address.host);
    payload.extend_from_slice(&address.port.to_be_bytes());
    write_varint(&mut payload, 1); // next state: status
    frame(0x00, &payload)
}

async fn ping_modern(
    stream: &mut TcpStream,
    address: &ServerAddress,
    options: &PingOptions,
) -> Result<(Value, u64)> {
    let label = address.to_string();
    let timeout = options.timeout;
    let protocol_error = |msg: String| ping_error(&label, PingFailure::Protocol, msg);

    let mut request = handshake(address, options.protocol_version);
    request.extend_from_slice(&frame(0x00, &[]));
    let started = Instant::now();
    with_timeout(
        &label,
        timeout,
        "status request",
        stream.write_all(&request),
    )
    .await?;
    let (id, body) = with_timeout(&label, timeout, "status response", read_packet(stream)).await?;
    let status_rtt = started.elapsed();
    if id != 0x00 {
        return Err(protocol_error(format!("unexpected packet 0x{id:02x}")));
    }
    let (json_len, used) =
        read_varint_slice(&body).ok_or_else(|| protocol_error("bad string length".into()))?;
    let json = usize::try_from(json_len)
        .ok()
        .and_then(|len| body.get(used..used + len))
        .ok_or_else(|| protocol_error("truncated status string".into()))?;
    let status: Value = serde_json::from_slice(json)
        .map_err(|e| protocol_error(format!("status is not JSON: {e}")))?;
    if !status.is_object() {
        return Err(protocol_error("status JSON is not an object".into()));
    }

    // Ping/pong for latency. Some servers close the connection instead: fall back to the
    // status round trip.
    let payload = jiff::Timestamp::now().as_millisecond();
    let ping = frame(0x01, &payload.to_be_bytes());
    let started = Instant::now();
    let pong = async {
        stream.write_all(&ping).await?;
        read_packet(stream).await
    };
    let latency = match tokio::time::timeout(timeout, pong).await {
        Ok(Ok((0x01, body))) if body.len() >= 8 => started.elapsed(),
        _ => status_rtt,
    };
    Ok((status, latency.as_millis() as u64))
}

fn build_status(
    address: &str,
    connected: SocketAddr,
    status: Value,
    latency_ms: u64,
) -> ServerPing {
    let version = status.get("version").and_then(Value::as_object).map(|v| {
        let name = v
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        PingVersion {
            name_plain: strip_formatting(&name),
            name,
            protocol: v.get("protocol").and_then(int_like).unwrap_or(-1),
        }
    });
    let players = status.get("players").and_then(Value::as_object).map(|p| {
        let sample = p
            .get("sample")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| {
                        Some(PingPlayer {
                            name: item.get("name")?.as_str()?.to_owned(),
                            id: item
                                .get("id")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_owned(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        PingPlayers {
            online: p.get("online").and_then(int_like).unwrap_or(0),
            max: p.get("max").and_then(int_like).unwrap_or(0),
            sample,
        }
    });
    let motd = parse_chat(status.get("description").unwrap_or(&Value::Null));
    let favicon_png = status
        .get("favicon")
        .and_then(Value::as_str)
        .and_then(decode_favicon);
    let enforces_secure_chat = status.get("enforcesSecureChat").and_then(Value::as_bool);
    ServerPing {
        address: address.to_owned(),
        connected_to: connected.to_string(),
        version,
        players,
        motd,
        favicon_png,
        latency_ms,
        legacy: false,
        enforces_secure_chat,
    }
}

fn int_like(value: &Value) -> Option<i64> {
    match value {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// `data:image/png;base64,...` (whitespace tolerated) → PNG bytes, if it really is a PNG.
pub fn decode_favicon(data_url: &str) -> Option<Vec<u8>> {
    let data = match data_url.split_once(',') {
        Some((header, data)) if header.starts_with("data:") => data,
        _ => data_url,
    };
    let cleaned: String = data.chars().filter(|c| !c.is_whitespace()).collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(cleaned.as_bytes())
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(cleaned.as_bytes()))
        .ok()?;
    bytes.starts_with(b"\x89PNG\r\n\x1a\n").then_some(bytes)
}

// --- Legacy (pre-1.7) ping ------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
struct LegacyStatus {
    protocol: Option<i64>,
    version: Option<String>,
    motd: String,
    online: i64,
    max: i64,
}

/// The 1.6 ping request (`FE 01` + `MC|PingHost` plugin message), understood by 1.4–1.6 servers,
/// older servers (which ignore the extra bytes) and modern servers' legacy handler.
fn legacy_request(address: &ServerAddress) -> Vec<u8> {
    let utf16 = |s: &str| -> Vec<u8> { s.encode_utf16().flat_map(u16::to_be_bytes).collect() };
    let channel = "MC|PingHost";
    let host = utf16(&address.host);
    let host_chars = address.host.encode_utf16().count() as u16;
    let mut out = vec![0xfe, 0x01, 0xfa];
    out.extend_from_slice(&(channel.len() as u16).to_be_bytes());
    out.extend_from_slice(&utf16(channel));
    out.extend_from_slice(&(7 + host.len() as u16).to_be_bytes());
    out.push(78); // protocol 78 = 1.6.4
    out.extend_from_slice(&host_chars.to_be_bytes());
    out.extend_from_slice(&host);
    out.extend_from_slice(&u32::from(address.port).to_be_bytes());
    out
}

fn parse_legacy_response(text: &str) -> Option<LegacyStatus> {
    if let Some(rest) = text.strip_prefix("§1\0") {
        // 1.4+: §1 \0 protocol \0 version \0 motd \0 online \0 max
        let parts: Vec<&str> = rest.split('\0').collect();
        if parts.len() < 5 {
            return None;
        }
        return Some(LegacyStatus {
            protocol: parts[0].parse().ok(),
            version: Some(parts[1].to_owned()),
            motd: parts[2].to_owned(),
            online: parts[3].trim().parse().unwrap_or(0),
            max: parts[4].trim().parse().unwrap_or(0),
        });
    }
    // Beta 1.8 – 1.3: motd § online § max
    let mut parts = text.rsplitn(3, '§');
    let max = parts.next()?.trim().parse().ok()?;
    let online = parts.next()?.trim().parse().ok()?;
    let motd = parts.next()?.to_owned();
    Some(LegacyStatus {
        protocol: None,
        version: None,
        motd,
        online,
        max,
    })
}

async fn ping_legacy(
    stream: &mut TcpStream,
    address: &ServerAddress,
    timeout: Duration,
) -> Result<(LegacyStatus, u64)> {
    let label = address.to_string();
    let started = Instant::now();
    with_timeout(
        &label,
        timeout,
        "legacy request",
        stream.write_all(&legacy_request(address)),
    )
    .await?;
    let read = async {
        let kick = stream.read_u8().await?;
        if kick != 0xff {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "not a legacy kick packet",
            ));
        }
        let chars = stream.read_u16().await? as usize;
        let mut raw = vec![0u8; chars * 2];
        stream.read_exact(&mut raw).await?;
        Ok(raw)
    };
    let raw = with_timeout(&label, timeout, "legacy response", read).await?;
    let latency = started.elapsed().as_millis() as u64;
    let units: Vec<u16> = raw
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_be_bytes(*pair))
        .collect();
    let text = String::from_utf16_lossy(&units);
    let status = parse_legacy_response(&text).ok_or_else(|| {
        ping_error(
            &label,
            PingFailure::Protocol,
            "unrecognized legacy response",
        )
    })?;
    Ok((status, latency))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::net::TcpListener;

    /// Tauri state and async commands need these to be `Send`/`Sync`.
    #[test]
    fn pinger_and_futures_are_send() {
        fn send<T: Send>(_: &T) {}
        fn send_sync<T: Send + Sync>(_: &T) {}
        let pinger = ServerPinger::new();
        send_sync(&pinger);
        let options = PingOptions::default();
        send(&pinger.ping("localhost", &options));
        send(&ping_server("localhost", &options));
        let path = std::path::Path::new("servers.dat");
        send(&crate::server::ServerList::read(path));
        let list = crate::server::ServerList::default();
        send(&list.write(path));
    }

    /// Windows Defender Firewall prompts for an app whose UDP socket is bound to 0.0.0.0 on a port
    /// the app picked (hickory's default): there the SRV lookups let the OS pick it.
    #[tokio::test]
    async fn srv_lookups_let_windows_pick_the_udp_port() {
        let resolver = build_resolver().expect("a resolver");
        assert_eq!(resolver.options().os_port_selection, cfg!(windows));
    }

    #[test]
    fn varint_round_trip_and_known_encodings() {
        let cases: [(i32, &[u8]); 7] = [
            (0, &[0x00]),
            (1, &[0x01]),
            (127, &[0x7f]),
            (128, &[0x80, 0x01]),
            (25565, &[0xdd, 0xc7, 0x01]),
            (2147483647, &[0xff, 0xff, 0xff, 0xff, 0x07]),
            (-1, &[0xff, 0xff, 0xff, 0xff, 0x0f]),
        ];
        for (value, bytes) in cases {
            let mut out = Vec::new();
            write_varint(&mut out, value);
            assert_eq!(out, bytes, "encoding {value}");
            assert_eq!(read_varint_slice(bytes), Some((value, bytes.len())));
        }
        assert_eq!(read_varint_slice(&[0x80, 0x80]), None);
        assert_eq!(read_varint_slice(&[0xff; 6]), None);
    }

    #[test]
    fn address_parsing() {
        let a = ServerAddress::parse(" play.projects.gg ").unwrap();
        assert_eq!((a.host.as_str(), a.port), ("play.projects.gg", 25565));
        let a = ServerAddress::parse("mc.example.com:25570").unwrap();
        assert_eq!(a.port, 25570);
        let a = ServerAddress::parse("[::1]:25566").unwrap();
        assert_eq!((a.host.as_str(), a.port), ("::1", 25566));
        assert_eq!(a.to_string(), "[::1]:25566");
        let a = ServerAddress::parse("2001:db8::1").unwrap();
        assert_eq!(a.port, 25565);
        assert!(a.ip().is_some());
        for bad in ["", "host:", "host:99999", "host:0", ":25565", "[::1", "a b"] {
            assert!(ServerAddress::parse(bad).is_err(), "{bad:?} should fail");
        }
    }

    #[test]
    fn legacy_response_formats() {
        let modern =
            parse_legacy_response(&["§1", "127", "1.6.4", "§aA server", "3", "20"].join("\0"))
                .unwrap();
        assert_eq!(modern.protocol, Some(127));
        assert_eq!(modern.version.as_deref(), Some("1.6.4"));
        assert_eq!(modern.motd, "§aA server");
        assert_eq!((modern.online, modern.max), (3, 20));
        let old = parse_legacy_response("Old § server§5§10").unwrap();
        assert_eq!(old.motd, "Old § server");
        assert_eq!((old.online, old.max), (5, 10));
        assert!(parse_legacy_response("garbage").is_none());
    }

    #[test]
    fn legacy_request_layout() {
        let req = legacy_request(&ServerAddress::parse("ab:25565").unwrap());
        assert_eq!(&req[..3], &[0xfe, 0x01, 0xfa]);
        // channel length 11, then "MC|PingHost" in UTF-16BE
        assert_eq!(&req[3..5], &[0, 11]);
        let rest = &req[5 + 22..];
        assert_eq!(&rest[..2], &[0, 7 + 4]); // 7 + 2 chars * 2 bytes
        assert_eq!(rest[2], 78);
        assert_eq!(&rest[3..5], &[0, 2]);
        assert_eq!(&rest[5..9], &[0, b'a', 0, b'b']);
        assert_eq!(&rest[9..], &25565u32.to_be_bytes());
    }

    #[test]
    fn favicon_decoding() {
        let png = b"\x89PNG\r\n\x1a\nrest";
        let b64 = base64::engine::general_purpose::STANDARD.encode(png);
        let url = format!("data:image/png;base64,{}\n{}", &b64[..4], &b64[4..]);
        assert_eq!(decode_favicon(&url).unwrap(), png);
        assert!(decode_favicon("data:image/png;base64,bm90IGEgcG5n").is_none());
        assert!(decode_favicon("%%%").is_none());
    }

    #[test]
    fn status_json_mapping() {
        let png = base64::engine::general_purpose::STANDARD.encode(b"\x89PNG\r\n\x1a\n");
        let status = json!({
            "version": {"name": "§cPaper 1.21.1", "protocol": 767},
            "players": {"online": 5, "max": "100", "sample": [{"name": "Steve", "id": "u"}, {"bad": 1}]},
            "description": {"text": "Hi ", "extra": [{"text": "there", "color": "gold"}]},
            "favicon": format!("data:image/png;base64,{png}"),
            "enforcesSecureChat": true
        });
        let addr: SocketAddr = "127.0.0.1:25565".parse().unwrap();
        let ping = build_status("example", addr, status, 12);
        let version = ping.version.unwrap();
        assert_eq!(version.name_plain, "Paper 1.21.1");
        assert_eq!(version.protocol, 767);
        let players = ping.players.unwrap();
        assert_eq!((players.online, players.max), (5, 100));
        assert_eq!(players.sample.len(), 1);
        assert_eq!(ping.motd.plain, "Hi there");
        assert!(ping.favicon_png.is_some());
        assert_eq!(ping.enforces_secure_chat, Some(true));
        assert_eq!(ping.latency_ms, 12);
    }

    /// A minimal in-process server speaking the status protocol.
    async fn fake_server(answer_ping: bool) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let (id, body) = read_packet(&mut socket).await.unwrap();
            assert_eq!(id, 0x00);
            let (protocol, _) = read_varint_slice(&body).unwrap();
            assert_eq!(protocol, 767);
            let (id, _) = read_packet(&mut socket).await.unwrap();
            assert_eq!(id, 0x00);
            let json = r#"{"version":{"name":"Test","protocol":767},"players":{"online":1,"max":2},"description":"§bFake"}"#;
            let mut payload = Vec::new();
            write_string(&mut payload, json);
            socket.write_all(&frame(0x00, &payload)).await.unwrap();
            if answer_ping {
                let (id, body) = read_packet(&mut socket).await.unwrap();
                assert_eq!(id, 0x01);
                socket.write_all(&frame(0x01, &body)).await.unwrap();
            }
        });
        addr
    }

    #[tokio::test]
    async fn pings_a_local_server() {
        for answer_ping in [true, false] {
            let addr = fake_server(answer_ping).await;
            let ping = ping_server(&addr.to_string(), &PingOptions::default())
                .await
                .unwrap();
            assert_eq!(ping.motd.plain, "Fake");
            assert_eq!(ping.motd.spans[0].color.as_deref(), Some("#55ffff"));
            assert_eq!(ping.players.unwrap().max, 2);
            assert!(!ping.legacy);
        }
    }

    #[tokio::test]
    async fn legacy_fallback_against_old_server() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            // First connection: the modern handshake gets a legacy kick that is not a valid frame.
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 64];
            let _ = socket.read(&mut buf).await;
            let _ = socket
                .write_all(&[0xff, 0xff, 0xff, 0xff, 0xff, 0xff])
                .await;
            // Drain until the client hangs up so closing never turns into a reset.
            let _ = socket.read_to_end(&mut Vec::new()).await;
            // Second connection: legacy ping.
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut first = [0u8; 2];
            socket.read_exact(&mut first).await.unwrap();
            assert_eq!(first, [0xfe, 0x01]);
            let text = ["§1", "078", "1.6.4", "Old MOTD", "4", "8"].join("\0");
            let units: Vec<u16> = text.encode_utf16().collect();
            let mut out = vec![0xff];
            out.extend_from_slice(&(units.len() as u16).to_be_bytes());
            out.extend(units.iter().flat_map(|u| u.to_be_bytes()));
            socket.write_all(&out).await.unwrap();
            let _ = socket.read_to_end(&mut Vec::new()).await;
        });
        let ping = ping_server(&addr.to_string(), &PingOptions::default())
            .await
            .unwrap();
        assert!(ping.legacy);
        assert_eq!(ping.motd.plain, "Old MOTD");
        assert_eq!(ping.players.unwrap().online, 4);
        assert_eq!(ping.version.unwrap().protocol, 78);
    }

    #[tokio::test]
    async fn refused_connection_is_classified() {
        // Bind then drop to get a port that is (very likely) closed.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let options = PingOptions {
            timeout: Duration::from_secs(2),
            ..PingOptions::default()
        };
        let err = ping_server(&addr.to_string(), &options).await.unwrap_err();
        match err {
            Error::Ping { kind, .. } => {
                assert!(matches!(kind, PingFailure::Refused | PingFailure::Timeout))
            }
            other => panic!("unexpected error {other:?}"),
        }
    }

    // Live pings. Run with: cargo test -p pl-mc -- --ignored live_ping
    #[tokio::test]
    #[ignore = "needs the internet"]
    async fn live_ping_projects_and_hypixel() {
        let pinger = ServerPinger::new();
        for host in ["play.projects.gg", "mc.hypixel.net"] {
            let ping = pinger.ping(host, &PingOptions::default()).await;
            match &ping {
                Ok(p) => println!(
                    "{host}: {} via {} | {}ms | players {:?} | version {:?} | favicon {} bytes | motd {:?}",
                    if p.legacy { "legacy" } else { "modern" },
                    p.connected_to,
                    p.latency_ms,
                    p.players.as_ref().map(|p| (p.online, p.max)),
                    p.version.as_ref().map(|v| (&v.name_plain, v.protocol)),
                    p.favicon_png.as_ref().map_or(0, Vec::len),
                    p.motd.plain
                ),
                Err(e) => println!("{host}: {e}"),
            }
            assert!(ping.is_ok(), "{host} did not answer");
        }
    }
}
