// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const root = new URL("../../", import.meta.url);

test("iOS lifecycle diagnostic owns one simulator and records only closed outcomes", async () => {
  const [script, swift, project, justfile, lifecycle] = await Promise.all([
    readFile(new URL("scripts/test-ios-wallet-lifecycle-simulator.sh", root), "utf8"),
    readFile(new URL("tests/mobile/ios/OxidUITests/LifecycleRecoveryTests.swift", root), "utf8"),
    readFile(new URL("tests/mobile/ios/project.yml", root), "utf8"),
    readFile(new URL("Justfile", root), "utf8"),
    readFile(new URL("crates/ui-dioxus/src/wallet_realm_lifecycle.rs", root), "utf8"),
  ]);

  assert.match(script, /oxid_ios_create_owned/u);
  assert.match(script, /oxid_ios_owned_simctl/u);
  assert.match(script, /oxid_ios_delete_owned/u);
  assert.match(script, /\[ -z "\$\{OXID_IOS_DEVICE:-\}" \]/u);
  assert.match(script, /status --porcelain/u);
  assert.match(script, /-only-testing:"OxidUITests\/LifecycleRecoveryTests\//u);
  assert.match(script, /manualFamilySync:"not_used"/u);
  assert.doesNotMatch(script, /Sync DUST|Sync shielded assets/u);

  assert.match(swift, /XCUIDevice\.shared\.press\(\.home\)/u);
  assert.match(swift, /application\.terminate\(\)/u);
  assert.match(swift, /application\.launch\(\)/u);
  assert.doesNotMatch(swift, /buttons\["Sync now"\]\.tap/u);
  assert.match(project, /OXID_LIFECYCLE_DIAGNOSTIC_PATH/u);
  assert.match(justfile, /ios-wallet-lifecycle-simulator:/u);
  assert.match(lifecycle, /use_effect\(move \|\|/u);
  assert.match(lifecycle, /spawn\(async move/u);
  assert.doesNotMatch(lifecycle, /use_future\(move \|\|/u);
});
