// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::time::{SystemTime, UNIX_EPOCH};

use oxid_foundation::UnixTimestampMillis;
use oxid_platform_ports::{ClockPort, PlatformError, RandomPort};

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
}
