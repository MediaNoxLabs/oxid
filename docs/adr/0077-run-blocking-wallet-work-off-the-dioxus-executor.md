# ADR-0077: Run blocking wallet work off the Dioxus executor

- Status: Accepted
- Date: 2026-08-18
- Blueprint source: Sections 3, 6–7, 12–13, 16, and 18
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/dioxus-wallet/src/worker/mod.rs`
- Tracking: issues #2 and #42
- Implementation state: native profile/account reads and writes, wallet protection, account derivation, transfer preparation/authorization, Passport Vault call authorization, managed DID storage/custody operations, and complete/legacy backup cryptography execute on an 8 MiB background thread; the remaining synchronous Dioxus call sites still require explicit fast-local or background classification under issue #42

## Context

Dioxus `spawn` schedules a future without promising that synchronous work inside
that future leaves the UI executor. Oxid's synchronous application ports are
appropriate for small deterministic core operations and headless composition,
but some outgoing adapters can block: native Android authorization waits for an
operating-system credential result for up to 65 seconds, iOS may wait for user
presence, JSON/encrypted repositories perform filesystem work, and portable
backup runs Argon2id plus authenticated archive validation. Calling those ports
inside an `onclick`, `use_effect`, or an async block still freezes Dioxus's frame
and event loop.

The reviewed prototype recognized a related boundary. It dispatched heavy store,
chain, DID, and protocol work to a dedicated thread with an 8 MiB stack so those
state machines did not run on Android's smaller WebView dispatch stack. Its
worker messages also carried seeds, controller secrets, aggregate wallet state,
and UI-routing concerns. Oxid needs the scheduling property, not that coupling or
secret-bearing message schema.

## Decision

The Dioxus incoming adapter owns a private, executor-neutral blocking-task
bridge for native targets. Each admitted operation receives owned Oxid commands
and cloned capability ports, executes on a named `oxid-ui-blocking` OS thread
with an 8 MiB stack, and returns its typed result through a one-shot channel.
Dioxus signals and event handlers never cross the thread boundary; the awaiting
UI task alone applies the result.

An event must publish a busy/loading state before dispatch so a second click
cannot race the same operation. Worker creation, worker panic, and lost-result
failures collapse to one payload-free UI message. Adapter error bodies, panic
payloads, recovery secrets, backup bytes, signing inputs, key references, and
private credential material must not enter the bridge's error surface or logs.

The first mandatory set is every synchronous Dioxus path that can invoke native
user authorization, protected key derivation/signing, portable backup KDF or
recovery, or the persistent profile/account/DID repositories that surround those
flows. Transaction and recovery use cases retain their own atomicity,
cancellation, and reconciliation contracts. Dropping a component or future does
not pretend an already-started blocking operation was cancelled.

Pure bounded parsing/formatting may remain on the UI executor. An application
port whose contract is already asynchronous may be awaited directly only when
its adapter owns any blocking hop. Adapter-owned status/start/cancel operations
may remain direct only when their contract guarantees a bounded in-memory
transition and starts heavy work elsewhere. Every other synchronous call site
must be classified under issue #42 before that issue closes.

Browser composition uses its current in-memory adapters directly because a
native OS thread is unavailable. A production Tier-2 browser adapter that adds
filesystem, network, or expensive cryptography requires a separate reviewed Web
Worker boundary; this fallback is not permission to block a browser UI thread.

## Consequences

- Native authorization prompts and backup KDF/storage work no longer stop
  Dioxus event processing or depend on a Tokio runtime being installed.
- The 8 MiB stack preserves the prototype's useful Android safety margin while
  Oxid keeps capability ports and typed results instead of one aggregate worker
  protocol.
- A worker is deliberately one operation, not a global mutable wallet facade.
  Existing application/adapter gates remain the concurrency authority.
- Blocking work cannot be forcibly cancelled safely. UI cancellation remains
  available only where the application/adapter already defines a cooperative
  cancellation boundary.
- Native thread creation has a bounded per-operation cost. A reviewed shared
  executor may supersede this decision if measurements demonstrate a need
  without weakening isolation or error handling.

## Rejected alternatives

- Treating `spawn(async move { sync_execute() })` as non-blocking was rejected
  because it still runs `sync_execute()` on the Dioxus executor.
- Making every core use-case trait asynchronous was rejected because it would
  leak one incoming adapter's scheduling concern through headless and core
  boundaries.
- Calling Tokio `spawn_blocking` was rejected because the Dioxus mobile runtime
  contract does not guarantee an ambient Tokio runtime.
- Copying the prototype `WorkMsg`/`WorkOutcome` facade was rejected because it
  centralizes unrelated capabilities and previously carried secrets across the
  UI worker boundary.
- Moving Dioxus signals into a shared worker registry was rejected because those
  values are UI-thread-affine and do not belong in an outgoing execution layer.
