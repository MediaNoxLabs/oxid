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
  assert.match(ios, /\.\/scripts\/run-ios-simulator\.sh deploy/u);
  assert.match(
    ios,
    /nix run \.#maestro -- test tests\/maestro\/ios-lunar-aegis\.yaml --udid "\$OXID_IOS_DEVICE"/u,
  );

  assert.match(android, /OXID_ANDROID_DEVICE/gu);
  assert.match(android, /case "\$OXID_ANDROID_DEVICE" in emulator-\*/u);
  assert.match(android, /refusing non-emulator device/u);
  assert.match(android, /OXID_ANDROID_REQUIRE_EMULATOR=1/u);
  assert.match(android, /OXID_STANDALONE_NETWORK_PROFILE=simulated/u);
  assert.match(android, /\.\/scripts\/run-android-emulator\.sh deploy/u);
  assert.match(
    android,
    /nix run \.#maestro -- test tests\/maestro\/android-lunar-aegis\.yaml --device "\$OXID_ANDROID_DEVICE"/u,
  );
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
    assert.match(source, /Current realm/u);
    assert.match(source, /Receive/u);
    assert.match(source, /Send/u);
    assert.match(source, /Documents/u);
    assert.match(source, /Activity/u);
    assert.match(source, /Open global application menu/u);
    assert.match(source, /Settings/u);
    assert.match(source, /Backup/u);
    assert.match(source, new RegExp(`takeScreenshot: lunar-aegis-${platform}-01-first-run`, "u"));
    assert.doesNotMatch(source, /takeScreenshot:.*recovery/iu);
  }

  assert.doesNotMatch(ios, /androidWebViewHierarchy/u);
  assert.match(android, /androidWebViewHierarchy: devtools/u);
});
