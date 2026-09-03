// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import test from "node:test";

import {
  discoverWorkspacePackageInventory,
  evaluateCoverage,
  normalizeLlvmReport,
  parseArguments,
  scoreChangedLines,
  validatePolicy,
} from "../../scripts/coverage/run.mjs";

const repoRoot = path.resolve(new URL("../..", import.meta.url).pathname);
const fixtureRoot = path.join(repoRoot, "tests/repository/fixtures/coverage");
const policyPath = path.join(repoRoot, "scripts/coverage/policy.json");

async function loadPolicy() {
  return JSON.parse(await readFile(policyPath, "utf8"));
}

function lineFile(packageEntry, lines = { count: 100, covered: 100 }) {
  return {
    path: `${packageEntry.root}/src/lib.rs`,
    package: packageEntry.name,
    lines: { ...lines },
    executableLines: [],
  };
}

function reportsFor(policy, inventory, overrides = {}) {
  const byName = new Map(inventory.map((entry) => [entry.name, entry]));
  const classified = (names) => names.map((name) => {
    const defaultLines = { count: 100, covered: 100 };
    return lineFile(byName.get(name), overrides[name] ?? defaultLines);
  });
  const workspaceNames = [
    ...policy.classifications.core,
    ...policy.classifications.critical,
    ...policy.classifications.workspaceOnly,
  ];
  const report = (scope, files) => ({
    schemaVersion: 2,
    scope,
    totals: {
      lines: files.reduce((total, file) => ({
        count: total.count + file.lines.count,
        covered: total.covered + file.lines.covered,
      }), { count: 0, covered: 0 }),
    },
    files,
  });
  return [
    report("workspace-aggregate", classified(workspaceNames)),
    ...policy.classifications.additionalScopes.map(({ package: name, scope }) => (
      report(scope, classified([name]))
    )),
  ];
}

function changedReport(inventory) {
  const packageFor = (name) => inventory.find((entry) => entry.name === name);
  return [{
    schemaVersion: 2,
    scope: "workspace-aggregate",
    totals: { lines: { count: 5, covered: 3 } },
    files: [
      {
        ...lineFile(packageFor("oxid-foundation"), { count: 2, covered: 1 }),
        executableLines: [{ line: 10, covered: true }, { line: 12, covered: false }],
      },
      {
        path: `${packageFor("oxid-identity-domain").root}/src/new.rs`,
        package: "oxid-identity-domain",
        lines: { count: 2, covered: 1 },
        executableLines: [{ line: 1, covered: true }, { line: 2, covered: false }],
      },
      {
        path: `${packageFor("oxid-wallet-domain").root}/src/renamed.rs`,
        package: "oxid-wallet-domain",
        lines: { count: 1, covered: 1 },
        executableLines: [{ line: 21, covered: true }],
      },
    ],
  }];
}

const reviewedException = {
  id: "issue-157-changed-lines-1",
  kind: "changed-lines",
  packages: ["oxid-foundation", "oxid-identity-domain", "oxid-wallet-domain"],
  owner: "coverage-working-group",
  issue: "#157",
  rationale: "A deterministic fixture proving the reviewed exception contract.",
  compensatingTest: "tests/repository/coverage-policy-contract.test.mjs",
  approval: "CODEOWNERS review required",
  expiry: "2026-12-01",
  removalCondition: "Remove when every executable changed line is covered.",
};

test("policy pins reviewed 85/90 floors, baseline placeholders, paths, diff semantics, and exceptions", async () => {
  const policy = await loadPolicy();
  const inventory = await discoverWorkspacePackageInventory(repoRoot);
  assert.doesNotThrow(() => validatePolicy(policy, inventory));
  assert.deepEqual(policy.packageFloorsPercent, { core: 85, critical: 90 });
  assert.equal(policy.baselines.mode, "placeholder");
  assert.deepEqual(policy.baselines.scopes.map(({ scope }) => scope), policy.scopes.map(({ id }) => id));
  assert.ok(policy.baselines.scopes.every(({ count, covered }) => count === null && covered === null));
  assert.deepEqual(policy.pathRules.production, ["apps/*/src/**/*.rs", "crates/*/src/**/*.rs"]);
  assert.deepEqual(policy.pathRules.excludedDirectories, ["tests", "examples", "benches"]);
  assert.deepEqual(policy.pathRules.nonExecutableSources, ["crates/composition/src/lib.rs"]);
  assert.deepEqual(policy.pathRules.testOnlySources, ["crates/ui-dioxus/src/desktop_test_driver.rs"]);
  assert.equal(policy.pathRules.testModuleFilename, "tests.rs");
  assert.deepEqual(policy.changedLines, {
    floorPercent: 90,
    diffArguments: ["--unified=0", "--diff-filter=AMCR"],
    range: "BASE...HEAD",
    zeroDenominator: "not-applicable",
  });
  assert.deepEqual(policy.comparisonBase, {
    resolveToCommit: true,
    requireAncestor: true,
    range: "BASE...HEAD",
  });
  assert.deepEqual(policy.exceptions, []);

  for (const mutation of [
    (copy) => { copy.packageFloorsPercent.core = 84; },
    (copy) => { copy.packageFloorsPercent.critical = 89; },
    (copy) => { copy.changedLines.floorPercent = 89; },
    (copy) => { copy.pathRules.production.push("tools/**/*.rs"); },
    (copy) => { copy.baselines.unreviewed = true; },
  ]) {
    const drifted = structuredClone(policy);
    mutation(drifted);
    assert.throws(() => validatePolicy(drifted, inventory), /floor|contract|unknown policy key/iu);
  }
});

test("per-package integer evaluation prevents aggregate masking and honors exact boundaries", async () => {
  const policy = await loadPolicy();
  const inventory = await discoverWorkspacePackageInventory(repoRoot);
  const reports = reportsFor(policy, inventory, {
    "oxid-foundation": { count: 20, covered: 17 },
    "oxid-platform-ports": { count: 100, covered: 84 },
    "oxid-wallet-application": { count: 20, covered: 18 },
    "oxid-identity-application": { count: 100, covered: 89 },
  });
  const evaluation = evaluateCoverage(policy, reports, { packageInventory: inventory, changedLines: null });
  assert.equal(evaluation.status, "fail");
  assert.equal(evaluation.aggregate.lines.covered * 100 >= evaluation.aggregate.lines.count * 80, true);
  const ledger = new Map(evaluation.packages.map((entry) => [entry.name, entry]));
  assert.equal(ledger.get("oxid-foundation").status, "pass");
  assert.equal(ledger.get("oxid-platform-ports").status, "fail");
  assert.equal(ledger.get("oxid-wallet-application").status, "pass");
  assert.equal(ledger.get("oxid-identity-application").status, "fail");
  assert.equal(ledger.get("oxid-identity-application").requiredCovered, 90);
  assert.deepEqual(evaluation.packages.map(({ name }) => name), evaluation.packages.map(({ name }) => name).toSorted());
});

test("package evidence rejects unmapped, duplicate, wrong-scope, and missing records", async () => {
  const policy = await loadPolicy();
  const inventory = await discoverWorkspacePackageInventory(repoRoot);
  const valid = reportsFor(policy, inventory);

  const missing = structuredClone(valid);
  missing[0].files = missing[0].files.filter(({ package: name }) => name !== "oxid-foundation");
  assert.throws(() => evaluateCoverage(policy, missing, { packageInventory: inventory }), /missing package evidence.*oxid-foundation/iu);

  const duplicate = structuredClone(valid);
  duplicate[0].files.push(structuredClone(duplicate[0].files[0]));
  assert.throws(() => evaluateCoverage(policy, duplicate, { packageInventory: inventory }), /duplicate.*source path/iu);

  const unmapped = structuredClone(valid);
  unmapped[0].files.push({ path: "tools/secret.rs", lines: { count: 1, covered: 1 }, executableLines: [] });
  assert.throws(() => evaluateCoverage(policy, unmapped, { packageInventory: inventory }), /unmapped.*package/iu);

  const wrongScope = structuredClone(valid);
  wrongScope[1].files.push(structuredClone(valid[0].files[0]));
  assert.throws(() => evaluateCoverage(policy, wrongScope, { packageInventory: inventory }), /duplicate|unexpected package evidence/iu);
});

test("LLVM segments normalize to explicit repository-relative executable line evidence", async () => {
  const policy = await loadPolicy();
  const inventory = await discoverWorkspacePackageInventory(repoRoot);
  const raw = JSON.parse(await readFile(path.join(fixtureRoot, "llvm-segments.json"), "utf8"));
  const normalized = normalizeLlvmReport(raw, {
    repoRoot,
    scopeId: "workspace-aggregate",
    packageInventory: inventory,
    workspaceFloorPercent: null,
    pathRules: policy.pathRules,
  });
  assert.deepEqual(normalized.files[0].executableLines, [
    { line: 10, covered: true },
    { line: 12, covered: false },
  ]);
  assert.equal(normalized.files[0].package, "oxid-foundation");
  assert.doesNotMatch(JSON.stringify(normalized), new RegExp(repoRoot.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&"), "u"));
});

test("changed-line scorer handles changes, new files, renames, comments, tests, generated files, and siblings", async () => {
  const policy = await loadPolicy();
  const inventory = await discoverWorkspacePackageInventory(repoRoot);
  const diff = await readFile(path.join(fixtureRoot, "changed-production.diff"), "utf8");
  const score = scoreChangedLines(diff, changedReport(inventory), { policy, packageInventory: inventory });
  assert.deepEqual(score.lines, { count: 5, covered: 3, requiredCovered: 5 });
  assert.equal(score.status, "fail");
  assert.deepEqual(score.files.filter(({ status }) => status === "excluded").map(({ reason }) => reason).toSorted(), [
    "generated", "sibling-test", "test-directory",
  ]);
  assert.equal(score.files.find(({ path: name }) => name.endsWith("lib.rs")).lines.count, 2);
  assert.equal(score.files.find(({ path: name }) => name.endsWith("new.rs")).change, "added");
  assert.equal(score.files.find(({ path: name }) => name.endsWith("renamed.rs")).change, "renamed");
});

test("changed-line scorer parses an added line beginning with two plus signs", async () => {
  const policy = await loadPolicy();
  const inventory = await discoverWorkspacePackageInventory(repoRoot);
  const foundation = inventory.find(({ name }) => name === "oxid-foundation");
  const diff = "diff --git a/crates/foundation/src/lib.rs b/crates/foundation/src/lib.rs\n--- a/crates/foundation/src/lib.rs\n+++ b/crates/foundation/src/lib.rs\n@@ -0,0 +1 @@\n+++\n";
  const reports = [{ files: [{
    ...lineFile(foundation, { count: 1, covered: 1 }),
    executableLines: [{ line: 1, covered: true }],
  }] }];
  assert.deepEqual(scoreChangedLines(diff, reports, { policy, packageInventory: inventory }).lines,
    { count: 1, covered: 1, requiredCovered: 1 });
});

test("conventional Rust tests.rs modules are excluded from production changed lines", async () => {
  const policy = await loadPolicy();
  const inventory = await discoverWorkspacePackageInventory(repoRoot);
  const sourcePath = "crates/composition/src/environment/tests.rs";
  const diff = `diff --git a/${sourcePath} b/${sourcePath}\n--- /dev/null\n+++ b/${sourcePath}\n@@ -0,0 +1 @@\n+fn test_only() {}\n`;
  const score = scoreChangedLines(diff, [], { policy, packageInventory: inventory });
  assert.deepEqual(score.files, [{
    path: sourcePath,
    change: "modified",
    status: "excluded",
    reason: "test-module",
  }]);
  assert.equal(score.status, "not-applicable");
});

test("reviewed declaration-only Rust facades are excluded from executable changed lines", async () => {
  const policy = await loadPolicy();
  const inventory = await discoverWorkspacePackageInventory(repoRoot);
  const sourcePath = "crates/composition/src/lib.rs";
  const diff = `diff --git a/${sourcePath} b/${sourcePath}\n--- a/${sourcePath}\n+++ b/${sourcePath}\n@@ -0,0 +1 @@\n+pub use environment::AppEnvironment;\n`;
  const score = scoreChangedLines(diff, [], { policy, packageInventory: inventory });
  assert.deepEqual(score.files, [{
    path: sourcePath,
    change: "modified",
    status: "excluded",
    reason: "non-executable-source",
  }]);
  assert.equal(score.status, "not-applicable");
});

test("reviewed runtime test drivers are excluded from production changed lines", async () => {
  const policy = await loadPolicy();
  const inventory = await discoverWorkspacePackageInventory(repoRoot);
  const sourcePath = "crates/ui-dioxus/src/desktop_test_driver.rs";
  const diff = `diff --git a/${sourcePath} b/${sourcePath}\n--- /dev/null\n+++ b/${sourcePath}\n@@ -0,0 +1 @@\n+fn drive_test() {}\n`;
  const score = scoreChangedLines(diff, [], { policy, packageInventory: inventory });
  assert.deepEqual(score.files, [{
    path: sourcePath,
    change: "modified",
    status: "excluded",
    reason: "test-only-source",
  }]);
  assert.equal(score.status, "not-applicable");
});

test("changed-line exact boundary passes and zero executable denominator is visibly not-applicable", async () => {
  const policy = await loadPolicy();
  const inventory = await discoverWorkspacePackageInventory(repoRoot);
  const foundation = inventory.find(({ name }) => name === "oxid-foundation");
  const diff = "diff --git a/crates/foundation/src/lib.rs b/crates/foundation/src/lib.rs\n--- a/crates/foundation/src/lib.rs\n+++ b/crates/foundation/src/lib.rs\n@@ -0,0 +1,10 @@\n" + "+line\n".repeat(10);
  const files = [{
    path: `${foundation.root}/src/lib.rs`, package: foundation.name,
    lines: { count: 10, covered: 9 },
    executableLines: Array.from({ length: 10 }, (_, index) => ({ line: index + 1, covered: index < 9 })),
  }];
  const reports = [{ schemaVersion: 2, scope: "workspace-aggregate", totals: { lines: { count: 10, covered: 9 } }, files }];
  assert.equal(scoreChangedLines(diff, reports, { policy, packageInventory: inventory }).status, "pass");

  const commentsOnly = structuredClone(reports);
  commentsOnly[0].files[0].executableLines = [];
  const score = scoreChangedLines(diff, commentsOnly, { policy, packageInventory: inventory });
  assert.deepEqual(score.lines, { count: 0, covered: 0, requiredCovered: null });
  assert.equal(score.status, "not-applicable");
  assert.notEqual(score.percent, 100);
});

test("changed-line scorer honors only the reviewed plain-test package exclusion", async () => {
  const policy = await loadPolicy();
  const inventory = await discoverWorkspacePackageInventory(repoRoot);
  const app = inventory.find(({ name }) => name === "oxid-app");
  const appPath = `${app.root}/src/main.rs`;
  const diff = `diff --git a/${appPath} b/${appPath}\n--- a/${appPath}\n+++ b/${appPath}\n@@ -0,0 +1 @@\n+fn main() {}\n`;

  const excluded = scoreChangedLines(diff, changedReport(inventory), {
    policy,
    packageInventory: inventory,
  });
  assert.equal(excluded.status, "not-applicable");
  assert.deepEqual(excluded.files, [{
    path: appPath,
    package: "oxid-app",
    change: "modified",
    status: "excluded",
    reason: "plain-test-only",
  }]);

  const mapped = [{ files: [{
    ...lineFile(app, { count: 1, covered: 1 }),
    path: appPath,
    executableLines: [{ line: 1, covered: true }],
  }] }];
  assert.equal(scoreChangedLines(diff, mapped, {
    policy,
    packageInventory: inventory,
  }).status, "pass");
});

test("changed-line scorer fails closed for malformed diffs and missing or ambiguous mappings", async () => {
  const policy = await loadPolicy();
  const inventory = await discoverWorkspacePackageInventory(repoRoot);
  assert.throws(() => scoreChangedLines("@@ -0,0 +1 @@\n+x\n", [], { policy, packageInventory: inventory }), /malformed git diff/iu);
  const missing = "diff --git a/crates/foundation/src/missing.rs b/crates/foundation/src/missing.rs\n--- /dev/null\n+++ b/crates/foundation/src/missing.rs\n@@ -0,0 +1 @@\n+x\n";
  assert.throws(() => scoreChangedLines(missing, changedReport(inventory), { policy, packageInventory: inventory }), /missing coverage mapping/iu);
  const secondMissing = "diff --git a/crates/foundation/src/also-missing.rs b/crates/foundation/src/also-missing.rs\n--- /dev/null\n+++ b/crates/foundation/src/also-missing.rs\n@@ -0,0 +1 @@\n+y\n";
  assert.throws(
    () => scoreChangedLines(`${missing}${secondMissing}`, changedReport(inventory), { policy, packageInventory: inventory }),
    /missing.*also-missing\.rs.*missing\.rs|missing.*missing\.rs.*also-missing\.rs/iu,
  );
  const ambiguous = [...inventory, { name: "duplicate-foundation", root: "crates/foundation" }];
  assert.throws(() => scoreChangedLines(missing, changedReport(inventory), { policy, packageInventory: ambiguous }), /ambiguous package mapping/iu);
});

test("reviewed exceptions are closed, expire fail-closed, and never bypass absolute floors", async () => {
  const policy = await loadPolicy();
  const inventory = await discoverWorkspacePackageInventory(repoRoot);
  const diff = await readFile(path.join(fixtureRoot, "changed-production.diff"), "utf8");
  policy.exceptions = [reviewedException];
  assert.doesNotThrow(() => validatePolicy(policy, inventory, { now: new Date("2026-09-01T00:00:00Z") }));
  const reports = reportsFor(policy, inventory, { "oxid-foundation": { count: 100, covered: 84 } });
  const changedLines = scoreChangedLines(diff, changedReport(inventory), {
    policy, packageInventory: inventory, now: new Date("2026-09-01T00:00:00Z"),
  });
  assert.equal(changedLines.status, "excepted");
  const evaluation = evaluateCoverage(policy, reports, { packageInventory: inventory, changedLines });
  assert.equal(evaluation.status, "fail");
  assert.equal(evaluation.packages.find(({ name }) => name === "oxid-foundation").status, "fail");

  const expired = structuredClone(policy);
  expired.exceptions[0].expiry = "2026-08-31";
  assert.throws(() => validatePolicy(expired, inventory, { now: new Date("2026-09-01T00:00:00Z") }), /expired/iu);
  const malformed = structuredClone(policy);
  malformed.exceptions[0].floorBypass = true;
  assert.throws(() => validatePolicy(malformed, inventory), /unknown policy key/iu);
  const absolute = structuredClone(policy);
  absolute.exceptions[0].kind = "absolute-floor";
  assert.throws(() => validatePolicy(absolute, inventory), /absolute|kind/iu);
});

test("CLI defaults to measurement and accepts --enforce exactly once", () => {
  assert.deepEqual(parseArguments(["--base", "origin/develop"]), {
    base: "origin/develop", dryRun: false, enforce: false, policyPath: undefined,
  });
  assert.deepEqual(parseArguments(["--base", "origin/develop", "--enforce"]), {
    base: "origin/develop", dryRun: false, enforce: true, policyPath: undefined,
  });
  assert.throws(() => parseArguments(["--base", "HEAD", "--enforce", "--enforce"]), /more than once/iu);
});
