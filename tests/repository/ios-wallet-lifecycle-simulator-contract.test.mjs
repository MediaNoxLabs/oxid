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
  assert.match(script, /oxid_ios_supervise_acceptance "\$ROOT" ios-wallet-lifecycle 1800/u);
  assert.match(script, /oxid_ios_run_xctest "\$ROOT" lifecycle-background-recovery 1200/u);
  assert.match(script, /\[ -z "\$\{OXID_IOS_DEVICE:-\}" \]/u);
  assert.match(script, /status --porcelain/u);
  assert.match(script, /-only-testing:"OxidUITests\/LifecycleRecoveryTests\//u);
  assert.match(script, /manualFamilySync:"not_used"/u);
  assert.doesNotMatch(script, /OXID_MOBILE_CUSTODY=native/u);
  assert.match(swift, /buttons\["Use public demo wallet"\]/u);
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

test("profile acceptance serializes the host and bounds each XCTest scenario", async () => {
  const [script, profileFlow] = await Promise.all([
    readFile(new URL("scripts/test-ios-profile-flow.sh", root), "utf8"),
    readFile(new URL("tests/mobile/ios/OxidUITests/ProfileFlowTests.swift", root), "utf8"),
  ]);
  assert.match(script, /oxid_ios_supervise_acceptance "\$repository_root" ios-profile-flow 3600/u);
  assert.match(script, /oxid_ios_run_xctest "\$repository_root" "\$scenario_name" 600/u);
  assert.match(profileFlow, /XCTNSPredicateExpectation\([\s\S]*?value == %@/u);
  assert.match(profileFlow, /shieldedTransfer\.tap\(\)[\s\S]*?waitForSwitch\(shieldedTransfer, value: "1"\)/u);
  assert.doesNotMatch(profileFlow, /shieldedTransfer\.tap\(\)[\s\S]{0,120}sleep/u);
});

test("development XCTest fixtures complete the protected recovery ceremony", async () => {
  const fixture = await readFile(
    new URL("tests/mobile/ios/OxidUITests/WalletOnboardingFixture.swift", root),
    "utf8",
  );
  const migrated = await Promise.all([
    "ProfileFlowTests.swift",
    "DeveloperProfileTests.swift",
    "BackupFlowTests.swift",
    "IdentityIngressTests.swift",
    "StandaloneLocalAccountTests.swift",
  ].map((name) => readFile(
    new URL(`tests/mobile/ios/OxidUITests/${name}`, root),
    "utf8",
  )));

  assert.match(fixture, /Generate recovery phrase/u);
  assert.match(fixture, /New wallet recovery phrase/u);
  assert.match(fixture, /I have securely saved or verified this recovery phrase\./u);
  assert.match(fixture, /Finish and open wallet/u);
  assert.match(fixture, /must not expose the retired protection bypass/u);
  for (const source of migrated) {
    assert.match(source, /WalletOnboardingFixture\.completeDevelopmentRecoveryCeremony/u);
    assert.doesNotMatch(source, /Skip for now/u);
  }
});
