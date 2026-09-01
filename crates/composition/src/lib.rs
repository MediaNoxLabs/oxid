// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

#[cfg(feature = "mobile-portal")]
#[cfg(not(any(target_os = "ios", target_os = "android")))]
compile_error!("mobile-portal is available only on iOS and Android");

#[cfg(all(feature = "mobile-portal-tailnet", not(target_os = "android")))]
compile_error!("mobile-portal-tailnet is available only on Android");

#[cfg(not(target_arch = "wasm32"))]
#[cfg(any(
    not(any(target_os = "ios", target_os = "android")),
    feature = "mobile-portal"
))]
mod portal;

mod environment;
mod identity;
mod passport_vault;
mod profile_environment;
mod profile_headless;
mod profile_in_memory;
mod profile_mobile;
mod profile_production;
mod services;
#[cfg(not(target_arch = "wasm32"))]
#[cfg(any(test, feature = "standalone-development"))]
mod standalone_genesis;
mod wiring;
pub use environment::*;
pub use identity::*;
pub use passport_vault::simulated_passport_vault_contract_address_hex;
pub use profile_environment::*;
pub use profile_headless::*;
pub use profile_in_memory::*;
#[cfg(any(
    test,
    feature = "standalone-development",
    target_os = "ios",
    target_os = "android"
))]
pub use profile_mobile::*;
pub use profile_production::*;
pub use services::ApplicationServices;

#[cfg(all(test, not(target_arch = "wasm32")))]
mod standalone_funding_tests;
#[cfg(test)]
mod verification;
