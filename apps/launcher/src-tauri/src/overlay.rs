//! The in-game overlay (OVL2): the player's choices, kept in the data folder so the starts before
//! the page asks follow them too; one bridge per game start; what the page hears.
//!
//! The logic lives in `pl_overlay` (tested there): this module wires it to the app. A game start
//! gets a bridge only while "Oyun içi katman" is on and the official pack put the mod in the
//! instance ([`for_launch`]); the game's exit, "Oyun açılınca launcher: Kapansın", quitting and the
//! switch going off end it. Between games the overlay costs nothing.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use pl_content::{LauncherItems, OVERLAY_ITEM, PackState};
use pl_overlay::protocol::ByeReason;
use pl_overlay::{ConnectOutcome, Endpoint, Overlay, OverlayConfig};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::AppState;

/// The page hears whether the mod in the running game is connected (`OverlayStatus`): while it
/// is, its toasts stand in for the system notifications of the same events.
pub const STATUS_EVENT: &str = "overlay-status";
/// The switch changed without the page (the crash help's "Bu modu kapat" on the overlay).
pub const ENABLED_EVENT: &str = "overlay-enabled";
/// The choices, in the data folder.
const CONFIG_FILE: &str = "overlay.json";

/// "Oyun içi katman" for the code that plans pack syncs far from the app state (the mod center,
/// repairs, version switches, background preparation). Follows the hub's setting.
static ENABLED: AtomicBool = AtomicBool::new(true);

/// The overlay with the choices saved in `root` (or the defaults). Status changes reach the page.
pub fn create(app: &AppHandle, root: &Path) -> Overlay {
    let config: OverlayConfig = std::fs::read(root.join(CONFIG_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    ENABLED.store(config.enabled, Ordering::Relaxed);
    let overlay = Overlay::new(config);
    let emitter = app.clone();
    overlay.on_status(move |status| {
        let _ = emitter.emit(STATUS_EVENT, status);
    });
    // "Discord'dan mesaj at" from the game's friend menu (OVL-UX): the profile in the Discord app
    // when it is installed, else the web profile (`https://` only), as the page's button does.
    overlay.on_discord(|id, url| {
        tauri::async_runtime::spawn_blocking(move || {
            let in_app = pl_desktop::discord::open_profile_in_app(&id, |link| {
                tauri_plugin_opener::open_url(link, None::<&str>).is_ok()
            });
            if !in_app
                && url.starts_with("https://")
                && let Err(error) = tauri_plugin_opener::open_url(&url, None::<&str>)
            {
                tracing::warn!(%error, "could not open a friend's Discord profile");
            }
        });
    });
    overlay
}

/// The launcher settings that own pack items, for every pack sync.
pub fn pack_items() -> LauncherItems {
    LauncherItems {
        overlay: ENABLED.load(Ordering::Relaxed),
    }
}

fn save(root: &Path, config: &OverlayConfig) {
    let saved = serde_json::to_vec(config)
        .map_err(|error| error.to_string())
        .and_then(|bytes| {
            pl_mc::paths::write_atomic_blocking(&root.join(CONFIG_FILE), &bytes)
                .map_err(|error| error.to_string())
        });
    if let Err(error) = saved {
        tracing::warn!(%error, "overlay setting not saved");
    }
}

/// Applies (and keeps) new choices: a running session follows them, turning the overlay off ends
/// it, the next pack sync installs or removes the mod.
fn apply(state: &AppState, config: OverlayConfig) {
    if state.overlay.configure(config.clone()) {
        ENABLED.store(config.enabled, Ordering::Relaxed);
        save(&state.root, &config);
    }
}

/// The switch alone (a launch's settings, the crash help): the rest of the choices stay.
pub fn set_enabled(app: &AppHandle, enabled: bool) {
    let state = app.state::<AppState>();
    let mut config = state.overlay.config();
    if config.enabled == enabled {
        return;
    }
    config.enabled = enabled;
    tracing::info!(enabled, "in-game overlay switched");
    apply(&state, config);
}

/// The overlay's mod was switched outside Settings (the crash help's "Bu modu kapat"): the switch
/// follows and so does the page's. The next launch's sync removes or brings back the mod.
pub fn switched_elsewhere(app: &AppHandle, enabled: bool) {
    set_enabled(app, enabled);
    let _ = app.emit(ENABLED_EVENT, enabled);
}

/// Whether the official pack's overlay item is in place in this game folder: the last sync
/// installed it, left it switched on, and its file is there.
async fn installed(game_dir: &Path) -> bool {
    let Ok(Some(pack)) = PackState::load(game_dir).await else {
        return false;
    };
    let Some(path) = pack.installed_item(OVERLAY_ITEM) else {
        return false;
    };
    tokio::fs::metadata(game_dir.join(path))
        .await
        .is_ok_and(|meta| meta.is_file())
}

/// `PROJECTS_OVERLAY_ALWAYS=1` (debug builds only): every launch gets a bridge, for a local test
/// without the pack's mod (a stand-in Java, a mock mod client).
fn debug_always() -> bool {
    #[cfg(debug_assertions)]
    {
        std::env::var("PROJECTS_OVERLAY_ALWAYS").is_ok_and(|value| value == "1")
    }
    #[cfg(not(debug_assertions))]
    {
        false
    }
}

/// A launch's bridge. Dropped without [`OverlayLaunch::keep`] (the launch stopped before its game
/// ran), it closes again.
pub struct OverlayLaunch {
    overlay: Option<Overlay>,
    endpoint: Option<Endpoint>,
}

impl OverlayLaunch {
    pub fn endpoint(&self) -> Option<&Endpoint> {
        self.endpoint.as_ref()
    }

    /// The game started: the bridge stays until it ends. Answers the bridge's port for
    /// [`game_ended`].
    pub fn keep(mut self) -> Option<u16> {
        self.overlay = None;
        self.endpoint.as_ref().map(Endpoint::port)
    }
}

impl Drop for OverlayLaunch {
    fn drop(&mut self) {
        if let Some(overlay) = self.overlay.take()
            && let Some(port) = self.endpoint.as_ref().map(Endpoint::port)
        {
            // Only this launch's bridge: a newer launch may have its own already.
            tauri::async_runtime::spawn(async move {
                overlay.stop_for(port, ByeReason::LauncherExit).await;
            });
        }
    }
}

/// A fresh bridge for the game about to start, when the switch is on and the instance has the
/// pack's overlay mod. Never stops a launch: a bridge that cannot open means a game without it.
/// `game_name` is the account the game plays as: codeless linking (SKIN1-O) vouches only for it.
pub async fn for_launch(
    state: &AppState,
    game_dir: &Path,
    launcher_version: &str,
    game_name: &str,
) -> OverlayLaunch {
    let none = OverlayLaunch {
        overlay: None,
        endpoint: None,
    };
    if !state.overlay.enabled() {
        return none;
    }
    if !installed(game_dir).await && !debug_always() {
        return none;
    }
    match state
        .overlay
        .start_with_game(state.social.clone(), launcher_version, game_name)
        .await
    {
        Ok(endpoint) => OverlayLaunch {
            overlay: Some(state.overlay.clone()),
            endpoint: Some(endpoint),
        },
        Err(error) => {
            tracing::warn!(%error, "the in-game overlay's bridge did not open; the game starts without it");
            none
        }
    }
}

/// The game ended: its bridge (`port`, from [`OverlayLaunch::keep`]) goes; a newer game's stays.
pub async fn game_ended(state: &AppState, port: Option<u16>) {
    if let Some(port) = port {
        state.overlay.stop_for(port, ByeReason::LauncherExit).await;
    }
}

/// The launcher quits: the mod hears why (`launcher-exit`, or `game-start-close` for "Oyun açılınca
/// launcher: Kapansın") before the bridge goes.
pub async fn leaving(app: &AppHandle, reason: ByeReason) {
    if let Some(state) = app.try_state::<AppState>() {
        state.overlay.stop(reason).await;
    }
}

/// "Bağlan" (a crew call, an invite, a plan), right after the player chose and confirmed it in
/// the launcher: the running game joins `target` (`host[:port]`: Projects, or an address a friend
/// or crew shares now) through its overlay instead of starting again. One at a time; `unavailable`
/// (no overlay, an older mod, another one on its way, a target nobody shares, no answer in 5 s,
/// the game said no): start the game again as before. Never call it on the launcher's own
/// initiative (a reminder, a timer).
pub async fn connect_in_game(state: &AppState, target: &str) -> ConnectOutcome {
    state.overlay.connect_in_game(target).await
}

/// [`connect_in_game`] for the page.
#[tauri::command]
#[specta::specta]
pub async fn overlay_connect_in_game(app: AppHandle, target: String) -> ConnectOutcome {
    connect_in_game(&app.state::<AppState>(), &target).await
}

/// The page's choices: Settings → Genel "Oyun içi katman", the friends notification choices and
/// reduced motion, at its start and on every change.
#[tauri::command]
#[specta::specta]
pub fn overlay_configure(state: State<'_, AppState>, config: OverlayConfig) {
    apply(&state, config);
}

/// What the page shows at its start.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OverlayStateView {
    pub config: OverlayConfig,
    /// The mod in the running game is connected now.
    pub connected: bool,
}

#[tauri::command]
#[specta::specta]
pub fn overlay_state(state: State<'_, AppState>) -> OverlayStateView {
    OverlayStateView {
        config: state.overlay.config(),
        connected: state.overlay.connected(),
    }
}
