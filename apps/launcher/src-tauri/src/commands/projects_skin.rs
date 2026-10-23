//! SKIN1: Projects skins of username accounts and the game account's link to the player's Discord
//! account (skin1.md). Thin commands over `pl_social::ProjectsSkins`, which emits
//! `projects-skin-changed` (`ProjectsSkinChange`). The entry points follow the signed config's
//! `features.friends` and `features.skins` (debug builds: also `PROJECTS_SOCIAL_SKINS=1`, for a
//! local `wrangler dev`). The file picker is the page's own (`@tauri-apps/plugin-dialog`).
//!
//! Refusals of the friends service are answers (`refused` outcomes), not errors: `CmdError` is
//! only for what never reached it (an unknown account, a failed cancel).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use pl_lang::pick;
use pl_social::{
    AccountsFn, McFailure, ProjectsSkinLook, ProjectsSkinLookup, ProjectsSkinOutcome,
    ProjectsSkinOverview, ProjectsSkinPick, ProjectsSkinSave, ProjectsSkinView, ProjectsSkins,
    SkinAccount, SkinChangeSink, SkinsConfig, SocialService,
};
use tauri::{AppHandle, Emitter, Manager, State};
use uuid::Uuid;

use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

/// Builds SKIN1's backend beside the friends service; the entry points stay hidden until
/// [`apply_flag`] (or the debug override) shows them.
pub fn create(
    app: &AppHandle,
    social: &SocialService,
    http: &pl_net::Http,
    root: &Path,
    runtime: tokio::runtime::Handle,
) -> ProjectsSkins {
    let emitter = app.clone();
    let sink: SkinChangeSink = Arc::new(move |change| {
        let _ = emitter.emit("projects-skin-changed", &change);
    });
    let handle = app.clone();
    // Read when a job closes (its account by name) and once a run (copies of removed accounts).
    let accounts: AccountsFn = Arc::new(move || {
        handle
            .try_state::<AppState>()
            .map(|state| {
                state
                    .auth
                    .accounts()
                    .into_iter()
                    .map(|account| SkinAccount {
                        id: account.id.to_string(),
                        name: account.username,
                    })
                    .collect()
            })
            .unwrap_or_default()
    });
    let skins = ProjectsSkins::new(
        social,
        http.client().clone(),
        SkinsConfig {
            store_dir: root.join("skins").join("projects"),
            versions_dir: root.join("versions"),
            mojang: pl_auth::MojangEndpoints::production(),
        },
        runtime,
        sink,
        accounts,
    );
    skins.set_shown(debug_skins());
    skins
}

/// The entry points show with the signed config's `features.friends` and `features.skins`.
pub fn apply_flag(skins: &ProjectsSkins, friends: bool, shown: bool) {
    skins.set_shown((friends && shown) || debug_skins());
}

/// `PROJECTS_SOCIAL_SKINS=1` (debug builds only): the entry points show whatever the config says,
/// for a local `wrangler dev`.
fn debug_skins() -> bool {
    #[cfg(debug_assertions)]
    {
        std::env::var("PROJECTS_SOCIAL_SKINS").is_ok_and(|value| value == "1")
    }
    #[cfg(not(debug_assertions))]
    {
        false
    }
}

fn account(state: &AppState, account_id: &str) -> CmdResult<SkinAccount> {
    let missing = || {
        CmdError::new(
            "PSKIN-HESAP",
            pick("Hesap bulunamadı.", "Account not found."),
            format!("no account {account_id}"),
        )
    };
    let id = Uuid::parse_str(account_id).map_err(|_| missing())?;
    state
        .auth
        .accounts()
        .into_iter()
        .find(|account| account.id == id)
        .map(|account| SkinAccount {
            id: account.id.to_string(),
            name: account.username,
        })
        .ok_or_else(missing)
}

/// Entry points, the service's switches and the link as last known (from memory).
#[tauri::command]
#[specta::specta]
pub fn projects_skin_overview(state: State<'_, AppState>) -> CmdResult<ProjectsSkinOverview> {
    Ok(state.projects_skin.overview())
}

/// The Görünüm panel's view of a username account: the service read now, this computer's copy.
#[tauri::command]
#[specta::specta]
pub async fn projects_skin_state(
    state: State<'_, AppState>,
    account_id: String,
) -> CmdResult<ProjectsSkinView> {
    let account = account(&state, &account_id)?;
    Ok(state.projects_skin.state(&account).await)
}

/// What the account looks like on Projects without a Projects skin.
#[tauri::command]
#[specta::specta]
pub async fn projects_skin_default(
    state: State<'_, AppState>,
    account_id: String,
) -> CmdResult<ProjectsSkinLook> {
    let account = account(&state, &account_id)?;
    state
        .projects_skin
        .default_look(&account)
        .await
        .ok_or_else(|| {
            CmdError::new(
                "PSKIN-VARSAYILAN",
                pick("Görsel yüklenemedi.", "Couldn't load the picture."),
                "no premium skin and no default skin in a client jar",
            )
        })
}

/// Reads (at most 1 MiB), checks and re-encodes the skin file the player picked.
#[tauri::command]
#[specta::specta]
pub async fn projects_skin_pick_file(
    state: State<'_, AppState>,
    path: String,
) -> CmdResult<ProjectsSkinPick> {
    Ok(state.projects_skin.pick_file(PathBuf::from(path)).await)
}

/// A premium player's skin by name (Mojang; kept 10 minutes).
#[tauri::command]
#[specta::specta]
pub async fn projects_skin_lookup(
    state: State<'_, AppState>,
    account_id: String,
    name: String,
) -> CmdResult<ProjectsSkinLookup> {
    let account = account(&state, &account_id)?;
    Ok(state.projects_skin.lookup(&account, &name).await)
}

/// Saves a picked skin, the default or the one before ("Geri al") as the Projects skin.
#[tauri::command]
#[specta::specta]
pub async fn projects_skin_save(
    state: State<'_, AppState>,
    account_id: String,
    save: ProjectsSkinSave,
) -> CmdResult<ProjectsSkinOutcome> {
    let account = account(&state, &account_id)?;
    Ok(state.projects_skin.save(&account, save).await)
}

/// Cancels a job ProBot has not taken yet; false once it has (`409 job_taken`).
#[tauri::command]
#[specta::specta]
pub async fn projects_skin_cancel(state: State<'_, AppState>, job_id: String) -> CmdResult<bool> {
    state
        .projects_skin
        .cancel(&job_id)
        .await
        .map_err(|failure: Box<McFailure>| {
            CmdError::new(
                "PSKIN-IPTAL",
                pick(
                    "Şu an iptal edilemedi. Biraz sonra yeniden dene.",
                    "Couldn't cancel right now. Try again in a moment.",
                ),
                format!("cancel refused: HTTP {} {}", failure.status, failure.code),
            )
        })
}

/// Links the Discord account to the account's name with the code from `/discord eşle`.
#[tauri::command]
#[specta::specta]
pub async fn projects_link(
    state: State<'_, AppState>,
    account_id: String,
    pin: String,
) -> CmdResult<ProjectsSkinOutcome> {
    let account = account(&state, &account_id)?;
    Ok(state.projects_skin.link(&account, pin.trim()).await)
}

/// Removes the link (the Projects skin stays).
#[tauri::command]
#[specta::specta]
pub async fn projects_unlink(state: State<'_, AppState>) -> CmdResult<ProjectsSkinOutcome> {
    Ok(state.projects_skin.unlink().await)
}

/// Asks ProBot for the link; the answer arrives as `projects-skin-changed`.
#[tauri::command]
#[specta::specta]
pub async fn projects_link_check(state: State<'_, AppState>) -> CmdResult<()> {
    state.projects_skin.link_check().await;
    Ok(())
}
