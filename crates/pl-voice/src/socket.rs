//! The room socket (`GET /v1/voice?ticket=…` with the presence session's bearer token): one task
//! per connection attempt. It reports [`SocketEvent`]s and sends the frames it is handed; when its
//! sender is dropped it closes the socket (1000) and says nothing more. Reconnecting is the
//! machine's ([`crate::machine`]): each attempt is a new task with a new generation.
//!
//! Keep-alive: WebSocket protocol pings every [`KEEPALIVE`] while connected (Cloudflare closes idle
//! sockets after about 100 s; a hibernating room answers them without waking); a ping unanswered
//! for [`PONG_TIMEOUT`] counts as a drop. Nothing runs without a call.

use std::sync::Arc;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio::time::{Instant, sleep_until, timeout};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::http::header::{AUTHORIZATION, USER_AGENT};
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::{CloseFrame, WebSocketConfig};
use tokio_tungstenite::tungstenite::{self, Message};
use tokio_tungstenite::{Connector, connect_async_tls_with_config};
use url::Url;

use crate::wire::{FromRoom, Ticket, ToRoom, limits};

pub const KEEPALIVE: Duration = Duration::from_secs(45);
pub const PONG_TIMEOUT: Duration = Duration::from_secs(20);
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// Where room sockets go, and as whom.
#[derive(Clone)]
pub struct SocketTarget {
    /// `wss://social.projects.gg/v1/voice` (a local service: `ws://127.0.0.1:8797/v1/voice`).
    pub url: Url,
    /// The presence session's token.
    pub bearer: String,
    pub user_agent: String,
    /// TLS for `wss` (the friends service's).
    pub tls: Option<Arc<rustls::ClientConfig>>,
}

impl std::fmt::Debug for SocketTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SocketTarget")
            .field("url", &self.url.as_str())
            .finish_non_exhaustive()
    }
}

impl SocketTarget {
    /// The room socket's address from the friends service's base (`https://…` → `wss://…`).
    pub fn url_from_base(base: &Url) -> Option<Url> {
        let mut url = base.join("v1/voice").ok()?;
        let scheme = match url.scheme() {
            "https" | "wss" => "wss",
            "http" | "ws" => "ws",
            _ => return None,
        };
        url.set_scheme(scheme).ok()?;
        url.set_query(None);
        Some(url)
    }
}

/// What a socket task reports.
#[derive(Debug, Clone, PartialEq)]
pub enum SocketEvent {
    Open,
    Message(Box<FromRoom>),
    /// It closed (`code`: the close frame's) or dropped (none).
    Closed(Option<u16>),
    /// It never opened: the service's refusal (`status`, its `error.code`), or the network
    /// (none).
    Refused {
        status: Option<u16>,
        code: Option<String>,
    },
}

pub type EventSink = Arc<dyn Fn(SocketEvent) + Send + Sync>;

/// Starts a socket task on `runtime`; drop the returned sender to close it.
pub fn spawn(
    runtime: &tokio::runtime::Handle,
    target: SocketTarget,
    ticket: Ticket,
    events: EventSink,
) -> mpsc::UnboundedSender<ToRoom> {
    let (frames, outgoing) = mpsc::unbounded_channel();
    runtime.spawn(run(target, ticket, outgoing, events));
    frames
}

async fn run(
    target: SocketTarget,
    ticket: Ticket,
    mut outgoing: mpsc::UnboundedReceiver<ToRoom>,
    events: EventSink,
) {
    let mut url = target.url.clone();
    url.query_pairs_mut().append_pair("ticket", ticket.expose());
    let Ok(mut request) = url.as_str().into_client_request() else {
        events(SocketEvent::Refused {
            status: None,
            code: Some("bad_request".into()),
        });
        return;
    };
    if let Ok(value) = HeaderValue::from_str(&format!("Bearer {}", target.bearer)) {
        request.headers_mut().insert(AUTHORIZATION, value);
    }
    if let Ok(value) = HeaderValue::from_str(&target.user_agent) {
        request.headers_mut().insert(USER_AGENT, value);
    }
    let connector = target.tls.clone().map(Connector::Rustls);
    let config = WebSocketConfig::default()
        .max_message_size(Some(64 * 1024))
        .max_frame_size(Some(64 * 1024));
    let connected = timeout(
        CONNECT_TIMEOUT,
        connect_async_tls_with_config(request, Some(config), false, connector),
    )
    .await;
    let socket = match connected {
        Ok(Ok((socket, _))) => socket,
        Ok(Err(tungstenite::Error::Http(response))) => {
            let status = response.status().as_u16();
            let code = response
                .body()
                .as_deref()
                .and_then(|body| serde_json::from_slice::<serde_json::Value>(body).ok())
                .and_then(|body| {
                    body.pointer("/error/code")
                        .or_else(|| body.get("code"))
                        .or_else(|| body.get("error"))
                        .and_then(|code| code.as_str().map(str::to_owned))
                });
            events(SocketEvent::Refused {
                status: Some(status),
                code,
            });
            return;
        }
        Ok(Err(error)) => {
            tracing::debug!(%error, "voice: the room socket did not open");
            events(SocketEvent::Refused {
                status: None,
                code: None,
            });
            return;
        }
        Err(_) => {
            events(SocketEvent::Refused {
                status: None,
                code: None,
            });
            return;
        }
    };
    events(SocketEvent::Open);
    let (mut sink, mut stream) = socket.split();
    let mut next_ping = Instant::now() + KEEPALIVE;
    let mut pong_due: Option<Instant> = None;
    loop {
        let wake = pong_due.map_or(next_ping, |due| due.min(next_ping));
        tokio::select! {
            frame = outgoing.recv() => {
                let Some(frame) = frame else {
                    // The machine is done with this socket.
                    let _ = sink
                        .send(Message::Close(Some(CloseFrame {
                            code: CloseCode::Normal,
                            reason: "".into(),
                        })))
                        .await;
                    return;
                };
                let text = frame.text();
                if text.len() > limits::MAX_FRAME_CHARS {
                    tracing::warn!(kind = frame.kind(), "voice: a frame too big for the room");
                    continue;
                }
                if sink.send(Message::text(text)).await.is_err() {
                    events(SocketEvent::Closed(None));
                    return;
                }
            }
            message = stream.next() => {
                match message {
                    Some(Ok(Message::Text(text))) => {
                        let message = FromRoom::parse(text.as_str());
                        if message != FromRoom::Unknown {
                            events(SocketEvent::Message(Box::new(message)));
                        }
                    }
                    Some(Ok(Message::Pong(_))) => pong_due = None,
                    Some(Ok(Message::Close(frame))) => {
                        events(SocketEvent::Closed(frame.map(|frame| u16::from(frame.code))));
                        return;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(_)) | None => {
                        events(SocketEvent::Closed(None));
                        return;
                    }
                }
            }
            () = sleep_until(wake) => {
                let now = Instant::now();
                if pong_due.is_some_and(|due| now >= due) {
                    tracing::debug!("voice: the room socket stopped answering");
                    events(SocketEvent::Closed(None));
                    return;
                }
                if now >= next_ping {
                    next_ping = now + KEEPALIVE;
                    if sink.send(Message::Ping(Vec::new().into())).await.is_err() {
                        events(SocketEvent::Closed(None));
                        return;
                    }
                    pong_due.get_or_insert(now + PONG_TIMEOUT);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_room_socket_lives_beside_the_service() {
        let production = Url::parse("https://social.projects.gg/").unwrap();
        assert_eq!(
            SocketTarget::url_from_base(&production).unwrap().as_str(),
            "wss://social.projects.gg/v1/voice"
        );
        let local = Url::parse("http://127.0.0.1:8797/").unwrap();
        assert_eq!(
            SocketTarget::url_from_base(&local).unwrap().as_str(),
            "ws://127.0.0.1:8797/v1/voice"
        );
        assert!(SocketTarget::url_from_base(&Url::parse("ftp://x/").unwrap()).is_none());
    }

    #[test]
    fn the_token_stays_out_of_debug_output() {
        let target = SocketTarget {
            url: Url::parse("wss://social.projects.gg/v1/voice").unwrap(),
            bearer: "secret-token".into(),
            user_agent: "ProjectsLauncher/2.0.0".into(),
            tls: None,
        };
        assert!(!format!("{target:?}").contains("secret"));
    }
}
