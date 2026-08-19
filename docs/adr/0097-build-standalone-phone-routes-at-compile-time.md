# ADR-0097: Build standalone phone routes at compile time

- Status: Accepted
- Date: 2026-08-19
- Source: Blueprint §§3–8, 12–13, 16–18, 21; reviewed prototype phone profile; issues #2/#32
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`
- Implementation state: Opt-in Android physical-device harness and protected
  live-account synchronization implemented; production and native-custody
  composition unchanged

## Context

The reviewed prototype contains a second undeployed network entry whose node,
indexer, and proof-server routes point at one developer's fixed Tailscale IPv4
address. It lets a physical phone reach the laptop-hosted standalone stack, but
committing a personal endpoint into Oxid would violate the public-repository
and route-ownership boundaries. Android cannot obtain Cargo build environment
variables as process environment at launch, so Oxid's existing runtime
headless configuration is also not a phone deployment mechanism.

The standalone proof server receives private DUST witness material. ADR-0027
therefore permits non-loopback proving only over HTTPS. Treating a tailnet IP as
implicitly secure and admitting plain HTTP would weaken that existing contract.

## Decision

Add an explicit `standalone-tailnet` app feature that is valid only together
with `standalone-development` on iOS or Android. The repository Android phone
launcher obtains the laptop's current MagicDNS name from the local Tailscale
client and supplies four complete routes to Rust at compile time. The feature
contains a stable marker so the release-exclusion gate can prove that a normal
artifact contains neither profile code nor configured endpoints. It cannot be
combined with native custody and cannot alter a normal production build.

Keep network identity separate from transport. The profile still selects the
single `undeployed` Midnight network. A public fixture address validates the
transport at composition time only; after profile activation, the existing
account derivation use case binds the profile's protected derived address and
the live adapter discards any cached placeholder state. No fixture key, seed,
or funded account is added.

Package a repository-owned Docker Compose harness using the exact reviewed
prototype image versions for node, indexer, and proof server. Containers bind
only to loopback. `standalone-phone-up` requires an otherwise-empty Tailscale
Serve configuration, then creates three owned TLS reverse proxies:

- indexer HTTPS/WSS on port 8443;
- node WSS on port 10000;
- proof-server HTTPS on port 443.

The script generates the indexer's development passwords and secret under the
ignored `target/standalone` directory with owner-only permissions. It does not
commit them. A marker records that Oxid owns the temporary Serve configuration;
the paired down command resets it only when that marker exists and leaves the
generated local environment file in place.

## Consequences

- A physical phone can use the same typed standalone adapters without a
  hard-coded personal address or a generic native/JavaScript command channel.
- Tailscale performs TLS termination, so ADR-0027's remote-prover policy stays
  intact and private witness transport is not downgraded to plaintext HTTP.
- Route selection remains build-time-only and development-only. Changing the
  stack or tailnet requires a rebuild; production discovery remains open work.
- The initial public address is not evidence of ownership, funding, sync, or
  settlement. Only the profile-derived binding can become wallet state.
- Every persistent live/standalone constructor gives the Midnight adapter the
  exact same public profile repository used by the application services. The
  selected network and non-secret derivation coordinates therefore survive a
  process restart; process-local development custody still returns honestly as
  uninitialized and withholds the former account addresses after that restart.
- The pinned `indexer-standalone:4.0.0` image exposes regular-transaction fees
  through the compatible `fees { paidFees }` shape. Oxid uses that shape for
  account history while retaining its existing response, event, and timeout
  bounds.
- The harness may temporarily configure local Tailscale Serve and run three
  Docker containers. It refuses to overwrite an unrelated Serve configuration
  and supplies an explicit cleanup command.
- Verified public Android App Links remain blocked on an approved HTTPS domain
  and `assetlinks.json`; a private MagicDNS route is not substituted as that
  production proof.
- The reviewed prototype also has a localhost standalone transport profile
  with the same undeployed chain identity. Oxid still needs a separate
  compile-time local-stack simulator profile; it must not be implemented as
  runtime production route selection.
