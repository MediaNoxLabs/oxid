// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFile } from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { loadInventory, renderPreparationBrief, validateInventory } from "../../scripts/demo-inventory.mjs";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const cli = path.join(repoRoot, "scripts/demo-inventory.mjs");
const clone = (value) => JSON.parse(JSON.stringify(value));

test("inventory validates the recovery native-presence scenario and renders only data", () => {
  const inventory = loadInventory();
  const scenario = inventory.scenarios.find(({ id }) => id === "wallet-root-recovery-native-presence");
  assert.equal(scenario.evidenceClass, "acceptance");
  assert.equal(scenario.cadence, "before-release");
  assert.equal(scenario.defaultTargetId, "android-physical");
  assert.deepEqual(scenario.orderedUseCaseIds, [
    "recover-existing-midnight-wallet-root",
    "reject-stale-or-duplicate-root-recovery",
  ]);
  const brief = renderPreparationBrief(inventory, scenario.id);
  assert.match(brief, /Target: android-physical \(supported; acceptance\)/u);
  assert.match(brief, /OXID_MOBILE_CUSTODY=native just android-run/u);
  assert.match(brief, /active AGENT\.md define execution authority/u);
  const unsupported = renderPreparationBrief(inventory, scenario.id, "ios-physical");
  assert.match(unsupported, /Target: ios-physical \(unsupported; planned\)/u);
  assert.match(unsupported, /physical iOS signing and deployment are unavailable/i);
});

test("inventory keeps standalone asset synchronization diagnostic and route-scoped", () => {
  const inventory = loadInventory();
  const scenario = inventory.scenarios.find(({ id }) => id === "standalone-profile-asset-synchronization");
  assert.equal(scenario.evidenceClass, "diagnostic");
  assert.equal(scenario.cadence, "on-demand");
  assert.deepEqual(scenario.orderedUseCaseIds, [
    "select-compile-time-standalone-profile",
    "synchronize-profile-scoped-midnight-assets",
  ]);
  const local = renderPreparationBrief(inventory, scenario.id, "android-emulator");
  assert.match(local, /Target: android-emulator \(supported; diagnostic\)/u);
  assert.match(local, /just android-standalone-local-smoke/u);
  assert.match(local, /Loopback transport is not Tailnet routing or a public network/u);
  const tailnet = renderPreparationBrief(inventory, scenario.id, "android-physical");
  assert.match(tailnet, /Target: android-physical \(supported; diagnostic\)/u);
  assert.match(tailnet, /just android-phone/u);
  assert.match(tailnet, /not production or public-network acceptance/i);
  assert.doesNotMatch(tailnet, /stable public NIGHT, DUST, or shielded balances/i);
});

test("inventory splits a bounded low-k proof from headless and rendered local diagnostics", async () => {
  const inventory = loadInventory();
  const proof = inventory.scenarios.find(({ id }) => id === "development-proof-benchmark-desktop");
  assert.equal(proof.evidenceClass, "diagnostic");
  assert.equal(proof.defaultTargetId, "desktop-development");
  assert.deepEqual(proof.orderedUseCaseIds, ["bound-development-proof-benchmark"]);
  const proofBrief = renderPreparationBrief(inventory, proof.id);
  assert.match(proofBrief, /Target: desktop-development \(supported; diagnostic\)/u);
  assert.match(proofBrief, /just desktop-proof-benchmark-run/u);
  assert.match(proofBrief, /Circuit k=1/u);
  assert.match(proofBrief, /no reviewed public sampler exists/u);
  assert.match(proofBrief, /not a performance budget/u);
  assert.doesNotMatch(proofBrief, /Target: android-emulator/u);

  const headless = inventory.scenarios.find(({ id }) => id === "bounded-local-diagnostics-headless");
  assert.equal(headless.evidenceClass, "preflight");
  assert.equal(headless.defaultTargetId, "headless-development");
  const headlessBrief = renderPreparationBrief(inventory, headless.id);
  assert.match(headlessBrief, /just headless/u);
  assert.match(headlessBrief, /CLEAR_LOCAL_DIAGNOSTICS/u);
  assert.match(headlessBrief, /payloadsRetained is false/u);

  const desktop = inventory.scenarios.find(({ id }) => id === "bounded-local-diagnostics-desktop");
  assert.equal(desktop.evidenceClass, "diagnostic");
  assert.equal(desktop.defaultTargetId, "desktop-development");
  const desktopBrief = renderPreparationBrief(inventory, desktop.id);
  assert.match(desktopBrief, /Warning and Error filters/u);
  assert.match(desktopBrief, /durable support journal/u);
  const demo = inventory.demos.find(({ id }) => id === "developer-proof-and-local-diagnostics");
  assert.deepEqual(demo.targetIds, ["headless-development", "desktop-development"]);
  assert.deepEqual(demo.scenarioIds, [
    "development-proof-benchmark-desktop",
    "bounded-local-diagnostics-headless",
    "bounded-local-diagnostics-desktop",
  ]);
  const justfile = await readFile(path.join(repoRoot, "Justfile"), "utf8");
  assert.match(justfile, /desktop-proof-benchmark-build:\n\s+cargo build -p oxid-app --no-default-features --features desktop,developer-proof-benchmark/u);
  assert.match(justfile, /desktop-proof-benchmark-run:\n\s+cargo run -p oxid-app --no-default-features --features desktop,developer-proof-benchmark/u);
  const documentation = await readFile(path.join(repoRoot, "docs/factory/demo-inventory.md"), "utf8");
  assert.match(documentation, /runs exactly one operator-selected k=1 proof/u);
  assert.match(documentation, /no reviewed\s+public sampler exists/u);
  assert.match(documentation, /durable support journal/u);
});

test("inventory composes holder-DID bootstrap into the existing physical diagnostic lane", () => {
  const inventory = loadInventory();
  const scenario = inventory.scenarios.find(({ id }) => id === "portal-final-issuance-physical-tailnet");
  assert.equal(scenario.evidenceClass, "diagnostic");
  assert.deepEqual(scenario.orderedUseCaseIds, [
    "bootstrap-managed-holder-did-for-test-issuer",
    "issue-and-protect-portal-digital-passport",
  ]);
  const brief = renderPreparationBrief(inventory, scenario.id);
  assert.match(brief, /Target: android-physical \(supported; diagnostic\)/u);
  assert.match(brief, /just android-portal-tailnet-physical-smoke/u);
  assert.match(brief, /development diagnostic/i);
  assert.match(scenario.testMapping.planned, /in-memory/i);
  assert.match(scenario.testMapping.planned, /resolve, sign, update, or deactivate/i);
});

test("inventory keeps Portal Final issuance evidence target-scoped", () => {
  const inventory = loadInventory();
  const useCase = inventory.useCases.find(({ id }) => id === "issue-and-protect-portal-digital-passport");
  assert.deepEqual(useCase.scenarioIds, [
    "portal-final-issuance-localhost",
    "portal-final-issuance-virtual-mobile",
    "portal-final-issuance-physical-tailnet",
  ]);
  const localhost = renderPreparationBrief(inventory, "portal-final-issuance-localhost", "headless-development");
  assert.match(localhost, /Target: headless-development \(supported; preflight\)/u);
  assert.match(localhost, /just portal-headless-e2e/u);
  assert.match(localhost, /not a rendered UI, device, production, release, node, or proof-server claim/u);
  const virtual = renderPreparationBrief(inventory, "portal-final-issuance-virtual-mobile", "android-emulator");
  assert.match(virtual, /Target: android-emulator \(supported; diagnostic\)/u);
  assert.match(virtual, /just android-portal-exact-sequence-avd/u);
  assert.match(virtual, /cannot substitute for physical Android acceptance/u);
  const physical = renderPreparationBrief(inventory, "portal-final-issuance-physical-tailnet");
  assert.match(physical, /Target: android-physical \(supported; diagnostic\)/u);
  assert.match(physical, /just android-portal-tailnet-physical-smoke/u);
  assert.match(physical, /not production, release, native-custody, live-KYC, or public-network acceptance/u);
  assert.match(physical, /restores its exact prior Serve baseline/u);
});

test("validator rejects broken references, unsafe operations, and invalid evidence contracts", () => {
  const valid = loadInventory();
  const cases = [
    ["unknown reference", (inventory) => { inventory.scenarios[0].targetPlans[0].targetId = "unknown-target"; }, /unknown target/u],
    ["duplicate id", (inventory) => { inventory.targets.push(clone(inventory.targets[0])); }, /duplicate id/u],
    ["unsafe absolute command", (inventory) => { inventory.commands[0].command = "/bin/sh"; }, /unsafe|absolute/u],
    ["destructive git command", (inventory) => { inventory.commands[0].command = "git clean -fdx"; }, /not a supported repository command/u],
    ["unsupported environment", (inventory) => { inventory.commands[0].environment = { HOME: "elsewhere" }; }, /unsupported HOME|additional property/u],
    ["missing mutable cleanup", (inventory) => { inventory.dependencies[0].cleanupCommandIds = []; }, /missing cleanup/u],
    ["invalid cadence", (inventory) => { inventory.scenarios[0].cadence = "daily"; }, /invalid cadence|schema enum/u],
    ["invalid evidence", (inventory) => { inventory.scenarios[0].evidenceClass = "live"; }, /invalid evidence|schema enum/u],
    ["invalid target evidence", (inventory) => { inventory.scenarios[0].targetPlans[0].evidenceClass = "live"; }, /invalid evidence class|schema enum/u],
    ["unsupported default target", (inventory) => { inventory.scenarios[0].defaultTargetId = "ios-physical"; }, /default target must be supported/u],
    ["wrong command phase", (inventory) => { inventory.scenarios[0].targetPlans[0].commandIds.build = ["desktop-run"]; }, /from phase 'run'/u],
    ["missing test mapping", (inventory) => { delete inventory.scenarios[0].testMapping; }, /missing a test mapping|schema required property 'testMapping'/u],
    ["unknown schema property", (inventory) => { inventory.products[0].unpublished = true; }, /schema.*additional property|additional property.*schema/u],
    ["invalid command oneOf", (inventory) => { inventory.commands[0].status = "manual"; }, /schema.*oneOf|oneOf.*schema/u],
  ];
  for (const [name, mutate, error] of cases) {
    const inventory = clone(valid); mutate(inventory);
    assert.throws(() => validateInventory(inventory), error, name);
  }
});

test("CLI exposes check, list, show, use-case, and preparation without a command executor", () => {
  const run = (...args) => execFileSync(process.execPath, [cli, ...args], { cwd: repoRoot, encoding: "utf8" });
  assert.match(run("check"), /PASS/u);
  assert.match(run("list"), /^wallet-root-recovery-native-presence\tacceptance\tbefore-release/mu);
  assert.match(run("show", "wallet-root-recovery-native-presence"), /"manualSteps"/u);
  assert.match(run("use-case", "show", "recover-existing-midnight-wallet-root"), /use-case:/u);
  assert.match(run("prepare", "wallet-root-recovery-native-presence"), /active AGENT\.md define execution authority/u);
});

test("Pi scenario and use-case commands invoke only the validator and preserve authority boundaries", async () => {
  const [extension, agent, charter, loop, strategy, demo, runner] = await Promise.all([
    readFile(path.join(repoRoot, ".pi/extensions/scenario.ts"), "utf8"),
    readFile(path.join(repoRoot, ".pi/agents/product-manager.agent.md"), "utf8"),
    readFile(path.join(repoRoot, "docs/factory/charter.md"), "utf8"),
    readFile(path.join(repoRoot, "docs/factory/productive-loop.md"), "utf8"),
    readFile(path.join(repoRoot, "docs/site/src/testing-strategy.md"), "utf8"),
    readFile(path.join(repoRoot, "demo/README.md"), "utf8"),
    readFile(path.join(repoRoot, "run.sh"), "utf8"),
  ]);
  assert.match(extension, /registerCommand\("scenario"/u);
  assert.match(extension, /registerCommand\("use-case"/u);
  assert.match(extension, /scripts\/demo-inventory\.mjs/u);
  assert.match(extension, /sendUserMessage/u);
  assert.match(extension, /Prepare the selected target now/u);
  assert.match(extension, /\[target-id\]/u);
  assert.doesNotMatch(extension, /exec\("(?:sh|bash)"/u);
  assert.match(agent, /Never execute operational command entries/u);
  assert.match(charter, /### Product Manager/u);
  assert.match(loop, /demo-inventory\.json/u);
  assert.match(strategy, /demo inventory/u);
  assert.match(demo, /demo-inventory/u);
  assert.match(runner, /demo-inventory-contract\.test\.mjs/u);
  const schema = JSON.parse(await readFile(path.join(repoRoot, "docs/factory/demo-inventory.schema.json"), "utf8"));
  assert.equal(schema.additionalProperties, false);
  assert.equal(schema.$defs.scenario.additionalProperties, false);
});
