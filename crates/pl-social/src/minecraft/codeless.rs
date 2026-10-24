//! SKIN1 §3.7, codeless linking in game: the permit the launcher hands the in-game overlay
//! (`POST`/`DELETE /v1/mc/intents`). The overlay's session asks for it only when a Projects server
//! asked the game on `projects:link` (`docs/tasks/skin1-o.md`), so a game session needs one at
//! most a few times; the service allows 20 a day.
//!
//! A permit is a secret of this computer and its game: it never goes to a log.

use super::wire::{FEATURE_MC_CODELESS, McIntent};
use crate::service::SocialService;

impl SocialService {
    /// The player is signed in to friends and the service offers codeless linking now
    /// (`mcCodeless` among the friend list's features: Studio has it on). The service still
    /// refuses a permit for an account that is linked, said "Hayır" lately, or while it is paused.
    pub fn mc_codeless_offered(&self) -> bool {
        self.mc_session().is_some()
            && self
                .friends_features()
                .is_some_and(|features| features.iter().any(|f| f == FEATURE_MC_CODELESS))
    }

    /// A permit for the game account `name` (30 minutes, once; a newer one replaces it). None
    /// when not signed in, refused (`already_linked`, `declined`, `rate_limited`, `unavailable`)
    /// or unreachable: the game then answers the server that it has none, and `/discord eşle`
    /// shows its code.
    pub async fn mc_codeless_permit(&self, name: &str) -> Option<McIntent> {
        let (session, generation) = self.mc_session()?;
        match session.mc_open_intent(name).await {
            Ok(intent) => Some(intent),
            Err(failure) => {
                if failure.unauthorized() {
                    self.mc_unauthorized(generation);
                }
                tracing::info!(
                    status = failure.status,
                    code = %failure.code,
                    "no codeless linking permit"
                );
                None
            }
        }
    }

    /// The permit stops working (the game that held it closed). Nothing happens when none is open.
    pub async fn mc_codeless_close(&self) {
        let Some((session, generation)) = self.mc_session() else {
            return;
        };
        if let Err(failure) = session.mc_close_intent().await {
            if failure.unauthorized() {
                self.mc_unauthorized(generation);
            }
            tracing::debug!(
                status = failure.status,
                "the codeless linking permit was not closed"
            );
        }
    }
}
