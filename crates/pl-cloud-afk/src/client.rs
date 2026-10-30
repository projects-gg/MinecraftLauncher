//! The player API (`https://afk.projects.gg/api/v1`). Stateless besides configuration: the
//! launcher's friends session is the Bearer token (the service accepts only `launcher` sessions
//! that way), handed in per call.
//!
//! Retries follow the contract's "What clients do on errors": a request that never reached the
//! service (no connection) is tried once more, and so is a read that timed out or met a 502/504.
//! `restart` and `send` are never repeated after they may have arrived. Everything else returns a
//! typed error at once; callers never loop.

use std::sync::Arc;
use std::time::Duration;

use reqwest::header::{ACCEPT, CONTENT_LENGTH, CONTENT_TYPE, HeaderName, IF_MATCH, RETRY_AFTER};
use reqwest::{Method, Response};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use url::{Host, Url};

use crate::error::{CloudError, Result, from_response};
use crate::model::{
    AccountList, AccountPatch, CloudAccount, CloudLogPage, CloudVersions, Me, Offer, RevealAnswer,
    VaultKey,
};
use crate::pattern::{PatternSuggestion, pattern_text};

/// The production API.
pub const PRODUCTION_URL: &str = "https://afk.projects.gg/api/v1/";

const MAX_RESPONSE_BYTES: usize = 1 << 20;

/// The friends session's token (`pls_…`), never logged.
#[derive(Clone, PartialEq, Eq)]
pub struct Bearer(Arc<str>);

impl Bearer {
    pub fn new(token: impl Into<String>) -> Self {
        Self(Arc::from(token.into()))
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Bearer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Bearer(…)")
    }
}

/// The API's base: production, or a development override (`PROJECTS_CLOUD_AFK_URL`, read by the
/// launcher in debug builds only) for a local `wrangler dev`. An override may name the Worker's
/// root (`http://127.0.0.1:8790`) or the API itself (`…/api/v1`); `http` only for loopback hosts,
/// and never with credentials, a query or a fragment. An unusable override is ignored.
pub fn api_url(override_url: Option<&str>) -> Url {
    override_url
        .and_then(parse_override)
        .unwrap_or_else(|| Url::parse(PRODUCTION_URL).expect("the production URL is valid"))
}

fn parse_override(raw: &str) -> Option<Url> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let mut url = Url::parse(raw).ok()?;
    let loopback = match url.host()? {
        Host::Domain(name) => name.eq_ignore_ascii_case("localhost"),
        Host::Ipv4(ip) => ip.is_loopback(),
        Host::Ipv6(ip) => ip.is_loopback(),
    };
    let allowed = match url.scheme() {
        "https" => true,
        "http" => loopback,
        _ => false,
    };
    if !allowed
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    let mut path = url.path().trim_end_matches('/').to_owned();
    if !path.ends_with("/api/v1") {
        path.push_str("/api/v1");
    }
    path.push('/');
    url.set_path(&path);
    Some(url)
}

#[derive(Debug, Clone)]
pub struct CloudConfig {
    /// The API's base, ending in `/api/v1/`.
    pub base_url: Url,
    /// `ProjectsLauncher/<version>`.
    pub user_agent: String,
    /// Whole-request timeout.
    pub timeout: Duration,
}

impl CloudConfig {
    pub fn new(base_url: Url, user_agent: impl Into<String>) -> Self {
        Self {
            base_url,
            user_agent: user_agent.into(),
            timeout: Duration::from_secs(12),
        }
    }
}

/// Cheap to clone.
#[derive(Clone)]
pub struct CloudClient {
    inner: Arc<Inner>,
}

struct Inner {
    http: reqwest::Client,
    base: Url,
    user_agent: String,
    timeout: Duration,
}

impl std::fmt::Debug for CloudClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CloudClient")
            .field("base", &self.inner.base.as_str())
            .finish_non_exhaustive()
    }
}

pub(crate) struct Reply {
    pub(crate) status: u16,
    pub(crate) retry_after: Option<String>,
    pub(crate) body: Vec<u8>,
}

impl Reply {
    fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    pub(crate) fn json<T: DeserializeOwned>(&self) -> Result<T> {
        serde_json::from_slice(&self.body).map_err(|error| {
            CloudError::Protocol(format!("invalid JSON (HTTP {}): {error}", self.status))
        })
    }

    pub(crate) fn into_result(self) -> Result<Self> {
        if self.is_success() {
            Ok(self)
        } else {
            Err(from_response(
                self.status,
                self.retry_after.as_deref(),
                &self.body,
            ))
        }
    }
}

/// Whether a request may be sent again after it may have reached the service.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Repeat {
    /// A read, or a write that sets a wanted state: a timeout or a 502/504 is tried once more.
    Safe,
    /// `restart`, `send`, `reveal`: only a connection that never opened is tried again.
    Never,
}

/// What `GET /logs` reads.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LogQuery {
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub after: Option<String>,
    pub limit: Option<u32>,
    /// Only lines of these console kinds (`c` values, comma-separated: `chat,pm`); lines from
    /// before console kinds match none (CAFK-FEAT).
    pub kinds: Option<String>,
}

impl CloudClient {
    /// Reuses the launcher's shared reqwest client (connection pool, proxy settings).
    pub fn with_http_client(http: reqwest::Client, config: CloudConfig) -> Self {
        let mut base = config.base_url;
        if !base.path().ends_with('/') {
            let path = format!("{}/", base.path());
            base.set_path(&path);
        }
        Self {
            inner: Arc::new(Inner {
                http,
                base,
                user_agent: config.user_agent,
                timeout: config.timeout,
            }),
        }
    }

    pub fn base_url(&self) -> &Url {
        &self.inner.base
    }

    pub fn user_agent(&self) -> &str {
        &self.inner.user_agent
    }

    /// A plain GET of a public file (AFK+'s item data on our CDN): the bytes, or None (also past
    /// `max` bytes: what it says it is is never trusted for the size).
    pub(crate) async fn fetch_public(&self, url: &str, max: usize) -> Option<Vec<u8>> {
        let mut response = self
            .inner
            .http
            .get(url)
            .timeout(self.inner.timeout)
            .header(reqwest::header::USER_AGENT, &self.inner.user_agent)
            .send()
            .await
            .ok()?;
        if !response.status().is_success()
            || response
                .content_length()
                .is_some_and(|length| length > u64::try_from(max).unwrap_or(u64::MAX))
        {
            return None;
        }
        let mut out = Vec::new();
        while let Some(chunk) = response.chunk().await.ok()? {
            if out.len() + chunk.len() > max {
                return None;
            }
            out.extend_from_slice(&chunk);
        }
        Some(out)
    }

    /// A page of the site, on the API's origin (`https://afk.projects.gg/<path>`).
    pub fn site_url(&self, path: &str) -> Url {
        let mut root = self.inner.base.clone();
        let trimmed = root
            .path()
            .trim_end_matches('/')
            .trim_end_matches("/api/v1")
            .to_owned();
        root.set_path(&format!("{trimmed}/"));
        root.join(path.trim_start_matches('/')).unwrap_or(root)
    }

    /// The live console socket of one machine: `wss://…/api/v1/live?host=<host>`.
    pub fn live_url(&self, host: &str) -> Url {
        let mut url = self
            .inner
            .base
            .join("live")
            .expect("a fixed relative path always joins");
        let scheme = if url.scheme() == "https" { "wss" } else { "ws" };
        // http(s) → ws(s) is always a valid scheme change.
        let _ = url.set_scheme(scheme);
        url.query_pairs_mut().clear().append_pair("host", host);
        url
    }

    // --- without a session ------------------------------------------------------------------

    /// What is sold now and in which mode the service is: the launcher's gate.
    pub async fn offer(&self) -> Result<Offer> {
        self.get(None, "offer").await
    }

    pub async fn versions(&self) -> Result<CloudVersions> {
        self.get(None, "versions").await
    }

    pub async fn vault_key(&self) -> Result<VaultKey> {
        self.get(None, "vault-key").await
    }

    // --- with the player's session ----------------------------------------------------------

    pub async fn me(&self, token: &Bearer) -> Result<Me> {
        self.get(Some(token), "me").await
    }

    pub async fn accounts(&self, token: &Bearer) -> Result<AccountList> {
        self.get(Some(token), "accounts").await
    }

    pub async fn account(&self, token: &Bearer, id: &str) -> Result<CloudAccount> {
        #[derive(Deserialize)]
        struct Detail {
            account: CloudAccount,
        }
        let detail: Detail = self.get(Some(token), &account_path(id, "")?).await?;
        Ok(detail.account)
    }

    /// Changes an account; `rev` is the revision the change was made on (`If-Match`). A stale one
    /// is `412 rev_conflict` with the current `rev`.
    pub async fn patch_account(
        &self,
        token: &Bearer,
        id: &str,
        rev: u64,
        patch: &AccountPatch,
    ) -> Result<CloudAccount> {
        if patch.is_empty() {
            return Err(CloudError::Invalid("an empty change"));
        }
        let body = serde_json::to_value(patch).map_err(|e| CloudError::Protocol(e.to_string()))?;
        let reply = self
            .send(
                Method::PATCH,
                &account_path(id, "")?,
                Some(token),
                Some(body),
                &[(IF_MATCH, format!("\"{rev}\""))],
                Repeat::Never,
            )
            .await?
            .into_result()?;
        Ok(reply.json::<AccountAnswer>()?.account)
    }

    /// `desired = "run"`.
    pub async fn start(&self, token: &Bearer, id: &str) -> Result<CloudAccount> {
        self.action(token, id, "start", Repeat::Safe).await
    }

    /// `desired = "stop"`; always allowed.
    pub async fn stop(&self, token: &Bearer, id: &str) -> Result<CloudAccount> {
        self.action(token, id, "stop", Repeat::Safe).await
    }

    /// "Yeniden bağlan": the machine starts the client again as a reconnect.
    pub async fn restart(&self, token: &Bearer, id: &str) -> Result<CloudAccount> {
        self.action(token, id, "restart", Repeat::Never).await
    }

    /// A plain console line (never a password command: the service refuses those).
    pub async fn send_line(&self, token: &Bearer, id: &str, line: &str) -> Result<()> {
        self.send_console(token, id, serde_json::json!({ "line": line }))
            .await
    }

    /// A sealed console line (row id `console`).
    pub async fn send_sealed(&self, token: &Bearer, id: &str, sealed: &str) -> Result<()> {
        self.send_console(token, id, serde_json::json!({ "sealed": sealed }))
            .await
    }

    pub async fn logs(&self, token: &Bearer, id: &str, query: &LogQuery) -> Result<CloudLogPage> {
        let mut path = account_path(id, "/logs")?;
        let params = log_params(query);
        if !params.is_empty() {
            path = format!("{path}?{params}");
        }
        self.get(Some(token), &path).await
    }

    /// "Göster": asks the account's machine for one secret row, sealed to `public_key` (a
    /// one-time P-256 key, base64url). Each call is a new, counted and audited request.
    pub async fn reveal(
        &self,
        token: &Bearer,
        id: &str,
        row_id: &str,
        public_key: &str,
    ) -> Result<RevealAnswer> {
        let reply = self
            .send(
                Method::POST,
                &account_path(id, "/reveal")?,
                Some(token),
                Some(serde_json::json!({ "rowId": row_id, "pub": public_key })),
                &[],
                Repeat::Never,
            )
            .await?
            .into_result()?;
        let answer: RevealAnswer = reply.json()?;
        if answer.row_id != row_id || answer.revealed.is_empty() || answer.nonce.is_empty() {
            return Err(CloudError::Protocol("reveal answer for another row".into()));
        }
        Ok(answer)
    }

    /// "ProBot AI ile kalıp kur" (README "Pattern helper"): a rule's trigger from the player's own
    /// words, its sentence in `lang` (`en`, else `tr`). Never repeated by itself: the service
    /// counts each call that reaches its model.
    pub async fn suggest_pattern(
        &self,
        token: &Bearer,
        id: &str,
        text: &str,
        lang: &str,
        fresh: bool,
    ) -> Result<PatternSuggestion> {
        #[derive(Deserialize)]
        struct Answer {
            suggestion: PatternSuggestion,
        }
        let reply = self
            .send(
                Method::POST,
                &account_path(id, "/rules/pattern")?,
                Some(token),
                Some(pattern_body(text, lang, fresh)?),
                &[],
                Repeat::Never,
            )
            .await?
            .into_result()?;
        reply.json::<Answer>()?.suggestion.checked()
    }

    // --- plumbing ----------------------------------------------------------------------------

    pub(crate) async fn get<T: DeserializeOwned>(
        &self,
        token: Option<&Bearer>,
        path: &str,
    ) -> Result<T> {
        self.send(Method::GET, path, token, None, &[], Repeat::Safe)
            .await?
            .into_result()?
            .json()
    }

    async fn action(
        &self,
        token: &Bearer,
        id: &str,
        verb: &str,
        repeat: Repeat,
    ) -> Result<CloudAccount> {
        let reply = self
            .send(
                Method::POST,
                &account_path(id, &format!("/{verb}"))?,
                Some(token),
                None,
                &[],
                repeat,
            )
            .await?
            .into_result()?;
        Ok(reply.json::<AccountAnswer>()?.account)
    }

    async fn send_console(&self, token: &Bearer, id: &str, body: serde_json::Value) -> Result<()> {
        self.send(
            Method::POST,
            &account_path(id, "/send")?,
            Some(token),
            Some(body),
            &[],
            Repeat::Never,
        )
        .await?
        .into_result()
        .map(|_| ())
    }

    pub(crate) async fn send(
        &self,
        method: Method,
        path: &str,
        token: Option<&Bearer>,
        json: Option<serde_json::Value>,
        headers: &[(HeaderName, String)],
        repeat: Repeat,
    ) -> Result<Reply> {
        let url = self
            .inner
            .base
            .join(path)
            .map_err(|_| CloudError::Invalid("invalid path"))?;
        let body = json
            .map(|value| serde_json::to_vec(&value))
            .transpose()
            .map_err(|error| CloudError::Protocol(error.to_string()))?;
        let mut attempt = 0u32;
        loop {
            let mut request = self
                .inner
                .http
                .request(method.clone(), url.clone())
                .timeout(self.inner.timeout)
                .header(reqwest::header::USER_AGENT, &self.inner.user_agent)
                .header(ACCEPT, "application/json");
            if let Some(token) = token {
                request = request.bearer_auth(token.expose());
            }
            for (name, value) in headers {
                request = request.header(name, value);
            }
            if let Some(body) = &body {
                request = request
                    .header(CONTENT_TYPE, "application/json")
                    .body(body.clone());
            }
            match request.send().await {
                Ok(response) => {
                    let status = response.status().as_u16();
                    if repeat == Repeat::Safe && attempt == 0 && matches!(status, 502 | 504) {
                        attempt += 1;
                        tokio::time::sleep(pause()).await;
                        continue;
                    }
                    return read_reply(response).await;
                }
                Err(error) => {
                    let retry =
                        error.is_connect() || (repeat == Repeat::Safe && error.is_timeout());
                    if retry && attempt == 0 {
                        attempt += 1;
                        tokio::time::sleep(pause()).await;
                        continue;
                    }
                    let described = describe(&error);
                    tracing::debug!(error = %described, "cloud AFK request failed");
                    return Err(CloudError::Network(described));
                }
            }
        }
    }
}

#[derive(Deserialize)]
struct AccountAnswer {
    account: CloudAccount,
}

/// The query of `GET /logs` (the serializer is not `Sync`, so it never lives across an await).
fn log_params(query: &LogQuery) -> String {
    let mut params = url::form_urlencoded::Serializer::new(String::new());
    if let Some(from) = query.from {
        params.append_pair("from", &from.to_string());
    }
    if let Some(to) = query.to {
        params.append_pair("to", &to.to_string());
    }
    if let Some(after) = &query.after {
        params.append_pair("after", after);
    }
    if let Some(limit) = query.limit {
        params.append_pair("limit", &limit.to_string());
    }
    if let Some(kinds) = &query.kinds {
        params.append_pair("kinds", kinds);
    }
    params.finish()
}

/// The pattern helper's request: the description folded as the service reads it; `fresh` only
/// when asked ("Tekrar dene").
fn pattern_body(text: &str, lang: &str, fresh: bool) -> Result<serde_json::Value> {
    let text = pattern_text(text).ok_or(CloudError::Invalid("pattern text"))?;
    let lang = if lang == "en" { "en" } else { "tr" };
    let mut body = serde_json::json!({ "text": text, "lang": lang });
    if fresh {
        body["fresh"] = serde_json::Value::Bool(true);
    }
    Ok(body)
}

/// `accounts/<id><rest>`, refusing ids that could leave the route.
pub(crate) fn account_path(id: &str, rest: &str) -> Result<String> {
    let safe = !id.is_empty()
        && id.len() <= 32
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if !safe {
        return Err(CloudError::Invalid("account id"));
    }
    Ok(format!("accounts/{id}{rest}"))
}

/// 0.3-1 s before the one repeat.
fn pause() -> Duration {
    Duration::from_millis(300 + u64::from(rand::random::<u16>() % 700))
}

async fn read_reply(mut response: Response) -> Result<Reply> {
    let status = response.status().as_u16();
    let header = |name: HeaderName| {
        response
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
    };
    let retry_after = header(RETRY_AFTER);
    if header(CONTENT_LENGTH)
        .and_then(|value| value.parse::<usize>().ok())
        .is_some_and(|length| length > MAX_RESPONSE_BYTES)
    {
        return Err(CloudError::Protocol("response too large".into()));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| CloudError::Network(describe(&error)))?
    {
        if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(CloudError::Protocol("response too large".into()));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(Reply {
        status,
        retry_after,
        body,
    })
}

/// The error and its causes on one line (reqwest's own text leaves out why it failed).
fn describe(error: &reqwest::Error) -> String {
    let mut text = error.to_string();
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        let cause_text = cause.to_string();
        if !text.contains(&cause_text) {
            text.push_str(": ");
            text.push_str(&cause_text);
        }
        source = cause.source();
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_unless_a_safe_override() {
        assert_eq!(api_url(None).as_str(), PRODUCTION_URL);
        assert_eq!(
            api_url(Some("http://127.0.0.1:8790")).as_str(),
            "http://127.0.0.1:8790/api/v1/"
        );
        assert_eq!(
            api_url(Some(" http://localhost:8790/api/v1/ ")).as_str(),
            "http://localhost:8790/api/v1/"
        );
        assert_eq!(
            api_url(Some("https://afk.example.test/api/v1")).as_str(),
            "https://afk.example.test/api/v1/"
        );
        for bad in [
            "",
            "not a url",
            "http://afk.example.test/",
            "ftp://127.0.0.1/",
            "http://user:pw@127.0.0.1:8790/",
            "http://127.0.0.1:8790/?x=1",
        ] {
            assert_eq!(api_url(Some(bad)).as_str(), PRODUCTION_URL, "{bad}");
        }
    }

    #[test]
    fn site_and_live_urls_follow_the_api() {
        let client = CloudClient::with_http_client(
            reqwest::Client::new(),
            CloudConfig::new(api_url(None), "test"),
        );
        assert_eq!(client.site_url("").as_str(), "https://afk.projects.gg/");
        assert_eq!(
            client.site_url("/satin-al?hesap=a_1").as_str(),
            "https://afk.projects.gg/satin-al?hesap=a_1"
        );
        assert_eq!(
            client.live_url("kz1").as_str(),
            "wss://afk.projects.gg/api/v1/live?host=kz1"
        );
        let local = CloudClient::with_http_client(
            reqwest::Client::new(),
            CloudConfig::new(api_url(Some("http://127.0.0.1:8790")), "test"),
        );
        assert_eq!(
            local.site_url("hesaplar").as_str(),
            "http://127.0.0.1:8790/hesaplar"
        );
        assert_eq!(
            local.live_url("kz1").as_str(),
            "ws://127.0.0.1:8790/api/v1/live?host=kz1"
        );
    }

    #[test]
    fn account_paths_stay_in_their_route() {
        assert_eq!(
            account_path("a_7KQ2MZ3P9TW4XB1C", "/start").unwrap(),
            "accounts/a_7KQ2MZ3P9TW4XB1C/start"
        );
        assert!(account_path("../me", "").is_err());
        assert!(account_path("", "").is_err());
        assert!(account_path("a/b", "").is_err());
    }

    #[test]
    fn the_pattern_request_is_the_contracts() {
        assert_eq!(
            pattern_body("  biri tp   yazınca ", "tr", false).unwrap(),
            serde_json::json!({ "text": "biri tp yazınca", "lang": "tr" })
        );
        assert_eq!(
            pattern_body("when someone says tp", "en", true).unwrap(),
            serde_json::json!({ "text": "when someone says tp", "lang": "en", "fresh": true })
        );
        assert_eq!(
            pattern_body("tp", "de", false).unwrap()["lang"],
            serde_json::json!("tr")
        );
        assert!(pattern_body(" ", "tr", false).is_err());
        assert!(pattern_body(&"a".repeat(201), "tr", false).is_err());
        assert_eq!(
            account_path("a_7KQ2MZ3P9TW4XB1C", "/rules/pattern").unwrap(),
            "accounts/a_7KQ2MZ3P9TW4XB1C/rules/pattern"
        );
    }

    #[test]
    fn tokens_are_redacted() {
        let token = Bearer::new(format!("pls_{}", "A".repeat(43)));
        assert_eq!(format!("{token:?}"), "Bearer(…)");
    }
}
