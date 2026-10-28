use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use parking_lot::Mutex;
use pl_auth::{AccountStore, AuthManager, KeyringStore, MemoryStore, MicrosoftAuth, SecretStore};
use pl_content::ContentClient;
use pl_discord::{PresenceConfig, PresenceManager};
use pl_mc::install::McContext;
use pl_mc::instance::InstanceStore;
use pl_mc::launch::GameProcess;
use pl_mc::paths::DataDirs;
use pl_net::{CancellationToken, Downloader, Http, HttpConfig};

/// Discord application of the Projects launcher (public id).
pub const DISCORD_CLIENT_ID: &str = "1560400835589181552";

/// Content mirrors, tried in order (see docs/content-format.md).
pub const CONTENT_MIRRORS: &[&str] = &[
    "https://cdn.projects.gg/v1/",
    "https://raw.githubusercontent.com/projects-gg/launcher-content/main/v1/",
];

/// Content mirrors in use. Debug builds accept `PROJECTS_CONTENT_MIRRORS` (comma-separated base
/// URLs) for local end-to-end tests; the content must still carry a trusted signature.
pub fn content_mirrors() -> Vec<String> {
    test_content_mirrors()
        .unwrap_or_else(|| CONTENT_MIRRORS.iter().map(|m| (*m).to_owned()).collect())
}

/// Debug builds: the test mirrors `PROJECTS_CONTENT_MIRRORS` names (each ending in `/`), if any.
/// Release builds never have any. A test launcher's updater reads only these
/// (`pl_content::release_channel_urls`).
pub fn test_content_mirrors() -> Option<Vec<String>> {
    #[cfg(debug_assertions)]
    if let Ok(list) = std::env::var("PROJECTS_CONTENT_MIRRORS") {
        let mirrors: Vec<String> = list
            .split(',')
            .map(str::trim)
            .filter(|m| !m.is_empty())
            .map(|m| {
                if m.ends_with('/') {
                    m.to_owned()
                } else {
                    format!("{m}/")
                }
            })
            .collect();
        if !mirrors.is_empty() {
            return Some(mirrors);
        }
    }
    None
}

/// Keys trusted to sign the content index: `pl_content::CONTENT_PUBLIC_KEYS` (the list Studio
/// checks its key against too). Debug builds also trust `PROJECTS_CONTENT_TEST_KEY` (base64 public
/// key) so a throwaway key can sign test content; release builds never do.
fn trusted_content_keys() -> Vec<pl_content::VerifyingKey> {
    #[allow(unused_mut)]
    let mut keys = pl_content::content_public_keys();
    #[cfg(debug_assertions)]
    if let Ok(key) = std::env::var("PROJECTS_CONTENT_TEST_KEY")
        && let Ok(key) = pl_content::decode_verifying_key(key.trim())
    {
        keys.push(key);
    }
    keys
}

/// A game the launcher runs (several may run at once, PLAY-AGAIN).
pub struct RunningGame {
    /// The launcher's own number for it (the page names a game with it: closing, "Bağlan").
    pub id: u64,
    pub instance_id: String,
    /// The instance's game folder (games of one folder share its files).
    pub game_dir: PathBuf,
    pub process: Arc<GameProcess>,
    /// A game an earlier start of the launcher left running when it quit, followed again
    /// (`commands::launch::adopt_detached`): it does not keep the launcher open.
    pub adopted: bool,
    /// The account it plays with ("Oyunda" for that account alone); `None` only for a game an
    /// older launcher left running.
    pub account: Option<pl_mc::launch::GameAccount>,
    /// Its overlay bridge's port (none without the overlay).
    pub overlay_port: Option<u16>,
    /// Its bridge's token: world sharing's gate key (HOST1), when world sharing moves to it.
    pub host_token: Option<String>,
    /// Set once another game runs in the same folder beside it (its crash report is then only
    /// the one its own output names).
    pub shared_folder: Arc<AtomicBool>,
}

/// Every game the launcher runs, in the order they started (PLAY-AGAIN). The oldest leads: world
/// sharing (HOST1) follows it.
#[derive(Default)]
pub struct Games(Vec<RunningGame>);

impl Games {
    /// A game runs (any).
    pub fn any(&self) -> bool {
        !self.0.is_empty()
    }

    /// A game this start of the launcher started runs (one a former start left running does not
    /// keep the launcher open).
    pub fn any_followed(&self) -> bool {
        self.0.iter().any(|game| !game.adopted)
    }

    /// A game of `instance_id` runs.
    pub fn of_instance(&self, instance_id: &str) -> bool {
        self.0.iter().any(|game| game.instance_id == instance_id)
    }

    /// The games running in `game_dir`.
    pub fn in_folder<'a>(&'a self, game_dir: &'a Path) -> impl Iterator<Item = &'a RunningGame> {
        self.0.iter().filter(move |game| game.game_dir == game_dir)
    }

    /// The instances whose games run (each once).
    pub fn instance_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = Vec::new();
        for game in &self.0 {
            if !ids.contains(&game.instance_id) {
                ids.push(game.instance_id.clone());
            }
        }
        ids
    }

    pub fn get(&self, id: u64) -> Option<&RunningGame> {
        self.0.iter().find(|game| game.id == id)
    }

    /// The oldest game this start of the launcher started: the one world sharing follows (a game
    /// a former start left running has no bridge).
    pub fn lead(&self) -> Option<&RunningGame> {
        self.0.iter().find(|game| !game.adopted)
    }

    pub fn iter(&self) -> impl Iterator<Item = &RunningGame> {
        self.0.iter()
    }

    pub fn push(&mut self, game: RunningGame) {
        self.0.push(game);
    }

    /// Takes the game `id` out; it was the lead when it was the oldest.
    pub fn remove(&mut self, id: u64) -> Option<RunningGame> {
        let at = self.0.iter().position(|game| game.id == id)?;
        Some(self.0.remove(at))
    }
}

/// Everything long-lived the commands need. Cheap handles only; heavy work runs in tasks.
pub struct AppState {
    pub root: PathBuf,
    pub http: Http,
    pub auth: AuthManager,
    pub content: ContentClient,
    pub presence: PresenceManager,
    pub afk: pl_afk::AfkManager,
    pub mc: McContext,
    pub instances: InstanceStore,
    /// The games that run now (PLAY-AGAIN: several at once).
    pub games: Mutex<Games>,
    /// The play time counted this run, so that games of one instance side by side count once.
    pub sessions: Mutex<pl_mc::launch::SessionLedger>,
    /// The launch being prepared, if any (`commands::launch`).
    pub launch_cancel: Mutex<Option<crate::commands::launch::PreparingLaunch>>,
    /// Downloaded, signature-verified launcher update waiting for "restart to update".
    pub pending_update: Mutex<Option<(tauri_plugin_updater::Update, Vec<u8>)>>,
    /// True when the OS keychain is unavailable and secrets only live for this session.
    pub ephemeral_secrets: bool,
    pub login_cancel: Mutex<Option<CancellationToken>>,
    pub content_refreshing: AtomicBool,
    /// The optional friends feature (off until the signed config turns it on).
    pub social: pl_social::SocialService,
    /// SKIN1: Projects skins of username accounts and the game account link
    /// (`commands::projects_skin`).
    pub projects_skin: pl_social::ProjectsSkins,
    /// The in-game overlay: its choices and the running game's bridge (`crate::overlay`).
    pub overlay: pl_overlay::Overlay,
    /// "Dünyanı arkadaşlarına aç" (HOST1, `crate::host`): off until the signed config and the
    /// service turn it on.
    pub worlds: pl_worlds::Worlds,
    /// The Discord bot's news feed (`<data>/cache/feed/`); used only while the signed config
    /// enables it (`commands::feed`).
    pub feed: pl_content::FeedClient,
}

/// Data root. `PROJECTS_LAUNCHER_DATA_DIR` overrides the per-OS default (portable installs,
/// development on a second disk); after that, a folder chosen with Settings → Veri → Taşı
/// (pointer file outside the data root, see `commands::data`).
pub fn default_root() -> PathBuf {
    if let Some(dir) = std::env::var_os("PROJECTS_LAUNCHER_DATA_DIR").filter(|d| !d.is_empty()) {
        return PathBuf::from(dir);
    }
    if let Some(dir) = crate::commands::data::relocated_root() {
        return dir;
    }
    platform_default_root()
        .unwrap_or_else(|| PathBuf::from(&crate::identity::current().data_folder))
}

/// The data root nothing moved: `DataDirs::default_root` (`%APPDATA%\ProjectsLauncher` on
/// Windows), or a debug build's own folder next to it (`crate::identity`).
pub fn platform_default_root() -> Option<PathBuf> {
    let identity = crate::identity::current();
    if identity.is_isolated() {
        dirs::data_dir().map(|dir| dir.join(&identity.data_folder))
    } else {
        DataDirs::default_root()
    }
}

impl AppState {
    pub fn init(app: &tauri::AppHandle, version: &str) -> anyhow::Result<Self> {
        let root = default_root();
        std::fs::create_dir_all(&root)?;

        let http = Http::new(HttpConfig::new(format!(
            "ProjectsLauncher/{version} (+https://projects.gg)"
        )))?;
        let downloader = Downloader::new(http.clone());

        // Debug builds keep their secrets under a keychain service of their own (crate::identity).
        let keychain_service = &crate::identity::current().keychain_service;
        let (secrets, ephemeral_secrets): (Arc<dyn SecretStore>, bool) =
            if KeyringStore::is_available() {
                (
                    Arc::new(KeyringStore::with_service(keychain_service.clone())),
                    false,
                )
            } else {
                tracing::warn!(
                    "OS keychain unavailable; account secrets are kept for this session only"
                );
                (Arc::new(MemoryStore::new()), true)
            };
        let accounts = AccountStore::open(root.join("accounts.json"))?;
        // Skins of Microsoft accounts, kept on this computer for "Önceki skinler".
        let auth = AuthManager::new(MicrosoftAuth::new(http.clone()), accounts, secrets)
            .with_skin_history(pl_auth::SkinHistory::new(root.join("skins")));

        let content = ContentClient::new(
            http.clone(),
            &content_mirrors(),
            root.join("cache").join("content"),
            trusted_content_keys(),
        )?
        .with_request_timeout(Duration::from_secs(4));
        let feed = pl_content::FeedClient::new(http.clone(), root.join("cache").join("feed"))
            .with_request_timeout(Duration::from_secs(4));

        // Setup runs outside the async runtime, so hand the actor Tauri's tokio handle.
        let tauri::async_runtime::RuntimeHandle::Tokio(handle) = tauri::async_runtime::handle();
        let presence = PresenceManager::start_on(&handle, PresenceConfig::new(DISCORD_CLIENT_ID));
        // Before the friends feature: what it tells the page also feeds a running game's overlay.
        let overlay = crate::overlay::create(app, &root);
        // "Hızlı doku paketi yükleme": the kept choice, before any pack sync can run.
        crate::fastpack::load(&root);
        let social = crate::commands::social::create(
            app,
            &http,
            version,
            handle.clone(),
            ephemeral_secrets,
            &root,
            overlay.clone(),
        );
        let projects_skin =
            crate::commands::projects_skin::create(app, &social, &http, &root, handle.clone());
        let worlds = crate::host::create(app, &root, version, social.clone(), overlay.clone());

        // AFK accounts (Projects Console Client sessions). Events go straight to the UI (and
        // state changes to the tray's summary). A connect that finds no client installed and no
        // build known refreshes the content index right away (`commands::afk::release_source`).
        let afk_paths = pl_afk::AfkPaths::new(root.join("afk"));
        let afk_secrets: Arc<dyn pl_afk::SecretStore> = if ephemeral_secrets {
            Arc::new(pl_afk::MemorySecretStore::new())
        } else {
            Arc::new(pl_afk::KeyringSecretStore::with_service(
                keychain_service.clone(),
            ))
        };
        let (afk_store, report) = pl_afk::AccountStore::open(afk_paths.clone(), afk_secrets)?;
        tracing::info!(?report, "AFK accounts loaded");
        let afk = pl_afk::AfkManager::new(
            Arc::new(afk_store),
            Arc::new(pl_afk::AfkRuntime::new(afk_paths.bin_dir())),
            crate::commands::afk::event_sink(app),
            pl_afk::ManagerOptions {
                config: pl_afk::ManagerConfig::default(),
                downloader: Some(downloader.clone()),
                download_progress: None,
                release_source: Some(crate::commands::afk::release_source(app)),
                spawner: None,
                launcher_nickname: auth
                    .active_account()
                    .map(|a| a.username)
                    .unwrap_or_default(),
            },
        );

        let data_dirs = DataDirs::new(&root);
        data_dirs.ensure()?;
        let instances = InstanceStore::new(&data_dirs);
        let mc = McContext::new(http.clone(), data_dirs);

        Ok(Self {
            root,
            http,
            auth,
            content,
            presence,
            afk,
            mc,
            instances,
            games: Mutex::new(Games::default()),
            sessions: Mutex::new(pl_mc::launch::SessionLedger::new()),
            launch_cancel: Mutex::new(None),
            pending_update: Mutex::new(None),
            ephemeral_secrets,
            login_cancel: Mutex::new(None),
            content_refreshing: AtomicBool::new(false),
            social,
            projects_skin,
            overlay,
            worlds,
            feed,
        })
    }
}
