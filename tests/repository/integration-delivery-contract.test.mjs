// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import vm from "node:vm";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const read = (relativePath) => readFile(path.join(repoRoot, relativePath), "utf8");

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
  const workflow = await read(".github/workflows/pr-check.yml");
  const jobStart = workflow.indexOf("  require-integration-base:");
  const jobEnd = workflow.indexOf("\n  validate-pr-title:", jobStart);
  assert.ok(jobStart >= 0 && jobEnd > jobStart, "missing integration-base job");
  const job = workflow.slice(jobStart, jobEnd);
  const lines = job.split("\n");
  const start = lines.findIndex((line) => line.includes("PR_BASE_CONTRACT_START"));
  const end = lines.findIndex((line) => line.includes("PR_BASE_CONTRACT_END"));
  assert.ok(start >= 0 && end > start, "missing PR base contract sentinels");
  const sourceLines = lines.slice(start + 1, end).filter((line) => line.trim());
  const indentation = Math.min(...sourceLines.map((line) => line.match(/^ */)[0].length));
  const source = sourceLines.map((line) => line.slice(indentation)).join("\n");
  return new vm.Script(`(async (context, core, github) => {\n${source}\n})`).runInNewContext();
}

async function evaluateMetadata({ body, baseRef, headRef, linkedIssues = 0 }) {
  const failures = [];
  const execute = await metadataContract();
  await execute(
    { repo: { owner: "MediaNoxLabs", repo: "oxid" }, payload: { number: 152, pull_request: { body, base: { ref: baseRef }, head: { ref: headRef } } } },
    { setFailed: (message) => failures.push(message), info: () => {} },
    { graphql: async () => ({ repository: { pullRequest: { closingIssuesReferences: { nodes: Array.from({ length: linkedIssues }, (_, id) => ({ id })) } } } }) },
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

test("documentation workflows cover integration", async () => {
  const links = await read(".github/workflows/docs-link-check.yml");
  assert.equal(eventBranches(links, "pull_request"), null);
  assert.deepEqual(new Set(eventBranches(links, "push")), new Set(["integration", "develop", "main"]));
  const pages = await read(".github/workflows/pages.yml");
  assert.deepEqual(new Set(eventBranches(pages, "push")), new Set(["integration", "develop"]));
  assert.match(pages, /if: github\.ref == 'refs\/heads\/integration'/);
});

test("DCO and base-owned PR metadata cover every PR base", async () => {
  assert.equal(eventBranches(await read(".github/workflows/dco.yml"), "pull_request"), null);
  assert.equal(eventBranches(await read(".github/workflows/pr-check.yml"), "pull_request_target"), null);
});

test("issue-backed integration PR passes the exact metadata contract", async () => {
  assert.deepEqual(await evaluateMetadata({ body: "Closes #144", baseRef: "integration", headRef: "issue-144" }), []);
});

test("non-issue automation and quoted examples remain outside the base contract", async () => {
  assert.deepEqual(await evaluateMetadata({ body: "Dependency refresh", baseRef: "develop", headRef: "renovate/rust" }), []);
  assert.deepEqual(await evaluateMetadata({ body: "<!-- Closes #144 -->\n```text\nFixes #145\n```", baseRef: "develop", headRef: "docs" }), []);
});

test("wrong bases fail for closing keywords and sidebar-linked issues", async () => {
  for (const body of ["Closes: #144", "Closes #144", "fixed MediaNoxLabs/oxid#144", "RESOLVES https://github.com/MediaNoxLabs/oxid/issues/144"]) {
    const failures = await evaluateMetadata({ body, baseRef: "develop", headRef: "issue-144" });
    assert.equal(failures.length, 1, body);
    assert.match(failures[0], /integration/);
  }
  assert.equal((await evaluateMetadata({ body: "", baseRef: "develop", headRef: "issue-144", linkedIssues: 1 })).length, 1);
});

test("only integration-to-main is an issue-backed release-promotion exception", async () => {
  assert.deepEqual(await evaluateMetadata({ body: "Closes #144", baseRef: "main", headRef: "integration" }), []);
  assert.equal((await evaluateMetadata({ body: "Closes #144", baseRef: "main", headRef: "feature" })).length, 1);
});

test("repository gate runs architecture and the delivery contract with its declared Node", async () => {
  const gate = await read("run.sh");
  assert.match(gate, /\.\/scripts\/check-architecture\.sh/);
  assert.match(gate, /node --test tests\/repository\/integration-delivery-contract\.test\.mjs/);
  assert.match(await read("nix/devshells/default.nix"), /nodejs_24/);
});

test("guidance, required contexts, and review configuration agree", async () => {
  for (const file of ["AGENT.md", "CONTRIBUTING.md", ".github/pull_request_template.md", "docs/site/src/contributing.md"]) assert.match(await read(file), /integration/, file);
  const contract = await read("docs/integration-delivery.md");
  for (const pattern of [/--base origin\/integration/, /--base integration/, /git merge-base HEAD origin\/integration/, /git merge-base --is-ancestor origin\/integration HEAD/, /git merge-tree --write-tree origin\/integration HEAD/, /integration -> main/]) assert.match(contract, pattern);
  const workflowNames = ["pr-check.yml", "dco.yml", "ci.yml", "quality.yml", "scan.yml", "docs-link-check.yml"];
  const workflows = await Promise.all(workflowNames.map((name) => read(`.github/workflows/${name}`)));
  for (const name of ["Require integration for issue-backed PRs", "Verify commit sign-offs", "Validate PR title", "Validate PR body", "Repository gate (fmt, architecture, lint, tests, coverage)", "Locked Nix package and Compact artifacts", "Audit, Licenses, Sources, and Documentation", "Check documentation links"]) assert.ok(workflows.some((workflow) => workflow.includes(`name: ${name}`)), name);
  assert.match(workflows[4], /jobs:\n  scan:/);
  const config = await read(".devloops");
  assert.match(config, /maxCopilotRounds: 0/);
  assert.match(config, /Claude CLI/);
  assert.match(config, /current head/i);
});
