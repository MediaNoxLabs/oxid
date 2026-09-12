#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { chmod, lstat, mkdir, open, readFile, rm } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

import { runManagedChild } from "../lib/managed-child-process.mjs";

export const LOCAL_GATE_SCHEMA = "oxid-local-gate-v1";
const HEAD = /^[0-9a-f]{40}$/u;
const DIGEST = /^[0-9a-f]{64}$/u;
const GATE_ID = /^[a-z0-9][a-z0-9.-]{0,63}$/u;
const DELIVERY_BASE = /^origin\/(?:develop|milestone-[0-9]+\.[0-9]+\.[0-9]+)$/u;
const RECEIPT_KEYS = Object.freeze([
  "schema", "headSha", "deliveryBase", "deliveryBaseOid", "gateId",
  "commandDigest", "durationMs", "completedAt", "outcome",
]);
export function resolveProductionReadyGateCommand(deliveryBase) {
  return ["env", `OXID_COVERAGE_BASE=${deliveryBase}`, "just", "check"];
}

function git(cwd, args) {
  return execFileSync("git", args, { cwd, encoding: "utf8" }).trim();
}

function canonicalTimestamp(value) {
  const milliseconds = Date.parse(value);
  return typeof value === "string"
    && Number.isFinite(milliseconds)
    && new Date(milliseconds).toISOString() === value;
}

function assertGateIdentity({ headSha, deliveryBase, deliveryBaseOid, gateId, commandDigest }) {
  if (!HEAD.test(headSha ?? "")) throw new Error("local gate head must be an exact lowercase Git SHA");
  if (!DELIVERY_BASE.test(deliveryBase ?? "")) throw new Error("local gate delivery base is malformed");
  if (!HEAD.test(deliveryBaseOid ?? "")) throw new Error("local gate delivery-base OID is malformed");
  if (!GATE_ID.test(gateId ?? "")) throw new Error("local gate id is malformed");
  if (commandDigest !== undefined && !DIGEST.test(commandDigest ?? "")) throw new Error("local gate command digest is malformed");
}

function assertCanonicalGateCommand(deliveryBase, gateId, command) {
  const canonical = resolveProductionReadyGateCommand(deliveryBase);
  if (gateId === "production-ready"
    && (command.length !== canonical.length
      || command.some((argument, index) => argument !== canonical[index]))) {
    throw new Error(`production-ready local gate requires the canonical command: env OXID_COVERAGE_BASE=${deliveryBase} just check`);
  }
}

export function digestGateCommand(command) {
  if (!Array.isArray(command) || command.length === 0
    || command.some((argument) => typeof argument !== "string" || argument.length === 0 || argument.includes("\0"))) {
    throw new Error("local gate command must contain bounded non-empty arguments");
  }
  if (command.length > 64 || command.some((argument) => Buffer.byteLength(argument, "utf8") > 4096)) {
    throw new Error("local gate command is oversized");
  }
  const hash = createHash("sha256");
  for (const argument of command) {
    const bytes = Buffer.from(argument, "utf8");
    const length = Buffer.alloc(4);
    length.writeUInt32BE(bytes.length);
    hash.update(length).update(bytes);
  }
  return hash.digest("hex");
}

export function validateLocalGateReceipt(receipt, expected = {}) {
  if (!receipt || typeof receipt !== "object" || Array.isArray(receipt)) throw new Error("local gate receipt must be an object");
  const keys = Object.keys(receipt).sort();
  if (JSON.stringify(keys) !== JSON.stringify([...RECEIPT_KEYS].sort())) throw new Error("local gate receipt has missing or unknown fields");
  assertGateIdentity(receipt);
  if (receipt.schema !== LOCAL_GATE_SCHEMA) throw new Error(`local gate receipt schema must be ${LOCAL_GATE_SCHEMA}`);
  if (receipt.outcome !== "passed") throw new Error("local gate receipt outcome must be passed");
  if (!Number.isSafeInteger(receipt.durationMs) || receipt.durationMs < 0) throw new Error("local gate duration must be a non-negative integer");
  if (!canonicalTimestamp(receipt.completedAt)) throw new Error("local gate completion timestamp is malformed");
  for (const field of ["headSha", "deliveryBase", "deliveryBaseOid", "gateId", "commandDigest"]) {
    if (expected[field] !== undefined && receipt[field] !== expected[field]) throw new Error(`local gate receipt ${field} does not match current delivery state`);
  }
  return receipt;
}

export function buildLocalGateReceipt({ headSha, deliveryBase, deliveryBaseOid, gateId, commandDigest, durationMs, completedAt }) {
  return validateLocalGateReceipt({
    schema: LOCAL_GATE_SCHEMA,
    headSha,
    deliveryBase,
    deliveryBaseOid,
    gateId,
    commandDigest,
    durationMs,
    completedAt,
    outcome: "passed",
  });
}

function inspectCheckout(cwd, deliveryBase) {
  const root = git(cwd, ["rev-parse", "--path-format=absolute", "--show-toplevel"]);
  const commonDir = git(cwd, ["rev-parse", "--path-format=absolute", "--git-common-dir"]);
  return {
    root,
    commonDir,
    headSha: git(root, ["rev-parse", "HEAD"]),
    deliveryBaseOid: git(root, ["rev-parse", deliveryBase]),
    dirty: git(root, ["status", "--porcelain=v1"]),
  };
}

function gatePaths(commonDir, headSha, gateId) {
  const directory = path.join(commonDir, "oxid-factory", "local-gates-v1");
  const stem = `${headSha}-${gateId}`;
  return { directory, receipt: path.join(directory, `${stem}.json`), lock: path.join(directory, `${stem}.lock`) };
}

async function readReceipt(receiptPath) {
  try {
    const info = await lstat(receiptPath);
    if (!info.isFile() || info.isSymbolicLink() || (info.mode & 0o077) !== 0) throw new Error("local gate receipt must be a private regular file");
    return JSON.parse(await readFile(receiptPath, "utf8"));
  } catch (error) {
    if (error?.code === "ENOENT") return null;
    if (error instanceof SyntaxError) throw new Error("local gate receipt is not valid JSON", { cause: error });
    throw error;
  }
}

async function writeExclusiveJson(destination, value) {
  const handle = await open(destination, "wx", 0o600);
  try {
    await handle.writeFile(`${JSON.stringify(value, null, 2)}\n`, "utf8");
    await handle.sync();
  } finally {
    await handle.close();
  }
  await chmod(destination, 0o600);
}

function assertCleanState(state) {
  if (state.dirty) throw new Error("local gate requires a clean checkout so evidence binds the exact head");
}

export async function verifyLocalGate({ cwd = process.cwd(), deliveryBase, gateId, command }) {
  assertCanonicalGateCommand(deliveryBase, gateId, command);
  const commandDigest = digestGateCommand(command);
  assertGateIdentity({
    headSha: "0".repeat(40),
    deliveryBase,
    deliveryBaseOid: "0".repeat(40),
    gateId,
    commandDigest,
  });
  const state = inspectCheckout(cwd, deliveryBase);
  assertCleanState(state);
  const paths = gatePaths(state.commonDir, state.headSha, gateId);
  const receipt = await readReceipt(paths.receipt);
  if (!receipt) throw new Error(`no local gate receipt exists for ${gateId} at ${state.headSha}`);
  validateLocalGateReceipt(receipt, {
    headSha: state.headSha,
    deliveryBase,
    deliveryBaseOid: state.deliveryBaseOid,
    gateId,
    commandDigest,
  });
  return { ok: true, action: "verified", receipt };
}

export async function runLocalGate({
  cwd = process.cwd(), deliveryBase, gateId, command,
  runChild = runManagedChild, now = () => Date.now(),
}) {
  assertCanonicalGateCommand(deliveryBase, gateId, command);
  const commandDigest = digestGateCommand(command);
  assertGateIdentity({ headSha: "0".repeat(40), deliveryBase, deliveryBaseOid: "0".repeat(40), gateId, commandDigest });
  const before = inspectCheckout(cwd, deliveryBase);
  assertCleanState(before);
  const paths = gatePaths(before.commonDir, before.headSha, gateId);
  await mkdir(paths.directory, { recursive: true, mode: 0o700 });
  await chmod(paths.directory, 0o700);

  const existing = await readReceipt(paths.receipt);
  if (existing) {
    validateLocalGateReceipt(existing, {
      headSha: before.headSha,
      deliveryBase,
      deliveryBaseOid: before.deliveryBaseOid,
      gateId,
      commandDigest,
    });
    return { ok: true, action: "reused", receipt: existing };
  }

  try {
    await mkdir(paths.lock, { mode: 0o700 });
  } catch (error) {
    if (error?.code === "EEXIST") throw new Error(`local gate ${gateId} already has an owned run for ${before.headSha}; reconcile it instead of launching a replacement`);
    throw error;
  }

  try {
    await writeExclusiveJson(path.join(paths.lock, "owner.json"), {
      schema: LOCAL_GATE_SCHEMA,
      pid: process.pid,
      headSha: before.headSha,
      gateId,
    });
    const startedAt = now();
    const exitCode = await runChild(command[0], command.slice(1), {
      cwd: before.root,
      env: process.env,
      label: `local gate ${gateId}`,
    });
    const completedAtMs = now();
    if (exitCode !== 0) return { ok: false, action: "failed", exitCode };

    const after = inspectCheckout(before.root, deliveryBase);
    assertCleanState(after);
    if (after.headSha !== before.headSha || after.deliveryBaseOid !== before.deliveryBaseOid) {
      throw new Error("local gate head or delivery base changed while validation was running");
    }
    const receipt = buildLocalGateReceipt({
      headSha: before.headSha,
      deliveryBase,
      deliveryBaseOid: before.deliveryBaseOid,
      gateId,
      commandDigest,
      durationMs: Math.max(0, completedAtMs - startedAt),
      completedAt: new Date(completedAtMs).toISOString(),
    });
    await writeExclusiveJson(paths.receipt, receipt);
    return { ok: true, action: "ran", receipt };
  } finally {
    await rm(paths.lock, { recursive: true, force: true });
  }
}

function parseCli(argv) {
  const separator = argv.indexOf("--");
  const optionArgs = separator === -1 ? argv : argv.slice(0, separator);
  const command = separator === -1 ? [] : argv.slice(separator + 1);
  const { values, positionals } = parseArgs({
    args: optionArgs,
    options: {
      "delivery-base": { type: "string" },
      "gate-id": { type: "string" },
      help: { type: "boolean", short: "h" },
    },
    allowPositionals: true,
    strict: true,
  });
  return { operation: positionals[0], values, command };
}

const USAGE = `Usage:
  node scripts/loop/local-gate.mjs run --delivery-base origin/<target> --gate-id <id> -- <command> [args...]
  node scripts/loop/local-gate.mjs verify --delivery-base origin/<target> --gate-id <id> -- <command> [args...]

A successful run writes one private exact-head receipt. Repeating the same run
reuses it without launching a child; an in-flight or mismatched receipt stops.`;

export async function runCli(argv = process.argv.slice(2), { cwd = process.cwd(), stdout = process.stdout } = {}) {
  const { operation, values, command } = parseCli(argv);
  if (values.help || !operation) {
    stdout.write(`${USAGE}\n`);
    return 0;
  }
  if (!values["delivery-base"] || !values["gate-id"]) throw new Error("local gate requires --delivery-base and --gate-id");
  let result;
  if (operation === "run") {
    if (command.length === 0) throw new Error("local gate run requires a command after --");
    result = await runLocalGate({ cwd, deliveryBase: values["delivery-base"], gateId: values["gate-id"], command });
  } else if (operation === "verify") {
    if (command.length === 0) throw new Error("local gate verify requires the expected command after --");
    result = await verifyLocalGate({
      cwd,
      deliveryBase: values["delivery-base"],
      gateId: values["gate-id"],
      command,
    });
  } else {
    throw new Error(`unknown local gate operation: ${operation}`);
  }
  stdout.write(`${JSON.stringify(result)}\n`);
  return result.ok ? 0 : result.exitCode ?? 1;
}

if (path.resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  runCli().then((code) => { process.exitCode = code; }).catch((error) => {
    process.stderr.write(`[local-gate] ${error.message}\n`);
    process.exitCode = 1;
  });
}
