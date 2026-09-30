// SPDX-License-Identifier: Apache-2.0

import { spawnSync } from "node:child_process";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
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

test("settings and developer surfaces expose stable privacy-safe contracts", async () => {
  const sourceRoot = path.join(repository, "crates", "ui-dioxus", "src");
  const sourceFiles = [
    "lib.rs",
    "profile_quick_switcher.rs",
    "diagnostics.rs",
    "developer_tools.rs",
    "proof_benchmark.rs",
  ];
  const productionSources = await Promise.all(sourceFiles.map(async (file) => {
    const source = await readFile(path.join(sourceRoot, file), "utf8");
    return source.split("\n#[cfg(test)]\nmod tests {")[0];
  }));
  const sources = productionSources.join("\n");
  for (const required of [
    "settings-hub",
    "settings-security",
    "settings-backup",
    "settings-recovery",
    "settings-preferences",
    "settings-about",
    "profile-management",
    "diagnostics-overview",
    "developer-tools-hub",
    "capability-manifest",
    "proof-benchmark",
    "event-log",
    "data-view-state",
    "data-action",
    '"IconButton"',
    '"DetailDisclosure"',
    '"StateSurface"',
    '"TaskRow"',
  ]) {
    assert.ok(sources.includes(required), `missing UI contract: ${required}`);
  }
  assert.ok(!productionSources[0].includes("30 seconds"));
  assert.ok(!productionSources[0].includes("30-second"));
});
