# Demo inventory

[`demo-inventory.json`](demo-inventory.json) is the single machine-readable source
for product demonstration truth. It links atomic **use cases** to ordered
**scenarios**, then scenarios to product/audience **demos**. IDs, rather than
copied runbook prose, carry every relationship. Its closed structure is
documented by [`demo-inventory.schema.json`](demo-inventory.schema.json), while
the repository validator enforces cross-reference and safe-command semantics.

Use the read-only renderer; it validates before rendering and never evaluates
inventory command text:

```bash
node scripts/demo-inventory.mjs check
node scripts/demo-inventory.mjs list
node scripts/demo-inventory.mjs show wallet-root-recovery-native-presence
node scripts/demo-inventory.mjs prepare wallet-root-recovery-native-presence android-physical
node scripts/demo-inventory.mjs use-case list
```

In Pi, `/scenario list`, `/scenario show <id>`, and
`/scenario prepare <id> [target-id]` use the same validator; `/use-case list`
and `/use-case show <id>` expose the atomic aliases. When no target is supplied,
`prepare` uses the scenario's reviewed default target.

The repository CLI only renders a brief and never evaluates inventory command
text. Invoking Pi's `/scenario prepare` is the user's request for the active
agent to perform bounded preparation: load resource-hygiene, select one
supported target, check prerequisites, start only authorized dependencies,
build/deploy or use the one-command run path, prove readiness, and return
non-sensitive URLs plus manual acceptance and cleanup steps. It does not
authorize entering credentials, completing consent, deleting pre-existing
state, or exceeding `AGENT.md`. Commands marked `unsupported`, `manual`, or
`delegated` describe an honest boundary instead of inventing automation.

Each target plan exposes separate `build`, `deploy`, and `run` operations. The
agent chooses the all-in-one `run` path or the reusable build/deploy path; it
does not execute both redundantly.

## Maintenance contract

The Product Manager assesses every shipped capability for a use case. If it has
a demonstrable journey, add it to an ordered scenario and a product demo; if it
does not, record the justified no-demo impact in feature closeout. Keep target
support, dependency ownership, health checks, cleanup, evidence class, cadence,
test mapping, manual acceptance, and expected outcomes explicit. Reuse the
linked scripts/runbooks rather than copying their operational detail.

Validation fails closed for duplicate or unknown IDs, missing source references,
unsafe or absolute commands, mutable dependencies without cleanup, unsupported
cadence/evidence values, and scenarios without test mappings. Live, expensive
or physical evidence remains on its declared cadence; it is not a universal PR
gate.

## Initial slice

`wallet-root-recovery-native-presence` records the merged native-presence recovery
behavior. Desktop is only a fast development preflight; simulator diagnostics
must not be promoted to acceptance. Physical Android is authoritative manual
acceptance and uses the existing native-custody build/deploy/run launcher.
Physical iOS deployment remains explicitly unsupported. The scenario covers
denial, one-shot root installation, duplicate rejection, and lifecycle clearing;
automated, privacy-preserving physical-device evidence remains a planned gap.
