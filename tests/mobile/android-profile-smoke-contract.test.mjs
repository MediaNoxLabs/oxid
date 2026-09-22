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

  const credentialHelper = await readFile(
    path.join(root, "scripts", "lib", "android-test-credential.sh"),
    "utf8",
  );
  assert.match(
    credentialHelper,
    /device-credential prompt observed[\s\S]*sleep 1[\s\S]*input text[\s\S]*keyevent ENTER[\s\S]*seq 1 50/,
  );
  assert.match(credentialHelper, /authorization_count=\$\(\(authorization_count \+ 1\)\)/);
  assert.match(credentialHelper, /seq 1 1000/);
  assert.match(credentialHelper, /oxid_android_test_credential_completion_file/);
  assert.match(credentialHelper, /OXID_ANDROID_ONBOARDING_COMPLETE_FILE/);
  assert.match(credentialHelper, /bounded onboarding completion/);
  assert.match(credentialHelper, /device-credential prompt remained open after PIN submission/);
  assert.match(onboarding, /writeFile\(completionFile, "complete\\n"/);
  assert.match(onboarding, /flag: "wx"/);
  assert.match(onboarding, /mode: 0o600/);
});

test("shared Android profile callers own the native authorization ceremony", async () => {
  const helper = await readFile(
    path.join(root, "scripts", "lib", "android-test-credential.sh"),
    "utf8",
  );
  assert.match(helper, /requires a disposable QEMU emulator/);
  assert.match(helper, /oxid_android_test_credential_require_emulator/);
  assert.match(helper, /locksettings set-pin/);
  assert.doesNotMatch(helper, /locksettings get-disabled/);
  assert.match(helper, /an existing credential was not replaced/);
  assert.match(helper, /device-credential prompt observed/);
  assert.match(helper, /oxid_android_test_credential_resume_app/);
  assert.match(helper, /seq 1 75/);
  assert.match(helper, /Do not race that delivery with a synthetic `am start`/);
  assert.doesNotMatch(helper, /shell am start -W/);
  assert.match(helper, /locksettings clear/);
  assert.match(helper, /Failed to remove the disposable emulator PIN/);
  assert.match(helper, /Credential ownership remains recorded/);
  assert.match(helper, /private recovery helper retained at/);
  assert.match(helper, /mktemp -d/);
  assert.match(helper, /chmod 700/);
  assert.doesNotMatch(helper, /recover it with:[\s\S]*oxid_android_test_credential_pin/);
  assert.doesNotMatch(helper, /locksettings clear[\s\S]{0,100}\|\| true/);
  assert.doesNotMatch(helper, /non-emulator device '\$device'/);

  for (const script of [
    "test-android-backup-flow.sh",
    "test-android-developer-profile.sh",
    "test-android-standalone-local.sh",
  ]) {
    const source = await readFile(path.join(root, "scripts", script), "utf8");
    assert.match(source, /source .*android-test-credential\.sh/);
    assert.match(source, /oxid_android_test_credential_prepare/);
    assert.match(source, /oxid_android_test_credential_authorize/);
    assert.match(source, /oxid_android_test_credential_cleanup/);
    assert.doesNotMatch(source, /"\$adb_command" forward --remove/);
  }

  for (const script of [
    "test-android-developer-profile.sh",
    "test-android-standalone-local.sh",
  ]) {
    const source = await readFile(path.join(root, "scripts", script), "utf8");
    assert.match(source, /app_state_owned=0/);
    assert.match(source, /if \[ "\$\{app_state_owned:-0\}" -eq 1 \]/);
    assert.match(source, /shell pm clear io\.medianox\.oxid/);
    assert.match(source, /app_state_owned=1/);
    assert.doesNotMatch(source, /device \$device|device '\$device'|passed on \$device/);
  }

  const developer = await readFile(
    path.join(root, "scripts", "test-android-developer-profile.sh"),
    "utf8",
  );
  assert.match(
    developer,
    /oxid_android_test_credential_require_emulator[\s\S]*run-android-emulator\.sh/,
  );
  assert.match(developer, /OXID_ANDROID_REQUIRE_EMULATOR=1/);
  assert.doesNotMatch(
    developer,
    /awk 'NR > 1 && \$2 == "device" \{ print \$1; exit \}'/,
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

test("Android settings and developer automation use the separated header controls", async () => {
  const flow = await readFile(
    path.join(root, "tests", "mobile", "android-wallet-flow.mjs"),
    "utf8",
  );
  const settingsStart = flow.indexOf("async function openSettings()");
  const settingsEnd = flow.indexOf("\nasync function openPassportVault", settingsStart);
  const settings = flow.slice(settingsStart, settingsEnd);
  const developerStart = flow.indexOf('if (mode === "developer")');
  const developerEnd = flow.indexOf('} else if (mode === "demo")', developerStart);
  const developer = flow.slice(developerStart, developerEnd);

  assert.match(settings, /Open global application menu/);
  assert.match(settings, /clickGlobalAction\("Settings"\)/);
  assert.match(developer, /Open global application menu/);
  assert.match(developer, /clickGlobalAction\("Developer tools"\)/);
  assert.match(developer, /clickButton\("Open manifest"\)/);
  assert.doesNotMatch(`${settings}\n${developer}`, /Open profile menu|Open developer capabilities/);
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
  assert.match(flow, /clickFirstDidCard\(\)/);
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
  assert.match(smoke, /source .*android-test-credential\.sh/);
  assert.match(smoke, /oxid_android_test_credential_prepare/);
  assert.match(smoke, /oxid_android_test_credential_authorize/);
  assert.match(smoke, /oxid_android_test_credential_cleanup/);
  assert.doesNotMatch(smoke, /246810/);
  assert.match(smoke, /if \[ "\$app_state_owned" -eq 1 \]/);
  assert.match(smoke, /shell pm clear io\.medianox\.oxid/);
  assert.match(smoke, /onboarding_authorizer=""/);
  assert.match(
    smoke,
    /if \[ -n "\$onboarding_authorizer" \].*kill "\$onboarding_authorizer".*wait "\$onboarding_authorizer"/s,
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
  assert.match(launcher, /git status --porcelain --untracked-files=all/);
  assert.match(launcher, /prebuilt Android smoke requires a clean source worktree/);
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
