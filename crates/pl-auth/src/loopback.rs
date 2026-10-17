//! Loopback redirect listener for the browser sign-in (RFC 8252 §7.3).
//!
//! Listens on an OS-assigned port on both `127.0.0.1` and `::1`: the redirect URI says
//! `localhost`, and browsers may resolve that to either address. Each connection is served on its
//! own task with a short deadline, so speculative pre-connects and `/favicon.ico` requests never
//! block the real redirect.

use std::collections::HashMap;
use std::io;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use std::time::Duration;

use jiff::Timestamp;
use pl_net::CancellationToken;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::task::JoinSet;

use crate::error::{AuthError, Result, sanitize};
use crate::page::{PAGE_CSP, result_page};

const MAX_REQUEST_HEAD: usize = 16 * 1024;
const CONNECTION_DEADLINE: Duration = Duration::from_secs(15);

pub(crate) struct LoopbackListener {
    listeners: Vec<TcpListener>,
    port: u16,
}

impl LoopbackListener {
    /// Binds `127.0.0.1:0`, then the same port on `::1`. A port taken on IPv6 is retried with a
    /// fresh one; without IPv6 the listener runs on IPv4 alone.
    pub async fn bind() -> io::Result<Self> {
        let mut ipv6_collision = None;
        for _ in 0..8 {
            let v4 = match TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await {
                Ok(listener) => listener,
                Err(v4_error) => {
                    // No IPv4 loopback at all (rare): IPv6 alone.
                    let v6 = TcpListener::bind((Ipv6Addr::LOCALHOST, 0))
                        .await
                        .map_err(|_| v4_error)?;
                    let port = v6.local_addr()?.port();
                    return Ok(Self {
                        listeners: vec![v6],
                        port,
                    });
                }
            };
            let port = v4.local_addr()?.port();
            match TcpListener::bind((Ipv6Addr::LOCALHOST, port)).await {
                Ok(v6) => {
                    return Ok(Self {
                        listeners: vec![v4, v6],
                        port,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AddrInUse => {
                    ipv6_collision = Some(error);
                }
                Err(error) => {
                    tracing::debug!(%error, "IPv6 loopback unavailable, listening on IPv4 only");
                    return Ok(Self {
                        listeners: vec![v4],
                        port,
                    });
                }
            }
        }
        tracing::debug!(error = ?ipv6_collision, "IPv6 ports kept colliding, listening on IPv4 only");
        let v4 = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
        let port = v4.local_addr()?.port();
        Ok(Self {
            listeners: vec![v4],
            port,
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    #[cfg(test)]
    pub fn has_ipv6(&self) -> bool {
        self.listeners
            .iter()
            .any(|listener| listener.local_addr().is_ok_and(|addr| addr.is_ipv6()))
    }

    /// `http://localhost:<port>` without a trailing slash: Azure matches the registered
    /// `http://localhost` against any port on loopback.
    pub fn redirect_uri(&self) -> String {
        format!("http://localhost:{}", self.port)
    }

    /// Waits for the redirect and returns the authorization code. The browser gets the branded
    /// result page; a wrong `state`, an `error` parameter or a missing code end the wait with
    /// the matching error. Unrelated requests are answered with 404 and ignored.
    pub async fn wait_for_code(
        self,
        expected_state: &str,
        timeout: Duration,
        cancel: &CancellationToken,
    ) -> Result<String> {
        let expected_state: Arc<str> = Arc::from(expected_state);
        let (outcome_tx, mut outcome_rx) = mpsc::channel::<Result<String>>(1);
        let mut connections = JoinSet::new();
        let deadline = tokio::time::sleep(timeout);
        tokio::pin!(deadline);

        loop {
            tokio::select! {
                biased;
                () = cancel.cancelled() => return Err(AuthError::Cancelled),
                Some(outcome) = outcome_rx.recv() => return outcome,
                () = &mut deadline => {
                    return Err(AuthError::BrowserTimeout { at: Timestamp::now() });
                }
                accepted = accept_any(&self.listeners) => match accepted {
                    Ok(stream) => {
                        connections.spawn(serve_connection(
                            stream,
                            Arc::clone(&expected_state),
                            outcome_tx.clone(),
                        ));
                    }
                    Err(error) => {
                        // Transient (e.g. too many open files): back off briefly.
                        tracing::debug!(%error, "loopback accept failed");
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                },
                Some(_) = connections.join_next(), if !connections.is_empty() => {}
            }
        }
    }
}

async fn accept_any(listeners: &[TcpListener]) -> io::Result<TcpStream> {
    match listeners {
        [] => std::future::pending().await,
        [only] => only.accept().await.map(|(stream, _)| stream),
        [first, second, ..] => tokio::select! {
            accepted = first.accept() => accepted.map(|(stream, _)| stream),
            accepted = second.accept() => accepted.map(|(stream, _)| stream),
        },
    }
}

async fn serve_connection(
    mut stream: TcpStream,
    expected_state: Arc<str>,
    outcome_tx: mpsc::Sender<Result<String>>,
) {
    let head = match tokio::time::timeout(CONNECTION_DEADLINE, read_request_head(&mut stream)).await
    {
        Ok(Ok(head)) if !head.is_empty() => head,
        // Idle pre-connect, reset or garbage: nothing to answer.
        _ => return,
    };
    let Some(target) = request_target(&head) else {
        let _ = respond(
            &mut stream,
            "400 Bad Request",
            "text/plain; charset=utf-8",
            b"",
        )
        .await;
        return;
    };
    let Some(params) = callback_params(&target) else {
        let _ = respond(
            &mut stream,
            "404 Not Found",
            "text/plain; charset=utf-8",
            b"",
        )
        .await;
        return;
    };

    let outcome = evaluate_callback(&params, &expected_state);
    let page = match &outcome {
        Ok(_) => result_page(true, None),
        Err(error) => result_page(false, Some(page_reason(error))),
    };
    let _ = respond(
        &mut stream,
        "200 OK",
        "text/html; charset=utf-8",
        page.as_bytes(),
    )
    .await;
    // The page is out before the flow moves on (the wait loop drops this task once it returns).
    let _ = outcome_tx.send(outcome).await;
}

/// Reads until the end of the request head (or 16 KiB / EOF).
async fn read_request_head(stream: &mut TcpStream) -> io::Result<String> {
    let mut buffer = Vec::with_capacity(2048);
    let mut chunk = [0u8; 2048];
    loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        let complete =
            buffer.windows(4).any(|w| w == b"\r\n\r\n") || buffer.windows(2).any(|w| w == b"\n\n");
        if complete || buffer.len() >= MAX_REQUEST_HEAD {
            break;
        }
    }
    Ok(String::from_utf8_lossy(&buffer).into_owned())
}

/// The request target of a `GET` request line (`GET /?code=… HTTP/1.1`).
fn request_target(head: &str) -> Option<String> {
    let line = head.lines().next()?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?;
    let target = parts.next()?;
    method
        .eq_ignore_ascii_case("GET")
        .then(|| target.to_owned())
}

/// Query parameters of a redirect to `/` that carries `code` or `error`; `None` for anything
/// else (favicon, probes).
fn callback_params(target: &str) -> Option<HashMap<String, String>> {
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    if path != "/" && !path.is_empty() {
        return None;
    }
    let params: HashMap<String, String> = url::form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect();
    (params.contains_key("code") || params.contains_key("error")).then_some(params)
}

fn evaluate_callback(params: &HashMap<String, String>, expected_state: &str) -> Result<String> {
    let at = Timestamp::now();
    if params.get("state").map(String::as_str) != Some(expected_state) {
        return Err(AuthError::StateMismatch { at });
    }
    if let Some(error) = params.get("error") {
        return Err(AuthError::Authorize {
            error: error.clone(),
            description: params
                .get("error_description")
                .map(|description| sanitize(description)),
            at,
        });
    }
    match params.get("code") {
        Some(code) if !code.is_empty() => Ok(code.clone()),
        _ => Err(AuthError::MissingCode { at }),
    }
}

/// Short Turkish reason for the browser page; the full message appears in the launcher.
fn page_reason(error: &AuthError) -> &'static str {
    match error {
        AuthError::Authorize { error, .. } if error.eq_ignore_ascii_case("access_denied") => {
            "Giriş izni verilmedi."
        }
        AuthError::Authorize { .. } => "Microsoft giriş isteğini tamamlamadı.",
        AuthError::StateMismatch { .. } => {
            "Giriş yanıtı doğrulanamadı, güvenliğin için giriş durduruldu."
        }
        _ => "Microsoft'tan giriş onayı alınamadı.",
    }
}

async fn respond(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
) -> io::Result<()> {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nPragma: no-cache\r\nX-Content-Type-Options: nosniff\r\n\
         Referrer-Policy: no-referrer\r\nContent-Security-Policy: {PAGE_CSP}\r\n\
         Connection: close\r\n\r\n",
        body.len()
    );
    let write = async {
        stream.write_all(head.as_bytes()).await?;
        stream.write_all(body).await?;
        stream.flush().await?;
        stream.shutdown().await
    };
    tokio::time::timeout(CONNECTION_DEADLINE, write)
        .await
        .unwrap_or_else(|_| Err(io::ErrorKind::TimedOut.into()))
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use super::*;

    async fn browser_get(addr: SocketAddr, target: &str) -> String {
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let request =
            format!("GET {target} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();
        String::from_utf8(response).unwrap()
    }

    fn v4(port: u16) -> SocketAddr {
        SocketAddr::from((Ipv4Addr::LOCALHOST, port))
    }

    async fn start(state: &str) -> (u16, bool, tokio::task::JoinHandle<Result<String>>) {
        let listener = LoopbackListener::bind().await.unwrap();
        let port = listener.port();
        let has_ipv6 = listener.has_ipv6();
        assert_eq!(listener.redirect_uri(), format!("http://localhost:{port}"));
        let state = state.to_owned();
        let cancel = CancellationToken::new();
        let task = tokio::spawn(async move {
            listener
                .wait_for_code(&state, Duration::from_secs(10), &cancel)
                .await
        });
        (port, has_ipv6, task)
    }

    #[tokio::test]
    async fn returns_the_code_and_serves_the_success_page() {
        let (port, _, task) = start("st4te").await;
        let response = browser_get(v4(port), "/?code=M.C123-abc&state=st4te").await;
        assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
        assert!(response.contains("Content-Type: text/html; charset=utf-8"));
        assert!(response.contains("Hesabın onaylandı"));
        assert!(response.contains("Cache-Control: no-store"));
        assert_eq!(task.await.unwrap().unwrap(), "M.C123-abc");
    }

    #[tokio::test]
    async fn listens_on_ipv6_loopback_too() {
        let (port, has_ipv6, task) = start("s").await;
        let addr = if has_ipv6 {
            SocketAddr::from((Ipv6Addr::LOCALHOST, port))
        } else {
            v4(port) // IPv6 disabled on this machine: still works over IPv4.
        };
        let response = browser_get(addr, "/?state=s&code=xyz").await;
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert_eq!(task.await.unwrap().unwrap(), "xyz");
    }

    #[tokio::test]
    async fn rejects_a_state_mismatch() {
        let (port, _, task) = start("expected").await;
        let response = browser_get(v4(port), "/?code=abc&state=forged").await;
        assert!(response.contains("Giriş tamamlanamadı"));
        assert!(response.contains("doğrulanamadı"));
        assert!(matches!(
            task.await.unwrap(),
            Err(AuthError::StateMismatch { .. })
        ));
    }

    #[tokio::test]
    async fn rejects_a_missing_state() {
        let (port, _, task) = start("expected").await;
        browser_get(v4(port), "/?code=abc").await;
        assert!(matches!(
            task.await.unwrap(),
            Err(AuthError::StateMismatch { .. })
        ));
    }

    #[tokio::test]
    async fn reports_authorization_errors() {
        let (port, _, task) = start("s").await;
        let response = browser_get(
            v4(port),
            "/?error=access_denied&error_description=The+user+has+denied+access&state=s",
        )
        .await;
        assert!(response.contains("Giriş izni verilmedi."));
        match task.await.unwrap() {
            Err(AuthError::Authorize {
                error, description, ..
            }) => {
                assert_eq!(error, "access_denied");
                assert_eq!(description.as_deref(), Some("The user has denied access"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[tokio::test]
    async fn ignores_preconnects_and_unrelated_requests() {
        let (port, _, task) = start("s").await;
        // A speculative pre-connect that never sends anything must not block the redirect.
        let _idle = TcpStream::connect(v4(port)).await.unwrap();
        let favicon = browser_get(v4(port), "/favicon.ico").await;
        assert!(favicon.starts_with("HTTP/1.1 404"));
        let other_path = browser_get(v4(port), "/elsewhere?code=1&state=s").await;
        assert!(other_path.starts_with("HTTP/1.1 404"));
        let no_params = browser_get(v4(port), "/").await;
        assert!(no_params.starts_with("HTTP/1.1 404"));
        assert!(!task.is_finished());
        browser_get(v4(port), "/?code=real&state=s").await;
        assert_eq!(task.await.unwrap().unwrap(), "real");
    }

    #[tokio::test]
    async fn times_out_and_cancels() {
        let listener = LoopbackListener::bind().await.unwrap();
        let cancel = CancellationToken::new();
        let result = listener
            .wait_for_code("s", Duration::from_millis(50), &cancel)
            .await;
        assert!(matches!(result, Err(AuthError::BrowserTimeout { .. })));

        let listener = LoopbackListener::bind().await.unwrap();
        let port = listener.port();
        let cancel = CancellationToken::new();
        cancel.cancel();
        let result = listener
            .wait_for_code("s", Duration::from_secs(60), &cancel)
            .await;
        assert!(matches!(result, Err(AuthError::Cancelled)));
        // The listener is gone once the wait ends: the port no longer accepts connections.
        assert!(TcpStream::connect(v4(port)).await.is_err());
    }

    #[test]
    fn callback_parsing() {
        assert!(callback_params("/favicon.ico").is_none());
        assert!(callback_params("/?foo=bar").is_none());
        let params = callback_params("/?code=a%2Bb&state=x").unwrap();
        assert_eq!(params["code"], "a+b");
        assert_eq!(
            request_target("GET /?code=1 HTTP/1.1\r\n").as_deref(),
            Some("/?code=1")
        );
        assert_eq!(request_target("POST / HTTP/1.1\r\n"), None);
        assert_eq!(request_target(""), None);
    }
}
