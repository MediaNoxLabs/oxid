// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import path from "node:path";
import test from "node:test";

import { applyOxidSanctionedCommandOverrides } from "../../scripts/dev-loops.mjs";
import { runDevLoopsPackageScript } from "../../scripts/lib/dev-loop-package-script.mjs";
import { resolveDevLoopsPackageRoot } from "../../scripts/lib/dev-loop-runtime.mjs";

const repoRoot = path.resolve(import.meta.dirname, "../..");

test("every sanctioned envelope command resolves to a repository entrypoint", async () => {
  const { packageRoot } = await resolveDevLoopsPackageRoot({ cwd: repoRoot });
  const { SANCTIONED_COMMANDS } = await import(path.join(packageRoot, "scripts/loop/sanctioned-commands.mjs"));
  const commands = applyOxidSanctionedCommandOverrides({ sanctionedCommands: SANCTIONED_COMMANDS }).sanctionedCommands;

  for (const group of ["reads", "edits", "lifecycle"]) {
    for (const [operation, command] of Object.entries(commands[group])) {
      const entrypoint = command.split(/\s+/, 1)[0];
      assert.ok(existsSync(path.join(repoRoot, entrypoint)), `${group}.${operation} advertises missing ${entrypoint}`);
    }
  }
});

test("package-script forwarding rejects traversal before execution", async () => {
  await assert.rejects(
    runDevLoopsPackageScript("scripts/github/../package.json", [], { cwd: repoRoot }),
    /invalid dev-loops package script/,
  );
});
