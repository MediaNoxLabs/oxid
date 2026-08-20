# ADR-0009: Credential models are separate from serializations

- Status: Accepted
- Date: 2026-08-11
- Blueprint source: Section 10
- Implementation state: Planned for M3 through M5
- Amended by: ADR-0038, ADR-0041, ADR-0042, ADR-0043, ADR-0045

## Context

Identifier methods, credential data models, proof/serialization formats, and
communication protocols are independent axes. Conflating them would make a
wire representation such as JWT or JSON-LD the wallet's credential domain.

## Decision

Oxid's credential domain represents stable envelope and metadata concepts
without equating `Credential` to JWT, JSON-LD, SD-JWT, mdoc, or Open Badges
bytes. Format adapters parse, verify, and produce wire representations.

Store original signed bytes alongside normalized searchable metadata. Never
silently rewrite a signed payload. Verification returns structured stage
outcomes rather than a single `valid` boolean.

## Consequences

- Multiple formats can implement shared credential use cases.
- Signed evidence remains available for faithful export and re-verification.
- Normalization, provenance, and verification-stage models require careful
  design before the first credential store is introduced.
- Protocol adapters cannot substitute their wire DTOs for credential entities.
