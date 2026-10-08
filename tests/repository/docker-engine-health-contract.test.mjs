// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { chmod, mkdtemp, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const helper = path.join(root, "scripts", "lib", "docker-engine-health.sh");

async function classify(t, dockerBody, timeoutSeconds = "1") {
  const directory = await mkdtemp(path.join(os.tmpdir(), "oxid-docker-health-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const docker = path.join(directory, "docker");
  await writeFile(docker, `#!/usr/bin/env bash\n${dockerBody}\n`, { mode: 0o755 });
  await chmod(docker, 0o755);
  return spawnSync("bash", ["-c", [
    `source ${JSON.stringify(helper)}`,
    `PATH=${JSON.stringify(directory)}:$PATH`,
    "export PATH",
    `OXID_DOCKER_PROBE_TIMEOUT_SECONDS=${JSON.stringify(timeoutSeconds)}`,
    "export OXID_DOCKER_PROBE_TIMEOUT_SECONDS",
    "oxid_require_docker_engine",
  ].join("\n")], { cwd: root, encoding: "utf8", timeout: 5_000 });
}

test("Docker admission classifies a responsive engine without mutation", async (t) => {
  const result = await classify(t, "test \"$1\" = info");
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /READY state=responsive/);
});

test("Docker admission distinguishes unavailable and unresponsive engines", async (t) => {
  const unavailable = await classify(t, "exit 1");
  assert.equal(unavailable.status, 1);
  assert.match(unavailable.stderr, /state=unavailable remediation=start-docker-desktop/);

  const unresponsive = await classify(t, "sleep 5", "0.1");
  assert.equal(unresponsive.status, 2);
  assert.match(unresponsive.stderr, /state=unresponsive remediation=restart-docker-desktop/);
});

test("Portal preparation entrypoints use the bounded Docker admission", async () => {
  const { readFile } = await import("node:fs/promises");
  const [taskflow, physical, consumer, runner] = await Promise.all([
    readFile(path.join(root, "scripts", "factory", "portal-tailnet-taskflow-step.sh"), "utf8"),
    readFile(path.join(root, "scripts", "test-android-portal-tailnet-physical.sh"), "utf8"),
    readFile(path.join(root, "scripts", "portal-consumer-lifecycle.sh"), "utf8"),
    readFile(path.join(root, "run.sh"), "utf8"),
  ]);
  for (const source of [taskflow, physical, consumer]) {
    assert.match(source, /docker-engine-health\.sh/);
    assert.match(source, /oxid_require_docker_engine/);
  }
  assert.doesNotMatch(taskflow, /docker info/);
  assert.match(physical, /oxid_docker_read ps -a/);
  assert.match(runner, /docker-engine-health-contract\.test\.mjs/);
});
