//! TLS for the live console socket: rustls with the operating system's trust store (the verifier
//! reqwest and the presence socket use here), built once.

use std::sync::{Arc, OnceLock};

use rustls::ClientConfig;
use rustls_platform_verifier::BuilderVerifierExt;

static CONFIG: OnceLock<Result<Arc<ClientConfig>, String>> = OnceLock::new();

pub(crate) fn client_config() -> Result<Arc<ClientConfig>, String> {
    CONFIG
        .get_or_init(|| {
            // Explicit provider: the workspace enables more than one rustls backend, so there is
            // no process-wide default to fall back on.
            let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
            let config = ClientConfig::builder_with_provider(provider)
                .with_safe_default_protocol_versions()
                .map_err(|error| error.to_string())?
                .with_platform_verifier()
                .map_err(|error| error.to_string())?
                .with_no_client_auth();
            Ok(Arc::new(config))
        })
        .clone()
}

#[cfg(test)]
mod tests {
    #[test]
    fn builds_a_client_config() {
        assert!(super::client_config().is_ok());
    }
}
