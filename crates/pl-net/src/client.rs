use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
        Self::build(config, reqwest::redirect::Policy::default())
    }

    /// Like [`Self::new`], but no redirect is followed (it comes back as an error status): for
    /// addresses that must stay on the host they name (OVL-AVATARS: the overlay's pictures stay on
    /// the launcher's picture hosts, as the page's CSP keeps its own).
    pub fn new_without_redirects(config: HttpConfig) -> Result<Self> {
        Self::build(config, reqwest::redirect::Policy::none())
    }

    fn build(config: HttpConfig, redirects: reqwest::redirect::Policy) -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent(config.user_agent)
            .connect_timeout(config.connect_timeout)
            .read_timeout(config.read_timeout)
            .pool_idle_timeout(Duration::from_secs(60))
            .redirect(redirects)
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
            let retry_after = match &outcome {
                Ok(resp) => retry_after_delay(resp.status(), resp.headers(), SystemTime::now()),
                Err(_) => None,
            };
            tokio::time::sleep(retry_delay(attempt, retry_after, random_unit())).await;
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

/// Longest wait a server's `Retry-After` can impose on one retry.
pub(crate) const MAX_RETRY_AFTER: Duration = Duration::from_secs(30);

/// Exponential ceiling for an attempt: 400ms, 800ms, 1.6s ... capped at 8s.
fn backoff_ceiling(attempt: u32) -> Duration {
    Duration::from_millis((400u64 << attempt.min(5)).min(8_000))
}

/// Backoff with "equal jitter": between half the ceiling and the ceiling, so launchers that
/// failed together do not retry together. `unit` is a random number in `[0, 1]`.
pub(crate) fn backoff_with(attempt: u32, unit: f64) -> Duration {
    let ceiling = backoff_ceiling(attempt);
    let unit = if unit.is_finite() {
        unit.clamp(0.0, 1.0)
    } else {
        1.0
    };
    ceiling / 2 + ceiling.div_f64(2.0).mul_f64(unit)
}

pub(crate) fn backoff(attempt: u32) -> Duration {
    backoff_with(attempt, random_unit())
}

/// The wait before the next retry: the jittered backoff, or the server's (capped) `Retry-After`
/// when that is longer.
pub(crate) fn retry_delay(attempt: u32, retry_after: Option<Duration>, unit: f64) -> Duration {
    let jittered = backoff_with(attempt, unit);
    match retry_after {
        Some(wait) => jittered.max(wait.min(MAX_RETRY_AFTER)),
        None => jittered,
    }
}

fn random_unit() -> f64 {
    f64::from(rand::random::<u16>()) / f64::from(u16::MAX)
}

/// `Retry-After` of a 429 or 503 answer (the statuses that define it).
pub(crate) fn retry_after_delay(
    status: StatusCode,
    headers: &reqwest::header::HeaderMap,
    now: SystemTime,
) -> Option<Duration> {
    if status != StatusCode::TOO_MANY_REQUESTS && status != StatusCode::SERVICE_UNAVAILABLE {
        return None;
    }
    let value = headers.get(reqwest::header::RETRY_AFTER)?.to_str().ok()?;
    parse_retry_after(value, now)
}

/// Parses `Retry-After`: delta-seconds or an HTTP-date (IMF-fixdate). Garbage gives `None`;
/// a date in the past gives zero. Not capped here (see `retry_delay`), but saturating.
pub(crate) fn parse_retry_after(value: &str, now: SystemTime) -> Option<Duration> {
    let value = value.trim();
    if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) {
        return Some(Duration::from_secs(value.parse().unwrap_or(u64::MAX)));
    }
    let target = parse_http_date(value)?;
    Some(target.duration_since(now).unwrap_or(Duration::ZERO))
}

/// `Sun, 06 Nov 1994 08:49:37 GMT`.
fn parse_http_date(value: &str) -> Option<SystemTime> {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let (_, rest) = value.split_once(", ")?;
    let mut parts = rest.split(' ');
    let day: u64 = parts.next()?.parse().ok()?;
    let month_name = parts.next()?;
    let month = MONTHS.iter().position(|m| *m == month_name)? as u64 + 1;
    let year: u64 = parts.next()?.parse().ok()?;
    let mut clock = parts.next()?.split(':');
    let hour: u64 = clock.next()?.parse().ok()?;
    let minute: u64 = clock.next()?.parse().ok()?;
    let second: u64 = clock.next()?.parse().ok()?;
    if parts.next()? != "GMT" || parts.next().is_some() || clock.next().is_some() {
        return None;
    }
    if !(1970..=9999).contains(&year)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    // Days from civil (Howard Hinnant), year >= 1970.
    let y = if month <= 2 { year - 1 } else { year };
    let era = y / 400;
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    UNIX_EPOCH.checked_add(Duration::from_secs(
        days * 86_400 + hour * 3_600 + minute * 60 + second,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_stays_within_jitter_bounds() {
        for (attempt, ceiling) in [(0, 400u64), (1, 800), (2, 1_600), (5, 8_000), (10, 8_000)] {
            assert_eq!(
                backoff_with(attempt, 0.0),
                Duration::from_millis(ceiling / 2)
            );
            assert_eq!(backoff_with(attempt, 1.0), Duration::from_millis(ceiling));
            let mid = backoff_with(attempt, 0.5);
            assert!(
                mid > Duration::from_millis(ceiling / 2) && mid < Duration::from_millis(ceiling)
            );
        }
        // Out-of-range or NaN random input stays bounded.
        assert_eq!(backoff_with(0, 7.0), Duration::from_millis(400));
        assert_eq!(backoff_with(0, -1.0), Duration::from_millis(200));
        assert_eq!(backoff_with(0, f64::NAN), Duration::from_millis(400));
        for _ in 0..200 {
            let d = backoff(3);
            assert!(d >= Duration::from_millis(1_600) && d <= Duration::from_millis(3_200));
        }
    }

    fn at(secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(secs)
    }

    #[test]
    fn retry_after_seconds() {
        let now = at(0);
        assert_eq!(
            parse_retry_after("120", now),
            Some(Duration::from_secs(120))
        );
        assert_eq!(parse_retry_after(" 0 ", now), Some(Duration::ZERO));
        assert_eq!(parse_retry_after("-5", now), None);
        assert_eq!(parse_retry_after("1.5", now), None);
        assert_eq!(parse_retry_after("", now), None);
        assert_eq!(parse_retry_after("soon", now), None);
        assert_eq!(
            parse_retry_after("99999999999999999999999999", now),
            Some(Duration::from_secs(u64::MAX))
        );
    }

    #[test]
    fn retry_after_http_date() {
        // Sun, 06 Nov 1994 08:49:37 GMT = 784111777
        let date = "Sun, 06 Nov 1994 08:49:37 GMT";
        assert_eq!(
            parse_retry_after(date, at(784_111_777 - 90)),
            Some(Duration::from_secs(90))
        );
        assert_eq!(
            parse_retry_after(date, at(784_111_777 + 5)),
            Some(Duration::ZERO)
        );
        assert_eq!(
            parse_http_date("Thu, 01 Jan 1970 00:00:00 GMT"),
            Some(at(0))
        );
        assert_eq!(
            parse_http_date("Fri, 29 Feb 2008 12:00:00 GMT"),
            Some(at(1_204_286_400))
        );
        for bad in [
            "Sun, 06 Nov 1994 08:49:37",
            "Sun, 06 Foo 1994 08:49:37 GMT",
            "Sun, 06 Nov 1994 25:49:37 GMT",
            "Sun, 06 Nov 1969 08:49:37 GMT",
            "Sun 06 Nov 1994 08:49:37 GMT",
            "Sun, 06 Nov 1994 08:49:37 GMT extra",
        ] {
            assert_eq!(parse_retry_after(bad, at(0)), None, "{bad}");
        }
    }

    #[test]
    fn retry_after_only_read_on_429_and_503() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(reqwest::header::RETRY_AFTER, "7".parse().unwrap());
        let now = at(0);
        assert_eq!(
            retry_after_delay(StatusCode::TOO_MANY_REQUESTS, &headers, now),
            Some(Duration::from_secs(7))
        );
        assert_eq!(
            retry_after_delay(StatusCode::SERVICE_UNAVAILABLE, &headers, now),
            Some(Duration::from_secs(7))
        );
        assert_eq!(
            retry_after_delay(StatusCode::BAD_GATEWAY, &headers, now),
            None
        );
        let empty = reqwest::header::HeaderMap::new();
        assert_eq!(
            retry_after_delay(StatusCode::TOO_MANY_REQUESTS, &empty, now),
            None
        );
    }

    #[test]
    fn retry_delay_honours_and_caps_retry_after() {
        assert_eq!(retry_delay(0, None, 0.0), Duration::from_millis(200));
        // Shorter than the backoff: the backoff wins.
        assert_eq!(
            retry_delay(0, Some(Duration::ZERO), 1.0),
            Duration::from_millis(400)
        );
        assert_eq!(
            retry_delay(0, Some(Duration::from_secs(5)), 0.3),
            Duration::from_secs(5)
        );
        // Huge values are capped.
        assert_eq!(
            retry_delay(0, Some(Duration::from_secs(86_400)), 0.3),
            MAX_RETRY_AFTER
        );
        assert_eq!(
            retry_delay(2, Some(Duration::from_secs(u64::MAX)), 1.0),
            MAX_RETRY_AFTER
        );
    }

    #[test]
    fn total_wait_over_the_default_attempts_is_bounded() {
        let config = HttpConfig::new("test");
        let total = |unit: f64| -> Duration {
            (0..config.retries)
                .map(|attempt| retry_delay(attempt, None, unit))
                .sum()
        };
        assert_eq!(total(0.0), Duration::from_millis(1_400));
        assert_eq!(total(1.0), Duration::from_millis(2_800));
        let with_header: Duration = (0..config.retries)
            .map(|attempt| retry_delay(attempt, Some(Duration::MAX), 1.0))
            .sum();
        assert_eq!(with_header, MAX_RETRY_AFTER * config.retries);
    }

    #[test]
    fn retryable_statuses() {
        assert!(is_retryable_status(StatusCode::BAD_GATEWAY));
        assert!(is_retryable_status(StatusCode::TOO_MANY_REQUESTS));
        assert!(!is_retryable_status(StatusCode::NOT_FOUND));
        assert!(!is_retryable_status(StatusCode::OK));
    }

    /// A local server: `/moved` redirects to `/here`, which answers "ok".
    async fn redirecting_server() -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 2048];
                    let n = socket.read(&mut buf).await.unwrap_or(0);
                    let reply = if buf[..n].starts_with(b"GET /moved ") {
                        "HTTP/1.1 302 Found\r\nLocation: /here\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    } else {
                        "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok"
                    };
                    let _ = socket.write_all(reply.as_bytes()).await;
                });
            }
        });
        format!("http://{addr}/moved")
    }

    #[tokio::test]
    async fn a_client_without_redirects_stays_on_the_address() {
        let url = redirecting_server().await;
        let mut config = HttpConfig::new("test");
        config.retries = 0;
        let follows = Http::new(config.clone()).unwrap();
        assert_eq!(follows.get_text(&url).await.unwrap(), "ok");
        let stays = Http::new_without_redirects(config).unwrap();
        assert!(matches!(
            stays.get(&url).await,
            Err(NetError::Status { status: 302, .. })
        ));
    }
}
