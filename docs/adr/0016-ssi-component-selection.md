# ADR-0016: Select focused SSI components

- Status: Proposed
- Date: 2026-08-11
- Blueprint source: Sections 9, 10, 11, and 17
- Implementation state: Research required before M3

## Context

The SSI ecosystem spans DID methods, document resolution, credential models,
proof suites, status, OIDC protocols, DIDComm, and policy. A monolithic SDK can
collapse those boundaries and dictate types throughout the wallet.

## Proposed decision

Evaluate focused maintained Rust components—including SpruceID ecosystem
components where suitable—per capability. Prefer independently replaceable
libraries over adopting one universal SSI façade. Apply the full dependency
review template and verify the exact normative standard version each adapter
implements.

SDK and wire types remain within DID, credential-format, verification, status,
or protocol adapters under ADR-0003.

## Consequences if accepted

- Oxid can evolve DID methods and credential formats independently.
- More explicit mapping and interoperability testing will be required.
- Standards-version compatibility becomes visible per adapter.
- This proposal does not select or authorize an SSI dependency today.
