# ADR-0106: Bind each wallet profile to one concrete network

- Status: Proposed
- Date: 2026-09-08
- Blueprint source: Sections 1, 3–8, 12–13, 16–18, and 21
- Decision source: owner domain-model review after the milestone 0.2.0 mobile-shell walkthrough
- Tracking: issues #335, #337, and #340
- Amends if accepted: ADR-0025, ADR-0087, ADR-0090, ADR-0097, ADR-0098, and ADR-0105
- Implementation state: current profiles can retain several network associations; migration and enforcement are not implemented

## Context

Oxid currently models a public `WalletProfile` separately from
`WalletProfileAssociations`. One profile can retain accounts for several
networks and select one of them. Home then projects NIGHT, DUST, shielded
state, credentials, and product records as if the selected association were a
presentation filter over one larger wallet.

That is the wrong product aggregate. A user connects to one concrete
blockchain realm, controls one wallet in that realm, and sees the assets whose
authority and provenance belong to that wallet and realm. Switching realm must
not relabel the same collection or leak the previous realm's values under a new
header.

Midnight currently defines the network identifiers `undeployed`, `preview`,
`preprod`, and `mainnet`. The identifier alone is insufficient for standalone:
unrelated local or Tailnet deployments can all report `undeployed`. The signed
deployment-profile work already authenticates node genesis, so concrete
network identity can include that fingerprint without making endpoints part of
the domain identity.

Oxid is intended to support Cardano later, but introducing Cardano account or
asset semantics now would turn a concrete Midnight correction into a generic
multi-chain framework. Likewise, no accepted use case currently requires an
asset to exist outside a network profile.

## Decision

Adopt the aggregate:

```text
Network profile = (profile identity, network type, concrete network, wallet)
Network profile -> wallet -> typed assets
```

### Network type and concrete network

`NetworkType` identifies the blockchain family. `Midnight` is the only
selectable value in this milestone. `Cardano` is reserved for a later accepted
ADR, implementation, and migration; code must not infer Cardano behavior from
an unused enum variant.

A concrete Midnight network identity contains:

1. the native Midnight network identifier (`undeployed`, `preview`, `preprod`,
   or `mainnet`); and
2. an authenticated chain/genesis fingerprint.

The fingerprint distinguishes unrelated standalone realms that share
`undeployed`. Preview, Preprod, and Mainnet also verify it when trusted data is
available. Display names such as “Local standalone” and “Tailnet standalone”
describe connection choices, not a substitute identity.

### One profile, one wallet, one network

Each wallet profile binds exactly one `NetworkType`, one concrete network
identity, and one wallet/custody root. Its network cannot be changed in place.
To use another network, the user creates or selects another profile.

A user may independently restore the same recovery material into more than one
profile, but Oxid does not infer that those profiles, keys, or assets are one
global identity. Any future linkage or cross-chain ownership claim requires an
explicit signed protocol and a later decision.

Connection routes are replaceable configuration beneath the network identity.
Local loopback and Tailnet/MagicDNS may address the same standalone node. A
route change preserves the profile only after the route proves the exact
expected network identifier and genesis fingerprint. A different fingerprint
requires another profile and cannot reuse cached state.

### Assets are profile- and network-scoped

All current user-visible assets are scoped to the active profile and its
concrete network:

- public, shielded, DUST, native, and non-native tokens, including NFTs;
- accounts and addresses;
- deployed or joined smart contracts and application bindings;
- DIDs and their managed keys;
- verifiable credentials, presentations, status/trust evidence, and their
  issuer/holder provenance; and
- synchronization, transaction, and activity projections derived from them.

This list is an information-architecture aggregate, not permission to collapse
the wallet, identity, credential, protocol, and product hexagons into one
generic `Asset` enum. Each bounded context retains its own invariants and
facades; the selected network profile coordinates their projections.

A credential may be intentionally exported, presented, or imported into
another profile. That creates a separately authorized record retaining its
origin and copy lineage. It does not become one mutable global object shared by
profiles.

Oxid defines no global asset store or global Home inventory. A later concrete
use case must explain ownership, provenance, conflict resolution, backup,
deletion, and cross-network security in a superseding ADR before any global
asset category is introduced.

### Profile switching

The top-left profile circle is the primary quick switcher. It opens a bounded
list showing the profile name, network type, concrete network label, and safe
connection status. Profile settings and create/restore actions remain reachable
from the same surface.

Selection is atomic from the user's perspective: clear the prior presentation
projection, establish the new profile/network context, then load its typed
assets. A prior profile's balances, DIDs, contracts, credentials, or activity
must never flash beneath the newly selected profile name. Consent and in-flight
write ceremonies cannot silently cross a profile switch.

### Development wallet

Only an authenticated `undeployed` standalone profile may offer the predefined
public developer wallet. Both local and Tailnet routes may use it when they
prove the compatible standalone genesis. The UI must label it shared, public,
replaceable, and unsuitable for personal identity or value. The user can
instead create a private wallet for either route. Preview, Preprod, and Mainnet
never expose the public fixture.

Wallet creation, mnemonic restoration, raw development seeds, and encrypted QR
recovery are specified separately by issue #340 and existing custody ADRs.

## Migration

Migration from multi-network `WalletProfileAssociations` must be explicit and
lossless:

1. the currently selected network keeps the original profile identity;
2. every additional network association becomes a proposed new profile rather
   than remaining hidden inside the original;
3. records with authenticated network provenance move only to the matching
   profile;
4. records without enough provenance remain in a read-only migration review
   and are not guessed, duplicated, or presented as global; and
5. old checkpoints and endpoint configuration are accepted only after network
   ID and genesis verification.

The migration requires fixtures before the domain schema changes. A fresh
development install may use the new schema directly, but code must not delete
an old association merely because the current milestone has no production
users.

## Consequences

- Home becomes task-oriented and realm-neutral; Wallet and the selected profile
  expose the concrete network's assets.
- Switching profiles is the only normal way to switch network/wallet authority.
- Local and Tailnet connectivity can change without duplicating assets when
  both authenticate the same chain.
- Standalone environments with different genesis fingerprints cannot
  accidentally share balances, DIDs, contracts, or caches.
- Cardano can later add its own concrete identity and typed asset projections
  without weakening Midnight semantics now.
- Portable credentials remain useful, but portability is an explicit operation
  with provenance rather than an implicit global store.

## Verification

- Domain tests reject zero-network and multi-network profiles and reject
  endpoint bundles whose reported network/genesis differs from the profile.
- Migration fixtures cover one and several associations, ambiguous legacy
  records, retry, and rollback without data loss.
- Application/UI tests prove profile switching clears prior projections before
  loading the next profile and cannot carry an in-flight authorization across.
- Standalone tests prove local and Tailnet routes may preserve one profile only
  for the same genesis and require another profile for a different genesis.
- Release checks prove the public developer wallet is unreachable outside the
  authenticated `undeployed` development composition.

## Rejected alternatives

- Keeping one profile with a mutable selected network preserves ambiguous
  ownership and makes stale cross-network UI state easy.
- Treating a Tailnet route as the blockchain identity conflates transport with
  the realm and cannot distinguish two Tailnet-hosted standalone chains.
- A global asset inventory has no current ownership, backup, synchronization,
  or deletion semantics.
- A generic cross-chain `Asset` domain enum would erase the existing hexagon
  boundaries before Cardano requirements exist.
- Inferring global identity from equal roots, DIDs, addresses, or credential
  bytes creates privacy and correlation claims the user did not authorize.
