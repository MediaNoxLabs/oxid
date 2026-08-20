# ADR-0080: Bound runtime diagnostics to secret-safe closed codes

- Status: Accepted
- Date: 2026-08-18
- Blueprint source: Sections 3–7, 12–13, 16–18, and 21
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/dioxus-wallet/src/logs.rs`, `telemetry_panel.rs`, `proc_stats.rs`, and the wallet worker boundary
- Tracking: issues #2, #46, and #60
- Amends: ADR-0013, ADR-0018, ADR-0021, ADR-0024, ADR-0032 through ADR-0035, and ADR-0077
- Implementation state: closed-code application port, bounded process-local adapter, standalone composition, headless snapshot/reset, Dioxus health panel, DUST/Zswap panic recovery, transfer-worker loss reporting, and retained Passport Vault call cleanup implemented; persistent logs, telemetry, upload, process statistics, and free-form fields remain excluded
- Amended by: ADR-0095

## Context

The prototype includes useful operator visibility, but its tracing store and
telemetry panel retain free-form event fields, HTTP and operation statistics,
process memory/CPU readings, and a persistent Redb log. Those facilities are
not safe to move into an identity wallet unchanged: a message, endpoint,
external response, credential identifier, or transaction detail can become a
second unreviewed data channel. ADR-0013 keeps telemetry off, and ADR-0021
therefore excluded captured diagnostics and benchmark telemetry by default.

Oxid still needs to distinguish healthy runtime behavior from bounded worker
failures. In particular, a panic in a DUST or shielded sync thread previously
could leave the published session permanently `syncing`. A panic during the
retained Passport Vault completion path could skip removal of its active
submission reservation. The ordinary transfer worker already converted sender
loss into an outcome-unknown state, but exposed no safe reason for operators or
the headless conformance harness.

## Decision

Add an Oxid-owned diagnostics application boundary containing a closed
`DiagnosticCode` enum, a closed severity enum, payload-free sequence events,
aggregate counts, and snapshot/reset use cases. There is deliberately no custom
message, target, timestamp, field map, endpoint, profile identifier, request
identifier, external error, or binary payload variant.

Compose one bounded in-memory ring per application process. Its capacity is
fixed at composition time and capped at 1,024 events; the default is 256. It
retains total and evicted counts plus aggregate closed-code counts. It does not
write a file, upload data, sample the process, install a tracing subscriber, or
read ambient telemetry configuration. Recording is fail-silent and can never
change a wallet result or become authorization evidence.

Expose the snapshot to Dioxus and the versioned headless incoming adapter. The
views state `process_local`, `telemetry: off`, and `payloadsRetained: false`.
Reset requires the exact `CLEAR_LOCAL_DIAGNOSTICS` intent. Invalid JSON,
invalid request envelopes, and unknown headless methods add only their closed
codes; the rejected input and request id do not enter the store.

Wrap native DUST and shielded worker bodies in an unwind boundary. An
unexpected panic publishes the same sanitized transport-unavailable failure
class used at the application boundary, marks the session terminal, and
records one worker-panic code. Normal terminal failures and thread-spawn
failures use distinct closed codes. Cancellation semantics do not change.

Always release the process-local Passport Vault call reservation after
completion, including unwind. If unwind happens after broadcast, retain the
existing outcome-unknown safety rule; before broadcast, return unavailable.
The ordinary transfer sender-disconnect path records a worker-terminated code
while preserving its non-retryable outcome-unknown semantics.

This ADR reviews a narrow replacement for useful runtime-health visibility. It
does not import or authorize the prototype's persistent logs, tracing fields,
benchmark tabs, HTTP statistics, RSS/CPU sampling, or telemetry panel.

## Consequences

- Standalone mobile and headless runs can inspect the same bounded health state
  without creating a second secret-bearing protocol.
- Runtime diagnostics disappear on process exit and cannot support historical
  analytics, crash uploads, or remote observability.
- A closed enum requires a reviewed code change for every new event class;
  callers cannot smuggle data through code names or fields.
- Sync worker panics no longer wedge a session in `syncing`, and retained
  contract-call panics no longer leave an active process reservation behind.
- Panic hooks and platform crash reporting remain outside this boundary. They
  must not be enabled or widened without a separate privacy review.

## Rejected alternatives

- Copying the prototype Redb tracing layer was rejected because persistence and
  free-form fields violate the wallet's local-first secret boundary.
- Retaining sanitized strings was rejected because sanitization is incomplete
  and strings inevitably become an escape hatch for identifiers and payloads.
- Exporting diagnostics through the profile, backup, or credential stores was
  rejected because runtime health is neither wallet state nor recovery state.
- Making diagnostics authoritative for retry or readiness was rejected because
  dropped events, poisoned locks, and process restarts are expected and must not
  change wallet safety decisions.
