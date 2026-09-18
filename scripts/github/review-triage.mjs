#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { parseArgs } from "node:util";

import { deliveryTargetFromIssueBody } from "../lib/delivery-target.mjs";

export const TRIAGE_MARKER = "<!-- oxid-review-triage-v1 -->";
const RECEIPT_KEYS = ["schemaVersion", "headSha", "blockingFindingCount", "followUpIssues"];
const FOLLOW_UP_HEADINGS = new Set(["Problem", "Acceptance criteria", "Dependencies"]);
const LIST_ITEM = /^\s*(?:[-*]|[1-9]\d*\.)\s+(?:\[[ xX]\]\s*)?\S/u;
const ORIGIN_PR_REFERENCE = /\bOrigin\s*:?\s*(?:PR\s+#|https:\/\/github\.com\/[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+\/pull\/|\/pull\/)([1-9]\d*)/iu;

export function buildTriageReceipt({ headSha, blockingFindingCount = 0, followUpIssues = [] }) {
  const receipt = { schemaVersion: 1, headSha, blockingFindingCount, followUpIssues };
  validateTriageReceipt(receipt);
  return `${TRIAGE_MARKER}\n${JSON.stringify(receipt)}`;
}

export function validateTriageReceipt(receipt) {
  if (!receipt || typeof receipt !== "object" || Array.isArray(receipt)) throw new Error("triage receipt must be an object");
  const keys = Object.keys(receipt).sort();
  if (JSON.stringify(keys) !== JSON.stringify([...RECEIPT_KEYS].sort())) throw new Error("triage receipt has missing or unknown fields");
  if (receipt.schemaVersion !== 1) throw new Error("triage receipt schemaVersion must be 1");
  if (typeof receipt.headSha !== "string" || !/^[0-9a-f]{40}$/u.test(receipt.headSha)) throw new Error("triage receipt headSha is malformed");
  if (!Number.isSafeInteger(receipt.blockingFindingCount) || receipt.blockingFindingCount < 0) throw new Error("triage receipt blockingFindingCount must be non-negative");
  if (!Array.isArray(receipt.followUpIssues) || receipt.followUpIssues.length > 32
    || receipt.followUpIssues.some((issue) => !Number.isSafeInteger(issue) || issue < 1)
    || new Set(receipt.followUpIssues).size !== receipt.followUpIssues.length) {
    throw new Error("triage receipt followUpIssues must be unique positive issue numbers");
  }
  return receipt;
}

export function parseTriageComment(body) {
  if (typeof body !== "string" || !body.startsWith(`${TRIAGE_MARKER}\n`)) return null;
  const encoded = body.slice(TRIAGE_MARKER.length + 1).trim();
  if (!encoded || encoded.includes("\n")) throw new Error("triage receipt must contain one JSON line after its marker");
  let receipt;
  try {
    receipt = JSON.parse(encoded);
  } catch (error) {
    throw new Error(`triage receipt is invalid JSON: ${error.message}`, { cause: error });
  }
  return validateTriageReceipt(receipt);
}

export function currentTriageReceipt(comments, headSha) {
  if (!Array.isArray(comments)) throw new Error("pull-request comments are unavailable");
  const matches = [];
  for (const comment of comments) {
    const receipt = parseTriageComment(comment?.body);
    if (receipt?.headSha === headSha) matches.push(receipt);
  }
  if (matches.length !== 1) throw new Error(`expected exactly one review triage receipt for current head ${headSha}`);
  if (matches[0].blockingFindingCount !== 0) throw new Error("current review triage still contains blocking findings");
  return matches[0];
}

export function markdownSection(body, heading) {
  if (typeof body !== "string" || !FOLLOW_UP_HEADINGS.has(heading)) return null;
  const lines = body.split(/\r?\n/u);
  const indexes = lines.flatMap((line, index) => line.trimEnd() === `## ${heading}` ? [index] : []);
  if (indexes.length !== 1) return null;
  const start = indexes[0] + 1;
  const endOffset = lines.slice(start).findIndex((line) => /^##\s+/u.test(line));
  const end = endOffset === -1 ? lines.length : start + endOffset;
  return lines.slice(start, end).join("\n").trim();
}

export function originPullRequest(body) {
  const match = (typeof body === "string" ? body : "").match(ORIGIN_PR_REFERENCE);
  return match ? Number(match[1]) : null;
}

export function validateFollowUpIssue(issue, { originPr, requireOpen = true } = {}) {
  const failures = [];
  const labels = Array.isArray(issue?.labels)
    ? issue.labels.map((label) => typeof label === "string" ? label : label?.name).filter(Boolean)
    : [];
  const body = typeof issue?.body === "string" ? issue.body : "";
  if (requireOpen && issue?.state !== "OPEN") failures.push("must be open");
  if (!requireOpen && !["OPEN", "CLOSED"].includes(issue?.state)) failures.push("must have a known lifecycle state");
  if (!labels.includes("factory:follow-up")) failures.push("must carry factory:follow-up");
  const problem = markdownSection(body, "Problem");
  const acceptance = markdownSection(body, "Acceptance criteria");
  const dependencies = markdownSection(body, "Dependencies");
  if (!problem || problem.length < 20) failures.push("must contain a substantive Problem section");
  if (!acceptance || !acceptance.split("\n").some((line) => LIST_ITEM.test(line))) {
    failures.push("must contain substantive list items in Acceptance criteria");
  }
  if (!dependencies || !dependencies.split("\n").some((line) => LIST_ITEM.test(line))) {
    failures.push("must contain substantive list items in Dependencies");
  }
  try {
    deliveryTargetFromIssueBody(body);
  } catch {
    failures.push("must contain exactly one valid Delivery target");
  }
  if (originPr !== undefined) {
    if (originPullRequest(dependencies) !== originPr) failures.push(`must link origin PR #${originPr} in Dependencies`);
  } else if (originPullRequest(dependencies) === null) {
    failures.push("must link an origin PR in Dependencies");
  }
  return { ok: failures.length === 0, failures, labels };
}

function parseCli(argv) {
  const { values } = parseArgs({
    args: argv,
    options: {
      repo: { type: "string" }, pr: { type: "string" }, head: { type: "string" },
      "blocking-count": { type: "string" }, "follow-up": { type: "string" },
      post: { type: "boolean" }, help: { type: "boolean", short: "h" },
    },
    strict: true,
  });
  if (values.help) return { help: true };
  if (values.repo !== "MediaNoxLabs/oxid") throw new Error("--repo must be MediaNoxLabs/oxid");
  const pr = Number(values.pr);
  if (!Number.isSafeInteger(pr) || pr < 1) throw new Error("--pr must be a positive integer");
  const blockingFindingCount = Number(values["blocking-count"] ?? "0");
  const followUpIssues = (values["follow-up"] ?? "").split(",").filter(Boolean).map(Number);
  const receipt = validateTriageReceipt({ schemaVersion: 1, headSha: values.head, blockingFindingCount, followUpIssues });
  return { help: false, repo: values.repo, pr, post: values.post === true, receipt };
}

function run(command, args) {
  try {
    return execFileSync(command, args, { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
  } catch (error) {
    throw new Error(error?.stderr?.trim() || error.message, { cause: error });
  }
}

function cli(argv = process.argv.slice(2)) {
  const options = parseCli(argv);
  if (options.help) {
    process.stdout.write("Usage: review-triage.mjs --repo MediaNoxLabs/oxid --pr NUMBER --head SHA [--blocking-count N] [--follow-up N,N] [--post]\n");
    return;
  }
  const body = buildTriageReceipt(options.receipt);
  if (!options.post) {
    process.stdout.write(`${body}\n`);
    return;
  }
  const pr = JSON.parse(run("gh", ["pr", "view", String(options.pr), "--repo", options.repo, "--json", "headRefOid"]));
  if (pr?.headRefOid !== options.receipt.headSha) throw new Error("PR head does not match --head; refusing stale triage");
  for (const issue of options.receipt.followUpIssues) {
    const item = JSON.parse(run("gh", ["issue", "view", String(issue), "--repo", options.repo, "--json", "state,body,labels"]));
    const validation = validateFollowUpIssue(item, { originPr: options.pr });
    if (!validation.ok) throw new Error(`follow-up issue #${issue} ${validation.failures.join("; ")}`);
  }
  const pages = JSON.parse(run("gh", ["api", `repos/${options.repo}/issues/${options.pr}/comments`, "--paginate", "--slurp"])).flat();
  const existing = pages.filter((comment) => parseTriageComment(comment?.body)?.headSha === options.receipt.headSha);
  if (existing.length > 1) throw new Error("multiple current-head triage receipts exist; repair manually");
  if (existing.length === 1) {
    run("gh", ["api", "--method", "PATCH", `repos/${options.repo}/issues/comments/${existing[0].id}`, "-f", `body=${body}`]);
  } else {
    run("gh", ["pr", "comment", String(options.pr), "--repo", options.repo, "--body", body]);
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    cli();
  } catch (error) {
    process.stderr.write(`[review-triage] ${error.message}\n`);
    process.exitCode = 1;
  }
}
