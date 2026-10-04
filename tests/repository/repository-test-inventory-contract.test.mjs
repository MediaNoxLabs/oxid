// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  registeredRepositoryTests,
  validateRepositoryTestInventory,
} from "../../scripts/ci/check-repository-test-inventory.mjs";

const root = new URL("../../", import.meta.url);

test("repository inventory registers every tracked contract exactly once", async () => {
  const [runScript, trackedOutput] = await Promise.all([
    readFile(new URL("run.sh", root), "utf8"),
    (await import("node:child_process")).execFileSync("git", ["ls-files", "tests/repository/*.test.mjs"], {
      cwd: new URL("../../", import.meta.url), encoding: "utf8",
    }),
  ]);
  const trackedTests = trackedOutput.trim().split("\n").filter(Boolean).sort();
  validateRepositoryTestInventory({ trackedTests, registeredTests: registeredRepositoryTests(runScript) });
});

test("repository inventory rejects an orphan and duplicate registration", () => {
  assert.throws(
    () => validateRepositoryTestInventory({
      trackedTests: ["tests/repository/registered.test.mjs", "tests/repository/orphan.test.mjs"],
      registeredTests: ["tests/repository/registered.test.mjs", "tests/repository/registered.test.mjs"],
    }),
    /duplicate registrations: tests\/repository\/registered\.test\.mjs; unregistered repository tests: tests\/repository\/orphan\.test\.mjs/u,
  );
});

test("a contract registered only in another target remains orphaned", () => {
  const script = `run_repository() {\n  node --test tests/repository/registered.test.mjs\n}\nrun_ui() {\n  node --test tests/repository/ui-only.test.mjs\n}\n`;
  assert.deepEqual(registeredRepositoryTests(script), ["tests/repository/registered.test.mjs"]);
  assert.throws(
    () => validateRepositoryTestInventory({
      trackedTests: ["tests/repository/registered.test.mjs", "tests/repository/ui-only.test.mjs"],
      registeredTests: registeredRepositoryTests(script),
    }),
    /unregistered repository tests: tests\/repository\/ui-only\.test\.mjs/u,
  );
});
