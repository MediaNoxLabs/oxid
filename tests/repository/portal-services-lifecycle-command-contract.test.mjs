// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import { chmod, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

const root = new URL("../../", import.meta.url).pathname;
const manualRoot = join(root, "target/portal-tailnet-manual");
const state = join(manualRoot, "runtime/portal-consumer");
const source = join(manualRoot, "prepared/portal-source");

async function executable(path, contents) {
  await writeFile(path, contents);
  await chmod(path, 0o755);
}

test("public Portal service commands bind manual state and wait for compose readiness", async () => {
  const bin = await mkdtemp(join(tmpdir(), "oxid-portal-services-bin-"));
  const dockerState = join(bin, "docker-state");
  const dockerLog = join(bin, "docker.log");
  const env = { ...process.env, PATH: `${bin}:${process.env.PATH}`, DOCKER_STATE: dockerState, DOCKER_LOG: dockerLog };
  assert.equal(spawnSync("test", ["!", "-e", manualRoot], { cwd: root }).status, 0, "test requires isolated manual state");
  try {
    await mkdir(source, { recursive: true, mode: 0o700 });
    await mkdir(state, { recursive: true, mode: 0o700 });
    await writeFile(join(state, "owner-receipt.json"), "{}", { mode: 0o600 });
    await executable(join(bin, "nix"), "#!/usr/bin/env bash\nexec bash -c \"$5\" \"$6\" \"${@:7}\"\n");
    await executable(join(bin, "just"), "#!/usr/bin/env bash\nexec ./scripts/e2e/portal-services-lifecycle.sh \"${1#portal-tailnet-services-}\"\n");
    await executable(join(bin, "git"), "#!/usr/bin/env bash\ncase \"$*\" in *'remote get-url origin'*) echo https://github.com/input-output-hk/lace-id-portal.git;; *'HEAD^{tree}'*) echo 2d845d2293603dfd8adce5362c8a9941e6ba78a9;; *'rev-parse HEAD'*) echo 25499870f84d77173c46e4af3021311decfb840b;; esac\n");
    await executable(join(bin, "jq"), "#!/usr/bin/env bash\ncase \" $* \" in *' -e '*) exit 0;; *' --arg state running'*) echo '{\"state\":\"running\"}';; *) echo '{\"state\":\"stopped\"}';; esac\n");
    await executable(join(bin, "docker"), "#!/usr/bin/env bash\nif [ \"$1\" = compose ]; then printf '%s\\n' \"$*\" >>\"$DOCKER_LOG\"; case \" $* \" in *' start '*) echo running >\"$DOCKER_STATE\";; esac; exit 0; fi\ncase \" $* \" in *' ps -a '*) printf 'one\\ntwo\\nthree\\nfour\\nfive\\n';; *' ps '*) [ \"$(cat \"$DOCKER_STATE\" 2>/dev/null)\" = running ] && printf 'one\\ntwo\\nthree\\nfour\\n' || true;; esac\n");
    for (const command of ["curl", "openssl", "shasum"]) await executable(join(bin, command), "#!/usr/bin/env bash\nexit 0\n");

    const stopped = spawnSync("./scripts/e2e/portal-services-lifecycle.sh", ["services-status"], { cwd: root, env, encoding: "utf8" });
    assert.equal(stopped.status, 0, `${stopped.stderr}\n${stopped.stdout}`);
    assert.match(stopped.stdout, /"state":"stopped"/u);
    const started = spawnSync("./scripts/e2e/portal-services-lifecycle.sh", ["services-up"], { cwd: root, env, encoding: "utf8" });
    assert.equal(started.status, 0, `${started.stderr}\n${started.stdout}`);
    assert.match(started.stdout, /"state":"running"/u);
    assert.match(await readFile(dockerLog, "utf8"), /start smocker did-resolver did-manager issuer/u);
  } finally {
    await rm(manualRoot, { recursive: true, force: true });
    await rm(bin, { recursive: true, force: true });
  }
});

test("manual supervisor treats the receipt-owned stopped service state as non-fatal", async () => {
  const script = await readFile(new URL("../../scripts/test-android-portal-tailnet-physical.sh", import.meta.url), "utf8");
  assert.match(script, /manual_consumer_stopped\(\)[\s\S]*services-status/u);
  assert.match(script, /if ! manual_consumer_running && ! manual_consumer_stopped; then[\s\S]*manual_cleanup/u);
});
