// SPDX-License-Identifier: Apache-2.0

//! Closed errors for the development-only standalone funding adapter.

use std::{error::Error, fmt};

/// Startup failures intentionally reveal no custody or transaction material.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaucetStartupError {
    ProfileUnavailable,
    AmbiguousAuthority,
    ProtectionUnavailable,
    AccountUnavailable,
}

impl fmt::Display for FaucetStartupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ProfileUnavailable => "standalone funding profile is unavailable",
            Self::AmbiguousAuthority => "standalone funding profile is ambiguous",
            Self::ProtectionUnavailable => "standalone funding protection is unavailable",
            Self::AccountUnavailable => "standalone funding account is unavailable",
        })
    }
}

impl Error for FaucetStartupError {}

/// Failures while reading or writing the faucet protocol stream.
#[derive(Debug)]
pub enum FaucetIoError {
    Read(std::io::Error),
    Write(std::io::Error),
    Serialize(serde_json::Error),
}

impl fmt::Display for FaucetIoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("standalone faucet protocol I/O failed")
    }
}

impl Error for FaucetIoError {}
