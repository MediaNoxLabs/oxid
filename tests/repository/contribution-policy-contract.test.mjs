// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import test from "node:test";

import {
  contributionPolicy,
  labelsForSubject,
  parseConventionalSubject,
  validateBranchName,
  validateCommitMessage,
  validateCommitEvidence,
  validateHostedCommits,
  validateIntegrationPromotionEvidence,
  validatePullRequest,
  validatePullRequestBody,
} from "../../scripts/ci/contribution-policy.mjs";
import { desiredContributionLabels } from "../../scripts/github/sync-contribution-labels.mjs";

const repoRoot = path.resolve(import.meta.dirname, "../..");
const read = (relativePath) => readFile(path.join(repoRoot, relativePath), "utf8");

test("contribution policy has unique sorted bounded types and scopes", () => {
  for (const field of ["types", "scopes"]) {
    const values = contributionPolicy[field];
    assert.deepEqual(values, [...new Set(values)].sort());
    for (const value of values) assert.match(value, /^[a-z][a-z0-9-]*$/u);
  }
  assert.deepEqual(contributionPolicy.branch.protected, ["develop", "main", "milestone-*"]);
  assert.equal(contributionPolicy.commit.requireDco, true);
  assert.equal(contributionPolicy.commit.requireOpenPgp, true);
  assert.equal(contributionPolicy.commit.requireScope, true);
});

test("strict Conventional Commit subjects require an allowed type and scope", () => {
  for (const subject of [
    "feat(factory): enforce contribution provenance",
    "fix(openid): reject an invalid credential response",
    "docs(architecture): explain the adapter boundary",
  ]) assert.equal(parseConventionalSubject(subject).ok, true, subject);

  for (const subject of [
    "feat: missing scope",
    "bug(wallet): unsupported type",
    "fix(android): retired scope",
    "fix(wallet): trailing period.",
    "fix(wallet): WIP temporary result",
  ]) assert.equal(parseConventionalSubject(subject).ok, false, subject);
});

test("breaking subjects require an explicit breaking-change footer", () => {
  assert.equal(parseConventionalSubject("feat(protocol)!: replace the envelope").ok, false);
  assert.equal(parseConventionalSubject("feat(protocol)!: replace the envelope", {
    body: "BREAKING CHANGE: version two cannot read version one envelopes",
  }).ok, true);
});

test("issue branches use the PR type and no descriptive suffix", () => {
  assert.deepEqual(validateBranchName("feat/issue-191", { expectedType: "feat" }), {
    ok: true,
    errors: [],
    type: "feat",
    issue: 191,
    exempt: false,
  });
  for (const branch of ["feature/issue-191", "feat/191", "feat/issue-0", "feat/issue-191-description"]) {
    assert.equal(validateBranchName(branch).ok, false, branch);
  }
  assert.equal(validateBranchName("fix/issue-191", { expectedType: "feat" }).ok, false);
  assert.equal(validateBranchName("dependabot/cargo/serde", { actor: "dependabot[bot]" }).ok, true);
});

test("PR validation binds conventional title type to branch type", () => {
  assert.equal(validatePullRequest({
    title: "feat(factory): enforce contribution provenance",
    branch: "feat/issue-191",
    actor: "yshyn-iohk",
  }).ok, true);
  assert.equal(validatePullRequest({
    title: "fix(factory): enforce contribution provenance",
    branch: "feat/issue-191",
    actor: "yshyn-iohk",
  }).ok, false);
});

test("PR body closes the issue encoded in a human branch", () => {
  assert.equal(validatePullRequestBody("Closes #191", { expectedIssue: 191, actor: "yshyn-iohk" }).ok, true);
  assert.equal(validatePullRequestBody("Fixes #191", { expectedIssue: 191, actor: "yshyn-iohk" }).ok, true);
  assert.equal(validatePullRequestBody("Closes #19", { expectedIssue: 191, actor: "yshyn-iohk" }).ok, false);
  assert.equal(validatePullRequestBody("Generated update", { expectedIssue: 191, actor: "dependabot[bot]" }).ok, true);
});

test("commit evidence requires exact DCO identity and an OpenPGP envelope", () => {
  const valid = {
    message: "feat(factory): enforce contribution provenance\n\nSigned-off-by: Factory Agent <agent@example.com>\n",
    authorName: "Factory Agent",
    authorEmail: "agent@example.com",
    rawCommit: "tree deadbeef\ngpgsig -----BEGIN PGP SIGNATURE-----\n fake\n",
  };
  assert.equal(validateCommitEvidence(valid).ok, true);
  assert.equal(validateCommitEvidence({ ...valid, message: "feat(factory): enforce contribution provenance" }).ok, false);
  assert.equal(validateCommitEvidence({ ...valid, rawCommit: "tree deadbeef\n" }).ok, false);
  assert.equal(validateCommitEvidence({
    ...valid,
    message: "chore(deps): update serde",
    authorName: "dependabot[bot]",
    authorEmail: "bot@example.com",
    actor: "dependabot[bot]",
  }).ok, true);
});

test("local message policy validates DCO and subject before a signature object exists", () => {
  const result = validateCommitMessage({
    message: "feat(factory): enforce local hooks\n\nSigned-off-by: Factory Agent <agent@example.com>\n",
    authorName: "Factory Agent",
    authorEmail: "agent@example.com",
  });
  assert.equal(result.ok, true);
  assert.equal(validateCommitMessage({
    message: "feat: missing scope",
    authorName: "Factory Agent",
    authorEmail: "agent@example.com",
  }).ok, false);
});

test("hosted commit evidence is exact-head, unique, and GitHub-verified OpenPGP", () => {
  const sha = "a".repeat(40);
  const valid = [{
    sha,
    message: "feat(factory): enforce contribution provenance\n\nSigned-off-by: Factory Agent <agent@example.com>\n",
    authorName: "Factory Agent",
    authorEmail: "agent@example.com",
    verification: {
      verified: true,
      reason: "valid",
      signature: "-----BEGIN PGP SIGNATURE-----\nfake\n-----END PGP SIGNATURE-----",
    },
  }];
  assert.equal(validateHostedCommits(valid, { expectedHead: sha }).ok, true);
  assert.equal(validateHostedCommits(valid, { expectedHead: "b".repeat(40) }).ok, false);
  assert.equal(validateHostedCommits([...valid, ...valid], { expectedHead: sha }).ok, false);
  assert.equal(validateHostedCommits([{ ...valid[0], sha: "b".repeat(40) }, ...valid], { expectedHead: "b".repeat(40) }).ok, false);
  assert.equal(validateHostedCommits([{ ...valid[0], verification: { verified: true, reason: "valid", signature: "-----BEGIN SSH SIGNATURE-----" } }], { expectedHead: sha }).ok, false);
});

test("integration promotion evidence accepts only verified OpenPGP merge artifacts", () => {
  const sha = "a".repeat(40);
  const mergeArtifact = {
    sha,
    message: "feat: historical squash subject without current scope",
    authorName: "GitHub",
    authorEmail: "noreply@github.com",
    verification: {
      verified: true,
      reason: "valid",
      signature: "-----BEGIN PGP SIGNATURE-----\nfake\n-----END PGP SIGNATURE-----",
    },
  };
  assert.equal(validateHostedCommits([mergeArtifact], { expectedHead: sha }).ok, false);
  assert.equal(validateHostedCommits([mergeArtifact], {
    expectedHead: sha,
    mode: "integration-promotion",
  }).ok, true);
  assert.equal(validateHostedCommits([{
    ...mergeArtifact,
    verification: { verified: true, reason: "valid", signature: "-----BEGIN SSH SIGNATURE-----" },
  }], {
    expectedHead: sha,
    mode: "integration-promotion",
  }).ok, false);
  assert.equal(validateHostedCommits([mergeArtifact], {
    expectedHead: sha,
    mode: "unknown",
  }).ok, false);
  assert.equal(validateIntegrationPromotionEvidence({
    verification: mergeArtifact.verification,
  }).ok, true);
});

test("labels are a complete projection of canonical types and scopes", () => {
  assert.deepEqual(labelsForSubject("feat(factory): enforce contribution provenance").labels, ["type:feat", "scope:factory"]);
  const labels = desiredContributionLabels();
  assert.equal(labels.length, contributionPolicy.types.length + contributionPolicy.scopes.length);
  assert.equal(new Set(labels.map((label) => label.name)).size, labels.length);
});

test("hosted policy evaluates trusted base code and verifies OpenPGP through GitHub", async () => {
  const dco = await read(".github/workflows/contribution-commits.yml");
  assert.match(dco, /^  pull_request_target:/m);
  assert.match(dco, /# zizmor: ignore\[dangerous-triggers\] trusted-base metadata only; candidate code is never executed/);
  assert.match(dco, /path: policy/);
  assert.match(dco, /ref: \$\{\{ github\.workflow_sha \}\}/);
  assert.match(dco, /github\.rest\.pulls\.listCommits/);
  assert.match(dco, /candidate\.commit\.verification/);
  assert.match(dco, /policy\/scripts\/ci\/contribution-policy\.mjs hosted-commits/);
  assert.match(dco, /COMMIT_POLICY_MODE:/);
  assert.match(dco, /head\.ref == 'develop'/);
  assert.match(dco, /startsWith\(github\.event\.pull_request\.head\.ref, 'milestone-'\)/);
  assert.match(dco, /base\.ref == 'main'/);
  assert.match(dco, /base\.ref == 'develop'/);
  assert.equal((dco.match(/repo\.full_name == github\.repository/g) || []).length, 4);
  assert.match(dco, /'integration-promotion' \|\| 'contributor'/);
  assert.match(dco, /createCommitStatus/);
  assert.match(dco, /state: 'pending'/);
  assert.match(dco, /state: passed \? 'success' : 'failure'/);
  assert.match(dco, /core\.setFailed/);
  assert.match(dco, /sha: context\.payload\.pull_request\.head\.sha/);
  assert.match(dco, /context: 'Verify commit sign-offs'/);
  assert.doesNotMatch(dco, /pull_request\.head\.sha[^\n]*\n[^\n]*path:/);

  const prCheck = await read(".github/workflows/contribution-metadata.yml");
  assert.match(prCheck, /^  pull_request_target:/m);
  assert.match(prCheck, /# zizmor: ignore\[dangerous-triggers\] trusted-base metadata only; candidate code is never executed/);
  assert.match(prCheck, /path: policy/);
  assert.match(prCheck, /policy\/scripts\/ci\/contribution-policy\.mjs pr/);
  assert.match(prCheck, /context: 'Validate PR title'/);
  assert.match(prCheck, /context: 'Validate PR body'/);
  assert.match(prCheck, /state: 'pending'/);
  assert.match(prCheck, /ready_for_review, converted_to_draft/);
  assert.match(prCheck, /state: 'success'/);
  assert.match(prCheck, /Advisory: title, scope, or branch policy failed/);
  assert.match(prCheck, /core\.warning/);
  assert.doesNotMatch(prCheck, /core\.setFailed/);
  assert.match(prCheck, /cancel-in-progress: true/);
  assert.match(prCheck, /sha: context\.payload\.pull_request\.head\.sha/);
  assert.doesNotMatch(prCheck, /action-semantic-pull-request|pull_request\.head\.sha[^\n]*\n[^\n]*path:/);
});

test("legacy candidate-controlled contribution workflows are retired", async () => {
  await assert.rejects(read(".github/workflows/dco.yml"), { code: "ENOENT" });
  await assert.rejects(read(".github/workflows/pr-check.yml"), { code: "ENOENT" });
  const policy = await read("docs/factory/contribution-policy.md");
  assert.match(policy, /#193/);
  assert.match(policy, /completed rollout/i);
});

test("PR metadata workflow never checks out or executes candidate code", async () => {
  const labels = await read(".github/workflows/pr-labels.yml");
  assert.match(labels, /^  pull_request_target:/m);
  assert.match(labels, /# zizmor: ignore\[dangerous-triggers\] trusted-base metadata only; candidate code is never executed/);
  assert.match(labels, /ref: \$\{\{ github\.workflow_sha \}\}/);
  assert.doesNotMatch(labels, /pull_request\.head\.sha/);
  assert.match(labels, /pull-requests: write/);
  assert.doesNotMatch(labels, /issues: write/);
  assert.match(labels, /listLabelsOnIssue/);
  assert.match(labels, /cancel-in-progress: true/);
  assert.match(labels, /ready_for_review, converted_to_draft/);
  assert.match(labels, /DERIVATION_PASSED/);
  assert.match(labels, /desired = passed \?/);
  assert.match(labels, /core\.warning/);
  assert.doesNotMatch(labels, /core\.setFailed/);
});
