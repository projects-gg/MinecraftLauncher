//! AUTOJOIN: what "Doğrudan sunucuya bağlan" offers beside the lobby ("play.projects.gg →
//! SkyBlock") and whether the rows Studio marks "Yalnız testçiler" may show. The page keeps the
//! player's pick per instance and names it at launch by its id; the command it stands for comes
//! from the signed config there (`pl_overlay::route_pick`), never from the page.

use serde::Serialize;
use tauri::State;

use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

/// One sub-server as the page offers it (the command stays in Rust).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SubServerView {
    /// The pick the page keeps for the instance.
    pub id: String,
    /// "SkyBlock": the page writes "play.projects.gg → SkyBlock".
    pub name: String,
    /// Shown only while [`sub_servers_tester`] says yes.
    pub testers_only: bool,
}

/// The sub-servers offered for `instance_id`, in Studio's order. None while `features.subServers`
/// is off, the instance joins no Projects server, or the game would not get this launcher's own
/// overlay mod (not Fabric, no jar of ours for its version, the overlay and the chat's emoji picker
/// both off): the mod is what sends the pick's command.
#[tauri::command]
#[specta::specta]
pub async fn instance_sub_servers(
    state: State<'_, AppState>,
    instance_id: String,
) -> CmdResult<Vec<SubServerView>> {
    let instance = state
        .instances
        .get(&instance_id)
        .await
        .map_err(CmdError::from)?;
    let Some(server) = instance.auto_join.as_ref() else {
        return Ok(Vec::new());
    };
    if !crate::overlay::fits(&state, &instance) {
        return Ok(Vec::new());
    }
    let Ok(snapshot) = state.content.load_cached().await else {
        return Ok(Vec::new());
    };
    Ok(snapshot
        .config
        .sub_servers_for(&server.host)
        .into_iter()
        .map(|sub| SubServerView {
            id: sub.id.clone(),
            name: sub.name.clone(),
            testers_only: sub.testers_only,
        })
        .collect())
}

/// Whether this player is in the beta program (BETA-GATE: the friends service answers `member`),
/// for the rows marked "Yalnız testçiler". No while the gate is off, nobody is signed in, the
/// friends feature is off or the service cannot tell; the lobby still applies its own rules.
#[tauri::command]
#[specta::specta]
pub async fn sub_servers_tester(state: State<'_, AppState>) -> CmdResult<bool> {
    Ok(is_tester(&state).await)
}

/// [`sub_servers_tester`] for the launch: a pick marked "Yalnız testçiler" reaches the game only
/// while this says yes.
pub(crate) async fn is_tester(state: &AppState) -> bool {
    matches!(
        state.social.beta_answer().await,
        pl_social::BetaAnswer::Member
    )
}
