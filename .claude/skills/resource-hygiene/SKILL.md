---
name: resource-hygiene
description: >-
  Load before and during any resource-heavy or background work on this Mac,
  including Android emulators, Apple simulators, Docker, Gradle/JVM,
  Rust/Nix/Dioxus builds, dev servers, watchers, and disposable worktrees; load
  again before completion to prove receipt-scoped cleanup.
---

# Resource hygiene

This 96 GiB Mac has repeatedly frozen from aggregate overcommit: a
crash-looping Android QEMU process drove swap upward, Docker previously grew to
about 35 GiB, and abandoned worktrees/build outputs filled the disk. Starting a
resource creates an ownership obligation. Completion requires proving that the
same owned resource stopped or was deliberately handed off.

The host monitor writes `~/.claude-resmon/resources.log` and
`~/.claude-resmon/alerts.log`. Treat its alerts as a stop signal, not a prompt
for another retry.

## Before heavy work

1. Inspect swap, memory pressure, free disk, and existing heavy resources.
2. Identify the exact process group, ADB serial/AVD, simulator UDID, Compose
   project, build directory, and worktree the task will own. Write a private
   receipt before mutation.
3. Preserve unknown or pre-existing resources. Reuse one only when the active
   harness explicitly supports reuse and its ownership/identity is proven;
   otherwise fail closed.
4. Run one heavy virtual device at a time: Android **or** an Apple simulator,
   never both. Heavy child agents and platform builds run sequentially.

Useful read-only checks:

```bash
sysctl vm.swapusage
memory_pressure -Q
df -h /
pgrep -afil 'qemu-system|Simulator|com\.docker\.krun|GradleDaemon|KotlinCompileDaemon|dx build'
adb devices -l
xcrun simctl list devices
```

Do not launch heavy work when swap used exceeds 20 GiB, telemetry is
unavailable, or the resource monitor has a recent unresolved alert. First
clean only resources whose ownership is proven; otherwise stop and report.

## Supervision and pressure stops

- Launch long work under one recorded process group with signal/EXIT cleanup.
  Validate the live command identity before signalling it; a bare PID is not an
  ownership receipt.
- One QEMU exit is terminal. Never respawn an emulator inside an automatic
  retry. Multiple Crashpad processes are diagnostic evidence, not cleanup
  authority.
- While heavy work runs, sample pressure at bounded intervals. Stop the exact
  owned process group if swap grows by at least 1 GiB in 120 seconds, free
  memory falls to 2 GiB or less, telemetry fails, or the owned runtime crashes.
- A cancelled agent/session must also stop its owned process group. A detached
  PID adopted by PID 1 is still owned and must be reconciled before any new run.
- Return after each platform attempt. Do not hide multiple full aggregate
  retries inside one long-running agent call.

## Phase work to reduce peak memory

Build before booting a virtual device whenever the target architecture is known:

1. prepare an exact HEAD/tree/profile/manifest build;
2. finish Rust/Nix/Gradle work and release owned build processes;
3. recheck memory pressure;
4. boot one virtual device;
5. install the exact digest-bound artifact and run one attempt;
6. clean and report before deciding on another attempt.

Fresh protocol offers and app/runtime state do not require recompiling an
unchanged exact-head artifact. Reuse build output only through a reviewed cache
key that includes HEAD/tree, features/profile, manifest digest, target,
toolchains, and architecture.

## Tool-specific ownership

### Android

- Require the harness's exact ADB inventory and explicit AVD selector. Never
  substitute or reuse an unknown/physical device.
- Bind cleanup to the verified QEMU PID/process group, serial, AVD, port, and
  launch arguments. Use `adb -s "$serial" emu kill` only after that identity is
  proven. Never use blanket `pkill` cleanup.
- Prefer a reviewed low-memory test AVD. Do not start QEMU while a heavy build
  is still active.

### Apple simulators

- Store the exact UDID returned when the task creates a simulator.
- Shut down/delete only that receipt-owned UDID. Never use `simctl shutdown all`
  or terminate shared CoreSimulator services.

### Docker

- Preserve healthy pre-existing stacks unless the owner explicitly authorizes
  recreation. Use an exact Compose project/label/container receipt.
- Add reviewed per-service memory limits where compatible; do not apply an
  arbitrary cap that breaks the topology.
- Stop/remove only the receipt-owned project. Never use blanket prune or kill
  commands. Restart Docker Desktop rather than looping destructive operations
  against an unresponsive daemon.

### Gradle, JVM, Rust, Nix, and Dioxus

- Prefer Gradle `--no-daemon`, bounded workers, and a bounded heap. If a daemon
  or compiler process was spawned, stop only its proven process group; do not
  kill unrelated Java/Rust work.
- Bound Cargo/build jobs and avoid concurrent native builds. Keep compile gates
  separate from virtual-device runtime attempts.

### Dev servers and watchers

Record the exact PID/process group and command identity, install EXIT/signal
cleanup, and verify descendants/listeners are gone. `kill`, `pkill`, or port
cleanup without an ownership proof is prohibited.

### Worktrees and generated output

Use `node scripts/worktree-lifecycle.mjs audit` and the repository's exact-path
lifecycle command. Do not remove a durable dev-loop worktree merely because the
current task ended. Remove only task-created disposable worktrees after proving
clean status, remote commit reachability, and no unique artifacts. Delete build
outputs only when their exact path and ownership are known.

## Before reporting completion or a blocker

Prove and report:

- owned process groups and descendants are absent;
- owned emulator/simulator inventory is absent;
- ADB inventory and Tailscale Serve state match their recorded baselines;
- owned containers/listeners/build daemons are absent or intentionally handed
  off;
- tracked Git state is understood and private diagnostics remain mode `0600`;
- current swap/memory trend is stable.

If cleanup cannot be proven, preserve the receipt and ambiguous state for
owner review. Never turn uncertainty into broader deletion.
