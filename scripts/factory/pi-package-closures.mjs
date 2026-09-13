#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { auditPiPackageClosures, cleanupPiPackageClosures } from "../lib/dev-loop-runtime.mjs";

function usage() {
  process.stdout.write("Usage: node scripts/factory/pi-package-closures.mjs <audit|cleanup> [--older-than-days N] [--execute]\n");
}

function options(argv) {
  const command = argv[0] ?? "audit";
  if (!(["audit", "cleanup"].includes(command))) throw new Error("unknown command");
  let execute = false;
  let hasDays = false;
  let days = 7;
  for (let index = 1; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--execute" && !execute) {
      execute = true;
      continue;
    }
    if (argument === "--older-than-days" && !hasDays) {
      const value = argv[index + 1];
      if (!value || value.startsWith("-")) throw new Error("--older-than-days requires one positive value");
      days = Number(value);
      hasDays = true;
      index += 1;
      continue;
    }
    throw new Error(`unknown or repeated option: ${argument}`);
  }
  if (!Number.isFinite(days) || days < 1) throw new Error("--older-than-days must be at least 1");
  return { command, execute, olderThanMs: days * 24 * 60 * 60_000 };
}

try {
  const parsed = options(process.argv.slice(2));
  if (parsed.command === "cleanup" && !parsed.execute) throw new Error("refusing cleanup without --execute");
  const result = parsed.command === "audit"
    ? await auditPiPackageClosures({ olderThanMs: parsed.olderThanMs })
    : await cleanupPiPackageClosures({ olderThanMs: parsed.olderThanMs });
  if (result.cleanupBlocked) throw new Error("refusing cleanup: one or more registered worktree settings are malformed");
  process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
} catch (error) {
  process.stderr.write(`[pi-package-closures] ${error.message}\n`);
  process.exitCode = 1;
}
