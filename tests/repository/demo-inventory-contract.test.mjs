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

test("validator rejects broken references, unsafe operations, and invalid evidence contracts", () => {
  const valid = loadInventory();
  const cases = [
    ["unknown reference", (inventory) => { inventory.scenarios[0].targetPlans[0].targetId = "unknown-target"; }, /unknown target/u],
    ["duplicate id", (inventory) => { inventory.targets.push(clone(inventory.targets[0])); }, /duplicate id/u],
    ["unsafe absolute command", (inventory) => { inventory.commands[0].command = "/bin/sh"; }, /unsafe|absolute/u],
    ["destructive git command", (inventory) => { inventory.commands[0].command = "git clean -fdx"; }, /not a supported repository command/u],
    ["unsupported environment", (inventory) => { inventory.commands[0].environment = { HOME: "elsewhere" }; }, /unsupported HOME/u],
    ["missing mutable cleanup", (inventory) => { inventory.dependencies[0].cleanupCommandIds = []; }, /missing cleanup/u],
    ["invalid cadence", (inventory) => { inventory.scenarios[0].cadence = "daily"; }, /invalid cadence/u],
    ["invalid evidence", (inventory) => { inventory.scenarios[0].evidenceClass = "live"; }, /invalid evidence/u],
    ["invalid target evidence", (inventory) => { inventory.scenarios[0].targetPlans[0].evidenceClass = "live"; }, /invalid evidence class/u],
    ["unsupported default target", (inventory) => { inventory.scenarios[0].defaultTargetId = "ios-physical"; }, /default target must be supported/u],
    ["wrong command phase", (inventory) => { inventory.scenarios[0].targetPlans[0].commandIds.build = ["desktop-run"]; }, /from phase 'run'/u],
    ["missing test mapping", (inventory) => { delete inventory.scenarios[0].testMapping; }, /missing a test mapping/u],
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
