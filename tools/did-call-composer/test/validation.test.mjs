// SPDX-License-Identifier: Apache-2.0

import test from "node:test";
import assert from "node:assert/strict";

import { ComposerError, composeDidCall } from "../src/compose.mjs";

test("rejects unbounded or secret-free input before loading artifacts", async () => {
  await assert.rejects(
    composeDidCall({ schemaVersion: 1 }),
    (error) => error instanceof ComposerError && error.code === "invalid_request",
  );
});

test("rejects an unsupported operation before loading artifacts", async () => {
  const hex = "01".repeat(32);
  await assert.rejects(
    composeDidCall({
      schemaVersion: 1,
      operation: { kind: "deactivate", methodId: "#key-auth", publicKey: null },
      chain: {
        contractStateHex: "01",
        contractAddressHex: hex,
        zswapChainStateHex: null,
        ledgerParametersHex: null,
        networkId: "undeployed",
        timestampMillis: 1,
      },
      wallet: { coinPublicKeyHex: hex, encryptionPublicKeyHex: hex },
      controller: { secretHex: hex },
    }),
    (error) => error instanceof ComposerError && error.code === "invalid_request",
  );
});

test("rejects malformed public method material before loading artifacts", async () => {
  const hex = "01".repeat(32);
  await assert.rejects(
    composeDidCall({
      schemaVersion: 1,
      operation: {
        kind: "add_authentication_method",
        methodId: "#key-auth",
        publicKey: { xHex: "01" },
      },
      chain: {
        contractStateHex: "01",
        contractAddressHex: hex,
        zswapChainStateHex: null,
        ledgerParametersHex: null,
        networkId: "undeployed",
        timestampMillis: 1,
      },
      wallet: { coinPublicKeyHex: hex, encryptionPublicKeyHex: hex },
      controller: { secretHex: hex },
    }),
    (error) => error instanceof ComposerError && error.code === "invalid_request",
  );
});
