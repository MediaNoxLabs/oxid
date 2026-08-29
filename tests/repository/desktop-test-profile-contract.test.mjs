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
  assert.doesNotMatch(harness, /Xvfb|Openbox|xdotool|WebKitGTK|x86_64-linux/i);
});

test("desktop test feature is exact and its rendered-control driver has no direct capability calls", async () => {
  const [appManifest, compositionManifest, ingressManifest, driver, main] = await Promise.all([
    text("apps/oxid/Cargo.toml"),
    text("crates/composition/Cargo.toml"),
    text("crates/adapters/identity-ingress/Cargo.toml"),
    text("crates/ui-dioxus/src/desktop_test_driver.rs"),
    text("apps/oxid/src/main.rs"),
  ]);
  assert.match(appManifest, /desktop-portal-test = \[[\s\S]*"desktop"[\s\S]*"standalone-development"[\s\S]*"oxid-composition\/desktop-portal-test"[\s\S]*"oxid-ui-dioxus\/desktop-test-click-driver"[\s\S]*\]/);
  assert.match(compositionManifest, /desktop-portal-test = \["oxid-adapter-identity-ingress\/desktop-test-qr-scanner"\]/);
  assert.match(ingressManifest, /desktop-test-qr-scanner = \["dep:zeroize"\]/);
  assert.match(main, /OXID_DESKTOP_PORTAL_TEST_PROFILE/);
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
