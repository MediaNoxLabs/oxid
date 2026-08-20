// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

#[cfg(any(target_os = "ios", target_os = "android"))]
use std::time::Duration;
use std::{collections::VecDeque, sync::Mutex};

#[cfg(target_os = "android")]
use oxid_adapter_mobile_native::take_identity_link_json;
#[cfg(any(target_os = "ios", target_os = "android"))]
use oxid_adapter_mobile_native::{
    NativeBridgeError, start_scan_json, take_scan_result_json, timeout_scan_json,
};
use oxid_platform_ports::{
    IdentityLinkIngressError, IdentityLinkIngressPort, InboundIdentityLink, QrScanError,
    QrScanFuture, QrScannerPort, ScannedQrPayload,
};
use oxid_protocol_application::{
    IdentityRequestKind, IdentityRequestRouterPort, IdentityRequestRoutingError,
};
#[cfg(any(target_os = "ios", target_os = "android", test))]
use serde::Deserialize;
use url::Url;

#[cfg(any(target_os = "ios", target_os = "android"))]
const SCAN_POLL_INTERVAL: Duration = Duration::from_millis(100);
#[cfg(any(target_os = "ios", target_os = "android"))]
const SCAN_POLL_LIMIT: usize = 600;
// A second request must not replace a consent screen that the holder is
// already reviewing.
const IDENTITY_LINK_QUEUE_LIMIT: usize = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
struct RegisteredOpenId4VpRequest {
    client_id: String,
    request_uri: String,
}

/// Strict protocol-link classifier. `openid4vp` links are accepted only when
/// their client and request endpoints match an explicitly registered flow.
pub struct StrictIdentityRequestRouter {
    self_issued: Option<RegisteredOpenId4VpRequest>,
    presentation: Option<RegisteredOpenId4VpRequest>,
}

impl StrictIdentityRequestRouter {
    /// Routes credential offers but fails closed for ambiguous `openid4vp`
    /// links until production endpoint discovery supplies a registry.
    #[must_use]
    pub const fn credential_offers_only() -> Self {
        Self {
            self_issued: None,
            presentation: None,
        }
    }

    pub fn with_registered_openid4vp_requests(
        self_issued: &str,
        presentation: &str,
    ) -> Result<Self, IdentityRequestRoutingError> {
        let self_issued = registered_openid4vp_request(self_issued)?;
        let presentation = registered_openid4vp_request(presentation)?;
        if self_issued == presentation {
            return Err(IdentityRequestRoutingError::AmbiguousRequest);
        }
        Ok(Self {
            self_issued: Some(self_issued),
            presentation: Some(presentation),
        })
    }
}

impl IdentityRequestRouterPort for StrictIdentityRequestRouter {
    fn route(&self, request_uri: &str) -> Result<IdentityRequestKind, IdentityRequestRoutingError> {
        let parsed =
            Url::parse(request_uri).map_err(|_| IdentityRequestRoutingError::InvalidRequest)?;
        match parsed.scheme() {
            "openid-credential-offer" => {
                validate_credential_offer_route(&parsed)?;
                Ok(IdentityRequestKind::CredentialIssuance)
            }
            "openid4vp" => {
                let request = registered_openid4vp_request(request_uri)?;
                match (
                    self.self_issued.as_ref() == Some(&request),
                    self.presentation.as_ref() == Some(&request),
                ) {
                    (true, false) => Ok(IdentityRequestKind::SelfIssuedAuthentication),
                    (false, true) => Ok(IdentityRequestKind::CredentialPresentation),
                    (false, false) | (true, true) => {
                        Err(IdentityRequestRoutingError::AmbiguousRequest)
                    }
                }
            }
            _ => Err(IdentityRequestRoutingError::UnsupportedRequest),
        }
    }
}

fn validate_credential_offer_route(url: &Url) -> Result<(), IdentityRequestRoutingError> {
    if url.has_host()
        || !matches!(url.path(), "" | "/")
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return Err(IdentityRequestRoutingError::InvalidRequest);
    }
    let mut embedded = 0_u8;
    let mut referenced = 0_u8;
    for (name, value) in url.query_pairs() {
        if value.is_empty() {
            return Err(IdentityRequestRoutingError::InvalidRequest);
        }
        match name.as_ref() {
            "credential_offer" => embedded = embedded.saturating_add(1),
            "credential_offer_uri" => referenced = referenced.saturating_add(1),
            _ => return Err(IdentityRequestRoutingError::InvalidRequest),
        }
    }
    if (embedded, referenced) == (1, 0) || (embedded, referenced) == (0, 1) {
        Ok(())
    } else {
        Err(IdentityRequestRoutingError::InvalidRequest)
    }
}

fn registered_openid4vp_request(
    request_uri: &str,
) -> Result<RegisteredOpenId4VpRequest, IdentityRequestRoutingError> {
    let url = Url::parse(request_uri).map_err(|_| IdentityRequestRoutingError::InvalidRequest)?;
    if url.scheme() != "openid4vp"
        || url.host_str() != Some("authorize")
        || !matches!(url.path(), "" | "/")
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return Err(IdentityRequestRoutingError::InvalidRequest);
    }

    let mut client_id = None;
    let mut nested_request_uri = None;
    for (name, value) in url.query_pairs() {
        if value.is_empty() {
            return Err(IdentityRequestRoutingError::InvalidRequest);
        }
        let slot = match name.as_ref() {
            "client_id" => &mut client_id,
            "request_uri" => &mut nested_request_uri,
            _ => return Err(IdentityRequestRoutingError::InvalidRequest),
        };
        if slot.replace(value.into_owned()).is_some() {
            return Err(IdentityRequestRoutingError::InvalidRequest);
        }
    }

    Ok(RegisteredOpenId4VpRequest {
        client_id: client_id.ok_or(IdentityRequestRoutingError::InvalidRequest)?,
        request_uri: nested_request_uri.ok_or(IdentityRequestRoutingError::InvalidRequest)?,
    })
}

/// Native scanner backed by AVFoundation on iOS and Google Code Scanner on
/// Android. Other targets return `Unavailable` without attempting a bridge.
#[derive(Clone, Copy, Debug, Default)]
pub struct NativeQrScanner;

impl QrScannerPort for NativeQrScanner {
    fn scan<'a>(&'a self) -> QrScanFuture<'a> {
        Box::pin(async { scan_native().await })
    }
}

/// Bounded OS URL ingress. iOS/Tao events enter through `capture`; Android's
/// repository-owned activity queues VIEW intents in the static Kotlin bridge.
#[derive(Default)]
pub struct NativeIdentityLinkIngress {
    captured: Mutex<VecDeque<InboundIdentityLink>>,
}

impl IdentityLinkIngressPort for NativeIdentityLinkIngress {
    fn capture(&self, value: String) -> Result<(), IdentityLinkIngressError> {
        let link = InboundIdentityLink::new(value)?;
        let mut captured = self
            .captured
            .lock()
            .map_err(|_| IdentityLinkIngressError::Failed)?;
        if captured.len() >= IDENTITY_LINK_QUEUE_LIMIT {
            return Err(IdentityLinkIngressError::QueueFull);
        }
        captured.push_back(link);
        Ok(())
    }

    fn take_pending(&self) -> Result<Option<InboundIdentityLink>, IdentityLinkIngressError> {
        if let Some(link) = self
            .captured
            .lock()
            .map_err(|_| IdentityLinkIngressError::Failed)?
            .pop_front()
        {
            return Ok(Some(link));
        }
        take_native_identity_link()
    }
}

#[cfg(target_os = "android")]
fn take_native_identity_link() -> Result<Option<InboundIdentityLink>, IdentityLinkIngressError> {
    let response = take_identity_link_json().map_err(map_identity_link_bridge_error)?;
    let status: NativeIdentityLinkStatus =
        serde_json::from_str(&response).map_err(|_| IdentityLinkIngressError::Failed)?;
    match status.status.as_str() {
        "empty" => Ok(None),
        "succeeded" => InboundIdentityLink::new(
            status
                .payload
                .ok_or(IdentityLinkIngressError::InvalidLink)?,
        )
        .map(Some),
        "invalid" => Err(IdentityLinkIngressError::InvalidLink),
        "queue_full" => Err(IdentityLinkIngressError::QueueFull),
        "unavailable" => Err(IdentityLinkIngressError::Unavailable),
        _ => Err(IdentityLinkIngressError::Failed),
    }
}

#[cfg(not(target_os = "android"))]
fn take_native_identity_link() -> Result<Option<InboundIdentityLink>, IdentityLinkIngressError> {
    Ok(None)
}

#[cfg(any(target_os = "ios", target_os = "android"))]
async fn scan_native() -> Result<ScannedQrPayload, QrScanError> {
    let started = start_scan_json().map_err(map_qr_bridge_error)?;
    decode_scan_start(&started)?;

    for _ in 0..SCAN_POLL_LIMIT {
        tokio::time::sleep(SCAN_POLL_INTERVAL).await;
        let response = take_scan_result_json().map_err(map_qr_bridge_error)?;
        if let Some(payload) = decode_scan_poll(&response)? {
            return Ok(payload);
        }
    }

    // The timeout belongs to the Rust port contract, but the native
    // coordinator must acknowledge it so a stale capture cannot occupy the
    // one-scanner slot or deliver into a later request.
    let timed_out = timeout_scan_json().map_err(map_qr_bridge_error)?;
    match decode_scan_poll(&timed_out) {
        Ok(Some(payload)) => Ok(payload),
        Err(QrScanError::TimedOut) => Err(QrScanError::TimedOut),
        Err(error) => Err(error),
        _ => Err(QrScanError::Failed),
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
async fn scan_native() -> Result<ScannedQrPayload, QrScanError> {
    Err(QrScanError::Unavailable)
}

#[cfg(any(target_os = "ios", target_os = "android", test))]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeScanStatus {
    status: String,
    #[serde(default)]
    payload: Option<String>,
}

#[cfg(any(target_os = "ios", target_os = "android", test))]
fn decode_scan_start(response: &str) -> Result<(), QrScanError> {
    let status: NativeScanStatus =
        serde_json::from_str(response).map_err(|_| QrScanError::Failed)?;
    if status.payload.is_some() {
        return Err(QrScanError::Failed);
    }
    if status.status == "scanning" {
        Ok(())
    } else {
        Err(map_native_status(&status.status))
    }
}

#[cfg(any(target_os = "ios", target_os = "android", test))]
fn decode_scan_poll(response: &str) -> Result<Option<ScannedQrPayload>, QrScanError> {
    let status: NativeScanStatus =
        serde_json::from_str(response).map_err(|_| QrScanError::Failed)?;
    match (status.status.as_str(), status.payload) {
        ("scanning", None) => Ok(None),
        ("succeeded", Some(payload)) => ScannedQrPayload::new(payload).map(Some),
        ("cancelled" | "denied" | "unavailable" | "timed_out" | "invalid" | "failed", None) => {
            Err(map_native_status(&status.status))
        }
        _ => Err(QrScanError::Failed),
    }
}

#[cfg(target_os = "android")]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeIdentityLinkStatus {
    status: String,
    #[serde(default)]
    payload: Option<String>,
}

#[cfg(any(target_os = "ios", target_os = "android", test))]
fn map_native_status(status: &str) -> QrScanError {
    match status {
        "cancelled" => QrScanError::Cancelled,
        "denied" => QrScanError::Denied,
        "unavailable" => QrScanError::Unavailable,
        "timed_out" => QrScanError::TimedOut,
        "invalid" => QrScanError::InvalidPayload,
        _ => QrScanError::Failed,
    }
}

#[cfg(any(target_os = "ios", target_os = "android"))]
const fn map_qr_bridge_error(error: NativeBridgeError) -> QrScanError {
    match error {
        NativeBridgeError::Unavailable => QrScanError::Unavailable,
        NativeBridgeError::Failed => QrScanError::Failed,
    }
}

#[cfg(target_os = "android")]
const fn map_identity_link_bridge_error(error: NativeBridgeError) -> IdentityLinkIngressError {
    match error {
        NativeBridgeError::Unavailable => IdentityLinkIngressError::Unavailable,
        NativeBridgeError::Failed => IdentityLinkIngressError::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOGIN: &str = "openid4vp://authorize?client_id=http%3A%2F%2F127.0.0.1%3A32192%2Fverifier&request_uri=http%3A%2F%2F127.0.0.1%3A32192%2Fverifier%2Frequest";
    const PRESENTATION: &str = "openid4vp://authorize?client_id=http%3A%2F%2F127.0.0.1%3A32193%2Fverifier&request_uri=http%3A%2F%2F127.0.0.1%3A32193%2Fverifier%2Frequest";

    fn router() -> StrictIdentityRequestRouter {
        StrictIdentityRequestRouter::with_registered_openid4vp_requests(LOGIN, PRESENTATION)
            .expect("registry")
    }

    #[test]
    fn routes_each_supported_identity_request_without_reading_secrets() {
        assert_eq!(
            router().route("openid-credential-offer://?credential_offer=%7B%7D"),
            Ok(IdentityRequestKind::CredentialIssuance)
        );
        assert_eq!(
            router().route(LOGIN),
            Ok(IdentityRequestKind::SelfIssuedAuthentication)
        );
        assert_eq!(
            router().route(PRESENTATION),
            Ok(IdentityRequestKind::CredentialPresentation)
        );
    }

    #[test]
    fn routes_androids_empty_authority_offer_serialization() {
        assert_eq!(
            router().route("openid-credential-offer:///?credential_offer=%7B%7D"),
            Ok(IdentityRequestKind::CredentialIssuance)
        );
        assert_eq!(
            router().route("openid-credential-offer:///unexpected?credential_offer=%7B%7D"),
            Err(IdentityRequestRoutingError::InvalidRequest)
        );
    }

    #[test]
    fn fails_closed_for_unknown_or_smuggled_openid4vp_links() {
        assert_eq!(
            router().route("openid4vp://authorize?client_id=https%3A%2F%2Funknown.example&request_uri=https%3A%2F%2Funknown.example%2Frequest"),
            Err(IdentityRequestRoutingError::AmbiguousRequest)
        );
        assert_eq!(
            router().route(&format!(
                "{LOGIN}&request_uri=https%3A%2F%2Fattacker.example"
            )),
            Err(IdentityRequestRoutingError::InvalidRequest)
        );
    }

    #[test]
    fn rejects_offer_parameter_smuggling() {
        assert_eq!(
            router().route("openid-credential-offer://?credential_offer=%7B%7D&credential_offer_uri=https%3A%2F%2Fissuer.example"),
            Err(IdentityRequestRoutingError::InvalidRequest)
        );
    }

    #[test]
    fn native_link_ingress_retains_one_consent_request_and_fails_closed() {
        let ingress = NativeIdentityLinkIngress::default();
        ingress
            .capture("openid-credential-offer://?credential_offer=%7B%7D".to_owned())
            .expect("first link");
        assert_eq!(
            ingress.capture(LOGIN.to_owned()),
            Err(IdentityLinkIngressError::QueueFull)
        );

        let first = ingress
            .take_pending()
            .expect("take first")
            .expect("first pending");
        assert!(first.into_inner().starts_with("openid-credential-offer"));
        ingress.capture(LOGIN.to_owned()).expect("next link");
        let second = ingress
            .take_pending()
            .expect("take second")
            .expect("second pending");
        assert_eq!(second.into_inner(), LOGIN);
        assert_eq!(ingress.take_pending(), Ok(None));

        ingress
            .capture("openid4vp://authorize?slot=0".to_owned())
            .expect("queue slot");
        assert_eq!(
            ingress.capture("openid4vp://authorize?slot=overflow".to_owned()),
            Err(IdentityLinkIngressError::QueueFull)
        );
        assert_eq!(
            ingress.capture(" openid4vp://authorize".to_owned()),
            Err(IdentityLinkIngressError::InvalidLink)
        );
    }

    #[test]
    fn native_scan_statuses_are_closed_payload_free_and_exact() {
        assert_eq!(decode_scan_start(r#"{"status":"scanning"}"#), Ok(()));
        assert_eq!(decode_scan_poll(r#"{"status":"scanning"}"#), Ok(None));
        assert_eq!(
            decode_scan_poll(r#"{"status":"succeeded","payload":"openid4vp://authorize"}"#)
                .map(|payload| payload.map(ScannedQrPayload::into_inner)),
            Ok(Some("openid4vp://authorize".to_owned()))
        );

        for (status, expected) in [
            ("cancelled", QrScanError::Cancelled),
            ("denied", QrScanError::Denied),
            ("unavailable", QrScanError::Unavailable),
            ("timed_out", QrScanError::TimedOut),
            ("invalid", QrScanError::InvalidPayload),
            ("failed", QrScanError::Failed),
        ] {
            assert_eq!(
                decode_scan_poll(&format!(r#"{{"status":"{status}"}}"#)),
                Err(expected)
            );
        }

        assert_eq!(
            decode_scan_start(r#"{"status":"scanning","payload":"must-not-cross"}"#),
            Err(QrScanError::Failed)
        );
        assert_eq!(
            decode_scan_poll(r#"{"status":"cancelled","payload":"must-not-cross"}"#),
            Err(QrScanError::Failed)
        );
        assert_eq!(
            decode_scan_poll(r#"{"status":"new-native-status"}"#),
            Err(QrScanError::Failed)
        );
        assert_eq!(
            decode_scan_poll(r#"{"status":"failed","detail":"native error"}"#),
            Err(QrScanError::Failed)
        );
    }

    #[test]
    fn native_scan_success_rejects_empty_and_oversized_payloads() {
        assert_eq!(
            decode_scan_poll(r#"{"status":"succeeded","payload":""}"#),
            Err(QrScanError::InvalidPayload)
        );
        let oversized = serde_json::json!({
            "status": "succeeded",
            "payload": "x".repeat(32 * 1_024 + 1),
        });
        assert_eq!(
            decode_scan_poll(&oversized.to_string()),
            Err(QrScanError::InvalidPayload)
        );
    }
}
