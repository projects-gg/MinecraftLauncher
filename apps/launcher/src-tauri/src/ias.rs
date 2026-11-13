//! "Hesapları Oyun İçi Hesap Değiştiriciye Ekle" (SRV-ACC): right before a game starts, the
//! launcher's username accounts join the In-Game Account Switcher's list of the install, when the
//! mod is in its mods folder and on (`pl_mc::instance::ias`: one way, only adding, once per name and
//! install; Microsoft accounts never go). Settings → Genel shows the switch (on by default) while
//! the signed config's `features.iasSync` is on. The page owns the choice and pushes it at its start
//! and on every change; the data folder keeps a copy of it and of the last applied flag, so a launch
//! before the page asks follows them too. It never stops a launch: whatever goes wrong is a warning
//! in the log, never with the list's content.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use pl_auth::AccountKind;
use pl_mc::instance::ias::{self, IasSync};
use pl_mc::mods::scan::ModScanner;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::state::AppState;

/// The choice and the flag, in the data folder.
const CONFIG_FILE: &str = "ias-sync.json";

/// The switch (on by default).
static ENABLED: AtomicBool = AtomicBool::new(true);
/// The applied content's `features.iasSync` (off until Studio turns it on).
static FLAG: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Kept {
    enabled: bool,
    flag: bool,
}

impl Default for Kept {
    fn default() -> Self {
        Self {
            enabled: true,
            flag: false,
        }
    }
}

/// Reads the kept choice and flag once at the start (before any launch can run).
pub fn load(root: &Path) {
    let kept: Kept = std::fs::read(root.join(CONFIG_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    ENABLED.store(kept.enabled, Ordering::Relaxed);
    FLAG.store(kept.flag, Ordering::Relaxed);
}

/// The applied content snapshot's flag (`commands::content::apply_snapshot`), kept when it changed.
pub fn apply_flag(root: &Path, on: bool) {
    if FLAG.swap(on, Ordering::Relaxed) != on {
        save(root);
    }
}

fn on() -> bool {
    ENABLED.load(Ordering::Relaxed) && (FLAG.load(Ordering::Relaxed) || forced())
}

/// Debug builds started with `PROJECTS_IAS_SYNC=1` (end-to-end runs against the live content) add
/// whatever the signed config says; release builds never read the variable.
fn forced() -> bool {
    cfg!(debug_assertions)
        && std::env::var_os("PROJECTS_IAS_SYNC").is_some_and(|value| value == "1")
}

fn save(root: &Path) {
    let kept = Kept {
        enabled: ENABLED.load(Ordering::Relaxed),
        flag: FLAG.load(Ordering::Relaxed),
    };
    let saved = serde_json::to_vec(&kept)
        .map_err(|error| error.to_string())
        .and_then(|bytes| {
            pl_mc::paths::write_atomic_blocking(&root.join(CONFIG_FILE), &bytes)
                .map_err(|error| error.to_string())
        });
    if let Err(error) = saved {
        tracing::warn!(%error, "in-game account switcher setting not saved");
    }
}

/// The page's choice: called at its start and on every change; the next launch follows it.
#[tauri::command]
#[specta::specta]
pub fn ias_sync_configure(state: State<'_, AppState>, enabled: bool) {
    if ENABLED.swap(enabled, Ordering::Relaxed) != enabled {
        save(&state.root);
    }
}

/// Before a game of the install whose game folder is `game_dir` starts, while no other game runs
/// in that folder (its mod would save its own list over this one): the install's IAS list gets
/// the launcher's username accounts it has not had yet.
pub async fn before_launch(state: &AppState, game_dir: &Path) {
    if !on() {
        return;
    }
    let names: Vec<String> = state
        .auth
        .accounts()
        .into_iter()
        .filter(|account| account.kind == AccountKind::Offline)
        .map(|account| account.username)
        .collect();
    if names.is_empty() {
        return;
    }
    let game_dir = game_dir.to_path_buf();
    let synced = tokio::task::spawn_blocking(move || {
        let mods = ModScanner::shared().scan_dir(&game_dir.join("mods"));
        ias::installed(&mods).map(|version| ias::sync_names(&game_dir, &version, &names))
    })
    .await;
    match synced {
        Ok(None | Some(Ok(IasSync::UpToDate | IasSync::NoList))) => {}
        Ok(Some(Ok(IasSync::Added(count)))) => {
            tracing::info!(
                count,
                "username accounts added to the in-game account switcher"
            );
        }
        Ok(Some(Ok(IasSync::LeftAlone(problem)))) => {
            tracing::warn!(%problem, "in-game account switcher's list left as it is");
        }
        Ok(Some(Err(error))) => {
            tracing::warn!(%error, "in-game account switcher's list not updated");
        }
        Err(error) => tracing::warn!(%error, "in-game account switcher's list not updated"),
    }
}
