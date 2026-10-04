// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const root = new URL("../../", import.meta.url);

test("standalone iOS acceptance owns and cleans every mutated runtime resource", async () => {
  const [script, swift] = await Promise.all([
    readFile(new URL("scripts/test-ios-standalone-local.sh", root), "utf8"),
    readFile(
      new URL("tests/mobile/ios/OxidUITests/StandaloneLocalAccountTests.swift", root),
      "utf8",
    ),
  ]);

  assert.match(script, /oxid_ios_supervise_acceptance "\$ROOT" ios-standalone-local 3600/u);
  assert.match(script, /oxid_ios_create_owned/u);
  assert.match(script, /oxid_ios_owned_simctl/u);
  assert.match(script, /oxid_ios_delete_owned/u);
  assert.match(script, /occupied-standalone-stack/u);
  assert.match(script, /OXID_STANDALONE_STATE_DIR="\$STACK_STATE"/u);
  assert.match(script, /standalone-down\.sh/u);
  assert.match(script, /stop_faucet/u);
  assert.match(script, /receiptOwnedSimulator:true/u);
  assert.match(script, /receiptOwnedStandaloneStack:true/u);
  assert.match(script, /privateDiagnosticsRemoved:true/u);
  assert.match(script, /manualSyncUsed:false/u);
  assert.match(script, /fixedGrantNight:50000/u);
  assert.doesNotMatch(script, /simctl list devices booted/u);

  assert.match(swift, /requestFixedGrant\(for: address\)/u);
  assert.match(swift, /Transfer confirmed/u);
  assert.match(swift, /staticTexts\["50000"\]/u);
  assert.doesNotMatch(swift, /buttons\["Sync now"\]\.tap/u);
});
