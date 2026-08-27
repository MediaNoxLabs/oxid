// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  INTEGRATION_BLOB_PREFIX,
  buildLycheeArgs,
  candidatePath,
  validateCandidateLinks,
} from "../../scripts/docs/check-links.mjs";
import { renderAdrCatalog } from "../../scripts/docs/generate-adr-catalog.mjs";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const generator = path.join(repoRoot, "scripts/docs/generate-adr-catalog.mjs");

function tempDir() {
  return mkdtempSync(path.join(os.tmpdir(), "oxid-doc-links-"));
}

function git(cwd, ...args) {
  execFileSync("git", args, { cwd, stdio: "pipe" });
}

test("ADR catalog generation is identical without remotes and across arbitrary refs", () => {
  const outputs = [];
  for (const branch of ["integration", "candidate", "detached-fixture"]) {
    const root = tempDir();
    try {
      git(root, "init", "-q", "-b", branch);
      if (branch === "detached-fixture") writeFileSync(path.join(root, ".git", "HEAD"), "ref: refs/heads/missing\n");
      const index = path.join(root, "README.md");
      const output = path.join(root, "catalog.md");
      writeFileSync(index, "# ADRs\n\n| [0104](0104-example.md) Example | Accepted |\n");
      execFileSync(process.execPath, [generator, "--index", index, "--output", output], { cwd: root });
      outputs.push(readFileSync(output, "utf8"));
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  }
  assert.equal(new Set(outputs).size, 1);
  assert.match(outputs[0], /blob\/integration\/docs\/adr\/0104-example\.md/);
});

test("candidate integration blob links resolve to tracked local files before Lychee", async () => {
  const root = tempDir();
  try {
    git(root, "init", "-q", "-b", "candidate");
    mkdirSync(path.join(root, "docs", "adr"), { recursive: true });
    writeFileSync(path.join(root, "README.md"), `[ADR](${INTEGRATION_BLOB_PREFIX}docs/adr/0104-example.md)\n`);
    writeFileSync(path.join(root, "docs", "adr", "0104-example.md"), "# Example\n");
    git(root, "add", "README.md", "docs/adr/0104-example.md");
    await validateCandidateLinks(root);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("candidate integration blob validation fails for missing and ambiguous targets", async () => {
  const root = tempDir();
  try {
    git(root, "init", "-q", "-b", "candidate");
    writeFileSync(path.join(root, "README.md"), `[missing](${INTEGRATION_BLOB_PREFIX}docs/missing.md)\n`);
    git(root, "add", "README.md");
    await assert.rejects(validateCandidateLinks(root), /not tracked/);
    assert.throws(() => candidatePath(`${INTEGRATION_BLOB_PREFIX}docs/%2e%2e/secret.md`), /ambiguous/);
    assert.throws(() => candidatePath(`${INTEGRATION_BLOB_PREFIX}docs/../secret.md`), /unsafe/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("Lychee remaps only the same-repository integration prefix and still checks all Markdown", () => {
  const args = buildLycheeArgs(repoRoot);
  const remapIndex = args.indexOf("--remap");
  assert.notEqual(remapIndex, -1);
  assert.match(args[remapIndex + 1], /^https:\/\/github\.com\/MediaNoxLabs\/oxid\/blob\/integration\/ file:\/\//);
  assert.equal(args.at(-1), "./**/*.md");
  assert.equal(args.includes("--offline"), false);
  assert.equal(args.includes("--exclude"), false);
});

test("the renderer keeps durable integration URLs for candidate-only ADRs", () => {
  const rendered = renderAdrCatalog("# ADRs\n\n| [0104](0104-example.md) Example | Accepted |\n");
  assert.match(rendered, new RegExp(`${INTEGRATION_BLOB_PREFIX.replaceAll("/", "\\/")}docs\\/adr\\/0104-example\\.md`));
});
