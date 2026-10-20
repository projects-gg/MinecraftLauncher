//! Presence WebSocket client.
//!
//! - One background task per session; it reconnects by itself with exponential backoff and
//!   jitter (a healthy connection resets the backoff) and never retries in a tight loop.
//! - Status is sent only when it changes ([`PresenceHandle::set_status`]), and again after a
//!   reconnect when it is not "launcher" (the service starts every connection as "launcher").
//! - Keep-alive uses WebSocket protocol pings, which the service answers without waking up and
//!   which are not billed; a connection that stops answering is replaced.
//! - Close codes 4001 (session ended), 4002 (account deleted) and 4000 (replaced by a newer
//!   connection of the same account) stop the task: [`PresenceEvent::Stopped`].

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tokio::time::{Instant, sleep_until, timeout};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::http::header::{AUTHORIZATION, RETRY_AFTER, USER_AGENT};
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::{CloseFrame, WebSocketConfig};
use tokio_tungstenite::tungstenite::{self, Message};
use tokio_tungstenite::{
    Connector, MaybeTlsStream, WebSocketStream, connect_async_tls_with_config,
};
use url::Url;

use crate::auth::SessionToken;
use crate::backoff::{Backoff, BackoffPolicy, jitter};
use crate::error::{ErrorCode, Result, SocialError, parse_retry_after};
use crate::model::{FriendGroups, PublicProfile, StatusMode, Team, TeamExit, TeamInvite};
use crate::protocol::{
    CallAnswer, CallEndReason, ClientMessage, CrewCall, Invite, LocalStatus, Presence,
    ServerMessage,
};
use crate::tls;

#[derive(Debug, Clone, Copy)]
pub struct PresenceOptions {
    /// Protocol ping interval. Cloudflare closes idle sockets after about 100 s.
    pub keepalive: Duration,
    /// How long a ping may stay unanswered before the connection counts as dead.
    pub pong_timeout: Duration,
    pub backoff: BackoffPolicy,
    /// A connection that stayed up this long resets the backoff.
    pub stable_after: Duration,
    pub connect_timeout: Duration,
    /// How long [`PresenceHandle::invite`] (and a crew call request) waits for the service's
    /// answer.
    pub invite_timeout: Duration,
}

impl Default for PresenceOptions {
    fn default() -> Self {
        Self {
            keepalive: Duration::from_secs(45),
            pong_timeout: Duration::from_secs(20),
            backoff: BackoffPolicy::default(),
            stable_after: Duration::from_secs(60),
            connect_timeout: Duration::from_secs(15),
            invite_timeout: Duration::from_secs(10),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PresenceEvent {
    /// After every (re)connect: your status mode, the friends and teammates who are online right
    /// now (replace the whole online list), invites that waited for you (show them as a quiet
    /// list) and every open crew call you see (replace the list; show them without a
    /// notification). `clock` is the service's time (Unix seconds) for the calls' deadlines.
    Connected {
        status: StatusMode,
        friends: Vec<Presence>,
        invites: Vec<Invite>,
        calls: Vec<CrewCall>,
        clock: Option<i64>,
    },
    /// A crew call, new or changed: replaces the one with the same id. `quiet`: show it but do
    /// not announce it (your own call, do-not-disturb, or a call that is only new to you).
    Call {
        call: CrewCall,
        quiet: bool,
        clock: Option<i64>,
    },
    /// Someone answered a call (you too, from another device).
    CallAnswered {
        call: String,
        by: String,
        answer: CallAnswer,
    },
    /// A call ended before its time (calls that run out end without a message).
    CallEnded {
        call: String,
        reason: CallEndReason,
    },
    /// A friend's or a teammate's presence changed.
    Presence(Presence),
    /// Your team changed (members, roles, name, code), or you are no longer in one (None, with
    /// the reason; `by` is who made the change).
    TeamChanged {
        team: Option<Team>,
        reason: Option<TeamExit>,
        by: Option<String>,
    },
    /// A friend invited you to their team.
    TeamInvited(TeamInvite),
    /// Your friend categories changed on another device.
    GroupsChanged(FriendGroups),
    /// A request was accepted (by either side): a new friend, with their current presence.
    FriendAdded {
        friend: PublicProfile,
        presence: Presence,
    },
    /// Unfriended, blocked or deleted: drop them from the list (no reason is given).
    FriendRemoved {
        id: String,
    },
    RequestReceived {
        from: PublicProfile,
    },
    RequestCancelled {
        id: String,
    },
    /// "Birlikte oyna", live.
    Invite(Invite),
    /// Invites that waited while you were on do-not-disturb.
    QueuedInvites(Vec<Invite>),
    /// Your status mode changed on another device.
    StatusChanged(StatusMode),
    /// The connection dropped; the task reconnects after `retry_in`.
    Disconnected {
        retry_in: Duration,
    },
    /// The task ended and will not reconnect.
    Stopped(StopReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// The session is no longer valid (signed out, revoked, expired): sign in again.
    SignedOut,
    /// The account was deleted.
    AccountDeleted,
    /// A newer connection of the same account replaced this one (too many devices).
    Replaced,
    /// [`PresenceHandle::shutdown`] or the handle was dropped.
    Shutdown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InviteOutcome {
    /// The friend saw it right away.
    Delivered,
    /// The friend is offline or on do-not-disturb; it waits for up to an hour.
    Queued,
}

/// What a crew call request asks of the service ("Ekibi çağır", an answer, "Bitir").
#[derive(Debug, Clone, PartialEq, Eq)]
enum CrewRequest {
    Call,
    Answer { call: String, answer: CallAnswer },
    End { call: String },
}

enum Command {
    Invite {
        to: String,
        reply: oneshot::Sender<Result<InviteOutcome>>,
    },
    Crew {
        request: CrewRequest,
        reply: oneshot::Sender<Result<()>>,
    },
    Shutdown,
}

impl Command {
    /// The service cannot be reached for a while: the request fails at once.
    fn refuse(self, retry_after: Option<Duration>) {
        let error = || SocialError::Unavailable { retry_after };
        match self {
            Self::Invite { reply, .. } => drop(reply.send(Err(error()))),
            Self::Crew { reply, .. } => drop(reply.send(Err(error()))),
            Self::Shutdown => {}
        }
    }
}

/// Controls the background presence task: status, invites and crew calls. Cheap to clone; once
/// every control is gone the task closes the connection cleanly and stops.
#[derive(Clone)]
pub struct PresenceControl {
    status: Arc<watch::Sender<LocalStatus>>,
    commands: mpsc::Sender<Command>,
    invite_timeout: Duration,
}

impl std::fmt::Debug for PresenceControl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PresenceControl").finish_non_exhaustive()
    }
}

impl PresenceControl {
    /// Sets what this launcher is doing. Only a real change is sent; returns whether it changed.
    pub fn set_status(&self, status: LocalStatus) -> bool {
        self.status.send_if_modified(|current| {
            if *current == status {
                false
            } else {
                *current = status;
                true
            }
        })
    }

    pub fn status(&self) -> LocalStatus {
        self.status.borrow().clone()
    }

    /// Sends a "Birlikte oyna" invite to a friend (limits: one per friend per 5 minutes, 30 a
    /// day). Fails with `Unavailable` while disconnected.
    pub async fn invite(&self, friend_id: &str) -> Result<InviteOutcome> {
        let (reply, answer) = oneshot::channel();
        self.commands
            .send(Command::Invite {
                to: friend_id.to_owned(),
                reply,
            })
            .await
            .map_err(|_| SocialError::Unavailable { retry_after: None })?;
        match timeout(self.invite_timeout, answer).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(SocialError::Network("presence connection closed".into())),
            Err(_) => Err(SocialError::Network("no answer from the service".into())),
        }
    }

    /// "Ekibi çağır" (limits: one open call among teammates who see each other, one every 5
    /// minutes and 10 a day per player). The call itself arrives as [`PresenceEvent::Call`]
    /// before this returns. Fails with `RateLimited` (with the wait), `Refused` `call_active` or
    /// `team_not_found`, or `Unavailable` while disconnected.
    pub async fn call_crew(&self) -> Result<()> {
        self.crew(CrewRequest::Call).await
    }

    /// Answers (or changes the answer to) a teammate's call. `Refused` `call_not_found` when the
    /// call is gone.
    pub async fn answer_call(&self, call: &str, answer: CallAnswer) -> Result<()> {
        self.crew(CrewRequest::Answer {
            call: call.to_owned(),
            answer,
        })
        .await
    }

    /// Ends the player's own call ("Bitir").
    pub async fn end_call(&self, call: &str) -> Result<()> {
        self.crew(CrewRequest::End {
            call: call.to_owned(),
        })
        .await
    }

    async fn crew(&self, request: CrewRequest) -> Result<()> {
        let (reply, answer) = oneshot::channel();
        self.commands
            .send(Command::Crew { request, reply })
            .await
            .map_err(|_| SocialError::Unavailable { retry_after: None })?;
        match timeout(self.invite_timeout, answer).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(SocialError::Network("presence connection closed".into())),
            Err(_) => Err(SocialError::Network("no answer from the service".into())),
        }
    }

    /// Asks the task to close the connection cleanly and stop.
    pub async fn shutdown(&self) {
        let _ = self.commands.send(Command::Shutdown).await;
    }
}

/// The events of the background presence task. Dropping it stops the task.
pub struct PresenceEvents {
    events: mpsc::UnboundedReceiver<PresenceEvent>,
    task: Option<JoinHandle<()>>,
}

impl PresenceEvents {
    /// The next event; None after the task has stopped and every event was read.
    pub async fn next_event(&mut self) -> Option<PresenceEvent> {
        self.events.recv().await
    }

    pub fn try_next_event(&mut self) -> Option<PresenceEvent> {
        self.events.try_recv().ok()
    }

    /// Waits until the task has ended.
    async fn finished(mut self) {
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }
}

impl Drop for PresenceEvents {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

/// Controls the background presence task and reads its events. Dropping it stops the task;
/// [`PresenceHandle::split`] hands the two halves to different owners.
pub struct PresenceHandle {
    control: PresenceControl,
    events: PresenceEvents,
}

impl PresenceHandle {
    pub(crate) fn spawn(
        url: Url,
        token: SessionToken,
        user_agent: String,
        options: PresenceOptions,
    ) -> Self {
        let (status, status_rx) = watch::channel(LocalStatus::Launcher);
        let (commands, commands_rx) = mpsc::channel(32);
        let (events_tx, events) = mpsc::unbounded_channel();
        let endpoint = Endpoint {
            url,
            token,
            user_agent,
            connect_timeout: options.connect_timeout,
        };
        let task = tokio::spawn(run(endpoint, status_rx, commands_rx, events_tx, options));
        Self {
            control: PresenceControl {
                status: Arc::new(status),
                commands,
                invite_timeout: options.invite_timeout,
            },
            events: PresenceEvents {
                events,
                task: Some(task),
            },
        }
    }

    /// Sets what this launcher is doing. Only a real change is sent; returns whether it changed.
    pub fn set_status(&self, status: LocalStatus) -> bool {
        self.control.set_status(status)
    }

    pub fn status(&self) -> LocalStatus {
        self.control.status()
    }

    /// Sends a "Birlikte oyna" invite to a friend (limits: one per friend per 5 minutes, 30 a
    /// day). Fails with `Unavailable` while disconnected.
    pub async fn invite(&self, friend_id: &str) -> Result<InviteOutcome> {
        self.control.invite(friend_id).await
    }

    /// "Ekibi çağır"; see [`PresenceControl::call_crew`].
    pub async fn call_crew(&self) -> Result<()> {
        self.control.call_crew().await
    }

    /// See [`PresenceControl::answer_call`].
    pub async fn answer_call(&self, call: &str, answer: CallAnswer) -> Result<()> {
        self.control.answer_call(call, answer).await
    }

    /// See [`PresenceControl::end_call`].
    pub async fn end_call(&self, call: &str) -> Result<()> {
        self.control.end_call(call).await
    }

    /// The next event; None after the task has stopped and every event was read.
    pub async fn next_event(&mut self) -> Option<PresenceEvent> {
        self.events.next_event().await
    }

    pub fn try_next_event(&mut self) -> Option<PresenceEvent> {
        self.events.try_next_event()
    }

    /// A control for status and invites that can live elsewhere (the events stay here).
    pub fn control(&self) -> PresenceControl {
        self.control.clone()
    }

    /// The control half (status, invites) and the event half, for separate owners.
    pub fn split(self) -> (PresenceControl, PresenceEvents) {
        (self.control, self.events)
    }

    /// Closes the connection cleanly and waits for the task.
    pub async fn shutdown(self) {
        self.control.shutdown().await;
        self.events.finished().await;
    }
}

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// How a connection attempt or a connection ended.
#[derive(Debug, PartialEq)]
enum Outcome {
    Retry { at_least: Duration },
    Stop(StopReason),
}

const RETRY_NOW: Outcome = Outcome::Retry {
    at_least: Duration::ZERO,
};

struct Endpoint {
    url: Url,
    token: SessionToken,
    user_agent: String,
    connect_timeout: Duration,
}

impl Endpoint {
    async fn connect(&self) -> std::result::Result<Socket, Outcome> {
        let mut request = self
            .url
            .as_str()
            .into_client_request()
            .map_err(|_| Outcome::Stop(StopReason::Shutdown))?;
        let headers = request.headers_mut();
        let bearer = HeaderValue::from_str(&format!("Bearer {}", self.token.expose()))
            .map_err(|_| Outcome::Stop(StopReason::SignedOut))?;
        headers.insert(AUTHORIZATION, bearer);
        if let Ok(agent) = HeaderValue::from_str(&self.user_agent) {
            headers.insert(USER_AGENT, agent);
        }
        let connector = if self.url.scheme() == "wss" {
            match tls::client_config() {
                Ok(config) => Connector::Rustls(config),
                Err(error) => {
                    tracing::warn!(%error, "presence TLS setup failed");
                    return Err(Outcome::Retry {
                        at_least: Duration::from_secs(300),
                    });
                }
            }
        } else {
            Connector::Plain
        };
        let config = WebSocketConfig::default()
            .max_message_size(Some(1 << 20))
            .max_frame_size(Some(1 << 20));
        let attempt = connect_async_tls_with_config(request, Some(config), true, Some(connector));
        match timeout(self.connect_timeout, attempt).await {
            Ok(Ok((socket, _))) => Ok(socket),
            Ok(Err(tungstenite::Error::Http(response))) => {
                let retry_after = parse_retry_after(
                    response
                        .headers()
                        .get(RETRY_AFTER)
                        .and_then(|value| value.to_str().ok()),
                );
                Err(handshake_outcome(response.status().as_u16(), retry_after))
            }
            Ok(Err(error)) => {
                tracing::debug!(%error, "presence connect failed");
                Err(RETRY_NOW)
            }
            Err(_) => Err(RETRY_NOW),
        }
    }
}

fn handshake_outcome(status: u16, retry_after: Option<Duration>) -> Outcome {
    match status {
        401 => Outcome::Stop(StopReason::SignedOut),
        // Rate limited or a daily limit reached: wait as long as asked (at most an hour).
        429 | 503 => Outcome::Retry {
            at_least: retry_after
                .unwrap_or(Duration::from_secs(60))
                .min(Duration::from_secs(3_600)),
        },
        500..=599 => RETRY_NOW,
        // Something is misconfigured; do not hammer the service.
        _ => Outcome::Retry {
            at_least: Duration::from_secs(300),
        },
    }
}

fn close_outcome(frame: Option<&CloseFrame>) -> Outcome {
    match frame.map(|frame| u16::from(frame.code)) {
        Some(4001) => Outcome::Stop(StopReason::SignedOut),
        Some(4002) => Outcome::Stop(StopReason::AccountDeleted),
        Some(4000) => Outcome::Stop(StopReason::Replaced),
        Some(4008) => Outcome::Retry {
            at_least: Duration::from_secs(60),
        },
        Some(4003) => Outcome::Retry {
            at_least: Duration::from_secs(30),
        },
        _ => RETRY_NOW,
    }
}

async fn run(
    endpoint: Endpoint,
    mut status: watch::Receiver<LocalStatus>,
    mut commands: mpsc::Receiver<Command>,
    events: mpsc::UnboundedSender<PresenceEvent>,
    options: PresenceOptions,
) {
    let mut backoff = Backoff::new(options.backoff);
    loop {
        let started = Instant::now();
        let outcome = match endpoint.connect().await {
            Ok(socket) => session(socket, &mut status, &mut commands, &events, &options).await,
            Err(outcome) => outcome,
        };
        let wait = match outcome {
            Outcome::Stop(reason) => {
                let _ = events.send(PresenceEvent::Stopped(reason));
                return;
            }
            Outcome::Retry { at_least } => {
                if started.elapsed() >= options.stable_after {
                    backoff.reset();
                }
                backoff.next_delay().max(at_least)
            }
        };
        if events
            .send(PresenceEvent::Disconnected { retry_in: wait })
            .is_err()
        {
            return;
        }
        let deadline = Instant::now() + wait;
        loop {
            tokio::select! {
                () = sleep_until(deadline) => break,
                command = commands.recv() => match command {
                    None | Some(Command::Shutdown) => {
                        let _ = events.send(PresenceEvent::Stopped(StopReason::Shutdown));
                        return;
                    }
                    Some(command) => {
                        command.refuse(Some(deadline.saturating_duration_since(Instant::now())));
                    }
                },
            }
        }
    }
}

type Sink = SplitSink<Socket, Message>;

/// Who waits for the service's `ack` (or `error`) of a request, by its `rid`.
enum Reply {
    Invite(oneshot::Sender<Result<InviteOutcome>>),
    Crew(oneshot::Sender<Result<()>>),
}

impl Reply {
    fn fail(self, error: SocialError) {
        match self {
            Self::Invite(reply) => drop(reply.send(Err(error))),
            Self::Crew(reply) => drop(reply.send(Err(error))),
        }
    }
}

type Pending = HashMap<u32, Reply>;

/// One connection, from the handshake to its end.
async fn session(
    socket: Socket,
    status: &mut watch::Receiver<LocalStatus>,
    commands: &mut mpsc::Receiver<Command>,
    events: &mpsc::UnboundedSender<PresenceEvent>,
    options: &PresenceOptions,
) -> Outcome {
    let (mut sink, mut stream): (Sink, SplitStream<Socket>) = socket.split();
    let mut pending: Pending = HashMap::new();
    let mut state = SessionState {
        greeted: false,
        sent: None,
        next_rid: 0,
        keepalive: options.keepalive,
    };
    let mut last_inbound = Instant::now();
    let mut next_ping = Instant::now() + spread(state.keepalive);
    let mut ping_sent: Option<Instant> = None;

    let outcome = loop {
        let pong_deadline = ping_sent.map(|at| at + options.pong_timeout);
        tokio::select! {
            frame = stream.next() => match frame {
                Some(Ok(Message::Text(text))) => {
                    last_inbound = Instant::now();
                    ping_sent = None;
                    if let Err(outcome) =
                        on_text(text.as_str(), &mut state, &mut sink, status, events, &mut pending).await
                    {
                        break outcome;
                    }
                }
                Some(Ok(Message::Close(frame))) => break close_outcome(frame.as_ref()),
                Some(Ok(_)) => {
                    last_inbound = Instant::now();
                    ping_sent = None;
                }
                Some(Err(error)) => {
                    tracing::debug!(%error, "presence connection error");
                    break RETRY_NOW;
                }
                None => break RETRY_NOW,
            },
            changed = status.changed(), if state.greeted => {
                if changed.is_err() {
                    break Outcome::Stop(StopReason::Shutdown);
                }
                if send_status(&mut state, &mut sink, status).await.is_err() {
                    break RETRY_NOW;
                }
            }
            command = commands.recv() => match command {
                None | Some(Command::Shutdown) => {
                    let _ = sink
                        .send(Message::Close(Some(CloseFrame {
                            code: CloseCode::Normal,
                            reason: "bye".into(),
                        })))
                        .await;
                    break Outcome::Stop(StopReason::Shutdown);
                }
                Some(command) => {
                    if !state.greeted {
                        command.refuse(None);
                        continue;
                    }
                    state.next_rid = state.next_rid.wrapping_add(1) % 1_000_000;
                    let rid = state.next_rid;
                    let (message, reply) = match command {
                        Command::Invite { to, reply } => {
                            (ClientMessage::Invite { to, rid }, Reply::Invite(reply))
                        }
                        Command::Crew { request, reply } => {
                            (crew_message(request, rid), Reply::Crew(reply))
                        }
                        Command::Shutdown => continue,
                    };
                    let text = serde_json::to_string(&message).unwrap_or_default();
                    if sink.send(Message::text(text)).await.is_err() {
                        reply.fail(SocialError::Network("presence connection lost".into()));
                        break RETRY_NOW;
                    }
                    pending.insert(rid, reply);
                }
            },
            () = sleep_until(next_ping) => {
                if sink.send(Message::Ping(Default::default())).await.is_err() {
                    break RETRY_NOW;
                }
                ping_sent.get_or_insert_with(Instant::now);
                next_ping = Instant::now() + spread(state.keepalive);
            }
            () = sleep_until(pong_deadline.unwrap_or(next_ping)), if pong_deadline.is_some() => {
                if ping_sent.is_some_and(|at| last_inbound < at) {
                    tracing::debug!("presence connection stopped answering");
                    break RETRY_NOW;
                }
            }
        }
    };
    for (_, reply) in pending.drain() {
        reply.fail(SocialError::Network("presence connection lost".into()));
    }
    outcome
}

fn crew_message(request: CrewRequest, rid: u32) -> ClientMessage {
    match request {
        CrewRequest::Call => ClientMessage::Call { rid },
        CrewRequest::Answer { call, answer } => ClientMessage::CallAnswer { call, answer, rid },
        CrewRequest::End { call } => ClientMessage::CallEnd { call, rid },
    }
}

/// Pings every `keepalive` ± 10 %, so clients do not ping in step.
fn spread(keepalive: Duration) -> Duration {
    jitter(keepalive.mul_f64(0.9), keepalive.mul_f64(1.1))
}

struct SessionState {
    greeted: bool,
    /// The status the service has for this connection (None before `hello`).
    sent: Option<LocalStatus>,
    next_rid: u32,
    keepalive: Duration,
}

async fn send_status(
    state: &mut SessionState,
    sink: &mut Sink,
    status: &mut watch::Receiver<LocalStatus>,
) -> std::result::Result<(), tungstenite::Error> {
    let desired = status.borrow_and_update().clone();
    if state.sent.as_ref() == Some(&desired) {
        return Ok(());
    }
    let message = serde_json::to_string(&ClientMessage::status(&desired)).unwrap_or_default();
    sink.send(Message::text(message)).await?;
    state.sent = Some(desired);
    Ok(())
}

async fn on_text(
    text: &str,
    state: &mut SessionState,
    sink: &mut Sink,
    status: &mut watch::Receiver<LocalStatus>,
    events: &mpsc::UnboundedSender<PresenceEvent>,
    pending: &mut Pending,
) -> std::result::Result<(), Outcome> {
    let message: ServerMessage = match serde_json::from_str(text) {
        Ok(message) => message,
        Err(error) => {
            tracing::debug!(%error, "ignoring an unreadable presence message");
            return Ok(());
        }
    };
    let event = match message {
        ServerMessage::Hello {
            status: mode,
            keepalive_seconds,
            mut friends,
            team,
            invites,
            calls,
            now,
        } => {
            state.greeted = true;
            // Every connection starts as "launcher" on the service.
            state.sent = Some(LocalStatus::Launcher);
            if let Some(seconds) = keepalive_seconds.filter(|s| (15..=300).contains(s)) {
                state.keepalive = state.keepalive.min(Duration::from_secs(seconds));
            }
            // Everyone online the player may see: friends, then teammates who are not friends.
            friends.extend(team);
            let _ = events.send(PresenceEvent::Connected {
                status: mode,
                friends,
                invites,
                calls,
                clock: now,
            });
            return send_status(state, sink, status)
                .await
                .map_err(|_| RETRY_NOW);
        }
        ServerMessage::Call { call, quiet, now } => PresenceEvent::Call {
            call,
            quiet,
            clock: now,
        },
        ServerMessage::CallAnswer { call, by, answer } => {
            let Some(answer) = CallAnswer::parse(&answer) else {
                return Ok(());
            };
            PresenceEvent::CallAnswered { call, by, answer }
        }
        ServerMessage::CallEnd { call, reason } => PresenceEvent::CallEnded { call, reason },
        ServerMessage::Presence { presence } => PresenceEvent::Presence(presence),
        ServerMessage::FriendAdded { friend, presence } => {
            PresenceEvent::FriendAdded { friend, presence }
        }
        ServerMessage::FriendRemoved { id } => PresenceEvent::FriendRemoved { id },
        ServerMessage::Request { from } => PresenceEvent::RequestReceived { from },
        ServerMessage::RequestCancelled { id } => PresenceEvent::RequestCancelled { id },
        ServerMessage::Invite { invite } => PresenceEvent::Invite(invite),
        ServerMessage::Invites { invites } => PresenceEvent::QueuedInvites(invites),
        ServerMessage::Status { status } => PresenceEvent::StatusChanged(status),
        ServerMessage::Team { team, reason, by } => PresenceEvent::TeamChanged { team, reason, by },
        ServerMessage::TeamInvite { invite } => PresenceEvent::TeamInvited(invite),
        ServerMessage::Groups { groups } => PresenceEvent::GroupsChanged(groups),
        ServerMessage::Ack { rid, delivered } => {
            match rid.and_then(|rid| pending.remove(&rid)) {
                Some(Reply::Invite(reply)) => {
                    let outcome = if delivered == Some(true) {
                        InviteOutcome::Delivered
                    } else {
                        InviteOutcome::Queued
                    };
                    let _ = reply.send(Ok(outcome));
                }
                Some(Reply::Crew(reply)) => {
                    let _ = reply.send(Ok(()));
                }
                None => {}
            }
            return Ok(());
        }
        ServerMessage::Error {
            rid,
            code,
            retry_after,
        } => {
            if let Some(reply) = rid.and_then(|rid| pending.remove(&rid)) {
                reply.fail(socket_error(&code, retry_after));
            } else {
                tracing::debug!(%code, "presence error");
            }
            return Ok(());
        }
        ServerMessage::Bye { reason } => {
            tracing::debug!(%reason, "presence connection closing");
            return Ok(());
        }
        ServerMessage::Unknown => return Ok(()),
    };
    events
        .send(event)
        .map_err(|_| Outcome::Stop(StopReason::Shutdown))
}

/// The error of a socket request (an invite or a crew call request).
fn socket_error(code: &str, retry_after: Option<u64>) -> SocialError {
    let status = match code {
        "rate_limited" => {
            return SocialError::RateLimited {
                retry_after: retry_after.map(Duration::from_secs),
            };
        }
        "friend_not_found" | "team_not_found" | "call_not_found" => 404,
        "team_not_permitted" => 403,
        "call_active" => 409,
        _ => 400,
    };
    SocialError::Refused {
        status,
        code: ErrorCode::parse(code),
        message: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use tokio::net::TcpListener;
    use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};

    use super::*;
    use crate::SocialClient;
    use crate::client::SocialConfig;

    fn fast() -> PresenceOptions {
        PresenceOptions {
            keepalive: Duration::from_secs(30),
            pong_timeout: Duration::from_secs(5),
            backoff: BackoffPolicy {
                initial: Duration::from_millis(20),
                max: Duration::from_millis(50),
                spread: Duration::ZERO,
            },
            stable_after: Duration::from_secs(60),
            connect_timeout: Duration::from_secs(5),
            invite_timeout: Duration::from_secs(5),
        }
    }

    fn token() -> SessionToken {
        SessionToken::new(format!("pls_{}", "Z".repeat(43)))
    }

    type Server = WebSocketStream<TcpStream>;

    /// Accepts one connection; `reject` answers the handshake with that status instead.
    async fn accept(
        listener: &TcpListener,
        reject: Option<(u16, Option<&'static str>)>,
        seen: Arc<Mutex<Vec<(String, String)>>>,
    ) -> Option<Server> {
        let (stream, _) = listener.accept().await.unwrap();
        // tungstenite's handshake callback fixes the error type (`ErrorResponse`); it cannot be
        // boxed here.
        #[allow(clippy::result_large_err)]
        let callback = move |request: &Request, response: Response| {
            let mut log = seen.lock().unwrap();
            for (name, value) in request.headers() {
                log.push((name.to_string(), value.to_str().unwrap_or("").to_owned()));
            }
            match reject {
                None => Ok(response),
                Some((status, retry_after)) => {
                    let mut error = ErrorResponse::new(None);
                    *error.status_mut() = tungstenite::http::StatusCode::from_u16(status).unwrap();
                    if let Some(value) = retry_after {
                        error
                            .headers_mut()
                            .insert(RETRY_AFTER, HeaderValue::from_static(value));
                    }
                    Err(error)
                }
            }
        };
        tokio_tungstenite::accept_hdr_async(stream, callback)
            .await
            .ok()
    }

    async fn send_json(server: &mut Server, value: serde_json::Value) {
        server.send(Message::text(value.to_string())).await.unwrap();
    }

    async fn next_json(server: &mut Server) -> serde_json::Value {
        loop {
            match server.next().await.unwrap().unwrap() {
                Message::Text(text) => return serde_json::from_str(text.as_str()).unwrap(),
                Message::Ping(_) | Message::Pong(_) => continue,
                other => panic!("unexpected frame {other:?}"),
            }
        }
    }

    fn hello() -> serde_json::Value {
        serde_json::json!({
            "t": "hello", "v": 1, "status": "online", "keepaliveSeconds": 45,
            "friends": [{"id": "111111111111111111", "state": "launcher", "status": "online",
                         "game": null, "avatar": "discord", "avatarHash": null}],
            "invites": []
        })
    }

    async fn start() -> (TcpListener, SocialClient) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/", listener.local_addr().unwrap());
        let client = SocialClient::new(SocialConfig::new(
            Url::parse(&base).unwrap(),
            "ProjectsLauncher/test",
        ))
        .unwrap();
        (listener, client)
    }

    #[tokio::test]
    async fn connects_streams_events_and_sends_status_only_on_change() {
        let (listener, client) = start().await;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut handle = client.session(token()).connect_presence(fast());
        let mut server = accept(&listener, None, seen.clone()).await.unwrap();
        {
            let headers = seen.lock().unwrap();
            assert!(headers.iter().any(
                |(k, v)| k == "authorization" && v == &format!("Bearer pls_{}", "Z".repeat(43))
            ));
            assert!(
                headers
                    .iter()
                    .any(|(k, v)| k == "user-agent" && v == "ProjectsLauncher/test")
            );
        }
        send_json(&mut server, hello()).await;
        match handle.next_event().await.unwrap() {
            PresenceEvent::Connected {
                status, friends, ..
            } => {
                assert_eq!(status, StatusMode::Online);
                assert_eq!(friends.len(), 1);
            }
            other => panic!("unexpected {other:?}"),
        }

        assert!(handle.set_status(LocalStatus::Playing {
            host: Some("projects.gg".into())
        }));
        assert_eq!(
            next_json(&mut server).await,
            serde_json::json!({"t": "status", "state": "playing", "host": "projects.gg"})
        );
        // The same status again is not sent.
        assert!(!handle.set_status(LocalStatus::Playing {
            host: Some("projects.gg".into())
        }));
        assert!(handle.set_status(LocalStatus::Singleplayer));
        assert_eq!(
            next_json(&mut server).await,
            serde_json::json!({"t": "status", "state": "playing", "singleplayer": true})
        );

        send_json(
            &mut server,
            serde_json::json!({"t": "presence", "presence": {"id": "111111111111111111",
                "state": "offline", "lastSeen": "today"}}),
        )
        .await;
        assert!(matches!(
            handle.next_event().await.unwrap(),
            PresenceEvent::Presence(p) if p.state == crate::protocol::PresenceState::Offline
        ));

        // The control half works from another task while this one keeps the events.
        let invite = tokio::spawn({
            let control = handle.control();
            async move { control.invite("111111111111111111").await }
        });
        let sent = next_json(&mut server).await;
        assert_eq!(sent["t"], "invite");
        assert_eq!(sent["to"], "111111111111111111");
        send_json(
            &mut server,
            serde_json::json!({"t": "ack", "rid": sent["rid"], "delivered": false}),
        )
        .await;
        assert_eq!(invite.await.unwrap().unwrap(), InviteOutcome::Queued);

        handle.shutdown().await;
        assert!(matches!(
            server.next().await,
            Some(Ok(Message::Close(Some(frame)))) if frame.code == CloseCode::Normal
        ));
    }

    #[tokio::test]
    async fn reconnects_with_backoff_and_restores_the_status() {
        let (listener, client) = start().await;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut handle = client.session(token()).connect_presence(fast());
        let mut server = accept(&listener, None, seen.clone()).await.unwrap();
        send_json(&mut server, hello()).await;
        handle.next_event().await.unwrap();
        handle.set_status(LocalStatus::Playing { host: None });
        assert_eq!(next_json(&mut server).await["state"], "playing");

        // Cloudflare restarts the server: the socket just goes away.
        drop(server);
        match handle.next_event().await.unwrap() {
            PresenceEvent::Disconnected { retry_in } => {
                assert!(retry_in <= Duration::from_millis(50));
            }
            other => panic!("unexpected {other:?}"),
        }
        let mut server = accept(&listener, None, seen).await.unwrap();
        send_json(&mut server, hello()).await;
        assert!(matches!(
            handle.next_event().await.unwrap(),
            PresenceEvent::Connected { .. }
        ));
        // The new connection starts as "launcher" on the service; the client restores "playing".
        assert_eq!(
            next_json(&mut server).await,
            serde_json::json!({"t": "status", "state": "playing"})
        );
    }

    #[tokio::test]
    async fn stops_when_the_session_ends() {
        let (listener, client) = start().await;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut handle = client.session(token()).connect_presence(fast());
        let mut server = accept(&listener, None, seen.clone()).await.unwrap();
        send_json(&mut server, hello()).await;
        handle.next_event().await.unwrap();
        send_json(
            &mut server,
            serde_json::json!({"t": "bye", "reason": "session_revoked"}),
        )
        .await;
        server
            .close(Some(CloseFrame {
                code: CloseCode::from(4001),
                reason: "session_revoked".into(),
            }))
            .await
            .unwrap();
        assert_eq!(
            handle.next_event().await.unwrap(),
            PresenceEvent::Stopped(StopReason::SignedOut)
        );
        assert!(handle.next_event().await.is_none());

        // A token the service no longer accepts is rejected at the handshake: stop, no retry.
        let mut handle = client.session(token()).connect_presence(fast());
        assert!(accept(&listener, Some((401, None)), seen).await.is_none());
        assert_eq!(
            handle.next_event().await.unwrap(),
            PresenceEvent::Stopped(StopReason::SignedOut)
        );
    }

    #[tokio::test]
    async fn honours_retry_after_when_rate_limited() {
        let (listener, client) = start().await;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut handle = client.session(token()).connect_presence(fast());
        assert!(
            accept(&listener, Some((429, Some("120"))), seen)
                .await
                .is_none()
        );
        match handle.next_event().await.unwrap() {
            PresenceEvent::Disconnected { retry_in } => {
                assert!(retry_in >= Duration::from_secs(120));
            }
            other => panic!("unexpected {other:?}"),
        }
        // While waiting, invites fail fast instead of queueing up.
        let error = handle.invite("111111111111111111").await.unwrap_err();
        assert!(matches!(error, SocialError::Unavailable { .. }));
        handle.shutdown().await;
    }

    #[tokio::test]
    async fn replaces_a_connection_that_stops_answering_pings() {
        let (listener, client) = start().await;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let options = PresenceOptions {
            keepalive: Duration::from_millis(100),
            pong_timeout: Duration::from_millis(100),
            ..fast()
        };
        let mut handle = client.session(token()).connect_presence(options);
        let mut server = accept(&listener, None, seen).await.unwrap();
        send_json(&mut server, hello()).await;
        handle.next_event().await.unwrap();
        // The server stops reading (so it never answers pings) but keeps the TCP connection.
        let event = timeout(Duration::from_secs(5), handle.next_event())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(event, PresenceEvent::Disconnected { .. }));
        drop(server);
    }

    #[test]
    fn maps_close_codes_and_handshake_statuses() {
        let frame = |code: u16| CloseFrame {
            code: CloseCode::from(code),
            reason: "".into(),
        };
        assert_eq!(
            close_outcome(Some(&frame(4002))),
            Outcome::Stop(StopReason::AccountDeleted)
        );
        assert_eq!(
            close_outcome(Some(&frame(4000))),
            Outcome::Stop(StopReason::Replaced)
        );
        assert_eq!(
            close_outcome(Some(&frame(4008))),
            Outcome::Retry {
                at_least: Duration::from_secs(60)
            }
        );
        assert_eq!(close_outcome(Some(&frame(1006))), RETRY_NOW);
        assert_eq!(close_outcome(None), RETRY_NOW);
        assert_eq!(
            handshake_outcome(503, Some(Duration::from_secs(7_200))),
            Outcome::Retry {
                at_least: Duration::from_secs(3_600)
            }
        );
        assert_eq!(handshake_outcome(502, None), RETRY_NOW);
        assert_eq!(
            handshake_outcome(426, None),
            Outcome::Retry {
                at_least: Duration::from_secs(300)
            }
        );
        assert!(matches!(
            socket_error("rate_limited", Some(30)),
            SocialError::RateLimited { retry_after: Some(d) } if d == Duration::from_secs(30)
        ));
        assert_eq!(
            socket_error("friend_not_found", None).code(),
            Some(&ErrorCode::FriendNotFound)
        );
        assert_eq!(
            socket_error("call_active", None).code(),
            Some(&ErrorCode::CallActive)
        );
        assert!(matches!(
            socket_error("call_not_found", None),
            SocialError::Refused {
                status: 404,
                code: ErrorCode::CallNotFound,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn crew_calls_go_out_with_their_rid_and_come_back_as_events() {
        let (listener, client) = start().await;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut handle = client.session(token()).connect_presence(fast());
        let mut server = accept(&listener, None, seen).await.unwrap();
        let mut greeting = hello();
        greeting["calls"] = serde_json::json!([{"id": "c0", "from": "111111111111111111",
            "kind": "ready", "at": 1_792_521_000, "until": 1_792_522_200, "answers": {}}]);
        greeting["now"] = serde_json::json!(1_792_521_300);
        send_json(&mut server, greeting).await;
        match handle.next_event().await.unwrap() {
            PresenceEvent::Connected { calls, clock, .. } => {
                assert_eq!(calls.len(), 1);
                assert_eq!(clock, Some(1_792_521_300));
            }
            other => panic!("unexpected {other:?}"),
        }

        // "Ekibi çağır": the own call arrives first, then the ack.
        let call = tokio::spawn({
            let control = handle.control();
            async move { control.call_crew().await }
        });
        let sent = next_json(&mut server).await;
        assert_eq!(sent["t"], "call");
        send_json(
            &mut server,
            serde_json::json!({"t": "call", "call": {"id": "c1", "from": "100",
                "kind": "join", "at": 10, "until": 1210, "answers": {}}, "quiet": true, "now": 12}),
        )
        .await;
        send_json(
            &mut server,
            serde_json::json!({"t": "ack", "rid": sent["rid"]}),
        )
        .await;
        call.await.unwrap().unwrap();
        assert!(matches!(
            handle.next_event().await.unwrap(),
            PresenceEvent::Call { call, quiet: true, clock: Some(12) } if call.id == "c1"
        ));

        // An answer to a call that is gone, and one more call too soon.
        let answer = tokio::spawn({
            let control = handle.control();
            async move { control.answer_call("c0", CallAnswer::Soon).await }
        });
        let sent = next_json(&mut server).await;
        assert_eq!(sent["t"], "call_answer");
        assert_eq!(sent["call"], "c0");
        assert_eq!(sent["answer"], "soon");
        send_json(
            &mut server,
            serde_json::json!({"t": "error", "rid": sent["rid"], "code": "call_not_found"}),
        )
        .await;
        assert_eq!(
            answer.await.unwrap().unwrap_err().code(),
            Some(&ErrorCode::CallNotFound)
        );
        let again = tokio::spawn({
            let control = handle.control();
            async move { control.call_crew().await }
        });
        let sent = next_json(&mut server).await;
        send_json(
            &mut server,
            serde_json::json!({"t": "error", "rid": sent["rid"], "code": "rate_limited",
                "retryAfter": 240}),
        )
        .await;
        assert_eq!(
            again.await.unwrap().unwrap_err().retry_after(),
            Some(Duration::from_secs(240))
        );

        // Answers and ends arrive as events; an unknown answer is skipped.
        send_json(
            &mut server,
            serde_json::json!({"t": "call_answer", "call": "c1", "by": "7", "answer": "maybe"}),
        )
        .await;
        send_json(
            &mut server,
            serde_json::json!({"t": "call_answer", "call": "c1", "by": "7", "answer": "yes"}),
        )
        .await;
        assert_eq!(
            handle.next_event().await.unwrap(),
            PresenceEvent::CallAnswered {
                call: "c1".into(),
                by: "7".into(),
                answer: CallAnswer::Yes
            }
        );
        let end = tokio::spawn({
            let control = handle.control();
            async move { control.end_call("c1").await }
        });
        let sent = next_json(&mut server).await;
        assert_eq!(sent["t"], "call_end");
        assert_eq!(sent["call"], "c1");
        send_json(
            &mut server,
            serde_json::json!({"t": "call_end", "call": "c1", "reason": "ended"}),
        )
        .await;
        send_json(
            &mut server,
            serde_json::json!({"t": "ack", "rid": sent["rid"]}),
        )
        .await;
        end.await.unwrap().unwrap();
        assert_eq!(
            handle.next_event().await.unwrap(),
            PresenceEvent::CallEnded {
                call: "c1".into(),
                reason: CallEndReason::Ended
            }
        );

        // While the connection is down (a flood closes it for a minute), a call fails at once.
        server
            .close(Some(CloseFrame {
                code: CloseCode::from(4008),
                reason: "rate_limited".into(),
            }))
            .await
            .unwrap();
        assert!(matches!(
            handle.next_event().await.unwrap(),
            PresenceEvent::Disconnected { retry_in } if retry_in >= Duration::from_secs(60)
        ));
        let error = handle.call_crew().await.unwrap_err();
        assert!(matches!(error, SocialError::Unavailable { .. }));
        handle.shutdown().await;
    }
}
