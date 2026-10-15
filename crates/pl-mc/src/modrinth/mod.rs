//! Modrinth API v2 client: search, projects, versions, hash lookups, update checks and a
//! dependency resolver. Built on the shared [`pl_net::Http`] client (which sets the user agent);
//! rate limiting (HTTP 429) is handled by waiting for `X-Ratelimit-Reset`.

mod models;
mod resolve;

use std::collections::HashMap;
use std::time::Duration;

use pl_net::{Http, NetError};
use reqwest::header::HeaderMap;
use reqwest::{Method, StatusCode};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use url::Url;

pub use models::*;

use crate::error::{Error, Result};

pub const MODRINTH_API: &str = "https://api.modrinth.com/v2";

/// Extra attempts for rate limits, server errors and network hiccups.
const MAX_RETRIES: u32 = 3;
/// Ids per `/projects` or `/versions` request (keeps URLs short).
const IDS_PER_REQUEST: usize = 100;
/// Hashes per `/version_files` request.
const HASHES_PER_REQUEST: usize = 500;

#[derive(Debug, Clone)]
pub struct ModrinthClient {
    http: Http,
    base: String,
}

impl ModrinthClient {
    pub fn new(http: Http) -> Self {
        Self::with_base_url(http, MODRINTH_API)
    }

    /// Use another API root (staging server, tests).
    pub fn with_base_url(http: Http, base: impl Into<String>) -> Self {
        Self {
            http,
            base: base.into().trim_end_matches('/').to_owned(),
        }
    }

    /// `base/segments...?query`; each segment is percent-encoded (ids cannot change the path).
    fn url(&self, segments: &[&str], query: &[(&str, String)]) -> Result<Url> {
        let bad_base = || Error::Invalid(format!("bad Modrinth API URL {}", self.base));
        let mut url = Url::parse(&self.base).map_err(|_| bad_base())?;
        {
            let mut path = url.path_segments_mut().map_err(|_| bad_base())?;
            path.pop_if_empty();
            for segment in segments {
                if segment.is_empty() || *segment == "." || *segment == ".." {
                    return Err(Error::Invalid(format!("not a Modrinth id: {segment:?}")));
                }
                path.push(segment);
            }
        }
        if !query.is_empty() {
            url.query_pairs_mut()
                .extend_pairs(query.iter().map(|(k, v)| (*k, v.as_str())));
        }
        Ok(url)
    }

    /// Send a request with retries. `Ok(None)` means 404.
    async fn send(&self, method: Method, url: Url, body: Option<&Value>) -> Result<Option<Value>> {
        let url_text = url.to_string();
        let mut attempt = 0;
        loop {
            let mut request = self.http.client().request(method.clone(), url.clone());
            if let Some(body) = body {
                request = request.json(body);
            }
            let wait = match request.send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        let bytes = response.bytes().await.map_err(|source| NetError::Request {
                            url: url_text.clone(),
                            source,
                        })?;
                        let value =
                            serde_json::from_slice(&bytes).map_err(|source| NetError::Json {
                                url: url_text.clone(),
                                source,
                            })?;
                        return Ok(Some(value));
                    }
                    if status == StatusCode::NOT_FOUND {
                        return Ok(None);
                    }
                    let retryable = status == StatusCode::TOO_MANY_REQUESTS
                        || status == StatusCode::REQUEST_TIMEOUT
                        || status.is_server_error();
                    if !retryable || attempt >= MAX_RETRIES {
                        return Err(NetError::Status {
                            url: url_text,
                            status: status.as_u16(),
                        }
                        .into());
                    }
                    if status == StatusCode::TOO_MANY_REQUESTS {
                        let wait = rate_limit_wait(response.headers());
                        tracing::warn!(url = %url_text, wait_secs = wait.as_secs(), "Modrinth rate limit hit, waiting");
                        wait
                    } else {
                        backoff(attempt)
                    }
                }
                Err(source) => {
                    let transient =
                        source.is_connect() || source.is_timeout() || source.is_request();
                    if !transient || attempt >= MAX_RETRIES {
                        return Err(NetError::Request {
                            url: url_text,
                            source,
                        }
                        .into());
                    }
                    backoff(attempt)
                }
            };
            tracing::debug!(url = %url_text, attempt, "retrying Modrinth request");
            tokio::time::sleep(wait).await;
            attempt += 1;
        }
    }

    async fn fetch<T: DeserializeOwned>(
        &self,
        method: Method,
        url: Url,
        body: Option<&Value>,
        what: &str,
    ) -> Result<Option<T>> {
        match self.send(method, url, body).await? {
            Some(value) => decode(value, what).map(Some),
            None => Ok(None),
        }
    }

    async fn get<T: DeserializeOwned>(
        &self,
        segments: &[&str],
        query: &[(&str, String)],
    ) -> Result<T> {
        let what = segments.join("/");
        let url = self.url(segments, query)?;
        self.fetch(Method::GET, url, None, &what)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Modrinth {what}")))
    }

    async fn post<T: DeserializeOwned>(&self, segments: &[&str], body: &Value) -> Result<T> {
        let what = segments.join("/");
        let url = self.url(segments, &[])?;
        self.fetch(Method::POST, url, Some(body), &what)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Modrinth {what}")))
    }

    /// Search projects.
    pub async fn search(&self, query: &ModrinthSearchQuery) -> Result<ModrinthSearchResults> {
        let mut params: Vec<(&str, String)> = Vec::new();
        if let Some(text) = query
            .query
            .as_deref()
            .map(str::trim)
            .filter(|q| !q.is_empty())
        {
            params.push(("query", text.to_owned()));
        }
        if let Some(facets) = query.facets() {
            params.push(("facets", facets));
        }
        params.push(("index", query.index.as_str().to_owned()));
        params.push(("offset", query.offset.to_string()));
        params.push(("limit", query.limit.clamp(1, 100).to_string()));
        self.get(&["search"], &params).await
    }

    /// A project by id or slug.
    pub async fn project(&self, id_or_slug: &str) -> Result<ModrinthProject> {
        self.get(&["project", id_or_slug], &[]).await
    }

    /// Several projects by id or slug (unknown ones are simply missing from the result).
    pub async fn projects(&self, ids: &[String]) -> Result<Vec<ModrinthProject>> {
        let mut out = Vec::new();
        for chunk in ids.chunks(IDS_PER_REQUEST) {
            let list: Vec<ModrinthProject> = self
                .get(&["projects"], &[("ids", json!(chunk).to_string())])
                .await?;
            out.extend(list);
        }
        Ok(out)
    }

    /// Versions of a project, newest first, optionally filtered (empty filter = all).
    pub async fn project_versions(
        &self,
        id_or_slug: &str,
        loaders: &[String],
        game_versions: &[String],
    ) -> Result<Vec<ModrinthVersion>> {
        let mut params = vec![("include_changelog", "false".to_owned())];
        if !loaders.is_empty() {
            params.push(("loaders", json!(loaders).to_string()));
        }
        if !game_versions.is_empty() {
            params.push(("game_versions", json!(game_versions).to_string()));
        }
        self.get(&["project", id_or_slug, "version"], &params).await
    }

    pub async fn version(&self, id: &str) -> Result<ModrinthVersion> {
        self.get(&["version", id], &[]).await
    }

    /// Several versions by id (unknown ones are missing from the result).
    pub async fn versions(&self, ids: &[String]) -> Result<Vec<ModrinthVersion>> {
        let mut out = Vec::new();
        for chunk in ids.chunks(IDS_PER_REQUEST) {
            let list: Vec<ModrinthVersion> = self
                .get(&["versions"], &[("ids", json!(chunk).to_string())])
                .await?;
            out.extend(list);
        }
        Ok(out)
    }

    /// The version a file belongs to, by hash. `None` if Modrinth does not know the file.
    pub async fn version_from_hash(
        &self,
        hash: &str,
        algorithm: ModrinthHashAlgorithm,
    ) -> Result<Option<ModrinthVersion>> {
        let url = self.url(
            &["version_file", hash],
            &[("algorithm", algorithm.as_str().to_owned())],
        )?;
        self.fetch(Method::GET, url, None, "version_file").await
    }

    /// Versions for many file hashes at once: `hash → version` (unknown hashes are missing).
    pub async fn version_files(
        &self,
        hashes: &[String],
        algorithm: ModrinthHashAlgorithm,
    ) -> Result<HashMap<String, ModrinthVersion>> {
        let mut out = HashMap::new();
        for chunk in hashes.chunks(HASHES_PER_REQUEST) {
            let body = json!({ "hashes": chunk, "algorithm": algorithm.as_str() });
            let map: HashMap<String, ModrinthVersion> =
                self.post(&["version_files"], &body).await?;
            out.extend(map);
        }
        Ok(out)
    }

    /// Latest compatible version for each file hash: `hash → latest version`.
    pub async fn update_check(
        &self,
        hashes: &[String],
        algorithm: ModrinthHashAlgorithm,
        loaders: &[String],
        game_versions: &[String],
    ) -> Result<HashMap<String, ModrinthVersion>> {
        let mut out = HashMap::new();
        for chunk in hashes.chunks(HASHES_PER_REQUEST) {
            let body = json!({
                "hashes": chunk,
                "algorithm": algorithm.as_str(),
                "loaders": loaders,
                "game_versions": game_versions,
            });
            let map: HashMap<String, ModrinthVersion> =
                self.post(&["version_files", "update"], &body).await?;
            out.extend(map);
        }
        Ok(out)
    }

    /// Installed files (by SHA-1) that have a newer compatible version on Modrinth.
    pub async fn available_updates(
        &self,
        sha1s: &[String],
        target: &ModrinthResolveTarget,
    ) -> Result<Vec<ModrinthUpdate>> {
        let current = self
            .version_files(sha1s, ModrinthHashAlgorithm::Sha1)
            .await?;
        let known: Vec<String> = sha1s
            .iter()
            .filter(|h| current.contains_key(*h))
            .cloned()
            .collect();
        if known.is_empty() {
            return Ok(Vec::new());
        }
        let latest = self
            .update_check(
                &known,
                ModrinthHashAlgorithm::Sha1,
                &target.loaders,
                &target.game_versions,
            )
            .await?;
        let mut updates: Vec<ModrinthUpdate> = latest
            .into_iter()
            .filter_map(|(hash, latest)| {
                let current = current.get(&hash).cloned();
                let newer = current.as_ref().is_none_or(|c| c.id != latest.id);
                newer.then_some(ModrinthUpdate {
                    hash,
                    current,
                    latest,
                })
            })
            .collect();
        updates.sort_by(|a, b| a.latest.project_id.cmp(&b.latest.project_id));
        Ok(updates)
    }

    pub async fn categories(&self) -> Result<Vec<ModrinthCategory>> {
        self.get(&["tag", "category"], &[]).await
    }

    pub async fn loaders(&self) -> Result<Vec<ModrinthLoaderTag>> {
        self.get(&["tag", "loader"], &[]).await
    }

    pub async fn game_versions(&self) -> Result<Vec<ModrinthGameVersionTag>> {
        self.get(&["tag", "game_version"], &[]).await
    }

    /// Collect the required dependencies of `roots` (recursively) that are compatible with
    /// `target`, skipping projects in `installed` and avoiding cycles and duplicates. See
    /// [`ModrinthDependencyResolution`]. Drop the future to cancel.
    pub async fn resolve_dependencies(
        &self,
        roots: &[ModrinthVersion],
        target: &ModrinthResolveTarget,
        installed: &ModrinthInstalled,
    ) -> Result<ModrinthDependencyResolution> {
        resolve::resolve(self, roots, target, installed).await
    }
}

fn decode<T: DeserializeOwned>(mut value: Value, what: &str) -> Result<T> {
    camelize_keys(&mut value);
    serde_json::from_value(value).map_err(|e| Error::json(format!("Modrinth {what}"), e))
}

/// Recursively turn snake_case object keys into camelCase (`project_id` → `projectId`).
/// Map keys that are data (file hashes) contain no underscores and stay unchanged.
pub(crate) fn camelize_keys(value: &mut Value) {
    match value {
        Value::Object(map) => {
            let entries = std::mem::take(map);
            for (key, mut inner) in entries {
                camelize_keys(&mut inner);
                map.insert(snake_to_camel(&key), inner);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(camelize_keys),
        _ => {}
    }
}

fn snake_to_camel(key: &str) -> String {
    if !key.contains('_') {
        return key.to_owned();
    }
    let mut out = String::with_capacity(key.len());
    let mut upper = false;
    for c in key.chars() {
        if c == '_' && !out.is_empty() {
            upper = true;
        } else if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// How long to wait after a 429: `X-Ratelimit-Reset` (seconds until the window resets), else
/// `Retry-After`, else 2 s; clamped to 1–60 s.
fn rate_limit_wait(headers: &HeaderMap) -> Duration {
    let seconds = ["x-ratelimit-reset", "retry-after"]
        .iter()
        .find_map(|name| {
            headers
                .get(*name)?
                .to_str()
                .ok()?
                .trim()
                .parse::<f64>()
                .ok()
        })
        .unwrap_or(2.0);
    Duration::from_secs_f64(seconds.clamp(1.0, 60.0))
}

fn backoff(attempt: u32) -> Duration {
    Duration::from_millis((500u64 << attempt.min(4)).min(8_000))
}

#[cfg(test)]
mod tests;
