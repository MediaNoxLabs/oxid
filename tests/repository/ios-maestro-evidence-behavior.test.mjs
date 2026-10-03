// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";

import { validateIosMaestroEvidence } from "../../scripts/lib/validate-ios-maestro-evidence.mjs";

async function fixture({ entries, files = {}, cleanup = {}, outcomes = [] }) {
  const root = await mkdtemp(path.join(tmpdir(), "oxid-ios-maestro-evidence-"));
  await mkdir(path.join(root, "scenarios"), { recursive: true });
  const manifest = entries.map(JSON.stringify).join("\n") + (entries.length ? "\n" : "");
  await writeFile(path.join(root, "scenarios/manifest.jsonl"), manifest);
  for (const [relative, content] of Object.entries(files)) {
    await mkdir(path.dirname(path.join(root, relative)), { recursive: true });
    await writeFile(path.join(root, relative), content);
  }
  const artifactBytes = Buffer.byteLength(manifest)
    + Object.values(files).reduce((total, content) => total + Buffer.byteLength(content), 0);
  const receipt = {
    schema: "oxid-ios-maestro-evidence-v3",
    oxid: { head: "a".repeat(40), capturePolicy: "holder-public" },
    platform: {
      kind: "ios_simulator",
      viewport: "375-pt-class",
      deviceType: "com.apple.CoreSimulator.SimDeviceType.iPhone-SE-3rd-generation",
    },
    outcome: { passed: true, scenarios: outcomes },
    artifacts: {
      publicBytes: artifactBytes,
      screenshotCount: entries.filter((entry) => entry.kind === "screenshot").length,
      manifest: "scenarios/manifest.jsonl",
    },
    cleanup: {
      receiptOwnedSimulator: true,
      privateDiagnosticsRemoved: true,
      rawArtifactsRemoved: true,
      ...cleanup,
    },
  };
  await writeFile(path.join(root, "receipt.json"), `${JSON.stringify(receipt)}\n`);
  return root;
}

async function rejects(root, pattern) {
  await assert.rejects(validateIosMaestroEvidence(root), pattern);
  await rm(root, { recursive: true, force: true });
}

test("validates retained public artifacts and bounded logs", async () => {
  const entries = [
    { scenario: "home", artifact: "scenarios/home/screenshots/a.png", kind: "screenshot", route: "home", state: "simulated-empty", designReference: "kI1o6I63AKA1Z0AIAwHI", uiProfile: "demo" },
    { scenario: "home", artifact: "scenarios/home/maestro-tail.log", kind: "bounded-log", route: "home", state: "scenario-log", designReference: "no-match", uiProfile: "demo" },
  ];
  const root = await fixture({
    entries,
    files: {
      "scenarios/home/screenshots/a.png": "png",
      "scenarios/home/maestro-tail.log": "one\ntwo\n",
    },
    outcomes: [{ id: "home", outcome: "passed" }],
  });
  const result = await validateIosMaestroEvidence(root);
  assert.equal(result.artifacts, 2);
  assert.equal(result.screenshots, 1);
  assert.ok(result.publicBytes > 0);
  await rm(root, { recursive: true, force: true });
});

test("rejects dangling and escaping artifact paths", async () => {
  await rejects(await fixture({
    entries: [{ scenario: "home", artifact: "scenarios/home/missing.png", kind: "screenshot", route: "home", state: "public-safe", designReference: "no-match", uiProfile: "demo" }],
  }), /dangling/u);
  await rejects(await fixture({
    entries: [{ scenario: "home", artifact: "../outside.log", kind: "bounded-log", route: "home", state: "public-safe", designReference: "no-match", uiProfile: "demo" }],
  }), /escapes|outside scenarios/u);
});

test("rejects unbounded logs and incomplete raw/private cleanup", async () => {
  const artifact = "scenarios/home/maestro-tail.log";
  await rejects(await fixture({
    entries: [{ scenario: "home", artifact, kind: "bounded-log", route: "home", state: "public-safe", designReference: "no-match", uiProfile: "demo" }],
    files: { [artifact]: `${Array.from({ length: 201 }, (_, index) => `line-${index}`).join("\n")}\n` },
  }), /exceeds 200 lines/u);
  await rejects(await fixture({ entries: [], cleanup: { rawArtifactsRemoved: false } }), /cleanup receipt is incomplete/u);
});

test("rejects missing artifact provenance", async () => {
  await rejects(await fixture({
    entries: [{ scenario: "home", artifact: "scenarios/home/screenshots/a.png", kind: "screenshot" }],
    files: { "scenarios/home/screenshots/a.png": "png" },
  }), /has no route/u);
});

test("rejects an unknown design reference and missing UI profile", async () => {
  const artifact = "scenarios/home/screenshots/a.png";
  const base = { scenario: "home", artifact, kind: "screenshot", route: "home", state: "empty" };
  await rejects(await fixture({ entries: [{ ...base, designReference: "unknown-id", uiProfile: "demo" }], files: { [artifact]: "png" } }), /invalid design reference/u);
  await rejects(await fixture({ entries: [{ ...base, designReference: "no-match" }], files: { [artifact]: "png" } }), /valid UI profile/u);
});

test("rejects repeated collection that reuses an artifact path", async () => {
  const entry = { scenario: "home", artifact: "scenarios/home/screenshots/a.png", kind: "screenshot", route: "home", state: "public-safe", designReference: "no-match", uiProfile: "demo" };
  await rejects(await fixture({
    entries: [entry, entry],
    files: { [entry.artifact]: "png" },
  }), /reuses artifact path/u);
});
