<!-- SPDX-License-Identifier: Apache-2.0 -->

# ADR-0051: Isolate Passport Vault as a product-specific hexagon

- Status: Accepted
- Date: 2026-08-14
- Blueprint: §§3–7, 9–13, 16–18
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/wallet-core/vault`
- Contract reference: `midnight-identity-solution-examples` commit `e4a92a6be2cc6dc34f68261f10c19c9312043807`, `packages/contracts/vault/src/passport-vault.compact`, SHA-256 `2ebc5b34dd440bc9a9736408f29f5003e7a78f26a564b392be2af36de69102f4`
- Related: ADR-0001, ADR-0003, ADR-0004, ADR-0006, ADR-0013, ADR-0015, ADR-0017, ADR-0020, ADR-0021, ADR-0024, ADR-0038, ADR-0042, ADR-0045, ADR-0050, issues #2 and #31
- Implementation state: the exact public multi-lock behavior, Compact Digital Passport policy verification, standalone product adapter, owner-private durable standalone ledger, headless flow, and Dioxus mobile journey are implemented; live Compact state and transactions remain issue #31

## Context

The reviewed prototype exposes a Passport Vault dApp journey alongside the
wallet: create a lock, add unshielded NIGHT, claim with a selectively disclosed
Digital Passport, withdraw as the creator, list locks, and show aggregate
accounting. A lock constrains minimum age, optional issuing state and document
number, maximum claim amount, credential issuer/key trust, expiry, a fresh
challenge, and current chain time. Claims consume a nullifier derived from the
lock and credential root, so the same credential cannot claim twice from one
lock.

The prototype Rust service is coupled to a WebView/Node bridge, iframe origin,
hard-coded dApp configuration, generated artifacts, and relative companion
workspace paths. Its useful behavior is product-specific; importing those
runtime and filesystem assumptions would violate Oxid's Rust-first,
reproducible, and capability-specific boundaries. Putting the behavior in
generic wallet or credential core would also make those reusable capabilities
depend on one dApp's policy and accounting model.

The companion Compact source was not pinned by the prototype. The reference
above records the reviewed remote `develop` state and exact source digest for
provenance only. It is not yet an authenticated runtime artifact dependency.

## Decision

Passport Vault owns separate dependency-free `domain` and `application` crates.
The domain owns bounded lock identifiers, public policy, creator authorization,
checked accounting, maximum-claim and solvency rules, and per-lock
credential-root replay prevention. It does not own wallet account, VC format,
Midnight SDK, Dioxus, or persistence types.

The application owns focused repository and credential-policy ports plus list,
create, deposit, claim, and withdraw use cases. Every state-changing command
requires an exact, human-readable confirmation intent. Lock creation obtains a
fresh non-zero challenge from the platform randomness port. Claim rechecks that
the lock policy has not changed after asynchronous credential verification.

The Midnight VC adapter verifies the exact detached Compact issuer proof,
signed credential/body binding, private openings and commitment root, the
standalone issuer DID and Jubjub public point, expiry, minimum age, optional
issuing-state/document predicates, and current day. It returns only a redacted
credential fingerprint and verifier-controlled day to the vault application;
claim values, openings, proof bytes, and credential roots never enter incoming
views or logs.

Standalone composition originally used a bounded process-local repository.
ADR-0068 supersedes that repository choice for native headless/mobile
composition with a bounded owner-private atomic file while preserving the
exact credential-policy adapter. In-memory and WASM composition remains
process-local. Every composition labels the source `standalone` and never
reports creation, deposit, claim, or withdrawal as an on-chain submission.
Production composition wires unavailable ports and fails closed. The headless
protocol and Dioxus mobile UI call the same use cases.

A live adapter is a separate issue #31 delivery. It must authenticate immutable
Compact sources/artifacts through Nix, decode native state in Rust, and route
all four transactions through the existing prepare/authorize/prove/submit/
reconcile boundary. Its public state must be chain-derived, its current day
must be block-derived, and ambiguous broadcast outcomes must not be retried
blindly.

## Rejected alternatives

- Copying the WebView JavaScript, Node modules, iframe route, or relative
  companion-workspace lookup would preserve prototype coupling and ambient
  mutable state.
- Putting Passport Vault policy inside wallet or credential core would invert
  ownership and make generic capabilities depend on one product contract.
- Returning successful standalone mutations as chain submissions would create
  false custody and settlement claims.
- Reusing the prototype's historical credential timestamps would make newly
  issued standalone credentials immediately expire; standalone issuance must
  use the composed clock and re-sign the exact body.
- Hard-coding a contract address or accepting generated artifacts without
  source/digest authentication would make live semantics non-reproducible.

## Consequences

- Product behavior can evolve without contaminating reusable wallet and SSI
  packages.
- Headless and mobile flows can exercise complete accounting, policy, consent,
  and replay behavior without a chain or foreign runtime.
- Native standalone state and consumed-credential replay evidence survive
  restart in the separately reviewed ADR-0068 store; in-memory and WASM
  composition remain process-local.
- Live parity is not claimed by the standalone adapter; source and state mode
  remain explicit at every incoming boundary.

## Validation

- Domain tests cover multi-lock accounting, creator authorization, bounds,
  maximum claims, solvency, checked arithmetic, and per-lock replay rejection.
- VC adapter tests cover issuer/key anchoring, commitments, proof, expiry, age,
  required values, and credential fingerprint construction.
- The headless end-to-end flow creates a profile and managed DID, issues the
  exact Compact Digital Passport, creates and deposits to a lock, refuses an
  unconfirmed claim, succeeds once, rejects replay, withdraws, and verifies
  aggregate accounting without exposing credential material.
- Tier-1 iOS and Android smoke flows exercise the same create/deposit/claim/
  withdraw journey through visible Dioxus controls.
