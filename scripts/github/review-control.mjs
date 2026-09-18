#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { parseArgs } from "node:util";

import { validateFollowUpIssue } from "./review-triage.mjs";

export const REVIEW_CONTROL_MARKER = "<!-- oxid-review-control-v1 -->";
export const MAX_REVIEW_ROUNDS = 3;
export const BLOCKER_OVERRIDES = Object.freeze([
  "security",
  "irreversible-effect",
  "required-ci",
  "acceptance",
]);
const FREEZE_DISPOSITIONS = new Set(["clean", "follow-up"]);
const ALLOWED_FROZEN_ACTIONS = new Set(["ci", "metrics", "merge", "closeout", "status"]);
const CONTROL_KEYS = [
  "schemaVersion", "headSha", "reviewedHeads", "reviewRounds", "disposition",
  "frozen", "followUpIssues", "freezeReason", "blockerOverride",
];

function sha(value, label = "head SHA") {
  if (typeof value !== "string" || !/^[0-9a-f]{40}$/u.test(value)) throw new Error(`${label} is malformed`);
  return value;
}

function issueNumbers(value, label = "follow-up issues") {
  if (!Array.isArray(value) || value.length > 32
    || value.some((entry) => !Number.isSafeInteger(entry) || entry < 1)
    || new Set(value).size !== value.length) {
    throw new Error(`${label} must be unique positive issue numbers`);
  }
  return value;
}

export function initialReviewControl(headSha) {
  return {
    schemaVersion: 1,
    headSha: sha(headSha),
    reviewedHeads: [],
    reviewRounds: 0,
    disposition: "pending",
    frozen: false,
    followUpIssues: [],
    freezeReason: null,
    blockerOverride: null,
  };
}

export function validateReviewControl(control) {
  if (!control || typeof control !== "object" || Array.isArray(control)) throw new Error("review control must be an object");
  if (JSON.stringify(Object.keys(control).sort()) !== JSON.stringify([...CONTROL_KEYS].sort())) {
    throw new Error("review control has missing or unknown fields");
  }
  if (control.schemaVersion !== 1) throw new Error("review control schemaVersion must be 1");
  sha(control.headSha);
  if (!Array.isArray(control.reviewedHeads) || control.reviewedHeads.length > 32) throw new Error("reviewedHeads is invalid");
  control.reviewedHeads.forEach((head) => sha(head, "reviewed head"));
  if (new Set(control.reviewedHeads).size !== control.reviewedHeads.length) throw new Error("reviewedHeads must be unique");
  if (!Number.isSafeInteger(control.reviewRounds) || control.reviewRounds < 0
    || control.reviewRounds !== control.reviewedHeads.length) {
    throw new Error("reviewRounds must equal reviewedHeads length");
  }
  if (!["pending", "clean", "follow-up"].includes(control.disposition)) throw new Error("review control disposition is invalid");
  if (typeof control.frozen !== "boolean") throw new Error("review control frozen must be boolean");
  issueNumbers(control.followUpIssues);
  if (control.freezeReason !== null && !FREEZE_DISPOSITIONS.has(control.freezeReason)) throw new Error("freezeReason is invalid");
  if (control.blockerOverride !== null && !BLOCKER_OVERRIDES.includes(control.blockerOverride)) throw new Error("blockerOverride is invalid");
  if (control.frozen) {
    if (!FREEZE_DISPOSITIONS.has(control.disposition) || control.freezeReason !== control.disposition) {
      throw new Error("frozen review control requires a terminal disposition and matching reason");
    }
    if (!control.reviewedHeads.includes(control.headSha)) throw new Error("frozen head must have review evidence");
    if (control.disposition === "follow-up" && control.followUpIssues.length === 0) {
      throw new Error("follow-up freeze requires at least one issue");
    }
    if (control.disposition === "clean" && control.followUpIssues.length !== 0) {
      throw new Error("clean freeze cannot carry follow-up issues");
    }
  } else if (control.freezeReason !== null) {
    throw new Error("unfrozen review control cannot carry a freeze reason");
  }
  return control;
}

export function buildReviewControlComment(control) {
  validateReviewControl(control);
  return `${REVIEW_CONTROL_MARKER}\n${JSON.stringify(control)}`;
}

export function parseReviewControlComment(body) {
  if (typeof body !== "string" || !body.startsWith(`${REVIEW_CONTROL_MARKER}\n`)) return null;
  const encoded = body.slice(REVIEW_CONTROL_MARKER.length + 1).trim();
  if (!encoded || encoded.includes("\n")) throw new Error("review control must contain one JSON line after its marker");
  try {
    return validateReviewControl(JSON.parse(encoded));
  } catch (error) {
    throw new Error(`invalid review control: ${error.message}`, { cause: error });
  }
}

export function currentReviewControl(comments, headSha, { required = false } = {}) {
  if (!Array.isArray(comments)) throw new Error("pull-request comments are unavailable");
  const matches = comments.map((comment) => parseReviewControlComment(comment?.body)).filter(Boolean);
  if (matches.length > 1) throw new Error("multiple review-control comments exist");
  if (matches.length === 0) {
    if (required) throw new Error("review-control comment is missing");
    return null;
  }
  const control = matches[0];
  if (headSha !== undefined && control.headSha !== headSha) {
    if (control.frozen) throw new Error(`frozen review head changed from ${control.headSha} to ${headSha}`);
    control.headSha = sha(headSha);
  }
  return control;
}

export function authorizeReview(control, { headSha, blockerOverride = null } = {}) {
  const current = validateReviewControl(structuredClone(control));
  sha(headSha);
  if (current.frozen) throw new Error(`head ${current.headSha} is frozen; another review is forbidden`);
  if (current.reviewedHeads.includes(headSha)) throw new Error(`head ${headSha} already consumed a review round`);
  if (blockerOverride !== null && !BLOCKER_OVERRIDES.includes(blockerOverride)) throw new Error("blocker override is invalid");
  if (current.reviewRounds >= MAX_REVIEW_ROUNDS && blockerOverride === null) {
    throw new Error(`review budget exhausted at ${MAX_REVIEW_ROUNDS} rounds; defer safe findings or record a blocker override`);
  }
  return validateReviewControl({
    ...current,
    headSha,
    reviewedHeads: [...current.reviewedHeads, headSha],
    reviewRounds: current.reviewRounds + 1,
    disposition: "pending",
    frozen: false,
    followUpIssues: [],
    freezeReason: null,
    blockerOverride,
  });
}

export function freezeReview(control, { headSha, disposition, followUpIssues = [] } = {}) {
  const current = validateReviewControl(structuredClone(control));
  if (!FREEZE_DISPOSITIONS.has(disposition)) throw new Error("freeze disposition must be clean or follow-up");
  if (current.frozen) throw new Error(`head ${current.headSha} is already frozen`);
  if (current.headSha !== headSha || !current.reviewedHeads.includes(headSha)) {
    throw new Error("only the current reviewed exact head can be frozen");
  }
  issueNumbers(followUpIssues);
  return validateReviewControl({
    ...current,
    disposition,
    frozen: true,
    followUpIssues,
    freezeReason: disposition,
  });
}

export function assertReviewActionAllowed(control, { headSha, action } = {}) {
  const current = validateReviewControl(structuredClone(control));
  sha(headSha);
  if (current.frozen && current.headSha !== headSha) throw new Error("frozen PR head changed without an explicit blocker repair");
  if (!current.frozen) return { ok: true, action, frozen: false };
  if (!ALLOWED_FROZEN_ACTIONS.has(action)) {
    throw new Error(`frozen head rejects ${action}; only CI observation, metrics, exact-head merge, closeout, and status are allowed`);
  }
  return { ok: true, action, frozen: true, disposition: current.disposition };
}

function run(command, args) {
  try {
    return execFileSync(command, args, { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
  } catch (error) {
    throw new Error(error?.stderr?.trim() || error.message, { cause: error });
  }
}

function ghJson(args) {
  return JSON.parse(run("gh", args));
}

function readPr(repo, pr) {
  const facts = ghJson(["pr", "view", String(pr), "--repo", repo, "--json", "headRefOid"]);
  const comments = ghJson(["api", `repos/${repo}/issues/${pr}/comments`, "--paginate", "--slurp"]).flat();
  return { headSha: facts.headRefOid, comments };
}

function upsert(repo, pr, comments, control) {
  const body = buildReviewControlComment(control);
  const existing = comments.filter((comment) => parseReviewControlComment(comment?.body));
  if (existing.length > 1) throw new Error("multiple review-control comments exist");
  if (existing.length === 1) {
    run("gh", ["api", "--method", "PATCH", `repos/${repo}/issues/comments/${existing[0].id}`, "-f", `body=${body}`]);
  } else {
    run("gh", ["pr", "comment", String(pr), "--repo", repo, "--body", body]);
  }
}

function parseCli(argv) {
  const [command, ...rest] = argv;
  const { values } = parseArgs({
    args: rest,
    options: {
      repo: { type: "string" }, pr: { type: "string" }, head: { type: "string" },
      blocker: { type: "string" }, disposition: { type: "string" },
      "follow-up": { type: "string" }, action: { type: "string" }, post: { type: "boolean" },
      help: { type: "boolean", short: "h" },
    },
    strict: true,
  });
  if (values.help || !command) return { help: true };
  if (values.repo !== "MediaNoxLabs/oxid") throw new Error("--repo must be MediaNoxLabs/oxid");
  const pr = Number(values.pr);
  if (!Number.isSafeInteger(pr) || pr < 1) throw new Error("--pr must be a positive integer");
  return {
    command, repo: values.repo, pr, head: values.head, blocker: values.blocker ?? null,
    disposition: values.disposition, action: values.action, post: values.post === true,
    followUpIssues: (values["follow-up"] ?? "").split(",").filter(Boolean).map(Number),
  };
}

export function cli(argv = process.argv.slice(2)) {
  const options = parseCli(argv);
  if (options.help) {
    process.stdout.write("Usage: review-control.mjs <authorize-review|freeze|assert|status> --repo MediaNoxLabs/oxid --pr N [--head SHA] [--blocker KIND] [--disposition clean|follow-up] [--follow-up N,N] [--action ACTION] [--post]\n");
    return;
  }
  const { headSha, comments } = readPr(options.repo, options.pr);
  if (options.head !== undefined && options.head !== headSha) throw new Error("PR head does not match --head");
  const existing = currentReviewControl(comments, headSha);
  if (options.command === "status") {
    process.stdout.write(`${JSON.stringify(existing ?? initialReviewControl(headSha))}\n`);
    return;
  }
  if (options.command === "authorize-review") {
    const updated = authorizeReview(existing ?? initialReviewControl(headSha), { headSha, blockerOverride: options.blocker });
    if (options.post) upsert(options.repo, options.pr, comments, updated);
    process.stdout.write(`${JSON.stringify(updated)}\n`);
    return;
  }
  if (options.command === "freeze") {
    for (const issue of options.followUpIssues) {
      const facts = ghJson(["issue", "view", String(issue), "--repo", options.repo, "--json", "state,body,labels"]);
      const validation = validateFollowUpIssue(facts, { originPr: options.pr });
      if (!validation.ok) throw new Error(`follow-up issue #${issue} ${validation.failures.join("; ")}`);
    }
    const updated = freezeReview(existing ?? initialReviewControl(headSha), {
      headSha, disposition: options.disposition, followUpIssues: options.followUpIssues,
    });
    if (options.post) upsert(options.repo, options.pr, comments, updated);
    process.stdout.write(`${JSON.stringify(updated)}\n`);
    return;
  }
  if (options.command === "assert") {
    if (!options.action) throw new Error("assert requires --action");
    if (existing === null) throw new Error("review-control comment is missing");
    process.stdout.write(`${JSON.stringify(assertReviewActionAllowed(existing, { headSha, action: options.action }))}\n`);
    return;
  }
  throw new Error(`unknown command ${options.command}`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    cli();
  } catch (error) {
    process.stderr.write(`[review-control] ${error.message}\n`);
    process.exitCode = 1;
  }
}
