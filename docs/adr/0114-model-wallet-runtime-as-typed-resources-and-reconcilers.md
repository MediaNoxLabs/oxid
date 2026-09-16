# ADR-0114: Model wallet runtime as typed resources and reconcilers

- Status: Accepted for iterative adoption
- Date: 2026-09-17
- Issue: [#559](https://github.com/MediaNoxLabs/oxid/issues/559)
- Follow-up: [#558](https://github.com/MediaNoxLabs/oxid/issues/558)

## Context

Oxid already separates wallet domain objects, application use cases, outgoing
ports, adapters, composition, and incoming headless/Dioxus adapters. It also
has independent public-account, shielded, DUST, transaction, deployment-health,
and custody state. The missing boundary is the runtime model that explains how
those resources relate and how bounded background work reconciles them.

The absence of that model leaks implementation details into the UI. A user can
be asked to start account, shielded, or DUST synchronization manually and then
infer whether a later action is ready. Dioxus lifecycle effects can also become
the accidental owner of timers and retry policy even though the headless wallet
needs the same behavior.

The secret vocabulary also needs a precise correction. BIP-39 defines mnemonic
generation and mnemonic-to-seed derivation as separate operations. Its
specification explicitly notes that the latter is one-way for arbitrary wallet
seeds: a wallet seed cannot be converted back into its originating mnemonic.
A mnemonic, wallet seed, and protected custody root therefore cannot be one
generic "seed" resource.

Relevant primary sources are:

- the [BIP-39 specification](https://github.com/bitcoin/bips/blob/master/bip-0039.mediawiki),
  including its one-way mnemonic-to-seed limitation;
- the Rust documentation for [enums and exhaustive pattern matching](https://doc.rust-lang.org/book/ch06-00-enums.html);
- the Kubernetes description of [controller reconciliation loops](https://kubernetes.io/docs/concepts/architecture/controller/);
- the Dioxus documentation for [reactive signals](https://dioxuslabs.com/learn/0.7/essentials/basics/signals/)
  and [cancel-safe asynchronous work](https://dioxuslabs.com/learn/0.7/essentials/basics/async/);
- the OpenTelemetry [trace model](https://opentelemetry.io/docs/specs/otel/trace/api/)
  for operation correlation, spans, events, durations, and bounded attributes.

## Decision

Oxid will evolve toward a **typed resource graph with application-owned
reconcilers**, implemented as a functional core behind the existing hexagonal
ports. This is CQRS-like at the incoming boundary, but it is not full event
sourcing and does not introduce a generic workflow framework.

### Secret resources are directional and distinct

The secret path is modeled as:

`entropy -> mnemonic -> (mnemonic + passphrase) -> wallet seed -> custody root`

- A `Mnemonic` is validated human-portable recovery material. Its words are a
  secret value with explicit reveal, backup, retention, and destruction rules.
- A `WalletSeed` is binary secret material produced by an explicit derivation
  or admitted from another reviewed seed source. It has its own format and
  provenance. There is no seed-to-mnemonic operation.
- A `CustodyRoot` is the opaque protected identity installed from a wallet
  seed. Normal application and presentation code hold a safe handle and state,
  never its secret bytes.

Rust must enforce these boundaries with distinct nominal structs, private
fields, redacted `Debug`, zeroizing `Drop`, no accidental `Clone` or `Copy`, and
directional port methods. Conversion is not represented by interchangeable
byte slices or string aliases.

### Realm, environment, wallet, and facets remain separate

- `NetworkRealm` is stable ledger identity: chain kind, network identifier,
  and a verified genesis or equivalent fingerprint.
- `EnvironmentProfile` is selectable access topology for one realm: node,
  indexer, prover, optional SSI services, route class, and policy.
- `EnvironmentObservation` is the independently refreshed compatibility and
  health state of those services. Changing an endpoint does not change realm
  identity; a healthy endpoint does not imply healthy siblings.
- `WalletProfile` binds a custody root to one realm and its derived accounts.
- Unshielded, shielded, DUST, and activity are typed wallet facets. Each keeps
  its own cursor, freshness, failure, and consistency invariant.
- Wallet and action readiness are derived projections, not mutable booleans
  stored independently from those facets.

Resources are passive typed state. They do not start tasks or call adapters.
Where desired and observed state are both useful, a resource may expose typed
metadata, specification, observed status, conditions, and revisions. Oxid will
not add an inheritance hierarchy, arbitrary string conditions, or a universal
property bag.

### A pure reducer plans effects

The functional core receives one typed input and returns state plus effects:

`reduce(current_state, trigger_or_fact) -> transition { state, effects }`

Inputs distinguish:

- actor intent;
- validated command;
- lifecycle, connectivity, and timer trigger;
- domain event;
- external observation;
- effect completion or sanitized failure.

Effects are closed Rust enum variants such as probing an environment, syncing
one wallet facet, reconciling a transaction, or waiting for confirmation.
Exhaustive `match` statements make missing cases visible during compilation.
Invalid combinations should be unrepresentable where practical and rejected
by constructors otherwise.

The reducer has no Dioxus, Tokio timer, HTTP, filesystem, platform, or proving
dependency. Deterministic tests supply clocks, triggers, and observations. An
imperative application shell executes planned effects behind ports and feeds
the resulting facts back to the reducer.

### Reconcilers and process managers own scheduling

The scheduler is not a domain resource and not a Dioxus timer. It is an
application process manager with observable operational state and policy. For
each `(profile, realm)` it:

- compares desired readiness with observed facet state;
- consumes lifecycle, connectivity, action, completion, and timer triggers;
- acquires single-flight leases;
- prioritizes interactive preflight and confirmation over periodic refresh;
- applies deadlines, cancellation, retry ceilings, jitter, and backoff;
- resumes durable workflows idempotently after restart;
- reports `ActionRequired` rather than crossing recovery, custody, signing, or
  transaction-authorization boundaries automatically.

Separate focused reconcilers may own environment, custody, synchronization,
and transaction concerns. A wallet-level process manager composes their typed
projections; it must not become one god state machine.

### Commands, queries, facts, and effects have different contracts

- Commands request change and carry a typed target, expected revision,
  idempotency key, deadline, actor authority, and operation context.
- Queries are side-effect free and return a projection with revision,
  freshness, and consistency metadata.
- Domain events are past-tense business facts.
- Observations are sanitized facts learned through outgoing ports.
- Triggers prompt reconciliation but are not proof that state changed.
- Effects describe explicit external work selected by the reducer.
- A command returns an operation receipt or typed rejection, never an
  ambiguous success boolean for long-running work.

Delivery is treated as at least once. Effects and workflow steps are
idempotent, revisions detect stale decisions, and the chain is reconciled after
an outcome-unknown submission. Oxid will not claim exactly-once execution.

### Snapshots plus bounded journals, not full event sourcing

Typed snapshots and chain checkpoints remain authoritative for current state.
Only resumable workflow facts are durable. A bounded, secret-safe operation
timeline explains control flow and supplies metrics.

Operation records use closed enums and include operation, correlation, and
causation identifiers; resource key and revision; trigger/effect/outcome;
attempt; timestamps and durations; retry decision; and bounded resource
measurements where available. They never include mnemonics, seeds, keys,
addresses, transaction bodies, credential payloads, shielded notes, endpoint
credentials, or unrestricted adapter errors.

OpenTelemetry compatibility guides identifiers and span relationships, but
this ADR does not add an exporter or require a telemetry service.

### Dioxus and headless are equal incoming adapters

The application runtime exposes presentation-neutral operations:

- dispatch an intent or command and receive an operation receipt;
- query a projection with revision and freshness;
- observe ordered projection changes;
- inspect a sanitized operation timeline and result.

Dioxus maps projections into signals and renders intents. It does not own sync
timers, retries, or workflow sequencing. This avoids holding Dioxus signal
guards across `await` points and keeps component cancellation from corrupting
application workflows.

The headless wallet uses the same commands and projections through its JSON
adapter. Tests can drive explicit ticks and scripted observations without
wall-clock sleeps. Application ports must not expose Dioxus signals, Tokio
channels, JSON values, or a specific telemetry backend.

## Iterative adoption

1. Establish the vocabulary and make mnemonic-to-seed direction explicit with
   distinct Rust types and port operations. Preserve compatibility at existing
   composition boundaries.
2. Add a pure reconciliation planner around the existing selected-realm sync
   service and prove transition tables with headless deterministic tests.
3. Move lifecycle, freshness, priority, retry, and single-flight policy into an
   application runtime. Keep current manual commands as diagnostic escape
   hatches.
4. Expose application-owned projection observation to both headless and Dioxus;
   remove UI-owned scheduling only after parity tests pass.
5. Adopt the operation envelope and resumable process model for transfers and
   DUST registration, one workflow at a time.

Every iteration must pass the headless and desktop fast line. Mobile evidence
is reserved for the final integration slice rather than becoming a dependency
of architecture work.

## Alternatives rejected

- UI-owned orchestration duplicates behavior and is cancelled with component
  lifecycles.
- A generic OOP resource superclass loses Rust exhaustiveness and permits
  invalid stringly states.
- One actor per resource adds mailbox and ordering semantics before they are
  needed and does not itself define domain invariants.
- Full event sourcing would persist too much sensitive history and makes chain
  checkpoints harder, not easier, to reason about.
- An external workflow engine adds deployment and mobile runtime cost without
  solving domain vocabulary.
- A monolithic wallet state machine would replace scattered logic with a new
  god object.

## Consequences

- The compiler will reject more invalid transitions and secret-type confusion.
- Headless and Dioxus behavior can share one policy and one deterministic test
  suite.
- Debugging can answer why work ran, waited, retried, failed, or succeeded from
  one sanitized timeline.
- Existing types are adopted incrementally; temporary compatibility names may
  exist during migration but no second runtime is composed.
- The design adds explicit state and messages. Each abstraction must remove
  duplicated behavior or enforce an invariant; otherwise it is not admitted.
- New requirements should add typed variants, focused reconcilers, or ports
  rather than bypassing the model with presentation-specific side effects.
