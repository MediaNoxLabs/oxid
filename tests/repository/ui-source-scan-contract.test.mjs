// SPDX-License-Identifier: Apache-2.0

import { spawnSync } from "node:child_process";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import assert from "node:assert/strict";

const repository = path.resolve(import.meta.dirname, "../..");
const cssCheck = path.join(repository, "scripts/check-ui-css-classes.sh");
const copyCheck = path.join(repository, "scripts/check-ui-copy-labels.sh");

async function fixture() {
  const root = await mkdtemp(path.join(os.tmpdir(), "oxid-ui-source-scan-"));
  const sourceRoot = path.join(root, "src");
  await mkdir(sourceRoot);
  await writeFile(path.join(sourceRoot, "lib.rs"), "// root module\n");
  return { root, sourceRoot };
}

function run(script, environment) {
  return spawnSync(script, [], {
    cwd: repository,
    encoding: "utf8",
    env: { ...process.env, ...environment },
  });
}

test("CSS validation includes extracted sibling modules", async () => {
  const { root, sourceRoot } = await fixture();
  try {
    const stylesheet = path.join(root, "styles.css");
    await writeFile(path.join(sourceRoot, "diagnostics.rs"), 'const VIEW: &str = r#"class: "extracted-card""#;\n');
    await writeFile(stylesheet, ".existing-card {}\n");

    const rejected = run(cssCheck, {
      OXID_UI_SOURCE_ROOT: sourceRoot,
      OXID_UI_STYLESHEET: stylesheet,
    });
    assert.notEqual(rejected.status, 0);
    assert.match(rejected.stderr, /extracted-card/);

    await writeFile(stylesheet, ".existing-card {}\n.extracted-card {}\n");
    const accepted = run(cssCheck, {
      OXID_UI_SOURCE_ROOT: sourceRoot,
      OXID_UI_STYLESHEET: stylesheet,
    });
    assert.equal(accepted.status, 0, accepted.stderr);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("copy validation includes extracted sibling modules", async () => {
  const { root, sourceRoot } = await fixture();
  try {
    const sibling = path.join(sourceRoot, "diagnostics.rs");
    await writeFile(sibling, 'const COPY: &str = "12 atomic units";\n');

    const rejected = run(copyCheck, { OXID_UI_SOURCE_ROOT: sourceRoot });
    assert.notEqual(rejected.status, 0);
    assert.match(rejected.stderr, /atomic units/);
    assert.match(rejected.stderr, /diagnostics\.rs/);

    await writeFile(sibling, 'const COPY: &str = "12 NIGHT";\n');
    const accepted = run(copyCheck, { OXID_UI_SOURCE_ROOT: sourceRoot });
    assert.equal(accepted.status, 0, accepted.stderr);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
