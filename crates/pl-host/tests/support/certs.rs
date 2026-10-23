//! A throwaway certificate authority for the local relays' TLS.

use rcgen::{BasicConstraints, CertificateParams, DnType, IsCa, Issuer, KeyPair, KeyUsagePurpose};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

pub struct TestCa {
    pub root: CertificateDer<'static>,
    params: CertificateParams,
    key: KeyPair,
}

impl TestCa {
    pub fn new() -> Self {
        let key = KeyPair::generate().unwrap();
        let mut params = CertificateParams::new(Vec::<String>::new()).unwrap();
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::DigitalSignature,
        ];
        params
            .distinguished_name
            .push(DnType::CommonName, "pl-host test CA");
        let root = params.self_signed(&key).unwrap().der().clone();
        Self { root, params, key }
    }

    /// A certificate for these names (DNS names or IP literals) and its key.
    pub fn leaf(&self, names: &[&str]) -> (Vec<CertificateDer<'static>>, PrivateKeyDer<'static>) {
        let key = KeyPair::generate().unwrap();
        let params = CertificateParams::new(
            names
                .iter()
                .map(|name| (*name).to_owned())
                .collect::<Vec<_>>(),
        )
        .unwrap();
        let issuer = Issuer::from_params(&self.params, &self.key);
        let certificate = params.signed_by(&key, &issuer).unwrap();
        (
            vec![certificate.der().clone()],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.serialize_der())),
        )
    }
}

pub fn provider() -> std::sync::Arc<rustls::crypto::CryptoProvider> {
    std::sync::Arc::new(rustls::crypto::aws_lc_rs::default_provider())
}

/// A rustls server config for a leaf of `ca` (TLS 1.3 only when it is for QUIC).
pub fn server_tls(
    ca: &TestCa,
    names: &[&str],
    alpn: &[&[u8]],
    tls13_only: bool,
) -> rustls::ServerConfig {
    let (chain, key) = ca.leaf(names);
    let versions: &[&'static rustls::SupportedProtocolVersion] = if tls13_only {
        &[&rustls::version::TLS13]
    } else {
        rustls::DEFAULT_VERSIONS
    };
    let mut config = rustls::ServerConfig::builder_with_provider(provider())
        .with_protocol_versions(versions)
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(chain, key)
        .unwrap();
    config.alpn_protocols = alpn.iter().map(|protocol| protocol.to_vec()).collect();
    config
}
