//! The Microsoft session refreshed before the player presses OYNA (LIGHT). A launch refreshes the
//! Minecraft token only when it expires within 10 minutes, and then waits for Microsoft's five
//! requests in a row: 2.5–2.9 s of the player's first game of the day. So at the moments the
//! player is about to play (the launcher's start, its window coming back, another account chosen)
//! a token that expires within [`pl_auth::WARM_MARGIN`] is refreshed in the background, once per
//! moment: no timer, no polling. What is known of each token's expiry stays in memory, so a window
//! coming back with nothing due costs no keychain read. A launch whose token is good past its own
//! margin (10 minutes) never waits for a warm-up (pl-auth takes the refresh lock only when a
//! refresh is due); one that must refresh waits for a warm-up that is doing it, unless the
//! warm-up has stalled (past [`STALLED_AFTER`]): the launch stops it and refreshes itself. A
//! warm-up stops on its own after [`WARM_TIMEOUT`]. Without a network it stays
//! quiet; the next moment tries again, and the launch decides as before. Any other failure (a
//! refresh token Microsoft no longer takes) is left to the launch, with the token it still has:
//! the warm-up neither marks the account nor asks again this run.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use jiff::Timestamp;
use pl_auth::WarmSession;
use pl_net::CancellationToken;
use tauri::{AppHandle, Manager};
use uuid::Uuid;

use crate::state::AppState;

/// When each account's Minecraft token expires, as last seen; `None`: the warm-up failed for it
/// (other than offline) and leaves it to the launch for this run.
static EXPIRY: Mutex<Option<HashMap<Uuid, Option<Timestamp>>>> = Mutex::new(None);
/// A warm-up is on its way.
static RUNNING: AtomicBool = AtomicBool::new(false);
/// The running warm-up: its account, its stop and when it started.
static CURRENT: Mutex<Option<(Uuid, CancellationToken, Instant)>> = Mutex::new(None);

/// A warm-up that has not finished by then stops: the network stalls (each of the five requests
/// may wait 30 s), and the launch decides as before.
const WARM_TIMEOUT: Duration = Duration::from_secs(20);
/// A healthy refresh takes 2.5–2.9 s; a launch stops a warm-up that has run longer than this.
const STALLED_AFTER: Duration = Duration::from_secs(4);

/// Refreshes the selected Microsoft account's session in the background when its token expires
/// within the margin (or its expiry is not known yet). Cheap when nothing is due: no IO.
pub fn warm(app: &AppHandle, why: &'static str) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let Some(account) = state.auth.active_account() else {
        return;
    };
    // An account that needs a new sign-in is the launch's to tell ("Yeniden Giriş Yap").
    if !account.is_microsoft() || account.needs_relogin || !due(account.id, Timestamp::now()) {
        return;
    }
    if RUNNING.swap(true, Ordering::AcqRel) {
        return;
    }
    let app = app.clone();
    let stop = CancellationToken::new();
    let started = Instant::now();
    if let Ok(mut current) = CURRENT.lock() {
        *current = Some((account.id, stop.clone(), started));
    }
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let result = tokio::select! {
            result = state.auth.warm_session(account.id, pl_auth::WARM_MARGIN, &stop) => result,
            () = tokio::time::sleep(WARM_TIMEOUT) => {
                stop.cancel();
                Err(pl_auth::AuthError::Cancelled)
            }
        };
        if let Ok(mut current) = CURRENT.lock() {
            *current = None;
        }
        RUNNING.store(false, Ordering::Release);
        let elapsed_ms = started.elapsed().as_millis() as u64;
        match result {
            Ok(warmed) => {
                remember(account.id, Some(warmed.expires_at()));
                if matches!(warmed, WarmSession::Refreshed { .. }) {
                    tracing::info!(why, elapsed_ms, "session refreshed ahead of play");
                }
            }
            Err(error) => {
                // Stopped (by a launch, or its own time ran out) or offline: the next moment tries
                // again; any other failure is left to the launch for this run.
                if !error.is_cancelled() && error.class() != pl_auth::FailureClass::Offline {
                    remember(account.id, None);
                }
                tracing::debug!(
                    why,
                    elapsed_ms,
                    code = %error.code(),
                    class = ?error.class(),
                    "session not refreshed ahead of play"
                );
            }
        }
    });
}

/// A launch of `account` checks its session now: a warm-up of it that has stalled stops, so the
/// launch refreshes itself instead of waiting (a younger one is the very refresh the launch would
/// make, and pl-auth's lock lets the launch wait for it).
pub fn launch_starts(account: Uuid) {
    let Ok(current) = CURRENT.lock() else {
        return;
    };
    if let Some((id, stop, started)) = current.as_ref()
        && *id == account
        && started.elapsed() > STALLED_AFTER
    {
        tracing::debug!(
            ran_ms = started.elapsed().as_millis() as u64,
            "a launch stops a stalled session warm-up"
        );
        stop.cancel();
    }
}

/// Whether the account's token may expire within the margin: unknown counts as due, a failed
/// warm-up never.
fn due(id: Uuid, now: Timestamp) -> bool {
    let known = EXPIRY
        .lock()
        .ok()
        .and_then(|map| map.as_ref().and_then(|map| map.get(&id).copied()));
    match known {
        None => true,
        Some(None) => false,
        Some(Some(expires_at)) => expires_within(expires_at, now),
    }
}

fn expires_within(expires_at: Timestamp, now: Timestamp) -> bool {
    let margin =
        jiff::SignedDuration::try_from(pl_auth::WARM_MARGIN).unwrap_or(jiff::SignedDuration::MAX);
    now.checked_add(margin)
        .map_or(true, |deadline| expires_at <= deadline)
}

fn remember(id: Uuid, expires_at: Option<Timestamp>) {
    if let Ok(mut map) = EXPIRY.lock() {
        map.get_or_insert_with(HashMap::new).insert(id, expires_at);
    }
}
