# ADR-0118: Centralize native transport trust

- Status: Accepted
- Date: 2026-09-24
- Issue: [#699](https://github.com/MediaNoxLabs/oxid/issues/699)
- Depends on: ADR-0097, ADR-0098, ADR-0103, and ADR-0116

## Context

Oxid's native HTTP and WebSocket clients had different implicit certificate
authorities. `reqwest` used the operating-system verifier while
`tokio-tungstenite` used its compiled WebPKI roots. On Android the Tailnet
WebSocket route succeeded, but HTTP readiness and Midnight requests failed in
the platform-verifier path. The resulting partial connectivity was presented
as an unavailable indexer and made a healthy demo look like a chain failure.

Other adapters copied WebPKI root construction independently. A new network
client could therefore select a third policy, disable verification, or work on
desktop while failing only after deployment to iOS or Android.

Two `rustls-platform-verifier` versions are currently unavoidable. Reqwest
uses 0.7 while the pinned Jsonrpsee/Subxt graph uses 0.5. The native Android
boundary initializes both exact versions before application networking. This
is an upstream compatibility constraint, not authorization for more verifier
versions or client factories.

## Decision

`oxid-adapter-platform-system` owns one closed native transport trust policy and
the only production HTTP/WebSocket client constructors:

| Route | Policy | Constraint |
| --- | --- | --- |
| Public production HTTPS/WSS | `PlatformTrust` | Android/iOS/macOS system trust and hostname validation |
| Exact `*.ts.net` HTTPS/WSS demo route | `BundledPublicRoots` | Reviewed Mozilla/WebPKI roots and hostname validation |
| `localhost` or loopback HTTP/WS | `DevelopmentLoopback` | Plaintext permitted only for local development |

There is no automatic fallback between policies. Remote plaintext, embedded
credentials, unsupported schemes, invalid hostnames, and direct Tailnet IPs
fail before connection. Tailnet discovery must provide the exact MagicDNS FQDN
covered by the public certificate; neither leaf pinning nor a private CA is
introduced.

Deployment readiness, Midnight indexer/DUST/shielded transports, proof and
parameter HTTP, Portal issuance/handoff, DID resolution/publication, and live
Passport state use the shared boundary. The repository gate rejects new ad-hoc
client builders, direct bundled-root owners, disabled verification, or
unreviewed WebSocket call sites.

The bundled Tailnet mode is an explicit development/demo policy. Normal public
services retain canonical platform trust on Android and iOS. Consolidating the
two verifier crate versions requires a separately verified Jsonrpsee/Subxt
upgrade; it is not coupled to this correctness fix.

## Consequences

- HTTP and WebSocket routes use the same explicit trust decision.
- Mobile failures are deterministic configuration failures rather than silent
  fallback or protocol-specific behavior.
- Adding a transport requires a reviewable policy change and repository-gate
  update.
- Public Tailnet certificates remain portable across Android and iOS without
  installing a user certificate or trusting a direct IP address.
- Upstream verifier duplication stays visible and bounded until its owners can
  be upgraded together.

## Alternatives rejected

- Disabling certificate or hostname verification would turn a demo routing
  problem into a credential and wallet-state interception vulnerability.
- Retrying platform trust with bundled roots would make trust depend on the
  first failure and obscure deployment errors.
- Pinning one Tailnet leaf certificate would create brittle host-specific
  rotation and recovery procedures.
- Keeping per-adapter builders would preserve the exact drift that caused the
  Android split-brain failure.
