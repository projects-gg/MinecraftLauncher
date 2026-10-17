//! Skins and capes of Microsoft accounts: the "Görünüm" panel of Settings → Hesaplar, with its
//! skin history ("Önceki skinler", `pl_auth::SkinHistory`).
//!
//! Textures reach the 3D preview as data URLs downloaded here: Mojang's texture host sends no
//! CORS headers, and WebGL only takes CORS-clean images.

use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use pl_auth::{Account, AuthError, Cape, SkinVariant};
use pl_net::CancellationToken;
use serde::Serialize;
use tauri::State;
use uuid::Uuid;

use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

/// The active skin of an account.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AppearanceSkin {
    /// Texture on textures.minecraft.net (HTTPS).
    pub url: String,
    pub variant: SkinVariant,
    /// One of Mojang's default skins (Steve, Alex and the others) rather than an upload.
    pub is_default: bool,
}

/// What the appearance panel shows for a Microsoft account.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AppearanceView {
    pub account_id: String,
    pub username: String,
    pub skin: Option<AppearanceSkin>,
    /// Capes the profile owns; at most one is `active`.
    pub capes: Vec<Cape>,
    /// Minecraft Services could not be reached: this is the saved copy and may be out of date.
    pub stale: bool,
}

/// A skin PNG the player picked or dropped, checked and ready for the preview.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SkinFilePreview {
    pub path: String,
    /// File name, for the UI.
    pub name: String,
    /// `data:image/png;base64,…`
    pub data_url: String,
    pub width: u32,
    pub height: u32,
}

fn view(account: &Account, stale: bool) -> AppearanceView {
    AppearanceView {
        account_id: account.id.to_string(),
        username: account.username.clone(),
        skin: account.skin.as_ref().map(|skin| AppearanceSkin {
            url: skin.url.clone(),
            variant: skin.variant,
            is_default: skin.is_default(),
        }),
        capes: account.capes.clone(),
        stale,
    }
}

fn parse_id(id: &str) -> CmdResult<Uuid> {
    Uuid::parse_str(id).map_err(|e| CmdError::internal(format!("bad account id {id}: {e}")))
}

fn png_data_url(png: &[u8]) -> String {
    format!("data:image/png;base64,{}", STANDARD.encode(png))
}

/// Skin and capes of a Microsoft account, fresh from Minecraft Services, or the saved copy
/// (`stale`) when they cannot be reached. Offline accounts get an error: skins need Microsoft.
#[tauri::command]
#[specta::specta]
pub async fn skin_appearance(
    state: State<'_, AppState>,
    account_id: String,
) -> CmdResult<AppearanceView> {
    let id = parse_id(&account_id)?;
    let appearance = state.auth.appearance(id, &CancellationToken::new()).await?;
    Ok(view(&appearance.account, appearance.stale))
}

/// A skin or cape texture from textures.minecraft.net as a PNG data URL, for the 3D preview.
/// Skins the history keeps come from their local copy.
#[tauri::command]
#[specta::specta]
pub async fn skin_texture(state: State<'_, AppState>, url: String) -> CmdResult<String> {
    if !pl_auth::is_mojang_texture_url(&url) {
        return Err(CmdError::new(
            "SKIN-ADRES",
            "Görsel yüklenemedi.",
            format!("not a Mojang texture URL: {url}"),
        ));
    }
    if let Some(png) = pl_auth::texture_id(&url).and_then(|id| {
        state
            .auth
            .skin_history()
            .and_then(|history| history.texture(&id))
    }) {
        return Ok(png_data_url(&png));
    }
    let png = state
        .auth
        .download_texture(&url, &CancellationToken::new())
        .await?;
    Ok(png_data_url(&png))
}

/// Reads and checks a skin PNG (64×64, or legacy 64×32, at most 1 MiB) so the panel can preview
/// it before anything is sent to Mojang.
#[tauri::command]
#[specta::specta]
pub async fn skin_file_preview(path: String) -> CmdResult<SkinFilePreview> {
    let file = PathBuf::from(&path);
    let png = pl_auth::read_skin_file(&file).await?;
    let (width, height) = pl_auth::validate_skin_png(&png)
        .map_err(|problem| AuthError::InvalidSkinFile { problem })?;
    let name = file
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(SkinFilePreview {
        path,
        name,
        data_url: png_data_url(&png),
        width,
        height,
    })
}

/// Uploads the skin PNG at `path` (read and checked again here) with the chosen arm model.
#[tauri::command]
#[specta::specta]
pub async fn skin_upload(
    state: State<'_, AppState>,
    account_id: String,
    path: String,
    variant: SkinVariant,
) -> CmdResult<AppearanceView> {
    let id = parse_id(&account_id)?;
    let account = state
        .auth
        .upload_skin_file(id, Path::new(&path), variant, &CancellationToken::new())
        .await?;
    tracing::info!(account = %id, ?variant, "skin uploaded");
    Ok(view(&account, false))
}

/// Switches the arm model of the current skin (classic or slim), keeping its texture.
#[tauri::command]
#[specta::specta]
pub async fn skin_set_variant(
    state: State<'_, AppState>,
    account_id: String,
    variant: SkinVariant,
) -> CmdResult<AppearanceView> {
    let id = parse_id(&account_id)?;
    let account = state
        .auth
        .set_skin_variant(id, variant, &CancellationToken::new())
        .await?;
    Ok(view(&account, false))
}

/// Shows an owned cape, or hides the cape when `cape_id` is `None`.
#[tauri::command]
#[specta::specta]
pub async fn skin_set_cape(
    state: State<'_, AppState>,
    account_id: String,
    cape_id: Option<String>,
) -> CmdResult<AppearanceView> {
    let id = parse_id(&account_id)?;
    let account = state
        .auth
        .set_cape(id, cape_id.as_deref(), &CancellationToken::new())
        .await?;
    Ok(view(&account, false))
}

/// Back to Mojang's default skin.
#[tauri::command]
#[specta::specta]
pub async fn skin_reset(
    state: State<'_, AppState>,
    account_id: String,
) -> CmdResult<AppearanceView> {
    let id = parse_id(&account_id)?;
    let account = state.auth.reset_skin(id, &CancellationToken::new()).await?;
    tracing::info!(account = %id, "skin reset to the default");
    Ok(view(&account, false))
}

// --- skin history ("Önceki skinler") ------------------------------------------------------------

/// One skin of the history, as the "Önceki skinler" grid shows it.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SkinHistoryEntryView {
    /// Key for the history commands; never shown.
    pub id: String,
    /// The texture on textures.minecraft.net, for the 3D preview (`skin_texture` serves the local
    /// copy).
    pub url: String,
    /// The arm model it was last worn with (and is worn with again).
    pub variant: SkinVariant,
    /// When the launcher first saw it (RFC 3339).
    pub first_seen: String,
    /// The last time it was seen worn or put on (RFC 3339).
    pub last_worn: String,
    pub favorite: bool,
    /// The whole player from the front (PNG data URL) once drawn; see `skin_history_prepare`.
    pub thumb_url: Option<String>,
}

/// The skin history of a Microsoft account.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SkinHistoryView {
    /// Favourites first, then the most recently worn.
    pub entries: Vec<SkinHistoryEntryView>,
    /// `id` of the skin the account wears now, when the history has it.
    pub current_id: Option<String>,
}

/// The skins a Microsoft account wore while the launcher knew it (kept on this computer only).
/// Pictures that are not drawn yet come with `skin_history_prepare`.
#[tauri::command]
#[specta::specta]
pub async fn skin_history(
    state: State<'_, AppState>,
    account_id: String,
) -> CmdResult<SkinHistoryView> {
    let id = parse_id(&account_id)?;
    let list = state.auth.skin_history_entries(id)?;
    let history = state.auth.skin_history();
    let entries = list
        .entries
        .into_iter()
        .map(|entry| SkinHistoryEntryView {
            url: format!(
                "https://{}/texture/{}",
                pl_auth::TEXTURE_HOST,
                entry.texture_id
            ),
            thumb_url: history
                .and_then(|history| history.thumbnail(&entry.texture_id, entry.variant))
                .map(|png| png_data_url(&png)),
            variant: entry.variant,
            first_seen: entry.first_seen.to_string(),
            last_worn: entry.last_worn.to_string(),
            favorite: entry.favorite,
            id: entry.texture_id,
        })
        .collect();
    Ok(SkinHistoryView {
        entries,
        current_id: list.current,
    })
}

/// Downloads the history's missing textures from Mojang's public texture host (no credentials)
/// and draws the missing pictures. Returns how many were drawn; the panel reads the history
/// again afterwards either way.
#[tauri::command]
#[specta::specta]
pub async fn skin_history_prepare(
    state: State<'_, AppState>,
    account_id: String,
) -> CmdResult<u32> {
    let id = parse_id(&account_id)?;
    let cancel = CancellationToken::new();
    let prepared = tokio::time::timeout(
        Duration::from_secs(60),
        state.auth.prepare_skin_history(id, &cancel),
    )
    .await;
    match prepared {
        Ok(made) => Ok(made?),
        Err(_) => {
            cancel.cancel();
            tracing::debug!(account = %id, "skin history pictures took too long");
            Ok(0)
        }
    }
}

/// Puts a skin of the history back on, with the arm model it was last worn with.
#[tauri::command]
#[specta::specta]
pub async fn skin_history_wear(
    state: State<'_, AppState>,
    account_id: String,
    entry_id: String,
) -> CmdResult<AppearanceView> {
    let id = parse_id(&account_id)?;
    let account = state
        .auth
        .wear_history_skin(id, &entry_id, &CancellationToken::new())
        .await?;
    Ok(view(&account, false))
}

/// Marks a skin of the history as a favourite (listed first, never pruned) or unmarks it.
#[tauri::command]
#[specta::specta]
pub async fn skin_history_set_favorite(
    state: State<'_, AppState>,
    account_id: String,
    entry_id: String,
    favorite: bool,
) -> CmdResult<()> {
    let id = parse_id(&account_id)?;
    state.auth.set_skin_favorite(id, &entry_id, favorite)?;
    Ok(())
}

/// Removes a skin from the history; `skin_history_restore` brings it back for a few minutes.
#[tauri::command]
#[specta::specta]
pub async fn skin_history_remove(
    state: State<'_, AppState>,
    account_id: String,
    entry_id: String,
) -> CmdResult<()> {
    let id = parse_id(&account_id)?;
    state.auth.forget_skin(id, &entry_id)?;
    Ok(())
}

/// Undo of `skin_history_remove`.
#[tauri::command]
#[specta::specta]
pub async fn skin_history_restore(
    state: State<'_, AppState>,
    account_id: String,
    entry_id: String,
) -> CmdResult<()> {
    let id = parse_id(&account_id)?;
    state.auth.restore_skin(id, &entry_id)?;
    Ok(())
}
