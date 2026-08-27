// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import test from "node:test";

import {
  HOSTED_TARGETS,
  HostedTarget,
  Profile,
  classifyAreas,
  makeTargetPlan,
} from "../../scripts/ci/target-plan.mjs";

test("documentation, harness, and workflow-only feature changes keep the basic gate", () => {
  for (const paths of [
    ["README.md", "docs/factory/runbook.md"],
    [".devloops", "scripts/loop/pre-flight-gate.mjs"],
    [".github/workflows/ci.yml", "scripts/ci/target-plan.mjs"],
    ["docs/factory/metrics.md", "scripts/ci/target-plan.mjs"],
  ]) {
    assert.deepEqual(makeTargetPlan(paths).targets, [HostedTarget.BASIC], paths.join(","));
  }
});

test("focused application changes select their component lanes", () => {
  assert.deepEqual(
    makeTargetPlan(["apps/oxid-headless/src/main.rs"]).targets,
    [
      HostedTarget.BASIC,
      HostedTarget.UNIT_LINUX,
      HostedTarget.HEADLESS_LINUX,
      HostedTarget.COVERAGE_LINUX,
      HostedTarget.QUALITY,
    ],
  );
  assert.deepEqual(
    makeTargetPlan(["crates/ui-dioxus/src/lib.rs"]).targets,
    [
      HostedTarget.BASIC,
      HostedTarget.UNIT_LINUX,
      HostedTarget.UI_LINUX,
      HostedTarget.UI_RELEASE_LINUX,
      HostedTarget.COVERAGE_LINUX,
      HostedTarget.QUALITY,
    ],
  );
});

test("shared core fans out to both public host consumers", () => {
  const plan = makeTargetPlan(["crates/foundation/src/lib.rs"]);
  assert.deepEqual(plan.areas, ["core"]);
  assert.deepEqual(plan.targets, [
    HostedTarget.BASIC,
    HostedTarget.UNIT_LINUX,
    HostedTarget.HEADLESS_LINUX,
    HostedTarget.UI_LINUX,
    HostedTarget.UI_RELEASE_LINUX,
    HostedTarget.COVERAGE_LINUX,
    HostedTarget.QUALITY,
  ]);
});

test("build inputs and unavailable diffs fail closed to all hosted targets", () => {
  assert.deepEqual(makeTargetPlan(["flake.lock"]).targets, HOSTED_TARGETS);
  const unknownDiff = makeTargetPlan([]);
  assert.deepEqual(unknownDiff.targets, HOSTED_TARGETS);
  assert.equal(unknownDiff.rustChanged, true);
});

test("Compact changes include artifacts and their host consumers", () => {
  const plan = makeTargetPlan(["contracts/presentation/src/presentation.compact"]);
  assert.equal(plan.areas.includes("compact"), true);
  assert.equal(plan.targets.includes(HostedTarget.NIX_PACKAGE), true);
  assert.equal(plan.targets.includes(HostedTarget.COMPACT_ARTIFACTS), true);
  assert.equal(plan.targets.includes(HostedTarget.HEADLESS_LINUX), true);
  assert.equal(plan.targets.includes(HostedTarget.UI_LINUX), true);
  assert.equal(plan.targets.includes(HostedTarget.UI_RELEASE_LINUX), true);
});

test("integration and release profiles are complete backstops", () => {
  for (const profile of [Profile.INTEGRATION, Profile.RELEASE]) {
    assert.deepEqual(makeTargetPlan(["README.md"], { profile }).targets, HOSTED_TARGETS);
  }
});

test("known on-demand targets can be added and unknown targets are rejected", () => {
  const plan = makeTargetPlan(["README.md"], { extraTargets: [HostedTarget.HEADLESS_LINUX] });
  assert.deepEqual(plan.targets, [HostedTarget.BASIC, HostedTarget.HEADLESS_LINUX]);
  assert.throws(() => makeTargetPlan(["README.md"], { extraTargets: ["preprod-live"] }), /unknown hosted CI target/);
});

test("an unknown path is owned by core instead of silently skipping validation", () => {
  assert.deepEqual(classifyAreas(["new-surface/config.custom"]), ["core"]);
});

test("the headless lane owns every integration target without repeating unit tests", async () => {
  const [runScript, entries] = await Promise.all([
    readFile(new URL("../../run.sh", import.meta.url), "utf8"),
    readdir(new URL("../../apps/oxid-headless/tests", import.meta.url)),
  ]);
  assert.doesNotMatch(runScript, /cargo test -p oxid-headless --tests/);
  for (const entry of entries.filter((candidate) => candidate.endsWith(".rs"))) {
    assert.match(runScript, new RegExp(`--test ${entry.replace(/\.rs$/, "")}`), entry);
  }
});

test("unit and UI commands have non-overlapping native test ownership", async () => {
  const runScript = await readFile(new URL("../../run.sh", import.meta.url), "utf8");
  const unitBlock = runScript.slice(runScript.indexOf("run_unit()"), runScript.indexOf("run_core()"));
  const uiBlock = runScript.slice(runScript.indexOf("run_ui()"), runScript.indexOf("run_headless()"));
  assert.match(unitBlock, /cargo test --workspace/);
  assert.match(unitBlock, /--exclude oxid-ui-dioxus/);
  assert.match(unitBlock, /--exclude oxid-app/);
  assert.doesNotMatch(unitBlock, /cargo test -p oxid-(?:ui-dioxus|app)/);
  assert.match(uiBlock, /cargo test -p oxid-ui-dioxus --features ui-profile-demo,app-profile-authority/);
  assert.match(uiBlock, /cargo test -p oxid-app/);
});

test("UI profile guards and optimized release evidence are independently runnable", async () => {
  const [runScript, releaseScript] = await Promise.all([
    readFile(new URL("../../run.sh", import.meta.url), "utf8"),
    readFile(new URL("../../scripts/check-ui-profile-release.sh", import.meta.url), "utf8"),
  ]);
  assert.match(runScript, /check-ui-profile-release\.sh --guards/);
  assert.match(runScript, /run_ui_release\(\)[\s\S]*check-ui-profile-release\.sh --artifact/);
  assert.match(releaseScript, /all\|--guards\|--artifact/);
});
