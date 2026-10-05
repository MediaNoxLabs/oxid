// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { access, chmod, copyFile, mkdir, mkdtemp, readFile, realpath, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

const root = new URL("../../", import.meta.url);
const text = (relative) => readFile(new URL(relative, root), "utf8");
const LEASE_PUBLICATION_TIMEOUT_MS = 30_000;
const CHILD_TERMINATION_GRACE_MS = 1_000;
const CHILD_DIAGNOSTIC_LIMIT_BYTES = 512;

function superviseExactChild(child) {
  let outcome;
  const completion = new Promise((resolve) => {
    const settle = (value) => {
      if (outcome === undefined) {
        outcome = value;
        resolve(value);
      }
    };
    child.once("error", (error) => settle({ kind: "spawn-error", code: error.code ?? "unknown" }));
    child.once("close", (code, signal) => settle({ kind: "close", code, signal }));
  });
  return { completion, outcome: () => outcome };
}

function boundedDiagnostics(stream) {
  let bytes = Buffer.alloc(0);
  stream?.on("data", (chunk) => {
    bytes = Buffer.concat([bytes, Buffer.from(chunk)]);
    if (bytes.length > CHILD_DIAGNOSTIC_LIMIT_BYTES) {
      bytes = bytes.subarray(bytes.length - CHILD_DIAGNOSTIC_LIMIT_BYTES);
    }
  });
  return () => bytes.toString("utf8");
}

async function terminateExactChild(child, supervision) {
  if (supervision.outcome() !== undefined) return supervision.completion;
  child.kill("SIGTERM");
  const closed = await Promise.race([
    supervision.completion.then(() => true),
    new Promise((resolve) => setTimeout(() => resolve(false), CHILD_TERMINATION_GRACE_MS)),
  ]);
  if (!closed && supervision.outcome() === undefined) child.kill("SIGKILL");
  return supervision.completion;
}

async function waitForLeaseOrExit(leasePath, supervision) {
  const startedAt = Date.now();
  while (Date.now() - startedAt < LEASE_PUBLICATION_TIMEOUT_MS) {
    try {
      await access(leasePath);
      return { state: "ready", elapsedMs: Date.now() - startedAt };
    } catch {
      const outcome = supervision.outcome();
      if (outcome !== undefined) return { state: "exited", elapsedMs: Date.now() - startedAt, outcome };
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
  }
  return { state: "timeout", elapsedMs: Date.now() - startedAt };
}

test("lease fixture supervision reaps bounded failure modes with bounded diagnostics", async (t) => {
  const cases = [
    { name: "early exit", command: [process.execPath, ["-e", "process.exit(17)"]] },
    { name: "signal exit", command: [process.execPath, ["-e", "process.kill(process.pid, 'SIGTERM')"]] },
    { name: "spawn error", command: ["/oxid/definitely-missing-command", []] },
    { name: "ignores SIGTERM", command: [process.execPath, ["-e", "process.on('SIGTERM',()=>{});setInterval(()=>{},1000)"]] },
  ];
  for (const fixture of cases) {
    await t.test(fixture.name, async () => {
      const child = spawn(fixture.command[0], fixture.command[1], { stdio: ["ignore", "ignore", "pipe"] });
      const diagnostics = boundedDiagnostics(child.stderr);
      const supervision = superviseExactChild(child);
      if (fixture.name === "ignores SIGTERM") await new Promise((resolve) => setTimeout(resolve, 100));
      const outcome = fixture.name === "ignores SIGTERM"
        ? await terminateExactChild(child, supervision)
        : await supervision.completion;
      assert.ok(outcome.kind === "spawn-error" || outcome.kind === "close");
      if (fixture.name === "ignores SIGTERM") assert.equal(outcome.signal, "SIGKILL");
      assert.ok(Buffer.byteLength(diagnostics()) <= CHILD_DIAGNOSTIC_LIMIT_BYTES);
    });
  }
});

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

test("standalone lifecycle uses durable Git-common state with an atomic verified lease", async () => {
  const [up, down, status, state] = await Promise.all([
    text("scripts/standalone-up.sh"),
    text("scripts/standalone-down.sh"),
    text("scripts/standalone-status.sh"),
    text("scripts/lib/standalone-state.sh"),
  ]);
  for (const source of [up, down]) {
    assert.match(source, /oxid_standalone_state_directory/);
    assert.doesNotMatch(source, /temporary_root=.*TMPDIR/);
  }
  assert.match(status, /oxid_standalone_state_directory/);
  assert.match(status, /durable receipt/);
  assert.match(state, /git -C "\$repository_root" rev-parse --show-toplevel/);
  assert.match(state, /git -C "\$git_top_level" rev-parse --git-common-dir/);
  assert.match(state, /\$\{git_common_directory%\/\}\/oxid\/standalone/);
  assert.match(state, /OXID_STANDALONE_STATE_DIR/);
  assert.match(state, /must not be a symlink/);
  assert.match(state, /pwd -P/);
  assert.match(up, /scripts\/standalone-stack\.yml/);
  assert.match(state, /oxid-standalone-lease-v3/);
  assert.match(state, /oxid-standalone-lease-v2/);
  assert.match(state, /ln -- "\$candidate" "\$lease_record"/);
  assert.match(state, /state:"contention"/);
  assert.match(state, /ownerPrefix/);
  assert.match(state, /proc:/);
  assert.match(state, /oxid_standalone_cleanup_lease_artifacts/);
  assert.match(up, /oxid_standalone_acquire_lease/);
  assert.match(down, /oxid_standalone_acquire_lease/);
  assert.match(up, /canonical-compose\.yml/);
  assert.match(up, /canonical-indexer\.env/);
  assert.match(up, /Reusing the healthy candidate standalone stack without Compose mutation/);
  assert.doesNotMatch(up, /rm -f --[^\n]*\$serve_marker/);
  assert.match(down, /owner-receipt\.json/);
  assert.match(down, /\.session == \$session/);
  assert.match(down, /ownership is not proven/);
  assert.match(status, /nodeHeight/);
  assert.match(status, /indexerHeight/);
  assert.match(status, /catchingUp/);
  assert.match(status, /containerIds == \$containers/);
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

test("two worktrees serialize startup, reuse without Compose mutation, and preserve exact cleanup ownership", async (t) => {
  const temporary = await mkdtemp(join(tmpdir(), "oxid-standalone-worktrees-"));
  const physicalTemporary = await realpath(temporary);
  const fakeBin = join(temporary, "bin");
  const sharedState = join(physicalTemporary, "shared-state");
  const dockerState = join(temporary, "docker-running");
  const dockerLedger = join(temporary, "docker-ledger");
  const worktreeA = join(temporary, "worktree-a");
  const worktreeB = join(temporary, "worktree-b");
  let first;
  let firstSupervision;
  const writeExecutable = async (path, source) => {
    await writeFile(path, source, "utf8");
    await chmod(path, 0o755);
  };

  try {
    await Promise.all([
      mkdir(fakeBin, { recursive: true }),
      mkdir(join(worktreeA, "scripts"), { recursive: true }),
      mkdir(join(worktreeB, "scripts"), { recursive: true }),
      mkdir(join(worktreeA, "scripts", "lib"), { recursive: true }),
      mkdir(join(worktreeB, "scripts", "lib"), { recursive: true }),
    ]);
    for (const worktree of [worktreeA, worktreeB]) {
      for (const file of ["standalone-up.sh", "standalone-down.sh", "standalone-stack.yml"]) {
        await copyFile(new URL(`../../scripts/${file}`, import.meta.url), join(worktree, "scripts", file));
      }
      await copyFile(
        new URL("../../scripts/lib/standalone-compose-ownership.sh", import.meta.url),
        join(worktree, "scripts", "lib", "standalone-compose-ownership.sh"),
      );
      await copyFile(
        new URL("../../scripts/lib/standalone-state.sh", import.meta.url),
        join(worktree, "scripts", "lib", "standalone-state.sh"),
      );
      await chmod(join(worktree, "scripts", "standalone-up.sh"), 0o755);
      await chmod(join(worktree, "scripts", "standalone-down.sh"), 0o755);
    }
    await writeExecutable(join(fakeBin, "openssl"), `#!/usr/bin/env bash
printf '%064d\\n' "$$"
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
  inspect)
    case "\${4:-}" in
      indexer-id) service=indexer ;;
      node-id) service=node ;;
      prover-id) service=proof-server ;;
      *) exit 1 ;;
    esac
    case "\${3:-}" in
      *compose.service*) printf '%s\n' "$service" ;;
      *project.config_files*) printf '%s/canonical-compose.yml\n' "$OXID_STANDALONE_STATE_DIR" ;;
      *project.working_dir*) printf '%s\n' "$OXID_STANDALONE_STATE_DIR" ;;
      *compose.project*) printf 'oxid-standalone\n' ;;
      *) exit 1 ;;
    esac
    ;;
  ps)
    if [ -f "$FAKE_DOCKER_STATE" ]; then printf 'node-id\\nindexer-id\\nprover-id\\n'; fi
    ;;
  compose)
    case " $* " in
      *' up -d --wait '*)
        sleep "\${FAKE_COMPOSE_SLEEP:-0}"
        : >"$FAKE_DOCKER_STATE"
        ;;
      *' down --remove-orphans '*)
        sleep "\${FAKE_COMPOSE_DOWN_SLEEP:-0}"
        rm -f "$FAKE_DOCKER_STATE"
        ;;
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
    first = spawn("bash", [join(worktreeA, "scripts", "standalone-up.sh"), "local"], {
      env: { ...environment, FAKE_COMPOSE_SLEEP: "3" },
      stdio: ["ignore", "pipe", "pipe"],
    });
    const firstDiagnostics = boundedDiagnostics(first.stderr);
    first.stdout.resume();
    firstSupervision = superviseExactChild(first);
    // A copied shell entrypoint can take several seconds to start through the
    // macOS/Nix toolchain on a cold host. Distinguish a live scheduled child
    // from an early failure while retaining a finite reviewed bound.
    const lease = await waitForLeaseOrExit(
      join(sharedState, "startup-lease", "owner.json"),
      firstSupervision,
    );
    if (lease.state === "exited") {
      assert.fail(`the first worktree exited before publishing its lease (${JSON.stringify(lease.outcome)}): ${firstDiagnostics()}`);
    }
    if (lease.state === "timeout") {
      await terminateExactChild(first, firstSupervision);
      assert.fail(`the first worktree remained live without publishing its lease after ${lease.elapsedMs}ms`);
    }
    t.diagnostic(`factory-metrics phase=standalone-lease result=ready duration_ms=${lease.elapsedMs}`);
    const loser = spawnSync("bash", [join(worktreeB, "scripts", "standalone-up.sh"), "local"], {
      env: environment,
      encoding: "utf8",
    });
    assert.equal(loser.status, 2);
    assert.match(loser.stderr, /"state":"contention"/);
    const firstOutcome = await firstSupervision.completion;
    assert.deepEqual(firstOutcome, { kind: "close", code: 0, signal: null }, firstDiagnostics());

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

    const ownerDown = spawn("bash", [join(worktreeA, "scripts", "standalone-down.sh")], {
      env: { ...environment, FAKE_COMPOSE_DOWN_SLEEP: "3" },
      stdio: ["ignore", "pipe", "pipe"],
    });
    let ownerDownStderr = "";
    ownerDown.stdout.resume();
    ownerDown.stderr.on("data", (chunk) => { ownerDownStderr += chunk; });
    const ownerDownStatus = new Promise((resolve, reject) => {
      ownerDown.once("error", reject);
      ownerDown.once("close", resolve);
    });
    let downLeaseObserved = false;
    for (let attempt = 0; attempt < 500; attempt += 1) {
      try {
        await access(join(sharedState, "startup-lease", "owner.json"));
        downLeaseObserved = true;
        break;
      } catch {
        await new Promise((resolve) => setTimeout(resolve, 20));
      }
    }
    assert.equal(downLeaseObserved, true, "teardown must publish the shared lease");
    const startDuringDown = spawnSync("bash", [join(worktreeB, "scripts", "standalone-up.sh"), "local"], {
      env: environment,
      encoding: "utf8",
    });
    assert.equal(startDuringDown.status, 2, startDuringDown.stderr);
    assert.match(startDuringDown.stderr, /"state":"contention"/);
    assert.equal(await ownerDownStatus, 0, ownerDownStderr);
    ledger = await readFile(dockerLedger, "utf8");
    assert.equal(ledger.split("\\n").filter((line) => /compose .* down --remove-orphans/.test(line)).length, 1);

    const staleLease = join(sharedState, "startup-lease", "owner.json");
    await writeFile(staleLease, JSON.stringify({
      schema: "oxid-standalone-lease-v2",
      session: "stale-session",
      lease: "stale-lease",
      pid: 2_147_483_647,
      processStart: "stale-process",
    }), { mode: 0o600 });
    const recovered = spawnSync("bash", [join(worktreeA, "scripts", "standalone-up.sh"), "local"], {
      env: environment,
      encoding: "utf8",
    });
    assert.equal(recovered.status, 0, recovered.stderr);
    const recoveredDown = spawnSync("bash", [join(worktreeA, "scripts", "standalone-down.sh")], {
      env: environment,
      encoding: "utf8",
    });
    assert.equal(recoveredDown.status, 0, recoveredDown.stderr);

    const unsafeState = join(physicalTemporary, "unsafe-state");
    await mkdir(unsafeState, { mode: 0o755 });
    await chmod(unsafeState, 0o755);
    const unsafeOverride = spawnSync("bash", [join(worktreeA, "scripts", "standalone-up.sh"), "local"], {
      env: { ...environment, OXID_STANDALONE_STATE_DIR: unsafeState },
      encoding: "utf8",
    });
    assert.equal(unsafeOverride.status, 1);
    assert.match(unsafeOverride.stderr, /mode 700; refusing permission mutation/);
  } finally {
    if (first && firstSupervision && firstSupervision.outcome() === undefined) {
      await terminateExactChild(first, firstSupervision);
    }
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
