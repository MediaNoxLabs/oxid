# ADR-0023: Prioritize staged Midnight prototype parity after M0

- Status: Accepted
- Date: 2026-08-11
- Source: Product direction and [parity epic](https://github.com/MediaNoxLabs/oxid/issues/2)
- Implementation state: Wallet shell, headless harness, and profile lifecycle implemented; capability backlog open
- Amended by: ADR-0036, ADR-0037, ADR-0038, ADR-0039, ADR-0040, ADR-0041, ADR-0042, ADR-0043, ADR-0045

## Context

The blueprint orders Cardano before Midnight in its initial delivery roadmap.
After completing M0, product direction changed to prioritize all useful
functionality demonstrated by the reviewed `midnight-ledger` mobile prototype.
The prototype spans later wallet, Midnight, identity, credential, protocol,
native-platform, and development-tooling milestones.

Treating parity as a single directory move would conflict with ADR-0021 and
would reintroduce ledger-relative dependencies, secrets, UI-to-SDK calls, and a
monolithic application surface. Treating the original milestone numbers as a
strict sequence would defer the newly prioritized product outcome.

## Decision

Prioritize functional parity with `midnight-ledger` commit
`074b1a4bccbfee1740ee188374b606a022ecef42` after M0. Deliver the parity epic in
bounded vertical slices, starting with the safe wallet presentation shell and
then adding capabilities behind Oxid-owned domain types, application use cases,
focused ports, and reviewed adapters.

Keep Create Wallet Profile throughout the migration. Its onboarding, selection,
and public-metadata persistence integration is delivered under
[issue #1](https://github.com/MediaNoxLabs/oxid/issues/1); protected key custody
remains gated by ADR-0017.

This reprioritizes delivery, not architecture. Proposed ADR-0015 through
ADR-0018 remain gates for Midnight, SSI, secret storage, and error-taxonomy
choices. Cardano remains in the product roadmap but is not a prerequisite for
the reviewed Midnight parity slices.

## Consequences

- The parity epic, rather than milestone numbering alone, determines the next
  ordered product slices.
- Every capability remains independently reviewable, testable, and mobile
  smoke-tested.
- The UI can regain the prototype's recognizable structure before its adapters
  exist, but must label unavailable capabilities honestly.
- Visual and behavioral parity may use new internal implementations.
- No dependency or production custody mechanism is authorized by this ADR.
