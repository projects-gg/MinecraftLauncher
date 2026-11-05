//! Crew discovery ("Ekip bul", CREW-DISCOVER): the listing, joining from its cards and the
//! leader's "Keşif" block. Everything here goes through `pl_social`; the interface shows any of it
//! only while the service lists `discover` in `teamFeatures` (Studio's switch).

use pl_lang::pick;
use pl_social::{DiscoverJoin, DiscoverPage, DiscoverStatus, SocialError};
use tauri::State;

use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

/// What the player is told when discovery went away meanwhile (Studio switched it off).
fn discover_off(error: SocialError) -> CmdError {
    CmdError::new(
        "SOSYAL-KESIF-KAPALI",
        pick("Ekip bul şu an kapalı.", "Find a crew is off right now."),
        error.to_string(),
    )
}

/// Studio's switch went off between opening and acting: one honest sentence.
fn mapped<T>(result: Result<T, SocialError>) -> CmdResult<T> {
    match result {
        Err(error @ SocialError::InvalidInput("crew discovery")) => Err(discover_off(error)),
        Err(error)
            if error
                .code()
                .is_some_and(|code| code.as_str() == "discover_off") =>
        {
            Err(discover_off(error))
        }
        Err(error @ SocialError::InvalidInput("discover text")) => Err(CmdError::new(
            "SOSYAL-KESIF-YAZI",
            pick(
                "Yazı 10 ile 160 karakter arasında olmalı.",
                "The text needs 10 to 160 characters.",
            ),
            error.to_string(),
        )),
        other => Ok(other?),
    }
}

/// The listing, or None while the copy `etag` names is still current. The interface keeps the copy
/// until its `next` and asks again with its `etag` after a quarter of an hour.
#[tauri::command]
#[specta::specta]
pub async fn social_discover_list(
    state: State<'_, AppState>,
    etag: Option<String>,
) -> CmdResult<Option<DiscoverPage>> {
    mapped(state.social.discover_listing(etag).await)
}

/// "Katıl" / "Katılma isteği gönder" on a card: `joined`, `requested`, or `gone` (the card goes).
#[tauri::command]
#[specta::specta]
pub async fn social_discover_join(
    state: State<'_, AppState>,
    team: String,
) -> CmdResult<DiscoverJoin> {
    mapped(state.social.discover_join(&team).await)
}

/// The leader's "Keşif" block of one of their crews.
#[tauri::command]
#[specta::specta]
pub async fn social_discover_status(
    state: State<'_, AppState>,
    team: String,
) -> CmdResult<DiscoverStatus> {
    mapped(state.social.discover_status(&team).await)
}

/// Asks the next wave to list the crew with this text and these tags; answers the block as it is
/// now (a refusal shows in it).
#[tauri::command]
#[specta::specta]
pub async fn social_discover_set(
    state: State<'_, AppState>,
    team: String,
    text: String,
    tags: Vec<String>,
) -> CmdResult<DiscoverStatus> {
    mapped(state.social.set_discover(&team, &text, &tags).await)
}

/// Takes the crew out of discovery at once; None while the service's switch is off.
#[tauri::command]
#[specta::specta]
pub async fn social_discover_off(
    state: State<'_, AppState>,
    team: String,
) -> CmdResult<Option<DiscoverStatus>> {
    mapped(state.social.discover_off(&team).await)
}
