// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

test("release-candidate build is arm64-only, statically verified, and device-free", async () => {
  const [script, justfile, guide] = await Promise.all([
    readFile(path.join(root, "scripts", "build-android-release-candidate.sh"), "utf8"),
    readFile(path.join(root, "Justfile"), "utf8"),
    readFile(path.join(root, "docs", "factory", "application-targets.md"), "utf8"),
  ]);

  assert.match(justfile, /^android-release-build:\n    \.\/scripts\/build-android-release-candidate\.sh$/m);
  assert.match(script, /--release/);
  assert.match(script, /--target aarch64-linux-android/);
  assert.match(script, /target\/android-release-candidate\/oxid-app-arm64-v8a-release\.apk/);
  assert.match(script, /node scripts\/android-verify-16k\.mjs "\$raw_artifact"/);
  assert.match(script, /"\$zipalign" -c -P 16 -v 4 "\$raw_artifact"/);
  assert.match(script, /max-page-size=16384/);
  assert.match(script, /common-page-size=16384/);
  assert.match(script, /if ! node scripts\/android-verify-16k\.mjs/);
  assert.match(script, /chmod 600 "\$receipt"/);
  assert.doesNotMatch(script, /^\s*adb\s|OXID_ANDROID_DEVICE|OXID_ANDROID_AVD|am start/m);
  assert.match(guide, /just android-release-build/);
  assert.match(guide, /does not select, boot, install to, or launch/);
});
