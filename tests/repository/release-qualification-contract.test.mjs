// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { validateReleaseQualification, verifyReleaseQualification } from "../../scripts/github/verify-release-qualification.mjs";

const sha = "a".repeat(40);
const completedRun = (overrides = {}) => ({
  event: "workflow_dispatch",
  head_branch: "milestone-0.2.0",
  head_sha: sha,
  status: "completed",
  conclusion: "success",
  jobs: [
    { name: "Hermetic nix flake check (full sandboxed test suite)", status: "completed", conclusion: "success" },
    { name: "Scorecard analysis", status: "completed", conclusion: "success" },
  ],
  ...overrides,
});

test("release qualification requires both completed checks at the exact milestone head", () => {
  assert.deepEqual(validateReleaseQualification(completedRun(), {
    branch: "milestone-0.2.0",
    sha,
  }), { ok: true, failures: [] });

  for (const run of [
    completedRun({ head_sha: "b".repeat(40) }),
    completedRun({ head_branch: "develop" }),
    completedRun({ event: "schedule" }),
    completedRun({ conclusion: "failure" }),
    completedRun({ jobs: [{ name: "Hermetic nix flake check (full sandboxed test suite)", status: "completed", conclusion: "success" }] }),
  ]) {
    assert.equal(validateReleaseQualification(run, { branch: "milestone-0.2.0", sha }).ok, false);
  }
});

test("a new milestone head invalidates retained qualification evidence", () => {
  const result = validateReleaseQualification(completedRun(), {
    branch: "milestone-0.2.0",
    sha: "c".repeat(40),
  });
  assert.equal(result.ok, false);
  assert.match(result.failures.join("; "), /head SHA/);
});

test("the verifier rejects an old receipt when the remote milestone moves", () => {
  const calls = [];
  const run = (_command, args) => {
    calls.push(args);
    if (args[3]?.endsWith("/git/ref/heads/milestone-0.2.0")) {
      return JSON.stringify({ object: { sha: "b".repeat(40) } });
    }
    throw new Error("stale evidence must be rejected before workflow lookup");
  };
  assert.throws(() => verifyReleaseQualification({ repo: "MediaNoxLabs/oxid", branch: "milestone-0.2.0", sha }, { run }), /no longer points/);
  assert.equal(calls.length, 1);
});

test("the verifier rejects a branch move during evidence lookup", () => {
  let refReads = 0;
  const run = (_command, args) => {
    if (args[3]?.endsWith("/git/ref/heads/milestone-0.2.0")) {
      refReads += 1;
      return JSON.stringify({ object: { sha: refReads === 1 ? sha : "b".repeat(40) } });
    }
    if (args[3]?.endsWith("/actions/workflows/nightly.yml/runs")) {
      return JSON.stringify({ workflow_runs: [{ ...completedRun(), id: 12, html_url: "https://example.test/run" }] });
    }
    if (args[3]?.endsWith("/actions/runs/12/jobs")) {
      return JSON.stringify({ jobs: completedRun().jobs });
    }
    throw new Error(`unexpected GitHub request: ${args.join(" ")}`);
  };
  assert.throws(() => verifyReleaseQualification({ repo: "MediaNoxLabs/oxid", branch: "milestone-0.2.0", sha }, { run }), /moved while qualification evidence was checked/);
  assert.equal(refReads, 2);
});

test("existing dispatchable Nightly workflow qualifies the selected source SHA", async () => {
  const workflow = await readFile(new URL("../../.github/workflows/nightly.yml", import.meta.url), "utf8");
  assert.match(workflow, /^  workflow_dispatch: \{\}$/m);
  assert.doesNotMatch(workflow, /^  (?:push|pull_request):/m);
  assert.match(workflow, /if: github\.event_name == 'workflow_dispatch' && startsWith\(github\.ref, 'refs\/heads\/milestone-'\)/);
  assert.equal((workflow.match(/ref: \$\{\{ github\.sha \}\}/g) ?? []).length, 2);
  assert.match(workflow, /nix flake check --print-build-logs/);
  assert.match(workflow, /ossf\/scorecard-action@[0-9a-f]{40}/);
});

test("the develop promotion audit requires exact-head qualification for milestone sources", async () => {
  const audit = await readFile(new URL("../../scripts/github/merge-develop-pr.mjs", import.meta.url), "utf8");
  assert.match(audit, /MILESTONE_BRANCH_PATTERN\.test\(pr\.headRefName/);
  assert.match(audit, /verifyReleaseQualification\(\{ repo: options\.repo, branch: pr\.headRefName, sha: pr\.headRefOid \}/);
});
