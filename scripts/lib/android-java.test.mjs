// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { chmod, mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const policy = path.join(root, "scripts", "lib", "android-java.sh");

async function fakeJdk(parent, name) {
  const home = path.join(parent, name);
  const binary = path.join(home, "bin", "java");
  await mkdir(path.dirname(binary), { recursive: true });
  await writeFile(binary, "#!/bin/sh\nexit 0\n", { mode: 0o755 });
  await writeFile(path.join(home, "release"), 'JAVA_VERSION="17.0.12"\n');
  await chmod(binary, 0o755);
  return home;
}

function select(environment) {
  return spawnSync("bash", ["-c", `set -e; source "$1"; oxid_android_java_major() { printf '%s' "$OXID_TEST_JAVA_MAJOR"; }; oxid_android_select_java; printf '%s|%s' "$JAVA_HOME" "$OXID_ANDROID_JAVA_MAJOR"`, "android-java-test", policy], {
    encoding: "utf8",
    env: environment,
  });
}

test("explicit JDK 17 is selected and exported", async (t) => {
  const directory = await mkdtemp(path.join(tmpdir(), "oxid-android-java-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const jdk17 = await fakeJdk(directory, "jdk17");
  const result = select({ ...process.env, OXID_ANDROID_JAVA_HOME: jdk17, OXID_TEST_JAVA_MAJOR: "17", JAVA_HOME: "" });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.stdout, `${jdk17}|17`);
});

test("unsupported explicit JDK fails before Gradle can run", async (t) => {
  const directory = await mkdtemp(path.join(tmpdir(), "oxid-android-java-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const jdk26 = await fakeJdk(directory, "jdk26");
  const result = select({ ...process.env, OXID_ANDROID_JAVA_HOME: jdk26, OXID_TEST_JAVA_MAJOR: "26", JAVA_HOME: "" });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /must name a JDK 17 home/);
});

test("an ambient JDK 17 is accepted without host-path inference", async (t) => {
  const directory = await mkdtemp(path.join(tmpdir(), "oxid-android-java-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const jdk17 = await fakeJdk(directory, "ambient-jdk17");
  const result = select({ ...process.env, OXID_ANDROID_JAVA_HOME: "", OXID_TEST_JAVA_MAJOR: "17", JAVA_HOME: jdk17 });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.stdout, `${jdk17}|17`);
});
