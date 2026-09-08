// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(any(target_os = "ios", target_os = "android"))]
use oxid_adapter_mobile_native::{
    NativeBridgeError, copy_public_receive_address as native_copy_public_receive_address,
    set_screen_privacy as native_set_screen_privacy,
    share_public_receive_address as native_share_public_receive_address,
};
use oxid_foundation::UnixTimestampMillis;
use oxid_platform_ports::{
    ClockPort, PlatformError, ProcessResourceSample, ProcessResourceSampleError,
    ProcessResourceSamplerPort, PublicReceiveAddress, PublicTextExportError, PublicTextExportPort,
    RandomPort, ScreenPrivacyError, ScreenPrivacyPort,
};
#[cfg(not(target_arch = "wasm32"))]
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, get_current_pid};

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

/// Native snapshot-protection adapter. It receives only a boolean policy bit:
/// no balance, address, identity, or credential value crosses this boundary.
#[derive(Clone, Copy, Debug, Default)]
pub struct NativeScreenPrivacy;

impl ScreenPrivacyPort for NativeScreenPrivacy {
    fn set_protected(&self, protected: bool) -> Result<(), ScreenPrivacyError> {
        set_screen_privacy(protected)
    }
}

/// Current-process resource sampler for the opt-in development proof benchmark.
///
/// The first process refresh primes CPU accounting. Subsequent samples are
/// meaningful when callers respect `sysinfo`'s minimum refresh interval; the
/// UI polls at 500 milliseconds.
#[cfg(not(target_arch = "wasm32"))]
pub struct SystemProcessResourceSampler {
    pid: sysinfo::Pid,
    system: Mutex<System>,
}

#[cfg(not(target_arch = "wasm32"))]
impl SystemProcessResourceSampler {
    pub fn new() -> Result<Self, ProcessResourceSampleError> {
        let pid = get_current_pid().map_err(|_| ProcessResourceSampleError::Unavailable)?;
        let mut system = System::new();
        refresh_current_process(&mut system, pid);
        system
            .process(pid)
            .ok_or(ProcessResourceSampleError::Unavailable)?;
        Ok(Self {
            pid,
            system: Mutex::new(system),
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl ProcessResourceSamplerPort for SystemProcessResourceSampler {
    fn sample(&self) -> Result<ProcessResourceSample, ProcessResourceSampleError> {
        let mut system = self
            .system
            .lock()
            .map_err(|_| ProcessResourceSampleError::Unavailable)?;
        refresh_current_process(&mut system, self.pid);
        let process = system
            .process(self.pid)
            .ok_or(ProcessResourceSampleError::Unavailable)?;
        let cpu_usage = process.cpu_usage();
        if !cpu_usage.is_finite() || cpu_usage.is_sign_negative() {
            return Err(ProcessResourceSampleError::Unavailable);
        }
        let basis_points = (f64::from(cpu_usage) * 100.0)
            .round()
            .clamp(0.0, f64::from(u32::MAX)) as u32;
        Ok(ProcessResourceSample::new(process.memory(), basis_points))
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn refresh_current_process(system: &mut System, pid: sysinfo::Pid) {
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing().with_memory().with_cpu(),
    );
}

#[cfg(any(target_os = "ios", target_os = "android"))]
fn set_screen_privacy(protected: bool) -> Result<(), ScreenPrivacyError> {
    let status = native_set_screen_privacy(protected).map_err(map_screen_privacy_bridge_error)?;
    match (protected, status.as_str()) {
        (true, "protected") | (false, "unprotected") => Ok(()),
        (_, "unavailable") => Err(ScreenPrivacyError::Unavailable),
        _ => Err(ScreenPrivacyError::Failed),
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn set_screen_privacy(_protected: bool) -> Result<(), ScreenPrivacyError> {
    Err(ScreenPrivacyError::Unavailable)
}

#[cfg(any(target_os = "ios", target_os = "android"))]
const fn map_screen_privacy_bridge_error(error: NativeBridgeError) -> ScreenPrivacyError {
    match error {
        NativeBridgeError::Unavailable => ScreenPrivacyError::Unavailable,
        NativeBridgeError::Failed => ScreenPrivacyError::Failed,
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
        assert_eq!(
            NativeScreenPrivacy.set_protected(true),
            Err(ScreenPrivacyError::Unavailable)
        );
        assert_eq!(
            NativeScreenPrivacy.set_protected(false),
            Err(ScreenPrivacyError::Unavailable)
        );
    }

    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
    #[test]
    fn system_sampler_reports_only_the_current_process() {
        let sampler = SystemProcessResourceSampler::new().expect("sampler should initialize");
        let sample = sampler.sample().expect("current process should be sampled");
        assert!(sample.resident_bytes() > 0);
    }
}
