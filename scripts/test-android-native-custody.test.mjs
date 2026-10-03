// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

test("native custody smoke admits the exact onboarding and restart ceremonies", async () => {
  const script = await readFile(
    path.join(root, "scripts", "test-android-native-custody.sh"),
    "utf8",
  );
  assert.match(script, /authorize_prompts\(\)/);
  assert.match(script, /authorize_prompts 2 &\s*first_authorizer=/);
  assert.match(script, /authorize_prompts 1 &\s*second_authorizer=/);
  assert.match(script, /first_authorization[\s\S]*securityAction[\s\S]*already unlocked/);
  assert.match(script, /prompt remained focused after authorization/);
  assert.match(script, /10#\$_digit \+ 7/);
  assert.match(script, /sleep 0\.15/);
  assert.match(script, /transition to settle before injecting/);
  assert.doesNotMatch(script, /shell input text "\$test_pin"/);
  assert.match(script, /locksettings clear --old/);
  assert.match(script, /pm clear io\.medianox\.oxid/);

  const driver = await readFile(
    path.join(root, "tests", "mobile", "android-wallet-flow.mjs"),
    "utf8",
  );
  assert.match(driver, /async function openSecuritySettings/);
  assert.match(driver, /clickButtonByLabel\("Open Security"\)/);
  assert.match(driver, /async function openBackupSettings/);
  assert.match(driver, /document\.querySelector\('\.account-sync-card'\)/);
  assert.doesNotMatch(driver, /Wallet overview/);
  assert.doesNotMatch(driver, /settled pre-authorization account state/);
});
