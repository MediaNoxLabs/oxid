#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { execFileSync } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

import { verifyLocalGate } from "./local-gate.mjs";

const REPO = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/u;
const HEAD = /^[0-9a-f]{40}$/u;
const DELIVERY_BASE = /^origin\/(?:develop|milestone-[0-9]+\.[0-9]+\.[0-9]+)$/u;
const GATES = new Set(["draft_gate", "pre_approval_gate", "review"]);
const GATE_ID = "production-ready";

function git(cwd, args) {
  return execFileSync("git", args, { cwd, encoding: "utf8" }).trim();
}

function positiveInteger(value, label) {
  if (!/^[1-9]\d*$/u.test(value ?? "")) throw new Error(`${label} must be a positive integer`);
  return Number(value);
}

function safeTmpRoot(repoRoot, value) {
  if (typeof value !== "string" || value.length === 0 || path.isAbsolute(value)) {
    throw new Error("--tmp-root must be a non-empty repository-relative path");
  }
  const resolved = path.resolve(repoRoot, value);
  const relative = path.relative(repoRoot, resolved);
  if (relative.startsWith("..") || path.isAbsolute(relative)) {
    throw new Error("--tmp-root must remain inside the repository");
  }
  return { relative, resolved };
}

export function parseGateValidationArgs(argv) {
  const { values } = parseArgs({
    args: argv,
    options: {
      repo: { type: "string" },
      pr: { type: "string" },
      gate: { type: "string" },
      "head-sha": { type: "string" },
      "delivery-base": { type: "string" },
      "tmp-root": { type: "string", default: "tmp" },
      help: { type: "boolean", short: "h" },
    },
    allowPositionals: false,
    strict: true,
  });
  if (values.help) return { help: true };
  if (!REPO.test(values.repo ?? "") || values.repo.split("/").some((part) => part === "." || part === "..")) {
    throw new Error("--repo must be a safe owner/name slug");
  }
  const pr = positiveInteger(values.pr, "--pr");
  if (!GATES.has(values.gate)) throw new Error("--gate must be draft_gate, pre_approval_gate, or review");
  if (!HEAD.test(values["head-sha"] ?? "")) throw new Error("--head-sha must be an exact lowercase Git SHA");
  if (!DELIVERY_BASE.test(values["delivery-base"] ?? "")) throw new Error("--delivery-base must be origin/develop or origin/milestone-X.Y.Z");
  return {
    repo: values.repo,
    pr,
    gate: values.gate,
    headSha: values["head-sha"],
    deliveryBase: values["delivery-base"],
    tmpRoot: values["tmp-root"],
  };
}

function artifactPaths({ repoRoot, repo, pr, gate, headSha, tmpRoot }) {
  const safeRoot = safeTmpRoot(repoRoot, tmpRoot);
  const repoSlug = repo.replace("/", "-");
  const directory = path.join(safeRoot.resolved, "gate-context", repoSlug, `pr-${pr}`);
  const stem = `${gate}-${headSha}`;
  return {
    directory,
    artifact: path.join(directory, `${stem}.validation.json`),
    log: path.join(directory, `${stem}.validation-oxid-production-ready.log`),
    relativeLog: path.join(safeRoot.relative, "gate-context", repoSlug, `pr-${pr}`, `${stem}.validation-oxid-production-ready.log`),
  };
}

export async function buildCargoGateValidationArtifact(input, {
  repoRoot = process.cwd(),
  verifyReceipt = verifyLocalGate,
  now = () => new Date(),
} = {}) {
  const root = git(repoRoot, ["rev-parse", "--path-format=absolute", "--show-toplevel"]);
  const actualHead = git(root, ["rev-parse", "HEAD"]);
  if (actualHead !== input.headSha) throw new Error("gate validation head does not match the checked-out exact head");

  const verified = await verifyReceipt({
    cwd: root,
    deliveryBase: input.deliveryBase,
    gateId: GATE_ID,
    // Receipt verification starts no child, so GNU timeout is not part of this
    // read-only adapter's capability boundary.
    assertCapabilities: () => {},
  });
  if (!verified?.ok || verified.action !== "verified" || verified.receipt?.headSha !== input.headSha) {
    throw new Error("repository-owned local-gate receipt did not verify at the requested head");
  }

  const paths = artifactPaths({ repoRoot: root, ...input });
  await mkdir(paths.directory, { recursive: true, mode: 0o700 });
  const command = `node scripts/loop/local-gate.mjs verify --delivery-base ${input.deliveryBase} --gate-id ${GATE_ID}`;
  const message = `Verified Oxid Cargo-workspace exact-head receipt for ${input.headSha}.`;
  await writeFile(paths.log, `${message}\n`, { encoding: "utf8", mode: 0o600 });
  const artifact = {
    ok: true,
    repo: input.repo,
    pr: input.pr,
    gate: input.gate,
    headSha: input.headSha,
    generatedAt: now().toISOString(),
    allPassed: true,
    depState: {
      status: "n-a",
      detail: "Cargo-workspace validation is bound to the repository-owned exact-head local-gate receipt; npm dependency state is not applicable.",
    },
    suites: [{
      name: "oxid-production-ready",
      command,
      exitCode: 0,
      outputTail: message,
      outputPath: paths.relativeLog,
    }],
  };
  await writeFile(paths.artifact, `${JSON.stringify(artifact, null, 2)}\n`, { encoding: "utf8", mode: 0o600 });
  return { artifact, artifactPath: paths.artifact };
}

const USAGE = `Usage:
  node scripts/loop/gate-validation.mjs --repo owner/name --pr N \\
    --gate draft_gate --head-sha <exact-sha> --delivery-base origin/<target>

Verifies Oxid's Cargo-native production-ready receipt and writes the validation
artifact expected by the pinned dev-loops gate reviewers. It never runs npm or Bun.`;

export async function runCli(argv = process.argv.slice(2), runtime = {}) {
  const options = parseGateValidationArgs(argv);
  if (options.help) {
    process.stdout.write(`${USAGE}\n`);
    return 0;
  }
  const result = await buildCargoGateValidationArtifact(options, runtime);
  process.stdout.write(`${JSON.stringify({ ok: true, artifactPath: result.artifactPath, ...result.artifact })}\n`);
  return 0;
}

if (path.resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  runCli().then((code) => { process.exitCode = code; }).catch((error) => {
    process.stderr.write(`[gate-validation] ${error.message}\n`);
    process.exitCode = 1;
  });
}
