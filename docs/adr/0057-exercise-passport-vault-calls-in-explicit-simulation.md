<!-- SPDX-License-Identifier: Apache-2.0 -->

# ADR-0057: Exercise Passport Vault calls in explicit deterministic simulation

- Status: Accepted
- Date: 2026-08-14
- Blueprint: §§3–8, 12–13, 16–18, 21
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/wallet-core/src/wallet.rs`, `mobile-bench/dioxus-wallet/web/src/entry.ts`, and `mobile-bench/dioxus-wallet/src/bridge.rs`
- Related: ADR-0021, ADR-0023, ADR-0024, ADR-0034, ADR-0035, ADR-0051 through ADR-0056, and issue #31
- Implementation state: all four staged calls execute through the zero-configuration headless/development composition; generated-Compact composition, proving, live funding/submission, durable history, and finalized reconciliation remain issue #31

## Context

ADR-0056 established the retained Passport Vault call lifecycle and deliberately
left every composition fail-closed until a native generated-Compact adapter was
available. That protected the live boundary, but it meant the headless harness
could validate only error handling. It could not exercise the complete prepare,
authorize, submit, cancellation, history, and reconciliation protocol used by
the migrated mobile flows.

Oxid already uses deterministic development adapters to exercise complete
wallet journeys without implying that test balances or transactions exist on
Midnight. Passport Vault needs the same kind of harness. Reusing the
`canonical_finalized_replay` authentication label or reporting synthetic
inclusion as chain settlement would, however, erase the trust distinction
created by ADR-0054 and ADR-0055.

## Decision

Add a separate `deterministic_simulation` contract-state authentication class.
The deterministic state source is scoped to one published fixture address,
decodes the existing bounded generated-client fixture, and supplies synthetic
public anchors. It does not claim node finality, indexer provenance, storage
proof authentication, or canonical replay.

The ordinary call-service constructor continues to admit only
`canonical_finalized_replay`. A separate simulation constructor admits only
`deterministic_simulation`. Tests require both constructors to reject the other
authentication class. The simulated outgoing adapter additionally repeats this
check, so an accidental composition change fails before a draft is retained.

The process-local adapter supports exactly the four ADR-0056 wallet operations.
It binds each retained plan to the profile, fixture state anchor, typed
operation, and expiry; derives opaque draft/challenge/transaction/block values
deterministically; preserves the two exact confirmation intents; and models the
pre-broadcast cancellation boundary. Its public history remains bounded to 128
records per profile. No credential identifier, credential body, opening,
witness, custody reference, signature, proof, or serialized transaction is
projected in the public protocol.

Successful simulated submission advances through running and broadcasting to
an adapter-local included status with a deterministic fee and hashes. Every
such result is labelled `deterministic_simulation_only`. The capability summary
reports `mode: deterministic_simulation`, publishes the fixture address, and
reports `settlesOnMidnight: false`. “Included” therefore means included in the
deterministic harness lifecycle, never included in a Midnight block.

Only zero-configuration headless, in-memory, and
`oxid-app/standalone-development` composition receive this adapter. Explicit
standalone live/indexer/replay composition remains `native_pending` and uses an
unavailable call port. Default production composition remains unavailable.
Simulation state and retained history are process-local and disappear at
restart.

## Rejected alternatives

- Labelling fixture state `canonical_finalized_replay` would let synthetic
  anchors satisfy a live mutation precondition.
- Pairing authenticated live replay with simulated submission would make a
  successful response look like a chain mutation even though nothing was
  broadcast.
- Reusing the local `vault.*` accounting repository would couple the contract
  protocol harness to a different state model and conceal the chain-call seam.
- Returning immediate success without the retained worker lifecycle would miss
  cancellation races and mobile/headless status polling behavior.
- Enabling the simulator in production or environment-selected live
  composition would make deployment configuration silently downgrade trust.

## Consequences

- Headless integration tests can execute create, deposit, claim, and withdraw
  through the exact public contract-call protocol today.
- Capability discovery and every inclusion response preserve an explicit,
  machine-readable non-settlement boundary.
- Live replay and deterministic fixtures are different authentication types,
  making accidental cross-composition detectable in code and tests.
- The harness validates lifecycle behavior but does not validate generated
  Compact witnesses, NIGHT/DUST balancing, proving, node submission, durable
  recovery, or finalized chain reconciliation.
- Issue #31 still owns the native adapter; that implementation must replace the
  simulated outcome with authenticated artifact composition and real Midnight
  submission without weakening ADR-0056.

## Validation

- Adapter tests cover determinism, all four operation plans, exact expiry,
  live-state rejection, and cancellation before broadcast.
- Application tests cover bidirectional rejection between live replay and
  simulation authentication classes.
- Headless tests execute all four operations, assert capability and settlement
  labels, retain public history, reject non-canonical amounts, and verify that
  credential identifiers do not appear in the transcript.
- Live/production composition tests continue to fail closed without the native
  adapter.
- `cargo test -p oxid-passport-vault-application -p oxid-adapter-passport-vault`
- `cargo test -p oxid-composition -p oxid-headless --lib`
- `./run.sh --light --strict`
