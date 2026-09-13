// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import test from "node:test";

import {
  DeliveryProfile,
  HOSTED_TARGETS,
  HostedTarget,
  Profile,
  classifyOwnershipMapChanges,
  classifyAreas,
  makeTargetPlan,
  resolveProfile,
} from "../../scripts/ci/target-plan.mjs";

const ownershipMap = (crates, schemaVersion = 1) => JSON.stringify({ schemaVersion, crates });

const crate = (name, capabilityOwners = []) => ({
  name,
  sourceRoot: `crates/${name}/src`,
  facadeFiles: [`crates/${name}/src/lib.rs`],
  facadeMaximumPhysicalLines: 1,
  facadeMaximumPhysicalLinesByPath: { [`crates/${name}/src/lib.rs`]: 1 },
  capabilityOwners: capabilityOwners.map((owner) => ({
    name: owner.name,
    modulePathPrefixes: owner.modulePathPrefixes ?? [`crates/${name}/src/${owner.name}`],
  })),
  exclusions: [],
  temporaryExceptions: [],
});

test("ownership-map changes inherit the affected crate's CI lane", () => {
  const before = ownershipMap([
    crate("oxid-ui-dioxus", [{ name: "labels" }]),
    crate("oxid-composition", [{ name: "profiles" }]),
  ]);
  const uiOnly = ownershipMap([
    crate("oxid-ui-dioxus", [{ name: "labels" }, { name: "presentation" }]),
    crate("oxid-composition", [{ name: "profiles" }]),
  ]);
  const sharedCore = ownershipMap([
    crate("oxid-ui-dioxus", [{ name: "labels" }]),
    crate("oxid-composition", [{ name: "profiles" }, { name: "wiring" }]),
  ]);

  assert.deepEqual(classifyOwnershipMapChanges(before, uiOnly), ["ui"]);
  assert.deepEqual(classifyOwnershipMapChanges(before, sharedCore), ["core"]);
  assert.deepEqual(
    makeTargetPlan([
      "crates/ui-dioxus/src/presentation.rs",
      "docs/site/src/presentation.md",
      "scripts/architecture/capability-facades.json",
    ], { ownershipAreas: classifyOwnershipMapChanges(before, uiOnly) }).targets,
    [HostedTarget.BASIC, HostedTarget.UNIT_LINUX, HostedTarget.UI_LINUX],
  );
});

test("ownership-map changes fail closed for malformed or mixed ownership", () => {
  const before = ownershipMap([crate("oxid-ui-dioxus")]);
  const mixed = ownershipMap([crate("oxid-ui-dioxus", [{ name: "labels" }]), crate("oxid-composition")]);

  assert.deepEqual(classifyOwnershipMapChanges(before, "not JSON"), ["core"]);
  assert.deepEqual(classifyOwnershipMapChanges(before, ownershipMap([crate("oxid-ui-dioxus")], 2)), ["core"]);
  const invalidOwner = JSON.parse(before);
  invalidOwner.crates[0].capabilityOwners = [{ name: "presentation" }];
  assert.deepEqual(classifyOwnershipMapChanges(before, JSON.stringify(invalidOwner)), ["core"]);
  assert.deepEqual(classifyOwnershipMapChanges(before, mixed), ["core", "ui"]);
  assert.deepEqual(
    makeTargetPlan(["scripts/architecture/capability-facades.json"], {
      ownershipAreas: classifyOwnershipMapChanges(before, mixed),
    }).targets,
    [HostedTarget.BASIC, HostedTarget.UNIT_LINUX, HostedTarget.HEADLESS_LINUX, HostedTarget.UI_LINUX],
  );
});

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
    ["scripts/lib/managed-child-process.mjs"],
    [".github/workflows/ci.yml", "scripts/ci/target-plan.mjs"],
    ["scripts/coverage/policy.json", "scripts/coverage/run.mjs"],
    ["docs/factory/metrics.md", "scripts/ci/target-plan.mjs"],
    ["scripts/factory/metrics.mjs", "docs/factory/work-item-metrics-v1.schema.json"],
  ]) {
    assert.deepEqual(makeTargetPlan(paths).targets, [HostedTarget.BASIC], paths.join(","));
  }
});

test("scanner policy paths retain the bounded policy lane", () => {
  for (const paths of [
    [".github/workflows/scan.yml"],
    [".gitleaksignore"],
    [".gitleaks.toml"],
    [".github/workflows/scan.yml", ".gitleaksignore", ".gitleaks.toml"],
  ]) {
    assert.deepEqual(makeTargetPlan(paths).targets, [HostedTarget.BASIC], paths.join(","));
  }
});

test("scan workflow remains independently required for every pull request", async () => {
  const workflow = await readFile(new URL("../../.github/workflows/scan.yml", import.meta.url), "utf8");
  assert.match(workflow, /pull_request:/);
  assert.match(workflow, /name: scan/);
});

test("scanner policy changes retain conservative product and unknown-root combinations", () => {
  assert.deepEqual(
    makeTargetPlan([".gitleaksignore", "crates/foundation/src/lib.rs"]).targets,
    [HostedTarget.BASIC, HostedTarget.UNIT_LINUX, HostedTarget.HEADLESS_LINUX],
  );
  assert.deepEqual(
    makeTargetPlan([".gitleaks.toml", "unknown-root-file"]).targets,
    [HostedTarget.BASIC, HostedTarget.UNIT_LINUX, HostedTarget.HEADLESS_LINUX],
  );
});

test("unclassified scripts/lib helpers remain conservative", () => {
  assert.deepEqual(
    makeTargetPlan(["scripts/lib/unclassified-helper.mjs"]).targets,
    [HostedTarget.BASIC, HostedTarget.UNIT_LINUX, HostedTarget.HEADLESS_LINUX],
  );
});

test("root bootstrap and repository-contract changes retain only the Basic gate", () => {
  assert.deepEqual(makeTargetPlan(["bootstrap.sh"]).targets, [HostedTarget.BASIC]);
  assert.deepEqual(
    makeTargetPlan(["bootstrap.sh", "tests/repository/target-plan-contract.test.mjs"]).targets,
    [HostedTarget.BASIC],
  );
});

test("root bootstrap preserves Rust/product target selection", () => {
  assert.deepEqual(
    makeTargetPlan(["bootstrap.sh", "crates/foundation/src/lib.rs"]).targets,
    [HostedTarget.BASIC, HostedTarget.UNIT_LINUX, HostedTarget.HEADLESS_LINUX],
  );
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

test("draft PR state defers selected lanes until the PR is ready", () => {
  const changedPaths = ["crates/ui-dioxus/src/lib.rs"];
  const normalTargets = [HostedTarget.BASIC, HostedTarget.UNIT_LINUX, HostedTarget.UI_LINUX];

  assert.deepEqual(
    makeTargetPlan(changedPaths, { eventName: "pull_request", pullRequestDraft: true }).targets,
    [HostedTarget.BASIC],
  );
  assert.deepEqual(
    makeTargetPlan(changedPaths, { eventName: "pull_request", pullRequestDraft: false }).targets,
    normalTargets,
  );
});

test("manual dispatch escalates draft-independent hosted targets", () => {
  const plan = makeTargetPlan(["README.md"], {
    eventName: "workflow_dispatch",
    pullRequestDraft: true,
    extraTargets: [HostedTarget.UI_RELEASE_LINUX],
  });
  assert.deepEqual(plan.targets, [HostedTarget.BASIC, HostedTarget.UI_RELEASE_LINUX]);
});

test("CI supplies PR draft state for every draft transition", async () => {
  const workflow = await readFile(new URL("../../.github/workflows/ci.yml", import.meta.url), "utf8");
  assert.match(workflow, /types: \[opened, synchronize, reopened, ready_for_review, converted_to_draft\]/);
  assert.match(workflow, /PR_DRAFT: \$\{\{ github\.event\.pull_request\.draft \|\| 'false' \}\}/);
  assert.equal((workflow.match(/--pr-draft "\$PR_DRAFT"/g) ?? []).length, 2);
});

test("integration and release profiles are complete durable-branch backstops", () => {
  for (const profile of [Profile.INTEGRATION, Profile.RELEASE]) {
    assert.deepEqual(makeTargetPlan(["README.md"], { profile, eventName: "push", pullRequestDraft: true }).targets, HOSTED_TARGETS);
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
