// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{error::Error, fmt};

use oxid_foundation::UnixTimestampMillis;

/// Safe, adapter-neutral platform failure categories.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlatformError {
    ClockUnavailable,
    RandomnessUnavailable,
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
