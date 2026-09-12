// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { chmod, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const bootstrapSource = await readFile(path.join(repoRoot, "bootstrap.sh"), "utf8");

async function fixture(t, { daemonProfile } = {}) {
  const root = await mkdtemp(path.join(os.tmpdir(), "oxid-bootstrap-nix-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const bootstrap = path.join(root, "bootstrap.sh");
  const source = daemonProfile
    ? bootstrapSource.replace("/nix/var/nix/profiles/default/bin", daemonProfile)
    : bootstrapSource;
  await writeFile(bootstrap, source, { mode: 0o755 });
  return { root, bootstrap };
}

async function writeFakeNix(directory) {
  await mkdir(directory, { recursive: true });
  const nix = path.join(directory, "nix");
  await writeFile(nix, `#!/bin/bash
printf '%s' "$PATH" > "$NIX_INVOCATION_PATH"
[ "$1" = develop ] || exit 90
shift
[ "$1" = --command ] || exit 91
shift
[ "$1" = bash ] && [ "$2" = -c ] || exit 92
bootstrap_command="$3"
shift 4
PATH="$FAKE_DEVSHELL_PATH"
export PATH
exec() {
  [ "$1" = bash ] && [ "$2" = -c ] || command exec "$@"
  command_text="$3"
  shift 3
  eval "$command_text"
}
eval "$bootstrap_command"
`);
  await chmod(nix, 0o755);
}

function run(bootstrap, env) {
  return spawnSync("/usr/bin/env", [
    "-i",
    ...Object.entries(env).map(([name, value]) => `${name}=${value}`),
    "/bin/bash", bootstrap, "--", "bash", "-c", 'printf %s "$PATH" > "$NESTED_PATH"',
  ], { encoding: "utf8" });
}

test("bootstrap preserves an ambient nix without prepending the daemon profile", async (t) => {
  const { root, bootstrap } = await fixture(t);
  const ambient = path.join(root, "ambient");
  const daemon = path.join(root, "daemon");
  await Promise.all([writeFakeNix(ambient), writeFakeNix(daemon)]);
  const invocation = path.join(root, "invocation");
  const nested = path.join(root, "nested");
  const result = run(bootstrap, {
    PATH: `${ambient}:/usr/bin:/bin`,
    FAKE_DEVSHELL_PATH: "/devshell/bin:/usr/bin:/bin",
    NIX_INVOCATION_PATH: invocation,
    NESTED_PATH: nested,
  });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(await readFile(invocation, "utf8"), `${ambient}:/usr/bin:/bin`);
  assert.equal(await readFile(nested, "utf8"), "/devshell/bin:/usr/bin:/bin");
});

test("bootstrap discovers the executable daemon nix and carries it into nested commands", async (t) => {
  const daemon = path.join(await mkdtemp(path.join(os.tmpdir(), "oxid-daemon-nix-")), "bin");
  t.after(() => rm(path.dirname(daemon), { recursive: true, force: true }));
  await writeFakeNix(daemon);
  const { root, bootstrap } = await fixture(t, { daemonProfile: daemon });
  const invocation = path.join(root, "invocation");
  const nested = path.join(root, "nested");
  const result = run(bootstrap, {
    PATH: "/usr/bin:/bin",
    FAKE_DEVSHELL_PATH: "/devshell/bin:/usr/bin:/bin",
    NIX_INVOCATION_PATH: invocation,
    NESTED_PATH: nested,
  });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(await readFile(invocation, "utf8"), `${daemon}:/usr/bin:/bin`);
  assert.equal(await readFile(nested, "utf8"), `${daemon}:/devshell/bin:/usr/bin:/bin`);
});

test("bootstrap reports a missing Nix only after ambient and daemon discovery fail", async (t) => {
  const { bootstrap } = await fixture(t, { daemonProfile: path.join(os.tmpdir(), "oxid-no-daemon-nix", "bin") });
  const result = spawnSync("/usr/bin/env", ["-i", "PATH=/usr/bin:/bin", "/bin/bash", bootstrap, "--help"], {
    encoding: "utf8",
  });
  assert.equal(result.status, 1);
  assert.match(result.stderr, /Nix is required/u);
});
