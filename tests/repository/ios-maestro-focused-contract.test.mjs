// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import test from "node:test";

const root = new URL("../../", import.meta.url);
const read = (path) => readFile(new URL(path, root), "utf8");

test("focused iOS Maestro runner admits one inventory-owned diagnostic only", async () => {
  const script = await read("scripts/test-ios-maestro-focused.sh");
  assert.match(script, /\.id == \$id and \.composition == \$composition and \.authority == "maestro"/u);
  assert.match(script, /\(\.platforms \| index\("ios"\)\)/u);
  assert.match(script, /if length == 1 then first\.flow else error/u);
  assert.match(script, /\^flows\/\[a-z0-9-\]\+\\\.yaml\$/u);
  assert.match(script, /\[ -z "\$\{OXID_IOS_DEVICE:-\}" \] \|\| fail ambient-device-selector/u);
  assert.match(script, /\[ -z "\$\(git -C "\$ROOT" status --porcelain\)" \] \|\| fail dirty-source/u);
});

test("focused iOS Maestro runner owns cleanup and retains only bounded non-release metrics", async () => {
  const script = await read("scripts/test-ios-maestro-focused.sh");
  assert.match(script, /oxid_ios_create_owned/u);
  assert.match(script, /oxid_ios_delete_owned/u);
  assert.match(script, /trap cleanup EXIT/u);
  assert.match(script, /trap on_signal INT TERM HUP/u);
  assert.match(script, /run-maestro-ios\.sh" --composition "\$COMPOSITION" --flow "\$FLOW_ID"/u);
  assert.match(script, /OXID_IOS_OPERATION_TIMEOUT_SECONDS:-300/u);
  assert.match(script, />"\$PRIVATE_LOG" 2>&1/u);
  assert.match(script, /rm -rf -- "\$ROOT\/target\/mobile-visual-accessibility\/ios\/\$DEVICE"/u);
  assert.match(script, /rm -rf -- "\$PRIVATE_ROOT"/u);
  assert.match(script, /authority:"diagnostic-only",releaseEvidence:false/u);
  assert.match(script, /privateDiagnosticsRemoved:\$privateRemoved/u);
  assert.doesNotMatch(script, /udid:\$DEVICE|device:\$DEVICE/u);
});

test("focused iOS Maestro selectors fail before host mutation", () => {
  for (const args of [
    [],
    ["--composition", "release", "--flow", "canonical-holder-evidence"],
    ["--composition", "demo", "--flow", "not-in-the-inventory"],
    ["--composition", "dev", "--flow", "canonical-holder-evidence"],
  ]) {
    const result = spawnSync("bash", ["scripts/test-ios-maestro-focused.sh", ...args], {
      cwd: new URL(root), encoding: "utf8",
    });
    assert.equal(result.status, 2, `${args.join(" ")} must be a usage failure`);
    assert.match(result.stderr, /usage:/u);
  }
});
