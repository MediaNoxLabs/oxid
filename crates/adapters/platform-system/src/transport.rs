// SPDX-License-Identifier: Apache-2.0

//! One explicit trust-policy boundary for native HTTP and WebSocket clients.

use std::{error::Error, fmt, net::IpAddr, sync::Arc};

use reqwest::{Certificate, ClientBuilder};
use rustls::{ClientConfig, RootCertStore};
use rustls_platform_verifier_07::ConfigVerifierExt as _;
use tokio_tungstenite::Connector;
use url::Url;

/// The only TLS trust modes admitted by the native application.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportTrustPolicy {
    /// Use the operating system trust policy for normal public services.
    PlatformTrust,
    /// Use the reviewed Mozilla/WebPKI bundle for Tailnet demo routes.
    BundledPublicRoots,
    /// Permit plaintext only for an exact loopback development route.
    DevelopmentLoopback,
}

/// Endpoint or TLS construction failed before any network operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportTrustError {
    InvalidEndpoint,
    InsecureRemoteEndpoint,
    TlsConfigurationUnavailable,
}

impl fmt::Display for TransportTrustError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidEndpoint => "transport endpoint is invalid",
            Self::InsecureRemoteEndpoint => "plaintext transport is restricted to loopback",
            Self::TlsConfigurationUnavailable => "TLS trust configuration is unavailable",
        })
    }
}

impl Error for TransportTrustError {}

/// Returns a hardened HTTP builder whose trust policy is derived from the
/// already-validated route. No retry or fallback to another trust policy is
/// performed.
pub fn http_client_builder_for(endpoint: &Url) -> Result<ClientBuilder, TransportTrustError> {
    ensure_crypto_provider();
    match classify(endpoint, &["http", "https"])? {
        TransportTrustPolicy::PlatformTrust | TransportTrustPolicy::DevelopmentLoopback => {
            Ok(reqwest::Client::builder())
        }
        TransportTrustPolicy::BundledPublicRoots => {
            let roots = webpki_root_certs::TLS_SERVER_ROOT_CERTS
                .iter()
                .map(|certificate| Certificate::from_der(certificate.as_ref()))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| TransportTrustError::TlsConfigurationUnavailable)?;
            Ok(reqwest::Client::builder().tls_certs_only(roots))
        }
    }
}

/// Returns an explicit WebSocket TLS connector for the route. Loopback `ws`
/// needs no connector. TLS routes never rely on tungstenite feature defaults.
pub fn websocket_connector_for(endpoint: &Url) -> Result<Option<Connector>, TransportTrustError> {
    ensure_crypto_provider();
    match classify(endpoint, &["ws", "wss"])? {
        TransportTrustPolicy::DevelopmentLoopback => Ok(None),
        TransportTrustPolicy::PlatformTrust => {
            let config = ClientConfig::with_platform_verifier()
                .map_err(|_| TransportTrustError::TlsConfigurationUnavailable)?;
            Ok(Some(Connector::Rustls(Arc::new(config))))
        }
        TransportTrustPolicy::BundledPublicRoots => {
            let mut roots = RootCertStore::empty();
            let (accepted, _) = roots.add_parsable_certificates(
                webpki_root_certs::TLS_SERVER_ROOT_CERTS.iter().cloned(),
            );
            if accepted == 0 {
                return Err(TransportTrustError::TlsConfigurationUnavailable);
            }
            let config = ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth();
            Ok(Some(Connector::Rustls(Arc::new(config))))
        }
    }
}

fn ensure_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

fn classify(endpoint: &Url, schemes: &[&str]) -> Result<TransportTrustPolicy, TransportTrustError> {
    if endpoint.host_str().is_none()
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || !schemes.contains(&endpoint.scheme())
    {
        return Err(TransportTrustError::InvalidEndpoint);
    }
    let host = endpoint
        .host_str()
        .ok_or(TransportTrustError::InvalidEndpoint)?;
    match endpoint.scheme() {
        "http" | "ws" => {
            if is_loopback(host) {
                Ok(TransportTrustPolicy::DevelopmentLoopback)
            } else {
                Err(TransportTrustError::InsecureRemoteEndpoint)
            }
        }
        "https" | "wss" if host.to_ascii_lowercase().ends_with(".ts.net") => {
            if is_magic_dns_name(host) {
                Ok(TransportTrustPolicy::BundledPublicRoots)
            } else {
                Err(TransportTrustError::InvalidEndpoint)
            }
        }
        "https" | "wss" => Ok(TransportTrustPolicy::PlatformTrust),
        _ => Err(TransportTrustError::InvalidEndpoint),
    }
}

fn is_loopback(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

fn is_magic_dns_name(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    let labels = host.split('.').collect::<Vec<_>>();
    labels.len() >= 3
        && host.ends_with(".ts.net")
        && labels.iter().all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
                && !label.starts_with('-')
                && !label.ends_with('-')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(value: &str) -> Url {
        Url::parse(value).expect("test endpoint should parse")
    }

    #[test]
    fn classifies_closed_trust_modes() {
        assert_eq!(
            classify(&url("http://127.0.0.1:8080/"), &["http", "https"]),
            Ok(TransportTrustPolicy::DevelopmentLoopback)
        );
        assert_eq!(
            classify(
                &url("https://wallet.example-tailnet.ts.net:8443/"),
                &["http", "https"]
            ),
            Ok(TransportTrustPolicy::BundledPublicRoots)
        );
        assert_eq!(
            classify(&url("https://example.com/"), &["http", "https"]),
            Ok(TransportTrustPolicy::PlatformTrust)
        );
    }

    #[test]
    fn rejects_plaintext_remote_and_noncanonical_tailnet_hosts() {
        assert_eq!(
            classify(&url("http://192.0.2.1:8080/"), &["http", "https"]),
            Err(TransportTrustError::InsecureRemoteEndpoint)
        );
        assert_eq!(
            classify(&url("wss://-bad.example.ts.net/"), &["ws", "wss"]),
            Err(TransportTrustError::InvalidEndpoint)
        );
    }

    #[test]
    fn constructs_each_supported_native_transport() {
        assert!(http_client_builder_for(&url("http://localhost:8080/")).is_ok());
        assert!(
            http_client_builder_for(&url("https://wallet.example-tailnet.ts.net:8443/")).is_ok()
        );
        assert!(websocket_connector_for(&url("ws://127.0.0.1:9944/")).is_ok());
        assert!(websocket_connector_for(&url("wss://wallet.example-tailnet.ts.net:8443/")).is_ok());
    }
}
