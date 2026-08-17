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

/// Fail-closed scanner used by non-mobile and unavailable composition.
pub struct UnavailableQrScanner;

impl QrScannerPort for UnavailableQrScanner {
    fn scan<'a>(&'a self) -> QrScanFuture<'a> {
        Box::pin(async { Err(QrScanError::Unavailable) })
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
}
