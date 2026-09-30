// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import test from "node:test";

const root = new URL("../../", import.meta.url);
const requiredCoverage = [
  "onboarding and safe recovery boundary",
  "profile and realm switching",
  "Home, Receive, Send entry and blocked states",
  "wallet synchronization and status",
  "Documents reachability and empty state",
  "DID inventory reachability and empty state",
  "credential detail",
  "DID create, resolve, and detail",
  "issuance, presentation, SIOPv2 accept/refuse/outcome review",
  "Activity and fixture transaction detail",
  "Passport Vault review entry",
  "Passport Vault terminal outcomes",
  "Settings, security, and backup entry",
  "development capabilities, diagnostics, event log, and benchmark entry",
  "developer profile banner open and closed",
];
const privateSelector = /(?:did:[a-z0-9]|openid|https?:\/\/|request_uri|issuer[-_ ]?(?:id|uri)|recovery phrase|seed phrase|\$\{)/iu;

async function read(file) {
  return readFile(new URL(file, root), "utf8");
}

test("Maestro inventory is closed, classified, and references every runnable flow", async () => {
  const inventory = JSON.parse(await read("tests/maestro/inventory.json"));
  assert.equal(inventory.schema, "oxid-maestro-inventory-v1");
  assert.deepEqual(inventory.privacy.composition, ["demo", "dev"]);
  assert.equal(inventory.privacy.network, "simulated-or-undeployed-only");
  assert.deepEqual(inventory.privacy.publicCaptures, ["canonical-holder-evidence", "developer-profile-banner"]);
  assert.equal(inventory.privacy.failureArtifacts, "private-and-deleted");

  const coverage = new Set(inventory.scenarios.map(({ coverage: item }) => item));
  for (const item of requiredCoverage) assert.ok(coverage.has(item), `missing coverage classification: ${item}`);

  const referencedFlows = new Set();
  for (const scenario of inventory.scenarios) {
    assert.match(scenario.id, /^[a-z0-9]+(?:-[a-z0-9]+)*$/u);
    assert.ok(["demo", "dev"].includes(scenario.composition));
    assert.ok(["maestro", "lower-authoritative-layer", "manual-local"].includes(scenario.authority));
    assert.ok(Array.isArray(scenario.platforms));
    if (scenario.authority === "maestro") {
      assert.match(scenario.flow, /^flows\/[a-z0-9-]+\.yaml$/u);
      referencedFlows.add(scenario.flow.slice("flows/".length));
      const source = await read(`tests/maestro/${scenario.flow}`);
      assert.match(source, /^appId: io\.medianox\.oxid/mu);
      assert.match(source, /runFlow: \.\.\/subflows\/launch-clean\.yaml/u);
      if (!["onboarding-safe-boundary", "developer-profile-banner"].includes(scenario.id)) {
        assert.match(source, /runFlow: \.\.\/subflows\/demo-profile\.yaml/u);
      }
      assert.doesNotMatch(source, privateSelector, `${scenario.id} leaks a private or dynamic selector`);
    } else {
      assert.equal(scenario.flow, null);
      assert.match(scenario.reason, /authoritative|protected|proof|clean-state|evidence/iu);
    }
  }

  const flowNames = new Set((await readdir(new URL("tests/maestro/flows/", root))).filter((name) => name.endsWith(".yaml")));
  assert.deepEqual(flowNames, referencedFlows, "every checked-in modular flow must be inventory-owned");
});

test("Maestro subflows centralize clean setup and keep selectors privacy-safe", async () => {
  const flowNames = (await readdir(new URL("tests/maestro/flows/", root))).filter((name) => name.endsWith(".yaml"));
  const flows = await Promise.all(flowNames.map(async (name) => [name, await read(`tests/maestro/flows/${name}`)]));
  for (const [name, source] of flows) {
    assert.doesNotMatch(source, /launchApp:/u, `${name} must reuse launch setup`);
    assert.doesNotMatch(source, /Open saved DID details|Forget saved DID/u, `${name} must not select an identity value`);
    assert.doesNotMatch(source, privateSelector, `${name} must not interpolate or embed a private selector`);
    if (!["canonical-holder-evidence.yaml", "developer-profile-banner.yaml"].includes(name)) {
      assert.doesNotMatch(source, /takeScreenshot:/u);
    }
  }

  const [launch, profile, canonical] = await Promise.all([
    read("tests/maestro/subflows/launch-clean.yaml"),
    read("tests/maestro/subflows/demo-profile.yaml"),
    read("tests/maestro/flows/canonical-holder-evidence.yaml"),
  ]);
  for (const subflow of [launch, profile, await read("tests/maestro/subflows/open-menu.yaml")]) {
    assert.match(subflow, /^appId: io\.medianox\.oxid[\s\S]*\n---\n/mu);
  }
  assert.match(launch, /launchApp:[\s\S]*clearState: true/u);
  assert.match(launch, /extendedWaitUntil:[\s\S]*visible: "Create private wallet"[\s\S]*timeout: 60000/u);
  assert.match(profile, /Create or select demo profile/u);
  assert.doesNotMatch(
    profile,
    /Run demo action: (?:Derive Midnight account|Load simulated funding|Credential offer)|Generate recovery phrase|Create a DID/iu,
  );
  const developerBanner = await read("tests/maestro/flows/developer-profile-banner.yaml");
  assert.match(developerBanner, /assertVisible: "Developer profile"/u);
  assert.doesNotMatch(developerBanner, /demo-profile\.yaml/u);
  assert.match(developerBanner, /tapOn: "Dismiss developer profile notice for this session"/u);
  assert.match(developerBanner, /assertNotVisible: "Developer profile"/u);
  assert.equal(developerBanner.match(/takeScreenshot: developer-profile-banner-/gu)?.length, 2);
  assert.equal(canonical.match(/takeScreenshot: lunar-aegis-ios-/gu)?.length, 8);
  assert.doesNotMatch(canonical, /takeScreenshot:.*(?:recovery|credential|DID)/iu);
});

test("platform wrappers admit only inventory-owned flows and reuse build receipts", async () => {
  const [ios, android, runner, runScript] = await Promise.all([
    read("scripts/run-maestro-ios.sh"),
    read("scripts/run-maestro-android.sh"),
    read("scripts/test-ios-maestro-holder-evidence.sh"),
    read("run.sh"),
  ]);

  for (const wrapper of [ios, android]) {
    assert.match(wrapper, /--composition demo\|dev --flow <inventory-id>/u);
    assert.match(wrapper, /tests\/maestro\/inventory\.json/u);
    assert.match(wrapper, /authority == "maestro"/u);
    assert.match(wrapper, /run_phase build/u);
    assert.match(wrapper, /run_phase deploy/u);
    assert.match(wrapper, /--debug-output "\$debug_root"/u);
  }
  assert.match(ios, /run-ios-simulator\.sh ensure/u);
  assert.match(ios, /\.maestro-lane\.lock/u);
  assert.match(ios, /OXID_IOS_DEVICE must be an explicit simulator UDID/u);
  assert.match(ios, /--udid "\$OXID_IOS_DEVICE"/u);
  assert.match(android, /\^emulator-\[0-9\]\+\$/u);
  assert.match(android, /refusing non-emulator device/u);
  assert.match(android, /--device "\$OXID_ANDROID_DEVICE"/u);
  for (const wrapper of [ios, android]) {
    assert.match(wrapper, /if \[ "\$status" -ne 0 \]; then[\s\S]*rm -rf -- "\$artifact_root"/u);
  }
  assert.match(runner, /id != "canonical-holder-evidence"/u);
  assert.match(runner, /scenarios\+=\(canonical-holder-evidence\)/u);
  assert.equal(runScript.match(/node --test tests\/repository\/maestro-mobile-pilot-contract\.test\.mjs/gu)?.length, 1);
});

test("Maestro remains local, non-blocking, and additive to authoritative layers", async () => {
  const [packages, docs] = await Promise.all([
    read("nix/packages/default.nix"),
    read("docs/factory/maestro-mobile-pilot.md"),
  ]);
  assert.match(packages, /maestro = pkgs\.maestro;/u);
  assert.match(docs, /local only/iu);
  assert.match(docs, /CDP and iOS XCTest/u);
  assert.match(docs, /not GitHub CI/iu);
  assert.match(docs, /lower-authoritative-layer/u);
  assert.match(docs, /manual-local/u);

  const workflowNames = (await readdir(new URL(".github/workflows/", root))).filter((name) => /\.ya?ml$/u.test(name));
  for (const name of workflowNames) {
    assert.doesNotMatch(await read(`.github/workflows/${name}`), /maestro/iu, `${name} must not run local Maestro`);
  }
});

test("mobile visual accessibility evidence preserves the scoped matrix and privacy boundary", async () => {
  const [matrix, inventory, iosRunner, androidRunner] = await Promise.all([
    read("docs/factory/mobile-visual-accessibility-evidence.md"),
    read("tests/maestro/inventory.json").then(JSON.parse),
    read("scripts/test-ios-maestro-holder-evidence.sh"),
    read("scripts/test-android-maestro-semantic-evidence.sh"),
  ]);
  for (const scenario of inventory.scenarios) {
    assert.match(matrix, new RegExp(`\\| ${scenario.id} \\| ${scenario.authority} \\|`, "u"));
  }
  for (const heading of ["Scenario ID", "Authority", "Platform", "Design reference/no-match", "Artifact", "Evidence layer", "Known gap"]) {
    assert.match(matrix, new RegExp(heading, "u"));
  }
  assert.match(iosRunner, /oxid-ios-maestro-evidence-v2/u);
  assert.match(iosRunner, /simctl list runtimes -j/u);
  assert.match(iosRunner, /select\(\.identifier == \$runtime and \.isAvailable == true\)/u);
  assert.doesNotMatch(iosRunner, /\bmapfile\b/u);
  assert.match(iosRunner, /id != "canonical-holder-evidence"/u);
  assert.match(iosRunner, /scenarios\+=\(canonical-holder-evidence\)/u);
  assert.match(iosRunner, /scenario_outcomes/u);
  assert.match(androidRunner, /oxid-android-maestro-semantic-evidence-v1/u);
  assert.match(androidRunner, /emulator-\\*/u);
  assert.match(androidRunner, /OXID_ANDROID_DISPOSABLE/u);
  assert.doesNotMatch(androidRunner, /\bmapfile\b/u);
  assert.match(androidRunner, /rm -rf -- "\$artifact_root"/u);
  for (const state of [
    "Welcome and create-vs-restore fork",
    "Mandatory device-protection explanation",
    "Recovery boundary and Ready/Home",
    "Receive and Send entry",
    "Empty Documents and fixture Activity",
    "Settings and native-custody Backup boundary",
  ]) {
    assert.match(matrix, new RegExp(state, "u"));
  }
  for (const screenId of [
    "XSwTg6CjwXruX8QP3tXy",
    "FFMmLvVQlc5xIun63FYX",
    "xYA9BiozNUetlxPJYHPT",
    "7u81lbjNIKcn8dS79axb",
  ]) {
    assert.match(matrix, new RegExp(screenId, "u"));
  }
  for (const safeguard of [
    "375 pt/dp",
    "larger width",
    "safe-area/navigation non-overlap",
    "44 px touch targets",
    "large-text truncation",
    "non-color status meaning",
    "deterministic Back",
    "modal focus return",
    "reduced motion",
    "screen-reader labels/order",
  ]) {
    assert.match(matrix, new RegExp(safeguard, "u"));
  }
  assert.match(matrix, /target\/mobile-visual-accessibility\/<platform>/u);
  assert.match(matrix, /never capture a recovery phrase/iu);
  assert.match(matrix, /iOS Simulator.*Android Emulator/us);
});
