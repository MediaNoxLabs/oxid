#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { execFileSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const FACTORY_STATE_LABELS = Object.freeze([
  { name: "factory:ready", color: "0e8a16", description: "Factory item passed ready-check and may be claimed" },
  { name: "factory:claimed", color: "fbca04", description: "Factory claim lease is held; draft PR not open" },
  { name: "factory:in-progress", color: "1d76db", description: "Factory item has an active draft PR" },
  { name: "factory:gate-draft", color: "5319e7", description: "Factory item is in bounded draft review" },
  { name: "factory:gate-preapproval", color: "6f42c1", description: "Factory item is in final pre-approval review" },
  { name: "factory:merge-ready", color: "1f883d", description: "Factory item has complete exact-head delivery evidence" },
  { name: "factory:blocked", color: "d73a4a", description: "Factory item is blocked with a recorded reason" },
]);

export function syncFactoryLabels({ repository = "MediaNoxLabs/oxid", execute = false, run = execFileSync, stdout = process.stdout } = {}) {
  if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/u.test(repository)) throw new Error("--repo must be OWNER/REPO");
  for (const label of FACTORY_STATE_LABELS) {
    stdout.write(`${execute ? "sync" : "would sync"} ${label.name}\n`);
    if (execute) {
      run("gh", ["label", "create", label.name, "--repo", repository, "--color", label.color, "--description", label.description, "--force"], {
        stdio: "inherit",
      });
    }
  }
}

function main(argv = process.argv.slice(2)) {
  const repositoryIndex = argv.indexOf("--repo");
  const repository = repositoryIndex >= 0 ? argv[repositoryIndex + 1] : "MediaNoxLabs/oxid";
  syncFactoryLabels({ repository, execute: argv.includes("--execute") });
}

const directPath = process.argv[1] ? path.resolve(process.argv[1]) : "";
if (directPath === fileURLToPath(import.meta.url)) {
  try {
    main();
  } catch (error) {
    process.stderr.write(`[sync-factory-labels] ${error.message}\n`);
    process.exitCode = 1;
  }
}
