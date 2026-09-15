# ADR-0112: Bound standalone funding to loopback HTTP

- Status: Accepted
- Date: 2026-09-15
- Issue: [#539](https://github.com/MediaNoxLabs/oxid/issues/539)

## Context

ADR-0111 isolates the fixed standalone NIGHT authority behind a compile- and
runtime-gated process protocol. A browser or later Tailnet adapter cannot call
stdin directly. Adding a general web framework or exposing the ordinary wallet
protocol would widen dependencies and authority for a development-only surface.

## Decision

Provide a second feature-gated binary that translates bounded HTTP/1.1 requests
into the same in-process faucet dispatcher. It binds to `127.0.0.1:36301` by
default and rejects every non-loopback bind address. The operator may select a
different loopback port for isolated tests, but cannot select the realm, route,
asset, amount, custody root, or transaction policy.

The adapter exposes only `GET /health` and `POST /fund`. It processes one
connection at a time, applies ten-second read/write deadlines, limits headers
to 8 KiB and bodies to 4 KiB, requires HTTP/1.1 with exactly one Host header,
and rejects transfer encoding, expectations, folded or duplicate framing, and
unknown methods or paths. Every response closes the connection, disables
caching, and uses the existing closed protocol response vocabulary.

The implementation deliberately uses the standard library. Its small HTTP
subset is fully rejected outside the documented shape; it is not a general web
server. Tailscale Serve will terminate TLS and forward to this loopback port in
a separately reviewed slice.

## Consequences

- Funding and idempotency still have one implementation and one receipt store.
- Normal headless, UI, and release builds do not include either faucet binary.
- A disconnected or malformed client cannot terminate the listener.
- The synchronous listener bounds in-flight funding to one operation, matching
  the public genesis authority and avoiding stale-state double spends.
- Browser HTML, CORS, Tailnet routes, QR discovery, and native wallet UX remain
  outside this decision.

## Alternatives rejected

- Expose the full headless wallet protocol over HTTP: unnecessarily grants
  custody, backup, identity, and transaction operations to a funding client.
- Bind to all interfaces and rely on a firewall: makes local development
  accidentally reachable beyond the reviewed transport boundary.
- Add a general asynchronous HTTP framework: adds dependency and compile cost
  without a need for concurrency or protocol breadth.
