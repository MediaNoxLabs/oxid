// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { chmod, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const policy = path.join(root, "scripts", "e2e", "android-avd-process-ownership.sh");

function run(body, environment) {
  return spawnSync("bash", ["-c", `set -euo pipefail; source "$1"; ${body}`, "receipt-test", policy], {
    encoding: "utf8",
    env: environment,
  });
}

test("receipt records an exact owned process and rejects public state", async (t) => {
  const directory = await mkdtemp(path.join(tmpdir(), "oxid-emulator-receipt-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const executable = path.join(directory, "emulator");
  const receipt = path.join(directory, "owner.receipt");
  await writeFile(executable, "fixture\n", { mode: 0o700 });
  const environment = { ...process.env, OXID_TEST_EXECUTABLE: executable, OXID_TEST_RECEIPT: receipt };

  const created = run(`
    oxid_emulator_owner_receipt_write "$OXID_TEST_RECEIPT" 111 111 "Sun Oct  4 02:00:00 2026" "$OXID_TEST_EXECUTABLE" exact_avd 5562 1:2
    oxid_emulator_owner_receipt_read "$OXID_TEST_RECEIPT"
    printf '%s|%s|%s' "$OXID_EMULATOR_RECEIPT_LAUNCH_PID" "$OXID_EMULATOR_RECEIPT_CURRENT_PID" "$OXID_EMULATOR_RECEIPT_PORT"
  `, environment);
  assert.equal(created.status, 0, created.stderr);
  assert.equal(created.stdout, "111|111|5562");

  await chmod(receipt, 0o644);
  const publicReceipt = run('oxid_emulator_owner_receipt_read "$OXID_TEST_RECEIPT"', environment);
  assert.notEqual(publicReceipt.status, 0);
});

test("receipt advances only after the prior process is dead", async (t) => {
  const directory = await mkdtemp(path.join(tmpdir(), "oxid-emulator-handoff-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const executable = path.join(directory, "emulator");
  const receipt = path.join(directory, "owner.receipt");
  await writeFile(executable, "fixture\n", { mode: 0o700 });
  const environment = { ...process.env, OXID_TEST_EXECUTABLE: executable, OXID_TEST_RECEIPT: receipt };

  const advanced = run(`
    oxid_emulator_owner_receipt_write "$OXID_TEST_RECEIPT" 111 111 "Sun Oct  4 02:00:00 2026" "$OXID_TEST_EXECUTABLE" exact_avd 5562 1:2
    oxid_emulator_owner_receipt_matches() { return 1; }
    oxid_emulator_process_is_live() { return 1; }
    oxid_find_unique_emulator_process() { printf '222\\n'; }
    oxid_emulator_process_start_identity() { printf 'Sun Oct  4 02:00:01 2026\\n'; }
    oxid_filesystem_identity() { printf '1:2\\n'; }
    oxid_emulator_owner_receipt_refresh "$OXID_TEST_RECEIPT" 111 "$OXID_TEST_EXECUTABLE" exact_avd 5562
    oxid_emulator_owner_receipt_read "$OXID_TEST_RECEIPT"
    printf '%s|%s' "$OXID_EMULATOR_RECEIPT_CURRENT_PID" "$OXID_EMULATOR_RECEIPT_CURRENT_START"
  `, environment);
  assert.equal(advanced.status, 0, advanced.stderr);
  assert.equal(advanced.stdout, "222|Sun Oct  4 02:00:01 2026");

  const stale = run(`
    oxid_emulator_owner_receipt_write "$OXID_TEST_RECEIPT" 111 111 "Sun Oct  4 02:00:00 2026" "$OXID_TEST_EXECUTABLE" exact_avd 5562 1:2
    oxid_emulator_owner_receipt_matches() { return 1; }
    oxid_emulator_process_is_live() { return 0; }
    oxid_find_unique_emulator_process() { printf '333\\n'; }
    oxid_filesystem_identity() { printf '1:2\\n'; }
    oxid_emulator_owner_receipt_refresh "$OXID_TEST_RECEIPT" 111 "$OXID_TEST_EXECUTABLE" exact_avd 5562
  `, environment);
  assert.notEqual(stale.status, 0, "a live prior PID must prevent handoff reacquisition");
});
