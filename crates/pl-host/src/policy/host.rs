//! A host preparing a path (PROTOCOL.md §3.1 step 5, §5.7; `testdata/policy/host-open.json`).
//!
//! The answer to `turn_creds{role:"host", reason:"primary"}` names the order. The host prepares
//! the first path of it and, if that fails, the next one; TURN as a second choice asks for
//! `fallback` credentials first. Unused credentials are released at once. Each path tried is one
//! telemetry event; a broker failure the e4mc fallback relays made up for is its own event before.

use std::borrow::Cow;
use std::sync::Arc;
use std::time::Duration;

use tokio::time::{Instant, timeout};

use super::env::{Creds, CredsReason, HostPaths};
use crate::report::{Event, Role, step_belongs};
use crate::transport::{HostPath, PathKind, Step};

/// The longest a host waits for one path to open. The paths have their own, shorter timeouts
/// (broker 1.5 s, QUIC 2.5 s, domain 2 s; TURN's allocation); this only keeps a stuck one from
/// holding the session.
pub const HOST_PATH_LIMIT: Duration = Duration::from_secs(15);

/// A path the host opened, with the TURN credentials it runs on.
pub(crate) struct OpenedPath {
    pub path: Arc<dyn HostPath>,
    pub creds: Option<Creds>,
}

/// Opens a path of `kind`. TURN uses `creds` or asks for credentials with `reason`; credentials
/// that did not get an allocation are released.
pub(crate) async fn open_path(
    env: &dyn HostPaths,
    kind: PathKind,
    creds: Option<Creds>,
    reason: CredsReason,
    report: &(dyn Fn(Event) + Send + Sync),
) -> Option<OpenedPath> {
    let start = Instant::now();
    match kind {
        PathKind::Turn => {
            let creds = match creds {
                Some(creds) => creds,
                None => match timeout(HOST_PATH_LIMIT, env.turn_creds(reason)).await {
                    Ok(Ok(answer)) => match answer.creds {
                        Some(creds) => creds,
                        None => {
                            emit(report, kind, Step::Creds, "turn_creds_error".into(), start);
                            return None;
                        }
                    },
                    Ok(Err(error)) => {
                        emit(report, kind, Step::Creds, error.code(), start);
                        return None;
                    }
                    Err(_) => {
                        emit(report, kind, Step::Creds, "turn_creds_error".into(), start);
                        return None;
                    }
                },
            };
            match timeout(HOST_PATH_LIMIT, env.turn_open(creds)).await {
                Ok(Ok(path)) => {
                    emit(report, kind, Step::Alloc, "ok".into(), start);
                    Some(OpenedPath {
                        path,
                        creds: Some(creds),
                    })
                }
                Ok(Err(error)) => {
                    let step = if step_belongs(kind, error.step) {
                        error.step
                    } else {
                        Step::Alloc
                    };
                    emit(report, kind, step, error.code, start);
                    env.turn_release(creds);
                    None
                }
                Err(_) => {
                    emit(
                        report,
                        kind,
                        Step::Alloc,
                        "turn_alloc_timeout".into(),
                        start,
                    );
                    env.turn_release(creds);
                    None
                }
            }
        }
        PathKind::E4mc => {
            if let Some(creds) = creds {
                env.turn_release(creds);
            }
            let Ok(opened) = timeout(HOST_PATH_LIMIT, env.e4mc_open()).await else {
                emit(
                    report,
                    kind,
                    Step::Domain,
                    "e4mc_domain_timeout".into(),
                    start,
                );
                return None;
            };
            if let Some(code) = opened.broker_failure {
                emit(report, kind, Step::Broker, code, start);
            }
            match opened.result {
                Ok(path) => {
                    let event = Event::attempt(
                        kind,
                        Role::Host,
                        Step::Domain,
                        "ok",
                        start.elapsed(),
                        opened.region.as_deref(),
                    )
                    .or_else(|| {
                        Event::attempt(kind, Role::Host, Step::Domain, "ok", start.elapsed(), None)
                    });
                    if let Some(event) = event {
                        report(event);
                    }
                    Some(OpenedPath { path, creds: None })
                }
                Err(error) => {
                    let step = if step_belongs(kind, error.step) {
                        error.step
                    } else {
                        Step::Domain
                    };
                    emit(report, kind, step, error.code, start);
                    None
                }
            }
        }
    }
}

/// Prepares the first path of `order` that opens (§3.1 step 5). `creds`: the primary answer's.
pub(crate) async fn open_first(
    env: &dyn HostPaths,
    order: &[PathKind],
    mut creds: Option<Creds>,
    report: &(dyn Fn(Event) + Send + Sync),
) -> Option<(PathKind, OpenedPath)> {
    let mut tried = Vec::new();
    for &kind in order {
        if tried.contains(&kind) {
            continue;
        }
        tried.push(kind);
        let given = if kind == PathKind::Turn {
            creds.take()
        } else {
            None
        };
        if let Some(opened) = open_path(env, kind, given, CredsReason::Fallback, report).await {
            if let Some(unused) = creds.take() {
                env.turn_release(unused);
            }
            return Some((kind, opened));
        }
    }
    if let Some(unused) = creds {
        env.turn_release(unused);
    }
    None
}

fn emit(
    report: &(dyn Fn(Event) + Send + Sync),
    kind: PathKind,
    step: Step,
    code: Cow<'static, str>,
    start: Instant,
) {
    if let Some(event) = Event::attempt(kind, Role::Host, step, code, start.elapsed(), None) {
        report(event);
    }
}
