#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { execFile } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import { promisify } from "node:util";

import { ensureSharedPiPackageStore } from "../lib/dev-loop-runtime.mjs";

const execFileAsync = promisify(execFile);

async function installIntoStage({ root, configuration, pins }) {
  await mkdir(`${root}/.pi`, { recursive: true, mode: 0o700 });
  await writeFile(`${root}/.pi/settings.json`, `${JSON.stringify({ packages: configuration })}\n`, { mode: 0o600 });
  for (const pin of pins) {
    await execFileAsync("pi", ["install", pin.spec, "--local", "--approve"], {
      cwd: root,
      env: { ...process.env, PI_OFFLINE: "" },
      maxBuffer: 8 * 1024 * 1024,
    });
  }
}

try {
  const result = await ensureSharedPiPackageStore({ install: installIntoStage });
  process.stdout.write(`${JSON.stringify(result)}\n`);
} catch (error) {
  process.stderr.write(`[provision-pi-packages] ${error.message}\n`);
  process.exitCode = 1;
}
