// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const read = (relativePath) => readFile(path.join(repoRoot, relativePath), "utf8");

async function markdownFilesUnder(relativeRoot) {
  return (await readdir(path.join(repoRoot, relativeRoot), { recursive: true }))
    .filter((file) => file.endsWith(".md"))
    .map((file) => path.join(relativeRoot, file));
}

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
  test(`${workflowPath} runs for integration without retiring migration branches`, async () => {
    const workflow = await read(workflowPath);
    for (const eventName of ["push", "pull_request"]) {
      assert.deepEqual(new Set(eventBranches(workflow, eventName)), new Set(["integration", "develop", "main"]));
    }
  });
}

test("Scorecard scans each integration delivery with its exact context", async () => {
  const scorecard = await read(".github/workflows/scorecard.yml");
  assert.deepEqual(eventBranches(scorecard, "push"), ["integration", "main"]);
  assert.doesNotMatch(eventBlock(scorecard, "push"), /^    paths(?:-ignore)?:/m);
  assert.match(scorecard, /^  workflow_dispatch: \{\}$/m);
  assert.match(scorecard, /^  schedule:\n    - cron: "30 1 \* \* 6"$/m);
  assert.equal((scorecard.match(/^    name: Scorecard analysis$/gm) || []).length, 1);
});

test("documentation links always emit a context and skip outbound work safely", async () => {
  const links = await read(".github/workflows/docs-link-check.yml");
  assert.equal(eventBranches(links, "pull_request"), null);
  assert.doesNotMatch(eventBlock(links, "pull_request"), /^    paths(?:-ignore)?:/m);
  assert.deepEqual(new Set(eventBranches(links, "push")), new Set(["integration", "develop", "main"]));
  assert.doesNotMatch(eventBlock(links, "push"), /^    paths(?:-ignore)?:/m);
  for (const eventCase of ["workflow_dispatch)", "pull_request)", "push)"]) assert.match(links, new RegExp(eventCase.replace(/[()]/g, "\\$&")));
  for (const safety of [/fetch-depth: 0/, /valid_sha/, /git cat-file -e/, /git merge-base/, /git diff --quiet/, /running the link check conservatively/]) assert.match(links, safety);
  assert.equal((links.match(/if: steps\.changes\.outputs\.docs_changed == 'true'/g) || []).length, 2);
  assert.match(links, /nix develop \.#docs --command node scripts\/docs\/check-links\.mjs/);
  assert.doesNotMatch(links, /--exclude.*blob\/integration/);
});

test("Pages builds and publishes only from integration", async () => {
  const pages = await read(".github/workflows/pages.yml");
  assert.deepEqual(eventBranches(pages, "push"), ["integration"]);
  assert.doesNotMatch(eventBlock(pages, "push"), /develop|main/);
  assert.match(pages, /if: github\.ref == 'refs\/heads\/integration'/);

  const contract = await read("docs/integration-delivery.md");
  assert.match(contract, /`github-pages` environment/);
  assert.match(contract, /policy `58259903`/);
  assert.match(contract, /only allowed branch is\s+`integration`/);
});

test("required PR contexts remain attached to pull_request head SHAs", async () => {
  assert.equal(eventBranches(await read(".github/workflows/dco.yml"), "pull_request"), null);
  const prCheck = await read(".github/workflows/pr-check.yml");
  assert.equal(eventBranches(prCheck, "pull_request"), null);
  assert.doesNotMatch(prCheck, /^  pull_request_target:/m);
});

test("cross-base authority stays in the owner ruleset, not a dangerous advisory workflow", async () => {
  await assert.rejects(read(".github/workflows/pr-base-check.yml"), { code: "ENOENT" });
  const contract = await read("docs/integration-delivery.md");
  assert.match(contract, /ruleset `21481544` is the\s+cross-base authority/);
  assert.match(contract, /workflows deliberately make no cross-base enforcement claim/);
  assert.match(contract, /false failures for stacked pull requests/);
});

test("dependency automation inherits integration from default-branch authority", async () => {
  const dependabot = await read(".github/dependabot.yml");
  assert.doesNotMatch(dependabot, /^\s+target-branch:/m);

  const renovate = JSON.parse(await read("renovate.json"));
  assert.equal(Object.hasOwn(renovate, "baseBranchPatterns"), false);

  for (const file of ["docs/dependencies/README.md", "docs/integration-delivery.md"]) {
    const guidance = await read(file);
    assert.match(guidance, /default branch/i, file);
    assert.match(guidance, /Dependabot[\s\S]*`target-branch`[\s\S]*disables security updates/i, file);
    for (const pr of ["#138", "#139"]) assert.match(guidance, new RegExp(pr), file);
    assert.match(guidance, /stale/i, file);
    assert.match(guidance, /close\s+(?:them|those stale)/i, file);
    assert.match(guidance, /recreate/i, file);
  }
});

test("repository gate runs architecture and the delivery contract with its declared Node", async () => {
  const gate = await read("run.sh");
  assert.match(gate, /\.\/scripts\/check-architecture\.sh/);
  assert.match(gate, /node --test tests\/repository\/integration-delivery-contract\.test\.mjs/);
  assert.match(await read("nix/devshells/default.nix"), /nodejs_24/);
});

test("guidance, required contexts, and review configuration agree", async () => {
  const guidancePatterns = {
    "AGENT.md": /only writable delivery and Pages publishing branch/,
    "CONTRIBUTING.md": /only writable delivery branch and the sole Pages publishing/,
    ".github/pull_request_template.md": /historical `main` and `develop` are read-only/,
    "docs/site/src/contributing.md": /only writable delivery and\s+Pages publishing branch/,
  };
  for (const [file, pattern] of Object.entries(guidancePatterns)) assert.match(await read(file), pattern, file);
  const documentationFiles = [
    "AGENT.md",
    "CHANGELOG.md",
    "CONTRIBUTING.md",
    "OXID_IDENTITY_WALLET_BLUEPRINT.md",
    "README.md",
    "SECURITY.md",
    "SUPPORT.md",
    ".github/pull_request_template.md",
    ...(await markdownFilesUnder("docs")),
  ];
  for (const file of documentationFiles) {
    const content = await read(file);
    assert.doesNotMatch(
      content,
      /https?:\/\/(?:www\.)?github\.com\/MediaNoxLabs\/oxid\/(?:blob|tree)\/(?:develop|main)(?:\/|\b)/i,
      file,
    );
    assert.doesNotMatch(
      content,
      /\b(?:base|target(?:s|ed|ing)?|against)\s+(?:branch\s+)?["'`]*(?:develop|main)["'`]*\b/i,
      file,
    );
  }
  const routedSurfaces = {
    "README.md": /badge\.svg\?branch=integration/,
    "SECURITY.md": /latest commit on `integration` receives\s+security fixes/,
    "docs/site/book.toml": /edit\/integration\/docs\/site\/\{path\}/,
    "docs/factory/charter.md": /Review `integration` deltas on a schedule/,
    "docs/site/src/agent-process.md": /reviews\s+`integration` deltas on a schedule/,
    "docs/migration/delivery-audit-2026-08-20.md": /fetch and verify signed `integration`/,
  };
  for (const [file, pattern] of Object.entries(routedSurfaces)) {
    const content = await read(file);
    assert.match(content, pattern, file);
  }
  const siteBuild = `${await read("scripts/build-docs-site.sh")}\n${await read("scripts/docs/generate-adr-catalog.mjs")}`;
  assert.match(siteBuild, /blob\/integration\/docs\/adr/);
  assert.doesNotMatch(siteBuild, /blob\/(?:develop|main)\/docs\/adr/);
  const contract = await read("docs/integration-delivery.md");
  for (const pattern of [/default branch/, /--base origin\/integration/, /--base integration/, /git merge-base HEAD origin\/integration/, /git merge-base --is-ancestor origin\/integration HEAD/, /git merge-tree --write-tree origin\/integration HEAD/, /no `integration -> main` release-promotion exception/i, /separate tracked issue/, /owner ruleset change/, /21481544/, /Pages workflow must trigger and deploy only from\s+`integration`/]) assert.match(contract, pattern);
  const expectedNames = {
    "pr-check.yml": ["Validate PR title", "Validate PR body"],
    "dco.yml": ["Verify commit sign-offs"],
    "ci.yml": ["Repository gate (fmt, architecture, lint, tests, coverage)", "Locked Nix package and Compact artifacts"],
    "quality.yml": ["Audit, Licenses, Sources, and Documentation"],
    "docs-link-check.yml": ["Check documentation links"],
  };
  for (const [file, names] of Object.entries(expectedNames)) {
    const workflow = await read(`.github/workflows/${file}`);
    for (const name of names) assert.ok(workflow.split("\n").some((line) => line === `    name: ${name}`), `${file}: ${name}`);
  }
  const prCheck = await read(".github/workflows/pr-check.yml");
  assert.doesNotMatch(prCheck, /actions\/checkout/);
  const config = await read(".devloops");
  const draftGate = config.slice(config.indexOf("  draft:"), config.indexOf("  preApproval:"));
  const preApprovalGate = config.slice(config.indexOf("  preApproval:"), config.indexOf("  requireFanoutEvidence:"));
  assert.match(draftGate, /^      - external-review$/m);
  assert.match(preApprovalGate, /^      - external-review$/m);
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
  assert.equal((scanJob.match(/continue-on-error: true/g) || []).length, 3);
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
  assert.match(config, /Claude CLI/);
  assert.match(config, /current head/i);
  assert.match(config, /^  stopAt: \[\]$/m);
  assert.match(config, /^  humanMergeOnly: false$/m);
  assert.doesNotMatch(config, /humanHandoff|candidatesFrom:\s*\n\s*- codeowners/);
  const contractReviewPolicy = await read("docs/integration-delivery.md");
  assert.match(contractReviewPolicy, /required_approving_review_count: 0/);
  assert.match(contractReviewPolicy, /require_code_owner_reviews: false/);
  assert.match(contractReviewPolicy, /Post the\s+current-head evidence to the pull request before merge/);
  for (const file of ["docs/integration-delivery.md", "docs/factory/runbook.md"]) {
    const guidance = await read(file);
    assert.match(guidance, /manually invoked/i, file);
    assert.match(guidance, /not a hosted GitHub\s+(?:status\s+)?check/i, file);
    assert.doesNotMatch(guidance, /humanMergeOnly: true|approval\.humanHandoff/);
  }
});
