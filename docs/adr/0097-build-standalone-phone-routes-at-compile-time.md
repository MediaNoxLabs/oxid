# ADR-0097: Build standalone mobile routes at compile time

- Status: Accepted
- Date: 2026-08-19
- Source: Blueprint §§3–8, 12–13, 16–18, 21; reviewed prototype route profiles; issues #2/#32/#89
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`
- Implementation state: Opt-in localhost simulator/desktop and Android
  physical-device tailnet profiles implemented with public standalone-genesis
  development custody; production and native-custody composition unchanged

## Context

The reviewed prototype contains `Standalone` loopback and `Standalone -
Tailscale` entries for one undeployed network. Its UI selects those entries at
runtime. The second entry points at one developer's fixed Tailscale IPv4
address. It lets a physical phone reach the laptop-hosted standalone stack, but
copying either runtime production selection or a personal endpoint into Oxid
would violate its composition and public-repository boundaries. Android also
cannot obtain Cargo build environment variables as process environment at
launch, so Oxid's existing runtime headless configuration is not a mobile
deployment mechanism.

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

Add a separate `standalone-local` app feature for native development builds.
It composes the same typed live adapters and the same `undeployed` chain
identity from these immutable routes:

- `ws://127.0.0.1:8088/api/v4/graphql/ws`;
- `http://127.0.0.1:8088/api/v4/graphql`;
- `ws://127.0.0.1:9944`;
- `http://127.0.0.1:6300`.

It requires `standalone-development`, conflicts with `standalone-tailnet` and
native custody, cannot select the simulation-only demo drawer, and is
unavailable to WebAssembly. An iOS Simulator reaches
the host's loopback directly. An Android emulator uses repository-owned `adb
reverse` mappings for only ports 8088, 9944, and 6300; the launcher verifies
the mappings and rejects a physical device. Do not substitute `10.0.2.2`:
ADR-0027 allows plaintext proof transport only to syntactic loopback, and that
substitution would weaken the existing witness-transport invariant.

Keep network identity separate from transport. The profile still selects the
single `undeployed` Midnight network. A public fixture address validates the
transport at composition time only; after profile activation, the existing
account derivation use case binds the profile's protected derived address and
the live adapter discards any cached placeholder state.

The explicit live development composition supplies the undeployed chain's
public scalar-one genesis root exactly once when development custody initializes
its first profile. This is intentionally public test authority, not protected
wallet material: anyone can derive it and spend funds assigned to it. Every
later nonce, key reference, and additional profile root uses OS randomness.
The fixture is absent from normal and native-custody composition, never enters
UI/application DTOs or logs, and carries a release-exclusion marker. This
exception exists only so the live Wallet can synchronize the chain's known
NIGHT, shielded, and DUST state through the ordinary protected ports.

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

## Validation

On 2026-08-20, `just ios-standalone-local-smoke` passed its fresh-install
XCUITest on iPhone 17 Pro / iOS 26.4 and `just
android-standalone-local-smoke` passed from a stopped AVD on
`sdk_gphone64_arm64` / Android 15 (API 35). Both flows activated a newly derived
protected account, required `Live` and synchronized live-source state with both
receive-address rails, and rejected the deterministic simulation labels and
balances. The Android run also verified exact reverse mappings for 8088, 9944,
and 6300. The launcher admits only an AVD name backed by a configured `.ini`
file because Android emulator 34.2.16 may write crash-report setup notices to
standard output before the actual `-list-avds` result.

## Consequences

- A physical phone can use the same typed standalone adapters without a
  hard-coded personal address or a generic native/JavaScript command channel.
- The first profile initialized by a live development profile is deliberately
  the shared public genesis wallet. It is suitable only for local demos and
  tests; it provides no privacy, ownership, or safe-funding guarantee.
- iOS Simulator, Android emulator, and native desktop development can use the
  real loopback stack without conflating it with deterministic simulation. Only
  transport differs between localhost and tailnet; network identity, account
  derivation, profile binding, and all state machines are shared.
- Tailscale performs TLS termination, so ADR-0027's remote-prover policy stays
  intact and private witness transport is not downgraded to plaintext HTTP.
- Route selection remains build-time-only and development-only. Changing the
  local/tailnet profile requires a rebuild; production discovery remains open
  work. This is deliberately stricter than the prototype's runtime network
  picker.
- The initial public address is not evidence of balance freshness or
  settlement. Only profile-derived binding plus independent live NIGHT, DUST,
  and shielded synchronization can become wallet display state.
- Every persistent live/standalone constructor gives the Midnight adapter the
  exact same public profile repository used by the application services. The
  selected network and non-secret derivation coordinates therefore survive a
  process restart; process-local development custody still returns honestly as
  uninitialized and withholds the former account addresses until the public
  development fixture is explicitly initialized again.
- The prototype's reviewed
  `wallet-core/queries/midnight-indexer/unshielded_transactions.subscription.graphql`
  does not request transaction fees, so its working sync flow does not exercise
  the schema discrepancy. Oxid's richer account-history projection needs that
  value. Although the reviewed prototype schema advertises both `fee` and the
  deprecated `fees`, the pinned `indexer-standalone:4.0.0` image rejects the
  singular field and accepts `fees { paidFees }`. Oxid uses that proven shape
  while retaining its existing response, event, and timeout bounds.
- The harness may temporarily configure local Tailscale Serve and run three
  Docker containers. It refuses to overwrite an unrelated Serve configuration
  and supplies an explicit cleanup command.
- Verified public Android App Links remain blocked on an approved HTTPS domain
  and `assetlinks.json`; a private MagicDNS route is not substituted as that
  production proof.
- The reviewed prototype also has a localhost standalone transport profile
  with the same undeployed chain identity. Oxid implements its safer equivalent
  as a separate compile-time profile and scans the normal release binary for
  the profile marker and exact loopback routes.
