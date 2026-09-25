// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const root = new URL("../../", import.meta.url);

test("virtual mobile lanes discover capability and publish truthful state", async () => {
  const [ios, android, evidence] = await Promise.all([
    readFile(new URL("scripts/test-ios-portal-exact-sequence-simulator.sh", root), "utf8"),
    readFile(new URL("scripts/test-android-portal-exact-sequence-avd.sh", root), "utf8"),
    readFile(new URL("scripts/e2e/portal-virtual-mobile-evidence.mjs", root), "utf8"),
  ]);

  assert.match(ios, /oxid_ios_discover_developer_directory/u);
  assert.match(ios, /oxid_ios_resolve_selectors/u);
  assert.match(ios, /oxid_ios_supervise_acceptance "\$ROOT" ios-portal-exact-sequence 7200/u);
  assert.match(ios, /oxid_ios_run_xctest "\$ROOT" "\$scenario_name" 600/u);
  assert.match(android, /oxid_android_discover_avd/u);
  for (const harness of [ios, android]) {
    assert.match(
      harness,
      /applicationState:\{install:"fresh",restart:"preserved",migration:"not_exercised"\}/u,
    );
  }
  assert.match(evidence, /oxid-portal-virtual-mobile-evidence-v2/u);
  assert.match(evidence, /truthfulApplicationState/u);
  assert.match(evidence, /migration !== "not_exercised"/u);
});

test("physical and service-only paths do not overclaim lifecycle mutation", async () => {
  const [physical, phases, services] = await Promise.all([
    readFile(new URL("scripts/test-android-portal-tailnet-physical.sh", root), "utf8"),
    readFile(new URL("scripts/portal-tailnet-manual-phases.sh", root), "utf8"),
    readFile(new URL("scripts/e2e/portal-services-lifecycle.sh", root), "utf8"),
  ]);

  assert.match(physical, /deviceDataMode:"preserved"/u);
  assert.match(phases, /adb" install -r/u);
  assert.match(phases, /applicationDataCleared:false/u);
  assert.match(phases, /pidof/u);
  assert.doesNotMatch(services, /\b(?:build|install|configure|launch)\b/u);
});
