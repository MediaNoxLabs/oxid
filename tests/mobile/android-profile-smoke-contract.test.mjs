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
    "Create and continue",
    "Generate recovery phrase",
    "New wallet recovery phrase",
    "I have securely saved or verified this recovery phrase.",
    "Finish and open wallet",
  ]) {
    assert.match(onboarding, new RegExp(label.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")));
  }
  assert.doesNotMatch(onboarding, /Skip for now/);
  assert.ok(
    onboarding.indexOf('clickButton("Create and continue")') <
      onboarding.indexOf('clickButton("Generate recovery phrase")'),
  );
  assert.ok(
    onboarding.indexOf('clickButton("Generate recovery phrase")') <
      onboarding.indexOf("native-authorized recovery phrase"),
  );
  assert.ok(
    onboarding.indexOf("native-authorized recovery phrase") <
      onboarding.indexOf("I have securely saved or verified this recovery phrase."),
  );
});

test("Android privacy automation uses the current global application menu", async () => {
  const flow = await readFile(
    path.join(root, "tests", "mobile", "android-wallet-flow.mjs"),
    "utf8",
  );
  const start = flow.indexOf('mode === "privacy-reveal"');
  const end = flow.indexOf('mode === "backup-export"', start);
  const privacy = flow.slice(start, end);

  assert.match(privacy, /await openWallet\(\)/);
  assert.match(privacy, /Open global application menu/);
  assert.match(privacy, /clickGlobalAction\("Session privacy"\)/);
  assert.doesNotMatch(privacy, /Show private values for 30 seconds|Hide private values/);
});

test("Android Home automation follows the realm-scoped product composition", async () => {
  const flow = await readFile(
    path.join(root, "tests", "mobile", "android-wallet-flow.mjs"),
    "utf8",
  );
  const start = flow.indexOf("async function assertHomeComposition()");
  const end = flow.indexOf("\nasync function setInput", start);
  const home = flow.slice(start, end);

  assert.match(home, /\.home-hero.*Current realm/s);
  assert.match(home, /\.home-quick-actions/);
  assert.match(home, /button\.home-card--assets\[aria-label\^="Open Wallet for "\]/);
  for (const label of [
    "Open newest document",
    "Open Passport Vault",
    "Open wallet security settings",
    "See all activity",
  ]) {
    assert.match(home, new RegExp(label));
  }
  assert.doesNotMatch(
    flow,
    /Everything in one place|Open Wallet NIGHT account|Open Wallet shielded account/,
  );
  assert.match(flow, /settled protected Receive state/);
  assert.match(flow, /receiveNeedsActivation/);
  assert.match(flow, /protected Receive state failed closed/);
  assert.doesNotMatch(home, /Use my receive address/);
  assert.match(flow, /1 protected notes/);
  assert.doesNotMatch(flow, /1 shielded notes/);
  assert.match(flow, /clickButton\("Create a DID"\).*clickButton\("Create DID"\)/s);
  assert.match(flow, /clickButton\("Open DID details"\)/);
  assert.match(flow, /Run demo action: Review login request/);
  assert.doesNotMatch(flow, /Use standalone login request/);
  assert.doesNotMatch(flow, /standalone-[12]/);
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
  assert.match(smoke, /resume_onboarding_after_authorization/);
  assert.match(
    smoke,
    /resume_onboarding_after_authorization\(\).*shell am start -W.*io\.medianox\.oxid\/dev\.dioxus\.main\.MainActivity/s,
  );
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
