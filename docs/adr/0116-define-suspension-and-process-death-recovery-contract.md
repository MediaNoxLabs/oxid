# ADR-0116: Define suspension and process-death recovery contract

- Status: Accepted for iterative adoption
- Date: 2026-09-19
- Parent: [#558](https://github.com/MediaNoxLabs/oxid/issues/558)
- Follow-ups: [#645](https://github.com/MediaNoxLabs/oxid/issues/645), [#646](https://github.com/MediaNoxLabs/oxid/issues/646)
- Issue: [#644](https://github.com/MediaNoxLabs/oxid/issues/644)
- Implementation state: decision only; native adapters and background schedulers are not introduced by this ADR

## Context

A mobile app can be suspended, backgrounded, or terminated without a final
transport event. Connectivity callbacks only signal a possible change; they do
not prove a request or WebSocket remains healthy. In-memory UI or runtime state
therefore cannot be the authority for a selected wallet realm.

The selected-realm coordinator established by ADR-0114 already owns application
policy and reconciliation. UIKit/SwiftUI, Android lifecycle components,
WorkManager, `BGTaskScheduler`, Dioxus, and transports must not become
independent synchronization planners.

## Decision

The app process and every transport are disposable. Durable truth is the chain
plus an atomic local selected-realm checkpoint and a bounded operation journal.
A WebSocket is only an acceleration path for new observations.

Thin native adapters emit only these normalized application inputs:

| Signal | Coordinator effect |
| --- | --- |
| `Activated { generation }` | Fence old work, publish retained projection, and plan bounded freshness reconciliation. |
| `Deactivating { deadline }` | Stop nonessential admission, atomically persist durable state before the deadline, then cancel work. |
| `Suspended` | Keep no correctness dependency on process memory, a timer, or an open socket. |
| `ConnectivityChanged { generation, state }` | Treat the observation as a trigger; replace invalid transport and reconcile from durable cursors. |
| `ColdStarted { generation }` | Validate durable state, increment the lifecycle generation, publish retained truth, and reconcile. |
| `BackgroundOpportunity { deadline, constraints }` | Optionally plan a bounded, secret-safe, cancellable reconciliation only. |

Every effect and observation carries the current lifecycle/transport generation.
The coordinator rejects a completion, failure, cursor update, or projection
write from an older generation. Replacement work is single-flight per
`(profile, realm)` and supersedes the previous transport without waiting for a
close frame.

When a `Deactivating` deadline expires, the coordinator cancels the remaining
work, discards any uncommitted candidate checkpoint, and retains the preceding
complete checkpoint. Deadline expiry cannot promote a partial projection or
extend execution in the hope of finishing a write; the next activation or cold
start performs normal reconciliation from the retained checkpoint.

### Durable boundary

One atomic checkpoint contains the selected realm identity, coherent projection,
durable cursors, checkpoint revision, and safe retry metadata. The operation
journal contains only unresolved public operation identities, durable phase,
idempotency/reconciliation state, and bounded payload-safe evidence. A crash at
any persistence boundary restores either the preceding complete checkpoint or
the next complete checkpoint, never a mixed projection. Incomplete writes are
rejected or quarantined.

A submitted operation that loses observation is `outcome_unknown`, not failed.
On recovery the coordinator reconciles it by exact public transaction identity
before any retry; it never blindly resubmits. Duplicate lifecycle events,
transport failures, and completion events are idempotent under the journal
revision and generation fence.

### Recovery and freshness

On activation or cold start, the coordinator:

1. opens protected storage and validates the atomic checkpoint;
2. increments the runtime/lifecycle generation and invalidates old workers;
3. immediately publishes the retained last-consistent projection with
   `Updating` or `Offline` freshness;
4. reconciles unresolved submissions by exact public identity;
5. recreates request/response and stream transport from durable cursors;
6. plans the minimum stale facets needed for one coherent selected-realm
   projection: public NIGHT, shielded NIGHT, DUST, activity, and relevant
   operation or registration status; and
7. atomically records completion evidence while retaining the prior consistent
   projection on partial failure.

Actions request their own freshness preflight and post-action settlement. The
normal UI exposes no family-specific synchronization controls; it may offer one
contextual retry when automatic recovery cannot proceed.

### Authorization and optional background work

Deactivation expires protected authorization capabilities. Seed or mnemonic
access, signing, proving, DUST registration, and transfer authorization cannot
silently resume after the user-presence boundary. A durable phase that cannot
continue with valid authorization publishes `Action required`, not a false
failure or automatic continuation.

`BGAppRefreshTask`/`BGProcessingTask` and Android unique WorkManager work are
optional optimization opportunities, never correctness dependencies. They may
perform only idempotent, deadline-aware, network-constrained, cancellable,
single-flight reconciliation from durable cursors. They must not unlock
custody, sign, prove, submit a protected transaction, retain an unrestricted
WebSocket, or be required for foreground recovery. Initial correctness evidence
is foreground and cold-start recovery; platform background work follows only
when measured benefit justifies it.

### Metrics and evidence

The coordinator records secret-safe, bounded metrics for recovery latency,
reconnect attempts, checkpoint age, retry count, and abandoned-worker count.
Metrics exclude secret material, transaction bodies, protected handles, and
endpoint credentials.

Deterministic headless transition tests will cover active → deactivating →
suspended → active, cold-start from checkpoint, crash points around atomic
persistence, stale-generation rejection, duplicate events, transport
replacement, and `outcome_unknown` reconciliation. Platform tests follow the
headless contract and simulate iOS background/termination and Android process
death/network replacement.

## Consequences

- The selected-realm coordinator remains the sole policy owner across headless,
  Dioxus, iOS, and Android surfaces.
- A retained projection can be truthful immediately while reconciliation is
  bounded and visible.
- Native adapters stay thin and platform execution timing cannot compromise
  correctness.
- Follow-up implementation must preserve the command/event matrix above rather
  than adding platform-specific state machines.

## Alternatives rejected

- Treating an open process or WebSocket as durable state loses correctness on
  suspension, termination, and silent transport failure.
- Letting each platform schedule its own synchronization policy duplicates
  retries, freshness rules, and authorization boundaries.
- Blindly retrying an unobserved submission can duplicate a public operation.
- Requiring iOS or Android background execution for correctness conflicts with
  system-controlled scheduling and protected authorization boundaries.

## References

- Apple: [Managing your app's life cycle](https://developer.apple.com/documentation/uikit/managing-your-app-s-life-cycle),
  [`ScenePhase.background`](https://developer.apple.com/documentation/swiftui/scenephase/background),
  and [Using background tasks to update your app](https://developer.apple.com/documentation/uikit/using-background-tasks-to-update-your-app).
- Android: [Processes and app lifecycle](https://developer.android.com/guide/components/activities/process-lifecycle),
  [Save UI states](https://developer.android.com/topic/libraries/architecture/saving-states),
  [persistent background tasks](https://developer.android.com/develop/background-work/background-tasks/persistent),
  and [network state](https://developer.android.com/develop/connectivity/network-ops/reading-network-state).
- ADR-0114: typed resource graph and application-owned reconcilers.
