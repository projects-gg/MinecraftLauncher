//! The in-game overlay (OVL2, OVL-B): the player's choices, kept in the data folder so the starts
//! before the page asks follow them too; the mod's jars, shipped with the launcher; one bridge per
//! game start; what the page hears.
//!
//! The logic lives in `pl_overlay` (tested there): this module wires it to the app. Right before a
//! game starts, the instance gets the jar for its Minecraft version while "Oyun içi katman" is on
//! and it is a Fabric instance the mod supports, official pack or not ([`prepare_mod`]), and then a
//! bridge ([`for_launch`]); the game's exit, "Oyun açılınca launcher: Kapansın", quitting and the
//! switch going off end it. Between games the overlay costs nothing.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use pl_content::LauncherItems;
use pl_mc::addons::{AddonInstallOptions, Addons};
use pl_mc::gamefiles::ImportKind;
use pl_mc::instance::Instance;
use pl_mc::loaders::LoaderKind;
use pl_mc::modrinth::ModrinthResolveTarget;
use pl_net::CancellationToken;
use pl_overlay::install::{BUNDLE_DIR, Bundle, Outcome, Request};
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
/// A friend's favourite or "Çevrimiçi olunca bildir" changed from the game's person menu
/// (`PeopleChange`, OVL-UX2): the page keeps those marks, as its own buttons would.
pub const PEOPLE_EVENT: &str = "overlay-people";
/// The choices, in the data folder.
const CONFIG_FILE: &str = "overlay.json";
/// How long a start waits for the Fabric API the overlay's jar needs: past it, the game starts
/// without the overlay this time.
const FABRIC_API_WAIT: Duration = Duration::from_secs(30);

/// "Oyun içi katman" for the code that plans pack syncs far from the app state (the mod center,
/// repairs, version switches, background preparation). Follows the hub's setting.
static ENABLED: AtomicBool = AtomicBool::new(true);

/// The jars this build ships, read once at the start ([`load_bundle`]).
static BUNDLE: OnceLock<Option<Bundle>> = OnceLock::new();

fn bundle() -> Option<&'static Bundle> {
    BUNDLE.get().and_then(Option::as_ref)
}

/// The overlay with the choices saved in `root` (or the defaults). Status changes reach the page.
/// Reads the shipped jars' index too.
pub fn create(app: &AppHandle, root: &Path) -> Overlay {
    let _ = BUNDLE.set(load_bundle(app));
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
    // Favourites and "Çevrimiçi olunca bildir" from the game's person menu (OVL-UX2): the page keeps
    // them, so it hears the change awake (a resting page is woken for it on the main thread), and
    // the data folder's copy follows for the starts before the page pushes its choices again.
    let emitter = app.clone();
    overlay.on_people(move |change| {
        if let Some(state) = emitter.try_state::<AppState>() {
            save(&state.root, &state.overlay.config());
        }
        let main = emitter.clone();
        let _ = emitter.run_on_main_thread(move || {
            crate::webview_rest::wake_if_suspended(&main, "overlay-people");
            let _ = main.emit(PEOPLE_EVENT, change);
        });
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

/// The resources' `overlay/` folder (`scripts/release/overlay-jars.mjs` fills it before a build).
/// Debug builds take `PROJECTS_OVERLAY_JARS=<folder>` instead (a test exe copied out of the target
/// folder has no resources next to it).
fn load_bundle(app: &AppHandle) -> Option<Bundle> {
    let dir = debug_jars_dir().or_else(|| {
        app.path()
            .resource_dir()
            .ok()
            .map(|dir| dir.join(BUNDLE_DIR))
    })?;
    let bundle = Bundle::load(&dir);
    match &bundle {
        Some(bundle) => tracing::info!(versions = ?bundle.versions(), "in-game overlay jars"),
        None => tracing::info!(
            dir = %dir.display(),
            "no in-game overlay jars in this build; the official pack's item stands in"
        ),
    }
    bundle
}

fn debug_jars_dir() -> Option<PathBuf> {
    #[cfg(debug_assertions)]
    {
        std::env::var_os("PROJECTS_OVERLAY_JARS")
            .filter(|dir| !dir.is_empty())
            .map(PathBuf::from)
    }
    #[cfg(not(debug_assertions))]
    {
        None
    }
}

/// The launcher settings that own pack items, for every pack sync: with jars of its own the
/// launcher never installs the pack's overlay item and keeps the pack's Fabric API for its jar.
pub fn pack_items() -> LauncherItems {
    LauncherItems {
        overlay: ENABLED.load(Ordering::Relaxed),
        overlay_bundled: bundle().is_some(),
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
/// it, the next start (and pack sync) installs or removes the mod.
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
/// follows and so does the page's. The next start removes or brings back the mod.
pub fn switched_elsewhere(app: &AppHandle, enabled: bool) {
    set_enabled(app, enabled);
    let _ = app.emit(ENABLED_EVENT, enabled);
}

/// The overlay jars the launcher put in this game folder (lowercase file names without
/// `.disabled`): mod lists leave them out, "Bu modu kapat" on one turns the switch off.
pub fn owned_files(game_dir: &Path) -> std::collections::HashSet<String> {
    pl_overlay::install::owned_files(game_dir)
}

/// Right before a start, after the pack's sync and before the mod check: the overlay's jar for
/// this instance in place, or ours taken away (`pl_overlay::install`). Fabric API the jar needs
/// comes from Modrinth the way the launcher installs mods ([`install_fabric_api`]). Answers whether
/// the game gets an overlay mod (ours, or a copy someone else put there). Never stops a launch.
pub async fn prepare_mod(
    state: &AppState,
    instance: &Instance,
    (instance_dir, game_dir): (&Path, &Path),
    java_major: u32,
    cancel: &CancellationToken,
) -> bool {
    let env = crate::commands::modhealth::environment(instance, Some(java_major));
    let enabled = state.overlay.enabled();
    let ask = |ask_fabric_api: bool| {
        pl_overlay::install::reconcile(Request {
            game_dir,
            bundle: bundle(),
            env: &env,
            enabled,
            ask_fabric_api,
        })
    };
    let outcome = match ask(true).await {
        Outcome::NeedsFabricApi => {
            install_fabric_api(state, instance, (instance_dir, game_dir), cancel).await;
            ask(false).await
        }
        outcome => outcome,
    };
    matches!(outcome, Outcome::Ready { .. })
}

/// Fabric API for the overlay's jar: its newest version for the instance's Minecraft, installed
/// as the mod pages install a mod (verified, recorded in the instance's add-on index, so the
/// player sees it and can update it). It stays when the overlay goes: other mods may need it.
async fn install_fabric_api(
    state: &AppState,
    instance: &Instance,
    (instance_dir, game_dir): (&Path, &Path),
    cancel: &CancellationToken,
) {
    let client = crate::commands::modrinth::modrinth_client(state);
    let addons = Addons::new(
        game_dir,
        crate::commands::modrinth::addon_index_path(instance_dir),
    )
    .with_icon_dir(crate::commands::modrinth::icon_dir(state));
    let target = ModrinthResolveTarget::for_loader(LoaderKind::Fabric, instance.mc_version.clone());
    let job = async {
        let version = client
            .best_version(pl_content::FABRIC_API_PROJECT, &target)
            .await?
            .ok_or_else(|| pl_mc::Error::NotFound("Fabric API for this version".to_owned()))?;
        addons
            .install(
                &client,
                &state.mc.downloader,
                ImportKind::Mod,
                vec![version],
                &target,
                AddonInstallOptions {
                    with_dependencies: true,
                    concurrency: 4,
                    cancel: cancel.clone(),
                    progress: None,
                },
            )
            .await
    };
    match tokio::time::timeout(FABRIC_API_WAIT, job).await {
        Ok(Ok(report)) => tracing::info!(
            installed = ?report.installed.iter().map(|i| &i.file_name).collect::<Vec<_>>(),
            "Fabric API installed for the in-game overlay"
        ),
        Ok(Err(error)) => {
            tracing::info!(%error, "Fabric API not installed; the game starts without the overlay");
        }
        Err(_) => tracing::info!("Fabric API took too long; the game starts without the overlay"),
    }
}

/// `PROJECTS_OVERLAY_ALWAYS=1` (debug builds only): every launch gets a bridge, for a local test
/// without the mod (a stand-in Java, a mock mod client).
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

/// A fresh bridge for the game about to start, when the switch is on and the instance has an
/// overlay mod ([`prepare_mod`]). Never stops a launch: a bridge that cannot open means a game
/// without it. `game_name` is the account the game plays as: codeless linking (SKIN1-O) vouches
/// only for it.
pub async fn for_launch(
    state: &AppState,
    has_mod: bool,
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
    if !has_mod && !debug_always() {
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
    /// The Minecraft versions this build ships the mod for (`26.1.2`, `26.2`, …): a Fabric
    /// instance on one of them gets it. Empty in a build without the jars.
    pub versions: Vec<String>,
}

#[tauri::command]
#[specta::specta]
pub fn overlay_state(state: State<'_, AppState>) -> OverlayStateView {
    OverlayStateView {
        config: state.overlay.config(),
        connected: state.overlay.connected(),
        versions: bundle().map(Bundle::versions).unwrap_or_default(),
    }
}
