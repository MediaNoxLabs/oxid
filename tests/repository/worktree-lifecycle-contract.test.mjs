// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import test from "node:test";

import {
  githubMergeQuery,
  githubRepositoryFromRemote,
  indexGithubMergeProofs,
  loadGithubMergeEvidence,
  parseGithubMergeResponse,
  parseWorktrees,
  removalEligibility,
  resolveMergeState,
} from "../../scripts/worktree-lifecycle.mjs";

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
  assert.equal(
    removalEligibility({ ...candidate, merged: false, mergeProof: "unavailable" }, { primary: "/repo" }),
    "head is not integrated into origin/integration (merge proof: unavailable)",
  );
  assert.match(removalEligibility({ ...candidate, ageDays: 2 }, { primary: "/repo" }), /newer than 7 days/);
});

test("GitHub squash proof requires one exact integrated PR", () => {
  const head = "a".repeat(40);
  const mergeCommit = "b".repeat(40);
  const valid = {
    number: 189,
    state: "MERGED",
    baseRefName: "integration",
    headRefOid: head,
    mergedAt: "2026-08-28T12:00:00Z",
    mergeCommit: { oid: mergeCommit },
  };
  const integrated = (candidate) => candidate === mergeCommit;

  const result = indexGithubMergeProofs([valid], integrated);
  assert.equal(result.proofs.get(head), "github-pr:189");
  assert.equal(result.ambiguous.size, 0);

  for (const invalid of [
    { ...valid, number: 0 },
    { ...valid, state: "OPEN" },
    { ...valid, baseRefName: "develop" },
    { ...valid, headRefOid: "c".repeat(39) },
    { ...valid, mergedAt: null },
    { ...valid, mergeCommit: null },
    { ...valid, mergeCommit: { oid: "c".repeat(40) } },
  ]) {
    assert.equal(indexGithubMergeProofs([invalid], integrated).proofs.size, 0);
  }
});

test("duplicate exact-head GitHub merge proofs fail closed", () => {
  const head = "a".repeat(40);
  const first = {
    number: 188,
    state: "MERGED",
    baseRefName: "integration",
    headRefOid: head,
    mergedAt: "2026-08-28T11:39:28Z",
    mergeCommit: { oid: "b".repeat(40) },
  };
  const second = {
    ...first,
    number: 189,
    mergeCommit: { oid: "c".repeat(40) },
  };
  const result = indexGithubMergeProofs([first, second], () => true);
  assert.equal(result.proofs.size, 0);
  assert.deepEqual([...result.ambiguous], [head]);
});

test("repeated identical GraphQL associations are one proof", () => {
  const head = "a".repeat(40);
  const pull = {
    number: 189,
    state: "MERGED",
    baseRefName: "integration",
    headRefOid: head,
    mergedAt: "2026-08-28T12:00:00Z",
    mergeCommit: { oid: "b".repeat(40) },
  };
  const result = indexGithubMergeProofs([pull, pull], () => true);
  assert.equal(result.proofs.get(head), "github-pr:189");
  assert.equal(result.ambiguous.size, 0);
});

test("malformed GitHub merge evidence fails closed", () => {
  assert.throws(() => indexGithubMergeProofs({}, () => true), /must be an array/);
});

test("GitHub evidence is bound to an exact github.com origin", () => {
  assert.equal(githubRepositoryFromRemote("https://github.com/MediaNoxLabs/oxid.git"), "MediaNoxLabs/oxid");
  assert.equal(githubRepositoryFromRemote("git@github.com:MediaNoxLabs/oxid.git"), "MediaNoxLabs/oxid");
  assert.equal(githubRepositoryFromRemote("ssh://git@github.com/MediaNoxLabs/oxid"), "MediaNoxLabs/oxid");
  assert.equal(githubRepositoryFromRemote("https://example.com/MediaNoxLabs/oxid.git"), null);
  assert.equal(githubRepositoryFromRemote("https://github.com/MediaNoxLabs/oxid/extra"), null);
});

test("merge state distinguishes ancestry, hosted, absent, ambiguous, and unavailable proofs", () => {
  const head = "a".repeat(40);
  assert.deepEqual(resolveMergeState(head, true, {
    status: "unavailable", proofs: new Map(), ambiguous: new Set(),
  }), { merged: true, mergeProof: "ancestry" });
  assert.deepEqual(resolveMergeState(head, false, {
    status: "available", proofs: new Map([[head, "github-pr:189"]]), ambiguous: new Set(),
  }), { merged: true, mergeProof: "github-pr:189" });
  assert.deepEqual(resolveMergeState(head, false, {
    status: "available", proofs: new Map(), ambiguous: new Set(),
  }), { merged: false, mergeProof: "none" });
  assert.deepEqual(resolveMergeState(head, false, {
    status: "available", proofs: new Map(), ambiguous: new Set([head]),
  }), { merged: false, mergeProof: "ambiguous" });
  assert.deepEqual(resolveMergeState(head, false, {
    status: "unavailable", proofs: new Map(), ambiguous: new Set(),
  }), { merged: false, mergeProof: "unavailable" });
});

function graphqlEvidence(head, mergeCommit, { hasNextPage = false } = {}) {
  return JSON.stringify({
    data: {
      repository: {
        h0: {
          associatedPullRequests: {
            nodes: [{
              number: 189,
              state: "MERGED",
              baseRefName: "integration",
              headRefOid: head,
              mergedAt: "2026-08-28T12:00:00Z",
              mergeCommit: { oid: mergeCommit },
            }],
            pageInfo: { hasNextPage },
          },
        },
      },
    },
  });
}

function evidenceRunner({ head, tracked, mergeCommit, remoteHeads = [tracked, tracked], ghStatus = 0, trackedAvailable = true }) {
  const calls = [];
  let remoteIndex = 0;
  const run = (command, args) => {
    calls.push([command, ...args]);
    if (command === "gh") return { status: ghStatus, stdout: graphqlEvidence(head, mergeCommit) };
    if (args.includes("merge-base")) {
      return { status: args.at(-2) === mergeCommit ? 0 : 1, stdout: "" };
    }
    if (args.includes("rev-parse")) return trackedAvailable
      ? { status: 0, stdout: `${tracked}\n` }
      : { status: 128, stdout: "" };
    if (args.includes("get-url")) return { status: 0, stdout: "https://github.com/MediaNoxLabs/oxid.git\n" };
    if (args.includes("ls-remote")) {
      const remote = remoteHeads[Math.min(remoteIndex, remoteHeads.length - 1)];
      remoteIndex += 1;
      return { status: 0, stdout: `${remote}\trefs/heads/integration\n` };
    }
    throw new Error(`unexpected command: ${command} ${args.join(" ")}`);
  };
  return { calls, run };
}

test("hosted evidence loader accepts one head-scoped exact squash proof", () => {
  const head = "a".repeat(40);
  const tracked = "b".repeat(40);
  const mergeCommit = "c".repeat(40);
  const fixture = evidenceRunner({ head, tracked, mergeCommit });
  const result = loadGithubMergeEvidence("/repo", [head], { run: fixture.run });
  assert.equal(result.status, "available");
  assert.equal(result.proofs.get(head), "github-pr:189");
  const ghCall = fixture.calls.find(([command]) => command === "gh");
  assert.ok(ghCall);
  assert.match(ghCall.join(" "), new RegExp(head));
  assert.doesNotMatch(ghCall.join(" "), /pr list|limit 1000/);
});

test("hosted evidence loader degrades on missing, stale, moved, or failed I/O", () => {
  const head = "a".repeat(40);
  const tracked = "b".repeat(40);
  const moved = "d".repeat(40);
  const mergeCommit = "c".repeat(40);
  const cases = [
    evidenceRunner({ head, tracked, mergeCommit, trackedAvailable: false }),
    evidenceRunner({ head, tracked, mergeCommit, remoteHeads: [moved] }),
    evidenceRunner({ head, tracked, mergeCommit, remoteHeads: [tracked, moved] }),
    evidenceRunner({ head, tracked, mergeCommit, ghStatus: 1 }),
  ];
  for (const fixture of cases) {
    const result = loadGithubMergeEvidence("/repo", [head], { run: fixture.run });
    assert.equal(result.status, "unavailable");
    assert.equal(result.proofs.size, 0);
  }
});

test("head-scoped GraphQL truncation is unavailable rather than absent", () => {
  const head = "a".repeat(40);
  const mergeCommit = "b".repeat(40);
  const parsed = parseGithubMergeResponse(graphqlEvidence(head, mergeCommit, { hasNextPage: true }), [head]);
  assert.deepEqual([...parsed.unavailableHeads], [head]);
  assert.equal(parsed.pulls.length, 0);
  assert.match(githubMergeQuery([head]), new RegExp(head));
  assert.throws(() => githubMergeQuery([]), /requires exact commit heads/);
  assert.throws(() => githubMergeQuery(["not-a-sha"]), /requires exact commit heads/);
});
