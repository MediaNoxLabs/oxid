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

function assertDocumentsSurfacePrimitives(source) {
  for (const [surface, marker] of [
    ["DID capability unavailable", '"data-ui-primitive": "ErrorState"'],
    ['"data-testid": "identity-did-item-{index}"', '"data-ui-primitive": "IdentityCard"'],
    ['"data-testid": "identity-did-detail"', '"data-ui-primitive": "IdentityDetail"'],
  ]) {
    const surfaceOffset = source.indexOf(surface);
    assert.notEqual(surfaceOffset, -1, `missing Documents surface: ${surface}`);
    const start = source.lastIndexOf("article {", surfaceOffset);
    const end = source.indexOf("\n                            }", surfaceOffset);
    assert.ok(source.slice(start, end).includes(marker), `missing primitive marker for ${surface}`);
  }
}

test("Documents surfaces retain their own primitive markers", async () => {
  const dids = await readFile(path.join(repository, "crates", "ui-dioxus", "src", "dids.rs"), "utf8");
  assertDocumentsSurfacePrimitives(dids);

  assert.throws(() => {
    assertDocumentsSurfacePrimitives(
      dids.replace('"data-ui-primitive": "IdentityCard",', ""),
    );
  }, /missing primitive marker/);
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

test("prototype presentation inventory is closed and maps every required area", async () => {
  const inventory = await readFile(
    path.join(repository, "docs", "migration", "prototype-presentation-classification.md"),
    "utf8",
  );
  const rows = inventory.split("\n")
    .filter((line) => line.startsWith("| ") && !line.startsWith("| ---"))
    .slice(1)
    .map((line) => line.split("|").slice(1, -1).map((cell) => cell.trim()));
  const classifications = new Set([
    "Product capability",
    "Reusable engineering pattern",
    "Presentation debt",
    "Prototype-only behavior",
  ]);
  assert.ok(rows.length >= 16, "classification inventory must cover the reviewed surface");
  for (const [area, classification, evidence, authority, disposition] of rows) {
    assert.ok(area && evidence && authority && disposition, `incomplete classification row: ${area}`);
    assert.ok(classifications.has(classification), `unknown classification for ${area}: ${classification}`);
  }
  for (const area of [
    "Profile, wallet, and realm selection",
    "DID inventory and lifecycle",
    "Credential inventory, issuance, presentation, and consent",
    "Android/iOS TLS and trust initialization",
    "UI-thread isolation and bounded workers",
    "Suspend, resume, reconnect, and checkpoint recovery",
    "Safe areas, snapshot privacy, and native lifecycle bridges",
    "QR, app-link, and native identity ingress",
    "Embedded proof demonstration",
    "Persistent free-form logs, arbitrary tracing fields, and process telemetry",
  ]) {
    assert.ok(rows.some(([candidate]) => candidate === area), `missing prototype area: ${area}`);
  }
  assert.match(inventory, /evidence library, not an\nOxid visual or architectural authority/u);
  assert.match(inventory, /scenario inventory contains no supported use\ncase that names it/u);
});
