# ADR-0113: Expose fixed standalone funding through Tailnet discovery

- Status: Accepted
- Date: 2026-09-16
- Issue: [#540](https://github.com/MediaNoxLabs/oxid/issues/540)

## Context

ADR-0112 supplies a loopback-only HTTP adapter over the fixed standalone NIGHT
faucet. A second wallet process needs private HTTPS discovery without turning
Tailnet into a Midnight network, recording a machine hostname, or taking over
unrelated Tailscale Serve configuration.

## Decision

An owner-invoked lifecycle dynamically discovers MagicDNS and selects an unused
HTTPS Serve port. It starts the existing loopback faucet, adds that port and a
Tailnet-only setup QR, and records the canonical prior and active Serve JSON in
a mode-0600 receipt. Cleanup proceeds only when the complete active JSON still
matches the receipt, then removes that new port and verifies exact restoration
of the prior configuration. It never calls `tailscale serve reset` or Funnel.

The responsive discovery page and owner-generated setup QR are served by the
existing faucet adapter through the same loopback origin. This keeps the
Tailnet lifecycle to one reverse-proxy route and works with macOS Tailscale,
which cannot serve a local file path. The page has only the fixed funding form
and health boundary. The QR contains the version, exact `undeployed`
realm/fingerprint, and HTTPS route. It contains no wallet, recipient, key,
personal identity, or policy control. The current origin is discovered by the
browser; no hostname is committed or logged.

A live HTTPS health/funding path requires both explicit owner invocation and
`OXID_ENABLE_OWNER_TAILNET_FAUCET_ACCEPTANCE=1`. It accepts an operator-private
undeployed recipient and verifies the unchanged 50,000 NIGHT result without a
phone.

## Consequences

- The grant remains the sole dispatcher-owned fixed policy and authority.
- Tailnet is authenticated private transport, not a public Internet listener,
  Midnight realm, wallet selector, or deployment claim.
- Drift is preserved for review rather than risking unrelated route deletion.
- Deterministic script and HTML contracts do not mutate Tailnet state; the
  live acceptance remains owner-authorized and on demand.
