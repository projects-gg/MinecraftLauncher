//! "Launcher'ı sıfırla" (Settings → Veri). The command (`commands::data::data_reset`) leaves a
//! note and restarts; the next start runs the reset ([`run_pending`]) before anything opens the
//! data folder, the keychain or the page: it deletes the keychain entries of the accounts the data
//! folder names, removes the files (`pl_data::reset`: what goes and what stays) and has the page
//! clear its own storage on its first load ([`page_storage`]).

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use pl_afk::SecretStore as _;
use pl_auth::SecretStore as _;
use pl_social::TokenStore as _;
use tauri::plugin::TauriPlugin;

/// The page's storage key that names the last reset it cleared itself for. The welcome reads
/// [`FRESH_KEY`]: `features/onboarding` (`RESET_FRESH_KEY`).
const DONE_KEY: &str = "pl.reset";
const FRESH_KEY: &str = "pl.freshStart";
/// How long the start waits for the launcher that asked to be gone (it quits within a second).
const QUIT_WAIT: Duration = Duration::from_secs(15);

/// Leaves the note for the next start, naming this process so that start waits for it to quit.
pub fn request(root: &Path) -> pl_mc::Result<()> {
    let pid = std::process::id();
    let asked_by = pl_desktop::process::start_time(pid).map(|started| (pid, started));
    let request = pl_data::reset::Request {
        at_ms: jiff::Timestamp::now().as_millisecond(),
        asked_by,
    };
    pl_data::reset::request(root, &request)
}

/// A reset this start ran.
#[derive(Debug)]
pub struct Done {
    /// When the player asked; it names this reset to the page.
    pub id: i64,
    pub files: pl_data::reset::ResetReport,
    /// Keychain entries of accounts and AFK accounts deleted, and those that could not be.
    pub secrets: usize,
    pub secrets_kept: usize,
}

/// Runs the reset the player asked for. First thing in `run`, before the log file opens (the
/// logs go too) and before anything reads the data folder; the report is logged afterwards.
pub fn run_pending() -> Option<Done> {
    let root = crate::state::default_root();
    let request = pl_data::reset::pending(&root)?;
    // The launcher that asked may still be saving what it holds as it quits.
    if let Some((pid, started)) = request.asked_by
        && pid != std::process::id()
    {
        let deadline = Instant::now() + QUIT_WAIT;
        while Instant::now() < deadline && pl_desktop::process::is_same(pid, started) {
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    let (secrets, secrets_kept) = forget_secrets(&root);
    let files = pl_data::reset::remove_launcher_data(&root);
    pl_data::reset::finish(&root);
    Some(Done {
        id: request.at_ms,
        files,
        secrets,
        secrets_kept,
    })
}

/// Deletes the keychain entries the data folder names: each account's secrets, each AFK account's
/// login commands, the cloud AFK copies and the friends session. A debug build touches only its
/// own keychain service (`crate::identity`); without a keychain nothing was kept there.
fn forget_secrets(root: &Path) -> (usize, usize) {
    if !pl_auth::KeyringStore::is_available() {
        return (0, 0);
    }
    let service = crate::identity::current().keychain_service.clone();
    let (mut deleted, mut kept) = (0, 0);
    let mut tally = |ok: bool| {
        if ok {
            deleted += 1;
        } else {
            kept += 1;
        }
    };

    let secrets = pl_auth::KeyringStore::with_service(service.clone());
    if let Ok(accounts) = pl_auth::AccountStore::open(root.join("accounts.json")) {
        for account in accounts.list() {
            tally(secrets.delete(account.id).is_ok());
        }
    }

    let afk = pl_afk::KeyringSecretStore::with_service(service.clone());
    let afk_accounts = std::fs::read_to_string(root.join("afk").join("accounts.json"))
        .ok()
        .and_then(|json| pl_afk::legacy::parse_accounts(&json).ok())
        .unwrap_or_default();
    for listed in afk_accounts {
        tally(
            afk.delete(&pl_afk::login_commands_key(&listed.account.id))
                .is_ok(),
        );
    }

    pl_cloud_afk::Copies::new(
        Arc::new(pl_afk::KeyringSecretStore::with_service(service.clone())),
        root.join("cloud-afk").join("copies.json"),
    )
    .forget_all();
    pl_social::KeyringTokenStore::with_service(service).clear();
    (deleted, kept)
}

/// The page's storage after the reset `id`: the first page load of this run clears it (the
/// preferences, what was read, the welcome's progress) and marks it, so a window opened later
/// keeps what the player sets meanwhile. The welcome then skips the old launcher's import and
/// the name it would suggest, which would bring the removed names back.
pub fn page_storage(id: i64) -> TauriPlugin<tauri::Wry> {
    let script = format!(
        r#"(() => {{
  try {{
    if (localStorage.getItem("{DONE_KEY}") === "{id}") return;
    localStorage.clear();
    localStorage.setItem("{DONE_KEY}", "{id}");
    localStorage.setItem("{FRESH_KEY}", "1");
  }} catch {{}}
}})();"#
    );
    tauri::plugin::Builder::new("pl-reset")
        .js_init_script(script)
        .build()
}

/// Logs what the start's reset did (the log opens after it ran).
pub fn log(done: &Done) {
    tracing::info!(
        files = done.files.removed_files,
        bytes = done.files.removed_bytes,
        files_in_use = done.files.kept_files,
        secrets = done.secrets,
        secrets_kept = done.secrets_kept,
        "the launcher was reset: accounts, settings and logs removed, the game kept"
    );
}
