//! Request plumbing for the account flows: cancellation, timeouts, connect retries and error
//! mapping. Bodies of successful responses are never logged (they hold tokens).

use std::time::Duration;

use pl_net::CancellationToken;
use serde::de::DeserializeOwned;

use crate::error::{AuthError, AuthStep, Result, sanitize};

/// A finished response: status plus the whole (small) body.
pub(crate) struct Reply {
    pub status: u16,
    pub body: Vec<u8>,
}

impl Reply {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    pub fn json<T: DeserializeOwned>(&self, step: AuthStep) -> Result<T> {
        serde_json::from_slice(&self.body).map_err(|error| {
            AuthError::unexpected(
                step,
                format!("invalid JSON (HTTP {}): {error}", self.status),
            )
        })
    }

    pub fn json_value(&self) -> Option<serde_json::Value> {
        serde_json::from_slice(&self.body).ok()
    }

    /// Sanitized, truncated body text for diagnostics.
    pub fn snippet(&self) -> String {
        sanitize(&String::from_utf8_lossy(&self.body))
    }
}

/// How a request is sent.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SendOptions {
    pub timeout: Duration,
    pub connect_retries: u32,
}

/// Sends `request`, honouring `cancel`. Only failures to connect are retried (the request never
/// reached the server); everything else is returned as it is. Non-2xx statuses are not errors
/// here: callers map them per endpoint.
pub(crate) async fn send(
    request: reqwest::RequestBuilder,
    step: AuthStep,
    options: SendOptions,
    cancel: &CancellationToken,
) -> Result<Reply> {
    let mut next = Some(request.timeout(options.timeout));
    let mut attempt = 0u32;
    while let Some(current) = next.take() {
        if attempt < options.connect_retries {
            next = current.try_clone();
        }
        let outcome = tokio::select! {
            biased;
            () = cancel.cancelled() => return Err(AuthError::Cancelled),
            outcome = execute(current) => outcome,
        };
        match outcome {
            Ok(reply) => {
                if !reply.is_success() {
                    tracing::debug!(
                        %step,
                        status = reply.status,
                        body = %reply.snippet(),
                        "account request failed"
                    );
                }
                return Ok(reply);
            }
            Err(error) => {
                let message = describe(&error);
                if !error.is_connect() || next.is_none() {
                    tracing::debug!(%step, attempt, error = %message, "account request error");
                    return Err(AuthError::network(step, message));
                }
                tracing::debug!(%step, attempt, error = %message, "could not connect, retrying");
            }
        }
        let delay = Duration::from_millis(400 << attempt.min(4));
        tokio::select! {
            biased;
            () = cancel.cancelled() => return Err(AuthError::Cancelled),
            () = tokio::time::sleep(delay) => {}
        }
        attempt += 1;
    }
    // Every path above returns; this only satisfies the type checker.
    Err(AuthError::network(step, "request could not be sent"))
}

async fn execute(request: reqwest::RequestBuilder) -> reqwest::Result<Reply> {
    let response = request.send().await?;
    let status = response.status().as_u16();
    let body = response.bytes().await?.to_vec();
    Ok(Reply { status, body })
}

/// A reqwest error with its source chain, e.g. "error sending request for url (…): client
/// error (Connect): tcp connect error: connection refused". URLs never carry secrets here.
pub(crate) fn describe(error: &reqwest::Error) -> String {
    let mut message = error.to_string();
    let mut source = std::error::Error::source(error);
    while let Some(inner) = source {
        let text = inner.to_string();
        if !message.contains(&text) {
            message.push_str(": ");
            message.push_str(&text);
        }
        source = inner.source();
    }
    message
}

/// `application/x-www-form-urlencoded` body.
pub(crate) fn form_body(pairs: &[(&str, &str)]) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in pairs {
        serializer.append_pair(key, value);
    }
    serializer.finish()
}

pub(crate) const FORM_CONTENT_TYPE: &str = "application/x-www-form-urlencoded";
