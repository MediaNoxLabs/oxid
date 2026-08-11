# Security policy

## Reporting a vulnerability

Please report suspected vulnerabilities through GitHub private vulnerability
reporting for this repository. Do not open a public issue and do not include
wallet secrets, credential claims, identifiers, or exploit details in public
channels.

Include the affected revision, platform, reproduction steps, impact, and a
minimal proof of concept where safe. Maintainers will acknowledge and triage the
report as promptly as possible and coordinate disclosure after a fix is ready.

## Current security posture

Oxid is in an early migration milestone and is **not production-ready**. Public
wallet-profile metadata can be persisted separately. The standalone headless
harness can create process-local Ed25519 and P-256 development keys to test
opaque-reference flows; those keys are never durable and are not asset custody.
Production composition reports protected storage unavailable. No seed, DID, or
credential material is persisted by the current slice.

The following rules are already enforced as architecture constraints:

- no raw private key or seed material in UI/application DTOs;
- platform time and randomness behind explicit ports;
- persistence behind a wallet-owned repository port;
- Dioxus isolated as an incoming adapter;
- telemetry disabled by default;
- no secrets or claims in logs;
- dependency and advisory checks independent from tests.

The in-memory profile adapter and `storage-dev` signing adapter are not secure
storage. `storage-dev` is selected only by explicit headless/test composition,
reports `development_only`, requires application confirmation before signing
or deletion, and zeroizes supported software key types on drop. Future custody
code must satisfy ADR-0017 with platform-backed protection and native user
authorization before it is described as production-capable.

## Supported versions

Until the first release, only the latest commit on `develop` receives security
fixes. Release support policy will be published before a stable version.
