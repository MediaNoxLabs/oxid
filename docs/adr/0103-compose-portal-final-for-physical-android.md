# ADR-0103: Compose Portal Final for physical Android

- Status: Accepted
- Date: 2026-08-25
- Issue: [#124](https://github.com/MediaNoxLabs/oxid/issues/124)
- Portal source: `22ae5369b6f939e6b20648f4b85dd993527748ef`
- Portal tree: `74d8d1a5b87c160ea554006e47d5f3edc3cd3e10`

## Context

ADR-0102 admits the strict Portal OpenID4VCI Final client only in native
headless development. A physical Android conformance journey also needs the
same native client, holder-managed authentication and Jubjub methods, native
credential-family import, encrypted storage, mobile consent, and strict
identity ingress. It must not make Portal transport runtime-selectable in a
normal or production build.

The merged Portal source now contains the strict Final issuer and a generic,
host-agnostic Tailscale HTTPS profile. Earlier prototype branches coupled Oxid
to an unmerged Portal lifecycle helper, a personal MagicDNS name, a fixed HTTPS
port, and a fixed Android serial. Those inputs are not delivery dependencies.

## Decision

Oxid owns the consumer composition and lifecycle.

The `oxid-app/standalone-portal-tailnet` feature is available only with the
standalone development and tailnet profiles on `aarch64-linux-android`. The
repository launcher supplies two digest-authenticated build inputs:

- the canonical Portal deployment manifest; and
- the canonical physical-Android profile authority.

The deployment manifest schema is `oxid-portal-deployment-v3`. It pins only the
merged Portal commit/tree, the existing Final-profile provenance digest, the
public issuer DID/method/JWK trust facts, and the exact issuer/resolver bases.
Historical pull-request heads and helper revisions are not runtime authority.
Normal production, native custody, WebAssembly, and ordinary mobile builds do
not select the Portal client or offer handoff.

The application receives only a fixed non-secret custom-scheme trigger. A
fresh 64-byte hexadecimal capability is streamed into app-private storage over
ADB standard input. The app unlinks it before issuing a single bounded HTTPS
`/offer` request. The offer, grant, tokens, proof JWT, credential bundle,
private parts, DIDs, and capability never enter process arguments, retained
logs, or evidence.

The holder explicitly previews the claim-free plan and accepts
`ACCEPT_CREDENTIAL_ISSUANCE`. Oxid uses separate managed authentication and
Jubjub assertion methods. Portal verifies the holder proof. Oxid then verifies
and imports the exact body, detached proof, and private-part relation through
the existing native adapter and encrypted credential repository. Status stays
`not_checked`.

## Runtime lifecycle

`scripts/portal-consumer-lifecycle.sh` validates the exact merged Portal source,
builds its public Nix images, and starts five Portal-only services. It attaches
to the existing three-container `oxid-standalone` project instead of starting a
second Midnight node, indexer, or proof server. Portal-owned containers,
network, volume, private environment, and receipt are removed only after exact
ownership validation.

The physical harness discovers exactly one connected non-QEMU Android device
and passes its selector through process environment rather than a recipe or
argument. It discovers the current Tailscale MagicDNS identity from
`tailscale status --json` and chooses an unused HTTPS listener at runtime.
Existing Oxid Serve routes on 443, 8443, and 10000 are immutable baseline state.

Oxid writes a private, transient config for the merged Portal
`scripts/tailscale-https-profile.sh`. That profile installs only `/`,
`/issuer-resolver`, and `/offer` on the selected listener. Its exact receipt
cleanup must restore the byte-equivalent pre-state before evidence can be
published. Oxid does not call `tailscale serve reset` for this flow.

## Evidence

`just portal-headless-e2e` proves real issuance followed by a second headless
process restoring, listing, and freshly reverifying the encrypted credential.

`just android-portal-tailnet-physical-smoke` proves physical Android ingress,
explicit consent, real Final issuance, encrypted storage shape, process death,
development-custody reactivation, listing, and fresh reverification. The
post-install journey is bounded to 300 seconds. Evidence contains only the exact
Oxid head, Portal commit/tree, OCI image digests, coarse Android OS/API facts,
and closed booleans. It excludes identifiers, endpoints, protocol artifacts,
claims, credentials, proofs, keys, capabilities, device serials, and tailnet
identity.

## Consequences

This is standalone conformance evidence, not production discovery, production
trust, native-custody persistence, real KYC, a live holder DID write, verified
App Link delivery, or release readiness. The mock KYC decision remains clearly
separate from Portal signing and Oxid verification.
