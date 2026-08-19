# ADR-0098: Authenticate deployment profiles and isolate standalone funding

- Status: Accepted
- Date: 2026-08-20
- Source: Blueprint §§3–8, 12–13, 16–18, 21; reviewed prototype live wallet path; issues #2/#90
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`
- Implementation state: Signed profile and node-genesis gates implemented;
  guarded funded standalone unshielded finality and restart flow proven;
  no production trust root, signed profile, issuer/verifier transport, or
  deployment is selected by the default application

## Context

ADR-0097 deliberately limits localhost and tailnet routes to compile-time
development profiles. Production cannot infer Midnight or SSI authority from
environment variables, an indexer response, DNS alone, or a runtime network
picker. An endpoint set must be authenticated as one unit, bound to the exact
Midnight chain, and replaceable without permitting profile rollback.

The reviewed prototype completes real standalone transactions successfully,
but its deployment-oriented development code also contains a public genesis
funding fixture and runtime network choices. Those are useful evidence inputs,
not production configuration. Oxid previously had live-capable typed adapters
and deterministic completion tests but no guarded funded real-node acceptance
flow. Live testing also exposed three assumptions hidden by synthetic fixtures:

- Docker health does not mean the indexer has replayed the node;
- indexer v4 block timestamps are Unix milliseconds, while the ledger uses
  seconds; and
- DUST event identifiers are increasing, sparse global cursors rather than a
  contiguous DUST-only sequence.

## Decision

Introduce a dependency-light deployment-profile adapter with a closed
`oxid.deployment-profile.v1` JSON envelope. The payload is serialized in the
adapter's exact canonical field order and signed with Ed25519. One signature
atomically binds:

- application audience, profile identifier, validity interval, and monotonic
  sequence;
- Midnight network identifier and 32-byte genesis hash;
- Midnight indexer HTTP/WebSocket, node WebSocket, and proof-server routes;
  and
- SSI DID resolver, issuer metadata, and verifier metadata routes.

The verifier accepts only build-reviewed public trust roots. Each root has an
activation interval, optional revocation time, and minimum sequence; the
verifier also applies an application-wide sequence floor. Duplicate roots,
unknown signers, unsigned/tampered/noncanonical payloads, rollback, expired or
future profiles, unknown production network identifiers, credentials in URLs,
query/fragment ambiguity, and non-HTTPS/WSS routes fail closed with
payload-free errors. A thin application that eventually selects this path must
compile its reviewed roots and sequence floor into the release; runtime
environment variables are not a trust-root or profile source.

After signature verification, asynchronously connect to the signed node route
and require its Substrate genesis hash to equal the signed hash before creating
an `AuthenticatedProductionDeployment`. Composition receives that opaque
value, cannot splice alternate routes, uses the same durable public profile
repository for account association, and enables only the reviewed HTTPS DID
resolver. The default `compose()` function is unchanged and fail-closed.
Issuer/verifier protocol transports, status/revocation, background policy, and
live DID writes remain unavailable because a URL profile does not implement or
authorize those capabilities.

Add a separate ignored, test-only funded standalone harness. It requires both
`OXID_ENABLE_LIVE_STANDALONE_FUNDING=1` and an operator-supplied
`OXID_STANDALONE_FUNDER_SEED_HEX`. The seed is never committed, rendered,
logged, or persisted; a zeroizing random adapter supplies it exactly once for
the development root and delegates all later randomness to the operating
system. Every run derives a fresh OS-random recipient, transfers exactly five
NIGHT after the unchanged preview and explicit authorization, proves and
submits through the typed standalone adapter, observes finalized inclusion,
reconstructs the adapter from its public submission journal, reconciles, waits
boundedly for indexer convergence, and proves a stable exact recipient balance
on a second read. The normal release binary is scanned for the harness markers.

Strengthen the standalone launcher so its readiness gate compares node and
indexer heights rather than trusting shallow container health. Decode indexer
v4 timestamps as milliseconds and preserve subsecond truncation at the ledger
boundary. Accept sparse DUST cursors only when they move strictly forward and
their advertised target never moves backward. These rules match the reviewed
prototype's working flow while retaining Oxid's response, size, time, replay,
and cancellation bounds.

## Validation

The focused repository evidence is:

```bash
nix develop -c cargo test -p oxid-adapter-deployment-profile
nix develop -c cargo test -p oxid-adapter-midnight chain_tip_
nix develop -c cargo test -p oxid-adapter-midnight dust_
OXID_ENABLE_LIVE_STANDALONE_FUNDING=1 \
  OXID_STANDALONE_FUNDER_SEED_HEX=<operator-supplied-development-seed> \
  nix develop -c just standalone-funded-finality
```

The funded command passed against the repository-owned standalone node,
`indexer-standalone:4.0.0`, and proof server on 2026-08-20. Its seed remains
out-of-band evidence. The integrated release exclusion, strict, Nix, iOS, and
Android gates remain required before the implementation commit is pushed.

## Consequences

- Production endpoint discovery now has a typed authentication and chain
  identity boundary without pretending that a deployment exists.
- Midnight and SSI routes rotate atomically; signature, validity, revocation,
  audience, and sequence checks prevent unauthenticated mixing or rollback.
- A signed endpoint is not proof of protocol correctness, issuer trust,
  credential status, custody durability, or response-delivery authorization;
  those remain separate ports and issues.
- The standalone funding authority is test infrastructure only. It cannot be
  selected by a product feature, normal composition, mobile runtime
  environment, or production release.
- Funded unshielded prepare/authorize/prove/submit/finalize/restart evidence is
  complete for the headless standalone adapter. A funded shielded spend,
  funded mobile UI journey, physical proof budgets, and real production
  deployment remain open.
- No private root, fixture seed, endpoint credential, personal tailnet name,
  transaction material, or device identifier is added to the repository or
  public issue evidence.
