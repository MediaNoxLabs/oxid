// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  closingIssueNumber,
  parseMergeDevelopArgs,
  validatePrForDevelopMerge,
  validateRequiredChecks,
} from "../../scripts/github/merge-develop-pr.mjs";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const read = (relativePath) => readFile(path.join(repoRoot, relativePath), "utf8");

function eligibleDevelopPr(overrides = {}) {
  return {
    state: "OPEN",
    baseRefName: "develop",
    baseRefOid: "a".repeat(40),
    headRefOid: "b".repeat(40),
    isDraft: false,
    isCrossRepository: false,
    mergeable: "MERGEABLE",
    mergeStateStatus: "CLEAN",
    title: "ci(ci): stabilize delivery",
    body: "Closes #168",
    ...overrides,
  };
}

test("develop merge audit is read-only and execution fails closed", () => {
  assert.throws(
    () => parseMergeDevelopArgs(["--repo", "someone/else", "--pr", "168"]),
    /must be MediaNoxLabs\/oxid/,
  );
  assert.throws(
    () => parseMergeDevelopArgs(["--repo", "MediaNoxLabs/oxid", "--pr", "168", "--execute"]),
    /automated merges to develop are disabled/,
  );
  assert.deepEqual(parseMergeDevelopArgs(["--repo", "MediaNoxLabs/oxid", "--pr", "168"]), {
    help: false,
    repo: "MediaNoxLabs/oxid",
    pr: 168,
    execute: false,
    authorizedByOwner: false,
  });
});

test("only issue-backed develop pull requests are eligible for automated merge", () => {
  assert.equal(closingIssueNumber("Fixes #168"), 168);
  assert.equal(closingIssueNumber("Refs #168"), null);
  assert.equal(validatePrForDevelopMerge(eligibleDevelopPr()).ok, true);
  for (const baseRefName of ["main", "integration"]) {
    const result = validatePrForDevelopMerge(eligibleDevelopPr({ baseRefName }));
    assert.equal(result.ok, false);
    assert.match(result.failures.join("; "), /base must be develop/);
  }
  for (const overrides of [
    { state: "CLOSED" },
    { isDraft: true },
    { isCrossRepository: true },
    { mergeable: "UNKNOWN" },
    { mergeStateStatus: "BEHIND" },
    { title: "WIP: not ready" },
    { body: "Refs #168" },
  ]) assert.equal(validatePrForDevelopMerge(eligibleDevelopPr(overrides)).ok, false);
});

test("required checks include a passing signature and DCO gate", () => {
  const passing = [
    { name: "Verify commit sign-offs", bucket: "pass", state: "SUCCESS" },
    { name: "Repository gate", bucket: "pass", state: "SUCCESS" },
  ];
  assert.equal(validateRequiredChecks(passing).ok, true);
  assert.equal(validateRequiredChecks([]).ok, false);
  assert.equal(validateRequiredChecks(passing.map((check) => ({ ...check, bucket: "pending" }))).ok, false);
  assert.equal(validateRequiredChecks([{ name: "Repository gate", bucket: "pass", state: "SUCCESS" }]).ok, false);
});

test("legacy develop wrapper retains exact-head audit but cannot execute a merge", async () => {
  const source = await read("scripts/github/merge-develop-pr.mjs");
  for (const required of [
    /git[\s\S]*fetch/,
    /merge-base[\s\S]*--is-ancestor/,
    /merge-tree[\s\S]*--write-tree/,
    /pr[\s\S]*checks[\s\S]*--required/,
    /devLoops, "gates"/,
    /gate[\s\S]*detect-evidence/,
  ]) assert.match(source, required);
  assert.doesNotMatch(source, /gh", \["pr", "merge"|--admin|--match-head-commit|--squash/);
});

function eventBlock(workflow, eventName) {
  const lines = workflow.split("\n");
  const start = lines.findIndex((line) => line === `  ${eventName}:`);
  assert.notEqual(start, -1, `missing ${eventName} trigger`);
  let end = lines.length;
  for (let index = start + 1; index < lines.length; index += 1) {
    if (/^  \S/.test(lines[index])) {
      end = index;
      break;
    }
  }
  return lines.slice(start, end).join("\n");
}

function eventBranches(workflow, eventName) {
  const lines = workflow.split("\n");
  const start = lines.findIndex((line) => line === `  ${eventName}:`);
  assert.notEqual(start, -1, `missing ${eventName} trigger`);
  for (let index = start + 1; index < lines.length; index += 1) {
    const line = lines[index];
    if (/^  \S/.test(line)) break;
    const inline = line.match(/^    branches: \[([^\]]+)\]$/);
    if (inline) return inline[1].split(",").map((branch) => branch.trim());
    if (line === "    branches:") {
      const branches = [];
      for (let branchIndex = index + 1; branchIndex < lines.length; branchIndex += 1) {
        const branch = lines[branchIndex].match(/^      - (\S+)$/);
        if (!branch) break;
        branches.push(branch[1]);
      }
      return branches;
    }
  }
  return null;
}

for (const workflowPath of [".github/workflows/ci.yml", ".github/workflows/quality.yml", ".github/workflows/scan.yml"]) {
  test(`${workflowPath} runs only for durable branches`, async () => {
    const workflow = await read(workflowPath);
    for (const eventName of ["push", "pull_request"]) {
      assert.deepEqual(new Set(eventBranches(workflow, eventName)), new Set(["develop", "main"]));
    }
  });
}

test("Scorecard scans durable branch deliveries with its exact context", async () => {
  const scorecard = await read(".github/workflows/scorecard.yml");
  assert.deepEqual(eventBranches(scorecard, "push"), ["develop", "main"]);
  assert.doesNotMatch(eventBlock(scorecard, "push"), /^    paths(?:-ignore)?:/m);
  assert.match(scorecard, /^  workflow_dispatch: \{\}$/m);
  assert.match(scorecard, /^  schedule:\n    - cron: "30 1 \* \* 6"$/m);
  assert.equal((scorecard.match(/^    name: Scorecard analysis$/gm) || []).length, 1);
});

test("documentation links always emit a context and skip outbound work safely", async () => {
  const links = await read(".github/workflows/docs-link-check.yml");
  assert.equal(eventBranches(links, "pull_request"), null);
  assert.doesNotMatch(eventBlock(links, "pull_request"), /^    paths(?:-ignore)?:/m);
  assert.deepEqual(new Set(eventBranches(links, "push")), new Set(["develop", "main"]));
  assert.doesNotMatch(eventBlock(links, "push"), /^    paths(?:-ignore)?:/m);
  for (const eventCase of ["workflow_dispatch)", "pull_request)", "push)"]) assert.match(links, new RegExp(eventCase.replace(/[()]/g, "\\$&")));
  for (const safety of [/fetch-depth: 0/, /valid_sha/, /git cat-file -e/, /git merge-base/, /git diff --quiet/, /running the link check conservatively/]) assert.match(links, safety);
  assert.equal((links.match(/if: steps\.changes\.outputs\.docs_changed == 'true'/g) || []).length, 2);
  assert.match(links, /if \[\[ "\$EVENT_NAME" == "pull_request" \]\]; then\n\s+nix develop \.#docs --command node scripts\/docs\/check-links\.mjs --candidate\n\s+else\n\s+nix develop \.#docs --command node scripts\/docs\/check-links\.mjs\n\s+fi/);
  assert.match(links, /EVENT_NAME: \$\{\{ github\.event_name \}\}/);
  assert.doesNotMatch(links, /--exclude.*blob\/integration/);
});

test("Pages builds and publishes only from main", async () => {
  const pages = await read(".github/workflows/pages.yml");
  assert.deepEqual(eventBranches(pages, "push"), ["main"]);
  assert.doesNotMatch(eventBlock(pages, "push"), /develop|integration/);
  assert.match(pages, /if: github\.ref == 'refs\/heads\/main'/);

  const contract = await read("docs/issue-branch-delivery.md");
  assert.match(contract, /`main` is the stable release branch and GitHub Pages source/);
  assert.match(contract, /Pages is configured\s+for `main`/);
});

test("trusted policy workflows publish required contexts on exact PR head SHAs", async () => {
  const dco = await read(".github/workflows/contribution-commits.yml");
  assert.equal(eventBranches(dco, "pull_request_target"), null);
  const prCheck = await read(".github/workflows/contribution-metadata.yml");
  assert.equal(eventBranches(prCheck, "pull_request_target"), null);
  for (const [workflow, job] of [[dco, "signoff"], [prCheck, "validate"]]) {
    const jobStart = workflow.indexOf(`  ${job}:\n`);
    assert.notEqual(jobStart, -1, `${job} job is present`);
    const jobBlock = workflow.slice(jobStart);
    const topPermissions = workflow.match(/^permissions:\n((?:  [^\n]+\n)+)/m)?.[1] ?? "";
    assert.match(topPermissions, /contents: read/);
    assert.match(topPermissions, /pull-requests: read/);
    assert.doesNotMatch(topPermissions, /write/);
    assert.match(jobBlock, /^    permissions:\n(?:      [^\n]+\n)*      statuses: write$/m);
    assert.match(workflow, /ref: \$\{\{ github\.workflow_sha \}\}/);
    assert.match(workflow, /createCommitStatus/);
    assert.match(workflow, /sha: context\.payload\.pull_request\.head\.sha/);
    assert.doesNotMatch(workflow, /ref: \$\{\{ github\.event\.pull_request\.head\.sha \}\}/);
  }
});

test("branch authority stays in protection settings, not a candidate-controlled workflow", async () => {
  await assert.rejects(read(".github/workflows/pr-base-check.yml"), { code: "ENOENT" });
  const contract = await read("docs/issue-branch-delivery.md");
  assert.match(contract, /Milestone rulesets require commit authenticity/);
  assert.match(contract, /Humans alone\s+merge pull requests to it/);
  assert.match(contract, /`develop`-to-`main` promotion is human-only/);
});

test("dependency automation inherits develop from default-branch authority", async () => {
  const dependabot = await read(".github/dependabot.yml");
  assert.doesNotMatch(dependabot, /^\s+target-branch:/m);

  const renovate = JSON.parse(await read("renovate.json"));
  assert.equal(Object.hasOwn(renovate, "baseBranchPatterns"), false);

  for (const file of ["docs/dependencies/README.md", "docs/issue-branch-delivery.md"]) {
    const guidance = await read(file);
    assert.match(guidance, /default branch/i, file);
    assert.match(guidance, /Dependabot[\s\S]*`target-branch`[\s\S]*(?:disables|changes).*security[- ]update/i, file);
    assert.match(guidance, /develop/i, file);
  }
});

test("repository gate runs architecture and the delivery contract with its declared Node", async () => {
  const gate = await read("run.sh");
  assert.match(gate, /\.\/scripts\/check-architecture\.sh/);
  assert.match(gate, /node --test tests\/repository\/contribution-policy-contract\.test\.mjs/);
  assert.match(gate, /node --test tests\/repository\/integration-delivery-contract\.test\.mjs/);
  const shells = await read("nix/devshells/default.nix");
  assert.match(shells, /nodejs_24/);
  assert.match(shells, /ciRustPackages[\s\S]*ripgrep/);
});

test("guidance, required contexts, and review configuration agree", async () => {
  const guidancePatterns = {
    "AGENT.md": /explicit delivery base/,
    "CONTRIBUTING.md": /explicit delivery base/,
    ".github/pull_request_template.md": /Delivery target: `milestone-x\.y\.z`/,
    "docs/site/src/contributing.md": /exact target\s+recorded on the issue/,
  };
  for (const [file, pattern] of Object.entries(guidancePatterns)) assert.match(await read(file), pattern, file);
  const branchAuthorityFiles = [
    "AGENT.md",
    "CONTRIBUTING.md",
    ".github/pull_request_template.md",
    "docs/issue-branch-delivery.md",
    "docs/factory/productive-loop.md",
    "docs/factory/runbook.md",
  ];
  for (const file of branchAuthorityFiles) {
    const content = await read(file);
    assert.doesNotMatch(
      content,
      /(?:origin\/integration|--base integration|refs\/heads\/integration|merge-integration-pr)/i,
      file,
    );
  }
  const routedSurfaces = {
    "README.md": /badge\.svg\?branch=develop/,
    "SECURITY.md": /latest commit on `develop` receives\s+security fixes/,
    "docs/site/book.toml": /edit\/develop\/docs\/site\/\{path\}/,
    "docs/factory/charter.md": /Review active milestone and `develop` deltas on a schedule/,
    "docs/site/src/agent-process.md": /reviews\s+`develop` deltas on a schedule/,
  };
  for (const [file, pattern] of Object.entries(routedSurfaces)) {
    const content = await read(file);
    assert.match(content, pattern, file);
  }
  const siteBuild = `${await read("scripts/build-docs-site.sh")}\n${await read("scripts/docs/generate-adr-catalog.mjs")}`;
  assert.match(siteBuild, /blob\/develop\/docs\/adr/);
  assert.doesNotMatch(siteBuild, /blob\/integration\/docs\/adr/);
  const contract = await read("docs/issue-branch-delivery.md");
  for (const pattern of [/default branch/, /explicit base/, /milestone-<x\.y\.z>/, /git merge-base HEAD "\$delivery_base"/, /stable-enough shared engineering baseline/, /`main` is the stable release branch/, /Pages source/, /temporary branch/i]) assert.match(contract, pattern);
  const expectedNames = {
    "ci.yml": ["Repository gate (fmt, architecture, lint, tests, coverage)", "Locked Nix package and Compact artifacts"],
    "quality.yml": ["Audit, Licenses, Sources, and Documentation"],
    "docs-link-check.yml": ["Check documentation links"],
  };
  for (const [file, names] of Object.entries(expectedNames)) {
    const workflow = await read(`.github/workflows/${file}`);
    for (const name of names) assert.ok(workflow.split("\n").some((line) => line === `    name: ${name}`), `${file}: ${name}`);
  }
  const contributionContexts = `${await read(".github/workflows/contribution-metadata.yml")}\n${await read(".github/workflows/contribution-commits.yml")}`;
  for (const context of ["Validate PR title", "Validate PR body", "Verify commit sign-offs"]) {
    assert.match(contributionContexts, new RegExp(`context: '${context}'`), context);
  }
  const prCheck = await read(".github/workflows/contribution-metadata.yml");
  assert.match(prCheck, /actions\/checkout@[0-9a-f]{40}/);
  assert.match(prCheck, /Checkout trusted contribution policy/);
  assert.match(prCheck, /persist-credentials: false/);
  const config = await read(".devloops");
  const draftGate = config.slice(config.indexOf("  draft:"), config.indexOf("  preApproval:"));
  const preApprovalGate = config.slice(config.indexOf("  preApproval:"), config.indexOf("  requireFanoutEvidence:"));
  assert.doesNotMatch(draftGate, /^      - external-review$/m);
  assert.doesNotMatch(preApprovalGate, /^      - external-review$/m);
  assert.match(draftGate, /^    requireCi: false$/m);
  assert.match(config, /^  fanOut: 2$/m);
  assert.match(config, /^  stopOnLowSignal: true$/m);
  assert.match(config, /^  maxFanoutReviewers: 2$/m);
  assert.match(config, /^  requireFanoutEvidence: false$/m);
  assert.match(config, /^  requireFanoutProvenance: false$/m);
  const scan = await read(".github/workflows/scan.yml");
  const scanJobStart = scan.indexOf("  scan:");
  assert.ok(scanJobStart >= 0, "scan.yml: scan job");
  const scanJob = scan.slice(scanJobStart);
  assert.match(scanJob, /^    name: scan$/m);
  assert.doesNotMatch(scanJob, /^    strategy:|\bmatrix\b/m);
  for (const line of scanJob.split("\n").filter((candidate) => /^\s+uses:/.test(candidate))) {
    assert.match(line, /@[0-9a-f]{40}\b/, line);
  }
  assert.match(scanJob, /midnightntwrk\/upload-sarif-github-action@4bbe849e9707b46342832d4b7f94fec585823ca4/);
  assert.match(scanJob, /Run scanners and upload SARIF/);
  assert.equal((scanJob.match(/if: always\(\)/g) || []).length, 3);
  assert.equal((scanJob.match(/continue-on-error: true/g) || []).length, 1);
  assert.match(scanJob, /STAGE_OUTCOME: \$\{\{ steps\.stage-checkov-exclusion\.outcome \}\}/);
  assert.match(scanJob, /SCAN_OUTCOME: \$\{\{ steps\.security-scan\.outcome \}\}/);
  assert.match(scanJob, /RESTORE_OUTCOME: \$\{\{ steps\.restore-checkov-exclusion\.outcome \}\}/);
  assert.match(scanJob, /Aggregate scanner and fixture results/);
  assert.equal((scanJob.match(/fixtures\/laceid-portal\/76e8edf394a4cb37ca822037272d543c68f25f71\/openid4vci-final\/negative\/unsupported-proof-alg\.json/g) || []).length, 2);
  assert.doesNotMatch(scanJob, /skip_checkov_scan:/);
  assert.doesNotMatch(scanJob, /^\s+skip_(?:check|framework):/m);
  assert.doesNotMatch(scanJob, /\bsoft_fail:/);
  assert.doesNotMatch(scanJob, /skip_(?:zizmor|gitleaks|opengrep|trivy)_scan:\s*["']?true/i);
  assert.match(config, /maxCopilotRounds: 0/);
  assert.match(config, /^  stopAt: \[\]$/m);
  assert.match(config, /^  humanMergeOnly: true$/m);
  assert.match(config, /^  requireRetrospective: false$/m);
  assert.match(config, /^  maxParallel: 1$/m);
  assert.doesNotMatch(config, /humanHandoff|candidatesFrom:\s*\n\s*- codeowners/);
  const [ci, ciShells, sccacheRunner] = await Promise.all([
    read(".github/workflows/ci.yml"),
    read("nix/devshells/default.nix"),
    read("scripts/ci/run-with-sccache-stats.sh"),
  ]);
  assert.match(ci, /scripts\/ci\/target-plan\.mjs/);
  assert.match(ci, /^  plan:$/m);
  assert.match(ci, /^  basic:$/m);
  assert.match(ci, /^  unit_linux:$/m);
  assert.match(ci, /^  headless_linux:$/m);
  assert.match(ci, /^  ui_linux:$/m);
  assert.match(ci, /^  ui_release_linux:$/m);
  assert.match(ci, /^  coverage_linux:$/m);
  assert.match(ci, /^  repository_gate:$/m);
  assert.match(ci, /^  locked_nix_gate:$/m);
  assert.match(ci, /name: Basic gate \(policy, lint, compile\)[\s\S]*?timeout-minutes: 5/);
  assert.match(ci, /name: Unit tests \(Linux host\)[\s\S]*?timeout-minutes: 12/);
  assert.match(ci, /name: Headless integration tests \(Linux host\)[\s\S]*?timeout-minutes: 10/);
  assert.match(ci, /name: Optimized UI release artifact \(Linux host\)[\s\S]*?timeout-minutes: 25/);
  assert.match(ci, /nix develop \.#ci-rust --command \.\/scripts\/ci\/run-with-sccache-stats\.sh \.\/run\.sh basic --strict/);
  assert.match(ci, /nix develop \.#ci-rust --command \.\/scripts\/ci\/run-with-sccache-stats\.sh \.\/run\.sh unit --strict/);
  assert.match(ci, /nix develop \.#ci-rust --command \.\/scripts\/ci\/run-with-sccache-stats\.sh \.\/run\.sh headless-integration --strict/);
  assert.match(ci, /nix develop \.#ci-ui --command \.\/scripts\/ci\/run-with-sccache-stats\.sh \.\/run\.sh ui --strict/);
  assert.match(ci, /nix develop \.#ci-ui --command \.\/scripts\/ci\/run-with-sccache-stats\.sh \.\/run\.sh ui-release --strict/);
  assert.match(ci, /nix develop \.#ci-coverage --command \.\/scripts\/ci\/run-with-sccache-stats\.sh \.\/run\.sh coverage --strict/);
  assert.doesNotMatch(ci, /run: nix develop --command \.\/run\.sh (?:basic|unit|headless-integration|ui|ui-release|coverage)/);
  assert.equal((ci.match(/name: Configure object-level compiler cache/g) || []).length, 6);
  assert.equal((ci.match(/core\.exportVariable\('SCCACHE_GHA_ENABLED', 'on'\)/g) || []).length, 6);
  assert.equal((ci.match(/core\.exportVariable\('SCCACHE_GHA_RW_MODE', 'READ_ONLY'\)/g) || []).length, 5);
  const unitJob = ci.slice(
    ci.indexOf("\n  unit_linux:\n    name:"),
    ci.indexOf("\n  headless_linux:\n    name:"),
  );
  assert.match(unitJob, /trustedDevelopPush[\s\S]*SCCACHE_GHA_RW_MODE[\s\S]*READ_WRITE[\s\S]*READ_ONLY/);
  assert.match(ciShells, /export CARGO_INCREMENTAL="''\$\{CARGO_INCREMENTAL:-0\}"/);
  assert.match(ciShells, /devShells\.ci-quality = pkgs\.mkShell/);
  assert.match(sccacheRunner, /"\$@" \|\| command_status=\$\?/);
  assert.match(sccacheRunner, /sccache --show-stats \|\| true/);
  assert.match(sccacheRunner, /write-error counters are expected for rejected local puts/);
  assert.doesNotMatch(ci, /path: ~\/\.cache\/oxid-sccache/);
  assert.doesNotMatch(ci, /key: sccache-/);
  assert.match(ci, /save: \$\{\{ github\.event_name == 'push' && github\.ref == 'refs\/heads\/develop' \}\}/);
  assert.match(ci, /if: always\(\)[\s\S]*?needs: \[plan, basic, unit_linux, headless_linux, ui_linux, ui_release_linux, coverage_linux\]/);
  assert.doesNotMatch(ci, /Run full repository gate/);
  assert.doesNotMatch(ci, /^\s+target$/m);
  const quality = await read(".github/workflows/quality.yml");
  assert.match(quality, /nix develop \.#ci-quality --command \.\/run\.sh quality --strict/);
  assert.doesNotMatch(quality, /cache-nix-action|nix7-devshell/);
  const contractReviewPolicy = await read("docs/issue-branch-delivery.md");
  assert.match(contractReviewPolicy, /critical set/);
  assert.match(contractReviewPolicy, /follow-up issue/);
  assert.match(contractReviewPolicy, /milestone-to-`develop` promotion PR is human-only/);
  assert.match(contractReviewPolicy, /milestone guard/);
  for (const file of ["docs/factory/runbook.md"]) {
    const guidance = await read(file);
    assert.match(guidance, /manually invoked/i, file);
    assert.match(guidance, /not a hosted GitHub\s+(?:status\s+)?check/i, file);
    assert.match(guidance, /high-risk/i, file);
  }
});
