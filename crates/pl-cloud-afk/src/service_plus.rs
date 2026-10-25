//! The AFK+ panel's calls on [`CloudService`]: its first look at an account (settings, the mode's
//! catalog, the quota and the approvals together), saving settings with the launcher's checks first,
//! the AI's controls, approvals, the QoL v1 routes and the AI's live stream.
//!
//! Nothing polls: the panel reads when it opens and after an action; while it is on screen the
//! account is watched with `ai: true` and the socket brings the rest. Catalogs are kept an hour (as
//! the service's caches do).

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::client::Bearer;
use crate::error::{CloudError, Result};
use crate::plus::{
    CloudNotifyPrefs, PlusAction, PlusApproval, PlusCatalog, PlusConfig, PlusControlAnswer,
    PlusDealPage, PlusDealSummary, PlusFeedPage, PlusHistory, PlusInboxLine, PlusInboxPage,
    PlusIssue, PlusParseAnswer, PlusQuota, PlusSettings, PlusThreadPage, PlusWeeklyReport,
    PlusWritten, check_config, normalized, refusal_issues,
};
use crate::service::CloudService;

const CATALOG_FRESH: Duration = Duration::from_secs(3_600);

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        })
}

/// The AFK+ panel's first look at an account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PlusPanelView {
    pub settings: PlusSettings,
    /// The mode's catalog (item names, enchantments, the money's limits); None when the service
    /// does not offer that mode now.
    pub catalog: Option<PlusCatalog>,
    /// None while the account's machine cannot be asked (`machine_away`).
    pub quota: Option<PlusQuota>,
    /// Pending ones first, then the last 24 hours' answers.
    pub approvals: Vec<PlusApproval>,
    /// The account's machine is not connected now: the quota, the feed and "Mesajlar" wait.
    pub machine_away: bool,
}

/// How a save (or "Bu hale dön") ended. Problems are worded where they are.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PlusSaveOutcome {
    Saved {
        settings: PlusSettings,
    },
    /// Something the service would refuse, found here or said by it.
    Invalid {
        issues: Vec<PlusIssue>,
    },
    /// The settings changed elsewhere since the panel read them: their newer state.
    Conflict {
        settings: PlusSettings,
    },
}

/// The machine's own refusals of a read: the panel shows the rest and says the machine is away.
fn machine_away(error: &CloudError) -> bool {
    matches!(error.code(), "host_offline" | "unavailable")
}

impl CloudService {
    /// The mode's catalog, kept an hour; None when the service does not offer it now.
    pub async fn plus_catalog(&self, mode: &str) -> Option<PlusCatalog> {
        if mode.is_empty() {
            return None;
        }
        if let Some((at, catalog)) = self.inner.catalogs.lock().get(mode)
            && at.elapsed() < CATALOG_FRESH
        {
            return Some(catalog.clone());
        }
        match self.inner.client.plus_catalog(mode).await {
            Ok(catalog) => {
                self.inner
                    .catalogs
                    .lock()
                    .insert(mode.to_owned(), (Instant::now(), catalog.clone()));
                Some(catalog)
            }
            Err(error) => {
                tracing::debug!(%error, mode, "the AFK+ catalog is not readable now");
                None
            }
        }
    }

    /// An item's names (Turkish, English) from a catalog this launcher read lately, without asking
    /// the service (an AFK notice's words; the in-game overlay's card).
    pub fn cached_item_name(&self, id: &str) -> Option<(String, String)> {
        self.inner
            .catalogs
            .lock()
            .values()
            .find_map(|(_, catalog)| catalog.item(id))
            .map(|item| (item.tr.clone(), item.en.clone()))
    }

    /// The panel's first look: the settings (they must read), and with them the catalog, the quota
    /// and the approvals (each may wait for the machine).
    pub async fn plus_panel(&self, token: &Bearer, id: &str) -> Result<PlusPanelView> {
        let client = &self.inner.client;
        let (settings, quota, approvals) = tokio::join!(
            client.plus_settings(token, id),
            client.plus_quota(token, id),
            client.plus_approvals(token, id)
        );
        let settings = settings?;
        let catalog = self.plus_catalog(&settings.config.mode).await;
        let mut away = false;
        let quota = match quota {
            Ok(quota) => Some(quota),
            Err(error) if machine_away(&error) => {
                away = true;
                None
            }
            Err(error) => {
                tracing::debug!(%error, "the AFK+ quota is not readable now");
                None
            }
        };
        let mut approvals = match approvals {
            Ok(list) => list.approvals,
            Err(error) => {
                away |= machine_away(&error);
                tracing::debug!(%error, "the AFK+ approvals are not readable now");
                Vec::new()
            }
        };
        approvals.sort_by(|a, b| {
            b.is_pending()
                .cmp(&a.is_pending())
                .then(b.created_at.cmp(&a.created_at))
        });
        Ok(PlusPanelView {
            settings,
            catalog,
            quota,
            approvals,
            machine_away: away,
        })
    }

    /// The settings as stored now.
    pub async fn plus_settings(&self, token: &Bearer, id: &str) -> Result<PlusSettings> {
        self.inner.client.plus_settings(token, id).await
    }

    /// Saves the settings (`rev`: the revision the panel started from). The launcher's checks
    /// come first; a change made elsewhere meanwhile comes back as `Conflict`.
    pub async fn plus_save(
        &self,
        token: &Bearer,
        id: &str,
        rev: u64,
        config: PlusConfig,
    ) -> Result<PlusSaveOutcome> {
        let now = unix_now();
        let config = normalized(config, now);
        let catalog = self.plus_catalog(&config.mode).await;
        let issues = check_config(&config, catalog.as_ref(), now);
        if !issues.is_empty() {
            return Ok(PlusSaveOutcome::Invalid { issues });
        }
        let answer = self.inner.client.put_plus(token, id, rev, &config).await;
        self.written(token, id, answer).await
    }

    /// "Bu hale dön" (QoL Q7): the revision `rev` becomes the next one; `current` is the revision
    /// on screen.
    pub async fn plus_restore(
        &self,
        token: &Bearer,
        id: &str,
        rev: u64,
        current: u64,
    ) -> Result<PlusSaveOutcome> {
        let answer = self
            .inner
            .client
            .plus_restore(token, id, rev, current)
            .await;
        self.written(token, id, answer).await
    }

    async fn written(
        &self,
        token: &Bearer,
        id: &str,
        answer: Result<PlusWritten>,
    ) -> Result<PlusSaveOutcome> {
        match answer {
            Ok(written) => {
                // The profile and the spent total come with a read: one more, cheap (D1).
                let mut settings = self
                    .inner
                    .client
                    .plus_settings(token, id)
                    .await
                    .unwrap_or_default();
                if settings.ai_rev < written.ai_rev {
                    settings.config = written.config;
                    settings.ai_rev = written.ai_rev;
                    settings.judge = written.judge;
                }
                Ok(PlusSaveOutcome::Saved { settings })
            }
            Err(error) => match refusal_issues(&error) {
                Some(issues) => Ok(PlusSaveOutcome::Invalid { issues }),
                None if error.code() == "rev_conflict" => Ok(PlusSaveOutcome::Conflict {
                    settings: self.inner.client.plus_settings(token, id).await?,
                }),
                None => Err(error),
            },
        }
    }

    /// "Yapay zekâyı durdur" / "başlat", a player blocked, unblocked or forgotten, "Harcananı
    /// sıfırla". The account's AI state goes into what the page shows.
    pub async fn plus_control(
        &self,
        token: &Bearer,
        id: &str,
        action: PlusAction,
        name: Option<&str>,
    ) -> Result<PlusControlAnswer> {
        let answer = self
            .inner
            .client
            .plus_control(token, id, action, name)
            .await?;
        if let Some(ai) = &answer.ai
            && let Some(account) = self.inner.known.lock().accounts.get_mut(id)
        {
            account.ai = Some(ai.clone());
        }
        Ok(answer)
    }

    pub async fn plus_feed(
        &self,
        token: &Bearer,
        id: &str,
        after: Option<&str>,
    ) -> Result<PlusFeedPage> {
        self.inner
            .client
            .plus_feed(token, id, after, Some(50))
            .await
    }

    pub async fn plus_deals(
        &self,
        token: &Bearer,
        id: &str,
        after: Option<&str>,
    ) -> Result<PlusDealPage> {
        self.inner.client.plus_deals(token, id, after).await
    }

    pub async fn plus_approvals(&self, token: &Bearer, id: &str) -> Result<Vec<PlusApproval>> {
        Ok(self.inner.client.plus_approvals(token, id).await?.approvals)
    }

    /// "Onayla", "Reddet", or "Fiyatı düşür" (`approve` with a lower `max_price`).
    pub async fn plus_answer(
        &self,
        token: &Bearer,
        id: &str,
        approval: &str,
        approve: bool,
        max_price: Option<u64>,
    ) -> Result<PlusApproval> {
        self.inner
            .client
            .answer_approval(token, id, approval, approve, max_price)
            .await
    }

    pub async fn plus_quota(&self, token: &Bearer, id: &str) -> Result<PlusQuota> {
        self.inner.client.plus_quota(token, id).await
    }

    pub async fn plus_inbox(
        &self,
        token: &Bearer,
        id: &str,
        after: Option<&str>,
    ) -> Result<PlusInboxPage> {
        self.inner.client.plus_inbox(token, id, after).await
    }

    pub async fn plus_thread(
        &self,
        token: &Bearer,
        id: &str,
        name: &str,
        after: Option<&str>,
    ) -> Result<PlusThreadPage> {
        self.inner.client.plus_thread(token, id, name, after).await
    }

    /// The owner's own words to a player in game ("Mesajlar"): checked by the service, never
    /// repeated.
    pub async fn plus_reply(
        &self,
        token: &Bearer,
        id: &str,
        name: &str,
        text: &str,
    ) -> Result<PlusInboxLine> {
        let text = text.trim();
        if text.is_empty() || text.chars().count() > crate::plus::MAX_REPLY {
            return Err(CloudError::Invalid("reply"));
        }
        self.inner.client.plus_reply(token, id, name, text).await
    }

    pub async fn plus_read(&self, token: &Bearer, id: &str, name: &str, up_to: i64) -> Result<()> {
        self.inner.client.plus_read(token, id, name, up_to).await
    }

    pub async fn plus_hand_back(&self, token: &Bearer, id: &str, name: &str) -> Result<()> {
        self.inner.client.plus_hand_back(token, id, name).await
    }

    /// "Yazarak ekle": a sentence becomes a rule's draft (nothing is stored).
    pub async fn plus_parse(
        &self,
        token: &Bearer,
        id: &str,
        text: &str,
        lang: &str,
    ) -> Result<PlusParseAnswer> {
        let text = text.trim();
        if text.is_empty() || text.chars().count() > crate::plus::MAX_PARSE {
            return Err(CloudError::Invalid("typed rule"));
        }
        self.inner.client.plus_parse(token, id, text, lang).await
    }

    pub async fn plus_history(&self, token: &Bearer, id: &str) -> Result<PlusHistory> {
        self.inner.client.plus_history(token, id).await
    }

    pub async fn plus_deal_summary(
        &self,
        token: &Bearer,
        id: &str,
        from: Option<i64>,
        to: Option<i64>,
    ) -> Result<PlusDealSummary> {
        self.inner
            .client
            .plus_deal_summary(token, id, from, to)
            .await
    }

    pub async fn plus_deals_csv(
        &self,
        token: &Bearer,
        id: &str,
        from: Option<i64>,
        to: Option<i64>,
        lang: &str,
    ) -> Result<Vec<u8>> {
        self.inner
            .client
            .plus_deals_csv(token, id, from, to, lang)
            .await
    }

    pub async fn plus_weekly_report(
        &self,
        token: &Bearer,
        week: Option<&str>,
    ) -> Result<PlusWeeklyReport> {
        self.inner.client.plus_weekly_report(token, week).await
    }

    pub async fn notify_prefs(&self, token: &Bearer) -> Result<CloudNotifyPrefs> {
        self.inner.client.notify_prefs(token).await
    }

    pub async fn set_notify_prefs(
        &self,
        token: &Bearer,
        prefs: &CloudNotifyPrefs,
    ) -> Result<CloudNotifyPrefs> {
        self.inner.client.put_notify_prefs(token, prefs).await
    }

    /// The AFK+ panel is on screen: the account's AI state, feed and approvals stream as live
    /// events (and its console with them, the same subscription).
    pub async fn watch_ai(&self, token: &Bearer, id: &str) -> Result<()> {
        let account = self.account(token, id).await?;
        let Some(host) = account.host.clone() else {
            return Err(CloudError::Invalid("no machine yet"));
        };
        let url = self.inner.client.live_url(&host);
        self.inner.live.watch_ai(url, token.clone(), &host, id);
        Ok(())
    }

    pub fn unwatch_ai(&self, id: &str) {
        self.inner.live.unwatch_ai(id);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::runtime::Handle;

    use super::*;
    use crate::client::{CloudConfig, api_url};
    use crate::copies::Copies;
    use crate::live::LiveSink;
    use crate::plus::{PlusBuyRow, PlusIssue};

    fn offline_service(dir: &std::path::Path) -> CloudService {
        let store = Arc::new(pl_afk::MemorySecretStore::new());
        let copies = Copies::new(store, dir.join("copies.json"));
        let config = CloudConfig::new(api_url(Some("http://127.0.0.1:9")), "test");
        let sink: LiveSink = Arc::new(|_| {});
        CloudService::new(
            reqwest::Client::new(),
            config,
            copies,
            Handle::current(),
            sink,
        )
    }

    #[tokio::test]
    async fn broken_settings_are_named_before_anything_is_sent() {
        let dir = tempfile::tempdir().unwrap();
        let service = offline_service(dir.path());
        let mut config = PlusConfig {
            mode: "projects-smp".into(),
            ..PlusConfig::default()
        };
        config.buy.push(PlusBuyRow {
            id: "b1".into(),
            item: "minecraft:elytra".into(),
            target: 40_000,
            max: 30_000,
            ..PlusBuyRow::default()
        });
        let outcome = service
            .plus_save(&Bearer::new("t"), "a_1", 3, config)
            .await
            .unwrap();
        assert_eq!(
            outcome,
            PlusSaveOutcome::Invalid {
                issues: vec![PlusIssue::new("buy[0].max", "order")]
            }
        );
    }

    #[tokio::test]
    async fn texts_out_of_bounds_never_leave() {
        let dir = tempfile::tempdir().unwrap();
        let service = offline_service(dir.path());
        let token = Bearer::new("t");
        let long = "a".repeat(crate::plus::MAX_REPLY + 1);
        assert!(matches!(
            service.plus_reply(&token, "a_1", "Kaan_07", &long).await,
            Err(CloudError::Invalid("reply"))
        ));
        assert!(matches!(
            service.plus_reply(&token, "a_1", "Kaan_07", "   ").await,
            Err(CloudError::Invalid("reply"))
        ));
        assert!(matches!(
            service.plus_parse(&token, "a_1", "", "tr").await,
            Err(CloudError::Invalid("typed rule"))
        ));
        assert_eq!(service.plus_catalog("").await, None);
    }
}
