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

Oxid is in its foundation milestone and is **not production-ready**. The current
slice creates public wallet-profile metadata in process-local memory. It does
not create or persist asset keys, seeds, DIDs, or credentials.

The following rules are already enforced as architecture constraints:

- no raw private key or seed material in UI/application DTOs;
- platform time and randomness behind explicit ports;
- persistence behind a wallet-owned repository port;
- Dioxus isolated as an incoming adapter;
- telemetry disabled by default;
- no secrets or claims in logs;
- dependency and advisory checks independent from tests.

The in-memory adapter is not secure storage. Future custody code must use
platform-backed protection, opaque key references, explicit authorization, and
separate security review before it is described as production-capable.

## Supported versions

Until the first release, only the latest commit on `develop` receives security
fixes. Release support policy will be published before a stable version.
