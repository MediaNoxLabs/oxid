// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import { prepareAdmission, verifyAdmission } from "../../scripts/loop/prepare-dev-loop-admission.mjs";

function fixture(t) {
  const root = mkdtempSync(path.join(os.tmpdir(), "oxid-admission-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const git = (...args) => execFileSync("git", ["-C", root, ...args], { encoding: "utf8" }).trim();
  git("init", "-q");
  git("-c", "commit.gpgsign=false", "-c", "user.name=Fixture", "-c", "user.email=fixture@example.test",
    "commit", "-q", "--allow-empty", "-m", "fixture");
  git("checkout", "-q", "-b", "fix/issue-937");
  git("remote", "add", "origin", "https://github.com/MediaNoxLabs/oxid.git");
  git("config", "--local", "branch.fix/issue-937.oxidDeliveryBase", "origin/milestone-0.2.0");
  const directory = path.join(root, "target", "tmp", "dev-loop");
  mkdirSync(directory, { recursive: true });
  const receipt = path.join(directory, "issue-937-admission.json");
  const startup = path.join(directory, "issue-937-startup.json");
  writeFileSync(receipt, JSON.stringify({
    schema: "oxid-dev-loop-admission-v1", issue: 937,
    repository: "MediaNoxLabs/oxid", branch: "fix/issue-937",
    deliveryBase: "origin/milestone-0.2.0",
    calls: { commands: 5, ensureWorktree: 0, recordDeliveryBase: 1 },
  }));
  return { root, git, receipt, startup };
}

test("pre-Pi admission is recorded outside the child and verified against its checkout", (t) => {
  const { root, receipt, startup } = fixture(t);
  const result = prepareAdmission(937, {
    cwd: root,
    run: () => JSON.stringify({ ok: true, canonicalStateSummary: { target: { issue: 937 } } }),
  });
  assert.equal(result.repository, "MediaNoxLabs/oxid");
  const verified = verifyAdmission(937, { cwd: root });
  assert.equal(verified.deliveryBase, "origin/milestone-0.2.0");
  assert.equal(verified.prePiCalls.commands, 5);
  assert.equal(JSON.parse(readFileSync(receipt, "utf8")).implementationChildCalls, 0);
  const nested = path.join(root, "nested");
  mkdirSync(nested);
  assert.equal(verifyAdmission(937, { cwd: nested }).issue, 937);
  writeFileSync(startup, "{}\n");
  assert.throws(() => verifyAdmission(937, { cwd: root }), /stale or incomplete/u);
});

test("admission refuses a stale snapshot or changed origin", (t) => {
  const { root, git, receipt } = fixture(t);
  prepareAdmission(937, {
    cwd: root,
    run: () => JSON.stringify({ ok: true, canonicalStateSummary: { target: { issue: 937 } } }),
  });
  const prepared = JSON.parse(readFileSync(receipt, "utf8"));
  assert.throws(() => verifyAdmission(937, {
    cwd: root, now: Date.parse(prepared.preparedAt) + 6 * 60 * 1000,
  }), /stale or incomplete/u);
  git("remote", "set-url", "origin", "https://github.com/input-output-hk/oxid.git");
  assert.throws(() => verifyAdmission(937, { cwd: root }), /disagrees with the current checkout/u);
  git("remote", "set-url", "origin", "https://github.com/MediaNoxLabs/oxid.git");
  git("checkout", "--detach", "-q");
  assert.throws(() => verifyAdmission(937, { cwd: root }), /requires the recorded delivery base/u);
});
