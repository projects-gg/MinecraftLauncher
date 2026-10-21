//! The loopback listener for one game session.

use std::fmt;
use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

use crate::protocol::{ByeReason, FromGame, MAX_GAME_FRAME, MAX_LAUNCHER_FRAME, PROTO, ToGame};
use crate::token::SessionToken;

/// JVM argument prefixes the mod reads (`-Dprojects.overlay.port=…`, `-Dprojects.overlay.token=…`).
pub const PORT_ARG: &str = "-Dprojects.overlay.port=";
pub const TOKEN_ARG: &str = "-Dprojects.overlay.token=";

/// A connection must say hello this quickly or it is closed.
const HELLO_TIMEOUT: Duration = Duration::from_secs(3);

/// Whether a launch argument carries the session token and must be masked in logs.
pub fn is_secret_jvm_arg(arg: &str) -> bool {
    arg.starts_with(TOKEN_ARG)
}

/// Where the game finds this session's bridge.
pub struct Endpoint {
    port: u16,
    token: SessionToken,
}

impl Endpoint {
    pub fn port(&self) -> u16 {
        self.port
    }

    /// The two JVM arguments for the game's command line.
    pub fn jvm_args(&self) -> [String; 2] {
        [
            format!("{PORT_ARG}{}", self.port),
            format!("{TOKEN_ARG}{}", self.token.expose()),
        ]
    }
}

impl fmt::Debug for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Endpoint")
            .field("port", &self.port)
            .field("token", &"<redacted>")
            .finish()
    }
}

/// What the launcher learns from the bridge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeEvent {
    /// The mod proved the token and was greeted.
    Connected {
        mod_version: String,
        minecraft: String,
        language: String,
    },
    /// A player action from the panel.
    Frame(FromGame),
    /// The mod went away (game closed, or a newer connection replaced it).
    Disconnected,
}

enum Incoming {
    Ready {
        id: u64,
        writer: OwnedWriteHalf,
        mod_version: String,
        minecraft: String,
        language: String,
    },
    Frame(u64, FromGame),
    Closed(u64),
}

/// The listener for one game launch. Bound to `127.0.0.1` only; one live session at a time.
pub struct Bridge {
    listener: TcpListener,
    token: SessionToken,
}

impl Bridge {
    /// Binds a fresh port and makes a fresh token for one game launch.
    pub async fn bind() -> io::Result<(Self, Endpoint)> {
        let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).await?;
        let port = listener.local_addr()?.port();
        let token = SessionToken::generate();
        Ok((
            Self {
                listener,
                token: token.clone(),
            },
            Endpoint { port, token },
        ))
    }

    /// Serves until `outgoing` is closed (the game ended or the launcher quits), then says goodbye.
    ///
    /// Each new authenticated connection gets `greet()` (welcome and snapshot) and replaces the previous
    /// one, so the mod can reconnect at any time. Sending [`ToGame::Bye`] ends the current session.
    pub async fn serve<G>(
        self,
        mut outgoing: mpsc::Receiver<ToGame>,
        events: mpsc::Sender<BridgeEvent>,
        greet: G,
    ) where
        G: Fn() -> Vec<ToGame>,
    {
        let (incoming_tx, mut incoming_rx) = mpsc::channel(64);
        let mut next_id = 0u64;
        let mut current: Option<(u64, OwnedWriteHalf)> = None;
        loop {
            tokio::select! {
                accepted = self.listener.accept() => match accepted {
                    Ok((stream, _)) => {
                        next_id += 1;
                        tokio::spawn(connection(stream, self.token.clone(), next_id, incoming_tx.clone()));
                    }
                    Err(err) => tracing::debug!(%err, "overlay bridge accept failed"),
                },
                Some(incoming) = incoming_rx.recv() => match incoming {
                    Incoming::Ready { id, mut writer, mod_version, minecraft, language } => {
                        if let Some((_, mut old)) = current.take() {
                            let _ = write_frame(&mut old, &ToGame::Bye { reason: ByeReason::Replaced }).await;
                            let _ = old.shutdown().await;
                            let _ = events.send(BridgeEvent::Disconnected).await;
                        }
                        let mut greeted = true;
                        for frame in greet() {
                            if write_frame(&mut writer, &frame).await.is_err() {
                                greeted = false;
                                break;
                            }
                        }
                        if greeted {
                            current = Some((id, writer));
                            let connected = BridgeEvent::Connected { mod_version, minecraft, language };
                            let _ = events.send(connected).await;
                        }
                    }
                    Incoming::Frame(id, frame) => {
                        if current.as_ref().is_some_and(|(c, _)| *c == id) {
                            let _ = events.send(BridgeEvent::Frame(frame)).await;
                        }
                    }
                    Incoming::Closed(id) => {
                        if current.as_ref().is_some_and(|(c, _)| *c == id) {
                            current = None;
                            let _ = events.send(BridgeEvent::Disconnected).await;
                        }
                    }
                },
                frame = outgoing.recv() => match frame {
                    Some(frame) => {
                        let ends = matches!(frame, ToGame::Bye { .. });
                        let failed = match current.as_mut() {
                            Some((_, writer)) => write_frame(writer, &frame).await.is_err(),
                            None => false,
                        };
                        if (failed || ends)
                            && let Some((_, mut writer)) = current.take()
                        {
                            let _ = writer.shutdown().await;
                            let _ = events.send(BridgeEvent::Disconnected).await;
                        }
                    }
                    None => {
                        if let Some((_, mut writer)) = current.take() {
                            let bye = ToGame::Bye { reason: ByeReason::LauncherExit };
                            let _ = write_frame(&mut writer, &bye).await;
                            let _ = writer.shutdown().await;
                        }
                        return;
                    }
                },
            }
        }
    }
}

/// One accepted connection: handshake, then forward the mod's frames until it closes.
async fn connection(
    stream: TcpStream,
    token: SessionToken,
    id: u64,
    incoming: mpsc::Sender<Incoming>,
) {
    let _ = stream.set_nodelay(true);
    let (read, mut writer) = stream.into_split();
    let mut reader = BufReader::new(read);
    let mut buf = Vec::new();
    let first = tokio::time::timeout(
        HELLO_TIMEOUT,
        read_frame(&mut reader, &mut buf, MAX_GAME_FRAME),
    )
    .await;
    let hello = match first {
        Ok(Ok(Some(line))) => serde_json::from_str::<FromGame>(&line).ok(),
        _ => None,
    };
    // Anything but our hello (a browser's HTTP request, a port scan) gets no answer at all.
    let Some(FromGame::Hello {
        proto,
        token: offered,
        mod_version,
        mc,
        lang,
    }) = hello
    else {
        return;
    };
    if !token.matches(offered.expose()) {
        let _ = write_frame(
            &mut writer,
            &ToGame::Bye {
                reason: ByeReason::Auth,
            },
        )
        .await;
        return;
    }
    if proto != PROTO {
        let _ = write_frame(
            &mut writer,
            &ToGame::Bye {
                reason: ByeReason::Proto,
            },
        )
        .await;
        return;
    }
    let ready = Incoming::Ready {
        id,
        writer,
        mod_version,
        minecraft: mc,
        language: lang,
    };
    if incoming.send(ready).await.is_err() {
        return;
    }
    loop {
        match read_frame(&mut reader, &mut buf, MAX_GAME_FRAME).await {
            Ok(Some(line)) => match serde_json::from_str::<FromGame>(&line) {
                Ok(FromGame::Hello { .. } | FromGame::Unknown) | Err(_) => {}
                Ok(frame) => {
                    if incoming.send(Incoming::Frame(id, frame)).await.is_err() {
                        return;
                    }
                }
            },
            Ok(None) | Err(_) => {
                let _ = incoming.send(Incoming::Closed(id)).await;
                return;
            }
        }
    }
}

/// Reads one newline-terminated frame of at most `max` bytes. `Ok(None)` at the end of the stream (a
/// partial last frame is dropped); an error for longer frames or invalid UTF-8.
async fn read_frame<R>(reader: &mut R, buf: &mut Vec<u8>, max: usize) -> io::Result<Option<String>>
where
    R: AsyncBufRead + Unpin,
{
    buf.clear();
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            return Ok(None);
        }
        let (chunk, done) = match available.iter().position(|&b| b == b'\n') {
            Some(newline) => (newline, true),
            None => (available.len(), false),
        };
        if buf.len() + chunk > max {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "overlay frame too long",
            ));
        }
        buf.extend_from_slice(&available[..chunk]);
        reader.consume(if done { chunk + 1 } else { chunk });
        if done {
            if buf.last() == Some(&b'\r') {
                buf.pop();
            }
            return String::from_utf8(std::mem::take(buf))
                .map(Some)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e));
        }
    }
}

async fn write_frame(writer: &mut OwnedWriteHalf, frame: &ToGame) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(frame).map_err(io::Error::other)?;
    if bytes.len() > MAX_LAUNCHER_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "overlay frame too long",
        ));
    }
    bytes.push(b'\n');
    writer.write_all(&bytes).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{CallAnswer, Me, ToastKind, ToastView};
    use tokio::io::AsyncReadExt;
    use tokio::net::tcp::OwnedReadHalf;
    use tokio::time::timeout;

    const WAIT: Duration = Duration::from_secs(5);

    struct Harness {
        endpoint: Endpoint,
        outgoing: mpsc::Sender<ToGame>,
        events: mpsc::Receiver<BridgeEvent>,
    }

    async fn start() -> Harness {
        let (bridge, endpoint) = Bridge::bind().await.unwrap();
        let (outgoing, outgoing_rx) = mpsc::channel(16);
        let (events_tx, events) = mpsc::channel(16);
        tokio::spawn(bridge.serve(outgoing_rx, events_tx, || {
            vec![ToGame::Welcome {
                proto: PROTO,
                launcher: "test".into(),
                me: Me { name: "Ece".into() },
                dnd: false,
                reduced_motion: false,
                features: vec!["chat".into()],
            }]
        }));
        Harness {
            endpoint,
            outgoing,
            events,
        }
    }

    struct Client {
        reader: BufReader<OwnedReadHalf>,
        writer: OwnedWriteHalf,
    }

    impl Client {
        async fn connect(port: u16) -> Self {
            let stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port))
                .await
                .unwrap();
            let (read, writer) = stream.into_split();
            Self {
                reader: BufReader::new(read),
                writer,
            }
        }

        async fn send(&mut self, line: &str) {
            self.writer
                .write_all(format!("{line}\n").as_bytes())
                .await
                .unwrap();
        }

        async fn hello(&mut self, token: &str) {
            self.send(&format!(
                r#"{{"t":"hello","proto":1,"token":"{token}","mod":"0.1.0","mc":"26.1.2","lang":"tr_tr"}}"#
            ))
            .await;
        }

        /// The next frame as JSON, or None when the connection closed.
        async fn next(&mut self) -> Option<serde_json::Value> {
            let mut line = String::new();
            let n = timeout(WAIT, self.reader.read_line(&mut line))
                .await
                .unwrap()
                .unwrap();
            (n > 0).then(|| serde_json::from_str(&line).unwrap())
        }
    }

    fn token_of(endpoint: &Endpoint) -> String {
        endpoint.jvm_args()[1][TOKEN_ARG.len()..].to_owned()
    }

    async fn next_event(h: &mut Harness) -> BridgeEvent {
        timeout(WAIT, h.events.recv()).await.unwrap().unwrap()
    }

    #[tokio::test]
    async fn greets_then_carries_frames_both_ways() {
        let mut h = start().await;
        let mut c = Client::connect(h.endpoint.port()).await;
        c.hello(&token_of(&h.endpoint)).await;
        assert_eq!(c.next().await.unwrap()["t"], "welcome");
        assert!(
            matches!(next_event(&mut h).await, BridgeEvent::Connected { ref language, .. } if language == "tr_tr")
        );

        c.send(r#"{"t":"answer","call":"c1","answer":"coming"}"#)
            .await;
        c.send(r#"{"t":"from-the-future","x":1}"#).await;
        c.send(r#"{"t":"panel","open":true}"#).await;
        assert!(matches!(
            next_event(&mut h).await,
            BridgeEvent::Frame(FromGame::Answer {
                answer: CallAnswer::Coming,
                ..
            })
        ));
        assert_eq!(
            next_event(&mut h).await,
            BridgeEvent::Frame(FromGame::Panel { open: true })
        );

        let toast = ToGame::Toast(ToastView {
            kind: ToastKind::Message,
            key: "dm:1".into(),
            title: "Ayşe".into(),
            body: "Akşam bedwars?".into(),
            conv: Some("dm:1".into()),
        });
        h.outgoing.send(toast).await.unwrap();
        let got = c.next().await.unwrap();
        assert_eq!(got["t"], "toast");
        assert_eq!(got["title"], "Ayşe");
    }

    #[tokio::test]
    async fn wrong_token_is_turned_away() {
        let mut h = start().await;
        let mut c = Client::connect(h.endpoint.port()).await;
        c.hello(&"x".repeat(43)).await;
        assert_eq!(
            c.next().await.unwrap(),
            serde_json::json!({"t":"bye","reason":"auth"})
        );
        assert!(c.next().await.is_none());
        assert!(
            timeout(Duration::from_millis(300), h.events.recv())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn http_requests_get_no_answer() {
        let h = start().await;
        let mut c = Client::connect(h.endpoint.port()).await;
        c.send("GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nOrigin: https://example.com\r\n")
            .await;
        let mut rest = Vec::new();
        match timeout(WAIT, c.reader.read_to_end(&mut rest))
            .await
            .unwrap()
        {
            Ok(_) => assert!(rest.is_empty()),
            // Closing with unread bytes may reset instead of a clean close; either way nothing came back.
            Err(err) => assert_eq!(err.kind(), io::ErrorKind::ConnectionReset),
        }
    }

    #[tokio::test]
    async fn oversized_frames_end_the_session() {
        let mut h = start().await;
        let mut c = Client::connect(h.endpoint.port()).await;
        c.hello(&token_of(&h.endpoint)).await;
        assert!(matches!(
            next_event(&mut h).await,
            BridgeEvent::Connected { .. }
        ));
        let huge = format!(
            r#"{{"t":"send","cid":"c","conv":"x","text":"{}"}}"#,
            "a".repeat(MAX_GAME_FRAME)
        );
        c.send(&huge).await;
        assert_eq!(next_event(&mut h).await, BridgeEvent::Disconnected);
    }

    #[tokio::test]
    async fn a_new_connection_replaces_the_old_one() {
        let mut h = start().await;
        let token = token_of(&h.endpoint);
        let mut first = Client::connect(h.endpoint.port()).await;
        first.hello(&token).await;
        assert_eq!(first.next().await.unwrap()["t"], "welcome");
        assert!(matches!(
            next_event(&mut h).await,
            BridgeEvent::Connected { .. }
        ));

        let mut second = Client::connect(h.endpoint.port()).await;
        second.hello(&token).await;
        assert_eq!(second.next().await.unwrap()["t"], "welcome");
        assert_eq!(first.next().await.unwrap()["reason"], "replaced");
        assert!(first.next().await.is_none());
        assert_eq!(next_event(&mut h).await, BridgeEvent::Disconnected);
        assert!(matches!(
            next_event(&mut h).await,
            BridgeEvent::Connected { .. }
        ));
    }

    #[tokio::test]
    async fn closing_the_bridge_says_goodbye() {
        let mut h = start().await;
        let mut c = Client::connect(h.endpoint.port()).await;
        c.hello(&token_of(&h.endpoint)).await;
        assert_eq!(c.next().await.unwrap()["t"], "welcome");
        assert!(matches!(
            next_event(&mut h).await,
            BridgeEvent::Connected { .. }
        ));
        drop(h.outgoing);
        assert_eq!(
            c.next().await.unwrap(),
            serde_json::json!({"t":"bye","reason":"launcher-exit"})
        );
        assert!(c.next().await.is_none());
    }

    #[tokio::test]
    async fn jvm_args_carry_port_and_token_and_are_redactable() {
        let h = start().await;
        let [port, token] = h.endpoint.jvm_args();
        assert_eq!(port, format!("{PORT_ARG}{}", h.endpoint.port()));
        assert!(token.starts_with(TOKEN_ARG));
        assert_eq!(token.len(), TOKEN_ARG.len() + 43);
        assert!(is_secret_jvm_arg(&token));
        assert!(!is_secret_jvm_arg(&port));
        assert!(!format!("{:?}", h.endpoint).contains(&token[TOKEN_ARG.len()..]));
    }
}
