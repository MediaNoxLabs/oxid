# Taskflow synthetic conformance matrix

Run the repository-pinned runtime without loading the Taskflow Pi extension:

```sh
node scripts/factory/taskflow-conformance.mjs --json
```

The probe creates only a temporary directory and short-lived Node child processes. It
never enables Taskflow mutations or touches Docker, Tailnet, devices, or factory
flows. Timings are configurable with `--step-ms`; the default stays below one second.
The report is deterministic JSON with the exact `pi-taskflow` and `taskflow-core`
closure versions and a capability status of `supported`, `unsupported`, or
`unverified` for each property.

## Current pinned-runtime result

`pi-taskflow@0.2.10` and `taskflow-core@0.2.10` demonstrate bounded progress
callbacks, immutable resume forks, and changed-argument cache invalidation. Distinct
slow-versus-stalled classification, process-tree termination escalation, and terminal
registry cleanup remain **unverified** because their public black-box result has no
idle/stall reason, child-tree receipt, or registry observation. This is not admission
evidence for real long-running factory work.

## Smallest upstream-ready delta

Taskflow needs a public, deterministic lifecycle receipt for script phases. The receipt
should expose the spawned process-group identity, TERM request timestamp, KILL escalation
timestamp/result, and terminal reap/registry-removal result without exposing command
secrets. A configurable script timeout above 300,000 ms must be accepted only when a
flow explicitly opts in, while preserving the existing default cap. With that public
receipt, this probe can classify cancellation escalation and terminal cleanup as
supported or unsupported rather than unverified; no Oxid scheduler is required.

The on-demand real-duration acceptance command, intentionally excluded from fast CI,
is:

```sh
node scripts/factory/taskflow-conformance.mjs --json --step-ms 360000
```

A future upstream version must add an explicit `--long-process` mode before this command
is treated as a five-minute-plus acceptance probe; the current bounded synthetic command
is only a timing-scale smoke and must not be used to claim long-process admission.
