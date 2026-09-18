// SPDX-License-Identifier: Apache-2.0

// Contract tests for the audit evidence collector.
//
// Each collector is exercised against a planted known-bad fixture it must flag
// and a known-good fixture it must pass. The pairing is the point: a collector
// tested only against known-bad state passes when it reports everything as
// broken, and one tested only against known-good state passes when it reports
// nothing at all. Both halves must hold or the collector cannot discriminate.
//
// Host-dependent collectors receive a stubbed read-only runner, so the tests
// exercise the real parsing and verdict logic rather than asserting on a mock.

import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";

import {
  COLLECTOR_KEYS,
  collect,
  collectAdrCollisions,
  collectAdvisoryState,
  collectBranchProtection,
  collectCoveragePolicyDrift,
  collectFacadeHeadroom,
  collectGateBranchCoverage,
  collectGateCannotFail,
  collectIssueClosureGap,
  collectMainlineDivergence,
  collectPrCensus,
  resolveBranches,
} from "../../scripts/audit/collect.mjs";

function sandbox() {
  const root = mkdtempSync(path.join(tmpdir(), "oxid-audit-fixture-"));
  test.after?.(() => rmSync(root, { recursive: true, force: true }));
  return root;
}

function plant(root, relative, content) {
  const absolute = path.join(root, relative);
  mkdirSync(path.dirname(absolute), { recursive: true });
  writeFileSync(absolute, content);
  return absolute;
}

/** A runner that answers from a table and throws for anything unlisted. */
function stubRunner(table) {
  return (command, args, options = {}) => {
    const key = `${command} ${args.join(" ")}`;
    for (const [pattern, value] of Object.entries(table)) {
      if (key === pattern || key.startsWith(pattern)) {
        if (value instanceof Error) throw value;
        return typeof value === "function" ? value(options) : value;
      }
    }
    const error = new Error(`unstubbed command: ${key}`);
    error.stderr = "not stubbed";
    throw error;
  };
}

/**
 * Answer `git cat-file --batch` from a `branch:path` map, in git's real wire
 * format, so the collector's parser is exercised rather than bypassed.
 */
function catFileBatch(blobs) {
  return ({ input = "" }) => input
    .split("\n")
    .filter(Boolean)
    .map((reference, index) => {
      const content = blobs[reference];
      if (content === undefined) return `${reference} missing\n`;
      const size = Buffer.byteLength(content, "utf8");
      return `${String(index).padStart(40, "0")} blob ${size}\n${content}\n`;
    })
    .join("");
}

test("the exported collector key list matches the schema's closed property set", () => {
  // Drift here means the collector emits an anchor the schema rejects, or the
  // schema permits one nothing produces.
  const schema = JSON.parse(
    readFileSync(
      path.join(import.meta.dirname, "..", "..", "docs", "factory", "audit", "audit-evidence-v1.schema.json"),
      "utf8",
    ),
  );
  assert.deepEqual([...COLLECTOR_KEYS].sort(), Object.keys(schema.properties.collectors.properties).sort());
});

// --- gate.cannotFail ---------------------------------------------------------

const CRITICAL_SOURCE = `
const CRITICAL_CHECKS = [
  'Validate PR title',
  'Real gate',
];
`;

const LITERAL_STATE_WORKFLOW = `
name: metadata
jobs:
  publish:
    steps:
      - name: Validate title
        id: title
        continue-on-error: true
        run: node policy.mjs
      - name: Publish
        with:
          script: |
            const statuses = [{ context: 'Validate PR title', passed: titlePassed }];
            for (const status of statuses) {
              await github.rest.repos.createCommitStatus({
                sha: head,
                state: 'success',
                context: status.context,
              });
            }
`;

const HONEST_WORKFLOW = `
name: real
jobs:
  gate:
    steps:
      - name: Real gate
        run: ./scripts/real-gate.sh
`;

test("gate.cannotFail flags a critical check published with a literal success state", () => {
  const root = sandbox();
  plant(root, "scripts/github/merge-milestone-pr.mjs", CRITICAL_SOURCE);
  plant(root, ".github/workflows/metadata.yml", LITERAL_STATE_WORKFLOW);
  plant(root, ".github/workflows/real.yml", HONEST_WORKFLOW);

  const result = collectGateCannotFail({ root });
  assert.equal(result.status, "ok");

  const broken = result.facts.find((fact) => fact.context === "Validate PR title");
  assert.equal(broken.canReportFailure, false, "a literal success state must be flagged");
  assert.match(broken.reason, /literal "success"/u);
  assert.match(broken.publishedBy, /metadata\.yml:\d+/u);

  const honest = result.facts.find((fact) => fact.context === "Real gate");
  assert.equal(honest.canReportFailure, true, "a genuine gate must not be flagged");
});

test("gate.cannotFail passes a status whose state is computed rather than literal", () => {
  // The negative case matters as much as the positive one: a collector that
  // flags every advisory step would make the finding worthless. Only a literal
  // state is reported, so the same workflow with a computed state is clean
  // even though its validation step is still continue-on-error.
  const root = sandbox();
  plant(root, "scripts/github/merge-milestone-pr.mjs", "const CRITICAL_CHECKS = ['Validate PR title'];");
  plant(root, ".github/workflows/metadata.yml", LITERAL_STATE_WORKFLOW.replace("state: 'success'", "state: status.passed ? 'success' : 'failure'"));
  const result = collectGateCannotFail({ root });
  assert.equal(result.facts[0].canReportFailure, true);
  assert.equal(result.facts[0].reason, undefined);
});

test("gate.cannotFail looks past a pending seed to the real publishing site", () => {
  // A context is published twice: a `pending` seed near the top and the real
  // status later. Examining only the first occurrence reported every gate as
  // healthy against the real repository, so every occurrence is checked.
  const root = sandbox();
  plant(root, "scripts/github/merge-milestone-pr.mjs", "const CRITICAL_CHECKS = Object.freeze(['Validate PR title']);");
  plant(root, ".github/workflows/metadata.yml", [
    "jobs:",
    "  seed:",
    "    steps:",
    "      - name: Seed pending",
    "        with:",
    "          script: |",
    "            for (const context of ['Validate PR title']) {",
    "              await github.rest.repos.createCommitStatus({",
    "                state: 'pending',",
    "                context,",
    "              });",
    "            }",
    ...Array.from({ length: 40 }, () => "      # filler so the windows do not overlap"),
    "      - name: Publish result",
    "        with:",
    "          script: |",
    "            const statuses = [{ context: 'Validate PR title', passed }];",
    "            for (const status of statuses) {",
    "              await github.rest.repos.createCommitStatus({",
    "                state: 'success',",
    "                context: status.context,",
    "              });",
    "            }",
  ].join("\n"));

  const result = collectGateCannotFail({ root });
  assert.equal(result.facts[0].canReportFailure, false, "the later literal success must be found");
  assert.match(result.facts[0].reason, /literal "success"/u);
  assert.match(result.facts[0].publishedBy, /metadata\.yml:5[0-9]/u, "must point at the real publish, not the seed");
});

test("gate.cannotFail accepts the Object.freeze wrapper around the critical list", () => {
  // The bare `= [...]` form was the only one recognised, so this collector
  // reported `unavailable` against the real repository.
  const root = sandbox();
  plant(root, "scripts/github/merge-milestone-pr.mjs", "export const CRITICAL_CHECKS = Object.freeze([\n  \"scan\",\n]);");
  plant(root, ".github/workflows/real.yml", HONEST_WORKFLOW.replace("Real gate", "scan"));
  const result = collectGateCannotFail({ root });
  assert.equal(result.status, "ok");
  assert.deepEqual(result.facts.map((fact) => fact.context), ["scan"]);
});

test("gate.cannotFail is unavailable rather than empty when the critical list is absent", () => {
  // An absent list and an empty list are different conclusions.
  const result = collectGateCannotFail({ root: sandbox() });
  assert.equal(result.status, "unavailable");
  assert.equal(result.facts, undefined, "an unavailable collector omits facts rather than nulling them");
  assert.match(result.reason, /is absent/u);
});

test("gate.cannotFail is unavailable when the file declares no critical array", () => {
  const root = sandbox();
  plant(root, "scripts/github/merge-milestone-pr.mjs", "const OTHER = [];");
  const result = collectGateCannotFail({ root });
  assert.equal(result.status, "unavailable");
  assert.match(result.reason, /declares no CRITICAL_CHECKS/u);
});

// --- gate.branchCoverage -----------------------------------------------------

test("gate.branchCoverage reports a gate missing from an examined branch", () => {
  const root = sandbox();
  plant(root, ".github/workflows/docs.yml", "on:\n  push:\n    branches:\n      - develop\n      - main\n");
  plant(root, ".github/workflows/all.yml", "on:\n  push:\n    branches: [develop, 'milestone-*']\n");

  const result = collectGateBranchCoverage({
    root,
    branches: { default: "develop", examined: ["develop", "milestone-0.2.0"] },
  });
  assert.equal(result.status, "ok");

  const docs = result.facts.find((fact) => fact.workflow.endsWith("docs.yml"));
  assert.deepEqual(docs.missingFrom, ["milestone-0.2.0"], "a literal list must not cover a train");
  assert.equal(docs.coversDefault, true);

  const all = result.facts.find((fact) => fact.workflow.endsWith("all.yml"));
  assert.deepEqual(all.missingFrom, [], "a milestone-* glob must cover the train");
});

test("gate.branchCoverage degrades rather than claiming full coverage for an unreadable trigger", () => {
  const root = sandbox();
  plant(root, ".github/workflows/odd.yml", "on: workflow_dispatch\n");
  const result = collectGateBranchCoverage({ root, branches: { default: "develop", examined: ["develop"] } });
  assert.equal(result.status, "degraded");
  assert.match(result.reason, /no literal branch filter/u);
});

test("gate.branchCoverage interprets branches-ignore as exclusions", () => {
  const root = sandbox();
  plant(root, ".github/workflows/not-main.yml", "on:\n  push:\n    branches-ignore: [main]\n");
  const result = collectGateBranchCoverage({
    root,
    branches: { default: "develop", examined: ["develop", "main", "feature/x"] },
  });
  assert.equal(result.status, "ok");
  assert.deepEqual(result.facts[0].branchesIgnore, ["main"]);
  assert.equal(result.facts[0].coversDefault, true);
  assert.deepEqual(result.facts[0].missingFrom, ["main"]);
});

// --- coverage.policyDrift ----------------------------------------------------

test("coverage.policyDrift reports an unenforced floor and a documented figure that disagrees", () => {
  const root = sandbox();
  plant(root, "scripts/coverage/policy.json", JSON.stringify({ workspaceFloorPercent: 70, changedLinesFloor: 0 }));
  plant(root, "run.sh", "node scripts/coverage/run.mjs --scope workspace\n");
  plant(root, "CONTRIBUTING.md", "Changes must maintain 80% line coverage.\n");

  const result = collectCoveragePolicyDrift({ root });
  assert.equal(result.status, "ok");
  assert.equal(result.facts.enforced, false, "no --enforce anywhere means the policy is not enforced");
  assert.deepEqual(
    result.facts.scopes.find((scope) => scope.scope === "workspace"),
    { scope: "workspace", hasFloor: true, floorPercent: 70 },
  );
  assert.equal(result.facts.scopes.find((scope) => scope.scope === "changedLines").hasFloor, false);
  assert.deepEqual(result.facts.documentedClaims, [{ path: "CONTRIBUTING.md", line: 1, claimedPercent: 80 }]);
});

test("coverage.policyDrift records enforcement when the flag reaches the runner", () => {
  const root = sandbox();
  plant(root, "scripts/coverage/policy.json", JSON.stringify({ workspaceFloorPercent: 70 }));
  plant(root, "run.sh", "node scripts/coverage/run.mjs --scope workspace --enforce\n");
  const result = collectCoveragePolicyDrift({ root });
  assert.equal(result.facts.enforced, true);
  assert.equal(result.facts.enforcementPath, "run.sh");
});

test("coverage.policyDrift is unavailable when the policy file cannot be parsed", () => {
  const root = sandbox();
  plant(root, "scripts/coverage/policy.json", "{ not json");
  const result = collectCoveragePolicyDrift({ root });
  assert.equal(result.status, "unavailable");
  assert.match(result.reason, /not valid JSON/u);
});

// --- facade.headroom ---------------------------------------------------------

test("facade.headroom reports headroom and the largest ungoverned sibling", () => {
  const root = sandbox();
  plant(root, "scripts/architecture/capability-facades.json", JSON.stringify({
    schemaVersion: 1,
    crates: [{
      name: "demo",
      sourceRoot: "src",
      facadeFiles: ["src/lib.rs"],
      facadeMaximumPhysicalLines: 100,
      facadeMaximumPhysicalLinesByPath: { "src/lib.rs": 100 },
    }],
  }));
  plant(root, "src/lib.rs", `${"x\n".repeat(95)}`);
  plant(root, "src/big_sibling.rs", `${"y\n".repeat(400)}`);
  plant(root, "src/small.rs", "z\n");
  plant(root, "Cargo.toml", 'members = ["src", "other"]\n');

  const result = collectFacadeHeadroom({ root });
  assert.equal(result.status, "ok");
  const [governed] = result.facts.governed;
  assert.equal(governed.actual, 95);
  assert.equal(governed.headroom, 5, "a file five lines under its ceiling has stopped governing");
  assert.deepEqual(governed.largestUngovernedSibling, { path: "src/big_sibling.rs", lines: 400 });
  assert.equal(result.facts.workspaceMemberCount, 2);
});

test("facade.headroom degrades when a governed path is absent from the tree", () => {
  const root = sandbox();
  plant(root, "scripts/architecture/capability-facades.json", JSON.stringify({
    schemaVersion: 1,
    crates: [{ name: "demo", sourceRoot: "src", facadeFiles: ["src/gone.rs"], facadeMaximumPhysicalLines: 10 }],
  }));
  const result = collectFacadeHeadroom({ root });
  assert.equal(result.status, "degraded");
  assert.match(result.reason, /absent from the tree/u);
});

// --- advisory.state ----------------------------------------------------------

test("advisory.state reports an allowlist entry with no review date and an unpinned action", () => {
  const root = sandbox();
  plant(root, "scripts/check-advisories.sh", 'allowed_yanked=(\n  "arrayref@0.3.9"\n)\n');
  plant(root, "docs/security/advisory-exceptions.md", "## arrayref\n\nRetained because the successor pulls a typosquat.\n");
  plant(root, ".github/workflows/ci.yml", "jobs:\n  a:\n    steps:\n      - uses: actions/checkout@v4\n      - uses: actions/cache@55cc8345863c7cc4c66a329aec7e433d2d1c52a9\n");

  const result = collectAdvisoryState({ root, offline: true });
  assert.equal(result.status, "degraded", "an unrun advisory scan must not read as a clean one");
  assert.deepEqual(result.facts.allowlist, [{ entry: "arrayref@0.3.9", hasRationale: true, hasReviewDate: false }]);
  assert.deepEqual(result.facts.unpinnedActions, [{ workflow: ".github/workflows/ci.yml", uses: "actions/checkout@v4" }]);
});

test("advisory.state reads an allowlist whose rationale comments contain parentheses", () => {
  // Terminating the block at the first ")" truncated it before the entries and
  // reported an empty allowlist against the real repository, which reads as
  // "no exceptions" rather than "could not parse".
  const root = sandbox();
  plant(root, "scripts/check-advisories.sh", [
    "allowed_yanked=(",
    "  # #113: arrayref 0.3.5-0.3.9 were yanked, and the replacement adds a",
    '  # dependency created by `dtolney` (display name "David Tolnay",',
    "  # impersonating `dtolnay`) with build-time network access.",
    "  # Review by 2026-12-01.",
    '  "arrayref@0.3.9"',
    ")",
  ].join("\n"));
  const result = collectAdvisoryState({ root, offline: true });
  assert.deepEqual(result.facts.allowlist, [
    { entry: "arrayref@0.3.9", hasRationale: true, hasReviewDate: true },
  ]);
});

test("advisory.state reports an entry whose rationale never names it", () => {
  // Attribution is per entry, not per block: a comment that never names the
  // crate cannot be credited to it, which is what makes the field meaningful
  // when a block holds several entries.
  const root = sandbox();
  plant(root, "scripts/check-advisories.sh", [
    "allowed_yanked=(",
    "  # Kept for now.",
    '  "somecrate@1.2.3"',
    ")",
  ].join("\n"));
  const result = collectAdvisoryState({ root, offline: true });
  assert.deepEqual(result.facts.allowlist, [
    { entry: "somecrate@1.2.3", hasRationale: false, hasReviewDate: false },
  ]);
});

test("advisory.state records a review date when the exception carries one", () => {
  const root = sandbox();
  plant(root, "scripts/check-advisories.sh", 'allowed_yanked=(\n  "arrayref@0.3.9"\n)\n');
  plant(root, "docs/security/advisory-exceptions.md", "## arrayref\n\nRetained because of risk. Review by 2026-12-01.\n");
  const result = collectAdvisoryState({ root, offline: true });
  assert.equal(result.facts.allowlist[0].hasReviewDate, true);
});

test("advisory.state preserves cargo-audit JSON when vulnerabilities make it exit nonzero", () => {
  const root = sandbox();
  const vulnerability = new Error("cargo audit found vulnerabilities");
  vulnerability.stderr = "error: 1 vulnerability found";
  vulnerability.stdout = JSON.stringify({
    vulnerabilities: { count: 1, list: [{ advisory: { id: "RUSTSEC-TEST" } }] },
    warnings: {},
  });
  vulnerability.status = 1;
  const result = collectAdvisoryState({
    root,
    run: stubRunner({ "cargo audit --json": vulnerability }),
  });
  assert.equal(result.status, "ok");
  assert.equal(result.facts.vulnerabilitiesFound, 1);
});

// --- adr.collisions ----------------------------------------------------------

test("adr.collisions finds a duplicate number that no single branch sees", () => {
  // Distinct filenames sharing a number merge without conflict, so a
  // per-branch check finds both corpora clean.
  const run = stubRunner({
    "git ls-tree -r --name-only refs/remotes/origin/develop": "docs/adr/0106-seedless-ux.md",
    "git ls-tree -r --name-only refs/remotes/origin/milestone-0.2.0": "docs/adr/0106-bind-profiles.md",
    "git cat-file --batch": catFileBatch({
      "refs/remotes/origin/develop:docs/adr/0106-seedless-ux.md": "# ADR-0106\n\nStatus: Proposed\n",
      "refs/remotes/origin/milestone-0.2.0:docs/adr/0106-bind-profiles.md": "# ADR-0106\n\nStatus: Proposed\n",
    }),
  });
  const result = collectAdrCollisions({
    branches: ["refs/remotes/origin/develop", "refs/remotes/origin/milestone-0.2.0"],
    run,
  });
  assert.equal(result.status, "ok");
  assert.equal(result.facts.duplicateNumbers.length, 1);
  assert.deepEqual(result.facts.duplicateNumbers[0].paths, ["docs/adr/0106-bind-profiles.md", "docs/adr/0106-seedless-ux.md"]);
  assert.equal(result.facts.mergedCorpusLintFailures, 1);
});

test("adr.collisions flags an accepted record depending on a proposed one", () => {
  const run = stubRunner({
    "git ls-tree -r --name-only refs/remotes/origin/develop": "docs/adr/0025-old.md\ndocs/adr/0106-new.md",
    "git cat-file --batch": catFileBatch({
      "refs/remotes/origin/develop:docs/adr/0025-old.md": "Status: Accepted\n\nAmended by: ADR-0106\n",
      "refs/remotes/origin/develop:docs/adr/0106-new.md": "Status: Proposed\n",
    }),
  });
  const result = collectAdrCollisions({
    branches: ["refs/remotes/origin/develop"], run });
  assert.deepEqual(result.facts.statusBlindBacklinks, [
    { from: "docs/adr/0025-old.md", fromStatus: "Accepted", to: "docs/adr/0106-new.md", toStatus: "Proposed" },
  ]);
});

test("adr.collisions reports a clean corpus as clean", () => {
  const run = stubRunner({
    "git ls-tree -r --name-only refs/remotes/origin/develop": "docs/adr/0025-old.md\ndocs/adr/0106-new.md",
    "git cat-file --batch": catFileBatch({
      "refs/remotes/origin/develop:docs/adr/0025-old.md": "Status: Accepted\n\nAmended by: ADR-0106\n",
      "refs/remotes/origin/develop:docs/adr/0106-new.md": "Status: Accepted\n",
    }),
  });
  const result = collectAdrCollisions({
    branches: ["refs/remotes/origin/develop"], run });
  assert.deepEqual(result.facts.duplicateNumbers, []);
  assert.deepEqual(result.facts.statusBlindBacklinks, []);
  assert.equal(result.facts.mergedCorpusLintFailures, 0);
});

// --- branch.protection -------------------------------------------------------

test("branch.protection distinguishes an unprotected train from a protected default", () => {
  const notProtected = new Error("gh: Branch not protected (HTTP 404)");
  notProtected.stderr = "gh: Branch not protected (HTTP 404)";
  const run = stubRunner({
    "gh api repos/o/r --jq .delete_branch_on_merge": "true",
    "gh api repos/o/r/branches/milestone-0.2.0/protection": notProtected,
    "gh api repos/o/r/rules/branches/milestone-0.2.0": "[]",
    "gh api repos/o/r/branches/develop/protection": JSON.stringify({
      required_status_checks: { contexts: ["basic", "quality"] },
      required_pull_request_reviews: { required_approving_review_count: 0 },
      allow_force_pushes: { enabled: false },
      allow_deletions: { enabled: false },
    }),
    "gh api repos/o/r/rules/branches/develop": "[]",
  });

  const result = collectBranchProtection({ repository: "o/r", branches: ["milestone-0.2.0", "develop"], run });
  assert.equal(result.status, "ok");
  const train = result.facts.find((fact) => fact.branch === "milestone-0.2.0");
  assert.equal(train.protected, false);
  assert.equal(train.rulesetCount, 0);
  assert.equal(train.deleteBranchOnMerge, true, "auto-delete on an unprotected train removes the train on merge");
  const develop = result.facts.find((fact) => fact.branch === "develop");
  assert.equal(develop.protected, true);
  assert.deepEqual(develop.requiredChecks, ["basic", "quality"]);
});

test("branch.protection is unavailable when no branch could be queried", () => {
  const failure = new Error("network down");
  failure.stderr = "network down";
  const run = stubRunner({ "gh api": failure });
  const result = collectBranchProtection({ repository: "o/r", branches: ["develop"], run });
  assert.equal(result.status, "unavailable");
  assert.equal(result.facts, undefined, "an unavailable collector omits facts rather than nulling them");
});

// --- mainline.divergence -----------------------------------------------------

test("mainline.divergence reports paths modified on both sides", () => {
  const run = stubRunner({
    "git merge-base a b": "5ba38b9b00000000000000000000000000000000",
    "git diff --name-only a...b": "x.rs\ny.rs",
    "git rev-list --count 5ba38b9b00000000000000000000000000000000..a": "12",
    "git rev-list --count 5ba38b9b00000000000000000000000000000000..b": "20",
    "git diff --name-only 5ba38b9b00000000000000000000000000000000..a": "shared.rs\nonly-a.rs",
    "git diff --name-only 5ba38b9b00000000000000000000000000000000..b": "shared.rs\nonly-b.rs",
  });
  const result = collectMainlineDivergence({ branches: ["a", "b"], run });
  assert.equal(result.status, "ok");
  assert.equal(result.facts.mergeBase, "5ba38b9b00000000000000000000000000000000");
  const [pair] = result.facts.pairs;
  assert.equal(pair.changedFiles, 3, "changed files is the symmetric union of both branch tips");
  assert.equal(pair.leftOnlyCommits, 12);
  assert.equal(pair.rightOnlyCommits, 20);
  assert.deepEqual(pair.bothSidesModified, ["shared.rs"], "a path touched on both sides auto-merges silently");
});

test("invalid audit window bounds fail before any collector runs", () => {
  assert.throws(
    () => collect({ repository: "o/r", primary: "develop", since: "not-a-date", root: sandbox(), run: stubRunner({}) }),
    /--since must be an ISO-8601 date-time/u,
  );
  assert.throws(
    () => collect({
      repository: "o/r",
      primary: "develop",
      since: "2026-09-02T00:00:00Z",
      until: "2026-09-01T00:00:00Z",
      root: sandbox(),
      run: stubRunner({}),
    }),
    /--since must not be later than --until/u,
  );
});

test("default branch resolution never invents develop after an API failure", () => {
  const failure = new Error("network down");
  failure.stderr = "network down";
  const result = resolveBranches({ repository: "o/r", branches: ["develop"], run: stubRunner({ "gh api": failure }) });
  assert.equal(result.defaultResolved, false);
  assert.equal(result.defaultBranch, null);
});

// --- pr.census ---------------------------------------------------------------

test("pr.census classifies conventional titles and counts approvals", () => {
  const run = stubRunner({
    "gh pr list": JSON.stringify([
      { number: 1, title: "fix(ui): a", baseRefName: "develop", additions: 10, deletions: 2, mergedAt: "2026-09-01T00:00:00Z", reviews: [{ state: "APPROVED" }] },
      { number: 2, title: "feat(wallet): b", baseRefName: "milestone-0.2.0", additions: 90, deletions: 8, mergedAt: "2026-09-02T00:00:00Z", reviews: [] },
      { number: 3, title: "fix(ci): c", baseRefName: "develop", additions: 1, deletions: 1, mergedAt: "2026-07-01T00:00:00Z", reviews: [] },
    ]),
  });
  const result = collectPrCensus({ repository: "o/r", since: "2026-08-01T00:00:00Z", run });
  assert.equal(result.status, "ok");
  assert.equal(result.facts.merged, 2, "the pre-window pull request must be excluded");
  assert.deepEqual(result.facts.byType, { feat: 1, fix: 1 });
  assert.deepEqual(result.facts.byTargetBranch, { develop: 1, "milestone-0.2.0": 1 });
  assert.equal(result.facts.additions, 100);
  assert.equal(result.facts.withApprovingReview, 1);
});

// --- issue.closureGap --------------------------------------------------------

test("issue.closureGap reports an open issue whose fix merged outside the default branch", () => {
  const run = stubRunner({
    "gh pr list": JSON.stringify([
      { number: 351, body: "Closes #95", baseRefName: "milestone-0.2.0", mergedAt: "2026-09-05T00:00:00Z" },
      { number: 400, body: "Fixes #200", baseRefName: "develop", mergedAt: "2026-09-06T00:00:00Z" },
    ]),
    // One listing, not one request per candidate: a per-issue query makes the
    // collector's cost scale with the backlog. #200 is absent, so it is closed.
    "gh issue list": JSON.stringify([{ number: 95 }, { number: 412 }]),
  });
  const result = collectIssueClosureGap({ repository: "o/r", defaultBranch: "develop", run });
  assert.equal(result.status, "ok");
  assert.deepEqual(result.facts, [
    { issue: 95, closedBy: 351, mergedInto: "milestone-0.2.0", stillOpen: true, keywordWouldFire: false },
  ]);
});

test("issue.closureGap refuses to run on an assumed default branch", () => {
  // Every verdict here turns on which branch is default, and a stale premise
  // produces confident, wrong findings faster than no premise at all.
  const result = collectIssueClosureGap({
    repository: "o/r",
    defaultBranch: "develop",
    defaultBranchCollected: false,
    run: stubRunner({}),
  });
  assert.equal(result.status, "unavailable");
  assert.match(result.reason, /default branch could not be collected/u);
});

// --- determinism -------------------------------------------------------------

test("a collector produces identical output across two runs on one fixture", () => {
  const root = sandbox();
  plant(root, "scripts/github/merge-milestone-pr.mjs", CRITICAL_SOURCE);
  plant(root, ".github/workflows/metadata.yml", LITERAL_STATE_WORKFLOW);
  plant(root, ".github/workflows/real.yml", HONEST_WORKFLOW);
  const first = JSON.stringify(collectGateCannotFail({ root }));
  const second = JSON.stringify(collectGateCannotFail({ root }));
  assert.equal(first, second, "evidence must not vary between runs at one anchor");
});

test("directory listings are sorted so output does not depend on filesystem order", () => {
  const root = sandbox();
  plant(root, "scripts/architecture/capability-facades.json", JSON.stringify({
    schemaVersion: 1,
    crates: [{
      name: "demo",
      sourceRoot: "src",
      facadeFiles: ["src/z.rs", "src/a.rs"],
      facadeMaximumPhysicalLines: 10,
    }],
  }));
  plant(root, "src/z.rs", "x\n");
  plant(root, "src/a.rs", "x\n");
  plant(root, "Cargo.toml", 'members = ["src"]\n');
  const result = collectFacadeHeadroom({ root });
  assert.deepEqual(result.facts.governed.map((entry) => entry.path), ["src/a.rs", "src/z.rs"]);
});
