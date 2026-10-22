use std::path::PathBuf;
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

/// The game currently running (one at a time).
pub struct RunningGame {
    pub instance_id: String,
    pub process: Arc<GameProcess>,
    /// A game an earlier start of the launcher left running when it quit, followed again
    /// (`commands::launch::adopt_detached`): it does not keep the launcher open.
    pub adopted: bool,
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
    pub running: Mutex<Option<RunningGame>>,
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
    /// The in-game overlay: its choices and the running game's bridge (`crate::overlay`).
    pub overlay: pl_overlay::Overlay,
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
        let social = crate::commands::social::create(
            app,
            &http,
            version,
            handle.clone(),
            ephemeral_secrets,
            &root,
            overlay.clone(),
        );

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
            running: Mutex::new(None),
            launch_cancel: Mutex::new(None),
            pending_update: Mutex::new(None),
            ephemeral_secrets,
            login_cancel: Mutex::new(None),
            content_refreshing: AtomicBool::new(false),
            social,
            overlay,
            feed,
        })
    }
}
