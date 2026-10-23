//! What both paths share: name resolution, TLS trust, secrets and small helpers.

use std::borrow::Cow;
use std::fmt;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use rustls::crypto::CryptoProvider;
use rustls::pki_types::CertificateDer;
use rustls::{ClientConfig, RootCertStore};
use rustls_platform_verifier::BuilderVerifierExt;
use serde::{Deserialize, Deserializer};
use tokio::task::AbortHandle;
use zeroize::Zeroizing;

use super::{BoxFuture, PathError, Step};

type ResolveFn =
    dyn Fn(String, u16) -> BoxFuture<'static, io::Result<Vec<SocketAddr>>> + Send + Sync;

/// Turns a host name into socket addresses. [`Resolver::system`] asks the operating system; tests
/// point the relay names at local servers with [`Resolver::custom`].
#[derive(Clone)]
pub struct Resolver(Arc<ResolveFn>);

impl Resolver {
    pub fn system() -> Self {
        Self(Arc::new(|host, port| {
            Box::pin(async move {
                Ok(tokio::net::lookup_host((host.as_str(), port))
                    .await?
                    .collect())
            })
        }))
    }

    pub fn custom<F>(resolve: F) -> Self
    where
        F: Fn(String, u16) -> BoxFuture<'static, io::Result<Vec<SocketAddr>>>
            + Send
            + Sync
            + 'static,
    {
        Self(Arc::new(resolve))
    }

    /// The addresses for `host`, IPv4 first (the relays hand out IPv4 addresses). An IP literal is
    /// its own answer.
    pub(crate) async fn resolve(&self, host: &str, port: u16) -> io::Result<Vec<SocketAddr>> {
        if let Ok(ip) = host.parse::<IpAddr>() {
            return Ok(vec![SocketAddr::new(ip, port)]);
        }
        let mut addresses = (self.0)(host.to_owned(), port).await?;
        addresses.sort_by_key(SocketAddr::is_ipv6);
        if addresses.is_empty() {
            return Err(io::ErrorKind::NotFound.into());
        }
        Ok(addresses)
    }
}

impl Default for Resolver {
    fn default() -> Self {
        Self::system()
    }
}

impl fmt::Debug for Resolver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Resolver")
    }
}

/// Which certificates a relay's TLS (the e4mc relay's QUIC, TURN over TLS) must chain to.
#[derive(Clone, Default)]
pub enum Trust {
    /// The operating system's trust store.
    #[default]
    Platform,
    /// Only these roots (tests with local relays).
    Roots(Vec<CertificateDer<'static>>),
}

impl fmt::Debug for Trust {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Platform => f.write_str("Platform"),
            Self::Roots(roots) => write!(f, "Roots({})", roots.len()),
        }
    }
}

/// A string that never shows in `Debug` and is wiped from memory when dropped (TURN usernames and
/// credentials).
#[derive(Clone)]
pub struct Secret(Zeroizing<String>);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(Zeroizing::new(value.into()))
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

impl<'de> Deserialize<'de> for Secret {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(Self::new)
    }
}

/// The rustls provider every config here names (the workspace builds more than one, so there is no
/// process-wide default).
pub(crate) fn provider() -> Arc<CryptoProvider> {
    static PROVIDER: OnceLock<Arc<CryptoProvider>> = OnceLock::new();
    PROVIDER
        .get_or_init(|| Arc::new(rustls::crypto::aws_lc_rs::default_provider()))
        .clone()
}

/// A TLS client config that checks the server against `trust`; `tls13_only` for QUIC. No session
/// resumption.
pub(crate) fn client_tls(trust: &Trust, tls13_only: bool) -> Result<ClientConfig, rustls::Error> {
    let versions: &[&'static rustls::SupportedProtocolVersion] = if tls13_only {
        &[&rustls::version::TLS13]
    } else {
        rustls::DEFAULT_VERSIONS
    };
    let builder =
        ClientConfig::builder_with_provider(provider()).with_protocol_versions(versions)?;
    let mut config = match trust {
        Trust::Platform => builder.with_platform_verifier()?.with_no_client_auth(),
        Trust::Roots(roots) => {
            let mut store = RootCertStore::empty();
            for root in roots {
                store.add(root.clone())?;
            }
            builder.with_root_certificates(store).with_no_client_auth()
        }
    };
    config.resumption = rustls::client::Resumption::disabled();
    Ok(config)
}

/// Where a client's UDP socket binds to reach `remote`: the loopback address for a loopback peer
/// (local relays in tests and development; nothing listens on the network), else any address of
/// the peer's family.
pub(crate) fn local_for(remote: SocketAddr) -> SocketAddr {
    match remote.ip() {
        ip if ip.is_loopback() => SocketAddr::new(ip, 0),
        IpAddr::V4(_) => SocketAddr::from(([0u8; 4], 0)),
        IpAddr::V6(_) => SocketAddr::from(([0u16; 8], 0)),
    }
}

/// A path failure with a fixed code.
pub(crate) fn fail(step: Step, code: &'static str) -> PathError {
    PathError {
        step,
        code: Cow::Borrowed(code),
    }
}

/// Background tasks of one path, aborted together.
#[derive(Default)]
pub(crate) struct Tasks(Mutex<Vec<AbortHandle>>);

impl Tasks {
    pub(crate) fn add(&self, task: AbortHandle) {
        let mut tasks = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        tasks.retain(|task| !task.is_finished());
        tasks.push(task);
    }

    pub(crate) fn abort_all(&self) {
        for task in self
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .drain(..)
        {
            task.abort();
        }
    }
}

impl Drop for Tasks {
    fn drop(&mut self) {
        self.abort_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_stay_out_of_debug() {
        let secret = Secret::new("hunter2");
        assert_eq!(format!("{secret:?}"), "<redacted>");
        assert_eq!(secret.expose(), "hunter2");
    }

    #[test]
    fn builds_both_kinds_of_client_config() {
        assert!(client_tls(&Trust::Platform, false).is_ok());
        assert!(client_tls(&Trust::Roots(Vec::new()), true).is_ok());
    }

    #[tokio::test]
    async fn an_ip_literal_resolves_to_itself() {
        let resolver =
            Resolver::custom(|_, _| Box::pin(async { Err(io::ErrorKind::Other.into()) }));
        let addresses = resolver.resolve("127.0.0.1", 9).await.unwrap();
        assert_eq!(addresses, vec![SocketAddr::from(([127, 0, 0, 1], 9))]);
    }

    #[tokio::test]
    async fn ipv4_comes_first() {
        let resolver = Resolver::custom(|_, port| {
            Box::pin(async move {
                Ok(vec![
                    SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], port)),
                    SocketAddr::from(([127, 0, 0, 1], port)),
                ])
            })
        });
        let addresses = resolver.resolve("relay.example", 9).await.unwrap();
        assert!(addresses[0].is_ipv4());
    }

    #[test]
    fn sockets_bind_to_loopback_only_for_loopback_peers() {
        let local = |remote: &str| local_for(remote.parse().unwrap()).to_string();
        assert_eq!(local("127.0.0.1:3478"), "127.0.0.1:0");
        assert_eq!(local("[::1]:3478"), "[::1]:0");
        assert_eq!(local("162.159.207.10:3478"), "0.0.0.0:0");
        assert_eq!(local("[2606:4700::1]:3478"), "[::]:0");
    }
}
