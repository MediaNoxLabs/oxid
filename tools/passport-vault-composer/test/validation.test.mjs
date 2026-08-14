// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import test from "node:test";

import { ComposerError, composePassportVaultCall } from "../src/compose.mjs";

const PROTECTED_SCHEMA_TEST_CONFIGURED =
  process.env.OXID_PASSPORT_VAULT_ARTIFACTS_DIR !== undefined &&
  process.env.OXID_PASSPORT_VAULT_CONTRACT_STATE_FIXTURE !== undefined;

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

function protectedMaterial() {
  const bytes32 = () => Array(32).fill(0);
  const method = () => ({ didContractAddress: bytes32(), methodId: bytes32() });
  const point = () => ({ xLe: bytes32(), yLe: bytes32() });
  const proof = () => ({
    signer: method(),
    createdAt: "1",
    challengeHash: bytes32(),
    publicKey: point(),
    announcement: point(),
    responseLe: bytes32(),
  });
  const schema = {
    packageId: bytes32(),
    schemaId: bytes32(),
    majorVersion: 1,
    minorVersion: 0,
  };
  return {
    credential: {
      version: 1,
      ...schema,
      issuer: method(),
      holder: method(),
      issuedAt: "1",
      hasExpiration: false,
      expiresAt: "0",
      firstNameCommitment: bytes32(),
      lastNameCommitment: bytes32(),
      dateOfBirthCommitment: bytes32(),
      documentNumberCommitment: bytes32(),
      issuingStateCommitment: bytes32(),
      claimRoot: bytes32(),
    },
    credentialProof: proof(),
    presentation: {
      version: 1,
      ...schema,
      credentialClaimRoot: bytes32(),
      issuer: method(),
      holder: method(),
      disclosures: {
        revealFirstName: false,
        firstNameValuePadded: Array(64).fill(0),
        firstNameOpening: bytes32(),
        revealLastName: false,
        lastNameValuePadded: Array(64).fill(0),
        lastNameOpening: bytes32(),
        proveAgeOverThreshold: false,
        ageThresholdYears: 0,
        revealDocumentNumber: false,
        documentNumberValue: bytes32(),
        documentNumberOpening: bytes32(),
        revealIssuingState: false,
        issuingStateValue: bytes32(),
        issuingStateOpening: bytes32(),
      },
    },
    presentationProof: proof(),
    currentDay: 1,
    witness: {
      holderDateOfBirthDays: 1,
      holderDateOfBirthOpening: bytes32(),
    },
  };
}

test("rejects malformed protected claim material before loading generated artifacts", async () => {
  await rejectsWith(
    { ...base, operation: { kind: "claim_from_lock", credential: "forbidden" } },
    "invalid_request",
  );
});

test(
  "accepts only the fixed protected claim shape before circuit validation",
  { skip: !PROTECTED_SCHEMA_TEST_CONFIGURED },
  async () => {
    const operation = {
      kind: "claim_from_lock",
      lockId: "0",
      amount: "1",
      recipientAddressHex: "33".repeat(32),
      material: protectedMaterial(),
    };
    const request = {
      ...base,
      operation,
      chain: {
        ...base.chain,
        contractStateHex: process.env.OXID_PASSPORT_VAULT_CONTRACT_STATE_FIXTURE,
      },
    };
    await rejectsWith(request, "composition_failed");
    operation.material.credential.unexpected = "secret-smuggling";
    await rejectsWith(request, "invalid_request");
  },
);

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
