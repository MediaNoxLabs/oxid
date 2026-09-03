// SPDX-License-Identifier: Apache-2.0

//! Transport checks for an already-selected standalone deployment profile.

use std::{fmt, time::Duration};

use futures::future::{join, join4};
use oxid_capabilities_application::{
    DeploymentReadinessPort, DeploymentServiceReadiness, DeploymentServiceSnapshot,
    StandaloneDeploymentProfile,
};
use reqwest::{Client, redirect::Policy};
use tokio::time::timeout;
use url::{Host, Url};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);

/// Stable configuration failure that never reproduces an endpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StandaloneReadinessConfigurationError {
    InvalidEndpoint,
    RouteClassMismatch,
}

impl fmt::Display for StandaloneReadinessConfigurationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidEndpoint => "standalone readiness endpoint is invalid",
            Self::RouteClassMismatch => {
                "standalone readiness endpoints do not share the selected route class"
            }
        })
    }
}

impl std::error::Error for StandaloneReadinessConfigurationError {}

/// Bounded probes for immutable routes supplied by the repository launcher.
///
/// Debug intentionally omits every URL. The only output is the closed public
/// readiness projection from `oxid-capabilities-application`.
#[derive(Clone)]
pub struct StandaloneDeploymentReadiness {
    profile: StandaloneDeploymentProfile,
    indexer_websocket: Url,
    indexer_http: Url,
    node_websocket: Url,
    proof_server: Url,
    ssi: Option<Url>,
}

impl fmt::Debug for StandaloneDeploymentReadiness {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StandaloneDeploymentReadiness")
            .field("profile", &self.profile)
            .field("ssi_configured", &self.ssi.is_some())
            .finish_non_exhaustive()
    }
}

impl StandaloneDeploymentReadiness {
    pub fn new(
        profile: StandaloneDeploymentProfile,
        indexer_websocket_url: &str,
        indexer_http_url: &str,
        node_websocket_url: &str,
        proof_server_url: &str,
        ssi_url: Option<&str>,
    ) -> Result<Self, StandaloneReadinessConfigurationError> {
        let indexer_websocket = parse_endpoint(indexer_websocket_url)?;
        let indexer_http = parse_endpoint(indexer_http_url)?;
        let node_websocket = parse_endpoint(node_websocket_url)?;
        let proof_server = parse_endpoint(proof_server_url)?;
        let ssi = ssi_url.map(parse_endpoint).transpose()?;
        let endpoints = [
            &indexer_websocket,
            &indexer_http,
            &node_websocket,
            &proof_server,
        ];
        validate_route_class(profile, &endpoints, ssi.as_ref())?;
        validate_schemes(
            profile,
            &indexer_websocket,
            &indexer_http,
            &node_websocket,
            &proof_server,
            ssi.as_ref(),
        )?;
        Ok(Self {
            profile,
            indexer_websocket,
            indexer_http,
            node_websocket,
            proof_server,
            ssi,
        })
    }

    async fn inspect_on_runtime(&self) -> DeploymentServiceSnapshot {
        let client = match Client::builder()
            .no_proxy()
            .redirect(Policy::none())
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .user_agent("oxid-identity-wallet/0.1")
            .build()
        {
            Ok(client) => client,
            Err(_) => return DeploymentServiceSnapshot::unavailable(self.ssi.is_some()),
        };
        let indexer = probe_indexer(
            client.clone(),
            self.indexer_http.clone(),
            self.indexer_websocket.clone(),
        );
        let node = probe_websocket(self.node_websocket.clone());
        let prover = probe_http(client.clone(), self.proof_server.clone());
        let ssi = async {
            match self.ssi.clone() {
                Some(endpoint) => probe_http(client, endpoint).await,
                None => DeploymentServiceReadiness::NotConfigured,
            }
        };
        let (indexer, node, prover, ssi) = join4(indexer, node, prover, ssi).await;
        DeploymentServiceSnapshot::new(indexer, node, prover, ssi)
    }
}

impl DeploymentReadinessPort for StandaloneDeploymentReadiness {
    fn inspect(&self) -> DeploymentServiceSnapshot {
        let _ = rustls::crypto::ring::default_provider().install_default();
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_or_else(
                |_| DeploymentServiceSnapshot::unavailable(self.ssi.is_some()),
                |runtime| runtime.block_on(self.inspect_on_runtime()),
            )
    }
}

async fn probe_http(client: Client, endpoint: Url) -> DeploymentServiceReadiness {
    match client.get(endpoint).send().await {
        Ok(response) if !response.status().is_server_error() => DeploymentServiceReadiness::Ready,
        Ok(_) | Err(_) => DeploymentServiceReadiness::Unavailable,
    }
}

async fn probe_indexer(
    client: Client,
    http_endpoint: Url,
    websocket_endpoint: Url,
) -> DeploymentServiceReadiness {
    let (http, websocket) = join(
        probe_http(client, http_endpoint),
        probe_websocket(websocket_endpoint),
    )
    .await;
    if http == DeploymentServiceReadiness::Ready && websocket == DeploymentServiceReadiness::Ready {
        DeploymentServiceReadiness::Ready
    } else {
        DeploymentServiceReadiness::Unavailable
    }
}

async fn probe_websocket(endpoint: Url) -> DeploymentServiceReadiness {
    match timeout(
        REQUEST_TIMEOUT,
        tokio_tungstenite::connect_async(endpoint.as_str()),
    )
    .await
    {
        Ok(Ok(_)) => DeploymentServiceReadiness::Ready,
        Ok(Err(_)) | Err(_) => DeploymentServiceReadiness::Unavailable,
    }
}

fn parse_endpoint(value: &str) -> Result<Url, StandaloneReadinessConfigurationError> {
    let endpoint =
        Url::parse(value).map_err(|_| StandaloneReadinessConfigurationError::InvalidEndpoint)?;
    if endpoint.cannot_be_a_base()
        || endpoint.host().is_none()
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
    {
        return Err(StandaloneReadinessConfigurationError::InvalidEndpoint);
    }
    Ok(endpoint)
}

fn validate_route_class(
    profile: StandaloneDeploymentProfile,
    endpoints: &[&Url],
    ssi: Option<&Url>,
) -> Result<(), StandaloneReadinessConfigurationError> {
    let mut routes = endpoints.to_vec();
    routes.extend(ssi);
    match profile {
        StandaloneDeploymentProfile::Local => {
            if routes
                .iter()
                .all(|endpoint| endpoint.host() == Some(Host::Ipv4(std::net::Ipv4Addr::LOCALHOST)))
            {
                Ok(())
            } else {
                Err(StandaloneReadinessConfigurationError::RouteClassMismatch)
            }
        }
        StandaloneDeploymentProfile::Tailnet => {
            let Some(expected_host) = routes.first().and_then(|endpoint| endpoint.host_str())
            else {
                return Err(StandaloneReadinessConfigurationError::RouteClassMismatch);
            };
            if !is_magic_dns_name(expected_host)
                || !routes
                    .iter()
                    .all(|endpoint| endpoint.host_str() == Some(expected_host))
            {
                return Err(StandaloneReadinessConfigurationError::RouteClassMismatch);
            }
            Ok(())
        }
    }
}

fn validate_schemes(
    profile: StandaloneDeploymentProfile,
    indexer_websocket: &Url,
    indexer_http: &Url,
    node_websocket: &Url,
    proof_server: &Url,
    ssi: Option<&Url>,
) -> Result<(), StandaloneReadinessConfigurationError> {
    let (http, websocket) = match profile {
        StandaloneDeploymentProfile::Local => ("http", "ws"),
        StandaloneDeploymentProfile::Tailnet => ("https", "wss"),
    };
    if indexer_websocket.scheme() != websocket
        || node_websocket.scheme() != websocket
        || indexer_http.scheme() != http
        || proof_server.scheme() != http
        || ssi.is_some_and(|endpoint| endpoint.scheme() != http)
    {
        return Err(StandaloneReadinessConfigurationError::RouteClassMismatch);
    }
    Ok(())
}

fn is_magic_dns_name(host: &str) -> bool {
    let Some(prefix) = host.strip_suffix(".ts.net") else {
        return false;
    };
    !prefix.is_empty()
        && prefix.split('.').count() >= 2
        && prefix.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                && !label.starts_with('-')
                && !label.ends_with('-')
        })
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    use super::*;

    fn http_server() -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local test server");
        let address = listener.local_addr().expect("server address");
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept readiness request");
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request);
            stream
                .write_all(
                    b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .expect("write readiness response");
        });
        (format!("http://{address}"), handle)
    }

    fn websocket_server() -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local WebSocket server");
        let address = listener.local_addr().expect("WebSocket server address");
        let handle = thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("WebSocket test runtime");
            runtime.block_on(async move {
                let (stream, _) = listener.accept().expect("accept readiness WebSocket");
                stream
                    .set_nonblocking(true)
                    .expect("configure readiness WebSocket stream");
                tokio_tungstenite::accept_async(
                    tokio::net::TcpStream::from_std(stream)
                        .expect("convert readiness WebSocket stream"),
                )
                .await
                .expect("complete readiness WebSocket handshake");
            });
        });
        // Local-only test listener; Tailnet validation below requires WSS.
        (format!("{}://{address}", "ws"), handle)
    }

    #[test]
    fn local_probes_are_independent_and_ssi_is_explicit() {
        let (indexer, indexer_handle) = http_server();
        let (indexer_websocket, indexer_websocket_handle) = websocket_server();
        let (node, node_handle) = http_server();
        let (prover, prover_handle) = http_server();
        let (ssi, ssi_handle) = http_server();
        let readiness = StandaloneDeploymentReadiness::new(
            StandaloneDeploymentProfile::Local,
            &format!("{indexer_websocket}/graphql/ws"),
            &indexer,
            // Local-only negative fixture; it never enters Tailnet composition.
            &node.replacen("http://", &format!("{}://", "ws"), 1),
            &prover,
            Some(&ssi),
        )
        .expect("valid local routes");

        let snapshot = readiness.inspect();

        assert_eq!(snapshot.indexer(), DeploymentServiceReadiness::Ready);
        assert_eq!(snapshot.node(), DeploymentServiceReadiness::Unavailable);
        assert_eq!(snapshot.prover(), DeploymentServiceReadiness::Ready);
        assert_eq!(snapshot.ssi(), DeploymentServiceReadiness::Ready);
        indexer_handle.join().expect("indexer test server");
        indexer_websocket_handle
            .join()
            .expect("indexer WebSocket test server");
        node_handle.join().expect("node test server");
        prover_handle.join().expect("prover test server");
        ssi_handle.join().expect("ssi test server");
    }

    #[test]
    fn tailnet_requires_one_tls_magic_dns_identity() {
        let valid = StandaloneDeploymentReadiness::new(
            StandaloneDeploymentProfile::Tailnet,
            "wss://laptop.example-tailnet.ts.net:8443/graphql/ws",
            "https://laptop.example-tailnet.ts.net:8443/graphql",
            "wss://laptop.example-tailnet.ts.net:10000",
            "https://laptop.example-tailnet.ts.net",
            Some("https://laptop.example-tailnet.ts.net/issuer"),
        );
        assert!(valid.is_ok());
        let insecure_indexer = format!("{}://100.64.0.1:8443/graphql/ws", "ws");
        let insecure_node = format!("{}://100.64.0.1:10000", "ws");
        assert!(matches!(
            StandaloneDeploymentReadiness::new(
                StandaloneDeploymentProfile::Tailnet,
                // Deliberately insecure inputs prove Tailnet validation fails closed.
                &insecure_indexer,
                "http://100.64.0.1:8443/graphql",
                &insecure_node,
                "http://100.64.0.1",
                None,
            ),
            Err(StandaloneReadinessConfigurationError::RouteClassMismatch)
        ));
        assert!(matches!(
            StandaloneDeploymentReadiness::new(
                StandaloneDeploymentProfile::Tailnet,
                "wss://laptop.example-tailnet.ts.net:8443/graphql/ws",
                "https://other.example-tailnet.ts.net:8443/graphql",
                "wss://laptop.example-tailnet.ts.net:10000",
                "https://laptop.example-tailnet.ts.net",
                None,
            ),
            Err(StandaloneReadinessConfigurationError::RouteClassMismatch)
        ));
    }

    #[test]
    fn endpoint_validation_rejects_embedded_authority_and_request_data() {
        for indexer_http in [
            "https://owner:secret@laptop.example-tailnet.ts.net:8443/graphql",
            "https://laptop.example-tailnet.ts.net:8443/graphql?token=secret",
            "https://laptop.example-tailnet.ts.net:8443/graphql#secret",
        ] {
            assert!(matches!(
                StandaloneDeploymentReadiness::new(
                    StandaloneDeploymentProfile::Tailnet,
                    "wss://laptop.example-tailnet.ts.net:8443/graphql/ws",
                    indexer_http,
                    "wss://laptop.example-tailnet.ts.net:10000",
                    "https://laptop.example-tailnet.ts.net",
                    None,
                ),
                Err(StandaloneReadinessConfigurationError::InvalidEndpoint)
            ));
        }
    }

    #[test]
    fn debug_and_errors_do_not_reproduce_routes() {
        let readiness = StandaloneDeploymentReadiness::new(
            StandaloneDeploymentProfile::Local,
            &format!("{}://127.0.0.1:8088/graphql/ws", "ws"),
            "http://127.0.0.1:8088/graphql",
            &format!("{}://127.0.0.1:9944", "ws"),
            "http://127.0.0.1:6300",
            None,
        )
        .expect("valid local routes");
        let rendered = format!("{readiness:?}");
        assert!(!rendered.contains("127.0.0.1"));
        assert!(!rendered.contains("8088"));
    }
}
