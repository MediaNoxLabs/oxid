// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import { chmod, copyFile, mkdir, mkdtemp, readFile, rm, stat, writeFile } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

const repositoryRoot = new URL("../../", import.meta.url).pathname;
const sourceScript = join(repositoryRoot, "scripts/portal-tailnet-manual-phases.sh");
const head = "1111111111111111111111111111111111111111";
const tree = "2222222222222222222222222222222222222222";

async function executable(path, contents) {
  await writeFile(path, contents);
  await chmod(path, 0o755);
}

async function fixture() {
  const root = await mkdtemp(join(tmpdir(), "oxid-portal-phases-"));
  const scripts = join(root, "scripts");
  const bin = join(root, "bin");
  const sdk = join(root, "android-sdk");
  const state = join(root, "phase-state");
  const manifest = join(root, "deployment.json");
  const runnerLog = join(root, "runner.log");
  const signerLog = join(root, "signer.log");
  const adbLog = join(root, "adb.log");
  await mkdir(scripts, { recursive: true });
  await mkdir(bin, { recursive: true });
  await mkdir(join(sdk, "build-tools/35.0.0"), { recursive: true });
  await mkdir(join(sdk, "platform-tools"), { recursive: true });
  await copyFile(sourceScript, join(scripts, "portal-tailnet-manual-phases.sh"));
  await chmod(join(scripts, "portal-tailnet-manual-phases.sh"), 0o755);
  await writeFile(manifest, `${JSON.stringify({
    schema: "oxid-portal-deployment-v3",
    issuerOrigin: "https://wallet.tail.example:11003",
    issuerDid: "did:midnight:issuer",
    issuerMethod: "did:midnight:issuer#key-1",
    issuerJubjubJwkSha256: "a".repeat(64),
  })}\n`);
  await executable(join(bin, "git"), `#!/usr/bin/env bash
case "$*" in
  *"HEAD^{tree}"*) echo ${tree} ;;
  *"rev-parse HEAD"*) echo ${head} ;;
  *) exit 1 ;;
esac
`);
  await executable(join(scripts, "run-android-tailnet.sh"), `#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"$RUNNER_LOG"
[ "$1" = build ] || exit 91
root="$(cd -- "${'$'}{BASH_SOURCE[0]%/*}/.." && pwd -P)"
apk="$root/target/dx/oxid-app/debug/android/app/app/build/outputs/apk/debug/app-debug.apk"
receipt="$root/target/dx/oxid-app/debug/android/oxid-app-artifact-receipt.json"
mkdir -p "${'$'}{apk%/*}"
printf 'signed-apk' >"$apk"
digest="$(shasum -a 256 "$apk" | awk '{print $1}')"
jq -cn --arg head ${head} --arg tree ${tree} --arg artifact "$apk" --arg digest "$digest" \
  '{schema:"oxid-app-artifact-receipt-v1",repository:"MediaNoxLabs/oxid",head:$head,tree:$tree,sourceSha256:"source",platform:"android",target:"aarch64-linux-android",configuration:"mobile,standalone-development,standalone-portal-tailnet|ui=user|custody=development|network=tailnet|tailnet=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa|portal=tailnet-android|preprod=0|proving=disabled",artifact:$artifact,artifactSha256:$digest}' >"$receipt"
chmod 600 "$receipt"
`);
  await executable(join(sdk, "build-tools/35.0.0/apksigner"), `#!/usr/bin/env bash
printf '%s\n' "$*" >>"$SIGNER_LOG"
[ "${'$'}{SIGN_FAIL:-0}" != 1 ]
`);
  await executable(join(sdk, "platform-tools/adb"), `#!/usr/bin/env bash
printf '%s\n' "$*" >>"$ADB_LOG"
case "$*" in
  devices) printf 'List of devices attached\nPHYSICAL-1\tdevice\n' ;;
  "shell getprop ro.kernel.qemu") echo 0 ;;
  "shell pidof io.medianox.oxid") echo 4242 ;;
esac
`);
  for (const log of [runnerLog, signerLog, adbLog]) await writeFile(log, "");
  const env = {
    ...process.env,
    PATH: `${bin}:${process.env.PATH}`,
    ANDROID_HOME: sdk,
    OXID_PORTAL_PHASE_STATE_DIR: state,
    OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_PATH: manifest,
    RUNNER_LOG: runnerLog,
    SIGNER_LOG: signerLog,
    ADB_LOG: adbLog,
  };
  const run = (phase, overrides = {}) => spawnSync(join(scripts, "portal-tailnet-manual-phases.sh"), [phase], {
    cwd: root,
    env: { ...env, ...overrides },
    encoding: "utf8",
  });
  return { root, state, manifest, runnerLog, signerLog, adbLog, run };
}

async function prepareBuild(value) {
  assert.equal(value.run("configure").status, 0);
  assert.equal(value.run("build").status, 0);
}

test("manual Portal phases resume idempotently and never clear application data", async () => {
  const value = await fixture();
  try {
    for (const phase of ["configure", "configure", "build", "build", "admit", "admit", "install", "install", "launch", "launch", "status"]) {
      const result = value.run(phase);
      assert.equal(result.status, 0, `${phase}: ${result.stderr}\n${result.stdout}`);
    }
    assert.equal(await readFile(value.runnerLog, "utf8"), "build\n");
    assert.equal((await readFile(value.signerLog, "utf8")).trim().split("\n").length, 1);
    const adb = await readFile(value.adbLog, "utf8");
    assert.equal((adb.match(/install -r/gu) ?? []).length, 1);
    assert.equal((adb.match(/shell am start/gu) ?? []).length, 1);
    assert.doesNotMatch(adb, /pm clear|uninstall/u);
    for (const phase of ["configure", "build", "admit", "install", "launch"]) {
      const receipt = join(value.state, `${phase}-receipt.json`);
      assert.equal((await stat(receipt)).mode & 0o777, 0o600);
      assert.doesNotMatch(await readFile(receipt, "utf8"), /privateKey|wallet_seed|secret|token/u);
    }
  } finally {
    await rm(value.root, { recursive: true, force: true });
  }
});

test("manual Portal admission rejects a stale predecessor before signing or device mutation", async () => {
  const value = await fixture();
  try {
    await prepareBuild(value);
    const path = join(value.state, "build-receipt.json");
    const receipt = JSON.parse(await readFile(path, "utf8"));
    receipt.predecessor.configureSha256 = "0".repeat(64);
    await writeFile(path, `${JSON.stringify(receipt)}\n`, { mode: 0o600 });
    await chmod(path, 0o600);
    const result = value.run("admit");
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /FAIL phase=build-receipt/u);
    assert.equal(await readFile(value.signerLog, "utf8"), "");
    assert.equal(await readFile(value.adbLog, "utf8"), "");
  } finally {
    await rm(value.root, { recursive: true, force: true });
  }
});

test("manual Portal phases reject manifest schema and artifact profile drift", async () => {
  const invalidManifest = await fixture();
  try {
    await writeFile(invalidManifest.manifest, '{"schema":"unexpected"}\n');
    const result = invalidManifest.run("configure");
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /FAIL phase=manifest-schema/u);
  } finally {
    await rm(invalidManifest.root, { recursive: true, force: true });
  }

  const invalidProfile = await fixture();
  try {
    await prepareBuild(invalidProfile);
    const path = join(invalidProfile.root, "target/dx/oxid-app/debug/android/oxid-app-artifact-receipt.json");
    const receipt = JSON.parse(await readFile(path, "utf8"));
    receipt.configuration = receipt.configuration.replace("portal=tailnet-android", "portal=unavailable");
    await writeFile(path, `${JSON.stringify(receipt)}\n`, { mode: 0o600 });
    await chmod(path, 0o600);
    const result = invalidProfile.run("admit");
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /FAIL phase=build-receipt/u);
    assert.equal(await readFile(invalidProfile.signerLog, "utf8"), "");
    assert.equal(await readFile(invalidProfile.adbLog, "utf8"), "");
  } finally {
    await rm(invalidProfile.root, { recursive: true, force: true });
  }
});

test("manual Portal signing failure is reported before any device mutation", async () => {
  const value = await fixture();
  try {
    await prepareBuild(value);
    const result = value.run("admit", { SIGN_FAIL: "1" });
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /FAIL phase=signing/u);
    assert.equal(await readFile(value.adbLog, "utf8"), "");
  } finally {
    await rm(value.root, { recursive: true, force: true });
  }
});
