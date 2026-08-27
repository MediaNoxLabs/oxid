// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import test from "node:test";

import { parseWorktrees, removalEligibility } from "../../scripts/worktree-lifecycle.mjs";

test("worktree porcelain parsing preserves paths and branches", () => {
  assert.deepEqual(parseWorktrees([
    "worktree /repo",
    "HEAD abc",
    "branch refs/heads/integration",
    "",
    "worktree /repo/tmp/worktrees/issue-1",
    "HEAD def",
    "detached",
  ].join("\n")), [
    { worktree: "/repo", HEAD: "abc", branch: "refs/heads/integration" },
    { worktree: "/repo/tmp/worktrees/issue-1", HEAD: "def", detached: true },
  ]);
});

test("removal requires a clean, merged, old, non-primary worktree", () => {
  const candidate = { worktree: "/repo/w", clean: true, merged: true, ageDays: 8 };
  assert.equal(removalEligibility(candidate, { primary: "/repo" }), null);
  assert.equal(removalEligibility({ ...candidate, worktree: "/repo" }, { primary: "/repo" }), "primary checkout");
  assert.equal(removalEligibility({ ...candidate, clean: false }, { primary: "/repo" }), "worktree is dirty");
  assert.equal(removalEligibility({ ...candidate, merged: false }, { primary: "/repo" }), "head is not merged into origin/integration");
  assert.match(removalEligibility({ ...candidate, ageDays: 2 }, { primary: "/repo" }), /newer than 7 days/);
});
