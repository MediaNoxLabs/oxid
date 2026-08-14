// SPDX-License-Identifier: Apache-2.0

//! Deterministic contract-state fixture for the headless development harness.
//! It is deliberately labelled as simulation and cannot satisfy the live
//! finalized-node replay constructor.

use std::sync::Arc;

use oxid_passport_vault_application::{
    PassportVaultContractStateAuthentication, PassportVaultContractStateReadFuture,
    PassportVaultContractStateSnapshot, PassportVaultContractStateSourceError,
    PassportVaultContractStateSourcePort,
};

pub const SIMULATED_PASSPORT_VAULT_CONTRACT_ADDRESS_HEX: &str =
    "9d57c7c697a747bac5b8c5828686728049d2e032cf98ff357607f086a3916fd0";
const SIMULATED_DEPLOYMENT_TRANSACTION_HASH_HEX: &str =
    "39c69049a72910796547714d804d59f38a51a7b0315df1e517e39fea57b70c79";
const SIMULATED_ACTION_BLOCK_HASH_HEX: &str =
    "cc4d0f5ad40aab5c4aed0d053299c3a0b12eb7320a6be143fd7e77e04a9988f7";
const SIMULATED_FINALIZED_HEAD_HASH_HEX: &str =
    "aa5e59f371f63cf91cd83cd126c95d8491adae8c991fd8f728c2d46c93972ab6";
const SIMULATED_ACTION_BLOCK_HEIGHT: u64 = 3;
const SIMULATED_FINALIZED_HEAD_HEIGHT: u64 = 5;
const FIXTURE_HEX: &str = include_str!("../../../../fixtures/passport-vault/contract-state-v1.hex");

#[derive(Clone)]
pub struct SimulatedPassportVaultStateSource {
    snapshot: Arc<PassportVaultContractStateSnapshot>,
}

impl SimulatedPassportVaultStateSource {
    pub fn new() -> Result<Self, PassportVaultContractStateSourceError> {
        let serialized_contract_state = hex::decode(FIXTURE_HEX.trim())
            .map_err(|_| PassportVaultContractStateSourceError::InvalidResponse)?;
        if serialized_contract_state.is_empty() {
            return Err(PassportVaultContractStateSourceError::InvalidResponse);
        }
        Ok(Self {
            snapshot: Arc::new(PassportVaultContractStateSnapshot {
                serialized_contract_state,
                authentication: PassportVaultContractStateAuthentication::DeterministicSimulation,
                contract_address_hex: SIMULATED_PASSPORT_VAULT_CONTRACT_ADDRESS_HEX.to_owned(),
                transaction_hash_hex: SIMULATED_DEPLOYMENT_TRANSACTION_HASH_HEX.to_owned(),
                action_block_hash_hex: SIMULATED_ACTION_BLOCK_HASH_HEX.to_owned(),
                action_block_height: SIMULATED_ACTION_BLOCK_HEIGHT,
                finalized_head_hash_hex: SIMULATED_FINALIZED_HEAD_HASH_HEX.to_owned(),
                finalized_head_height: SIMULATED_FINALIZED_HEAD_HEIGHT,
            }),
        })
    }

    #[must_use]
    pub const fn contract_address_hex(&self) -> &'static str {
        SIMULATED_PASSPORT_VAULT_CONTRACT_ADDRESS_HEX
    }
}

impl PassportVaultContractStateSourcePort for SimulatedPassportVaultStateSource {
    fn read<'a>(
        &'a self,
        contract_address_hex: &'a str,
    ) -> PassportVaultContractStateReadFuture<'a> {
        if contract_address_hex != SIMULATED_PASSPORT_VAULT_CONTRACT_ADDRESS_HEX {
            return Box::pin(async { Err(PassportVaultContractStateSourceError::NotFound) });
        }
        let snapshot = self.snapshot.as_ref().clone();
        Box::pin(async move { Ok(snapshot) })
    }
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;
    use oxid_passport_vault_application::PassportVaultContractStateAuthentication;

    use super::*;

    #[test]
    fn fixture_is_explicitly_simulated_and_address_scoped() {
        let source = SimulatedPassportVaultStateSource::new().expect("valid fixture");
        let snapshot = block_on(source.read(source.contract_address_hex())).expect("snapshot");
        assert_eq!(
            snapshot.authentication,
            PassportVaultContractStateAuthentication::DeterministicSimulation
        );
        assert_eq!(snapshot.contract_address_hex, source.contract_address_hex());
        assert!(!snapshot.serialized_contract_state.is_empty());
        assert_eq!(
            block_on(source.read(&"11".repeat(32))),
            Err(PassportVaultContractStateSourceError::NotFound)
        );
    }
}
