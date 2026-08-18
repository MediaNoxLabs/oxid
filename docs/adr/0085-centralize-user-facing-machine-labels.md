# ADR-0085: Centralize user-facing machine labels

- Status: Accepted
- Date: 2026-08-19
- Blueprint source: Sections 1, 3–7, 9–13, 16, 18, and 21
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/dioxus-wallet/src/format.rs`, and the Dioxus wallet screens
- Tracking: issues #2, #65, and #77
- Implementation state: Dioxus routes user-visible machine values through one reviewed module; exact asset/date formatting and a repository copy gate are active

## Context

Oxid's application views expose bounded public projections to incoming
adapters. Several projections intentionally serialize domain enums as stable
machine strings, including synchronization and submission states, adapter
sources, credential formats, verification stages, protocol outcomes, and
Passport Vault authentication classes. The first mobile migration rendered
some values directly or converted underscores to spaces. That exposed
implementation vocabulary such as `outcome_unknown` and
`indexer_supplied_not_proven`, and it could make a new adapter value appear as
unreviewed product copy.

The same presentation surface also showed NIGHT and DUST as raw subunit counts,
displayed Unix milliseconds, and described synchronization using adapter cursor
positions. Those values were technically exact but not meaningful product
language. They also made it easy to overstate whether a result was simulated,
indexer-reported, or authenticated against finalized Midnight history.

This slice is presentation-only. Changing application view types back to domain
enums would broaden the dependency boundary and the UI redesign program, so the
labeling adapter must handle the existing string projections safely.

## Decision

Use `crates/ui-dioxus/src/labels.rs` as the only user-facing translation seam
for machine-valued application fields.

- Each known state, mode, source, format, authentication class, reason code,
  network, and disclosure term has an explicit category-specific match.
- Unknown strings map to neutral unavailable/unknown language. A fallback must
  never return or interpolate its input.
- Typed values already owned by an application dependency, such as
  `IdentityRequestKind`, use an exhaustive Rust match directly.
- Application strings remain available to Dioxus control flow and CSS-state
  selection, but they may reach visible text only through this module.
- NIGHT uses six decimals and DUST uses fifteen decimals through integer/string
  formatting only. User-entered Passport Vault NIGHT decimals are converted to
  exact integer amounts before an application command is created.
- The all-zero native shielded token is formatted as NIGHT. An unknown token
  type keeps its exact integer quantity without inventing decimal precision.
- Unix milliseconds render as a stable UTC civil time without a locale,
  floating-point, or network dependency. Adapter cursor positions may drive a
  progress bar but are not product copy.

Add `scripts/check-ui-copy-labels.sh` to `run.sh`'s UI gate. It rejects direct
interpolation or underscore replacement of machine-valued fields, raw subunit
terminology, cursor prose, direct Unix-millisecond copy, and label fallbacks
that echo unknown inputs. It also requires the reviewed high-risk vocabulary
and exact formatters to remain present.

## Security and truth boundaries

- Simulation, live settlement, finalized replay, and unproven indexer state
  remain distinct labels. Presentation code cannot upgrade a capability or
  authentication claim.
- `outcome_unknown` means the wallet is checking finalized history and will not
  submit a duplicate. A friendlier label does not change reconciliation rules.
- Unknown machine strings are hidden, not normalized into plausible prose.
  Control flow remains fail-closed in the owning application/adapter.
- Technical identifiers such as transaction hashes, block hashes, DIDs, and
  public addresses remain available in explicit detail rows. Labels never
  expose secrets, witnesses, credential values, or protected key material.
- The module changes copy and exact presentation conversion only. It does not
  change consent intents, state transitions, storage, proving, submission, or
  reconciliation authority.

## Consequences

- The route-stack and journey redesign can reuse one vocabulary and later place
  its localization seam around one module.
- A newly serialized application value cannot leak into user copy; it appears
  as unavailable until its label is reviewed.
- Because some views currently flatten enums to strings, Rust cannot detect a
  newly added upstream string variant at compile time. The explicit vocabulary
  gate and safe-unknown behavior cover that boundary without changing the
  application contract; future typed views may replace those parsers
  incrementally.
- Passport Vault amount inputs now mean decimal NIGHT rather than internal
  integer subunits. Commands still receive the same exact integer shape.
- Unknown shielded assets cannot receive a fabricated symbol or decimal scale.

## Validation

- Unit tests cover safe unknowns, required vocabulary, exact NIGHT/DUST
  formatting, decimal-to-integer conversion, and UTC date rendering.
- Existing Dioxus tests cover sync, durable submission, consent, QR, and
  Passport Vault state behavior after the presentation conversion.
- `scripts/check-ui-copy-labels.sh` runs before every UI compilation in
  `run.sh`.
- The strict repository, headless flow, and standalone mobile simulator gates
  exercise the unchanged application behavior through the new copy boundary.

## Rejected alternatives

- Replacing underscores with spaces makes internal names look accidental and
  can overstate authentication or settlement.
- Echoing unknown values in a technical-details style still leaks unreviewed
  copy into the ordinary user profile.
- Adding a date/number localization dependency for this first seam would widen
  the dependency graph before English/Ukrainian locale policy is implemented.
- Moving label ownership into domain or application crates would couple product
  voice to reusable wallet/identity behavior.
