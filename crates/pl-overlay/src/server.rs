//! The loopback listener for one game session.
//!
//! [`Bridge::serve`] owns the listener and the live connection. A connection must say `hello` with
//! this launch's token within [`HELLO_TIMEOUT`]: anything else gets no answer at all (a browser's
//! HTTP request, a port scan), a wrong token gets `bye{auth}` and another protocol version
//! `bye{proto}`. A newer authenticated connection replaces the live one (`bye{replaced}`), so the
//! mod can reconnect at any time. Failed handshakes are budgeted per window ([`Doorman`]) and only
//! a few may be under way at once, so a flood neither fills the log nor keeps tasks busy.
//!
//! Nothing runs between frames: the listener and the readers wait on their sockets, the only
//! timers are the handshake's and a write's while one is under way.

use std::fmt;
use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinSet;

use crate::protocol::{ByeReason, FromGame, MAX_GAME_FRAME, MAX_LAUNCHER_FRAME, PROTO, ToGame};
use crate::token::SessionToken;

/// JVM argument prefixes the mod reads (`-Dprojects.overlay.port=…`, `-Dprojects.overlay.token=…`).
pub const PORT_ARG: &str = "-Dprojects.overlay.port=";
pub const TOKEN_ARG: &str = "-Dprojects.overlay.token=";

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

    /// The token's value, for the launch command's list of values to mask in logs. Never log it.
    pub fn secret(&self) -> &str {
        self.token.expose()
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

/// One accepted connection, numbered from 1 in the order they arrived.
pub type ConnId = u64;

/// What the launcher tells the bridge.
#[derive(Debug)]
pub enum Control {
    /// Frames for one connection, written in order; dropped when that connection is no longer
    /// the live one (a reconnect brings a fresh snapshot instead).
    Send { conn: ConnId, frames: Vec<ToGame> },
    /// Ends the session: the listener goes first, the live connection hears `bye{reason}`, every
    /// connection closes. `done` fires once that is over.
    Close {
        reason: ByeReason,
        done: Option<oneshot::Sender<()>>,
    },
}

/// What the launcher learns from the bridge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeEvent {
    /// The mod proved the token: it waits for `welcome` (and a `snapshot`).
    Connected {
        conn: ConnId,
        mod_version: String,
        minecraft: String,
        language: String,
        /// What the mod can do beyond protocol 1 (`connect`).
        features: Vec<String>,
    },
    /// A player action from the live connection.
    Frame { conn: ConnId, frame: FromGame },
    /// The live connection went away (the game closed, a write got stuck, a newer one replaced it).
    Disconnected { conn: ConnId },
}

/// A connection must say hello this quickly or it is closed.
const HELLO_TIMEOUT: Duration = Duration::from_secs(3);
/// A frame that cannot be written this long means a mod that stopped reading: the connection goes
/// (the mod reconnects by itself and gets a fresh snapshot).
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);
/// How long a goodbye may take to leave.
pub const BYE_TIMEOUT: Duration = Duration::from_millis(500);
/// Connections still in their handshake at once; more are closed straight away.
const MAX_HANDSHAKES: usize = 4;
/// Failed handshakes are counted per window of this length ...
const FAILURE_WINDOW: Duration = Duration::from_secs(10);
/// ... and past this many the rest of the window turns newcomers away unread.
const FAILURES_PER_WINDOW: u32 = 8;

/// Why a connection did not get through its handshake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Refusal {
    /// Not our hello, or none in time: no answer at all.
    Silent,
    /// A wrong token (`bye{auth}`).
    Token,
    /// Another protocol version (`bye{proto}`).
    Proto,
}

enum Incoming {
    Refused(Refusal),
    Ready {
        id: ConnId,
        writer: OwnedWriteHalf,
        mod_version: String,
        minecraft: String,
        language: String,
        features: Vec<String>,
    },
    Frame(ConnId, FromGame),
    Closed(ConnId),
}

/// Budgets failed handshakes: [`FAILURES_PER_WINDOW`] per [`FAILURE_WINDOW`], then newcomers are
/// turned away until the window ends. The 256-bit token cannot be guessed anyway; this keeps a
/// port scanner or a misbehaving program from filling the log and keeping tasks busy. One line is
/// logged when the budget runs out and one when that window ends.
#[derive(Debug)]
struct Doorman {
    window_start: Instant,
    failures: u32,
    turned_away: u32,
}

impl Doorman {
    fn new(now: Instant) -> Self {
        Self {
            window_start: now,
            failures: 0,
            turned_away: 0,
        }
    }

    fn roll(&mut self, now: Instant) {
        if now.duration_since(self.window_start) < FAILURE_WINDOW {
            return;
        }
        if self.turned_away > 0 {
            tracing::info!(
                failures = self.failures,
                turned_away = self.turned_away,
                "overlay bridge listens to newcomers again"
            );
        }
        *self = Self::new(now);
    }

    /// Whether a new connection may try its handshake now.
    fn admits(&mut self, now: Instant) -> bool {
        self.roll(now);
        if self.failures < FAILURES_PER_WINDOW {
            return true;
        }
        self.turned_away += 1;
        false
    }

    fn failed(&mut self, now: Instant, why: Refusal) {
        self.roll(now);
        self.failures += 1;
        if self.failures == 1 {
            tracing::debug!(?why, "overlay bridge: a connection failed its handshake");
        } else if self.failures == FAILURES_PER_WINDOW {
            tracing::info!(
                failures = self.failures,
                "overlay bridge: too many failed handshakes, newcomers wait for a few seconds"
            );
        }
    }
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

    /// Serves until [`Control::Close`] (or until `control` closes, which says `launcher-exit`).
    pub async fn serve(
        self,
        mut control: mpsc::UnboundedReceiver<Control>,
        events: mpsc::UnboundedSender<BridgeEvent>,
    ) {
        let Self { listener, token } = self;
        let (incoming_tx, mut incoming_rx) = mpsc::channel(64);
        let mut tasks = JoinSet::new();
        let mut next_id: ConnId = 0;
        let mut handshakes = 0usize;
        let mut doorman = Doorman::new(Instant::now());
        let mut live: Option<(ConnId, OwnedWriteHalf)> = None;
        let (reason, done) = loop {
            // The launcher first (a close never waits behind a busy connection), then what the
            // connections report (a failed handshake counts before the next newcomer), then
            // newcomers.
            tokio::select! {
                biased;
                command = control.recv() => match command {
                    Some(Control::Send { conn, frames }) => {
                        let Some((id, writer)) = live.as_mut().filter(|(id, _)| *id == conn) else {
                            continue;
                        };
                        let id = *id;
                        let mut written = true;
                        for frame in &frames {
                            match tokio::time::timeout(WRITE_TIMEOUT, write_frame(writer, frame)).await {
                                Ok(Ok(())) => {}
                                Ok(Err(error)) => {
                                    tracing::debug!(%error, "overlay frame not written; the connection goes");
                                    written = false;
                                    break;
                                }
                                Err(_) => {
                                    tracing::info!("the overlay mod stopped reading; its connection goes");
                                    written = false;
                                    break;
                                }
                            }
                        }
                        if !written && let Some((_, mut writer)) = live.take() {
                            let _ = tokio::time::timeout(BYE_TIMEOUT, writer.shutdown()).await;
                            let _ = events.send(BridgeEvent::Disconnected { conn: id });
                        }
                    }
                    Some(Control::Close { reason, done }) => break (reason, done),
                    None => break (ByeReason::LauncherExit, None),
                },
                Some(incoming) = incoming_rx.recv() => match incoming {
                    Incoming::Refused(why) => {
                        handshakes = handshakes.saturating_sub(1);
                        doorman.failed(Instant::now(), why);
                    }
                    Incoming::Ready { id, writer, mod_version, minecraft, language, features } => {
                        handshakes = handshakes.saturating_sub(1);
                        if let Some((old, writer)) = live.take() {
                            say_bye(writer, ByeReason::Replaced).await;
                            let _ = events.send(BridgeEvent::Disconnected { conn: old });
                        }
                        live = Some((id, writer));
                        let _ = events.send(BridgeEvent::Connected {
                            conn: id,
                            mod_version,
                            minecraft,
                            language,
                            features,
                        });
                    }
                    Incoming::Frame(id, frame) => {
                        if live.as_ref().is_some_and(|(live, _)| *live == id) {
                            let _ = events.send(BridgeEvent::Frame { conn: id, frame });
                        }
                    }
                    Incoming::Closed(id) => {
                        if live.as_ref().is_some_and(|(live, _)| *live == id) {
                            live = None;
                            let _ = events.send(BridgeEvent::Disconnected { conn: id });
                        }
                    }
                },
                accepted = listener.accept() => match accepted {
                    Ok((stream, _)) => {
                        if handshakes >= MAX_HANDSHAKES || !doorman.admits(Instant::now()) {
                            // Closed at once, unread.
                            drop(stream);
                            continue;
                        }
                        next_id += 1;
                        handshakes += 1;
                        tasks.spawn(connection(stream, token.clone(), next_id, incoming_tx.clone()));
                    }
                    Err(err) => tracing::debug!(%err, "overlay bridge accept failed"),
                },
            }
        };
        // The order matters: no newcomer can arrive between the goodbye and the end, the live
        // session hears why, then every connection task ends.
        drop(listener);
        if let Some((_, writer)) = live.take() {
            say_bye(writer, reason).await;
        }
        tasks.shutdown().await;
        tracing::debug!(?reason, "overlay bridge closed");
        if let Some(done) = done {
            let _ = done.send(());
        }
    }
}

/// `bye{reason}` and the end of the stream, within [`BYE_TIMEOUT`].
async fn say_bye(mut writer: OwnedWriteHalf, reason: ByeReason) {
    let goodbye = async {
        let _ = write_frame(&mut writer, &ToGame::Bye { reason }).await;
        let _ = writer.shutdown().await;
    };
    let _ = tokio::time::timeout(BYE_TIMEOUT, goodbye).await;
}

/// One accepted connection: handshake, then forward the mod's frames until it closes. It reports
/// exactly one handshake outcome (`Refused` or `Ready`) first.
async fn connection(
    stream: TcpStream,
    token: SessionToken,
    id: ConnId,
    incoming: mpsc::Sender<Incoming>,
) {
    let _ = stream.set_nodelay(true);
    let (read, writer) = stream.into_split();
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
        features,
    }) = hello
    else {
        let _ = incoming.send(Incoming::Refused(Refusal::Silent)).await;
        return;
    };
    let refusal = if !token.matches(offered.expose()) {
        Some((Refusal::Token, ByeReason::Auth))
    } else if proto != PROTO {
        Some((Refusal::Proto, ByeReason::Proto))
    } else {
        None
    };
    if let Some((why, reason)) = refusal {
        // Counted before the goodbye leaves: whoever reads it finds the budget already spent.
        let _ = incoming.send(Incoming::Refused(why)).await;
        say_bye(writer, reason).await;
        return;
    }
    let ready = Incoming::Ready {
        id,
        writer,
        mod_version,
        minecraft: mc,
        language: lang,
        features,
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

/// The JSON of one frame, or an error for a frame past [`MAX_LAUNCHER_FRAME`] (the feed keeps its
/// frames well below; one that is not is a bug, never a reason to cut a frame in half).
pub(crate) fn encode(frame: &ToGame) -> io::Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec(frame).map_err(io::Error::other)?;
    if bytes.len() > MAX_LAUNCHER_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "overlay frame too long",
        ));
    }
    bytes.push(b'\n');
    Ok(bytes)
}

async fn write_frame(writer: &mut OwnedWriteHalf, frame: &ToGame) -> io::Result<()> {
    match encode(frame) {
        Ok(bytes) => writer.write_all(&bytes).await,
        Err(error) => {
            // Skipped, not fatal: the session goes on without it.
            tracing::warn!(%error, "overlay frame skipped");
            Ok(())
        }
    }
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
        control: mpsc::UnboundedSender<Control>,
        events: mpsc::UnboundedReceiver<BridgeEvent>,
        served: tokio::task::JoinHandle<()>,
    }

    async fn start() -> Harness {
        let (bridge, endpoint) = Bridge::bind().await.unwrap();
        let (control, control_rx) = mpsc::unbounded_channel();
        let (events_tx, events) = mpsc::unbounded_channel();
        let served = tokio::spawn(bridge.serve(control_rx, events_tx));
        Harness {
            endpoint,
            control,
            events,
            served,
        }
    }

    fn welcome() -> ToGame {
        ToGame::Welcome {
            proto: PROTO,
            launcher: "test".into(),
            me: Me { name: "Ece".into() },
            dnd: false,
            reduced_motion: false,
            features: vec!["chat".into()],
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
                .unwrap_or(0);
            (n > 0).then(|| serde_json::from_str(&line).unwrap())
        }
    }

    fn token_of(endpoint: &Endpoint) -> String {
        endpoint.secret().to_owned()
    }

    async fn next_event(h: &mut Harness) -> BridgeEvent {
        timeout(WAIT, h.events.recv()).await.unwrap().unwrap()
    }

    /// Connects and greets like the launcher would: `welcome` once `Connected` arrives.
    async fn greeted(h: &mut Harness) -> (Client, ConnId) {
        let mut c = Client::connect(h.endpoint.port()).await;
        c.hello(&token_of(&h.endpoint)).await;
        let BridgeEvent::Connected {
            conn,
            language,
            features,
            ..
        } = next_event(h).await
        else {
            panic!("not connected");
        };
        assert_eq!(language, "tr_tr");
        assert!(features.is_empty(), "this test mod says no features");
        h.control
            .send(Control::Send {
                conn,
                frames: vec![welcome()],
            })
            .unwrap();
        assert_eq!(c.next().await.unwrap()["t"], "welcome");
        (c, conn)
    }

    #[tokio::test]
    async fn greets_then_carries_frames_both_ways() {
        let mut h = start().await;
        let (mut c, conn) = greeted(&mut h).await;

        c.send(r#"{"t":"answer","call":"c1","answer":"coming"}"#)
            .await;
        c.send(r#"{"t":"from-the-future","x":1}"#).await;
        c.send(r#"{"t":"panel","open":true}"#).await;
        assert!(matches!(
            next_event(&mut h).await,
            BridgeEvent::Frame {
                frame: FromGame::Answer {
                    answer: CallAnswer::Coming,
                    ..
                },
                ..
            }
        ));
        assert_eq!(
            next_event(&mut h).await,
            BridgeEvent::Frame {
                conn,
                frame: FromGame::Panel { open: true }
            }
        );

        let toast = ToGame::Toast(ToastView {
            kind: ToastKind::Message,
            key: "d:1:2".into(),
            title: "Ayşe".into(),
            body: "Akşam bedwars?".into(),
            conv: Some("d:1:2".into()),
            crew: None,
            sticker: None,
            badge: None,
        });
        h.control
            .send(Control::Send {
                conn,
                frames: vec![toast],
            })
            .unwrap();
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
    async fn another_protocol_version_hears_why() {
        let h = start().await;
        let mut c = Client::connect(h.endpoint.port()).await;
        c.send(&format!(
            r#"{{"t":"hello","proto":2,"token":"{}"}}"#,
            token_of(&h.endpoint)
        ))
        .await;
        assert_eq!(c.next().await.unwrap()["reason"], "proto");
        assert!(c.next().await.is_none());
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
        let (mut c, conn) = greeted(&mut h).await;
        let huge = format!(
            r#"{{"t":"send","cid":"c","conv":"x","text":"{}"}}"#,
            "a".repeat(MAX_GAME_FRAME)
        );
        c.send(&huge).await;
        assert_eq!(next_event(&mut h).await, BridgeEvent::Disconnected { conn });
    }

    #[tokio::test]
    async fn a_new_connection_replaces_the_old_one() {
        let mut h = start().await;
        let (mut first, one) = greeted(&mut h).await;

        let mut second = Client::connect(h.endpoint.port()).await;
        second.hello(&token_of(&h.endpoint)).await;
        assert_eq!(first.next().await.unwrap()["reason"], "replaced");
        assert!(first.next().await.is_none());
        assert_eq!(
            next_event(&mut h).await,
            BridgeEvent::Disconnected { conn: one }
        );
        let BridgeEvent::Connected { conn: two, .. } = next_event(&mut h).await else {
            panic!("the new connection was not announced");
        };
        assert_ne!(one, two);
        // Frames meant for the replaced connection go nowhere; the new one gets its own.
        h.control
            .send(Control::Send {
                conn: one,
                frames: vec![ToGame::Dnd { on: true }],
            })
            .unwrap();
        h.control
            .send(Control::Send {
                conn: two,
                frames: vec![welcome()],
            })
            .unwrap();
        assert_eq!(second.next().await.unwrap()["t"], "welcome");
    }

    #[tokio::test]
    async fn closing_says_why_and_waits_for_it() {
        let mut h = start().await;
        let (mut c, _) = greeted(&mut h).await;
        let (done, closed) = oneshot::channel();
        h.control
            .send(Control::Close {
                reason: ByeReason::GameStartClose,
                done: Some(done),
            })
            .unwrap();
        timeout(WAIT, closed).await.unwrap().unwrap();
        assert_eq!(
            c.next().await.unwrap(),
            serde_json::json!({"t":"bye","reason":"game-start-close"})
        );
        assert!(c.next().await.is_none());
        // The listener went with it: serving is over.
        timeout(WAIT, h.served).await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn a_dropped_control_says_launcher_exit() {
        let mut h = start().await;
        let (mut c, _) = greeted(&mut h).await;
        drop(h.control);
        assert_eq!(
            c.next().await.unwrap(),
            serde_json::json!({"t":"bye","reason":"launcher-exit"})
        );
        assert!(c.next().await.is_none());
    }

    #[tokio::test]
    async fn closing_ends_connections_still_saying_hello() {
        let h = start().await;
        // Connected but silent: still in its handshake when the bridge closes.
        let mut silent = Client::connect(h.endpoint.port()).await;
        let (done, closed) = oneshot::channel();
        h.control
            .send(Control::Close {
                reason: ByeReason::Disabled,
                done: Some(done),
            })
            .unwrap();
        timeout(WAIT, closed).await.unwrap().unwrap();
        let mut rest = Vec::new();
        let read = timeout(Duration::from_secs(1), silent.reader.read_to_end(&mut rest))
            .await
            .expect("the silent connection was not closed");
        assert!(read.is_err() || rest.is_empty());
    }

    #[tokio::test]
    async fn jvm_args_carry_port_and_token_and_are_redactable() {
        let h = start().await;
        let [port, token] = h.endpoint.jvm_args();
        assert_eq!(port, format!("{PORT_ARG}{}", h.endpoint.port()));
        assert!(token.starts_with(TOKEN_ARG));
        assert_eq!(token.len(), TOKEN_ARG.len() + 43);
        assert_eq!(&token[TOKEN_ARG.len()..], h.endpoint.secret());
        assert!(is_secret_jvm_arg(&token));
        assert!(!is_secret_jvm_arg(&port));
        assert!(!format!("{:?}", h.endpoint).contains(h.endpoint.secret()));
    }

    #[test]
    fn the_doorman_budgets_failures_per_window() {
        let start = Instant::now();
        let mut door = Doorman::new(start);
        for _ in 0..FAILURES_PER_WINDOW {
            assert!(door.admits(start));
            door.failed(start, Refusal::Silent);
        }
        // The budget is spent: newcomers wait for the rest of the window.
        assert!(!door.admits(start + Duration::from_secs(1)));
        assert!(!door.admits(start + FAILURE_WINDOW - Duration::from_millis(1)));
        assert_eq!(door.turned_away, 2);
        // A new window starts afresh.
        assert!(door.admits(start + FAILURE_WINDOW));
        assert_eq!((door.failures, door.turned_away), (0, 0));
    }

    #[tokio::test]
    async fn a_flood_of_bad_handshakes_is_turned_away_then_let_in_again() {
        let mut h = start().await;
        for _ in 0..FAILURES_PER_WINDOW {
            let mut c = Client::connect(h.endpoint.port()).await;
            c.hello(&"y".repeat(43)).await;
            assert_eq!(c.next().await.unwrap()["reason"], "auth");
        }
        // The next one, even with the right token, is closed unread for the rest of the window.
        let mut late = Client::connect(h.endpoint.port()).await;
        late.hello(&token_of(&h.endpoint)).await;
        assert!(late.next().await.is_none());
        assert!(
            timeout(Duration::from_millis(200), h.events.recv())
                .await
                .is_err()
        );
    }

    #[test]
    fn frames_past_the_limit_are_refused_before_writing() {
        let frame = ToGame::History {
            conv: "c:1".into(),
            msgs: vec![
                crate::protocol::MsgView {
                    conv: None,
                    id: "i:1".into(),
                    cid: None,
                    from: "a".into(),
                    text: "ş".repeat(MAX_LAUNCHER_FRAME),
                    at: 0,
                    mine: false,
                    edited: false,
                    deleted: false,
                    ..crate::protocol::MsgView::default()
                };
                1
            ],
        };
        assert!(encode(&frame).is_err());
        assert!(encode(&ToGame::Dnd { on: true }).unwrap().ends_with(b"\n"));
    }
}
