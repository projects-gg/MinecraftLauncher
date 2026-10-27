//! The cloud features' calls (CAFK-FEAT phase 1): the inventory's stream and moves, the account's
//! inventory choice, the console download, and copying one account's settings to others ("Ayarları
//! kopyala"); movement's stream and operations (README "Movement"). Nothing here polls: the
//! inventory and movement stream only while their panel is on screen.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::client::{Bearer, LogQuery};
use crate::config::{
    BuildError, CloudEditConfig, CloudRowText, SealContext, build, copies_after, edit_config,
    fingerprints,
};
use crate::error::{CloudError, Result};
use crate::feat::{
    CloudInvError, CloudLogRange, PlaceholderRules, check_inv_op, console_text, is_inv_op_id,
};
use crate::model::{AccountPatch, CloudAccount, CloudConsoleLine, CloudLimits};
use crate::movement::{CloudMovementError, check_move_op};
use crate::service::{CloudEdit, CloudSaveOutcome, CloudService};

/// A console download reads at most this many pages of 1,000 lines.
const MAX_LOG_PAGES: usize = 40;

/// A secret row of the source this computer has no words for: the player types them before
/// "Ayarları kopyala" (once for all, or per account). `place` is `commands` or `rules`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CloudCopyRow {
    pub row: String,
    pub place: String,
    /// `/login ******`.
    pub preview: String,
}

/// What copying from an account needs: its rows to type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CloudCopySource {
    pub id: String,
    pub to_type: Vec<CloudCopyRow>,
}

/// The words typed for one secret row of the source: for one target (`target`) or for all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CloudCopySecret {
    pub row: String,
    pub target: Option<String>,
    pub text: String,
}

/// How copying to one account ended: the account as stored now, or the service's refusal (the
/// launcher words it).
#[derive(Debug)]
pub enum CloudCopyOutcome {
    Copied(Box<CloudAccount>),
    Failed(CloudError),
    /// The copied settings do not fit this account (a limit, a placeholder rule): its own settings
    /// stay as they were.
    Unfit,
}

impl CloudService {
    /// An inventory view is on screen: the account's inventory streams with its console.
    pub async fn watch_inventory(&self, token: &Bearer, id: &str) -> Result<()> {
        let account = self.account(token, id).await?;
        let Some(host) = account.host.clone() else {
            return Err(CloudError::Invalid("no machine yet"));
        };
        let url = self.inner.client.live_url(&host);
        self.inner
            .live
            .watch_inventory(url, token.clone(), &host, id);
        Ok(())
    }

    /// The inventory view left the screen.
    pub fn unwatch_inventory(&self, id: &str) {
        self.inner.live.unwatch_inventory(id);
    }

    /// The movement panel is on screen: the account's movement state and position stream with its
    /// console.
    pub async fn watch_movement(&self, token: &Bearer, id: &str) -> Result<()> {
        let account = self.account(token, id).await?;
        let Some(host) = account.host.clone() else {
            return Err(CloudError::Invalid("no machine yet"));
        };
        let url = self.inner.client.live_url(&host);
        self.inner
            .live
            .watch_movement(url, token.clone(), &host, id);
        Ok(())
    }

    /// The movement panel left the screen.
    pub fn unwatch_movement(&self, id: &str) {
        self.inner.live.unwatch_movement(id);
    }

    /// A movement operation: `op` is the app's `MoveOp` as JSON, `op_id` its own id. None: sent,
    /// the answer follows as `moveResult`; Some: refused here (its shape, or no socket:
    /// `host_offline`).
    pub fn movement_op(&self, id: &str, op_id: &str, op: &str) -> Option<CloudMovementError> {
        if !is_inv_op_id(op_id) || op.len() > 768 {
            return Some(CloudMovementError::invalid("id", None));
        }
        let Ok(value) = serde_json::from_str::<Value>(op) else {
            return Some(CloudMovementError::invalid("type", None));
        };
        if let Err(problem) = check_move_op(&value) {
            return Some(problem);
        }
        (!self.inner.live.movement_op(id, op_id, &value))
            .then(|| CloudMovementError::new("host_offline"))
    }

    /// A move: `op` is the app's `InvOp` as JSON, `op_id` its own id. None: sent, the answer
    /// follows as `invResult`; Some: refused here (its shape, or no socket: `host_offline`).
    pub fn inv_op(&self, id: &str, op_id: &str, op: &str) -> Option<CloudInvError> {
        let invalid = |reason: &str| CloudInvError {
            code: "invalid".into(),
            reason: Some(reason.into()),
            ..CloudInvError::default()
        };
        if !is_inv_op_id(op_id) || op.len() > 768 {
            return Some(invalid("id"));
        }
        let Ok(value) = serde_json::from_str::<Value>(op) else {
            return Some(invalid("type"));
        };
        if let Err(problem) = check_inv_op(&value) {
            return Some(problem);
        }
        (!self.inner.live.inv_op(id, op_id, &value)).then(|| CloudInvError::new("host_offline"))
    }

    /// Turns the inventory view on or off for the account. It applies at the client's next start
    /// (the service never restarts a client for it): "Şimdi yeniden bağla" is a restart.
    pub async fn set_inventory(
        &self,
        token: &Bearer,
        id: &str,
        enabled: bool,
    ) -> Result<CloudSaveOutcome> {
        let current = self.fresh_account(token, id).await?;
        let owner = self.owner(token).await?;
        let copies = self.load_copies(&owner, id).await?;
        let mut config = edit_config(&current.config, &copies);
        config.inventory = Some(enabled);
        let edit = CloudEdit {
            rev: current.rev,
            name: None,
            server: None,
            version: None,
            lang: None,
            config: Some(config),
        };
        self.save(token, id, edit).await
    }

    /// "İndir": the masked log of `range` up to `now`, oldest first, as the contract's `.txt`
    /// (`lang` `en` for English kind names). None: no line in that range (no empty file).
    pub async fn console_download(
        &self,
        token: &Bearer,
        id: &str,
        range: CloudLogRange,
        lang: &str,
        now: i64,
    ) -> Result<Option<String>> {
        let mut lines: Vec<CloudConsoleLine> = Vec::new();
        let mut query = LogQuery {
            from: Some(range.from(now)),
            to: Some(now),
            limit: Some(1000),
            ..LogQuery::default()
        };
        for _ in 0..MAX_LOG_PAGES {
            let page = self.inner.client.logs(token, id, &query).await?;
            let empty = page.lines.is_empty();
            lines.extend(page.lines);
            match page.next {
                Some(next) if !empty => query.after = Some(next),
                _ => break,
            }
        }
        Ok((!lines.is_empty()).then(|| console_text(&lines, lang)))
    }

    /// The source of "Ayarları kopyala": its secret rows this computer has no words for.
    pub async fn copy_source(&self, token: &Bearer, id: &str) -> Result<CloudCopySource> {
        let source = self.fresh_account(token, id).await?;
        let owner = self.owner(token).await?;
        let copies = self.load_copies(&owner, id).await?;
        let config = edit_config(&source.config, &copies);
        let sealed = |place: &str, row: &str, words: &CloudRowText| match words {
            CloudRowText::Sealed { preview } => Some(CloudCopyRow {
                row: row.to_owned(),
                place: place.to_owned(),
                preview: preview.clone(),
            }),
            _ => None,
        };
        let to_type = config
            .commands
            .iter()
            .filter_map(|c| sealed("commands", &c.id, &c.words))
            .chain(
                config
                    .rules
                    .iter()
                    .filter_map(|r| sealed("rules", &r.id, &r.reply)),
            )
            .collect();
        Ok(CloudCopySource {
            id: id.to_owned(),
            to_type,
        })
    }

    /// "Ayarları kopyala": the source's owner name, anti-AFK, commands and replies (with its server
    /// and version when `with_server`) onto each target, one change each, in order. Never its name,
    /// language or inventory choice. Every secret row is sealed again for its target: from this
    /// computer's copy, else from what the player typed (`secrets`). A change made elsewhere
    /// meanwhile is read again and tried once more.
    pub async fn copy_settings(
        &self,
        token: &Bearer,
        source_id: &str,
        targets: &[String],
        with_server: bool,
        secrets: &[CloudCopySecret],
    ) -> Result<Vec<(String, CloudCopyOutcome)>> {
        let source = self.fresh_account(token, source_id).await?;
        let owner = self.owner(token).await?;
        let copies = self.load_copies(&owner, source_id).await?;
        let base = edit_config(&source.config, &copies);
        let offer = self.offer(false).await;
        let placeholders = PlaceholderRules::of(offer.as_ref().and_then(|o| o.features.as_ref()));
        let limits = offer.map(|offer| offer.limits).unwrap_or_default();
        let mut results = Vec::with_capacity(targets.len());
        for target in targets.iter().filter(|t| t.as_str() != source_id) {
            let job = CopyJob {
                owner: &owner,
                source: &source,
                base: &base,
                with_server,
                secrets,
                limits: &limits,
                placeholders: &placeholders,
            };
            let outcome = match self.copy_to(token, &job, target).await {
                Ok(outcome) => outcome,
                Err(error) => CloudCopyOutcome::Failed(error),
            };
            results.push((target.clone(), outcome));
        }
        Ok(results)
    }

    async fn copy_to(
        &self,
        token: &Bearer,
        job: &CopyJob<'_>,
        id: &str,
    ) -> Result<CloudCopyOutcome> {
        let Some(config) = job.config_for(id) else {
            return Ok(CloudCopyOutcome::Unfit);
        };
        for attempt in 0..2 {
            let target = self.fresh_account(token, id).await?;
            let server = if job.with_server {
                job.source.server.clone()
            } else {
                target.server.clone()
            };
            let server_changed = server != target.server;
            let key = self.vault_key().await?;
            let ctx = SealContext {
                key: &key,
                discord_id: job.owner,
                account_id: id,
                server: &server,
                server_changed,
            };
            let before = self.load_copies(job.owner, id).await?;
            let built = match build(
                &config,
                job.limits,
                job.placeholders,
                &ctx,
                &before,
                &fingerprints(&target.config),
            ) {
                Ok(built) => built,
                Err(BuildError::Invalid(_)) => return Ok(CloudCopyOutcome::Unfit),
                Err(BuildError::Seal(error)) => return Err(CloudError::Seal(error.to_string())),
            };
            let patch = AccountPatch {
                server: server_changed.then(|| server.clone()),
                version: (job.with_server && job.source.version != target.version)
                    .then(|| job.source.version.clone()),
                config: Some(built.config.clone()),
                ..AccountPatch::default()
            };
            match self
                .inner
                .client
                .patch_account(token, id, target.rev, &patch)
                .await
            {
                Ok(account) => {
                    self.remember(&account);
                    let after = copies_after(built, &before, &account.config);
                    self.save_copies(job.owner, id, after).await;
                    return Ok(CloudCopyOutcome::Copied(Box::new(account)));
                }
                Err(error) if error.code() == "rev_conflict" && attempt == 0 => continue,
                Err(error) => return Ok(CloudCopyOutcome::Failed(error)),
            }
        }
        Ok(CloudCopyOutcome::Failed(CloudError::Invalid(
            "changed elsewhere",
        )))
    }
}

/// One "Ayarları kopyala" run's shared parts.
struct CopyJob<'a> {
    owner: &'a str,
    source: &'a CloudAccount,
    /// The source's settings as the editor holds them (secret rows with this computer's words).
    base: &'a CloudEditConfig,
    with_server: bool,
    secrets: &'a [CloudCopySecret],
    limits: &'a CloudLimits,
    placeholders: &'a PlaceholderRules,
}

impl CopyJob<'_> {
    /// The source's settings for target `id`: every secret row with words to seal for it (this
    /// computer's copy, or what the player typed for this target, else for all); None when a row's
    /// words are missing. The target keeps its own inventory and movement choices.
    fn config_for(&self, id: &str) -> Option<CloudEditConfig> {
        let typed = |row: &str| {
            self.secrets
                .iter()
                .find(|s| s.row == row && s.target.as_deref() == Some(id))
                .or_else(|| {
                    self.secrets
                        .iter()
                        .find(|s| s.row == row && s.target.is_none())
                })
                .map(|s| s.text.clone())
        };
        let words = |row: &str, words: &CloudRowText| -> Option<CloudRowText> {
            Some(match words {
                CloudRowText::Plain { text } => CloudRowText::Plain { text: text.clone() },
                CloudRowText::Secret { text, .. } => CloudRowText::Secret {
                    text: text.clone(),
                    kept: false,
                },
                CloudRowText::Sealed { .. } => CloudRowText::Secret {
                    text: typed(row).filter(|t| !t.trim().is_empty())?,
                    kept: false,
                },
            })
        };
        let mut config = self.base.clone();
        for command in &mut config.commands {
            command.words = words(&command.id, &command.words)?;
        }
        for rule in &mut config.rules {
            rule.reply = words(&rule.id, &rule.reply)?;
        }
        config.inventory = None;
        config.movement = None;
        Some(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CloudEditCommand, CloudEditRule};
    use crate::model::{CloudAntiAfk, CloudServer};

    fn source() -> CloudAccount {
        CloudAccount {
            id: "a_1".into(),
            name: "Ahmet_AFK".into(),
            server: CloudServer::new("play.projects.gg", 0),
            version: "26.3".into(),
            ..CloudAccount::default()
        }
    }

    fn base() -> CloudEditConfig {
        CloudEditConfig {
            owner: "Ahmet".into(),
            anti_afk: CloudAntiAfk {
                enabled: true,
                delay_min: 60,
                delay_max: 90,
            },
            commands: vec![
                CloudEditCommand {
                    id: "c1".into(),
                    enabled: true,
                    on: "join".into(),
                    delay: Some(2),
                    every: None,
                    at: None,
                    words: CloudRowText::Sealed {
                        preview: "/login ******".into(),
                    },
                },
                CloudEditCommand {
                    id: "c2".into(),
                    enabled: true,
                    on: "join".into(),
                    delay: Some(3),
                    every: None,
                    at: None,
                    words: CloudRowText::Secret {
                        text: "/is home".into(),
                        kept: true,
                    },
                },
            ],
            rules: vec![CloudEditRule {
                id: "r1".into(),
                enabled: true,
                pattern: "selam".into(),
                regex: false,
                from: "player".into(),
                owner_only: false,
                cooldown: 30,
                reply: CloudRowText::Plain {
                    text: "Selam!".into(),
                },
            }],
            inventory: Some(true),
            movement: Some(true),
        }
    }

    #[test]
    fn a_copy_seals_every_secret_for_its_target_and_keeps_its_inventory_choice() {
        let source = source();
        let base = base();
        let limits = CloudLimits::default();
        let off = PlaceholderRules::off();
        let secrets = vec![
            CloudCopySecret {
                row: "c1".into(),
                target: None,
                text: "/login ortak".into(),
            },
            CloudCopySecret {
                row: "c1".into(),
                target: Some("a_3".into()),
                text: "/login ozel".into(),
            },
        ];
        let job = CopyJob {
            owner: "1",
            source: &source,
            base: &base,
            with_server: false,
            secrets: &secrets,
            limits: &limits,
            placeholders: &off,
        };
        let for_two = job.config_for("a_2").unwrap();
        assert_eq!(
            for_two.commands[0].words,
            CloudRowText::Secret {
                text: "/login ortak".into(),
                kept: false
            }
        );
        assert_eq!(
            for_two.commands[1].words,
            CloudRowText::Secret {
                text: "/is home".into(),
                kept: false
            },
            "this computer's words, sealed again for the target"
        );
        assert_eq!(for_two.inventory, None);
        assert_eq!(for_two.movement, None, "movement is never copied");
        assert_eq!(for_two.owner, "Ahmet");
        let for_three = job.config_for("a_3").unwrap();
        assert_eq!(
            for_three.commands[0].words,
            CloudRowText::Secret {
                text: "/login ozel".into(),
                kept: false
            }
        );
        let none = CopyJob {
            secrets: &[],
            ..job
        };
        assert_eq!(none.config_for("a_2"), None, "a row without words stops it");
    }
}
