// SPDX-License-Identifier: Apache-2.0

//! Shared custody migration contract, independent of JNI and Swift transport.
//!
//! Control JSON is closed, bounded and secret-free. Custody uses a separate
//! mutable-byte channel, never serde or strings. The legacy platform entrypoints
//! do not implement this contract yet; see `custody-contract.md`.

use std::fmt;

use serde::{Deserialize, Serialize};
use zeroize::{Zeroize as _, Zeroizing};

pub const MAX_CUSTODY_BYTES: usize = 512 * 1024;
pub const MAX_CONTROL_BYTES: usize = 512;

/// Closed, payload-free errors. Never attach native exception text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Unavailable,
    NotInitialized,
    AlreadyInitialized,
    Locked,
    AuthorizationDenied,
    Cancelled,
    TimedOut,
    Invalid,
    Failed,
}

/// Fixed-size, non-cloneable custody allocation. Debug never reveals material.
/// No Serialize, Deserialize, Display, or owned unprotected conversion exists.
/// Dropping this value wipes its entire allocation, including on unwinding.
pub struct CustodyBytes(Zeroizing<Box<[u8]>>);

impl fmt::Debug for CustodyBytes {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CustodyBytes([REDACTED])")
    }
}

impl CustodyBytes {
    /// Allocate zeroed Rust storage before native code writes the first secret
    /// byte. The callback must fill the exact slice or fail, never retain it.
    /// JNI/Swift must wipe their own temporary buffers on every exit as well.
    pub fn receive(
        len: usize,
        fill: impl FnOnce(&mut [u8]) -> Result<usize, Error>,
    ) -> Result<Self, Error> {
        if len == 0 || len > MAX_CUSTODY_BYTES {
            return Err(Error::Invalid);
        }
        let mut bytes = Zeroizing::new(vec![0; len].into_boxed_slice());
        if fill(&mut bytes)? != len {
            return Err(Error::Invalid);
        }
        Ok(Self(bytes))
    }

    /// Copy from exclusively borrowed native mutable storage, then wipe that
    /// source even when its length is rejected. This is not a String decoder.
    pub fn take_native(source: &mut [u8]) -> Result<Self, Error> {
        let result = Self::receive(source.len(), |destination| {
            destination.copy_from_slice(source);
            Ok(source.len())
        });
        source.zeroize();
        result
    }

    /// Borrow only for synchronous decode/use. Do not log or retain a copy.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Inspect,
    Initialize,
    Unlock,
    Load,
    Save,
    Lock,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Protection {
    OperatingSystem,
    HardwareBacked,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Status {
    Succeeded,
    Uninitialized,
    Locked,
    Unlocked,
    Unavailable,
    NotInitialized,
    AlreadyInitialized,
    AuthorizationDenied,
    Cancelled,
    TimedOut,
    Invalid,
    Failed,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Control {
    version: u8,
    operation: Operation,
    status: Status,
    #[serde(skip_serializing_if = "Option::is_none")]
    protection: Option<Protection>,
}

/// Operation-validated result. Only load/unlock can carry custody.
#[derive(Debug)]
pub enum Reply {
    Uninitialized,
    Locked(Protection),
    Unlocked(Protection),
    Stored(Protection),
    Material {
        protection: Protection,
        bytes: CustodyBytes,
    },
}

/// Secret-free state returned by `inspect`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CustodyState {
    Uninitialized,
    Locked(Protection),
    Unlocked(Protection),
}

/// Custody and its native protection class stay coupled across the bridge.
#[derive(Debug)]
pub struct ProtectedCustody {
    pub protection: Protection,
    pub bytes: CustodyBytes,
}

/// Validate one synchronous native completion. Callers retain the operation and
/// exact profile binding; native callbacks must never select their own caller.
/// Invalid, denied, cancelled and timed-out completions drop any supplied bytes.
/// Parse failures deliberately discard serde's potentially payload-bearing text.
pub fn decode_reply(
    expected: Operation,
    control: &[u8],
    bytes: Option<CustodyBytes>,
) -> Result<Reply, Error> {
    if control.is_empty() || control.len() > MAX_CONTROL_BYTES {
        return Err(Error::Invalid);
    }
    let control: Control = serde_json::from_slice(control).map_err(|_| Error::Invalid)?;
    if control.version != 1 || control.operation != expected {
        return Err(Error::Invalid);
    }
    if control.status != Status::Succeeded && bytes.is_some() {
        return Err(Error::Invalid);
    }
    match control.status {
        Status::Succeeded => {
            let protection = control.protection.ok_or(Error::Invalid)?;
            match (expected, bytes) {
                (Operation::Load | Operation::Unlock, Some(bytes)) => {
                    Ok(Reply::Material { protection, bytes })
                }
                (Operation::Initialize | Operation::Save, None) => Ok(Reply::Stored(protection)),
                _ => Err(Error::Invalid),
            }
        }
        Status::Uninitialized => {
            if expected == Operation::Inspect && control.protection.is_none() {
                Ok(Reply::Uninitialized)
            } else {
                Err(Error::Invalid)
            }
        }
        Status::Locked => match (expected, control.protection) {
            (Operation::Inspect | Operation::Lock, Some(protection)) => {
                Ok(Reply::Locked(protection))
            }
            (_, None) => Err(Error::Locked),
            (_, Some(_)) => Err(Error::Invalid),
        },
        Status::Unlocked => match (expected, control.protection) {
            (Operation::Inspect, Some(protection)) => Ok(Reply::Unlocked(protection)),
            _ => Err(Error::Invalid),
        },
        Status::Unavailable => payload_free_error(control.protection, Error::Unavailable),
        Status::NotInitialized => payload_free_error(control.protection, Error::NotInitialized),
        Status::AlreadyInitialized => {
            payload_free_error(control.protection, Error::AlreadyInitialized)
        }
        Status::AuthorizationDenied => {
            payload_free_error(control.protection, Error::AuthorizationDenied)
        }
        Status::Cancelled => payload_free_error(control.protection, Error::Cancelled),
        Status::TimedOut => payload_free_error(control.protection, Error::TimedOut),
        Status::Invalid => payload_free_error(control.protection, Error::Invalid),
        Status::Failed => payload_free_error(control.protection, Error::Failed),
    }
}

fn payload_free_error(protection: Option<Protection>, error: Error) -> Result<Reply, Error> {
    if protection.is_none() {
        Err(error)
    } else {
        Err(Error::Invalid)
    }
}

/// Transport seam for independently migrated platforms. Methods must be
/// synchronous and bind one exact profile and native authorization session.
/// Initialize/unlock may request presence; inspect/load/save/lock must not
/// silently prompt. A locked load/save fails; only explicit unlock recovers.
/// No implementation may translate these bytes through the legacy JSON bridge.
pub trait CustodyTransport: Send + Sync {
    fn inspect(&self, profile_id: &str) -> Result<CustodyState, Error>;
    fn initialize(&self, profile_id: &str, bytes: &CustodyBytes) -> Result<Protection, Error>;
    fn unlock(&self, profile_id: &str, reason: &str) -> Result<ProtectedCustody, Error>;
    fn load(&self, profile_id: &str) -> Result<ProtectedCustody, Error>;
    fn save(&self, profile_id: &str, bytes: &CustodyBytes) -> Result<Protection, Error>;
    fn lock(&self, profile_id: &str) -> Result<Protection, Error>;
}

#[cfg(test)]
#[path = "custody_tests.rs"]
mod tests;
