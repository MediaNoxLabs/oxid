// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { chmod, mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { artifactSha256, sourceSha256 } from "./app-artifact-receipt.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const script = path.join(root, "scripts", "app-artifact-receipt.mjs");

function invoke(operation, temporaryRoot, overrides = {}) {
  const values = {
    platform: "ios-simulator",
    artifact: path.join(temporaryRoot, "OxidApp.app"),
    target: "aarch64-apple-ios-sim",
    configuration: "mobile,standalone-development|user|development|simulated|unavailable",
    receipt: path.join(temporaryRoot, "receipt.json"),
    ...overrides,
  };
  return spawnSync(process.execPath, [
    script,
    operation,
    "--platform", values.platform,
    "--artifact", values.artifact,
    "--target", values.target,
    "--configuration", values.configuration,
    "--receipt", values.receipt,
  ], { cwd: root, encoding: "utf8" });
}

test("receipt binds a private manifest to the exact source, configuration, and bundle", async () => {
  const temporaryRoot = await mkdtemp(path.join(tmpdir(), "oxid-artifact-receipt-"));
  try {
    const bundle = path.join(temporaryRoot, "OxidApp.app");
    await mkdir(path.join(bundle, "Frameworks"), { recursive: true });
    await writeFile(path.join(bundle, "OxidApp"), "binary-one", { mode: 0o755 });
    await writeFile(path.join(bundle, "Frameworks", "library"), "library-one");

    const writeResult = invoke("write", temporaryRoot);
    assert.equal(writeResult.status, 0, writeResult.stderr);
    const receipt = JSON.parse(await readFile(path.join(temporaryRoot, "receipt.json"), "utf8"));
    assert.equal(receipt.head, execFileSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8" }).trim());
    assert.equal(receipt.sourceSha256, sourceSha256());
    assert.equal(receipt.artifactSha256, artifactSha256(bundle));

    const verifyResult = invoke("verify", temporaryRoot);
    assert.equal(verifyResult.status, 0, verifyResult.stderr);

    await writeFile(path.join(bundle, "Frameworks", "library"), "modified");
    const modifiedResult = invoke("verify", temporaryRoot);
    assert.notEqual(modifiedResult.status, 0);
    assert.match(modifiedResult.stderr, /does not match/);

    await writeFile(path.join(bundle, "Frameworks", "library"), "library-one");
    const wrongConfiguration = invoke("verify", temporaryRoot, { configuration: "different" });
    assert.notEqual(wrongConfiguration.status, 0);
    assert.match(wrongConfiguration.stderr, /does not match/);

    await chmod(path.join(temporaryRoot, "receipt.json"), 0o644);
    const publicReceipt = invoke("verify", temporaryRoot);
    assert.notEqual(publicReceipt.status, 0);
    assert.match(publicReceipt.stderr, /private regular/);
  } finally {
    await rm(temporaryRoot, { recursive: true, force: true });
  }
});

test("directory hashing is deterministic and observes executable mode", async () => {
  const temporaryRoot = await mkdtemp(path.join(tmpdir(), "oxid-artifact-hash-"));
  try {
    const bundle = path.join(temporaryRoot, "bundle");
    await mkdir(bundle);
    const executable = path.join(bundle, "app");
    await writeFile(executable, "same bytes", { mode: 0o644 });
    const first = artifactSha256(bundle);
    assert.equal(artifactSha256(bundle), first);
    await chmod(executable, 0o755);
    assert.notEqual(artifactSha256(bundle), first);
  } finally {
    await rm(temporaryRoot, { recursive: true, force: true });
  }
});

test("target recipes preserve run and expose receipt-gated build and deploy modes", async () => {
  const [justfile, android, ios, runScript, guide] = await Promise.all([
    readFile(path.join(root, "Justfile"), "utf8"),
    readFile(path.join(root, "scripts", "run-android-emulator.sh"), "utf8"),
    readFile(path.join(root, "scripts", "run-ios-simulator.sh"), "utf8"),
    readFile(path.join(root, "run.sh"), "utf8"),
    readFile(path.join(root, "docs", "factory", "application-targets.md"), "utf8"),
  ]);
  for (const recipe of [
    "desktop-build:",
    "desktop-run:",
    "android-build:",
    "android-deploy:",
    "android-run:",
    "ios-build:",
    "ios-deploy:",
    "ios-run:",
  ]) assert.match(justfile, new RegExp(`^${recipe}`, "m"));
  for (const launcher of [android, ios]) {
    assert.match(launcher, /build\|deploy\|run/);
    assert.match(launcher, /if \[ "\$operation" != "deploy" \]; then/);
    assert.match(launcher, /app-artifact-receipt\.mjs" write/);
    assert.match(launcher, /app-artifact-receipt\.mjs" verify/);
    assert.match(launcher, /if \[ "\$operation" = "build" \]; then/);
    assert.match(launcher, /if \[ "\$operation" = "deploy" \]; then/);
  }
  assert.match(guide, /Physical iOS deployment is not\s+implemented/);
  const registration = "node --test scripts/app-artifact-receipt.test.mjs";
  assert.equal(runScript.split(registration).length - 1, 1);
});
