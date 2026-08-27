#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { lstat, readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { parseArgs } from "node:util";

import { assertMinimumGhVersion, assertRepositoryName, parseGhVersion, runGhCommand } from "../github/rest-client.mjs";
import { validateFanoutRepairEvidence } from "../lib/gate-evidence-repair.mjs";

const MAX_PROVENANCE_BYTES = 1024 * 1024;

async function readProvenance(file) {
  const info = await lstat(file);
  if (!info.isFile() || info.isSymbolicLink()) throw new Error("--provenance must be a regular non-symlink file");
  if (info.size > MAX_PROVENANCE_BYTES) throw new Error(`--provenance exceeds ${MAX_PROVENANCE_BYTES} bytes`);
  return JSON.parse(await readFile(file, "utf8"));
}

function ghJson(ghCommand, args) {
  const output = runGhCommand(ghCommand, args, { failureLabel: "gate evidence repair GitHub request" });
  if (output.trim() === "") throw new Error("gate evidence repair GitHub request returned empty output");
  try { return JSON.parse(output); } catch (error) { throw new Error(`gate evidence repair GitHub request returned malformed JSON: ${error.message}`); }
}

function existingGateComment(comments, gate, headSha, viewerLogin) {
  const heading = `### Gate review: \`${gate}\``;
  const head = `**Reviewed head SHA:** \`${headSha}\``;
  const matches = comments.filter((comment) =>
    typeof comment?.body === "string"
    && comment.body.includes(heading)
    && comment.body.includes(head)
    && comment.user?.login === viewerLogin
  );
  if (matches.length !== 1) throw new Error(`repair requires exactly one current-head ${gate} comment; found ${matches.length}`);
  const comment = matches[0];
  const verdict = /^\*\*Verdict:\*\*\s+([^\s]+)$/m.exec(comment.body)?.[1];
  const executionMode = /^\*\*Execution mode:\*\*\s+([^\s—]+).*$/m.exec(comment.body)?.[1];
  if (!new Set(["clean", "findings", "findings_present"]).has(verdict)) throw new Error("existing gate verdict is missing or unsupported");
  if (!executionMode) throw new Error("existing gate execution mode is missing");
  return {
    comment,
    evidence: {
      visible: true,
      contractComplete: true,
      headSha,
      verdict: verdict === "findings_present" ? "findings" : verdict,
      executionMode,
      commentId: comment.id,
    },
  };
}

function renderRepair(body, plan, provenance) {
  let repaired = body.replace(/^\*\*Execution mode:\*\*.*$/m, "**Execution mode:** fanout_fanin");
  repaired = repaired.replace(/^\*\*Verdict:\*\*.*$/m, `**Verdict:** ${plan.verdict === "findings" ? "findings_present" : "clean"}`);
  repaired = repaired.replace(
    /^\*\*Findings summary:\*\*.*$/m,
    `**Findings summary:** Fan-out evidence repair recorded ${plan.findingCount} finding${plan.findingCount === 1 ? "" : "s"}; see the audit block below.`,
  );
  const reviewerLines = provenance.reviewers.map((reviewer) =>
    `- \`${reviewer.angle}\`: ${reviewer.verdict}; reviewer \`${reviewer.reviewerId}\`; artifact \`${reviewer.artifactSha256}\``
  );
  const findingLines = plan.findings.map((finding) =>
    `  - \`${finding.angle}\` / \`${finding.severity}\`: ${JSON.stringify(finding.summary)}`
  );
  const audit = [
    "",
    "**Fan-out evidence repair audit:**",
    `- upgraded from \`${plan.audit.fromExecutionMode}\` on ${plan.audit.repairedAt}`,
    `- provenance generated at ${plan.audit.provenanceGeneratedAt}`,
    ...reviewerLines,
    ...(findingLines.length > 0 ? ["- recorded findings:", ...findingLines] : []),
    "",
  ].join("\n");
  const nextAction = repaired.indexOf("**Next action:**");
  if (nextAction < 0) throw new Error("existing gate comment is missing Next action");
  return `${repaired.slice(0, nextAction)}${audit}${repaired.slice(nextAction)}`;
}

export async function repairGateEvidence(options, { ghCommand = "gh", nowMs = Date.now() } = {}) {
  assertMinimumGhVersion(parseGhVersion(runGhCommand(ghCommand, ["--version"], { failureLabel: "GitHub CLI version probe" })));
  const pr = ghJson(ghCommand, ["pr", "view", String(options.pr), "--repo", options.repo, "--json", "headRefOid"]);
  if (pr.headRefOid !== options.headSha) throw new Error("requested repair head is stale; it does not match the current PR head");
  const viewer = ghJson(ghCommand, ["api", "user"]);
  if (typeof viewer.login !== "string" || viewer.login.trim() === "") throw new Error("GitHub viewer response is missing login");
  const comments = ghJson(ghCommand, ["api", `repos/${options.repo}/issues/${options.pr}/comments?per_page=100`]);
  if (!Array.isArray(comments)) throw new Error("GitHub comments response must be an array");
  const existing = existingGateComment(comments, options.gate, options.headSha, viewer.login);
  const provenance = await readProvenance(options.provenance);
  const plan = validateFanoutRepairEvidence({
    existing: existing.evidence,
    requested: options,
    provenance,
    nowMs,
  });
  if (plan.action === "noop") return { ok: true, ...plan, commentId: existing.comment.id };
  const body = renderRepair(existing.comment.body, plan, provenance);
  const updated = ghJson(ghCommand, ["api", "-X", "PATCH", `repos/${options.repo}/issues/comments/${existing.comment.id}`, "-f", `body=${body}`]);
  if (updated.id !== existing.comment.id) throw new Error("GitHub comment repair returned an unexpected comment id");
  return { ok: true, action: "upgraded", commentId: updated.id, headSha: options.headSha, gate: options.gate, audit: plan.audit };
}

function parseCli(argv) {
  const { values } = parseArgs({
    args: argv,
    options: {
      repo: { type: "string" }, pr: { type: "string" }, gate: { type: "string" },
      head: { type: "string" }, verdict: { type: "string" }, provenance: { type: "string" },
      help: { type: "boolean", short: "h" },
    },
    strict: true,
  });
  if (values.help) return { help: true };
  assertRepositoryName(values.repo);
  const pr = Number(values.pr);
  if (!Number.isInteger(pr) || pr < 1) throw new Error("--pr must be a positive integer");
  if (!new Set(["draft_gate", "pre_approval_gate"]).has(values.gate)) throw new Error("--gate must be draft_gate or pre_approval_gate");
  if (!/^[a-f0-9]{40}$/.test(values.head ?? "")) throw new Error("--head must be a full lowercase SHA");
  if (!new Set(["clean", "findings"]).has(values.verdict)) throw new Error("--verdict must be clean or findings");
  if (!values.provenance) throw new Error("--provenance is required");
  return { repo: values.repo, pr, gate: values.gate, headSha: values.head, verdict: values.verdict, provenance: path.resolve(values.provenance) };
}

async function runCli() {
  const options = parseCli(process.argv.slice(2));
  if (options.help) {
    process.stdout.write("Usage: repair-gate-evidence.mjs --repo OWNER/REPO --pr N --gate GATE --head SHA --verdict clean|findings --provenance FILE\n");
    return;
  }
  process.stdout.write(`${JSON.stringify(await repairGateEvidence(options))}\n`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  runCli().catch((error) => {
    process.stderr.write(`[repair-gate-evidence] ${error.message}\n`);
    process.exitCode = 1;
  });
}
