// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use oxid_identity_application::{
    DidResolutionPort, DidResolutionPortError, DidResolutionPortFuture,
};

mod lifecycle;

pub use lifecycle::StandaloneDidLifecycle;
use oxid_identity_domain::{
    DID_CONTEXT, DidDocument, DidDocumentMetadata, DidDocumentParts, DidResolution,
    DidResolutionMetadata, DidResolutionSource, JWK_CONTEXT, JwkCurve, JwkKeyType, MidnightDid,
    PublicJwk, Service, ServiceEndpointValue, VerificationMethod, VerificationRelationship,
    VerificationRelationshipEntry,
};
use serde_json::{Map, Value, json};

pub const STANDALONE_FIXTURE_DID: &str =
    "did:midnight:undeployed:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const MAX_RESPONSE_BYTES: usize = 512 * 1_024;
const MAX_JSON_DEPTH: usize = 16;
const MAX_METADATA_TEXT: usize = 2_048;

/// Deterministic resolver for the single documented standalone DID. It never
/// manufactures successful results for arbitrary identifiers.
#[derive(Clone, Copy, Debug, Default)]
pub struct StandaloneDidResolver;

impl DidResolutionPort for StandaloneDidResolver {
    fn resolve<'a>(&'a self, did: &'a MidnightDid) -> DidResolutionPortFuture<'a> {
        let result = if did.as_str() == STANDALONE_FIXTURE_DID {
            standalone_resolution()
        } else {
            Err(DidResolutionPortError::NotFound)
        };
        Box::pin(async move { result })
    }
}

fn standalone_resolution() -> Result<DidResolution, DidResolutionPortError> {
    let did = MidnightDid::parse(STANDALONE_FIXTURE_DID)
        .map_err(|_| DidResolutionPortError::InvalidResponse)?;
    let signing = VerificationMethod::new(
        &did,
        "#authentication-1",
        did.clone(),
        PublicJwk::new(JwkKeyType::Okp, JwkCurve::Ed25519, "A".repeat(43), None)
            .map_err(|_| DidResolutionPortError::InvalidResponse)?,
    )
    .map_err(|_| DidResolutionPortError::InvalidResponse)?;
    let agreement = VerificationMethod::new(
        &did,
        "#key-agreement-1",
        did.clone(),
        PublicJwk::new(JwkKeyType::Okp, JwkCurve::X25519, "A".repeat(43), None)
            .map_err(|_| DidResolutionPortError::InvalidResponse)?,
    )
    .map_err(|_| DidResolutionPortError::InvalidResponse)?;
    let service = Service::new(
        "#wallet",
        vec!["IdentityWallet".to_owned()],
        vec![
            ServiceEndpointValue::uri("https://wallet.example.invalid/oxid")
                .map_err(|_| DidResolutionPortError::InvalidResponse)?,
        ],
        false,
    )
    .map_err(|_| DidResolutionPortError::InvalidResponse)?;
    let document = DidDocument::new(DidDocumentParts {
        contexts: vec![DID_CONTEXT.to_owned(), JWK_CONTEXT.to_owned()],
        id: did.clone(),
        controllers: vec![did],
        also_known_as: vec!["https://wallet.example.invalid/profiles/standalone".to_owned()],
        verification_methods: vec![signing, agreement],
        relationships: vec![
            VerificationRelationshipEntry::new(
                VerificationRelationship::Authentication,
                vec!["#authentication-1".to_owned()],
            ),
            VerificationRelationshipEntry::new(
                VerificationRelationship::KeyAgreement,
                vec!["#key-agreement-1".to_owned()],
            ),
        ],
        services: vec![service],
    })
    .map_err(|_| DidResolutionPortError::InvalidResponse)?;
    Ok(DidResolution::new(
        document,
        DidDocumentMetadata {
            version_id: Some("standalone-fixture-v1".to_owned()),
            ..DidDocumentMetadata::default()
        },
        DidResolutionMetadata {
            content_type: Some("application/did+ld+json".to_owned()),
        },
        DidResolutionSource::Standalone,
    ))
}

#[cfg(not(target_arch = "wasm32"))]
mod http {
    use std::{error::Error, fmt, net::IpAddr, sync::Arc, thread, time::Duration};

    use futures::{StreamExt as _, channel::oneshot};
    use reqwest::{Certificate, Client, Url, redirect::Policy};

    use super::*;

    const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum HttpDidResolverConfigError {
        InvalidUrl,
        UnsupportedScheme,
        InsecureRemoteTransport,
        CredentialsNotAllowed,
        QueryNotAllowed,
        FragmentNotAllowed,
        RouteTooLong,
        ClientUnavailable,
    }

    impl fmt::Display for HttpDidResolverConfigError {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str(match self {
                Self::InvalidUrl => "DID resolver URL is invalid",
                Self::UnsupportedScheme => "DID resolver URL must use HTTP or HTTPS",
                Self::InsecureRemoteTransport => "non-loopback DID resolver URLs must use HTTPS",
                Self::CredentialsNotAllowed => "DID resolver URL must not contain credentials",
                Self::QueryNotAllowed => "DID resolver URL must not contain a query",
                Self::FragmentNotAllowed => "DID resolver URL must not contain a fragment",
                Self::RouteTooLong => "DID resolver URL is too long",
                Self::ClientUnavailable => "DID resolver HTTP client is unavailable",
            })
        }
    }

    impl Error for HttpDidResolverConfigError {}

    #[derive(Clone)]
    pub struct HttpDidResolverConfig {
        endpoint: Url,
        client: Client,
    }

    impl HttpDidResolverConfig {
        pub fn new(value: impl AsRef<str>) -> Result<Self, HttpDidResolverConfigError> {
            let value = value.as_ref();
            if value.len() > 2_048 {
                return Err(HttpDidResolverConfigError::RouteTooLong);
            }
            let mut base = Url::parse(value).map_err(|_| HttpDidResolverConfigError::InvalidUrl)?;
            if !base.username().is_empty() || base.password().is_some() {
                return Err(HttpDidResolverConfigError::CredentialsNotAllowed);
            }
            if base.query().is_some() {
                return Err(HttpDidResolverConfigError::QueryNotAllowed);
            }
            if base.fragment().is_some() {
                return Err(HttpDidResolverConfigError::FragmentNotAllowed);
            }
            if base.host_str().is_none() {
                return Err(HttpDidResolverConfigError::InvalidUrl);
            }
            match base.scheme() {
                "https" => {}
                "http" if host_is_loopback(&base) => {}
                "http" => return Err(HttpDidResolverConfigError::InsecureRemoteTransport),
                _ => return Err(HttpDidResolverConfigError::UnsupportedScheme),
            }
            if !base.path().ends_with('/') {
                let path = format!("{}/", base.path());
                base.set_path(&path);
            }
            let endpoint = base
                .join("resolve")
                .map_err(|_| HttpDidResolverConfigError::InvalidUrl)?;
            let trusted_roots = webpki_root_certs::TLS_SERVER_ROOT_CERTS
                .iter()
                .map(|certificate| Certificate::from_der(certificate.as_ref()))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| HttpDidResolverConfigError::ClientUnavailable)?;
            let client = Client::builder()
                .no_proxy()
                .redirect(Policy::none())
                .timeout(REQUEST_TIMEOUT)
                .user_agent("oxid-identity-wallet/0.1")
                .tls_certs_only(trusted_roots)
                .build()
                .map_err(|_| HttpDidResolverConfigError::ClientUnavailable)?;
            Ok(Self { endpoint, client })
        }
    }

    fn host_is_loopback(url: &Url) -> bool {
        url.host_str().is_some_and(|host| {
            host.eq_ignore_ascii_case("localhost")
                || host
                    .parse::<IpAddr>()
                    .is_ok_and(|address| address.is_loopback())
        })
    }

    #[derive(Clone)]
    pub struct HttpDidResolver(Arc<HttpDidResolverConfig>);

    impl HttpDidResolver {
        #[must_use]
        pub fn new(config: HttpDidResolverConfig) -> Self {
            Self(Arc::new(config))
        }

        async fn resolve_on_runtime(
            config: Arc<HttpDidResolverConfig>,
            did: MidnightDid,
        ) -> Result<DidResolution, DidResolutionPortError> {
            let response = config
                .client
                .post(config.endpoint.clone())
                .json(&json!({ "did": did.as_str() }))
                .send()
                .await
                .map_err(|_| DidResolutionPortError::Unavailable)?;
            let status = response.status();
            if status.as_u16() == 404 {
                return Err(DidResolutionPortError::NotFound);
            }
            if status.as_u16() == 400 {
                return Err(DidResolutionPortError::InvalidDid);
            }
            if status.as_u16() == 405 || status.as_u16() == 501 {
                return Err(DidResolutionPortError::MethodNotSupported);
            }
            if status.is_client_error() {
                return Err(DidResolutionPortError::Rejected);
            }
            if !status.is_success() {
                return Err(DidResolutionPortError::Unavailable);
            }
            if response
                .content_length()
                .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
            {
                return Err(DidResolutionPortError::InvalidResponse);
            }
            let mut body = Vec::new();
            let mut stream = response.bytes_stream();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|_| DidResolutionPortError::Unavailable)?;
                if body.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
                    return Err(DidResolutionPortError::InvalidResponse);
                }
                body.extend_from_slice(&chunk);
            }
            parse_resolution_bytes(&body, DidResolutionSource::Live)
        }
    }

    impl DidResolutionPort for HttpDidResolver {
        fn resolve<'a>(&'a self, did: &'a MidnightDid) -> DidResolutionPortFuture<'a> {
            let config = Arc::clone(&self.0);
            let did = did.clone();
            let (sender, receiver) = oneshot::channel();
            let spawned = thread::Builder::new()
                .name("oxid-did-resolver".to_owned())
                .spawn(move || {
                    let result = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|_| DidResolutionPortError::Unavailable)
                        .and_then(|runtime| {
                            runtime.block_on(Self::resolve_on_runtime(config, did))
                        });
                    let _ = sender.send(result);
                });
            if spawned.is_err() {
                return Box::pin(async { Err(DidResolutionPortError::Unavailable) });
            }
            Box::pin(async move {
                receiver
                    .await
                    .unwrap_or(Err(DidResolutionPortError::Unavailable))
            })
        }
    }

    pub use self::HttpDidResolver as Resolver;
    pub use self::HttpDidResolverConfig as Config;
    pub use self::HttpDidResolverConfigError as ConfigError;
}

#[cfg(not(target_arch = "wasm32"))]
pub use http::{
    Config as HttpDidResolverConfig, ConfigError as HttpDidResolverConfigError,
    Resolver as HttpDidResolver,
};

/// Parse a bounded official DID Resolution Result. Kept public so persisted
/// records can be validated through the same untrusted-input boundary.
pub fn parse_resolution_bytes(
    bytes: &[u8],
    source: DidResolutionSource,
) -> Result<DidResolution, DidResolutionPortError> {
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(DidResolutionPortError::InvalidResponse);
    }
    let value: Value =
        serde_json::from_slice(bytes).map_err(|_| DidResolutionPortError::InvalidResponse)?;
    if json_depth(&value, 0) > MAX_JSON_DEPTH {
        return Err(DidResolutionPortError::InvalidResponse);
    }
    parse_resolution_value(&value, source)
}

fn json_depth(value: &Value, depth: usize) -> usize {
    match value {
        Value::Array(values) => values
            .iter()
            .map(|value| json_depth(value, depth + 1))
            .max()
            .unwrap_or(depth),
        Value::Object(values) => values
            .values()
            .map(|value| json_depth(value, depth + 1))
            .max()
            .unwrap_or(depth),
        _ => depth,
    }
}

fn object(value: &Value) -> Result<&Map<String, Value>, DidResolutionPortError> {
    value
        .as_object()
        .ok_or(DidResolutionPortError::InvalidResponse)
}

fn required_string<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    max: usize,
) -> Result<&'a str, DidResolutionPortError> {
    let value = object
        .get(key)
        .and_then(Value::as_str)
        .ok_or(DidResolutionPortError::InvalidResponse)?;
    if value.is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(DidResolutionPortError::InvalidResponse);
    }
    Ok(value)
}

fn optional_string(
    object: &Map<String, Value>,
    key: &str,
) -> Result<Option<String>, DidResolutionPortError> {
    match object.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value))
            if !value.is_empty()
                && value.len() <= MAX_METADATA_TEXT
                && !value.chars().any(char::is_control) =>
        {
            Ok(Some(value.clone()))
        }
        _ => Err(DidResolutionPortError::InvalidResponse),
    }
}

fn parse_resolution_value(
    value: &Value,
    source: DidResolutionSource,
) -> Result<DidResolution, DidResolutionPortError> {
    let root = object(value)?;
    let resolution_metadata = object(
        root.get("didResolutionMetadata")
            .ok_or(DidResolutionPortError::InvalidResponse)?,
    )?;
    if let Some(error) = optional_string(resolution_metadata, "error")? {
        return Err(match error.as_str() {
            "notFound" => DidResolutionPortError::NotFound,
            "invalidDid" => DidResolutionPortError::InvalidDid,
            "methodNotSupported" => DidResolutionPortError::MethodNotSupported,
            _ => DidResolutionPortError::Rejected,
        });
    }
    let document = parse_document(
        root.get("didDocument")
            .ok_or(DidResolutionPortError::InvalidResponse)?,
    )?;
    let metadata = object(
        root.get("didDocumentMetadata")
            .ok_or(DidResolutionPortError::InvalidResponse)?,
    )?;
    let equivalent_ids =
        parse_optional_string_array(metadata.get("equivalentId"), 128, MAX_METADATA_TEXT)?;
    let deactivated = match metadata.get("deactivated") {
        None | Some(Value::Null) => None,
        Some(Value::Bool(value)) => Some(*value),
        _ => return Err(DidResolutionPortError::InvalidResponse),
    };
    let content_type = optional_string(resolution_metadata, "contentType")?;
    if content_type
        .as_deref()
        .is_some_and(|value| !matches!(value, "application/did+ld+json" | "application/did+json"))
    {
        return Err(DidResolutionPortError::InvalidResponse);
    }
    Ok(DidResolution::new(
        document,
        DidDocumentMetadata {
            created: optional_string(metadata, "created")?,
            updated: optional_string(metadata, "updated")?,
            deactivated,
            version_id: optional_string(metadata, "versionId")?,
            next_update: optional_string(metadata, "nextUpdate")?,
            next_version_id: optional_string(metadata, "nextVersionId")?,
            equivalent_ids,
            canonical_id: optional_string(metadata, "canonicalId")?,
        },
        DidResolutionMetadata { content_type },
        source,
    ))
}

fn parse_document(value: &Value) -> Result<DidDocument, DidResolutionPortError> {
    let document = object(value)?;
    let id = MidnightDid::parse(required_string(
        document,
        "id",
        MidnightDid::MAX_CHARACTERS,
    )?)
    .map_err(|_| DidResolutionPortError::InvalidResponse)?;
    let contexts = parse_required_string_array(document.get("@context"), 32, MAX_METADATA_TEXT)?;
    let controllers = match document.get("controller") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::String(value)) => vec![
            MidnightDid::parse(value.clone())
                .map_err(|_| DidResolutionPortError::InvalidResponse)?,
        ],
        Some(Value::Array(_)) => {
            parse_required_string_array(document.get("controller"), 1, MidnightDid::MAX_CHARACTERS)?
                .into_iter()
                .map(|value| {
                    MidnightDid::parse(value).map_err(|_| DidResolutionPortError::InvalidResponse)
                })
                .collect::<Result<Vec<_>, _>>()?
        }
        _ => return Err(DidResolutionPortError::InvalidResponse),
    };
    let aliases = parse_optional_string_array(document.get("alsoKnownAs"), 128, MAX_METADATA_TEXT)?;
    if aliases
        .iter()
        .any(|value| reqwest::Url::parse(value).is_err())
    {
        return Err(DidResolutionPortError::InvalidResponse);
    }
    let methods = parse_methods(document.get("verificationMethod"), &id)?;
    let relationships = [
        ("authentication", VerificationRelationship::Authentication),
        ("assertionMethod", VerificationRelationship::AssertionMethod),
        ("keyAgreement", VerificationRelationship::KeyAgreement),
        (
            "capabilityInvocation",
            VerificationRelationship::CapabilityInvocation,
        ),
        (
            "capabilityDelegation",
            VerificationRelationship::CapabilityDelegation,
        ),
    ]
    .into_iter()
    .filter_map(|(key, relationship)| {
        document.get(key).map(|value| {
            parse_required_string_array(Some(value), DidDocument::MAX_METHODS, MAX_METADATA_TEXT)
                .map(|ids| VerificationRelationshipEntry::new(relationship, ids))
        })
    })
    .collect::<Result<Vec<_>, _>>()?;
    let services = parse_services(document.get("service"))?;
    DidDocument::new(DidDocumentParts {
        contexts,
        id,
        controllers,
        also_known_as: aliases,
        verification_methods: methods,
        relationships,
        services,
    })
    .map_err(|_| DidResolutionPortError::InvalidResponse)
}

fn parse_required_string_array(
    value: Option<&Value>,
    max_items: usize,
    max_text: usize,
) -> Result<Vec<String>, DidResolutionPortError> {
    let values = value
        .and_then(Value::as_array)
        .ok_or(DidResolutionPortError::InvalidResponse)?;
    if values.is_empty() || values.len() > max_items {
        return Err(DidResolutionPortError::InvalidResponse);
    }
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|value| {
                    !value.is_empty()
                        && value.len() <= max_text
                        && !value.chars().any(char::is_control)
                })
                .map(str::to_owned)
                .ok_or(DidResolutionPortError::InvalidResponse)
        })
        .collect()
}

fn parse_optional_string_array(
    value: Option<&Value>,
    max_items: usize,
    max_text: usize,
) -> Result<Vec<String>, DidResolutionPortError> {
    match value {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(values)) if values.is_empty() => Ok(Vec::new()),
        Some(value) => parse_required_string_array(Some(value), max_items, max_text),
    }
}

fn parse_methods(
    value: Option<&Value>,
    subject: &MidnightDid,
) -> Result<Vec<VerificationMethod>, DidResolutionPortError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .ok_or(DidResolutionPortError::InvalidResponse)?;
    if values.len() > DidDocument::MAX_METHODS {
        return Err(DidResolutionPortError::InvalidResponse);
    }
    values
        .iter()
        .map(|value| {
            let method = object(value)?;
            if required_string(method, "type", 64)? != "JsonWebKey" {
                return Err(DidResolutionPortError::InvalidResponse);
            }
            let controller = MidnightDid::parse(required_string(
                method,
                "controller",
                MidnightDid::MAX_CHARACTERS,
            )?)
            .map_err(|_| DidResolutionPortError::InvalidResponse)?;
            let jwk_object = object(
                method
                    .get("publicKeyJwk")
                    .ok_or(DidResolutionPortError::InvalidResponse)?,
            )?;
            if jwk_object.contains_key("d") {
                return Err(DidResolutionPortError::InvalidResponse);
            }
            let key_type = match required_string(jwk_object, "kty", 8)? {
                "OKP" => JwkKeyType::Okp,
                "EC" => JwkKeyType::Ec,
                _ => return Err(DidResolutionPortError::InvalidResponse),
            };
            let curve = match required_string(jwk_object, "crv", 32)? {
                "Ed25519" => JwkCurve::Ed25519,
                "X25519" => JwkCurve::X25519,
                "Jubjub" => JwkCurve::Jubjub,
                "P-256" => JwkCurve::P256,
                "secp256k1" => JwkCurve::Secp256k1,
                "BLS12381G1" => JwkCurve::Bls12381G1,
                "BLS12381G2" => JwkCurve::Bls12381G2,
                _ => return Err(DidResolutionPortError::InvalidResponse),
            };
            let x = required_string(jwk_object, "x", 128)?.to_owned();
            let y = optional_string(jwk_object, "y")?;
            let jwk = PublicJwk::new(key_type, curve, x, y)
                .map_err(|_| DidResolutionPortError::InvalidResponse)?;
            VerificationMethod::new(
                subject,
                required_string(method, "id", MAX_METADATA_TEXT)?,
                controller,
                jwk,
            )
            .map_err(|_| DidResolutionPortError::InvalidResponse)
        })
        .collect()
}

fn parse_services(value: Option<&Value>) -> Result<Vec<Service>, DidResolutionPortError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .ok_or(DidResolutionPortError::InvalidResponse)?;
    if values.len() > DidDocument::MAX_SERVICES {
        return Err(DidResolutionPortError::InvalidResponse);
    }
    values
        .iter()
        .map(|value| {
            let service = object(value)?;
            let types = match service.get("type") {
                Some(Value::String(value)) if !value.is_empty() => vec![value.clone()],
                Some(Value::Array(_)) => parse_required_string_array(service.get("type"), 32, 128)?,
                _ => return Err(DidResolutionPortError::InvalidResponse),
            };
            let endpoint = service
                .get("serviceEndpoint")
                .ok_or(DidResolutionPortError::InvalidResponse)?;
            let (values, was_array) = match endpoint {
                Value::Array(values) if !values.is_empty() && values.len() <= 64 => {
                    (values.clone(), true)
                }
                Value::Array(_) => return Err(DidResolutionPortError::InvalidResponse),
                value => (vec![value.clone()], false),
            };
            let endpoints = values
                .iter()
                .map(|value| match value {
                    Value::String(value) if reqwest::Url::parse(value).is_ok() => {
                        ServiceEndpointValue::uri(value.clone())
                            .map_err(|_| DidResolutionPortError::InvalidResponse)
                    }
                    Value::Object(_) => serde_json::to_string(value)
                        .map_err(|_| DidResolutionPortError::InvalidResponse)
                        .and_then(|json| {
                            ServiceEndpointValue::json_object(json)
                                .map_err(|_| DidResolutionPortError::InvalidResponse)
                        }),
                    _ => Err(DidResolutionPortError::InvalidResponse),
                })
                .collect::<Result<Vec<_>, _>>()?;
            Service::new(
                required_string(service, "id", MAX_METADATA_TEXT)?,
                types,
                endpoints,
                was_array,
            )
            .map_err(|_| DidResolutionPortError::InvalidResponse)
        })
        .collect()
}

/// Canonical safe representation used by the durable public-record adapter.
#[must_use]
pub fn resolution_to_json_value(resolution: &DidResolution) -> Value {
    let document = resolution.document();
    let mut relationships: BTreeMap<&str, Value> = BTreeMap::new();
    for entry in document.relationships() {
        relationships.insert(entry.relationship().as_str(), json!(entry.method_ids()));
    }
    let methods = document.verification_methods().iter().map(|method| {
        let jwk = method.public_key_jwk();
        let mut public = json!({ "kty": jwk.key_type().as_str(), "crv": jwk.curve().as_str(), "x": jwk.x() });
        if let Some(y) = jwk.y() { public["y"] = json!(y); }
        json!({ "id": method.id(), "type": "JsonWebKey", "controller": method.controller().as_str(), "publicKeyJwk": public })
    }).collect::<Vec<_>>();
    let services = document
        .services()
        .iter()
        .map(|service| {
            let endpoints = service
                .endpoints()
                .iter()
                .map(|endpoint| {
                    if endpoint.is_json_object() {
                        serde_json::from_str(endpoint.value()).unwrap_or(Value::Null)
                    } else {
                        json!(endpoint.value())
                    }
                })
                .collect::<Vec<_>>();
            let endpoint = if service.endpoint_was_array() {
                Value::Array(endpoints)
            } else {
                endpoints.into_iter().next().unwrap_or(Value::Null)
            };
            let service_type = if service.types().len() == 1 {
                json!(service.types()[0])
            } else {
                json!(service.types())
            };
            json!({ "id": service.id(), "type": service_type, "serviceEndpoint": endpoint })
        })
        .collect::<Vec<_>>();
    let metadata = resolution.document_metadata();
    let mut document_value = json!({
        "@context": document.contexts(), "id": document.id().as_str(), "controller": document.id().as_str(),
        "alsoKnownAs": document.also_known_as(), "verificationMethod": methods, "service": services
    });
    for (name, value) in relationships {
        document_value[name] = value;
    }
    json!({
        "didDocument": document_value,
        "didDocumentMetadata": {
            "created": metadata.created, "updated": metadata.updated, "deactivated": metadata.deactivated,
            "versionId": metadata.version_id, "nextUpdate": metadata.next_update,
            "nextVersionId": metadata.next_version_id, "equivalentId": metadata.equivalent_ids,
            "canonicalId": metadata.canonical_id
        },
        "didResolutionMetadata": { "contentType": resolution.resolution_metadata().content_type }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standalone_resolves_only_the_documented_fixture() {
        let resolver = StandaloneDidResolver;
        let fixture = MidnightDid::parse(STANDALONE_FIXTURE_DID).expect("fixture DID");
        let resolved = futures::executor::block_on(resolver.resolve(&fixture)).expect("resolve");
        assert_eq!(resolved.document().verification_methods().len(), 2);
        let unknown =
            MidnightDid::parse(format!("did:midnight:undeployed:{}", "f".repeat(64))).expect("DID");
        assert_eq!(
            futures::executor::block_on(resolver.resolve(&unknown)),
            Err(DidResolutionPortError::NotFound)
        );
    }

    #[test]
    fn codec_round_trips_without_private_key_material() {
        let resolution = standalone_resolution().expect("fixture");
        let bytes = serde_json::to_vec(&resolution_to_json_value(&resolution)).expect("JSON");
        let parsed = parse_resolution_bytes(&bytes, DidResolutionSource::Stored).expect("parse");
        assert_eq!(parsed.document(), resolution.document());
        assert_eq!(parsed.source(), DidResolutionSource::Stored);
        let mut value: Value = serde_json::from_slice(&bytes).expect("value");
        value["didDocument"]["verificationMethod"][0]["publicKeyJwk"]["d"] = json!("private");
        assert_eq!(
            parse_resolution_bytes(
                &serde_json::to_vec(&value).expect("JSON"),
                DidResolutionSource::Live
            ),
            Err(DidResolutionPortError::InvalidResponse)
        );
    }

    #[test]
    fn parses_every_midnight_did_0_5_public_key_profile() {
        let profiles: [(&str, &str, usize, bool); 7] = [
            ("OKP", "Ed25519", 32, false),
            ("OKP", "X25519", 32, false),
            ("EC", "Jubjub", 32, true),
            ("EC", "P-256", 32, true),
            ("EC", "secp256k1", 32, true),
            ("OKP", "BLS12381G1", 48, false),
            ("OKP", "BLS12381G2", 96, false),
        ];
        let methods = profiles
            .iter()
            .enumerate()
            .map(|(index, (key_type, curve, bytes, has_y))| {
                let coordinate = "A".repeat((*bytes).saturating_mul(8).div_ceil(6));
                let mut jwk = json!({ "kty": key_type, "crv": curve, "x": coordinate });
                if *has_y {
                    jwk["y"] = json!("A".repeat(43));
                }
                json!({
                    "id": format!("#key-{index}"),
                    "type": "JsonWebKey",
                    "controller": STANDALONE_FIXTURE_DID,
                    "publicKeyJwk": jwk,
                })
            })
            .collect::<Vec<_>>();
        let value = json!({
            "didDocument": {
                "@context": [DID_CONTEXT, JWK_CONTEXT],
                "id": STANDALONE_FIXTURE_DID,
                "controller": STANDALONE_FIXTURE_DID,
                "verificationMethod": methods,
                "authentication": ["#key-0", "#key-2", "#key-3", "#key-4", "#key-5", "#key-6"],
                "keyAgreement": ["#key-1"]
            },
            "didDocumentMetadata": {},
            "didResolutionMetadata": { "contentType": "application/did+ld+json" }
        });
        let resolution = parse_resolution_bytes(
            &serde_json::to_vec(&value).expect("JSON"),
            DidResolutionSource::Live,
        )
        .expect("all current profiles should parse");
        assert_eq!(resolution.document().verification_methods().len(), 7);
    }

    #[test]
    fn rejects_oversized_resolution_before_json_parsing() {
        assert_eq!(
            parse_resolution_bytes(
                &vec![b' '; MAX_RESPONSE_BYTES + 1],
                DidResolutionSource::Live
            ),
            Err(DidResolutionPortError::InvalidResponse)
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn http_configuration_rejects_ambient_or_insecure_routes() {
        assert!(matches!(
            HttpDidResolverConfig::new("http://resolver.example/"),
            Err(HttpDidResolverConfigError::InsecureRemoteTransport)
        ));
        assert!(matches!(
            HttpDidResolverConfig::new("https://user:secret@resolver.example/"),
            Err(HttpDidResolverConfigError::CredentialsNotAllowed)
        ));
        assert!(matches!(
            HttpDidResolverConfig::new("https://resolver.example/?token=secret"),
            Err(HttpDidResolverConfigError::QueryNotAllowed)
        ));
        assert!(HttpDidResolverConfig::new("http://127.0.0.1:8080/").is_ok());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn http_adapter_uses_official_post_contract() {
        use std::{
            io::{Read as _, Write as _},
            net::TcpListener,
            thread,
        };
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
        let address = listener.local_addr().expect("address");
        let response = serde_json::to_vec(&resolution_to_json_value(
            &standalone_resolution().expect("fixture"),
        ))
        .expect("response");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut request = vec![0_u8; 8_192];
            let read = stream.read(&mut request).expect("read");
            let request = String::from_utf8_lossy(&request[..read]);
            assert!(request.starts_with("POST /resolve HTTP/1.1"));
            assert!(request.contains(STANDALONE_FIXTURE_DID));
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", response.len()).expect("headers");
            stream.write_all(&response).expect("body");
        });
        let config = HttpDidResolverConfig::new(format!("http://{address}/")).expect("config");
        let resolver = HttpDidResolver::new(config);
        let did = MidnightDid::parse(STANDALONE_FIXTURE_DID).expect("DID");
        let resolved = futures::executor::block_on(resolver.resolve(&did)).expect("resolve");
        assert_eq!(resolved.source(), DidResolutionSource::Live);
        server.join().expect("server");
    }
}
