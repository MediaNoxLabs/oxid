// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

test("native transport trust guard has a portable hosted-runner search fallback", async () => {
  const source = await readFile(path.join(repoRoot, "scripts", "check-transport-trust.sh"), "utf8");
  assert.match(source, /command -v rg/u);
  assert.match(source, /grep -R "\$mode" -E --include='\*\.rs'/u);
  assert.match(source, /search_rust_sources -n/u);
  assert.match(source, /search_rust_sources -l/u);
  assert.match(source, /search_file_quietly/u);
});
