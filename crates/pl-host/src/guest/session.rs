//! A guest's part of a world session (PROTOCOL.md §3.2, §6.5–§6.7, §8).
//!
//! [`GuestSession::join`] works through the service's order to a control stream and waits for the
//! host's manifest and state. The game then connects to the local endpoint and its connection is
//! relayed over a game stream. When a stream breaks, or the host, the service or a credential
//! renewal asks for a move, the session opens a new stream (on the other path, or the same path at
//! a new address) and resumes from the last byte each side received; the game notices a stall at
//! most. Everything the launcher has to show or send comes out as [`GuestEvent`]s.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::{mpsc, oneshot};
use tokio::time::{Instant, sleep_until, timeout_at};

use super::local::{AcceptFilter, GameHandshake, LocalEndpoint, refuse_game};
use crate::manifest::Manifest;
use crate::policy::Policy;
use crate::policy::env::{CredsReason, GuestPaths};
use crate::policy::guest::{
    AttemptError, Ctx, JoinFailure, Route, TurnHold, establish, join, merge_paths, open_stream,
    release,
};
use crate::report::{Event, MoveCause, Role, RttSinks, bucket_minutes};
use crate::transport::{BoxStream, PathKind};
use crate::tunnel::channel::{Channel, In};
use crate::tunnel::control::{
    ByeReason, Control, ControlError, Hello, MoveWhy, NameVerdict, Refusal,
};
use crate::tunnel::link::{self, LinkConfig, LinkEnd, LinkHandle, LinkState};
use crate::tunnel::{GuestId, Join, Paths, StreamId};

/// What a guest joins with.
#[derive(Debug, Clone)]
pub struct GuestConfig {
    pub join: Join,
    /// The in-game name the guest will log in with.
    pub name: String,
    /// A Microsoft account (the game authenticates with Mojang).
    pub premium: bool,
    /// This launcher's version (`hello.launcher`).
    pub launcher: String,
    pub policy: Policy,
}

/// How a guest's session ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuestEnd {
    /// The host closed its world: "Ayşe dünyayı kapattı."
    HostClosed,
    /// "Bu dünyadan çıkarıldın."
    Removed,
    /// The connection to the host was lost and could not be resumed: the row shows "Yeniden katıl"
    /// (the same invite joins again).
    Lost,
    /// The guest left (or had no game connection for 2 minutes).
    Left,
}

/// What the session tells the launcher.
#[derive(Debug, Clone)]
pub enum GuestEvent {
    /// The world's guest count or limit changed ("2/8" is `guests + 1` of `max + 1`).
    State { guests: u8, max: u8 },
    /// The game's connection is relayed to the host.
    GameConnected,
    /// The host refused the game's connection (§6.9); the game got a disconnect packet.
    GameRefused(Refusal),
    /// The game's connection ended.
    GameEnded,
    /// The session is over.
    Ended(GuestEnd),
    /// Telemetry for the presence socket.
    Report(Event),
}

/// The launcher's side of a guest session: [`GuestPaths`] and somewhere to put [`GuestEvent`]s.
pub trait GuestEnv: GuestPaths {
    fn event(&self, event: GuestEvent);
}

/// What the getters read.
#[derive(Debug, Clone)]
struct View {
    guest: GuestId,
    verdict: NameVerdict,
    manifest: Manifest,
    guests: u8,
    max: u8,
}

/// Localized refusal texts for the game's disconnect packet (the guest's language).
pub type RefusalText = Arc<dyn Fn(Refusal) -> String + Send + Sync>;

enum Cmd {
    Game {
        stream: BoxStream,
        handshake: GameHandshake,
        mc: String,
        text: Option<RefusalText>,
    },
    Rename {
        name: String,
        premium: bool,
        reply: oneshot::Sender<Option<NameVerdict>>,
    },
    WorldUpdate {
        guests: u8,
        max: u8,
        paths: Option<Paths>,
    },
    TurnMove(MoveWhy),
    HostGone(GuestEnd),
    Leave(oneshot::Sender<()>),
}

/// A guest's joined world. Dropping every clone leaves it.
#[derive(Clone)]
pub struct GuestSession {
    cmd: mpsc::Sender<Cmd>,
    view: Arc<Mutex<View>>,
    policy: Arc<Policy>,
}

impl std::fmt::Debug for GuestSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GuestSession")
            .field("guest", &self.guest_id())
            .finish_non_exhaustive()
    }
}

/// How long the host's manifest and state may take after `ok`.
const WELCOME_WAIT: Duration = Duration::from_secs(5);
/// The pause between rounds of resume attempts.
const RETRY_PAUSE: Duration = Duration::from_millis(500);

impl GuestSession {
    /// "Katıl": joins the world through the service's order (§5.7) and returns once the host's
    /// manifest and state came.
    pub async fn join(config: GuestConfig, env: Arc<dyn GuestEnv>) -> Result<Self, JoinFailure> {
        let rtt = RttSinks::default();
        let paths_env: &dyn GuestPaths = env.as_ref();
        let ctx = Ctx {
            env: paths_env,
            join: &config.join,
            policy: &config.policy,
            rtt: &rtt,
        };
        let report = |event| env.event(GuestEvent::Report(event));
        let mut paths = config.join.paths.clone();
        let hello = Hello::Control {
            name: config.name.clone(),
            premium: config.premium,
            launcher: config.launcher.clone(),
        };
        let joined = join(&ctx, hello, &mut paths, &report).await?;
        let mut channel = joined.channel;
        let (mut manifest, mut state) = (None, None);
        let deadline = Instant::now() + WELCOME_WAIT;
        while manifest.is_none() || state.is_none() {
            match timeout_at(deadline, channel.recv()).await {
                Ok(Some(In::Control(Ok(Control::Manifest(m))))) => manifest = Some(*m),
                Ok(Some(In::Control(Ok(Control::State(s))))) => state = Some(s),
                Ok(Some(In::Control(Ok(Control::Bye(reason))))) => {
                    let mut turn = joined.turn;
                    release(&ctx, &mut turn);
                    joined.route.path.close().await;
                    return Err(JoinFailure::Refused(match reason {
                        ByeReason::Removed => Refusal::Removed,
                        _ => Refusal::Closed,
                    }));
                }
                Ok(Some(In::Control(_))) => {}
                _ => {
                    let mut turn = joined.turn;
                    release(&ctx, &mut turn);
                    joined.route.path.close().await;
                    return Err(JoinFailure::NoPath);
                }
            }
        }
        let (Some(manifest), Some(state)) = (manifest, state) else {
            return Err(JoinFailure::NoPath);
        };
        merge_paths(&mut paths, state.paths.clone());
        let accept = joined.accept;
        let view = Arc::new(Mutex::new(View {
            guest: accept.guest.expect("checked when joining"),
            verdict: accept.name.unwrap_or(NameVerdict::Ok),
            manifest,
            guests: state.guests,
            max: state.max,
        }));
        let (cmd, cmds) = mpsc::channel(16);
        let policy = Arc::new(config.policy.clone());
        let driver = Driver {
            env: env.clone(),
            join: config.join,
            policy: config.policy,
            name: config.name,
            premium: config.premium,
            launcher: config.launcher,
            paths,
            turn: joined.turn,
            route: joined.route,
            control: Some(channel),
            game: None,
            cmds,
            starts: VecDeque::new(),
            linger_until: None,
            view: view.clone(),
            rtt,
            started: Instant::now(),
            moves: 0,
            rx_turn: 0,
            rx_e4mc: 0,
            usage_at: Instant::now(),
            usage_rx: 0,
            leave_reply: None,
        };
        env.event(GuestEvent::State {
            guests: state.guests,
            max: state.max,
        });
        tokio::spawn(driver.run());
        Ok(Self { cmd, view, policy })
    }

    pub fn guest_id(&self) -> GuestId {
        self.view.lock().expect("view").guest
    }

    /// The host's verdict on the name (§6.4): anything but `Ok` offers to switch accounts.
    pub fn name_verdict(&self) -> NameVerdict {
        self.view.lock().expect("view").verdict
    }

    /// The host's world manifest, for install matching.
    pub fn manifest(&self) -> Manifest {
        self.view.lock().expect("view").manifest.clone()
    }

    /// Guests in the world and the limit.
    pub fn world(&self) -> (u8, u8) {
        let view = self.view.lock().expect("view");
        (view.guests, view.max)
    }

    /// The guest switched accounts: a new control stream with the new name; the host's verdict.
    pub async fn rename(&self, name: String, premium: bool) -> Option<NameVerdict> {
        let (reply, answer) = oneshot::channel();
        self.cmd
            .send(Cmd::Rename {
                name,
                premium,
                reply,
            })
            .await
            .ok()?;
        answer.await.ok().flatten()
    }

    /// "Katıl" / "Yeniden katıl": opens the local endpoint (60 s) and returns its port for the
    /// game's quick play target. When the game connects its connection is relayed. `mc`: the
    /// install's Minecraft version; `text`: the refusal texts in the guest's language for the game's
    /// disconnect packet; `filter`: which connections belong to the game the launcher started.
    pub async fn open_local(
        &self,
        mc: String,
        text: Option<RefusalText>,
        filter: Option<AcceptFilter>,
    ) -> std::io::Result<u16> {
        let window = Duration::from_millis(self.policy.guest.local_accept_window_ms);
        let endpoint = LocalEndpoint::bind(window).await?;
        let port = endpoint.port();
        let cmd = self.cmd.clone();
        tokio::spawn(async move {
            if let Some((stream, handshake)) = endpoint.accept(filter).await {
                let stream: BoxStream = Box::new(stream);
                let _ = cmd
                    .send(Cmd::Game {
                        stream,
                        handshake,
                        mc,
                        text,
                    })
                    .await;
            }
        });
        Ok(port)
    }

    /// A game connection that did not come through the local endpoint (tests).
    #[cfg(test)]
    pub(crate) async fn game_connected(
        &self,
        stream: BoxStream,
        handshake: GameHandshake,
        mc: &str,
    ) {
        let cmd = Cmd::Game {
            stream,
            handshake,
            mc: mc.to_owned(),
            text: None,
        };
        let _ = self.cmd.send(cmd).await;
    }

    /// The service's `world_update` for this world.
    pub fn world_update(&self, guests: u8, max: u8, paths: Option<Paths>) {
        let _ = self.cmd.try_send(Cmd::WorldUpdate { guests, max, paths });
    }

    /// The service's `turn_move{reason}`: TURN streams move to e4mc and the credentials go.
    pub fn turn_move(&self, reason: &str) {
        let _ = self
            .cmd
            .try_send(Cmd::TurnMove(MoveWhy::from_turn_move(reason)));
    }

    /// The service's `world_closed` (`HostClosed`) or `world_revoked` (`Removed`).
    pub fn host_gone(&self, end: GuestEnd) {
        let _ = self.cmd.try_send(Cmd::HostGone(end));
    }

    /// The guest leaves: `bye{leave}`, streams and paths closed, credentials released.
    pub async fn leave(&self) {
        let (reply, done) = oneshot::channel();
        if self.cmd.send(Cmd::Leave(reply)).await.is_ok() {
            let _ = done.await;
        }
    }
}

struct Game {
    stream: StreamId,
    link: LinkHandle,
    kind: PathKind,
    seen: LinkState,
}

struct Driver {
    env: Arc<dyn GuestEnv>,
    join: Join,
    policy: Policy,
    name: String,
    premium: bool,
    launcher: String,
    paths: Paths,
    turn: Option<TurnHold>,
    route: Route,
    control: Option<Channel>,
    game: Option<Game>,
    cmds: mpsc::Receiver<Cmd>,
    starts: VecDeque<Instant>,
    linger_until: Option<Instant>,
    view: Arc<Mutex<View>>,
    rtt: RttSinks,
    started: Instant,
    moves: u32,
    rx_turn: u64,
    rx_e4mc: u64,
    usage_at: Instant,
    usage_rx: u64,
    leave_reply: Option<oneshot::Sender<()>>,
}

/// How a resume attempt went.
enum Resumed {
    Yes,
    /// Not on this route now; another try may work.
    No,
    /// The host does not know the stream any more.
    Forgotten,
    /// The host's world is over for this guest.
    Over(GuestEnd),
}

/// Whether the driver goes on.
enum Next {
    Go,
    End(GuestEnd),
}

async fn recv_control(control: &mut Option<Channel>) -> Option<In> {
    match control {
        Some(channel) => channel.recv().await,
        None => std::future::pending().await,
    }
}

async fn game_changed(game: &mut Option<Game>) -> LinkState {
    match game {
        Some(game) => game.link.changed_from(game.seen).await,
        None => std::future::pending().await,
    }
}

fn far() -> Instant {
    Instant::now() + Duration::from_secs(86_400 * 365)
}

impl Driver {
    fn ctx(&self) -> Ctx<'_> {
        Ctx {
            env: self.env.as_ref(),
            join: &self.join,
            policy: &self.policy,
            rtt: &self.rtt,
        }
    }

    fn report(&self, event: Event) {
        self.env.event(GuestEvent::Report(event));
    }

    fn control_hello(&self) -> Hello {
        Hello::Control {
            name: self.name.clone(),
            premium: self.premium,
            launcher: self.launcher.clone(),
        }
    }

    fn renew_at(&self) -> Option<Instant> {
        let hold = self.turn.as_ref()?;
        if self.route.kind != PathKind::Turn {
            return None;
        }
        let lead = Duration::from_secs(self.policy.turn.renew_when_left_s);
        Some(hold.since + hold.creds.ttl.saturating_sub(lead))
    }

    fn usage_due(&self) -> Option<Instant> {
        (self.route.kind == PathKind::Turn)
            .then(|| self.usage_at + Duration::from_millis(self.policy.report.usage_every_ms))
    }

    fn next_deadline(&self) -> Instant {
        [self.linger_until, self.renew_at(), self.usage_due()]
            .into_iter()
            .flatten()
            .min()
            .unwrap_or_else(far)
    }

    async fn run(mut self) {
        let end = loop {
            let deadline = self.next_deadline();
            let next = tokio::select! {
                // The host's word first: a `bye` beats a stream breaking at the same time.
                biased;
                item = recv_control(&mut self.control) => self.on_control(item).await,
                cmd = self.cmds.recv() => match cmd {
                    Some(cmd) => self.command(cmd).await,
                    // Every handle is gone: leave.
                    None => Next::End(GuestEnd::Left),
                },
                state = game_changed(&mut self.game) => self.on_game(state).await,
                () = sleep_until(deadline) => self.on_timer().await,
            };
            if let Next::End(end) = next {
                break end;
            }
        };
        self.finish(end).await;
    }

    /// The session is over: streams, paths and credentials go, the report and the end are told.
    async fn finish(mut self, end: GuestEnd) {
        if let Some(game) = self.game.take() {
            if end == GuestEnd::Left {
                // The host closes its side of the game connection at once.
                game.link.close(Duration::from_secs(2)).await;
            } else {
                game.link.abort().await;
            }
        }
        if let Some(control) = self.control.take() {
            if end == GuestEnd::Left {
                let _ = control.send(Control::Bye(ByeReason::Leave)).await;
            }
            control.finish().await;
        }
        self.close_route_usage();
        self.route.path.close().await;
        let mut turn = self.turn.take();
        release(&self.ctx(), &mut turn);
        let (rtt_turn, rtt_e4mc) = self.rtt.medians();
        self.report(Event::Session {
            role: Role::Guest,
            min: bucket_minutes(self.started.elapsed()),
            guests: 0,
            turn_rx: self.rx_turn,
            e4mc_rx: self.rx_e4mc,
            moves: self.moves,
            rtt_turn,
            rtt_e4mc,
        });
        tracing::info!(?end, "world session ended for this guest");
        self.env.event(GuestEvent::Ended(end));
        // Answer leaves once everything is closed.
        if let Some(reply) = self.leave_reply.take() {
            let _ = reply.send(());
        }
        self.cmds.close();
        while let Ok(cmd) = self.cmds.try_recv() {
            if let Cmd::Leave(reply) = cmd {
                let _ = reply.send(());
            }
        }
    }

    /// Counts the current route's relay bytes into the totals (and the TURN usage report).
    fn close_route_usage(&mut self) {
        let rx = self.route.path.rx_bytes();
        match self.route.kind {
            PathKind::Turn => {
                self.rx_turn += rx;
                self.report(Event::Usage {
                    session: self.join.session,
                    role: Role::Guest,
                    turn_rx: rx.saturating_sub(self.usage_rx),
                });
            }
            PathKind::E4mc => self.rx_e4mc += rx,
        }
        self.usage_rx = 0;
        self.usage_at = Instant::now();
    }

    async fn command(&mut self, cmd: Cmd) -> Next {
        match cmd {
            Cmd::Game {
                stream,
                handshake,
                mc,
                text,
            } => self.start_game(stream, handshake, mc, text).await,
            Cmd::Rename {
                name,
                premium,
                reply,
            } => {
                self.name = name;
                self.premium = premium;
                let verdict = self.reopen_control().await;
                let _ = reply.send(verdict);
                Next::Go
            }
            Cmd::WorldUpdate { guests, max, paths } => {
                if let Some(paths) = paths {
                    merge_paths(&mut self.paths, paths);
                }
                self.set_state(guests, max);
                Next::Go
            }
            Cmd::TurnMove(why) => {
                if self.route.kind == PathKind::Turn {
                    self.planned_move(PathKind::E4mc, why.into(), None).await;
                }
                // The credentials go either way; the service revokes them after the grace.
                if self.route.kind != PathKind::Turn {
                    let mut turn = self.turn.take();
                    release(&self.ctx(), &mut turn);
                }
                Next::Go
            }
            Cmd::HostGone(end) => Next::End(end),
            Cmd::Leave(reply) => {
                // Answered in `finish` once everything is closed.
                self.leave_reply = Some(reply);
                Next::End(GuestEnd::Left)
            }
        }
    }

    fn set_state(&mut self, guests: u8, max: u8) {
        {
            let mut view = self.view.lock().expect("view");
            view.guests = guests;
            view.max = max;
        }
        self.env.event(GuestEvent::State { guests, max });
    }

    async fn on_control(&mut self, item: Option<In>) -> Next {
        match item {
            Some(In::Control(Ok(message))) => match message {
                Control::State(state) => {
                    merge_paths(&mut self.paths, state.paths.clone());
                    self.set_state(state.guests, state.max);
                }
                Control::Manifest(manifest) => {
                    self.view.lock().expect("view").manifest = *manifest;
                }
                Control::Move { to, why: _ } => {
                    self.planned_move(to, MoveCause::Host, None).await;
                }
                Control::Bye(ByeReason::Removed) => return Next::End(GuestEnd::Removed),
                Control::Bye(_) => return Next::End(GuestEnd::HostClosed),
                _ => {}
            },
            // Messages this version does not know are skipped.
            Some(In::Control(Err(ControlError::Unknown(_)))) => {}
            Some(In::Control(Err(_)) | In::Data { .. } | In::Ack(_)) => {
                // The host broke the rules on this stream: it closes.
                self.control = None;
                return self.control_broken().await;
            }
            Some(In::Ended(_)) | None => {
                self.control = None;
                return self.control_broken().await;
            }
        }
        Next::Go
    }

    /// The control stream broke. A broken game stream resumes on its own (and brings a new
    /// control stream); otherwise a new control stream is opened, on another path if needed.
    async fn control_broken(&mut self) -> Next {
        let deadline = Instant::now() + Duration::from_millis(self.policy.guest.resume_try_ms);
        while Instant::now() < deadline {
            if self.game_broken() {
                // The game stream broke too: its resume brings a control stream.
                return Next::Go;
            }
            // With a game on the current route the control stream comes back on the same route
            // (the game's own stream decides when to move); without one, any path will do.
            let targets = if self.game.is_some() {
                vec![self.route.kind]
            } else {
                self.targets(None)
            };
            for kind in targets {
                let attempt = self.attempt_deadline(deadline);
                if let Some(route) = self.route_to(kind, attempt).await
                    && self.new_control(&route, attempt).await.is_some()
                {
                    self.switch_route(route);
                    return Next::Go;
                }
                if self.game_broken() {
                    return Next::Go;
                }
            }
            sleep_until((Instant::now() + RETRY_PAUSE).min(deadline)).await;
        }
        // Without a control stream and a game, there is nothing left to resume.
        if self.game.is_none() {
            Next::End(GuestEnd::Lost)
        } else {
            Next::Go
        }
    }

    async fn on_game(&mut self, state: LinkState) -> Next {
        let Some(game) = &mut self.game else {
            return Next::Go;
        };
        game.seen = state;
        match state {
            LinkState::Live => Next::Go,
            LinkState::Broken(at) => match self.resume(at).await {
                Some(end) => Next::End(end),
                None => Next::Go,
            },
            LinkState::Ended(end) => {
                let game = self.game.take().expect("checked above");
                if end == LinkEnd::Expired {
                    self.report(Event::moved(
                        game.kind,
                        game.kind,
                        MoveCause::Failure,
                        false,
                        Duration::from_millis(self.policy.guest.resume_try_ms),
                    ));
                }
                self.env.event(GuestEvent::GameEnded);
                self.linger_until = Some(
                    Instant::now() + Duration::from_millis(self.policy.guest.control_linger_ms),
                );
                if end == LinkEnd::Expired && self.control.is_none() {
                    return Next::End(GuestEnd::Lost);
                }
                Next::Go
            }
        }
    }

    async fn on_timer(&mut self) -> Next {
        let now = Instant::now();
        if self.linger_until.is_some_and(|t| now >= t) && self.game.is_none() {
            // Two minutes without a game stream: bye{leave}.
            return Next::End(GuestEnd::Left);
        }
        if self.usage_due().is_some_and(|t| now >= t) {
            let rx = self.route.path.rx_bytes();
            self.report(Event::Usage {
                session: self.join.session,
                role: Role::Guest,
                turn_rx: rx.saturating_sub(self.usage_rx),
            });
            self.usage_rx = rx;
            self.usage_at = now;
        }
        if self.renew_at().is_some_and(|t| now >= t) {
            self.renew().await;
        }
        Next::Go
    }

    fn game_broken(&self) -> bool {
        self.game
            .as_ref()
            .is_some_and(|g| matches!(g.link.state(), LinkState::Broken(_)))
    }

    /// One try within a longer window: at most a path attempt's time.
    fn attempt_deadline(&self, window: Instant) -> Instant {
        (Instant::now() + Duration::from_millis(self.policy.guest.path_attempt_ms)).min(window)
    }

    /// Waits until another stream may be opened (at most 5 a minute per session).
    async fn rate(&mut self, deadline: Instant) -> bool {
        let per_minute = self.policy.guest.new_streams_per_minute as usize;
        loop {
            let now = Instant::now();
            while self
                .starts
                .front()
                .is_some_and(|t| now.duration_since(*t) >= Duration::from_secs(60))
            {
                self.starts.pop_front();
            }
            if self.starts.len() < per_minute {
                self.starts.push_back(now);
                return true;
            }
            let free = self.starts[0] + Duration::from_secs(60);
            if free > deadline {
                return false;
            }
            sleep_until(free).await;
        }
    }

    /// The paths to try for a move: `to` when given, else the other path the host has, then the
    /// same path (at its current address).
    fn targets(&self, to: Option<PathKind>) -> Vec<PathKind> {
        if let Some(to) = to {
            return vec![to];
        }
        let other = match self.route.kind {
            PathKind::Turn => PathKind::E4mc,
            PathKind::E4mc => PathKind::Turn,
        };
        if self.paths.has(other) {
            vec![other, self.route.kind]
        } else {
            vec![self.route.kind]
        }
    }

    /// A route of `kind`: the current one when it still goes to the host's current address,
    /// otherwise a new one.
    async fn route_to(&mut self, kind: PathKind, deadline: Instant) -> Option<Route> {
        if kind == self.route.kind && self.paths.address(kind).as_ref() == Some(&self.route.address)
        {
            return Some(self.route.clone());
        }
        let mut paths = self.paths.clone();
        let mut turn = self.turn.take();
        let result = {
            let ctx = Ctx {
                env: self.env.as_ref(),
                join: &self.join,
                policy: &self.policy,
                rtt: &self.rtt,
            };
            establish(
                &ctx,
                kind,
                &mut turn,
                CredsReason::Fallback,
                &mut paths,
                deadline,
            )
            .await
        };
        self.turn = turn;
        self.paths = paths;
        result.ok()
    }

    /// Opens a control stream on `route`; on `ok` it replaces the current one. Returns the verdict.
    async fn new_control(&mut self, route: &Route, deadline: Instant) -> Option<NameVerdict> {
        if !self.rate(deadline).await {
            return None;
        }
        let hello = self.control_hello();
        let opened = open_stream(&self.ctx(), route, hello, deadline).await;
        match opened {
            Ok((channel, accept)) => {
                let verdict = accept.name.unwrap_or(NameVerdict::Ok);
                if let Some(old) = self.control.replace(channel) {
                    old.finish().await;
                }
                let mut view = self.view.lock().expect("view");
                view.verdict = verdict;
                if let Some(guest) = accept.guest {
                    view.guest = guest;
                }
                Some(verdict)
            }
            Err(_) => None,
        }
    }

    /// A new control stream on the current route (a new name, after "hesabı değiştir").
    async fn reopen_control(&mut self) -> Option<NameVerdict> {
        let deadline = Instant::now() + Duration::from_millis(self.policy.guest.path_attempt_ms);
        let route = self.route.clone();
        self.new_control(&route, deadline).await
    }

    /// Makes `route` the current one, closing the old route when it is a different path.
    fn switch_route(&mut self, route: Route) {
        if Arc::ptr_eq(&route.path, &self.route.path) {
            return;
        }
        self.close_route_usage();
        let old = std::mem::replace(&mut self.route, route);
        tokio::spawn(async move { old.path.close().await });
    }

    /// Opens a resume stream on `route` for the game link; on `ok` the link continues on it.
    async fn resume_on(&mut self, route: &Route, deadline: Instant) -> Resumed {
        let Some(game) = &self.game else {
            return Resumed::No;
        };
        let (stream, link) = (game.stream, game.link.clone());
        if !self.rate(deadline).await {
            return Resumed::No;
        }
        let hello = Hello::Resume {
            stream,
            recv: link.received(),
        };
        match open_stream(&self.ctx(), route, hello, deadline).await {
            Ok((channel, accept)) => {
                let Some(recv) = accept.recv else {
                    return Resumed::No;
                };
                let attached = link.attach(channel, recv, None).await.is_ok();
                if let (true, Some(game)) = (attached, &mut self.game) {
                    game.kind = route.kind;
                }
                if attached { Resumed::Yes } else { Resumed::No }
            }
            Err(AttemptError::Refused(Refusal::Resume)) => Resumed::Forgotten,
            Err(AttemptError::Refused(Refusal::Removed)) => Resumed::Over(GuestEnd::Removed),
            Err(AttemptError::Refused(Refusal::Closed)) => Resumed::Over(GuestEnd::HostClosed),
            Err(_) => Resumed::No,
        }
    }

    /// A broken game stream (§6.7, "Guest"): the other path, else the same one, for 12 s.
    async fn resume(&mut self, broken_at: Instant) -> Option<GuestEnd> {
        let deadline = broken_at + Duration::from_millis(self.policy.guest.resume_try_ms);
        let from = self.game.as_ref().map_or(self.route.kind, |g| g.kind);
        while Instant::now() < deadline {
            for kind in self.targets(None) {
                if let Some(end) = self.bye_waiting() {
                    return Some(end);
                }
                let attempt = self.attempt_deadline(deadline);
                let Some(route) = self.route_to(kind, attempt).await else {
                    continue;
                };
                match self.resume_on(&route, attempt).await {
                    Resumed::Yes => {
                        self.moved(from, route.kind, MoveCause::Failure, broken_at);
                        self.after_move(route, deadline).await;
                        return None;
                    }
                    Resumed::No => {}
                    Resumed::Forgotten => {
                        // The host no longer has the stream: the game connection is over.
                        self.report(Event::moved(
                            from,
                            kind,
                            MoveCause::Failure,
                            false,
                            broken_at.elapsed(),
                        ));
                        if let Some(game) = &self.game {
                            game.link.abort().await;
                        }
                        return None;
                    }
                    Resumed::Over(end) => return Some(end),
                }
            }
            let pause = (Instant::now() + RETRY_PAUSE).min(deadline);
            sleep_until(pause).await;
        }
        // The link gives up at the same time and reports through `on_game`.
        None
    }

    /// Reads what the control stream already brought without waiting: the host's `bye` ends the
    /// session (a resume would only be refused), the world's state is noted.
    fn bye_waiting(&mut self) -> Option<GuestEnd> {
        loop {
            let channel = self.control.as_mut()?;
            let item = match channel.rx.try_recv() {
                Ok(item) => item,
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => return None,
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                    self.control = None;
                    return None;
                }
            };
            match item {
                In::Control(Ok(Control::Bye(ByeReason::Removed))) => {
                    return Some(GuestEnd::Removed);
                }
                In::Control(Ok(Control::Bye(_))) => return Some(GuestEnd::HostClosed),
                In::Control(Ok(Control::State(state))) => {
                    merge_paths(&mut self.paths, state.paths.clone());
                    self.set_state(state.guests, state.max);
                }
                In::Control(Ok(Control::Manifest(manifest))) => {
                    self.view.lock().expect("view").manifest = *manifest;
                }
                In::Control(_) => {}
                In::Data { .. } | In::Ack(_) | In::Ended(_) => {
                    self.control = None;
                    return None;
                }
            }
        }
    }

    /// A planned move (`move`, `turn_move`, renewal): the new stream opens while the old one still
    /// works, and the link switches on `ok`.
    async fn planned_move(&mut self, to: PathKind, cause: MoveCause, renewed: Option<TurnHold>) {
        let start = Instant::now();
        let deadline = start + Duration::from_millis(self.policy.guest.resume_try_ms);
        let from = self.game.as_ref().map_or(self.route.kind, |g| g.kind);
        let old_turn = match renewed {
            Some(hold) => self.turn.replace(hold),
            None => None,
        };
        let route = if old_turn.is_some() {
            // A renewal: a new allocation even though the address may be the same.
            let mut paths = self.paths.clone();
            let mut turn = self.turn.take();
            let result = {
                let ctx = Ctx {
                    env: self.env.as_ref(),
                    join: &self.join,
                    policy: &self.policy,
                    rtt: &self.rtt,
                };
                establish(
                    &ctx,
                    PathKind::Turn,
                    &mut turn,
                    CredsReason::Renew,
                    &mut paths,
                    deadline,
                )
                .await
            };
            self.turn = turn;
            self.paths = paths;
            result.ok()
        } else {
            self.route_to(to, deadline).await
        };
        let Some(route) = route else {
            // Nowhere to go: the old stream carries on while it can.
            if let Some(old) = old_turn {
                // The renewal's credentials are not used; the old ones stay.
                let mut new = self.turn.replace(old);
                release(&self.ctx(), &mut new);
            }
            return;
        };
        if Arc::ptr_eq(&route.path, &self.route.path) {
            return;
        }
        let moved_game =
            self.game.is_some() && matches!(self.resume_on(&route, deadline).await, Resumed::Yes);
        if moved_game {
            self.moved(from, route.kind, cause, start);
        }
        self.after_move(route, deadline).await;
        if let Some(old) = old_turn {
            self.env.turn_release(old.creds);
        }
    }

    /// After a game stream moved: the control stream follows to the same route.
    async fn after_move(&mut self, route: Route, deadline: Instant) {
        let deadline =
            deadline.max(Instant::now() + Duration::from_millis(self.policy.guest.path_attempt_ms));
        self.new_control(&route, deadline).await;
        self.switch_route(route);
    }

    fn moved(&mut self, from: PathKind, to: PathKind, cause: MoveCause, since: Instant) {
        self.moves += 1;
        self.report(Event::moved(from, to, cause, true, since.elapsed()));
    }

    /// TURN credential renewal (§5.5): new credentials, a new allocation, the streams moved to it,
    /// the old credentials released. Refused: the streams move to e4mc.
    async fn renew(&mut self) {
        let answer = self.env.turn_creds(CredsReason::Renew).await;
        match answer.ok().and_then(|a| a.creds) {
            Some(creds) => {
                let hold = TurnHold {
                    creds,
                    allocation: None,
                    since: Instant::now(),
                };
                self.planned_move(PathKind::Turn, MoveCause::Renew, Some(hold))
                    .await;
            }
            None => {
                self.planned_move(PathKind::E4mc, MoveCause::Renew, None)
                    .await;
                if self.route.kind != PathKind::Turn {
                    let mut turn = self.turn.take();
                    release(&self.ctx(), &mut turn);
                } else if let Some(hold) = &mut self.turn {
                    // Nothing else works: keep going until the credentials end.
                    hold.since = Instant::now();
                }
            }
        }
    }

    /// The game connected to the local endpoint: a game stream, then the relay.
    async fn start_game(
        &mut self,
        stream: BoxStream,
        handshake: GameHandshake,
        mc: String,
        text: Option<RefusalText>,
    ) -> Next {
        if self.game.is_some() {
            // One game connection at a time.
            return Next::Go;
        }
        let deadline = Instant::now() + Duration::from_millis(self.policy.guest.path_attempt_ms);
        let hello = Hello::Game {
            name: self.name.clone(),
            premium: self.premium,
            mc,
            proto: handshake.proto,
        };
        let route = self.route.clone();
        let opened = if self.rate(deadline).await {
            open_stream(&self.ctx(), &route, hello, deadline).await
        } else {
            Err(AttemptError::Refused(Refusal::Busy))
        };
        match opened {
            Ok((channel, accept)) => {
                let link = link::spawn(
                    stream,
                    handshake.kept,
                    channel,
                    LinkConfig::guest(&self.policy),
                );
                self.game = Some(Game {
                    stream: accept.stream,
                    link,
                    kind: route.kind,
                    seen: LinkState::Live,
                });
                self.linger_until = None;
                self.env.event(GuestEvent::GameConnected);
                Next::Go
            }
            Err(error) => {
                let reason = match error {
                    AttemptError::Refused(reason) => reason,
                    AttemptError::Failed { .. } => Refusal::Busy,
                };
                if let Some(text) = text {
                    refuse_game(stream, &text(reason)).await;
                }
                self.env.event(GuestEvent::GameRefused(reason));
                match reason {
                    Refusal::Removed => Next::End(GuestEnd::Removed),
                    Refusal::Closed => Next::End(GuestEnd::HostClosed),
                    _ => Next::Go,
                }
            }
        }
    }
}
