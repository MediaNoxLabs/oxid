// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  auditCompatibility,
  validatePortalDidPackages,
} from "../../scripts/check-midnight-integration-compatibility.mjs";

test("reviewed wallet, Standalone, and Portal pins form one DID release line", () => {
  const result = auditCompatibility();
  assert.equal(result.state, "compatible");
  assert.equal(result.contractRelease, "0.4.0");
  assert.equal(result.demoReady, false, "compatibility must not masquerade as native holder-DID readiness");
  assert.match(result.manifestSha256, /^[0-9a-f]{64}$/u);
});

test("a deliberate Portal DID package mismatch fails closed", () => {
  const dependencies = Object.fromEntries([
    "midnight-did",
    "midnight-did-api",
    "midnight-did-contract",
    "midnight-did-domain",
    "midnight-did-jubjub-schnorr",
  ].map((name) => [`@midnight-ntwrk/${name}`, "0.5.0"]));
  assert.throws(
    () => validatePortalDidPackages({ dependencies }, "0.4.0"),
    /differs from the reviewed DID release/u,
  );
});

test("a runtime proof-server override cannot bypass the reviewed image pin", () => {
  assert.throws(
    () => auditCompatibility({ environment: { PROOF_SERVER_IMAGE: "example.invalid/proof:drifted" } }),
    /proof-server image override differs/u,
  );
});

test("cleanup remains receipt-owned when compatibility inputs change", async () => {
  const lifecycle = await readFile(new URL("../../scripts/portal-consumer-lifecycle.sh", import.meta.url), "utf8");
  assert.match(lifecycle, /if \[ "\$OPERATION" = down \]; then[\s\S]*COMPATIBILITY_MANIFEST_SHA256=""/u);
  assert.match(lifecycle, /run_down\(\)[\s\S]*cleanup_receipt_valid \|\| cleanup_starting_receipt_valid/u);
  assert.match(lifecycle, /PREPARED_RECEIPT_SCHEMA="oxid-portal-consumer-prepared-v2"/u);
  assert.match(lifecycle, /discard_legacy_prepared_state/u);
});
