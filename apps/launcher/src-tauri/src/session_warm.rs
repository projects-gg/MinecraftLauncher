//! The Microsoft session refreshed before the player presses OYNA (LIGHT). A launch refreshes the
//! Minecraft token only when it expires within 10 minutes, and then waits for Microsoft's five
//! requests in a row: 2.5–2.9 s of the player's first game of the day. So at the moments the
//! player is about to play (the launcher's start, its window coming back, another account chosen)
//! a token that expires within [`pl_auth::WARM_MARGIN`] is refreshed in the background, once per
//! moment: no timer, no polling. What is known of each token's expiry stays in memory, so a window
//! coming back with nothing due costs no keychain read. A launch during a warm-up waits for it
//! (they share pl-auth's lock) and asks Microsoft nothing itself. Without a network it stays
//! quiet; the next moment tries again, and the launch decides as before. Any other failure (a
//! refresh token Microsoft no longer takes) is left to the launch, with the token it still has:
//! the warm-up neither marks the account nor asks again this run.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

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
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let started = Instant::now();
        let result = state
            .auth
            .warm_session(account.id, pl_auth::WARM_MARGIN, &CancellationToken::new())
            .await;
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
                if error.class() != pl_auth::FailureClass::Offline {
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
