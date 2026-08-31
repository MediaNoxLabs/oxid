// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

#[path = "capability_contracts/credentials.rs"]
mod credentials;
#[path = "capability_contracts/dids.rs"]
mod dids;
#[path = "capability_contracts/dust_and_shielded.rs"]
mod dust_and_shielded;
#[path = "capability_contracts/identity_routing.rs"]
mod identity_routing;
#[path = "capability_contracts/passport_vault.rs"]
mod passport_vault;
#[path = "capability_contracts/presentations.rs"]
mod presentations;
#[path = "capability_contracts/security.rs"]
mod security;
#[path = "capability_contracts/support.rs"]
mod support;
#[path = "capability_contracts/system.rs"]
mod system;
#[path = "capability_contracts/wallet.rs"]
mod wallet;
#[path = "capability_contracts/wallet_profiles.rs"]
mod wallet_profiles;
