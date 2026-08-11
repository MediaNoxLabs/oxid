# ADR-0010: OIDC and DIDComm are protocol adapters

- Status: Accepted
- Date: 2026-08-11
- Blueprint source: Sections 7 and 11
- Implementation state: Planned for M4 and M6

## Context

OpenID4VCI, OpenID4VP, SIOP 2.0, Presentation Exchange, and DIDComm define wire
messages, state transitions, transport, and interoperability profiles. They do
not own the wallet's credential or presentation semantics.

## Decision

Place OIDC-family and DIDComm implementations behind protocol adapters and
focused ports. Adapters validate wire requests and map them to application
commands; application use cases own candidate selection, consent, credential
use, and presentation outcomes.

DIDComm transport and packing remain distinct capabilities. SIOP
authentication remains distinct from VC presentation even where the protocols
share OIDC machinery.

## Consequences

- Protocol version changes are isolated from credential and presentation core.
- Origin, audience, nonce, state, encryption, and authentication checks remain
  explicit adapter responsibilities.
- Deep links and QR data must enter through validation boundaries.
- No OIDC or DIDComm dependency belongs in an M0 core crate.
