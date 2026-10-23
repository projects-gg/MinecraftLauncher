//! A guest working through the service's `order` (PROTOCOL.md §3.2, §5.7;
//! `testdata/policy/guest-connect.json`).
//!
//! One path attempt has 4 s, from its first step to `ok` on the control stream. TURN: (fallback
//! credentials) → allocation → `world_peer` → `world_peer_ok` → QUIC → PLJ1. e4mc: (`world_peer`
//! when the guest knows no domain) → TCP → PLJ1. A failed step moves to the next path; a PLJ1 `no`
//! is final. Each attempt is one telemetry event.

use std::borrow::Cow;
use std::sync::Arc;
use std::time::Duration;

use tokio::time::{Instant, timeout_at};

use super::Policy;
use super::env::{Creds, CredsError, CredsReason, GuestPaths, TurnAllocation};
use crate::report::{Event, Role, RttSinks, step_belongs};
use crate::transport::{GuestPath, PathAddress, PathError, PathKind, Step};
use crate::tunnel::channel::{Channel, In, Keepalive};
use crate::tunnel::control::{Accept, Control, Hello, Refusal};
use crate::tunnel::handshake;
use crate::tunnel::{Join, Paths};

/// Why joining did not work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinFailure {
    /// No path worked, or none was offered: "Bağlanılamadı. Tekrar dene."
    NoPath,
    /// The host refused: show its reason's text (§6.9).
    Refused(Refusal),
}

/// How one attempt (or one step of a move) ended without a stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AttemptError {
    Failed { step: Step, code: Cow<'static, str> },
    Refused(Refusal),
}

fn failed(step: Step, code: &'static str) -> AttemptError {
    AttemptError::Failed {
        step,
        code: Cow::Borrowed(code),
    }
}

fn from_path(kind: PathKind, fallback: Step, error: PathError) -> AttemptError {
    let step = if step_belongs(kind, error.step) {
        error.step
    } else {
        fallback
    };
    AttemptError::Failed {
        step,
        code: error.code,
    }
}

/// The guest's TURN credentials and the allocation made with them.
pub(crate) struct TurnHold {
    pub creds: Creds,
    pub allocation: Option<Arc<dyn TurnAllocation>>,
    /// When the credentials came (renewal is due `ttl − 20 min` later).
    pub since: Instant,
}

/// A usable path to the host.
#[derive(Clone)]
pub(crate) struct Route {
    pub kind: PathKind,
    pub path: Arc<dyn GuestPath>,
    /// The host's address it goes to.
    pub address: PathAddress,
    /// TURN worked only over TCP or TLS.
    pub tcp_only: bool,
}

/// What every step needs.
pub(crate) struct Ctx<'a> {
    pub env: &'a dyn GuestPaths,
    pub join: &'a Join,
    pub policy: &'a Policy,
    pub rtt: &'a RttSinks,
}

impl Ctx<'_> {
    pub fn keepalive(&self) -> Keepalive {
        Keepalive::from_policy(self.policy)
    }

    pub fn attempt_time(&self) -> Duration {
        Duration::from_millis(self.policy.guest.path_attempt_ms)
    }
}

/// Makes a path of `kind` usable before `deadline`. TURN reuses `turn` (credentials and
/// allocation) or asks for credentials with `reason`; the caller releases them when it gives up.
pub(crate) async fn establish(
    ctx: &Ctx<'_>,
    kind: PathKind,
    turn: &mut Option<TurnHold>,
    reason: CredsReason,
    paths: &mut Paths,
    deadline: Instant,
) -> Result<Route, AttemptError> {
    match kind {
        PathKind::Turn => establish_turn(ctx, turn, reason, paths, deadline).await,
        PathKind::E4mc => establish_e4mc(ctx, paths, deadline).await,
    }
}

async fn establish_turn(
    ctx: &Ctx<'_>,
    turn: &mut Option<TurnHold>,
    reason: CredsReason,
    paths: &mut Paths,
    deadline: Instant,
) -> Result<Route, AttemptError> {
    if turn.is_none() {
        let creds = match timeout_at(deadline, ctx.env.turn_creds(reason)).await {
            Ok(Ok(answer)) => answer
                .creds
                .ok_or_else(|| failed(Step::Creds, "turn_creds_error"))?,
            Ok(Err(error)) => {
                return Err(AttemptError::Failed {
                    step: Step::Creds,
                    code: error.code(),
                });
            }
            Err(_) => return Err(failed(Step::Creds, "turn_creds_error")),
        };
        *turn = Some(TurnHold {
            creds,
            allocation: None,
            since: Instant::now(),
        });
    }
    let hold = turn.as_mut().expect("credentials are held");
    let allocation = match &hold.allocation {
        Some(allocation) => allocation.clone(),
        None => match timeout_at(deadline, ctx.env.turn_allocate(hold.creds)).await {
            Ok(Ok(allocation)) => {
                hold.allocation = Some(allocation.clone());
                allocation
            }
            Ok(Err(error)) => return Err(from_path(PathKind::Turn, Step::Alloc, error)),
            Err(_) => return Err(failed(Step::Alloc, "turn_alloc_timeout")),
        },
    };
    // The host permits this allocation's relay address; its answer has its current paths.
    match timeout_at(
        deadline,
        ctx.env.world_peer(PathKind::Turn, Some(allocation.relay())),
    )
    .await
    {
        Ok(Some(answer)) => merge_paths(paths, answer),
        _ => return Err(failed(Step::Peer, "turn_peer_timeout")),
    }
    let Some(host) = paths.turn else {
        return Err(failed(Step::Peer, "turn_peer_timeout"));
    };
    let path = match timeout_at(deadline, allocation.connect(host.relay, host.pin)).await {
        Ok(Ok(path)) => path,
        Ok(Err(error)) => return Err(from_path(PathKind::Turn, Step::Quic, error)),
        Err(_) => return Err(failed(Step::Quic, "turn_quic_timeout")),
    };
    Ok(Route {
        kind: PathKind::Turn,
        path,
        address: PathAddress::Turn {
            relay: host.relay,
            pin: host.pin,
        },
        tcp_only: allocation.tcp_only(),
    })
}

async fn establish_e4mc(
    ctx: &Ctx<'_>,
    paths: &mut Paths,
    deadline: Instant,
) -> Result<Route, AttemptError> {
    if paths.e4mc.is_none() {
        // The host has not opened e4mc (or the guest lost its domain): ask for it.
        match timeout_at(deadline, ctx.env.world_peer(PathKind::E4mc, None)).await {
            Ok(Some(answer)) => merge_paths(paths, answer),
            _ => return Err(failed(Step::Tcp, "e4mc_unknown_domain")),
        }
    }
    let Some(domain) = paths.e4mc.clone() else {
        return Err(failed(Step::Tcp, "e4mc_unknown_domain"));
    };
    match timeout_at(deadline, ctx.env.e4mc_connect(&domain, ctx.join.proto)).await {
        Ok(Ok(path)) => Ok(Route {
            kind: PathKind::E4mc,
            path,
            address: PathAddress::E4mc { domain },
            tcp_only: false,
        }),
        Ok(Err(error)) => Err(from_path(PathKind::E4mc, Step::Tcp, error)),
        Err(_) => Err(failed(Step::Tcp, "e4mc_tcp_timeout")),
    }
}

/// A `world_peer_ok`, `state` or `world_update` brings the host's current paths: they replace what
/// the guest knew (addresses a guest must not use are refused when parsed).
pub(crate) fn merge_paths(paths: &mut Paths, answer: Paths) {
    if answer.is_valid() {
        *paths = answer;
    }
}

/// Opens a PLJ1 stream on `route`, sends `hello` and waits for the host's answer.
pub(crate) async fn open_stream(
    ctx: &Ctx<'_>,
    route: &Route,
    hello: Hello,
    deadline: Instant,
) -> Result<(Channel, Accept), AttemptError> {
    let (open_step, open_timeout) = match route.kind {
        PathKind::Turn => (Step::Quic, "turn_quic_timeout"),
        PathKind::E4mc => (Step::Tcp, "e4mc_tcp_timeout"),
    };
    let mut stream = match timeout_at(deadline, route.path.open()).await {
        Ok(Ok(stream)) => stream,
        Ok(Err(error)) => return Err(from_path(route.kind, open_step, error)),
        Err(_) => return Err(failed(open_step, open_timeout)),
    };
    let opening = handshake::open(&mut stream, ctx.join, route.path.as_ref());
    let cipher = match timeout_at(deadline, opening).await {
        Ok(Ok(cipher)) => cipher,
        Ok(Err(failure)) => {
            return Err(AttemptError::Failed {
                step: Step::Plj,
                code: failure.code(),
            });
        }
        Err(_) => return Err(failed(Step::Plj, "plj_timeout")),
    };
    let mut channel = Channel::spawn(
        stream,
        cipher,
        ctx.keepalive(),
        Some(ctx.rtt.for_kind(route.kind)),
    );
    if !channel.send(Control::Hello(hello)).await {
        return Err(failed(Step::Plj, "plj_error"));
    }
    loop {
        match timeout_at(deadline, channel.recv()).await {
            Ok(Some(In::Control(Ok(Control::Ok(accept))))) => return Ok((channel, accept)),
            Ok(Some(In::Control(Ok(Control::No(reason))))) => {
                return Err(AttemptError::Refused(reason));
            }
            // Messages this version does not know are skipped.
            Ok(Some(In::Control(Err(crate::tunnel::ControlError::Unknown(_))))) => {}
            Ok(_) => return Err(failed(Step::Plj, "plj_error")),
            Err(_) => return Err(failed(Step::Plj, "plj_timeout")),
        }
    }
}

/// The control stream joining opened.
pub(crate) struct Joined {
    pub route: Route,
    pub turn: Option<TurnHold>,
    pub channel: Channel,
    pub accept: Accept,
}

/// Joins: asks `turn_creds{primary}` for the order, then tries each path of it (§5.7). `hello` is
/// the control stream's `hello`. `paths` starts as the invite's and follows the host's answers.
pub(crate) async fn join(
    ctx: &Ctx<'_>,
    hello: Hello,
    paths: &mut Paths,
    report: &(dyn Fn(Event) + Send + Sync),
) -> Result<Joined, JoinFailure> {
    let (order, creds) = match ctx.env.turn_creds(CredsReason::Primary).await {
        Ok(answer) => (answer.order, answer.creds),
        Err(CredsError::Gone) => return Err(JoinFailure::NoPath),
        Err(error) => {
            // TURN is out of reach right now; the relay may still work.
            report_attempt(
                report,
                PathKind::Turn,
                Step::Creds,
                error.code(),
                Duration::ZERO,
            );
            (vec![PathKind::E4mc], None)
        }
    };
    let mut turn = creds.map(|creds| TurnHold {
        creds,
        allocation: None,
        since: Instant::now(),
    });
    let mut tried = Vec::new();
    for kind in order {
        if tried.contains(&kind) {
            continue;
        }
        tried.push(kind);
        let start = Instant::now();
        let deadline = start + ctx.attempt_time();
        let established =
            establish(ctx, kind, &mut turn, CredsReason::Fallback, paths, deadline).await;
        let outcome = match established {
            Ok(route) => match open_stream(ctx, &route, hello.clone(), deadline).await {
                Ok((channel, accept)) if accept.guest.is_some() && accept.name.is_some() => {
                    Ok((route, channel, accept))
                }
                Ok(_) => Err((Some(route), failed(Step::Plj, "plj_error"))),
                Err(error) => Err((Some(route), error)),
            },
            Err(error) => Err((None, error)),
        };
        match outcome {
            Ok((route, channel, accept)) => {
                let code = if route.tcp_only {
                    "turn_tcp_only"
                } else {
                    "ok"
                };
                report_attempt(report, kind, Step::Plj, code.into(), start.elapsed());
                if kind != PathKind::Turn {
                    release(ctx, &mut turn);
                }
                return Ok(Joined {
                    route,
                    turn,
                    channel,
                    accept,
                });
            }
            Err((route, error)) => {
                if let Some(route) = route {
                    route.path.close().await;
                }
                if kind == PathKind::Turn {
                    release(ctx, &mut turn);
                }
                match error {
                    AttemptError::Refused(reason) => {
                        report_attempt(
                            report,
                            kind,
                            Step::Plj,
                            reason.code().into(),
                            start.elapsed(),
                        );
                        return Err(JoinFailure::Refused(reason));
                    }
                    AttemptError::Failed { step, code } => {
                        report_attempt(report, kind, step, code, start.elapsed());
                    }
                }
            }
        }
    }
    release(ctx, &mut turn);
    Err(JoinFailure::NoPath)
}

/// Releases TURN credentials (and drops their allocation).
pub(crate) fn release(ctx: &Ctx<'_>, turn: &mut Option<TurnHold>) {
    if let Some(hold) = turn.take() {
        ctx.env.turn_release(hold.creds);
    }
}

pub(crate) fn report_attempt(
    report: &(dyn Fn(Event) + Send + Sync),
    kind: PathKind,
    step: Step,
    code: Cow<'static, str>,
    elapsed: Duration,
) {
    if let Some(event) = Event::attempt(kind, Role::Guest, step, code, elapsed, None) {
        report(event);
    }
}
