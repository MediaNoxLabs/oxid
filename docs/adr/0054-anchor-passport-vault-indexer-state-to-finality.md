<!-- SPDX-License-Identifier: Apache-2.0 -->

# ADR-0054: Anchor Passport Vault indexer state to node finality

- Status: Accepted
- Date: 2026-08-14
- Blueprint: §§3–8, 12–13, 16–18, 21
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/wallet-core/queries/contract_state.graphql` and `src/indexer.rs`
- Related: ADR-0003, ADR-0004, ADR-0006, ADR-0013, ADR-0015, ADR-0018, ADR-0020, ADR-0027, ADR-0035, ADR-0051, ADR-0052, ADR-0053, and issue #31
- Supersedes: ADR-0052's undifferentiated “authenticated indexer acquisition” follow-up with the staged trust model below
- Implementation state: bounded finalized-height acquisition, canonical action-block verification, native decoding, headless read method, and fail-closed standalone composition are implemented; state-byte replay/proof authentication, caching, and contract calls remain issue #31
- Amended by: ADR-0055

## Context

The prototype asks the indexer for the latest contract action and directly uses
its state, Zswap state, transaction metadata, and ledger parameters to compose a
call. It does not constrain the query to a finalized height or verify the
reported block against an independently configured node.

Oxid's pinned decoder proves that bytes match the reviewed Passport Vault
layout, but not where those bytes came from. A node can independently establish
a finalized canonical block hash. However, the Midnight transaction stored in
that block contains call transcripts and effects, not the post-call contract
state. A canonical block hash therefore cannot authenticate arbitrary state
bytes returned alongside it by an indexer. Strong state authentication requires
replay from an authenticated prior state or a node storage proof supported by
the runtime.

Conflating those guarantees would let a compromised indexer provide validly
encoded but false policy or accounting state under a real finalized block hash.
That is unsafe input for a claim or any state-changing call.

## Decision

Oxid introduces a Passport Vault contract-state source port separate from the
layout decoder. The native source takes an explicit 32-byte contract address;
it never chooses or hard-codes one. It obtains the latest finalized head and
height from the configured Midnight node, queries `contractAction` at that
height, verifies the returned address and bounds, obtains the node's canonical
hash at the action's reported height, and requires it to equal the indexer's
action-block hash. Only then does the application invoke the exact native
decoder.

The resulting view is labelled `node_anchored_indexer`. It carries the contract
address, indexer transaction hash, canonical action block, observed finalized
head, and the explicit state-authentication value
`indexer_supplied_not_proven`. This is a finality and fork-consistency check,
not a proof of the state bytes or even independent proof that the reported
action occurred in that block. The indexer transaction hash is provenance, not
an independently authenticated transaction inclusion claim.

The headless `vault.contract_state.read` method accepts only
`contractAddressHex`, returns the same safe public projection as the native
decoder plus its anchor, and never mutates wallet or chain state. The source is
composed only when the complete standalone configuration supplies both indexer
HTTP and node WebSocket routes. Normal, simulated, WebAssembly, incomplete, or
invalid compositions fail closed.

Remote indexers require HTTPS and remote nodes require WSS; plaintext HTTP/WS
is accepted only for loopback development. Routes reject credentials, query,
fragment, whitespace, controls, and overlong values. The HTTP client ignores
ambient proxies, rejects redirects, pins the public root set, uses timeouts,
and streams into an exact response bound. State remains bounded to 16 MiB and
all hashes/addresses are canonical 32-byte hexadecimal values.

No node-anchored indexer snapshot may authorize, prepare, prove, submit, or
retry a Passport Vault call. The next transaction slice must first authenticate
the input state by deterministic ledger replay from deployment (or a reviewed
equivalent node proof), retain the relevant Zswap and ledger-parameter anchor,
and then preserve the existing prepare/authorize/prove/submit/reconcile
boundaries per capability.

## Rejected alternatives

- Calling a block-hash-matched indexer response “authenticated state” would
  claim a guarantee the node has not provided.
- Querying the indexer's unconstrained latest action would permit best-chain
  reorgs and lag to be confused with finality.
- Importing the prototype's JavaScript/WebView reader and caller would restore
  an ambient foreign runtime and would not improve the trust model.
- Hard-coding the verifier's deployed address would couple product policy to a
  demo deployment and make network/address mismatches hard to detect.
- Implementing calls immediately on the anchored snapshot would allow validly
  encoded malicious indexer state to influence proofs and public effects.
- Replaying the complete contract history in this slice would combine a useful
  bounded read-model improvement with a materially larger consensus-verification
  and persistence boundary.

## Consequences

- Headless and future UI adapters can read a caller-selected finalized-chain
  view without claiming its bytes are proven.
- A malicious or inconsistent indexer is detected when it reports the wrong
  address, a non-finalized height, or a block hash not canonical at that height.
- A malicious indexer can still forge state bytes and transaction provenance
  under a real canonical action block; the explicit authentication label and
  closed mutation port keep that residual risk visible and non-authoritative.
- The source and decoder can be tested independently, and normal product
  compositions remain fail-closed.
- Issue #31 is narrowed to authenticated replay/proofs, freshness-aware cache
  semantics, exact call construction/proving, submission reconciliation, and
  mobile presentation of live/cached/unavailable states.

## Validation

- Application tests require canonical metadata and preserve the unproven-state
  label.
- Adapter tests reject insecure routes, wrong addresses, future blocks,
  malformed GraphQL, and over-bound state.
- Headless tests validate the address-only method, unavailable composition, and
  public provenance projection.
- `./run.sh --light --strict`
- `nix flake check --print-build-logs`
