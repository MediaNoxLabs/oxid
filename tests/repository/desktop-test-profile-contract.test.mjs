// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const root = new URL("../../", import.meta.url);
const text = (relative) => readFile(new URL(relative, root), "utf8");

test("ARM64 Darwin desktop Portal remains owner-invoked and outside HostedTarget", async () => {
  const [justfile, planner, harness] = await Promise.all([
    text("Justfile"),
    text("scripts/ci/target-plan.mjs"),
    text("scripts/e2e/portal-desktop-e2e.sh"),
  ]);
  assert.match(justfile, /portal-desktop-e2e:\n\s+\.\/scripts\/e2e\/portal-desktop-e2e\.sh/);
  assert.doesNotMatch(planner, /darwin|macos|desktop-portal/i);
  assert.match(harness, /target\/debug\/oxid-app/);
  assert.match(harness, /Mach-O 64-bit arm64/);
  assert.match(harness, /CGWindowListCopyWindowInfo/);
  assert.match(harness, /kCGWindowOwnerPID/);
  assert.doesNotMatch(harness, /kCGWindowLayer/);
  assert.match(harness, /"\$x" =~ \^-\?\[0-9\]\+\$ && "\$y" =~ \^-\?\[0-9\]\+\$/);
  assert.match(harness, /\/Applications\/Xcode\.app\/Contents\/Developer/);
  assert.match(harness, /\/usr\/bin\/xcrun --sdk macosx swiftc/);
  assert.doesNotMatch(harness, /System Events|osascript|xcode-select -p/);
  assert.doesNotMatch(harness, /Xvfb|Openbox|xdotool|WebKitGTK|x86_64-linux/i);
  assert.match(harness, /\.state == "empty"/);
  assert.doesNotMatch(harness, /\.state == "consumed"/);
  assert.match(harness, /rm -f -- "\$CONTROL_ROOT\/driver-admitted"/);
});

test("canonical macOS laptop lane runs headless before desktop and validates both exact-head records", async () => {
  const [justfile, runner] = await Promise.all([text("Justfile"), text("run.sh")]);
  const match = justfile.match(/^portal-macos-laptop-e2e:\n((?:    .*\n)+)/m);
  assert.ok(match, "missing no-argument portal-macos-laptop-e2e recipe");

  const recipe = match[1];
  const headless = [...recipe.matchAll(/just portal-headless-e2e/g)];
  const desktop = [...recipe.matchAll(/just portal-desktop-e2e/g)];
  assert.equal(headless.length, 1);
  assert.equal(desktop.length, 1);
  assert.ok(headless[0].index < desktop[0].index, "headless must precede desktop");
  const aggregateStart = recipe.indexOf("jq -s -e");
  assert.ok(desktop[0].index < aggregateStart, "both harnesses must precede aggregation");
  const aggregate = recipe.slice(aggregateStart, recipe.indexOf("\n    echo"));
  assert.equal([...aggregate.matchAll(/\bjq /g)].length, 1);
  assert.match(aggregate, /--arg head "\$\(git rev-parse HEAD\)"/);
  assert.match(aggregate, /--arg tree "\$\(git rev-parse 'HEAD\^\{tree\}'\)"/);
  assert.match(aggregate, /length == 2 and all\(\.\[\]; \.oxid == \{head:\$head,tree:\$tree\}\)/);
  assert.match(aggregate, /target\/portal-headless-e2e\/evidence\.json/);
  assert.match(aggregate, /target\/portal-desktop-e2e\/evidence\.json/);
  assert.match(recipe, /portal-macos-laptop-e2e: PASS evidence=target\/portal-headless-e2e\/evidence\.json,target\/portal-desktop-e2e\/evidence\.json/);

  const registration = "node --test tests/repository/desktop-test-profile-contract.test.mjs";
  assert.equal(runner.split(registration).length - 1, 1, "desktop contract must be registered exactly once");
  assert.match(runner.match(/run_repository\(\) \{([\s\S]*?)\n\}/)?.[1] ?? "", new RegExp(registration.replaceAll(".", "\\.")));
});

test("macOS runbook fails closed before inferring standalone ownership", async () => {
  const runbook = await text("docs/factory/portal-macos-laptop.md");
  const commands = runbook.match(/Before starting,[\s\S]*?```bash\n([\s\S]*?)\n```/)?.[1];
  assert.ok(commands, "missing owner-safe command block");

  const failClosedPrefix = [
    "mkdir -p tmp/portal-macos-laptop",
    'if ! standalone_before="$(docker ps -a \\',
    "  --filter label=com.docker.compose.project=oxid-standalone \\",
    "  --format '{{.ID}}' 2>/dev/null)\"; then",
    "  printf '%s\\n' 'standalone ownership query failed; no ownership recorded and no stack command run' >&2",
    "  exit 1",
    "fi",
  ].join("\n");
  assert.equal(commands.slice(0, failClosedPrefix.length), failClosedPrefix, "ownership query must use the exact fail-closed guard");

  const guardEnd = commands.indexOf("\nfi\n") + "\nfi".length;
  const inference = commands.indexOf("standalone_preexisting=");
  assert.ok(guardEnd > 0 && guardEnd < inference, "ownership inference must follow the successful query guard");
  assert.doesNotMatch(commands.slice(0, guardEnd), /ownership\.txt|standalone-(?:up|down)/);
});

test("desktop test feature is exact and its rendered-control driver has no direct capability calls", async () => {
  const [appManifest, compositionManifest, ingressManifest, driver, ui, main] = await Promise.all([
    text("apps/oxid/Cargo.toml"),
    text("crates/composition/Cargo.toml"),
    text("crates/adapters/identity-ingress/Cargo.toml"),
    text("crates/ui-dioxus/src/desktop_test_driver.rs"),
    text("crates/ui-dioxus/src/lib.rs"),
    text("apps/oxid/src/main.rs"),
  ]);
  assert.match(appManifest, /desktop-portal-test = \[[\s\S]*"desktop"[\s\S]*"standalone-development"[\s\S]*"oxid-composition\/desktop-portal-test"[\s\S]*"oxid-ui-dioxus\/desktop-test-click-driver"[\s\S]*\]/);
  assert.match(compositionManifest, /desktop-portal-test = \["oxid-adapter-identity-ingress\/desktop-test-qr-scanner"\]/);
  assert.match(ingressManifest, /desktop-test-qr-scanner = \["dep:zeroize"\]/);
  assert.match(main, /OXID_DESKTOP_PORTAL_TEST_PROFILE/);
  assert.match(ui, /desktop_test_driver::use_desktop_test_driver\(\);/);
  assert.match(driver, /pub\(super\) fn use_desktop_test_driver\(\)/);
  assert.match(driver, /\.click\(\)/);
  assert.match(driver, /dioxus_document::eval/);
  assert.doesNotMatch(driver, /qr_scanner|route_identity_request|UseCase|\.execute\(/);
});

test("normal release gate excludes every desktop-test marker and localhost route", async () => {
  const release = await text("scripts/check-ui-profile-release.sh");
  assert.match(release, /OXID_DESKTOP_PORTAL_TEST_PROFILE/);
  assert.match(release, /desktop-portal-test compiled outside ARM64 macOS/);
  assert.match(release, /portal-offer\\\.capability/);
});
