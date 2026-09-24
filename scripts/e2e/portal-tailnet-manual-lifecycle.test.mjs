// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { access, readFile } from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const lifecyclePath = path.join(root, "scripts", "test-android-portal-tailnet-physical.sh");
const consumerLifecyclePath = path.join(root, "scripts", "portal-consumer-lifecycle.sh");
const pinnedPackageClosure = path.join(root, ".pi", "npm", "node_modules", "dev-loops", "package.json");

async function pathExists(file) {
  try {
    await access(file);
    return true;
  } catch {
    return false;
  }
}

test("manual Tailnet Portal lifecycle is a bounded, receipt-supervised owner demo", async () => {
  const [lifecycle, consumerLifecycle, justfile] = await Promise.all([
    readFile(lifecyclePath, "utf8"),
    readFile(consumerLifecyclePath, "utf8"),
    readFile(path.join(root, "Justfile"), "utf8"),
  ]);

  for (const recipe of [
    "portal-tailnet-manual-prepare:",
    "portal-tailnet-manual-prepared-status:",
    "portal-tailnet-manual-start:",
    "portal-tailnet-manual-status:",
    "portal-tailnet-manual-reset:",
    "portal-tailnet-manual-stop:",
  ]) assert.match(justfile, new RegExp(`^${recipe}`, "m"));

  for (const operation of ["manual-prepare", "manual-prepared-status", "manual-start", "manual-status", "manual-reset", "manual-stop"]) {
    assert.match(lifecycle, new RegExp(operation));
  }
  assert.doesNotMatch(lifecycle, /--manual-supervise/);
  assert.doesNotMatch(lifecycle, /nohup bash .*manual-supervise/);
  assert.match(lifecycle, /supervisor_pid="\$\$"/);
  assert.match(lifecycle, /manual_ready \|\| fail manual-not-ready/);
  assert.match(lifecycle, /sleep 2\n  manual_ready \|\| fail manual-readiness-unstable/);
  assert.match(lifecycle, /manual_public_page_ready/);
  assert.match(lifecycle, /deviceDataMode == "preserved"/);
  assert.match(lifecycle, /deviceDataMode:"preserved"/);
  assert.match(lifecycle, /if \[ "\$OPERATION" = automated \]; then\n  adb_device shell pm clear io\.medianox\.oxid/);
  assert.doesNotMatch(lifecycle, /adb_reverse_before=.*\nadb_device shell pm clear io\.medianox\.oxid/);
  assert.match(lifecycle, /manual-session-active/);
  assert.match(lifecycle, /RESET package=io\.medianox\.oxid scope=application-data/);
  assert.match(lifecycle, /target\/portal-tailnet-manual\/runtime/);
  assert.match(lifecycle, /target\/portal-tailnet-manual\/prepared/);
  assert.match(lifecycle, /prepared-receipt\.json/);
  assert.match(lifecycle, /manual_prepared_status/);
  assert.match(lifecycle, /fail artifacts-not-prepared/);
  assert.match(lifecycle, /PORTAL_CONSUMER_PREPARED_RECEIPT="\$prepared_receipt_for_support"/);
  assert.match(consumerLifecycle, /\[\.images\[\]\.durationSeconds\] \| add \/\/ 0/);
  assert.match(consumerLifecycle, /fail preparation-busy/);
  assert.match(consumerLifecycle, /oxid-portal-consumer-starting-v1/);
  assert.match(consumerLifecycle, /starting_receipt_valid/);
  assert.match(consumerLifecycle, /stale-starting-receipt/);
  assert.match(consumerLifecycle, /receipt_valid \|\| starting_receipt_valid \|\| fail ownership/);
  assert.match(consumerLifecycle, /docker pull "\$SMOCKER_IMAGE"/);
  assert.match(consumerLifecycle, /docker image inspect "\$SMOCKER_IMAGE"/);
  assert.match(consumerLifecycle, /--out-link "\$gc_root"/);
  assert.match(consumerLifecycle, /current_digest="sha256:\$\(shasum -a 256 "\$output"/);
  assert.match(consumerLifecycle, /gc_root" = "\$prepared_directory\/nix-\$key"/);
  assert.match(lifecycle, /portal_source_valid \|\| fail source-dirty/);
  assert.match(lifecycle, /servicesSeconds/);
  assert.match(lifecycle, /tailnetSeconds/);
  assert.match(lifecycle, /androidSeconds/);
  assert.match(lifecycle, /readySeconds/);
  assert.match(lifecycle, /manual-public-page-url/);
  assert.match(lifecycle, /readonly MOCK_STATE="\$STATE\/mock-state"/);
  assert.match(lifecycle, /tailnet-mock-transform\.mjs/);
  assert.match(lifecycle, /tailnet-mock-route\.mjs/);
  assert.match(lifecycle, /--create "\$SOURCE" "\$MOCK_STATE" "\$public_origin"/);
  assert.match(lifecycle, /--validate "\$MOCK_STATE" "\$manual_public_origin"/);
  assert.match(lifecycle, /PORTAL_TAILNET_MOCK_STATE_DIR="\$MOCK_STATE"/);
  assert.match(lifecycle, /--config "\$public_origin" "\$listener"/);
  assert.match(lifecycle, /\$mock_route\.route/);
  assert.match(lifecycle, /manual-mock-page\.html/);
  assert.match(lifecycle, /mockRoute:true/);
  assert.match(lifecycle, /holderBootstrap:true/);
  assert.match(lifecycle, /path:"\/holder"/);
  assert.match(lifecycle, /portal-holder\.capability/);
  assert.match(lifecycle, /chmod 600 \"\$MANUAL_PAGE_URL\"/);
  assert.match(lifecycle, /open \"\$public_page_url\"/);
  assert.match(lifecycle, /manual_control_receipt=none/);
  assert.match(lifecycle, /OXID_PORTAL_MOBILE_CONTROL_RECEIPT="\$manual_control_receipt"/);
  assert.match(lifecycle, /tailscale-https-profile\.sh" cleanup/);
  assert.match(lifecycle, /\[ "\$after_cleanup" = "\$baseline" \]/);
  assert.match(lifecycle, /portal-consumer-lifecycle\.sh/);
  assert.match(lifecycle, /OXID_MOBILE_PORTAL_PROFILE=tailnet-android/);
  assert.match(lifecycle, /manual_status/);
  assert.match(lifecycle, /manual_mock_state_valid/);
  assert.match(lifecycle, /manual_cleanup/);
  assert.doesNotMatch(lifecycle, /manual.*evidence/i);
});

test("manual lifecycle is included in repository contracts exactly once", async () => {
  const runner = await readFile(path.join(root, "run.sh"), "utf8");
  const registration = "node --test scripts/e2e/portal-tailnet-manual-lifecycle.test.mjs";
  assert.equal(runner.split(registration).length - 1, 1);
});

test("Portal preparation is a saved, static-first Taskflow with one explicit long-build boundary", async () => {
  const flowPath = path.join(root, ".pi", "taskflows", "flows", "demos", "portal-tailnet-prepare.json");
  const [flowSource, staticAdapter, stepAdapter, settings, gitignore, inventorySource] = await Promise.all([
    readFile(flowPath, "utf8"),
    readFile(path.join(root, "scripts", "factory", "taskflow-static.mjs"), "utf8"),
    readFile(path.join(root, "scripts", "factory", "portal-tailnet-taskflow-step.sh"), "utf8"),
    readFile(path.join(root, ".pi", "settings.json"), "utf8"),
    readFile(path.join(root, ".gitignore"), "utf8"),
    readFile(path.join(root, "docs", "factory", "demo-inventory.json"), "utf8"),
  ]);
  const flow = JSON.parse(flowSource);
  const piSettings = JSON.parse(settings);
  const inventory = JSON.parse(inventorySource);

  assert.equal(flow.name, "portal-tailnet-prepare");
  assert.equal(flow.scriptCwd, "invocation");
  assert.equal(flow.strictInterpolation, true);
  assert.equal(flow.incremental, false);
  assert.equal(flow.concurrency, 1);
  assert.deepEqual(flow.args.mode.values, ["prepare-only"]);
  assert.deepEqual(flow.phases.map(({ id }) => id), [
    "preflight",
    "prepare-artifacts",
    "verify-prepared-artifacts",
    "handoff",
  ]);
  assert.deepEqual(flow.phases.map(({ type }) => type), ["script", "approval", "script", "script"]);
  assert.deepEqual(flow.phases[1].dependsOn, ["preflight"]);
  assert.deepEqual(flow.phases[2].dependsOn, ["prepare-artifacts"]);
  assert.deepEqual(flow.phases[3].dependsOn, ["verify-prepared-artifacts"]);
  assert.equal(flow.phases[3].final, true);
  assert.equal(flow.phases[1].idempotent, undefined);
  assert.match(flow.phases[1].task, /just portal-tailnet-manual-prepare/);
  assert.match(flow.phases[1].task, /five-minute script limit/);
  for (const phase of flow.phases.filter(({ type }) => type === "script")) {
    assert.ok(Array.isArray(phase.run), `${phase.id} must use argv execution`);
    assert.ok(phase.timeout <= 300_000, `${phase.id} exceeds the pinned runtime script limit`);
    assert.equal(phase.cache.scope, "off");
  }
  assert.equal(flow.phases.some(({ type }) => ["agent", "gate", "map", "reduce"].includes(type)), false);

  assert.match(staticAdapter, /EXPECTED_TASKFLOW_VERSION = "0\.2\.10"/);
  assert.match(staticAdapter, /resolveDevLoopsPackageRoot/);
  assert.match(staticAdapter, /preflightTaskflow/);
  assert.match(staticAdapter, /compileTaskflow/);
  assert.match(staticAdapter, /validateTaskflow/);
  assert.doesNotMatch(staticAdapter, /executeTaskflow|runTaskflow/);
  assert.match(stepAdapter, /device=not-required tailnet=not-required/);
  assert.doesNotMatch(stepAdapter, /manual-prepared-status/);

  const scenario = inventory.scenarios.find(({ id }) => id === "portal-final-issuance-physical-tailnet");
  assert.ok(scenario, "physical Tailnet Portal scenario must remain inventoried");
  const target = scenario.targetPlans.find(({ targetId }) => targetId === "android-physical");
  assert.ok(target?.dependencyIds.includes("portal-tailnet-consumer-harness"));
  assert.ok(target?.commandIds.run.includes("portal-android-tailnet-diagnostic"));
  const runCommand = inventory.commands.find(({ id }) => id === "portal-android-tailnet-diagnostic");
  assert.equal(runCommand?.reference, "docs/factory/portal-android-tailnet-physical.md");

  const taskflowPackage = piSettings.packages.find((entry) =>
    typeof entry === "object" && entry.source === "npm:pi-taskflow@0.2.10");
  assert.deepEqual(taskflowPackage.extensions, []);
  assert.deepEqual(taskflowPackage.skills, []);
  assert.match(gitignore, /!\/\.pi\/taskflows\/flows\/demos\/\*\*/);
});

test("installed pinned Taskflow runtime verifies, plans, and compiles the saved flow", {
  skip: !(await pathExists(pinnedPackageClosure)) && "pinned local Pi package closure is not installed",
}, () => {
  const flowPath = path.join(root, ".pi", "taskflows", "flows", "demos", "portal-tailnet-prepare.json");
  for (const action of ["verify", "plan", "compile"]) {
    const result = spawnSync(process.execPath, [
      path.join(root, "scripts", "factory", "taskflow-static.mjs"),
      action,
      path.relative(root, flowPath),
      JSON.stringify({ mode: "prepare-only" }),
    ], { cwd: root, encoding: "utf8" });
    assert.equal(result.status, 0, `${action} failed:\n${result.stdout}\n${result.stderr}`);
  }
});
