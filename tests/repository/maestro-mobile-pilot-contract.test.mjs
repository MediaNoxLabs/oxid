// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readdir, readFile } from "node:fs/promises";
import test from "node:test";

const root = new URL("../../", import.meta.url);
const requiredCoverage = [
  "onboarding and safe recovery boundary",
  "wallet restore entry and safe cancellation boundary",
  "profile and realm switching",
  "Home, Receive, Send entry and blocked states",
  "wallet synchronization and status",
  "Documents reachability and empty state",
  "DID inventory reachability and empty state",
  "credential detail",
  "DID create, resolve, and detail",
  "credential-offer review consent boundary and refusal",
  "credential-presentation review consent boundary and refusal before proof generation",
  "presentation proof/finality and SIOPv2 accept/refuse/outcome review",
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

test("Maestro interaction measurements stay bound to approved use-case budgets", async () => {
  const [evidence, maestro, demos] = await Promise.all([
    read("tests/maestro/interaction-budgets.json").then(JSON.parse),
    read("tests/maestro/inventory.json").then(JSON.parse),
    read("docs/factory/demo-inventory.json").then(JSON.parse),
  ]);
  assert.equal(evidence.schema, "oxid-maestro-interaction-budget-evidence-v1");
  assert.equal(evidence.countingRules, "docs/factory/demo-inventory.md#approved-product-journeys");

  const scenarios = new Map(maestro.scenarios.map((scenario) => [scenario.id, scenario]));
  const useCases = new Map(demos.useCases.map((useCase) => [useCase.id, useCase]));
  const measuredIds = new Set();
  const requiredUseCases = [
    "fresh-wallet-onboarding",
    "wallet-recovery",
    "profile-and-realm-switching",
    "automatic-account-reconciliation",
    "receive-and-fund-night",
    "send-night",
    "did-inventory-and-creation",
    "oid4vci-issuance",
    "oid4vp-presentation",
    "activity-and-transaction-detail",
    "security-and-backup-settings",
  ];
  const measuredUseCases = new Set();
  const fieldByKind = {
    "entry-tap": "entryTaps",
    "decision-screen": "decisionScreens",
    "authorization-prompt": "authorizationPrompts",
    "routine-manual-sync": "routineManualSyncActions",
  };
  const limitByObservedField = {
    entryTaps: "entryTapsMax",
    decisionScreens: "decisionScreensMax",
    authorizationPrompts: "authorizationPromptsMax",
    routineManualSyncActions: "routineManualSyncActionsMax",
  };

  for (const measurement of evidence.measurements) {
    assert.match(measurement.id, /^[a-z0-9]+(?:-[a-z0-9]+)*$/u);
    assert.ok(!measuredIds.has(measurement.id), `duplicate measurement: ${measurement.id}`);
    measuredIds.add(measurement.id);
    assert.ok(["ready", "blocked", "refusal", "recovery"].includes(measurement.state));

    const scenario = scenarios.get(measurement.scenarioId);
    assert.equal(scenario?.authority, "maestro", `${measurement.id} must reference a runnable Maestro scenario`);
    const source = await read(`tests/maestro/${scenario.flow}`);
    assert.equal(
      createHash("sha256").update(source).digest("hex"),
      measurement.flowSha256,
      `${measurement.id} is stale; remeasure the changed flow instead of accepting silent interaction drift`,
    );

    const useCase = useCases.get(measurement.useCaseId);
    assert.equal(useCase?.interactionBudget?.status, "active", `${measurement.id} requires an approved active budget`);
    measuredUseCases.add(measurement.useCaseId);
    const counted = Object.fromEntries(Object.values(fieldByKind).map((field) => [field, 0]));
    for (const action of measurement.countedActions) {
      const field = fieldByKind[action.kind];
      assert.ok(field, `${measurement.id} has unknown action kind: ${action.kind}`);
      assert.match(action.label, /\S/u);
      counted[field] += 1;
    }
    assert.deepEqual(measurement.observed, counted, `${measurement.id} counts must be explained action by action`);
    for (const [field, limitField] of Object.entries(limitByObservedField)) {
      assert.ok(
        measurement.observed[field] <= useCase.interactionBudget[limitField],
        `${measurement.id} exceeds ${measurement.useCaseId}.${limitField}`,
      );
    }
    assert.equal(
      measurement.observed.routineManualSyncActions,
      0,
      `${measurement.id} must not make routine synchronization a holder task`,
    );
  }

  for (const useCaseId of requiredUseCases) {
    assert.ok(measuredUseCases.has(useCaseId), `missing interaction measurement for ${useCaseId}`);
  }
});

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
      if (!["onboarding-safe-boundary", "onboarding-restore-boundary", "developer-profile-banner"].includes(scenario.id)) {
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
  const [ios, android, runner, largerWidthRunner, runScript] = await Promise.all([
    read("scripts/run-maestro-ios.sh"),
    read("scripts/run-maestro-android.sh"),
    read("scripts/test-ios-maestro-holder-evidence.sh"),
    read("scripts/test-ios-maestro-holder-evidence-large.sh"),
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
  assert.match(ios, /maestro-ios-lane\.mjs acquire/u);
  assert.match(ios, /--cleanup-stale-lane <owner-token>/u);
  assert.match(ios, /factory-metrics phase=maestro-ios-lane result=/u);
  assert.match(ios, /OXID_IOS_DEVICE must be an explicit simulator UDID/u);
  assert.match(ios, /--udid "\$OXID_IOS_DEVICE"/u);
  assert.match(android, /\^emulator-\[0-9\]\+\$/u);
  assert.match(android, /refusing non-emulator device/u);
  assert.match(android, /--device "\$OXID_ANDROID_DEVICE"/u);
  assert.match(android, /run_phase maestro nix run \.#maestro -- test "\$flow"/u);
  assert.doesNotMatch(android, /local maestro_pid=\$!|wait "\$maestro_pid"/u);
  for (const wrapper of [ios, android]) {
    assert.match(wrapper, /if \[ "\$status" -ne 0 \]; then[\s\S]*rm -rf -- "\$artifact_root"/u);
  }
  assert.match(runner, /id != "canonical-holder-evidence"/u);
  assert.match(runner, /scenarios\+=\(canonical-holder-evidence\)/u);
  assert.match(largerWidthRunner, /OXID_IOS_EVIDENCE_VIEWPORT="402-pt-class"/u);
  assert.match(largerWidthRunner, /OXID_IOS_EVIDENCE_DEVICE_TYPE="com\.apple\.CoreSimulator\.SimDeviceType\.iPhone-17-Pro"/u);
  assert.match(largerWidthRunner, /OXID_IOS_RUNTIME_ID="com\.apple\.CoreSimulator\.SimRuntime\.iOS-26-4"/u);
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
  const [matrix, checkpoint, inventory, iosRunner, androidRunner] = await Promise.all([
    read("docs/factory/mobile-visual-accessibility-evidence.md"),
    read("docs/factory/milestone-0.2.0-assistive-navigation-evidence.md"),
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
  assert.match(iosRunner, /oxid-ios-maestro-evidence-v3/u);
  assert.match(iosRunner, /simctl list runtimes -j/u);
  assert.match(iosRunner, /select\(\.identifier == \$runtime and \.isAvailable == true\)/u);
  assert.doesNotMatch(iosRunner, /\bmapfile\b/u);
  assert.match(iosRunner, /id != "canonical-holder-evidence"/u);
  assert.match(iosRunner, /scenarios\+=\(canonical-holder-evidence\)/u);
  assert.match(iosRunner, /scenario_outcomes/u);
  assert.match(iosRunner, /scenarios\/\$scenario\/screenshots/u);
  assert.match(iosRunner, /scenarios\/manifest\.jsonl/u);
  assert.match(iosRunner, /for command in jq nix node rustup shasum timeout/u);
  assert.match(iosRunner, /source_key="\$\(shasum -a 256 "\$source"/u);
  assert.doesNotMatch(iosRunner, /printf '%s' "\$source" \| shasum/u);
  assert.match(iosRunner, /while IFS= read -r source; do[\s\S]*mkdir -p "\$scenario_root\/screenshots"[\s\S]*done < <\(find/u);
  assert.match(iosRunner, /manifest:"scenarios\/manifest\.jsonl"/u);
  assert.match(iosRunner, /capturePolicy:\$capture_policy/u);
  assert.match(iosRunner, /designReference:\$design,uiProfile:\$uiProfile/u);
  assert.match(iosRunner, /OXID_IOS_EVIDENCE_VIEWPORT:-375-pt-class/u);
  assert.match(iosRunner, /OXID_IOS_EVIDENCE_DEVICE_TYPE:-com\.apple\.CoreSimulator\.SimDeviceType\.iPhone-SE-3rd-generation/u);
  assert.doesNotMatch(iosRunner, /boundedLog:"maestro-tail\.log"/u);
  assert.match(iosRunner, /collect_public_artifacts "\$scenario" "\$ui_profile"/u);
  assert.match(iosRunner, /if \[ -n "\$\{scenario:-\}" \] && \[ -n "\$\{ui_profile:-\}" \]; then[\s\S]*collect_public_artifacts "\$scenario" "\$ui_profile"/u);
  assert.match(iosRunner, /rm -rf -- "\$ROOT\/target\/mobile-visual-accessibility\/ios\/\$DEVICE"/u);
  assert.match(androidRunner, /oxid-android-maestro-semantic-evidence-v1/u);
  assert.match(androidRunner, /emulator-\*/u);
  assert.match(androidRunner, /OXID_ANDROID_DISPOSABLE/u);
  assert.doesNotMatch(androidRunner, /\bmapfile\b/u);
  assert.match(androidRunner, /rm -rf -- "\$artifact_root"/u);
  assert.match(androidRunner, /privateDiagnosticsRemoved:\$privateRemoved/u);
  assert.match(androidRunner, /"\$\{#scenarios\[@\]\}" -gt 0/u);
  const passportVault = await read("tests/maestro/flows/passport-vault-entry.yaml");
  assert.match(passportVault, /- swipe:\n    direction: LEFT\n- swipe:\n    direction: LEFT\n- tapOn: "Open Passport Vault"/u);
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
  assert.match(matrix, /milestone-0\.2\.0-assistive-navigation-evidence\.md/u);

  assert.match(checkpoint, /a000ec06aeffd5ea2a86fe3ad9180b07dc4e97bf/u);
  assert.match(checkpoint, /15\/15 Maestro scenarios/u);
  assert.match(checkpoint, /608 seconds/u);
  assert.match(checkpoint, /10 bounded public screenshots/u);
  assert.match(checkpoint, /5,711,481 public bytes/u);
  assert.match(checkpoint, /1\/1 test; 8\.608 seconds/u);
  assert.match(checkpoint, /receiptOwnedSimulator=true/u);
  assert.match(checkpoint, /privateDiagnosticsRemoved=true/u);
  assert.match(checkpoint, /rawArtifactsRemoved=true/u);
  assert.match(checkpoint, /not a claim that a human completed a VoiceOver or TalkBack\s+traversal/iu);
  assert.match(checkpoint, /200% text/u);
  assert.match(checkpoint, /receipt-owned larger-width run/u);
  assert.match(checkpoint, /receipt-owned launcher that proves emulator identity/iu);
  assert.match(checkpoint, /No physical or ambient Android target was used/u);
  assert.match(checkpoint, /Rust, XCTest, and\s+CDP tests remain authoritative/u);
});
