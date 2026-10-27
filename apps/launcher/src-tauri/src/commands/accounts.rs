use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use pl_auth::{Account, AccountKind, HeadCache, LoginStep, SessionCheck};
use pl_net::CancellationToken;
use pl_social::{ProjectsSkins, SkinAccount};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;
use uuid::Uuid;

use crate::error::{CmdError, CmdResult};
use crate::state::AppState;
use pl_lang::pick;

/// Account as the UI shows it (no secrets ever cross this boundary).
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AccountView {
    pub id: String,
    pub kind: AccountKind,
    pub username: String,
    /// Minecraft profile UUID without dashes.
    pub uuid: String,
    pub active: bool,
    pub skin_url: Option<String>,
    /// The player's head (8×8 PNG data URL) once it is cached; see `accounts_ensure_heads`.
    pub head_url: Option<String>,
    /// When the account was added (RFC 3339).
    pub added_at: String,
    /// Last time the game was started with it (RFC 3339).
    pub last_used_at: Option<String>,
    /// Microsoft refused the saved session: a new sign-in is needed.
    pub needs_relogin: bool,
    /// Pinned: listed first, in the player's order.
    pub pinned: bool,
    /// Bundled picture shown instead of the head (`preset:<slug>`, offline accounts).
    pub avatar: Option<String>,
}

fn head_cache(state: &AppState) -> HeadCache {
    HeadCache::new(state.root.join("cache").join("heads"))
}

/// Client jars that may hold the default skins (`<root>/versions/<id>/<id>.jar`).
fn client_jars(state: &AppState) -> Vec<PathBuf> {
    pl_auth::client_jars(&state.root.join("versions"))
}

/// The account's face: the head of its Projects skin while SKIN1 shows (a username account's
/// applied skin, kept on this computer; skin1.md §5.6), else its own head once cached. A preset
/// picture the player chose still wins in the interface (`avatar`).
fn face_png(account: &Account, heads: &HeadCache, projects: &ProjectsSkins) -> Option<Vec<u8>> {
    let projects_head = if matches!(account.kind, AccountKind::Offline) {
        let skin = projects.head_skin(&SkinAccount {
            id: account.id.to_string(),
            name: account.username.clone(),
        });
        skin.and_then(|skin| pl_auth::head_png(&skin).ok())
    } else {
        None
    };
    projects_head.or_else(|| heads.for_account(account))
}

fn view(
    account: &Account,
    active_id: Option<Uuid>,
    heads: &HeadCache,
    projects: &ProjectsSkins,
) -> AccountView {
    AccountView {
        id: account.id.to_string(),
        kind: account.kind,
        username: account.username.clone(),
        uuid: account.uuid.simple().to_string(),
        active: Some(account.id) == active_id,
        skin_url: account.skin.as_ref().map(|s| s.url.clone()),
        head_url: face_png(account, heads, projects)
            .map(|png| format!("data:image/png;base64,{}", STANDARD.encode(png))),
        added_at: account.added_at.to_string(),
        last_used_at: account.last_used_at.map(|at| at.to_string()),
        needs_relogin: account.needs_relogin,
        pinned: account.pinned,
        avatar: account.avatar.clone(),
    }
}

fn view_of(state: &AppState, account: &Account) -> AccountView {
    view(
        account,
        state.auth.store().active_id(),
        &head_cache(state),
        &state.projects_skin,
    )
}

fn parse_id(id: &str) -> CmdResult<Uuid> {
    Uuid::parse_str(id).map_err(|e| CmdError::internal(format!("bad account id {id}: {e}")))
}

#[tauri::command]
#[specta::specta]
pub fn accounts_list(state: State<'_, AppState>) -> Vec<AccountView> {
    let active = state.auth.store().active_id();
    let heads = head_cache(&state);
    state
        .auth
        .accounts()
        .iter()
        .map(|a| view(a, active, &heads, &state.projects_skin))
        .collect()
}

#[tauri::command]
#[specta::specta]
pub async fn account_add_offline(
    state: State<'_, AppState>,
    name: String,
) -> CmdResult<AccountView> {
    let account = state.auth.add_offline(&name).await?;
    Ok(view_of(&state, &account))
}

/// What a Microsoft sign-in did, so the UI can say it plainly.
#[derive(Debug, Clone, Copy, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum MicrosoftLoginOutcome {
    /// A new account, now active.
    Added,
    /// The account was already saved: its session was renewed and it is active.
    AlreadyAdded,
    /// The account asked for (`relogin`) has a working session again.
    Relogged,
    /// A sign-in for `relogin` came back with another Microsoft account, which was saved
    /// (or updated) on its own; the selection did not change.
    OtherAccount,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MicrosoftLoginResult {
    pub account: AccountView,
    pub outcome: MicrosoftLoginOutcome,
}

/// Debug builds started with `PROJECTS_E2E_NO_MICROSOFT=1` (end-to-end test runs) refuse every
/// way a real Microsoft account gets in, at once and before a browser opens (`pl_auth::e2e`;
/// release builds never read the variable).
pub(crate) fn refuse_microsoft_in_tests() -> CmdResult<()> {
    if !pl_auth::e2e::microsoft_blocked() {
        return Ok(());
    }
    tracing::warn!(
        variable = pl_auth::e2e::NO_MICROSOFT_ENV,
        "Microsoft sign-in refused in a test run"
    );
    Err(CmdError::new(
        "E2E-MICROSOFT",
        pick(
            "Bu test ortamında Microsoft ile giriş kapalı.",
            "Signing in with Microsoft is turned off in this test setup.",
        ),
        format!("{}=1 (debug build)", pl_auth::e2e::NO_MICROSOFT_ENV),
    ))
}

/// Browser sign-in. Progress steps stream over `on_step`. Without `relogin` the account is added
/// (or renewed) and becomes active; with `relogin` (an account id) the sign-in renews that
/// account's session and keeps the current selection.
#[tauri::command]
#[specta::specta]
pub async fn account_login_microsoft(
    app: AppHandle,
    state: State<'_, AppState>,
    on_step: Channel<LoginStep>,
    relogin: Option<String>,
) -> CmdResult<MicrosoftLoginResult> {
    refuse_microsoft_in_tests()?;
    let target = relogin.as_deref().map(parse_id).transpose()?;
    let cancel = CancellationToken::new();
    if let Some(previous) = state.login_cancel.lock().replace(cancel.clone()) {
        previous.cancel();
    }
    let opener = app.clone();
    let open_url: pl_auth::OpenUrlFn = Arc::new(move |url: &str| {
        if let Err(error) = opener.opener().open_url(url, None::<&str>) {
            tracing::warn!(%error, "could not open the system browser");
        }
    });
    let progress: pl_auth::LoginProgressFn = Arc::new(move |step| {
        let _ = on_step.send(step);
    });
    let result = state
        .auth
        .sign_in_microsoft(open_url, progress, &cancel, target.is_none())
        .await;
    state.login_cancel.lock().take();
    let signed_in = result?;
    let outcome = match target {
        None if signed_in.created => MicrosoftLoginOutcome::Added,
        None => MicrosoftLoginOutcome::AlreadyAdded,
        Some(id) if id == signed_in.account.id => MicrosoftLoginOutcome::Relogged,
        Some(_) => MicrosoftLoginOutcome::OtherAccount,
    };
    Ok(MicrosoftLoginResult {
        account: view_of(&state, &signed_in.account),
        outcome,
    })
}

#[tauri::command]
#[specta::specta]
pub fn account_cancel_login(state: State<'_, AppState>) {
    if let Some(cancel) = state.login_cancel.lock().take() {
        cancel.cancel();
    }
}

#[tauri::command]
#[specta::specta]
pub fn account_set_active(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.auth.set_active(parse_id(&id)?)?;
    Ok(())
}

/// Renames an offline account (its UUID follows the name).
#[tauri::command]
#[specta::specta]
pub async fn account_rename(
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> CmdResult<AccountView> {
    let account = state.auth.rename_offline(parse_id(&id)?, &name).await?;
    Ok(view_of(&state, &account))
}

/// After a removal: which account is active now.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AccountRemoval {
    /// Another account when the removed one was active; `None` when none is left.
    pub active_id: Option<String>,
}

/// Removes an account; its sign-in leaves the keychain at once. `account_undo_remove` can bring
/// it back for a while, `account_forget_removed` makes it final sooner.
#[tauri::command]
#[specta::specta]
pub async fn account_remove(state: State<'_, AppState>, id: String) -> CmdResult<AccountRemoval> {
    let active = state.auth.remove(parse_id(&id)?).await?;
    Ok(AccountRemoval {
        active_id: active.map(|id| id.to_string()),
    })
}

#[tauri::command]
#[specta::specta]
pub async fn account_undo_remove(state: State<'_, AppState>, id: String) -> CmdResult<AccountView> {
    let account = state.auth.undo_remove(parse_id(&id)?).await?;
    Ok(view_of(&state, &account))
}

/// The undo window of a removal ended: drop the copy kept for it.
#[tauri::command]
#[specta::specta]
pub fn account_forget_removed(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.auth.forget_removed(parse_id(&id)?);
    Ok(())
}

/// Pins an account to the top of the list (it joins the end of the pinned ones) or unpins it.
#[tauri::command]
#[specta::specta]
pub fn account_set_pinned(
    state: State<'_, AppState>,
    id: String,
    pinned: bool,
) -> CmdResult<AccountView> {
    let account = state.auth.set_pinned(parse_id(&id)?, pinned)?;
    Ok(view_of(&state, &account))
}

/// Saves the player's order of the pinned accounts (unpinned ids are ignored).
#[tauri::command]
#[specta::specta]
pub fn accounts_reorder_pinned(state: State<'_, AppState>, ids: Vec<String>) -> CmdResult<()> {
    let ids = ids
        .iter()
        .map(|id| parse_id(id))
        .collect::<CmdResult<Vec<_>>>()?;
    state.auth.reorder_pinned(&ids)?;
    Ok(())
}

/// Shows a bundled picture (`preset:<slug>`) instead of an offline account's head; `None`
/// goes back to the head.
#[tauri::command]
#[specta::specta]
pub fn account_set_avatar(
    state: State<'_, AppState>,
    id: String,
    avatar: Option<String>,
) -> CmdResult<AccountView> {
    let account = state.auth.set_avatar(parse_id(&id)?, avatar.as_deref())?;
    Ok(view_of(&state, &account))
}

/// Whether a Microsoft account's saved session still works (refreshes it when it is about to
/// expire, like a launch would). A working session also re-reads the profile when it was not
/// read in the last quarter of an hour, so a skin or name changed elsewhere reaches the list
/// and the skin history.
#[tauri::command]
#[specta::specta]
pub async fn account_check_session(
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<SessionCheck> {
    let id = parse_id(&id)?;
    let cancel = CancellationToken::new();
    let check = state.auth.check_session(id, &cancel).await?;
    if check == SessionCheck::Valid
        && let Err(error) = state.auth.sync_profile(id, &cancel).await
    {
        tracing::debug!(account = %id, code = %error.code(), "profile not re-read");
    }
    Ok(check)
}

/// Cuts and caches the heads that are missing: Mojang textures for Microsoft accounts, default
/// skins from local client jars for offline ones. Returns how many were added; the list
/// (`accounts_list`) then carries them.
#[tauri::command]
#[specta::specta]
pub async fn accounts_ensure_heads(state: State<'_, AppState>) -> CmdResult<u32> {
    let heads = head_cache(&state);
    let missing: Vec<Account> = state
        .auth
        .accounts()
        .into_iter()
        .filter(|account| {
            pl_auth::HeadSource::for_account(account).is_some()
                && heads.for_account(account).is_none()
        })
        .collect();
    if missing.is_empty() {
        return Ok(0);
    }
    let jars = client_jars(&state);
    let cancel = CancellationToken::new();
    let mut added = 0;
    for account in missing {
        let made = tokio::time::timeout(
            Duration::from_secs(20),
            state.auth.head(account.id, &heads, &jars, &cancel),
        )
        .await;
        match made {
            Ok(Ok(Some(_))) => added += 1,
            Ok(Ok(None)) => {}
            Ok(Err(error)) => {
                tracing::debug!(account = %account.id, code = %error.code(), "head not made");
            }
            Err(_) => tracing::debug!(account = %account.id, "head download timed out"),
        }
    }
    Ok(added)
}

/// Result of importing the old launcher's premium session (Windows only).
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum LegacyAccountImport {
    #[cfg_attr(not(windows), allow(dead_code))]
    Imported { account: AccountView },
    #[cfg_attr(not(windows), allow(dead_code))]
    NotFound,
    #[cfg_attr(not(windows), allow(dead_code))]
    Unreadable,
    #[cfg_attr(windows, allow(dead_code))]
    Unsupported,
}

#[tauri::command]
#[specta::specta]
pub async fn account_import_legacy(state: State<'_, AppState>) -> CmdResult<LegacyAccountImport> {
    refuse_microsoft_in_tests()?;
    #[cfg(windows)]
    {
        use pl_auth::LegacyImport;
        // Where the v1 scan looks (a pretend home in debug builds: `commands::import::locations`).
        let imported = match crate::commands::migrate::legacy_premium_file() {
            Some(path) => state.auth.import_legacy_premium_from(&path).await?,
            None => LegacyImport::NotFound,
        };
        Ok(match imported {
            LegacyImport::Imported { account } => LegacyAccountImport::Imported {
                account: view_of(&state, &account),
            },
            LegacyImport::NotFound => LegacyAccountImport::NotFound,
            LegacyImport::Unreadable { reason } => {
                tracing::warn!(%reason, "legacy premium session unreadable");
                LegacyAccountImport::Unreadable
            }
        })
    }
    #[cfg(not(windows))]
    {
        let _ = state;
        Ok(LegacyAccountImport::Unsupported)
    }
}
