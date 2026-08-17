// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(any(target_os = "ios", target_os = "android"))]
use oxid_adapter_mobile_native::{
    NativeBridgeError, copy_public_receive_address as native_copy_public_receive_address,
    share_public_receive_address as native_share_public_receive_address,
};
use oxid_foundation::UnixTimestampMillis;
use oxid_platform_ports::{
    ClockPort, PlatformError, PublicReceiveAddress, PublicTextExportError, PublicTextExportPort,
    RandomPort,
};

/// Clock backed by the host system.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl ClockPort for SystemClock {
    fn now(&self) -> Result<UnixTimestampMillis, PlatformError> {
        let elapsed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| PlatformError::ClockUnavailable)?;
        let millis =
            u64::try_from(elapsed.as_millis()).map_err(|_| PlatformError::ClockUnavailable)?;
        Ok(UnixTimestampMillis::new(millis))
    }
}

/// Cryptographically secure randomness supplied by the host operating system.
#[derive(Clone, Copy, Debug, Default)]
pub struct OsRandom;

impl RandomPort for OsRandom {
    fn fill_bytes(&self, destination: &mut [u8]) -> Result<(), PlatformError> {
        getrandom::fill(destination).map_err(|_| PlatformError::RandomnessUnavailable)
    }
}

/// Native clipboard and share-sheet adapter restricted to typed public receive
/// addresses by the platform port.
#[derive(Clone, Copy, Debug, Default)]
pub struct NativePublicTextExporter;

impl PublicTextExportPort for NativePublicTextExporter {
    fn copy_receive_address(
        &self,
        address: PublicReceiveAddress,
    ) -> Result<(), PublicTextExportError> {
        copy_public_receive_address(address)
    }

    fn share_receive_address(
        &self,
        address: PublicReceiveAddress,
    ) -> Result<(), PublicTextExportError> {
        share_public_receive_address(address)
    }
}

#[cfg(any(target_os = "ios", target_os = "android"))]
fn copy_public_receive_address(address: PublicReceiveAddress) -> Result<(), PublicTextExportError> {
    let status = native_copy_public_receive_address(address.as_str())
        .map_err(map_public_export_bridge_error)?;
    map_public_export_status(&status, "copied")
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn copy_public_receive_address(
    _address: PublicReceiveAddress,
) -> Result<(), PublicTextExportError> {
    Err(PublicTextExportError::Unavailable)
}

#[cfg(any(target_os = "ios", target_os = "android"))]
fn share_public_receive_address(
    address: PublicReceiveAddress,
) -> Result<(), PublicTextExportError> {
    let status = native_share_public_receive_address(address.as_str())
        .map_err(map_public_export_bridge_error)?;
    map_public_export_status(&status, "presented")
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn share_public_receive_address(
    _address: PublicReceiveAddress,
) -> Result<(), PublicTextExportError> {
    Err(PublicTextExportError::Unavailable)
}

#[cfg(any(target_os = "ios", target_os = "android"))]
fn map_public_export_status(status: &str, success: &str) -> Result<(), PublicTextExportError> {
    match status {
        value if value == success => Ok(()),
        "unavailable" => Err(PublicTextExportError::Unavailable),
        _ => Err(PublicTextExportError::Failed),
    }
}

#[cfg(any(target_os = "ios", target_os = "android"))]
const fn map_public_export_bridge_error(error: NativeBridgeError) -> PublicTextExportError {
    match error {
        NativeBridgeError::Unavailable => PublicTextExportError::Unavailable,
        NativeBridgeError::Failed => PublicTextExportError::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_adapters_supply_time_and_randomness() {
        let now = SystemClock.now().expect("clock should be available");
        assert!(now.value() > 0);

        let mut bytes = [0_u8; 16];
        OsRandom
            .fill_bytes(&mut bytes)
            .expect("randomness should be available");
        assert_ne!(bytes, [0_u8; 16]);
    }

    #[test]
    fn public_export_fails_closed_without_a_native_bridge() {
        if cfg!(any(target_os = "ios", target_os = "android")) {
            return;
        }
        let address = PublicReceiveAddress::new("mn_addr_public".to_owned()).expect("address");
        assert_eq!(
            NativePublicTextExporter.copy_receive_address(address.clone()),
            Err(PublicTextExportError::Unavailable)
        );
        assert_eq!(
            NativePublicTextExporter.share_receive_address(address),
            Err(PublicTextExportError::Unavailable)
        );
    }
}
