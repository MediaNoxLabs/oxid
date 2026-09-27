// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import test from "node:test";

const root = new URL("../../", import.meta.url);

async function read(path) {
  return readFile(new URL(path, root), "utf8");
}

test("Maestro stays pinned, local-only, and additive to native coverage", async () => {
  const [packages, docs, runScript] = await Promise.all([
    read("nix/packages/default.nix"),
    read("docs/factory/maestro-mobile-pilot.md"),
    read("run.sh"),
  ]);

  assert.match(packages, /maestro = pkgs\.maestro;/u);
  assert.match(docs, /local only/iu);
  assert.match(docs, /complements and does not replace the existing Android CDP and iOS XCTest suites/iu);
  assert.match(docs, /It is not GitHub CI/iu);
  assert.match(docs, /Never use a physical phone/iu);
  assert.equal(
    runScript.match(/node --test tests\/repository\/maestro-mobile-pilot-contract\.test\.mjs/gu)?.length,
    1,
  );

  const workflowDirectory = new URL(".github/workflows/", root);
  const workflowNames = (await readdir(workflowDirectory)).filter((name) => /\.ya?ml$/u.test(name));
  const workflowSources = await Promise.all(
    workflowNames.map(async (name) => [name, await read(`.github/workflows/${name}`)]),
  );
  for (const [name, source] of workflowSources) {
    assert.doesNotMatch(source, /maestro/iu, `${name} must not adopt the local-only pilot`);
  }
});

test("Maestro wrappers own only explicit simulator and emulator targets", async () => {
  const [ios, android] = await Promise.all([
    read("scripts/run-maestro-ios.sh"),
    read("scripts/run-maestro-android.sh"),
  ]);

  assert.match(ios, /OXID_IOS_DEVICE/gu);
  assert.match(ios, /OXID_IOS_RESET_DATA=1/u);
  assert.match(ios, /OXID_STANDALONE_NETWORK_PROFILE=simulated/u);
  assert.match(ios, /OXID_UI_PROFILE=demo/u);
  assert.match(ios, /artifact_root="\$root\/target\/mobile-visual-accessibility\/ios\/\$OXID_IOS_DEVICE"/u);
  assert.match(ios, /debug_root="\$artifact_root\/debug"/u);
  assert.match(ios, /\.\/scripts\/run-ios-simulator\.sh deploy/u);
  assert.match(
    ios,
    /nix run \.#maestro -- test tests\/maestro\/ios-lunar-aegis\.yaml[\s\\]*--udid "\$OXID_IOS_DEVICE" --test-output-dir "\$artifact_root"[\s\\]*--debug-output "\$debug_root"/u,
  );

  assert.match(android, /OXID_ANDROID_DEVICE/gu);
  assert.match(android, /case "\$OXID_ANDROID_DEVICE" in emulator-\*/u);
  assert.match(android, /refusing non-emulator device/u);
  assert.match(android, /OXID_ANDROID_REQUIRE_EMULATOR=1/u);
  assert.match(android, /OXID_STANDALONE_NETWORK_PROFILE=simulated/u);
  assert.match(android, /OXID_UI_PROFILE=demo/u);
  assert.match(android, /artifact_root="\$root\/target\/mobile-visual-accessibility\/android\/\$OXID_ANDROID_DEVICE"/u);
  assert.match(android, /debug_root="\$artifact_root\/debug"/u);
  assert.match(android, /\.\/scripts\/run-android-emulator\.sh deploy/u);
  assert.match(
    android,
    /nix run \.#maestro -- test tests\/maestro\/android-lunar-aegis\.yaml[\s\\]*--device "\$OXID_ANDROID_DEVICE" --test-output-dir "\$artifact_root"[\s\\]*--debug-output "\$debug_root"/u,
  );
});

test("mobile visual accessibility evidence keeps the scoped matrix and privacy boundary", async () => {
  const matrix = await read("docs/factory/mobile-visual-accessibility-evidence.md");

  for (const state of [
    "Welcome and create-vs-restore fork",
    "Mandatory device-protection explanation",
    "Recovery boundary and Ready/Home",
    "Receive and Send entry",
    "Empty Documents and fixture Activity",
    "Settings and native-custody Backup boundary",
  ]) {
    assert.match(matrix, new RegExp(state, "u"));
  }
  for (const screenId of ["XSwTg6CjwXruX8QP3tXy", "FFMmLvVQlc5xIun63FYX", "xYA9BiozNUetlxPJYHPT", "7u81lbjNIKcn8dS79axb"]) {
    assert.match(matrix, new RegExp(screenId, "u"));
  }
  assert.match(matrix, /375 pt\/dp/u);
  assert.match(matrix, /larger width/u);
  assert.match(matrix, /safe-area\/navigation non-overlap/u);
  assert.match(matrix, /44 px touch targets/u);
  assert.match(matrix, /large-text truncation/u);
  assert.match(matrix, /non-color status meaning/u);
  assert.match(matrix, /deterministic Back/u);
  assert.match(matrix, /modal focus return/u);
  assert.match(matrix, /reduced motion/u);
  assert.match(matrix, /screen-reader labels\/order/u);
  assert.match(matrix, /target\/mobile-visual-accessibility\/<platform>/u);
  assert.match(matrix, /never capture a recovery phrase/iu);
  assert.match(matrix, /iOS Simulator.*Android Emulator/us);
});

test("Maestro flows cover the holder shell without exposing recovery secrets", async () => {
  const [ios, android] = await Promise.all([
    read("tests/maestro/ios-lunar-aegis.yaml"),
    read("tests/maestro/android-lunar-aegis.yaml"),
  ]);

  for (const [platform, source] of [
    ["ios", ios],
    ["android", android],
  ]) {
    assert.match(source, /appId: io\.medianox\.oxid/u);
    assert.match(source, /Create private wallet/u);
    assert.match(source, /scrollUntilVisible:[\s\S]*text: "Create and continue"[\s\S]*direction: DOWN/u);
    assert.match(source, /Create and continue/u);
    assert.match(source, /Device protection is required\.\*/u);
    assert.match(source, /Open standalone demo setup/u);
    assert.match(source, /Run demo action: Create or select demo profile/u);
    assert.match(source, /Run demo action: Initialize or unlock wallet/u);
    assert.match(
      source,
      /\(Initialized process-local standalone custody\.\|Wallet session was already unlocked; no key was regenerated\.\)/u,
    );
    assert.match(source, /Close standalone demo setup/u);
    assert.match(source, /Current realm/u);
    assert.match(source, /Receive/u);
    assert.match(source, /Send/u);
    assert.match(source, /Documents/u);
    assert.match(source, /Activity/u);
    assert.match(source, /SEND NIGHT/u);
    assert.match(source, /tapOn: "Go back"/u);
    assert.match(source, /No credentials yet/u);
    assert.match(source, /visible: "Sent"/u);
    assert.match(source, /assertVisible: "Received"/u);
    assert.match(source, /scrollUntilVisible:[\s\S]*text: "No credentials yet"[\s\S]*direction: DOWN/u);
    assert.match(source, new RegExp(`takeScreenshot: lunar-aegis-${platform}-01-first-run`, "u"));
    assert.doesNotMatch(source, /takeScreenshot:.*recovery/iu);
    assert.doesNotMatch(source, /Generate recovery phrase|New wallet recovery phrase/iu);
    assert.doesNotMatch(source, /Run full demo setup|Derive Midnight account|Load simulated funding/iu);
  }

  assert.doesNotMatch(ios, /androidWebViewHierarchy/u);
  assert.match(ios, /takeScreenshot: lunar-aegis-ios-02-device-protection/u);
  assert.match(ios, /Open global application menu/u);
  assert.match(ios, /Settings/u);
  assert.match(android, /androidWebViewHierarchy: devtools/u);
  assert.match(android, /tapOn: "Session privacy"[\s\S]*takeScreenshot: lunar-aegis-android-03-home-public-revealed/u);
  assert.match(android, /Private values revealed[\s\S]*tapOn: "Session privacy"[\s\S]*Private values hidden/u);
  assert.doesNotMatch(android, /takeScreenshot: lunar-aegis-android-0[4-9]/u);
  assert.match(android, /tapOn: "Settings"/u);
});
