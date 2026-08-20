# ADR-0072: Embed authenticated Compact artifacts for mobile measurement

- Status: Accepted
- Date: 2026-08-17
- Blueprint source: Sections 3–7, 9–13, 16–18, and 21
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, Digital Passport presentation and mobile proving paths
- Tracking: issues #2, #27, #29, and #30
- Amended by: ADR-0083
- Implementation state: the exact runtime-minimal artifact set can be embedded and authenticated in an opt-in iOS/Android native-custody build; ADR-0083 now connects that explicit build to a foreground-only proof worker and visible Dioxus success, while normal mobile composition, physical-device budgets, and issue #29 release evidence remain fail-closed

## Context

ADR-0050 proves and independently verifies the reviewed Digital Passport
presentation circuit in native headless mode. Its runtime accepts an absolute,
authenticated Nix artifact root and the first Apple-silicon baseline reports
high enough memory use that directly enabling the same path on mobile would be
unsafe. ADR-0071 supplies device-bound native custody, but physical-device
recovery and lifecycle evidence remains open.

Mobile packaging adds a distinct trust problem. An iOS bundle resource can be
opened as a regular file, while an Android APK asset is normally compressed or
must be extracted through platform APIs. Copying the 135 MiB runtime set to a
mutable app-private directory would add storage pressure and a new update,
cleanup, partial-write, and authentication lifecycle. Runtime download or
ambient cache discovery would also contradict the wallet's offline,
reproducible proof boundary.

The current proof future performs substantial synchronous CPU work when it is
polled. Wiring it directly to the Dioxus consent handler would block the UI
executor and would not provide trustworthy cancellation, backgrounding, or
process-death semantics.

## Decision

For the first mobile proof-resource gate, embed only the runtime-required
artifact set in the native executable:

- `manifest.json` from the pinned Nix derivation;
- the 85,011,711-byte prover key;
- the 2,311-byte verifier key;
- the 2,915-byte compiled ZKIR; and
- the 50,332,036-byte `bls_midnight_2p18` parameter file.

The four runtime artifacts total 135,348,973 bytes; including the 2,764-byte
manifest makes the selected embedded input 135,351,737 bytes. Build-time
`include_bytes!` reads only `OXID_PRESENTATION_ARTIFACTS_DIR` supplied by
`nix build .#presentation-compact-artifacts`. The resulting application does
not discover a runtime path, extract an APK asset, consult a mutable cache, or
fetch a network resource. At startup the existing adapter checks the compiled-in
source/toolchain/circuit identity plus every exact size and SHA-256, parses the
compiled circuit and verifier key, and fails closed before the UI launches.
The embedded bytes stay borrowed from the executable image until the proving
library requires owned key material.

Select this package only with the app feature
`standalone-native-proving-artifacts`. It implies
`standalone-native-custody`, is rejected on non-mobile targets, and is exposed
by the launch scripts only as
`OXID_MOBILE_PRESENTATION_PROVING=artifacts`. The launch scripts ignore an
ambient artifact path and resolve the exact Nix output themselves. Development
custody plus the artifact package is rejected.

This feature originally authenticated and measured packaging only. ADR-0083
amends that execution deferral: the same explicit feature now selects the
foreground proof worker. Normal builds do not select the feature and carry no
Compact presentation artifact payload.

The future execution adapter must satisfy all of the following before mobile
proof success is composed:

- execute at most one proof on a dedicated worker while the app is foreground;
- never perform proving or artifact authentication on the Dioxus UI executor;
- accept cancellation at reviewed safe points and acknowledge it only after the
  worker has stopped using witness and custody material;
- discard partial state and any late result on backgrounding, timeout,
  low-memory interruption, explicit cancellation, or process death;
- require a fresh request preview, consent, holder authorization, and proof
  attempt for retry rather than resuming a partial proof; and
- expose only bounded status and timing/resource measurements, never a proof,
  witness, opening, nonce, holder material, or serialized proof preimage.

Physical iOS and Android measurements must still establish release-mode prover
and verifier latency, peak/resident memory, package delta, free-storage
requirements, thermal behavior, interruption behavior, and regression budgets.
Until those budgets and ADR-0071's physical-device gate are accepted, the
mobile presentation capability remains unavailable.

## Security and truth boundaries

- The embedded artifacts are public proving material, not wallet data or a
  mutable cache. They must never contain witnesses, credentials, openings,
  nonces, custody references, or device-specific state.
- The Nix store path is a build input only. No store path or environment route
  becomes a runtime trust decision.
- A signed app package does not replace the adapter's exact manifest, digest,
  size, circuit, and verifier-key checks.
- Successful package authentication is not evidence that proof execution fits
  a device budget. ADR-0083 sets `compact_presentation_proof_available` only in
  the explicit standalone execution harness, not normal mobile composition.
- Package byte counts are build evidence, not a claim about installed size,
  memory pressure, thermal behavior, or store-download size.

## Consequences

- iOS and Android use one platform-neutral immutable packaging model without
  an extraction bridge or duplicated artifact copy.
- Opt-in measurement packages are large by design. This creates honest package
  and startup evidence before proof execution is enabled.
- Normal standalone simulator testing remains fast and unchanged.
- ADR-0083 supplies the standalone proof worker, cancellation, and lifecycle
  integration. Reviewed physical-device budgets remain required before any
  production composition can connect ADR-0050 proof success to Dioxus.

## Current measurement evidence

The first debug-only simulator/emulator packaging run on 2026-08-17 records:

- iPhone 17e, iOS 26.4, `aarch64-apple-ios-sim`: 257,526,696
  uncompressed application-bundle bytes versus 173,593,496 for the ordinary
  development build, a debug bundle delta of 83,933,200 bytes; after more than
  one minute the wallet UI was responsive and host `ps` reported 455,136 KiB
  RSS for the app process;
- Android arm64 emulator: 539,163,753 APK bytes; after more than 45 seconds the
  wallet UI was responsive; the ordinary development APK was 404,307,855
  bytes, making the debug APK delta 134,855,898 bytes; `dumpsys meminfo`
  reported 310,462 KB total PSS, 427,424 KB total RSS, and no swap.

The focused aarch64-darwin release test authenticated the embedded set and
constructed the checked runtime in 3.92 seconds. `/usr/bin/time -l` reported
5.44 seconds wall time including the test process, 440,074,240 bytes maximum
resident set size, 211,911,424 bytes peak memory footprint, and no swaps. This
is a host baseline for package authentication only; it does not execute a
proof and is not a mobile budget.

These debug virtual-device numbers prove only that the exact package
authenticates and reaches the existing fail-closed wallet UI on both targets.
They are not release-build deltas, physical-device measurements, thermal
evidence, proof-execution measurements, or accepted regression budgets.

## Rejected alternatives

- Bundling regular iOS resources and extracting Android APK assets would create
  divergent platform paths and mutable-copy lifecycle rules before their value
  is demonstrated.
- Downloading keys or parameters at runtime would introduce network discovery,
  rollback, partial-download, cache-authentication, and availability risks.
- Embedding the entire generated Compact output would increase the package with
  compiler metadata and JavaScript that the native runtime never consumes.
- Enabling the existing proof future directly in Dioxus would block the UI
  executor and overstate cancellation and lifecycle safety.
- Using development custody for proof measurements would bypass the native
  user-presence dependency that governs eventual mobile composition.
