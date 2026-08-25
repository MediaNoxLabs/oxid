// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import vm from "node:vm";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

async function read(relativePath) {
  return readFile(path.join(repoRoot, relativePath), "utf8");
}

function eventBranches(workflow, eventName) {
  const lines = workflow.split("\n");
  const start = lines.findIndex((line) => line === `  ${eventName}:`);
  assert.notEqual(start, -1, `missing ${eventName} trigger`);

  for (let index = start + 1; index < lines.length; index += 1) {
    const line = lines[index];
    if (/^  \S/.test(line)) break;
    const match = line.match(/^    branches: \[([^\]]+)\]$/);
    if (match) return match[1].split(",").map((branch) => branch.trim());
  }
  return null;
}

async function metadataContract() {
  const workflow = await read(".github/workflows/pr-check.yml");
  const lines = workflow.split("\n");
  const start = lines.findIndex((line) => line.includes("PR_BASE_CONTRACT_START"));
  const end = lines.findIndex((line) => line.includes("PR_BASE_CONTRACT_END"));
  assert.ok(start >= 0 && end > start, "missing PR base contract sentinels");
  const sourceLines = lines.slice(start + 1, end).filter((line) => line.trim());
  const indentation = Math.min(...sourceLines.map((line) => line.match(/^ */)[0].length));
  const source = sourceLines.map((line) => line.slice(indentation)).join("\n");
  return new vm.Script(`(async (context, core) => {\n${source}\n})`).runInNewContext();
}

async function evaluateMetadata({ body, baseRef, headRef }) {
  const failures = [];
  const execute = await metadataContract();
  await execute(
    { payload: { pull_request: { body, base: { ref: baseRef }, head: { ref: headRef } } } },
    { setFailed: (message) => failures.push(message), info: () => {} },
  );
  return failures;
}

for (const workflowPath of [
  ".github/workflows/ci.yml",
  ".github/workflows/quality.yml",
  ".github/workflows/scan.yml",
]) {
  test(`${workflowPath} runs for integration without retiring migration branches`, async () => {
    const workflow = await read(workflowPath);
    for (const eventName of ["push", "pull_request"]) {
      const branches = eventBranches(workflow, eventName);
      assert.deepEqual(new Set(branches), new Set(["integration", "develop", "main"]));
    }
  });
}

test("documentation workflows cover integration", async () => {
  const links = await read(".github/workflows/docs-link-check.yml");
  assert.equal(eventBranches(links, "pull_request"), null, "documentation PR check must not be branch-filtered");
  assert.deepEqual(new Set(eventBranches(links, "push")), new Set(["integration", "develop", "main"]));

  const pages = await read(".github/workflows/pages.yml");
  assert.deepEqual(new Set(eventBranches(pages, "push")), new Set(["integration", "develop"]));
});

test("DCO and PR metadata workflows cover every PR base", async () => {
  assert.equal(eventBranches(await read(".github/workflows/dco.yml"), "pull_request"), null);
  assert.equal(eventBranches(await read(".github/workflows/pr-check.yml"), "pull_request"), null);
});

test("issue-backed integration PR passes the exact metadata contract", async () => {
  assert.deepEqual(await evaluateMetadata({ body: "Closes #144", baseRef: "integration", headRef: "issue-144" }), []);
});

test("non-issue automation remains outside the base contract", async () => {
  assert.deepEqual(await evaluateMetadata({ body: "Dependency refresh", baseRef: "develop", headRef: "renovate/rust" }), []);
});

test("issue-backed wrong bases fail for every supported closing keyword form", async () => {
  for (const body of [
    "Closes #144",
    "fixed MediaNoxLabs/oxid#144",
    "RESOLVES https://github.com/MediaNoxLabs/oxid/issues/144",
  ]) {
    const failures = await evaluateMetadata({ body, baseRef: "develop", headRef: "issue-144" });
    assert.equal(failures.length, 1, body);
    assert.match(failures[0], /integration/);
  }
});

test("only integration-to-main is an issue-backed release-promotion exception", async () => {
  assert.deepEqual(await evaluateMetadata({ body: "Closes #144", baseRef: "main", headRef: "integration" }), []);
  assert.equal((await evaluateMetadata({ body: "Closes #144", baseRef: "main", headRef: "feature" })).length, 1);
});

test("repository gate runs architecture and the delivery contract", async () => {
  const gate = await read("run.sh");
  assert.match(gate, /\.\/scripts\/check-architecture\.sh/);
  assert.match(gate, /node --test tests\/repository\/integration-delivery-contract\.test\.mjs/);
});

test("authoritative guidance and dev-loop config agree on integration and Claude current-head review", async () => {
  for (const file of ["AGENT.md", "CONTRIBUTING.md", ".github/pull_request_template.md", "docs/site/src/contributing.md"]) {
    assert.match(await read(file), /integration/, file);
  }
  const contract = await read("docs/integration-delivery.md");
  assert.match(contract, /--base origin\/integration/);
  assert.match(contract, /--base integration/);
  assert.match(contract, /git merge-base HEAD origin\/integration/);
  assert.match(contract, /git merge-base --is-ancestor origin\/integration HEAD/);
  assert.match(contract, /git merge-tree --write-tree origin\/integration HEAD/);
  assert.match(contract, /integration -> main/);
  assert.match(contract, /Require integration for issue-backed PRs/);
  const config = await read(".devloops");
  assert.match(config, /maxCopilotRounds: 0/);
  assert.match(config, /Claude CLI/);
  assert.match(config, /current head/i);
});
