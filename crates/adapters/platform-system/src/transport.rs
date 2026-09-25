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
            http_client_builder_with_roots(webpki_root_certs::TLS_SERVER_ROOT_CERTS.iter())
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
            websocket_connector_with_roots(webpki_root_certs::TLS_SERVER_ROOT_CERTS.iter().cloned())
                .map(Some)
        }
    }
}

fn http_client_builder_with_roots<I, C>(roots: I) -> Result<ClientBuilder, TransportTrustError>
where
    I: IntoIterator<Item = C>,
    C: AsRef<[u8]>,
{
    let roots = roots
        .into_iter()
        .map(|certificate| Certificate::from_der(certificate.as_ref()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| TransportTrustError::TlsConfigurationUnavailable)?;
    if roots.is_empty() {
        return Err(TransportTrustError::TlsConfigurationUnavailable);
    }
    Ok(reqwest::Client::builder().tls_certs_only(roots))
}

fn websocket_connector_with_roots<I>(roots: I) -> Result<Connector, TransportTrustError>
where
    I: IntoIterator<Item = rustls::pki_types::CertificateDer<'static>>,
{
    let mut store = RootCertStore::empty();
    let (accepted, _) = store.add_parsable_certificates(roots);
    if accepted == 0 {
        return Err(TransportTrustError::TlsConfigurationUnavailable);
    }
    let config = ClientConfig::builder()
        .with_root_certificates(store)
        .with_no_client_auth();
    Ok(Connector::Rustls(Arc::new(config)))
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
        .ok_or(TransportTrustError::InvalidEndpoint)?
        .trim_matches(['[', ']']);
    if host.parse::<IpAddr>().is_ok_and(is_tailnet_address) {
        return Err(TransportTrustError::InvalidEndpoint);
    }
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

fn is_tailnet_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => {
            let octets = address.octets();
            octets[0] == 100 && (64..=127).contains(&octets[1])
        }
        IpAddr::V6(address) => address.segments()[..3] == [0xfd7a, 0x115c, 0xa1e0],
    }
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
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        thread,
        time::Duration,
    };

    use rcgen::{CertificateParams, KeyPair, date_time_ymd};
    use rustls::{
        ServerConfig, ServerConnection, StreamOwned,
        pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer},
    };
    use tokio_tungstenite::{connect_async_tls_with_config as connect_test_websocket, tungstenite};

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum CertificateCase {
        Valid,
        Expired,
        UnknownChain,
        HostnameMismatch,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum HandshakeOutcome {
        Accepted,
        Rejected,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum LifecyclePhase {
        InitialConnect,
        Resume,
        ReconnectAfterDisconnect,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum TransportClass {
        Http,
        WebSocket,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum FailureClass {
        InvalidEndpoint,
        InsecureRemoteEndpoint,
        TlsConfigurationUnavailable,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct FailureEvidence {
        phase: LifecyclePhase,
        transport: TransportClass,
        failure: FailureClass,
    }

    #[derive(Clone, Copy)]
    enum ServerProtocol {
        Http,
        WebSocket,
    }

    struct TestIdentity {
        certificate: CertificateDer<'static>,
        private_key: PrivateKeyDer<'static>,
    }

    fn url(value: &str) -> Url {
        Url::parse(value).expect("test endpoint should parse")
    }

    fn identity(hostname: &str, expired: bool) -> TestIdentity {
        let signing_key = KeyPair::generate().expect("test signing key should generate");
        let mut params = CertificateParams::new(vec![hostname.to_owned()])
            .expect("test certificate parameters should be valid");
        if expired {
            params.not_before = date_time_ymd(2000, 1, 1);
            params.not_after = date_time_ymd(2001, 1, 1);
        }
        let certificate = params
            .self_signed(&signing_key)
            .expect("test certificate should be created")
            .der()
            .clone();
        let private_key = PrivatePkcs8KeyDer::from(signing_key.serialize_der()).into();
        TestIdentity {
            certificate,
            private_key,
        }
    }

    fn fixture(case: CertificateCase) -> (TestIdentity, CertificateDer<'static>) {
        let hostname = if case == CertificateCase::HostnameMismatch {
            "not-localhost.invalid"
        } else {
            "localhost"
        };
        let server = identity(hostname, case == CertificateCase::Expired);
        let trusted = if case == CertificateCase::UnknownChain {
            identity("localhost", false).certificate
        } else {
            server.certificate.clone()
        };
        (server, trusted)
    }

    fn spawn_tls_server(
        protocol: ServerProtocol,
        identity: TestIdentity,
    ) -> (u16, thread::JoinHandle<()>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("test TLS listener should bind");
        let port = listener
            .local_addr()
            .expect("test TLS listener should have an address")
            .port();
        let config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![identity.certificate], identity.private_key)
            .expect("test TLS server configuration should be valid");
        let handle = thread::spawn(move || {
            let Ok((socket, _)) = listener.accept() else {
                return;
            };
            bound_socket(&socket);
            let Ok(connection) = ServerConnection::new(Arc::new(config)) else {
                return;
            };
            let mut stream = StreamOwned::new(connection, socket);
            match protocol {
                ServerProtocol::Http => {
                    let mut request = [0_u8; 1024];
                    if stream.read(&mut request).is_ok() {
                        let _ = stream.write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
                        );
                        let _ = stream.flush();
                    }
                }
                ServerProtocol::WebSocket => {
                    if let Ok(mut socket) = tungstenite::accept(stream) {
                        let _ = socket.close(None);
                    }
                }
            }
        });
        (port, handle)
    }

    fn bound_socket(socket: &TcpStream) {
        let timeout = Some(Duration::from_secs(3));
        socket
            .set_read_timeout(timeout)
            .expect("test TLS read timeout should install");
        socket
            .set_write_timeout(timeout)
            .expect("test TLS write timeout should install");
    }

    fn expected(case: CertificateCase) -> HandshakeOutcome {
        if case == CertificateCase::Valid {
            HandshakeOutcome::Accepted
        } else {
            HandshakeOutcome::Rejected
        }
    }

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime should build")
    }

    fn recreate_transport(
        phase: LifecyclePhase,
        transport: TransportClass,
        endpoint: &Url,
    ) -> Result<TransportTrustPolicy, FailureEvidence> {
        let (schemes, result) = match transport {
            TransportClass::Http => (
                &["http", "https"][..],
                http_client_builder_for(endpoint).map(|_| ()),
            ),
            TransportClass::WebSocket => (
                &["ws", "wss"][..],
                websocket_connector_for(endpoint).map(|_| ()),
            ),
        };
        result.map_err(|failure| FailureEvidence {
            phase,
            transport,
            failure: failure.into(),
        })?;
        classify(endpoint, schemes).map_err(|failure| FailureEvidence {
            phase,
            transport,
            failure: failure.into(),
        })
    }

    impl From<TransportTrustError> for FailureClass {
        fn from(value: TransportTrustError) -> Self {
            match value {
                TransportTrustError::InvalidEndpoint => Self::InvalidEndpoint,
                TransportTrustError::InsecureRemoteEndpoint => Self::InsecureRemoteEndpoint,
                TransportTrustError::TlsConfigurationUnavailable => {
                    Self::TlsConfigurationUnavailable
                }
            }
        }
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
    fn rejects_remote_plaintext_and_direct_tailnet_ips_for_both_transports() {
        for (scheme, schemes) in [("http", &["http", "https"][..]), ("ws", &["ws", "wss"][..])] {
            let endpoint = format!("{scheme}://192.0.2.1:8080/");
            assert_eq!(
                classify(&url(&endpoint), schemes),
                Err(TransportTrustError::InsecureRemoteEndpoint)
            );
        }
        for endpoint in ["https://100.64.0.1/", "wss://[fd7a:115c:a1e0::1]/"] {
            let schemes = if endpoint.starts_with("https") {
                &["http", "https"][..]
            } else {
                &["ws", "wss"][..]
            };
            assert_eq!(
                classify(&url(endpoint), schemes),
                Err(TransportTrustError::InvalidEndpoint)
            );
        }
    }

    #[test]
    fn gives_http_and_websocket_the_same_secure_policy_matrix() {
        for (http, websocket, expected) in [
            (
                "https://public.example/",
                "wss://public.example/",
                TransportTrustPolicy::PlatformTrust,
            ),
            (
                "https://wallet.example-tailnet.ts.net/",
                "wss://wallet.example-tailnet.ts.net/",
                TransportTrustPolicy::BundledPublicRoots,
            ),
        ] {
            assert_eq!(classify(&url(http), &["http", "https"]), Ok(expected));
            assert_eq!(classify(&url(websocket), &["ws", "wss"]), Ok(expected));
        }
        assert_eq!(
            classify(&url("wss://-bad.example.ts.net/"), &["ws", "wss"]),
            Err(TransportTrustError::InvalidEndpoint)
        );
    }

    #[test]
    fn recreation_preserves_trust_after_resume_and_disconnect() {
        for (transport, endpoint) in [
            (
                TransportClass::Http,
                url("https://wallet.example-tailnet.ts.net/"),
            ),
            (
                TransportClass::WebSocket,
                url("wss://wallet.example-tailnet.ts.net/"),
            ),
        ] {
            for phase in [
                LifecyclePhase::InitialConnect,
                LifecyclePhase::Resume,
                LifecyclePhase::ReconnectAfterDisconnect,
            ] {
                assert_eq!(
                    recreate_transport(phase, transport, &endpoint),
                    Ok(TransportTrustPolicy::BundledPublicRoots),
                    "{transport:?} changed trust policy during {phase:?}"
                );
            }
        }
    }

    #[test]
    fn recreation_failure_evidence_is_closed_and_never_falls_back() {
        for (transport, endpoint) in [
            (TransportClass::Http, url("https://100.64.0.1/")),
            (TransportClass::WebSocket, url("wss://[fd7a:115c:a1e0::1]/")),
        ] {
            for phase in [
                LifecyclePhase::InitialConnect,
                LifecyclePhase::Resume,
                LifecyclePhase::ReconnectAfterDisconnect,
            ] {
                assert_eq!(
                    recreate_transport(phase, transport, &endpoint),
                    Err(FailureEvidence {
                        phase,
                        transport,
                        failure: FailureClass::InvalidEndpoint,
                    })
                );
            }
        }
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

    #[test]
    fn http_rejects_invalid_certificate_classes() {
        ensure_crypto_provider();
        for case in [
            CertificateCase::Valid,
            CertificateCase::Expired,
            CertificateCase::UnknownChain,
            CertificateCase::HostnameMismatch,
        ] {
            let (server, trusted) = fixture(case);
            let (port, handle) = spawn_tls_server(ServerProtocol::Http, server);
            let client = http_client_builder_with_roots([trusted])
                .expect("test HTTP trust should configure")
                .build()
                .expect("test HTTP client should build");
            let accepted = runtime()
                .block_on(client.get(format!("https://localhost:{port}/")).send())
                .is_ok();
            let outcome = if accepted {
                HandshakeOutcome::Accepted
            } else {
                HandshakeOutcome::Rejected
            };
            assert_eq!(outcome, expected(case), "HTTP {case:?}");
            handle.join().expect("test HTTP TLS server should stop");
        }
    }

    #[test]
    fn websocket_rejects_invalid_certificate_classes() {
        ensure_crypto_provider();
        for case in [
            CertificateCase::Valid,
            CertificateCase::Expired,
            CertificateCase::UnknownChain,
            CertificateCase::HostnameMismatch,
        ] {
            let (server, trusted) = fixture(case);
            let (port, handle) = spawn_tls_server(ServerProtocol::WebSocket, server);
            let connector = websocket_connector_with_roots([trusted])
                .expect("test WebSocket trust should configure");
            let accepted = runtime()
                .block_on(connect_test_websocket(
                    format!("wss://localhost:{port}/"),
                    None,
                    false,
                    Some(connector),
                ))
                .is_ok();
            let outcome = if accepted {
                HandshakeOutcome::Accepted
            } else {
                HandshakeOutcome::Rejected
            };
            assert_eq!(outcome, expected(case), "WebSocket {case:?}");
            handle
                .join()
                .expect("test WebSocket TLS server should stop");
        }
    }
}
