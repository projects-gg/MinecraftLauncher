use std::sync::Arc;

use pl_auth::{Account, AccountKind, LoginStep};
use pl_net::CancellationToken;
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;
use uuid::Uuid;

use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

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
}

fn view(account: &Account, active_id: Option<Uuid>) -> AccountView {
    AccountView {
        id: account.id.to_string(),
        kind: account.kind,
        username: account.username.clone(),
        uuid: account.uuid.simple().to_string(),
        active: Some(account.id) == active_id,
        skin_url: account.skin.as_ref().map(|s| s.url.clone()),
    }
}

fn parse_id(id: &str) -> CmdResult<Uuid> {
    Uuid::parse_str(id).map_err(|e| CmdError::internal(format!("bad account id {id}: {e}")))
}

#[tauri::command]
#[specta::specta]
pub fn accounts_list(state: State<'_, AppState>) -> Vec<AccountView> {
    let active = state.auth.active_account().map(|a| a.id);
    state
        .auth
        .accounts()
        .iter()
        .map(|a| view(a, active))
        .collect()
}

#[tauri::command]
#[specta::specta]
pub async fn account_add_offline(
    state: State<'_, AppState>,
    name: String,
) -> CmdResult<AccountView> {
    let account = state.auth.add_offline(&name).await?;
    Ok(view(&account, Some(account.id)))
}

/// Browser sign-in. Progress steps stream over `on_step`; the result is the new active account.
#[tauri::command]
#[specta::specta]
pub async fn account_login_microsoft(
    app: AppHandle,
    state: State<'_, AppState>,
    on_step: Channel<LoginStep>,
) -> CmdResult<AccountView> {
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
        .login_microsoft(open_url, progress, &cancel)
        .await;
    state.login_cancel.lock().take();
    let account = result?;
    Ok(view(&account, Some(account.id)))
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

#[tauri::command]
#[specta::specta]
pub async fn account_remove(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.auth.remove(parse_id(&id)?).await?;
    Ok(())
}

/// Result of importing the old launcher's premium session (Windows only).
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum LegacyAccountImport {
    Imported { account: AccountView },
    NotFound,
    Unreadable,
    #[cfg_attr(windows, allow(dead_code))]
    Unsupported,
}

#[tauri::command]
#[specta::specta]
pub async fn account_import_legacy(state: State<'_, AppState>) -> CmdResult<LegacyAccountImport> {
    #[cfg(windows)]
    {
        use pl_auth::LegacyImport;
        Ok(match state.auth.import_legacy_premium().await? {
            LegacyImport::Imported { account } => LegacyAccountImport::Imported {
                account: view(&account, state.auth.active_account().map(|a| a.id)),
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
