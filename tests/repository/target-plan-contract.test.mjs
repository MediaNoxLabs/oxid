// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import test from "node:test";

import {
  DeliveryProfile,
  HOSTED_TARGETS,
  HostedTarget,
  Profile,
  classifyAreas,
  makeTargetPlan,
  resolveProfile,
} from "../../scripts/ci/target-plan.mjs";

test("automatic profiles distinguish feature, milestone promotion, and release flows", () => {
  assert.equal(resolveProfile("auto", "pull_request", "develop"), Profile.FEATURE);
  assert.equal(resolveProfile("auto", "pull_request", "milestone-0.4.0", "", "feat/issue-1"), Profile.FEATURE);
  assert.equal(resolveProfile("auto", "pull_request", "develop", "", "milestone-0.4.0"), Profile.INTEGRATION);
  assert.equal(resolveProfile("auto", "pull_request", "develop", "", "milestone-latest"), Profile.FEATURE);
  assert.equal(resolveProfile("auto", "push", "develop"), Profile.INTEGRATION);
  assert.equal(resolveProfile("auto", "push", "", "milestone-0.4.0"), Profile.INTEGRATION);
  assert.equal(resolveProfile("auto", "pull_request", "main"), Profile.RELEASE);
  assert.equal(resolveProfile("auto", "push", "", "main"), Profile.RELEASE);
  assert.equal(resolveProfile(Profile.FEATURE, "push", "", "main"), Profile.FEATURE);
});

test("documentation, harness, and workflow-only feature changes keep the basic gate", () => {
  for (const paths of [
    ["README.md", "docs/factory/runbook.md"],
    ["scripts/docs/check-links.mjs", "scripts/docs/generate-adr-catalog.mjs"],
    [".devloops", "scripts/loop/pre-flight-gate.mjs"],
    ["scripts/git-hooks/local-policy.mjs"],
    ["scripts/check-pi-devshell.sh", "scripts/lib/dev-loop-runtime.mjs"],
    [".github/workflows/ci.yml", "scripts/ci/target-plan.mjs"],
    ["scripts/coverage/policy.json", "scripts/coverage/run.mjs"],
    ["docs/factory/metrics.md", "scripts/ci/target-plan.mjs"],
    ["scripts/factory/metrics.mjs", "docs/factory/work-item-metrics-v1.schema.json"],
  ]) {
    assert.deepEqual(makeTargetPlan(paths).targets, [HostedTarget.BASIC], paths.join(","));
  }
});

test("the repository gate driver remains a fail-closed global build input", () => {
  assert.deepEqual(makeTargetPlan(["run.sh"]).targets, Object.values(HostedTarget));
});

test("focused application changes select their component lanes", () => {
  assert.deepEqual(
    makeTargetPlan(["apps/oxid-headless/src/main.rs"]).targets,
    [
      HostedTarget.BASIC,
      HostedTarget.UNIT_LINUX,
      HostedTarget.HEADLESS_LINUX,
    ],
  );
  assert.deepEqual(
    makeTargetPlan(["crates/ui-dioxus/src/lib.rs"]).targets,
    [
      HostedTarget.BASIC,
      HostedTarget.UNIT_LINUX,
      HostedTarget.UI_LINUX,
    ],
  );
});

test("shared core uses the headless consumer on the feature PR critical path", () => {
  const plan = makeTargetPlan(["crates/foundation/src/lib.rs"]);
  assert.deepEqual(plan.areas, ["core"]);
  assert.deepEqual(plan.targets, [
    HostedTarget.BASIC,
    HostedTarget.UNIT_LINUX,
    HostedTarget.HEADLESS_LINUX,
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
  assert.equal(plan.targets.includes(HostedTarget.NIX_PACKAGE), false);
  assert.equal(plan.targets.includes(HostedTarget.COMPACT_ARTIFACTS), true);
  assert.equal(plan.targets.includes(HostedTarget.HEADLESS_LINUX), true);
  assert.equal(plan.targets.includes(HostedTarget.UI_LINUX), false);
  assert.equal(plan.targets.includes(HostedTarget.UI_RELEASE_LINUX), false);
});

test("expensive assurance lanes remain available explicitly on feature PRs", () => {
  const targets = [
    HostedTarget.COVERAGE_LINUX,
    HostedTarget.QUALITY,
    HostedTarget.UI_RELEASE_LINUX,
    HostedTarget.NIX_PACKAGE,
  ];
  const plan = makeTargetPlan(["crates/foundation/src/lib.rs"], { extraTargets: targets });
  for (const target of targets) assert.equal(plan.targets.includes(target), true, target);
});

test("integration and release profiles are complete backstops", () => {
  for (const profile of [Profile.INTEGRATION, Profile.RELEASE]) {
    assert.deepEqual(makeTargetPlan(["README.md"], { profile }).targets, HOSTED_TARGETS);
  }
});

test("prototype delivery stays basic until a focused target is requested", () => {
  for (const paths of [
    ["crates/foundation/src/lib.rs"],
    ["flake.lock"],
    [],
  ]) {
    const plan = makeTargetPlan(paths, { deliveryProfile: DeliveryProfile.PROTOTYPE });
    assert.equal(plan.deliveryProfile, DeliveryProfile.PROTOTYPE);
    assert.deepEqual(plan.targets, [HostedTarget.BASIC]);
  }

  const focused = makeTargetPlan(["apps/oxid-headless/src/main.rs"], {
    deliveryProfile: DeliveryProfile.PROTOTYPE,
    extraTargets: [HostedTarget.HEADLESS_LINUX],
  });
  assert.deepEqual(focused.targets, [HostedTarget.BASIC, HostedTarget.HEADLESS_LINUX]);
});

test("prototype delivery cannot masquerade as an integration or release run", () => {
  for (const profile of [Profile.INTEGRATION, Profile.RELEASE]) {
    assert.throws(
      () => makeTargetPlan(["README.md"], { deliveryProfile: DeliveryProfile.PROTOTYPE, profile }),
      /prototype delivery is local-only/u,
    );
  }
  assert.throws(
    () => makeTargetPlan(["README.md"], { deliveryProfile: "fast-ish" }),
    /unknown delivery profile/u,
  );
  assert.throws(
    () => makeTargetPlan(["README.md"], {
      deliveryProfile: DeliveryProfile.PROTOTYPE,
      extraTargets: [HostedTarget.COVERAGE_LINUX],
    }),
    /not available in prototype delivery/u,
  );
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
