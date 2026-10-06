// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import { chmod, copyFile, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
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
  const env = {
    ...process.env,
    PATH: `${bin}:${process.env.PATH}`,
    DOCKER_STATE: dockerState,
    DOCKER_LOG: dockerLog,
    OXID_PORTAL_CONSUMER_LEASE_DIR: join(bin, "lease"),
  };
  assert.equal(spawnSync("test", ["!", "-e", manualRoot], { cwd: root }).status, 0, "test requires isolated manual state");
  try {
    await mkdir(source, { recursive: true, mode: 0o700 });
    await mkdir(join(source, "sidecar/did-manager-bridge"), { recursive: true, mode: 0o700 });
    await copyFile(
      join(root, "tests/repository/fixtures/portal-did-manager-package-0.4.0.json"),
      join(source, "sidecar/did-manager-bridge/package.json"),
    );
    await copyFile(
      join(root, "tests/repository/fixtures/portal-flake-lock-midnight-did-0.4.0.json"),
      join(source, "flake.lock"),
    );
    await mkdir(state, { recursive: true, mode: 0o700 });
    await executable(join(bin, "nix"), "#!/usr/bin/env bash\nexec bash -c \"$5\" \"$6\" \"${@:7}\"\n");
    await executable(join(bin, "just"), "#!/usr/bin/env bash\nexec ./scripts/e2e/portal-services-lifecycle.sh \"${1#portal-tailnet-services-}\"\n");
    await executable(join(bin, "git"), "#!/usr/bin/env bash\ncase \"$*\" in *'show HEAD:sidecar/did-manager-bridge/package.json'*) cat \"$2/sidecar/did-manager-bridge/package.json\";; *'show HEAD:flake.lock'*) cat \"$2/flake.lock\";; *'remote get-url origin'*) echo https://github.com/input-output-hk/lace-id-portal.git;; *'HEAD^{tree}'*) echo 2d845d2293603dfd8adce5362c8a9941e6ba78a9;; *'rev-parse HEAD'*) echo 25499870f84d77173c46e4af3021311decfb840b;; esac\n");
    await executable(join(bin, "docker"), "#!/usr/bin/env bash\nif [ \"$1\" = compose ]; then printf '%s\\n' \"$*\" >>\"$DOCKER_LOG\"; case \" $* \" in *' up '*) echo running >\"$DOCKER_STATE\";; esac; exit 0; fi\ncase \" $* \" in *' ps -a '*) printf 'one\\ntwo\\nthree\\nfour\\nfive\\n';; *' ps '*) [ \"$(cat \"$DOCKER_STATE\" 2>/dev/null)\" = running ] && printf 'one\\ntwo\\nthree\\nfour\\n' || true;; esac\n");
    for (const command of ["curl", "openssl"]) await executable(join(bin, command), "#!/usr/bin/env bash\nexit 0\n");
    await executable(join(bin, "shasum"), "#!/usr/bin/env bash\nprintf '%064d\\n' 0\n");
    await executable(join(bin, "timeout"), "#!/usr/bin/env bash\nif [ \"${1:-}\" = -k ]; then shift 3; else shift; fi\nexec \"$@\"\n");

    const compatibility = spawnSync(
      process.execPath,
      ["./scripts/check-midnight-integration-compatibility.mjs", "--portal-source", source],
      { cwd: root, env, encoding: "utf8" },
    );
    assert.equal(compatibility.status, 0, `${compatibility.stderr}\n${compatibility.stdout}`);
    env.COMPATIBILITY_MANIFEST_SHA256 = JSON.parse(compatibility.stdout).manifestSha256;
    await writeFile(join(state, "owner-receipt.json"), JSON.stringify({
      schema: "oxid-portal-consumer-owner-v1",
      source: {
        commit: "25499870f84d77173c46e4af3021311decfb840b",
        tree: "2d845d2293603dfd8adce5362c8a9941e6ba78a9",
      },
      compatibilityManifestSha256: env.COMPATIBILITY_MANIFEST_SHA256,
      composeSha256: "0".repeat(64),
      project: "oxid-portal-consumer",
      containerIds: ["five", "four", "one", "three", "two"],
      images: { resolver: "resolver", didManager: "manager", issuer: "issuer" },
    }), { mode: 0o600 });

    const stopped = spawnSync("./scripts/e2e/portal-services-lifecycle.sh", ["services-status"], { cwd: root, env, encoding: "utf8" });
    assert.equal(stopped.status, 0, `${stopped.stderr}\n${stopped.stdout}`);
    assert.match(stopped.stdout, /"state":"stopped"/u);
    const started = spawnSync("./scripts/e2e/portal-services-lifecycle.sh", ["services-up"], { cwd: root, env, encoding: "utf8" });
    assert.equal(started.status, 0, `${started.stderr}\n${started.stdout}`);
    assert.match(started.stdout, /"state":"running"/u);
    assert.match(await readFile(dockerLog, "utf8"), /up -d --wait --wait-timeout 600 smocker did-resolver did-manager issuer/u);
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
