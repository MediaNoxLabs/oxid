#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { execFileSync } from "node:child_process";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

export function registeredRepositoryTests(runScript) {
  return [...runScript.matchAll(/^\s*node --test (tests\/repository\/[^\s]+\.test\.mjs)\s*$/gmu)]
    .map((match) => match[1]);
}

export function validateRepositoryTestInventory({ trackedTests, registeredTests }) {
  const duplicates = registeredTests.filter((test, index) => registeredTests.indexOf(test) !== index);
  const registered = new Set(registeredTests);
  const tracked = new Set(trackedTests);
  const unregistered = trackedTests.filter((test) => !registered.has(test));
  const unknown = registeredTests.filter((test) => !tracked.has(test));
  if (duplicates.length > 0 || unregistered.length > 0 || unknown.length > 0) {
    const details = [
      duplicates.length > 0 && `duplicate registrations: ${[...new Set(duplicates)].join(", ")}`,
      unregistered.length > 0 && `unregistered repository tests: ${unregistered.join(", ")}`,
      unknown.length > 0 && `non-tracked registrations: ${[...new Set(unknown)].join(", ")}`,
    ].filter(Boolean).join("; ");
    throw new Error(`repository test inventory failed: ${details}`);
  }
}

async function main() {
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
  const [runScript, trackedOutput] = await Promise.all([
    readFile(path.join(root, "run.sh"), "utf8"),
    Promise.resolve(execFileSync("git", ["ls-files", "tests/repository/*.test.mjs"], { cwd: root, encoding: "utf8" })),
  ]);
  const trackedTests = trackedOutput.trim().split("\n").filter(Boolean).sort();
  validateRepositoryTestInventory({ trackedTests, registeredTests: registeredRepositoryTests(runScript) });
  console.log(`repository test inventory PASS: ${trackedTests.length} tracked tests registered exactly once`);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
