#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { closeSync, openSync } from "node:fs";
import path from "node:path";
import process from "node:process";
import { spawn } from "node:child_process";

const [executable, logPath] = process.argv.slice(2);
if (!executable || !logPath || process.argv.length !== 4) {
  process.stderr.write("spawn-detached: expected one executable and one log path\n");
  process.exit(2);
}
if (!path.isAbsolute(executable) || !path.isAbsolute(logPath)) {
  process.stderr.write("spawn-detached: paths must be absolute\n");
  process.exit(2);
}

let log;
try {
  log = openSync(logPath, "a", 0o600);
  const child = spawn(executable, [], {
    detached: true,
    env: process.env,
    stdio: ["ignore", log, log],
  });
  child.once("error", (error) => {
    process.stderr.write(`spawn-detached: ${error.code ?? "launch-failed"}\n`);
    closeSync(log);
    process.exit(1);
  });
  child.once("spawn", () => {
    process.stdout.write(`${child.pid}\n`);
    child.unref();
    closeSync(log);
  });
} catch (error) {
  process.stderr.write(`spawn-detached: ${error.code ?? "launch-failed"}\n`);
  if (log !== undefined) closeSync(log);
  process.exit(1);
}
