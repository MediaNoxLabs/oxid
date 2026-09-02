// SPDX-License-Identifier: Apache-2.0

//! Explicit read-only PreProd composition for owner-entered mobile recovery.
//!
//! Only public signed deployment material lives here. The wallet root is never
//! accepted by this module and reaches the process only through the typed UI
//! and platform-backed custody boundary.

use std::{
    fmt,
    time::{SystemTime, UNIX_EPOCH},
};

use oxid_adapter_deployment_profile::{
    AuthenticatedDeploymentProfile, DeploymentProfileVerifier, DeploymentTrustRoot,
};

use crate::{
    ApplicationServices, ProductionDeploymentCompositionError, authenticate_production_deployment,
    compose_authenticated_production,
};

const PREPROD_AUDIENCE: &str = "io.medianox.oxid";
const PREPROD_PROFILE_ID: &str = "oxid-preprod-registration-e2e-2026-08";
const PREPROD_SIGNING_KEY_ID: &str = "oxid-preprod-e2e-2026-01";
const PREPROD_PROFILE_VALID_FROM_SECONDS: u64 = 1_782_864_000;
const PREPROD_PROFILE_VALID_UNTIL_SECONDS: u64 = 1_893_456_000;
const PREPROD_PROFILE_VERIFYING_KEY: [u8; 32] = [
    0x78, 0x67, 0x5f, 0xb8, 0x60, 0xe6, 0xcc, 0xde, 0xaa, 0xf5, 0xe4, 0xd9, 0xc2, 0x7e, 0x0a, 0xa7,
    0x80, 0xdd, 0x11, 0x7c, 0xbd, 0x58, 0x38, 0x21, 0xb4, 0x6b, 0x77, 0xb9, 0xcd, 0xfd, 0x3f, 0x5f,
];
const PREPROD_PROFILE_ENVELOPE: &[u8] =
    include_bytes!("../tests/fixtures/preprod-registration-deployment-profile.json");

/// Payload-free startup failures for the opt-in PreProd observation profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreprodObservationCompositionError {
    ClockUnavailable,
    InvalidTrustPolicy,
    DeploymentProfileRejected,
    RuntimeUnavailable,
    Deployment(ProductionDeploymentCompositionError),
}

impl fmt::Display for PreprodObservationCompositionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ClockUnavailable => "trusted system time is unavailable",
            Self::InvalidTrustPolicy => "embedded PreProd trust policy is invalid",
            Self::DeploymentProfileRejected => "embedded PreProd deployment profile was rejected",
            Self::RuntimeUnavailable => "PreProd authentication runtime is unavailable",
            Self::Deployment(_) => "PreProd deployment authentication failed",
        })
    }
}

impl std::error::Error for PreprodObservationCompositionError {}

/// Verifies the embedded public profile at a caller-supplied trusted time.
/// Kept separate from node authentication so validity/rollback tests remain
/// deterministic and network-free.
fn verified_preprod_profile(
    now_seconds: u64,
) -> Result<AuthenticatedDeploymentProfile, PreprodObservationCompositionError> {
    let root = DeploymentTrustRoot::new(
        PREPROD_SIGNING_KEY_ID,
        PREPROD_PROFILE_VERIFYING_KEY,
        PREPROD_PROFILE_VALID_FROM_SECONDS,
        PREPROD_PROFILE_VALID_UNTIL_SECONDS,
        None,
        1,
    )
    .map_err(|_| PreprodObservationCompositionError::InvalidTrustPolicy)?;
    let verifier = DeploymentProfileVerifier::new(PREPROD_AUDIENCE, [root], 1)
        .map_err(|_| PreprodObservationCompositionError::InvalidTrustPolicy)?;
    let profile = verifier
        .verify(PREPROD_PROFILE_ENVELOPE, now_seconds)
        .map_err(|_| PreprodObservationCompositionError::DeploymentProfileRejected)?;
    if profile.profile_id() != PREPROD_PROFILE_ID
        || profile.signing_key_id() != PREPROD_SIGNING_KEY_ID
        || profile.midnight().network_id() != "preprod"
    {
        return Err(PreprodObservationCompositionError::DeploymentProfileRejected);
    }
    Ok(profile)
}

/// Authenticates the embedded signed profile and its live node genesis before
/// exposing the observation-only mobile recovery capability.
pub fn compose_preprod_observation()
-> Result<ApplicationServices, PreprodObservationCompositionError> {
    let now_seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| PreprodObservationCompositionError::ClockUnavailable)?
        .as_secs();
    let profile = verified_preprod_profile(now_seconds)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| PreprodObservationCompositionError::RuntimeUnavailable)?;
    let deployment = runtime
        .block_on(authenticate_production_deployment(profile))
        .map_err(PreprodObservationCompositionError::Deployment)?;
    compose_authenticated_production(deployment)
        .map_err(PreprodObservationCompositionError::Deployment)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_profile_is_signed_current_and_exactly_preprod() {
        let profile = verified_preprod_profile(1_800_000_000).expect("profile");
        assert_eq!(profile.profile_id(), PREPROD_PROFILE_ID);
        assert_eq!(profile.sequence(), 1);
        assert_eq!(profile.midnight().network_id(), "preprod");
        assert_eq!(
            profile.midnight().indexer_http_url(),
            "https://indexer.preprod.midnight.network/api/v4/graphql"
        );
        assert_eq!(
            profile.midnight().node_websocket_url(),
            "wss://rpc.preprod.midnight.network"
        );
    }

    #[test]
    fn embedded_profile_rejects_not_yet_valid_and_stale_time() {
        assert_eq!(
            verified_preprod_profile(PREPROD_PROFILE_VALID_FROM_SECONDS - 1),
            Err(PreprodObservationCompositionError::DeploymentProfileRejected)
        );
        assert_eq!(
            verified_preprod_profile(PREPROD_PROFILE_VALID_UNTIL_SECONDS),
            Err(PreprodObservationCompositionError::DeploymentProfileRejected)
        );
    }

    #[test]
    fn startup_error_never_exposes_profile_verifier_payloads() {
        let upstream = oxid_adapter_deployment_profile::DeploymentProfileError::InvalidSignature;
        assert!(!format!("{upstream}").contains("indexer.preprod"));
        assert_eq!(
            PreprodObservationCompositionError::DeploymentProfileRejected.to_string(),
            "embedded PreProd deployment profile was rejected"
        );
    }
}
