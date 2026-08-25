// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import vm from "node:vm";

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

async function metadataContract() {
  const workflow = await read(".github/workflows/pr-base-check.yml");
  const jobStart = workflow.indexOf("  require-integration-base:");
  assert.ok(jobStart >= 0, "missing integration-base job");
  const job = workflow.slice(jobStart);
  const lines = job.split("\n");
  const start = lines.findIndex((line) => line.includes("PR_BASE_CONTRACT_START"));
  const end = lines.findIndex((line) => line.includes("PR_BASE_CONTRACT_END"));
  assert.ok(start >= 0 && end > start, "missing PR base contract sentinels");
  const sourceLines = lines.slice(start + 1, end).filter((line) => line.trim());
  const indentation = Math.min(...sourceLines.map((line) => line.match(/^ */)[0].length));
  const source = sourceLines.map((line) => line.slice(indentation)).join("\n");
  return new vm.Script(`(async (context, core, github) => {\n${source}\n})`).runInNewContext();
}

async function evaluateMetadata({ body, baseRef, headRef, linkedIssueRepositories = [], graphqlError = false }) {
  const failures = [];
  const execute = await metadataContract();
  await execute(
    {
      repo: { owner: "MediaNoxLabs", repo: "oxid" },
      payload: {
        number: 152,
        pull_request: {
          body,
          head: { ref: headRef, repo: { full_name: "MediaNoxLabs/oxid" } },
          base: { ref: baseRef, repo: { full_name: "MediaNoxLabs/oxid" } },
        },
      },
    },
    { setFailed: (message) => failures.push(message), info: () => {} },
    {
      graphql: async () => {
        if (graphqlError) throw new Error("unavailable");
        return {
          repository: {
            pullRequest: {
              closingIssuesReferences: {
                nodes: linkedIssueRepositories.map((nameWithOwner) => ({ repository: { nameWithOwner } })),
              },
            },
          },
        };
      },
    },
  );
  return failures;
}

for (const workflowPath of [".github/workflows/ci.yml", ".github/workflows/quality.yml", ".github/workflows/scan.yml"]) {
  test(`${workflowPath} runs for integration without retiring migration branches`, async () => {
    const workflow = await read(workflowPath);
    for (const eventName of ["push", "pull_request"]) {
      assert.deepEqual(new Set(eventBranches(workflow, eventName)), new Set(["integration", "develop", "main"]));
    }
  });
}

test("documentation links always emit a context and skip outbound work safely", async () => {
  const links = await read(".github/workflows/docs-link-check.yml");
  assert.equal(eventBranches(links, "pull_request"), null);
  assert.doesNotMatch(eventBlock(links, "pull_request"), /^    paths(?:-ignore)?:/m);
  assert.deepEqual(new Set(eventBranches(links, "push")), new Set(["integration", "develop", "main"]));
  assert.doesNotMatch(eventBlock(links, "push"), /^    paths(?:-ignore)?:/m);
  for (const eventCase of ["workflow_dispatch)", "pull_request)", "push)"]) assert.match(links, new RegExp(eventCase.replace(/[()]/g, "\\$&")));
  for (const safety of [/fetch-depth: 0/, /valid_sha/, /git cat-file -e/, /git merge-base/, /git diff --quiet/, /running the link check conservatively/]) assert.match(links, safety);
  assert.equal((links.match(/if: steps\.changes\.outputs\.docs_changed == 'true'/g) || []).length, 2);
});

test("Pages builds and publishes only from integration", async () => {
  const pages = await read(".github/workflows/pages.yml");
  assert.deepEqual(eventBranches(pages, "push"), ["integration"]);
  assert.doesNotMatch(eventBlock(pages, "push"), /develop|main/);
  assert.match(pages, /if: github\.ref == 'refs\/heads\/integration'/);
});

test("required head-SHA PR contexts survive the base-metadata isolation", async () => {
  assert.equal(eventBranches(await read(".github/workflows/dco.yml"), "pull_request"), null);
  const prCheck = await read(".github/workflows/pr-check.yml");
  assert.equal(eventBranches(prCheck, "pull_request"), null);
  assert.doesNotMatch(prCheck, /^  pull_request_target:/m);

  const baseCheck = await read(".github/workflows/pr-base-check.yml");
  assert.equal(eventBranches(baseCheck, "pull_request_target"), null);
  assert.doesNotMatch(baseCheck, /^  pull_request:/m);
  assert.match(baseCheck, /# zizmor: ignore\[dangerous-triggers\]/);
  assert.equal((baseCheck.match(/^\s+uses:/gm) || []).length, 1);
  assert.match(baseCheck, /^\s+uses: actions\/github-script@[0-9a-f]{40}\b/m);
  assert.doesNotMatch(baseCheck, /actions\/checkout/i);
  assert.doesNotMatch(baseCheck, /^\s*(?:-\s*)?run\s*:/mi);
  assert.doesNotMatch(baseCheck, /^\s*shell\s*:/mi);
  // Forbid all Actions expressions, not only known event fields, so untrusted
  // target-event values cannot later reach action inputs by interpolation.
  assert.doesNotMatch(baseCheck, /\$\{\{/);
});

test("issue-backed integration PR passes the exact metadata contract", async () => {
  assert.deepEqual(await evaluateMetadata({ body: "Closes #144", baseRef: "integration", headRef: "issue-144" }), []);
});

test("non-issue automation and quoted examples remain outside the base contract", async () => {
  assert.deepEqual(await evaluateMetadata({ body: "Dependency refresh", baseRef: "develop", headRef: "renovate/rust" }), []);
  assert.deepEqual(await evaluateMetadata({ body: "<!-- Closes #144 -->\n```text\nFixes #145\n```\n~~~text\nResolves #146\n~~~\n`Closes #147`\n> Fixes #148\n    Closes #149", baseRef: "develop", headRef: "docs" }), []);
});

test("wrong bases fail for bare, local-qualified, and local sidebar references", async () => {
  for (const body of ["Closes: #144", "Closes #144.", "fixed MediaNoxLabs/oxid#144", "RESOLVES https://github.com/MediaNoxLabs/oxid/issues/144"]) {
    const failures = await evaluateMetadata({ body, baseRef: "develop", headRef: "issue-144" });
    assert.equal(failures.length, 1, body);
    assert.match(failures[0], /integration/);
  }
  assert.equal((await evaluateMetadata({ body: "", baseRef: "develop", headRef: "issue-144", linkedIssueRepositories: ["MediaNoxLabs/oxid"] })).length, 1);
});

test("cross-repository and malformed closing references are not local issue metadata", async () => {
  const foreignOrMalformed = [
    "Closes other/repository#144",
    "Fixes https://github.com/other/repository/issues/144",
    "Resolves MediaNoxLabs/oxidation#144",
    "Closes other/MediaNoxLabs/oxid#144",
    "Closes #144suffix",
    "Closes MediaNoxLabs/oxid#144/extra",
  ];
  for (const body of foreignOrMalformed) {
    assert.deepEqual(await evaluateMetadata({ body, baseRef: "develop", headRef: "automation" }), [], body);
  }
  assert.deepEqual(await evaluateMetadata({
    body: "Closes other/repository#144",
    baseRef: "develop",
    headRef: "automation",
    linkedIssueRepositories: ["other/repository", "MEDIANOXLABS/not-oxid"],
  }), []);
  assert.equal((await evaluateMetadata({
    body: "Closes other/repository#144",
    baseRef: "develop",
    headRef: "issue-144",
    linkedIssueRepositories: ["other/repository", "medianoxlabs/OXID"],
  })).length, 1);
});

test("metadata lookup failures fail closed", async () => {
  const failures = await evaluateMetadata({ body: "", baseRef: "develop", headRef: "issue-144", graphqlError: true });
  assert.equal(failures.length, 1);
  assert.match(failures[0], /Could not resolve/);
});

test("every issue-backed non-integration base fails without a promotion exception", async () => {
  for (const { baseRef, headRef } of [
    { baseRef: "main", headRef: "integration" },
    { baseRef: "main", headRef: "feature" },
    { baseRef: "develop", headRef: "integration" },
  ]) {
    const failures = await evaluateMetadata({ body: "Closes #144", baseRef, headRef });
    assert.equal(failures.length, 1, `${headRef} -> ${baseRef}`);
    assert.match(failures[0], /No release-promotion exception is active/);
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
  const siteBuild = await read("scripts/build-docs-site.sh");
  assert.match(siteBuild, /blob\/integration\/docs\/adr/);
  assert.doesNotMatch(siteBuild, /blob\/(?:develop|main)\/docs\/adr/);
  const contract = await read("docs/integration-delivery.md");
  for (const pattern of [/--base origin\/integration/, /--base integration/, /git merge-base HEAD origin\/integration/, /git merge-base --is-ancestor origin\/integration HEAD/, /git merge-tree --write-tree origin\/integration HEAD/, /no `integration -> main` release-promotion exception/i, /separate tracked issue/, /owner ruleset change/, /21481544/, /Pages workflow must trigger and deploy only from\s+`integration`/]) assert.match(contract, pattern);
  assert.doesNotMatch(contract, /^- `Require integration for issue-backed PRs`$/m);
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
  const baseCheck = await read(".github/workflows/pr-base-check.yml");
  assert.ok(baseCheck.split("\n").some((line) => line === "    name: Require integration for issue-backed PRs"));
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
  assert.match(scanJob, /bridgecrewio\/checkov-action@59b9d7edfcad5b87fbe3f473a9a134a721ad03f8/);
  assert.match(scanJob, /skip_checkov_scan: "true"/);
  assert.match(scanJob, /fixtures\/laceid-portal\/76e8edf394a4cb37ca822037272d543c68f25f71\/openid4vci-final\/negative\/unsupported-proof-alg\[\.\]json\$/);
  assert.doesNotMatch(scanJob, /skip_(?:zizmor|gitleaks|opengrep|trivy)_scan:\s*["']?true/i);
  assert.match(config, /maxCopilotRounds: 0/);
  assert.match(config, /Claude CLI/);
  assert.match(config, /current head/i);
  for (const file of ["docs/integration-delivery.md", "docs/factory/runbook.md"]) {
    const guidance = await read(file);
    assert.match(guidance, /manually invoked/i, file);
    assert.match(guidance, /not a hosted GitHub\s+(?:status\s+)?check/i, file);
  }
});
