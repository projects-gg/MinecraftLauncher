use std::time::Duration;

use bytes::Bytes;
use reqwest::StatusCode;
use serde::de::DeserializeOwned;

use crate::error::{NetError, Result};

#[derive(Debug, Clone)]
pub struct HttpConfig {
    pub user_agent: String,
    pub connect_timeout: Duration,
    /// Maximum silence between two reads of a response body.
    pub read_timeout: Duration,
    /// Extra attempts after the first one for transient failures (network, 5xx, 429).
    pub retries: u32,
}

impl HttpConfig {
    pub fn new(user_agent: impl Into<String>) -> Self {
        Self {
            user_agent: user_agent.into(),
            connect_timeout: Duration::from_secs(10),
            read_timeout: Duration::from_secs(30),
            retries: 3,
        }
    }
}

/// Shared HTTP client. Cheap to clone (reqwest clients are reference counted).
#[derive(Debug, Clone)]
pub struct Http {
    client: reqwest::Client,
    retries: u32,
}

impl Http {
    pub fn new(config: HttpConfig) -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent(config.user_agent)
            .connect_timeout(config.connect_timeout)
            .read_timeout(config.read_timeout)
            .pool_idle_timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| NetError::Setup(e.to_string()))?;
        Ok(Self {
            client,
            retries: config.retries,
        })
    }

    /// The underlying client, for requests that need custom headers or methods.
    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }

    pub fn retries(&self) -> u32 {
        self.retries
    }

    /// GET with retries on transient failures. Non-success statuses become errors.
    pub async fn get(&self, url: &str) -> Result<reqwest::Response> {
        self.get_with_retries(url, self.retries).await
    }

    pub(crate) async fn get_with_retries(
        &self,
        url: &str,
        retries: u32,
    ) -> Result<reqwest::Response> {
        let mut attempt = 0;
        loop {
            let outcome = self.client.get(url).send().await;
            let retryable = match &outcome {
                Ok(resp) => is_retryable_status(resp.status()),
                Err(e) => e.is_connect() || e.is_timeout() || e.is_request() || e.is_body(),
            };
            if !retryable || attempt >= retries {
                return match outcome {
                    Ok(resp) if resp.status().is_success() => Ok(resp),
                    Ok(resp) => Err(NetError::Status {
                        url: url.to_owned(),
                        status: resp.status().as_u16(),
                    }),
                    Err(source) => Err(NetError::Request {
                        url: url.to_owned(),
                        source,
                    }),
                };
            }
            tokio::time::sleep(backoff(attempt)).await;
            attempt += 1;
        }
    }

    pub async fn get_bytes(&self, url: &str) -> Result<Bytes> {
        let resp = self.get(url).await?;
        resp.bytes().await.map_err(|source| NetError::Request {
            url: url.to_owned(),
            source,
        })
    }

    pub async fn get_text(&self, url: &str) -> Result<String> {
        let resp = self.get(url).await?;
        resp.text().await.map_err(|source| NetError::Request {
            url: url.to_owned(),
            source,
        })
    }

    pub async fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        let bytes = self.get_bytes(url).await?;
        serde_json::from_slice(&bytes).map_err(|source| NetError::Json {
            url: url.to_owned(),
            source,
        })
    }

    /// Try each URL in order and return the first successful body plus its index.
    /// Each mirror gets a single retry so a dead primary fails over quickly.
    pub async fn get_bytes_from_mirrors(&self, urls: &[String]) -> Result<(Bytes, usize)> {
        let mut last = String::from("no URLs given");
        for (index, url) in urls.iter().enumerate() {
            match self.get_with_retries(url, self.retries.min(1)).await {
                Ok(resp) => match resp.bytes().await {
                    Ok(bytes) => return Ok((bytes, index)),
                    Err(e) => last = format!("{url}: {e}"),
                },
                Err(e) => last = e.to_string(),
            }
            tracing::debug!(%url, error = %last, "mirror failed, trying next");
        }
        Err(NetError::AllMirrorsFailed {
            what: urls.first().cloned().unwrap_or_default(),
            last,
        })
    }

    pub async fn get_json_from_mirrors<T: DeserializeOwned>(
        &self,
        urls: &[String],
    ) -> Result<(T, usize)> {
        let (bytes, index) = self.get_bytes_from_mirrors(urls).await?;
        let value = serde_json::from_slice(&bytes).map_err(|source| NetError::Json {
            url: urls[index].clone(),
            source,
        })?;
        Ok((value, index))
    }

    /// Quick connectivity probe: any HTTP answer (even 404) counts as reachable.
    pub async fn is_reachable(&self, url: &str, timeout: Duration) -> bool {
        self.client.head(url).timeout(timeout).send().await.is_ok()
    }
}

fn is_retryable_status(status: StatusCode) -> bool {
    status.is_server_error()
        || status == StatusCode::TOO_MANY_REQUESTS
        || status == StatusCode::REQUEST_TIMEOUT
}

pub(crate) fn backoff(attempt: u32) -> Duration {
    // 400ms, 800ms, 1.6s, 3.2s ... capped at 8s.
    Duration::from_millis((400u64 << attempt.min(5)).min(8_000))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_and_caps() {
        assert_eq!(backoff(0), Duration::from_millis(400));
        assert_eq!(backoff(1), Duration::from_millis(800));
        assert_eq!(backoff(10), Duration::from_millis(8_000));
    }

    #[test]
    fn retryable_statuses() {
        assert!(is_retryable_status(StatusCode::BAD_GATEWAY));
        assert!(is_retryable_status(StatusCode::TOO_MANY_REQUESTS));
        assert!(!is_retryable_status(StatusCode::NOT_FOUND));
        assert!(!is_retryable_status(StatusCode::OK));
    }
}
