#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
import { realpathSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { runDevLoopsPackageScript } from "../lib/dev-loop-package-script.mjs";

const script = fileURLToPath(import.meta.url);
const checkout = path.resolve(path.dirname(script), "../..");

if (process.argv[1] && realpathSync(process.argv[1]) === realpathSync(script)) {
  try {
    process.exitCode = await runDevLoopsPackageScript("scripts/github/view-issue.mjs", process.argv.slice(2), {
      cwd: checkout,
    });
  } catch (error) {
    process.stderr.write(`[view-issue] ${error.message}\n`);
    process.exitCode = 1;
  }
}
