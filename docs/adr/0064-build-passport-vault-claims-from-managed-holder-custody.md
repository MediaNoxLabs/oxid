<!-- SPDX-License-Identifier: Apache-2.0 -->

# ADR-0064: Build Passport Vault claims from managed holder custody

- Status: Accepted
- Date: 2026-08-14
- Blueprint: §§3–7, 9–13, 16–18, 21
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/dioxus-wallet/web/src/entry.ts` (`prepareVaultClaim`)
- Related: ADR-0042, ADR-0045, ADR-0048 through ADR-0050, ADR-0056 through ADR-0063, and issue #31
- Supersedes: ADR-0063's implementation-state boundary that protected claim material has no native preparation path
- Implementation state: protected claim material preparation is implemented in `vc-midnight`; generated-client composition, funding, proving, submission, and public capability enablement remain pending
- Amended by: ADR-0065

## Context

The Passport Vault claim circuit consumes an exact issuer-signed Digital
Passport credential, its detached issuer proof, a selectively disclosed
presentation, a holder presentation proof, the block-relative current day, and
a private date-of-birth witness. Unlike create, deposit, and withdraw, this
operation cannot be composed from public state and wallet addresses alone.

The prototype assembles the complete claim in a WebView. It derives the holder
secret scalar from the credential's public claim root and uses the constant
presentation nonce `17`. That makes the holder key reconstructible from public
data and reuses signing randomness. It also lets JavaScript receive the complete
credential bundle, openings, witness, scalar, and transaction context. Those
choices cannot cross Oxid's protected credential and key-custody boundaries.

The repository already has the necessary safe primitives: exact native
credential/proof/private-material codecs, current-holder reauthorization,
managed Jubjub challenge signing with custody-owned randomness, independent
holder-proof verification, canonical replay, authenticated contract state, and
the generic protected Midnight settlement lifecycle. The missing decision is
where these pieces meet for a claim without exposing private material or
prematurely advertising the capability.

## Decision

`vc-midnight` owns a composition-only protected Digital Passport presentation
source. Its request identifies a protected credential and carries the exact
public issuer anchor, lock policy, verifier challenge, and finalized timestamp
obtained by a later caller from authenticated Passport Vault state. It is not an
incoming protocol type, and UI/headless callers may not provide those policy or
trust fields.

Preparation re-fetches the profile-scoped credential, requires its stored
verification outcome and exact identifier binding, and natively re-verifies the
signed body plus detached issuer proof. It recomputes the issuer public-key hash
using the same `ValueReprAlignedValue` binary representation as Compact's
`persistentHash<JubjubPoint>`, then compares the DID contract address, method
identifier, and key hash with the contract-pinned anchor. Expiry uses the
finalized chain timestamp. Current day is derived from that same timestamp, and
the age, issuing-state, and document-number requirements are checked against
commitment-validated private parts before custody is invoked.

The adapter constructs only the disclosures required by the lock. It first
requires current protected control of the exact credential-bound holder method,
then asks the managed Compact holder-proof port to sign the presentation root
and lock challenge. That port retains the Jubjub scalar and chooses fresh
randomness inside custody. The adapter parses and independently verifies the
returned signer, timestamp, challenge, and Schnorr proof before producing any
composer input. No scalar or nonce derived from credential data is permitted,
and the prototype nonce `17` is never used in production.

The result is a fixed-shape serializable composer DTO containing the exact
credential, public proofs, selectively disclosed values/openings, current day,
and age witness expected by the generated claim client. It has a redacted
`Debug` representation and recursively zeroizes all retained fields on drop.
Decoded private parts are explicitly zeroized after the DTO is formed. A later
Passport Vault adapter must stream this DTO only to the authenticated,
one-request child composer after the existing exact authorization boundary and
must zeroize any serialization buffer it creates.

This slice does not add `claim_from_lock` to native capability discovery and
does not relax the existing composer rejection. Claim remains fail-closed until
authenticated lock state supplies the request, the generated client consumes
the protected DTO, and the resulting transaction passes the same authorization,
funding, DUST proving, journal, submission, and reconciliation lifecycle as the
other native vault calls.

## Rejected alternatives

- Copying the WebView claim composer would expose private credential material
  to JavaScript and recreate a second transaction authority.
- Deriving a holder scalar from the public claim root would make the holder key
  public rather than prove wallet custody.
- Reusing nonce `17`, or any deterministic public nonce, would violate Schnorr
  signing requirements and permit linkability or key compromise.
- Trusting the credential's embedded issuer key without the contract-pinned
  DID/method/key hash would let a valid but untrusted issuer satisfy the lock.
- Accepting wall-clock `currentDay` or proof time from an incoming caller would
  split contract and wallet time authority.
- Advertising claim before the generated composition and settlement path is
  complete would make the public capability label false.

## Consequences

- Oxid can now construct the sensitive credential/presentation portion of a
  native claim without exporting a holder scalar, nonce, or full private bundle
  to an incoming adapter.
- Contract trust, policy, expiry, and holder custody all fail before a claim
  transaction can be composed.
- The generated Passport Vault composer needs a protected claim input route
  distinct from its public create/deposit/withdraw schemas.
- Public and mobile claim capability remains unchanged until that route is
  wired through the existing native settlement lifecycle.

## Validation

- `cargo test -p oxid-adapter-vc-midnight protected_presentation`
- `cargo test -p oxid-adapter-vc-midnight --lib`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `nix develop --command ./run.sh --light --strict`
- `nix flake check`
