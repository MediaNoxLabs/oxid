#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { execFileSync } from "node:child_process";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

import { supervise } from "./ios-xcode-supervisor.mjs";

const DELIVERY_BASE = /^origin\/(?:develop|milestone-(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*))$/u;
const HEAD = /^[0-9a-f]{40}$/u;

export function parseHostMobileArgs(argv) {
  const split = argv.indexOf("--");
  if (split < 0 || split === argv.length - 1) throw new Error("missing-command");
  const args = [];
  let deliveryBase;
  for (let index = 0; index < split; index += 1) {
    const argument = argv[index];
    if (argument === "--delivery-base") {
      if (deliveryBase !== undefined) throw new Error("duplicate-delivery-base");
      deliveryBase = argv[index + 1];
      if (!deliveryBase || deliveryBase.startsWith("--")) throw new Error("missing-delivery-base");
      index += 1;
    } else if (argument.startsWith("--delivery-base=")) {
      if (deliveryBase !== undefined) throw new Error("duplicate-delivery-base");
      deliveryBase = argument.slice("--delivery-base=".length);
    } else {
      args.push(argument);
    }
  }
  if (!DELIVERY_BASE.test(deliveryBase ?? "")) throw new Error("invalid-delivery-base");
  return { deliveryBase, args: [...args, "--", ...argv.slice(split + 1)] };
}

function command(commandName, args, options = {}) {
  return execFileSync(commandName, args, {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
    ...options,
  });
}

function fixedReason(error) {
  if (/contention|lease-busy/u.test(error?.message ?? "")) return "occupied-lane";
  if (/local-gate/u.test(error?.message ?? "")) return "missing-local-gate";
  return "admission-failed";
}

export async function runHostMobile(argv, {
  cwd = process.cwd(),
  verifyGate = (deliveryBase) => command(process.execPath, [
    path.join(cwd, "scripts/loop/local-gate.mjs"), "verify",
    "--delivery-base", deliveryBase, "--gate-id", "production-ready",
  ], { cwd }),
  readHead = () => command("git", ["-C", cwd, "rev-parse", "HEAD"]).trim(),
  runSupervised = (args) => supervise(args),
  stderr = process.stderr,
  now = () => Date.now(),
} = {}) {
  const started = now();
  let parsed;
  let head = "unresolved";
  let supervisorStarted = false;
  try {
    parsed = parseHostMobileArgs(argv);
    const supervisorArgs = parsed.args.slice(0, parsed.args.indexOf("--"));
    const cwdPositions = supervisorArgs.flatMap((value, index) => value === "--cwd" ? [index] : []);
    if (cwdPositions.length !== 1 || parsed.args[cwdPositions[0] + 1] === undefined
        || path.resolve(parsed.args[cwdPositions[0] + 1]) !== path.resolve(cwd)) {
      throw new Error("child-cwd-mismatch");
    }
    head = readHead();
    if (!HEAD.test(head)) throw new Error("invalid-head");
    try {
      verifyGate(parsed.deliveryBase);
    } catch (error) {
      throw new Error("local-gate-unavailable", { cause: error });
    }
    supervisorStarted = true;
    const code = await runSupervised(parsed.args);
    stderr.write(`factory-metrics phase=host-mobile-admission result=${code === 0 ? "passed" : "failed"} reason=${code === 0 ? "none" : "child-failed"} head=${head} cleanup=released duration_ms=${Math.max(0, now() - started)}\n`);
    return code;
  } catch (error) {
    const reason = fixedReason(error);
    const cleanup = !supervisorStarted || reason === "occupied-lane" ? "not-acquired" : "supervisor-owned";
    stderr.write(`factory-metrics phase=host-mobile-admission result=rejected reason=${reason} head=${head} cleanup=${cleanup} duration_ms=${Math.max(0, now() - started)}\n`);
    throw error;
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  runHostMobile(process.argv.slice(2)).then((code) => {
    process.exitCode = code;
  }, (error) => {
    process.stderr.write(`host-mobile-supervisor: FAIL classification=${fixedReason(error)}\n`);
    process.exitCode = /contention|lease-busy|local-gate/u.test(error?.message ?? "") ? 75 : 1;
  });
}
