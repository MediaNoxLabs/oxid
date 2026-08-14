<!-- SPDX-License-Identifier: Apache-2.0 -->

# ADR-0066: Enable native vault claim discovery after managed conformance

- Status: Accepted
- Date: 2026-08-14
- Blueprint: §§3–7, 9–13, 16–18, 21
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/dioxus-wallet/web/src/entry.ts` (`prepareVaultClaim`)
- Related: ADR-0047 through ADR-0050, ADR-0058 through ADR-0065, and issue #31
- Supersedes: ADR-0065's temporary public-discovery hold
- Implementation state: native `claim_from_lock` discovery is enabled for `native_settlement`; mobile live-call UX and real-node fixtures remain backlog work

## Context

ADR-0065 connected protected Digital Passport presentation material to the
generated `claimFromLock` client and the shared native settlement lifecycle,
but deliberately kept the operation out of capability discovery. Schema and
isolated adapter tests did not prove that standalone issuance, current managed
DID custody, authenticated contract trust, generated composition, Midnight
funding, and terminal submission could work as one path.

The final evidence gate must not use the prototype's public claim-root-derived
holder scalar or fixed nonce `17`. It must also exercise the packaged generated
client rather than a fake composer, and contract authority must come from an
exact serialized ledger state rather than incoming issuer or policy fields.

## Decision

Add a composition-level conformance flow that runs when the authenticated
packaged Passport Vault composer is configured. The flow:

1. creates a standalone wallet profile and initializes protected custody;
2. derives and synchronizes the profile's Midnight account;
3. creates a managed Midnight DID containing the custody-backed Jubjub holder
   method;
4. completes the standalone OpenID4VCI flow and stores a freshly issued,
   holder-bound Digital Passport credential;
5. builds a claim-ready serialized contract-state fixture whose issuer DID,
   method, and `persistentHash<JubjubPoint>` are derived from the reviewed
   standalone issuer anchor;
6. prepares a native claim without touching protected presentation material;
7. authorizes the exact public plan, causing the real managed-holder source and
   packaged generated `claimFromLock` client to run; and
8. submits the resulting funded transaction through the shared Midnight
   completion port and verifies terminal public inclusion metadata.

The fixture state and deterministic terminal completion are conformance
adapters only. They do not label simulated chain state as live or prove a real
node broadcast. Production `native_settlement` composition still requires
canonical finalized replay and uses ADR-0063's protected DUST proving,
persist-before-broadcast, node submission, and reconciliation adapters.

The standalone issuer trust-anchor type exposes its public method identifier
and Compact persistent public-key hash so the conformance contract state uses
the same byte-exact trust values as credential verification. No secret scalar,
nonce, proof, opening, or credential bytes are exposed to an incoming port.

After this flow passes, headless capability discovery may advertise
`claim_from_lock` in `native_settlement` alongside create, deposit, and
withdraw. Deterministic simulation remains visibly separate. The incoming
claim request continues to contain only lock ID, amount, and opaque credential
ID; issuer trust, policy, finalized time, proof, and witness remain unavailable
to callers.

## Rejected alternatives

- Enabling discovery after only the fixed-schema JavaScript validation test
  would not prove custody, credential storage, or composition wiring.
- Injecting a fake protected-presentation source would not prove that the
  current managed DID method signs the credential-family presentation.
- Reusing the prototype holder scalar or nonce would violate ADR-0048,
  ADR-0064, and the wallet custody boundary.
- Calling the deterministic completion a live node test would overstate the
  evidence. Real-node and mobile live-call fixtures remain explicit backlog
  items.

## Consequences

- Native headless clients can truthfully discover all four wallet-facing
  Passport Vault operations when the complete native stack is configured.
- The conformance test crosses the actual standalone issuance, protected DID,
  credential repository, generated client, funding, and submission ports.
- A failure anywhere before terminal submission keeps capability enablement
  from passing repository CI.
- Mobile live-call presentation and real-node fixture coverage still need
  separate delivery; this decision does not claim those surfaces are complete.

## Validation

- `nix develop --command cargo test -p oxid-composition standalone_managed_claim_composes_and_settles_through_the_native_stack`
- `cargo test -p oxid-adapter-vc-midnight --lib`
- `cargo test -p oxid-headless --lib`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `nix develop --command ./run.sh --light --strict`
- `nix flake check`
