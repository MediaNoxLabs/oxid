// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const lifecyclePath = path.join(root, "scripts", "test-android-portal-tailnet-physical.sh");

test("manual Tailnet Portal lifecycle is a bounded, receipt-supervised owner demo", async () => {
  const [lifecycle, justfile] = await Promise.all([
    readFile(lifecyclePath, "utf8"),
    readFile(path.join(root, "Justfile"), "utf8"),
  ]);

  for (const recipe of [
    "portal-tailnet-manual-start:",
    "portal-tailnet-manual-status:",
    "portal-tailnet-manual-stop:",
  ]) assert.match(justfile, new RegExp(`^${recipe}`, "m"));

  for (const operation of ["manual-start", "manual-status", "manual-stop", "--manual-supervise"]) {
    assert.match(lifecycle, new RegExp(operation));
  }
  assert.match(lifecycle, /target\/portal-tailnet-manual\/runtime/);
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
