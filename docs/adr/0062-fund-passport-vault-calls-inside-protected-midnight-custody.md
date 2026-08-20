<!-- SPDX-License-Identifier: Apache-2.0 -->

# ADR-0062: Fund Passport Vault calls inside protected Midnight custody

- Status: Accepted
- Date: 2026-08-14
- Blueprint: §§3–8, 12–13, 16–18, 21
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/wallet-core/src/wallet.rs` and `mobile-bench/wallet-core/src/tx/balance.rs`
- Related: ADR-0013, ADR-0015, ADR-0017, ADR-0026 through ADR-0028, ADR-0033 through ADR-0035, ADR-0056 through ADR-0061, and issue #31
- Supersedes: only ADR-0061's implementation-state boundary that NIGHT funding remains pending
- Implementation state: complete standalone composition funds authorized native create/deposit drafts with synchronized unshielded NIGHT inputs and retains withdraw drafts without funding; claim, DUST completion, proving, durable submission, and finalized reconciliation remain pending
- Amended by: ADR-0063

## Context

ADR-0061 joins authenticated contract state with exact public Midnight wallet
context and retains a generated, unproven call transaction. Create and deposit
transactions contain a native unshielded NIGHT deficit. Before DUST balancing
or proving, the wallet must cover that exact deficit from synchronized account
UTXOs, return change, and sign every selected input.

The prototype performs this work between generated-call composition and DUST
completion. Its later JavaScript bridge failed when the number of supplied
signatures did not equal the number of selected inputs. Migrating only the
generated call would therefore preserve neither the behavior nor the critical
authorization invariant.

Funding consumes a serialized transaction and produces another serialized
transaction. Neither value is safe for an incoming protocol, application view,
diagnostic log, or ordinary wallet service. The user must also authorize the
exact staged operation before UTXO selection or signing begins.

## Decision

The native Passport Vault adapter retains the composed transaction in a
zeroizing buffer while the draft is prepared. Only a byte-for-byte matching,
unexpired authorization challenge may invoke its composition-only funding
port. A challenge mismatch, expired draft, or already-invalid state performs no
funding work.

The composition root connects that private port to the same protected Midnight
adapter used by ordinary wallet transactions. It transfers only the opaque
profile identifier, exact network identifier, expiry, an operation-derived
funding requirement, and the zeroizing serialized transaction. No application
or incoming port can obtain that request or result.

The Midnight adapter fully decodes one bounded, tagged standard transaction,
rejects trailing bytes, requires the selected profile network and exactly one
generated contract intent, and calculates the exact native unshielded NIGHT
deficit from ledger balance semantics. Create and deposit must have exactly one
such deficit. Withdraw must have none. Any other negative unshielded token,
multiple deficit segments, or disagreement with the typed operation fails
closed.

For a valid create or deposit, the adapter requires a synchronized protected
account, greedily selects bounded spendable UTXOs using the existing wallet
selection rule, adds exact change, and signs the funding intent through the
opaque account authorizer. It independently verifies the returned Schnorr
signature and supplies one signature for every selected input. A guaranteed
deficit receives a separate `0xBEEF` funding intent; a fallible deficit is
grafted only into the exact generated contract intent's fallible unshielded
offer. The result must have no remaining native NIGHT deficit before it is
serialized into a new zeroizing buffer.

Funding failure leaves the retained draft and its original composed transaction
unchanged in `prepared`, allowing an explicitly reauthorized retry after wallet
state changes. Successful funding replaces the retained transaction and moves
the draft to `authorized`. Repeated authorization is idempotent and does not
select or sign again.

Complete standalone composition reports `native_funded_draft` when canonical
replay, exact public context, and the packaged composer are present.
`settlesOnMidnight` remains false. Submit and claim remain unavailable until
protected claim composition, DUST completion, proving, durable journaling,
broadcast, and reconciliation are composed.

## Rejected alternatives

- Funding during prepare would spend privacy-sensitive synchronization and
  custody work before the user authorized the exact operation.
- Sending the transaction through headless or mobile code would expose
  signatures and serialized ledger material and invert the hexagonal boundary.
- Supplying one signature regardless of input count would reproduce the
  prototype bridge failure and create an invalid offer.
- Treating create/deposit funding as an amount supplied by the caller would let
  incoming code disagree with the generated ledger effects.
- Reusing the ordinary transfer draft/submission port would conflate a complete
  contract transaction with a simple recipient transfer and expose the wrong
  lifecycle.
- Reporting settlement after NIGHT funding would conceal the still-missing
  DUST, proof, journal, broadcast, and outcome authorities.

## Consequences

- Native create/deposit authorization now retains a correctly funded unproven
  transaction without exporting private ledger material.
- Withdraw follows the same protected validation boundary but cannot acquire
  NIGHT inputs accidentally.
- The selected UTXO set is an execution-time wallet decision; the authorized
  operation binds the required amount while fresh synchronized state chooses
  inputs and change.
- The next slice can consume the funded retained transaction behind a combined
  DUST balancing/proving boundary. It must preserve the current zeroizing
  custody, fail-closed retry, and truthful capability labels.

## Validation

- `cargo test -p oxid-adapter-midnight --lib`
- `cargo test -p oxid-adapter-passport-vault --lib`
- `cargo test -p oxid-composition --lib`
- `cargo clippy -p oxid-adapter-midnight -p oxid-adapter-passport-vault -p oxid-composition --all-targets -- -D warnings`
- `nix develop --command ./run.sh --light --strict`
- `nix flake check`
