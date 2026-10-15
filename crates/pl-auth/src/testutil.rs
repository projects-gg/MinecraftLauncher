//! Test helpers: a tiny HTTP/1.1 mock server on 127.0.0.1 and canned Microsoft / Xbox /
//! Minecraft Services answers. All tokens here are dummies.

use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use parking_lot::Mutex;
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, ToSocketAddrs};
use uuid::Uuid;

use crate::config::{AuthConfig, Endpoints};

type Handler = Arc<dyn Fn(&Recorded) -> MockReply + Send + Sync>;

/// A request the mock server received.
#[derive(Debug, Clone)]
pub(crate) struct Recorded {
    pub method: String,
    pub path: String,
    pub query: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Recorded {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    pub fn form(&self) -> HashMap<String, String> {
        url::form_urlencoded::parse(&self.body)
            .into_owned()
            .collect()
    }

    pub fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body).unwrap()
    }
}

pub(crate) struct MockReply {
    pub status: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
}

impl MockReply {
    pub fn json(status: u16, value: serde_json::Value) -> Self {
        Self {
            status,
            content_type: "application/json",
            body: value.to_string().into_bytes(),
        }
    }

    pub fn bytes(status: u16, content_type: &'static str, body: Vec<u8>) -> Self {
        Self {
            status,
            content_type,
            body,
        }
    }
}

pub(crate) struct MockServer {
    base: String,
    log: Arc<Mutex<Vec<Recorded>>>,
    task: tokio::task::JoinHandle<()>,
}

impl MockServer {
    pub async fn start(handler: impl Fn(&Recorded) -> MockReply + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let log = Arc::new(Mutex::new(Vec::new()));
        let handler: Handler = Arc::new(handler);
        let task_log = Arc::clone(&log);
        let task = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    continue;
                };
                let handler = Arc::clone(&handler);
                let log = Arc::clone(&task_log);
                tokio::spawn(async move {
                    let _ = serve(stream, handler, log).await;
                });
            }
        });
        Self {
            base: format!("http://{addr}"),
            log,
            task,
        }
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    pub fn requests(&self) -> Vec<Recorded> {
        self.log.lock().clone()
    }

    pub fn hits(&self, path: &str) -> usize {
        self.log
            .lock()
            .iter()
            .filter(|request| request.path == path)
            .count()
    }

    /// The single request made to `path` (panics unless there was exactly one).
    pub fn only(&self, path: &str) -> Recorded {
        let matching: Vec<Recorded> = self
            .requests()
            .into_iter()
            .filter(|request| request.path == path)
            .collect();
        assert_eq!(matching.len(), 1, "requests to {path}: {matching:?}");
        matching.into_iter().next().unwrap()
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn serve(
    mut stream: TcpStream,
    handler: Handler,
    log: Arc<Mutex<Vec<Recorded>>>,
) -> std::io::Result<()> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 8192];
    let head_end = loop {
        if let Some(position) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            break position + 4;
        }
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            return Ok(());
        }
        buffer.extend_from_slice(&chunk[..read]);
    };
    let head = String::from_utf8_lossy(&buffer[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut request_line = lines.next().unwrap_or_default().split_whitespace();
    let method = request_line.next().unwrap_or_default().to_owned();
    let target = request_line.next().unwrap_or_default().to_owned();
    let headers: Vec<(String, String)> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(key, value)| (key.trim().to_owned(), value.trim().to_owned()))
        .collect();
    let header = |name: &str| {
        headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.clone())
    };

    let mut body = buffer[head_end..].to_vec();
    if let Some(length) = header("content-length").and_then(|value| value.parse::<usize>().ok()) {
        while body.len() < length {
            let read = stream.read(&mut chunk).await?;
            if read == 0 {
                break;
            }
            body.extend_from_slice(&chunk[..read]);
        }
    } else if header("transfer-encoding").is_some_and(|value| value.contains("chunked")) {
        while !body.windows(5).any(|window| window == b"0\r\n\r\n") {
            let read = stream.read(&mut chunk).await?;
            if read == 0 {
                break;
            }
            body.extend_from_slice(&chunk[..read]);
        }
        body = dechunk(&body);
    }

    let (path, query) = match target.split_once('?') {
        Some((path, query)) => (path.to_owned(), query.to_owned()),
        None => (target.clone(), String::new()),
    };
    let recorded = Recorded {
        method,
        path,
        query,
        headers,
        body,
    };
    let reply = handler(&recorded);
    log.lock().push(recorded);

    let head = format!(
        "HTTP/1.1 {} Mock\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        reply.status,
        reply.content_type,
        reply.body.len()
    );
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(&reply.body).await?;
    stream.flush().await?;
    stream.shutdown().await
}

fn dechunk(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut rest = raw;
    while let Some(line_end) = rest.windows(2).position(|window| window == b"\r\n") {
        let size_text = String::from_utf8_lossy(&rest[..line_end]);
        let Ok(size) = usize::from_str_radix(size_text.split(';').next().unwrap_or("0").trim(), 16)
        else {
            break;
        };
        if size == 0 {
            break;
        }
        let start = line_end + 2;
        let end = (start + size).min(rest.len());
        out.extend_from_slice(&rest[start..end]);
        rest = rest.get(end + 2..).unwrap_or_default();
    }
    out
}

/// A plain GET over a raw socket, returning the whole response (head and body).
pub(crate) async fn raw_get(addr: impl ToSocketAddrs, target: &str) -> String {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!("GET {target} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();
    String::from_utf8_lossy(&response).into_owned()
}

pub(crate) fn http() -> pl_net::Http {
    let mut config = pl_net::HttpConfig::new("ProjectsLauncher-tests");
    config.connect_timeout = Duration::from_secs(5);
    pl_net::Http::new(config).unwrap()
}

pub(crate) fn config_for_base(base: &str) -> AuthConfig {
    AuthConfig {
        endpoints: Endpoints::with_base(base),
        request_timeout: Duration::from_secs(10),
        connect_retries: 0,
        ..AuthConfig::default()
    }
}

pub(crate) fn config(server: &MockServer) -> AuthConfig {
    config_for_base(server.base())
}

pub(crate) fn profile_uuid() -> Uuid {
    Uuid::parse_str("069a79f444e94726a5befca90e38aaf5").unwrap()
}

/// A JWT-shaped dummy Minecraft token whose payload carries an `xuid` claim.
pub(crate) fn mc_token() -> String {
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"HS256"}"#);
    let payload = URL_SAFE_NO_PAD.encode(br#"{"xuid":"2535400000000001","exp":4102444800}"#);
    format!("{header}.{payload}.dummy-signature")
}

/// The first 24 bytes of a PNG with the given IHDR size (enough for the validators).
pub(crate) fn png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR".to_vec();
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
    bytes
}

pub(crate) fn profile_json() -> serde_json::Value {
    json!({
        "id": "069a79f444e94726a5befca90e38aaf5",
        "name": "Steve",
        "skins": [{
            "id": "skin-1",
            "state": "ACTIVE",
            "url": "http://textures.minecraft.net/texture/abc",
            "textureKey": "abc",
            "variant": "CLASSIC"
        }],
        "capes": [{
            "id": "cape-1",
            "state": "ACTIVE",
            "url": "http://textures.minecraft.net/texture/cape",
            "alias": "Migrator"
        }],
        "profileActions": {}
    })
}

/// Successful answers for every endpoint the crate calls.
pub(crate) fn happy_chain(request: &Recorded) -> MockReply {
    match (request.method.as_str(), request.path.as_str()) {
        ("POST", "/consumers/oauth2/v2.0/token") => MockReply::json(
            200,
            json!({
                "token_type": "Bearer",
                "scope": "XboxLive.signin offline_access",
                "expires_in": 3600,
                "access_token": "ms-access",
                "refresh_token": "ms-refresh-new"
            }),
        ),
        ("POST", "/user/authenticate") => MockReply::json(
            200,
            json!({
                "IssueInstant": "2026-10-15T10:00:00.0000000Z",
                "NotAfter": "2026-10-29T10:00:00.0000000Z",
                "Token": "xbl-token",
                "DisplayClaims": {"xui": [{"uhs": "user-hash"}]}
            }),
        ),
        ("POST", "/xsts/authorize") => MockReply::json(
            200,
            json!({
                "Token": "xsts-token",
                "DisplayClaims": {"xui": [{"uhs": "user-hash"}]}
            }),
        ),
        ("POST", "/authentication/login_with_xbox") => MockReply::json(
            200,
            json!({
                "username": "00000000-0000-0000-0000-000000000000",
                "roles": [],
                "access_token": mc_token(),
                "token_type": "Bearer",
                "expires_in": 86400
            }),
        ),
        ("GET", "/entitlements/license") => MockReply::json(
            200,
            json!({"items": [
                {"name": "product_minecraft", "source": "PURCHASE"},
                {"name": "game_minecraft", "source": "PURCHASE"}
            ], "signature": "dummy", "keyId": "1"}),
        ),
        ("GET", "/minecraft/profile")
        | ("POST", "/minecraft/profile/skins")
        | ("DELETE", "/minecraft/profile/skins/active")
        | ("PUT" | "DELETE", "/minecraft/profile/capes/active") => {
            MockReply::json(200, profile_json())
        }
        ("GET", "/textures/skin.png") => MockReply::bytes(200, "image/png", png(64, 64)),
        _ => MockReply::json(404, json!({"error": "not found"})),
    }
}
