// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import test from "node:test";

import { ComposerError, composePassportVaultCall } from "../src/compose.mjs";

const base = {
  schemaVersion: 1,
  operation: {
    kind: "deposit_to_lock",
    lockId: "0",
    amount: "1",
  },
  chain: {
    contractStateHex: "00",
    contractAddressHex: "00".repeat(32),
    zswapChainStateHex: null,
    ledgerParametersHex: null,
    networkId: "undeployed",
  },
  wallet: {
    coinPublicKeyHex: "11".repeat(32),
    encryptionPublicKeyHex: "22".repeat(32),
  },
};

async function rejectsWith(request, code) {
  await assert.rejects(
    composePassportVaultCall(request),
    (error) => error instanceof ComposerError && error.code === code,
  );
}

test("rejects claim material before loading generated artifacts", async () => {
  await rejectsWith(
    { ...base, operation: { kind: "claim_from_lock", credential: "forbidden" } },
    "claim_requires_protected_custody",
  );
});

test("rejects the administrative circuit", async () => {
  await rejectsWith(
    { ...base, operation: { kind: "set_trusted_issuer" } },
    "administrative_circuit_forbidden",
  );
});

test("rejects unknown fields and non-canonical amounts", async () => {
  await rejectsWith({ ...base, secret: "must-not-cross" }, "invalid_request");
  await rejectsWith(
    { ...base, operation: { kind: "deposit_to_lock", lockId: "0", amount: "01" } },
    "invalid_request",
  );
  await rejectsWith(
    { ...base, operation: { kind: "deposit_to_lock", lockId: "0", amount: "0" } },
    "invalid_request",
  );
});
