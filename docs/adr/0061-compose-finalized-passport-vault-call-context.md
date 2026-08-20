<!-- SPDX-License-Identifier: Apache-2.0 -->

# ADR-0061: Compose finalized Passport Vault call context

- Status: Accepted
- Date: 2026-08-14
- Blueprint: §§3–8, 12–13, 16–18, 21
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/wallet-core/src/indexer.rs` and `mobile-bench/wallet-core/src/wallet.rs`
- Related: ADR-0013, ADR-0015, ADR-0017, ADR-0026 through ADR-0028, ADR-0033 through ADR-0035, ADR-0054 through ADR-0060, and issue #31
- Implementation state: complete standalone composition can prepare and authorize retained native create/deposit/withdraw drafts from canonical replay and exact public Midnight context; claim, NIGHT funding, DUST completion, proving, durable submission, and finalized reconciliation remain pending
- Amended by: ADR-0062

## Context

ADR-0060 retained a generated Compact call transaction but intentionally left
its public-context source uncomposed. The prototype obtains a contract action's
Zswap state and current block ledger parameters from the indexer, and the
wallet's coin, encryption, and unshielded public values from its selected
account. Those authorities belong to separate outgoing adapters. Incoming
headless or mobile commands must not choose them, and neither outgoing adapter
may depend on the other.

Indexer state is not independently authenticated. Oxid already reconstructs
the contract state through complete canonical finalized-node replay, so public
composition material is usable only when its indexer action is the exact action
authenticated by that replay.

## Decision

The Midnight wallet adapter owns a profile-scoped public call-context port. It
loads the selected account, requires exactly one unshielded and one shielded
address, checks the exact network-specific Bech32m HRPs and 32/64-byte payload
lengths, and splits only the shielded public payload into coin and encryption
public keys. Its debug view exposes only the network identifier.

The node-anchored Passport Vault source requests, at one node-finalized height,
the exact block hash and ledger parameters plus the contract action's state and
Zswap state. All hexadecimal payloads and the entire response are bounded. The
source verifies the returned finalized block against the node, verifies the
action block's canonical node hash, and retains at most one context per bounded
contract entry. A context is bound to the contract-state digest, transaction,
action block, and compatible finalized head.

The canonical replay source composes that node-anchored source only in complete
standalone mode. It rejects any address, state byte, transaction hash, action
block hash, or action height mismatch. A later finalized head is accepted only
for the same replayed action; an equal-height head must have the exact replayed
hash.

The composition root is the only component that joins the two sources. The
native composer receives context for the exact state snapshot in the prepare
request. Neither incoming protocol nor application state exposes keys, chain
parameters, Zswap bytes, or the serialized transaction.

When `OXID_PASSPORT_VAULT_DEPLOYMENT_HEIGHT` selects canonical replay and the
packaged `OXID_PASSPORT_VAULT_COMPOSER` is present, complete standalone
composition reports `native_composed_draft` and installs the ADR-0060 retained
adapter. A missing composer preserves `native_pending`; an invalid configured
composer fails startup. `settlesOnMidnight` remains false. Claim and submit
remain unavailable.

## Rejected alternatives

- Accepting Zswap state, ledger parameters, or public keys in incoming commands
  would let an untrusted caller choose transaction authority.
- Having the Passport Vault adapter decode Midnight addresses would create a
  second address-codec authority.
- Having one outgoing adapter call the other would violate the hexagonal
  dependency direction and make composition implicit.
- Trusting a node-anchored indexer state without byte-for-byte replay agreement
  would promote an explicitly unproven read model into mutation authority.
- Reporting settlement readiness after composition would hide the still-missing
  funding, combined proving, journal, broadcast, and reconciliation boundaries.

## Consequences

- The complete standalone stack can exercise real generated create, deposit,
  and withdraw composition behind the same headless application ports as the
  mobile UI.
- Public chain material is cached only after node anchoring and is usable only
  with its exact canonical replay snapshot.
- Address decoding stays inside the Midnight adapter and cross-adapter joining
  stays inside the composition root.
- The next settlement slice must consume the retained transaction within the
  Midnight adapter's protected funding/DUST/proving/submission boundary; it may
  not export the transaction to accomplish that.

## Validation

- `cargo test -p oxid-adapter-midnight --lib`
- `cargo test -p oxid-adapter-passport-vault --lib`
- `cargo test -p oxid-composition --lib`
- `cargo clippy -p oxid-adapter-midnight -p oxid-adapter-passport-vault -p oxid-composition --all-targets -- -D warnings`
- `nix develop --command ./run.sh --light --strict`
