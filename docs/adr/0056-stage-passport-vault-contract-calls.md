<!-- SPDX-License-Identifier: Apache-2.0 -->

# ADR-0056: Stage Passport Vault contract calls before proof and submission

- Status: Accepted
- Date: 2026-08-14
- Blueprint: §§3–8, 12–13, 16–18, 21
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/wallet-core/src/wallet.rs`, `mobile-bench/dioxus-wallet/web/src/entry.ts`, and `mobile-bench/dioxus-wallet/src/bridge.rs`
- Contract source: `midnight-identity-solution-examples` commit `e4a92a6be2cc6dc34f68261f10c19c9312043807`, distributed byte-identically at `contracts/passport-vault/passport-vault.compact`
- Related: ADR-0013, ADR-0015, ADR-0017, ADR-0026 through ADR-0028, ADR-0034, ADR-0035, ADR-0048 through ADR-0055, ADR-0058, and issue #31
- Implementation state: capability-specific application port, authenticated-state gate, retained lifecycle, fail-closed composition, headless protocol, and authenticated four-circuit proof resolver are implemented; the generated-Compact composer/funding/submission adapter remains issue #31

## Context

The prototype composes Passport Vault calls in JavaScript, passes serialized
unproven transactions back to Rust, balances NIGHT and DUST, proves, and submits
them. It implements create, deposit, and claim paths, compiles withdrawal
artifacts without exposing the withdrawal operation through the wallet bridge,
and performs every expensive step after one immediate UI action.

That path cannot be copied directly. Its claim composer derives a holder scalar
from a public credential root and uses the fixed nonce `17`. It also accepts
contract state from an indexer without authenticating the state bytes. A direct
`call(circuit, args)` incoming API would expose contract ABI details and make it
easy to bypass consent, custody, freshness, and retry rules.

Oxid already has stronger transaction rules: prepare an exact public preview,
authorize that retained draft, prove and submit separately, persist public
post-broadcast metadata, permit cancellation only before broadcast, and
reconcile ambiguous outcomes against finalized chain state. Passport Vault
calls need the same guarantees, but their state anchor and operation semantics
are different from a simple unshielded transfer.

## Decision

Passport Vault mutation uses a product-specific retained-call port. Its closed
operation enum contains exactly the four wallet operations:

1. `create_lock`;
2. `deposit_to_lock`;
3. `claim_from_lock`;
4. `withdraw_from_lock`.

`setTrustedIssuer` remains a deployment/administration capability rather than
an end-user wallet operation. It is reproducibly compiled and verified but is
not exposed by an incoming wallet adapter.

Preparation normalizes the exact 32-byte contract address, validates canonical
decimal amounts and policy fields, generates each create-lock verifier
challenge through the platform randomness port, and reads the contract state
through the application-owned state-source port. It proceeds only when the
snapshot is labeled `CanonicalFinalizedReplay`. An indexer-supplied or merely
node-anchored snapshot is read-only and cannot reach the call adapter.

The outgoing adapter receives the exact authenticated serialized state, its
canonical transaction/block anchor, the selected profile identifier, the typed
operation, and a one-hour expiry. For a claim it receives only an opaque
credential identifier. Credential bytes, private values, openings, holder
keys, signing nonces, witness values, serialized transactions, signatures, and
proofs remain adapter-owned and never appear in incoming commands or public
views.

The adapter retains all chain-specific material behind an opaque draft ID and
an authorization challenge. The public lifecycle is:

`prepared → authorized → submitting → submitted`, with explicit `expired`
state and a separate submission-attempt state. Authorization requires the
exact `AUTHORIZE_PASSPORT_VAULT_CALL` intent. Proving/broadcast requires the
separate `SUBMIT_PASSPORT_VAULT_CALL` intent. An adapter must reject a changed
state anchor, operation, amount, credential reference, or expired draft.

Submission follows ADR-0034/0035 semantics. Cancellation is cooperative and
safe only before the adapter's atomic broadcast boundary. Once broadcasting,
an unknown outcome remains non-retryable until finalized reconciliation.
Public history may contain at most 128 unique draft records and never contains
the signed/proven transaction.

The headless protocol exposes typed prepare, authorize, draft, submit/start,
status, history, cancellation, and reconciliation methods. It is the primary
flow harness for the future native adapter. Until that adapter is installed,
composition reports the methods as `composition_dependent` and returns a safe
capability-unavailable error. Local process-only `vault.*` simulation remains a
separate, explicitly labeled test path.

The future native adapter may use a headless generated-Compact sidecar only as
an outgoing implementation detail with authenticated artifacts and bounded
IPC. It may not expose a browser/WebView bridge, raw circuit arguments, or a
general signing oracle. Holder authorization must use current opaque managed
custody and fresh randomness, and its result must be independently checked
before the claim circuit is composed.

## Rejected alternatives

- Extending the generic unshielded-transfer request with arbitrary contract
  bytes would erase the product policy and authenticated-state boundary.
- Copying the prototype WebView bridge would retain public-data-derived keys,
  fixed nonces, unauthenticated state, and secret-bearing JavaScript messages.
- Submitting immediately from `vault.deposit` or `vault.claim` would bypass an
  exact retained preview, deliberate authorization, safe cancellation, and
  ambiguous-outcome recovery.
- Treating node-anchored indexer state as sufficient would authorize mutation
  from bytes ADR-0054 explicitly labels unproven.
- Omitting withdrawal because the prototype UI did not wire it would preserve
  a known parity gap despite the reviewed contract and artifacts supporting it.
- Exposing `setTrustedIssuer` beside holder/locker operations would turn a
  deployment authority into a general wallet capability without a reviewed
  administration model.

## Consequences

- Incoming adapters have one stable, typed contract-call lifecycle while
  concrete Compact composition can be delivered independently.
- All four user-facing operations share the wallet's strongest authorization,
  cancellation, persistence, and reconciliation expectations.
- Every draft binds to authenticated canonical replay state, making stale or
  unproven indexer bytes unusable for mutation.
- Claim composition has a deliberately narrower boundary than the prototype:
  only an opaque credential reference crosses the application port.
- The current headless methods are useful for validation and discovery but
  truthfully fail closed until the native adapter is composed.
- ADR-0058 supplies the authenticated generated client, exact wallet-circuit
  resolver, and parameters needed by a later call adapter. That adapter must
  still implement generated-Compact composition, funding, combined
  contract/DUST proving, node submission, durable public journaling, and
  finalized reconciliation; this ADR does not claim those operations are live.

## Validation

- Application tests cover all four typed operations, canonical amount policy,
  authenticated-state admission, one-hour expiry, two-stage confirmation,
  draft binding, submission projection, history, and reconciliation.
- Unproven state and unavailable adapters fail before any call is composed.
- Headless tests cover the closed action shape, canonical decimals, capability
  discovery, active-profile scope, safe errors, and credential-ID redaction.
- `cargo test -p oxid-passport-vault-application`
- `cargo test -p oxid-headless --lib`
- `./run.sh --light --strict`
