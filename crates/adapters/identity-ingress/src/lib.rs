// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

#[cfg(any(target_os = "ios", target_os = "android"))]
use std::time::Duration;

use oxid_platform_ports::{QrScanError, QrScanFuture, QrScannerPort, ScannedQrPayload};
use oxid_protocol_application::{
    IdentityRequestKind, IdentityRequestRouterPort, IdentityRequestRoutingError,
};
#[cfg(any(target_os = "ios", target_os = "android"))]
use serde::Deserialize;
use url::Url;

#[cfg(any(target_os = "ios", target_os = "android"))]
const SCAN_POLL_INTERVAL: Duration = Duration::from_millis(100);
#[cfg(any(target_os = "ios", target_os = "android"))]
const SCAN_POLL_LIMIT: usize = 600;

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
        || !url.path().is_empty()
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

#[cfg(any(target_os = "ios", target_os = "android"))]
async fn scan_native() -> Result<ScannedQrPayload, QrScanError> {
    let plugin = OxidQrScannerPlugin::new().map_err(|_| QrScanError::Unavailable)?;
    let started = startScanJson(&plugin).map_err(|_| QrScanError::Failed)?;
    let status: NativeScanStatus =
        serde_json::from_str(&started).map_err(|_| QrScanError::Failed)?;
    if status.status != "scanning" {
        return Err(map_native_status(&status.status));
    }

    for _ in 0..SCAN_POLL_LIMIT {
        tokio::time::sleep(SCAN_POLL_INTERVAL).await;
        let plugin = OxidQrScannerPlugin::new().map_err(|_| QrScanError::Failed)?;
        let response = takeScanResultJson(&plugin).map_err(|_| QrScanError::Failed)?;
        let status: NativeScanStatus =
            serde_json::from_str(&response).map_err(|_| QrScanError::Failed)?;
        match status.status.as_str() {
            "scanning" => {}
            "succeeded" => {
                return ScannedQrPayload::new(status.payload.ok_or(QrScanError::InvalidPayload)?);
            }
            other => return Err(map_native_status(other)),
        }
    }
    Err(QrScanError::TimedOut)
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
async fn scan_native() -> Result<ScannedQrPayload, QrScanError> {
    Err(QrScanError::Unavailable)
}

#[cfg(any(target_os = "ios", target_os = "android"))]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeScanStatus {
    status: String,
    #[serde(default)]
    payload: Option<String>,
}

#[cfg(any(target_os = "ios", target_os = "android"))]
fn map_native_status(status: &str) -> QrScanError {
    match status {
        "cancelled" => QrScanError::Cancelled,
        "unavailable" => QrScanError::Unavailable,
        "invalid" => QrScanError::InvalidPayload,
        _ => QrScanError::Failed,
    }
}

#[cfg(target_os = "ios")]
use ios_bridge::{OxidQrScannerPlugin, startScanJson, takeScanResultJson};

#[cfg(target_os = "ios")]
#[allow(non_snake_case)]
mod ios_bridge {
    #[manganis::ffi("src/ios/plugin")]
    extern "Swift" {
        pub type OxidQrScannerPlugin;
        pub fn startScanJson(this: &OxidQrScannerPlugin) -> String;
        pub fn takeScanResultJson(this: &OxidQrScannerPlugin) -> String;
    }
}

#[cfg(target_os = "android")]
use android_bridge::{OxidQrScannerPlugin, startScanJson, takeScanResultJson};

#[cfg(target_os = "android")]
#[allow(non_snake_case)]
mod android_bridge {
    #[manganis::ffi("src/android")]
    extern "Kotlin" {
        pub type OxidQrScannerPlugin;
        pub fn startScanJson(this: &OxidQrScannerPlugin) -> String;
        pub fn takeScanResultJson(this: &OxidQrScannerPlugin) -> String;
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
}
