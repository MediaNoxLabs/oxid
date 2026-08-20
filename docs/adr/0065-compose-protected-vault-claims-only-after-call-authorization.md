<!-- SPDX-License-Identifier: Apache-2.0 -->

# ADR-0065: Compose protected vault claims only after call authorization

- Status: Accepted
- Date: 2026-08-14
- Blueprint: §§3–7, 9–13, 16–18, 21
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/dioxus-wallet/web/src/entry.ts` (`prepareVaultClaim`)
- Related: ADR-0042, ADR-0048 through ADR-0050, ADR-0056 through ADR-0064, and issue #31
- Supersedes: ADR-0059's blanket claim rejection and ADR-0064's implementation-state boundary that the protected DTO has no generated-client consumer
- Implementation state: authorization-bound protected composition and the shared funding/submission route are implemented; an exact managed-custody generated-client settlement conformance test and public claim capability enablement remain pending
- Amended by: ADR-0066

## Context

ADR-0064 produces the exact Digital Passport credential, issuer proof,
selective presentation, holder proof, current day, and private age witness that
`claimFromLock` consumes. Calling that source during draft preparation would
read protected credential material and trigger holder authorization before the
user has confirmed the vault call. Calling it from an incoming adapter would
also let UI or headless fields replace contract trust, policy, or time.

The existing native create/deposit/withdraw path composes an unproven
transaction before call authorization and binds the transaction hash into the
authorization challenge. A protected claim cannot use that sequence: its
transaction does not exist until holder custody has authorized and signed the
presentation. The claim therefore needs a public plan challenge and a distinct
post-authorization composition phase without weakening draft replay,
concurrency, funding, or settlement controls.

## Decision

Canonical replay snapshots now carry the captured finalized-head timestamp in
seconds. The finalized-node collector derives it from the same contiguous block
range used for replay; the node-anchored indexer source independently reads the
finalized block timestamp and must match it. The timestamp is public provenance,
is projected in headless state views, and is included in the native planning
fingerprint. A zero timestamp is invalid.

During `prepare`, the native Passport Vault adapter decodes the exact pinned
ledger layout and obtains the contract-global trusted issuer DID contract,
method, public-key hash, and the selected lock's byte-exact policy and verifier
challenge. It also rejects missing locks, zero or over-limit claims, and claims
larger than the authenticated remaining balance. Display-formatted policy
strings are never parsed back into authority. The adapter retains only the
opaque credential identifier, authenticated public snapshot, wallet/chain
composition context, and exact decoded policy. It does not read a credential,
authorize a holder, construct a presentation, invoke the generated client, or
fund a transaction.

The claim draft ID remains the domain-separated planning fingerprint. Its
authorization challenge uses a claim-specific domain and binds the draft ID,
state anchor, and full planning fingerprint instead of a not-yet-created
transaction. Variable-width fields are length-prefixed. The fingerprint
includes profile, credential ID, operation, contract state and all anchors,
finalized time, expiry, network, Zswap state, ledger parameters, and wallet
public addresses.

Only `authorize` may atomically mark a prepared claim as in progress. The
application service has already required `confirmed: true` and the exact
`AUTHORIZE_PASSPORT_VAULT_CALL` intent before it invokes that adapter method.
After the exact challenge check, and outside the retained-draft lock, the
adapter calls ADR-0064's protected source, streams its owned zeroizing DTO to
the one-request generated composer, validates the returned unproven Midnight
transaction, and passes it to the existing protected funding boundary with no
NIGHT input requirement. The DTO and serialized request are zeroized on drop;
the composer process exits after its one request.

The generated composer accepts one fixed claim schema only. It validates exact
object keys, canonical decimal bounds, booleans, and every 32/64-byte array,
then constructs the generated `DigitalPassportCredential`, `Proof`,
`DigitalPassportPresentation`, witness private state, and the exact
`claimFromLock` argument order. Administration and arbitrary circuit selection
remain forbidden.

Concurrent authorization of the same claim fails with a draft conflict.
Protected composition or funding failure clears the in-progress marker and
leaves the public draft prepared for an explicit retry without retaining a
presentation. Expiry clears pending claim context and any transaction. A
successful claim enters the same authorized DUST/proving, durable journal,
submission, cancellation, and finalized reconciliation lifecycle as the other
native vault calls.

The headless capability manifest continues to omit native `claim_from_lock`
until a real managed-custody credential is composed and settled through the
packaged generated client in conformance. Direct requests therefore remain an
unadvertised fail-closed integration surface during this slice.

## Rejected alternatives

- Preparing the presentation during `prepare` would cross credential custody
  before explicit call confirmation.
- Accepting issuer, policy, challenge, current day, proof, or witness fields
  from headless/mobile input would create a second authority beside the
  authenticated contract state.
- Reusing the public-operation transaction-hash challenge is impossible before
  the protected transaction exists; composing a placeholder transaction would
  not bind the eventual claim.
- Holding the global draft mutex while holder authorization or the child
  process runs would block unrelated calls and make concurrency behavior
  unbounded.
- Retaining the prepared presentation across funding failure would keep
  credential openings and witnesses alive beyond the one authorized attempt.
- Enabling the public claim capability based only on schema and unit tests
  would overstate the current conformance evidence.

## Consequences

- The existing explicit call confirmation is now the first operation allowed
  to touch protected claim material.
- Contract trust, lock policy, balance, verifier challenge, and finalized time
  are authenticated public inputs; credential custody remains adapter-private.
- Native claims can reuse the already reviewed funding, DUST, proving,
  submission, cancellation, and reconciliation machinery without a second
  transaction stack.
- A finalized day may become stale before inclusion near a UTC boundary. The
  contract remains the final acceptance gate; a rejection requires a fresh
  canonical snapshot and a newly authorized claim.
- The next slice must prove a complete managed-custody claim with the packaged
  generated client and then make capability discovery truthful.

## Validation

- `cargo test -p oxid-adapter-passport-vault --lib`
- `cargo test -p oxid-passport-vault-application --lib`
- `cargo test -p oxid-composition --lib`
- `nix build .#passport-vault-call-composer`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `nix develop --command ./run.sh --light --strict`
- `nix flake check`
