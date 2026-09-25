// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { access, chmod, copyFile, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

const root = new URL("../../", import.meta.url);
const text = (relative) => readFile(new URL(relative, root), "utf8");

test("root demo entrypoints are strict thin wrappers over one canonical lifecycle", async () => {
  for (const operation of ["start", "status", "stop"]) {
    const wrapper = await text(`demo/${operation}.sh`);
    assert.match(wrapper, /^#!\/usr\/bin\/env bash/m);
    assert.match(wrapper, /set -euo pipefail/);
    assert.match(wrapper, new RegExp(`scripts/demo-stack\\.sh" ${operation}$`, "m"));
    assert.doesNotMatch(wrapper, /docker|tailscale|adb|curl/);
  }
});

test("demo lifecycle records exact-head ownership and delegates existing boundaries", async () => {
  const lifecycle = await text("scripts/demo-stack.sh");
  const query = lifecycle.indexOf("existing=\"$(query_standalone_containers)\"");
  const start = lifecycle.indexOf("standalone-phone-up");
  assert.ok(query >= 0 && query < start, "ownership query must precede standalone startup");
  assert.match(lifecycle, /oxid-tailnet-identity-demo-v1/);
  assert.match(lifecycle, /git -C "\$repository_root" rev-parse HEAD/);
  assert.match(lifecycle, /git -C "\$repository_root" rev-parse 'HEAD\^\{tree\}'/);
  assert.match(lifecycle, /chmod 600 "\$candidate"/);
  assert.match(lifecycle, /standaloneOwned/);
  assert.match(lifecycle, /git -C "\$repository_root" status --porcelain/);
  assert.match(lifecycle, /standalone-phone-up/);
  assert.match(lifecycle, /standalone-status\.sh" phone/);
  assert.match(lifecycle, /android-phone/);
  assert.match(lifecycle, /if \[ "\$standalone_owned" = true \]; then\s+just -f "\$repository_root\/Justfile" standalone-down/);
  assert.doesNotMatch(lifecycle, /tailscale serve|adb |docker compose|https:\/\//);
});

test("standalone status is read-only and checks local plus Tailnet readiness", async () => {
  const status = await text("scripts/standalone-status.sh");
  assert.match(status, /com\.docker\.compose\.project=oxid-standalone/);
  assert.match(status, /chain_getHeader/);
  assert.match(status, /StandaloneReadiness/);
  assert.match(status, /\.TCP\["443"\]\.HTTPS == true/);
  assert.match(status, /\.TCP\["8443"\]\.HTTPS == true/);
  assert.match(status, /\.TCP\["10000"\]\.HTTPS == true/);
  assert.match(status, /oxid standalone \(\$mode\): READY/);
  assert.doesNotMatch(status, /\b(up|down|start|stop|reset|rm)\b/);
});

test("standalone lifecycle uses checkout-independent canonical state with an atomic verified lease", async () => {
  const [up, down, status] = await Promise.all([
    text("scripts/standalone-up.sh"),
    text("scripts/standalone-down.sh"),
    text("scripts/standalone-status.sh"),
  ]);
  for (const source of [up, down, status]) {
    assert.match(source, /OXID_STANDALONE_STATE_DIR:-\$\{TMPDIR:-\/tmp\}\/oxid-standalone/);
  }
  assert.match(up, /scripts\/standalone-stack\.yml/);
  assert.match(up, /mkdir "\$lease_directory"/);
  assert.match(up, /oxid-standalone-lease-v1/);
  assert.match(up, /state:"contention"/);
  assert.match(up, /ownerPrefix/);
  assert.match(up, /canonical-compose\.yml/);
  assert.match(up, /canonical-indexer\.env/);
  assert.match(up, /Reusing the healthy candidate standalone stack without Compose mutation/);
  assert.match(down, /owner-receipt\.json/);
  assert.match(down, /\.session == \$session/);
  assert.match(down, /ownership is not proven/);
  assert.match(status, /nodeHeight/);
  assert.match(status, /indexerHeight/);
  assert.match(status, /catchingUp/);
});

test("standalone shutdown is receipt-scoped", async () => {
  const shutdown = await text("scripts/standalone-down.sh");
  assert.match(shutdown, /owner-receipt\.json/);
  assert.match(shutdown, /containerIds == \$containers/);
  assert.match(shutdown, /Standalone ownership is not proven; preserving resources\./);
  assert.match(
    shutdown,
    /docker compose -p oxid-standalone -f "\$compose_file" down --remove-orphans/,
  );
  assert.ok(
    shutdown.indexOf(".session == $session") < shutdown.indexOf("tailscale serve reset"),
    "caller ownership must be proven before route or Compose cleanup",
  );
});

test("two worktrees serialize startup, reuse without Compose mutation, and preserve exact cleanup ownership", async () => {
  const temporary = await mkdtemp(join(tmpdir(), "oxid-standalone-worktrees-"));
  const fakeBin = join(temporary, "bin");
  const sharedState = join(temporary, "shared-state");
  const dockerState = join(temporary, "docker-running");
  const dockerLedger = join(temporary, "docker-ledger");
  const worktreeA = join(temporary, "worktree-a");
  const worktreeB = join(temporary, "worktree-b");
  const writeExecutable = async (path, source) => {
    await writeFile(path, source, "utf8");
    await chmod(path, 0o755);
  };

  try {
    await Promise.all([
      mkdir(fakeBin, { recursive: true }),
      mkdir(join(worktreeA, "scripts"), { recursive: true }),
      mkdir(join(worktreeB, "scripts"), { recursive: true }),
    ]);
    for (const worktree of [worktreeA, worktreeB]) {
      for (const file of ["standalone-up.sh", "standalone-down.sh", "standalone-stack.yml"]) {
        await copyFile(new URL(`../../scripts/${file}`, import.meta.url), join(worktree, "scripts", file));
      }
      await chmod(join(worktree, "scripts", "standalone-up.sh"), 0o755);
      await chmod(join(worktree, "scripts", "standalone-down.sh"), 0o755);
    }
    await writeExecutable(join(fakeBin, "openssl"), `#!/usr/bin/env bash
printf '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\\n'
`);
    await writeExecutable(join(fakeBin, "curl"), `#!/usr/bin/env bash
case "$*" in
  *127.0.0.1:9944*) printf '{"result":{"number":"0x64"}}\\n' ;;
  *127.0.0.1:8088*) printf '{"data":{"block":{"height":100}}}\\n' ;;
esac
`);
    await writeExecutable(join(fakeBin, "docker"), `#!/usr/bin/env bash
printf '%s\\n' "$*" >>"$FAKE_DOCKER_LEDGER"
case "\${1:-}" in
  info) exit 0 ;;
  ps)
    if [ -f "$FAKE_DOCKER_STATE" ]; then printf 'node-id\\nindexer-id\\nprover-id\\n'; fi
    ;;
  compose)
    case " $* " in
      *' up -d --wait '*)
        sleep "\${FAKE_COMPOSE_SLEEP:-0}"
        : >"$FAKE_DOCKER_STATE"
        ;;
      *' down --remove-orphans '*) rm -f "$FAKE_DOCKER_STATE" ;;
    esac
    ;;
esac
`);

    const environment = {
      ...process.env,
      PATH: `${fakeBin}:${process.env.PATH}`,
      OXID_STANDALONE_STATE_DIR: sharedState,
      FAKE_DOCKER_STATE: dockerState,
      FAKE_DOCKER_LEDGER: dockerLedger,
    };
    const first = spawn("bash", [join(worktreeA, "scripts", "standalone-up.sh"), "local"], {
      env: { ...environment, FAKE_COMPOSE_SLEEP: "3" },
      stdio: ["ignore", "pipe", "pipe"],
    });
    let leaseObserved = false;
    for (let attempt = 0; attempt < 100; attempt += 1) {
      try {
        await access(join(sharedState, "startup-lease", "owner.json"));
        leaseObserved = true;
        break;
      } catch {
        await new Promise((resolve) => setTimeout(resolve, 20));
      }
    }
    assert.equal(leaseObserved, true, "the first worktree must publish its lease before contention is tested");
    const loser = spawnSync("bash", [join(worktreeB, "scripts", "standalone-up.sh"), "local"], {
      env: environment,
      encoding: "utf8",
    });
    assert.equal(loser.status, 2);
    assert.match(loser.stderr, /"state":"contention"/);
    const firstStatus = await new Promise((resolve, reject) => {
      first.once("error", reject);
      first.once("close", resolve);
    });
    assert.equal(firstStatus, 0);

    const reuse = spawnSync("bash", [join(worktreeB, "scripts", "standalone-up.sh"), "local"], {
      env: environment,
      encoding: "utf8",
    });
    assert.equal(reuse.status, 0, reuse.stderr);
    assert.match(reuse.stdout, /without Compose mutation/);
    let ledger = await readFile(dockerLedger, "utf8");
    assert.equal(ledger.split("\\n").filter((line) => /compose .* up -d --wait/.test(line)).length, 1);

    const wrongOwner = spawnSync("bash", [join(worktreeB, "scripts", "standalone-down.sh")], {
      env: environment,
      encoding: "utf8",
    });
    assert.equal(wrongOwner.status, 1);
    assert.match(wrongOwner.stderr, /ownership is not proven/);
    assert.doesNotMatch(await readFile(dockerLedger, "utf8"), /compose .* down --remove-orphans/);

    const ownerDown = spawnSync("bash", [join(worktreeA, "scripts", "standalone-down.sh")], {
      env: environment,
      encoding: "utf8",
    });
    assert.equal(ownerDown.status, 0, ownerDown.stderr);
    ledger = await readFile(dockerLedger, "utf8");
    assert.equal(ledger.split("\\n").filter((line) => /compose .* down --remove-orphans/.test(line)).length, 1);
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
});

test("operator runbook separates abstract OpenID roles from Midnight transport", async () => {
  const [runbook, runner, mainReadme, factoryIndex] = await Promise.all([
    text("demo/README.md"),
    text("run.sh"),
    text("README.md"),
    text("docs/factory/README.md"),
  ]);
  for (const phrase of [
    "demo/start.sh",
    "demo/status.sh",
    "demo/stop.sh",
    "OpenID4VCI 1.0 Final",
    "OpenID4VP 1.0 Final",
    "SIOPv2 draft 13",
    "in-process issuer",
    "proof_unavailable",
    "physical Android only",
  ]) assert.match(runbook, new RegExp(phrase, "i"));
  assert.match(mainReadme, /demo\/README\.md/);
  assert.match(factoryIndex, /demo\/README\.md/);
  const registration = "node --test tests/repository/tailnet-identity-demo-kit-contract.test.mjs";
  assert.equal(runner.split(registration).length - 1, 1);
});
