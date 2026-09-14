// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");

test("Android profile automation follows native-authorized recovery onboarding", async () => {
  const flow = await readFile(
    path.join(root, "tests", "mobile", "android-wallet-flow.mjs"),
    "utf8",
  );
  const start = flow.indexOf("async function createFreshProfile()");
  const end = flow.indexOf("\nasync function assertHomeComposition", start);
  const onboarding = flow.slice(start, end);

  for (const label of [
    "Create private wallet",
    "Generate recovery phrase",
    "New wallet recovery phrase",
    "I have securely saved or verified this recovery phrase.",
    "Finish and open wallet",
  ]) {
    assert.match(onboarding, new RegExp(label.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")));
  }
  assert.doesNotMatch(onboarding, /Create and continue|Skip for now/);
  assert.ok(
    onboarding.indexOf('clickButton("Generate recovery phrase")') <
      onboarding.indexOf("native-authorized recovery phrase"),
  );
  assert.ok(
    onboarding.indexOf("native-authorized recovery phrase") <
      onboarding.indexOf("I have securely saved or verified this recovery phrase."),
  );
});

test("Android smoke owns only disposable-emulator credential and app state", async () => {
  const smoke = await readFile(
    path.join(root, "scripts", "test-android-profile-flow.sh"),
    "utf8",
  );

  assert.match(smoke, /OXID_ANDROID_REQUIRE_EMULATOR=1/);
  assert.match(smoke, /case "\$device" in\n  emulator-\*\)/);
  assert.match(smoke, /getprop ro\.kernel\.qemu/);
  assert.match(smoke, /locksettings get-disabled/);
  assert.match(smoke, /refusing to replace it/);
  assert.match(smoke, /test_pin="\$\(od -An -N4 -tu4 \/dev\/urandom/);
  assert.doesNotMatch(smoke, /246810/);
  assert.match(smoke, /credential_owned=0/);
  assert.match(smoke, /if \[ "\$credential_owned" -eq 1 \]/);
  assert.match(smoke, /locksettings clear --old "\$test_pin"/);
  assert.match(smoke, /if \[ "\$app_state_owned" -eq 1 \]/);
  assert.match(smoke, /shell pm clear io\.medianox\.oxid/);
  assert.match(smoke, /authorize_onboarding_prompt &/);
  assert.doesNotMatch(smoke, /passed on \$device|device \$device|device '\$device'/);
  assert.doesNotMatch(smoke, /recovery phrase.*echo|echo.*recovery phrase/i);
});

test("prebuilt Android smoke is explicit, exact-source, and digest-bound", async () => {
  const [launcher, smoke, justfile, guide, runScript] = await Promise.all([
    readFile(path.join(root, "scripts", "run-android-emulator.sh"), "utf8"),
    readFile(path.join(root, "scripts", "test-android-profile-flow.sh"), "utf8"),
    readFile(path.join(root, "Justfile"), "utf8"),
    readFile(path.join(root, "docs", "factory", "application-targets.md"), "utf8"),
    readFile(path.join(root, "run.sh"), "utf8"),
  ]);

  assert.match(justfile, /^android-smoke-prebuilt .*:\n    \.\/scripts\/test-android-profile-flow\.sh --apk/m);
  assert.match(smoke, /--apk APK --receipt RECEIPT/);
  assert.match(smoke, /run-android-emulator\.sh" deploy/);
  assert.match(launcher, /A prebuilt Android artifact is admitted only by the deploy operation/);
  assert.match(launcher, /OXID_ANDROID_PREBUILT_APK must name an absolute regular non-symlink file/);
  assert.match(launcher, /The prebuilt Android receipt must be a mode-0600 private file/);
  assert.match(launcher, /\.source\.head == \$head/);
  assert.match(launcher, /\.source\.tree == \$tree/);
  assert.match(launcher, /\.artifact\.sha256 == \$sha256/);
  assert.match(launcher, /\.apk\.package == "io\.medianox\.oxid"/);
  assert.match(launcher, /apk_activity.*dev\.dioxus\.main\.MainActivity/s);
  assert.match(launcher, /resolved_activity=.*resolve-activity/s);
  assert.match(launcher, /sha256=\$actual_apk_sha256/);
  assert.match(guide, /just android-smoke-prebuilt/);
  assert.match(guide, /never prints that credential or\n+the device identifier/);

  const registration = "node --test tests/mobile/android-profile-smoke-contract.test.mjs";
  assert.equal(runScript.split(registration).length - 1, 1);
});
