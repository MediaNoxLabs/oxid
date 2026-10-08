# Taskflow synthetic conformance matrix

Run the repository-pinned runtime without loading the Taskflow Pi extension:

```sh
node scripts/factory/taskflow-conformance.mjs --json
```

The probe creates only a temporary directory and Node child processes. It
never enables Taskflow mutations or touches Docker, Tailnet, devices, or factory
flows. Timings are configurable with `--step-ms`; the default stays below one second.
The report is deterministic JSON with the exact `pi-taskflow` and `taskflow-core`
closure versions and a capability status of `supported`, `unsupported`, or
`unverified` for each property.

## Current pinned-runtime result

`pi-taskflow@0.2.10` and `taskflow-core@0.2.10` demonstrate bounded progress
callbacks for short phases, immutable resume forks, changed-argument cache
invalidation, and process-group cleanup after a timed-out script. The cleanup probe
records the spawned group leader and one descendant, then verifies that both are gone
when the executor returns. Distinct slow-versus-stalled classification remains
**unverified** because script phases expose a wall timeout but no separate idle/stall
reason. This is not admission evidence for real long-running factory work.

The explicit long-process mode also separates two facts which must not be conflated:
the core executor can keep a directly constructed script phase alive beyond five
minutes, while the public saved-flow validator rejects that phase because script
timeouts are capped at 300,000 ms. Supervisor heartbeats show that this probe is still
alive; they are labelled as supervisor evidence and do not masquerade as Taskflow
phase progress. The report records the maximum gap between genuine Taskflow callbacks.

## Smallest upstream-ready delta

Taskflow still needs a public, deterministic lifecycle receipt for script phases. The
black-box probe proves the terminal effect, but a receipt should expose the spawned
process-group identity, TERM request timestamp, KILL escalation timestamp/result, and
terminal registry-removal result without exposing command secrets. A configurable
script timeout above 300,000 ms must be accepted only when a flow explicitly opts in,
while preserving the existing default cap. No Oxid scheduler is required.

The on-demand real-duration acceptance command, intentionally excluded from fast CI,
is:

```sh
node scripts/factory/taskflow-conformance.mjs --json --long-process --step-ms 360000
```

This command is an evidence probe, not an admission bypass. Until the
`saved-flow-long-script-admission` row is supported, Portal preparation must retain its
explicit foreground approval boundary. A successful `direct-executor-long-script` row
alone is insufficient to admit long mechanical phases.
