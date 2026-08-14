// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import test from "node:test";

import { composePassportVaultCallForConformance } from "../src/compose.mjs";

const ARTIFACTS_CONFIGURED = process.env.OXID_PASSPORT_VAULT_ARTIFACTS_DIR !== undefined;
const CONTRACT_STATE = process.env.OXID_PASSPORT_VAULT_CONTRACT_STATE_FIXTURE;

function request(operation, contractStateHex) {
  return {
    schemaVersion: 1,
    operation,
    chain: {
      contractStateHex,
      contractAddressHex: "00".repeat(32),
      zswapChainStateHex: null,
      ledgerParametersHex: null,
      networkId: "undeployed",
    },
    wallet: {
      coinPublicKeyHex: "1bd4f827be97ff013c4a702e4b08f30ec378728a54670cf7cc92cb9b1a14eff6",
      encryptionPublicKeyHex: "b62e630a030171b5e11af2487f0103e650cc703f284d0a478b2a3abdf9715b70",
    },
  };
}

test(
  "composes create, deposit, and withdraw through the generated client",
  { skip: !ARTIFACTS_CONFIGURED || CONTRACT_STATE === undefined },
  async () => {
    const created = await composePassportVaultCallForConformance(
      request(
        {
          kind: "create_lock",
          minimumAgeYears: 18,
          requiredIssuingStateHex: null,
          requiredDocumentNumberHex: null,
          maximumClaimAmount: "40",
          verifierChallengeHashHex: "01".repeat(32),
          initialAmount: "10",
        },
        CONTRACT_STATE,
      ),
    );
    assert.equal(created.circuitId, "createLock");
    assert.ok(created.unprovenTransactionBytes > 100);

    const deposited = await composePassportVaultCallForConformance(
      request(
        { kind: "deposit_to_lock", lockId: "2", amount: "5" },
        created.nextContractStateHex,
      ),
    );
    assert.equal(deposited.circuitId, "depositToLock");
    assert.ok(deposited.unprovenTransactionBytes > 100);

    const withdrawn = await composePassportVaultCallForConformance(
      request(
        {
          kind: "withdraw_from_lock",
          lockId: "2",
          amount: "1",
          recipientAddressHex: "03".repeat(32),
        },
        deposited.nextContractStateHex,
      ),
    );
    assert.equal(withdrawn.circuitId, "withdrawFromLock");
    assert.ok(withdrawn.unprovenTransactionBytes > 100);
  },
);
