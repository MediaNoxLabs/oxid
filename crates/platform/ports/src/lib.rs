// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{error::Error, fmt, future::Future, pin::Pin};

use oxid_foundation::UnixTimestampMillis;

/// Safe, adapter-neutral platform failure categories.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlatformError {
    ClockUnavailable,
    RandomnessUnavailable,
}

/// A bounded future returned by a platform QR scanner.
pub type QrScanFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ScannedQrPayload, QrScanError>> + Send + 'a>>;

/// Opaque QR text returned by a platform scanner.
///
/// Debug output deliberately discloses only the payload size because identity
/// requests commonly carry nonces, state, offer codes, and request objects.
#[derive(Clone, PartialEq, Eq)]
pub struct ScannedQrPayload(String);

impl ScannedQrPayload {
    pub fn new(value: String) -> Result<Self, QrScanError> {
        if value.is_empty() || value.len() > 32 * 1_024 {
            return Err(QrScanError::InvalidPayload);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Debug for ScannedQrPayload {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ScannedQrPayload")
            .field("length", &self.0.len())
            .finish_non_exhaustive()
    }
}

/// Stable, payload-free scanner failure categories.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QrScanError {
    Cancelled,
    Unavailable,
    TimedOut,
    InvalidPayload,
    Failed,
}

impl fmt::Display for QrScanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Cancelled => "QR scan was cancelled",
            Self::Unavailable => "QR scanning is unavailable on this device",
            Self::TimedOut => "QR scan timed out",
            Self::InvalidPayload => "QR payload is invalid",
            Self::Failed => "QR scan failed",
        })
    }
}

impl Error for QrScanError {}

/// Supplies one QR payload without coupling incoming adapters to camera APIs.
pub trait QrScannerPort: Send + Sync {
    fn scan<'a>(&'a self) -> QrScanFuture<'a>;
}

/// Opaque, bounded protocol link delivered by an operating-system URL event.
///
/// Debug output deliberately omits the link because credential offers and
/// OpenID4VP requests commonly carry authorization codes, nonces, and state.
#[derive(Clone, PartialEq, Eq)]
pub struct InboundIdentityLink(String);

impl InboundIdentityLink {
    pub fn new(value: String) -> Result<Self, IdentityLinkIngressError> {
        if value.is_empty()
            || value.len() > 32 * 1_024
            || value.trim() != value
            || value.chars().any(char::is_control)
        {
            return Err(IdentityLinkIngressError::InvalidLink);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Debug for InboundIdentityLink {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("InboundIdentityLink")
            .field("length", &self.0.len())
            .finish_non_exhaustive()
    }
}

/// Stable, link-free operating-system ingress failures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentityLinkIngressError {
    Unavailable,
    InvalidLink,
    QueueFull,
    Failed,
}

impl fmt::Display for IdentityLinkIngressError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "identity app links are unavailable on this device",
            Self::InvalidLink => "identity app link is invalid",
            Self::QueueFull => "identity app-link queue is full",
            Self::Failed => "identity app-link ingress failed",
        })
    }
}

impl Error for IdentityLinkIngressError {}

/// Receives bounded OS URL events and exposes them one at a time to an incoming
/// adapter. Implementations must never log or include the raw link in errors.
pub trait IdentityLinkIngressPort: Send + Sync {
    fn capture(&self, value: String) -> Result<(), IdentityLinkIngressError>;

    fn take_pending(&self) -> Result<Option<InboundIdentityLink>, IdentityLinkIngressError>;
}

/// A public receive address explicitly approved for clipboard or
/// operating-system share export. Callers cannot pass an arbitrary string to
/// the export port without first opting into this capability-specific type.
#[derive(Clone, PartialEq, Eq)]
pub struct PublicReceiveAddress(String);

impl PublicReceiveAddress {
    pub fn new(value: String) -> Result<Self, PublicTextExportError> {
        if value.is_empty()
            || value.len() > 4 * 1_024
            || value.trim() != value
            || value.chars().any(char::is_control)
        {
            return Err(PublicTextExportError::InvalidPublicText);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for PublicReceiveAddress {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PublicReceiveAddress")
            .field("length", &self.0.len())
            .finish_non_exhaustive()
    }
}

/// Stable public-export failures that never reproduce the exported value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicTextExportError {
    Unavailable,
    InvalidPublicText,
    Failed,
}

impl fmt::Display for PublicTextExportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "public text export is unavailable on this device",
            Self::InvalidPublicText => "public text is invalid",
            Self::Failed => "public text export failed",
        })
    }
}

impl Error for PublicTextExportError {}

/// Copies or shares only a typed public receive address. Credential requests,
/// authorization responses, and other secret-bearing strings have no method on
/// this port.
pub trait PublicTextExportPort: Send + Sync {
    fn copy_receive_address(
        &self,
        address: PublicReceiveAddress,
    ) -> Result<(), PublicTextExportError>;

    fn share_receive_address(
        &self,
        address: PublicReceiveAddress,
    ) -> Result<(), PublicTextExportError>;
}

/// Stable, payload-free screen-privacy failures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenPrivacyError {
    Unavailable,
    Failed,
}

impl fmt::Display for ScreenPrivacyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "screen privacy is unavailable on this device",
            Self::Failed => "screen privacy could not be changed",
        })
    }
}

impl Error for ScreenPrivacyError {}

/// Applies only operating-system snapshot protection.
///
/// Presentation masking remains an incoming-adapter concern. Implementations
/// must not inspect wallet state or receive any rendered value.
pub trait ScreenPrivacyPort: Send + Sync {
    fn set_protected(&self, protected: bool) -> Result<(), ScreenPrivacyError>;
}

/// Fail-closed scanner used by non-mobile and unavailable composition.
pub struct UnavailableQrScanner;

impl QrScannerPort for UnavailableQrScanner {
    fn scan<'a>(&'a self) -> QrScanFuture<'a> {
        Box::pin(async { Err(QrScanError::Unavailable) })
    }
}

/// Fail-closed app-link ingress for targets without an OS URL adapter.
pub struct UnavailableIdentityLinkIngress;

impl IdentityLinkIngressPort for UnavailableIdentityLinkIngress {
    fn capture(&self, _value: String) -> Result<(), IdentityLinkIngressError> {
        Err(IdentityLinkIngressError::Unavailable)
    }

    fn take_pending(&self) -> Result<Option<InboundIdentityLink>, IdentityLinkIngressError> {
        Ok(None)
    }
}

/// Fail-closed public text exporter for non-mobile compositions.
pub struct UnavailablePublicTextExporter;

impl PublicTextExportPort for UnavailablePublicTextExporter {
    fn copy_receive_address(
        &self,
        _address: PublicReceiveAddress,
    ) -> Result<(), PublicTextExportError> {
        Err(PublicTextExportError::Unavailable)
    }

    fn share_receive_address(
        &self,
        _address: PublicReceiveAddress,
    ) -> Result<(), PublicTextExportError> {
        Err(PublicTextExportError::Unavailable)
    }
}

/// Fail-closed screen-privacy edge for targets without a native window.
pub struct UnavailableScreenPrivacy;

impl ScreenPrivacyPort for UnavailableScreenPrivacy {
    fn set_protected(&self, _protected: bool) -> Result<(), ScreenPrivacyError> {
        Err(ScreenPrivacyError::Unavailable)
    }
}

impl fmt::Display for PlatformError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::ClockUnavailable => "system clock is unavailable",
            Self::RandomnessUnavailable => "secure randomness is unavailable",
        };
        formatter.write_str(message)
    }
}

impl Error for PlatformError {}

/// Supplies time without coupling application code to an OS or date-time crate.
pub trait ClockPort: Send + Sync {
    fn now(&self) -> Result<UnixTimestampMillis, PlatformError>;
}

/// Supplies random bytes without exposing a particular RNG implementation.
pub trait RandomPort: Send + Sync {
    fn fill_bytes(&self, destination: &mut [u8]) -> Result<(), PlatformError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::task::{Context, Poll, Waker};

    #[test]
    fn scanned_payload_is_bounded_and_redacted() {
        let payload =
            ScannedQrPayload::new("openid-credential-offer://?credential_offer=private".to_owned())
                .expect("bounded payload");
        let debug = format!("{payload:?}");
        assert!(debug.contains("length"));
        assert!(!debug.contains("credential_offer"));
        assert!(ScannedQrPayload::new(String::new()).is_err());
        assert!(ScannedQrPayload::new("x".repeat(32 * 1_024 + 1)).is_err());
        assert!(payload.into_inner().starts_with("openid-credential-offer"));
    }

    #[test]
    fn unavailable_scanner_fails_with_a_payload_free_error() {
        let mut context = Context::from_waker(Waker::noop());
        let mut scan = UnavailableQrScanner.scan();
        let error = match scan.as_mut().poll(&mut context) {
            Poll::Ready(result) => result.expect_err("scanner must fail closed"),
            Poll::Pending => panic!("unavailable scanner must resolve immediately"),
        };
        assert_eq!(error, QrScanError::Unavailable);
        assert_eq!(
            error.to_string(),
            "QR scanning is unavailable on this device"
        );
    }

    #[test]
    fn app_links_are_bounded_and_redacted() {
        let link = InboundIdentityLink::new("openid4vp://authorize?request_uri=private".to_owned())
            .expect("bounded app link");
        let debug = format!("{link:?}");
        assert!(debug.contains("length"));
        assert!(!debug.contains("request_uri"));
        assert!(InboundIdentityLink::new(" openid4vp://authorize".to_owned()).is_err());
        assert!(InboundIdentityLink::new("openid4vp://authorize\n".to_owned()).is_err());
        assert!(InboundIdentityLink::new("x".repeat(32 * 1_024 + 1)).is_err());
        assert!(link.into_inner().starts_with("openid4vp"));
    }

    #[test]
    fn only_bounded_public_receive_addresses_reach_export_ports() {
        let address = PublicReceiveAddress::new("mn_addr_undeployed1public".to_owned())
            .expect("public address");
        let debug = format!("{address:?}");
        assert!(debug.contains("length"));
        assert!(!debug.contains("mn_addr"));
        assert_eq!(address.as_str(), "mn_addr_undeployed1public");
        assert!(PublicReceiveAddress::new(String::new()).is_err());
        assert!(PublicReceiveAddress::new("address\nsecret".to_owned()).is_err());
        assert!(PublicReceiveAddress::new("x".repeat(4 * 1_024 + 1)).is_err());
    }

    #[test]
    fn unavailable_native_edges_fail_closed_without_payloads() {
        assert_eq!(
            UnavailableIdentityLinkIngress.capture("openid4vp://private".to_owned()),
            Err(IdentityLinkIngressError::Unavailable)
        );
        assert_eq!(UnavailableIdentityLinkIngress.take_pending(), Ok(None));
        let address = PublicReceiveAddress::new("mn_addr_public".to_owned()).expect("address");
        assert_eq!(
            UnavailablePublicTextExporter.copy_receive_address(address.clone()),
            Err(PublicTextExportError::Unavailable)
        );
        assert_eq!(
            UnavailablePublicTextExporter.share_receive_address(address),
            Err(PublicTextExportError::Unavailable)
        );
        assert_eq!(
            UnavailableScreenPrivacy.set_protected(true),
            Err(ScreenPrivacyError::Unavailable)
        );
        assert_eq!(
            UnavailableScreenPrivacy.set_protected(false),
            Err(ScreenPrivacyError::Unavailable)
        );
    }
}
