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
pub const DISCORD_CLIENT_ID: &str = "949311557542756362";

/// Content mirrors, tried in order (see docs/content-format.md).
pub const CONTENT_MIRRORS: &[&str] = &[
    "https://cdn.projects.gg/v1/",
    "https://raw.githubusercontent.com/projects-gg/launcher-content/main/v1/",
];

/// Public keys trusted to sign the content index (base64, Ed25519). Studio prints the key it
/// generates; add it here and keep old keys during a rotation.
pub const CONTENT_PUBLIC_KEYS: &[&str] = &[
    // key id 5125c0d365b019ce (generated 2026-10-15)
    "muj3CSu3m9jpB7fUP5TbkRSnISGEH13XbTt6agmIK68=",
];

/// The game currently running (one at a time).
pub struct RunningGame {
    pub instance_id: String,
    pub process: Arc<GameProcess>,
}

/// Everything long-lived the commands need. Cheap handles only; heavy work runs in tasks.
pub struct AppState {
    pub root: PathBuf,
    pub http: Http,
    pub downloader: Downloader,
    pub auth: AuthManager,
    pub content: ContentClient,
    pub presence: PresenceManager,
    pub mc: McContext,
    pub instances: InstanceStore,
    pub running: Mutex<Option<RunningGame>>,
    pub launch_cancel: Mutex<Option<CancellationToken>>,
    /// True when the OS keychain is unavailable and secrets only live for this session.
    pub ephemeral_secrets: bool,
    pub login_cancel: Mutex<Option<CancellationToken>>,
    pub content_refreshing: AtomicBool,
}

/// Data root. `PROJECTS_LAUNCHER_DATA_DIR` overrides the per-OS default (portable installs,
/// development on a second disk).
pub fn default_root() -> PathBuf {
    if let Some(dir) = std::env::var_os("PROJECTS_LAUNCHER_DATA_DIR").filter(|d| !d.is_empty()) {
        return PathBuf::from(dir);
    }
    DataDirs::default_root().unwrap_or_else(|| PathBuf::from("ProjectsLauncher"))
}

impl AppState {
    pub fn init(version: &str) -> anyhow::Result<Self> {
        let root = default_root();
        std::fs::create_dir_all(&root)?;

        let http = Http::new(HttpConfig::new(format!(
            "ProjectsLauncher/{version} (+https://projects.gg)"
        )))?;
        let downloader = Downloader::new(http.clone());

        let (secrets, ephemeral_secrets): (Arc<dyn SecretStore>, bool) =
            if KeyringStore::is_available() {
                (Arc::new(KeyringStore::new()), false)
            } else {
                tracing::warn!(
                    "OS keychain unavailable; account secrets are kept for this session only"
                );
                (Arc::new(MemoryStore::new()), true)
            };
        let accounts = AccountStore::open(root.join("accounts.json"))?;
        let auth = AuthManager::new(MicrosoftAuth::new(http.clone()), accounts, secrets);

        let trusted = CONTENT_PUBLIC_KEYS
            .iter()
            .filter_map(|key| pl_content::decode_verifying_key(key).ok())
            .collect();
        let content = ContentClient::new(
            http.clone(),
            CONTENT_MIRRORS,
            root.join("cache").join("content"),
            trusted,
        )?
        .with_request_timeout(Duration::from_secs(4));

        // Setup runs outside the async runtime, so hand the actor Tauri's tokio handle.
        let tauri::async_runtime::RuntimeHandle::Tokio(handle) = tauri::async_runtime::handle();
        let presence = PresenceManager::start_on(&handle, PresenceConfig::new(DISCORD_CLIENT_ID));

        let data_dirs = DataDirs::new(&root);
        data_dirs.ensure()?;
        let instances = InstanceStore::new(&data_dirs);
        let mc = McContext::new(http.clone(), data_dirs);

        Ok(Self {
            root,
            http,
            downloader,
            auth,
            content,
            presence,
            mc,
            instances,
            running: Mutex::new(None),
            launch_cancel: Mutex::new(None),
            ephemeral_secrets,
            login_cancel: Mutex::new(None),
            content_refreshing: AtomicBool::new(false),
        })
    }
}
